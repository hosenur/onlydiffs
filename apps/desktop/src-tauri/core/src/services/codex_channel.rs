//! The bridge to a Codex session working in the open repository.
//!
//! The same idea as `claude_channel` and almost none of the same mechanics,
//! because Codex is reached by a different route. Claude Code registers a
//! loopback server per session and a message is handed to a process that is
//! listening right now; Codex keeps a durable per-thread queue, and a message
//! put there is delivered the next time that thread runs. So this cannot ask
//! "is anything listening" — nothing ever is — and instead asks which threads
//! belong to this repository.
//!
//! What that buys is a send that does not require the session to be up. A
//! message queued against a repository whose Codex session is closed waits in
//! the queue and arrives when the user opens it again, which is the behaviour
//! anyone would want from a review comment and the one thing the Claude side
//! cannot do.
//!
//! It runs where the repository is, for the same reason the Claude bridge does:
//! a thread's working directory is a path on the machine the session is on, and
//! reading the index from the user's laptop would match nothing on a host.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

use crate::contract::CodexChannelStatus;
use crate::error::AppError;
use crate::services::codex_app_server::AppServer;
use crate::services::codex_session;

/// Matches the Claude bridge rather than any limit Codex documents. The message
/// crosses as one argument to a child process, and while the OS would take far
/// more, a review comment that runs to tens of kilobytes is a bug upstream of
/// here.
const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// Where Codex keeps one JSONL transcript per thread, partitioned by date.
const SESSIONS_DIR: &str = ".codex/sessions";

/// How far back to look for threads belonging to a repository.
///
/// The partitioning is what makes a window worth having: without one this
/// would walk every transcript ever written, and there are thousands. Two weeks
/// is well past the point where a thread is one the user would still expect a
/// comment to reach, and the scan stays a handful of files.
const LOOKBACK_DAYS: i64 = 14;

/// The most of a transcript's first line that will be read looking for its
/// header. The header carries the session's full base instructions, so it is
/// tens of kilobytes rather than the hundreds of bytes the two useful fields
/// would suggest.
const MAX_HEADER_BYTES: u64 = 1024 * 1024;


const NO_SESSION_MESSAGE: &str =
    "No Codex session is running in this repository. Open one with `codex` there.";

/// A session is running but cannot be reached.
///
/// Only a session attached to Codex's shared daemon can be sent to, and a plain
/// `codex` never attaches to one. Saying "no session" here would be a lie the
/// user can see through — their session is right in front of them.
const NOT_CONNECTED_MESSAGE: &str = concat!(
    "A Codex session is running here but is not connected to Codex's shared daemon. ",
    "Start it with `codex --remote unix://` so OnlyDiffs can reach it."
);

/// The header Codex writes as the first line of every transcript.
#[derive(Deserialize)]
struct RolloutHeader {
    #[serde(rename = "type")]
    kind: String,
    payload: SessionMeta,
}

#[derive(Deserialize)]
struct SessionMeta {
    session_id: String,
    cwd: String,
}

/// One Codex thread, reduced to what addressing it needs.
#[derive(Debug, Clone)]
struct Thread {
    id: String,
    /// Last write to the transcript, which is the last turn it took.
    updated_at: SystemTime,
}

/// A thread's id and working directory never change once its transcript
/// exists, so a header that has been read once never needs reading again. This
/// is what keeps a four-second status poll down to one `stat` per file.
fn header_cache() -> &'static Mutex<HashMap<PathBuf, Option<(String, String)>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<(String, String)>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn sessions_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(SESSIONS_DIR)
}

/// Lexical `.`/`..` folding, so a thread's recorded `cwd` and the open
/// repository are compared in the same form. Deliberately not `canonicalize`:
/// symlinks stay unresolved, matching how the path was recorded.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The civil date `days` after the epoch, as `(year, month, day)`.
///
/// Howard Hinnant's algorithm, inlined rather than taken as a dependency: this
/// crate is compiled for four targets and uploaded to hosts, and a date library
/// would be the whole of `chrono` to name fourteen directories.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = (z - era * 146_097) as i64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// The transcript directories worth looking in, newest day first.
///
/// Codex partitions by UTC date, which is what the transcripts are stamped
/// with, so the days are counted in UTC too rather than in local time.
fn recent_day_directories() -> Vec<PathBuf> {
    let root = sessions_dir();
    let today = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs() as i64 / 86_400)
        .unwrap_or(0);

    (0..LOOKBACK_DAYS)
        .map(|back| {
            let (year, month, day) = civil_from_days(today - back);
            root.join(format!("{year:04}"))
                .join(format!("{month:02}"))
                .join(format!("{day:02}"))
        })
        .collect()
}

