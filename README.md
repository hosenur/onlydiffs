# cashew

An Electron desktop app for reviewing git diffs and generating a commit message.

It shows every change in the repo — staged, unstaged, and untracked — rendered
with [`@pierre/diffs`](https://diffs.com/).

Staged and unstaged edits are kept apart rather than folded together: the index
is diffed against `HEAD` (`git diff --cached`) and the working tree against the
index (`git diff`). A file edited, staged, then edited again therefore appears
as two rows showing two different patches.

## Stack

- Electron 44 + React 19 + Vite (via [`electron-vite`](https://electron-vite.org/))
- [Effect](https://effect.website/) for the main process: every command is an
  `Effect`, the git runner and the repo config are `Layer`s, and failures are
  tagged errors rather than strings. See [Main process](#main-process).
- [`@pierre/diffs`](https://diffs.com/) for diff rendering (Shiki-based)
- Tailwind CSS v4 (`@tailwindcss/vite`, no config file — theme lives in `src/index.css`)
- Colors from [Oscura](https://github.com/narative/oscura) by Fey (MIT), mapped
  onto Intent's existing token names — no parallel palette, no extra variables.
  `:root` is **Dusk**, `.dark` is **Midnight**; the two blocks carry an identical
  47-variable list and differ only in the background (`#131419` / `#0b0b0f`),
  which is the only value that differs between the two upstream themes.

  Oscura has no light variant, so the app is dark either way — the OS setting
  picks between the two dark variants rather than between light and dark.
  `color-scheme: dark` is set in both blocks accordingly.
- [Intent UI](https://intentui.com/) as the component library — React Aria
  Components + `tailwind-variants`, vendored into `src/components/ui/`:

  ```sh
  bunx --bun shadcn@latest init @intentui/theme-default   # once
  bunx --bun shadcn@latest add @intentui/<component>
  ```

  The `@intentui` namespace is wired up in `components.json`. Intent's design
  tokens (`--bg`, `--fg`, `--muted-fg`, `--primary-subtle`, …) are defined on
  `:root` / `.dark` in `src/index.css`, and the app chrome uses them directly
  rather than keeping a parallel palette.

  Intent switches theme on a `.dark` class, not `prefers-color-scheme`, so
  `src/components/theme-provider.tsx` (Intent's official Vite provider) applies
  it. It defaults to `system` and — unlike the upstream copy — subscribes to
  appearance changes, since a desktop window outlives them.

  Icons come from three places, each vendored rather than imported at runtime:

  - **Nucleo** (`src/icons/`) — git glyphs in the history sidebar, copied from
    the local Nucleo install. No package, no dependency.
  - **Material Icon Theme** (MIT) — file-type icons in the left sidebar.
    `bun run sync:icons` copies the ~590 reachable icons into
    `public/file-icons/` and writes `src/lib/file-icon-map.json`; the package
    itself is a devDependency and never ships. `src/lib/file-icon.ts` resolves
    a path the way the theme does — exact filename first, then the longest
    matching extension, so `app.d.ts` lands on `typescript-def`, not
    `typescript`.
  - **Heroicons** — the handful of generic UI glyphs left over.

- [TanStack Router](https://tanstack.com/router) with file-based routing
  (`@tanstack/router-plugin/vite` generates `src/routeTree.gen.ts`)
- [TanStack Hotkeys](https://tanstack.com/hotkeys) for keyboard shortcuts
  (`Mod` is ⌘ on macOS and Ctrl elsewhere)

## Routing

The sidebar is a **pathless layout route**. `src/routes/_app.tsx` is prefixed
with `_`, so it contributes no URL segment — it only wraps its children:

```
src/routes/
├── __root.tsx        →  (root)
├── _app.tsx          →  no URL of its own; sidebar + nav + <Outlet/>
├── _app.index.tsx    →  /
└── _app.file.$.tsx   →  /file/*   (splat: file paths contain slashes)
```

Generated route IDs are `/_app`, `/_app/` and `/_app/file/$`, but the public
paths are just `/` and `/file/$`. Because both pages live under the same
layout, the sidebar and its loader data survive navigation between them.

`_app.tsx` owns the data: its `loader` calls the `get_diff` command once, and
children read it with `getRouteApi('/_app').useLoaderData()`. Refresh is
`router.invalidate()`.

Two integration notes:

- **Hash history.** The packaged app is loaded from `file://`, where pushState
  routing has no server to fall back on, so `main.tsx` uses
  `createHashHistory()`.
- **Links.** React Aria renders `href` as a plain `<a>`, which would reload the
  webview. `src/components/ui/link.tsx` uses React Aria's `render` prop to swap
  in TanStack's `Link`. Intent documents `createLink` for this, but that makes
  `to` a required prop and stops their own `sidebar.tsx` from compiling; the
  `render` approach keeps the `href` API, so no Intent component needs editing.

## Scope

The repository under review comes from `CASHEW_REPO_PATH` in the Electron
process environment, read once by `electron/main/services/repo-config.ts`:

```sh
CASHEW_REPO_PATH=/Users/you/some/repo bun run dev
```

Without it, the path the Tauri build had compiled in is used
(`/Users/rahaman/Developer/minwinn`). A relative value is resolved against the
launch directory and `~` is expanded, so unlike the Rust constant it does not
have to be absolute. There is no repo picker yet.

## How it works

- Two sidebars. The left one is collapsible (`⌘B`); the right is
  `collapsible="none"`, which renders as a plain flex child so it keeps its own
  state and the shortcut only affects the left. The right sidebar is split in
  half: the top sends messages to the repository's Claude Code session and
  generates an editable commit message; the bottom lists branch history from
  `get_history`.
- `getDiff` shells out to `git status --porcelain -z -uall`, then walks the
  porcelain `XY` pair: `X` (index) becomes a staged row via `git diff --cached`,
  `Y` (working tree) an unstaged row via `git diff`. Untracked files use
  `git diff --no-index -- /dev/null <path>`. The per-file patches are collected
  with bounded concurrency rather than one at a time.
- `getDiff` returns change metadata for a lightweight startup. Diff cards load
  only as they approach the viewport; then `getFileContents` lazily reads the
  complete old and new versions. Only after both versions arrive does the
  frontend build a full `<FileDiff>`, so every unchanged region is present and
  additions and deletions remain highlighted.

## Copying a line reference

Clicking a **green (added) line** copies `<path>:<line>` to the clipboard and
appends it to the Claude message draft — e.g. `apps/web/src/pages/_app.tsx:42`.
The path is repo-relative, and the line number is the one in the file's current
state.

Only added lines respond. Deleted lines don't exist in the file any more, so
their numbers point at nothing you could open. Context lines are ignored too,
though they do carry valid current-state numbers if that's ever wanted.

Writing goes through the main process (`clipboard.writeText`) rather than
`navigator.clipboard`, which needs a secure context — `file://` is not one.

## Generating a commit message

The right sidebar sends the complete staged, unstaged, and untracked diff to
Groq's `openai/gpt-oss-120b` model. The request runs in the Electron main
process, not the Vite bundle, and reads `GROQ_API_KEY` from that process's
environment. Launch development with the variable exported or inline:

```sh
GROQ_API_KEY=gsk_... bun run dev
```

A Vite `.env` file is not loaded into Rust automatically. Do not use a
`VITE_GROQ_API_KEY` variable; Vite would expose it to the frontend bundle.

## Messaging Claude Code

Cashew includes a two-way Claude Code development channel. Register its MCP
server once from the Cashew repository:

```sh
bun run channel:setup
```

Then exit and restart Claude Code inside the repository Cashew is viewing:

```sh
cd /Users/rahaman/Developer/minwinn
claude --dangerously-load-development-channels server:cashew
```

Claude Code shows a development-channel confirmation on startup. Once
accepted, the message input in the right sidebar forwards messages into that
running conversation. After Claude finishes its work, the channel's `reply`
tool returns the complete response to the app at once; partial token output is
not streamed. The channel binds a random loopback port and publishes a
per-process bearer token in a user-only `~/.cashew/claude-channels/`
registration file. Cashew only targets a channel whose working directory
matches the repository under review.

The reply endpoint is a long poll. The channel server drops an idle connection
before a real Claude turn finishes, so the client re-opens the poll until the
reply lands or its own ten-minute budget runs out; the pending reply outlives
the connection either way.

Channels are currently a Claude Code research preview. The setup command stores
the channel script's absolute path in the user MCP configuration, so run it
again if this repository moves.

## Selecting text in a file

On the single-file view, selecting text opens a find toolbar over the
selection: how many times that string occurs in the current file, plus next /
previous. Removed (red) lines are skipped, so the count is the working-tree
file, not the old side of the diff.

The same selection copies a line reference — `path:42`, or `path:42-50` when
the selection spans more than one line.

## Keys

| Key         | Action                                              |
| ----------- | --------------------------------------------------- |
| `r`         | Refresh                                             |
| `s`         | Toggle split / unified                              |
| `⌘` `b`     | Toggle sidebar                                      |
| `⌘` `enter` | Stage the current file (single-file view only)      |

## Main process

Everything that used to be `src-tauri/src/lib.rs` now lives in `electron/`,
written with Effect:

```
electron/
├── shared/contract.ts     →  domain types, channel names, the window bridge
├── preload/index.ts       →  contextBridge, the only thing on `window`
└── main/
    ├── index.ts           →  app + BrowserWindow
    ├── runtime.ts         →  the ManagedRuntime the handlers run in
    ├── ipc.ts             →  one handler per channel
    ├── errors.ts          →  the tagged errors, and how they cross IPC
    └── services/
        ├── repo-config.ts →  CASHEW_REPO_PATH
        ├── git.ts         →  runs git through a CommandExecutor
        ├── diff.ts        →  getDiff, getFileContents, stageFile
        ├── history.ts     →  getHistory
        ├── commit-message.ts → the Groq call
        └── claude-channel.ts → the Claude Code bridge
```

Three things that follow from using Effect rather than plain promises:

- **Errors are values with tags.** `GitError`, `InvalidPathError`,
  `ClaudeChannelError` and the rest are `Data.TaggedError`s. A handler answers
  with `{ ok: false, error: { _tag, message } }` instead of rejecting, so the
  tag survives the process boundary — rejecting would stringify the cause and
  prefix it with "Error invoking remote method". `src/lib/ipc.ts` turns that
  back into a typed `IpcError` on the renderer side.
- **Payloads are decoded, not trusted.** Every channel that takes an argument
  runs it through a `Schema` before a service sees it, so a compromised
  renderer cannot hand an arbitrary shape to `git`. Paths are separately
  checked for `..` and absolute prefixes.
- **`git` is a swappable dependency.** It runs through `@effect/platform`'s
  `CommandExecutor`, provided by `NodeContext.layer`. The tests build the same
  layers against a throwaway repository.

Both stdout and stderr are drained concurrently with the exit-code wait — a
patch larger than the OS pipe buffer would otherwise deadlock the child.

## Develop

```sh
bun install
bun run dev          # electron-vite dev, with HMR for the renderer
bun test electron    # the service tests, against real temp repositories
bun run typecheck    # main/preload and renderer are separate TS projects
bun run build        # typecheck + tests + bundle into out/
bun run dist         # electron-builder → release/
```

`bun install` does not run Electron's postinstall on every setup; if
`bun run dev` fails with `Error: Electron uninstall`, fetch the binary once
with `node node_modules/electron/install.js`.
