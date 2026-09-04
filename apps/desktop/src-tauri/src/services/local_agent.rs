//! Installing the agent this app ships for the machine it is running on.
//!
//! The agent is not only for hosts. Its `channel` mode is the MCP server a
//! Claude Code session runs so the app can push a line into it, and that has to
//! be reachable by a path that does not change when the app updates. So on every
//! launch the bundled agent for this platform is copied into
//! `~/.onlydiffs/agent` under its versioned name, and `~/.onlydiffs/agent/current`
//! is pointed at it — the one path `claude mcp add` was given, kept true.
//!
//! Same directory, same naming, same rule as the host install in `ssh::agent`:
//! the name carries the version, the platform, and a digest of the bytes, so a
//! rebuild is a new file and an unchanged build is copied once.
//!
//! Registering with Claude Code happens here too, because a downloaded app is
//! all a user has: no checkout, no `bun`, no `channel-setup.sh`. The app finds
//! `claude` the way a Finder-launched bundle has to — its own PATH is fifteen
//! variables from launchd — and runs the same `claude mcp add` the script
//! would, once, only when the registration is missing or points elsewhere.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

use crate::error::AppError;
use crate::services::shell_env;
use crate::services::ssh::agent::{self, AGENT_DIR, CURRENT_LINK, MCP_SERVER_NAME};

/// Where `claude` gets installed when it is not on a login shell's PATH
/// either: the bun installer, pipx and friends, and Claude Code's own
/// self-managed install, then the two system prefixes.
const CLAUDE_HOME_DIRS: &[&str] = &[".bun/bin", ".local/bin", ".claude/local"];
const CLAUDE_SYSTEM_DIRS: &[&str] = &["/usr/local/bin", "/opt/homebrew/bin"];

/// `claude mcp get` starts the server to report its health, so this is not
/// instant; it runs in the background at launch and is allowed to take a while.
const CLAUDE_TIMEOUT: Duration = Duration::from_secs(30);

/// The Rust target triple of this build, spelled the way the agent binaries
/// are named. Empty on a platform no agent is built for.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub const TRIPLE: &str = "aarch64-apple-darwin";
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub const TRIPLE: &str = "x86_64-apple-darwin";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub const TRIPLE: &str = "x86_64-unknown-linux-gnu";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
pub const TRIPLE: &str = "aarch64-unknown-linux-gnu";
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "aarch64"),
)))]
pub const TRIPLE: &str = "";

/// Where the agents live on this machine. `$HOME` rather than the state
/// directory on purpose: `channel-setup.sh` and `claude mcp add` name this
/// path, and a state directory moved by `ONLYDIFFS_STATE_DIR` must not move it
/// out from under them.
pub fn agent_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(AGENT_DIR)
}

/// The stable path the channel is registered by.
pub fn current_link() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(CURRENT_LINK)
}

/// What registering came to, for the launch log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registered {
    /// Claude Code already had the channel pointed at `current`.
    Already,
    /// Registered, or re-registered from an older command.
    Now,
    /// No `claude` on this machine; nothing to register with.
    NoClaude,
}

/// Registers the channel with Claude Code on this machine, if it is installed.
///
/// `claude mcp get` is asked first and trusted when it names `current`, so a
/// launch that changes nothing writes nothing. Otherwise the old entry is
/// removed and the new one added at user scope — the same two commands
/// `channel-setup.sh` runs, for people who have a checkout.
pub async fn register_with_claude() -> Result<Registered, AppError> {
    let Some(claude) = find_claude().await else {
        return Ok(Registered::NoClaude);
    };
    let current = current_link();
    let wanted = current.to_string_lossy().into_owned();

    if let Ok(Ok(existing)) = tokio::time::timeout(
        CLAUDE_TIMEOUT,
        Command::new(&claude).args(["mcp", "get", MCP_SERVER_NAME]).output(),
    )
    .await
    {
        if existing.status.success() && String::from_utf8_lossy(&existing.stdout).contains(&wanted) {
            return Ok(Registered::Already);
        }
    }

    // Removing a registration that is not there is not a failure.
    let _ = tokio::time::timeout(
        CLAUDE_TIMEOUT,
        Command::new(&claude)
            .args(["mcp", "remove", "--scope", "user", MCP_SERVER_NAME])
            .output(),
    )
    .await;

    let added = tokio::time::timeout(
        CLAUDE_TIMEOUT,
        Command::new(&claude)
            .args(["mcp", "add", "--scope", "user", MCP_SERVER_NAME, "--"])
            .arg(&current)
            .arg("channel")
            .output(),
    )
    .await
    .map_err(|_| AppError::Ssh("claude mcp add did not finish in time".into()))?
    .map_err(|error| AppError::Ssh(format!("could not run claude: {error}")))?;
    if !added.status.success() {
        return Err(AppError::Ssh(format!(
            "claude mcp add failed: {}",
            String::from_utf8_lossy(&added.stderr).trim()
        )));
    }
    Ok(Registered::Now)
}

