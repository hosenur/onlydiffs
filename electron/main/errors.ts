import { Data } from "effect";
import type { IpcFailure } from "../shared/contract";

/**
 * Every failure the main process can produce. They all carry a `message`, so
 * `toIpcFailure` can flatten any of them for the renderer without a per-error
 * branch, and the renderer keeps the `_tag` it needs to react differently.
 */

export class RepoConfigError extends Data.TaggedError("RepoConfigError")<{
  readonly message: string;
}> {}

export class GitError extends Data.TaggedError("GitError")<{
  readonly message: string;
}> {}

export class WorkTreeError extends Data.TaggedError("WorkTreeError")<{
  readonly message: string;
}> {}

export class InvalidPathError extends Data.TaggedError("InvalidPathError")<{
  readonly message: string;
}> {}

export class CommitMessageError extends Data.TaggedError("CommitMessageError")<{
  readonly message: string;
}> {}

export class ClaudeChannelError extends Data.TaggedError("ClaudeChannelError")<{
  readonly message: string;
}> {}

export class ClipboardError extends Data.TaggedError("ClipboardError")<{
  readonly message: string;
}> {}

export type CashewError =
  | RepoConfigError
  | GitError
  | WorkTreeError
  | InvalidPathError
  | CommitMessageError
  | ClaudeChannelError
  | ClipboardError;

/** Anything a defect handler might be handed, reduced to readable text. */
export function describeUnknown(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

export function toIpcFailure(error: unknown): IpcFailure {
  if (
    typeof error === "object" &&
    error !== null &&
    "_tag" in error &&
    typeof (error as { _tag: unknown })._tag === "string"
  ) {
    const tagged = error as { _tag: string; message?: unknown };
    return {
      _tag: tagged._tag,
      message:
        typeof tagged.message === "string" && tagged.message.length > 0
          ? tagged.message
          : describeUnknown(error),
    };
  }
  return { _tag: "UnknownError", message: describeUnknown(error) };
}
