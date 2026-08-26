import { createFileRoute, Outlet, redirect } from '@tanstack/react-router'
import { Effect } from 'effect'
import { AppHistorySidebar } from '@/components/app-history-sidebar'
import { AppSidebar } from '@/components/app-sidebar'
import { AppSidebarNav } from '@/components/app-sidebar-nav'
import { SidebarInset, SidebarProvider } from '@/components/ui/sidebar'
import { ClaudeMessageDraftProvider } from '@/lib/claude-message-draft'
import { DiffLayoutProvider } from '@/lib/diff-layout'
import { currentProject, getDiff, getHistory, listFiles, runIpc } from '@/lib/ipc'

/**
 * Pathless layout route: the `_` prefix means this contributes no URL segment,
 * it only wraps its children. `/` and `/file/*` both render inside it, so the
 * sidebars and their loader data survive navigation between them.
 */
export const Route = createFileRoute('/_app')({
  loader: async () => {
    // Nothing below this layout can render without a repository, so bounce to
    // the landing page instead of letting every loader fail.
    if ((await runIpc(currentProject)) === null) {
      throw redirect({ to: '/welcome' })
    }
    return runIpc(
      Effect.all(
        { diff: getDiff, history: getHistory(100), paths: listFiles },
        { concurrency: 3 }
      )
    )
  },
  component: AppLayout,
})

function AppLayout() {
  const { diff, history, paths } = Route.useLoaderData()

  return (
    <DiffLayoutProvider>
      <ClaudeMessageDraftProvider>
        {/* Fixed-height shell so each column scrolls on its own. */}
        <SidebarProvider className="h-svh">
          <AppSidebar diff={diff} paths={paths} collapsible="dock" />

          <SidebarInset className="min-w-0 overflow-hidden">
            <AppSidebarNav files={diff.files} />
            <div className="min-h-0 flex-1 overflow-y-auto">
              <Outlet />
            </div>
          </SidebarInset>

          <AppHistorySidebar branch={diff.branch} files={diff.files} history={history} />
        </SidebarProvider>
      </ClaudeMessageDraftProvider>
    </DiffLayoutProvider>
  )
}
