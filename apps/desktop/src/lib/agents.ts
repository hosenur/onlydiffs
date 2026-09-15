import {
  claudeStatus,
  codexStatus,
  opencodeStatus,
  sendClaudeMessage,
  sendCodexMessage,
  sendOpenCodeMessage,
} from '@/lib/ipc'

/**
 * The agents a line can be sent to, and the small amount that differs between
 * them.
 *
 * All are reached the same way from here — ask whether there is a session,
 * hand over a message — and all refuse when nothing is running. Where they
 * differ is in how a session becomes reachable, which is what the hints below
 * spell out: Claude Code has to be started with its channel flag, and Codex
 * has to be started against its shared daemon with the repository named.
 */

export type Agent = 'claude' | 'codex' | 'opencode'

/** In the order they are offered. */
export const AGENTS: readonly Agent[] = ['claude', 'codex', 'opencode']

export const AGENT_NAMES: Record<Agent, string> = {
  claude: 'Claude',
  codex: 'Codex',
  opencode: 'OpenCode',
}

/** What a status probe answers, for either agent. */
export interface AgentStatus {
  /** A session is running and a message would reach it now. */
  connected: boolean
  /** How many sessions are running in the repository, reachable or not. */
  sessions: number
  /** Claude only: sessions whose channel Claude Code did not register, because
   *  they were started without the channel flag. */
  unregistered?: number
}

/** What a user runs to make a Claude Code session reachable. */
export const CLAUDE_START_COMMAND = 'claude --dangerously-load-development-channels server:onlydiffs'

/** What a user runs to open a Codex session this app can reach. Matches
 *  `START_COMMAND` in `codex_channel.rs`. */
export const CODEX_START_COMMAND = 'codex --remote unix:// -C "$PWD"'

/** What a user runs to reattach a running Codex session so it can be reached. */
export const CODEX_RESUME_COMMAND = 'codex resume --last --remote unix:// -C "$PWD"'

/** What a user runs to open an OpenCode session. */
export const OPENCODE_START_COMMAND = 'opencode'

/** Whether a session for the open repository can be sent to. */
export function readStatus(agent: Agent): Promise<AgentStatus> {
  if (agent === 'claude') return claudeStatus()
  if (agent === 'codex') return codexStatus()
  return opencodeStatus()
}

/** Hands the message over, resolving with the agent's own id for it. */
export function deliver(agent: Agent, message: string): Promise<string> {
  if (agent === 'claude') return sendClaudeMessage(message)
  if (agent === 'codex') return sendCodexMessage(message)
  return sendOpenCodeMessage(message)
}

/**
 * The status-bar sentence for one agent.
 *
 * `null` is "not asked yet", worth keeping apart from a known-absent session so
 * the bar does not claim there is nothing there before it has looked.
 */
export function statusLabel(agent: Agent, status: AgentStatus | null): string {
  const name = AGENT_NAMES[agent]
  if (status === null) return `Checking for ${name}…`
  if (!status.connected) {
    // A session that is running but cannot be reached — Codex not attached to
    // the shared daemon, Claude started without its channel flag — must not be
    // called absent: the user can see it open in front of them.
    if (status.sessions > 0) return `${name} session not connected`
    return `No ${name} session`
  }

  return status.sessions > 1 ? `${name} connected · ${status.sessions} sessions` : `${name} connected`
}

/** What the empty composer says, which is also where it says why it is empty. */
export function composerPlaceholder(agent: Agent, connected: boolean): string {
  if (connected) return 'What about this line?'
  return `No ${AGENT_NAMES[agent]} session`
}

/** How to get from "nothing to send to" to a session that will take a line. */
export interface ConnectHint {
  text: string
  /** The command to run in the repository, when there is one to paste. */
  command: string | null
}

/**
 * How to make a session reachable, for the agent the composer would send to.
 *
 * Shown whenever there is nothing to send to. With no session at all this is
 * the only place the app says how to start one, and a user who has just clicked
 * a line is exactly the person who needs to know. With a session that is
 * running but cannot be reached, it is the fix: Codex has to be attached to the
 * shared daemon with the repository named, Claude needs the channel flag, and
 * OpenCode's own service needs a restart.
 *
 * `null` before the first probe answers, so the bar does not tell someone to
 * start a session it has not looked for yet.
 */
export function connectHint(agent: Agent, status: AgentStatus | null): ConnectHint | null {
  if (status === null || status.connected) return null

  if (status.sessions === 0) {
    const command =
      agent === 'claude'
        ? CLAUDE_START_COMMAND
        : agent === 'codex'
          ? CODEX_START_COMMAND
          : OPENCODE_START_COMMAND
    const name = agent === 'claude' ? 'Claude Code' : AGENT_NAMES[agent]
    return { text: `No ${name} session in this repository. Start one there with:`, command }
  }

  if (agent === 'codex') {
    return {
      text: 'Codex is running outside its shared daemon. Close it, then run:',
      command: CODEX_RESUME_COMMAND,
    }
  }
  if (agent === 'claude') {
    return {
      text: 'Claude Code is running without the channel flag. Close it, then run:',
      command: CLAUDE_START_COMMAND,
    }
  }
  return {
    text: 'OpenCode is running, but its background service has no matching session. Restart it.',
    command: null,
  }
}

/**
 * Which agent to open the composer on.
 *
 * The remembered choice wins whenever it can still be sent to, because a user
 * who picked one meant it. Otherwise the first agent with a session, so the
 * common case of having only one installed never asks. When neither is
 * available it falls back to the remembered choice so the bar still names
 * something while explaining that there is nothing there.
 */
export function preferredAgent(
  remembered: Agent | null,
  statuses: Partial<Record<Agent, AgentStatus | null>>
): Agent {
  if (remembered && statuses[remembered]?.connected) return remembered
  return AGENTS.find((agent) => statuses[agent]?.connected) ?? remembered ?? 'claude'
}
