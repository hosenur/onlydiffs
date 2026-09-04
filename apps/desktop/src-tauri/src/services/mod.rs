//! The backend's services.
//!
//! Split across two crates. Everything that runs *where the repository is* —
//! the diff walk, the file list, the history, the watcher, the icon scan — is
//! `onlydiffs-core`, because the remote agent runs exactly that code. It is
//! re-exported here so the rest of the app keeps one set of paths regardless of
//! which side of the split a service lives on.
//!
//! What stays here is what only makes sense on the user's own machine: the
//! Groq calls and the key they need, the recents list, the settings file, the
//! updater, and SSH itself.

use std::path::PathBuf;

pub use onlydiffs_core::services::{
    attachment, claude_channel, codex_channel, diff, file_tree, history, icon_scan, repository,
    watcher,
};

pub mod commit_message;
pub mod project_icon;
pub mod repo_watch;
pub mod settings;
pub mod shell_env;
pub mod ssh;
pub mod updater;
pub mod workspace;

/// Where the app keeps its own files, relative to the user's home directory:
/// `projects.json` beside `config.json`.
const STATE_DIR: &str = ".onlydiffs";

/// `ONLYDIFFS_STATE_DIR` redirects all of it at once, which is what lets a
/// test drive the real stores without touching the developer's own.
pub fn state_dir() -> PathBuf {
    std::env::var("ONLYDIFFS_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(STATE_DIR)
        })
}
