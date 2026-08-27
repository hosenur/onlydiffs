import { invoke } from '@tauri-apps/api/core'
import type { InvokeArgs } from '@tauri-apps/api/core'
import { Cause, Data, Effect, Exit } from 'effect'
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
} from '@shared/contract'
import { Command } from '@shared/contract'

/**
 * A failure that came back from the backend, or the bridge itself being
 * missing. `Data.TaggedError` extends `Error`, so callers that only do
 * `error instanceof Error ? error.message : …` keep working unchanged, while
 * anything that cares can match on `cause`.
 */
export class IpcError extends Data.TaggedError('IpcError')<{
  readonly message: string
  /** The backend's error tag, e.g. `GitError`, or `BridgeUnavailable`. */
  readonly cause: string
  readonly operation: string
}> {}

/**
 * Turns one command into an Effect. The backend answers with a result value
 * rather than a rejection, so the tag survives the process boundary; a genuine
 * rejection can still happen if the command is missing or the payload fails to
 * deserialise, and is folded into the same error.
 */
function call<A>(operation: CommandName, args?: InvokeArgs): Effect.Effect<A, IpcError> {
  return Effect.tryPromise({
    try: () => invoke<IpcResult<A>>(operation, args),
    catch: (error) =>
      new IpcError({
        message: error instanceof Error ? error.message : String(error),
        cause: 'ChannelError',
        operation,
      }),
  }).pipe(
    Effect.flatMap((result) =>
      result.ok
        ? Effect.succeed(result.value)
        : Effect.fail(
            new IpcError({
              message: result.error.message,
              cause: result.error._tag,
              operation,
            })
          )
    )
  )
}

export const getDiff: Effect.Effect<RepoDiff, IpcError> = call(Command.getDiff)

export const getHistory = (limit?: number): Effect.Effect<Commit[], IpcError> =>
  call(Command.getHistory, { request: { limit } })

export const getFileContents = (request: {
  path: string
  oldPath: string | null
  status: ChangeStatus
  staged: boolean
}): Effect.Effect<FullFileContents, IpcError> => call(Command.getFileContents, { request })

export const stageFile = (request: {
  path: string
  oldPath: string | null
}): Effect.Effect<void, IpcError> => call(Command.stageFile, { request })

export const generateCommitMessage: Effect.Effect<string, IpcError> = call(
  Command.generateCommitMessage
)

/** One-way. Resolves with the channel's message id, not a reply. */
export const sendClaudeMessage = (message: string): Effect.Effect<string, IpcError> =>
  call(Command.sendClaudeMessage, { request: { message } })

export const writeClipboardText = (text: string): Effect.Effect<void, IpcError> =>
  call(Command.writeClipboardText, { text })

export const listProjects: Effect.Effect<Project[], IpcError> = call(Command.listProjects)

export const currentProject: Effect.Effect<Project | null, IpcError> = call(Command.currentProject)

export const openProject = (path: string): Effect.Effect<Project, IpcError> =>
  call(Command.openProject, { request: { path } })

export const forgetProject = (path: string): Effect.Effect<void, IpcError> =>
  call(Command.forgetProject, { request: { path } })

export const listFiles: Effect.Effect<string[], IpcError> = call(Command.listFiles)

export const commitAll = (message: string): Effect.Effect<string, IpcError> =>
  call(Command.commitAll, { request: { message } })

export const claudeStatus: Effect.Effect<ClaudeChannelStatus, IpcError> = call(Command.claudeStatus)

/** Repaints the native window frame to match the in-app theme. */
export const setTheme = (theme: AppTheme): Effect.Effect<void, IpcError> =>
  call(Command.setTheme, { request: { theme } })

/**
 * Runs an Effect for callers that live in promise-land — route loaders, event
 * handlers. Rejects with the `IpcError` itself rather than the `FiberFailure`
 * wrapper `Effect.runPromise` would throw, so `error.message` stays readable.
 */
export async function runIpc<A>(effect: Effect.Effect<A, IpcError>): Promise<A> {
  const exit = await Effect.runPromiseExit(effect)
  if (Exit.isSuccess(exit)) return exit.value
  throw Cause.squash(exit.cause)
}
