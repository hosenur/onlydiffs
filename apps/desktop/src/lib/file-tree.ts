import type { FileChange } from '@/types'

/**
 * Turns the flat path list from `git ls-files` into a directory tree, and
 * flattens the expanded part of that tree back into rows for rendering.
 *
 * The two halves are separate on purpose: building is O(paths) and happens once
 * per repository load, while flattening runs on every expand, collapse, and
 * keystroke in the filter.
 */

export interface TreeNode {
  /** Last path segment — what the row shows. */
  name: string
  /** Full repo-relative path. Unique, so it doubles as the React key. */
  path: string
  isDirectory: boolean
  /** Directories first, then files, each A–Z. Empty for a file. */
  children: TreeNode[]
}

export interface TreeRow {
  node: TreeNode
  /** 0 for a top-level entry; drives the indent. */
  depth: number
  /** Only meaningful for directories. */
  isExpanded: boolean
}

/** Directories above files, then case-insensitive by name. */
function compare(a: TreeNode, b: TreeNode): number {
  if (a.isDirectory !== b.isDirectory) return a.isDirectory ? -1 : 1
  return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
}

/**
 * Builds the tree from repo-relative paths. Sorting happens once here rather
 * than on every render.
 */
export function buildFileTree(paths: readonly string[]): TreeNode[] {
  const root: TreeNode = { name: '', path: '', isDirectory: true, children: [] }
  // Directories are looked up by path so each one is created exactly once,
  // which keeps this O(total segments) rather than O(paths × depth).
  const directories = new Map<string, TreeNode>([['', root]])

  for (const path of paths) {
    const segments = path.split('/').filter((segment) => segment.length > 0)
    if (segments.length === 0) continue

    let parent = root
    let prefix = ''

    for (let index = 0; index < segments.length; index += 1) {
      const segment = segments[index]
      prefix = prefix === '' ? segment : `${prefix}/${segment}`
      const isLeaf = index === segments.length - 1

      if (isLeaf) {
        parent.children.push({
          name: segment,
          path: prefix,
          isDirectory: false,
          children: [],
        })
        break
      }

      let directory = directories.get(prefix)
      if (directory === undefined) {
        directory = { name: segment, path: prefix, isDirectory: true, children: [] }
        directories.set(prefix, directory)
        parent.children.push(directory)
      }
      parent = directory
    }
  }

  for (const directory of directories.values()) directory.children.sort(compare)
  return root.children
}

/**
 * A directory whose only child is another directory is folded into one row —
 * `src/components/ui` rather than three nested rows holding one entry each.
 * Returns the chain of nodes that collapsed together, deepest last.
 */
function collapseChain(node: TreeNode): TreeNode[] {
  const chain = [node]
  let current = node
  while (
    current.isDirectory &&
    current.children.length === 1 &&
    current.children[0].isDirectory
  ) {
    current = current.children[0]
    chain.push(current)
  }
  return chain
}

export interface FlattenOptions {
  /** Paths of directories the user has opened. */
  expanded: ReadonlySet<string>
  /** Lower-cased substring; empty means no filtering. */
  filter?: string
  /** Fold single-child directory chains into one row. */
  compactFolders?: boolean
}

/**
 * Produces exactly the rows that should be on screen. Collapsed directories
 * contribute one row and none of their contents, which is what keeps a
 * 50,000-file repository cheap to render without a virtualiser.
 */
export function flattenTree(
  nodes: readonly TreeNode[],
  options: FlattenOptions
): TreeRow[] {
  const { expanded, compactFolders = true } = options
  const filter = options.filter?.trim().toLowerCase() ?? ''
  const rows: TreeRow[] = []

  const walk = (siblings: readonly TreeNode[], depth: number): void => {
    for (const node of siblings) {
      if (!node.isDirectory) {
        if (filter === '' || node.path.toLowerCase().includes(filter)) {
          rows.push({ node, depth, isExpanded: false })
        }
        continue
      }

      const chain = compactFolders ? collapseChain(node) : [node]
      const tail = chain[chain.length - 1]
      const display: TreeNode = {
        ...tail,
        name: chain.map((link) => link.name).join('/'),
      }

      // While filtering, a directory earns its row only if something under it
      // matches, and it is opened regardless of what the user had expanded.
      if (filter !== '') {
        const before = rows.length
        rows.push({ node: display, depth, isExpanded: true })
        walk(tail.children, depth + 1)
        if (rows.length === before + 1) rows.pop()
        continue
      }

      const isExpanded = expanded.has(tail.path)
      rows.push({ node: display, depth, isExpanded })
      if (isExpanded) walk(tail.children, depth + 1)
    }
  }

  walk(nodes, 0)
  return rows
}

/**
 * Change status per path. A path edited and staged has two rows in the diff, so
 * this keeps both and lets the caller decide which to show.
 */
export function indexChanges(
  files: readonly FileChange[]
): Map<string, FileChange[]> {
  const byPath = new Map<string, FileChange[]>()
  for (const file of files) {
    const existing = byPath.get(file.path)
    if (existing) existing.push(file)
    else byPath.set(file.path, [file])
  }
  return byPath
}

/** Every directory containing a change, so the tree can open to them. */
export function directoriesContaining(paths: Iterable<string>): Set<string> {
  const directories = new Set<string>()
  for (const path of paths) {
    const segments = path.split('/')
    let prefix = ''
    for (let index = 0; index < segments.length - 1; index += 1) {
      prefix = prefix === '' ? segments[index] : `${prefix}/${segments[index]}`
      directories.add(prefix)
    }
  }
  return directories
}
