//! Watching the open repository, so the renderer stops having to be told to
//! look again.
//!
//! The app used to read the diff only when a route loader ran, which made a
//! stale view the default: an agent rewriting files changed nothing on screen
//! until the user navigated, pressed refresh, or restarted. This pushes
//! instead — one debounced event per settled burst of writes.
//!
//! Two things keep that from turning into a storm. Events are filtered against
//! the repository's own ignore rules, so `node_modules` and `target` never
//! reach us; and what survives is debounced, so a thirty-file rewrite is one
//! refresh rather than thirty.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify_debouncer_full::notify::event::{AccessKind, AccessMode};
use notify_debouncer_full::notify::{self, EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer_opt, DebounceEventResult, Debouncer, NoCache};

/// Trailing debounce. Long enough that saving thirty files is one refresh,
/// short enough that saving one still feels immediate.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// Which paths under `.git` mean the diff moved. `index` is staging; `HEAD`
/// and `refs` are commits and branch switches. Everything else there — loose
/// objects, `index.lock`, reflogs, gc — churns on every git invocation without
/// changing a thing the app displays.
fn is_git_signal(entry: Option<&OsStr>) -> bool {
    matches!(
        entry.and_then(|name| name.to_str()),
        Some("index" | "HEAD" | "refs")
    )
}

/// Whether an event says the repository changed, as opposed to being read.
///
/// Linux reports opens. inotify is asked for `IN_OPEN` among the rest, and
/// `git diff` opens every file it produces a patch for — so a filter that
/// looked only at paths reported the app's own read of the repository as a
/// change to it, and the refresh that followed read it again. Measured on a
/// quiet checkout, two read-only git commands produced three `OPEN` events.
///
/// macOS never showed this. FSEvents reports what changed, not what was read,
/// which is why the same code was well behaved locally and looped over SSH.
///
/// A close-after-write is kept. It is an `Access` by name only — some editors
/// save by writing and closing, and dropping it could cost a real change.
fn is_change(kind: &EventKind) -> bool {
    match kind {
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        // Opens, reads, and closes-after-read. Somebody looked; nothing moved.
        EventKind::Access(_) => false,
        _ => true,
    }
}

/// Decides which filesystem events could change what the diff shows.
///
/// Kept separate from the watcher, and public, because this is the half worth
/// testing: the watcher itself is a thread and a channel.
pub struct ChangeFilter {
    root: PathBuf,
    ignore: Gitignore,
}

impl ChangeFilter {
    /// Reads the repository's ignore rules once, at the point it starts being
    /// watched.
    ///
    /// Nested `.gitignore` files below the root are deliberately not read. The
    /// rules that matter for volume — `node_modules/`, `target/`, `dist/` —
    /// are named at the root in practice, and the cost of missing one is a
    /// wasted refresh, not a wrong answer. Rules that change while the
    /// repository is open take effect the next time it is opened.
    pub fn new(root: PathBuf) -> Self {
        // Resolve symlinks, which `Workspace` deliberately does not: it keeps
        // the path the user typed so it can show it back to them. The kernel
        // has no such manners — FSEvents reports `/private/var/...` for a watch
        // on `/var/...` — and a root that does not prefix the paths being
        // matched would silently discard every event as foreign.
        let root = root.canonicalize().unwrap_or(root);
        let mut builder = GitignoreBuilder::new(&root);
        // Both are best-effort. A repository with no ignore rules is perfectly
        // ordinary, and one whose rules fail to parse should still be watched.
        let _ = builder.add(root.join(".gitignore"));
        let _ = builder.add(root.join(".git/info/exclude"));
        let ignore = builder.build().unwrap_or_else(|_| Gitignore::empty());
        Self { root, ignore }
    }

    /// Whether a changed path could show up in the diff.
    pub fn is_interesting(&self, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            // Genuinely outside the repository, now that both sides are
            // resolved. Nothing to do with the diff.
            return false;
        };

        let mut components = relative.components().map(|component| component.as_os_str());
        let Some(first) = components.next() else {
            // The repository root itself, touched rather than its contents.
            return false;
        };

        if first == OsStr::new(".git") {
            return is_git_signal(components.next());
        }

        // A gitignored path is absent from `git status`, so it cannot change
        // the diff. This is the rule that keeps a dependency install from
        // waking the app thousands of times.
        !self
            .ignore
            .matched_path_or_any_parents(path, path.is_dir())
            .is_ignore()
    }
}

/// The repository currently being watched, if any.
struct Active {
    root: PathBuf,
    /// Held only to keep it alive: dropping the debouncer stops its thread and
    /// releases the underlying watch.
    _debouncer: Debouncer<RecommendedWatcher, NoCache>,
}

/// One watch at a time, following whichever repository is open.
pub struct RepoWatcher {
    active: Mutex<Option<Active>>,
}

impl RepoWatcher {
    pub fn new() -> Self {
        Self {
            active: Mutex::new(None),
        }
    }

