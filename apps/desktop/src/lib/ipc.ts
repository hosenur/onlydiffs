import { invoke } from '@tauri-apps/api/core'
import type { InvokeArgs } from '@tauri-apps/api/core'
import type {
  AddSshHostRequest,
  AnswerSshPromptRequest,
  AppSettings,
  AppTheme,
  ConnectedHost,
  ClaudeChannelStatus,
  CodexChannelStatus,
  Commit,
  CommandName,
  CommitAllRequest,
  ForgetProjectRequest,
  FullFileContents,
  GetFileContentsRequest,
  GetHistoryRequest,
  HostRequest,
  IpcErrorTag,
  IpcResult,
  OpenProjectRequest,
  OpenRemoteProjectRequest,
  Project,
  RepoDiff,
  SendClaudeMessageRequest,
  SendCodexMessageRequest,
  SetGroqApiKeyRequest,
  SetThemeRequest,
  SshHostEntry,
  StageFileRequest,
  UnknownHostKeyPrompt,
  UpdateStatus,
} from '@shared/contract'
import { Command } from '@shared/contract'

/**
 * A failure that came back from the backend, or the bridge itself being
 * missing. Callers that only do `error instanceof Error ? error.message : …`
 * read it as any other Error, while anything that cares can match on `tag`.
 */
export class IpcError extends Error {
  /** The backend's error tag, e.g. `GitError`, or `ChannelError` locally. */
  readonly tag: IpcErrorTag
  readonly operation: CommandName

  constructor(options: { message: string; tag: IpcErrorTag; operation: CommandName }) {
    super(options.message)
    this.name = 'IpcError'
    this.tag = options.tag
    this.operation = options.operation
  }
}

/**
 * Runs one command. The backend answers with a result value rather than a
 * rejection, so the tag survives the process boundary; a genuine rejection can
 * still happen if the command is missing or the payload fails to deserialise,
 * and is folded into the same error.
 *
 * Every export below is a function rather than a value, so nothing fires at
 * import time — the call is made where it is awaited.
 */
async function call<A>(operation: CommandName, args?: InvokeArgs): Promise<A> {
  let result: IpcResult<A>
  try {
    result = await invoke<IpcResult<A>>(operation, args)
  } catch (error) {
    throw new IpcError({
      message: error instanceof Error ? error.message : String(error),
      tag: 'ChannelError',
      operation,
    })
  }
  if (!result.ok) {
    throw new IpcError({
      message: result.error.message,
      tag: result.error._tag,
      operation,
    })
  }
  return result.value
}

export const getDiff = (): Promise<RepoDiff> => call(Command.getDiff)

export const getHistory = (limit?: number): Promise<Commit[]> =>
  call(Command.getHistory, { request: { limit } satisfies GetHistoryRequest })

export const getFileContents = (
  request: GetFileContentsRequest
): Promise<FullFileContents> => call(Command.getFileContents, { request })

export const stageFile = (request: StageFileRequest): Promise<void> =>
  call(Command.stageFile, { request })

export const generateCommitMessage = (): Promise<string> =>
  call(Command.generateCommitMessage)

/** One-way. Resolves with the channel's message id, not a reply. */
export const sendClaudeMessage = (message: string): Promise<string> =>
  call(Command.sendClaudeMessage, { request: { message } satisfies SendClaudeMessageRequest })

/**
 * Queues a message for the Codex session working in the open repository.
 *
 * The Codex counterpart to `sendClaudeMessage`, with one difference worth
 * knowing at the call site: this one succeeds when nothing is running. Codex
 * keeps the message until that thread next takes a turn, so a send is a
 * promise of delivery rather than a delivery.
 */
export const sendCodexMessage = (message: string): Promise<string> =>
  call(Command.sendCodexMessage, { request: { message } satisfies SendCodexMessageRequest })

export const codexStatus = (): Promise<CodexChannelStatus> => call(Command.codexStatus)

/**
 * Writes a pasted image where the Claude session for the open repository can
 * open it, and resolves with the path it landed at — a path on *that*
 * repository's machine, which is what makes this work for a project on a host.
 *
 * The bytes go over as a raw body rather than in a JSON envelope: a screenshot
 * is megabytes, and JSON has no way to spell them that is not several times
 * their size. See the note on the request types in the contract.
 */
