import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { NodeContext } from "@effect/platform-node";
import { Effect, Layer, ManagedRuntime } from "effect";
import { Diff } from "./diff";
import { History } from "./history";

/**
 * The Rust build's integration tests, ported: they are what proves the
 * porcelain walk and the startup/lazy split still behave the same after the
 * move to Effect.
 */

/** `Diff` and `History` are Tags, so this is just a shorter `Effect.flatMap`. */
const withDiff = <A, E>(use: (service: Diff) => Effect.Effect<A, E>) =>
  Effect.flatMap(Diff, use);
const withHistory = <A, E>(use: (service: History) => Effect.Effect<A, E>) =>
  Effect.flatMap(History, use);

/** Built per test so each one gets a `RepoConfig` pointed at its own repo. */
const makeRuntime = () =>
  ManagedRuntime.make(
    Layer.mergeAll(Diff.Default, History.Default).pipe(
      Layer.provide(NodeContext.layer),
    ),
  );

let repoPath: string;
let runtime: ReturnType<typeof makeRuntime>;
let previousRepoPath: string | undefined;

function git(...args: string[]): string {
  return execFileSync("git", ["-C", repoPath, ...args], { encoding: "utf8" });
}

function write(name: string, contents: string): void {
  writeFileSync(path.join(repoPath, name), contents);
}

beforeEach(() => {
  // macOS hands out /var/folders/… which is a symlink to /private/var/folders/…;
  // git reports the resolved form, so resolve it here or the paths never match.
  repoPath = path.resolve(mkdtempSync(path.join(tmpdir(), "onlydiffs-test-")));
  git("init", "-q");
  git("config", "user.email", "onlydiffs@example.test");
  git("config", "user.name", "OnlyDiffs Test");

  // RepoConfig reads this when the layer is built, so it has to be in place
  // before the runtime exists.
  previousRepoPath = process.env.ONLYDIFFS_REPO_PATH;
  process.env.ONLYDIFFS_REPO_PATH = repoPath;
  runtime = makeRuntime();
});

afterEach(async () => {
  await runtime.dispose();
  if (previousRepoPath === undefined) delete process.env.ONLYDIFFS_REPO_PATH;
  else process.env.ONLYDIFFS_REPO_PATH = previousRepoPath;
  rmSync(repoPath, { recursive: true, force: true });
});

test("complete file contents are loaded separately from the startup diff", async () => {
  const original =
    Array.from({ length: 40 }, (_, index) =>
      index + 1 === 25 ? "UNCHANGED_MARKER" : `original line ${index + 1}`,
    ).join("\n") + "\n";

  write("example.ts", original);
  git("add", "example.ts");
  git("commit", "-q", "-m", "fixture");

  write("example.ts", original.replace("original line 1", "changed line 1"));

  const diff = await runtime.runPromise(withDiff((service) => service.getDiff));
  expect(diff.files).toHaveLength(1);

  const file = diff.files[0];
  expect(file.path).toBe("example.ts");
  expect(file.staged).toBe(false);
  expect(file.status).toBe("modified");
  expect(file.additions).toBe(1);
  expect(file.deletions).toBe(1);

  // The startup payload carries metadata only — no patch, no file contents.
  expect(file).not.toHaveProperty("patch");
  expect(file).not.toHaveProperty("oldContents");
  expect(file).not.toHaveProperty("newContents");

  const contents = await runtime.runPromise(
    withDiff((service) =>
      service.getFileContents(
        file.path,
        file.oldPath,
        file.status,
        file.staged,
      ),
    ),
  );

  // The unchanged region is present on both sides, which the patch alone would
  // not give the renderer.
  expect(contents.oldContents).toContain("UNCHANGED_MARKER");
  expect(contents.newContents).toContain("UNCHANGED_MARKER");
  expect(contents.oldContents).toContain("original line 1");
  expect(contents.newContents).toContain("changed line 1");

  await runtime.runPromise(
    withDiff((service) => service.stageFile(file.path, file.oldPath)),
  );

  const staged = await runtime.runPromise(
    withDiff((service) => service.getDiff),
  );
  expect(staged.files).toHaveLength(1);
  expect(staged.files[0].staged).toBe(true);
});

test("a path edited, staged, then edited again yields two rows", async () => {
  write("both.txt", "one\n");
  git("add", ".");
  git("commit", "-q", "-m", "fixture");

  write("both.txt", "two\n");
  git("add", "both.txt");
  write("both.txt", "three\n");

  const diff = await runtime.runPromise(withDiff((service) => service.getDiff));
  expect(diff.files).toHaveLength(2);
  // Staged first, per the sort.
  expect(diff.files.map((file) => file.staged)).toEqual([true, false]);
  expect(diff.files.map((file) => file.id)).toEqual([
    "staged:both.txt",
    "unstaged:both.txt",
  ]);
});

test("the commit-message diff includes every worktree half", async () => {
  write("staged.txt", "before staged\n");
  write("unstaged.txt", "before unstaged\n");
  git("add", ".");
  git("commit", "-q", "-m", "fixture");

  write("staged.txt", "after staged\n");
  git("add", "staged.txt");
  write("unstaged.txt", "after unstaged\n");
  write("untracked.txt", "new untracked\n");

  const diff = await runtime.runPromise(
    withDiff((service) => service.commitMessageDiff),
  );

  expect(diff).toContain("### staged: staged.txt");
  expect(diff).toContain("+after staged");
  expect(diff).toContain("### unstaged: unstaged.txt");
  expect(diff).toContain("+after unstaged");
  expect(diff).toContain("### untracked: untracked.txt");
  expect(diff).toContain("+new untracked");
});

test("a rename is reported with the path it moved from", async () => {
  write("before.txt", "same contents\n");
  git("add", ".");
  git("commit", "-q", "-m", "fixture");
  git("mv", "before.txt", "after.txt");

  const diff = await runtime.runPromise(withDiff((service) => service.getDiff));
  const renamed = diff.files.find((file) => file.path === "after.txt");

  expect(renamed).toBeDefined();
  expect(renamed?.status).toBe("renamed");
  expect(renamed?.oldPath).toBe("before.txt");
  expect(renamed?.staged).toBe(true);
});

test("history returns commits newest first", async () => {
  write("a.txt", "one\n");
  git("add", ".");
  git("commit", "-q", "-m", "first commit");
  write("a.txt", "two\n");
  git("commit", "-q", "-a", "-m", "second commit");

  const commits = await runtime.runPromise(
    withHistory((service) => service.getHistory(10)),
  );

  expect(commits).toHaveLength(2);
  expect(commits[0].subject).toBe("second commit");
  expect(commits[1].subject).toBe("first commit");
  expect(commits[0].author).toBe("OnlyDiffs Test");
  expect(commits[0].authorEmail).toBe("onlydiffs@example.test");
  expect(commits[0].isMerge).toBe(false);
  expect(commits[0].shortHash.length).toBeGreaterThan(0);
});

test("a path escaping the repository is rejected before git sees it", async () => {
  write("a.txt", "one\n");
  git("add", ".");
  git("commit", "-q", "-m", "fixture");

  const exit = await runtime.runPromiseExit(
    withDiff((service) =>
      service.getFileContents("../outside.txt", null, "modified", false),
    ),
  );

  expect(exit._tag).toBe("Failure");
});
