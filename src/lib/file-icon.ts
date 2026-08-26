import map from './file-icon-map.json'

const { fallback, fileNames, fileExtensions } = map as {
  fallback: string
  fileNames: Record<string, string>
  fileExtensions: Record<string, string>
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
