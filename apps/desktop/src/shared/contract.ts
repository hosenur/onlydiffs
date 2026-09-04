/**
 * The domain types that cross the process boundary, and the names of the
 * commands that carry them.
 *
 * The mirror of `src-tauri/src/contract.rs`. The Rust side serialises in
 * camelCase, so these apply to the payloads unchanged; when a field is added
 * there, add it here too.
 */

export type ChangeStatus =
  | "added"
  | "modified"
  | "deleted"
  | "renamed"
  | "untracked";

export interface FileChange {
  /** Unique per row — a path staged *and* modified again yields two rows. */
  id: string;
  path: string;
  oldPath: string | null;
  status: ChangeStatus;
  /** true = index vs HEAD, false = working tree vs index. */
  staged: boolean;
  additions: number;
  deletions: number;
  binary: boolean;
  /** Set when this file's patch couldn't be produced. */
  error: string | null;
}

export interface FullFileContents {
  oldContents: string | null;
  newContents: string | null;
}

export interface RepoDiff {
  repoPath: string;
  branch: string;
  head: string;
  files: FileChange[];
}

export interface Commit {
  hash: string;
  shortHash: string;
  subject: string;
  author: string;
  authorEmail: string;
  relativeDate: string;
  date: string;
  /** More than one parent — i.e. a merge. */
  isMerge: boolean;
  /** Branch/tag decorations, e.g. "HEAD -> dev, origin/dev". */
  refs: string;
}

export interface ProjectIcon {
  /** Repository-relative path chosen as the icon source. */
  sourcePath: string;
  /** Small cached image, ready for an <img> source. */
  dataUrl: string;
}

/** A repository the app can open. */
export interface Project {
  /**
   * How the project is written on screen and identified in the recents list:
   * an absolute path, or `host:/path` for one on another machine.
   */
  path: string;
  /** Last path segment, for display. */
  name: string;
  /** The SSH alias this project is on, or null for this machine. */
  host: string | null;
  /**
   * The repository root as *its own machine* writes it. `path` is for showing
   * and identifying; this is the one to send back to that machine.
   */
  root: string;
  /** Resolved in the background; null keeps the cube fallback. */
  icon: ProjectIcon | null;
}

/** Whether a host is reachable right now. */
export type HostConnectionState = "connected" | "disconnected";

/** A host with a live connection. */
export interface ConnectedHost {
  /** What the user typed, which is how it is labelled everywhere. */
  alias: string;
  hostname: string;
  user: string | null;
  port: number | null;
  state: HostConnectionState;
  /** From the probe, so the user can see what they connected to. */
  gitVersion: string | null;
  /** e.g. `Linux x86_64`. */
  platform: string | null;
  agentVersion: string | null;
}

/**
 * A question ssh is blocked on, on its way to the window. Arrives as the
 * `ssh:prompt` event and is answered with `answerSshPrompt`.
 */
export interface SshPromptRequest {
  id: number;
  /** ssh's own words, e.g. `me@host's password:`. */
  text: string;
  /** Whether to mask the field. A yes/no question is not a passphrase. */
  isSecret: boolean;
}

/** A host key nobody has approved yet, with the fingerprint to compare. */
export interface UnknownHostKeyPrompt {
  alias: string;
  hostname: string;
  port: number | null;
  keyType: string;
  /** `SHA256:…`, which is what the host's operator can confirm. */
  fingerprint: string;
}

/**
 * The request payloads, one per command that takes arguments. Each is the
 * mirror of a struct in `src-tauri/src/commands.rs`, and every one of them
 * travels under a `request` key — except two. `writeClipboardText` takes a
 * bare `text: String`, and `attachImage` takes no JSON at all: its payload is
 * an `ArrayBuffer` sent as a raw body, which is why it has no type here. Those
 * exceptions are why these are types rather than a single generic envelope.
 */
export interface GetFileContentsRequest {
  path: string;
  oldPath: string | null;
  status: ChangeStatus;
  staged: boolean;
}

export interface StageFileRequest {
  path: string;
  oldPath: string | null;
}

export interface GetHistoryRequest {
  limit?: number;
}

export interface SendClaudeMessageRequest {
  message: string;
}

export interface SendCodexMessageRequest {
  message: string;
}

export interface CommitAllRequest {
  message: string;
}

export interface OpenProjectRequest {
  path: string;
}

export interface ForgetProjectRequest {
  path: string;
}

export interface SetThemeRequest {
  theme: AppTheme;
}

export interface HostRequest {
  /** The SSH destination as the user typed it. */
  alias: string;
}

export interface AddSshHostRequest {
  /**
   * The command the user already uses — `ssh user@example -p 2222` — or just a
   * host. The options in it are kept and replayed on every later connection.
   */
  command: string;
}

/** A remembered destination and the options it is dialled with. */
export interface SshHostEntry {
  alias: string;
  args: string[];
}

export interface OpenRemoteProjectRequest {
  alias: string;
  /** A path on the host; resolved to a repository root there. */
  path: string;
}

export interface AnswerSshPromptRequest {
  id: number;
  /** `null` cancels, which ssh reads as a refusal rather than a retry. */
  answer: string | null;
}

export interface SetGroqApiKeyRequest {
  /**
   * `null` clears the stored key, handing the app back to `GROQ_API_KEY` where
   * that is set.
   */
  key: string | null;
}

