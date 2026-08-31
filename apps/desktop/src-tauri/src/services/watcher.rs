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
use notify_debouncer_full::notify::{self, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer_opt, DebounceEventResult, Debouncer, NoCache};
use tauri::{AppHandle, Emitter};

/// The event the renderer listens for. Part of the IPC contract.
pub const REPO_CHANGED: &str = "repo:changed";

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
                event.need_rescan() || event.paths.iter().any(|path| filter.is_interesting(path))
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

/// Points the app's watcher at `root` and has it emit [`REPO_CHANGED`].
///
/// The one place the watcher is wired to the window, shared by startup and by
/// `open_project` so both take the same path.
pub fn watch_repo(app: &AppHandle, watcher: &RepoWatcher, root: PathBuf) {
    let emitter = app.clone();
    watcher.watch(root, move || {
        // A failed emit means the window has gone away, and there is nothing
        // left to tell.
        let _ = emitter.emit(REPO_CHANGED, ());
    });
}
