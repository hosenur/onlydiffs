//! Telling the window that the open repository changed.
//!
//! The watching itself is `onlydiffs-core`: it runs where the repository is,
//! which for a remote project means on the host. This is the app-side half —
//! the one place a change becomes an event the renderer hears, whichever side
//! noticed it.

use onlydiffs_core::services::watcher::RepoWatcher;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

/// The event the renderer listens for. Part of the IPC contract.
pub const REPO_CHANGED: &str = "repo:changed";

/// Announces a change to the window. A failed emit means the window has gone
/// away, and there is nothing left to tell.
pub fn announce(app: &AppHandle) {
    let _ = app.emit(REPO_CHANGED, ());
}

/// Points the app's watcher at a local `root` and has it emit [`REPO_CHANGED`].
///
/// The one place a local watch is wired to the window, shared by startup and by
/// `open_project` so both take the same path. A repository on another machine
/// does not come through here: its changes arrive as protocol events, and reach
/// the window through [`announce`] instead.
pub fn watch_repo(app: &AppHandle, watcher: &RepoWatcher, root: PathBuf) {
    let emitter = app.clone();
    watcher.watch(root, move || announce(&emitter));
}
