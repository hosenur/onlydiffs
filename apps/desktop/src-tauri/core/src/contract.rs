//! The domain types that cross the process boundary.
//!
//! The mirror of `src/shared/contract.ts`. Field names are serialised in
//! camelCase so the renderer's types apply unchanged; when a field is added
//! here, add it there too.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    /// Unique per row — a path staged *and* modified again yields two rows.
    pub id: String,
    pub path: String,
    pub old_path: Option<String>,
    pub status: ChangeStatus,
    /// true = index vs HEAD, false = working tree vs index.
    pub staged: bool,
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
    /// Set when this file's patch couldn't be produced.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FullFileContents {
    pub old_contents: Option<String>,
    pub new_contents: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoDiff {
    pub repo_path: String,
    pub branch: String,
    pub head: String,
    pub files: Vec<FileChange>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    pub short_hash: String,
    pub subject: String,
    pub author: String,
    pub author_email: String,
    pub relative_date: String,
    pub date: String,
    /// More than one parent — i.e. a merge.
    pub is_merge: bool,
    /// Branch/tag decorations, e.g. "HEAD -> dev, origin/dev".
    pub refs: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIcon {
    /// Repository-relative path chosen as the icon source.
    pub source_path: String,
    /// A small cached image that the renderer can use without filesystem access.
    pub data_url: String,
}

/// A repository the app can open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Project {
    /// Absolute path to the repository root.
    pub path: String,
    /// Last path segment, for display.
    pub name: String,
    /// Resolved in the background; absent until a suitable image is found.
    pub icon: Option<ProjectIcon>,
}

/// Whether a Claude Code session is listening for the open repository.
#[derive(Debug, Clone, Serialize)]
pub struct ClaudeChannelStatus {
    pub connected: bool,
    /// How many live channels are registered; more than one is possible.
    pub sessions: usize,
}

/// Whether a newer release is waiting to be installed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub available: bool,
    /// The version on offer, e.g. `0.1.2`. `None` when nothing is.
    pub version: Option<String>,
    /// The release notes, when the manifest carries any.
    pub notes: Option<String>,
}

impl UpdateStatus {
    /// Nothing to install — either this is the newest release or we are in no
    /// position to know, which the renderer treats the same way.
    pub fn none() -> Self {
        Self {
            available: false,
            version: None,
            notes: None,
        }
    }
}

/// The renderer's theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppTheme {
    Light,
    Dark,
    System,
}
