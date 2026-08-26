import { clipboard, ipcMain } from "electron";
import { Cause, Effect, Exit, Schema } from "effect";
import type { IpcResult } from "../shared/contract";
import { IpcChannel } from "../shared/contract";
import { ClipboardError, toIpcFailure } from "./errors";
import type { AppServices } from "./runtime";
import { runtime } from "./runtime";
import { ClaudeChannel } from "./services/claude-channel";
import { CommitMessage } from "./services/commit-message";
import { Diff } from "./services/diff";
import { History } from "./services/history";
import { Workspace } from "./services/workspace";

const nullableString = Schema.optionalWith(Schema.NullOr(Schema.String), {
  default: () => null,
});

const ChangeStatus = Schema.Literal(
  "added",
  "modified",
  "deleted",
  "renamed",
  "untracked",
);

const GetFileContentsRequest = Schema.Struct({
  path: Schema.String,
  oldPath: nullableString,
  status: ChangeStatus,
  staged: Schema.Boolean,
});

const StageFileRequest = Schema.Struct({
  path: Schema.String,
  oldPath: nullableString,
});

const GetHistoryRequest = Schema.Struct({
  limit: Schema.optional(Schema.Number),
});

const SendClaudeMessageRequest = Schema.Struct({ message: Schema.String });

const ClipboardTextRequest = Schema.String;

const OpenProjectRequest = Schema.Struct({ path: Schema.String });
const ForgetProjectRequest = Schema.Struct({ path: Schema.String });

/**
 * Runs one Effect and flattens the outcome into a value. Every failure —
 * expected error or defect — comes back as something the renderer can
 * pattern-match on rather than a stringified rejection.
 */
async function settle<A, E>(
  effect: Effect.Effect<A, E, AppServices>,
): Promise<IpcResult<A>> {
  const exit = await runtime.runPromiseExit(effect);
  return Exit.match(exit, {
    // Electron's serializer refuses `undefined`, which is what a void handler
    // such as `stageFile` succeeds with — send `null` in its place.
    onSuccess: (value): IpcResult<A> => ({
      ok: true,
      value: (value === undefined ? null : value) as A,
    }),
    onFailure: (cause): IpcResult<A> => ({
      ok: false,
      error: toIpcFailure(Cause.squash(cause)),
    }),
  });
}

/**
 * Registers a channel that takes a payload. It is decoded before it reaches a
 * service, so a renderer that has been tampered with cannot hand arbitrary
 * shapes to `git`.
 */
function handle<S extends Schema.Schema<any, any, never>, A, E>(
  channel: string,
  schema: S,
  run: (input: Schema.Schema.Type<S>) => Effect.Effect<A, E, AppServices>,
): void {
  const decode = Schema.decodeUnknown(schema);
  ipcMain.handle(channel, (_event, payload: unknown) =>
    settle(decode(payload).pipe(Effect.flatMap(run))),
  );
}

/** Registers a channel that takes nothing. */
function handleNoInput<A, E>(
  channel: string,
  run: () => Effect.Effect<A, E, AppServices>,
): void {
  ipcMain.handle(channel, () => settle(run()));
}

export function registerIpcHandlers(): void {
  handleNoInput(IpcChannel.getDiff, () =>
    Effect.flatMap(Diff, (diff) => diff.getDiff),
  );

  handle(IpcChannel.getFileContents, GetFileContentsRequest, (request) =>
    Effect.flatMap(Diff, (diff) =>
      diff.getFileContents(
        request.path,
        request.oldPath,
        request.status,
        request.staged,
      ),
    ),
  );

  handle(IpcChannel.getHistory, GetHistoryRequest, (request) =>
    Effect.flatMap(History, (history) => history.getHistory(request.limit)),
  );

  handle(IpcChannel.stageFile, StageFileRequest, (request) =>
    Effect.flatMap(Diff, (diff) =>
      diff.stageFile(request.path, request.oldPath),
    ),
  );

  handleNoInput(IpcChannel.generateCommitMessage, () =>
    Effect.flatMap(CommitMessage, (service) => service.generate),
  );

  handle(IpcChannel.sendClaudeMessage, SendClaudeMessageRequest, (request) =>
    Effect.flatMap(ClaudeChannel, (channel) => channel.send(request.message)),
  );

  handleNoInput(IpcChannel.listProjects, () =>
    Effect.flatMap(Workspace, (workspace) => workspace.list),
  );

  handleNoInput(IpcChannel.currentProject, () =>
    Effect.flatMap(Workspace, (workspace) => workspace.currentProject),
  );

  handle(IpcChannel.openProject, OpenProjectRequest, (request) =>
    Effect.flatMap(Workspace, (workspace) => workspace.open(request.path)),
  );

  handle(IpcChannel.forgetProject, ForgetProjectRequest, (request) =>
    Effect.flatMap(Workspace, (workspace) => workspace.forget(request.path)),
  );

  // Clipboard writes go through the main process rather than
  // `navigator.clipboard`, which depends on the renderer being a secure
  // context — it is not when the app is loaded from `file:`.
  handle(IpcChannel.writeClipboardText, ClipboardTextRequest, (text) =>
    Effect.tryPromise({
      // `clipboard.writeText` resolves rather than returning void here, and an
      // unawaited promise is not something Electron can send back. Awaiting it
      // and answering with `null` covers both shapes of the API.
      try: async () => {
        await Promise.resolve(clipboard.writeText(text));
        return null;
      },
      catch: (error) =>
        new ClipboardError({
          message: `failed to write to the clipboard: ${String(error)}`,
        }),
    }),
  );
}
