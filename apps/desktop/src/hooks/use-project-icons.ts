import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { useRouter } from '@tanstack/react-router'

const PROJECT_ICON_CHANGED = 'project:icon-changed'

/** Reloads project metadata when the background resolver finds new artwork. */
export function useProjectIcons() {
  const router = useRouter()

  useEffect(() => {
    const subscription = listen(PROJECT_ICON_CHANGED, () => {
      void router.invalidate()
    }).catch(() => null)

    return () => {
      void subscription.then((unlisten) => unlisten?.())
    }
  }, [router])
}
