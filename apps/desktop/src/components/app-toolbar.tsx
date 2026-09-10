import { AppComposer } from '@/components/app-composer'
import { useEffect, useState } from 'react'
import type { ComponentType, SVGProps } from 'react'
import { useNavigate, useParams } from '@tanstack/react-router'
import { Button } from '@onlydiffs/ui/button'
import { useAgentStatus } from '@/hooks/use-agent-status'
import { AGENTS, type Agent, isAgentVisible, statusLabel } from '@/lib/agents'
import { useLineReference } from '@/lib/line-reference'
import { nextUnreviewed, reviewProgress } from '@/lib/review'
import { fileHref } from '@/lib/status'
import { useUpdate } from '@/lib/update'
import type { FileChange } from '@shared/contract'
import { ChevronRightIcon } from '@onlydiffs/ui/icons'
import { ClaudeIcon, CodexIcon, OpenCodeIcon } from '@/icons'

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

  const navigate = useNavigate()
  // SAFETY: with `strict: false` the params are the union of every route's,
  // and only `/file/$` contributes a key — `_splat`, a string. Everywhere else
  // it is absent, which is what the optional field says.
  const params = useParams({ strict: false }) as { _splat?: string }
  const next = nextUnreviewed(files, params._splat)

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
        {/* Claude and Codex remain as discoverable hints without sessions;
            OpenCode appears only while its local TUI is running. */}
        {AGENTS.filter((agent) => isAgentVisible(agent, statuses[agent])).map((agent) => (
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
            <>
              <span
                aria-live="polite"
                title="A file counts as reviewed once all of its changes are staged"
                className={isSwept ? 'text-success-subtle-fg' : 'text-muted-fg'}
              >
                {progress.reviewed}/{progress.total} files reviewed
              </span>

              {/* Where ⌘Enter leaves you: the file is staged, the count has
                  moved, and this is the step to the next one. Disabled rather
                  than hidden once the sweep is done, so the footer keeps its
                  shape and the disabled state itself says "nothing left". */}
              <Button
                intent="outline"
                size="xs"
                isDisabled={next === null}
                aria-label={next ? `Next unreviewed file: ${next}` : 'Every file is reviewed'}
                onPress={() => {
                  if (next) void navigate({ to: fileHref(next) })
                }}
                className="h-5 min-h-0 px-1.5 font-mono text-[11px] sm:min-h-0 sm:text-[11px]"
              >
                Next
                <ChevronRightIcon />
              </Button>
            </>
          )}
        </span>
      </footer>
    </>
  )
}

/**
 * Each agent's own mark, in place of a generic plug that said nothing about
 * which one. `tint` is how the mark looks with a live session; without one the
 * mark is greyed out.
 */
const AGENT_MARKS = {
  claude: { Mark: ClaudeIcon, tint: 'text-[#D97757]' },
  codex: { Mark: CodexIcon, tint: 'text-fg' },
  opencode: { Mark: OpenCodeIcon, tint: 'text-fg' },
} satisfies Record<Agent, { Mark: ComponentType<SVGProps<SVGSVGElement>>; tint: string }>

function AgentIndicator({
  agent,
  status,
}: {
  agent: Agent
  status: { connected: boolean; sessions: number } | null
}) {
  const connected = status?.connected ?? false
  const { Mark, tint } = AGENT_MARKS[agent]

  return (
    <span className="flex items-center gap-icon">
      <Mark
        aria-hidden
        className={`size-3 shrink-0 transition-colors ${connected ? tint : 'text-muted-fg'}`}
      />
      <span aria-live="polite" className={connected ? 'text-fg' : 'text-muted-fg'}>
        {statusLabel(agent, status)}
      </span>
    </span>
  )
}
