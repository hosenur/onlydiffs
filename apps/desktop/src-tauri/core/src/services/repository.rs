//! The machine a repository lives on, and everything the app needs from it.
//!
//! Every service above this asks a `Repository` rather than spawning `git` or
//! touching the filesystem itself. That is the whole point of the type: there
//! is exactly one place that knows whether the repository is on this machine or
//! another one, and adding the second answer does not touch the diff walk, the
//! history parser, or the file tree.
//!
//! `Host` is an enum rather than a trait object on purpose. There are two
//! answers and both are known at compile time, so this needs no `async-trait`,
//! allocates no boxed futures per call, and keeps every method a plain
//! `async fn` whose errors are the same `AppError` the rest of the app speaks.
//!
//! The remote variant holds a channel rather than a connection. That is what
//! keeps SSH out of this crate entirely: the agent compiled from it has no idea
//! a network exists, and the app owns the transport that services the channel.
//!
//! Note which methods are coarse. `diff` is not `git` called in a loop — it is
//! one question with one answer, because the local walk costs one `git`
//! invocation per changed file and running that over a network would be several
//! hundred round trips for one screen.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::contract::{ChangeStatus, ClaudeChannelStatus, CodexChannelStatus, Commit, FullFileContents, RepoDiff};
use crate::error::AppError;
use crate::protocol::{Request, Response};
use crate::services::icon_scan::Candidate;
use crate::services::{
    attachment, claude_channel, codex_channel, diff, file_tree, git, history, icon_scan,
};

/// A file's stat, reduced to the three things the app actually reads. Enough
/// for the icon scanner's size and mtime checks, and small enough to cross a
/// wire without a platform-specific `Metadata` on the other side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMeta {
    /// False for directories and for symlinks, which are never followed.
    pub is_file: bool,
    pub len: u64,
    /// Milliseconds since the Unix epoch, or 0 where the platform has no mtime.
    pub modified_millis: u128,
}

/// One question for a remote agent, and where its answer goes.
#[derive(Debug)]
pub struct RemoteCall {
    pub request: Request,
    pub reply: oneshot::Sender<Result<Response, AppError>>,
}

/// The app's end of the transport. Sending on it is what reaches the host.
pub type RemoteSender = mpsc::UnboundedSender<RemoteCall>;

/// Which machine a repository is on.
#[derive(Debug, Clone)]
enum Host {
    /// This one. Spawns processes and reads files directly.
    Local,
    /// Another one, reached through a channel the app services over SSH.
    Remote {
        calls: RemoteSender,
        /// How the host is written for display, e.g. `build-box`.
        label: String,
    },
}

/// An open repository: a root path, plus the machine that path is on.
///
/// Cheap to clone and to construct — it holds a path and a handle, never a
/// connection of its own — so services take one per call rather than being
/// built around one.
#[derive(Debug, Clone)]
pub struct Repository {
    root: PathBuf,
    host: Host,
}

impl Repository {
    /// A repository on this machine.
    pub fn local(root: PathBuf) -> Self {
        Self {
            root,
            host: Host::Local,
        }
    }

    /// A repository on `label`, reached through `calls`.
    pub fn remote(label: String, root: PathBuf, calls: RemoteSender) -> Self {
        Self {
            root,
            host: Host::Remote { calls, label },
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self.host, Host::Remote { .. })
    }

    /// The host this repository is on, or `None` when it is this machine.
    pub fn host_label(&self) -> Option<&str> {
        match &self.host {
            Host::Local => None,
            Host::Remote { label, .. } => Some(label),
        }
    }

