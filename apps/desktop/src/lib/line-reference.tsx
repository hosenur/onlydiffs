import { createContext, use, useCallback, useMemo, useState } from 'react'

/**
 * The line the user last clicked in a diff, shared with the toolbar so it can
 * offer an input for it.
 *
 * A context rather than a prop because the two ends are far apart in the tree:
 * the click happens inside a diff card nested under the route outlet, and the
 * input lives in the toolbar beside it.
 */

export interface LineReference {
  /** Repo-relative path. */
  path: string
  lineNumber: number
  /** `src/lib/app.tsx:42` — what gets sent to Claude, which needs the path. */
  label: string
  /** `app.tsx` — the file name alone, which is what the toolbar shows. */
  name: string
}

interface LineReferenceValue {
  reference: LineReference | null
  select: (path: string, lineNumber: number) => void
  clear: () => void
}

const LineReferenceContext = createContext<LineReferenceValue | null>(null)

export function LineReferenceProvider({ children }: { children: React.ReactNode }) {
  const [reference, setReference] = useState<LineReference | null>(null)

  const select = useCallback((path: string, lineNumber: number) => {
    setReference({
      path,
      lineNumber,
      label: `${path}:${lineNumber}`,
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
