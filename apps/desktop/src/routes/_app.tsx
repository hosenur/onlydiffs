import { createFileRoute, Outlet, redirect } from '@tanstack/react-router'
import { Effect } from 'effect'
import { AppCommandMenu } from '@/components/app-command-menu'
import { AppSidebar } from '@/components/app-sidebar'
import { AppSidebarNav } from '@/components/app-sidebar-nav'
import { AppToolbar } from '@/components/app-toolbar'
import { SidebarInset, SidebarProvider } from '@/components/ui/sidebar'
import { DiffLayoutProvider } from '@/lib/diff-layout'
import { LineReferenceProvider } from '@/lib/line-reference'
import { UpdateProvider } from '@/lib/update'
import { currentProject, getDiff, listFiles, runIpc } from '@/lib/ipc'

/**
 * Pathless layout route: the `_` prefix means this contributes no URL segment,
 * it only wraps its children. `/` and `/file/*` both render inside it, so the
 * sidebar and its loader data survive navigation between them.
 */
export const Route = createFileRoute('/_app')({
  loader: async () => {
    // Nothing below this layout can render without a repository, so bounce to
    // the landing page instead of letting every loader fail.
    if ((await runIpc(currentProject)) === null) {
      throw redirect({ to: '/welcome' })
    }
    return runIpc(
      Effect.all({ diff: getDiff, paths: listFiles }, { concurrency: 2 })
    )
  },
  component: AppLayout,
})

function AppLayout() {
  const { diff, paths } = Route.useLoaderData()

  return (
    <DiffLayoutProvider>
      <LineReferenceProvider>
      <UpdateProvider>
      {/* Fixed-height shell so each column scrolls on its own. */}
      <AppCommandMenu files={diff.files} />

      <SidebarProvider className="h-svh">
        <AppSidebar diff={diff} paths={paths} collapsible="hidden" />

        <SidebarInset className="min-w-0 overflow-hidden">
          <AppSidebarNav files={diff.files} />
          <div className="min-h-0 flex-1 overflow-y-auto">
            <Outlet />
          </div>

          <AppToolbar />
        </SidebarInset>
        </SidebarProvider>
      </UpdateProvider>
      </LineReferenceProvider>
    </DiffLayoutProvider>
  )
}
