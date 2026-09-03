import { useHotkey } from '@tanstack/react-hotkeys'
import { useNavigate } from '@tanstack/react-router'

/**
 * `⌘,` opens Settings, the way it does in every other Mac app.
 *
 * A hook rather than one registration somewhere central: `/settings` has to be
 * reachable from the landing page too, and that page sits outside the `_app`
 * layout where the rest of the app's shortcuts live. The two callers never
 * render at once, so the binding is only ever registered once.
 */
export function useSettingsHotkey() {
  const navigate = useNavigate()

  useHotkey(
    { key: ',', mod: true },
    () => void navigate({ to: '/settings' }),
    {
      enabled: true,
      // Deliberately not ignored in inputs: the shortcut has to work while
      // focus sits in the repository field or the Claude toolbar.
      ignoreInputs: false,
      requireReset: true,
      meta: { name: 'Settings', description: 'Open settings' },
    }
  )
}
