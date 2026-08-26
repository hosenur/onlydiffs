import map from './file-icon-map.json'

const { fallback, folderFallback, fileNames, fileExtensions, folderNames } =
  map as {
    fallback: string
    folderFallback: string
    fileNames: Record<string, string>
    fileExtensions: Record<string, string>
    folderNames: Record<string, string>
  }

/**
 * Resolves a path to a Material icon id, mirroring the theme's own precedence:
 * an exact filename wins, then the longest matching extension — `ts.map` and
 * `schema.json` are real keys, so a plain last-segment lookup would miss them.
 */
export function fileIconId(path: string): string {
  const name = (path.split('/').pop() ?? path).toLowerCase()

  const byName = fileNames[name]
  if (byName) return byName

  const segments = name.split('.')
  // Skip index 0: that is the basename, not an extension.
  for (let i = 1; i < segments.length; i += 1) {
    const suffix = segments.slice(i).join('.')
    const byExtension = fileExtensions[suffix]
    if (byExtension) return byExtension
  }

  return fallback
}

/**
 * Served from public/. The URL has to go through `BASE_URL` rather than start
 * with a slash: the packaged app is loaded from `file://`, where a root-
 * relative path points at the filesystem root instead of the bundle.
 */
export function fileIconUrl(path: string): string {
  return `${import.meta.env.BASE_URL}file-icons/${fileIconId(path)}.svg`
}

/**
 * Resolves a directory to a Material folder icon.
 *
 * `chain` is what the row displays: one segment normally, or several when the
 * tree folded a run of single-child directories into `.github/workflows`. The
 * deepest segment wins, but the walk continues up the folded chain when the
 * theme has no icon for it — the theme knows `.github` and not `workflows`, and
 * folding should not cost you the better icon. Only segments the row actually
 * represents are considered, so an unrecognised directory stays a plain folder
 * rather than borrowing its parent's icon.
 *
 * The open variant is always the closed icon with `-open` appended — an
 * invariant `bun run sync:icons` verifies against the theme, which is why the
 * expanded map is not shipped.
 */
export function folderIconId(chain: string, isOpen = false): string {
  const segments = chain.split('/').filter((segment) => segment.length > 0)
  let base = folderFallback
  for (let index = segments.length - 1; index >= 0; index -= 1) {
    const match = folderNames[segments[index].toLowerCase()]
    if (match) {
      base = match
      break
    }
  }
  return isOpen ? `${base}-open` : base
}

export function folderIconUrl(chain: string, isOpen = false): string {
  return `${import.meta.env.BASE_URL}file-icons/${folderIconId(chain, isOpen)}.svg`
}
