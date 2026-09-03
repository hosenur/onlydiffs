import { useEffect, useRef, useState } from 'react'
import type { CSSProperties } from 'react'
import { createFileRoute, Link, useRouter } from '@tanstack/react-router'
import { ArrowRightIcon, ServerStackIcon, XMarkIcon } from '@heroicons/react/16/solid'
import platypus from '@/assets/platypus.png'
import { Button } from '@onlydiffs/ui/button'
import { Loader } from '@onlydiffs/ui/loader'
import { useProjectIcons } from '@/hooks/use-project-icons'
import { useSettingsHotkey } from '@/hooks/use-settings-hotkey'
import {
  RemoteProjectsMenu,
  useRemoteProjectsHotkey,
} from '@/components/remote-projects-menu'
import { CodeBranchOutline18, GearOutline18 } from '@/icons'
import { projectInitials, projectTint } from '@/lib/project-identity'
import { forgetProject, listProjects, openProject, openRemoteProject } from '@/lib/ipc'
import { useSsh } from '@/lib/ssh'
import type { Project } from '@shared/contract'

/**
 * The landing page, and the app's index: a cold launch has no project open, so
 * serving the picker from `/` saves the `currentProject` round-trip that a
 * redirect would have cost. It sits outside the `_app` layout on purpose —
 * there is no repository to build a sidebar from until something here is opened.
 */
export const Route = createFileRoute('/')({
  // The history changes while the app is running, so never serve it from cache.
  shouldReload: true,
  staleTime: 0,
  loader: () => listProjects(),
  component: Welcome,
})

/** Rows past this all animate together, so the list always settles by ~360ms. */
const STAGGER_CAP = 4

interface RecentProjectStyle extends CSSProperties {
  '--i': number
}

