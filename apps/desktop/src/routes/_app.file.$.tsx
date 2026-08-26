import { useEffect, useState } from 'react'
import { useHotkey } from '@tanstack/react-hotkeys'
import { createFileRoute, getRouteApi, useRouter } from '@tanstack/react-router'
import { FileDiffCard } from '@/components/file-diff-card'
import { runIpc, stageFile } from '@/lib/ipc'

const layoutRoute = getRouteApi('/_app')

export const Route = createFileRoute('/_app/file/$')({
  component: SingleFile,
})

function SingleFile() {
  const { _splat } = Route.useParams()
  const { diff } = layoutRoute.useLoaderData()
  const router = useRouter()
  const [stageError, setStageError] = useState<string | null>(null)
  const [isStaging, setIsStaging] = useState(false)
  // A path can hold both halves; show each, staged first.
  const rows = diff.files.filter((file) => file.path === _splat)
  // Cmd+Enter stages the working-tree half. The index half is already there.
  const unstaged = rows.find((file) => !file.staged)

  useEffect(() => {
    setStageError(null)
  }, [_splat])

  useHotkey(
    'Mod+Enter',
    () => {
      if (!unstaged || isStaging) return
      setStageError(null)
      setIsStaging(true)
      void runIpc(stageFile({ path: unstaged.path, oldPath: unstaged.oldPath }))
        .then(() => router.invalidate())
        .catch((error: unknown) => {
          setStageError(error instanceof Error ? error.message : String(error))
        })
        .finally(() => setIsStaging(false))
    },
    {
      enabled: unstaged !== undefined && !isStaging,
      ignoreInputs: true,
      requireReset: true,
      meta: { name: 'Stage file', description: 'Add the current file to the index' },
    }
  )

  if (rows.length === 0) {
    return (
      <p className="p-5 text-center text-muted-fg">
        No change recorded for <span className="font-mono">{_splat}</span> — it may have been
        committed or reverted. Try Refresh.
      </p>
    )
  }

  return (
    <div className="flex flex-col gap-3.5">
      {stageError && (
        <p className="whitespace-pre-wrap p-5 font-mono text-danger-subtle-fg">{stageError}</p>
      )}
      {rows.map((file) => (
        <FileDiffCard key={file.id} file={file} bare />
      ))}
    </div>
  )
}
