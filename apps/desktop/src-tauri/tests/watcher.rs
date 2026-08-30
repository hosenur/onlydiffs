//! What the repository watcher chooses to react to.
//!
//! The filter is the half worth testing: too eager and a dependency install
//! floods the app with refreshes, too strict and the stale view this was
//! written to fix comes back.

use std::path::{Path, PathBuf};

use onlydiffs_lib::services::watcher::ChangeFilter;
use tempfile::TempDir;

/// A repository root carrying `.gitignore` rules, with no git involved — the
/// filter reads the file itself rather than asking git about it.
fn sandbox(gitignore: &str) -> TempDir {
    let root = TempDir::new().expect("temp root");
    std::fs::create_dir_all(root.path().join(".git")).expect("create .git");
    std::fs::write(root.path().join(".gitignore"), gitignore).expect("write .gitignore");
    root
}

fn filter(root: &TempDir) -> ChangeFilter {
    ChangeFilter::new(root.path().to_path_buf())
}

/// Event paths arrive from the kernel already resolved — on macOS a temp dir
/// under `/var` is reported as `/private/var` — so the tests have to ask the
/// same question the watcher is really asked.
fn resolved(root: &TempDir) -> PathBuf {
    root.path().canonicalize().expect("canonicalize root")
}

fn interesting(root: &TempDir, relative: &str) -> bool {
    filter(root).is_interesting(&resolved(root).join(relative))
}

#[test]
fn reacts_to_a_tracked_file() {
    let root = sandbox("node_modules/\n");
    assert!(interesting(&root, "src/main.rs"));
    assert!(interesting(&root, "README.md"));
}

#[test]
fn ignores_what_the_repository_ignores() {
    let root = sandbox("node_modules/\ntarget/\ndist/\n");
    assert!(!interesting(&root, "node_modules/react/index.js"));
    assert!(!interesting(&root, "target/debug/build/thing.rlib"));
    assert!(!interesting(&root, "dist/assets/app.js"));
}

#[test]
fn ignores_a_nested_match_of_a_root_rule() {
    // `matched_path_or_any_parents` is what makes the directory rule cover
    // everything beneath it, however deep.
    let root = sandbox("node_modules/\n");
    assert!(!interesting(
        &root,
        "node_modules/a/node_modules/b/c/d/e/f.js"
    ));
}

#[test]
fn reacts_to_staging_and_commits() {
    // The three paths under `.git` that mean the diff moved.
    let root = sandbox("");
    assert!(interesting(&root, ".git/index"));
    assert!(interesting(&root, ".git/HEAD"));
    assert!(interesting(&root, ".git/refs/heads/main"));
}

#[test]
fn ignores_the_rest_of_dot_git() {
    // These churn on every git invocation. Reacting to them would mean the
    // app refreshing itself in a loop after its own commit.
    let root = sandbox("");
    assert!(!interesting(&root, ".git/index.lock"));
    assert!(!interesting(&root, ".git/objects/ab/cdef0123456789"));
    assert!(!interesting(&root, ".git/logs/HEAD"));
    assert!(!interesting(&root, ".git/COMMIT_EDITMSG"));
}

#[test]
fn ignores_the_root_and_anything_outside_it() {
    let root = sandbox("");
    let filter = filter(&root);
    assert!(!filter.is_interesting(&resolved(&root)));
    assert!(!filter.is_interesting(Path::new("/somewhere/else/file.rs")));
}

#[test]
fn survives_a_repository_with_no_ignore_file() {
    let root = TempDir::new().expect("temp root");
    let filter = ChangeFilter::new(root.path().to_path_buf());
    assert!(filter.is_interesting(&resolved(&root).join("src/main.rs")));
}

/// The watcher end to end: a real thread, a real filesystem event, and the
/// callback that would emit to the renderer.
mod fires {
    use std::sync::mpsc;
    use std::time::Duration;

    use onlydiffs_lib::services::watcher::RepoWatcher;
    use tempfile::TempDir;

    /// The debounce is 300ms, so anything under that is too tight to be a
    /// signal. This is generous enough for a loaded CI machine and still
    /// bounded, so a broken watch fails rather than hangs.
    const PATIENCE: Duration = Duration::from_secs(5);

    fn watched(gitignore: &str) -> (TempDir, mpsc::Receiver<()>, RepoWatcher) {
        let root = TempDir::new().expect("temp root");
        std::fs::write(root.path().join(".gitignore"), gitignore).expect("write .gitignore");

        let (tx, rx) = mpsc::channel();
        let watcher = RepoWatcher::new();
        watcher.watch(root.path().to_path_buf(), move || {
            let _ = tx.send(());
        });
        // The watch is established on this thread, but FSEvents needs a moment
        // before it reports anything; a write racing that start-up is missed.
        std::thread::sleep(Duration::from_millis(300));
        (root, rx, watcher)
    }

    #[test]
    fn a_changed_file_reaches_the_callback() {
        let (root, rx, _watcher) = watched("node_modules/\n");
        std::fs::write(root.path().join("main.rs"), "fn main() {}").expect("write file");
        assert!(
            rx.recv_timeout(PATIENCE).is_ok(),
            "a tracked file changed and the watcher stayed silent"
        );
    }

    /// How many signals a thirty-file burst may produce before the debounce is
    /// not doing its job. Not one: a burst that takes longer to write than the
    /// debounce window legitimately closes one and opens another, and a loaded
    /// machine makes that likely. The property worth holding is the ratio.
    const BURST_CEILING: usize = 5;

    #[test]
    fn a_burst_of_writes_collapses_into_a_few_signals() {
        let (root, rx, _watcher) = watched("");
        for index in 0..30 {
            std::fs::write(root.path().join(format!("file{index}.rs")), "x").expect("write file");
        }
        assert!(rx.recv_timeout(PATIENCE).is_ok(), "no signal for the burst");

        // Drain whatever else the burst produced. Each refresh is a `git
        // status` plus a `git diff` per changed file, so the count here is the
        // cost the app actually pays for one agent run.
        let mut signals = 1;
        while rx.recv_timeout(Duration::from_millis(800)).is_ok() {
            signals += 1;
        }
        assert!(
            signals <= BURST_CEILING,
            "thirty writes produced {signals} refreshes"
        );
    }

    #[test]
    fn an_ignored_file_stays_silent() {
        let (root, rx, _watcher) = watched("node_modules/\n");
        let nested = root.path().join("node_modules/react");
        std::fs::create_dir_all(&nested).expect("create ignored dir");
        std::fs::write(nested.join("index.js"), "module.exports = {}").expect("write file");
        assert!(
            rx.recv_timeout(Duration::from_secs(1)).is_err(),
            "an ignored path woke the app"
        );
    }
}
