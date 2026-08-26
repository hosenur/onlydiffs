import { useEffect, useRef, useState } from 'react'
import { CheckIcon, PaperAirplaneIcon } from '@heroicons/react/16/solid'
import { Button } from '@/components/ui/button'
import { Loader } from '@/components/ui/loader'
import { useClaudeMessageDraft } from '@/lib/claude-message-draft'
import { runIpc, sendClaudeMessage } from '@/lib/ipc'

interface ChatMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

export function ClaudeMessageInput() {
  const { draft: message, setDraft: setMessage } = useClaudeMessageDraft()
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [error, setError] = useState<string | null>(null)
  const [isSending, setIsSending] = useState(false)
  const [received, setReceived] = useState(false)
  const receivedTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const nextTurn = useRef(0)
  const transcript = useRef<HTMLDivElement>(null)

  useEffect(() => () => clearTimeout(receivedTimer.current), [])

  useEffect(() => {
    const element = transcript.current
    if (element) element.scrollTop = element.scrollHeight
  }, [isSending, messages])

  async function send() {
    const content = message.trim()
    if (!content || isSending) return

    const turn = ++nextTurn.current
    const userMessage: ChatMessage = {
      id: `user-${turn}`,
      role: 'user',
      content,
    }

    setMessages((current) => [...current, userMessage])
    setMessage('')
    setError(null)
    setReceived(false)
    setIsSending(true)
    try {
      const reply = await runIpc(sendClaudeMessage(content))
      setMessages((current) => [
        ...current,
        { id: `assistant-${turn}`, role: 'assistant', content: reply },
      ])
      setReceived(true)
      clearTimeout(receivedTimer.current)
      receivedTimer.current = setTimeout(() => setReceived(false), 1800)
    } catch (cause) {
      setMessages((current) => current.filter(({ id }) => id !== userMessage.id))
      setMessage((current) => (current.trim() ? current : content))
      setError(errorMessage(cause))
    } finally {
      setIsSending(false)
    }
  }

  return (
    <section className="flex min-h-64 flex-1 flex-col gap-2.5 border-b border-sidebar-border p-3">
      <div className="flex items-baseline justify-between gap-2">
        <h2 className="font-medium text-sm">Message Claude</h2>
        <span className="font-mono text-muted-fg text-[10px]">Channel</span>
      </div>

      <div
        ref={transcript}
        aria-live="polite"
        className="min-h-20 flex-1 space-y-2 overflow-y-auto rounded-lg border border-border bg-overlay/50 p-2"
      >
        {messages.length === 0 && !isSending ? (
          <p className="px-1 py-2 text-muted-fg text-xs/5">
            Replies appear here after Claude finishes.
          </p>
        ) : (
          messages.map((item) => (
            <div
              key={item.id}
              className={
                item.role === 'user'
                  ? 'ml-5 rounded-md bg-primary-subtle px-2.5 py-2 text-primary-subtle-fg'
                  : 'mr-5 rounded-md border border-border bg-bg px-2.5 py-2 text-fg'
              }
            >
              <p className="mb-1 font-medium text-[10px] uppercase tracking-wide opacity-60">
                {item.role === 'user' ? 'You' : 'Claude'}
              </p>
              <p className="whitespace-pre-wrap break-words text-xs/5">{item.content}</p>
            </div>
          ))
        )}
        {isSending && (
          <div className="mr-5 flex items-center gap-2 rounded-md border border-border bg-bg px-2.5 py-2 text-muted-fg text-xs">
            <Loader variant="ring" />
            Waiting for Claude’s complete reply…
          </div>
        )}
      </div>

      <textarea
        aria-label="Message Claude Code"
        placeholder="Ask Claude about these changes…"
        value={message}
        disabled={isSending}
        onChange={(event) => setMessage(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
            event.preventDefault()
            void send()
          }
        }}
        rows={3}
        className="h-20 min-h-20 w-full flex-none resize-none rounded-lg border border-border bg-bg px-2.5 py-2 text-fg text-sm/5 outline-0 placeholder:text-muted-fg disabled:opacity-50 focus:border-ring focus:ring-2 focus:ring-ring/20"
      />

      {error && (
        <p role="alert" className="break-words text-danger-subtle-fg text-xs/5">
          {error}
        </p>
      )}

      <div className="flex items-center gap-2">
        <span className="min-w-0 flex-1 text-muted-fg text-xs">
          {received ? (
            <span className="inline-flex items-center gap-1 text-success-subtle-fg">
              <CheckIcon className="size-3" /> Reply received
            </span>
          ) : (
            '⌘↵ to send'
          )}
        </span>
        <Button
          intent="primary"
          size="xs"
          isDisabled={!message.trim()}
          isPending={isSending}
          onPress={() => void send()}
        >
          {isSending ? <Loader variant="ring" /> : <PaperAirplaneIcon />}
          Send
        </Button>
      </div>
    </section>
  )
}
