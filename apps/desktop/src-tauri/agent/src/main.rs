//! The half of OnlyDiffs that runs on the machine holding the repository.
//!
//! It speaks the protocol on stdin and stdout and nothing else. No port is
//! bound, no daemon is left behind, no state is kept between connections: the
//! process is started by `ssh`, lives as long as that connection, and dies when
//! its stdin closes. Everything it could hold is something a reconnect rebuilds
//! with one `git status`.
//!
//! It has one other job, `channel`: the MCP server a Claude Code session runs
//! so the app can push a line into it. Same binary, so a host that has the
//! agent has the channel, and the same rule about stdio — there it is MCP on
//! stdin and stdout, with the socket the app writes to beside it.
//!
//! stderr is left alone deliberately. A login shell that prints a banner writes
//! there, `ssh` shows it to the user, and nothing on this side ever parses it —
//! which is what stops a message-of-the-day from corrupting a frame.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use onlydiffs_core::error::AppError;
use onlydiffs_core::protocol::{
    read_frame, write_frame, Envelope, Event, FrameError, Message, Request, RequestId, Response,
    PROTOCOL_VERSION,
};
use onlydiffs_core::services::repository::Repository;
use onlydiffs_core::services::watcher::RepoWatcher;
use onlydiffs_core::services::{
    attachment, claude_channel, claude_channel_server, codex_channel, diff, file_tree, history,
    icon_scan,
};
use tokio::io::{stdin, stdout, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, Mutex};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// One frame at a time on the way out.
///
/// Responses are produced concurrently — a diff on a large repository should
/// not hold up a file read — so the writer is the one place that serialises
/// them, and interleaving two frames on one pipe would corrupt both.
type Writer = Arc<Mutex<tokio::io::Stdout>>;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        // How the app checks that the binary already on a host is the right
        // one. Printing and exiting is the whole contract.
        Some("--version") | Some("version") => {
            println!("{VERSION}");
            return;
        }
        Some("serve") | None => serve().await,
        // The Claude Code channel. Runs until Claude Code closes its stdin.
        Some("channel") => {
            if let Err(error) = claude_channel_server::run().await {
                eprintln!("onlydiffs-agent channel: {error}");
                std::process::exit(1);
            }
        }
        Some(other) => {
            eprintln!(
                "onlydiffs-agent: unknown argument {other:?}; expected `serve`, `channel`, or `--version`"
            );
            std::process::exit(2);
        }
    }
}

async fn serve() {
    let writer: Writer = Arc::new(Mutex::new(stdout()));
    let watcher = Arc::new(RepoWatcher::new());
    // Watches are per-root and outlive the request that started them, so they
    // are held here rather than in the handler.
    let watched: Arc<Mutex<HashMap<String, ()>>> = Arc::new(Mutex::new(HashMap::new()));

    // The watcher's callback runs on a notify thread with no runtime of its
    // own, so events reach the writer through a channel rather than by blocking
    // that thread on an async send.
    let (events, mut incoming) = mpsc::unbounded_channel::<Event>();
    {
        let writer = writer.clone();
        tokio::spawn(async move {
            while let Some(event) = incoming.recv().await {
                let mut out = writer.lock().await;
                if write_frame(&mut *out, &Message::Event(event)).await.is_err() {
                    return;
                }
            }
        });
    }

    let mut reader = BufReader::new(stdin());
    loop {
        let message = match read_frame(&mut reader).await {
            Ok(message) => message,
            // A closed stdin is how the connection ends. Anything else is worth
            // saying out loud on stderr, where ssh will show it.
            Err(FrameError::Closed) => break,
            Err(error) => {
                eprintln!("onlydiffs-agent: {error}");
                break;
            }
        };

        let Message::Request(Envelope { id, body }) = message else {
            // Only the app sends requests. A response or an event arriving here
            // means the stream is not what it claims to be.
            eprintln!("onlydiffs-agent: unexpected frame from the client");
            break;
        };

        if matches!(body, Request::Shutdown) {
            let _ = respond(&writer, id, Response::Unit).await;
            break;
        }

        let writer = writer.clone();
        let watcher = watcher.clone();
        let watched = watched.clone();
        let events = events.clone();
        tokio::spawn(async move {
            let response = handle(body, &watcher, &watched, &events).await;
            let _ = respond(&writer, id, response).await;
        });
    }

    // Flush anything the writer task has queued before the process exits, so a
    // final response is not lost to a race with `main` returning.
    let mut out = writer.lock().await;
    let _ = out.flush().await;
}

async fn respond(writer: &Writer, id: RequestId, response: Response) -> Result<(), FrameError> {
    let mut out = writer.lock().await;
    write_frame(&mut *out, &Message::Response(Envelope { id, body: response })).await
}

/// Every failure becomes a `Response::Err` carrying the same tag the app would
/// have produced locally, so a remote git failure is a git failure rather than
/// a transport failure.
fn failed(error: AppError) -> Response {
    Response::Err {
        tag: error.tag().to_owned(),
        message: error.message().to_owned(),
    }
}

