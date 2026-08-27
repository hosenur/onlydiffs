import { useState } from 'react'
import { useHotkey } from '@tanstack/react-hotkeys'
import { useNavigate, useParams, useRouter } from '@tanstack/react-router'
import {
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
import { fileIconUrl } from '@/lib/file-icon'
import {
  commitAll,
  generateCommitMessage,
  runIpc,
  stageFile,
  writeClipboardText,
} from '@/lib/ipc'
import { fileHref } from '@/lib/status'
import type { FileChange } from '@/types'

interface AppCommandMenuProps {
  files: FileChange[]
}

/** Which long-running action is in flight, so the palette can say so. */
type Busy = 'generate' | 'commit' | 'stage' | null

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

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

export function AppCommandMenu({ files }: AppCommandMenuProps) {
  const [isOpen, setIsOpen] = useState(false)
  const [busy, setBusy] = useState<Busy>(null)
  const [note, setNote] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const navigate = useNavigate()
  const router = useRouter()
  const { theme, setTheme } = useTheme()
  // `strict: false` — the palette also renders on `/`, which has no splat.
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
      const generated = await runIpc(generateCommitMessage)
      await runIpc(writeClipboardText(generated))
      setNote(`Copied — ${generated.split('\n')[0]}`)
    } catch (cause) {
      setError(errorMessage(cause))
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
      await runIpc(
        stageFile({ path: stageable.path, oldPath: stageable.oldPath })
      )
      setNote(`Staged ${stageable.path}`)
      await router.invalidate()
    } catch (cause) {
      setError(errorMessage(cause))
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
      const subject = await runIpc(generateCommitMessage)
      const head = await runIpc(commitAll(subject))
      // Left open on purpose: this note is the only confirmation there is,
      // and closing straight away takes the commit hash with it.
      setNote(`Committed ${head}`)
      await router.invalidate()
    } catch (cause) {
      setError(errorMessage(cause))
    } finally {
      setBusy(null)
    }
  }

  return (
    <CommandMenu isOpen={isOpen} onOpenChange={setIsOpen}>
      <CommandMenuSearch placeholder="Search files, or run a command…" />
      <CommandMenuList>
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
            <CommandMenuDescription>
              Reads the whole diff, copies it
            </CommandMenuDescription>
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
      </CommandMenuList>

      {(note || error) && (
        <p
          role={error ? 'alert' : 'status'}
          className={`border-border border-t px-4 py-2.5 text-xs ${
            error ? 'text-danger-subtle-fg' : 'text-muted-fg'
          }`}
        >
          {error ?? note}
        </p>
      )}
    </CommandMenu>
  )
}
