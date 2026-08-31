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
  /** Absolute path to the repository root. */
  path: string;
  /** Last path segment, for display. */
  name: string;
  /** Resolved in the background; null keeps the cube fallback. */
  icon: ProjectIcon | null;
}

/**
 * The request payloads, one per command that takes arguments. Each is the
 * mirror of a struct in `src-tauri/src/commands.rs`, and every one of them
 * travels under a `request` key — except `writeClipboardText`, whose command
 * takes a bare `text: String`. That one exception is why these are types
 * rather than a single generic envelope.
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

/** Whether a Claude Code session is listening for the open repository. */
export interface ClaudeChannelStatus {
  connected: boolean;
  /** How many live channels are registered; more than one is possible. */
  sessions: number;
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
  | "ClipboardError"
  | "NoProjectOpenError"
  | "InvalidProjectError"
  | "UpdaterError";

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
