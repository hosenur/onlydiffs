//! The stream between the app and a running agent.
//!
//! One `ssh` process, one stdio stream, framed. Not a port forward: OnlyDiffs
//! has no browser to point at a forwarded HTTP port, and stdio binds nothing on
//! the remote host at all — there is no socket for anything else on that
//! machine to reach, and no key to leak, because there is no listener.
//!
//! Three tasks and no locks on the hot path. A writer owns the pipe going out,
//! a reader owns the pipe coming in, and a small map correlates the two by
//! request id. Everything else — the app, the services, the agent — sees plain
//! `async fn` calls.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use onlydiffs_core::protocol::{
    read_frame, write_frame, Envelope, Event, FrameError, Message, Request, RequestId, Response,
    PROTOCOL_VERSION,
};
use onlydiffs_core::services::repository::{RemoteCall, RemoteSender};
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::error::AppError;
use crate::services::ssh::agent;
use crate::services::ssh::connection::SshConnection;

/// Requests still waiting for an answer, by id.
type Pending = Arc<Mutex<HashMap<RequestId, oneshot::Sender<Result<Response, AppError>>>>>;

/// A live agent on a host.
pub struct AgentTransport {
    calls: RemoteSender,
    /// The agent's own reported version, for showing next to the host.
    agent_version: String,
    /// The `ssh` process carrying the agent's stdio.
    ///
    /// Held, not dropped. `SshConnection::command` sets `kill_on_drop`, so
    /// letting this go out of scope after the handshake would kill the very
    /// connection that just succeeded — which is exactly what it did until a
    /// test caught it: the handshake passed and every request after it came
    /// back as a closed connection.
    child: Child,
    /// Aborting these is what stops the pumps; the agent exits when its stdin
    /// does, so there is nothing to clean up on the far side.
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for AgentTransport {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
        // The agent would exit on its own once stdin closed, but only after the
        // pipe drained. Killing the ssh process is immediate and leaves nothing
        // on the host: the agent's parent is gone, so it goes too.
        self.child.start_kill().ok();
    }
}

impl AgentTransport {
    /// The handle a `Repository` is built from.
    pub fn calls(&self) -> RemoteSender {
        self.calls.clone()
    }

    pub fn agent_version(&self) -> &str {
        &self.agent_version
    }

