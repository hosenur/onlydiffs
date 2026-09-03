import { Link } from '@tanstack/react-router'
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
import { useProjectOpener } from '@/hooks/use-project-opener'
import { GearOutline18 } from '@/icons'
import type { Project } from '@shared/contract'

interface ProjectSidebarProps {
  projects: Project[]
  currentPath: string
}

export function ProjectSidebar({ projects, currentPath }: ProjectSidebarProps) {
  const { openingPath, failure, open } = useProjectOpener()

  function openUnlessCurrent(project: Project) {
    // Reopening the project already on screen would blank it and reload it for
    // no change; the rail is the one place a current row can still be pressed.
    if (project.path === currentPath) return
    void open(project)
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
                isDisabled={openingPath !== null}
                onPress={() => openUnlessCurrent(project)}
                className={`rounded-lg p-0 ${
                  isCurrent
                    ? 'bg-primary-subtle inset-ring inset-ring-primary/70'
                    : projectFailure
                      ? 'inset-ring inset-ring-danger'
                      : ''
                }`}
              >
                {openingPath === project.path ? (
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
                {project.host && (
                  <span className="block text-bg/60">on {project.host}</span>
                )}
              </TooltipContent>
            </Tooltip>
          )
        })}
      </SidebarContent>

      {/* Settings alone down here. The cube that used to sit in this slot led
          back to the picker, which is what the platypus at the top of the rail
          already does — two links to one page, and the one people were looking
          for was missing. */}
      <SidebarFooter className="p-2.5">
        <Tooltip delay={0}>
          <Link
            to="/settings"
            aria-label="Settings"
            className="grid size-8 place-items-center rounded-lg text-muted-fg outline-hidden hover:bg-sidebar-accent hover:text-sidebar-accent-fg focus-visible:ring-2 focus-visible:ring-sidebar-ring"
          >
            <GearOutline18 aria-hidden className="size-5" />
          </Link>
          <TooltipContent inverse placement="right">
            Settings
          </TooltipContent>
        </Tooltip>
      </SidebarFooter>
    </Sidebar>
  )
}
