//! Claude Code sessions in a repository: finding them, and handing one a message.
//!
//! Finding uses `claude agents --json`, which Claude Code's docs call the
//! supported way to read session state from outside. It lists every live
//! session — terminal ones included — with its pid, working directory and
//! busy/idle status, and answers in about a tenth of a second.
//!
//! Sending uses channels (a research preview). A session spawns the onlydiffs
//! MCP server, which registers a socket in `~/.onlydiffs/claude-channels`;
//! a message written to that socket becomes a `notifications/claude/channel`
//! event in the session. Claude Code only acts on it when the session was
//! started with `--dangerously-load-development-channels server:onlydiffs`,
//! and it never says when it drops one. So reachability is decided here from
//! two facts this app can check, rather than from Claude Code's logs:
//!
//! - the flag is on the session's own command line, and
//! - a live onlydiffs channel was spawned by that session's process.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use onlydiffs_core::services::claude_channel_server::{
    MCP_SERVER_NAME, Registration, SCHEMA_VERSION, registrations_dir,
};
use onlydiffs_core::services::paths;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::Command;

use super::{find_binary, process_table, run_with_timeout};

/// What the user runs for a session this app can reach.
pub const START_COMMAND: &str = "claude --dangerously-load-development-channels server:onlydiffs";

const DEV_CHANNELS_FLAG: &str = "--dangerously-load-development-channels";
const CHANNELS_FLAG: &str = "--channels";
const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const LIST_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// How far up the process tree a channel server may sit below its session.
/// Claude Code normally spawns it directly; a wrapper script adds a level.
const MAX_ANCESTRY: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Busy,
    Waiting,
    Idle,
    Unknown,
}

/// Whether a message sent now would reach the session, and if not, why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    Ready { socket: PathBuf },
    /// Started without the channel flag: Claude Code would drop the message.
    MissingFlag,
    /// Started with the flag, but no onlydiffs channel is running under it —
    /// the MCP server is not configured, failed to start, or was disabled.
    MissingServer,
    /// A session driven by another program (the Claude app, an editor
    /// extension) over stdin. Nobody can add the flag to those.
    Embedded,
}

#[derive(Debug, Clone)]
pub struct ClaudeSession {
    pub pid: u32,
    pub name: Option<String>,
    pub cwd: PathBuf,
    pub activity: Activity,
    pub reach: Reach,
}

