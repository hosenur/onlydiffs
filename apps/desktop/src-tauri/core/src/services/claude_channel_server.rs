//! The Claude Code side of the channel: the MCP server the agent binary runs
//! as `onlydiffs-agent channel`.
//!
//! Claude Code spawns it once per session and speaks MCP to it over stdio.
//! Towards the app it listens on a unix socket in `~/.onlydiffs/claude-channels`,
//! and every message that arrives there goes out on stdout as a
//! `notifications/claude/channel` notification, which is the whole of what a
//! channel is. The MCP surface a channel needs is four messages — `initialize`,
//! an empty `tools/list`, `ping`, and that one notification — which is why this
//! is a few hundred lines of Rust rather than a runtime and an SDK on every
//! machine a repository lives on.
//!
//! One socket connection carries one message: the client writes the text,
//! closes its side, and reads one JSON line back. No port, no token. The
//! directory is `0700` and the socket `0600`, so whoever can connect is the
//! user, and a socket whose process has gone refuses the connection, which is
//! all the liveness check the app needs.
//!
//! What this cannot know is whether Claude Code has *registered* the channel.
//! A session started without `--dangerously-load-development-channels` still
//! spawns this server and still reads the notification off stdout, and then
//! drops it without a word. The app reads Claude Code's own log for that; see
//! `claude_channel`. The one thing this side does to help is announce its
//! socket on stderr, which Claude Code copies into that log and which is what
//! ties a log file to a registration.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

/// Where registrations live, relative to the home directory.
pub const REGISTRATIONS_DIR: &str = ".onlydiffs/claude-channels";

/// The registration shape this server writes and the app reads. Version 2 is
/// the socket form; the port-and-token form it replaced was version 1, and a
/// file of that version is ignored rather than misread.
pub const SCHEMA_VERSION: u32 = 2;

/// The name the server is registered under with `claude mcp add`, which is
/// also what `--dangerously-load-development-channels server:<name>` names.
pub const MCP_SERVER_NAME: &str = "onlydiffs";

/// The newest MCP protocol revision this server will agree to. Claude Code
/// declines to register a channel that negotiates a newer one, so a client
/// asking for more is answered with this.
const MAX_PROTOCOL_VERSION: &str = "2025-11-25";

/// The most a message may be. Matches the app's own limit.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// What Claude Code is told about the channel when it connects.
const INSTRUCTIONS: &str = "Messages from the local OnlyDiffs git diff viewer arrive as channel events. \
Treat them as requests from the user working in this repository. The channel is \
one-way: act on the message in this session as you normally would. There is no \
reply tool and nothing is waiting on a response.";

/// One running channel, as written beside its socket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Registration {
    pub schema_version: u32,
    pub pid: u32,
    /// The session's working directory, which is what the app matches a
    /// repository against.
    pub cwd: String,
    /// Absolute path of the socket.
    pub socket: String,
    /// Milliseconds since the epoch.
    pub started_at: u64,
}

pub fn registrations_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(REGISTRATIONS_DIR)
}

/// The line the server prints on stderr when it is up. Claude Code copies a
/// server's stderr into its MCP log, and this line is what lets the app find
/// the log that belongs to a particular registration.
pub fn announcement(cwd: &Path, socket: &Path) -> String {
    format!(
        "onlydiffs channel: listening for {} on {}",
        cwd.display(),
        socket.display()
    )
}

/// The registration's files, removed when this is dropped. Held for the life
/// of `serve`, so every way out of it — stdin closing, a signal, a panic that
/// unwinds — takes the files with it.
struct Files {
    socket: PathBuf,
    meta: PathBuf,
}

impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
        let _ = std::fs::remove_file(&self.meta);
    }
}

