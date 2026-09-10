import type { ReactNode } from 'react'

/**
 * A sentence in the middle of an otherwise empty pane.
 *
 * `min-h-full` rather than `h-full`: the pane it fills scrolls, and a message
 * that ever outgrew it should push the pane taller, not clip. The width cap
 * keeps a long sentence to a readable line or two instead of one wide strip.
 */
export function EmptyState({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-full items-center justify-center p-5">
      <p className="max-w-md text-center text-muted-fg">{children}</p>
    </div>
  )
}
