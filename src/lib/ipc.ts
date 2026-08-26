import { Cause, Data, Effect, Exit } from 'effect'
import type {
  OnlyDiffsApi,
  ChangeStatus,
  Commit,
  FullFileContents,
  IpcResult,
  Project,
  RepoDiff,
} from '@shared/contract'

/**
 * A failure that came back from the main process, or the bridge itself being
 * missing. `Data.TaggedError` extends `Error`, so callers that only do
 * `error instanceof Error ? error.message : …` keep working unchanged, while
 * anything that cares can match on `cause`.
 */
export class IpcError extends Data.TaggedError('IpcError')<{
  readonly message: string
  /** The main-process error tag, e.g. `GitError`, or `BridgeUnavailable`. */
  readonly cause: string
  readonly operation: string
}> {}

function bridge(): Effect.Effect<OnlyDiffsApi, IpcError> {
  const api = typeof window === 'undefined' ? undefined : window.onlydiffs
  return api === undefined
    ? Effect.fail(
        new IpcError({
          message: 'The OnlyDiffs bridge is unavailable — the preload script did not load.',
          cause: 'BridgeUnavailable',
          operation: 'bridge',
        })
      )
    : Effect.succeed(api)
}

/**
 * Turns one bridge call into an Effect. The main process answers with a result
 * value rather than a rejection, so the tag survives the process boundary; a
 * genuine rejection can still happen if the channel itself is gone, and is
 * folded into the same error.
 */
function call<A>(
  operation: string,
  invoke: (api: OnlyDiffsApi) => Promise<IpcResult<A>>
): Effect.Effect<A, IpcError> {
  return bridge().pipe(
    Effect.flatMap((api) =>
      Effect.tryPromise({
        try: () => invoke(api),
        catch: (error) =>
          new IpcError({
            message: error instanceof Error ? error.message : String(error),
            cause: 'ChannelError',
            operation,
          }),
      })
    ),
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

export const getDiff: Effect.Effect<RepoDiff, IpcError> = call('getDiff', (api) => api.getDiff())

export const getHistory = (limit?: number): Effect.Effect<Commit[], IpcError> =>
  call('getHistory', (api) => api.getHistory({ limit }))

export const getFileContents = (request: {
  path: string
  oldPath: string | null
  status: ChangeStatus
  staged: boolean
}): Effect.Effect<FullFileContents, IpcError> =>
  call('getFileContents', (api) => api.getFileContents(request))

export const stageFile = (request: {
  path: string
  oldPath: string | null
}): Effect.Effect<void, IpcError> => call('stageFile', (api) => api.stageFile(request))

export const generateCommitMessage: Effect.Effect<string, IpcError> = call(
  'generateCommitMessage',
  (api) => api.generateCommitMessage()
)

export const sendClaudeMessage = (message: string): Effect.Effect<string, IpcError> =>
  call('sendClaudeMessage', (api) => api.sendClaudeMessage({ message }))

export const writeClipboardText = (text: string): Effect.Effect<void, IpcError> =>
  call('writeClipboardText', (api) => api.writeClipboardText(text))

export const listProjects: Effect.Effect<Project[], IpcError> = call(
  'listProjects',
  (api) => api.listProjects()
)

export const currentProject: Effect.Effect<Project | null, IpcError> = call(
  'currentProject',
  (api) => api.currentProject()
)

export const openProject = (path: string): Effect.Effect<Project, IpcError> =>
  call('openProject', (api) => api.openProject({ path }))

export const forgetProject = (path: string): Effect.Effect<void, IpcError> =>
  call('forgetProject', (api) => api.forgetProject({ path }))

export const listFiles: Effect.Effect<string[], IpcError> = call(
  'listFiles',
  (api) => api.listFiles()
)

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
