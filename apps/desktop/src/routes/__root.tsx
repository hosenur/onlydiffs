import { createRootRoute, Outlet } from '@tanstack/react-router'
import { SshDialogs } from '@/components/ssh-dialogs'
import { SshProvider } from '@/lib/ssh'

/**
 * The root route, which exists to hold the one provider every route needs.
 *
 * ssh can ask for a passphrase while any route is on screen — including
 * `/settings`, which is where a host is added, and `/`, which is where one is
 * opened — so the prompt lives above all of them. It is here rather than in
 * `main.tsx` because it uses the router: `RouterProvider` renders the matched
 * route rather than children, so this is the outermost place that is still
 * inside the router.
 */
export const Route = createRootRoute({
  component: () => (
    <SshProvider>
      <Outlet />
      <SshDialogs />
    </SshProvider>
  ),
})
