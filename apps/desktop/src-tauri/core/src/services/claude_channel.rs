//! The bridge to a Claude Code session running in the open repository.
//!
//! Each session runs the channel server (`claude_channel_server`) as an MCP
//! server, and that server registers itself: a socket in
//! `~/.onlydiffs/claude-channels` and a JSON file beside it naming the session's
//! working directory. This reads those, picks the ones belonging to the
//! repository on screen, and hands a message over the socket.
//!
//! Liveness is the socket. A session that has gone leaves files behind — a
//! `kill -9`, an OOM kill, a crash, none of which run cleanup — but its socket
//! refuses the connection, and a registration that refuses is deleted on the
//! spot. There is no pid to check and no token to carry; the directory is
//! `0700` and that is the authentication.
//!
//! Reachable is a second question, and the one that matters. Claude Code
//! delivers channel messages only to a session started with the channel flag,
//! and a session started without it still runs the server, still takes the
//! notification, and drops it without a word to anyone. The only record of
//! which happened is Claude Code's own MCP log, so that is what gets read: the
//! server announces its socket on stderr, Claude Code copies stderr into the
//! log, and the log that carries the announcement is the log that says whether
//! the channel was registered or skipped.
//!
//! It runs where the repository is. A session reviewing a checkout on a build
//! box is a process *on that build box*, with a socket in *that machine's*
//! home directory — so reading the registry from the user's laptop would find
//! nothing, and finding something would be worse.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, UNIX_EPOCH};

use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::contract::ClaudeChannelStatus;
use crate::error::AppError;
use crate::services::claude_channel_server::{
    registrations_dir, Registration, MCP_SERVER_NAME, SCHEMA_VERSION,
};
use crate::services::paths;

const MAX_MESSAGE_BYTES: usize = 64 * 1024;
/// How long a session gets to take a message. It is a local socket to a
/// process that answers in microseconds; this only expires when something is
/// wrong.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a liveness probe waits to connect.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
/// How far before a registration's start Claude Code's log may have been
/// written and still be the log of that session. The server is spawned at
/// session start and registers within a second; the margin is for a slow disk.
const LOG_START_MARGIN: Duration = Duration::from_secs(60);
/// The most of a log worth reading. The two lines that matter are written at
/// connection time, at the very top.
const MAX_LOG_BYTES: u64 = 512 * 1024;

/// What a user runs to open a session this app can reach.
pub const START_COMMAND: &str =
    "claude --dangerously-load-development-channels server:onlydiffs";

const NO_CHANNEL_MESSAGE: &str = concat!(
    "No Claude Code session is running in this repository. ",
    "Start one there with `claude --dangerously-load-development-channels server:onlydiffs`."
);

/// A session is running, and Claude Code is throwing the channel away.
const UNREGISTERED_MESSAGE: &str = concat!(
    "Claude Code is running here but was started without the channel flag, so it drops every message. ",
    "Close it, then run `claude --dangerously-load-development-channels server:onlydiffs`."
);

/// One live session for the repository.
#[derive(Debug, Clone)]
struct Channel {
    registration: Registration,
    /// What Claude Code's log says: `Some(true)` registered, `Some(false)`
    /// skipped, `None` when no log could be tied to this session — treated as
    /// reachable, because a log format this app does not recognise must not
    /// turn every send into a refusal.
    registered: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reply {
    message_id: Option<String>,
    error: Option<String>,
}

/// The channel's own id format: what `/^[A-Za-z0-9_-]+$/` accepted.
fn is_valid_message_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Whether the socket answers. Connecting is the whole probe; the server
/// treats a connection that says nothing as exactly that.
async fn is_live(socket: &str) -> bool {
    matches!(
        tokio::time::timeout(PROBE_TIMEOUT, UnixStream::connect(socket)).await,
        Ok(Ok(_))
    )
}

/// The live channels for the repository under `dir`, newest first. Dead
/// registrations are removed as they are found.
async fn channels_in(dir: &Path, root: &Path, caches: &[PathBuf]) -> Vec<Channel> {
    let repo_path = paths::normalize(root);
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return Vec::new();
    };

