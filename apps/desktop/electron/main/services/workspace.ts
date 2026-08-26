import { homedir } from "node:os";
import * as path from "node:path";
import { FileSystem } from "@effect/platform";
import { Config, Effect, Option, Ref, Schema } from "effect";
import type { Project } from "../../shared/contract";
import { InvalidProjectError, NoProjectOpenError } from "../errors";

/**
 * Which repository the app is looking at, and which ones it has looked at
 * before. This is the one piece of genuinely mutable state in the main process:
 * everything else derives the path from here on each call, so opening a project
 * takes effect without rebuilding any layer.
 */

/**
 * Where the recents list is kept. Small enough to rewrite whole every time.
 * `ONLYDIFFS_STATE_DIR` redirects it, which is what keeps tests from writing
 * into the real store in the user's home directory.
 */
const STORE_DIR = ".onlydiffs";
const STORE_FILE = "projects.json";
const MAX_RECENTS = 20;

const StoredProject = Schema.Struct({
  path: Schema.String,
  lastOpenedAt: Schema.Number,
});

const Store = Schema.Struct({
  version: Schema.Literal(1),
  projects: Schema.Array(StoredProject),
});

const decodeStore = Schema.decodeUnknown(Schema.parseJson(Store));

/** `git -C` does no `~` expansion, so do it here rather than hand it through. */
function expandHome(value: string): string {
  if (value === "~") return homedir();
  if (value.startsWith("~/")) return path.join(homedir(), value.slice(2));
  return value;
}

/**
 * Turns whatever the user typed into an absolute path. A relative path is
 * resolved against the home directory rather than `process.cwd()`, which for a
 * packaged app is wherever Finder happened to launch it from.
 */
function toAbsolute(input: string): string {
  const expanded = expandHome(input.trim());
  return path.isAbsolute(expanded)
    ? path.normalize(expanded)
    : path.resolve(homedir(), expanded);
}

export class Workspace extends Effect.Service<Workspace>()(
  "onlydiffs/Workspace",
  {
    effect: Effect.gen(function* () {
      const fs = yield* FileSystem.FileSystem;
      const stateDir = yield* Config.string("ONLYDIFFS_STATE_DIR").pipe(
        Config.withDefault(path.join(homedir(), STORE_DIR)),
      );
      const storePath = path.join(stateDir, STORE_FILE);

      const readStore = fs.readFileString(storePath).pipe(
        Effect.flatMap(decodeStore),
        Effect.map((store) => [...store.projects]),
        // A missing or corrupt file just means "no history yet".
        Effect.orElseSucceed(() => [] as Array<{ path: string; lastOpenedAt: number }>),
      );

      const recents = yield* Ref.make(yield* readStore);
      const current = yield* Ref.make(Option.none<string>());

      const writeStore = Effect.gen(function* () {
        const projects = yield* Ref.get(recents);
        const body = JSON.stringify({ version: 1, projects }, null, 2);
        yield* fs.makeDirectory(path.dirname(storePath), { recursive: true }).pipe(
          Effect.andThen(fs.writeFileString(storePath, body)),
          // Losing the history file is not worth failing an open over.
          Effect.ignore,
        );
      });

      /**
       * Finds the repository root at or above `dir`, so pasting any path inside
       * a checkout opens the repository rather than being rejected.
       */
      const findRepoRoot = (
        dir: string,
      ): Effect.Effect<Option.Option<string>, never> =>
        Effect.gen(function* () {
          let candidate = dir;
          for (;;) {
            if (yield* fs.exists(path.join(candidate, ".git"))) {
              return Option.some(candidate);
            }
            const parent = path.dirname(candidate);
            if (parent === candidate) return Option.none();
            candidate = parent;
          }
        }).pipe(Effect.orElseSucceed(() => Option.none<string>()));

      const describe = (repoPath: string): Project => ({
        path: repoPath,
        name: path.basename(repoPath) || repoPath,
      });

      /** Validates a path and, if it checks out, makes it the current project. */
      const open = (
        input: string,
      ): Effect.Effect<Project, InvalidProjectError> =>
        Effect.gen(function* () {
          if (input.trim().length === 0) {
            return yield* new InvalidProjectError({
              message: "Enter a path to a git repository.",
            });
          }

          const absolute = toAbsolute(input);

          const isDirectory = yield* fs
            .stat(absolute)
            .pipe(
              Effect.map((info) => info.type === "Directory"),
              Effect.orElseSucceed(() => false),
            );
          if (!isDirectory) {
            return yield* new InvalidProjectError({
              message: `No such folder: ${absolute}`,
            });
          }

          const root = yield* findRepoRoot(absolute);
          if (Option.isNone(root)) {
            return yield* new InvalidProjectError({
              message: `Not a git repository (no .git found at or above ${absolute}).`,
            });
          }

          const repoPath = root.value;
          yield* Ref.set(current, Option.some(repoPath));
          yield* Ref.update(recents, (list) =>
            [
              { path: repoPath, lastOpenedAt: Date.now() },
              ...list.filter((entry) => entry.path !== repoPath),
            ].slice(0, MAX_RECENTS),
          );
          yield* writeStore;

          return describe(repoPath);
        });

      /** The active repository. Everything that shells out to git needs this. */
      const currentPath: Effect.Effect<string, NoProjectOpenError> = Ref.get(
        current,
      ).pipe(
        Effect.flatMap(
          Option.match({
            onNone: () =>
              new NoProjectOpenError({ message: "No project is open." }),
            onSome: (value) => Effect.succeed(value),
          }),
        ),
      );

      const currentProject: Effect.Effect<Project | null> = Ref.get(current).pipe(
        Effect.map(Option.match({ onNone: () => null, onSome: describe })),
      );

      /** Recents, newest first, with entries that no longer exist dropped. */
      const list: Effect.Effect<Project[]> = Effect.gen(function* () {
        const entries = yield* Ref.get(recents);
        const alive = yield* Effect.all(
          entries.map((entry) =>
            fs.exists(entry.path).pipe(
              Effect.orElseSucceed(() => false),
              Effect.map((exists) => ({ entry, exists })),
            ),
          ),
          { concurrency: 8 },
        );
        return alive
          .filter((row) => row.exists)
          .sort((a, b) => b.entry.lastOpenedAt - a.entry.lastOpenedAt)
          .map((row) => describe(row.entry.path));
      });

      const forget = (repoPath: string): Effect.Effect<void> =>
        Ref.update(recents, (l) => l.filter((e) => e.path !== repoPath)).pipe(
          Effect.andThen(writeStore),
        );

      // Keep the old environment knob working: if it is set and valid, the app
      // opens straight into that repository instead of the landing page.
      const configured = yield* Config.string("ONLYDIFFS_REPO_PATH").pipe(
        Config.option,
      );
      if (Option.isSome(configured) && configured.value.trim().length > 0) {
        yield* open(configured.value).pipe(Effect.ignore);
      }

      return { open, currentPath, currentProject, list, forget } as const;
    }),
  },
) {}
