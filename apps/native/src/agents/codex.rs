//! Codex sessions in a repository: finding them, and handing one a message.
//!
//! Since Codex 0.169 a plain `codex` in a terminal attaches to Codex's shared
//! background server (the "daemon") by default, so any ordinary session can be
//! reached — no special launch command. Two facts are checked and both must
//! hold before a session counts as reachable:
//!
//! 1. A `codex` process is running in the repository (the process table). This
//!    is what says a person is in front of it: the daemon keeps a thread
//!    loaded for a while after its terminal UI exits.
//! 2. The daemon holds a loaded, user-facing thread in that same directory
//!    (`thread/loaded/list` + `thread/read`). This is what a message is queued
//!    on. Sub-agent, ephemeral and `codex exec` threads share the directory
//!    and are skipped — queueing onto one of those would go nowhere visible.
//!
//! Sending is `thread/queue/add`, the call `codex queue` makes: an idle session
//! starts a turn at once, a busy one takes it when its turn ends, exactly like
//! typing into the TUI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use onlydiffs_core::services::codex_app_server::AppServer;
use onlydiffs_core::services::paths;
use serde_json::{Value, json};
use tokio::process::Command;

use super::{Process, find_binary, process_table};

const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// Subcommands that are not an interactive session. `resume` and `fork` are
/// absent on purpose: both open the terminal UI.
const NOT_A_SESSION: &[&str] = &[
    "exec", "e", "queue", "app-server", "mcp", "mcp-server", "login", "logout", "completion",
    "sandbox", "debug", "apply", "a", "cloud", "features", "remote-control", "proto",
    "responses-api-proxy", "stdio-to-uds", "code-mode-host", "help",
];

/// Flags that make a session run its own embedded server instead of the
/// shared one, which leaves it unreachable. From Codex 0.169's
/// `daemon_startup.rs`; used only to explain an unreachable session.
const STANDALONE_FLAGS: &[&str] = &[
    "--no-daemon",
    "--oss",
    "--profile",
    "-p",
    "--strict-config",
    "--dangerously-bypass-hook-trust",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Working,
    WaitingOnYou,
    Idle,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    Ready { thread_id: String },
    /// Running, but on its own embedded server; `reason` is the flag that did
    /// it when one is on the command line.
    Standalone { reason: Option<String> },
    /// Running, and Codex's background server is not.
    NoDaemon,
}

#[derive(Debug, Clone)]
pub struct CodexSession {
    pub pid: u32,
    pub cwd: PathBuf,
    pub activity: Activity,
    pub preview: Option<String>,
    pub reach: Reach,
}

#[derive(Debug, Clone)]
pub enum Detection {
    Sessions(Vec<CodexSession>),
    NotInstalled,
}

/// A user-facing thread the daemon holds in memory.
#[derive(Debug, Clone)]
struct LoadedThread {
    id: String,
    cwd: PathBuf,
    recency: i64,
    activity: Activity,
    preview: Option<String>,
}

pub fn codex_binary() -> Option<PathBuf> {
    find_binary("codex", &[".local/bin/codex"], &["/opt/homebrew/bin/codex", "/usr/local/bin/codex"])
}

/// Every Codex session running in `root` or below it.
pub async fn detect(root: &Path) -> Detection {
    detect_in(Some(root)).await
}

/// Every Codex session on this Mac, wherever it works.
pub async fn detect_all() -> Detection {
    detect_in(None).await
}