    let mut live = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        // A half-written or stale file is not an error worth showing; the next
        // candidate may well be live.
        let Ok(body) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        let Ok(registration) = serde_json::from_str::<Registration>(&body) else {
            // Not this shape at all. The port-and-token registrations the old
            // channel script wrote never had a `socket` field, land here, and
            // have nobody behind them: that script is gone. Anything newer
            // than this app would parse, so what fails to is safe to remove.
            let _ = tokio::fs::remove_file(&path).await;
            continue;
        };
        if registration.schema_version < SCHEMA_VERSION {
            let _ = tokio::fs::remove_file(&path).await;
            continue;
        }
        if registration.schema_version != SCHEMA_VERSION {
            continue;
        }
        if paths::normalize(Path::new(&registration.cwd)) != repo_path {
            continue;
        }
        if !is_live(&registration.socket).await {
            // Nobody is behind it. Removing it here is what keeps the
            // directory from filling with the remains of every session that
            // was ever killed rather than closed.
            let _ = tokio::fs::remove_file(&registration.socket).await;
            let _ = tokio::fs::remove_file(&path).await;
            continue;
        }
        let registered = registered_in(caches, &registration).await;
        live.push(Channel {
            registration,
            registered,
        });
    }

    live.sort_by(|a, b| b.registration.started_at.cmp(&a.registration.started_at));
    live
}

async fn channels(root: &Path) -> Vec<Channel> {
    channels_in(&registrations_dir(), root, &log_caches()).await
}

/// Sends a user-authored message into the live Claude Code session for this
/// repository. One direction only: the message is handed over and that is the
/// end of it — whatever Claude does next happens in Claude Code, not here.
///
/// Resolves with the channel's message id, which is only useful for correlating
/// logs.
pub async fn send(root: &Path, raw_message: &str) -> Result<String, AppError> {
    let fail = AppError::ClaudeChannel;

    let message = raw_message.trim();
    if message.is_empty() {
        return Err(fail("Message cannot be empty.".into()));
    }
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(fail(format!(
            "Message is too large (maximum {MAX_MESSAGE_BYTES} bytes)."
        )));
    }

    let channels = channels(root).await;
    if channels.is_empty() {
        return Err(fail(NO_CHANNEL_MESSAGE.into()));
    }
    let reachable: Vec<&Channel> = channels
        .iter()
        .filter(|channel| channel.registered != Some(false))
        .collect();
    if reachable.is_empty() {
        return Err(fail(UNREGISTERED_MESSAGE.into()));
    }

    let mut last_error = String::new();
    for channel in reachable {
        // A channel that fails mid-send is expected — try the next one before
        // giving up.
        match deliver(&channel.registration.socket, message).await {
            Ok(message_id) => return Ok(message_id),
            Err(detail) => last_error = detail,
        }
    }
    Err(fail(format!(
        "Could not reach a Claude Code session for this repository. Last error: {last_error}"
    )))
}

/// One message over one connection: write, close the write side, read the
/// single JSON line back.
async fn deliver(socket: &str, message: &str) -> Result<String, String> {
    let exchange = async {
        let mut stream = UnixStream::connect(socket)
            .await
            .map_err(|error| format!("could not connect to the channel: {error}"))?;
        stream
            .write_all(message.as_bytes())
            .await
            .map_err(|error| format!("could not write to the channel: {error}"))?;
        stream
            .shutdown()
            .await
            .map_err(|error| format!("could not finish writing to the channel: {error}"))?;
        let mut reply = String::new();
        (&mut stream)
            .take(4 * 1024)
            .read_to_string(&mut reply)
            .await
            .map_err(|error| format!("the channel gave no answer: {error}"))?;
        Ok::<String, String>(reply)
    };
    let reply = match tokio::time::timeout(SEND_TIMEOUT, exchange).await {
        Ok(result) => result?,
        Err(_) => return Err("the channel did not answer in time".into()),
    };

    let parsed: Reply = serde_json::from_str(reply.trim())
        .map_err(|error| format!("the channel answered with something unexpected: {error}"))?;
    if let Some(error) = parsed.error {
        return Err(format!("the channel refused the message: {error}"));
    }
    match parsed.message_id {
        Some(id) if is_valid_message_id(&id) => Ok(id),
        _ => Err("the channel returned an invalid message id".into()),
    }
}

