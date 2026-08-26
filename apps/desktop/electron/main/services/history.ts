import { Effect } from "effect";
import type { Commit } from "../../shared/contract";
import type { GitError, NoProjectOpenError } from "../errors";
import { Git } from "./git";

const DEFAULT_LIMIT = 100;
const MAX_LIMIT = 5000;

/**
 * \x1f between fields, \x1e between records: neither can appear in a subject or
 * a ref name, unlike newlines and tabs.
 */
const PRETTY_FORMAT =
  "--pretty=format:%H%x1f%h%x1f%an%x1f%ae%x1f%ar%x1f%aI%x1f%P%x1f%D%x1f%s%x1e";

export class History extends Effect.Service<History>()("onlydiffs/History", {
  effect: Effect.gen(function* () {
    const git = yield* Git;

    /** Commit history reachable from HEAD, newest first. */
    const getHistory = (
      limit?: number,
    ): Effect.Effect<Commit[], GitError | NoProjectOpenError> =>
      Effect.gen(function* () {
        // The limit arrives from the renderer and is interpolated into an
        // argument, so it is normalised to a plain integer before it gets near
        // the command line.
        const count =
          limit === undefined || !Number.isFinite(limit)
            ? DEFAULT_LIMIT
            : Math.min(Math.max(Math.trunc(limit), 1), MAX_LIMIT);

        const log = yield* git.run([
          "log",
          `--max-count=${count}`,
          PRETTY_FORMAT,
        ]);

        const commits: Commit[] = [];
        for (const rawRecord of log.split("\u001e")) {
          const record = rawRecord.replace(/^\n+/, "");
          if (record.trim().length === 0) continue;

          const fields = record.split("\u001f");
          if (fields.length < 9) continue;

          commits.push({
            hash: fields[0],
            shortHash: fields[1],
            author: fields[2],
            authorEmail: fields[3],
            relativeDate: fields[4],
            date: fields[5],
            isMerge: fields[6].split(/\s+/).filter(Boolean).length > 1,
            refs: fields[7],
            subject: fields[8],
          });
        }

        return commits;
      });

    return { getHistory } as const;
  }),
  dependencies: [Git.Default],
}) {}
