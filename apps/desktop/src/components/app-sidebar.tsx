import { Link, useParams } from '@tanstack/react-router'
import { ArrowsRightLeftIcon } from '@heroicons/react/16/solid'
import platypus from '@/assets/platypus.png'
import { AppFileTree } from '@/components/app-file-tree'
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarRail,
} from '@/components/ui/sidebar'
import { CodeBranchOutline18 } from '@/icons'
import type { RepoDiff } from '@/types'

interface AppSidebarProps extends React.ComponentProps<typeof Sidebar> {
  diff: RepoDiff
  /** Every file in the repository, not just the changed ones. */
  paths: string[]
}

export function AppSidebar({ diff, paths, ...props }: AppSidebarProps) {
  // `strict: false` — this renders on both `/` and `/file/$`, and only the
  // latter has a splat param.
  const params = useParams({ strict: false }) as { _splat?: string }
  const current = params._splat

  const totals = diff.files.reduce(
    (acc, file) => ({
      additions: acc.additions + file.additions,
      deletions: acc.deletions + file.deletions,
    }),
    { additions: 0, deletions: 0 }
  )

  return (
    <Sidebar {...props}>
      <SidebarHeader>
        <div className="flex min-w-0 items-center gap-x-2.5">
          {/* Artwork is dark-on-white with no alpha, so give it a tile. */}
          <img
            src={platypus}
            alt=""
            width={32}
            height={32}
            className="size-8 shrink-0 rounded-md"
          />
          <div className="flex min-w-0 flex-col">
            {/* The way back to the project picker, and the only one in the
                chrome — so it carries an icon rather than relying on someone
                guessing that the repository name is a link. */}
            <Link
              to="/"
              title="Switch project"
              className="flex min-w-0 items-center gap-x-1 font-medium hover:underline"
            >
              <span className="truncate">{diff.repoPath.split('/').pop()}</span>
              <ArrowsRightLeftIcon
                aria-hidden
                className="size-3 shrink-0 text-muted-fg"
              />
            </Link>
            <span className="flex min-w-0 items-center gap-x-1 text-muted-fg">
              <CodeBranchOutline18 aria-hidden className="size-3 shrink-0" />
              <span className="truncate font-mono text-xs" title={diff.branch}>
                {diff.branch}
              </span>
            </span>
          </div>
        </div>
      </SidebarHeader>

      <SidebarContent className="px-0">
        {/* The tree owns its own scrolling so the virtualiser has a container. */}
        <AppFileTree paths={paths} files={diff.files} current={current} />
      </SidebarContent>

      <SidebarFooter>
        <div className="flex w-full items-center justify-between px-2 py-1 font-mono text-xs">
          <span className="text-muted-fg">
            {diff.files.length} {diff.files.length === 1 ? 'file' : 'files'}
          </span>
          <span className="flex gap-1.5">
            {totals.additions > 0 && (
              <span className="text-success-subtle-fg">+{totals.additions}</span>
            )}
            {totals.deletions > 0 && (
              <span className="text-danger-subtle-fg">−{totals.deletions}</span>
            )}
          </span>
        </div>
      </SidebarFooter>

      <SidebarRail />
    </Sidebar>
  )
}