/// Whether a Claude Code session is listening for this repository, and
/// whether it would act on a message.
///
/// Reports rather than throws: "no channel" is an ordinary state for a status
/// indicator, not a failure, and it is polled often enough that turning it into
/// an error would just mean catching it again.
pub async fn status(root: &Path) -> ClaudeChannelStatus {
    let channels = channels(root).await;
    let unregistered = channels
        .iter()
        .filter(|channel| channel.registered == Some(false))
        .count();
    ClaudeChannelStatus {
        connected: channels.len() > unregistered,
        sessions: channels.len(),
        unregistered,
    }
}

/// Where Claude Code keeps its MCP logs, on the platforms it runs on. Both
/// are checked on every platform: it costs a directory lookup that fails.
fn log_caches() -> Vec<PathBuf> {
    let mut caches = Vec::new();
    if let Some(home) = dirs::home_dir() {
        caches.push(home.join("Library/Caches/claude-cli-nodejs"));
    }
    match std::env::var_os("XDG_CACHE_HOME") {
        Some(xdg) if !xdg.is_empty() => caches.push(PathBuf::from(xdg).join("claude-cli-nodejs")),
        _ => {
            if let Some(home) = dirs::home_dir() {
                caches.push(home.join(".cache/claude-cli-nodejs"));
            }
        }
    }
    caches
}

/// How Claude Code names a project's cache directory: the working directory
/// with every character that is not a letter or a digit replaced by a dash.
/// `/home/me/repo` becomes `-home-me-repo`.
fn project_slug(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Verdicts already read, by socket path. A log is written once at connection
/// time, so a definite answer never changes for the life of the registration —
/// and a four-second poll must not reread every log every time.
fn verdicts() -> &'static Mutex<HashMap<String, bool>> {
    static CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// What Claude Code's log says about this registration's channel.
///
/// The log is found by the announcement the server printed on stderr, which
/// Claude Code copies into the log of the session that spawned it. Only logs
/// touched since the registration started are opened, which is what keeps this
/// from reading the log of every session that ever ran in the directory.
async fn registered_in(caches: &[PathBuf], registration: &Registration) -> Option<bool> {
    if let Some(known) = verdicts()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&registration.socket).copied())
    {
        return Some(known);
    }

    let verdict = read_verdict(caches, registration).await;
    if let (Some(verdict), Ok(mut cache)) = (verdict, verdicts().lock()) {
        cache.insert(registration.socket.clone(), verdict);
    }
    verdict
}

async fn read_verdict(caches: &[PathBuf], registration: &Registration) -> Option<bool> {
    let slug = project_slug(&registration.cwd);
    let not_before = UNIX_EPOCH
        + Duration::from_millis(registration.started_at)
            .saturating_sub(LOG_START_MARGIN);
    // The announcement, as it appears inside the log's JSON string.
    let marker = format!(" on {}", registration.socket);

    for cache in caches {
        let dir = cache.join(&slug).join(format!("mcp-logs-{MCP_SERVER_NAME}"));
        let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
                continue;
            }
            let touched = entry
                .metadata()
                .await
                .and_then(|meta| meta.modified())
                .unwrap_or(UNIX_EPOCH);
            if touched < not_before {
                continue;
            }
            let Some(body) = read_head(&path).await else {
                continue;
            };
            if !body.contains(&marker) {
                continue;
            }
            return verdict_in(&body);
        }
    }
    None
}

/// The two lines Claude Code writes when it decides about a channel.
fn verdict_in(log: &str) -> Option<bool> {
    if log.contains("Channel notifications registered") {
        Some(true)
    } else if log.contains("not in --channels list") {
        Some(false)
    } else {
        None
    }
}

