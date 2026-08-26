/**
 * What the diff pane shows before a file is picked. There is no "all changes"
 * view any more — the tree is how you choose what to look at, so this stays
 * out of the way rather than offering a second thing to read.
 */
export function AppNoSelection() {
  return (
    <div className="flex h-full items-center justify-center">
      <p className="text-2xl text-muted-fg">hi!</p>
    </div>
  )
}
