import { useEffect, useState } from 'react'
import { AGENTS, type Agent, type AgentStatus, readStatus } from '@/lib/agents'

/**
 * Whether each agent has a session for the open repository.
 *
 * A session can start or stop at any moment and nothing pushes the change, so
 * this polls. Both probes are cheap — each one reads a small directory — and
 * they go out together rather than in sequence, so one slow answer does not
 * hold up the other's indicator.
 */

/** How often to re-ask. */
const POLL_MS = 4000

export type AgentStatuses = Record<Agent, AgentStatus | null>

/** `null` per agent is "not asked yet", not "nothing there". */
const UNASKED: AgentStatuses = { claude: null, codex: null }

export function useAgentStatus(): AgentStatuses {
  const [statuses, setStatuses] = useState<AgentStatuses>(UNASKED)

  useEffect(() => {
    let active = true
    let timer: ReturnType<typeof setTimeout>

    const check = async () => {
      const answers = await Promise.all(
        AGENTS.map(async (agent) => {
          try {
            return [agent, await readStatus(agent)] as const
          } catch {
            // A failed probe means the same thing as no session, and saying so
            // is more use than an error nobody can act on.
            return [agent, { connected: false, sessions: 0 }] as const
          }
        })
      )
      if (!active) return
      setStatuses(Object.fromEntries(answers) as AgentStatuses)
      timer = setTimeout(check, POLL_MS)
    }

    void check()
    return () => {
      active = false
      clearTimeout(timer)
    }
  }, [])

  return statuses
}
