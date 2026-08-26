import { FileSystem } from "@effect/platform";
import { Effect, Option } from "effect";
import type {
  ChangeStatus,
  FileChange,
  FullFileContents,
  RepoDiff,
} from "../../shared/contract";
import {
  GitError,
  InvalidPathError,
  NoProjectOpenError,
  WorkTreeError,
} from "../errors";
import { Git } from "./git";

/** Anything that can go wrong just getting at the repository. */
type RepoError = GitError | NoProjectOpenError;

/** How many `git diff` children may be in flight while collecting the repo. */
const PATCH_CONCURRENCY = 8;

/** Splits on newlines the way Rust's `str::lines` does — `\r\n` included. */
function splitLines(text: string): string[] {
  return text.split("\n").map((line) => line.replace(/\r$/, ""));
}

function isBinary(patch: string): boolean {
  return splitLines(patch).some(
    (line) => line.startsWith("Binary files ") || line === "GIT binary patch",
  );
}

function countChanges(patch: string): { additions: number; deletions: number } {
  let additions = 0;
  let deletions = 0;
  for (const line of splitLines(patch)) {
    // The file headers are not content lines even though they start with +/-.
    if (line.startsWith("+++") || line.startsWith("---")) continue;
    if (line.startsWith("+")) additions += 1;
    else if (line.startsWith("-")) deletions += 1;
  }
  return { additions, deletions };
}

/**
 * Maps one side of a porcelain XY pair to a status name. `null` means that side
 * has no change at all.
 */
function classify(code: string): ChangeStatus | null {
  switch (code) {
    case " ":
      return null;
    case "?":
      return "untracked";
    case "R":
    case "C":
      return "renamed";
    case "A":
      return "added";
    case "D":
      return "deleted";
    default:
      return "modified";
  }
}

/**
 * Repository-relative and staying inside the repository. Paths reach this from
 * the renderer, so `..`, a leading `/`, and a drive letter are all rejected
 * before they can be handed to `git show` or joined onto the repo root.
 */
function isSafeRepoPath(value: string): boolean {
  if (value.length === 0) return false;
  // Windows drive letters and UNC prefixes, plus any leading separator.
  if (/^[/\\]/.test(value) || /^[A-Za-z]:/.test(value)) return false;
  return !value.split(/[/\\]/).includes("..");
}

