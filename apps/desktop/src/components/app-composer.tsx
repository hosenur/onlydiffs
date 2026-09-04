import { useEffect, useRef, useState } from 'react'
import { PaperAirplaneIcon, XMarkIcon } from '@heroicons/react/16/solid'
import { Button } from '@onlydiffs/ui/button'
import { Input, InputGroup } from '@onlydiffs/ui/input'
import { TextField } from '@onlydiffs/ui/text-field'
import { type Attachments, useAttachments } from '@/hooks/use-attachments'
import { composeMessage, pastedImages } from '@/lib/attachments'
import type { LineReference } from '@/lib/line-reference'
import { sendClaudeMessage } from '@/lib/ipc'

/**
 * The floating input that carries a line — and anything pasted about it — to
 * the Claude session for the open repository.
 *
 * Text is only half of it. An image pasted here is written down on the machine
 * the repository is on, and what the message carries is where it landed; see
 * `hooks/use-attachments` for why that happens at the paste rather than at the
 * send, and `src-tauri/core/src/services/attachment.rs` for why it lands where
 * it does.
 */

/**
 * The pasted images, above the field they were pasted into.
 *
 * A dimmed thumbnail is one still being written down where the session can
 * reach it. A red-edged one could not be, and holds the send until it is taken
 * out — dropping it from the message silently would be worse than saying so.
 */
function PastedImages({ images }: { images: Attachments }) {
  if (images.items.length === 0) return null

  return (
    <div className="flex flex-wrap gap-1.5 px-1 pb-2">
      {images.items.map((image) => (
        <div key={image.id} className="group relative">
          <img
            src={image.preview}
            alt={image.name}
            title={image.error ?? image.name}
            className={`size-12 rounded-lg border object-cover ${
              image.error ? 'border-danger-subtle-fg/70' : 'border-input'
            } ${image.path === null && !image.error ? 'animate-pulse opacity-60' : ''}`}
          />
          <button
            type="button"
            onClick={() => images.remove(image.id)}
            aria-label={`Remove ${image.name}`}
            className="-end-1 -top-1 absolute rounded-full border bg-overlay p-0.5 text-muted-fg opacity-0 transition focus-visible:opacity-100 hover:text-fg group-hover:opacity-100"
          >
            <XMarkIcon className="size-3" />
          </button>
        </div>
      ))}
    </div>
  )
}

interface AppComposerProps {
  /** The line being asked about, or `null` while the bar animates out. */
  reference: LineReference | null
  /** What to draw: the live reference, or the last one there was, so the bar
   *  keeps its content on the way out. */
  shown: LineReference
  /** Whether there is a session to send to. */
  connected: boolean
  /** Dismisses the bar — on Escape, on the ×, and once a message is away. */
  onClose: () => void
}

export function AppComposer({ reference, shown, connected, onClose }: AppComposerProps) {
  const [message, setMessage] = useState('')
  const [isSending, setIsSending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const images = useAttachments()
  const input = useRef<HTMLInputElement>(null)

  // A fresh line means a fresh message; carrying the old draft across would
  // silently attach it to a line the user did not mean. Dismissal deliberately
  // leaves the draft alone: the bar is still on screen fading out, and watching
  // the text blank out first reads as a glitch.
  useEffect(() => {
    if (!reference) return
    setMessage('')
    setError(null)
    images.clear()
    input.current?.focus()
  }, [reference, images.clear])

  const draft = message.trim()
  // A screenshot with nothing typed is still a question worth sending, so
  // either half is enough on its own.
  const canSend =
    connected && !isSending && images.isReady && (draft.length > 0 || images.paths.length > 0)
  const failure = error ?? images.error

  async function send() {
    if (!reference || !canSend) return
    setIsSending(true)
    setError(null)
    try {
      // The full path, not the shortened label on screen — Claude has to be
      // able to open the file. Same for the images: what crosses is where they
      // landed on the repository's own machine.
      await sendClaudeMessage(composeMessage(reference.label, draft, images.paths))
      onClose()
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setIsSending(false)
    }
  }

  return (
    // Mounted from the first reference onwards; `data-open` is what opens and
    // closes it, so dismissal has something to animate.
    <div
      data-open={reference !== null}
      className="composer pointer-events-auto w-full max-w-xl rounded-xl border bg-overlay p-2 shadow-lg"
    >
      <div className="flex items-center justify-between gap-2 px-1 pb-1.5">
        <span
          title={shown.label}
          className="flex min-w-0 font-mono text-[11px] text-primary-subtle-fg"
        >
          {/* The number sits outside the truncation: a clipped name is still
              recognisable, a clipped line number is not. */}
          <span className="truncate">{shown.name}</span>
          <span>:{shown.lineNumber}</span>
        </span>
        <button
          type="button"
          onClick={onClose}
          aria-label="Dismiss"
          className="shrink-0 text-muted-fg hover:text-fg"
        >
          <XMarkIcon className="size-3.5" />
        </button>
      </div>

      <PastedImages images={images} />

      <TextField
        aria-label={`Message Claude about ${shown.label}`}
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
            onPaste={(event) => {
              const pasted = pastedImages(event.clipboardData)
              if (pasted.length === 0) return
              // The field is for words. An image goes to the row above it
              // rather than being pasted in as its own file name.
              event.preventDefault()
              void images.add(pasted)
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault()
                void send()
              }
              if (event.key === 'Escape') {
                event.preventDefault()
                onClose()
              }
              // Nothing left to delete in the field means the thing behind the
              // caret is the last image.
              const last = images.items[images.items.length - 1]
              if (event.key === 'Backspace' && message.length === 0 && last) {
                event.preventDefault()
                images.remove(last.id)
              }
            }}
          />
          <Button
            intent="plain"
            size="sm"
            onPress={() => void send()}
            isDisabled={!canSend}
            aria-label="Send to Claude"
          >
            <PaperAirplaneIcon />
          </Button>
        </InputGroup>
      </TextField>

      {/* A refused image is worth a sentence rather than only a red outline: it
          is the reason the send button has stopped working, and the fix is to
          take the image out. */}
      {failure && (
        <p role="alert" className="px-1 pt-1.5 text-danger-subtle-fg text-xs">
          {failure}
        </p>
      )}
    </div>
  )
}
