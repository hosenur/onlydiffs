//! The domain types that cross the process boundary.
//!
//! The mirror of `src/shared/contract.ts`. Field names are serialised in
//! camelCase so the renderer's types apply unchanged; when a field is added
//! here, add it there too.
//!
//! Everything a repository produces derives `Deserialize` as well as
//! `Serialize`. It is not only the renderer that reads these now: a repository
//! on another machine answers in exactly these shapes, and the agent that
//! collected them is the same code that would have collected them locally.

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullFileContents {
    pub old_contents: Option<String>,
    pub new_contents: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoDiff {
    pub repo_path: String,
    pub branch: String,
    pub head: String,
    pub files: Vec<FileChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIcon {
    /// Repository-relative path chosen as the icon source.
    pub source_path: String,
    /// A small cached image that the renderer can use without filesystem access.
    pub data_url: String,
}

/// A repository the app can open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// How the project is written on screen and identified in the recents
    /// list: an absolute path, or `host:/path` for one on another machine.
    pub path: String,
    /// Last path segment, for display.
    pub name: String,
    /// The SSH alias this project is on, or absent for this machine.
    #[serde(default)]
    pub host: Option<String>,
    /// The repository root as *its own machine* writes it. `path` is for
    /// showing and identifying; this is the one to send to that machine.
    pub root: String,
    /// Resolved in the background; absent until a suitable image is found.
    pub icon: Option<ProjectIcon>,
}

/// Whether a Claude Code session for the open repository can be sent to.
///
/// `sessions` counts every live channel, and `unregistered` the ones Claude
/// Code is ignoring because the session was started without the channel flag.
/// `connected` is the stricter claim: at least one session would act on a
/// message now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeChannelStatus {
    pub connected: bool,
    /// How many live channels there are; more than one is possible.
    pub sessions: usize,
    /// How many of those Claude Code did not register, so a message to them
    /// would be dropped silently.
    pub unregistered: usize,
}

/// Whether a Codex session for the open repository can be sent to.
///
/// `sessions` counts every Codex process working in the repository, attached to
/// the shared daemon or not, so the bar can say "running but not connected"
/// about a session the user can see. `connected` is the stricter claim: the
/// daemon holds a thread for this repository and a message would reach it now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexChannelStatus {
    pub connected: bool,
    /// How many Codex sessions are running in this repository.
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

/// Where the Groq key in use came from. Worth naming rather than reducing to a
/// boolean: someone whose key arrives from their shell should not be told they
/// have none configured, and someone who saved one should be able to see that
/// it is the one winning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GroqKeySource {
    /// Saved in `config.json` from the settings page.
    Config,
    /// `GROQ_API_KEY`, from the process environment or the login shell.
    Environment,
    /// Nothing to use — the Groq features stay off.
    None,
}

/// What the settings page renders.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// A masked form of the key in use, e.g. `gsk_…WxYz`. The key itself never
    /// crosses to the renderer; `None` here means there is no key at all.
    pub groq_api_key_hint: Option<String>,
    pub groq_key_source: GroqKeySource,
    /// Absolute path of the file the settings live in, so the page can name it.
    pub config_path: String,
    /// SSH destinations the user has added, in the order they added them.
    pub ssh_hosts: Vec<SshHostEntry>,
}

/// Where a project lives. A path on this machine, or a path on a host.
///
/// Kept as a pair rather than a single string: a remote root is a path *on the
/// host*, and flattening it into `host:/path` would mean parsing it back apart
/// every time it is used, on a value that can legitimately contain a colon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLocation {
    /// The SSH alias the project is on, or `None` for this machine.
    #[serde(default)]
    pub host: Option<String>,
    /// The repository root, in the path style of whichever machine that is.
    pub path: String,
}

impl ProjectLocation {
    pub fn local(path: impl Into<String>) -> Self {
        Self {
            host: None,
            path: path.into(),
        }
    }

    pub fn is_remote(&self) -> bool {
        self.host.is_some()
    }

    /// How the location is written on screen and in the recents file.
    pub fn display(&self) -> String {
        match &self.host {
            Some(host) => format!("{host}:{}", self.path),
            None => self.path.clone(),
        }
    }
}

/// Whether a host is reachable right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HostConnectionState {
    Connected,
    Disconnected,
}

/// A remembered SSH destination, and the options it is dialled with.
///
/// The options come from the command the user pasted, and are replayed on every
/// later connection. Storing them rather than asking once is the difference
/// between a host on a non-standard port working today and working next week.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshHostEntry {
    /// The destination as ssh reads it: `build-box`, `me@10.0.0.4`.
    pub alias: String,
    /// Everything else from the command, in order — `-p 2222`, `-i …`, `-J …`.
    #[serde(default)]
    pub args: Vec<String>,
}

impl SshHostEntry {
    /// The command back, as the user would type it. Shown when editing, so
    /// what they see is what they gave.
    pub fn command(&self) -> String {
        let mut parts = vec!["ssh".to_owned(), self.alias.clone()];
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }
}

/// A host as the settings page and the picker show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedHost {
    /// What the user typed, which is how it is labelled everywhere.
    pub alias: String,
    pub hostname: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub state: HostConnectionState,
    /// From the probe, so the user can see what they connected to.
    pub git_version: Option<String>,
    /// e.g. `Linux x86_64`.
    pub platform: Option<String>,
    pub agent_version: Option<String>,
    /// Whether the Claude channel was registered with Claude Code on the host
    /// when it was connected. `None` for a host that is not connected.
    pub channel_registered: Option<bool>,
}

/// A question ssh is blocked on, on its way to the window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshPromptRequest {
    pub id: u64,
    /// ssh's own words, e.g. `me@host's password:`.
    pub text: String,
    /// Whether to mask the field. A yes/no question is not a passphrase.
    pub is_secret: bool,
}

/// A host key nobody has approved yet, with the fingerprint to compare.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnknownHostKeyPrompt {
    pub alias: String,
    pub hostname: String,
    pub port: Option<u16>,
    pub key_type: String,
    /// `SHA256:…`, which is what the host's operator can confirm.
    pub fingerprint: String,
}
