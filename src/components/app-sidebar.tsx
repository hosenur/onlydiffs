import { useParams } from '@tanstack/react-router'
import { Squares2X2Icon } from '@heroicons/react/24/outline'
import platypus from '@/assets/platypus.png'
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarItem,
  SidebarLabel,
  SidebarRail,
  SidebarSection,
  SidebarSectionGroup,
} from '@/components/ui/sidebar'
import { CodeBranchOutline18 } from '@/icons'
import { fileIconUrl } from '@/lib/file-icon'
import { fileHref, splitPath } from '@/lib/status'
import type { RepoDiff } from '@/types'

interface AppSidebarProps extends React.ComponentProps<typeof Sidebar> {
  diff: RepoDiff
}

export function AppSidebar({ diff, ...props }: AppSidebarProps) {
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
          {/* Hidden alongside SidebarLabel, which drops itself when docked. */}
          <div className="flex min-w-0 flex-col group-data-[collapsible=dock]:hidden">
            <SidebarLabel className="truncate font-medium">
              {diff.repoPath.split('/').pop()}
            </SidebarLabel>
            <span className="flex min-w-0 items-center gap-x-1 text-muted-fg">
              <CodeBranchOutline18 aria-hidden className="size-3 shrink-0" />
              <span className="truncate font-mono text-xs" title={diff.branch}>
                {diff.branch}
              </span>
            </span>
          </div>
        </div>
      </SidebarHeader>

      <SidebarContent>
        <SidebarSectionGroup>
          <SidebarSection className="px-4 py-2">
            <SidebarItem tooltip="All changes" href="/" isCurrent={current === undefined}>
              <Squares2X2Icon />
              <SidebarLabel>All changes</SidebarLabel>
            </SidebarItem>
          </SidebarSection>

          {(['staged', 'unstaged'] as const).map((half) => {
            const rows = diff.files.filter((file) => (half === 'staged') === file.staged)
            if (rows.length === 0) return null
            return (
              <SidebarSection key={half} label={`${half} (${rows.length})`} className="px-4 py-2">
                {rows.map((file) => {
                  const { name } = splitPath(file.path)
                  return (
                    <SidebarItem
                      key={file.id}
                      tooltip={file.path}
                      href={fileHref(file.path)}
                      isCurrent={current === file.path}
                    >
                      {/*
                        SidebarItem only auto-spaces an `svg` or an avatar
                        before the label, so an `<img>` needs Intent's own
                        `me-2` applied by hand.
                      */}
                      <img
                        src={fileIconUrl(file.path)}
                        alt=""
                        width={16}
                        height={16}
                        className="me-2 size-4 shrink-0"
                      />
                      <SidebarLabel className="truncate">{name}</SidebarLabel>
                    </SidebarItem>
                  )
                })}
              </SidebarSection>
            )
          })}
        </SidebarSectionGroup>
      </SidebarContent>

      <SidebarFooter>
        <div className="flex items-center justify-between px-2 py-1 font-mono text-xs">
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
