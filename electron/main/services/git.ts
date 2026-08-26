import { Command, CommandExecutor } from "@effect/platform";
import { Effect, Stream } from "effect";
import { GitError } from "../errors";
import { RepoConfig } from "./repo-config";

/**
 * Runs `git` against the configured repository. Every other service goes
 * through this one, so process spawning, decoding, and exit-code handling live
 * in a single place.
 */
export class Git extends Effect.Service<Git>()("cashew/Git", {
  effect: Effect.gen(function* () {
    const executor = yield* CommandExecutor.CommandExecutor;
    const { repoPath } = yield* RepoConfig;

    const runIn = (
      cwd: string,
      args: ReadonlyArray<string>,
    ): Effect.Effect<string, GitError> =>
      Effect.scoped(
        Effect.gen(function* () {
          const process = yield* executor.start(
            Command.make("git", "-C", cwd, ...args),
          );

          // Draining both pipes concurrently with the exit-code wait is not
          // optional: a patch larger than the OS pipe buffer would otherwise
          // block the child forever while we wait for it to exit.
          const [stdout, stderr, exitCode] = yield* Effect.all(
            [
              Stream.mkString(Stream.decodeText(process.stdout)),
              Stream.mkString(Stream.decodeText(process.stderr)),
              process.exitCode,
            ],
            { concurrency: 3 },
          );

          // `git diff --no-index` exits non-zero precisely when it *did*
          // produce a patch, so a non-empty stdout is always success.
          if (stdout.length === 0 && exitCode !== 0) {
            const detail = stderr.trim();
            return yield* new GitError({
              message:
                detail.length > 0 ? detail : `git ${args.join(" ")} failed`,
            });
          }

          return stdout;
        }),
      ).pipe(
        Effect.mapError((error): GitError =>
          error._tag === "GitError"
            ? error
            : new GitError({ message: `failed to run git: ${error.message}` }),
        ),
      );

    const run = (args: ReadonlyArray<string>) => runIn(repoPath, args);

    /** Reads one blob. An empty revision means the index (`git show :path`). */
    const showFile = (revision: string, filePath: string) =>
      run(["show", revision.length === 0 ? `:${filePath}` : `${revision}:${filePath}`]);

    return { repoPath, run, runIn, showFile } as const;
  }),
  dependencies: [RepoConfig.Default],
}) {}
