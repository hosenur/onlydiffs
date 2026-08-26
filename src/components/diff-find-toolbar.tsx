import { useEffect, useRef, useState } from 'react'
import type { RefObject } from 'react'
import { ChevronDownIcon, ChevronUpIcon } from '@heroicons/react/16/solid'
import { Button } from '@/components/ui/button'
import { Toolbar, ToolbarGroup, ToolbarSeparator } from '@/components/ui/toolbar'
import { runIpc, writeClipboardText } from '@/lib/ipc'
import {
  diffHost,
  diffSelection,
  findOccurrences,
  formatLineRef,
  lineNumbersInRange,
  matchIndex,
  rangeInHost,
  toolbarAnchor,
} from '@/lib/diff-find'

interface DiffFindToolbarProps {
  rootRef: RefObject<HTMLElement | null>
  path: string
}

interface FindState {
  query: string
  matches: Range[]
  index: number
  coords: string | null
  top: number
  left: number
  multiline: boolean
}

export function DiffFindToolbar({ rootRef, path }: DiffFindToolbarProps) {
  const [state, setState] = useState<FindState | null>(null)
  const skipCopy = useRef(false)
  const lastCoords = useRef<string | null>(null)

  useEffect(() => {
    lastCoords.current = null
  }, [path])

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined

    function read(): FindState | null {
      const host = diffHost(rootRef.current)
      if (!host) return null
      const selection = diffSelection(host)
      if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return null

      const range = selection.getRangeAt(0)
      if (!rangeInHost(range, host)) return null

      const query = selection.toString()
      if (query.length === 0 || !query.trim()) return null

      const multiline = query.includes('\n')
      const matches = findOccurrences(host, query)
      const found = matchIndex(matches, range)
      const index = found === -1 ? 0 : found
      const coords = formatLineRef(path, lineNumbersInRange(host, range))
      const anchor = toolbarAnchor(range)
      if (!anchor) return null

      return {
        query,
        matches,
        index,
        coords,
        top: anchor.top,
        left: Math.min(Math.max(anchor.left, 96), window.innerWidth - 96),
        multiline,
      }
    }

    function apply(copy: boolean) {
      const next = read()
      setState(next)
      if (!copy || skipCopy.current) {
        skipCopy.current = false
        return
      }
      if (!next?.coords || next.coords === lastCoords.current) return
      lastCoords.current = next.coords
      void runIpc(writeClipboardText(next.coords))
    }

    function onSelectionChange() {
      clearTimeout(timer)
      timer = setTimeout(() => apply(true), 80)
    }

    function onScroll() {
      const next = read()
      setState(next)
    }

    document.addEventListener('selectionchange', onSelectionChange)
    window.addEventListener('scroll', onScroll, true)
    return () => {
      clearTimeout(timer)
      document.removeEventListener('selectionchange', onSelectionChange)
      window.removeEventListener('scroll', onScroll, true)
    }
  }, [path, rootRef])

  function go(delta: number) {
    if (!state || state.matches.length === 0) return
    const host = diffHost(rootRef.current)
    if (!host) return

    const index = (state.index + delta + state.matches.length) % state.matches.length
    const match = state.matches[index]
    if (!match) return

    skipCopy.current = true
    const selection = diffSelection(host)
    selection?.removeAllRanges()
    selection?.addRange(match)

    const node = match.startContainer instanceof Element
      ? match.startContainer
      : match.startContainer.parentElement
    node?.scrollIntoView({ block: 'nearest' })

    const anchor = toolbarAnchor(match)
    setState({
      ...state,
      index,
      top: anchor?.top ?? state.top,
      left: anchor ? Math.min(Math.max(anchor.left, 96), window.innerWidth - 96) : state.left,
    })
  }

  if (!state) return null

  const count = state.matches.length
  const label = state.multiline
    ? (state.coords ?? 'Selection')
    : count === 0
      ? 'No matches'
      : `${state.index + 1} of ${count}`

  return (
    <div
      className="diff-find-toolbar"
      style={{ top: state.top, left: state.left }}
      onPointerDown={(event) => event.preventDefault()}
    >
      <Toolbar aria-label="Find in file" className="shadow-lg">
        <ToolbarGroup aria-label="Matches">
          <span className="px-1.5 font-mono text-xs whitespace-nowrap text-muted-fg">{label}</span>
        </ToolbarGroup>
        {!state.multiline && (
          <>
            <ToolbarSeparator />
            <ToolbarGroup aria-label="Navigate">
              <Button
                intent="plain"
                size="sq-xs"
                aria-label="Previous match"
                isDisabled={count < 2}
                onPress={() => go(-1)}
              >
                <ChevronUpIcon />
              </Button>
              <Button
                intent="plain"
                size="sq-xs"
                aria-label="Next match"
                isDisabled={count < 2}
                onPress={() => go(1)}
              >
                <ChevronDownIcon />
              </Button>
            </ToolbarGroup>
          </>
        )}
      </Toolbar>
    </div>
  )
}
