import { createFileRoute, getRouteApi } from '@tanstack/react-router'
import { FileDiffCard } from '@/components/file-diff-card'
import type { FileChange } from '@/types'

const layoutRoute = getRouteApi('/_app')

export const Route = createFileRoute('/_app/')({
  component: AllChanges,
})

function Group({ label, files }: { label: string; files: FileChange[] }) {
  if (files.length === 0) return null
  return (
    <section className="flex flex-col gap-3.5">
      <h2 className="text-muted-fg text-xs uppercase tracking-wide">
        {label} · {files.length}
      </h2>
      {files.map((file) => (
        <FileDiffCard key={file.id} file={file} />
      ))}
    </section>
  )
}

function AllChanges() {
  const { diff } = layoutRoute.useLoaderData()

  if (diff.files.length === 0) {
    return (
      <p className="p-5 text-center text-muted-fg">Working tree is clean — nothing to diff.</p>
    )
  }

  return (
    <div className="flex flex-col gap-6">
      <Group label="Staged" files={diff.files.filter((file) => file.staged)} />
      <Group label="Unstaged" files={diff.files.filter((file) => !file.staged)} />
    </div>
  )
}
