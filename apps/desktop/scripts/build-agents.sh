#!/usr/bin/env bash
# Builds the remote agents the app uploads to hosts, one per platform it
# supports, into `src-tauri/agents/` where the bundle picks them up.
#
# Linux targets go through `cargo-zigbuild`, which uses zig as the linker and —
# the part that matters — lets the glibc version be pinned. Building against
# whatever glibc the runner happens to have would produce a binary that refuses
# to start on anything older, and "anything older" is most build servers. 2.17
# is CentOS 7, which is about as far back as anyone still runs.
#
# Not musl. A static musl binary would be simpler and would break `getaddrinfo`
# and NSS, which the agent needs for the Claude channel's loopback lookups.
set -euo pipefail

cd "$(dirname "$0")/.."
root="src-tauri"
out="$root/agents"
mkdir -p "$out"

GLIBC="${ONLYDIFFS_AGENT_GLIBC:-2.17}"

build() {
  local triple="$1" using="$2"
  echo "--- agent for $triple ($using)"
  case "$using" in
    zig) cargo zigbuild --manifest-path "$root/Cargo.toml" -p onlydiffs-agent \
           --release --target "$triple.$GLIBC" ;;
    *)   cargo build --manifest-path "$root/Cargo.toml" -p onlydiffs-agent \
           --release --target "$triple" ;;
  esac
  cp "$root/target/$triple/release/onlydiffs-agent" "$out/onlydiffs-agent-$triple"
  chmod 755 "$out/onlydiffs-agent-$triple"
}

# The hosts people actually have. A platform missing here is refused politely by
# the probe rather than guessed at, so adding one is only ever additive.
build x86_64-unknown-linux-gnu  zig
build aarch64-unknown-linux-gnu zig
build aarch64-apple-darwin      native
build x86_64-apple-darwin       native

echo
ls -la "$out"
