//! The bridge to a Claude Code session running in the open repository.
//!
//! Claude Code registers a loopback HTTP server per session by dropping a small
//! JSON file in `~/.onlydiffs/claude-channels`. This reads those, picks the
//! ones belonging to the repository on screen, and hands a message over.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::contract::ClaudeChannelStatus;
use crate::error::AppError;
use crate::services::workspace::{normalize, Workspace};

const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
const SEND_TIMEOUT_LABEL: &str = "10 seconds";
const REGISTRATIONS_DIR: &str = ".onlydiffs/claude-channels";
const SCHEMA_VERSION: u32 = 1;

const NO_CHANNEL_MESSAGE: &str =
    "No OnlyDiffs Claude channel is running. Restart Claude Code with the OnlyDiffs channel enabled.";
const NO_CHANNEL_FOR_REPO_MESSAGE: &str =
    "No OnlyDiffs Claude channel is running for this repository. Restart Claude Code in this repository with the OnlyDiffs channel enabled.";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Registration {
    schema_version: u32,
    pid: i64,
    cwd: String,
    port: u16,
    token: String,
    started_at: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageAccepted {
    message_id: String,
}

/// The channel's own id format: what `/^[A-Za-z0-9_-]+$/` accepted.
fn is_valid_message_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn registrations_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(REGISTRATIONS_DIR)
}

/// Live channels for this repository, newest first.
async fn registrations(workspace: &Workspace) -> Result<Vec<Registration>, AppError> {
    let repo_path = workspace
        .current_path()
        .map_err(|error| AppError::ClaudeChannel(error.message().to_owned()))?;
    let directory = registrations_dir();

    let mut entries = tokio::fs::read_dir(&directory).await.map_err(|error| {
        AppError::ClaudeChannel(if error.kind() == std::io::ErrorKind::NotFound {
            NO_CHANNEL_MESSAGE.to_owned()
        } else {
            format!("failed to read Claude channel registrations: {error}")
        })
    })?;

    let mut live: Vec<Registration> = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        // A half-written or stale file is not an error worth showing; the next
        // candidate may well be live.
        let Ok(body) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        let Ok(registration) = serde_json::from_str::<Registration>(&body) else {
            continue;
        };
        let belongs = normalize(Path::new(&registration.cwd)) == repo_path;
        if registration.schema_version == SCHEMA_VERSION
            && registration.pid > 0
            && registration.port > 0
            && !registration.token.is_empty()
            && belongs
        {
            live.push(registration);
        }
    }

    live.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    if live.is_empty() {
        return Err(AppError::ClaudeChannel(NO_CHANNEL_FOR_REPO_MESSAGE.into()));
    }
    Ok(live)
}

/// Sends a user-authored message into the live Claude Code session for this
/// repository. One direction only: the message is handed over and that is the
/// end of it — whatever Claude does next happens in Claude Code, not here. A
/// per-process bearer token protects the loopback bridge.
///
/// Resolves with the channel's message id, which is only useful for correlating
/// logs.
pub async fn send(
    workspace: &Workspace,
    http: &reqwest::Client,
    raw_message: &str,
) -> Result<String, AppError> {
    let fail = AppError::ClaudeChannel;

    let message = raw_message.trim();
    if message.is_empty() {
        return Err(fail("Message cannot be empty.".into()));
    }
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(fail(format!(
            "Message is too large (maximum {MAX_MESSAGE_BYTES} bytes)."
        )));
    }

    let channels = registrations(workspace).await?;
    let mut last_error: Option<String> = None;

    for channel in channels {
        let url = format!("http://127.0.0.1:{}/messages", channel.port);
        let sent = http
            .post(&url)
            .bearer_auth(&channel.token)
            .timeout(SEND_TIMEOUT)
            .body(message.to_owned())
            .send()
            .await;

        // A dead channel is expected — try the next one before giving up.
        let response = match sent {
            Ok(response) => response,
            Err(error) => {
                last_error = Some(if error.is_timeout() {
                    format!("channel did not accept the message within {SEND_TIMEOUT_LABEL}")
                } else {
                    error.to_string()
                });
                continue;
            }
        };

        let status = response.status();
        let body = match response.text().await {
            Ok(body) => body,
            Err(error) => {
                last_error = Some(error.to_string());
                continue;
            }
        };

        if !status.is_success() {
            let detail = body.trim();
            last_error = Some(if detail.is_empty() {
                format!("channel returned {}", status.as_u16())
            } else {
                format!("channel returned {}: {detail}", status.as_u16())
            });
            continue;
        }

        let accepted = serde_json::from_str::<MessageAccepted>(&body).map_err(|error| {
            fail(format!("failed to decode the channel message ID: {error}"))
        })?;
        if !is_valid_message_id(&accepted.message_id) {
            return Err(fail(
                "The Claude channel returned an invalid message ID.".into(),
            ));
        }
        return Ok(accepted.message_id);
    }

    Err(fail(format!(
        "Could not reach a OnlyDiffs Claude channel for this repository. Restart Claude Code with the OnlyDiffs channel enabled.{}",
        match last_error {
            Some(detail) => format!(" Last error: {detail}"),
            None => String::new(),
        }
    )))
}

/// Whether a Claude Code session is listening for this repository.
///
/// Reports rather than throws: "no channel" is an ordinary state for a status
/// indicator, not a failure, and it is polled often enough that turning it into
/// an error would just mean catching it again.
pub async fn status(workspace: &Workspace) -> ClaudeChannelStatus {
    match registrations(workspace).await {
        Ok(live) => ClaudeChannelStatus {
            connected: !live.is_empty(),
            sessions: live.len(),
        },
        Err(_) => ClaudeChannelStatus {
            connected: false,
            sessions: 0,
        },
    }
}
