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
    /// A message could not be queued for a Codex session.
    CodexChannel(String),
    /// A pasted image could not be read or written down.
    Attachment(String),
    Clipboard(String),
    /// No repository is open yet — the renderer should show the landing page.
    NoProjectOpen(String),
    /// The path the user typed is not somewhere we can review.
    InvalidProject(String),
    /// Checking for, downloading, or installing a new release went wrong.
    Updater(String),
    /// The settings file could not be read or written.
    Settings(String),
    /// Reaching a host over SSH went wrong.
    Ssh(String),
    /// The host has no key in `known_hosts` yet. Its own variant because the
    /// renderer has to answer it with a fingerprint prompt rather than an
    /// error toast — the payload is the hostname to ask about.
    SshUnknownHost(String),
    /// An established connection dropped. Separate from `Ssh` so the renderer
    /// can offer to reconnect rather than only reporting.
    SshDisconnected(String),
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
            Self::CodexChannel(_) => "CodexChannelError",
            Self::Attachment(_) => "AttachmentError",
            Self::Clipboard(_) => "ClipboardError",
            Self::NoProjectOpen(_) => "NoProjectOpenError",
            Self::InvalidProject(_) => "InvalidProjectError",
            Self::Updater(_) => "UpdaterError",
            Self::Settings(_) => "SettingsError",
            Self::Ssh(_) => "SshError",
            Self::SshUnknownHost(_) => "SshUnknownHostError",
            Self::SshDisconnected(_) => "SshDisconnectedError",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Git(m)
            | Self::WorkTree(m)
            | Self::InvalidPath(m)
            | Self::CommitMessage(m)
            | Self::ClaudeChannel(m)
            | Self::CodexChannel(m)
            | Self::Attachment(m)
            | Self::Clipboard(m)
            | Self::NoProjectOpen(m)
            | Self::InvalidProject(m)
            | Self::Updater(m)
            | Self::Settings(m)
            | Self::Ssh(m)
            | Self::SshUnknownHost(m)
            | Self::SshDisconnected(m) => m,
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

#[cfg(test)]
mod tests {
    use super::AppError;

    /// A second, independent spelling of every tag, kept exhaustive on purpose:
    /// adding a variant to `AppError` stops this compiling, which is the prompt
    /// to add the same string to the `BackendErrorTag` union in
    /// `src/shared/contract.ts`. Without that, the renderer is handed a tag it
    /// has no name for.
    fn tag_the_renderer_names(error: &AppError) -> &'static str {
        match error {
            AppError::Git(_) => "GitError",
            AppError::WorkTree(_) => "WorkTreeError",
            AppError::InvalidPath(_) => "InvalidPathError",
            AppError::CommitMessage(_) => "CommitMessageError",
            AppError::ClaudeChannel(_) => "ClaudeChannelError",
            AppError::CodexChannel(_) => "CodexChannelError",
            AppError::Attachment(_) => "AttachmentError",
            AppError::Clipboard(_) => "ClipboardError",
            AppError::NoProjectOpen(_) => "NoProjectOpenError",
            AppError::InvalidProject(_) => "InvalidProjectError",
            AppError::Updater(_) => "UpdaterError",
            AppError::Settings(_) => "SettingsError",
            AppError::Ssh(_) => "SshError",
            AppError::SshUnknownHost(_) => "SshUnknownHostError",
            AppError::SshDisconnected(_) => "SshDisconnectedError",
        }
    }

    #[test]
    fn every_failure_carries_the_tag_the_renderer_expects() {
        let message = || "something went wrong".to_owned();
        for error in [
            AppError::Git(message()),
            AppError::WorkTree(message()),
            AppError::InvalidPath(message()),
            AppError::CommitMessage(message()),
            AppError::ClaudeChannel(message()),
            AppError::CodexChannel(message()),
            AppError::Attachment(message()),
            AppError::Clipboard(message()),
            AppError::NoProjectOpen(message()),
            AppError::InvalidProject(message()),
            AppError::Updater(message()),
            AppError::Settings(message()),
            AppError::Ssh(message()),
            AppError::SshUnknownHost(message()),
            AppError::SshDisconnected(message()),
        ] {
            assert_eq!(error.tag(), tag_the_renderer_names(&error));
            assert_eq!(error.message(), message());
        }
    }
}
