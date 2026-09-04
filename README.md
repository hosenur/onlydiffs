<p align="center">
  <img src="apps/desktop/build/icon.png" alt="" width="128" height="128">
</p>

# onlydiffs

**The editor is over. The chat box was never it.**<br>
**See what your agent changed. Talk it through.**

A Turborepo holding the desktop app, its docs and blog site, and the Intent UI
components they share.

```
apps/desktop     Tauri · Rust · Vite · TanStack Router
apps/web         Next.js docs and blog
packages/ui      Intent UI components used by both
```

## Run it

```sh
bun install
bun run dev              # both apps
bun run dev:desktop      # just the desktop app
bun run dev:web          # just the site
```

Point the desktop app at a repository, or open one from its landing page:

```sh
ONLYDIFFS_REPO_PATH=~/code/my-project bun run dev:desktop
```

`bun run build` builds everything, `bun run test` runs the suites,
`bun run typecheck` covers all three workspaces, and `bun run dist` packages
the desktop app.

## The shared package

`@onlydiffs/ui` ships TypeScript source rather than a build — Vite compiles it
natively, and Next is told to via `transpilePackages`.

Two things are deliberately **not** in it. `link` and `menu` reach for the
app's router — `next/link` in web, TanStack Router in desktop — and a component
that needs a router cannot be shared without inverting that dependency;
`command-menu` and `text` are excluded for depending on them in turn. Each app
keeps its own copy of those four.

React, React DOM, and `react-aria-components` are pinned by `overrides` in the
root manifest. A second copy of any of them makes the same component's types
incompatible across the package boundary, and the resulting error names private
fields rather than the duplication that caused it.

## Reviewing a repository on another machine

`⌃` `⌘` `O` opens the remote picker: the hosts you have added, the projects you
have opened on each, and **Connect SSH server** at the top. It is also a row in
`⌘` `K` and a link on the landing page.

Adding a host takes **the command you already use** — `ssh user@example -p 2222`
— not just a name. The options in it are kept and replayed on every later
connection, so a host on a non-standard port or behind a jump box works without
editing `~/.ssh/config`. Anything your ssh can reach works, since this runs your
ssh rather than reimplementing it: aliases, `ProxyJump`, hardware keys,
certificate auth, `Match` blocks.

What happens on the first connect:

1. `ssh -G` resolves the destination through your own config.
2. An existing `ControlMaster` is adopted if you have one, so a host behind a
   hardware key is touched once rather than once per project.
3. A host with no key in `known_hosts` is a question with a fingerprint in it,
   not a silent `accept-new`. Approving writes to your real `~/.ssh/known_hosts`.
4. A small agent is uploaded to `~/.onlydiffs/agent/` on the host and run over
   the same connection. Everything after that — the diff walk, the file list,
   the history, the watcher, the icon scan — happens where the repository is,
   so opening a five-hundred-file diff is one round trip rather than five
   hundred.

Passphrase and password prompts appear as dialogs: the app points `SSH_ASKPASS`
at itself and gives ssh no stdin, so nothing can block on a terminal that a
bundle does not have.

The Groq key never crosses the connection. Commit messages and icon choices are
decided here, from inputs collected there.

**Agent sessions follow the repository.** A session reviewing a checkout on a
build box is a process on that build box, listening on that machine's loopback,
with a registration naming a path in that machine's filesystem — so the agent
reads the host's `~/.onlydiffs/claude-channels`, matches the registration
against the repository root *there*, and posts to loopback *there*. Reading the
registry from your Mac would find nothing, and finding something would be
worse: it would mean sending a line reference to a session that has never seen
that repository.

Which means the channel has to be set up on the host, in that checkout — the
same `channel:setup` this repo documents, run over there. Nothing about the
setup changes; it just has to be on the machine the code is on.

Codex works the same way for the same reason, by a different route. It has no
listener; it has a queue, and the thread to queue against is found by reading
the transcripts under `~/.codex/sessions` — an index of what has run on *that*
machine. So the agent reads it there, matches on the repository root there, and
runs `codex queue` there. The message is kept until the session next takes a
turn, so unlike the Claude channel it does not need one to be open.

Images pasted into the composer follow the repository for the same reason. The
bytes cross the connection once, the agent writes them into the repository's git
directory *there*, and what the message carries is the path they landed at —
which is a path the session can open, and which a path on your Mac would not
be.

Supported hosts are Linux on x86_64 or aarch64 and macOS on either
architecture. Anything else is refused by name rather than guessed at.

## Settings