function recentProjectStyle(index: number): RecentProjectStyle {
  return { '--i': Math.min(index, STAGGER_CAP) }
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
  const projectButtons = useRef<Array<HTMLButtonElement | null>>([])
  const ssh = useSsh()
  const [isRemoteOpen, setIsRemoteOpen] = useState(false)
  useRemoteProjectsHotkey(() => setIsRemoteOpen(true))
  useProjectIcons()
  // The palette that carries this shortcut everywhere else lives under `_app`,
  // and a fresh install with no repository yet never gets there.
  useSettingsHotkey()

  useEffect(() => {
    input.current?.focus()
  }, [])

  function focusProject(index: number) {
    if (index < 0) {
      input.current?.focus()
      return
    }
    const target = Math.min(index, projects.length - 1)
    projectButtons.current[target]?.focus()
  }

  async function open(value: string) {
    const target = value.trim()
    if (!target || busy) return
    setError(null)
    setBusy(target)
    try {
      await openProject(target)
      // The sidebar and every loader below it read the newly opened repository.
      await router.navigate({ to: '/diff' })
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
      setBusy(null)
    }
  }

  /// Reopens a project from the list, on whichever machine it is on.
  async function reopen(project: Project) {
    if (project.host === null) {
      await open(project.root)
      return
    }
    // A remembered remote project carries its host, so nothing has to be
    // chosen: the project already knows where it lives.
    setError(null)
    setBusy(project.path)
    try {
      if (!ssh.isConnected(project.host) && !(await ssh.connect(project.host))) {
        setBusy(null)
        return
      }
      await openRemoteProject(project.host, project.root)
      await router.navigate({ to: '/diff' })
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
      setBusy(null)
    }
  }

  async function forget(project: Project) {
    setError(null)
    try {
      await forgetProject(project.path)
      await router.invalidate()
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    }
  }

  return (
    <>
      <RemoteProjectsMenu
        isOpen={isRemoteOpen}
        onOpenChange={setIsRemoteOpen}
        projects={projects}
      />
    <div className="flex min-h-svh flex-col items-center justify-center p-8">
      <div className="flex w-full max-w-xl flex-col gap-8">
        <header className="flex items-center gap-3.5">
          <img src={platypus} alt="" width={44} height={44} className="size-11 rounded-lg" />
          <div className="flex flex-1 flex-col">
            <h1 className="font-medium text-lg tracking-tight">onlydiffs</h1>
            <p className="text-muted-fg text-sm">
              See what your agent changed. Talk it through.
            </p>
          </div>
          {/* The one way into settings before a repository is open, which is
              exactly when the Groq key is most likely to be missing. */}
          <Link
            to="/settings"
            aria-label="Settings"
            className="grid size-9 place-items-center rounded-lg text-muted-fg outline-hidden hover:bg-secondary hover:text-fg focus-visible:ring-2 focus-visible:ring-primary"
          >
            <GearOutline18 aria-hidden className="size-5" />
          </Link>
        </header>

        <form
          className="flex flex-col gap-2"
          onSubmit={(event) => {
            event.preventDefault()
            void open(path)
          }}
        >
          <div className="flex items-baseline justify-between gap-2">
            <label htmlFor="repo-path" className="font-medium text-sm">
              Open a repository
            </label>
            {/* One door to the remote flow rather than two. The palette holds
                adding a host and opening a path on one, which is the same
                errand a minute apart. */}
            <button
              type="button"
              onClick={() => setIsRemoteOpen(true)}
              className="flex items-center gap-1 text-muted-fg text-xs hover:text-fg"
            >
              <ServerStackIcon aria-hidden className="size-3" />
              On another machine
              <kbd className="font-mono">⌃⌘O</kbd>
            </button>
          </div>
          <div className="flex gap-2">
            <input
              id="repo-path"
              ref={input}
              value={path}
              onChange={(event) => {
                setPath(event.target.value)
                setError(null)
              }}
              onKeyDown={(event) => {
                if (busy !== null || projects.length === 0) return
                if (event.key === 'ArrowDown') {
                  event.preventDefault()
                  focusProject(0)
                } else if (event.key === 'ArrowUp') {
                  event.preventDefault()
                  focusProject(projects.length - 1)
                }
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
            Projects
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
                  style={recentProjectStyle(index)}
                  className="recent-project group/row flex items-center gap-1"
                >
                  <button
                    ref={(element) => {
                      projectButtons.current[index] = element
                    }}
                    type="button"
                    onClick={() => void reopen(project)}
                    onKeyDown={(event) => {
                      if (event.key === 'ArrowDown') {
                        event.preventDefault()
                        focusProject(index + 1)
                      } else if (event.key === 'ArrowUp') {
                        event.preventDefault()
                        focusProject(index - 1)
                      }
                    }}
                    disabled={busy !== null}
                    className="flex min-w-0 flex-1 items-center gap-2.5 rounded-lg px-3 py-2 text-start hover:bg-secondary disabled:opacity-50"
                  >
                    {busy === project.path ? (
                      <Loader className="size-4 shrink-0" />
                    ) : project.icon ? (
                      <img
                        src={project.icon.dataUrl}
                        alt=""
                        draggable={false}
                        className="size-4 shrink-0 rounded-sm bg-white object-contain"
                      />
                    ) : (
                      <span
                        aria-hidden
                        className={`grid size-4 shrink-0 select-none place-items-center rounded-sm font-semibold text-[7px] leading-none ${projectTint(project.path)}`}
                      >
                        {projectInitials(project.name)}
                      </span>
                    )}
                    <span className="flex min-w-0 flex-col">
                      <span className="flex min-w-0 items-center gap-1.5">
                        <span className="truncate font-medium text-sm">{project.name}</span>
                        {project.host && (
                          <span className="shrink-0 rounded border border-border px-1 font-mono text-[10px] text-muted-fg">
                            {project.host}
                          </span>
                        )}
                      </span>
                      <span className="truncate font-mono text-muted-fg text-xs">
                        {project.root}
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
    </>
  )
}
