import { ClaudeMessageInput } from '@/components/claude-message-input'
import { CommitMessageGenerator } from '@/components/commit-message-generator'
import { Avatar } from '@/components/ui/avatar'
import { Badge } from '@/components/ui/badge'
import { Sidebar, SidebarHeader, SidebarLabel } from '@/components/ui/sidebar'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useAvatarUrl } from '@/hooks/use-avatar-url'
import {
  CodeBranchOutline18,
  CodeCommitOutline18,
  CodeMergeOutline18,
  CodePullRequestOutline18,
} from '@/icons'
import { authorInitials } from '@/lib/avatar'
import type { Commit, FileChange } from '@/types'

/** GitHub writes "Merge pull request #N from …"; anything else is a plain merge. */
function commitIcon(commit: Commit) {
  if (!commit.isMerge) return CodeCommitOutline18
  return commit.subject.startsWith('Merge pull request')
    ? CodePullRequestOutline18
    : CodeMergeOutline18
}

const fullDate = new Intl.DateTimeFormat(undefined, {
  dateStyle: 'medium',
  timeStyle: 'short',
})

/** Avatar plus the authorship the row no longer has room to spell out. */
function CommitAuthor({ commit }: { commit: Commit }) {
  const src = useAvatarUrl(commit.authorEmail)

  return (
    <Tooltip delay={300}>
      <TooltipTrigger
        aria-label={`${commit.author}, ${commit.relativeDate}`}
        className="shrink-0 cursor-default rounded-full focus:outline-0 focus-visible:outline-2 focus-visible:outline-ring focus-visible:outline-offset-2"
      >
        {/* alt is empty on purpose: a broken image then collapses to nothing
            and lets the initials behind it show through. */}
        <Avatar
          size="xs"
          src={src}
          alt=""
          initials={authorInitials(commit.author)}
          className="bg-secondary text-secondary-fg"
        />
      </TooltipTrigger>
      <TooltipContent placement="left">
        <div className="flex flex-col">
          <strong>{commit.author}</strong>
          {commit.authorEmail && (
            <span className="font-mono text-muted-fg text-xs">{commit.authorEmail}</span>
          )}
          <span className="text-muted-fg text-xs">
            {commit.relativeDate} · {fullDate.format(new Date(commit.date))}
          </span>
        </div>
      </TooltipContent>
    </Tooltip>
  )
}

function CommitRow({ commit }: { commit: Commit }) {
  const Icon = commitIcon(commit)

  return (
    <li className="flex items-start gap-2 border-b px-3 py-2 last:border-b-0">
      <CommitAuthor commit={commit} />

      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <div className="flex items-baseline gap-2">
          <Icon
            aria-hidden
            className={`size-4 shrink-0 translate-y-0.5 ${
              commit.isMerge ? 'text-primary' : 'text-muted-fg'
            }`}
          />
          <span className="shrink-0 font-mono text-muted-fg text-xs">{commit.shortHash}</span>
          <span className="min-w-0 flex-1 truncate text-sm" title={commit.subject}>
            {commit.subject}
          </span>
        </div>

        {commit.refs && (
          <div className="flex flex-wrap gap-1">
            {commit.refs.split(', ').map((ref) => (
              <Badge key={ref} intent="outline" className="font-mono">
                {ref}
              </Badge>
            ))}
          </div>
        )}
      </div>
    </li>
  )
}

interface AppHistorySidebarProps extends React.ComponentProps<typeof Sidebar> {
  branch: string
  files: FileChange[]
  history: Commit[]
}

export function AppHistorySidebar({ branch, files, history, ...props }: AppHistorySidebarProps) {
  return (
    <Sidebar side="right" collapsible="none" className="border-l" {...props}>
      <div className="flex h-1/2 min-h-0 shrink-0 flex-col overflow-y-auto">
        <ClaudeMessageInput />
        <CommitMessageGenerator files={files} />
      </div>

      <div className="flex h-1/2 min-h-0 shrink-0 flex-col">
        <SidebarHeader className="flex-row items-center gap-2">
          <CodeBranchOutline18 aria-hidden className="size-4 shrink-0 text-muted-fg" />
          <SidebarLabel className="font-medium">History</SidebarLabel>
          <span className="truncate font-mono text-muted-fg text-xs">{branch}</span>
        </SidebarHeader>

        {history.length === 0 ? (
          <p className="px-3 py-2 text-muted-fg text-sm">No commits.</p>
        ) : (
          <ul className="min-h-0 flex-1 overflow-y-auto">
            {history.map((commit) => (
              <CommitRow key={commit.hash} commit={commit} />
            ))}
          </ul>
        )}
      </div>
    </Sidebar>
  )
}