export const attachImage = (bytes: ArrayBuffer): Promise<string> =>
  call(Command.attachImage, bytes)

/** The other command with no `request` envelope — `write_clipboard_text` takes
 *  the string itself. */
export const writeClipboardText = (text: string): Promise<void> =>
  call(Command.writeClipboardText, { text })

export const listProjects = (): Promise<Project[]> => call(Command.listProjects)

export const currentProject = (): Promise<Project | null> => call(Command.currentProject)

export const openProject = (path: string): Promise<Project> =>
  call(Command.openProject, { request: { path } satisfies OpenProjectRequest })

export const forgetProject = (path: string): Promise<void> =>
  call(Command.forgetProject, { request: { path } satisfies ForgetProjectRequest })

export const listFiles = (): Promise<string[]> => call(Command.listFiles)

export const commitAll = (message: string): Promise<string> =>
  call(Command.commitAll, { request: { message } satisfies CommitAllRequest })

export const claudeStatus = (): Promise<ClaudeChannelStatus> => call(Command.claudeStatus)

/** Repaints the native window frame to match the in-app theme. */
export const setTheme = (theme: AppTheme): Promise<void> =>
  call(Command.setTheme, { request: { theme } satisfies SetThemeRequest })

/**
 * Answers `available: false` rather than failing when there is nothing to
 * install, so there is one shape to read either way. Always answers that way in
 * a dev build, where the running tree is ahead of the last release.
 */
export const checkForUpdate = (): Promise<UpdateStatus> => call(Command.checkForUpdate)

/**
 * Installs what the last check found and relaunches into it — so this resolves
 * only if something went wrong on the way.
 */
export const installUpdate = (): Promise<void> => call(Command.installUpdate)

/**
 * The settings page's whole read. Resolving the key can reach for the login
 * shell, so this is a call rather than something the app holds from startup.
 */
export const getSettings = (): Promise<AppSettings> => call(Command.getSettings)

/**
 * Saves a Groq key, or clears the stored one with `null`. Answers with the
 * settings as they now stand — including which source ended up winning, which
 * is not always the one just written.
 */
export const setGroqApiKey = (key: string | null): Promise<AppSettings> =>
  call(Command.setGroqApiKey, { request: { key } satisfies SetGroqApiKeyRequest })

/** Every host with a live connection. */
export const listHosts = (): Promise<ConnectedHost[]> => call(Command.listHosts)

/**
 * Opens a connection, authenticating if it has to.
 *
 * Rejects with `SshUnknownHostError` when the host has no key in `known_hosts`
 * yet. That is a question rather than a failure: answer it with
 * `inspectHostKey`, show the fingerprint, and call `trustHostKey` if the user
 * recognises it.
 */
export const connectHost = (alias: string): Promise<ConnectedHost> =>
  call(Command.connectHost, { request: { alias } satisfies HostRequest })

export const disconnectHost = (alias: string): Promise<void> =>
  call(Command.disconnectHost, { request: { alias } satisfies HostRequest })

/** Fetches an unknown host's key so its fingerprint can be shown. */
export const inspectHostKey = (alias: string): Promise<UnknownHostKeyPrompt> =>
  call(Command.inspectHostKey, { request: { alias } satisfies HostRequest })

/** Records an approved key in the user's own `known_hosts`. */
export const trustHostKey = (alias: string): Promise<void> =>
  call(Command.trustHostKey, { request: { alias } satisfies HostRequest })

/** Answers something ssh asked. `null` cancels. */
export const answerSshPrompt = (id: number, answer: string | null): Promise<void> =>
  call(Command.answerSshPrompt, { request: { id, answer } satisfies AnswerSshPromptRequest })

/** Opens a repository on a connected host. The path is resolved there. */
export const openRemoteProject = (alias: string, path: string): Promise<Project> =>
  call(Command.openRemoteProject, {
    request: { alias, path } satisfies OpenRemoteProjectRequest,
  })

/**
 * Remembers a host from the command the user already uses, and answers with
 * what it made of it. Adding is not connecting.
 */
export const addSshHost = (command: string): Promise<SshHostEntry> =>
  call(Command.addSshHost, { request: { command } satisfies AddSshHostRequest })

export const forgetSshHost = (alias: string): Promise<AppSettings> =>
  call(Command.forgetSshHost, { request: { alias } satisfies HostRequest })
