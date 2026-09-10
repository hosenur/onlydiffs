import { describe, expect, test } from 'bun:test'
import { isReviewed, nextUnreviewed, reviewProgress } from './review'
import type { ChangeStatus, FileChange } from '@/types'

function change(path: string, staged: boolean, status: ChangeStatus = 'modified'): FileChange {
  return {
    id: `${path}:${staged ? 'staged' : 'unstaged'}`,
    path,
    oldPath: null,
    status,
    staged,
    additions: 1,
    deletions: 0,
    binary: false,
    error: null,
  }
}

describe('isReviewed', () => {
  test('a staged file has been read and put away', () => {
    expect(isReviewed([change('a.ts', true)])).toBe(true)
  })

  test('an unstaged file has not', () => {
    expect(isReviewed([change('a.ts', false)])).toBe(false)
  })

  test('staging and then editing again reopens the file', () => {
    // Both halves are one path. The unstaged half is a change nobody has read.
    expect(isReviewed([change('a.ts', true), change('a.ts', false)])).toBe(false)
  })

  test('a path with no changes at all is not something to count as reviewed', () => {
    expect(isReviewed([])).toBe(false)
  })
})

describe('reviewProgress', () => {
  test('counts paths rather than diff rows', () => {
    const progress = reviewProgress([
      change('a.ts', true),
      change('b.ts', true),
      // One path, two rows — and unreviewed, because half of it is unstaged.
      change('c.ts', true),
      change('c.ts', false),
      change('d.ts', false),
    ])

    expect(progress).toEqual({ reviewed: 2, total: 4 })
  })

  test('a clean working tree has nothing to review', () => {
    expect(reviewProgress([])).toEqual({ reviewed: 0, total: 0 })
  })

  test('everything staged is everything reviewed', () => {
    const progress = reviewProgress([change('a.ts', true), change('b.ts', true)])

    expect(progress).toEqual({ reviewed: 2, total: 2 })
  })
})

describe('nextUnreviewed', () => {
  // Sidebar order: directories first, then files, each A–Z. So `src/…` rows
  // come before `README.md` whatever order the diff lists them in.
  const files = [
    change('README.md', false),
    change('src/b.ts', false),
    change('src/a.ts', true),
    change('package.json', false),
  ]

  test('with no file open, it is the first unreviewed row of the sidebar', () => {
    expect(nextUnreviewed(files, undefined)).toBe('src/b.ts')
  })

  test('from an open file, it is the next unreviewed row below it', () => {
    expect(nextUnreviewed(files, 'src/b.ts')).toBe('package.json')
    expect(nextUnreviewed(files, 'package.json')).toBe('README.md')
  })

  test('reviewed rows are stepped over', () => {
    // `src/a.ts` sits between the top and `src/b.ts` but is already staged.
    expect(nextUnreviewed(files, 'src/a.ts')).toBe('src/b.ts')
  })

  test('the bottom of the list wraps round to the top', () => {
    expect(nextUnreviewed(files, 'README.md')).toBe('src/b.ts')
  })

  test('a path staged and edited again still needs a visit', () => {
    const edited = [change('a.ts', true), change('a.ts', false), change('b.ts', true)]

    expect(nextUnreviewed(edited, 'b.ts')).toBe('a.ts')
  })

  test('nothing left means nowhere to go', () => {
    expect(nextUnreviewed([change('a.ts', true), change('b.ts', true)], 'a.ts')).toBeNull()
    expect(nextUnreviewed([], undefined)).toBeNull()
  })

  test('the only unreviewed file being the open one is also nowhere to go', () => {
    expect(nextUnreviewed([change('a.ts', false), change('b.ts', true)], 'a.ts')).toBeNull()
  })
})
