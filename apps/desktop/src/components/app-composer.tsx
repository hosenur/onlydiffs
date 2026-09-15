import { useEffect, useRef, useState } from 'react'
import { Button } from '@onlydiffs/ui/button'
import { Input, InputGroup } from '@onlydiffs/ui/input'
import { TextField } from '@onlydiffs/ui/text-field'
import type { AgentStatuses } from '@/hooks/use-agent-status'
import { type Attachments, useAttachments } from '@/hooks/use-attachments'
import {
  AGENTS,
  AGENT_NAMES,
  type Agent,
  composerPlaceholder,
  connectHint,
  deliver,
  preferredAgent,
} from '@/lib/agents'
import { composeMessage, pastedImages } from '@/lib/attachments'
import { writeClipboardText } from '@/lib/ipc'
import type { LineReference } from '@/lib/line-reference'
import { CheckIcon, CloseIcon, SendIcon } from '@onlydiffs/ui/icons'

/**
 * The floating input that carries a line — and anything pasted about it — to a
 * coding agent working in the open repository.
 *
 * Text is only half of it. An image pasted here is written down on the machine
 * the repository is on, and what the message carries is where it landed; see
 * `hooks/use-attachments` for why that happens at the paste rather than at the
 * send, and `src-tauri/core/src/services/attachment.rs` for why it lands where
 * it does.
 *
 * Which agent it goes to is a choice, remembered across lines — see `lib/agents`
 * for what they do and do not have in common.
 */

/** Where the chosen agent is remembered, so picking one is a decision made
 *  once rather than on every line. */
const CHOICE_KEY = 'onlydiffs.composer.agent'

function rememberedAgent(): Agent | null {
  try {
    const stored = localStorage.getItem(CHOICE_KEY)
    return AGENTS.includes(stored as Agent) ? (stored as Agent) : null
  } catch {
    // A browser with site data blocked still gets a working composer; it just
    // does not remember which agent was picked.
    return null
  }
}

function rememberAgent(agent: Agent) {
  try {
    localStorage.setItem(CHOICE_KEY, agent)
  } catch {
    // Not remembering is not worth surfacing.
  }
}

/**
 * The agent picker, shown only when there is a choice to make.
 *
 * One agent installed is not a decision, and a toggle that can only be in one
 * position is furniture. It appears when both have a session, and also when the
 * chosen one has gone away — that is exactly the moment the user needs to see
 * where the message would otherwise go.
 */
function AgentPicker({
  agent,
  picked,
  statuses,
  onPick,
}: {
  /** The agent that would actually be sent to. */
  agent: Agent
  /** What the user chose, which is not always the same thing. */
  picked: Agent | null
  statuses: AgentStatuses
  onPick: (next: Agent) => void
}) {
  // `picked` is in here as well as `agent` so that an agent whose session has
  // gone away stays on screen: it is the one the user asked for, and watching
  // the toggle vanish while the message quietly goes somewhere else is worse
  // than seeing it sit there greyed out.
  const offered = AGENTS.filter(
    (candidate) => statuses[candidate]?.connected || candidate === agent || candidate === picked
  )
  if (offered.length < 2) return null

  return (
    <div className="flex items-center gap-0.5 rounded-md border bg-bg p-0.5">
      {offered.map((candidate) => (
        <button
          key={candidate}
          type="button"
          onClick={() => onPick(candidate)}
          aria-pressed={candidate === agent}
          title={
            statuses[candidate]?.connected
              ? `Send to ${AGENT_NAMES[candidate]}`
              : `No ${AGENT_NAMES[candidate]} session`
          }
          className={`rounded px-1.5 py-0.5 text-[10px] transition ${
            candidate === agent
              ? 'bg-primary-subtle text-primary-subtle-fg'
              : 'text-muted-fg hover:text-fg'
          }`}
        >
          {AGENT_NAMES[candidate]}
        </button>
      ))}
    </div>
  )
}

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
            <CloseIcon className="size-3" />
          </button>
        </div>
      ))}
    </div>
  )
}

/**
 * A command to run in the repository, with a button that copies it.
 *
 * The command is the whole fix, and it is long enough that retyping it from a
 * status line is where the typo happens.
 */
function CopyableCommand({ command }: { command: string }) {
  const [copied, setCopied] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  useEffect(() => () => clearTimeout(timer.current), [])

  return (
    <div className="mt-1 flex items-center gap-1.5 rounded-md border bg-bg py-0.5 ps-2 pe-0.5">
      <code className="min-w-0 flex-1 break-all font-mono text-[11px] text-fg">{command}</code>
      <Button
        intent="plain"
        size="xs"
        className="h-5 min-h-0 shrink-0 px-1.5 text-[11px] sm:min-h-0 sm:text-[11px]"
        onPress={() => {
          void writeClipboardText(command).then(
            () => {
              setCopied(true)
              clearTimeout(timer.current)
              timer.current = setTimeout(() => setCopied(false), 1600)
            },
            () => setCopied(false)
          )
        }}
        aria-label={`Copy ${command}`}
      >
        {copied ? <CheckIcon /> : null}
        {copied ? 'Copied' : 'Copy'}
      </Button>
    </div>
  )
}

