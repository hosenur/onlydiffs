<p align="center">
  <img src="build/icon.png" alt="" width="128" height="128">
</p>

# onlydiffs

**Diff Driven Development With Parallel Claude Code Sessions**

Review what changed, then tell Claude what to do about it — without leaving the
diff.

## Why

You already read diffs before you commit. onlydiffs makes that the place you
work from: point at a line, say what's wrong, and the Claude Code session
running in that repository picks it up. The review *is* the interface.

## What you get

- **See the real change.** Staged, unstaged, and untracked side by side, never
  folded together — a file you edited, staged, then edited again shows as two
  separate patches, because that is what it is.
- **Whole files, not fragments.** Scroll the untouched parts of a file without
  leaving the diff.
- **Point instead of describe.** Click a line to cite it — `src/app.tsx:42` —
  and it goes straight into the message you're writing to Claude.
- **Talk to the session in that repo.** Send a message, get Claude's complete
  reply back in the sidebar.
- **Commit messages that read the whole diff.** Staged, unstaged, and untracked
  together, in one pass.
- **Every project you work on.** Open a repository by path, and it's one click
  away next time.

## Run it

```sh
bun install
bun run dev
```

Open a repository from the landing page, or jump straight into one:

```sh
ONLYDIFFS_REPO_PATH=~/code/my-project bun run dev
```

Commit-message generation needs a Groq key in the app's environment:

```sh
GROQ_API_KEY=gsk_... bun run dev
```

To message Claude Code, register the channel once and restart Claude inside the
repository you're reviewing:

```sh
bun run channel:setup
```

Build a distributable with `bun run dist`.

## Keys

| Key | Action |
| --- | --- |
| `r` | Refresh |
| `s` | Split / unified |
| `⌘` `b` | Toggle sidebar |
| `⌘` `enter` | Stage the current file |

---

Built with Electron, React, TanStack Router, and Effect.
