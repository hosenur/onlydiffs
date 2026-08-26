import { existsSync } from 'node:fs'
import { expect, test } from 'bun:test'
import { fileIconId, folderIconId } from './file-icon'

const asset = (id: string) => `public/file-icons/${id}.svg`

test('a known folder name resolves to its themed icon', () => {
  expect(folderIconId('src')).toBe('folder-src')
  expect(folderIconId('node_modules')).toBe('folder-node')
})

test('the deepest segment of a folded chain wins', () => {
  expect(folderIconId('packages/app/src')).toBe('folder-src')
})

test('a folded chain falls back up to a segment the theme knows', () => {
  // The theme has `.github` but not `workflows`; folding must not lose that.
  expect(folderIconId('.github/workflows')).toBe('folder-github')
  expect(folderIconId('.github/workflows', true)).toBe('folder-github-open')
})

test('an unknown directory does not borrow its parent icon', () => {
  // Only what the row folded together counts, so this is a plain folder even
  // though a `src` sits above it in the tree.
  expect(folderIconId('mystery')).toBe('folder')
})

test('an unknown folder falls back to the plain folder icon', () => {
  expect(folderIconId('pkg000')).toBe('folder')
})

test('the open variant appends -open', () => {
  expect(folderIconId('src', true)).toBe('folder-src-open')
  expect(folderIconId('pkg000', true)).toBe('folder-open')
})

test('every folder icon the resolver can return was actually copied', () => {
  for (const [name, open] of [
    ['src', false],
    ['src', true],
    ['node_modules', true],
    ['unknown-dir-name', false],
    ['unknown-dir-name', true],
  ] as const) {
    const id = folderIconId(name, open)
    expect(existsSync(asset(id))).toBe(true)
  }
})

test('file icons still resolve, including multi-part extensions', () => {
  expect(fileIconId('a/b/app.d.ts')).toBe('typescript-def')
  expect(existsSync(asset(fileIconId('README.md')))).toBe(true)
})