    /// Uploads the agent if needed, starts it, and completes the handshake.
    ///
    /// `events` receives everything the agent says without being asked. It is a
    /// separate channel from the request replies on purpose: an event has no
    /// request to belong to, and threading it through the same map would mean
    /// inventing an id for something nobody asked for.
    pub async fn start(
        connection: &SshConnection,
        events: mpsc::UnboundedSender<Event>,
    ) -> Result<Self, AppError> {
        let agent_path = agent::ensure(connection).await?;
        let mut child = agent::serve_command(connection, &agent_path)
            .spawn()
            .map_err(|error| AppError::Ssh(format!("could not start the agent: {error}")))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::Ssh("the agent had no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::Ssh("the agent had no stdout".into()))?;
        let stderr = child.stderr.take();

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (calls, mut outgoing) = mpsc::unbounded_channel::<RemoteCall>();
        let next_id = Arc::new(AtomicU32::new(1));

        // Writer: the only thing that touches the outgoing pipe, so two frames
        // can never interleave on it.
        let writer = {
            let pending = pending.clone();
            let next_id = next_id.clone();
            tokio::spawn(async move {
                while let Some(RemoteCall { request, reply }) = outgoing.recv().await {
                    // Skipping zero keeps it reserved for events, which is what
                    // lets the reader tell an answer from news by id alone.
                    let id = next_id.fetch_add(1, Ordering::Relaxed).max(1);
                    pending.lock().await.insert(id, reply);
                    let frame = Message::Request(Envelope { id, body: request });
                    if let Err(error) = write_frame(&mut stdin, &frame).await {
                        // The pipe is gone. Everything outstanding fails with
                        // the same reason rather than hanging forever.
                        fail_all(&pending, &format!("{error}")).await;
                        return;
                    }
                }
            })
        };

        // Reader: matches answers to their questions, forwards events.
        let reader = {
            let pending = pending.clone();
            let events = events.clone();
            tokio::spawn(async move {
                let mut stream = BufReader::new(stdout);
                loop {
                    match read_frame(&mut stream).await {
                        Ok(Message::Response(Envelope { id, body })) => {
                            let waiting = pending.lock().await.remove(&id);
                            if let Some(reply) = waiting {
                                let _ = reply.send(Ok(body));
                            }
                            // An answer to a request nobody is waiting for is a
                            // cancelled call, not an error.
                        }
                        Ok(Message::Event(event)) => {
                            if events.send(event).is_err() {
                                return;
                            }
                        }
                        Ok(Message::Request(_)) => {
                            // The agent never asks questions. A stream that
                            // does is not the agent.
                            fail_all(&pending, "the agent sent a request").await;
                            return;
                        }
                        Err(FrameError::Closed) => {
                            fail_all(&pending, "the connection closed").await;
                            return;
                        }
                        Err(error) => {
                            fail_all(&pending, &format!("{error}")).await;
                            return;
                        }
                    }
                }
            })
        };

        // The agent's stderr is a log, never a frame. A login shell that prints
        // a banner writes there, and reading it separately is what stops that
        // banner from reaching the parser.
        let logger = tokio::spawn(async move {
            let Some(stderr) = stderr else { return };
            use tokio::io::AsyncBufReadExt;
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("agent: {line}");
            }
        });

        let mut transport = Self {
            calls,
            agent_version: String::new(),
            child,
            tasks: vec![writer, reader, logger],
        };

        // Mutated in place rather than rebuilt: this type aborts its tasks on
        // drop, so moving out of it to fill in one field would tear down the
        // connection the handshake just established.
        transport.agent_version = transport.handshake().await?;
        Ok(transport)
    }

    /// Proves the agent is the one this app expects before anything else is
    /// asked of it.
    async fn handshake(&self) -> Result<String, AppError> {
        let (reply, answered) = oneshot::channel();
        self.calls
            .send(RemoteCall {
                request: Request::Hello {
                    protocol: PROTOCOL_VERSION,
                },
                reply,
            })
            .map_err(|_| AppError::Ssh("the agent stopped before it answered".into()))?;

        match answered.await {
            Ok(Ok(Response::Hello {
                protocol,
                agent_version,
            })) => {
                if protocol != PROTOCOL_VERSION {
                    return Err(AppError::Ssh(format!(
                        "the agent speaks protocol {protocol}; this app speaks {PROTOCOL_VERSION}"
                    )));
                }
                Ok(agent_version)
            }
            Ok(Ok(Response::Err { message, .. })) => Err(AppError::Ssh(message)),
            Ok(Ok(other)) => Err(AppError::Ssh(format!(
                "the agent answered the handshake with {other:?}"
            ))),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(AppError::Ssh(
                "the agent stopped before it answered the handshake".into(),
            )),
        }
    }

    /// Asks the agent to exit, then lets the tasks be dropped.
    pub async fn shutdown(&self) {
        let (reply, answered) = oneshot::channel();
        if self
            .calls
            .send(RemoteCall {
                request: Request::Shutdown,
                reply,
            })
            .is_ok()
        {
            // A shutdown that goes unanswered is a connection that was already
            // gone, which is the outcome we wanted anyway.
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), answered).await;
        }
    }
}

/// Hands the same failure to everything still waiting, so a dropped connection
/// resolves every outstanding call instead of leaving them pending forever.
async fn fail_all(pending: &Pending, reason: &str) {
    let waiting = std::mem::take(&mut *pending.lock().await);
    for (_, reply) in waiting {
        let _ = reply.send(Err(AppError::SshDisconnected(reason.to_owned())));
    }
}
