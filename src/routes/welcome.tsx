import { useEffect, useRef, useState } from 'react'
import type { CSSProperties } from 'react'
import { createFileRoute, useRouter } from '@tanstack/react-router'
import { ArrowRightIcon, FolderOpenIcon, XMarkIcon } from '@heroicons/react/16/solid'
import platypus from '@/assets/platypus.png'
import { Button } from '@/components/ui/button'
import { Loader } from '@/components/ui/loader'
import { CodeBranchOutline18 } from '@/icons'
import { forgetProject, listProjects, openProject, runIpc } from '@/lib/ipc'
import type { Project } from '@shared/contract'

/**
 * The landing page. It sits outside the `_app` layout on purpose — there is no
 * repository to build a sidebar from until something here is opened.
 */
export const Route = createFileRoute('/welcome')({
  // The history changes while the app is running, so never serve it from cache.
  shouldReload: true,
  staleTime: 0,
  loader: () => runIpc(listProjects),
  component: Welcome,
})

/** Rows past this all animate together, so the list always settles by ~360ms. */
const STAGGER_CAP = 4

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function Welcome() {
  // Read straight from the loader rather than mirroring into state, which
  // would keep showing the list as it was the first time this page mounted.
  const projects = Route.useLoaderData()
  const router = useRouter()
  const [path, setPath] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const input = useRef<HTMLInputElement>(null)

  useEffect(() => {
    input.current?.focus()
  }, [])

  async function open(value: string) {
    const target = value.trim()
    if (!target || busy) return
    setError(null)
    setBusy(target)
    try {
      await runIpc(openProject(target))
      // The sidebar and every loader below it read the newly opened repository.
      await router.navigate({ to: '/' })
    } catch (cause) {
      setError(errorMessage(cause))
      setBusy(null)
    }
  }

  async function forget(project: Project) {
    setError(null)
    try {
      await runIpc(forgetProject(project.path))
      await router.invalidate()
    } catch (cause) {
      setError(errorMessage(cause))
    }
  }

  return (
    <div className="flex min-h-svh flex-col items-center justify-center p-8">
      <div className="flex w-full max-w-xl flex-col gap-8">
        <header className="flex items-center gap-3.5">
          <img src={platypus} alt="" width={44} height={44} className="size-11 rounded-lg" />
          <div className="flex flex-col">
            <h1 className="font-medium text-lg tracking-tight">onlydiffs</h1>
            <p className="text-muted-fg text-sm">
              See what your agent changed. Talk it through.
            </p>
          </div>
        </header>

        <form
          className="flex flex-col gap-2"
          onSubmit={(event) => {
            event.preventDefault()
            void open(path)
          }}
        >
          <label htmlFor="repo-path" className="font-medium text-sm">
            Open a repository
          </label>
          <div className="flex gap-2">
            <input
              id="repo-path"
              ref={input}
              value={path}
              onChange={(event) => {
                setPath(event.target.value)
                setError(null)
              }}
              placeholder="~/Developer/my-project"
              spellCheck={false}
              autoComplete="off"
              className="min-w-0 flex-1 rounded-lg border border-border bg-bg px-3 py-2 font-mono text-sm outline-hidden placeholder:text-muted-fg focus:border-primary"
            />
            <Button type="submit" isDisabled={path.trim().length === 0 || busy !== null}>
              {busy === path.trim() ? <Loader /> : <ArrowRightIcon />}
              Open
            </Button>
          </div>
          <p className="text-muted-fg text-xs">
            Absolute, <span className="font-mono">~</span>-relative, or relative to your home
            folder. Any path inside a checkout opens its repository root.
          </p>
          {error && <p className="text-danger-subtle-fg text-sm">{error}</p>}
        </form>

        <section className="flex flex-col gap-2">
          <h2 className="font-medium text-muted-fg text-xs uppercase tracking-wide">
            Recent
          </h2>
          {projects.length === 0 ? (
            <p className="rounded-lg border border-border border-dashed px-3 py-6 text-center text-muted-fg text-sm">
              Nothing opened yet.
            </p>
          ) : (
            <ul className="flex flex-col">
              {projects.map((project, index) => (
                <li
                  key={project.path}
                  // Capped so a long history does not turn into a slow reveal:
                  // past the fifth row everything lands together.
                  style={{ '--i': Math.min(index, STAGGER_CAP) } as CSSProperties}
                  className="recent-project group/row flex items-center gap-1"
                >
                  <button
                    type="button"
                    onClick={() => void open(project.path)}
                    disabled={busy !== null}
                    className="flex min-w-0 flex-1 items-center gap-2.5 rounded-lg px-3 py-2 text-start hover:bg-secondary disabled:opacity-50"
                  >
                    {busy === project.path ? (
                      <Loader className="size-4 shrink-0" />
                    ) : (
                      <FolderOpenIcon className="size-4 shrink-0 text-muted-fg" />
                    )}
                    <span className="flex min-w-0 flex-col">
                      <span className="truncate font-medium text-sm">{project.name}</span>
                      <span className="truncate font-mono text-muted-fg text-xs">
                        {project.path}
                      </span>
                    </span>
                  </button>
                  <Button
                    intent="plain"
                    size="sq-sm"
                    aria-label={`Remove ${project.name} from recents`}
                    onPress={() => void forget(project)}
                    className="opacity-0 group-hover/row:opacity-100"
                  >
                    <XMarkIcon />
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </section>

        <p className="flex items-center gap-1.5 text-muted-fg text-xs">
          <CodeBranchOutline18 aria-hidden className="size-3 shrink-0" />
          Staged, unstaged, and untracked changes are kept apart.
        </p>
      </div>
    </div>
  )
}
