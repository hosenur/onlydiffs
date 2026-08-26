/**
 * Find and line-reference helpers for a Pierre `diffs-container`.
 *
 * The renderer puts the file in an open shadow root. Searchable lines are the
 * current-file ones: unified minus deletions, or the additions column in split
 * (so context isn't counted twice).
 */

export function diffHost(root: HTMLElement | null): Element | null {
  return root?.querySelector('diffs-container') ?? null
}

export function diffSelection(host: Element): Selection | null {
  const shadow = host.shadowRoot as (ShadowRoot & { getSelection?: () => Selection | null }) | null
  return shadow?.getSelection?.() ?? document.getSelection()
}

export function rangeInHost(range: Range, host: Element): boolean {
  const root = host.shadowRoot
  return root != null && root.contains(range.commonAncestorContainer)
}

export function searchableLines(host: Element): HTMLElement[] {
  const root = host.shadowRoot
  if (!root) return []

  const columns = root.querySelectorAll('code[data-additions], code[data-unified]')
  const lines: HTMLElement[] = []
  for (const column of columns) {
    for (const node of column.querySelectorAll('[data-line]')) {
      if (!(node instanceof HTMLElement)) continue
      if (node.getAttribute('data-line-type') === 'change-deletion') continue
      lines.push(node)
    }
  }
  return lines
}

function pointAt(
  nodes: Text[],
  starts: number[],
  abs: number
): { node: Text; offset: number } | null {
  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i]!
    const start = starts[i]!
    const end = start + node.data.length
    if (abs < end || (abs === end && i === nodes.length - 1)) {
      return { node, offset: abs - start }
    }
  }
  return null
}

function rangesInLine(line: HTMLElement, query: string): Range[] {
  const nodes: Text[] = []
  const starts: number[] = []
  let haystack = ''

  const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT)
  let node: Node | null
  while ((node = walker.nextNode())) {
    const text = node as Text
    if (text.data.length === 0) continue
    starts.push(haystack.length)
    nodes.push(text)
    haystack += text.data
  }

  if (query.length === 0 || !haystack.includes(query)) return []

  const ranges: Range[] = []
  let from = 0
  while (from <= haystack.length - query.length) {
    const index = haystack.indexOf(query, from)
    if (index === -1) break
    const start = pointAt(nodes, starts, index)
    const end = pointAt(nodes, starts, index + query.length)
    if (start && end) {
      const range = document.createRange()
      range.setStart(start.node, start.offset)
      range.setEnd(end.node, end.offset)
      ranges.push(range)
    }
    from = index + query.length
  }
  return ranges
}

export function findOccurrences(host: Element, query: string): Range[] {
  if (query.length === 0 || query.includes('\n')) return []
  return searchableLines(host).flatMap((line) => rangesInLine(line, query))
}

export function matchIndex(matches: Range[], current: Range): number {
  const exact = matches.findIndex(
    (match) =>
      match.compareBoundaryPoints(Range.START_TO_START, current) === 0 &&
      match.compareBoundaryPoints(Range.END_TO_END, current) === 0
  )
  if (exact !== -1) return exact
  return matches.findIndex((match) => {
    try {
      return match.intersectsNode(current.commonAncestorContainer)
    } catch {
      return false
    }
  })
}

export function lineNumbersInRange(host: Element, range: Range): number[] {
  const numbers: number[] = []
  for (const line of searchableLines(host)) {
    if (!range.intersectsNode(line)) continue
    const number = Number(line.getAttribute('data-line'))
    if (Number.isFinite(number)) numbers.push(number)
  }
  return numbers
}

export function formatLineRef(path: string, lines: number[]): string | null {
  if (lines.length === 0) return null
  const start = Math.min(...lines)
  const end = Math.max(...lines)
  return start === end ? `${path}:${start}` : `${path}:${start}-${end}`
}

export function toolbarAnchor(range: Range): { top: number; left: number } | null {
  const rect = range.getBoundingClientRect()
  if (rect.width === 0 && rect.height === 0) return null
  return {
    top: rect.top,
    left: rect.left + rect.width / 2,
  }
}