/// The session id and working directory recorded in a transcript's header.
///
/// A transcript that cannot be read, or whose first line is not a header, is
/// not an error worth showing: it is a file Codex is in the middle of writing,
/// or one from a version that wrote something else. The next candidate may well
/// be the session being looked for.
async fn header_of(path: &Path) -> Option<(String, String)> {
    if let Some(cached) = header_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(path).cloned())
    {
        return cached;
    }

    let parsed = read_header(path).await;
    if let Ok(mut cache) = header_cache().lock() {
        cache.insert(path.to_path_buf(), parsed.clone());
    }
    parsed
}

async fn read_header(path: &Path) -> Option<(String, String)> {
    let file = tokio::fs::File::open(path).await.ok()?;
    let mut line = String::new();
    BufReader::new(file.take(MAX_HEADER_BYTES))
        .read_line(&mut line)
        .await
        .ok()?;

    let header = serde_json::from_str::<RolloutHeader>(&line).ok()?;
    if header.kind != "session_meta" {
        return None;
    }
    Some((header.payload.session_id, header.payload.cwd))
}

/// The Codex threads that have worked in this repository, newest first.
async fn threads(root: &Path) -> Result<Vec<Thread>, AppError> {
    let repo_path = normalize(root);
    let mut found: Vec<Thread> = Vec::new();

    for directory in recent_day_directories() {
        let Ok(mut entries) = tokio::fs::read_dir(&directory).await else {
            // A day with no sessions has no directory, which is the common case
            // for most of the window.
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some((id, cwd)) = header_of(&path).await else {
                continue;
            };
            if normalize(Path::new(&cwd)) != repo_path {
                continue;
            }
            let updated_at = entry
                .metadata()
                .await
                .and_then(|meta| meta.modified())
                .unwrap_or(UNIX_EPOCH);
            found.push(Thread { id, updated_at });
        }
    }

    found.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    if found.is_empty() {
        return Err(AppError::CodexChannel(NO_SESSION_MESSAGE.into()));
    }
    Ok(found)
}



/// Queues a user-authored message for the Codex session working in this
/// repository.
///
/// One direction only, like the Claude bridge: the message is handed over and
/// whatever happens next happens in Codex. Unlike the Claude bridge, the
/// session does not have to be running — the message waits in Codex's queue and
/// is delivered the next time that thread takes a turn.
///
/// It goes to the most recently active thread for the repository. A repository
/// with several is a user who has opened Codex in it more than once, and the
/// newest is the one they are looking at.
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

    if codex_session::running_in(root).await == 0 {
        return Err(fail(NO_SESSION_MESSAGE.into()));
    }

    let thread = deliverable_thread(root)
        .await
        .ok_or_else(|| fail(NOT_CONNECTED_MESSAGE.into()))?;

    let mut server = AppServer::connect().await?;
    server.start_turn(&thread, message).await
}

/// The thread a message for this repository should be started on: one the
/// repository owns, that the daemon has open.
///
/// The daemon is the only route to a running session, so a thread it has not
/// loaded cannot be reached at all — which is the case for a session started as
/// plain `codex`, since that never registers with it.
async fn deliverable_thread(root: &Path) -> Option<String> {
    let mine = threads(root).await.ok()?;
    let mut server = AppServer::connect().await.ok()?;
    let loaded = server.loaded_threads().await.ok()?;
    mine.into_iter()
        .find(|thread| loaded.iter().any(|id| id == &thread.id))
        .map(|thread| thread.id)
}

