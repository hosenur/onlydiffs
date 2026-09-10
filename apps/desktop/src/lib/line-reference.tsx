import { createContext, use, useCallback, useMemo, useState } from 'react'

/**
 * The line the user last clicked in a diff, shared with the toolbar so it can
 * offer an input for it.
 *
 * A context rather than a prop because the two ends are far apart in the tree:
 * the click happens inside a diff card nested under the route outlet, and the
 * input lives in the toolbar beside it.
 */

/** What a click on a diff row knows about the row. */
export interface ClickedLine {
  lineNumber: number
  /**
   * The line exists only in the version before this change, so `lineNumber`
   * counts in that version. In a unified diff a removed line shows its old
   * number in the same column as its neighbours' new numbers, which is why the
   * gutter can read 5, 6, 5.
   */
  removed: boolean
  /** The line as shown, trimmed. Quoted to the agent when the line is removed,
   *  because the number alone points at whatever occupies it now. */
  text: string
}

export interface LineReference extends ClickedLine {
  /** Repo-relative path. */
  path: string
  /**
   * `src/lib/app.tsx:42` — what gets sent to Claude, which needs the path. A
   * removed line is marked, `src/lib/app.tsx:42 (removed)`, so the number is
   * never read against the current file.
   */
  label: string
  /** `app.tsx` — the file name alone, which is what the toolbar shows. */
  name: string
}

interface LineReferenceValue {
  reference: LineReference | null
  select: (path: string, line: ClickedLine) => void
  clear: () => void
}

const LineReferenceContext = createContext<LineReferenceValue | null>(null)

function referenceLabel(path: string, line: ClickedLine): string {
  const at = `${path}:${line.lineNumber}`
  return line.removed ? `${at} (removed)` : at
}

export function LineReferenceProvider({ children }: { children: React.ReactNode }) {
  const [reference, setReference] = useState<LineReference | null>(null)

  const select = useCallback((path: string, line: ClickedLine) => {
    setReference({
      path,
      ...line,
      label: referenceLabel(path, line),
      name: path.slice(path.lastIndexOf('/') + 1),
    })
  }, [])

  const clear = useCallback(() => setReference(null), [])

  const value = useMemo<LineReferenceValue>(
    () => ({ reference, select, clear }),
    [reference, select, clear]
  )

  return <LineReferenceContext value={value}>{children}</LineReferenceContext>
}

export function useLineReference(): LineReferenceValue {
  const context = use(LineReferenceContext)
  if (context === null) {
    throw new Error('useLineReference must be used inside a LineReferenceProvider')
  }
  return context
}
