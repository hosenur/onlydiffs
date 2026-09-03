import { useEffect, useMemo, useRef, useState } from 'react'
import { useHotkey } from '@tanstack/react-hotkeys'
import { useRouter } from '@tanstack/react-router'
import { ArrowLeftIcon, FolderIcon, PlusIcon } from '@heroicons/react/16/solid'
import { Loader } from '@onlydiffs/ui/loader'
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
import { addSshHost, openRemoteProject } from '@/lib/ipc'
import { useSsh } from '@/lib/ssh'
import type { Project } from '@shared/contract'

/**
 * Opening a repository on another machine, in one place.
 *
 * Two panes rather than two dialogs: adding a host and opening a project on one
 * are the same errand a minute apart, and making the first a trip to Settings
 * is what turns "connect to the build box" into three screens.
 *
 * The path field is not a browser. Listing directories on a host means a round
 * trip per keystroke and a permission model to explain; the paths already
 * opened are the useful ones, and anything else is one line to type.
 */
export function RemoteProjectsMenu({
  isOpen,
  onOpenChange,
  projects,
}: {
  isOpen: boolean
  onOpenChange: (open: boolean) => void
  projects: Project[]
}) {
  const ssh = useSsh()
  const router = useRouter()
  /** `null` is the list; a string is the host a path is being typed for. */
  const [addingHost, setAddingHost] = useState(false)
  const [pathFor, setPathFor] = useState<string | null>(null)
  const [value, setValue] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const field = useRef<HTMLInputElement>(null)

  // Every entry into a pane starts empty and focused; carrying the last value
  // across would submit a path to a host that never had it.
  useEffect(() => {
    if (!isOpen) {
      setAddingHost(false)
      setPathFor(null)
    }
    setValue('')
    setError(null)
  }, [isOpen, addingHost, pathFor])

  useEffect(() => {
    if (addingHost || pathFor !== null) field.current?.focus()
  }, [addingHost, pathFor])

  /** Remembered projects grouped under the host they are on. */
  const byHost = useMemo(() => {
    const grouped = new Map<string, Project[]>()
    for (const host of ssh.hosts) grouped.set(host.alias, [])
    for (const project of projects) {
      if (project.host === null) continue
      // A project on a host that was since forgotten still belongs somewhere.
      const existing = grouped.get(project.host) ?? []
      existing.push(project)
      grouped.set(project.host, existing)
    }
    return grouped
  }, [ssh.hosts, projects])

  async function add() {
    const command = value.trim()
    if (!command || busy) return
    setBusy(true)
    setError(null)
    try {
      const entry = await addSshHost(command)
      setAddingHost(false)
      // Connecting immediately is the point: adding a host you cannot reach
      // should say so now, not the first time you try to open something.
      if (await ssh.connect(entry.alias)) setPathFor(entry.alias)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setBusy(false)
    }
  }

  async function open(host: string, path: string) {
    if (busy) return
    setBusy(true)
    setError(null)
    try {
      if (!ssh.isConnected(host) && !(await ssh.connect(host))) return
      await openRemoteProject(host, path)
      onOpenChange(false)
      await router.navigate({ to: '/diff', replace: true })
      await router.invalidate()
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setBusy(false)
    }
  }

  if (addingHost || pathFor !== null) {
    const isPath = pathFor !== null
    return (
      <CommandMenu isOpen={isOpen} onOpenChange={onOpenChange}>
        <form
          className="flex flex-col"
          onSubmit={(event) => {
            event.preventDefault()
            if (isPath) void open(pathFor, value.trim())
            else void add()
          }}
        >
          <div className="flex items-center gap-2 border-border border-b px-3 py-2.5">
            <button
              type="button"
              aria-label="Back"
              onClick={() => {
                setAddingHost(false)
                setPathFor(null)
              }}
              className="shrink-0 text-muted-fg hover:text-fg"
            >
              <ArrowLeftIcon className="size-4" />
            </button>
            <input
              ref={field}
              value={value}
              onChange={(event) => {
                setValue(event.target.value)
                setError(null)
              }}
              placeholder={isPath ? '~/src/service' : 'ssh user@example -p 2222'}
              spellCheck={false}
              autoComplete="off"
              aria-label={isPath ? `Path on ${pathFor}` : 'SSH command'}
              className="min-w-0 flex-1 bg-transparent font-mono text-sm outline-hidden placeholder:text-muted-fg"
            />
            {busy && <Loader className="size-4 shrink-0" />}
          </div>
          <p className="px-3 py-2.5 text-muted-fg text-xs">
            {isPath ? (
              <>
                A path on <span className="font-mono">{pathFor}</span>. Resolved there, so{' '}
                <span className="font-mono">~</span> is that machine&apos;s home and any path
                inside a checkout opens its repository root.
              </>
            ) : (
              <>
                The command you use to SSH into this server. Options in it —{' '}
                <span className="font-mono">-p</span>, <span className="font-mono">-i</span>,{' '}
                <span className="font-mono">-J</span> — are kept and used every time.
              </>
            )}
          </p>
          {error && (
            <p role="alert" className="border-border border-t px-3 py-2.5 text-danger-subtle-fg text-xs">
              {error}
            </p>
          )}
        </form>
      </CommandMenu>
    )
  }

  return (
    <CommandMenu isOpen={isOpen} onOpenChange={onOpenChange}>
      <CommandMenuSearch placeholder="Search remote projects…" />
      <CommandMenuList>
        <CommandMenuSection>
          <CommandMenuItem
            textValue="Connect SSH server add host remote"
            onAction={() => setAddingHost(true)}
          >
            <PlusIcon />
            <CommandMenuLabel>Connect SSH server</CommandMenuLabel>
          </CommandMenuItem>
        </CommandMenuSection>

        {[...byHost.entries()].map(([host, hostProjects]) => (
          <CommandMenuSection key={host} label={host}>
            {hostProjects.map((project) => (
              <CommandMenuItem
                key={project.path}
                textValue={`${host} ${project.root} ${project.name}`}
                onAction={() => void open(host, project.root)}
              >
                <FolderIcon />
                <CommandMenuLabel>{project.root}</CommandMenuLabel>
              </CommandMenuItem>
            ))}
            {/* Always offered, because the useful path is usually one you have
                not opened yet — and on a host with none, this is the only row. */}
            <CommandMenuItem
              textValue={`${host} open another path`}
              onAction={() => setPathFor(host)}
            >
              <PlusIcon />
              <CommandMenuLabel>Open another path…</CommandMenuLabel>
              <CommandMenuShortcut>
                {ssh.isConnected(host) ? 'connected' : 'not connected'}
              </CommandMenuShortcut>
            </CommandMenuItem>
          </CommandMenuSection>
        ))}

        {byHost.size === 0 && (
          <CommandMenuSection label="No hosts yet">
            <CommandMenuItem textValue="none" isDisabled onAction={() => {}}>
              <CommandMenuLabel>Nothing added yet</CommandMenuLabel>
              <CommandMenuDescription>
                Connect an SSH server to review a repository on another machine.
              </CommandMenuDescription>
            </CommandMenuItem>
          </CommandMenuSection>
        )}
      </CommandMenuList>

      {(error ?? ssh.error) && (
        <p role="alert" className="border-border border-t px-4 py-2.5 text-danger-subtle-fg text-xs">
          {error ?? ssh.error}
        </p>
      )}
    </CommandMenu>
  )
}

/** `⌃⌘O`, matching the shortcut every other editor uses for this. */
export function useRemoteProjectsHotkey(onOpen: () => void) {
  useHotkey({ key: 'O', mod: true, ctrl: true }, onOpen, {
    enabled: true,
    ignoreInputs: false,
    requireReset: true,
    meta: { name: 'Open remote project', description: 'Open a repository on another machine' },
  })
}
