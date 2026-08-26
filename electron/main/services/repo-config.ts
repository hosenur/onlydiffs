import { homedir } from "node:os";
import * as path from "node:path";
import { Config, Effect } from "effect";
import { RepoConfigError } from "../errors";

/**
 * The repository OnlyDiffs looks at. There is no picker yet, so this is the one
 * knob: `ONLYDIFFS_REPO_PATH` in the Electron process environment, falling back to
 * the path the Tauri build had compiled in.
 */
const DEFAULT_REPO_PATH = "/Users/rahaman/Developer/minwinn";

/** `git -C` does no `~` expansion, so do it here rather than hand it through. */
function expandHome(value: string): string {
  if (value === "~") return homedir();
  if (value.startsWith("~/")) return path.join(homedir(), value.slice(2));
  return value;
}

export class RepoConfig extends Effect.Service<RepoConfig>()(
  "onlydiffs/RepoConfig",
  {
    effect: Effect.gen(function* () {
      const configured = yield* Config.string("ONLYDIFFS_REPO_PATH").pipe(
        Config.withDefault(DEFAULT_REPO_PATH),
        Config.map((value) => value.trim()),
      );

      if (configured.length === 0) {
        return yield* new RepoConfigError({
          message: "ONLYDIFFS_REPO_PATH is set but empty.",
        });
      }

      // Relative paths would resolve against Electron's cwd, which is wherever
      // the app happened to be launched from — resolve once, here, so every
      // consumer sees one absolute path.
      const repoPath = path.resolve(expandHome(configured));

      return { repoPath } as const;
    }),
  },
) {}
