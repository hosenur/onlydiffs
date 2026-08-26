import { NodeContext, NodeHttpClient } from "@effect/platform-node";
import { Layer, ManagedRuntime } from "effect";
import { ClaudeChannel } from "./services/claude-channel";
import { CommitMessage } from "./services/commit-message";
import { Diff } from "./services/diff";
import { History } from "./services/history";
import { Workspace } from "./services/workspace";

/**
 * Everything the IPC handlers can ask for. `NodeContext` supplies the
 * `CommandExecutor` git runs through and the `FileSystem` the worktree is read
 * with; swapping either one is all a test harness needs to do.
 */
const AppLayer = Layer.mergeAll(
  Diff.Default,
  History.Default,
  CommitMessage.Default,
  ClaudeChannel.Default,
  Workspace.Default,
).pipe(Layer.provide(Layer.mergeAll(NodeContext.layer, NodeHttpClient.layer)));

export type AppServices = Layer.Layer.Success<typeof AppLayer>;

/**
 * One long-lived runtime for the whole app. Services are built once, on the
 * first request, and torn down when the app quits.
 */
export const runtime = ManagedRuntime.make(AppLayer);
