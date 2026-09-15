import { AppComposer } from '@/components/app-composer'
import { useEffect, useRef, useState } from 'react'
import type { ComponentType, SVGProps } from 'react'
import { useNavigate, useParams } from '@tanstack/react-router'
import { Button } from '@onlydiffs/ui/button'
import { useAgentStatus } from '@/hooks/use-agent-status'
import { AGENTS, AGENT_NAMES, type Agent, statusLabel } from '@/lib/agents'
import { type LineReference, useLineReference } from '@/lib/line-reference'
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

/** How long the footer says a message went out. */
const SENT_NOTE_MS = 4000

/**
 * The footer's word that a message was handed over, for a few seconds.
 *
 * The composer closes the moment a send succeeds, so without this a delivered
 * message and a bar that was simply dismissed look exactly alike.
 */
function useSentNote() {
  const [note, setNote] = useState<string | null>(null)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  useEffect(() => () => clearTimeout(timer.current), [])

  function announce(agent: Agent, reference: LineReference) {
    setNote(`Sent to ${AGENT_NAMES[agent]} · ${reference.name}:${reference.lineNumber}`)
    clearTimeout(timer.current)
    timer.current = setTimeout(() => setNote(null), SENT_NOTE_MS)
  }

  return { note, announce }
}

interface AppToolbarProps {
  /** The current diff, for the review count at the right end. */
  files: FileChange[]
  /** The open repository, so a line clicked in another one is let go. */
  repoPath: string
}

export function AppToolbar({ files, repoPath }: AppToolbarProps) {
  const statuses = useAgentStatus()
  const { reference, clear } = useLineReference()
  // The composer animates out, so it outlives the reference that opened it.
  const shown = useRetained(reference)
  const { offer } = useUpdate()
  const sent = useSentNote()

  const progress = reviewProgress(files)
  const isSwept = progress.reviewed === progress.total

  const navigate = useNavigate()
  // SAFETY: with `strict: false` the params are the union of every route's,
  // and only `/file/$` contributes a key — `_splat`, a string. Everywhere else
  // it is absent, which is what the optional field says.
  const params = useParams({ strict: false }) as { _splat?: string }
  const next = nextUnreviewed(files, params._splat)

  /*
   * A line belongs to the file it was clicked in, in the repository that file
   * is in. Nothing else ever let it go but the composer's own dismissal, so it
   * followed the user into the next file — and, because switching projects
   * keeps this layout mounted, into the next repository, where Enter would
   * hand that repository's agent a path from one it has never seen.
   */
  useEffect(() => {
    clear()
  }, [params._splat, repoPath, clear])

  return (
    <>
      {shown && (
        // Fixed to the window rather than the content pane, so hiding the
        // sidebar does not slide the composer sideways underneath the cursor.
        <div className="pointer-events-none fixed inset-x-0 bottom-0 z-20 flex justify-center px-4 pb-12">
          {/* Mounted from the first reference onwards and kept mounted after
              it clears, so dismissal has something to animate. */}
          <AppComposer
            reference={reference}
            shown={shown}
            statuses={statuses}
            onClose={clear}
            onSent={sent.announce}
          />
        </div>
      )}

      <footer className="flex shrink-0 items-center gap-3 border-t bg-navbar px-3 py-1.5 font-mono text-[11px]">
        {/* Every agent the app can talk to, session or not. An indicator that
            is only there once something is running cannot answer the question
            it exists for — whether this agent is running at all — and its
            absence reads as the app not supporting the agent. */}
        {AGENTS.map((agent) => (
          <AgentIndicator key={agent} agent={agent} status={statuses[agent]} />
        ))}

        <span className="ms-auto flex items-center gap-3">
          {/* Always mounted so the live region exists before it has anything
              to say; `empty:hidden` keeps it from claiming a gap meanwhile. */}
          <span role="status" className="text-success-subtle-fg empty:hidden">
            {sent.note}
          </span>

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
