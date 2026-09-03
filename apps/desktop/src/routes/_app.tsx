import type { CSSProperties } from 'react'
import { createFileRoute, Outlet, redirect, useRouter } from '@tanstack/react-router'
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
import { IpcError, currentProject, getDiff, listFiles, listProjects } from '@/lib/ipc'
import { useSsh } from '@/lib/ssh'
import { Button } from '@onlydiffs/ui/button'

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
  // A repository on another machine can stop answering mid-review — a laptop
  // sleeps, a VPN drops. Without this the loader's failure escapes to the
  // router's default and the window goes blank on what is usually a
  // ten-second problem.
  errorComponent: RepositoryUnreachable,
})

/**
 * What a dropped connection looks like.
 *
 * Deliberately not a toast: the diff on screen belongs to a repository this app
 * can no longer read, and leaving it up would let someone review a file whose
 * contents it cannot check.
 */
function RepositoryUnreachable({ error }: { error: Error }) {
  const router = useRouter()
  const ssh = useSsh()
  const disconnected = error instanceof IpcError && error.tag === 'SshDisconnectedError'
  const host = ssh.hosts.find((entry) => error.message.includes(entry.alias))

  return (
    <div className="grid min-h-svh place-items-center p-8">
      <div className="flex max-w-md flex-col gap-4 text-center">
        <h1 className="font-medium text-lg tracking-tight">
          {disconnected ? 'The connection dropped' : 'This repository could not be read'}
        </h1>
        <p className="text-muted-fg text-sm">{error.message}</p>
        <div className="flex justify-center gap-2">
          {disconnected && host && (
            <Button
              onPress={() => {
                void ssh.connect(host.alias).then((connected) => {
                  if (connected) void router.invalidate()
                })
              }}
            >
              Reconnect to {host.alias}
            </Button>
          )}
          <Button intent="outline" onPress={() => void router.invalidate()}>
            Try again
          </Button>
          <Button intent="plain" onPress={() => void router.navigate({ to: '/' })}>
            Open another project
          </Button>
        </div>
      </div>
    </div>
  )
}

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

              <AppToolbar files={diff.files} />
            </SidebarInset>
          </SidebarProvider>
        </UpdateProvider>
      </LineReferenceProvider>
    </DiffLayoutProvider>
  )
}
