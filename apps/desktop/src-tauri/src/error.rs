//! Every failure the backend can produce, and how one crosses to the renderer.
//!
//! Failures travel as *values*, not as rejected promises. Tauri's own error
//! path stringifies whatever it is handed, which would flatten the variant
//! into prose and lose the tag the renderer branches on; carrying the failure
//! inside a successful response keeps both halves intact.

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

#[derive(Debug, Clone)]
pub enum AppError {
    Git(String),
    WorkTree(String),
    InvalidPath(String),
    CommitMessage(String),
    ClaudeChannel(String),
    Clipboard(String),
    /// No repository is open yet — the renderer should show the landing page.
    NoProjectOpen(String),
    /// The path the user typed is not somewhere we can review.
    InvalidProject(String),
}

impl AppError {
    /// The discriminant the renderer matches on. These strings are part of the
    /// IPC contract: they are the `_tag` values `src/lib/ipc.ts` compares.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Git(_) => "GitError",
            Self::WorkTree(_) => "WorkTreeError",
            Self::InvalidPath(_) => "InvalidPathError",
            Self::CommitMessage(_) => "CommitMessageError",
            Self::ClaudeChannel(_) => "ClaudeChannelError",
            Self::Clipboard(_) => "ClipboardError",
            Self::NoProjectOpen(_) => "NoProjectOpenError",
            Self::InvalidProject(_) => "InvalidProjectError",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Git(m)
            | Self::WorkTree(m)
            | Self::InvalidPath(m)
            | Self::CommitMessage(m)
            | Self::ClaudeChannel(m)
            | Self::Clipboard(m)
            | Self::NoProjectOpen(m)
            | Self::InvalidProject(m) => m,
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for AppError {}

/// A failed command, flattened for the renderer.
#[derive(Debug, Clone, Serialize)]
pub struct IpcFailure {
    #[serde(rename = "_tag")]
    pub tag: &'static str,
    pub message: String,
}

/// The discriminated union every command resolves with:
/// `{ ok: true, value }` or `{ ok: false, error }`.
#[derive(Debug)]
pub enum IpcResult<T> {
    Ok(T),
    Err(AppError),
}

impl<T> From<Result<T, AppError>> for IpcResult<T> {
    fn from(result: Result<T, AppError>) -> Self {
        match result {
            Ok(value) => Self::Ok(value),
            Err(error) => Self::Err(error),
        }
    }
}

impl<T: Serialize> Serialize for IpcResult<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        match self {
            Self::Ok(value) => {
                map.serialize_entry("ok", &true)?;
                map.serialize_entry("value", value)?;
            }
            Self::Err(error) => {
                map.serialize_entry("ok", &false)?;
                map.serialize_entry(
                    "error",
                    &IpcFailure {
                        tag: error.tag(),
                        message: error.message().to_owned(),
                    },
                )?;
            }
        }
        map.end()
    }
}