async fn handle(
    request: Request,
    watcher: &Arc<RepoWatcher>,
    watched: &Arc<Mutex<HashMap<String, ()>>>,
    events: &mpsc::UnboundedSender<Event>,
) -> Response {
    match request {
        Request::Hello { protocol } => {
            if protocol != PROTOCOL_VERSION {
                return Response::Err {
                    tag: "SshError".into(),
                    message: format!(
                        "this agent speaks protocol {PROTOCOL_VERSION}; the app asked for {protocol}"
                    ),
                };
            }
            Response::Hello {
                protocol: PROTOCOL_VERSION,
                agent_version: VERSION.to_owned(),
            }
        }

        Request::Diff { root } => match diff::get_diff(&repo(&root)).await {
            Ok(value) => Response::Diff(value),
            Err(error) => failed(error),
        },

        Request::FileContents {
            root,
            path,
            old_path,
            status,
            staged,
        } => match diff::get_file_contents(&repo(&root), &path, old_path.as_deref(), status, staged)
            .await
        {
            Ok(value) => Response::FileContents(value),
            Err(error) => failed(error),
        },

        Request::History { root, limit } => match history::get_history(&repo(&root), limit).await {
            Ok(value) => Response::History(value),
            Err(error) => failed(error),
        },

        Request::ListFiles { root } => match file_tree::list_files(&repo(&root)).await {
            Ok(value) => Response::ListFiles(value),
            Err(error) => failed(error),
        },

        Request::StageFile {
            root,
            path,
            old_path,
        } => match diff::stage_file(&repo(&root), &path, old_path.as_deref()).await {
            Ok(()) => Response::Unit,
            Err(error) => failed(error),
        },

        Request::CommitAll { root, message } => {
            match diff::commit_all(&repo(&root), &message).await {
                Ok(head) => Response::Commit(head),
                Err(error) => failed(error),
            }
        }

        Request::CommitMessageDiff { root } => {
            match diff::commit_message_diff(&repo(&root)).await {
                Ok(document) => Response::CommitMessageDiff(document),
                Err(error) => failed(error),
            }
        }

        Request::IconCandidates { root } => match icon_scan::discover(&repo(&root)).await {
            Ok(candidates) => Response::IconCandidates(candidates),
            Err(message) => Response::Err {
                tag: "GitError".into(),
                message,
            },
        },

        Request::Git { root, args } => {
            let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
            match repo(&root).git(&borrowed).await {
                Ok(stdout) => Response::Git(stdout),
                Err(error) => failed(error),
            }
        }

        Request::ReadFile {
            root,
            path,
            max_bytes,
        } => match repo(&root)
            .read_file(std::path::Path::new(&path), max_bytes)
            .await
        {
            Ok(bytes) => Response::Bytes(bytes),
            Err(error) => failed(error),
        },

        Request::Metadata { root, path } => {
            match repo(&root).metadata(std::path::Path::new(&path)).await {
                Ok(meta) => Response::Metadata(meta),
                Err(error) => failed(error),
            }
        }

        Request::ClaudeStatus { root } => {
            Response::ClaudeStatus(claude_channel::status(std::path::Path::new(&root)).await)
        }

        Request::ClaudeSend { root, message } => {
            match claude_channel::send(std::path::Path::new(&root), &message).await {
                Ok(id) => Response::ClaudeSent(id),
                Err(error) => failed(error),
            }
        }

        Request::CodexStatus { root } => {
            Response::CodexStatus(codex_channel::status(std::path::Path::new(&root)).await)
        }

        Request::CodexSend { root, message } => {
            match codex_channel::send(std::path::Path::new(&root), &message).await {
                Ok(id) => Response::CodexSent(id),
                Err(error) => failed(error),
            }
        }

        Request::WriteAttachment { root, bytes } => {
            match attachment::write(&repo(&root), &bytes).await {
                Ok(path) => Response::Attachment(path),
                Err(error) => failed(error),
            }
        }

        Request::ResolveRepository { path } => Response::Repository(resolve_repository(&path)),

        Request::Watch { root } => {
            let mut watched = watched.lock().await;
            if watched.contains_key(&root) {
                return Response::Unit;
            }
            let events = events.clone();
            let announced = root.clone();
            watcher.watch(PathBuf::from(&root), move || {
                // A closed channel means the client is gone; there is nothing
                // to tell and nothing to do about it.
                let _ = events.send(Event::RepoChanged {
                    root: announced.clone(),
                });
            });
            watched.insert(root, ());
            Response::Unit
        }

        Request::Unwatch { root } => {
            watched.lock().await.remove(&root);
            // `RepoWatcher` holds one watch and swaps it; there is nothing to
            // release here beyond forgetting that this root was registered.
            Response::Unit
        }

        // Handled before dispatch, because it has to stop the loop.
        Request::Shutdown => Response::Unit,
    }
}

fn repo(root: &str) -> Repository {
    Repository::local(PathBuf::from(root))
}

/// The repository root at or above `path`, mirroring what the app does locally
/// so a path pasted into the picker resolves the same way on either machine.
fn resolve_repository(path: &str) -> Option<String> {
    let mut candidate = PathBuf::from(shell_expand_home(path));
    if !candidate.is_dir() {
        return None;
    }
    loop {
        if candidate.join(".git").exists() {
            return Some(candidate.to_string_lossy().into_owned());
        }
        if !candidate.pop() {
            return None;
        }
    }
}

/// `~` is the shell's, and nothing here goes through a shell.
fn shell_expand_home(path: &str) -> String {
    let Some(rest) = path.strip_prefix('~') else {
        return path.to_owned();
    };
    let Ok(home) = std::env::var("HOME") else {
        return path.to_owned();
    };
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    if rest.is_empty() {
        home
    } else {
        format!("{home}/{rest}")
    }
}