export class Diff extends Effect.Service<Diff>()("onlydiffs/Diff", {
  effect: Effect.gen(function* () {
    const git = yield* Git;
    const fs = yield* FileSystem.FileSystem;

    const validatePath = (value: string) =>
      isSafeRepoPath(value)
        ? Effect.void
        : new InvalidPathError({
            message: `invalid repository-relative path: ${value}`,
          });

    const worktreeFile = (filePath: string) =>
      git.currentPath.pipe(
        Effect.flatMap((repoPath) => fs.readFile(`${repoPath}/${filePath}`)),
      ).pipe(
        // Lossy by design: the Rust build used `String::from_utf8_lossy`, so a
        // file with a stray byte renders rather than blanking the card.
        Effect.map((bytes) => new TextDecoder().decode(bytes)),
        Effect.mapError(
          (error) =>
            new WorkTreeError({
              message: `failed to read ${filePath}: ${error.message}`,
            }),
        ),
      );

    const filePatch = (
      filePath: string,
      oldPath: string | null,
      status: ChangeStatus,
      staged: boolean,
    ): Effect.Effect<string, RepoError> => {
      if (status === "untracked") {
        return git.run(["diff", "--no-index", "--", "/dev/null", filePath]);
      }
      if (staged) {
        return oldPath === null
          ? git.run(["diff", "--cached", "--", filePath])
          : git.run(["diff", "--cached", "-M", "--", oldPath, filePath]);
      }
      return git.run(["diff", "--", filePath]);
    };

    /**
     * Loads complete file versions. Kept separate from `getDiff` so startup
     * never reads every changed file — cards ask for this as they approach the
     * viewport.
     */
    const getFileContents = (
      filePath: string,
      oldPath: string | null,
      status: ChangeStatus,
      staged: boolean,
    ): Effect.Effect<
      FullFileContents,
      RepoError | InvalidPathError | WorkTreeError
    > =>
      Effect.gen(function* () {
        yield* validatePath(filePath);
        if (oldPath !== null) yield* validatePath(oldPath);

        if (status === "untracked") {
          return {
            oldContents: null,
            newContents: yield* worktreeFile(filePath),
          };
        }

        if (staged) {
          return {
            oldContents:
              status === "added"
                ? null
                : yield* git.showFile("HEAD", oldPath ?? filePath),
            newContents:
              status === "deleted" ? null : yield* git.showFile("", filePath),
          };
        }

        return {
          oldContents:
            status === "added"
              ? null
              : yield* git.showFile(
                  "",
                  status === "renamed" ? (oldPath ?? filePath) : filePath,
                ),
          newContents:
            status === "deleted" ? null : yield* worktreeFile(filePath),
        };
      });

    /**
     * Stages the current file, including the previous path when the change is a
     * rename so Git records both halves of the move.
     */
    const stageFile = (
      filePath: string,
      oldPath: string | null,
    ): Effect.Effect<void, RepoError | InvalidPathError> =>
      Effect.gen(function* () {
        yield* validatePath(filePath);
        if (oldPath !== null) yield* validatePath(oldPath);

        const args = ["add", "-A", "--", filePath];
        if (oldPath !== null && oldPath !== filePath) args.push(oldPath);
        yield* git.run(args);
      });

    /**
     * Metadata for every change in the repo, untracked files included. Staged
     * and unstaged edits to the same path are returned as separate rows,
     * because they are genuinely two different patches.
     */
    const getDiff: Effect.Effect<RepoDiff, RepoError> = Effect.gen(function* () {
      const [branch, head, status] = yield* Effect.all(
        [
          git.run(["rev-parse", "--abbrev-ref", "HEAD"]),
          git.run(["log", "-1", "--pretty=%h %s"]),
          // -uall matters: without it an untracked *directory* collapses into a
          // single "?? dir/" record, and diffing a directory is not a thing.
          git.run(["status", "--porcelain", "-z", "-uall"]),
        ],
        { concurrency: 3 },
      );

      // With -z each record is "XY PATH\0"; renames and copies add the original
      // path as the next NUL-terminated field.
      const records = status.split("\0").filter((record) => record.length > 0);
      const sides: Array<{
        path: string;
        oldPath: string | null;
        status: ChangeStatus;
        staged: boolean;
      }> = [];

      for (let index = 0; index < records.length; ) {
        const record = records[index];
        index += 1;
        if (record.length < 4) continue;

        const indexCode = record[0];
        const workTreeCode = record[1];
        const filePath = record.slice(3);

        let oldPath: string | null = null;
        if ("RC".includes(indexCode) || "RC".includes(workTreeCode)) {
          if (index >= records.length) continue;
          oldPath = records[index];
          index += 1;
        }

        const indexStatus = indexCode === "?" ? null : classify(indexCode);
        const workTreeStatus = classify(workTreeCode);

        if (indexStatus !== null) {
          sides.push({
            path: filePath,
            oldPath,
            status: indexStatus,
            staged: true,
          });
        }
        if (workTreeStatus !== null) {
          sides.push({
            path: filePath,
            oldPath,
            status: workTreeStatus,
            staged: false,
          });
        }
      }

      const rows = yield* Effect.all(
        sides.map((side) =>
          filePatch(side.path, side.oldPath, side.status, side.staged).pipe(
            Effect.map((patch) =>
              patch.trim().length === 0
                ? Option.none<FileChange>()
                : Option.some(toFileChange(side, patch, null)),
            ),
            // One unreadable path shouldn't blank the whole view — surface it
            // on its own row instead.
            Effect.catchAll((error: RepoError) =>
              Effect.succeed(
                Option.some(toFileChange(side, "", error.message)),
              ),
            ),
          ),
        ),
        { concurrency: PATCH_CONCURRENCY },
      );

      const files = rows.filter(Option.isSome).map((row) => row.value);
      files.sort(
        (a, b) =>
          (a.path < b.path ? -1 : a.path > b.path ? 1 : 0) ||
          Number(b.staged) - Number(a.staged),
      );

      return {
        repoPath: yield* git.currentPath,
        branch: branch.trim(),
        head: head.trim(),
        files,
      };
    });

    /**
     * Stages everything and commits it in one step.
     *
     * `git add -A` then `git commit` rather than `commit -a`, because `-a`
     * ignores untracked files and this is the "commit all" the user means.
     * The message goes through `-m` as a single argument, so nothing in it is
     * interpreted as a flag or reaches a shell.
     */
    const commitAll = (
      message: string,
    ): Effect.Effect<string, RepoError | InvalidPathError> =>
      Effect.gen(function* () {
        const subject = message.trim();
        if (subject.length === 0) {
          return yield* new InvalidPathError({
            message: "A commit needs a message.",
          });
        }

        const status = yield* git.run(["status", "--porcelain"]);
        if (status.trim().length === 0) {
          return yield* new GitError({
            message: "Nothing to commit — the working tree is clean.",
          });
        }

        yield* git.run(["add", "-A"]);
        yield* git.run(["commit", "-m", subject]);
        return yield* git.run(["log", "-1", "--pretty=%h %s"]).pipe(
          Effect.map((line) => line.trim()),
        );
      });

    /**
     * The complete staged, unstaged, and untracked diff as one annotated
     * document — what the commit-message model is shown.
     */
    const commitMessageDiff: Effect.Effect<string, RepoError> = Effect.gen(
      function* () {
        const repoDiff = yield* getDiff;
        if (repoDiff.files.length === 0) {
          return yield* new GitError({
            message: "Working tree is clean; there is no diff to summarize.",
          });
        }

        const patches = yield* Effect.all(
          repoDiff.files.map((file) =>
            filePatch(file.path, file.oldPath, file.status, file.staged).pipe(
              Effect.map((patch) => ({ file, patch })),
            ),
          ),
          { concurrency: PATCH_CONCURRENCY },
        );

        let diff = "";
        for (const { file, patch } of patches) {
          if (patch.trim().length === 0) continue;
          const section =
            file.status === "untracked"
              ? "untracked"
              : file.staged
                ? "staged"
                : "unstaged";
          diff += `### ${section}: ${file.path}\n`;
          diff += patch;
          if (!patch.endsWith("\n")) diff += "\n";
          diff += "\n";
        }

        if (diff.trim().length === 0) {
          return yield* new GitError({
            message: "Working tree is clean; there is no diff to summarize.",
          });
        }

        return diff;
      },
    );

    return {
      getDiff,
      getFileContents,
      stageFile,
      commitAll,
      commitMessageDiff,
    } as const;
  }),
  dependencies: [Git.Default],
}) {}

function toFileChange(
  side: {
    path: string;
    oldPath: string | null;
    status: ChangeStatus;
    staged: boolean;
  },
  patch: string,
  error: string | null,
): FileChange {
  const binary = isBinary(patch);
  const { additions, deletions } = binary
    ? { additions: 0, deletions: 0 }
    : countChanges(patch);

  return {
    id: `${side.staged ? "staged" : "unstaged"}:${side.path}`,
    path: side.path,
    oldPath: side.oldPath,
    status: side.status,
    staged: side.staged,
    additions,
    deletions,
    binary,
    error,
  };
}
