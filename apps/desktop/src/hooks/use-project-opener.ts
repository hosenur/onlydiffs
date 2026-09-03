import { useState } from 'react'
import { useRouter } from '@tanstack/react-router'
import { openProject, openRemoteProject } from '@/lib/ipc'
import { useSsh } from '@/lib/ssh'
import type { Project } from '@shared/contract'

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
  const ssh = useSsh()
  const [openingPath, setOpeningPath] = useState<string | null>(null)
  const [failure, setFailure] = useState<OpenProjectFailure | null>(null)

  /**
   * Resolves true once the app is showing `project`, false if it never got
   * there.
   *
   * Takes the project rather than its path: a remote project's `path` is
   * `host:/root`, which is an identity rather than something either open
   * command can be handed. Passing it to the local one — which is what this
   * did until a remote project first failed to open from the rail — reads it
   * as a folder on this machine and reports that no such folder exists.
   */
  async function open(project: Project): Promise<boolean> {
    if (openingPath !== null) return false
    setOpeningPath(project.path)
    setFailure(null)
    try {
      if (project.host === null) {
        await openProject(project.root)
      } else {
        // A host that is asleep is the ordinary reason this fails, and
        // connecting may put a passphrase dialog on screen first.
        if (!ssh.isConnected(project.host) && !(await ssh.connect(project.host))) {
          return false
        }
        await openRemoteProject(project.host, project.root)
      }
      await router.navigate({ to: '/diff', replace: true })
      await router.invalidate()
      return true
    } catch (cause) {
      setFailure({
        path: project.path,
        message: cause instanceof Error ? cause.message : String(cause),
      })
      return false
    } finally {
      setOpeningPath(null)
    }
  }

  return { openingPath, failure, open }
}
