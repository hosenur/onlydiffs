//! Talking to Codex's shared app-server daemon.
//!
//! The daemon is the one process that can reach a running session. A TUI
//! started as `codex --remote unix://` is a client of it, and a thread the
//! daemon has loaded is a thread that TUI is sitting in front of. So the two
//! questions this app has — which threads belong to the repository on screen,
//! and how to hand one a message — are asked of the daemon rather than read off
//! disk. Transcripts are not consulted: a session that has not taken a turn yet
//! has no transcript, and Codex files the ones that exist by the host's local
//! date, which is exactly the session a user has just opened and the directory
//! a scan by UTC day never looks in.
//!
//! The protocol is the app-server's JSON-RPC carried over a **WebSocket** on a
//! unix socket: an HTTP upgrade first, then masked text frames. Plain JSON
//! written to the socket is dropped without a byte of reply, and
//! `codex app-server proxy` forwards bytes verbatim, so the framing has to be
//! spoken here. Measured, one operation over this client is about 150 ms where
//! a `codex queue` shell-out is about 700 ms, and this needs nothing on the
//! host's PATH — on a build box, `codex` is routinely installed somewhere only
//! an interactive shell can find.
//!
//! What follows is the smallest client that satisfies it: one connection, a
//! handful of requests, text frames only. No compression, no extensions, no
//! TLS — the transport is a socket in the user's own home directory, and the
//! server's `Sec-WebSocket-Accept` is not checked for the same reason.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::error::AppError;
use crate::services::paths;

/// Where the daemon listens, relative to the home directory.
const CONTROL_SOCKET: &str = ".codex/app-server-control/app-server-control.sock";

/// How long any single exchange may take. The daemon is a local process that
/// answers in milliseconds; this only expires when something is wrong, and it
/// is short so a stuck daemon costs a status poll a few seconds rather than
/// holding a click.
const TIMEOUT: Duration = Duration::from_secs(5);

/// The largest frame worth reading. Thread reads can carry a turn's items, so
/// this is generous, but it still bounds what a confused server can make us
/// allocate.
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// What the app tells the user when the socket is not there. Named here
/// because the same sentence has to appear whether it was a status poll or a
/// send that found the daemon missing.
pub const DAEMON_NOT_RUNNING_MESSAGE: &str =
    "Codex's shared daemon is not running. Start it with `codex app-server daemon start`.";

fn fail(message: impl Into<String>) -> AppError {
    AppError::CodexChannel(message.into())
}

pub fn socket_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(CONTROL_SOCKET)
}

/// One thread the daemon has loaded, reduced to what addressing it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedThread {
    pub id: String,
    /// The working directory the thread was created with. For a session
    /// started as `codex --remote unix://` without `-C`, this is the daemon's
    /// own directory rather than the terminal's, which is why the hints the
    /// app shows always include `-C`.
    pub cwd: String,
    /// Seconds since the epoch of the last write, so the newest can be chosen
    /// when a repository has more than one.
    pub updated_at: i64,
}

/// One connection to the daemon.
pub struct AppServer {
    stream: UnixStream,
    /// Bytes read past the end of the last frame. A read returns whatever the
    /// kernel had, which is regularly more than one frame and sometimes less.
    pending: Vec<u8>,
    next_id: i64,
}

impl AppServer {
    /// Opens the daemon's socket and completes the upgrade, leaving a
    /// connection that speaks JSON-RPC.
    pub async fn connect() -> Result<Self, AppError> {
        Self::connect_to(&socket_path()).await
    }

    /// The same, against an explicit socket. Public so a test can stand up a
    /// daemon of its own.
    pub async fn connect_to(path: &Path) -> Result<Self, AppError> {
        let stream = tokio::time::timeout(TIMEOUT, UnixStream::connect(path))
            .await
            .map_err(|_| fail("Codex's daemon did not accept a connection in time."))?
            .map_err(|_| fail(DAEMON_NOT_RUNNING_MESSAGE))?;

        let mut server = Self {
            stream,
            pending: Vec::new(),
            next_id: 0,
        };
        server.upgrade().await?;
        server
            .request(
                "initialize",
                json!({
                    "clientInfo": { "name": "onlydiffs", "version": env!("CARGO_PKG_VERSION") },
                    "capabilities": { "experimentalApi": true }
                }),
            )
            .await?;
        server.notify("initialized", Value::Null).await?;
        Ok(server)
    }

