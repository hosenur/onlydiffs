import { useEffect, useRef, useState } from 'react'
import { useHotkeys } from '@tanstack/react-hotkeys'
import { useParams, useRouter, useRouterState } from '@tanstack/react-router'
import { CheckIcon, TrashIcon } from '@heroicons/react/16/solid'
import { Badge } from '@onlydiffs/ui/badge'
import { Button } from '@onlydiffs/ui/button'
import { SidebarNav, SidebarTrigger } from '@/components/ui/sidebar'
import { useDiffLayout } from '@/lib/diff-layout'
import { writeClipboardText } from '@/lib/ipc'
import type { FileChange } from '@/types'

function FilePath({ path }: { path: string | undefined }) {
  const [copied, setCopied] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  useEffect(() => () => clearTimeout(timer.current), [])

  // Nothing selected: the nav has no file to name, and there is no longer an
  // "all changes" page for it to title.
  if (!path) {
    return null
  }

  return (
    <Button
      intent="plain"
      size="xs"
      className="min-w-0 justify-start font-mono font-normal"
      onPress={() => {
        void writeClipboardText(path).then(
          () => {
            setCopied(true)
            clearTimeout(timer.current)
            timer.current = setTimeout(() => setCopied(false), 1600)
          },
          () => setCopied(false)
        )
      }}
      aria-label="Copy relative path"
    >
      <span className="min-w-0 overflow-x-auto" title={path}>
        {path}
      </span>
      {copied && <CheckIcon />}
    </Button>
  )
}

export function AppSidebarNav({ files }: { files: FileChange[] }) {
  const router = useRouter()
  const { split, setSplit } = useDiffLayout()
  /*
   * `router.state` is a snapshot getter with nothing subscribed to it, so
   * reading it here would latch on the value from whatever render happened
   * last. `useRouterState` subscribes; `select` keeps the re-renders to the
   * flips of this one boolean.
   */
  const isLoading = useRouterState({ select: (state) => state.isLoading })
  // `strict: false` — only the `/file/$` route carries a splat.
  const params = useParams({ strict: false }) as { _splat?: string }

  // A path can hold both halves, so either one recording a removal counts.
  const isDeleted = files.some(
    (file) => file.path === params._splat && file.status === 'deleted'
  )

  // Cmd+B (sidebar) is SidebarProvider's own.
  // `r` and `s` are the only ways to reach refresh and split — neither has a
  // control in the UI.
  useHotkeys(
    [
      {
        hotkey: 'R',
        callback: () => {
          void router.invalidate()
        },
        options: { enabled: !isLoading, meta: { name: 'Refresh' } },
      },
      {
        hotkey: 'S',
        callback: () => setSplit(!split),
        options: { meta: { name: 'Toggle split / unified' } },
      },
    ],
    { requireReset: true }
  )

  return (
    <SidebarNav isSticky>
      <span className="flex min-w-0 items-center gap-x-4">
        <SidebarTrigger className="-ml-2.5" />
        <FilePath path={params._splat} />
      </span>

      {isDeleted && (
        <Badge intent="danger" className="ml-auto flex-none">
          <TrashIcon className="size-3 shrink-0" />
          Deleted
        </Badge>
      )}
    </SidebarNav>
  )
}
