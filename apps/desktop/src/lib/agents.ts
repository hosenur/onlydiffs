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

/** What a user runs to reattach a running Codex session so it can be reached. */
export const CODEX_RESUME_COMMAND = 'codex resume --last --remote unix:// -C "$PWD"'

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

/** OpenCode is discoverable rather than advertised when it is not running. */
export function isAgentVisible(agent: Agent, status: AgentStatus | null): boolean {
  return agent !== 'opencode' || (status?.sessions ?? 0) > 0
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

/**
 * How to make an unreachable session reachable, for the agent it would go to.
 *
 * Shown only when a session is running and cannot be sent to, which is the one
 * moment the user needs a fix rather than a status. For Codex the session has
 * to be attached to the shared daemon with the repository named, for Claude it
 * needs the channel flag, and OpenCode's own service needs a restart.
 */
export function deliveryNote(agent: Agent, status: AgentStatus | null): string | null {
  if (status?.connected || (status?.sessions ?? 0) === 0) return null
  if (agent === 'codex') return `Not reachable. Close it, then: ${CODEX_RESUME_COMMAND}`
  if (agent === 'claude') return `Not reachable. Close it, then: ${CLAUDE_START_COMMAND}`
  return 'Not reachable. Restart the OpenCode 2 session.'
}

/**
 * Which agent to open the composer on.
 *
 * The remembered choice wins whenever it can still be sent to, because a user
 * who picked one meant it. Otherwise the first agent with a session, so the
 * common case of having only one installed never asks. When neither is
 * available it falls back to the remembered built-in choice so the bar still
 * names something while explaining that there is nothing there. OpenCode is
 * excluded from that fallback because it is hidden when no TUI is running.
 */
export function preferredAgent(
  remembered: Agent | null,
  statuses: Partial<Record<Agent, AgentStatus | null>>
): Agent {
  if (remembered && statuses[remembered]?.connected) return remembered
  return (
    AGENTS.find((agent) => statuses[agent]?.connected) ??
    (remembered !== 'opencode' ? remembered : null) ??
    'claude'
  )
}