    /// The HTTP half. Exactly these five headers: the daemon closes on a
    /// handshake it does not like without saying why, and a general-purpose
    /// WebSocket library's extra headers were enough to make it do so.
    async fn upgrade(&mut self) -> Result<(), AppError> {
        let key = BASE64.encode(random_16());
        let request = format!(
            "GET / HTTP/1.1\r\n\
             Host: localhost\r\n\
             Connection: Upgrade\r\n\
             Upgrade: websocket\r\n\
             Sec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Key: {key}\r\n\r\n"
        );
        self.stream
            .write_all(request.as_bytes())
            .await
            .map_err(|error| fail(format!("failed to greet the Codex daemon: {error}")))?;

        // Read until the end of the headers, keeping whatever came after them:
        // the first frame often arrives in the same packet.
        let deadline = tokio::time::Instant::now() + TIMEOUT;
        loop {
            if let Some(end) = find(&self.pending, b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&self.pending[..end]).into_owned();
                self.pending.drain(..end + 4);
                let status = head.lines().next().unwrap_or_default();
                if !status.contains("101") {
                    return Err(fail(format!(
                        "the Codex daemon refused the connection: {status}"
                    )));
                }
                return Ok(());
            }
            if self.pending.len() > 64 * 1024 {
                return Err(fail("the Codex daemon sent no usable greeting."));
            }
            self.fill(deadline).await?;
        }
    }

    /// Reads once into `pending`, or fails if nothing arrives in time.
    async fn fill(&mut self, deadline: tokio::time::Instant) -> Result<(), AppError> {
        let mut chunk = [0u8; 16 * 1024];
        let read = tokio::time::timeout_at(deadline, self.stream.read(&mut chunk))
            .await
            .map_err(|_| fail("the Codex daemon stopped responding."))?
            .map_err(|error| fail(format!("lost the Codex daemon: {error}")))?;
        if read == 0 {
            return Err(fail("the Codex daemon closed the connection."));
        }
        self.pending.extend_from_slice(&chunk[..read]);
        Ok(())
    }

    /// One frame of `opcode`, masked as a client must.
    async fn write_frame(&mut self, opcode: u8, payload: &[u8]) -> Result<(), AppError> {
        let mask = random_16();
        let mask = &mask[..4];
        let mut frame = vec![0x80 | opcode];
        let length = payload.len();
        if length < 126 {
            frame.push(0x80 | length as u8);
        } else if length <= u16::MAX as usize {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(length as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(length as u64).to_be_bytes());
        }
        frame.extend_from_slice(mask);
        frame.extend(
            payload
                .iter()
                .enumerate()
                .map(|(i, byte)| byte ^ mask[i % 4]),
        );
        self.stream
            .write_all(&frame)
            .await
            .map_err(|error| fail(format!("failed to send to the Codex daemon: {error}")))
    }

    /// The next complete message, reassembling continuation frames and
    /// answering pings so a slow exchange does not drop the connection.
    async fn read_message(&mut self, deadline: tokio::time::Instant) -> Result<Vec<u8>, AppError> {
        let mut message = Vec::new();
        loop {
            let (fin, opcode, payload) = self.read_frame(deadline).await?;
            match opcode {
                // Ping: the pong carries the same body back.
                0x9 => {
                    let _ = self.write_frame(0xa, &payload).await;
                    continue;
                }
                0xa => continue,
                0x8 => return Err(fail("the Codex daemon hung up.")),
                _ => {}
            }
            message.extend_from_slice(&payload);
            if fin {
                return Ok(message);
            }
        }
    }

    /// One frame: `(fin, opcode, payload)`. Server frames are never masked.
    async fn read_frame(
        &mut self,
        deadline: tokio::time::Instant,
    ) -> Result<(bool, u8, Vec<u8>), AppError> {
        loop {
            if self.pending.len() >= 2 {
                let first = self.pending[0];
                let length_byte = self.pending[1] & 0x7f;
                let (length, header) = match length_byte {
                    126 if self.pending.len() >= 4 => (
                        u16::from_be_bytes([self.pending[2], self.pending[3]]) as usize,
                        4,
                    ),
                    127 if self.pending.len() >= 10 => {
                        let mut bytes = [0u8; 8];
                        bytes.copy_from_slice(&self.pending[2..10]);
                        (u64::from_be_bytes(bytes) as usize, 10)
                    }
                    n if n < 126 => (n as usize, 2),
                    // A length prefix that has not fully arrived yet.
                    _ => {
                        self.fill(deadline).await?;
                        continue;
                    }
                };
                if length > MAX_FRAME_BYTES {
                    return Err(fail("the Codex daemon sent an implausible frame."));
                }
                if self.pending.len() >= header + length {
                    let payload = self.pending[header..header + length].to_vec();
                    self.pending.drain(..header + length);
                    return Ok((first & 0x80 != 0, first & 0x0f, payload));
                }
            }
            self.fill(deadline).await?;
        }
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), AppError> {
        let mut message = json!({ "jsonrpc": "2.0", "method": method });
        if !params.is_null() {
            message["params"] = params;
        }
        self.write_frame(0x1, message.to_string().as_bytes()).await
    }

    /// Sends a request and reads until its answer arrives.
    ///
    /// Notifications stream in alongside — thread status, queue changes — and
    /// are discarded here. Nothing this client asks for needs them; the diff
    /// on screen is how the user watches a turn land.
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, AppError> {
        self.next_id += 1;
        let id = self.next_id;
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.write_frame(0x1, message.to_string().as_bytes()).await?;

        let deadline = tokio::time::Instant::now() + TIMEOUT;
        loop {
            let body = self.read_message(deadline).await?;
            let Ok(value) = serde_json::from_slice::<Value>(&body) else {
                continue;
            };
            if value.get("id").and_then(Value::as_i64) != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                let detail = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("the Codex daemon refused the request");
                return Err(fail(detail.to_owned()));
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    /// The loaded threads whose working directory is `root` or inside it,
    /// newest first.
    ///
    /// `thread/loaded/list` and then one `thread/read` per id, on this one
    /// connection. Not `thread/list` with its directory filter: that only
    /// returns threads that have taken a turn, and the session a user has just
    /// opened and is waiting to use has not.
    ///
    /// "Loaded" is not the same as "somebody is sitting in front of it" — a
    /// thread stays loaded for a while after its TUI exits — which is why the
    /// caller checks the process table before it believes in any of these.
    pub async fn loaded_threads_in(&mut self, root: &Path) -> Result<Vec<LoadedThread>, AppError> {
        let listed = self.request("thread/loaded/list", json!({})).await?;
        let ids: Vec<String> = listed
            .get("data")
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| id.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();

        let mut found = Vec::new();
        for id in ids {
            // A thread that unloads between the list and the read is not an
            // error; it is simply not one of ours any more.
            let Ok(answer) = self.request("thread/read", json!({ "threadId": id })).await else {
                continue;
            };
            let Some(thread) = answer.get("thread") else {
                continue;
            };
            let Some(cwd) = thread.get("cwd").and_then(Value::as_str) else {
                continue;
            };
            if !paths::is_within(Path::new(cwd), root) {
                continue;
            }
            found.push(LoadedThread {
                id,
                cwd: cwd.to_owned(),
                updated_at: thread.get("updatedAt").and_then(Value::as_i64).unwrap_or(0),
            });
        }
        found.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(found)
    }

    /// Hands `text` to `thread` the way the user typing it would.
    ///
    /// `thread/queue/add` is what `codex queue` itself calls. An idle thread
    /// starts a turn on it at once; a busy one takes it up when the current
    /// turn ends, which is exactly what typing into the TUI mid-turn does. It
    /// answers with the queued submission's id, useful only for correlating
    /// logs.
    pub async fn queue(&mut self, thread: &str, text: &str) -> Result<String, AppError> {
        let client_id = hex(&random_16());
        let result = self
            .request(
                "thread/queue/add",
                json!({
                    "threadId": thread,
                    "clientUserMessageId": client_id,
                    "input": [{ "type": "text", "text": text }]
                }),
            )
            .await?;
        Ok(result
            .get("queuedSubmission")
            .and_then(|queued| queued.get("id"))
            .and_then(Value::as_str)
            .unwrap_or(&client_id)
            .to_owned())
    }
}

/// Sixteen bytes nobody can predict, for the handshake key, frame masks, and
/// message ids.
///
/// `/dev/urandom` rather than a crate: this file is compiled into the agent and
/// uploaded to hosts, and sixteen bytes is not worth a dependency. A machine
/// that cannot produce them falls back to the clock, which is weak entropy and
/// entirely adequate — masking exists to defeat caching proxies, and there is
/// no proxy on a unix socket.
fn random_16() -> [u8; 16] {
    use std::io::Read;

    let mut bytes = [0u8; 16];
    // Exactly sixteen bytes, by `read_exact` on an open handle. Reading the
    // *file* would never return: `/dev/urandom` is an endless stream.
    if let Ok(mut source) = std::fs::File::open("/dev/urandom") {
        if source.read_exact(&mut bytes).is_ok() {
            return bytes;
        }
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = (nanos >> (index % 16 * 8)) as u8 ^ (index as u8).wrapping_mul(31);
    }
    bytes
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncBufReadExt;
    use tokio::net::UnixListener;

    #[test]
    fn the_header_terminator_is_found_where_it_is() {
        assert_eq!(find(b"HTTP/1.1 101\r\n\r\nrest", b"\r\n\r\n"), Some(12));
        assert_eq!(find(b"no terminator here", b"\r\n\r\n"), None);
    }

    #[test]
    fn two_reads_never_produce_the_same_mask() {
        // Not a randomness test — just that the fallback path is not returning
        // a constant, which would make every frame mask identical.
        assert_ne!(random_16(), [0u8; 16]);
        assert_ne!(random_16(), random_16());
    }

    /// A daemon of our own: the same handshake, the same framing, and canned
    /// answers for the three requests the client makes. Everything it sees
    /// goes into `seen` so a test can assert on what was asked.
    struct FakeDaemon {
        _dir: tempfile::TempDir,
        path: PathBuf,
        seen: std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
    }

    impl FakeDaemon {
        fn start(threads: Vec<(&'static str, &'static str)>) -> Self {
            // `/tmp` rather than the platform temp dir: a unix socket path is
            // capped near 104 bytes on macOS and the default temp dir is most
            // of that on its own.
            let dir = tempfile::Builder::new()
                .prefix("od-")
                .tempdir_in("/tmp")
                .expect("temp dir");
            let path = dir.path().join("d.sock");
            let listener = UnixListener::bind(&path).expect("bind");
            let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let log = seen.clone();
            tokio::spawn(async move {
                let Ok((stream, _)) = listener.accept().await else { return };
                serve(stream, threads, log).await;
            });
            Self { _dir: dir, path, seen }
        }
    }

    async fn serve(
        stream: UnixStream,
        threads: Vec<(&'static str, &'static str)>,
        seen: std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
    ) {
        let (reader, mut writer) = stream.into_split();
        let mut reader = tokio::io::BufReader::new(reader);

        // The upgrade, ending at the blank line.
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).await.unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
        }
        writer
            .write_all(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n")
            .await
            .expect("101");

        loop {
            // Client frames are masked; read the header, then the payload.
            let mut head = [0u8; 2];
            if reader.read_exact(&mut head).await.is_err() {
                return;
            }
            let opcode = head[0] & 0x0f;
            let mut length = (head[1] & 0x7f) as usize;
            if length == 126 {
                let mut ext = [0u8; 2];
                reader.read_exact(&mut ext).await.expect("len");
                length = u16::from_be_bytes(ext) as usize;
            } else if length == 127 {
                let mut ext = [0u8; 8];
                reader.read_exact(&mut ext).await.expect("len");
                length = u64::from_be_bytes(ext) as usize;
            }
            let mut mask = [0u8; 4];
            reader.read_exact(&mut mask).await.expect("mask");
            let mut payload = vec![0u8; length];
            reader.read_exact(&mut payload).await.expect("payload");
            for (i, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[i % 4];
            }
            if opcode == 0x8 {
                return;
            }
            if opcode != 0x1 {
                continue;
            }
            let message: Value = serde_json::from_slice(&payload).expect("json");
            seen.lock().expect("seen").push(message.clone());
            let Some(id) = message.get("id").cloned() else {
                continue; // a notification needs no answer
            };
            let result = match message.get("method").and_then(Value::as_str) {
                Some("initialize") => json!({ "userAgent": "fake" }),
                Some("thread/loaded/list") => {
                    json!({ "data": threads.iter().map(|(id, _)| *id).collect::<Vec<_>>() })
                }
                Some("thread/read") => {
                    let wanted = message["params"]["threadId"].as_str().unwrap_or("");
                    let (id, cwd) = threads.iter().find(|(id, _)| *id == wanted).expect("known thread");
                    json!({ "thread": { "id": id, "cwd": cwd, "updatedAt": id.len() } })
                }
                Some("thread/queue/add") => json!({
                    "queuedSubmission": {
                        "id": "queued-1",
                        "clientUserMessageId": message["params"]["clientUserMessageId"],
                        "input": message["params"]["input"]
                    }
                }),
                other => json!({ "error": format!("unknown method {other:?}") }),
            };
            // A notification in between, the way the real daemon interleaves
            // them, so the client is proven to skip what it did not ask for.
            let noise = json!({ "jsonrpc": "2.0", "method": "thread/status/changed", "params": {} });
            write_server_frame(&mut writer, noise.to_string().as_bytes()).await;
            let answer = json!({ "jsonrpc": "2.0", "id": id, "result": result });
            write_server_frame(&mut writer, answer.to_string().as_bytes()).await;
        }
    }

    /// Server frames are unmasked.
    async fn write_server_frame(writer: &mut tokio::net::unix::OwnedWriteHalf, payload: &[u8]) {
        let mut frame = vec![0x81u8];
        if payload.len() < 126 {
            frame.push(payload.len() as u8);
        } else {
            frame.push(126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        frame.extend_from_slice(payload);
        writer.write_all(&frame).await.expect("write frame");
    }

    #[tokio::test]
    async fn a_missing_daemon_is_reported_as_not_running() {
        let dir = tempfile::Builder::new().prefix("od-").tempdir_in("/tmp").expect("temp dir");

        let refused = AppServer::connect_to(&dir.path().join("absent.sock")).await;

        let error = refused.err().expect("refused");
        assert_eq!(error.tag(), "CodexChannelError");
        assert_eq!(error.message(), DAEMON_NOT_RUNNING_MESSAGE);
    }

    #[tokio::test]
    async fn the_threads_in_a_repository_are_found_newest_first_and_others_ignored() {
        let daemon = FakeDaemon::start(vec![
            ("older", "/w/repo/api"),
            ("elsewhere", "/w/other"),
            ("newest-one", "/w/repo"),
            ("sibling", "/w/repo-two"),
        ]);

        let mut server = AppServer::connect_to(&daemon.path).await.expect("connect");
        let threads = server.loaded_threads_in(Path::new("/w/repo")).await.expect("threads");

        let ids: Vec<&str> = threads.iter().map(|thread| thread.id.as_str()).collect();
        // `updatedAt` is faked as the id's length, so the longer id is newer.
        assert_eq!(ids, vec!["newest-one", "older"]);
        assert_eq!(threads[0].cwd, "/w/repo");
    }

    #[tokio::test]
    async fn a_message_is_queued_the_way_codex_queue_would() {
        let daemon = FakeDaemon::start(vec![("t1", "/w/repo")]);

        let mut server = AppServer::connect_to(&daemon.path).await.expect("connect");
        let queued = server.queue("t1", "about this line").await.expect("queued");

        assert_eq!(queued, "queued-1");
        let seen = daemon.seen.lock().expect("seen");
        let add = seen
            .iter()
            .find(|message| message["method"] == "thread/queue/add")
            .expect("the queue request was made");
        assert_eq!(add["params"]["threadId"], "t1");
        assert_eq!(add["params"]["input"][0]["text"], "about this line");
        assert!(
            add["params"]["clientUserMessageId"].as_str().is_some_and(|id| id.len() == 32),
            "a client message id is generated: {add}"
        );
        // The handshake happened before anything else was asked.
        assert_eq!(seen[0]["method"], "initialize");
        assert_eq!(seen[1]["method"], "initialized");
    }
}
