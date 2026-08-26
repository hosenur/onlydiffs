import { createContext, use, useMemo, useState } from 'react'
import type { FileDiffOptions } from '@pierre/diffs'

interface DiffLayoutValue {
  split: boolean
  setSplit: (split: boolean) => void
  options: FileDiffOptions<undefined>
}

const DiffLayoutContext = createContext<DiffLayoutValue | null>(null)

export function DiffLayoutProvider({ children }: { children: React.ReactNode }) {
  // Unified by default; the toggle for this is headed for a settings page.
  const [split, setSplit] = useState(false)

  const value = useMemo<DiffLayoutValue>(
    () => ({
      split,
      setSplit,
      options: {
        diffStyle: split ? 'split' : 'unified',
        themeType: 'system',
        hunkSeparators: 'line-info',
        // The renderer folds any run of unchanged lines (its threshold is 1),
        // which turns a single shared line into a "1 unmodified line" stub.
        expandUnchanged: true,
        lineDiffType: 'word-alt',
        diffIndicators: 'bars',
        stickyHeader: true,
        overflow: 'scroll',
      },
    }),
    [split]
  )

  return <DiffLayoutContext value={value}>{children}</DiffLayoutContext>
}

export function useDiffLayout() {
  const context = use(DiffLayoutContext)
  if (context === null) {
    throw new Error('useDiffLayout must be used within a DiffLayoutProvider')
  }
  return context
}
