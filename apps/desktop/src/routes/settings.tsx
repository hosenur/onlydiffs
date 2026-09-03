import { useState } from 'react'
import { createFileRoute, useCanGoBack, useNavigate, useRouter } from '@tanstack/react-router'
import { useHotkey } from '@tanstack/react-hotkeys'
import {
  ArrowLeftIcon,
  CheckIcon,
  ComputerDesktopIcon,
  MoonIcon,
  SunIcon,
  TrashIcon,
} from '@heroicons/react/16/solid'
import { Button } from '@onlydiffs/ui/button'
import { Loader } from '@onlydiffs/ui/loader'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectLabel,
  SelectTrigger,
} from '@onlydiffs/ui/select'
import { type Theme, useTheme } from '@/components/theme-provider'
import { useProjectIcons } from '@/hooks/use-project-icons'
import { addSshHost, forgetSshHost, getSettings, setGroqApiKey } from '@/lib/ipc'
import { useSsh } from '@/lib/ssh'
import type { AppSettings, GroqKeySource } from '@shared/contract'

/**
 * Settings, and the app's second page outside the `_app` layout.
 *
 * Outside on purpose: the one setting that matters most is the Groq key, and
 * the moment someone most needs it is a fresh install with no repository open,
 * where `_app`'s loader would have bounced them to the picker.
 */
export const Route = createFileRoute('/settings')({
  // Resolving the key can reach for the login shell and the answer changes
  // whenever it is edited here, so this is never served from cache.
  shouldReload: true,
  staleTime: 0,
  loader: () => getSettings(),
  component: Settings,
})

/** What the app does with a key, said in the place where one is entered. */
const KEY_USES = 'Generating commit messages, and picking each project’s icon.'

const SOURCE_NOTE = {
  config: 'Saved in this app.',
  environment:
    'Read from GROQ_API_KEY in your environment. Saving a key here replaces it.',
  none: 'Without one, commit-message generation and project icons stay off.',
} satisfies Record<GroqKeySource, string>

const THEMES: { value: Theme; label: string; Icon: typeof SunIcon }[] = [
  { value: 'light', label: 'Light', Icon: SunIcon },
  { value: 'dark', label: 'Dark', Icon: MoonIcon },
  { value: 'system', label: 'System', Icon: ComputerDesktopIcon },
]

function SectionHeading({ children }: { children: React.ReactNode }) {
  return (
    <h2 className="font-medium text-muted-fg text-xs uppercase tracking-wide">{children}</h2>
  )
}

/**
 * The hosts this app can open a repository on.
 *
 * Only the alias is stored. Everything else about a host — its real name, its
 * port, its user, its keys, its jump host — is already in `~/.ssh/config`, and
 * a second copy here would be one that can disagree with the one ssh uses.
 */
