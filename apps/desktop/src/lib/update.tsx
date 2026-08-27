import { createContext, use, useCallback, useEffect, useMemo, useState } from 'react'
import { checkForUpdate, installUpdate, runIpc } from '@/lib/ipc'

/**
 * The release waiting to be installed, shared between the footer that mentions
 * one and the palette that installs it.
 *
 * A context for the same reason as `line-reference`: the two ends sit far apart
 * in the tree. One check per launch and no polling — nobody leaves a diff
 * viewer open for days, and a second check would find the same answer. The
 * backend reports nothing available in a dev build, so this does not have to
 * know which build it is running inside.
 */

/** Late enough to stay out of the way of the first paint and its loaders. */
const CHECK_DELAY_MS = 5000

interface Offer {
  /** The version on offer, e.g. `0.1.2`. */
  version: string | null
  /** Release notes, when the manifest carried any. */
  notes: string | null
}

export interface UpdateValue {
  /** The release on offer, or `null` when there is nothing to install. */
  offer: Offer | null
  isInstalling: boolean
  /**
   * Set only by a failed install. A failed *check* stays quiet: the user did
   * not ask, and there is nothing they could do about it.
   */
  error: string | null
  install: () => Promise<void>
}

const UpdateContext = createContext<UpdateValue | null>(null)

export function UpdateProvider({ children }: { children: React.ReactNode }) {
  const [offer, setOffer] = useState<Offer | null>(null)
  const [isInstalling, setIsInstalling] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let active = true

    const timer = setTimeout(() => {
      void (async () => {
        try {
          const status = await runIpc(checkForUpdate)
          if (active && status.available) {
            setOffer({ version: status.version, notes: status.notes })
          }
        } catch {
          // Being unable to reach the release feed is not news — offline is the
          // normal state of a laptop, and staying quiet is the whole point of
          // checking in the background.
        }
      })()
    }, CHECK_DELAY_MS)

    return () => {
      active = false
      clearTimeout(timer)
    }
  }, [])

  const install = useCallback(async () => {
    setIsInstalling(true)
    setError(null)
    try {
      await runIpc(installUpdate)
      // Not reached: a successful install relaunches the app from the new
      // bundle. Anything past the await is a failure that resolved oddly.
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setIsInstalling(false)
    }
  }, [])

  const value = useMemo<UpdateValue>(
    () => ({ offer, isInstalling, error, install }),
    [offer, isInstalling, error, install]
  )

  return <UpdateContext value={value}>{children}</UpdateContext>
}

export function useUpdate(): UpdateValue {
  const context = use(UpdateContext)
  if (context === null) {
    throw new Error('useUpdate must be used inside an UpdateProvider')
  }
  return context
}
