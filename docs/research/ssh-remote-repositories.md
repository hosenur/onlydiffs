# Reviewing a repository over SSH

Research date: 2026-09-03
Status: **built.** Phases 0–5 are implemented; see `README.md` for how to use
it and the notes at the end of this file for what the build changed about the
plan.

How Zed, VS Code, and T3 Code let a local UI drive a repository that lives on
another machine, what that costs, and the end-to-end plan for doing it in
OnlyDiffs.

## Recommendation

Ship a **remote agent**: a small `onlydiffs-agent` binary that OnlyDiffs uploads
to the host and speaks to over a single multiplexed SSH connection. Every
question about the repository — the diff, the file list, the history, the
watcher, the icon candidates — is answered on the machine the repository is on,
and one answer comes back.

The alternative, running `ssh host git …` once per git invocation, is a smaller
change and a worse product. Opening one repository today costs **three git
invocations plus one per changed file**, eight at a time
(`diff.rs:13`, `PATCH_CONCURRENCY`). At 40ms of transatlantic round trip that is
a quarter-second of pure latency on a ten-file diff and two seconds on a
hundred-file one, before git has done any work — and it still leaves the file
watcher with nothing to watch, which is the feature that makes this app feel
alive when an agent is editing.

All three prior-art projects reached the same conclusion, independently.

## Prior art

### Zed

