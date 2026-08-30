import { describe, expect, test } from 'bun:test'
import { projectInitials, projectTint } from './project-identity'

describe('projectInitials', () => {
  test('takes one letter from each of the first two segments', () => {
    expect(projectInitials('omo-switch')).toBe('OS')
    expect(projectInitials('trybasebuild-www')).toBe('TW')
    expect(projectInitials('my_side_project')).toBe('MS')
    expect(projectInitials('dot.separated')).toBe('DS')
    expect(projectInitials('two words')).toBe('TW')
  })

  test('falls back to the first two letters of a single-word name', () => {
    expect(projectInitials('onlydiffs')).toBe('ON')
    expect(projectInitials('soil')).toBe('SO')
  })

  test('splits camelCase, so a one-word directory still gives two segments', () => {
    expect(projectInitials('MyProject')).toBe('MP')
    expect(projectInitials('reactRouter')).toBe('RR')
    expect(projectInitials('v2Engine')).toBe('VE')
  })

  test('ignores leading punctuation rather than turning it into a letter', () => {
    expect(projectInitials('.dotfiles')).toBe('DO')
    expect(projectInitials('@scope/package')).toBe('SP')
  })

  test('keeps working for names that are not Latin', () => {
    // `toUpperCase` is a no-op here, which is the correct result rather than a
    // fallback worth special-casing.
    expect(projectInitials('проект-два')).toBe('ПД')
    expect(projectInitials('日本語')).toBe('日本')
  })

  test('drops symbols instead of using one as a letter', () => {
    expect(projectInitials('🚀rocket')).toBe('RO')
  })

  test('never splits an astral character in half', () => {
    // Deseret capitals are letters outside the basic plane. Naive
    // `slice(0, 2)` would cut the first one in half and return two broken
    // surrogate halves, which render as a pair of replacement glyphs.
    const initials = projectInitials('\u{10400}\u{10401}\u{10402}')
    expect(Array.from(initials)).toHaveLength(2)
    expect(initials.length).toBe(4)
  })

  test('gives a placeholder rather than an empty tile', () => {
    expect(projectInitials('')).toBe('?')
    expect(projectInitials('---')).toBe('?')
  })

  test('is at most two characters, whatever the name', () => {
    for (const name of ['a-b-c-d-e', 'onlydiffs', 'MyLongProjectName', '日本語']) {
      expect(Array.from(projectInitials(name)).length).toBeLessThanOrEqual(2)
    }
  })
})

describe('projectTint', () => {
  test('is stable for a path, so a project keeps its colour', () => {
    expect(projectTint('/Users/me/code/app')).toBe(projectTint('/Users/me/code/app'))
  })

  test('separates projects whose names collide', () => {
    // Two checkouts of the same repository under different parents. The tint
    // reads the whole path, so the rail still tells them apart.
    expect(projectTint('/a/onlydiffs')).not.toBe(projectTint('/b/onlydiffs'))
  })
})