    /// The root as the *host* writes it, which is what every request carries.
    fn root_string(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    /// Sends one request and waits for its answer.
    ///
    /// A dropped reply channel means the transport went away mid-flight, which
    /// is a disconnection rather than a failure of whatever was being asked —
    /// the distinction is what lets the app offer to reconnect.
    async fn call(&self, request: Request) -> Result<Response, AppError> {
        let Host::Remote { calls, label } = &self.host else {
            return Err(AppError::Ssh(
                "a remote request was made against a local repository".into(),
            ));
        };
        let (reply, answered) = oneshot::channel();
        calls
            .send(RemoteCall { request, reply })
            .map_err(|_| AppError::SshDisconnected(format!("the connection to {label} is closed")))?;
        match answered.await {
            Ok(result) => result,
            Err(_) => Err(AppError::SshDisconnected(format!(
                "the connection to {label} dropped while it was answering"
            ))),
        }
    }

    /// Turns an agent's `Err` response back into the variant it started as, so
    /// a remote git failure reaches the renderer tagged as a git failure rather
    /// than as a transport one.
    fn rebuild(tag: &str, message: String) -> AppError {
        match tag {
            "GitError" => AppError::Git(message),
            "WorkTreeError" => AppError::WorkTree(message),
            "InvalidPathError" => AppError::InvalidPath(message),
            "CommitMessageError" => AppError::CommitMessage(message),
            "ClaudeChannelError" => AppError::ClaudeChannel(message),
            "CodexChannelError" => AppError::CodexChannel(message),
            "AttachmentError" => AppError::Attachment(message),
            "NoProjectOpenError" => AppError::NoProjectOpen(message),
            "InvalidProjectError" => AppError::InvalidProject(message),
            "SshDisconnectedError" => AppError::SshDisconnected(message),
            _ => AppError::Ssh(message),
        }
    }

    /// The repository root, in the path style of the machine it is on.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// How the root is written for display and for the recents list. Not
    /// `Path::display`: a remote root has to carry its host to be meaningful.
    pub fn display_path(&self) -> String {
        match &self.host {
            Host::Local => self.root_string(),
            Host::Remote { label, .. } => format!("{label}:{}", self.root.display()),
        }
    }

    /// Runs one `git` invocation in the repository root and returns its stdout.
    ///
    /// The narrow escape hatch, for the few things that have no richer request.
    /// Anything issued in a loop belongs behind a coarser method instead.
    pub async fn git(&self, args: &[&str]) -> Result<String, AppError> {
        match &self.host {
            Host::Local => git::run_in(&self.root, args).await,
            Host::Remote { .. } => {
                let request = Request::Git {
                    root: self.root_string(),
                    args: args.iter().map(|arg| (*arg).to_owned()).collect(),
                };
                match self.call(request).await? {
                    Response::Git(stdout) => Ok(stdout),
                    other => Err(unexpected(other)),
                }
            }
        }
    }

    /// Metadata for every change in the repository.
    pub async fn diff(&self) -> Result<RepoDiff, AppError> {
        match &self.host {
            Host::Local => diff::get_diff(self).await,
            Host::Remote { .. } => {
                match self.call(Request::Diff { root: self.root_string() }).await? {
                    Response::Diff(mut value) => {
                        // The agent knows only its own path; the label belongs
                        // to the side that has one.
                        value.repo_path = self.display_path();
                        Ok(value)
                    }
                    other => Err(unexpected(other)),
                }
            }
        }
    }

    pub async fn file_contents(
        &self,
        path: &str,
        old_path: Option<&str>,
        status: ChangeStatus,
        staged: bool,
    ) -> Result<FullFileContents, AppError> {
        match &self.host {
            Host::Local => diff::get_file_contents(self, path, old_path, status, staged).await,
            Host::Remote { .. } => {
                let request = Request::FileContents {
                    root: self.root_string(),
                    path: path.to_owned(),
                    old_path: old_path.map(str::to_owned),
                    status,
                    staged,
                };
                match self.call(request).await? {
                    Response::FileContents(value) => Ok(value),
                    other => Err(unexpected(other)),
                }
            }
        }
    }

    pub async fn history(&self, limit: Option<f64>) -> Result<Vec<Commit>, AppError> {
        match &self.host {
            Host::Local => history::get_history(self, limit).await,
            Host::Remote { .. } => {
                let request = Request::History {
                    root: self.root_string(),
                    limit,
                };
                match self.call(request).await? {
                    Response::History(value) => Ok(value),
                    other => Err(unexpected(other)),
                }
            }
        }
    }

    pub async fn list_files(&self) -> Result<Vec<String>, AppError> {
        match &self.host {
            Host::Local => file_tree::list_files(self).await,
            Host::Remote { .. } => {
                match self.call(Request::ListFiles { root: self.root_string() }).await? {
                    Response::ListFiles(value) => Ok(value),
                    other => Err(unexpected(other)),
                }
            }
        }
    }

    pub async fn stage_file(&self, path: &str, old_path: Option<&str>) -> Result<(), AppError> {
        match &self.host {
            Host::Local => diff::stage_file(self, path, old_path).await,
            Host::Remote { .. } => {
                let request = Request::StageFile {
                    root: self.root_string(),
                    path: path.to_owned(),
                    old_path: old_path.map(str::to_owned),
                };
                match self.call(request).await? {
                    Response::Unit => Ok(()),
                    other => Err(unexpected(other)),
                }
            }
        }
    }

    pub async fn commit_all(&self, message: &str) -> Result<String, AppError> {
        match &self.host {
            Host::Local => diff::commit_all(self, message).await,
            Host::Remote { .. } => {
                let request = Request::CommitAll {
                    root: self.root_string(),
                    message: message.to_owned(),
                };
                match self.call(request).await? {
                    Response::Commit(head) => Ok(head),
                    other => Err(unexpected(other)),
                }
            }
        }
    }

    /// The annotated diff the commit-message model is shown. Built where the
    /// repository is, so the patches cross once rather than once each.
    pub async fn commit_message_diff(&self) -> Result<String, AppError> {
        match &self.host {
            Host::Local => diff::commit_message_diff(self).await,
            Host::Remote { .. } => {
                match self
                    .call(Request::CommitMessageDiff { root: self.root_string() })
                    .await?
                {
                    Response::CommitMessageDiff(document) => Ok(document),
                    other => Err(unexpected(other)),
                }
            }
        }
    }

    /// The repository's own artwork, already shrunk to thumbnails.
    pub async fn icon_candidates(&self) -> Result<Vec<Candidate>, String> {
        match &self.host {
            Host::Local => icon_scan::discover(self).await,
            Host::Remote { .. } => {
                let answer = self
                    .call(Request::IconCandidates { root: self.root_string() })
                    .await
                    .map_err(|error| error.message().to_owned())?;
                match answer {
                    Response::IconCandidates(candidates) => Ok(candidates),
                    Response::Err { message, .. } => Err(message),
                    other => Err(unexpected(other).message().to_owned()),
                }
            }
        }
    }

    /// Whether a Claude Code session is listening for this repository, on the
    /// machine the repository is on.
    pub async fn claude_status(&self) -> ClaudeChannelStatus {
        match &self.host {
            Host::Local => claude_channel::status(&self.root).await,
            Host::Remote { .. } => {
                match self.call(Request::ClaudeStatus { root: self.root_string() }).await {
                    Ok(Response::ClaudeStatus(status)) => status,
                    // A host that cannot be reached has no session for us, and
                    // a status indicator is not the place to raise that.
                    _ => ClaudeChannelStatus {
                        connected: false,
                        sessions: 0,
                    },
                }
            }
        }
    }

    /// Hands a message to the Claude Code session for this repository.
    pub async fn claude_send(&self, message: &str) -> Result<String, AppError> {
        match &self.host {
            Host::Local => claude_channel::send(&self.root, message).await,
            Host::Remote { .. } => {
                let request = Request::ClaudeSend {
                    root: self.root_string(),
                    message: message.to_owned(),
                };
                match self.call(request).await? {
                    Response::ClaudeSent(id) => Ok(id),
                    other => Err(unexpected(other)),
                }
            }
        }
    }

    /// Whether a Codex session has worked in this repository.
    pub async fn codex_status(&self) -> CodexChannelStatus {
        match &self.host {
            Host::Local => codex_channel::status(&self.root).await,
            Host::Remote { .. } => {
                let request = Request::CodexStatus {
                    root: self.root_string(),
                };
                match self.call(request).await {
                    Ok(Response::CodexStatus(status)) => status,
                    // A host that cannot be reached has no session to report,
                    // which is the same answer as a host with none.
                    _ => CodexChannelStatus {
                        connected: false,
                        sessions: 0,
                        delivering: false,
                    },
                }
            }
        }
    }

    /// Queues a message for the Codex session working in this repository.
    ///
    /// Unlike the Claude bridge this does not need the session to be up: Codex
    /// keeps the message until that thread next runs.
    pub async fn codex_send(&self, message: &str) -> Result<String, AppError> {
        match &self.host {
            Host::Local => codex_channel::send(&self.root, message).await,
            Host::Remote { .. } => {
                let request = Request::CodexSend {
                    root: self.root_string(),
                    message: message.to_owned(),
                };
                match self.call(request).await? {
                    Response::CodexSent(id) => Ok(id),
                    other => Err(unexpected(other)),
                }
            }
        }
    }

    /// Writes a pasted image where the Claude session for this repository can
    /// open it, and answers with the path it landed at.
    ///
    /// The path is in the host's own filesystem, which is the point: the
    /// message that follows names it, and the session that reads that message
    /// is a process on the same machine as the file.
    pub async fn write_attachment(&self, bytes: &[u8]) -> Result<String, AppError> {
        match &self.host {
            Host::Local => attachment::write(self, bytes).await,
            Host::Remote { .. } => {
                let request = Request::WriteAttachment {
                    root: self.root_string(),
                    bytes: bytes.to_vec(),
                };
                match self.call(request).await? {
                    Response::Attachment(path) => Ok(path),
                    other => Err(unexpected(other)),
                }
            }
        }
    }

    /// The repository root at or above `path`, on whichever machine this is.
    ///
    /// `Ok(None)` means there is no `.git` there, which is an answer the picker
    /// shows rather than a failure.
    pub async fn resolve_repository(&self, path: &str) -> Result<Option<String>, AppError> {
        match &self.host {
            Host::Local => Ok(local_repository_root(path)),
            Host::Remote { .. } => {
                let request = Request::ResolveRepository {
                    path: path.to_owned(),
                };
                match self.call(request).await? {
                    Response::Repository(root) => Ok(root),
                    other => Err(unexpected(other)),
                }
            }
        }
    }

    /// Starts or stops the host watching this repository. Local repositories
    /// are watched by the app itself, so this is a no-op for them.
    pub async fn set_watched(&self, watched: bool) -> Result<(), AppError> {
        let Host::Remote { .. } = &self.host else {
            return Ok(());
        };
        let root = self.root_string();
        let request = if watched {
            Request::Watch { root }
        } else {
            Request::Unwatch { root }
        };
        match self.call(request).await? {
            Response::Unit => Ok(()),
            other => Err(unexpected(other)),
        }
    }

    /// Reads one blob. An empty revision means the index (`git show :path`).
    pub async fn show_file(&self, revision: &str, file_path: &str) -> Result<String, AppError> {
        let spec = if revision.is_empty() {
            format!(":{file_path}")
        } else {
            format!("{revision}:{file_path}")
        };
        self.git(&["show", &spec]).await
    }

    /// Reads a repository-relative file, refusing anything past `max_bytes`.
    ///
    /// The bound is not optional and not a remote-only concern: a caller that
    /// cannot say how much it is willing to receive cannot be given a sensible
    /// answer over a network, and locally it is the difference between an error
    /// and a renderer handed half a gigabyte it will never draw.
    pub async fn read_file(&self, relative: &Path, max_bytes: u64) -> Result<Vec<u8>, AppError> {
        if self.is_remote() {
            let request = Request::ReadFile {
                root: self.root_string(),
                path: relative.to_string_lossy().into_owned(),
                max_bytes,
            };
            return match self.call(request).await? {
                Response::Bytes(bytes) => Ok(bytes),
                other => Err(unexpected(other)),
            };
        }
        match self.host {
            Host::Remote { .. } => unreachable!("handled above"),
            Host::Local => {
                let absolute = self.root.join(relative);
                let meta = tokio::fs::symlink_metadata(&absolute).await.map_err(|error| {
                    AppError::WorkTree(format!("failed to read {}: {error}", relative.display()))
                })?;
                if meta.len() > max_bytes {
                    return Err(AppError::WorkTree(format!(
                        "{} is {} bytes; the limit is {max_bytes}",
                        relative.display(),
                        meta.len()
                    )));
                }
                tokio::fs::read(&absolute).await.map_err(|error| {
                    AppError::WorkTree(format!("failed to read {}: {error}", relative.display()))
                })
            }
        }
    }

    /// Stats a repository-relative path. `Ok(None)` is "no such path", which is
    /// an answer rather than a failure — the icon scanner asks about files that
    /// `git ls-files` listed and something may since have deleted.
    pub async fn metadata(&self, relative: &Path) -> Result<Option<FileMeta>, AppError> {
        if self.is_remote() {
            let request = Request::Metadata {
                root: self.root_string(),
                path: relative.to_string_lossy().into_owned(),
            };
            return match self.call(request).await? {
                Response::Metadata(meta) => Ok(meta),
                other => Err(unexpected(other)),
            };
        }
        match self.host {
            Host::Remote { .. } => unreachable!("handled above"),
            Host::Local => {
                // `symlink_metadata`, not `metadata`: a symlink out of the
                // repository is not a file this app will read.
                match tokio::fs::symlink_metadata(self.root.join(relative)).await {
                    Ok(meta) => Ok(Some(FileMeta {
                        is_file: meta.file_type().is_file(),
                        len: meta.len(),
                        modified_millis: meta
                            .modified()
                            .ok()
                            .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|since| since.as_millis())
                            .unwrap_or(0),
                    })),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(error) => Err(AppError::WorkTree(format!(
                        "failed to stat {}: {error}",
                        relative.display()
                    ))),
                }
            }
        }
    }
}

