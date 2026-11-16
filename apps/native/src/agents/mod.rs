//! The coding agents running in the open repository.
//!
//! Local machine only for now: every check here reads this Mac's process
//! table, sockets and CLIs.

pub mod claude;
pub mod codex;
pub mod summary;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use onlydiffs_core::services::paths;
use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub const ALL: [Agent; 2] = [Agent::Claude, Agent::Codex];

    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "Claude",
            Agent::Codex => "Codex",
        }
    }
}

/// What a message is about: a line in a diff, or a whole file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineReference {
    /// Repository-relative path.
    pub path: String,
    /// One-based, counted in the new version — or in the old one when
    /// `removed`, because a removed line has no number in the new file.
    /// `None` refers to the whole file.
    pub line: Option<u32>,
    pub removed: bool,
    /// The line as shown.
    pub text: String,
}

impl LineReference {
    /// The whole of `path`.
    pub fn file(path: impl Into<String>) -> Self {
        Self { path: path.into(), line: None, removed: false, text: String::new() }
    }

    pub fn is_whole_file(&self) -> bool {
        self.line.is_none()
    }

    /// `src/app.rs:42`, `src/app.rs:42 (removed)`, or `src/app.rs` for the
    /// whole file.
    pub fn label(&self) -> String {
        self.label_in(None)
    }

    /// The label with the path under `base` when there is one:
    /// `/Users/me/repo/src/app.rs:42`. A session working in another folder
    /// would read a relative path against its own.
    pub fn label_in(&self, base: Option<&Path>) -> String {
        let path = match base {
            Some(base) => base.join(&self.path).display().to_string(),
            None => self.path.clone(),
        };
        let Some(line) = self.line else {
            return path;
        };
        let at = format!("{path}:{line}");
        if self.removed { format!("{at} (removed)") } else { at }
    }
}

/// The text an agent receives, in the same shape the Tauri app sends: the
/// reference, then what the user wrote. A removed line is quoted, because its
/// number alone would be read against the current file.
///
/// `base` is the repository's root, given when the session works somewhere
/// else, so the path arrives in full.
pub fn compose_message(reference: &LineReference, said: &str, base: Option<&Path>) -> String {
    let said = said.trim();
    let label = reference.label_in(base);
    let mut message = if said.is_empty() { label } else { format!("{label} {said}") };
    if let (true, Some(line)) = (reference.removed, reference.line) {
        let text = reference.text.trim();
        message.push_str(&format!(
            "\n\nThe line was removed in this change. {} is its number in the version before, and it read:\n    {}",
            line,
            if text.is_empty() { "(an empty line)" } else { text }
        ));
    }
    message
}

/// A session in another folder that a window has been tied to: its messages
/// go there instead of to sessions in the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tie {
    pub agent: Agent,
    pub pid: u32,
    pub cwd: PathBuf,
    /// The session's name (Claude) or the start of its conversation (Codex).
    pub title: Option<String>,
}

impl Tie {
    /// `~/Developer/other`, for showing.
    pub fn folder(&self) -> String {
        match std::env::var_os("HOME").map(PathBuf::from) {
            Some(home) if self.cwd.starts_with(&home) => {
                let rest = self.cwd.strip_prefix(&home).unwrap_or(&self.cwd);
                if rest.as_os_str().is_empty() {
                    "~".to_owned()
                } else {
                    format!("~/{}", rest.display())
                }
            }
            _ => self.cwd.display().to_string(),
        }
    }
}

/// Sessions outside `root` that would take a message now — the choices for
/// tying. Claude first, each newest first.
pub async fn other_sessions(root: &Path) -> Vec<Tie> {
    let root = paths::normalize(root);
    let (claude, codex) = tokio::join!(claude::detect_all(), codex::detect_all());
    let mut ties = Vec::new();
    if let claude::Detection::Sessions(sessions) = claude {
        ties.extend(
            sessions
                .into_iter()
                .filter(|s| matches!(s.reach, claude::Reach::Ready { .. }))
                .map(|s| Tie { agent: Agent::Claude, pid: s.pid, cwd: s.cwd, title: s.name }),
        );
    }
    if let codex::Detection::Sessions(sessions) = codex {
        ties.extend(
            sessions
                .into_iter()
                .filter(|s| matches!(s.reach, codex::Reach::Ready { .. }))
                .map(|s| Tie { agent: Agent::Codex, pid: s.pid, cwd: s.cwd, title: s.preview }),
        );
    }
    ties.retain(|tie| !paths::is_within(&paths::normalize(&tie.cwd), &root));
    ties
}

/// Whether the tied session still takes messages.
pub async fn tie_alive(tie: &Tie) -> bool {
    match tie.agent {
        Agent::Claude => matches!(
            claude::detect_all().await,
            claude::Detection::Sessions(s) if s.iter().any(|s| s.pid == tie.pid && matches!(s.reach, claude::Reach::Ready { .. }))
        ),
        Agent::Codex => matches!(
            codex::detect_all().await,
            codex::Detection::Sessions(s) if s.iter().any(|s| s.pid == tie.pid && matches!(s.reach, codex::Reach::Ready { .. }))
        ),
    }
}