/**
 * The largest image that can be pasted into the composer. The mirror of
 * `MAX_IMAGE_BYTES` in `src-tauri/core/src/services/attachment.rs`, which is
 * the side that enforces it; this copy is what lets an oversized paste be
 * refused before megabytes are copied to be rejected.
 */
export const MAX_IMAGE_BYTES = 10 * 1024 * 1024;

/** Whether a Claude Code session is listening for the open repository. */
export interface ClaudeChannelStatus {
  connected: boolean;
  /** How many live channels are registered; more than one is possible. */
  sessions: number;
}

/**
 * Whether a Codex session has worked in the open repository.
 *
 * A softer claim than the Claude one. Codex is reached through a durable
 * per-thread queue rather than a live listener, so this says a thread exists
 * that a message can be queued against — not that anything is running. A
 * message sent to a closed session is delivered when it next opens.
 */
export interface CodexChannelStatus {
  connected: boolean;
  /** How many threads have worked in this repository recently. */
  sessions: number;
  /**
   * Whether Codex's shared daemon is up to deliver what is queued. A message
   * sent while this is false is kept rather than lost, but nothing acts on it
   * until the daemon runs again.
   */
  delivering: boolean;
}

/** Whether a newer release is waiting to be installed. */
export interface UpdateStatus {
  available: boolean;
  /** The version on offer, e.g. `0.1.2`. `null` when nothing is. */
  version: string | null;
  /** The release notes, when the manifest carries any. */
  notes: string | null;
}

/** The renderer's theme. `system` hands the window back to the OS setting. */
export type AppTheme = "light" | "dark" | "system";

/**
 * Where the Groq key in use came from. Worth naming rather than reducing to a
 * boolean: someone whose key arrives from their shell should not be told they
 * have none configured, and someone who saved one should be able to see that
 * it is the one winning.
 */
export type GroqKeySource = "config" | "environment" | "none";

/** What the settings page renders. */
export interface AppSettings {
  /**
   * A masked form of the key in use, e.g. `gsk_…WxYz`. The key itself never
   * crosses to the renderer; `null` here means there is no key at all.
   */
  groqApiKeyHint: string | null;
  groqKeySource: GroqKeySource;
  /** Absolute path of the file the settings live in, so the page can name it. */
  configPath: string;
  /** SSH destinations the user has added, in the order they added them. */
  sshHosts: SshHostEntry[];
}

/**
 * Every command the backend exposes. These are the `#[tauri::command]` function
 * names in `src-tauri/src/commands.rs`; nothing else is reachable.
 */
export const Command = {
  getDiff: "get_diff",
  getFileContents: "get_file_contents",
  getHistory: "get_history",
  stageFile: "stage_file",
  generateCommitMessage: "generate_commit_message",
  sendClaudeMessage: "send_claude_message",
  sendCodexMessage: "send_codex_message",
  codexStatus: "codex_status",
  attachImage: "attach_image",
  writeClipboardText: "write_clipboard_text",
  listProjects: "list_projects",
  openProject: "open_project",
  currentProject: "current_project",
  forgetProject: "forget_project",
  listFiles: "list_files",
  commitAll: "commit_all",
  claudeStatus: "claude_status",
  setTheme: "set_theme",
  checkForUpdate: "check_for_update",
  installUpdate: "install_update",
  getSettings: "get_settings",
  setGroqApiKey: "set_groq_api_key",
  listHosts: "list_hosts",
  connectHost: "connect_host",
  disconnectHost: "disconnect_host",
  inspectHostKey: "inspect_host_key",
  trustHostKey: "trust_host_key",
  answerSshPrompt: "answer_ssh_prompt",
  openRemoteProject: "open_remote_project",
  addSshHost: "add_ssh_host",
  forgetSshHost: "forget_ssh_host",
} as const;

export type CommandName = (typeof Command)[keyof typeof Command];

/**
 * The tag a backend failure carries: the return values of `AppError::tag()` in
 * `src-tauri/src/error.rs`. A test there is exhaustive over `AppError`, so a
 * new variant stops the backend compiling until its tag is added to both.
 */
export type BackendErrorTag =
  | "GitError"
  | "WorkTreeError"
  | "InvalidPathError"
  | "CommitMessageError"
  | "ClaudeChannelError"
  /** A message could not be queued for a Codex session. */
  | "CodexChannelError"
  /** A pasted image could not be read or written down. */
  | "AttachmentError"
  | "ClipboardError"
  | "NoProjectOpenError"
  | "InvalidProjectError"
  | "UpdaterError"
  | "SettingsError"
  | "SshError"
  /** The host has no key in `known_hosts` yet — a question, not a failure. */
  | "SshUnknownHostError"
  /** An established connection dropped; the app can offer to reconnect. */
  | "SshDisconnectedError";

/**
 * What a caller can find on a thrown `IpcError`. `ChannelError` is the one tag
 * the backend never sends: it means the call did not arrive, so there was
 * nothing there to tag it.
 */
export type IpcErrorTag = BackendErrorTag | "ChannelError";

/**
 * A failed command, flattened for the wire. Returning an `Err` from the command
 * would hand Tauri the failure to stringify, which flattens the variant into
 * prose and loses the tag; carrying it inside a successful response keeps both
 * halves intact.
 */
export interface IpcFailure {
  readonly _tag: BackendErrorTag;
  readonly message: string;
}

export type IpcResult<A> =
  | { readonly ok: true; readonly value: A }
  | { readonly ok: false; readonly error: IpcFailure };
