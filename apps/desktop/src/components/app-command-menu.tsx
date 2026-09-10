import { useState } from 'react'
import { useHotkey } from '@tanstack/react-hotkeys'
import { useNavigate, useParams, useRouter } from '@tanstack/react-router'
import { type Theme, useTheme } from '@/components/theme-provider'
import { useProjectOpener } from '@/hooks/use-project-opener'
import { useSettingsHotkey } from '@/hooks/use-settings-hotkey'
import {
  RemoteProjectsMenu,
  useRemoteProjectsHotkey,
} from '@/components/remote-projects-menu'
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
import { commitAll, generateCommitMessage, stageFile } from '@/lib/ipc'
import { projectInitials, projectTint } from '@/lib/project-identity'
import { fileHref } from '@/lib/status'
import { type UpdateValue, useUpdate } from '@/lib/update'
import type { FileChange } from '@/types'
import type { Project } from '@shared/contract'
import { DarkThemeIcon, DownloadIcon, LightThemeIcon, PlusIcon, RefreshIcon, RemoteIcon, SettingsIcon, SparkleIcon, SystemThemeIcon } from '@onlydiffs/ui/icons'

interface AppCommandMenuProps {
  files: FileChange[]
  projects: Project[]
  currentProjectPath: string
}

/** Which long-running action is in flight, so the palette can say so. */
type Busy = 'commit' | 'stage' | null

/**
 * One row per theme rather than a single cycling "Toggle theme": the palette
 * is a search field, so typing "dark" should land on Dark instead of on a
 * command whose effect depends on a current state you cannot see from here.
 *
 * The settings page offers the same three; both go through `useTheme`, which
 * is the only thing in the app that calls `setTheme`.
 */
const THEMES: { value: Theme; label: string; Icon: typeof LightThemeIcon }[] = [
  { value: 'light', label: 'Light', Icon: LightThemeIcon },
  { value: 'dark', label: 'Dark', Icon: DarkThemeIcon },
  { value: 'system', label: 'System', Icon: SystemThemeIcon },
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
        {update.isInstalling ? <Loader /> : <DownloadIcon />}
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
  /** The project being opened, if any, so its row can show the spinner. */
  openingPath: string | null
  isDisabled: boolean
  onOpen: (project: Project) => void
}

/**
 * Rows only. Opening a project is `useProjectOpener`'s job, and the palette
 * has to hear about it anyway to close itself, so the sequence stays up there.
 */
function OtherProjectsSection({
  projects,
  currentPath,
  openingPath,
  isDisabled,
  onOpen,
}: OtherProjectsSectionProps) {
  const otherProjects = projects.filter((project) => project.path !== currentPath)

  if (otherProjects.length === 0) return null

  return (
    <CommandMenuSection label="Other projects">
      {otherProjects.map((project) => (
        <CommandMenuItem
          key={project.path}
          textValue={`Open project ${project.name} ${project.path}`}
          isDisabled={isDisabled}
          onAction={() => onOpen(project)}
        >
          {openingPath === project.path ? (
            <Loader />
          ) : project.icon ? (
            <img
              src={project.icon.dataUrl}
              alt=""
              draggable={false}
              className="me-icon size-4 rounded-sm bg-white object-contain"
            />
          ) : (
            <span
              aria-hidden
              className={`me-icon grid size-4 shrink-0 select-none place-items-center rounded-sm font-semibold text-[7px] leading-none ${projectTint(project.path)}`}
            >
              {projectInitials(project.name)}
            </span>
          )}
          {/* The path stays searchable through `textValue` but is not drawn:
              a long one pushed the name onto a second line under the icon,
              and the rows above it have nothing on their right to line up
              with. */}
          <CommandMenuLabel>{project.name}</CommandMenuLabel>
        </CommandMenuItem>
      ))}
    </CommandMenuSection>
  )
}

export function AppCommandMenu({ files, projects, currentProjectPath }: AppCommandMenuProps) {
  const [isOpen, setIsOpen] = useState(false)
  const [isRemoteOpen, setIsRemoteOpen] = useState(false)
  const [busy, setBusy] = useState<Busy>(null)
  const [note, setNote] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const navigate = useNavigate()
  const router = useRouter()
  const { theme, setTheme } = useTheme()
  const update = useUpdate()
  const { openingPath, failure, open } = useProjectOpener()
  // Registered here rather than in the layout: this component is already the
  // app shell's keyboard surface, and it renders wherever the palette does.
  useSettingsHotkey()
  useRemoteProjectsHotkey(() => {
    setIsOpen(false)
    setIsRemoteOpen(true)
  })
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

  // Nothing else in the palette runs while a project is being opened: the
  // repository every other command reads is about to change underneath it.
  const isBusy = busy !== null || openingPath !== null

  async function stage() {
    if (isBusy || !stageable) return
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

  /*
   * One step: read the whole diff, write the message, `git add -A`, commit.
   * There used to be a separate "generate" that only copied the message to the
   * clipboard, but nobody wanted the message without the commit, and a message
   * generated ahead of time describes the diff as it was then — stale the
   * moment anything else is edited.
   */
  async function commit() {
    if (isBusy) return
    setBusy('commit')
    setError(null)
    setNote(null)
    try {
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

  const status = statusLine([error, failure?.message ?? null, update.error], note)

  async function switchProject(project: Project) {
    setError(null)
    setNote(null)
    // Left open on a failure so the status line below can say what went wrong.
    if (await open(project)) setIsOpen(false)
  }

  return (
    <>
      <RemoteProjectsMenu
        isOpen={isRemoteOpen}
        onOpenChange={setIsRemoteOpen}
        projects={projects}
      />
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
          {/* Searchable by either name it has gone by: the row generates the
              message *and* commits everything, staged or not. */}
          <CommandMenuItem
            textValue="Generate commit message and commit all"
            onAction={() => void commit()}
          >
            {busy === 'commit' ? <Loader /> : <SparkleIcon />}
            <CommandMenuLabel>Generate message and commit all</CommandMenuLabel>
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
            <RefreshIcon />
            <CommandMenuLabel>Refresh</CommandMenuLabel>
            <CommandMenuShortcut>r</CommandMenuShortcut>
          </CommandMenuItem>

          <CommandMenuItem
            textValue="Settings Groq API key appearance"
            onAction={() => {
              setIsOpen(false)
              void navigate({ to: '/settings' })
            }}
          >
            <SettingsIcon />
            <CommandMenuLabel>Settings</CommandMenuLabel>
            <CommandMenuShortcut>⌘,</CommandMenuShortcut>
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

          <CommandMenuItem
            textValue="Open remote project ssh server another machine host"
            onAction={() => {
              setIsOpen(false)
              setIsRemoteOpen(true)
            }}
          >
            <RemoteIcon />
            <CommandMenuLabel>Open remote project</CommandMenuLabel>
            <CommandMenuShortcut>⌃⌘O</CommandMenuShortcut>
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
                {/* `me-icon`, by hand: the row's own icon margin only reaches svg
                    children, and this is an img. */}
                <img
                  src={fileIconUrl(file.path)}
                  alt=""
                  width={16}
                  height={16}
                  className="me-icon size-4 shrink-0"
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
          openingPath={openingPath}
          isDisabled={isBusy}
          onOpen={(project) => void switchProject(project)}
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
    </>
  )
}
