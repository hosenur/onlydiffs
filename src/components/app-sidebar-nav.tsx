import { useEffect, useRef, useState } from 'react'
import { useHotkeys } from '@tanstack/react-hotkeys'
import { useParams, useRouter, useRouterState } from '@tanstack/react-router'
import { ArrowPathIcon, CheckIcon, TrashIcon } from '@heroicons/react/16/solid'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Loader } from '@/components/ui/loader'
import { SidebarNav, SidebarTrigger } from '@/components/ui/sidebar'
import { useDiffLayout } from '@/lib/diff-layout'
import { runIpc, writeClipboardText } from '@/lib/ipc'
import type { FileChange } from '@/types'

function FilePath({ path }: { path: string | undefined }) {
  const [copied, setCopied] = useState(false)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  useEffect(() => () => clearTimeout(timer.current), [])

  if (!path) {
    return <span className="text-sm">All changes</span>
  }

  return (
    <Button
      intent="plain"
      size="xs"
      className="min-w-0 justify-start font-mono font-normal"
      onPress={() => {
        void runIpc(writeClipboardText(path)).then(
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
  // `s` is the only way to reach split until the setting gets a home in the UI.
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

      <span className="ml-auto flex flex-none items-center gap-2">
        {isDeleted && (
          <Badge intent="danger" className="shrink-0">
            <TrashIcon className="size-3 shrink-0" />
            Deleted
          </Badge>
        )}

        <Button
          intent="outline"
          size="xs"
          onPress={() => void router.invalidate()}
          isPending={isLoading}
          aria-label="Refresh diff"
        >
          {isLoading ? <Loader variant="ring" /> : <ArrowPathIcon />}
          Refresh
        </Button>
      </span>
    </SidebarNav>
  )
}