/// Where `claude` is, for a process that may have inherited none of the
/// user's PATH. This process's PATH, then the login shell's, then the places
/// installers put it.
pub async fn find_claude() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    if let Some(path) = shell_env::var("PATH").await {
        dirs.extend(std::env::split_paths(&path));
    }
    if let Some(home) = dirs::home_dir() {
        dirs.extend(CLAUDE_HOME_DIRS.iter().map(|dir| home.join(dir)));
    }
    dirs.extend(CLAUDE_SYSTEM_DIRS.iter().map(PathBuf::from));
    first_executable(&dirs, "claude")
}

fn first_executable(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    dirs.iter()
        .map(|dir| dir.join(name))
        .find(|path| is_executable(path))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Puts this build's agent in place and points `current` at it. Answers with
/// the installed binary's path.
pub async fn install() -> Result<PathBuf, AppError> {
    if TRIPLE.is_empty() {
        return Err(AppError::Ssh(
            "this platform has no agent build, so the Claude channel cannot be installed".into(),
        ));
    }
    let source = agent::local_agent(TRIPLE)?;
    let bytes = tokio::fs::read(&source)
        .await
        .map_err(|error| AppError::Ssh(format!("could not read {}: {error}", source.display())))?;

    let directory = agent_dir();
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| AppError::Ssh(format!("could not create {}: {error}", directory.display())))?;
    restrict(&directory).await;

    let name = agent::agent_filename(TRIPLE, &agent::digest(&bytes));
    let destination = directory.join(&name);
    if !destination.is_file() {
        // Written under a temporary name and renamed, so a launch interrupted
        // mid-copy leaves nothing at the name `current` will trust.
        let staging = directory.join(format!("{name}.partial.{}", std::process::id()));
        tokio::fs::write(&staging, &bytes)
            .await
            .map_err(|error| AppError::Ssh(format!("could not write {}: {error}", staging.display())))?;
        restrict(&staging).await;
        tokio::fs::rename(&staging, &destination)
            .await
            .map_err(|error| AppError::Ssh(format!("could not install {}: {error}", destination.display())))?;
    }

    point_current_at(&directory, &name).await?;
    prune_other_builds(&directory, &name).await;
    Ok(destination)
}

/// Replaces the `current` link atomically: a new link beside it, then a rename
/// over it, so there is never a moment with no `current` at all.
#[cfg(unix)]
async fn point_current_at(directory: &std::path::Path, name: &str) -> Result<(), AppError> {
    let link = directory.join("current");
    let staging = directory.join(format!(
        "current.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or(0)
    ));
    let _ = tokio::fs::remove_file(&staging).await;
    // Relative, so the directory can be moved or mounted elsewhere intact.
    tokio::fs::symlink(name, &staging)
        .await
        .map_err(|error| AppError::Ssh(format!("could not link {}: {error}", staging.display())))?;
    tokio::fs::rename(&staging, &link)
        .await
        .map_err(|error| AppError::Ssh(format!("could not update {}: {error}", link.display())))
}

#[cfg(not(unix))]
async fn point_current_at(_directory: &std::path::Path, _name: &str) -> Result<(), AppError> {
    Ok(())
}

/// Removes every other build for this platform. Old versions have nothing
/// pointing at them once `current` has moved, and they are megabytes each.
async fn prune_other_builds(directory: &std::path::Path, keep: &str) {
    let Ok(mut entries) = tokio::fs::read_dir(directory).await else {
        return;
    };
    let suffix = format!("-{TRIPLE}-");
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == keep || !name.starts_with("onlydiffs-agent-") || !name.contains(&suffix) {
            continue;
        }
        if entry.file_type().await.is_ok_and(|kind| kind.is_file()) {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

/// Owner-only, like the host install: this binary is run by this user and
/// nobody else.
#[cfg(unix)]
async fn restrict(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await;
}

#[cfg(not(unix))]
async fn restrict(_path: &std::path::Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_directory_holding_an_executable_claude_wins() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let empty = dir.path().join("empty");
        let has = dir.path().join("has");
        let not_executable = dir.path().join("plain");
        std::fs::create_dir_all(&empty).expect("mkdir");
        std::fs::create_dir_all(&has).expect("mkdir");
        std::fs::create_dir_all(&not_executable).expect("mkdir");
        std::fs::write(has.join("claude"), "#!/bin/sh\n").expect("write");
        std::fs::write(not_executable.join("claude"), "#!/bin/sh\n").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(has.join("claude"), std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
            std::fs::set_permissions(not_executable.join("claude"), std::fs::Permissions::from_mode(0o644))
                .expect("chmod");
        }

        let found = first_executable(&[empty, not_executable, has.clone()], "claude");

        assert_eq!(found, Some(has.join("claude")));
        assert_eq!(first_executable(&[dir.path().join("nowhere")], "claude"), None);
    }
}
