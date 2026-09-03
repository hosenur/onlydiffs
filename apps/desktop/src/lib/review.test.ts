import { describe, expect, test } from 'bun:test'
import { isReviewed, reviewProgress } from './review'
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
