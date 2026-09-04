#!/usr/bin/env bash
# Registers the OnlyDiffs channel with Claude Code on this machine.
#
# The channel is the agent binary running as `onlydiffs-agent channel`, reached
# through the stable path `~/.onlydiffs/agent/current`. The app keeps that
# symlink pointing at the agent it ships; a fresh checkout that has never run
# the app builds one here so the registration works before the first launch.
#
# Registering is not enabling. Claude Code only delivers channel messages to a
# session started with the flag printed at the end, and `bun run claude` is
# that command.
set -euo pipefail

cd "$(dirname "$0")/.."

agent_dir="$HOME/.onlydiffs/agent"
current="$agent_dir/current"

if [ ! -x "$current" ]; then
  echo "No installed agent yet; building one."
  cargo build --manifest-path src-tauri/Cargo.toml -p onlydiffs-agent --release
  mkdir -p "$agent_dir"
  chmod 700 "$agent_dir"
  built="$agent_dir/onlydiffs-agent-$(src-tauri/target/release/onlydiffs-agent --version)-local"
  cp src-tauri/target/release/onlydiffs-agent "$built"
  chmod 700 "$built"
  ln -sfn "$built" "$current"
fi

claude mcp remove --scope user onlydiffs >/dev/null 2>&1 || true
claude mcp add --scope user onlydiffs -- "$current" channel

cat <<'EOF'

Registered. Claude Code only delivers channel messages to a session started as:

  claude --dangerously-load-development-channels server:onlydiffs

`bun run claude` runs exactly that.
EOF
