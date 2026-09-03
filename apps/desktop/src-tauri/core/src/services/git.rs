//! Runs `git` against the configured repository.
//!
//! Every other service goes through here, so process spawning, decoding, and
//! exit-code handling live in a single place.

use std::path::Path;

use tokio::process::Command;

use crate::error::AppError;

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
