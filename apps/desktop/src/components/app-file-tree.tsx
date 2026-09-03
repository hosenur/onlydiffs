import { memo, useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from 'react'
import { Link } from '@tanstack/react-router'
import { useVirtualizer } from '@tanstack/react-virtual'
import { ChevronRightIcon, MagnifyingGlassIcon } from '@heroicons/react/16/solid'
import type { TreeRow } from '@/lib/file-tree'
import {
  buildFileTree,
  directoriesContaining,
  flattenTree,
  indexChanges,
} from '@/lib/file-tree'
import { fileIconUrl, folderIconUrl } from '@/lib/file-icon'
import { isReviewed } from '@/lib/review'
import { fileHref } from '@/lib/status'
import type { FileChange } from '@/types'

/** Row height in px. Fixed, so the virtualiser needs no measurement pass. */
const ROW_HEIGHT = 26
/** Below this, rendering every row outright is cheaper than virtualising. */
const VIRTUALISE_ABOVE = 150
const INDENT = 10

interface AppFileTreeProps {
  /** Every file in the repository, from `git ls-files`. */
  paths: string[]
  /** The current diff, used to mark and colour changed rows. */
  files: FileChange[]
  /** Repo-relative path of the file being viewed, if any. */
  current: string | undefined
}

/**
 * `+12 −3` for a changed file, summed across its staged and unstaged halves,
 * and whether the file is done with. `reviewed` is the same rule the count in
 * the toolbar uses, so a green name and that count can never disagree — which
 * they did while this asked only whether *any* half was staged, and painted a
 * file green that had been staged and then edited again.
 */
function changeSummary(changes: FileChange[]) {
  let additions = 0
  let deletions = 0
  for (const change of changes) {
    additions += change.additions
    deletions += change.deletions
  }
  return { additions, deletions, reviewed: isReviewed(changes) }
}

const Row = memo(function Row({
  row,
  changes,
  isCurrent,
  onToggle,
}: {
  row: TreeRow
  changes: FileChange[] | undefined
  isCurrent: boolean
  onToggle: (path: string) => void
}) {
  const { node, depth } = row
  const indent = 6 + depth * INDENT

  if (node.isDirectory) {
    return (
      <button
        type="button"
        onClick={() => onToggle(node.path)}
        title={node.path}
        style={{ paddingInlineStart: indent, height: ROW_HEIGHT }}
        className="flex w-full min-w-0 items-center gap-1 pe-2 text-start hover:bg-sidebar-accent"
      >
        <ChevronRightIcon
          aria-hidden
          className={`size-3 shrink-0 text-muted-fg transition-transform ${
            row.isExpanded ? 'rotate-90' : ''
          }`}
        />
        <img
          src={folderIconUrl(node.name, row.isExpanded)}
          alt=""
          width={14}
          height={14}
          className="size-3.5 shrink-0"
        />
        <span className="truncate text-xs">{node.name}</span>
      </button>
    )
  }

  const summary = changes ? changeSummary(changes) : null

  return (
    <Link
      to={fileHref(node.path)}
      title={node.path}
      style={{ paddingInlineStart: indent + 16, height: ROW_HEIGHT }}
      className={`flex w-full min-w-0 items-center gap-1.5 pe-2 hover:bg-sidebar-accent ${
        isCurrent ? 'bg-sidebar-accent font-medium' : ''
      }`}
    >
      <img src={fileIconUrl(node.path)} alt="" width={14} height={14} className="size-3.5 shrink-0" />
      <span
        className={`truncate text-xs ${
          summary === null ? 'text-muted-fg' : summary.reviewed ? 'text-success-subtle-fg' : 'text-fg'
        }`}
      >
        {node.name}
      </span>
      {summary !== null && (
        <span className="ms-auto shrink-0 font-mono text-[10px] tabular-nums">
          {summary.additions > 0 && <span className="text-success-subtle-fg">+{summary.additions}</span>}
          {summary.deletions > 0 && <span className="ms-1 text-danger-subtle-fg">−{summary.deletions}</span>}
        </span>
      )}
    </Link>
  )
})

export function AppFileTree({ paths, files, current }: AppFileTreeProps) {
  const [filter, setFilter] = useState('')
  // Typing stays responsive while the (much heavier) re-flatten catches up.
  const deferredFilter = useDeferredValue(filter)
  const scroller = useRef<HTMLDivElement>(null)

  const tree = useMemo(() => buildFileTree(paths), [paths])
  const changesByPath = useMemo(() => indexChanges(files), [files])

  // Open to wherever the changes are: a diff viewer that starts fully collapsed
  // hides the only thing the user came for.
  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  useEffect(() => {
    setExpanded(directoriesContaining(files.map((file) => file.path)))
  }, [files])

  const rows = useMemo(
    () => flattenTree(tree, { expanded, filter: deferredFilter }),
    [tree, expanded, deferredFilter]
  )

  const onToggle = useCallback((path: string) => {
    setExpanded((current) => {
      const next = new Set(current)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
  }, [])

  const virtualiser = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
    enabled: rows.length > VIRTUALISE_ABOVE,
  })
  const isVirtual = rows.length > VIRTUALISE_ABOVE
  const items = virtualiser.getVirtualItems()

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-1.5 px-3 pb-2">
        <MagnifyingGlassIcon aria-hidden className="size-3.5 shrink-0 text-muted-fg" />
        <input
          value={filter}
          onChange={(event) => setFilter(event.target.value)}
          placeholder="Filter files"
          spellCheck={false}
          autoComplete="off"
          aria-label="Filter files"
          className="min-w-0 flex-1 bg-transparent text-xs outline-hidden placeholder:text-muted-fg"
        />
        {rows.length > 0 && (
          <span className="shrink-0 font-mono text-[10px] text-muted-fg">{rows.length}</span>
        )}
      </div>

      <div ref={scroller} role="tree" className="min-h-0 flex-1 overflow-y-auto">
        {rows.length === 0 ? (
          <p className="px-3 py-6 text-center text-muted-fg text-xs">
            {filter ? 'No file matches.' : 'No files.'}
          </p>
        ) : isVirtual ? (
          <div style={{ height: virtualiser.getTotalSize(), position: 'relative' }}>
            {items.map((item) => (
              <div
                key={rows[item.index].node.path}
                style={{
                  position: 'absolute',
                  top: 0,
                  insetInline: 0,
                  transform: `translateY(${item.start}px)`,
                }}
              >
                <Row
                  row={rows[item.index]}
                  changes={changesByPath.get(rows[item.index].node.path)}
                  isCurrent={rows[item.index].node.path === current}
                  onToggle={onToggle}
                />
              </div>
            ))}
          </div>
        ) : (
          rows.map((row) => (
            <Row
              key={row.node.path}
              row={row}
              changes={changesByPath.get(row.node.path)}
              isCurrent={row.node.path === current}
              onToggle={onToggle}
            />
          ))
        )}
      </div>
    </div>
  )
}
