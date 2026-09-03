import { createContext, use, useCallback, useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { useRouter } from '@tanstack/react-router'
import {
  answerSshPrompt,
  connectHost,
  disconnectHost,
  inspectHostKey,
  listHosts,
  trustHostKey,
  IpcError,
} from '@/lib/ipc'
import type { ConnectedHost, SshPromptRequest, UnknownHostKeyPrompt } from '@shared/contract'

const SSH_PROMPT = 'ssh:prompt'
const HOSTS_CHANGED = 'ssh:hosts-changed'

/**
 * The two questions connecting can ask, and the answer to whichever is open.
 *
 * They are kept apart rather than folded into one "prompt" because they are
 * genuinely different: one is a secret the user types, the other is a
 * fingerprint they either recognise or do not. Rendering them the same way
 * would invite answering the second the way you answer the first.
 */
export interface SshValue {
  /** Every host the user has added, connected or not. */
  hosts: ConnectedHost[]
  /** The host currently being connected, if any. */
  connecting: string | null
  /** What ssh is blocked on, if anything. */
  prompt: SshPromptRequest | null
  /** A host key nobody has approved yet. */
  unknownHost: UnknownHostKeyPrompt | null
  error: string | null
  connect: (alias: string) => Promise<boolean>
  disconnect: (alias: string) => Promise<void>
  answer: (value: string) => void
  cancel: () => void
  /** Records the fingerprint on screen, then connects. */
  trustAndConnect: () => Promise<boolean>
  dismissUnknownHost: () => void
  isConnected: (alias: string) => boolean
}

const SshContext = createContext<SshValue | null>(null)

export function SshProvider({ children }: { children: React.ReactNode }) {
  const [hosts, setHosts] = useState<ConnectedHost[]>([])
  const [connecting, setConnecting] = useState<string | null>(null)
  const [prompt, setPrompt] = useState<SshPromptRequest | null>(null)
  const [unknownHost, setUnknownHost] = useState<UnknownHostKeyPrompt | null>(null)
  const [error, setError] = useState<string | null>(null)
  const router = useRouter()

  const refresh = useCallback(async () => {
    try {
      setHosts(await listHosts())
    } catch {
      // The list is a display; failing to read it is not worth an alert.
    }
  }, [])

  useEffect(() => {
    void refresh()
    // ssh can prompt at any moment — a key removed from the agent mid-session
    // asks again — so this listens for the whole life of the window rather
    // than only while a connect is in flight.
    const prompts = listen<SshPromptRequest>(SSH_PROMPT, (event) => {
      setPrompt(event.payload)
    }).catch(() => null)
    const changed = listen(HOSTS_CHANGED, () => void refresh()).catch(() => null)

    return () => {
      void prompts.then((unlisten) => unlisten?.())
      void changed.then((unlisten) => unlisten?.())
    }
  }, [refresh])

  async function attempt(alias: string): Promise<boolean> {
    setConnecting(alias)
    setError(null)
    try {
      await connectHost(alias)
      await refresh()
      return true
    } catch (cause) {
      // An unknown host is the one failure that is really a question. Turning
      // it into a fingerprint prompt is what keeps `StrictHostKeyChecking` on.
      if (cause instanceof IpcError && cause.tag === 'SshUnknownHostError') {
        try {
          setUnknownHost(await inspectHostKey(alias))
        } catch (inspected) {
          setError(inspected instanceof Error ? inspected.message : String(inspected))
        }
        return false
      }
      setError(cause instanceof Error ? cause.message : String(cause))
      return false
    } finally {
      setConnecting(null)
      // A prompt only outlives the attempt if the attempt failed while it was
      // open, and leaving it on screen would ask for a password nothing wants.
      setPrompt(null)
    }
  }

  const value: SshValue = {
    hosts,
    connecting,
    prompt,
    unknownHost,
    error,
    connect: attempt,
    disconnect: async (alias) => {
      await disconnectHost(alias).catch(() => {})
      await refresh()
      // Projects on that host are still listed, just unreachable.
      await router.invalidate()
    },
    answer: (answered) => {
      if (!prompt) return
      void answerSshPrompt(prompt.id, answered).catch(() => {})
      setPrompt(null)
    },
    cancel: () => {
      if (!prompt) return
      void answerSshPrompt(prompt.id, null).catch(() => {})
      setPrompt(null)
    },
    trustAndConnect: async () => {
      if (!unknownHost) return false
      const alias = unknownHost.alias
      setUnknownHost(null)
      try {
        await trustHostKey(alias)
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause))
        return false
      }
      return attempt(alias)
    },
    dismissUnknownHost: () => setUnknownHost(null),
    isConnected: (alias) =>
      hosts.some((host) => host.alias === alias && host.state === 'connected'),
  }

  return <SshContext value={value}>{children}</SshContext>
}

export function useSsh(): SshValue {
  const context = use(SshContext)
  if (context === null) throw new Error('useSsh must be used within an SshProvider')
  return context
}
