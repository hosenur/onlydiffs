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

## Environment

`turbo.json` declares `globalPassThroughEnv`. Turbo strips anything not listed
there, so a variable the desktop app reads at runtime has to be named or it
arrives unset with no warning.

| Variable | Effect |
| --- | --- |
| `ONLYDIFFS_REPO_PATH` | Opens this repository at startup, skipping the landing page |
| `ONLYDIFFS_STATE_DIR` | Where the recents list lives (default `~/.onlydiffs`) |
| `GROQ_API_KEY` | Commit-message generation |

## Releasing

Tagging is the release. The version lives in three manifests, and CI refuses a
tag that disagrees with any of them:

```sh
# tauri.conf.json, Cargo.toml, package.json — all three, then:
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
