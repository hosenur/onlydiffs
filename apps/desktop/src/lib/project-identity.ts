/**
 * How a project is identified on screen when it has no artwork of its own.
 *
 * A generic icon made every unresolved project look like every other one, so
 * these two functions stand in instead: initials from the directory name, over
 * a tint derived from the path. Both are pure functions of the project, so a
 * repository keeps the same letters and the same colour between launches.
 */

const PROJECT_TINTS = [
  'bg-blue-500/15 text-blue-700 dark:text-blue-300',
  'bg-violet-500/15 text-violet-700 dark:text-violet-300',
  'bg-emerald-500/15 text-emerald-700 dark:text-emerald-300',
  'bg-amber-500/20 text-amber-800 dark:text-amber-300',
  'bg-rose-500/15 text-rose-700 dark:text-rose-300',
  'bg-cyan-500/15 text-cyan-700 dark:text-cyan-300',
] as const

export function projectTint(path: string) {
  let hash = 0
  for (let index = 0; index < path.length; index += 1) {
    hash = (hash * 31 + path.charCodeAt(index)) >>> 0
  }
  return PROJECT_TINTS[hash % PROJECT_TINTS.length]
}

/**
 * Two letters for a directory name. `omo-switch` gives OS and `MyProject`
 * gives MP; a single-word name like `onlydiffs` falls back to its first two
 * letters.
 *
 * Splitting on non-letters rather than on ASCII keeps this working for names
 * that are not Latin, where `toUpperCase` is a no-op. Characters come off the
 * front through `Array.from`, so a name starting with an emoji or any other
 * astral character yields that character rather than half of its surrogate
 * pair.
 */
export function projectInitials(name: string): string {
  const segments = name
    // Break camelCase first, so `MyProject` is two segments rather than one.
    .replace(/(\p{Ll}|\p{N})(\p{Lu})/gu, '$1 $2')
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean)

  if (segments.length === 0) return '?'
  const letters =
    segments.length === 1
      ? Array.from(segments[0]).slice(0, 2)
      : [Array.from(segments[0])[0], Array.from(segments[1])[0]]
  return letters.join('').toUpperCase()
}
