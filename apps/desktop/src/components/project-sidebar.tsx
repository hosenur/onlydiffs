import { useState } from 'react'
import { Link, useRouter } from '@tanstack/react-router'
import { Button } from '@onlydiffs/ui/button'
import { Loader } from '@onlydiffs/ui/loader'
import platypus from '@/assets/platypus.png'
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
} from '@/components/ui/sidebar'
import { TileImage, TileInitials } from '@/components/project-tile'
import { Tooltip, TooltipContent } from '@/components/ui/tooltip'
import { IsometricCubeIcon } from '@/icons'
import { openProject } from '@/lib/ipc'
import type { Project } from '@shared/contract'

interface ProjectSidebarProps {
  projects: Project[]
  currentPath: string
}

export function ProjectSidebar({ projects, currentPath }: ProjectSidebarProps) {
  const router = useRouter()
  const [busy, setBusy] = useState<string | null>(null)
  const [failure, setFailure] = useState<{ path: string; message: string } | null>(null)

  async function open(project: Project) {
    if (project.path === currentPath || busy !== null) return

    setBusy(project.path)
    setFailure(null)
    try {
      await openProject(project.path)
      // Replace the old project's file route, then refresh every loader against
      // the repository the backend has just made current.
      await router.navigate({ to: '/diff', replace: true })
      await router.invalidate()
    } catch (error) {
      setFailure({
        path: project.path,
        message: error instanceof Error ? error.message : 'Could not open this project.',
      })
    } finally {
      setBusy(null)
    }
  }

  return (
    <Sidebar
      collapsible="none"
      aria-label="Projects"
      className="hidden w-[calc(var(--sidebar-width-dock)+1px)] shrink-0 border-r md:flex"
    >
      <SidebarHeader className="h-16 items-center border-b px-2.5 py-4">
        <Tooltip delay={0}>
          <Link
            to="/"
            aria-label="Open another project"
            className="rounded-lg outline-hidden focus-visible:ring-2 focus-visible:ring-sidebar-ring"
          >
            <TileImage
              src={platypus}
              alt=""
              width={32}
              height={32}
              className="size-8"
            />
          </Link>
          <TooltipContent inverse placement="right">
            Open another project
          </TooltipContent>
        </Tooltip>
      </SidebarHeader>

      <SidebarContent className="mask-none items-center gap-2 px-2 py-2">
        {projects.map((project) => {
          const isCurrent = project.path === currentPath
          const projectFailure = failure?.path === project.path ? failure.message : null

          return (
            <Tooltip key={project.path} delay={0}>
              <Button
                intent="plain"
                size="sq-md"
                aria-label={`Open ${project.name}`}
                aria-current={isCurrent ? 'page' : undefined}
                isDisabled={busy !== null}
                onPress={() => void open(project)}
                className={`rounded-lg p-0 ${
                  isCurrent
                    ? 'bg-primary-subtle inset-ring inset-ring-primary/70'
                    : projectFailure
                      ? 'inset-ring inset-ring-danger'
                      : ''
                }`}
              >
                {busy === project.path ? (
                  <Loader />
                ) : project.icon ? (
                  <TileImage
                    src={project.icon.dataUrl}
                    alt=""
                    draggable={false}
                    className="size-8 bg-white object-contain"
                  />
                ) : (
                  <TileInitials project={project} className="size-8 text-[11px]" />
                )}
              </Button>
              <TooltipContent inverse placement="right" className="max-w-80">
                <span className="block font-medium">{project.name}</span>
                <span className={projectFailure ? 'block text-danger-subtle-fg' : 'block text-bg/60'}>
                  {projectFailure ?? project.path}
                </span>
              </TooltipContent>
            </Tooltip>
          )
        })}
      </SidebarContent>

      <SidebarFooter className="p-2.5">
        <Tooltip delay={0}>
          <Link
            to="/"
            aria-label="Open another project"
            className="grid size-8 place-items-center rounded-lg text-muted-fg outline-hidden hover:bg-sidebar-accent hover:text-sidebar-accent-fg focus-visible:ring-2 focus-visible:ring-sidebar-ring"
          >
            <IsometricCubeIcon aria-hidden className="size-5" />
          </Link>
          <TooltipContent inverse placement="right">
            Open another project
          </TooltipContent>
        </Tooltip>
      </SidebarFooter>
    </Sidebar>
  )
}
