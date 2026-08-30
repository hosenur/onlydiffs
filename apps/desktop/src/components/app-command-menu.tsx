import { useState } from 'react'
import { useHotkey } from '@tanstack/react-hotkeys'
import { useNavigate, useParams, useRouter } from '@tanstack/react-router'
import {
  ArrowDownTrayIcon,
  ArrowPathIcon,
  CheckIcon,
  ComputerDesktopIcon,
  MoonIcon,
  PlusIcon,
  SparklesIcon,
  SunIcon,
} from '@heroicons/react/16/solid'
import { type Theme, useTheme } from '@/components/theme-provider'
import {
  CommandMenu,
  CommandMenuDescription,
  CommandMenuItem,
  CommandMenuLabel,
  CommandMenuList,
  CommandMenuSearch,
  CommandMenuSection,
  CommandMenuShortcut,
} from '@/components/ui/command-menu'
import { Loader } from '@onlydiffs/ui/loader'
import { IsometricCubeIcon } from '@/icons'
import { fileIconUrl } from '@/lib/file-icon'
import {
  commitAll,
  generateCommitMessage,
  openProject,
  stageFile,
  writeClipboardText,
} from '@/lib/ipc'
import { projectInitials, projectTint } from '@/lib/project-identity'
import { fileHref } from '@/lib/status'
import { type UpdateValue, useUpdate } from '@/lib/update'
import type { FileChange } from '@/types'
import type { Project } from '@shared/contract'

interface AppCommandMenuProps {
  files: FileChange[]
  projects: Project[]
  currentProjectPath: string
}

/** Which long-running action is in flight, so the palette can say so. */
type Busy = 'generate' | 'commit' | 'stage' | 'project' | null

/**
 * One row per theme rather than a single cycling "Toggle theme": the palette
 * is a search field, so typing "dark" should land on Dark instead of on a
 * command whose effect depends on a current state you cannot see from here.
 *
 * This is the only way to change the theme — nothing else in the app calls
 * `setTheme`.
 */
const THEMES: { value: Theme; label: string; Icon: typeof SunIcon }[] = [
  { value: 'light', label: 'Light', Icon: SunIcon },
  { value: 'dark', label: 'Dark', Icon: MoonIcon },
  { value: 'system', label: 'System', Icon: ComputerDesktopIcon },
]

/** Release notes are a changelog; a palette row has space for its headline. */
function headline(notes: string) {
  return notes.trim().split('\n')[0]
}

/**
 * The row that only exists when a newer release is waiting. First in the list:
 * it is the rarest thing here, and the one worth acting on when it shows up.
 */
function UpdateSection({ update }: { update: UpdateValue }) {
  if (!update.offer) return null
  const { version, notes } = update.offer

  return (
    <CommandMenuSection label="Update">
      <CommandMenuItem
        textValue={`Install update ${version ?? ''}`}
        onAction={() => void update.install()}
      >
        {update.isInstalling ? <Loader /> : <ArrowDownTrayIcon />}
        <CommandMenuLabel>
          {/* No success state to render: installing relaunches the app. */}
          {update.isInstalling ? 'Downloading…' : `Install update — v${version}`}
        </CommandMenuLabel>
        {notes && <CommandMenuDescription>{headline(notes)}</CommandMenuDescription>}
      </CommandMenuItem>
    </CommandMenuSection>
  )
}

/**
 * The palette's one status line, shared by every action in it: a failure if
 * there is one, otherwise whatever the last action had to say.
 */
function statusLine(failures: (string | null)[], note: string | null) {
  const failure = failures.find((message) => message !== null) ?? null
  return { message: failure ?? note, isFailure: failure !== null }
}

interface OtherProjectsSectionProps {
  projects: Project[]
  currentPath: string
  isDisabled: boolean
  onStart: () => void
  onOpened: () => void
  onError: (message: string) => void
  onFinish: () => void
}

