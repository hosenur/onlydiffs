import { createFileRoute, Outlet, redirect } from '@tanstack/react-router'
import { AppCommandMenu } from '@/components/app-command-menu'
import { AppSidebar } from '@/components/app-sidebar'
import { AppSidebarNav } from '@/components/app-sidebar-nav'
import { AppToolbar } from '@/components/app-toolbar'
import { SidebarInset, SidebarProvider } from '@/components/ui/sidebar'
import { DiffLayoutProvider } from '@/lib/diff-layout'
import { LineReferenceProvider } from '@/lib/line-reference'
import { useRepoWatch } from '@/lib/repo-watch'
import { UpdateProvider } from '@/lib/update'
import { currentProject, getDiff, listFiles } from '@/lib/ipc'

/**
 * Pathless layout route: the `_` prefix means this contributes no URL segment,
 * it only wraps its children. `/diff` and `/file/*` both render inside it, so
 * the sidebar and its loader data survive navigation between them.
 */
export const Route = createFileRoute('/_app')({
  loader: async () => {
    // Nothing below this layout can render without a repository, so bounce to
    // the picker at `/` instead of letting every loader fail. A restored hash
    // URL is the case that still reaches here after a reload.
    if ((await currentProject()) === null) {
      throw redirect({ to: '/' })
    }
    // Both reads hit the same repository and neither needs the other, so they
    // go out together rather than one after the round-trip.
    const [diff, paths] = await Promise.all([getDiff(), listFiles()])
    return { diff, paths }
  },
  component: AppLayout,
})

function AppLayout() {
  const { diff, paths } = Route.useLoaderData()
  // Everything below reads this layout's loader data, so refreshing here
  // refreshes the sidebar, the file tree, and the open diff together.
  useRepoWatch()

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
