/**
 * The single source of truth for everything that crosses the Electron process
 * boundary: the domain types, the channel names, and the shape of the bridge
 * the preload script installs on `window`.
 *
 * Imported by all three processes, so it must stay free of Node, Electron, and
 * React imports.
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

/** A repository the app can open. */
export interface Project {
  /** Absolute path to the repository root. */
  path: string;
  /** Last path segment, for display. */
  name: string;
}

export interface OpenProjectRequest {
  /** Whatever the user typed: absolute, relative, or `~`-prefixed. */
  path: string;
}

export interface ForgetProjectRequest {
  path: string;
}

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

export const IpcChannel = {
  getDiff: "onlydiffs:get-diff",
  getFileContents: "onlydiffs:get-file-contents",
  getHistory: "onlydiffs:get-history",
  stageFile: "onlydiffs:stage-file",
  generateCommitMessage: "onlydiffs:generate-commit-message",
  sendClaudeMessage: "onlydiffs:send-claude-message",
  writeClipboardText: "onlydiffs:write-clipboard-text",
  listProjects: "onlydiffs:list-projects",
  openProject: "onlydiffs:open-project",
  currentProject: "onlydiffs:current-project",
  forgetProject: "onlydiffs:forget-project",
} as const;

/**
 * A failed Effect, flattened for structured cloning. Rejecting the underlying
 * `ipcRenderer.invoke` promise would stringify the cause and prefix it with
 * "Error invoking remote method …", losing the tag; carrying the failure as a
 * value keeps both halves intact.
 */
export interface IpcFailure {
  readonly _tag: string;
  readonly message: string;
}

export type IpcResult<A> =
  | { readonly ok: true; readonly value: A }
  | { readonly ok: false; readonly error: IpcFailure };

/** Installed on `window.onlydiffs` by the preload script. */
export interface OnlyDiffsApi {
  getDiff(): Promise<IpcResult<RepoDiff>>;
  getFileContents(
    request: GetFileContentsRequest,
  ): Promise<IpcResult<FullFileContents>>;
  getHistory(request: GetHistoryRequest): Promise<IpcResult<Commit[]>>;
  stageFile(request: StageFileRequest): Promise<IpcResult<void>>;
  generateCommitMessage(): Promise<IpcResult<string>>;
  sendClaudeMessage(
    request: SendClaudeMessageRequest,
  ): Promise<IpcResult<string>>;
  writeClipboardText(text: string): Promise<IpcResult<void>>;
  /** Recently opened repositories, newest first, missing ones filtered out. */
  listProjects(): Promise<IpcResult<Project[]>>;
  /** Validates the path, makes it current, and records it in the history. */
  openProject(request: OpenProjectRequest): Promise<IpcResult<Project>>;
  /** The repository currently open, or null if the app is on the landing page. */
  currentProject(): Promise<IpcResult<Project | null>>;
  forgetProject(request: ForgetProjectRequest): Promise<IpcResult<void>>;
}