async fn detect_in(root: Option<&Path>) -> Detection {
    if codex_binary().is_none() {
        return Detection::NotInstalled;
    }
    let root = root.map(paths::normalize);
    let root = root.as_deref();
    let processes = process_table().await;
    let running = sessions_in(root, &processes).await;
    if running.is_empty() {
        return Detection::Sessions(Vec::new());
    }

    let threads = match AppServer::connect().await {
        Ok(mut server) => Some(loaded_threads(&mut server, root).await),
        Err(_) => None,
    };

    let sessions = running
        .into_iter()
        .map(|(pid, cwd, command)| {
            let Some(threads) = &threads else {
                return CodexSession {
                    pid,
                    cwd,
                    activity: Activity::Unknown,
                    preview: None,
                    reach: Reach::NoDaemon,
                };
            };
            // The newest loaded thread in the directory this process works
            // in. A `/new` in the TUI can leave the previous thread loaded
            // beside it; the newer one is the one on screen.
            let matched = threads
                .iter()
                .filter(|thread| thread.cwd == cwd)
                .max_by_key(|thread| thread.recency);
            match matched {
                Some(thread) => CodexSession {
                    pid,
                    cwd,
                    activity: thread.activity,
                    preview: thread.preview.clone(),
                    reach: Reach::Ready {
                        thread_id: thread.id.clone(),
                    },
                },
                None => CodexSession {
                    pid,
                    cwd,
                    activity: Activity::Unknown,
                    preview: None,
                    reach: Reach::Standalone {
                        reason: standalone_flag(&command),
                    },
                },
            }
        })
        .collect();
    Detection::Sessions(sessions)
}

/// Interactive `codex` processes working in `root` (anywhere, for `None`), as (pid, effective
/// directory, command line). The effective directory honours `-C`/`--cd`,
/// which moves the session without moving the process.
async fn sessions_in(root: Option<&Path>, processes: &HashMap<u32, Process>) -> Vec<(u32, PathBuf, String)> {
    let candidates: Vec<(u32, &Process)> = processes
        .iter()
        .filter(|(_, process)| is_session(&process.command))
        .map(|(pid, process)| (*pid, process))
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }
    let cwds = process_cwds(candidates.iter().map(|(pid, _)| *pid)).await;

    let mut found = Vec::new();
    for (pid, process) in candidates {
        let Some(cwd) = cwds.get(&pid) else {
            continue;
        };
        let effective = match cd_flag(&process.command) {
            Some(dir) if Path::new(&dir).is_absolute() => PathBuf::from(dir),
            Some(dir) => cwd.join(dir),
            None => cwd.clone(),
        };
        let effective = paths::normalize(&effective);
        if root.is_none_or(|root| paths::is_within(&effective, root)) {
            found.push((pid, effective, process.command.clone()));
        }
    }
    found.sort_by_key(|(pid, _, _)| std::cmp::Reverse(*pid));
    found
}

fn is_session(command: &str) -> bool {
    let mut tokens = command.split_whitespace();
    let Some(argv0) = tokens.next() else {
        return false;
    };
    let name = Path::new(argv0)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if name != "codex" {
        return false;
    }
    // The first word that is not a flag is the subcommand, if there is one. A
    // flag's value can look like a word (`-m gpt-5`), so only a known
    // subcommand name disqualifies.
    !tokens
        .filter(|token| !token.starts_with('-'))
        .take(1)
        .any(|word| NOT_A_SESSION.contains(&word))
}

fn cd_flag(command: &str) -> Option<String> {
    let mut tokens = command.split_whitespace();
    while let Some(token) = tokens.next() {
        if token == "-C" || token == "--cd" {
            return tokens.next().map(str::to_owned);
        }
        if let Some(value) = token.strip_prefix("--cd=") {
            return Some(value.to_owned());
        }
    }
    None
}

fn standalone_flag(command: &str) -> Option<String> {
    command
        .split_whitespace()
        .find(|token| {
            let flag = token.split('=').next().unwrap_or(token);
            STANDALONE_FLAGS.contains(&flag)
        })
        .map(str::to_owned)
}

/// Working directories of `pids`, from one `lsof` call. macOS has no `/proc`.
async fn process_cwds(pids: impl Iterator<Item = u32>) -> HashMap<u32, PathBuf> {
    let list: Vec<String> = pids.map(|pid| pid.to_string()).collect();
    let mut command = Command::new("/usr/sbin/lsof");
    command.args(["-a", "-d", "cwd", "-Fn", "-p", &list.join(",")]);
    // lsof exits non-zero when any one pid has gone; its output is still good.
    let output = match command.output().await {
        Ok(output) => output.stdout,
        Err(_) => return HashMap::new(),
    };
    let mut cwds = HashMap::new();
    let mut current = None;
    for line in String::from_utf8_lossy(&output).lines() {
        if let Some(pid) = line.strip_prefix('p') {
            current = pid.parse::<u32>().ok();
        } else if let (Some(path), Some(pid)) = (line.strip_prefix('n'), current) {
            cwds.insert(pid, PathBuf::from(path));
        }
    }
    cwds
}