interface AppComposerProps {
  /** The line being asked about, or `null` while the bar animates out. */
  reference: LineReference | null
  /** What to draw: the live reference, or the last one there was, so the bar
   *  keeps its content on the way out. */
  shown: LineReference
  /** Which agents have a session for this repository. */
  statuses: AgentStatuses
  /** Dismisses the bar — on Escape, on the ×, and once a message is away. */
  onClose: () => void
  /** A message was handed over. The bar closes straight after, so whatever
   *  says so has to live somewhere that stays on screen. */
  onSent: (agent: Agent, reference: LineReference) => void
}

export function AppComposer({ reference, shown, statuses, onClose, onSent }: AppComposerProps) {
  const [message, setMessage] = useState('')
  const [isSending, setIsSending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // `null` until the user picks one, which is what lets the default follow
  // whichever agent is actually available instead of freezing on first render.
  const [picked, setPicked] = useState<Agent | null>(rememberedAgent)
  const images = useAttachments()
  const input = useRef<HTMLInputElement>(null)

  const agent = preferredAgent(picked, statuses)
  const connected = statuses[agent]?.connected ?? false

  /*
   * Opening the bar starts a fresh message; a click while it is already open
   * only moves the target.
   *
   * Every click hands over a new reference, including a second click on the
   * same line — which is what someone does to re-read it, or to get focus back
   * into the field. Clearing on each one threw away whatever had been typed.
   * A click on a different line keeps the draft too: it is far more often a
   * correction than a new thought, and the label above animates to the new
   * line, so the move is seen rather than silent.
   *
   * Dismissal deliberately leaves the draft alone: the bar is still on screen
   * fading out, and watching the text blank out first reads as a glitch. The
   * next open is what clears it.
   */
  const previous = useRef<LineReference | null>(null)
  useEffect(() => {
    const wasOpen = previous.current !== null
    previous.current = reference
    if (!reference) return
    setError(null)
    input.current?.focus()
    if (wasOpen) return
    setMessage('')
    images.clear()
  }, [reference, images.clear])

  const draft = message.trim()
  // A screenshot with nothing typed is still a question worth sending, so
  // either half is enough on its own.
  const canSend =
    connected && !isSending && images.isReady && (draft.length > 0 || images.paths.length > 0)
  const failure = error ?? images.error
  const hint = connectHint(agent, statuses[agent])

  async function send() {
    if (!reference || !canSend) return
    setIsSending(true)
    setError(null)
    try {
      // The full path, not the shortened label on screen — the agent has to be
      // able to open the file. Same for the images: what crosses is where they
      // landed on the repository's own machine.
      await deliver(agent, composeMessage(reference, draft, images.paths))
      onSent(agent, reference)
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
      // Escape is caught here rather than on the field: with no session the
      // field is disabled and cannot hold focus, and the copy button can.
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          event.preventDefault()
          onClose()
        }
      }}
      className="composer pointer-events-auto w-full max-w-xl rounded-xl border bg-overlay p-2 shadow-lg"
    >
      <div className="flex items-center justify-between gap-2 px-1 pb-1.5">
        <span
          title={shown.label}
          className="flex min-w-0 font-mono text-[11px] text-primary-subtle-fg"
        >
          {/* The number sits outside the truncation: a clipped name is still
              recognisable, a clipped line number is not.

              Each half is keyed on what it shows, so a click on another line
              remounts only the part that changed and `.composer-target` lets
              the new value enter — otherwise one number silently becomes
              another and the eye has no way to catch the swap. */}
          <span key={shown.path} className="composer-target truncate">
            {shown.name}
          </span>
          <span key={shown.label} className="composer-target">
            :{shown.lineNumber}
            {/* The same mark the agent gets, so what is on screen is what is sent. */}
            {shown.removed && <span className="ms-1 text-muted-fg">(removed)</span>}
          </span>
        </span>
        <span className="flex shrink-0 items-center gap-1.5">
          <AgentPicker
            agent={agent}
            picked={picked}
            statuses={statuses}
            onPick={(next) => {
              setPicked(next)
              rememberAgent(next)
              setError(null)
              input.current?.focus()
            }}
          />
          <button
            type="button"
            onClick={onClose}
            aria-label="Dismiss"
            className="text-muted-fg hover:text-fg"
          >
            <CloseIcon className="size-3.5" />
          </button>
        </span>
      </div>

      <PastedImages images={images} />

      <TextField
        aria-label={`Message ${AGENT_NAMES[agent]} about ${shown.label}`}
        value={message}
        onChange={setMessage}
        isDisabled={isSending || !connected}
        autoComplete="off"
        spellCheck="false"
      >
        <InputGroup>
          <Input
            ref={input}
            placeholder={composerPlaceholder(agent, connected)}
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
            aria-label={`Send to ${AGENT_NAMES[agent]}`}
          >
            <SendIcon />
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

      {/* Nothing to send to is the one state where the field cannot help, so
          this says how to get something there — and the status poll enables
          the field on its own once the session is up. */}
      {!failure && hint && (
        <div className="px-1 pt-1.5 text-[11px] text-muted-fg">
          <p>{hint.text}</p>
          {hint.command && <CopyableCommand command={hint.command} />}
        </div>
      )}
    </div>
  )
}
