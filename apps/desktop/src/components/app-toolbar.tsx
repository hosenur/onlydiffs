import { useEffect, useRef, useState } from 'react'
import { PaperAirplaneIcon, XMarkIcon } from '@heroicons/react/16/solid'
import { Button } from '@onlydiffs/ui/button'
import { Input, InputGroup } from '@onlydiffs/ui/input'
import { TextField } from '@onlydiffs/ui/text-field'
import { Plug2Outline18, PlugOffOutline18 } from '@/icons'
import { useLineReference } from '@/lib/line-reference'
import { claudeStatus, sendClaudeMessage } from '@/lib/ipc'
import { useUpdate } from '@/lib/update'
import type { ClaudeChannelStatus } from '@shared/contract'

/**
 * How often to re-ask. A session can start or stop at any moment and nothing
 * pushes the change, so this polls — cheaply, since answering only means
 * reading a small directory.
 */
const POLL_MS = 4000

export function AppToolbar() {
  const [status, setStatus] = useState<ClaudeChannelStatus | null>(null)
  const { reference, clear } = useLineReference()
  const { offer } = useUpdate()
  const [message, setMessage] = useState('')
  const [isSending, setIsSending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const input = useRef<HTMLInputElement>(null)

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

  // A fresh line means a fresh message; carrying the old draft across would
  // silently attach it to a line the user did not mean.
  useEffect(() => {
    setMessage('')
    setError(null)
    if (reference) input.current?.focus()
  }, [reference])

  const connected = status?.connected ?? false

  async function send() {
    const text = message.trim()
    if (!text || !reference || isSending) return
    setIsSending(true)
    setError(null)
    try {
      // The full path, not the shortened label on screen — Claude has to be
      // able to open the file.
      await sendClaudeMessage(`${reference.label} ${text}`)
      clear()
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setIsSending(false)
    }
  }

  // `null` is "not asked yet", worth keeping apart from a known-absent session
  // so the bar does not claim disconnection before it has looked.
  const label =
    status === null
      ? 'Checking for Claude…'
      : connected
        ? status.sessions > 1
          ? `Claude connected · ${status.sessions} sessions`
          : 'Claude connected'
        : 'No Claude session'

  const Plug = connected ? Plug2Outline18 : PlugOffOutline18

  return (
    <>
      {reference && (
        // Fixed to the window rather than the content pane, so hiding the
        // sidebar does not slide the composer sideways underneath the cursor.
        <div className="pointer-events-none fixed inset-x-0 bottom-0 z-20 flex justify-center px-4 pb-12">
          <div className="composer pointer-events-auto w-full max-w-xl rounded-xl border bg-overlay p-2 shadow-lg">
            <div className="flex items-center justify-between gap-2 px-1 pb-1.5">
              <span
                title={reference.label}
                className="flex min-w-0 font-mono text-[11px] text-primary-subtle-fg"
              >
                {/* The number sits outside the truncation: a clipped name is
                    still recognisable, a clipped line number is not. */}
                <span className="truncate">{reference.name}</span>
                <span>:{reference.lineNumber}</span>
              </span>
              <button
                type="button"
                onClick={clear}
                aria-label="Dismiss"
                className="shrink-0 text-muted-fg hover:text-fg"
              >
                <XMarkIcon className="size-3.5" />
              </button>
            </div>

            <TextField
              aria-label={`Message Claude about ${reference.label}`}
              value={message}
              onChange={setMessage}
              isDisabled={isSending || !connected}
              autoComplete="off"
              spellCheck="false"
            >
              <InputGroup>
                <Input
                  ref={input}
                  placeholder={connected ? 'What about this line?' : 'No Claude session'}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      event.preventDefault()
                      void send()
                    }
                    if (event.key === 'Escape') {
                      event.preventDefault()
                      clear()
                    }
                  }}
                />
                <Button
                  intent="plain"
                  size="sm"
                  onPress={() => void send()}
                  isDisabled={isSending || !connected || message.trim().length === 0}
                  aria-label="Send to Claude"
                >
                  <PaperAirplaneIcon />
                </Button>
              </InputGroup>
            </TextField>

            {error && (
              <p role="alert" className="px-1 pt-1.5 text-danger-subtle-fg text-xs">
                {error}
              </p>
            )}
          </div>
        </div>
      )}

      <footer className="flex shrink-0 items-center gap-1.5 border-t bg-navbar px-3 py-1.5 font-mono text-[11px]">
        <Plug aria-hidden className="size-3 shrink-0 text-muted-fg" />
        <span aria-live="polite" className={connected ? 'text-fg' : 'text-muted-fg'}>
          {label}
        </span>

        {/* A mention, not a prompt: the install lives in the command menu, so
            nothing here interrupts what the window is already showing. */}
        {offer && (
          <span
            title="Install it from the command menu (⌘K)"
            className="ms-auto text-primary-subtle-fg"
          >
            Update available · v{offer.version}
          </span>
        )}
      </footer>
    </>
  )
}