async fn loaded_threads(server: &mut AppServer, root: Option<&Path>) -> Vec<LoadedThread> {
    let Ok(listed) = server.request("thread/loaded/list", json!({})).await else {
        return Vec::new();
    };
    let ids: Vec<String> = listed["data"]
        .as_array()
        .map(|ids| ids.iter().filter_map(|id| id.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();

    let mut threads = Vec::new();
    for id in ids {
        let Ok(answer) = server.request("thread/read", json!({ "threadId": id })).await else {
            continue;
        };
        if let Some(thread) = user_thread(&answer["thread"], root) {
            threads.push(thread);
        }
    }
    threads
}

/// A thread a person types into, in `root` (anywhere, for `None`); `None`
/// for anything else.
fn user_thread(thread: &Value, root: Option<&Path>) -> Option<LoadedThread> {
    let id = thread["id"].as_str()?.to_owned();
    let cwd = paths::normalize(Path::new(thread["cwd"].as_str()?));
    if root.is_some_and(|root| !paths::is_within(&cwd, root)) {
        return None;
    }
    if !thread["parentThreadId"].is_null()
        || !thread["attachedToThreadId"].is_null()
        || thread["ephemeral"].as_bool() == Some(true)
    {
        return None;
    }
    // `source` is a string for the plain kinds and an object for
    // `custom`/`subAgent`. Only the two interactive front ends count.
    if !matches!(thread["source"].as_str(), Some("cli" | "vscode")) {
        return None;
    }
    let activity = match thread["status"]["type"].as_str() {
        Some("notLoaded" | "systemError") => return None,
        Some("idle") => Activity::Idle,
        Some("active") => {
            let flags = thread["status"]["activeFlags"].as_array();
            let waiting = flags.is_some_and(|flags| {
                flags
                    .iter()
                    .any(|flag| matches!(flag.as_str(), Some("waitingOnApproval" | "waitingOnUserInput")))
            });
            if waiting { Activity::WaitingOnYou } else { Activity::Working }
        }
        _ => Activity::Unknown,
    };
    let recency = thread["recencyAt"]
        .as_i64()
        .or_else(|| thread["updatedAt"].as_i64())
        .unwrap_or(0);
    let preview = thread["preview"]
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| text.chars().take(80).collect());
    Some(LoadedThread {
        id,
        cwd,
        recency,
        activity,
        preview,
    })
}

/// Hands `message` to the first reachable session in `sessions`. Resolves
/// with the session it went to.
pub async fn send(sessions: &[CodexSession], message: &str) -> Result<CodexSession, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("The message is empty.".into());
    }
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(format!("The message is over {MAX_MESSAGE_BYTES} bytes."));
    }
    let Some((session, thread_id)) = sessions.iter().find_map(|session| match &session.reach {
        Reach::Ready { thread_id } => Some((session, thread_id)),
        _ => None,
    }) else {
        return Err(explain_unreachable(sessions));
    };
    let mut server = AppServer::connect()
        .await
        .map_err(|error| format!("Could not reach Codex's background server: {}", error.message()))?;
    tokio::time::timeout(Duration::from_secs(10), server.queue(thread_id, message))
        .await
        .map_err(|_| "Codex did not answer in time.".to_owned())?
        .map_err(|error| error.message().to_owned())?;
    Ok(session.clone())
}

