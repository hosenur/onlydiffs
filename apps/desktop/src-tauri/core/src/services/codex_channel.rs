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
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;

use crate::contract::CodexChannelStatus;
use crate::error::AppError;

/// Matches the Claude bridge rather than any limit Codex documents. The message
/// crosses as one argument to a child process, and while the OS would take far
/// more, a review comment that runs to tens of kilobytes is a bug upstream of
/// here.
const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// How long to wait for `codex queue` to accept the message. The queue is a
/// local database write, so this only ever expires when something is wrong.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
const SEND_TIMEOUT_LABEL: &str = "10 seconds";

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

/// The socket Codex's shared app-server daemon listens on.
///
/// This is the piece that actually drains the queue. Without it a message is
/// accepted, written down, and delivered to nobody until the daemon next runs —
/// so whether it is up is part of the answer to "can this repository be sent
/// to", not a detail.
const CONTROL_SOCKET: &str = ".codex/app-server-control/app-server-control.sock";

const NO_SESSION_MESSAGE: &str =
    "No Codex session has worked in this repository. Open one with `codex` in this repository.";

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

/// Where the `codex` executable is.
///
/// `PATH` is searched by hand rather than left to the spawn, because the app is
/// usually launched from Finder and inherits the short `PATH` a GUI process
/// gets — `/usr/bin:/bin:/usr/sbin:/sbin`, which is not where anybody installs
/// Codex. The known install locations are the fallback, and bare `codex` is the
/// last resort so a host with it somewhere unusual still reports the spawn
/// failure rather than a path this function invented.
fn codex_binary() -> PathBuf {
    static RESOLVED: OnceLock<PathBuf> = OnceLock::new();
    RESOLVED
        .get_or_init(|| {
            if let Some(found) = std::env::var_os("PATH")
                .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
                .unwrap_or_default()
                .into_iter()
                .map(|directory| directory.join("codex"))
                .find(|candidate| candidate.is_file())
            {
                return found;
            }

            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            let fallbacks = [
                home.join(".local/bin/codex"),
                home.join(".codex/packages/standalone/current/codex"),
                PathBuf::from("/opt/homebrew/bin/codex"),
                PathBuf::from("/usr/local/bin/codex"),
            ];
            fallbacks
                .into_iter()
                .find(|candidate| candidate.is_file())
                .unwrap_or_else(|| PathBuf::from("codex"))
        })
        .clone()
}

/// The id out of `Queued message <id> for thread <id>.`
///
/// Parsed positionally because that is all the CLI offers — there is no
/// machine-readable form of this output. A shape that stops matching is
/// reported as a send that worked without an id rather than as a failure,
/// because by then the message really is queued and saying otherwise would
/// invite the user to send it twice.
fn queued_message_id(stdout: &str) -> Option<String> {
    let mut words = stdout.split_whitespace();
    if words.next()? != "Queued" || words.next()? != "message" {
        return None;
    }
    let id = words.next()?;
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    valid.then(|| id.to_owned())
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

    let thread = threads(root)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| fail(NO_SESSION_MESSAGE.into()))?;

    let run = Command::new(codex_binary())
        .arg("queue")
        .arg("--thread")
        .arg(&thread.id)
        .arg("--message")
        .arg(message)
        .output();

    let output = match tokio::time::timeout(SEND_TIMEOUT, run).await {
        Err(_) => {
            return Err(fail(format!(
                "Codex did not accept the message within {SEND_TIMEOUT_LABEL}."
            )))
        }
        Ok(Err(error)) => {
            return Err(fail(format!(
                "failed to run codex: {error}. Is the Codex CLI installed?"
            )))
        }
        Ok(Ok(output)) => output,
    };

    if !output.status.success() {
        // The CLI reports its failures on stderr, prefixed with `Error:`, and
        // the part after that prefix is the sentence worth showing.
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default()
            .trim()
            .trim_start_matches("Error:")
            .trim();
        return Err(fail(if detail.is_empty() {
            "Codex refused the message.".to_owned()
        } else {
            detail.to_owned()
        }));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(queued_message_id(&stdout).unwrap_or_else(|| thread.id.clone()))
}

/// Whether Codex's shared daemon is up to deliver what is queued.
///
/// Connecting is the check rather than looking for the socket file: a daemon
/// killed rather than stopped leaves the file behind, and a queue nobody is
/// draining is exactly the state worth catching.
#[cfg(unix)]
async fn is_delivering() -> bool {
    let socket = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(CONTROL_SOCKET);
    tokio::net::UnixStream::connect(&socket).await.is_ok()
}

/// Nothing equivalent to probe on Windows, so the optimistic answer: better a
/// missing warning than one shown to everybody.
#[cfg(not(unix))]
async fn is_delivering() -> bool {
    true
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
    match threads(root).await {
        Ok(found) => CodexChannelStatus {
            connected: !found.is_empty(),
            sessions: found.len(),
            delivering: is_delivering().await,
        },
        Err(_) => CodexChannelStatus {
            connected: false,
            sessions: 0,
            delivering: false,
        },
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
    fn the_queued_id_is_taken_out_of_what_the_cli_prints() {
        assert_eq!(
            queued_message_id("Queued message 01a06b04-77b6-7823 for thread 01a06afb-9106.\n")
                .as_deref(),
            Some("01a06b04-77b6-7823")
        );
    }

    #[test]
    fn a_line_that_is_not_the_queue_confirmation_yields_no_id() {
        // The caller falls back to the thread id rather than failing, because
        // by this point the message is queued.
        assert_eq!(queued_message_id(""), None);
        assert_eq!(queued_message_id("Error: No active session found."), None);
        assert_eq!(queued_message_id("Queued message"), None);
    }

    #[test]
    fn a_path_is_folded_before_it_is_compared() {
        assert_eq!(
            normalize(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
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
