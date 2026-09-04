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
reference and what you typed, to a coding agent working in that repository.
Claude Code and Codex are both offered; the status bar says which of them has a
session, and the composer shows a picker when both do. Both are one-way, and
both refuse when nothing is running: a message goes to a session that is there
now, or it does not go.

**Claude Code** is reached through a
[channel](https://code.claude.com/docs/en/channels): an MCP server that pushes
messages into a running session. The server is the agent binary in its
`channel` mode, reached through the stable path `~/.onlydiffs/agent/current`.
The app sets all of this up itself on launch — it installs the agent it ships,
points the link at it, finds `claude` even from a Finder-launched bundle, and
registers the channel with `claude mcp add` if that has not been done — so a
downloaded app needs nothing but Claude Code. `bun run channel:setup` does the
same from a checkout without launching the app. Each session's server listens
on a socket under `~/.onlydiffs/claude-channels`; a send connects, writes, and
is done.

Registering is not enough. Channels are a research preview, and Claude Code
delivers channel messages only to a session started with the server named:

```sh
claude --dangerously-load-development-channels server:onlydiffs
```

`bun run claude` runs exactly that from a checkout; the composer shows the same
command whenever a session needs it. A session started as plain `claude` still
runs the server and silently drops everything it receives, so the app reads
Claude Code's own MCP log, the status bar says "session not connected" for
such a session, and the composer refuses to send to it.

**Codex** is reached through its shared app-server daemon, which is the only
process that can put a message in front of a running session. Start the daemon
once with `codex app-server daemon start`, and start sessions attached to it,
with the repository named:

```sh
codex --remote unix:// -C "$PWD"
```

`-C` is not optional: a `--remote` session started without it is filed under
the daemon's own working directory, and no repository matches that. A session
started as plain `codex` never registers with the daemon and cannot be reached.
In either case the status bar says "session not connected" and the composer
gives the command that reattaches it. A send asks the daemon which of its
loaded threads belong to the repository and queues the message on the newest,
the same way typing into the TUI does: an idle session starts on it at once, a
busy one takes it up when its turn ends.

On a host, the agent does all of this on the host: the socket, the daemon, and
the process table it checks first are all there, and connecting to a host
registers the channel with the Claude Code installed on it.

Images can be pasted into it. They do not travel in the message — the transport
carries text and a screenshot is megabytes of binary — so the image is
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