`~/.onlydiffs/config.json` holds what you set in the app — today that is the
Groq API key, which powers commit-message generation and project icons. Open
Settings with `⌘` `,`, the gear in the project rail, or the command palette.
The file is written `0600`, and the key never crosses to the renderer: the page
shows a masked hint and writes a replacement.

`ONLYDIFFS_STATE_DIR` moves that file and the recents list beside it.

## Environment

`turbo.json` declares `globalPassThroughEnv`. Turbo strips anything not listed
there, so a variable the desktop app reads at runtime has to be named or it
arrives unset with no warning.

| Variable | Effect |
| --- | --- |
| `ONLYDIFFS_REPO_PATH` | Opens this repository at startup, skipping the landing page |
| `ONLYDIFFS_STATE_DIR` | Where the recents list and settings live (default `~/.onlydiffs`) |
| `GROQ_API_KEY` | Fallback for the key in Settings |
| `ONLYDIFFS_AGENT_GLIBC` | glibc floor for the cross-compiled agents (default `2.17`) |

A saved key wins over `GROQ_API_KEY`; clearing it hands the app back to the
environment. And a bundle launched from Finder, Spotlight, or the Dock inherits
launchd's environment, which never sources `.zshrc` — so when `GROQ_API_KEY` is
missing from the process the app asks your login shell (`$SHELL -ilc`) for it
once, on the first Groq call of a launch.

## The remote agent

`src-tauri` is a Cargo workspace of three crates:

| Crate | What it is |
| --- | --- |
| `onlydiffs` | The Tauri app. Windows, IPC, Groq, SSH, settings. |
| `onlydiffs-core` | Everything that runs *where the repository is*. No `tauri` dependency, and the manifest is what keeps it that way. |
| `onlydiffs-agent` | `onlydiffs-core` on a host, speaking the protocol on stdio. |

Build the agents before a local release build, or the bundle ships without them:

```sh
cd apps/desktop
./scripts/build-agents.sh     # needs zig and cargo-zigbuild
```

Linux targets link through `cargo-zigbuild` against glibc 2.17, which is
CentOS 7 — old enough for the build servers people actually have. Not musl: a
static binary would be simpler and would break `getaddrinfo` and NSS, which the
Claude channel needs.

The SSH tests run against a real `sshd` they start themselves. There is a second,
opt-in suite that runs against a real Linux host in Docker, which is the only
thing that proves a cross-compiled agent runs somewhere this Mac is not:

```sh
./scripts/linux-test-host.sh up
ONLYDIFFS_LINUX_HOST_DIR=$PWD/.linux-test-host \
  cargo test --manifest-path src-tauri/Cargo.toml --test linux_host
./scripts/linux-test-host.sh down
```

## Releasing

Tagging is the release. The version lives in five manifests, and CI refuses a
tag that disagrees with any of them:

```sh
# tauri.conf.json, package.json, and all three Cargo.toml files, then:
git tag v0.1.1 && git push --tags
```

That builds `aarch64-apple-darwin` on GitHub Actions and opens a **draft**
release. Publishing the draft is what makes `latest.json` reachable, and that
file is what an installed copy looks for a few seconds after launch: a waiting
release is named in the status bar and installs from `⌘` `k`.

Updates are signed with a key that is not Apple's and not GitHub's. Generate it
once with `bun tauri signer generate`, keep the public half in `tauri.conf.json`
and the private half in the `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets. **Lose the private
half and no installed copy can ever be updated again** — there is no recovery
path but a fresh download.

That key is required locally too. `bun run dist` now emits an update artifact,
and the bundler will not leave one unsigned:

```sh
TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/onlydiffs.key)" \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD=… bun run dist
```

### The download is not notarized yet

No Apple Developer ID sits behind these builds. CI gives the complete `.app`
bundle a valid ad-hoc seal, but Gatekeeper does not trust ad-hoc identities. A
`.dmg` downloaded in a browser may therefore open to *"Apple could not verify
onlydiffs"* or *"onlydiffs is damaged and can't be opened."* After dragging the
app into Applications, remove the browser's quarantine flag:

```sh
xattr -dr com.apple.quarantine /Applications/onlydiffs.app
```

This is required only for the first install. The updater writes later versions
without a quarantine flag. CI also runs strict `codesign` verification against
the app and the archived updater bundle before a draft can be published.

## Keys

| Key | Action |
| --- | --- |
| `⌘` `k` | Command menu |
| `r` | Refresh |
| `s` | Split / unified |
| `⌘` `b` | Show or hide the sidebar |
| `⌘` `enter` | Stage the current file |

---

Built with Tauri, Rust, Next.js, React, and TanStack Router.
