import { expect, test } from 'bun:test'
import {
  buildFileTree,
  directoriesContaining,
  expandedDirectories,
  flattenTree,
  indexChanges,
  NO_EXPANSION,
  toggleDirectory,
} from './file-tree'
import type { FileChange } from '@/types'

const PATHS = [
  'src/components/ui/button.tsx',
  'src/components/ui/tree.tsx',
  'src/components/app-sidebar.tsx',
  'src/lib/file-tree.ts',
  'README.md',
  'package.json',
]

const names = (rows: ReturnType<typeof flattenTree>) => rows.map((r) => r.node.name)

test('directories sort above files, each alphabetically', () => {
  const tree = buildFileTree(PATHS)
  expect(tree.map((n) => n.name)).toEqual(['src', 'package.json', 'README.md'])
  expect(tree[0].isDirectory).toBe(true)
})

test('a path becomes one leaf, and shared directories are created once', () => {
  const tree = buildFileTree(PATHS)
  const src = tree[0]
  expect(src.children.map((n) => n.name)).toEqual(['components', 'lib'])
  const components = src.children[0]
  expect(components.children.map((n) => n.name)).toEqual(['ui', 'app-sidebar.tsx'])
  expect(components.children[0].children).toHaveLength(2)
})

test('paths carry the full repo-relative path, not just the segment', () => {
  const tree = buildFileTree(PATHS)
  expect(tree[0].children[0].children[0].path).toBe('src/components/ui')
})

test('collapsed directories contribute one row and hide their contents', () => {
  const tree = buildFileTree(PATHS)
  const rows = flattenTree(tree, { expanded: new Set() })
  expect(names(rows)).toEqual(['src', 'package.json', 'README.md'])
})

test('expanding a directory reveals exactly its children', () => {
  const tree = buildFileTree(PATHS)
  const rows = flattenTree(tree, { expanded: new Set(['src']) })
  expect(names(rows)).toEqual([
    'src',
    'components',
    'lib',
    'package.json',
    'README.md',
  ])
  expect(rows[1].depth).toBe(1)
})

test('single-child directory chains fold into one row', () => {
  const tree = buildFileTree(['a/b/c/deep.ts', 'a/b/c/other.ts'])
  const rows = flattenTree(tree, { expanded: new Set() })
  expect(names(rows)).toEqual(['a/b/c'])
  // Expanding uses the deepest path in the chain.
  expect(rows[0].node.path).toBe('a/b/c')
})

test('compactFolders can be turned off', () => {
  const tree = buildFileTree(['a/b/c/deep.ts'])
  const rows = flattenTree(tree, { expanded: new Set(), compactFolders: false })
  expect(names(rows)).toEqual(['a'])
})

test('filtering shows matches and the directories holding them', () => {
  const tree = buildFileTree(PATHS)
  const rows = flattenTree(tree, { expanded: new Set(), filter: 'button' })
  // `src` and `components` each hold more than one entry, so neither folds.
  expect(names(rows)).toEqual(['src', 'components', 'ui', 'button.tsx'])
})

test('filtering drops directories with no match underneath', () => {
  const tree = buildFileTree(PATHS)
  const rows = flattenTree(tree, { expanded: new Set(), filter: 'readme' })
  expect(names(rows)).toEqual(['README.md'])
})

test('filtering matches on the whole path, not just the file name', () => {
  const tree = buildFileTree(PATHS)
  const rows = flattenTree(tree, { expanded: new Set(), filter: 'src/lib' })
  expect(names(rows)).toContain('file-tree.ts')
})

test('a path staged and modified again keeps both of its changes', () => {
  const change = (staged: boolean): FileChange => ({
    id: `${staged ? 'staged' : 'unstaged'}:a.ts`,
    path: 'a.ts',
    oldPath: null,
    status: 'modified',
    staged,
    additions: 1,
    deletions: 0,
    binary: false,
    error: null,
  })
  const index = indexChanges([change(true), change(false)])
  expect(index.get('a.ts')).toHaveLength(2)
})

test('directoriesContaining lists every ancestor of a change', () => {
  expect([...directoriesContaining(['src/components/ui/button.tsx'])]).toEqual([
    'src',
    'src/components',
    'src/components/ui',
  ])
})

test('scales: 50k paths build and flatten without walking everything', () => {
  const many: string[] = []
  for (let d = 0; d < 500; d += 1) {
    for (let f = 0; f < 100; f += 1) many.push(`pkg${d}/src/module${f}.ts`)
  }
  expect(many).toHaveLength(50_000)

  const start = performance.now()
  const tree = buildFileTree(many)
  const built = performance.now() - start

  const rows = flattenTree(tree, { expanded: new Set() })
  // Collapsed, a 50k-file repo is 500 rows — that is the whole optimisation.
  expect(rows).toHaveLength(500)
  expect(built).toBeLessThan(1000)
})

/*
 * The expansion model. What these are really about is the diff being re-read
 * constantly — on every save, stage, and window focus — while the tree the
 * user is working in has to hold still.
 */

test('with nothing touched, the tree is open to wherever the changes are', () => {
  const auto = directoriesContaining(['src/components/ui/button.tsx'])
  expect([...expandedDirectories(auto, NO_EXPANSION)].sort()).toEqual([
    'src',
    'src/components',
    'src/components/ui',
  ])
})

test('a directory closed by hand stays closed when the diff is read again', () => {
  const overrides = toggleDirectory(NO_EXPANSION, 'src/components', true)
  // Twice, because a re-read is what used to undo this.
  for (const paths of [['src/components/ui/button.tsx'], ['src/components/ui/tree.tsx']]) {
    expect(expandedDirectories(directoriesContaining(paths), overrides)).not.toContain(
      'src/components'
    )
  }
})

test('a directory opened by hand survives a diff that never mentions it', () => {
  const overrides = toggleDirectory(NO_EXPANSION, 'docs', false)
  expect(expandedDirectories(directoriesContaining(['src/a.ts']), overrides)).toContain('docs')
})

test('changes arriving in an untouched directory still open it', () => {
  // The half of the automatic behaviour worth keeping: only the directories
  // the user has ruled on are pinned, the rest follow the diff.
  const overrides = toggleDirectory(NO_EXPANSION, 'docs', false)
  const expanded = expandedDirectories(directoriesContaining(['src/lib/new.ts']), overrides)
  expect(expanded).toContain('src/lib')
})

test('toggling back to where the changes put it leaves no override behind', () => {
  const closed = toggleDirectory(NO_EXPANSION, 'src', true)
  const reopened = toggleDirectory(closed, 'src', false)
  expect(reopened.closed.has('src')).toBe(false)
  expect(expandedDirectories(directoriesContaining(['src/a.ts']), reopened)).toContain('src')
})

test('toggling does not mutate the overrides it was given', () => {
  const overrides = toggleDirectory(NO_EXPANSION, 'src', false)
  toggleDirectory(overrides, 'src', true)
  expect(overrides.opened.has('src')).toBe(true)
  expect(NO_EXPANSION.opened.size).toBe(0)
})

test('a closed directory contributes one row and hides its contents', () => {
  const tree = buildFileTree(PATHS)
  const auto = directoriesContaining(['src/components/ui/button.tsx'])
  const overrides = toggleDirectory(NO_EXPANSION, 'src/components/ui', true)

  // The row itself stays — its parent is still open — it just stops
  // contributing what is underneath it.
  const rows = flattenTree(tree, { expanded: expandedDirectories(auto, overrides) })
  expect(names(rows)).toContain('ui')
  expect(names(rows)).not.toContain('button.tsx')
})
