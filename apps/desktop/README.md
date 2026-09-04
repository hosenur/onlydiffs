<p align="center">
  <img src="build/icon.png" alt="" width="128" height="128">
</p>

# onlydiffs

**The editor is over. The chat box was never it.**<br>
**See what your agent changed. Talk it through.**

## Why

You already read diffs before you commit. onlydiffs makes that the place you
work from — the whole repository on the left, the real change on the right.
The review *is* the interface.

## What you get

- **See the real change.** Staged, unstaged, and untracked side by side, never
  folded together — a file you edited, staged, then edited again shows as two
  separate patches, because that is what it is.
- **Whole files, not fragments.** Scroll the untouched parts of a file without
  leaving the diff.
- **The whole repository, not just what changed.** A file tree with changed
  files marked in place, so you keep your bearings.
- **Every project you work on.** Open a repository by path, and it's one click
  away next time.

## Run it

```sh
bun install
bun run dev
```

The Rust toolchain is a prerequisite — see
[Tauri's setup guide](https://tauri.app/start/prerequisites/).

Open a repository from the landing page, or jump straight into one:

```sh
ONLYDIFFS_REPO_PATH=~/code/my-project bun run dev
```

Build a distributable with `bun run dist`.

### Groq

When `GROQ_API_KEY` is set, the background project-icon resolver shortlists a
repository's own artwork — app icons, logos, favicons — and shows the best three
to Groq's Qwen 3.6 vision model, which names one or declines them all. Three is
the most images that model accepts in a request. The chosen thumbnail and its
repository-relative source path stay in `~/.onlydiffs/projects.json`, keyed by a
hash of the shortlist so an unchanged repository is never re-sent. Projects with
no artwork, or none the model would use, keep the cube fallback.

### Talking to a session

Clicking a line in a diff opens a composer for it. What it sends is the line
reference and what you typed, over the channel `bun run channel:setup`
registers, to the Claude Code session running in that repository.

Images can be pasted into it. They do not travel in the message — the channel
carries 64 KB of text and a screenshot is megabytes of binary — so the image is
written to the repository's git directory and the message names the path
instead. That is the only form that works for a project on a host, where the
session is a process on the far side of an SSH connection: the bytes cross once,
the agent writes them down there, and the path it hands back is one that machine
can open. `.git` rather than the working tree, so a pasted screenshot is never
mistaken for a change to review; pastes older than a week are dropped the next
time one is written.

## Keys

| Key | Action |
| --- | --- |
| `r` | Refresh |
| `s` | Split / unified |
| `⌘` `b` | Toggle sidebar |
| `⌘` `enter` | Stage the current file |

---

Built with Tauri, Rust, React, and TanStack Router.
