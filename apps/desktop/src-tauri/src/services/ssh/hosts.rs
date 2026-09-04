//! The hosts this app currently has a connection to.
//!
//! One connection per host, shared by every project on it — which is the point
//! of the ControlMaster underneath: two repositories on the same build box cost
//! one authentication, not two.
//!
//! Connections are held here rather than on a project, because a project is a
//! path and a host is a machine, and closing one project should not disconnect
//! the other three.

use std::collections::HashMap;
use std::sync::Arc;

use onlydiffs_core::protocol::Event;
use onlydiffs_core::services::repository::{RemoteSender, Repository};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex};

use crate::contract::{ConnectedHost, HostConnectionState};
use crate::error::AppError;
use crate::services::repo_watch;
use crate::services::ssh::agent;
use crate::services::ssh::askpass::Prompt;
use crate::services::ssh::connection::SshConnection;
use crate::services::ssh::target::{self, SshTarget};
use crate::services::ssh::transport::AgentTransport;

/// Emitted when a host connects, disconnects, or drops.
pub const HOST_CHANGED: &str = "ssh:hosts-changed";
/// Emitted when ssh asks the user something. The renderer answers it with
/// `answer_ssh_prompt`.
pub const SSH_PROMPT: &str = "ssh:prompt";

/// One connected host: the ssh connection, and the agent riding it.
struct Connected {
    target: SshTarget,
    connection: SshConnection,
    transport: AgentTransport,
    /// The alias as the user typed it, which is what every path is shown with.
    label: String,
    /// Whether the Claude channel was registered with Claude Code on the host
    /// when this connection was made.
    channel_registered: bool,
}

/// Prompts waiting for the user, by id. Held so an answer arriving from the
/// renderer can be matched to the ssh process that is blocked on it.
type OpenPrompts = Arc<Mutex<HashMap<u64, Prompt>>>;

pub struct SshHosts {
    connected: Mutex<HashMap<String, Connected>>,
    prompts: OpenPrompts,
    next_prompt: std::sync::atomic::AtomicU64,
}