Zed's UI runs locally; **source code, language servers, tasks, and the terminal
run on the remote host** ([Zed remote development docs](https://github.com/zed-industries/zed/blob/main/docs/src/remote-development.md)).

The parts worth copying:

- **It shells out to the system `ssh`.** Not a Rust SSH library. That is what
  buys `~/.ssh/config`, `Include`, `ProxyJump`, `Match` blocks, hardware keys,
  `known_hosts`, and every corporate SSH setup for free.
- **One ControlMaster per project, and it reuses yours.** `MasterProcess::new`
  spawns `ssh -N -o ControlPersist=no -o ControlMaster=yes -o ControlPath=…`
  ([`crates/remote/src/transport/ssh.rs:180`](https://github.com/zed-industries/zed/blob/main/crates/remote/src/transport/ssh.rs)).
  Before doing that it runs `ssh -G <destination>` to resolve the user's
  effective `controlpath`, then `ssh -O check` against it, and adopts a live
  master rather than authenticating again — added in response to
  [issue #45271](https://github.com/zed-industries/zed/issues/45271).
- **Passwords go through `SSH_ASKPASS`.** The master process is spawned with
  `stdin(Stdio::null())`, `SSH_ASKPASS_REQUIRE=force` and `SSH_ASKPASS` pointing
  at a generated script, so a passphrase or 2FA prompt surfaces as a GUI dialog
  instead of deadlocking on a terminal that isn't there.
- **The wire format is boring on purpose**: a 4-byte little-endian length prefix
  followed by a protobuf `Envelope`
  ([`crates/remote/src/protocol.rs`](https://github.com/zed-industries/zed/blob/main/crates/remote/src/protocol.rs)).
- **The remote binary is version-matched by filename.** `ensure_server_binary`
  builds `zed-remote-server-{channel}-{version}`, runs it with `version` to see
  whether that exact file already works, and only then downloads or uploads
  (`ssh.rs:833`). By default the host downloads it from zed.dev;
  `upload_binary_over_ssh: true` sends it over `scp -C` instead, for hosts with
  no outbound internet.
- **Reconnection is the agent's job, not the connection's.** The remote binary
  runs in `proxy` mode with `--identifier` and `--reconnect`; proxy mode starts
  the daemon if it isn't running and re-attaches if it is, and exits with code
  90 (`ProxyLaunchError::ServerNotRunning`) so the client can tell "your session
  is gone" from "ssh failed"
  ([`crates/remote/src/proxy.rs`](https://github.com/zed-industries/zed/blob/main/crates/remote/src/proxy.rs)).

### VS Code Remote-SSH

Same shape, different plumbing. The extension installs a **VS Code Server** on
the host over SSH; the local window talks to it through an SSH tunnel, and
"commands, IntelliSense, debugging, and most extensions run on the remote
machine" ([Remote Development using SSH](https://code.visualstudio.com/docs/remote/ssh)).

Two details OnlyDiffs should steal:

- **The server binds loopback on a random port with a random key**, and that
  port is forwarded to the client. Nothing on the remote host's network can
  reach it, and the key is stored mode-600 on the remote disk
  ([Remote Development Tips and Tricks](https://code.visualstudio.com/docs/remote/troubleshooting)).
- **Two connections per window**: the first installs or finds the server, the
  second is the tunnel. Separating "bootstrap" from "session" is what makes the
  bootstrap failures legible.

### T3 Code

The closest analogue to OnlyDiffs — an agent-review workspace, Electron-shaped,
that added SSH after the fact. Its desktop app "probes the host, starts or
reuses a remote T3 server, opens a local port forward, and saves the
environment", and the renderer then talks to a **local forwarded HTTP/WebSocket
endpoint** while "the remote host still owns the actual T3 server, projects,
files, git state, terminals, and provider sessions"
([`docs/user/remote-access.md`](https://github.com/pingdotgg/t3code/blob/main/docs/user/remote-access.md)).

Its base SSH arguments are three lines and worth copying verbatim
([`packages/ssh/src/command.ts:102`](https://github.com/pingdotgg/t3code/blob/main/packages/ssh/src/command.ts)):

```
-o BatchMode={yes|no}  -o ConnectTimeout=10  [-p PORT]
```

`BatchMode=yes` for probes that must not hang on a prompt, `no` for the one
connection allowed to ask for a password. It also resolves the target through
`ssh -G` and parses `hostname` / `user` / `port` out of it, rather than trying
to parse `~/.ssh/config` itself.

**The failure mode T3 Code documents at length is the one OnlyDiffs already
knows.** Their SSH launcher uses a non-interactive `sh`, and their single
biggest support burden is `node: command not found` — because version managers
initialise from interactive shell profiles. This repository solved the mirror
image of that problem in `services/shell_env.rs` when a Finder-launched bundle
had no `GROQ_API_KEY`: ask a login shell, once, and cache it. The remote
bootstrap needs the same trick in the same shape, and should reuse the thinking.

### JetBrains Gateway

Worth one line for the contrast: Gateway runs the *entire IDE backend* remotely
and the local side is a thin client. That is the far end of the spectrum, and it
is more than OnlyDiffs needs — the diff rendering, the syntax highlighting, and
the Groq calls are all cheap and all fine locally.

## What SSH touches in this codebase

The good news first. **Every git invocation in the app goes through one
function** — `git::run_in(cwd, args)` in `services/git.rs:18`. Eighteen call
sites across four services, and nothing else spawns git:

| Service | Uses | Needs remoting |
| --- | --- | --- |
| `diff.rs` | `status --porcelain -z -uall`, `rev-parse`, `log -1`, `diff` ×N at concurrency 8, `show`, `add -A`, `commit` | Yes — via the seam |
| `file_tree.rs` | `ls-files -co --exclude-standard -z` | Yes — via the seam |
| `history.rs` | `log` | Yes — via the seam |
| `project_icon.rs` | `ls-files`, plus `std::fs::read` of image files and decoding (`project_icon.rs:224`) | Yes — git *and* file reads |
| `watcher.rs` | `notify` on the repo root, `.gitignore` and `.git/info/exclude` | Yes — the hard one |
| `workspace.rs` | `is_dir()`, `.git` existence, recents file | Split: probes remote, recents stay local |
| `claude_channel.rs` | `~/.onlydiffs/claude-channels`, loopback HTTP POST | Yes — the session lives on the host |
| `commit_message.rs` | diff via the seam, then HTTPS to Groq | No — the key stays local |
| `settings.rs`, `updater.rs` | Local config, local app updates | No |

So the work divides into three genuinely different problems, and only the first
one is small:

1. **Command execution.** One seam, already there.
2. **File reads.** Two callers (`project_icon.rs`, and `.gitignore` for the
   watcher). Both want bytes off the remote disk.
3. **Change notification.** No seam at all. `notify` watches a local path, and
   there is no version of that which works across a network without something
   running on the far side.

Problem 3 is the one that decides the architecture. Everything else could limp
along on `ssh host git …`; the watcher cannot.

## Architecture

```
┌─ local ────────────────────────────┐        ┌─ remote host ──────────────┐
│  Tauri renderer                    │        │                            │
│      ↕ IPC (unchanged)             │        │  onlydiffs-agent           │
│  AppState                          │        │    ├── git (native)        │
│    ├── Workspace   (local recents) │        │    ├── notify watcher      │
│    ├── Settings    (local, 0600)   │        │    ├── file reads          │
│    ├── Repository ──────────────────────────┼──▶ └── icon scan + resize  │
│    │     Local | Remote            │  stdio │                            │
│    ├── commit_message → Groq ──────┼─▶ HTTPS│                            │
│    └── updater     (local)         │        │                            │
└────────────────────────────────────┘        └────────────────────────────┘
                        │
              ssh -o ControlPath=… host  onlydiffs-agent serve
```

One `ssh` process per project, carrying one stdio stream, framed. Not a port
forward: OnlyDiffs has no browser to point at a forwarded HTTP port the way T3
Code does, and stdio avoids binding anything on the remote host at all —
strictly less exposed than VS Code's loopback-plus-random-key.

### The seam

```rust
/// Everything the app needs from the machine a repository lives on. `Local`
/// spawns processes and reads files; `Remote` sends the same requests down one
/// SSH connection to an agent that does exactly that on the other side.
#[async_trait]
pub trait Repository: Send + Sync {
    async fn git(&self, cwd: &Path, args: &[&str]) -> Result<String, AppError>;
    async fn read_file(&self, path: &Path, max_bytes: u64) -> Result<Vec<u8>, AppError>;
    async fn metadata(&self, path: &Path) -> Result<Option<FileMeta>, AppError>;
    async fn watch(&self, root: &Path) -> Result<BoxStream<'static, ()>, AppError>;
    async fn icon_candidates(&self, root: &Path) -> Result<Vec<Candidate>, AppError>;
}
```

`icon_candidates` is on the trait rather than being composed from `git` and
`read_file` on purpose: the current implementation reads up to 32 files at 4MB
each and downscales them to 256px thumbnails (`project_icon.rs`). Doing that
remotely and sending back three ~20KB PNG data URLs is the difference between
40KB and 128MB on the wire. The decision about *which* candidate wins still
happens locally, because that is the call that needs the Groq key, and the key
must never leave the user's machine.

### The wire

Length-prefixed frames over the agent's stdin/stdout, exactly as Zed does it —
4-byte little-endian length, then the body. Body encoding: **CBOR via `ciborium`**
rather than protobuf. Zed needs protobuf because the same `Envelope` type also
crosses its collab server; OnlyDiffs has one client and one agent, both built
from this repo at the same version, and `serde` derives on the existing
`contract.rs` types cost nothing. If the protocol ever needs to outlive a
version boundary, that is the moment to reach for something with a schema.

Requests carry a `u32` id; responses echo it. Watcher events are unsolicited
frames with id 0. That is the whole protocol.

`stderr` stays a separate pipe and is captured as agent logs — never parsed,
because a shell profile that prints a banner will end up there and must not be
able to corrupt a frame.

### The agent

A second binary in the same Cargo workspace, sharing `contract.rs`, `diff.rs`,
`file_tree.rs`, `history.rs`, `watcher.rs`, and the candidate-scanning half of
`project_icon.rs` with the desktop app. That sharing is the point: there is no
second implementation of the diff parser to drift.

- **Location**: `~/.onlydiffs/agent/onlydiffs-agent-{version}-{arch}`, matching
  the state directory the app already owns (`services/mod.rs::state_dir`).
- **Version match by filename**, Zed's approach: run `…-{version} --version`,
  and if that exact file executes and agrees, it is the right binary. No
  handshake negotiation, no compatibility matrix.
- **Upload over SSH**, not download from a CDN. Zed defaults to downloading
  because it has zed.dev and users on fast hosts; OnlyDiffs has neither a CDN
  nor the volume to want one, and `scp -C` of a ~6MB stripped binary is a
  one-time cost per version per host. This also means the feature works on hosts
  with no outbound internet, which is the interesting case (jump hosts, air-gapped
  build boxes).
- **Lifetime**: the agent exits when its stdin closes. No daemon, no pidfile, no
  orphan. Zed needs a daemon because it holds CRDT buffer state across
  reconnects; OnlyDiffs holds nothing a reconnect cannot rebuild in one
  `git status`.

### Connecting

The bootstrap sequence, each step failing with its own message:

1. `ssh -G <target>` → resolve `hostname`, `user`, `port`, `controlpath`.
   Never parse `~/.ssh/config` by hand.
2. If `controlpath` is set, `ssh -O check` it. Alive → adopt it and skip
   authentication entirely (Zed's #45271 fix).
3. Otherwise spawn our own master: `ssh -N -o ControlMaster=yes -o
   ControlPersist=no -o ControlPath=<our socket>`, with `stdin` null,
   `SSH_ASKPASS_REQUIRE=force`, and `SSH_ASKPASS` pointing at a script we wrote
   that pipes the prompt to a Tauri dialog over a unix socket.
4. Probe: `ssh … 'command -v git && git --version && uname -sm'` with
   `BatchMode=yes`. This is where "no git on the remote", "unsupported
   platform", and "login shell prints a banner" get diagnosed, separately.
5. Ensure the agent binary (above).
6. `ssh … 'exec ~/.onlydiffs/agent/onlydiffs-agent-… serve'`, `-o
   ServerAliveInterval=15 -o ServerAliveCountMax=3`, and start framing.

`exec` matters in step 6: without it the shell stays in the process tree and
signals reach the wrong process.

## Plan

Six phases. Each one ends somewhere shippable, and phases 1–2 are useful on
their own for anyone who already has a working `ssh` config.

### Phase 0 — The seam, local only

Introduce `Repository` with a single `LocalRepository` implementation that does
exactly what the code does today. Move `read_file`, `metadata`, and
`icon_candidates` behind it. `AppState` holds `Arc<dyn Repository>`.

Ships nothing user-visible. Every existing test still passes, and the twelve
tests in `project_icon.rs` and the integration tests in `tests/` become the
regression net for everything after it.

**Done when**: `git.rs` has no callers outside `LocalRepository`, and
`cargo test` is green.

### Phase 1 — The SSH connection

`SshConnection`: target resolution via `ssh -G`, ControlMaster adoption or
creation, the askpass script and its dialog, host-key TOFU surfaced as a real
prompt (never `StrictHostKeyChecking=no`), and the probe. No agent yet — this
phase ends with a "Connect" button that says *"Connected to build-box:
git 2.43.0, Linux x86_64"* and disconnects.

**Done when**: connecting to a host with a passphrase-protected key shows a
dialog and succeeds; connecting to an unknown host shows the fingerprint and
asks; every failure in the list above has its own message.

### Phase 2 — The agent and the protocol

The `onlydiffs-agent` crate, the framing, the request/response types, upload and
version match, and `RemoteRepository` implementing everything except `watch`.
Remote projects open, show their diff, and can be staged and committed.

**Done when**: opening a 500-file diff on a host 100ms away takes one round trip
for the file list and one per file *viewed*, not per file *changed*.

### Phase 3 — Watching

The agent runs the existing `watcher.rs` — `ChangeFilter` and all, unchanged,
which is why it was already factored out as a separately testable struct — and
emits debounced events as unsolicited frames. The client turns those into the
same `repo:changed` event the renderer already listens for.

**Done when**: an agent editing files on the remote host repaints the local
window with the same 300ms debounce it does today.

### Phase 4 — The rest of the surface

- `icon_candidates` on the agent; the Groq call stays local.
- The Claude channel: the agent reads the remote
  `~/.onlydiffs/claude-channels` and POSTs to the remote loopback port. The
  session runs where the code is.
- `workspace.rs`: recents become `(host, path)` pairs. A remote entry is not
  probed with `Path::exists()` (line 249) — it is shown, greyed, until its host
  is reachable.
- Settings: an SSH hosts section beside the Groq key, storing target,
  optional nickname, and extra `ssh` args — Zed's schema, minus the parts
  OnlyDiffs has no use for.

### Phase 5 — Release

The agent has to be cross-compiled for the hosts people actually have:
`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, and
`aarch64-apple-darwin`. Against **glibc**, not musl, and built on an old enough
image (or with `cargo-zigbuild`) that the symbol versions work on a CentOS 7-era
box — a musl static binary would be simpler and would break `getaddrinfo` and
NSS, which the agent needs for nothing today but the Claude channel needs
tomorrow.

Agents ride the existing tag-driven release (`README.md`, "Releasing") as extra
assets, and the version in the filename is the app version, so the match rule in
Phase 2 is trivially correct.

**Done when**: CI produces three agent binaries per tag and the desktop bundle
carries them.

## Decisions, and why

**System `ssh`, not a Rust SSH crate.** The `openssh` crate is the closest fit —
it is built around ControlMaster and offers a native-mux backend that talks the
control socket protocol directly — but it is
[explicitly Unix-only and supports "password-less authentication schemes only"](https://docs.rs/openssh/latest/openssh/),
which rules out passphrases, keyboard-interactive, and 2FA. `russh` is a real
protocol implementation, which means owning `known_hosts` parsing, agent
forwarding, `ProxyJump`, and certificate auth — a permanent tax for no gain.
Shelling out is what Zed and T3 Code both do, and for the same reason: the
user's SSH config is the product.

**Adopt the user's ControlMaster before making one.** Not politeness — it is the
difference between one authentication per boot and one per project open, on
hosts with hardware keys or 2FA where each one is a physical touch.

**No `StrictHostKeyChecking=no`, ever.** An unknown host key is a question for
the user, shown with the fingerprint, and answered into their real
`known_hosts`. Turning the check off to make the first connection succeed is the
exact workaround this plan exists to avoid.

**The Groq key never crosses the connection.** Commit messages and icon choices
are decided locally from remotely-collected inputs. `settings.rs` writes 0600 on
the user's own machine and that is the only disk it is ever on.

**No SSHFS, no rsync-a-copy-locally, no polling `git status` on a timer.** Each
is a way to avoid writing the agent, and each one is wrong in a way that shows
up later: SSHFS makes every `git status` a storm of network stats, a local
mirror is a second source of truth that will disagree, and polling is either too
slow to feel live or too chatty to leave running.

**Windows hosts are out of scope for v1.** Zed carries a whole second
`MasterProcess` implementation for Windows because `ControlMaster` is
unsupported there, detecting connection by echoing a magic string. That is a
real cost for a case OnlyDiffs can defer. Windows *clients* are already out of
scope — the app ships `aarch64-apple-darwin` only.

## Risks

| Risk | Mitigation |
| --- | --- |
| Non-interactive shell has no `git` on `PATH` — T3 Code's #1 support issue | Probe with `sh -lc` the way `shell_env.rs` already does for `GROQ_API_KEY`, and say exactly which shell was asked when it fails |
| A login shell prints a banner into the framed stream | The stream is the agent's stdout under `exec`; banners land on stderr, and stderr is captured as logs, never parsed |
| Agent upload over a slow link on first connect | Compress (`scp -C`), report progress, cache by version — once per version per host |
| Connection drops mid-review | The agent dies with its stdin; reconnect re-runs the bootstrap and re-reads `git status`. Nothing is held that a reconnect cannot rebuild |
| Cross-compiled agent won't run on an old host | glibc, built against an old baseline; the probe reports `uname -sm` before uploading anything and refuses politely on an unsupported triple |
| The seam leaks — some new code calls `git::run_in` directly | Make `git.rs` private to `LocalRepository` in Phase 0 so it cannot |

## Open questions

1. **Should a remote repository be a separate project entry, or a property of
   one?** A repo checked out on two machines is arguably one project. Leaning
   separate, keyed by `(host, path)`, because the recents list is also the icon
   cache and those genuinely differ.
2. **Does the Claude channel work at all if Claude Code is running locally
   against a remote checkout?** It cannot — the channel registration records a
   local `cwd`. Worth confirming that "run Claude on the host" is the intended
   story before building Phase 4.
3. **Terminal.** Zed and T3 Code both ship one. OnlyDiffs has no terminal today,
   and adding one is not implied by any of the above — but it is the first thing
   people will ask for once the connection exists.

## Sources

- [Zed: Remote Development docs](https://github.com/zed-industries/zed/blob/main/docs/src/remote-development.md)
- [Zed: `crates/remote/src/transport/ssh.rs`](https://github.com/zed-industries/zed/blob/main/crates/remote/src/transport/ssh.rs)
- [Zed: `crates/remote/src/protocol.rs`](https://github.com/zed-industries/zed/blob/main/crates/remote/src/protocol.rs)
- [Zed: `crates/remote/src/proxy.rs`](https://github.com/zed-industries/zed/blob/main/crates/remote/src/proxy.rs)
- [Zed issue #45271: existing SSH sessions not reused with ControlMaster](https://github.com/zed-industries/zed/issues/45271)
- [VS Code: Remote Development using SSH](https://code.visualstudio.com/docs/remote/ssh)
- [VS Code: Remote Development Tips and Tricks](https://code.visualstudio.com/docs/remote/troubleshooting)
- [T3 Code: `docs/user/remote-access.md`](https://github.com/pingdotgg/t3code/blob/main/docs/user/remote-access.md)
- [T3 Code: `packages/ssh/src/command.ts`](https://github.com/pingdotgg/t3code/blob/main/packages/ssh/src/command.ts)
- [`openssh` crate documentation](https://docs.rs/openssh/latest/openssh/)


## What the build changed

The plan survived contact largely intact. Four things came out differently, and
each was forced by something that only shows up once it is running.

**The seam became coarse, not just a `git` passthrough.** The trait as sketched
had `git`, `read_file`, `metadata`, `watch`, `icon_candidates` — and with only
those, `get_diff` still issues one `git diff` per changed file, which over a
network is the latency the whole design exists to avoid. `Repository` grew
`diff`, `history`, `list_files`, `commit_all`, `commit_message_diff` and the
rest: one question, one answer. `git` is still there as a narrow escape hatch,
and anything that would call it in a loop belongs behind a coarser method.

**The agent is named by content, not by version.** Zed's filename match is
`{version}-{triple}`, and this copied it. That is correct for a released app
and wrong for a working day: two builds of `0.1.8` that speak different
protocols are exactly what iteration produces, and the second one is never
uploaded because the name has not changed. It cost an afternoon to a stale
agent answering `unknown variant` before an FNV digest of the binary was added
to the name.

**Host keys needed more than `is_known`.** `ssh -G` prints its output
alphabetically, so `globalknownhostsfile` arrives *before* `userknownhostsfile`
— and a first implementation that collected them in print order would have
written an approved key to `/etc/ssh/ssh_known_hosts`. The two lists are kept
apart: read user-first, write only to the user's, and never to a path that
discards writes.

**Upload is `cat >`, not `scp`.** `scp`'s wire protocol changed to SFTP between
OpenSSH 8 and 9, and the old flag spelling is deprecated on some builds and
absent on others. Writing the bytes down the connection that is already open
needs no second authentication, no `scp` on the host, and behaves the same
everywhere. The staging filename carries the pid and a counter, because two
projects on one host connecting at the same moment otherwise write the same
`.partial` over each other — which is how it first failed.

## What is verified

- 139 tests, including 18 that run against a real `sshd` the suite starts
  itself: host-key refusal, fingerprint approval, shell-injection through a
  remote argument, upload and version match, the whole diff/stage/commit path,
  unsolicited watch events, and a dropped connection failing outstanding calls
  rather than hanging them.
- An opt-in suite that runs the same path against Debian on x86_64 in Docker,
  which is the only thing that proves a macOS-built agent runs somewhere else.
- The Linux agents require no glibc newer than 2.17, checked against RHEL 7.

## Still open

The three questions at the end of the plan are answered as follows.

1. **A remote repository is a separate project entry**, keyed by `(host, path)`
   — the recents list is also the icon cache, and those genuinely differ.
2. **The Claude channel runs on the host.** The agent reads that machine's
   `~/.onlydiffs/claude-channels` and POSTs to its loopback, because a session
   reviewing a checkout on a build box is a process on that build box.
3. **There is still no terminal.** Nothing above needs one, and it remains the
   first thing anyone will ask for.
