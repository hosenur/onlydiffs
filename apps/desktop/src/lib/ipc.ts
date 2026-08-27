import { invoke } from '@tauri-apps/api/core'
import type { InvokeArgs } from '@tauri-apps/api/core'
import type {
  AppTheme,
  ChangeStatus,
  Commit,
  CommandName,
  FullFileContents,
  IpcResult,
  ClaudeChannelStatus,
  Project,
  RepoDiff,
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
  readonly tag: string
  readonly operation: CommandName

  constructor(options: { message: string; tag: string; operation: CommandName }) {
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
  call(Command.getHistory, { request: { limit } })

export const getFileContents = (request: {
  path: string
  oldPath: string | null
  status: ChangeStatus
  staged: boolean
}): Promise<FullFileContents> => call(Command.getFileContents, { request })

export const stageFile = (request: {
  path: string
  oldPath: string | null
}): Promise<void> => call(Command.stageFile, { request })

export const generateCommitMessage = (): Promise<string> =>
  call(Command.generateCommitMessage)

/** One-way. Resolves with the channel's message id, not a reply. */
export const sendClaudeMessage = (message: string): Promise<string> =>
  call(Command.sendClaudeMessage, { request: { message } })

export const writeClipboardText = (text: string): Promise<void> =>
  call(Command.writeClipboardText, { text })

export const listProjects = (): Promise<Project[]> => call(Command.listProjects)

export const currentProject = (): Promise<Project | null> => call(Command.currentProject)

export const openProject = (path: string): Promise<Project> =>
  call(Command.openProject, { request: { path } })

export const forgetProject = (path: string): Promise<void> =>
  call(Command.forgetProject, { request: { path } })

export const listFiles = (): Promise<string[]> => call(Command.listFiles)

export const commitAll = (message: string): Promise<string> =>
  call(Command.commitAll, { request: { message } })

export const claudeStatus = (): Promise<ClaudeChannelStatus> => call(Command.claudeStatus)

/** Repaints the native window frame to match the in-app theme. */
export const setTheme = (theme: AppTheme): Promise<void> =>
  call(Command.setTheme, { request: { theme } })

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