#[derive(Debug, Clone)]
pub enum Detection {
    Sessions(Vec<ClaudeSession>),
    NotInstalled,
    Failed(String),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Listed {
    pid: u32,
    cwd: String,
    #[serde(default)]
    started_at: u64,
    name: Option<String>,
    status: Option<String>,
}

fn claude_binary() -> Option<PathBuf> {
    find_binary(
        "claude",
        &[".local/bin/claude", ".claude/local/claude"],
        &["/opt/homebrew/bin/claude", "/usr/local/bin/claude"],
    )
}

/// Every Claude Code session working in `root` or anywhere below it, newest
/// first.
pub async fn detect(root: &Path) -> Detection {
    detect_in(Some(root)).await
}

/// Every Claude Code session on this Mac, wherever it works, newest first:
/// the choices for tying a window to a session in another folder.
pub async fn detect_all() -> Detection {
    detect_in(None).await
}

async fn detect_in(root: Option<&Path>) -> Detection {
    let Some(claude) = claude_binary() else {
        return Detection::NotInstalled;
    };
    let mut command = Command::new(claude);
    command.args(["agents", "--json"]);
    let output = match run_with_timeout(command, LIST_TIMEOUT).await {
        Ok(output) => output,
        Err(error) => return Detection::Failed(format!("`claude agents --json` failed: {error}")),
    };
    let listed: Vec<Listed> = match serde_json::from_slice(&output) {
        Ok(listed) => listed,
        Err(error) => {
            return Detection::Failed(format!(
                "`claude agents --json` printed something unexpected: {error}"
            ));
        }
    };

    let root = root.map(paths::normalize);
    let mut mine: Vec<Listed> = listed
        .into_iter()
        .filter(|session| {
            root.as_ref().is_none_or(|root| paths::is_within(Path::new(&session.cwd), root))
        })
        .collect();
    if mine.is_empty() {
        return Detection::Sessions(Vec::new());
    }
    mine.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    let processes = process_table().await;
    let channels = live_channels(&processes).await;

    let sessions = mine
        .into_iter()
        .map(|session| {
            let command = processes.get(&session.pid).map(|p| p.command.as_str()).unwrap_or("");
            let reach = if is_embedded(command) {
                Reach::Embedded
            } else {
                match (has_channel_flag(command), channels.get(&session.pid)) {
                    (true, Some(socket)) => Reach::Ready {
                        socket: socket.clone(),
                    },
                    (true, None) => Reach::MissingServer,
                    (false, _) => Reach::MissingFlag,
                }
            };
            ClaudeSession {
                pid: session.pid,
                name: session.name,
                cwd: PathBuf::from(session.cwd),
                activity: match session.status.as_deref() {
                    Some("busy") => Activity::Busy,
                    Some("waiting") => Activity::Waiting,
                    Some("idle") => Activity::Idle,
                    _ => Activity::Unknown,
                },
                reach,
            }
        })
        .collect();
    Detection::Sessions(sessions)
}

/// Whether the command line enables the onlydiffs channel, through either the
/// development flag or `--channels`. Both take a space-separated list, and
/// either may be written `--flag=value`.
fn has_channel_flag(command: &str) -> bool {
    let wanted = format!("server:{MCP_SERVER_NAME}");
    let mut in_list = false;
    for token in command.split_whitespace() {
        if let Some((flag, value)) = token.split_once('=') {
            if flag == DEV_CHANNELS_FLAG || flag == CHANNELS_FLAG {
                if value.split(',').any(|entry| entry == wanted) {
                    return true;
                }
                in_list = true;
                continue;
            }
        }
        if token == DEV_CHANNELS_FLAG || token == CHANNELS_FLAG {
            in_list = true;
            continue;
        }
        if token.starts_with('-') {
            in_list = false;
            continue;
        }
        if in_list && token.split(',').any(|entry| entry == wanted) {
            return true;
        }
    }
    false
}

/// A session another program drives over stdin rather than a terminal UI:
/// the Claude app and the editor extensions start Claude Code this way.
fn is_embedded(command: &str) -> bool {
    command.contains("--input-format stream-json") || command.contains("--input-format=stream-json")
}

/// Live onlydiffs channels, keyed by the pid of the Claude Code session that
/// spawned each one.
async fn live_channels(processes: &HashMap<u32, super::Process>) -> HashMap<u32, PathBuf> {
    let mut found = HashMap::new();
    let Ok(mut entries) = tokio::fs::read_dir(registrations_dir()).await else {
        return found;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(body) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        let Ok(registration) = serde_json::from_str::<Registration>(&body) else {
            continue;
        };
        if registration.schema_version != SCHEMA_VERSION {
            continue;
        }
        // A server that was killed leaves its files; only one whose socket
        // still accepts is alive.
        let connected =
            tokio::time::timeout(PROBE_TIMEOUT, UnixStream::connect(&registration.socket)).await;
        if !matches!(connected, Ok(Ok(_))) {
            continue;
        }
        // Walk up from the server to the session that spawned it.
        let mut pid = registration.pid;
        for _ in 0..MAX_ANCESTRY {
            let Some(parent) = processes.get(&pid).map(|p| p.parent) else {
                break;
            };
            found
                .entry(parent)
                .or_insert_with(|| PathBuf::from(&registration.socket));
            pid = parent;
        }
    }
    found
}

/// Hands `message` to the newest reachable session in `sessions`. Resolves
/// with the session it went to.
pub async fn send(sessions: &[ClaudeSession], message: &str) -> Result<ClaudeSession, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("The message is empty.".into());
    }
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(format!("The message is over {MAX_MESSAGE_BYTES} bytes."));
    }

    let mut last_error = None;
    for session in sessions {
        let Reach::Ready { socket } = &session.reach else {
            continue;
        };
        match deliver(socket, message).await {
            Ok(()) => return Ok(session.clone()),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| explain_unreachable(sessions)))
}

