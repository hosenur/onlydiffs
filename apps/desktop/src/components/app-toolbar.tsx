import { useEffect, useState } from 'react'
import { AppComposer } from '@/components/app-composer'
import { Plug2Outline18, PlugOffOutline18 } from '@/icons'
import { useLineReference } from '@/lib/line-reference'
import { claudeStatus } from '@/lib/ipc'
import { reviewProgress } from '@/lib/review'
import { useUpdate } from '@/lib/update'
import type { ClaudeChannelStatus, FileChange } from '@shared/contract'

/**
 * How often to re-ask. A session can start or stop at any moment and nothing
 * pushes the change, so this polls — cheaply, since answering only means
 * reading a small directory.
 */
const POLL_MS = 4000

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

/**
 * `null` is "not asked yet", worth keeping apart from a known-absent session so
 * the bar does not claim disconnection before it has looked.
 */
function channelLabel(status: ClaudeChannelStatus | null) {
  if (status === null) return 'Checking for Claude…'
  if (!status.connected) return 'No Claude session'
  return status.sessions > 1 ? `Claude connected · ${status.sessions} sessions` : 'Claude connected'
}

interface AppToolbarProps {
  /** The current diff, for the review count at the right end. */
  files: FileChange[]
}

export function AppToolbar({ files }: AppToolbarProps) {
  const [status, setStatus] = useState<ClaudeChannelStatus | null>(null)
  const { reference, clear } = useLineReference()
  // The composer animates out, so it outlives the reference that opened it.
  const shown = useRetained(reference)
  const { offer } = useUpdate()

  useEffect(() => {
    let active = true
    let timer: ReturnType<typeof setTimeout>

    const check = async () => {
      try {
        const next = await claudeStatus()
        if (active) setStatus(next)
      } catch {
        // A failed probe means the same thing as no channel, and saying so is
        // more use than an error nobody can act on.
        if (active) setStatus({ connected: false, sessions: 0 })
      }
      if (active) timer = setTimeout(check, POLL_MS)
    }

    void check()
    return () => {
      active = false
      clearTimeout(timer)
    }
  }, [])

  const connected = status?.connected ?? false
  const label = channelLabel(status)
  const Plug = connected ? Plug2Outline18 : PlugOffOutline18

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
          <AppComposer reference={reference} shown={shown} connected={connected} onClose={clear} />
        </div>
      )}

      <footer className="flex shrink-0 items-center gap-1.5 border-t bg-navbar px-3 py-1.5 font-mono text-[11px]">
        <Plug aria-hidden className="size-3 shrink-0 text-muted-fg" />
        <span aria-live="polite" className={connected ? 'text-fg' : 'text-muted-fg'}>
          {label}
        </span>

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
