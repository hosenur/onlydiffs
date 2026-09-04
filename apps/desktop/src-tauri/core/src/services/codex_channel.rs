//! The bridge to a Codex session working in the open repository.
//!
//! The same idea as `claude_channel` and almost none of the same mechanics,
//! because Codex is reached by a different route. Claude Code registers a
//! channel per session and a message is handed to a process that is listening
//! right now; Codex has one shared daemon, and only a session started against
//! it — `codex --remote unix://` — can be spoken to at all. A plain `codex`
//! never registers with the daemon and cannot be reached, which is Codex's
//! design rather than this app's.
//!
//! Two questions are asked before a message goes anywhere, and both have to
//! answer yes. Is a Codex process running in this repository? That is the
//! process table, because the daemon keeps a thread loaded for a while after
//! its TUI exits and would otherwise run a message headlessly for a session
//! nobody is looking at. And does the daemon hold a thread for this
//! repository? That is the daemon, because a session's thread is the only thing
//! a message can be queued on. With both, `thread/queue/add` starts a turn on an
//! idle session at once and queues behind a busy one — the same thing typing
//! into the TUI does.
//!
//! It runs where the repository is, for the same reason the Claude bridge does:
//! a thread's working directory is a path on the machine the session is on, and
//! the daemon's socket is in that machine's home directory.

use std::path::Path;

use crate::contract::CodexChannelStatus;
use crate::error::AppError;
use crate::services::codex_app_server::AppServer;
use crate::services::codex_session;

/// Matches the Claude bridge rather than any limit Codex documents. A review
/// comment that runs to tens of kilobytes is a bug upstream of here.
const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// What a user runs to open a session this app can reach. `-C` is not
/// optional: a `--remote` session without it is filed under the daemon's own
/// working directory, and no repository matches that.
pub const START_COMMAND: &str = "codex --remote unix:// -C \"$PWD\"";

/// The same, for a session that already exists and needs reattaching.
pub const RESUME_COMMAND: &str = "codex resume --last --remote unix:// -C \"$PWD\"";

const NO_SESSION_MESSAGE: &str =
    "No Codex session is running in this repository. Open one there with `codex --remote unix:// -C \"$PWD\"`.";

/// A session is running but cannot be reached.
///
/// Only a session attached to Codex's shared daemon, and started with `-C`, has
/// a thread filed under this repository. Saying "no session" here would be a
/// lie the user can see through — their session is right in front of them.
const NOT_CONNECTED_MESSAGE: &str = concat!(
    "A Codex session is running here but is not attached to Codex's shared daemon under this repository. ",
    "Close it, then run `codex resume --last --remote unix:// -C \"$PWD\"` so OnlyDiffs can reach it."
);

/// Hands a user-authored message to the Codex session working in this
/// repository.
///
/// One direction only, like the Claude bridge: the message is queued and
/// whatever happens next happens in Codex. An idle session starts a turn on it
/// at once; a busy one takes it up when its current turn ends.
///
/// It goes to the most recently active thread the daemon holds for the
/// repository. A repository with several is a user who has opened Codex in it
/// more than once, and the newest is the one they are looking at.
pub async fn send(root: &Path, raw_message: &str) -> Result<String, AppError> {
    let fail = AppError::CodexChannel;

    let message = raw_message.trim();
    if message.is_empty() {
        return Err(fail("Message cannot be empty.".into()));
    }
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(fail(format!(
            "Message is too large (maximum {MAX_MESSAGE_BYTES} bytes)."
        )));
    }

    // The process table first, and once: it is the check that costs process
    // spawns on macOS, and nothing below is worth doing without it.
    if codex_session::running_in(root).await == 0 {
        return Err(fail(NO_SESSION_MESSAGE.into()));
    }

    // One connection for both the lookup and the send.
    let mut server = AppServer::connect().await?;
    let thread = server
        .loaded_threads_in(root)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| fail(NOT_CONNECTED_MESSAGE.into()))?;
    server.queue(&thread.id, message).await
}

/// Whether a Codex session for this repository can be sent to.
///
/// Reports rather than throws, for the same reason the Claude one does: "no
/// session" is an ordinary state for an indicator that asks four times a
/// minute, not a failure.
///
/// `sessions` counts every Codex process working in the repository, attached
/// or not, so the bar can say "running but not connected" about a session the
/// user can see. `connected` is the stricter claim: the daemon holds a thread
/// for this repository and a message would reach it.
pub async fn status(root: &Path) -> CodexChannelStatus {
    let sessions = codex_session::running_in(root).await;
    if sessions == 0 {
        return CodexChannelStatus {
            connected: false,
            sessions: 0,
        };
    }
    let connected = match AppServer::connect().await {
        Ok(mut server) => server
            .loaded_threads_in(root)
            .await
            .map(|threads| !threads.is_empty())
            .unwrap_or(false),
        // A daemon that is not running has no thread to offer, which is the
        // same answer as a session started without `--remote`.
        Err(_) => false,
    };
    CodexChannelStatus {
        connected,
        sessions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule the whole module turns on: a repository with no Codex process
    /// running in it has no session, whatever the daemon holds. A loaded thread
    /// is the record of a session, not a session.
    #[tokio::test]
    async fn a_repository_with_no_running_session_refuses_to_send() {
        let dir = tempfile::TempDir::new().expect("temp dir");

        let refused = send(dir.path(), "about this line").await;

        let error = refused.expect_err("refused");
        assert_eq!(error.tag(), "CodexChannelError");
        assert!(
            error.message().contains("No Codex session is running"),
            "got: {}",
            error.message()
        );
        assert!(error.message().contains("-C"), "the fix names -C: {}", error.message());
    }

    #[tokio::test]
    async fn a_repository_no_codex_session_is_running_in_reports_no_sessions() {
        let dir = tempfile::TempDir::new().expect("temp dir");

        let status = status(dir.path()).await;

        assert!(!status.connected);
        assert_eq!(status.sessions, 0);
    }

    #[tokio::test]
    async fn an_empty_message_is_refused_before_anything_is_spawned() {
        let dir = tempfile::TempDir::new().expect("temp dir");

        let refused = send(dir.path(), "   ").await;

        assert_eq!(refused.expect_err("refused").tag(), "CodexChannelError");
    }

    #[tokio::test]
    async fn an_oversized_message_is_refused_by_size_rather_than_by_the_daemon() {
        let dir = tempfile::TempDir::new().expect("temp dir");

        let refused = send(dir.path(), &"x".repeat(MAX_MESSAGE_BYTES + 1)).await;

        assert!(refused.expect_err("refused").message().contains("too large"));
    }

    #[test]
    fn every_command_the_app_shows_carries_the_directory_flag() {
        for command in [START_COMMAND, RESUME_COMMAND, NO_SESSION_MESSAGE, NOT_CONNECTED_MESSAGE] {
            assert!(command.contains("-C \"$PWD\""), "{command}");
            assert!(command.contains("--remote unix://"), "{command}");
        }
    }
}
