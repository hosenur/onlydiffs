import type { CSSProperties } from 'react'
import { createFileRoute, Outlet, redirect } from '@tanstack/react-router'
import { AppCommandMenu } from '@/components/app-command-menu'
import { AppSidebar } from '@/components/app-sidebar'
import { AppSidebarNav } from '@/components/app-sidebar-nav'
import { AppToolbar } from '@/components/app-toolbar'
import { ProjectSidebar } from '@/components/project-sidebar'
import { Sidebar, SidebarInset, SidebarProvider } from '@/components/ui/sidebar'
import { useProjectIcons } from '@/hooks/use-project-icons'
import { DiffLayoutProvider } from '@/lib/diff-layout'
import { LineReferenceProvider } from '@/lib/line-reference'
import { useRepoWatch } from '@/lib/repo-watch'
import { UpdateProvider } from '@/lib/update'
import { currentProject, getDiff, listFiles, listProjects } from '@/lib/ipc'

/**
 * Pathless layout route: the `_` prefix means this contributes no URL segment,
 * it only wraps its children. `/diff` and `/file/*` both render inside it, so
 * the sidebar and its loader data survive navigation between them.
 */
interface SidebarStyle extends CSSProperties {
  '--sidebar-width': string
}

const SIDEBAR_STYLE: SidebarStyle = {
  '--sidebar-width': 'calc(17rem + 3.25rem)',
}

export const Route = createFileRoute('/_app')({
  loader: async () => {
    // Nothing below this layout can render without a repository, so bounce to
    // the picker at `/` instead of letting every loader fail. A restored hash
    // URL is the case that still reaches here after a reload.
    if ((await currentProject()) === null) {
      throw redirect({ to: '/' })
    }
    // These reads hit the same repository but none needs another, so they go
    // out together rather than paying for three bridge round-trips in series.
    const [diff, paths, projects] = await Promise.all([
      getDiff(),
      listFiles(),
      listProjects(),
    ])
    return { diff, paths, projects }
  },
  component: AppLayout,
})

function AppLayout() {
  const { diff, paths, projects } = Route.useLoaderData()
  // Everything below reads this layout's loader data, so refreshing here
  // refreshes the sidebar, the file tree, and the open diff together.
  useRepoWatch()
  useProjectIcons()

  return (
    <DiffLayoutProvider>
      <LineReferenceProvider>
        <UpdateProvider>
          {/* Fixed-height shell so each column scrolls on its own. */}
          <AppCommandMenu
            files={diff.files}
            projects={projects}
            currentProjectPath={diff.repoPath}
          />

          <SidebarProvider className="h-svh" style={SIDEBAR_STYLE}>
            <Sidebar
              closeButton={false}
              collapsible="dock"
              className="overflow-hidden *:data-[sidebar=default]:flex-row"
            >
              <ProjectSidebar projects={projects} currentPath={diff.repoPath} />
              <AppSidebar diff={diff} paths={paths} collapsible="none" className="flex flex-1" />
            </Sidebar>

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