function SshHosts({
  settings,
  onSettings,
}: {
  settings: AppSettings
  onSettings: (next: AppSettings) => void
}) {
  const ssh = useSsh()
  const [alias, setAlias] = useState('')
  const [busy, setBusy] = useState<string | null>(null)
  const [failure, setFailure] = useState<string | null>(null)

  async function add() {
    const command = alias.trim()
    if (!command || busy) return
    setBusy(command)
    setFailure(null)
    try {
      const entry = await addSshHost(command)
      setAlias('')
      onSettings(await getSettings())
      // Adding is not connecting, but the first thing anyone wants after
      // adding a host is to know whether it works.
      await ssh.connect(entry.alias)
    } catch (cause) {
      setFailure(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setBusy(null)
    }
  }

  async function forget(target: string) {
    setFailure(null)
    try {
      onSettings(await forgetSshHost(target))
    } catch (cause) {
      setFailure(cause instanceof Error ? cause.message : String(cause))
    }
  }

  return (
    <section className="flex flex-col gap-2">
      <SectionHeading>SSH hosts</SectionHeading>
      <p className="text-muted-fg text-sm">
        Review a repository on another machine. Paste the command you already use to reach
        it — the options in it are kept and replayed every time. Anything your{' '}
        <span className="font-mono">ssh</span> can reach works here, because this runs your
        ssh rather than reimplementing it.
      </p>

      {settings.sshHosts.length > 0 && (
        <ul className="flex flex-col divide-y divide-border rounded-lg border border-border">
          {settings.sshHosts.map((entry) => {
            const host = entry.alias
            const connected = ssh.hosts.find(
              (live) => live.alias === host && live.state === 'connected'
            )
            return (
              <li key={host} className="group/host flex items-center gap-2 px-3 py-2">
                <span
                  aria-hidden
                  className={`size-1.5 shrink-0 rounded-full ${
                    connected ? 'bg-success' : 'bg-muted-fg/40'
                  }`}
                />
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate font-mono text-sm">{host}</span>
                  <span className="truncate text-muted-fg text-xs">
                    {connected
                      ? `${connected.platform ?? 'connected'} · git ${connected.gitVersion ?? '?'} · agent ${connected.agentVersion ?? '?'}`
                      : entry.args.length > 0
                        ? `Not connected · ${entry.args.join(' ')}`
                        : 'Not connected'}
                  </span>
                </span>
                {connected ? (
                  <Button intent="plain" size="xs" onPress={() => void ssh.disconnect(host)}>
                    Disconnect
                  </Button>
                ) : (
                  <Button
                    intent="plain"
                    size="xs"
                    isDisabled={ssh.connecting !== null}
                    onPress={() => void ssh.connect(host)}
                  >
                    {ssh.connecting === host ? <Loader /> : 'Connect'}
                  </Button>
                )}
                <Button
                  intent="plain"
                  size="sq-sm"
                  aria-label={`Forget ${host}`}
                  onPress={() => void forget(host)}
                  className="opacity-0 group-hover/host:opacity-100"
                >
                  <TrashIcon />
                </Button>
              </li>
            )
          })}
        </ul>
      )}

      <form
        className="flex gap-2"
        onSubmit={(event) => {
          event.preventDefault()
          void add()
        }}
      >
        <input
          value={alias}
          onChange={(event) => {
            setAlias(event.target.value)
            setFailure(null)
          }}
          placeholder="ssh user@example -p 2222"
          spellCheck={false}
          autoComplete="off"
          aria-label="SSH command"
          className="min-w-0 flex-1 rounded-lg border border-border bg-bg px-3 py-2 font-mono text-sm outline-hidden placeholder:text-muted-fg focus:border-primary"
        />
        <Button type="submit" isDisabled={alias.trim().length === 0 || busy !== null}>
          {busy !== null ? <Loader /> : <CheckIcon />}
          Add
        </Button>
      </form>

      {(failure ?? ssh.error) && (
        <p role="alert" className="text-danger-subtle-fg text-sm">
          {failure ?? ssh.error}
        </p>
      )}
    </section>
  )
}

function Settings() {
  const loaded = Route.useLoaderData()
  const router = useRouter()
  const navigate = useNavigate()
  const canGoBack = useCanGoBack()
  const { theme, setTheme } = useTheme()
  // Saving a key sends the resolver back over every project still on a
  // fallback. Listening from here is what makes those icons already be there
  // when the rail comes back into view.
  useProjectIcons()
  // Mirrored rather than read straight from the loader, unlike the picker:
  // saving answers with the settings as they now stand, so re-reading them
  // would be a round-trip for something already in hand.
  const [settings, setSettings] = useState<AppSettings>(loaded)
  const [draft, setDraft] = useState('')
  const [busy, setBusy] = useState<'save' | 'remove' | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [note, setNote] = useState<string | null>(null)

  function leave() {
    // The picker is the one page every launch can get back to; `canGoBack` is
    // false when settings was the first thing opened this session.
    if (canGoBack) router.history.back()
    else void navigate({ to: '/' })
  }

  useHotkey('Escape', leave, {
    enabled: true,
    // The key field is a text input, and Escape should still leave from there.
    ignoreInputs: false,
    requireReset: true,
    meta: { name: 'Close settings' },
  })

  async function write(key: string | null, action: 'save' | 'remove') {
    if (busy !== null) return
    setBusy(action)
    setError(null)
    setNote(null)
    try {
      const saved = await setGroqApiKey(key)
      setSettings(saved)
      setDraft('')
      setNote(key === null ? 'Key removed.' : 'Key saved.')
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setBusy(null)
    }
  }

  const trimmed = draft.trim()
  const isStored = settings.groqKeySource === 'config'

  return (
    <div className="flex min-h-svh flex-col items-center p-8">
      <div className="flex w-full max-w-xl flex-col gap-8">
        <header className="flex items-center gap-3">
          <Button intent="plain" size="sq-sm" aria-label="Back" onPress={leave}>
            <ArrowLeftIcon />
          </Button>
          <h1 className="font-medium text-lg tracking-tight">Settings</h1>
        </header>

        <section className="flex flex-col gap-2">
          <SectionHeading>Groq API key</SectionHeading>
          <p className="text-muted-fg text-sm">{KEY_USES}</p>

          {settings.groqApiKeyHint && (
            <div className="flex items-center gap-2 rounded-lg border border-border px-3 py-2">
              <CheckIcon className="size-4 shrink-0 text-success" />
              <span className="min-w-0 flex-1 truncate font-mono text-sm">
                {settings.groqApiKeyHint}
              </span>
              {isStored && (
                <Button
                  intent="plain"
                  size="sq-sm"
                  aria-label="Remove the saved key"
                  isDisabled={busy !== null}
                  onPress={() => void write(null, 'remove')}
                >
                  {busy === 'remove' ? <Loader /> : <TrashIcon />}
                </Button>
              )}
            </div>
          )}

          <form
            className="flex gap-2"
            onSubmit={(event) => {
              event.preventDefault()
              if (trimmed.length > 0) void write(trimmed, 'save')
            }}
          >
            <input
              id="groq-api-key"
              type="password"
              value={draft}
              onChange={(event) => {
                setDraft(event.target.value)
                setError(null)
                setNote(null)
              }}
              placeholder={settings.groqApiKeyHint ? 'Replace the key…' : 'gsk_…'}
              spellCheck={false}
              autoComplete="off"
              aria-label="Groq API key"
              className="min-w-0 flex-1 rounded-lg border border-border bg-bg px-3 py-2 font-mono text-sm outline-hidden placeholder:text-muted-fg focus:border-primary"
            />
            <Button type="submit" isDisabled={trimmed.length === 0 || busy !== null}>
              {busy === 'save' ? <Loader /> : <CheckIcon />}
              Save
            </Button>
          </form>

          <p className="text-muted-fg text-xs">
            {SOURCE_NOTE[settings.groqKeySource]}{' '}
            {/* A plain link: the window hands any external URL to the real
                browser rather than navigating away from the bundle. */}
            <a
              href="https://console.groq.com/keys"
              className="underline underline-offset-2 hover:text-fg"
            >
              Get a key
            </a>
            .
          </p>
        </section>

        <SshHosts settings={settings} onSettings={setSettings} />

        <section className="flex flex-col gap-2">
          <SectionHeading>Appearance</SectionHeading>
          <p className="text-muted-fg text-sm">
            Applies to the window frame as well as the page.
          </p>
          <Select
            aria-label="Theme"
            selectedKey={theme}
            // Resolved back through the list rather than read off the
            // selection: what comes out is one of the three themes by
            // construction, so nothing downstream has to be told it is.
            onSelectionChange={(key) => {
              const selected = THEMES.find(({ value }) => value === key)
              if (selected) setTheme(selected.value)
            }}
            className="max-w-56"
          >
            <SelectTrigger />
            <SelectContent>
              {THEMES.map(({ value, label, Icon }) => (
                <SelectItem key={value} id={value} textValue={label}>
                  <Icon />
                  <SelectLabel>{label}</SelectLabel>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </section>

        {(error ?? note) && (
          <p
            role={error ? 'alert' : 'status'}
            className={`text-sm ${error ? 'text-danger-subtle-fg' : 'text-muted-fg'}`}
          >
            {error ?? note}
          </p>
        )}

        <p className="text-muted-fg text-xs">
          Saved settings live in <span className="font-mono">{settings.configPath}</span>. The
          theme is the window’s own and stays in the app, so it is applied before the first
          frame rather than after a read from disk.
        </p>
      </div>
    </div>
  )
}
