//! Runs `git` against the configured repository.
//!
//! Every other service goes through here, so process spawning, decoding, and
//! exit-code handling live in a single place.

use std::path::Path;

use tokio::process::Command;

use crate::error::AppError;
use crate::services::workspace::Workspace;

/// Runs one `git` invocation in `cwd` and returns its stdout.
///
/// `output()` drains stdout and stderr concurrently with the exit-code wait,
/// which is not optional: a patch larger than the OS pipe buffer would
/// otherwise block the child forever while we waited for it to exit.
pub async fn run_in(cwd: &Path, args: &[&str]) -> Result<String, AppError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .await
        .map_err(|error| AppError::Git(format!("failed to run git: {error}")))?;

    // Decoding is lossy by design: a file with a stray byte should render
    // rather than blanking the card.
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

    // `git diff --no-index` exits non-zero precisely when it *did* produce a
    // patch, so a non-empty stdout is always success.
    if stdout.is_empty() && !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        return Err(AppError::Git(if detail.is_empty() {
            format!("git {} failed", args.join(" "))
        } else {
            detail.to_owned()
        }));
    }

    Ok(stdout)
}

/// The repository is resolved per call rather than captured at startup, so
/// opening a different project from the landing page takes effect immediately.
pub async fn run(workspace: &Workspace, args: &[&str]) -> Result<String, AppError> {
    let cwd = workspace.current_path()?;
    run_in(&cwd, args).await
}

/// Reads one blob. An empty revision means the index (`git show :path`).
pub async fn show_file(
    workspace: &Workspace,
    revision: &str,
    file_path: &str,
) -> Result<String, AppError> {
    let spec = if revision.is_empty() {
        format!(":{file_path}")
    } else {
        format!("{revision}:{file_path}")
    };
    run(workspace, &["show", &spec]).await
}