pub fn explain_unreachable(sessions: &[CodexSession]) -> String {
    if sessions.is_empty() {
        return "No Codex session is running in this repository. Start one there with `codex`.".into();
    }
    if sessions.iter().any(|s| s.reach == Reach::NoDaemon) {
        return "Codex is running here, but Codex's background server is not, so nothing can reach the session. Run `codex app-server daemon start`, then restart the session with `codex resume --last`.".into();
    }
    let reason = sessions.iter().find_map(|s| match &s.reach {
        Reach::Standalone { reason: Some(flag) } => Some(flag.clone()),
        _ => None,
    });
    match reason {
        Some(flag) => format!(
            "Codex is running here with `{flag}`, which makes it run on its own instead of Codex's shared background server, so it can't be reached. Restart it without that flag (`codex resume --last`)."
        ),
        None => "Codex is running here but isn't attached to Codex's shared background server, so it can't be reached. Restart it with `codex resume --last`.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_commands_are_sessions() {
        assert!(is_session("codex"));
        assert!(is_session("/Users/me/.local/bin/codex --model gpt-5"));
        assert!(is_session("codex resume --last"));
        assert!(is_session("codex -C /tmp/repo"));
    }

    #[test]
    fn helpers_and_one_shot_commands_are_not() {
        assert!(!is_session(
            "/x/codex app-server --listen unix:// --analytics-default-enabled --managed-daemon"
        ));
        assert!(!is_session("/x/codex app-server daemon pid-update-loop"));
        assert!(!is_session("codex exec 'fix it'"));
        assert!(!is_session("codex queue --thread abc --message hi"));
        assert!(!is_session("codex-helper"));
        assert!(!is_session("/bin/zsh -c codex"));
    }

    #[test]
    fn reads_the_directory_flag() {
        assert_eq!(cd_flag("codex -C /tmp/repo"), Some("/tmp/repo".into()));
        assert_eq!(cd_flag("codex --cd=sub"), Some("sub".into()));
        assert_eq!(cd_flag("codex --model x"), None);
    }

    #[test]
    fn names_the_flag_that_detaches_a_session() {
        assert_eq!(standalone_flag("codex --no-daemon"), Some("--no-daemon".into()));
        assert_eq!(standalone_flag("codex --profile=work"), Some("--profile=work".into()));
        assert_eq!(standalone_flag("codex --model x"), None);
    }

    fn thread(extra: Value) -> Value {
        let mut base = json!({
            "id": "t1",
            "cwd": "/repo/sub",
            "parentThreadId": null,
            "attachedToThreadId": null,
            "ephemeral": false,
            "source": "cli",
            "status": { "type": "idle" },
            "recencyAt": 10,
            "preview": "fix the parser"
        });
        for (key, value) in extra.as_object().unwrap() {
            base[key] = value.clone();
        }
        base
    }

    #[test]
    fn a_plain_cli_thread_in_the_repository_is_a_user_thread() {
        let found = user_thread(&thread(json!({})), Some(Path::new("/repo"))).expect("kept");
        assert_eq!(found.id, "t1");
        assert_eq!(found.activity, Activity::Idle);
    }

    #[test]
    fn sub_agent_ephemeral_and_exec_threads_are_skipped() {
        let root = Some(Path::new("/repo"));
        assert!(user_thread(&thread(json!({ "parentThreadId": "p" })), root).is_none());
        assert!(user_thread(&thread(json!({ "ephemeral": true })), root).is_none());
        assert!(user_thread(&thread(json!({ "source": "exec" })), root).is_none());
        assert!(
            user_thread(&thread(json!({ "source": { "subAgent": { "other": "guardian" } } })), root)
                .is_none()
        );
        assert!(user_thread(&thread(json!({ "cwd": "/elsewhere" })), root).is_none());
        // Looking everywhere, for tying a window to a session in another folder.
        assert!(user_thread(&thread(json!({ "cwd": "/elsewhere" })), None).is_some());
    }

    #[test]
    fn a_thread_waiting_on_approval_is_waiting_on_you() {
        let waiting = thread(json!({ "status": { "type": "active", "activeFlags": ["waitingOnApproval"] } }));
        assert_eq!(
            user_thread(&waiting, Some(Path::new("/repo"))).unwrap().activity,
            Activity::WaitingOnYou
        );
    }
}
