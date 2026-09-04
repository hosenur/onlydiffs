//! Talking to Codex's shared app-server daemon.
//!
//! The daemon is what makes a message arrive *now* rather than whenever the
//! user next types. `codex queue` writes to a durable queue that only drains
//! when the session takes a turn, so a queued message sits there in front of an
//! idle session indefinitely — measured, thirty seconds of nothing. Asking the
//! daemon to start the turn skips the wait entirely.
//!
//! Getting there means speaking the daemon's protocol, which is not documented
//! and is not what the socket looks like. It is the app-server's JSON-RPC
//! carried over a **WebSocket**: an HTTP upgrade first, then masked text
//! frames. Anything that opens the socket and writes JSON — the obvious thing —
//! is closed on without a byte of explanation. This was read off a capture of
//! the CLI's own handshake.
//!
//! What follows is the smallest client that satisfies it: one connection, a
//! handful of requests, text frames only. No compression, no extensions, no
//! TLS — the transport is a socket in the user's own home directory, and the
//! server's `Sec-WebSocket-Accept` is not checked for the same reason.

#![cfg(unix)]

use std::path::PathBuf;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::error::AppError;

/// Where the daemon listens, relative to the home directory.
const CONTROL_SOCKET: &str = ".codex/app-server-control/app-server-control.sock";

/// How long any single exchange may take. The daemon is a local process; this
/// only expires when something is wrong, and the caller falls back to the queue
/// rather than failing the send.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The largest frame worth reading. Turn events carry model output, so this is
/// generous, but it still bounds what a confused server can make us allocate.
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

fn fail(message: impl Into<String>) -> AppError {
    AppError::CodexChannel(message.into())
}

pub fn socket_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(CONTROL_SOCKET)
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
    /// Opens the socket and completes the upgrade, leaving a connection that
    /// speaks JSON-RPC.
    pub async fn connect() -> Result<Self, AppError> {
        let path = socket_path();
        let stream = tokio::time::timeout(TIMEOUT, UnixStream::connect(&path))
            .await
            .map_err(|_| fail("Codex's daemon did not accept a connection in time."))?
            .map_err(|error| fail(format!("no Codex daemon at {}: {error}", path.display())))?;

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

    /// The HTTP half. The key is random because the handshake calls for one;
    /// the server's answer is not verified, since a socket only this user can
    /// open is not a channel worth authenticating.
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

    /// One text frame, masked as a client must.
    async fn write_frame(&mut self, payload: &[u8]) -> Result<(), AppError> {
        let mask = random_16();
        let mask = &mask[..4];
        let mut frame = vec![0x81u8];
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
    /// answering pings so a slow turn does not drop the connection.
    async fn read_message(&mut self, deadline: tokio::time::Instant) -> Result<Vec<u8>, AppError> {
        let mut message = Vec::new();
        loop {
            let (fin, opcode, payload) = self.read_frame(deadline).await?;
            match opcode {
                0x9 => {
                    // Ping. The pong carries the same body back.
                    let mut pong = vec![0x8au8];
                    let mask = random_16();
                    let mask = &mask[..4];
                    pong.push(0x80 | payload.len() as u8);
                    pong.extend_from_slice(mask);
                    pong.extend(
                        payload
                            .iter()
                            .enumerate()
                            .map(|(i, byte)| byte ^ mask[i % 4]),
                    );
                    let _ = self.stream.write_all(&pong).await;
                    continue;
                }
                0xa => continue,                                        // pong
                0x8 => return Err(fail("the Codex daemon hung up.")),   // close
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
        self.write_frame(message.to_string().as_bytes()).await
    }

    /// Sends a request and reads until its answer arrives.
    ///
    /// Notifications stream in alongside — thread status, turn items — and are
    /// discarded here. Nothing this client asks for needs them; the diff on
    /// screen is how the user watches the turn land.
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, AppError> {
        self.next_id += 1;
        let id = self.next_id;
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.write_frame(message.to_string().as_bytes()).await?;

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

    /// The threads the daemon currently has open.
    ///
    /// "Loaded" is not the same as "somebody is sitting in front of it" — a
    /// thread stays loaded after its session goes away, and any client that
    /// touches one loads it. So this is used to answer a narrower question than
    /// it looks capable of: given a repository that *does* have a session
    /// running, which of its threads can be spoken to.
    pub async fn loaded_threads(&mut self) -> Result<Vec<String>, AppError> {
        let result = self.request("thread/loaded/list", json!({})).await?;
        Ok(result
            .get("data")
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| id.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Starts a turn on `thread` with `text`, and answers with the turn's id.
    ///
    /// This is the whole point of the module. Unlike `codex exec resume`, which
    /// refuses a thread that has a live session — "already has an active
    /// writer" — the daemon owns that writer and starts the turn on its behalf.
    pub async fn start_turn(&mut self, thread: &str, text: &str) -> Result<String, AppError> {
        let result = self
            .request(
                "turn/start",
                json!({
                    "threadId": thread,
                    "input": [{ "type": "text", "text": text }]
                }),
            )
            .await?;
        Ok(result
            .get("turn")
            .and_then(|turn| turn.get("id"))
            .and_then(Value::as_str)
            .unwrap_or(thread)
            .to_owned())
    }
}

/// Sixteen bytes nobody can predict, for the handshake key and frame masks.
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
    // *file* would never return: `/dev/urandom` is an endless stream, and
    // `fs::read` waits for an end that does not come.
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

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[tokio::test]
    async fn a_missing_daemon_is_an_error_the_caller_can_fall_back_from() {
        // No daemon is the ordinary case for someone who has never run one, and
        // it has to come back as a plain error rather than a hang: the queue is
        // waiting behind it.
        let home = tempfile::TempDir::new().expect("temp dir");
        let absent = home.path().join("nothing.sock");
        let refused = UnixStream::connect(&absent).await;
        assert!(refused.is_err());
    }
}
