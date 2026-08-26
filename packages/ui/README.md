# @onlydiffs/ui

Intent UI components shared by `apps/desktop` and `apps/web`.

Source-only: both apps bundle TypeScript directly, so there is no build step.
Next needs `transpilePackages: ["@onlydiffs/ui"]`; Vite handles it natively.

## What is not here

`link` and `menu` stay in each app. Both need the app's router — `next/link`
in web, TanStack Router in desktop — and a component that reaches for a router
is not shareable without inverting that dependency. `copy-button` and `snippet`
likewise depend on web's own icons and hooks.

Components came from the newer copies the web template shipped; the desktop app
was on an older Intent release.
