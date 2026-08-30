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

The Claude Code channel and the Groq commit-message generator are implemented
end to end, but the review UI does not call either yet. `bun run channel:setup`
still registers the channel.

## Keys

| Key | Action |
| --- | --- |
| `r` | Refresh |
| `s` | Split / unified |
| `⌘` `b` | Toggle sidebar |
| `⌘` `enter` | Stage the current file |

---

Built with Tauri, Rust, React, and TanStack Router.