/// Whether any Codex thread has worked in this repository.
///
/// Reports rather than throws, for the same reason the Claude one does: "no
/// session" is an ordinary state for an indicator that asks four times a
/// minute, not a failure.
///
/// "Connected" is a softer claim here than it is for Claude. It says a thread
/// exists that a message can be queued against, not that anything is running —
/// which is the honest reading, because a queued message is delivered whether
/// or not anything is running when it is sent.
pub async fn status(root: &Path) -> CodexChannelStatus {
    let sessions = codex_session::running_in(root).await;
    if sessions == 0 {
        return CodexChannelStatus {
            connected: false,
            sessions: 0,
            thread: None,
            delivering: false,
        };
    }
    // A session is running. Whether it can be *spoken to* is a second question:
    // only a session started against the shared daemon is reachable, and saying
    // so is more use than reporting a session that cannot be sent to.
    let reachable = deliverable_thread(root).await.is_some();
    // The newest thread for the repository, which is the one a session that is
    // running here would be resumed from. Only useful when it cannot be
    // reached, which is exactly when the app needs to name it.
    let thread = if reachable {
        None
    } else {
        threads(root)
            .await
            .ok()
            .and_then(|found| found.into_iter().next())
            .map(|thread| thread.id)
    };
    CodexChannelStatus {
        connected: reachable,
        sessions,
        thread,
        delivering: reachable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_and_a_leap_day_convert_back_to_themselves() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(20_700), (2026, 9, 4));
        // The two the algorithm's era arithmetic is most likely to get wrong.
        assert_eq!(civil_from_days(10_956), (1999, 12, 31));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
    }

    #[test]
    fn the_window_names_one_directory_per_day_newest_first() {
        let days = recent_day_directories();

        assert_eq!(days.len(), LOOKBACK_DAYS as usize);
        // Distinct, ordered, and shaped `<year>/<month>/<day>` under the
        // sessions directory — the layout Codex writes.
        let unique: std::collections::HashSet<_> = days.iter().collect();
        assert_eq!(unique.len(), days.len());
        for day in &days {
            let parts: Vec<_> = day
                .components()
                .rev()
                .take(3)
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            assert_eq!(parts[0].len(), 2, "day is zero-padded: {day:?}");
            assert_eq!(parts[1].len(), 2, "month is zero-padded: {day:?}");
            assert_eq!(parts[2].len(), 4, "year is four digits: {day:?}");
        }
    }



    #[test]
    fn a_path_is_folded_before_it_is_compared() {
        assert_eq!(
            normalize(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
    }

    /// The rule the whole module now turns on: a repository with no Codex
    /// process running in it has no session, whatever its history says. A
    /// transcript is the record of a session, not a session.
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
    }

    #[tokio::test]
    async fn a_repository_no_codex_session_has_touched_reports_no_sessions() {
        let dir = tempfile::TempDir::new().expect("temp dir");

        let status = status(dir.path()).await;

        assert!(!status.connected);
        assert_eq!(status.sessions, 0);
        // Nothing to deliver to means nothing to claim about delivery.
        assert!(!status.delivering);
    }

    #[tokio::test]
    async fn an_empty_message_is_refused_before_anything_is_spawned() {
        let dir = tempfile::TempDir::new().expect("temp dir");

        let refused = send(dir.path(), "   ").await;

        assert_eq!(refused.expect_err("refused").tag(), "CodexChannelError");
    }

    #[tokio::test]
    async fn an_oversized_message_is_refused_by_size_rather_than_by_the_cli() {
        let dir = tempfile::TempDir::new().expect("temp dir");

        let refused = send(dir.path(), &"x".repeat(MAX_MESSAGE_BYTES + 1)).await;

        assert!(refused.expect_err("refused").message().contains("too large"));
    }

    #[tokio::test]
    async fn a_header_that_is_not_a_session_header_is_skipped_rather_than_failing() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("rollout-nonsense.jsonl");
        tokio::fs::write(&path, "{\"type\":\"turn\",\"payload\":{}}\n")
            .await
            .expect("write");

        assert!(read_header(&path).await.is_none());
    }

    #[tokio::test]
    async fn a_session_header_yields_its_id_and_working_directory() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("rollout-good.jsonl");
        tokio::fs::write(
            &path,
            "{\"type\":\"session_meta\",\"payload\":{\"session_id\":\"abc\",\"cwd\":\"/w/repo\"}}\n{\"type\":\"turn\"}\n",
        )
        .await
        .expect("write");

        assert_eq!(
            read_header(&path).await,
            Some(("abc".to_owned(), "/w/repo".to_owned()))
        );
    }
}