/// Sends `message` to the tied session, checking first that it is still the
/// same one and still reachable.
pub async fn deliver_to(tie: &Tie, message: &str) -> Result<String, String> {
    let gone = || format!("The {} session in {} has ended.", tie.agent.name(), tie.folder());
    match tie.agent {
        Agent::Claude => {
            let claude::Detection::Sessions(sessions) = claude::detect_all().await else {
                return Err(gone());
            };
            let mine: Vec<_> = sessions.into_iter().filter(|s| s.pid == tie.pid).collect();
            if mine.is_empty() {
                return Err(gone());
            }
            claude::send(&mine, message).await?;
        }
        Agent::Codex => {
            let codex::Detection::Sessions(sessions) = codex::detect_all().await else {
                return Err(gone());
            };
            let mine: Vec<_> = sessions.into_iter().filter(|s| s.pid == tie.pid).collect();
            if mine.is_empty() {
                return Err(gone());
            }
            codex::send(&mine, message).await?;
        }
    }
    Ok(format!("{} · {}", tie.agent.name(), tie.folder()))
}

/// Sends `message` to `agent`'s session in `root`, checking again first: the
/// session the footer showed a few seconds ago may have quit since. Resolves
/// with a description of where it went.
pub async fn deliver(agent: Agent, root: &Path, message: &str) -> Result<String, String> {
    match agent {
        Agent::Claude => match claude::detect(root).await {
            claude::Detection::Sessions(sessions) => {
                let session = claude::send(&sessions, message).await?;
                Ok(match session.name {
                    Some(name) => format!("Claude · {name}"),
                    None => "Claude".to_owned(),
                })
            }
            claude::Detection::NotInstalled => Err("Claude Code is not installed.".into()),
            claude::Detection::Failed(error) => Err(error),
        },
        Agent::Codex => match codex::detect(root).await {
            codex::Detection::Sessions(sessions) => {
                codex::send(&sessions, message).await?;
                Ok("Codex".to_owned())
            }
            codex::Detection::NotInstalled => Err("Codex is not installed.".into()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(removed: bool) -> LineReference {
        LineReference {
            path: "src/math.rs".into(),
            line: Some(6),
            removed,
            text: "    a * b".into(),
        }
    }

    #[test]
    fn a_message_leads_with_the_reference() {
        assert_eq!(
            compose_message(&reference(false), "  should this saturate? ", None),
            "src/math.rs:6 should this saturate?"
        );
    }

    #[test]
    fn a_bare_reference_is_still_a_message() {
        assert_eq!(compose_message(&reference(false), "", None), "src/math.rs:6");
    }

    #[test]
    fn a_session_in_another_folder_gets_the_full_path() {
        assert_eq!(
            compose_message(&reference(false), "why?", Some(Path::new("/Users/me/repo"))),
            "/Users/me/repo/src/math.rs:6 why?"
        );
    }

    #[test]
    fn a_whole_file_is_referred_to_by_its_path() {
        assert_eq!(
            compose_message(&LineReference::file("src/math.rs"), "review this", None),
            "src/math.rs review this"
        );
        assert_eq!(
            compose_message(&LineReference::file("src/math.rs"), "", Some(Path::new("/r"))),
            "/r/src/math.rs"
        );
    }

    #[test]
    fn a_removed_line_is_quoted() {
        assert_eq!(
            compose_message(&reference(true), "why?", None),
            "src/math.rs:6 (removed) why?\n\nThe line was removed in this change. 6 is its number in the version before, and it read:\n    a * b"
        );
    }
}

/// One row of the process table.
#[derive(Debug, Clone)]
pub struct Process {
    pub parent: u32,
    /// The full command line, arguments joined by spaces.
    pub command: String,
}

/// Every process on the machine, by pid. One `ps` call per check, shared by
/// everything that needs a command line or a parent.
pub async fn process_table() -> HashMap<u32, Process> {
    let mut table = HashMap::new();
    let mut command = Command::new("/bin/ps");
    command.args(["-axo", "pid=,ppid=,command="]);
    let Ok(output) = run_with_timeout(command, Duration::from_secs(5)).await else {
        return table;
    };
    for line in String::from_utf8_lossy(&output).lines() {
        let mut fields = line.split_whitespace();
        let (Some(pid), Some(parent)) = (fields.next(), fields.next()) else {
            continue;
        };
        let (Ok(pid), Ok(parent)) = (pid.parse(), parent.parse()) else {
            continue;
        };
        let command = fields.collect::<Vec<_>>().join(" ");
        table.insert(pid, Process { parent, command });
    }
    table
}

/// Runs `command` and returns its stdout, failing on a non-zero exit or when
/// it takes longer than `timeout`.
pub async fn run_with_timeout(mut command: Command, timeout: Duration) -> Result<Vec<u8>, String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| format!("timed out after {}s", timeout.as_secs()))?
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("exited with {}: {}", output.status, stderr.trim()));
    }
    Ok(output.stdout)
}

/// Finds an executable: on `PATH` first, then in the places its installers
/// put it. An app started from Finder gets a minimal `PATH` that has none of
/// the user's shell setup, so the fallbacks are what make that case work.
pub fn find_binary(name: &str, in_home: &[&str], absolute: &[&str]) -> Option<PathBuf> {
    let on_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|dir| dir.join(name));
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let under_home = in_home
        .iter()
        .filter_map(|relative| home.as_ref().map(|home| home.join(relative)));
    let fixed = absolute.iter().map(PathBuf::from);
    on_path.chain(under_home).chain(fixed).find(|candidate| is_executable(candidate))
}

fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}
