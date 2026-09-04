import { claudeStatus, codexStatus, sendClaudeMessage, sendCodexMessage } from '@/lib/ipc'

/**
 * The agents a line can be sent to, and the small amount that differs between
 * them.
 *
 * Both are reached the same way from here — ask whether there is a session,
 * hand over a message — but they do not mean the same thing by either answer,
 * and the wording is where that shows. Claude Code registers a live listener,
 * so "connected" means a process is waiting and a send reaches it now. Codex
 * keeps a durable per-thread queue, so there is nothing to be connected *to*:
 * what exists is a thread, and a message sent to one whose session is closed
 * waits until it opens again.
 *
 * That difference is worth saying out loud rather than flattening, because it
 * changes what a user should expect after pressing Enter.
 */

export type Agent = 'claude' | 'codex'

/** In the order they are offered. */
export const AGENTS: readonly Agent[] = ['claude', 'codex']

export const AGENT_NAMES: Record<Agent, string> = {
  claude: 'Claude',
  codex: 'Codex',
}

/** What a status probe answers. Only Codex has a delivery step to report on;
 *  for Claude the send *is* the delivery, so it is always true. */
export interface AgentStatus {
  connected: boolean
  sessions: number
  delivering?: boolean
}

/** Whether a session for the open repository can be sent to. */
export function readStatus(agent: Agent): Promise<AgentStatus> {
  // Claude has no separate delivery step — a send either reaches the listening
  // session or fails — so it reports as always delivering.
  return agent === 'claude'
    ? claudeStatus().then((status) => ({ ...status, delivering: true }))
    : codexStatus()
}

/** Hands the message over, resolving with the agent's own id for it. */
export function deliver(agent: Agent, message: string): Promise<string> {
  return agent === 'claude' ? sendClaudeMessage(message) : sendCodexMessage(message)
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
  if (!status.connected) return `No ${name} session`

  // "Connected" would be a claim Codex cannot support: nothing is listening,
  // there is only a thread the message will be waiting in.
  if (agent === 'codex') {
    return status.sessions > 1 ? `Codex ready · ${status.sessions} threads` : 'Codex ready'
  }
  return status.sessions > 1 ? `Claude connected · ${status.sessions} sessions` : 'Claude connected'
}

/** What the empty composer says, which is also where it says why it is empty. */
export function composerPlaceholder(agent: Agent, connected: boolean): string {
  if (connected) return 'What about this line?'
  return `No ${AGENT_NAMES[agent]} session`
}

/**
 * What happens after the send, for the agent it was sent to.
 *
 * Only Codex has anything to add: its message may sit in the queue for a while,
 * and a user who expects an immediate reply and does not get one should be able
 * to tell that from the interface rather than from waiting.
 */
export function deliveryNote(agent: Agent, status: AgentStatus | null): string | null {
  if (agent !== 'codex' || !status?.connected) return null
  if (status.delivering === false) {
    // The message would still be written down and still be delivered
    // eventually. Saying nothing would leave the user watching for a reply
    // that cannot come until they start the daemon.
    return "Codex's shared daemon is not running — this will wait in the queue until it is."
  }
  return 'Queued for Codex — it arrives when the session next runs.'
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
