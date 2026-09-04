//! Finding the Codex sessions actually running in a repository.
//!
//! The Claude bridge asks whether a registered channel's process is still
//! alive before it believes in it. Codex registers nothing, so the equivalent
//! question has to be asked of the process table directly: is there a `codex`
//! running, and is it working in this repository?
//!
//! That is a deliberately narrow definition, and it is the one that matches
//! what a user means by "my Codex session". A thread in Codex's history is not
//! a session — it is the record of one, and a message sent to a repository
//! whose session was closed hours ago goes nowhere anybody is looking.
//!
//! It runs where the repository is, so for a project on a host it is the
//! host's process table that gets read, not this machine's.

use std::path::{Path, PathBuf};

use tokio::process::Command;

/// The helper processes Codex runs beside a session.
///
/// They inherit the working directory of whatever started them, so the daemon
/// launched from inside a repository looks exactly like a session in it. Only
/// the command line tells them apart.
const NOT_A_SESSION: [&str; 3] = ["app-server", "code-mode-host", "mcp-server"];

fn is_session(command: &str) -> bool {
    let looks_like_codex = command
        .split_whitespace()
        .next()
        .map(|argv0| {
            Path::new(argv0)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "codex" || name.starts_with("codex-"))
        })
        .unwrap_or(false);

    looks_like_codex && !NOT_A_SESSION.iter().any(|helper| command.contains(helper))
}

/// Whether `cwd` is the repository or somewhere inside it.
///
/// Inside counts: opening Codex in a subdirectory is opening it on the
/// repository, and refusing that would be a distinction the user never drew.
fn belongs_to(cwd: &Path, root: &Path) -> bool {
    cwd == root || cwd.starts_with(root)
}

/// How many Codex sessions are running in this repository.
pub async fn running_in(root: &Path) -> usize {
    sessions(root).await.len()
}

/// The working directories of every Codex session running in this repository.
///
/// Best-effort throughout. A process that exits between being listed and being
/// asked about is simply not counted, which is the right answer a moment later
/// anyway.
pub async fn sessions(root: &Path) -> Vec<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        linux_sessions(root).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        lsof_sessions(root).await
    }
}

/// Linux reads `/proc`, which costs no processes at all — worth having, since
/// this is polled every few seconds and hosts are usually Linux.
#[cfg(target_os = "linux")]
async fn linux_sessions(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir("/proc").await else {
        return found;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let Some(pid) = name.to_str().filter(|n| n.bytes().all(|b| b.is_ascii_digit())) else {
            continue;
        };
        let Ok(raw) = tokio::fs::read(format!("/proc/{pid}/cmdline")).await else {
            continue;
        };
        // `cmdline` separates arguments with NULs.
        let command = String::from_utf8_lossy(&raw).replace('\0', " ");
        if !is_session(command.trim()) {
            continue;
        }
        let Ok(cwd) = tokio::fs::read_link(format!("/proc/{pid}/cwd")).await else {
            continue;
        };
        if belongs_to(&cwd, root) {
            found.push(cwd);
        }
    }
    found
}

/// macOS has no `/proc`, so it takes the two-spawn route: `ps` to find the
/// candidates, then one `lsof` for their working directories.
#[cfg(not(target_os = "linux"))]
async fn lsof_sessions(root: &Path) -> Vec<PathBuf> {
    let Ok(listing) = Command::new("ps").args(["-eo", "pid=,command="]).output().await else {
        return Vec::new();
    };
    let listing = String::from_utf8_lossy(&listing.stdout).into_owned();

    let pids: Vec<String> = listing
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (pid, command) = line.split_once(' ')?;
            is_session(command.trim()).then(|| pid.to_owned())
        })
        .collect();
    if pids.is_empty() {
        return Vec::new();
    }

    // `-Fn` is the parseable form: one field per line, `n` prefixing a name.
    let Ok(output) = Command::new("lsof")
        .args(["-a", "-d", "cwd", "-Fn", "-p", &pids.join(",")])
        .output()
        .await
    else {
        return Vec::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix('n'))
        .map(PathBuf::from)
        .filter(|cwd| belongs_to(cwd, root))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_codex_session_is_recognised_however_it_was_launched() {
        assert!(is_session("codex"));
        assert!(is_session("codex --remote unix://"));
        assert!(is_session("/home/u/.local/bin/codex --remote unix://"));
    }

    #[test]
    fn the_helpers_codex_runs_beside_a_session_are_not_sessions() {
        // Each of these inherits the repository as its working directory when
        // it is started from inside one, so without this they would each count
        // as a session the user does not have.
        assert!(!is_session(
            "/home/u/.codex/packages/standalone/current/codex app-server --listen unix://"
        ));
        assert!(!is_session("codex app-server daemon pid-update-loop"));
        assert!(!is_session(
            "/home/u/.codex/packages/.../bin/codex-code-mode-host"
        ));
    }

    #[test]
    fn something_that_merely_mentions_codex_is_not_one() {
        assert!(!is_session("vim codex.md"));
        assert!(!is_session("grep codex src"));
        assert!(!is_session(""));
    }

    #[test]
    fn a_session_opened_in_a_subdirectory_still_belongs_to_the_repository() {
        let root = Path::new("/w/repo");
        assert!(belongs_to(Path::new("/w/repo"), root));
        assert!(belongs_to(Path::new("/w/repo/api/src"), root));
        assert!(!belongs_to(Path::new("/w/other"), root));
        // The prefix check must not match a sibling that merely starts the same.
        assert!(!belongs_to(Path::new("/w/repo-two"), root));
    }

    #[tokio::test]
    async fn a_directory_nothing_is_running_in_reports_no_sessions() {
        let dir = tempfile::TempDir::new().expect("temp dir");

        assert_eq!(running_in(dir.path()).await, 0);
    }
}