/// Why nothing in `sessions` can take a message, in words the user can act on.
pub fn explain_unreachable(sessions: &[ClaudeSession]) -> String {
    if sessions.is_empty() {
        return format!("No Claude Code session is running in this repository. Start one with `{START_COMMAND}`.");
    }
    if sessions.iter().any(|s| s.reach == Reach::MissingServer) {
        return format!(
            "Claude Code has the channel flag but the `{MCP_SERVER_NAME}` MCP server is not running in it. Check `claude mcp list`, then restart the session."
        );
    }
    if sessions.iter().all(|s| s.reach == Reach::Embedded) {
        return format!(
            "The Claude Code session here runs inside the Claude app or an editor, which cannot receive messages. Start one in a terminal with `{START_COMMAND}`."
        );
    }
    format!(
        "Claude Code is running here but was started without the channel flag, so it would drop the message. Restart it with `{START_COMMAND}` (add `--continue` to keep the conversation)."
    )
}

/// One message over one connection: write it, close the write side, read the
/// server's one-line JSON answer.
async fn deliver(socket: &Path, message: &str) -> Result<(), String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Reply {
        message_id: Option<String>,
        error: Option<String>,
    }

    let exchange = async {
        let mut stream = UnixStream::connect(socket)
            .await
            .map_err(|e| format!("could not connect to the channel: {e}"))?;
        stream
            .write_all(message.as_bytes())
            .await
            .map_err(|e| format!("could not write to the channel: {e}"))?;
        stream
            .shutdown()
            .await
            .map_err(|e| format!("could not finish writing: {e}"))?;
        let mut reply = String::new();
        (&mut stream)
            .take(4096)
            .read_to_string(&mut reply)
            .await
            .map_err(|e| format!("the channel gave no answer: {e}"))?;
        Ok::<_, String>(reply)
    };
    let reply = tokio::time::timeout(SEND_TIMEOUT, exchange)
        .await
        .map_err(|_| "the channel did not answer in time".to_owned())??;
    let reply: Reply = serde_json::from_str(reply.trim())
        .map_err(|e| format!("the channel answered with something unexpected: {e}"))?;
    match (reply.message_id, reply.error) {
        (_, Some(error)) => Err(format!("the channel refused the message: {error}")),
        (Some(_), None) => Ok(()),
        (None, None) => Err("the channel did not confirm the message".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_development_flag() {
        assert!(has_channel_flag(
            "claude --dangerously-load-development-channels server:onlydiffs"
        ));
        assert!(has_channel_flag(
            "/x/claude --continue --dangerously-load-development-channels server:a server:onlydiffs --model opus"
        ));
        assert!(has_channel_flag(
            "claude --dangerously-load-development-channels=server:onlydiffs"
        ));
        assert!(has_channel_flag("claude --channels server:onlydiffs"));
    }

    #[test]
    fn ignores_other_servers_and_plain_sessions() {
        assert!(!has_channel_flag("claude"));
        assert!(!has_channel_flag(
            "claude --dangerously-load-development-channels server:other"
        ));
        // The server name after an unrelated flag is that flag's value.
        assert!(!has_channel_flag(
            "claude --dangerously-load-development-channels server:a --add-dir server:onlydiffs"
        ));
    }

    #[test]
    fn recognises_embedded_sessions() {
        assert!(is_embedded(
            "/Users/me/Library/Application Support/Claude/claude-code/2.1/claude --output-format stream-json --verbose --input-format stream-json"
        ));
        assert!(!is_embedded("claude --continue"));
    }
}
