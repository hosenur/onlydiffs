import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import * as path from "node:path";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { NodeContext } from "@effect/platform-node";
import { Effect, Layer, ManagedRuntime } from "effect";
import { Workspace } from "./workspace";

const makeRuntime = () =>
  ManagedRuntime.make(Workspace.Default.pipe(Layer.provide(NodeContext.layer)));

const use = <A, E>(f: (service: Workspace) => Effect.Effect<A, E>) =>
  Effect.flatMap(Workspace, f);

let root: string;
let stateDir: string;
let runtime: ReturnType<typeof makeRuntime>;
const saved: Record<string, string | undefined> = {};

function makeRepo(name: string): string {
  const dir = path.join(root, name);
  mkdirSync(dir, { recursive: true });
  execFileSync("git", ["-C", dir, "init", "-q"]);
  return dir;
}

beforeEach(() => {
  root = path.resolve(mkdtempSync(path.join(tmpdir(), "onlydiffs-ws-")));
  stateDir = path.join(root, "state");
  for (const key of ["ONLYDIFFS_REPO_PATH", "ONLYDIFFS_STATE_DIR"]) {
    saved[key] = process.env[key];
  }
  // No repo preselected, and the store must never be the user's real one.
  delete process.env.ONLYDIFFS_REPO_PATH;
  process.env.ONLYDIFFS_STATE_DIR = stateDir;
  runtime = makeRuntime();
});

afterEach(async () => {
  await runtime.dispose();
  for (const [key, value] of Object.entries(saved)) {
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
  rmSync(root, { recursive: true, force: true });
});

test("no project is open until one is chosen", async () => {
  expect(await runtime.runPromise(use((w) => w.currentProject))).toBeNull();
  const exit = await runtime.runPromiseExit(use((w) => w.currentPath));
  expect(exit._tag).toBe("Failure");
});

test("opening a repository makes it current and records it", async () => {
  const repo = makeRepo("alpha");

  const opened = await runtime.runPromise(use((w) => w.open(repo)));
  expect(opened.path).toBe(repo);
  expect(opened.name).toBe("alpha");

  expect(await runtime.runPromise(use((w) => w.currentPath))).toBe(repo);
  expect(await runtime.runPromise(use((w) => w.list))).toEqual([
    { path: repo, name: "alpha" },
  ]);
});

test("a path inside a checkout opens the repository root", async () => {
  const repo = makeRepo("beta");
  const nested = path.join(repo, "src", "deep");
  mkdirSync(nested, { recursive: true });

  const opened = await runtime.runPromise(use((w) => w.open(nested)));
  expect(opened.path).toBe(repo);
});

test("a folder that is not a repository is rejected", async () => {
  const plain = path.join(root, "not-a-repo");
  mkdirSync(plain, { recursive: true });

  const exit = await runtime.runPromiseExit(use((w) => w.open(plain)));
  expect(exit._tag).toBe("Failure");
  // The rejection must not become the current project.
  expect(await runtime.runPromise(use((w) => w.currentProject))).toBeNull();
});

test("a path that does not exist is rejected", async () => {
  const exit = await runtime.runPromiseExit(
    use((w) => w.open(path.join(root, "nope"))),
  );
  expect(exit._tag).toBe("Failure");
});

test("an empty path is rejected", async () => {
  const exit = await runtime.runPromiseExit(use((w) => w.open("   ")));
  expect(exit._tag).toBe("Failure");
});

test("recents are newest first and de-duplicated", async () => {
  const a = makeRepo("one");
  const b = makeRepo("two");

  await runtime.runPromise(use((w) => w.open(a)));
  await runtime.runPromise(use((w) => w.open(b)));
  await runtime.runPromise(use((w) => w.open(a)));

  const list = await runtime.runPromise(use((w) => w.list));
  expect(list.map((p) => p.name)).toEqual(["one", "two"]);
});

test("recents survive a restart and skip folders that vanished", async () => {
  const kept = makeRepo("kept");
  const gone = makeRepo("gone");
  await runtime.runPromise(use((w) => w.open(kept)));
  await runtime.runPromise(use((w) => w.open(gone)));

  expect(
    JSON.parse(readFileSync(path.join(stateDir, "projects.json"), "utf8"))
      .projects,
  ).toHaveLength(2);

  rmSync(gone, { recursive: true, force: true });

  // A fresh runtime reads the store back off disk.
  await runtime.dispose();
  runtime = makeRuntime();

  const list = await runtime.runPromise(use((w) => w.list));
  expect(list.map((p) => p.name)).toEqual(["kept"]);
});

test("forget removes a project from the history", async () => {
  const repo = makeRepo("temporary");
  await runtime.runPromise(use((w) => w.open(repo)));
  await runtime.runPromise(use((w) => w.forget(repo)));
  expect(await runtime.runPromise(use((w) => w.list))).toEqual([]);
});

test("a corrupt store file is treated as empty history", async () => {
  mkdirSync(stateDir, { recursive: true });
  writeFileSync(path.join(stateDir, "projects.json"), "{ not json");

  await runtime.dispose();
  runtime = makeRuntime();
  expect(await runtime.runPromise(use((w) => w.list))).toEqual([]);
});

test("ONLYDIFFS_REPO_PATH still opens a repository at startup", async () => {
  const repo = makeRepo("preselected");
  process.env.ONLYDIFFS_REPO_PATH = repo;

  await runtime.dispose();
  runtime = makeRuntime();

  expect(await runtime.runPromise(use((w) => w.currentPath))).toBe(repo);
});

test("the store never lands in the real home directory during tests", () => {
  expect(stateDir.startsWith(path.join(homedir(), ".onlydiffs"))).toBe(false);
});
