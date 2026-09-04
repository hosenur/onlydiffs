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
///
/// `--no-optional-locks` is what stops the app watching itself. `git status`
/// refreshes the stat information it caches in `.git/index` and writes the
/// file back — on every run, whether or not anything changed. The watcher
/// treats a write to `.git/index` as staging, which is the one thing under
/// `.git` that genuinely moves the diff, so each read of the repository
/// announced itself as a change to it: read, refresh, read again, forever.
/// The flag suppresses exactly the locks that optional write needs and
/// nothing else, so staging and committing still take the locks they require.
pub async fn run_in(cwd: &Path, args: &[&str]) -> Result<String, AppError> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn repository() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let run = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .status()
                .expect("git runs")
                .success();
            assert!(ok, "git {args:?} failed");
        };
        run(&["init", "-q"]);
        std::fs::write(dir.path().join("seed.txt"), "one\n").expect("write");
        run(&["add", "-A"]);
        run(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "seed"]);
        // A dirty tree is the state that makes `git status` refresh hardest.
        std::fs::write(dir.path().join("seed.txt"), "two\n").expect("write");
        dir
    }

    fn index_mtime(root: &Path) -> std::time::SystemTime {
        std::fs::metadata(root.join(".git/index"))
            .expect("index exists")
            .modified()
            .expect("mtime")
    }

    /// The regression behind an app that refreshed forever.
    ///
    /// `git status` rewrites `.git/index` to refresh its stat cache, the
    /// watcher reads a write there as staging, and so every read of the
    /// repository looked like a change to it — which triggered another read.
    /// Reading must leave no trace.
    #[tokio::test]
    async fn reading_the_repository_does_not_look_like_a_change_to_it() {
        let dir = repository();
        let root = dir.path();

        // Once to settle whatever the fixture left behind, then measure.
        run_in(root, &["status", "--porcelain"]).await.expect("status");
        let before = index_mtime(root);

        for _ in 0..3 {
            run_in(root, &["status", "--porcelain", "-z", "-uall"])
                .await
                .expect("status");
            run_in(root, &["diff", "--", "seed.txt"]).await.expect("diff");
        }

        assert_eq!(
            index_mtime(root),
            before,
            "reading the repository rewrote .git/index, which the watcher reports as a change"
        );
    }

    /// The flag suppresses only the *optional* write. Staging still has to
    /// reach the index, or the diff would never move.
    #[tokio::test]
    async fn staging_still_writes_the_index() {
        let dir = repository();
        let root = dir.path();
        run_in(root, &["status", "--porcelain"]).await.expect("status");
        let before = index_mtime(root);

        run_in(root, &["add", "--", "seed.txt"]).await.expect("add");

        assert_ne!(index_mtime(root), before, "staging must still write the index");
    }
}
