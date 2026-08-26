import { Effect } from "effect";
import type { GitError, NoProjectOpenError } from "../errors";
import { Git } from "./git";

/**
 * Every file in the repository, as flat repo-relative paths.
 *
 * One `git ls-files` covers the whole tree: `-c` tracked, `-o` untracked,
 * `--exclude-standard` to honour `.gitignore`, `-z` because a path may contain
 * a newline. That is a single process for the entire repository — walking the
 * filesystem instead would mean reimplementing `.gitignore` and being slower
 * for it.
 *
 * Deliberately uncached. It costs ~10ms on a small repository, the renderer
 * asks for it once per load, and a cache would go stale the moment a file was
 * created.
 */
export class FileTree extends Effect.Service<FileTree>()("onlydiffs/FileTree", {
  effect: Effect.gen(function* () {
    const git = yield* Git;

    const listFiles: Effect.Effect<
      string[],
      GitError | NoProjectOpenError
    > = git
      .run(["ls-files", "-co", "--exclude-standard", "-z"])
      .pipe(
        Effect.map((output) =>
          output.split("\0").filter((path) => path.length > 0),
        ),
      );

    return { listFiles } as const;
  }),
  dependencies: [Git.Default],
}) {}