/// The walk up to a `.git`, for a path on this machine. The agent does exactly
/// this for a path on its own.
fn local_repository_root(path: &str) -> Option<String> {
    let mut candidate = PathBuf::from(path);
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

/// An answer that does not match its question. Only reachable if the agent and
/// the app disagree about the protocol, which the version match is there to
/// prevent — so this is a bug, reported rather than papered over.
fn unexpected(response: Response) -> AppError {
    match response {
        Response::Err { tag, message } => Repository::rebuild(&tag, message),
        other => AppError::Ssh(format!(
            "the agent answered a request with {other:?}, which does not match it"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::Repository;
    use std::path::Path;

    fn sandbox() -> (tempfile::TempDir, Repository) {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let repo = Repository::local(dir.path().to_path_buf());
        (dir, repo)
    }

    #[tokio::test]
    async fn a_missing_path_stats_as_absent_rather_than_failing() {
        let (_dir, repo) = sandbox();

        let absent = repo.metadata(Path::new("nothing.txt")).await.expect("stat");

        assert_eq!(absent, None);
    }

    #[tokio::test]
    async fn a_directory_is_reported_as_not_a_file() {
        let (dir, repo) = sandbox();
        std::fs::create_dir(dir.path().join("src")).expect("mkdir");

        let meta = repo
            .metadata(Path::new("src"))
            .await
            .expect("stat")
            .expect("present");

        assert!(!meta.is_file);
    }

    #[tokio::test]
    async fn reading_past_the_limit_is_refused_before_the_bytes_are_loaded() {
        let (dir, repo) = sandbox();
        std::fs::write(dir.path().join("big.bin"), vec![0u8; 4096]).expect("write");

        let refused = repo.read_file(Path::new("big.bin"), 1024).await;
        let allowed = repo.read_file(Path::new("big.bin"), 8192).await;

        assert!(refused.is_err(), "4096 bytes should not pass a 1024 limit");
        assert_eq!(allowed.expect("read").len(), 4096);
    }

    #[tokio::test]
    async fn a_file_exactly_at_the_limit_is_allowed() {
        let (dir, repo) = sandbox();
        std::fs::write(dir.path().join("edge.bin"), vec![7u8; 512]).expect("write");

        assert_eq!(
            repo.read_file(Path::new("edge.bin"), 512).await.expect("read").len(),
            512
        );
    }
}