function OtherProjectsSection({
  projects,
  currentPath,
  isDisabled,
  onStart,
  onOpened,
  onError,
  onFinish,
}: OtherProjectsSectionProps) {
  const router = useRouter()
  const [opening, setOpening] = useState<string | null>(null)
  const otherProjects = projects.filter((project) => project.path !== currentPath)

  if (otherProjects.length === 0) return null

  async function open(project: Project) {
    if (opening !== null || isDisabled) return
    setOpening(project.path)
    onStart()
    try {
      await openProject(project.path)
      await router.navigate({ to: '/diff', replace: true })
      await router.invalidate()
      onOpened()
    } catch (cause) {
      onError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setOpening(null)
      onFinish()
    }
  }

  return (
    <CommandMenuSection label="Other projects">
      {otherProjects.map((project) => (
        <CommandMenuItem
          key={project.path}
          textValue={`Open project ${project.name} ${project.path}`}
          isDisabled={opening !== null || isDisabled}
          onAction={() => void open(project)}
        >
          {opening === project.path ? (
            <Loader />
          ) : project.icon ? (
            <img
              src={project.icon.dataUrl}
              alt=""
              draggable={false}
              className="me-1.5 size-4 rounded-sm bg-white object-contain"
            />
          ) : (
            <span
              aria-hidden
              className={`me-1.5 grid size-4 shrink-0 select-none place-items-center rounded-sm font-semibold text-[7px] leading-none ${projectTint(project.path)}`}
            >
              {projectInitials(project.name)}
            </span>
          )}
          <CommandMenuLabel>{project.name}</CommandMenuLabel>
          <CommandMenuDescription className="max-w-72 truncate">
            {project.path}
          </CommandMenuDescription>
        </CommandMenuItem>
      ))}
    </CommandMenuSection>
  )
}