impl SshHosts {
    pub fn new() -> Self {
        Self {
            connected: Mutex::new(HashMap::new()),
            prompts: Arc::new(Mutex::new(HashMap::new())),
            next_prompt: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Everything currently connected, for the settings page and the picker.
    pub async fn list(&self) -> Vec<ConnectedHost> {
        let connected = self.connected.lock().await;
        let mut hosts: Vec<ConnectedHost> = connected
            .values()
            .map(|host| ConnectedHost {
                alias: host.label.clone(),
                hostname: host.target.hostname.clone(),
                user: host.target.user.clone(),
                port: host.target.port,
                state: HostConnectionState::Connected,
                git_version: Some(host.connection.probe().git_version.clone()),
                platform: Some(format!(
                    "{} {}",
                    host.connection.probe().os,
                    host.connection.probe().arch
                )),
                agent_version: Some(host.transport.agent_version().to_owned()),
                channel_registered: Some(host.channel_registered),
            })
            .collect();
        hosts.sort_by(|a, b| a.alias.cmp(&b.alias));
        hosts
    }

    /// The transport for a host, if it is connected.
    pub async fn sender(&self, alias: &str) -> Option<(String, RemoteSender)> {
        let connected = self.connected.lock().await;
        connected
            .get(alias)
            .map(|host| (host.label.clone(), host.transport.calls()))
    }

    /// A repository on a connected host.
    pub async fn repository(&self, alias: &str, root: &str) -> Result<Repository, AppError> {
        let (label, calls) = self.sender(alias).await.ok_or_else(|| {
            AppError::SshDisconnected(format!("{alias} is not connected."))
        })?;
        Ok(Repository::remote(label, root.into(), calls))
    }

    /// Connects, or answers immediately if the host is already connected.
    ///
    /// Keyed by the alias the user typed rather than the resolved triple: two
    /// aliases for one machine are the same machine to ssh, but they are two
    /// entries in the user's config and two labels on screen, and silently
    /// collapsing them would show a project under a name nobody chose.
    pub async fn connect(
        &self,
        app: &AppHandle,
        alias: &str,
        extra_args: Vec<String>,
    ) -> Result<ConnectedHost, AppError> {
        let alias = target::validate_target(alias)?.to_owned();
        if self.connected.lock().await.contains_key(&alias) {
            return self
                .list()
                .await
                .into_iter()
                .find(|host| host.alias == alias)
                .ok_or_else(|| AppError::Ssh(format!("{alias} vanished while connecting.")));
        }

        // The options the host was added with, replayed. A port or an identity
        // that only applied on the first connection would be a host that works
        // once and then stops.
        let resolved = target::resolve(&alias, &extra_args).await?;
        let prompts = self.prompt_channel(app);
        let connection = SshConnection::connect(resolved.clone(), extra_args, prompts).await?;

        // Watch events from this host reach the window the same way a local
        // watcher's do, so the renderer has one thing to listen for.
        let (events, mut incoming) = mpsc::unbounded_channel::<Event>();
        let emitter = app.clone();
        tokio::spawn(async move {
            while let Some(Event::RepoChanged { .. }) = incoming.recv().await {
                repo_watch::announce(&emitter);
            }
        });

        let transport = AgentTransport::start(&connection, events).await?;
        // The agent is on the host now, so Claude Code there can be pointed at
        // its channel mode. Best effort: a host without `claude` is still a
        // host worth reviewing on.
        let channel_registered = agent::register_claude_channel(&connection).await;

        self.connected.lock().await.insert(
            alias.clone(),
            Connected {
                target: resolved,
                connection,
                transport,
                label: alias.clone(),
                channel_registered,
            },
        );
        let _ = app.emit(HOST_CHANGED, ());

        self.list()
            .await
            .into_iter()
            .find(|host| host.alias == alias)
            .ok_or_else(|| AppError::Ssh(format!("{alias} vanished while connecting.")))
    }

    /// Closes a connection. Only ever stops a master this app started; a
    /// ControlMaster the user had already is left exactly as it was found.
    pub async fn disconnect(&self, app: &AppHandle, alias: &str) {
        let host = self.connected.lock().await.remove(alias);
        if let Some(mut host) = host {
            host.transport.shutdown().await;
            host.connection.disconnect().await;
        }
        let _ = app.emit(HOST_CHANGED, ());
    }

    /// Closes everything, for app shutdown.
    pub async fn disconnect_all(&self) {
        let hosts = std::mem::take(&mut *self.connected.lock().await);
        for (_, mut host) in hosts {
            host.transport.shutdown().await;
            host.connection.disconnect().await;
        }
    }

    /// The user's answer to a prompt ssh is blocked on.
    pub async fn answer_prompt(&self, id: u64, answer: Option<String>) {
        let prompt = self.prompts.lock().await.remove(&id);
        let Some(prompt) = prompt else { return };
        match answer {
            Some(value) => prompt.answer(value),
            None => prompt.cancel(),
        }
    }

    /// Bridges ssh's prompts to the window, and holds each one until answered.
    fn prompt_channel(&self, app: &AppHandle) -> mpsc::UnboundedSender<Prompt> {
        let (tx, mut rx) = mpsc::unbounded_channel::<Prompt>();
        let prompts = self.prompts.clone();
        let emitter = app.clone();
        let next = &self.next_prompt;
        let start = next.load(std::sync::atomic::Ordering::Relaxed);
        next.store(start, std::sync::atomic::Ordering::Relaxed);
        let counter = Arc::new(std::sync::atomic::AtomicU64::new(start));
        // The counter is cloned rather than shared with `self`, because this
        // task outlives the borrow. Ids only have to be unique among prompts
        // that are open at once, and a per-connection counter starting where
        // the last one left off is comfortably that.
        tokio::spawn(async move {
            while let Some(prompt) = rx.recv().await {
                let id = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let payload = crate::contract::SshPromptRequest {
                    id,
                    text: prompt.text.clone(),
                    is_secret: prompt.is_secret(),
                };
                prompts.lock().await.insert(id, prompt);
                // A window that has gone away cannot answer, and leaving the
                // prompt in the map would hold ssh open until it times out.
                if emitter.emit(SSH_PROMPT, payload).is_err() {
                    if let Some(orphan) = prompts.lock().await.remove(&id) {
                        orphan.cancel();
                    }
                }
            }
        });
        tx
    }
}

impl Default for SshHosts {
    fn default() -> Self {
        Self::new()
    }
}