/// Runs the server on this process's stdio until stdin closes or a signal
/// arrives. This is the whole of `onlydiffs-agent channel`.
pub async fn run() -> std::io::Result<()> {
    let cwd = std::env::current_dir()?;
    let state_dir = registrations_dir();
    let stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let stdout = tokio::io::stdout();
    let served = serve(stdin, stdout, &state_dir, &cwd, std::process::id());
    tokio::pin!(served);

    // Claude Code stops a server with SIGINT, which would end the process
    // without running the cleanup below; catching it is what keeps a closed
    // session from leaving a socket behind. `Files` is dropped when `serve`
    // is, which happens when this function returns either way.
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        result = &mut served => result,
        _ = tokio::signal::ctrl_c() => Ok(()),
        _ = terminate.recv() => Ok(()),
    }
}

/// Serves MCP on `input`/`output` and messages on a socket under `state_dir`,
/// until `input` ends. Everything `run` does, with the endpoints injectable so
/// a test can drive it in-process.
pub async fn serve<R, W>(
    input: R,
    output: W,
    state_dir: &Path,
    cwd: &Path,
    pid: u32,
) -> std::io::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    tokio::fs::create_dir_all(state_dir).await?;
    restrict(state_dir, 0o700).await;

    let files = Files {
        socket: state_dir.join(format!("{pid}.sock")),
        meta: state_dir.join(format!("{pid}.json")),
    };
    // A socket left by a previous process with this pid would refuse the bind.
    let _ = tokio::fs::remove_file(&files.socket).await;
    let listener = UnixListener::bind(&files.socket)?;
    restrict(&files.socket, 0o600).await;

    let registration = Registration {
        schema_version: SCHEMA_VERSION,
        pid,
        cwd: cwd.to_string_lossy().into_owned(),
        socket: files.socket.to_string_lossy().into_owned(),
        started_at: now_millis(),
    };
    tokio::fs::write(&files.meta, serde_json::to_vec(&registration)?).await?;
    restrict(&files.meta, 0o600).await;
    eprintln!("{}", announcement(cwd, &files.socket));

    // Every line to Claude Code goes through one writer, so a notification
    // and a response can never interleave on the pipe.
    let (lines, mut queued) = mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        let mut output = output;
        while let Some(line) = queued.recv().await {
            if output.write_all(line.as_bytes()).await.is_err()
                || output.write_all(b"\n").await.is_err()
                || output.flush().await.is_err()
            {
                return;
            }
        }
    });

    let repository = cwd
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let sequence = Arc::new(AtomicU64::new(0));
    let acceptor = {
        let lines = lines.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let lines = lines.clone();
                let repository = repository.clone();
                let sequence = sequence.clone();
                tokio::spawn(async move {
                    relay(stream, &lines, &repository, &sequence).await;
                });
            }
        })
    };

    let result = answer_mcp(input, &lines).await;

    acceptor.abort();
    drop(lines);
    let _ = writer.await;
    drop(files);
    result
}

/// The socket half: one message in, one JSON line out.
async fn relay(
    mut stream: UnixStream,
    lines: &mpsc::UnboundedSender<String>,
    repository: &str,
    sequence: &AtomicU64,
) {
    let mut raw = Vec::new();
    let read = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        (&mut stream).take(MAX_MESSAGE_BYTES as u64 + 1).read_to_end(&mut raw),
    )
    .await;
    let reply = match read {
        Err(_) | Ok(Err(_)) => json!({ "error": "the message did not arrive in time" }),
        Ok(Ok(_)) if raw.len() > MAX_MESSAGE_BYTES => json!({ "error": "message is too large" }),
        Ok(Ok(_)) => {
            let content = String::from_utf8_lossy(&raw).trim().to_owned();
            if content.is_empty() {
                json!({ "error": "message is empty" })
            } else {
                let message_id = format!(
                    "onlydiffs-{}-{}",
                    now_millis(),
                    sequence.fetch_add(1, Ordering::Relaxed) + 1
                );
                let notification = json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/claude/channel",
                    "params": {
                        "content": content,
                        // Keys must be identifiers: Claude Code drops any other.
                        "meta": { "repository": repository, "message_id": message_id }
                    }
                });
                if lines.send(notification.to_string()).is_err() {
                    json!({ "error": "channel transport is unavailable" })
                } else {
                    json!({ "messageId": message_id })
                }
            }
        }
    };
    let _ = stream.write_all(format!("{reply}\n").as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// The MCP half: newline-delimited JSON-RPC in, answers out, until EOF.
async fn answer_mcp<R: AsyncBufRead + Unpin>(
    mut input: R,
    lines: &mpsc::UnboundedSender<String>,
) -> std::io::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let Ok(message) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if let Some(reply) = respond(&message) {
            if lines.send(reply.to_string()).is_err() {
                return Ok(());
            }
        }
    }
}