export function AppCommandMenu({ files, projects, currentProjectPath }: AppCommandMenuProps) {
  const [isOpen, setIsOpen] = useState(false)
  const [busy, setBusy] = useState<Busy>(null)
  const [note, setNote] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const navigate = useNavigate()
  const router = useRouter()
  const { theme, setTheme } = useTheme()
  const update = useUpdate()
  // SAFETY: `strict: false` lets the palette render on both `/diff` and
  // `/file/$`; only the latter route can contribute the optional splat.
  const params = useParams({ strict: false }) as { _splat?: string }
  const current = params._splat

  useHotkey(
    { key: 'K', mod: true },
    () => setIsOpen((open) => !open),
    {
      enabled: true,
      ignoreInputs: false,
      requireReset: true,
      meta: { name: 'Command menu', description: 'Open the command menu' },
    }
  )

  // A path staged *and* modified again is two rows in the diff but one entry
  // here, since both lead to the same file.
  const changed = [...new Map(files.map((file) => [file.path, file])).values()]

  // Only the working-tree half can be staged; the index half is already there.
  const stageable = current
    ? files.find((file) => file.path === current && !file.staged)
    : undefined

  async function generate() {
    if (busy) return
    setBusy('generate')
    setError(null)
    setNote(null)
    try {
      const generated = await generateCommitMessage()
      await writeClipboardText(generated)
      setNote(`Copied — ${generated.split('\n')[0]}`)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setBusy(null)
    }
  }

  async function stage() {
    if (busy || !stageable) return
    setBusy('stage')
    setError(null)
    setNote(null)
    try {
      await stageFile({ path: stageable.path, oldPath: stageable.oldPath })
      setNote(`Staged ${stageable.path}`)
      await router.invalidate()
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setBusy(null)
    }
  }

  async function commit() {
    if (busy) return
    setBusy('commit')
    setError(null)
    setNote(null)
    try {
      // Always a fresh read of the current diff. Reusing a message from an
      // earlier Generate would commit a description of the diff as it was
      // then — stale the moment anything else is edited.
      const subject = await generateCommitMessage()
      const head = await commitAll(subject)
      // Left open on purpose: this note is the only confirmation there is,
      // and closing straight away takes the commit hash with it.
      setNote(`Committed ${head}`)
      await router.invalidate()
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setBusy(null)
    }
  }

  const status = statusLine([error, update.error], note)

  return (
    <CommandMenu isOpen={isOpen} onOpenChange={setIsOpen}>
      <CommandMenuSearch placeholder="Search files, or run a command…" />
      <CommandMenuList>
        <UpdateSection update={update} />

        {stageable && (
          <CommandMenuSection label="File">
            <CommandMenuItem
              textValue={`Stage ${stageable.path}`}
              onAction={() => void stage()}
            >
              {busy === 'stage' ? <Loader /> : <PlusIcon />}
              <CommandMenuLabel>Add to staging</CommandMenuLabel>
              {/* Description and shortcut share a grid cell, so only one can
                  show. The path says which file this will stage; ⌘↵ is in the
                  README and on the file view already. */}
              <CommandMenuDescription>{stageable.path}</CommandMenuDescription>
            </CommandMenuItem>
          </CommandMenuSection>
        )}

        <CommandMenuSection label="Commit">
          <CommandMenuItem
            textValue="Generate commit message"
            onAction={() => void generate()}
          >
            {busy === 'generate' ? <Loader /> : <SparklesIcon />}
            <CommandMenuLabel>Generate commit message</CommandMenuLabel>
          </CommandMenuItem>

          <CommandMenuItem
            textValue="Commit all"
            onAction={() => void commit()}
          >
            {busy === 'commit' ? <Loader /> : <CheckIcon />}
            <CommandMenuLabel>Commit all</CommandMenuLabel>
            <CommandMenuShortcut>
              {changed.length} {changed.length === 1 ? 'file' : 'files'}
            </CommandMenuShortcut>
          </CommandMenuItem>
        </CommandMenuSection>

        <CommandMenuSection label="Project">
          {/* `r` does the same thing, but a bare key is ignored while focus is
              in a text field -- and the Claude toolbar takes focus whenever a
              line is referenced. This row is the one that always works. */}
          <CommandMenuItem
            textValue="Refresh Reload the diff"
            onAction={() => {
              setIsOpen(false)
              void router.invalidate()
            }}
          >
            <ArrowPathIcon />
            <CommandMenuLabel>Refresh</CommandMenuLabel>
            <CommandMenuShortcut>r</CommandMenuShortcut>
          </CommandMenuItem>

          <CommandMenuItem
            textValue="Switch project Open another repository"
            onAction={() => {
              setIsOpen(false)
              void navigate({ to: '/' })
            }}
          >
            <IsometricCubeIcon />
            <CommandMenuLabel>Switch project</CommandMenuLabel>
          </CommandMenuItem>
        </CommandMenuSection>

        <CommandMenuSection label="Appearance">
          {THEMES.map(({ value, label, Icon }) => (
            <CommandMenuItem
              key={value}
              textValue={`Theme ${label}`}
              onAction={() => {
                setTheme(value)
                setIsOpen(false)
              }}
            >
              <Icon />
              <CommandMenuLabel>{label}</CommandMenuLabel>
              {theme === value && (
                <CommandMenuShortcut>active</CommandMenuShortcut>
              )}
            </CommandMenuItem>
          ))}
        </CommandMenuSection>

        {changed.length > 0 && (
          <CommandMenuSection label="Changed files">
            {changed.map((file) => (
              <CommandMenuItem
                key={file.path}
                textValue={file.path}
                onAction={() => {
                  setIsOpen(false)
                  void navigate({ to: fileHref(file.path) })
                }}
              >
                {/* `me-1.5` to match the sidebar's `gap-1.5`. The row's grid
                    spaces its icon column with `me-(--me-icon)`, but that rule
                    only selects svg children, so an img would sit flush
                    against the name. */}
                <img
                  src={fileIconUrl(file.path)}
                  alt=""
                  width={16}
                  height={16}
                  className="me-1.5 size-4 shrink-0"
                />
                <CommandMenuLabel>{file.path}</CommandMenuLabel>
                <CommandMenuShortcut>
                  {file.staged ? 'staged' : 'unstaged'}
                </CommandMenuShortcut>
              </CommandMenuItem>
            ))}
          </CommandMenuSection>
        )}

        <OtherProjectsSection
          projects={projects}
          currentPath={currentProjectPath}
          isDisabled={busy !== null}
          onStart={() => {
            setBusy('project')
            setError(null)
            setNote(null)
          }}
          onOpened={() => setIsOpen(false)}
          onError={setError}
          onFinish={() => setBusy(null)}
        />
      </CommandMenuList>

      {status.message && (
        <p
          role={status.isFailure ? 'alert' : 'status'}
          className={`border-border border-t px-4 py-2.5 text-xs ${
            status.isFailure ? 'text-danger-subtle-fg' : 'text-muted-fg'
          }`}
        >
          {status.message}
        </p>
      )}
    </CommandMenu>
  )
}