    /// Points the watcher at `root`, replacing whatever it was watching.
    ///
    /// Re-watching the current root is a no-op, so calling this on every
    /// `open_project` — including one that reopens the repository already on
    /// screen — costs nothing and restarts nothing.
    ///
    /// A watch that cannot be established is not an error worth surfacing: the
    /// app still works, it just stops updating on its own, and the manual
    /// refresh in the palette is exactly the fallback for that.
    pub fn watch<F>(&self, root: PathBuf, notify: F)
    where
        F: Fn() + Send + 'static,
    {
        let mut active = self.active.lock().expect("watcher lock");
        if active.as_ref().is_some_and(|current| current.root == root) {
            return;
        }
        // Drop the previous watch before opening the next, so a project switch
        // never holds two.
        *active = None;

        let filter = ChangeFilter::new(root.clone());
        let handler = move |result: DebounceEventResult| {
            let Ok(events) = result else {
                // The debouncer reports read errors per batch. Nothing here can
                // act on one, and going quiet would be worse than a refresh.
                return;
            };
            let worth_it = events.iter().any(|event| {
                // A dropped-event rescan carries no paths: the kernel queue
                // overflowed and the truth is unknown. Refreshing on a maybe
                // beats missing the change that prompted it.
                event.need_rescan()
                    || (is_change(&event.kind)
                        && event.paths.iter().any(|path| filter.is_interesting(path)))
            });
            if worth_it {
                notify();
            }
        };

        // `NoCache` rather than the default file-id cache, which walks the
        // entire tree and stats every entry before the watch is live. That is
        // a six-figure file count in any repository with `node_modules` or a
        // Rust `target/` beside it, it followed symlinks while doing it, and it
        // ran on the startup path. The cache exists to correlate renames by
        // file id; nothing here reads one. This filter asks only whether a
        // path could show up in the diff, and a rename reported as a delete
        // and a create answers that question the same way.
        let Ok(mut debouncer) = new_debouncer_opt::<_, RecommendedWatcher, NoCache>(
            DEBOUNCE,
            None,
            handler,
            NoCache::new(),
            notify::Config::default(),
        ) else {
            return;
        };
        if debouncer.watch(&root, RecursiveMode::Recursive).is_err() {
            return;
        }
        *active = Some(Active {
            root,
            _debouncer: debouncer,
        });
    }
}

impl Default for RepoWatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod reading_is_not_writing {
    use crate::services::repository::Repository;
    use crate::services::watcher::RepoWatcher;
    use crate::services::{diff, file_tree};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use notify_debouncer_full::notify::event::{AccessKind, AccessMode, ModifyKind};
    use notify_debouncer_full::notify::EventKind;

    /// The Linux loop, as a unit test that runs everywhere.
    ///
    /// `git diff` opens the files it patches, inotify reports those opens, and
    /// a filter that asked only "is this path interesting" said yes — so the
    /// app's own read came back as a change and it read again.
    #[test]
    fn a_file_being_opened_is_not_a_change() {
        // `AccessMode::Any` is precisely what notify's inotify backend emits
        // for `IN_OPEN`; a lookalike variant would not pin the real behaviour.
        assert!(!super::is_change(&EventKind::Access(AccessKind::Open(AccessMode::Any))));
        assert!(!super::is_change(&EventKind::Access(AccessKind::Open(AccessMode::Read))));
        assert!(!super::is_change(&EventKind::Access(AccessKind::Read)));
        assert!(!super::is_change(&EventKind::Access(AccessKind::Close(AccessMode::Read))));
    }

    /// The other half. Suppressing too much would trade a view that never
    /// settles for one that never updates.
    #[test]
    fn writing_creating_renaming_and_deleting_all_still_count() {
        assert!(super::is_change(&EventKind::Modify(ModifyKind::Any)));
        assert!(super::is_change(&EventKind::Create(
            notify_debouncer_full::notify::event::CreateKind::File
        )));
        assert!(super::is_change(&EventKind::Remove(
            notify_debouncer_full::notify::event::RemoveKind::File
        )));
        assert!(super::is_change(&EventKind::Any));
        // An editor that saves by writing and closing, with no modify event.
        assert!(super::is_change(&EventKind::Access(AccessKind::Close(
            AccessMode::Write
        ))));
    }

    fn git(root: &std::path::Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .arg("-C").arg(root).args(args)
            .status().expect("git runs").success();
        assert!(ok, "git {args:?} failed");
    }

    /// The whole bug in one assertion: reading the repository must not look
    /// like a change to it, or the app refreshes forever.
    #[tokio::test]
    async fn a_full_read_wakes_no_watcher() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let root = dir.path().to_path_buf();
        git(&root, &["init", "-q"]);
        std::fs::write(root.join("a.txt"), "one\n").expect("write");
        git(&root, &["add", "-A"]);
        git(&root, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "seed"]);
        std::fs::write(root.join("a.txt"), "two\n").expect("write");
        std::fs::write(root.join("untracked.txt"), "new\n").expect("write");

        let repo = Repository::local(root.clone());
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let watcher = RepoWatcher::new();
        watcher.watch(root.clone(), move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });

        // Let the watch settle and drain anything the fixture caused.
        tokio::time::sleep(Duration::from_millis(1200)).await;
        hits.store(0, Ordering::SeqCst);

        // Exactly what the layout loader does, several times over.
        for _ in 0..4 {
            let _ = diff::get_diff(&repo).await.expect("diff");
            let _ = file_tree::list_files(&repo).await.expect("files");
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        tokio::time::sleep(Duration::from_millis(1500)).await;

        assert_eq!(
            hits.load(Ordering::SeqCst),
            0,
            "reading the repository woke the watcher, which invalidates the router, which reads again"
        );
    }
}
