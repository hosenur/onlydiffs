#!/usr/bin/env bash
# Starts Claude Code with the OnlyDiffs channel enabled for the session.
#
# Registering the channel with `claude mcp add` is not enough: during the
# channels research preview, Claude Code delivers channel messages only to a
# session that names the server in this flag, and drops them silently
# otherwise. Everything after the flag is passed through, so this is `claude`
# with one more argument.
exec claude --dangerously-load-development-channels server:onlydiffs "$@"
