import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { useRouter } from '@tanstack/react-router'

/**
 * The event the backend emits when the open repository changes on disk.
 * Matches `REPO_CHANGED` in `services/watcher.rs`.
 */
const REPO_CHANGED = 'repo:changed'

/**
 * Keeps the diff on screen in step with the diff on disk.
 *
 * The backend has already done the hard filtering — this only decides how
 * often to act on what it sends. Two rules:
 *
 * Coalesce. `getDiff` spawns a `git status` plus one `git diff` per changed
 * file, so overlapping refreshes are genuinely expensive. A signal arriving
 * mid-refresh sets a flag instead of starting a second one, and the loop runs
 * once more when the first settles. That collapses any number of bursts into
 * at most one queued pass, and the last pass always reads the final state.
 *
 * Refresh on focus too. A watch can be missed — a network mount, a machine
 * resumed from sleep, a watcher that failed to establish — and coming back to
 * the window is exactly when a stale view would be noticed. It costs one diff
 * read per focus.
 */
export function useRepoWatch() {
  const router = useRouter()

  useEffect(() => {
    let disposed = false
    let running = false
    let queued = false

    async function refresh() {
      if (running) {
        queued = true
        return
      }
      running = true
      try {
        do {
          queued = false
          await router.invalidate()
        } while (queued && !disposed)
      } finally {
        running = false
      }
    }

    // Resolves to the unlisten function. Outside a Tauri window (a plain
    // `vite` server) there is no event bridge, and focus alone has to do.
    const subscription = listen(REPO_CHANGED, () => void refresh()).catch(() => null)

    const onFocus = () => void refresh()
    window.addEventListener('focus', onFocus)

    return () => {
      disposed = true
      window.removeEventListener('focus', onFocus)
      // The listener may still be registering when this runs, so unsubscribe
      // on resolution rather than assuming it is ready.
      void subscription.then((unlisten) => unlisten?.())
    }
  }, [router])
}
