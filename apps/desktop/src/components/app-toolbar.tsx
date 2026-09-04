import { AppComposer } from '@/components/app-composer'
import { useEffect, useState } from 'react'
import { useAgentStatus } from '@/hooks/use-agent-status'
import { Plug2Outline18, PlugOffOutline18 } from '@/icons'
import { AGENTS, type Agent, statusLabel } from '@/lib/agents'
import { useLineReference } from '@/lib/line-reference'
import { reviewProgress } from '@/lib/review'
import { useUpdate } from '@/lib/update'
import type { FileChange } from '@shared/contract'

/**
 * Holds the last non-null value so a surface can keep rendering its content
 * while it animates out. The live value always wins, so switching from one
 * reference straight to another shows the new one immediately.
 */
function useRetained<T>(value: T | null): T | null {
  const [retained, setRetained] = useState(value)

  useEffect(() => {
    if (value !== null) setRetained(value)
  }, [value])

  return value ?? retained
}

interface AppToolbarProps {
  /** The current diff, for the review count at the right end. */
  files: FileChange[]
}

export function AppToolbar({ files }: AppToolbarProps) {
  const statuses = useAgentStatus()
  const { reference, clear } = useLineReference()
  // The composer animates out, so it outlives the reference that opened it.
  const shown = useRetained(reference)
  const { offer } = useUpdate()

  const progress = reviewProgress(files)
  const isSwept = progress.reviewed === progress.total

  return (
    <>
      {shown && (
        // Fixed to the window rather than the content pane, so hiding the
        // sidebar does not slide the composer sideways underneath the cursor.
        <div className="pointer-events-none fixed inset-x-0 bottom-0 z-20 flex justify-center px-4 pb-12">
          {/* Mounted from the first reference onwards and kept mounted after
              it clears, so dismissal has something to animate. */}
          <AppComposer reference={reference} shown={shown} statuses={statuses} onClose={clear} />
        </div>
      )}

      <footer className="flex shrink-0 items-center gap-3 border-t bg-navbar px-3 py-1.5 font-mono text-[11px]">
        {/* One indicator per agent. Both are shown even when only one is
            installed: "No Codex session" is how someone finds out the composer
            can send there at all. */}
        {AGENTS.map((agent) => (
          <AgentIndicator key={agent} agent={agent} status={statuses[agent]} />
        ))}

        <span className="ms-auto flex items-center gap-3">
          {/* A mention, not a prompt: the install lives in the command menu, so
              nothing here interrupts what the window is already showing. */}
          {offer && (
            <span title="Install it from the command menu (⌘K)" className="text-primary-subtle-fg">
              Update available · v{offer.version}
            </span>
          )}

          {/* Nothing to review reads as nothing to say. The main pane already
              tells anyone with a clean tree that it is clean. */}
          {progress.total > 0 && (
            <span
              aria-live="polite"
              title="A file counts as reviewed once all of its changes are staged"
              className={isSwept ? 'text-success-subtle-fg' : 'text-muted-fg'}
            >
              {progress.reviewed}/{progress.total} files reviewed
            </span>
          )}
        </span>
      </footer>
    </>
  )
}

function AgentIndicator({
  agent,
  status,
}: {
  agent: Agent
  status: { connected: boolean; sessions: number } | null
}) {
  const connected = status?.connected ?? false
  const Plug = connected ? Plug2Outline18 : PlugOffOutline18

  return (
    <span className="flex items-center gap-1.5">
      <Plug aria-hidden className="size-3 shrink-0 text-muted-fg" />
      <span aria-live="polite" className={connected ? 'text-fg' : 'text-muted-fg'}>
        {statusLabel(agent, status)}
      </span>
    </span>
  )
}