async fn read_head(path: &Path) -> Option<String> {
    let file = tokio::fs::File::open(path).await.ok()?;
    let mut raw = Vec::new();
    file.take(MAX_LOG_BYTES).read_to_end(&mut raw).await.ok()?;
    Some(String::from_utf8_lossy(&raw).into_owned())
}

#[cfg(test)]
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixListener;

    fn registration(dir: &Path, name: &str, cwd: &str, socket: &Path) -> Registration {
        let registration = Registration {
            schema_version: SCHEMA_VERSION,
            pid: 1,
            cwd: cwd.to_owned(),
            socket: socket.to_string_lossy().into_owned(),
            started_at: now_millis(),
        };
        std::fs::write(
            dir.join(format!("{name}.json")),
            serde_json::to_vec(&registration).expect("json"),
        )
        .expect("write registration");
        registration
    }

    /// A session standing in for Claude Code's end: answers one message with
    /// the given reply and records what it received.
    fn fake_session(dir: &Path, name: &str, reply: &'static str) -> (PathBuf, std::sync::Arc<Mutex<Vec<String>>>) {
        let socket = dir.join(format!("{name}.sock"));
        let listener = UnixListener::bind(&socket).expect("bind");
        let received = std::sync::Arc::new(Mutex::new(Vec::new()));
        let log = received.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else { return };
                let mut body = String::new();
                let _ = stream.read_to_string(&mut body).await;
                if !body.is_empty() {
                    log.lock().expect("log").push(body);
                }
                let _ = stream.write_all(format!("{reply}\n").as_bytes()).await;
            }
        });
        (socket, received)
    }

    fn temp() -> tempfile::TempDir {
        // `/tmp`: a unix socket path is capped near 104 bytes on macOS.
        tempfile::Builder::new().prefix("od-").tempdir_in("/tmp").expect("temp dir")
    }

    #[tokio::test]
    async fn a_live_session_is_counted_and_a_dead_registration_is_pruned() {
        let dir = temp();
        let (socket, _) = fake_session(dir.path(), "live", r#"{"messageId":"m1"}"#);
        registration(dir.path(), "live", "/w/repo", &socket);
        registration(dir.path(), "dead", "/w/repo", &dir.path().join("gone.sock"));
        registration(dir.path(), "elsewhere", "/w/other", &socket);

        let found = channels_in(dir.path(), Path::new("/w/repo"), &[]).await;

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].registration.socket, socket.to_string_lossy());
        assert!(!dir.path().join("dead.json").exists(), "a registration nobody answers is removed");
        assert!(dir.path().join("live.json").exists());
        assert!(dir.path().join("elsewhere.json").exists(), "another repository's session is left alone");
    }

    #[tokio::test]
    async fn a_message_reaches_the_session_and_its_id_comes_back() {
        let dir = temp();
        let (socket, received) = fake_session(dir.path(), "s", r#"{"messageId":"onlydiffs-1-1"}"#);

        let id = deliver(&socket.to_string_lossy(), "src/main.rs:42 why is this hidden?")
            .await
            .expect("delivered");

        assert_eq!(id, "onlydiffs-1-1");
        assert_eq!(received.lock().expect("log").as_slice(), ["src/main.rs:42 why is this hidden?"]);
    }

    #[tokio::test]
    async fn a_refusal_from_the_session_is_reported_as_one() {
        let dir = temp();
        let (socket, _) = fake_session(dir.path(), "s", r#"{"error":"message is too large"}"#);

        let refused = deliver(&socket.to_string_lossy(), "x").await.expect_err("refused");

        assert!(refused.contains("message is too large"), "{refused}");
    }

    #[tokio::test]
    async fn a_session_started_without_the_flag_is_reported_not_sent_to() {
        // Claude Code's log for a session it did not register the channel on.
        let dir = temp();
        let (socket, received) = fake_session(dir.path(), "s", r#"{"messageId":"m"}"#);
        let reg = registration(dir.path(), "s", "/w/repo", &socket);
        let cache = temp();
        let logs = cache.path().join(project_slug("/w/repo")).join("mcp-logs-onlydiffs");
        std::fs::create_dir_all(&logs).expect("logs dir");
        std::fs::write(
            logs.join("2026-09-05T00-00-00-000Z.jsonl"),
            format!(
                "{{\"error\":\"Server stderr: onlydiffs channel: listening for /w/repo on {}\\n\"}}\n\
                 {{\"debug\":\"Channel notifications skipped: server onlydiffs not in --channels list for this session\"}}\n",
                reg.socket
            ),
        )
        .expect("write log");

        let found = channels_in(dir.path(), Path::new("/w/repo"), &[cache.path().to_path_buf()]).await;
        assert_eq!(found[0].registered, Some(false));

        // Nothing is sent to a session that would drop it, and the error says
        // what to do instead.
        let unregistered = Channel { registration: reg, registered: Some(false) };
        let reachable: Vec<&Channel> = [&unregistered]
            .into_iter()
            .filter(|channel| channel.registered != Some(false))
            .collect();
        assert!(reachable.is_empty());
        assert!(received.lock().expect("log").is_empty());
        assert!(UNREGISTERED_MESSAGE.contains(START_COMMAND));
    }

    #[tokio::test]
    async fn a_registered_channel_is_read_as_such_and_an_unknown_log_as_neither() {
        let dir = temp();
        let (socket, _) = fake_session(dir.path(), "s", r#"{"messageId":"m"}"#);
        let reg = registration(dir.path(), "s", "/w/repo", &socket);
        let cache = temp();
        let logs = cache.path().join(project_slug("/w/repo")).join("mcp-logs-onlydiffs");
        std::fs::create_dir_all(&logs).expect("logs dir");
        // Another session's log in the same directory, which must not be
        // mistaken for ours: it names a different socket.
        std::fs::write(
            logs.join("2026-09-04T00-00-00-000Z.jsonl"),
            "{\"error\":\"Server stderr: onlydiffs channel: listening for /w/repo on /elsewhere/1.sock\\n\"}\n\
             {\"debug\":\"Channel notifications skipped: server onlydiffs not in --channels list for this session\"}\n",
        )
        .expect("write other log");
        std::fs::write(
            logs.join("2026-09-05T00-00-00-000Z.jsonl"),
            format!(
                "{{\"error\":\"Server stderr: onlydiffs channel: listening for /w/repo on {}\\n\"}}\n\
                 {{\"debug\":\"Channel notifications registered\"}}\n",
                reg.socket
            ),
        )
        .expect("write log");

        assert_eq!(read_verdict(&[cache.path().to_path_buf()], &reg).await, Some(true));

        let stranger = Registration { socket: "/nowhere/9.sock".into(), ..reg.clone() };
        assert_eq!(read_verdict(&[cache.path().to_path_buf()], &stranger).await, None);
    }

    #[test]
    fn the_project_slug_is_the_directory_with_dashes() {
        assert_eq!(project_slug("/home/hosenur/od-test-repo"), "-home-hosenur-od-test-repo");
        assert_eq!(project_slug("/Users/me/Developer/onlydiffs"), "-Users-me-Developer-onlydiffs");
        assert_eq!(project_slug("/w/a.b_c"), "-w-a-b-c");
    }

    #[test]
    fn a_message_id_is_only_ever_what_the_channel_would_mint() {
        assert!(is_valid_message_id("onlydiffs-1788557645573-1"));
        assert!(!is_valid_message_id(""));
        assert!(!is_valid_message_id("has space"));
    }

    #[tokio::test]
    async fn an_empty_message_is_refused_before_anything_is_read() {
        let dir = tempfile::TempDir::new().expect("temp dir");

        let refused = send(dir.path(), "   ").await;

        assert_eq!(refused.expect_err("refused").tag(), "ClaudeChannelError");
    }
}
