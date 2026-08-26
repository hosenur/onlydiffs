import { useEffect, useRef, useState } from 'react'
import {
  CheckIcon,
  ClipboardDocumentIcon,
  SparklesIcon,
} from '@heroicons/react/16/solid'
import { Button } from '@/components/ui/button'
import { Loader } from '@/components/ui/loader'
import { generateCommitMessage, runIpc, writeClipboardText } from '@/lib/ipc'
import type { FileChange } from '@/types'

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

export function CommitMessageGenerator({ files }: { files: FileChange[] }) {
  const [message, setMessage] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [isGenerating, setIsGenerating] = useState(false)
  const [copied, setCopied] = useState(false)
  const generation = useRef(0)
  const copyTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  useEffect(() => {
    generation.current += 1
    setMessage('')
    setError(null)
    setIsGenerating(false)
    setCopied(false)
  }, [files])

  useEffect(() => () => clearTimeout(copyTimer.current), [])

  async function generate() {
    const request = ++generation.current
    setError(null)
    setIsGenerating(true)

    try {
      const generated = await runIpc(generateCommitMessage)
      if (request === generation.current) {
        setMessage(generated)
      }
    } catch (cause) {
      if (request === generation.current) {
        setError(errorMessage(cause))
      }
    } finally {
      if (request === generation.current) {
        setIsGenerating(false)
      }
    }
  }

  function copyMessage() {
    void runIpc(writeClipboardText(message)).then(
      () => {
        setCopied(true)
        clearTimeout(copyTimer.current)
        copyTimer.current = setTimeout(() => setCopied(false), 1600)
      },
      (cause: unknown) => setError(errorMessage(cause))
    )
  }

  return (
    <section className="flex shrink-0 flex-col gap-2.5 border-b border-sidebar-border p-3">
      <div className="flex items-baseline justify-between gap-2">
        <h2 className="font-medium text-sm">Commit message</h2>
        <span className="font-mono text-muted-fg text-[10px]">gpt-oss-120b</span>
      </div>

      {message && (
        <textarea
          aria-label="Generated commit message"
          value={message}
          onChange={(event) => setMessage(event.target.value)}
          rows={5}
          className="w-full resize-y rounded-lg border border-border bg-bg px-2.5 py-2 font-mono text-fg text-xs/5 outline-0 focus:border-ring focus:ring-2 focus:ring-ring/20"
        />
      )}

      {error && (
        <p role="alert" className="break-words text-danger-subtle-fg text-xs/5">
          {error}
        </p>
      )}

      <div className="flex gap-2">
        <Button
          intent="primary"
          size="xs"
          className="flex-1"
          isDisabled={files.length === 0}
          isPending={isGenerating}
          onPress={() => void generate()}
        >
          {isGenerating ? <Loader variant="ring" /> : <SparklesIcon />}
          {message ? 'Regenerate' : 'Generate'}
        </Button>

        {message && (
          <Button
            intent="outline"
            size="sq-xs"
            aria-label={copied ? 'Commit message copied' : 'Copy commit message'}
            onPress={copyMessage}
          >
            {copied ? <CheckIcon /> : <ClipboardDocumentIcon />}
          </Button>
        )}
      </div>
    </section>
  )
}
