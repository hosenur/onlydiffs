import { getRouteApi, Link } from '@tanstack/react-router'
import { fileIconUrl } from '@/lib/file-icon'
import { fileHref } from '@/lib/status'

const layoutRoute = getRouteApi('/_app')

/** Lists each changed path once before a file is picked. */
export function AppNoSelection() {
  const { diff } = layoutRoute.useLoaderData()
  // A path can have staged and unstaged changes, but both open the same file.
  const paths = [...new Set(diff.files.map((file) => file.path))]

  if (paths.length === 0) {
    return <p className="p-5 text-center text-muted-fg">Working tree is clean.</p>
  }

  return (
    <section className="mx-auto w-full max-w-3xl p-5">
      <div className="mb-3 flex items-baseline justify-between gap-4">
        <h1 className="font-medium">Changed files</h1>
        <span className="font-mono text-xs text-muted-fg">
          {paths.length} {paths.length === 1 ? 'file' : 'files'}
        </span>
      </div>

      <ul className="overflow-hidden rounded-lg border bg-overlay">
        {paths.map((path) => (
          <li key={path} className="border-b last:border-b-0">
            <Link
              to={fileHref(path)}
              title={path}
              className="flex min-w-0 items-center gap-2.5 px-3 py-2.5 hover:bg-muted"
            >
              <img
                src={fileIconUrl(path)}
                alt=""
                width={16}
                height={16}
                className="size-4 shrink-0"
              />
              <span className="truncate font-mono text-xs">{path}</span>
            </Link>
          </li>
        ))}
      </ul>
    </section>
  )
}
