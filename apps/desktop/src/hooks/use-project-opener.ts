import { useState } from 'react'
import { useRouter } from '@tanstack/react-router'
import { openProject } from '@/lib/ipc'

/** Which project failed to open, so a list can mark the row that failed. */
export interface OpenProjectFailure {
  path: string
  message: string
}

/**
 * Switching the app from one repository to another, in the one place that
 * knows the sequence.
 *
 * `replace: true` drops the outgoing project's file route, so going back does
 * not land on a file belonging to a repository that is no longer open, and the
 * `invalidate()` is what actually rereads the new one: navigating between two
 * children of `_app` leaves that layout match in place, and a match that stays
 * does not re-run its loader.
 *
 * Neither applies on the landing page, which sits outside `_app` — entering
 * the layout runs its loader anyway, and replacing would take the picker out
 * of the history. `routes/index.tsx` opens a project without this hook for
 * that reason.
 */
export function useProjectOpener() {
  const router = useRouter()
  const [openingPath, setOpeningPath] = useState<string | null>(null)
  const [failure, setFailure] = useState<OpenProjectFailure | null>(null)

  /** Resolves true once the app is showing `path`, false if it never got there. */
  async function open(path: string): Promise<boolean> {
    if (openingPath !== null) return false
    setOpeningPath(path)
    setFailure(null)
    try {
      await openProject(path)
      await router.navigate({ to: '/diff', replace: true })
      await router.invalidate()
      return true
    } catch (cause) {
      setFailure({
        path,
        message: cause instanceof Error ? cause.message : String(cause),
      })
      return false
    } finally {
      setOpeningPath(null)
    }
  }

  return { openingPath, failure, open }
}