/// The answer to one MCP message, or `None` for a notification.
fn respond(message: &Value) -> Option<Value> {
    let id = message.get("id")?.clone();
    let method = message.get("method").and_then(Value::as_str).unwrap_or_default();
    let result = match method {
        "initialize" => {
            let requested = message
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(MAX_PROTOCOL_VERSION);
            // Revisions are dates, so the string order is the version order.
            let agreed = if requested < MAX_PROTOCOL_VERSION {
                requested
            } else {
                MAX_PROTOCOL_VERSION
            };
            json!({
                "protocolVersion": agreed,
                "capabilities": { "experimental": { "claude/channel": {} }, "tools": {} },
                "serverInfo": { "name": MCP_SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS
            })
        }
        "tools/list" => json!({ "tools": [] }),
        "ping" => json!({}),
        other => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("method not found: {other}") }
            }))
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(unix)]
async fn restrict(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).await;
}

#[cfg(not(unix))]
async fn restrict(_path: &Path, _mode: u32) {}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, BufReader};

    /// A server on in-memory stdio, in a socket directory of its own.
    struct Running {
        dir: tempfile::TempDir,
        /// Claude Code's end of stdin: writing here is what Claude would send.
        to_server: tokio::io::DuplexStream,
        /// Claude Code's end of stdout: what the server says arrives here.
        from_server: BufReader<tokio::io::DuplexStream>,
        task: tokio::task::JoinHandle<std::io::Result<()>>,
    }

    impl Running {
        async fn start() -> Self {
            // `/tmp`: a unix socket path is capped near 104 bytes on macOS.
            let dir = tempfile::Builder::new().prefix("od-").tempdir_in("/tmp").expect("dir");
            let (to_server, server_stdin) = duplex(64 * 1024);
            let (server_stdout, from_server) = duplex(64 * 1024);
            let state = dir.path().to_path_buf();
            let task = tokio::spawn(async move {
                serve(BufReader::new(server_stdin), server_stdout, &state, Path::new("/w/repo"), 4242).await
            });
            let running = Self {
                dir,
                to_server,
                from_server: BufReader::new(from_server),
                task,
            };
            running.await_registration().await;
            running
        }

        async fn await_registration(&self) {
            for _ in 0..100 {
                if self.dir.path().join("4242.json").is_file() {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            panic!("the server never registered");
        }

        fn registration(&self) -> Registration {
            serde_json::from_slice(&std::fs::read(self.dir.path().join("4242.json")).expect("meta"))
                .expect("registration")
        }

        async fn say(&mut self, message: Value) {
            self.to_server
                .write_all(format!("{message}\n").as_bytes())
                .await
                .expect("write to server");
        }

        async fn hear(&mut self) -> Value {
            let mut line = String::new();
            tokio::time::timeout(std::time::Duration::from_secs(5), self.from_server.read_line(&mut line))
                .await
                .expect("the server answers in time")
                .expect("read");
            serde_json::from_str(line.trim()).expect("json line")
        }
    }

    /// What the app does: connect, write, close the write side, read a line.
    async fn send_over_socket(socket: &str, text: &str) -> Value {
        let mut stream = UnixStream::connect(socket).await.expect("connect");
        stream.write_all(text.as_bytes()).await.expect("write");
        stream.shutdown().await.expect("shutdown");
        let mut reply = String::new();
        stream.read_to_string(&mut reply).await.expect("read reply");
        serde_json::from_str(reply.trim()).expect("json reply")
    }

    #[tokio::test]
    async fn claude_is_told_this_is_a_channel() {
        let mut server = Running::start().await;

        server
            .say(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                         "params": { "protocolVersion": "2026-07-28", "capabilities": {} } }))
            .await;
        let answer = server.hear().await;

        assert_eq!(answer["id"], 1);
        assert!(answer["result"]["capabilities"]["experimental"]["claude/channel"].is_object());
        // Newer than we support is answered with what we support, since Claude
        // Code will not register a channel on the 2026-07-28 revision.
        assert_eq!(answer["result"]["protocolVersion"], MAX_PROTOCOL_VERSION);
        assert_eq!(answer["result"]["serverInfo"]["name"], "onlydiffs");
    }

    #[tokio::test]
    async fn the_rest_of_the_mcp_surface_is_answered_and_notifications_are_not() {
        let mut server = Running::start().await;

        server.say(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;
        server.say(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).await;
        assert_eq!(server.hear().await["result"]["tools"], json!([]));

        server.say(json!({ "jsonrpc": "2.0", "id": 3, "method": "ping" })).await;
        assert_eq!(server.hear().await["id"], 3);

        server.say(json!({ "jsonrpc": "2.0", "id": 4, "method": "prompts/list" })).await;
        assert_eq!(server.hear().await["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn a_message_on_the_socket_becomes_a_channel_notification() {
        let mut server = Running::start().await;
        let registration = server.registration();
        assert_eq!(registration.schema_version, SCHEMA_VERSION);
        assert_eq!(registration.cwd, "/w/repo");
        assert_eq!(registration.pid, 4242);

        let reply = send_over_socket(&registration.socket, "src/main.rs:42 why is this hidden?").await;
        let message_id = reply["messageId"].as_str().expect("a message id").to_owned();

        let notification = server.hear().await;
        assert_eq!(notification["method"], "notifications/claude/channel");
        assert_eq!(notification["params"]["content"], "src/main.rs:42 why is this hidden?");
        assert_eq!(notification["params"]["meta"]["repository"], "repo");
        assert_eq!(notification["params"]["meta"]["message_id"], message_id);
    }

    #[tokio::test]
    async fn an_empty_or_oversized_message_is_refused_without_reaching_claude() {
        let mut server = Running::start().await;
        let socket = server.registration().socket;

        assert_eq!(send_over_socket(&socket, "   ").await["error"], "message is empty");
        assert_eq!(
            send_over_socket(&socket, &"x".repeat(MAX_MESSAGE_BYTES + 1)).await["error"],
            "message is too large"
        );

        // Nothing went to Claude: the next thing it hears is the ping answer.
        server.say(json!({ "jsonrpc": "2.0", "id": 9, "method": "ping" })).await;
        assert_eq!(server.hear().await["id"], 9);
    }

    #[tokio::test]
    async fn a_bare_connection_is_a_liveness_probe_not_a_message() {
        // The app checks a session is there by connecting and closing.
        let mut server = Running::start().await;
        let socket = server.registration().socket;

        let stream = UnixStream::connect(&socket).await.expect("connect");
        drop(stream);

        server.say(json!({ "jsonrpc": "2.0", "id": 5, "method": "ping" })).await;
        assert_eq!(server.hear().await["id"], 5, "no notification was forged from an empty connection");
    }

    #[tokio::test]
    async fn closing_stdin_ends_the_server_and_removes_its_files() {
        let server = Running::start().await;
        let registration = server.registration();
        assert!(Path::new(&registration.socket).exists());

        drop(server.to_server);
        server.task.await.expect("join").expect("clean exit");

        assert!(!Path::new(&registration.socket).exists(), "socket removed");
        assert!(!server.dir.path().join("4242.json").exists(), "registration removed");
    }
}
