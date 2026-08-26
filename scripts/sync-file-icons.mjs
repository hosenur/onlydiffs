/**
 * Copies the icons out of material-icon-theme into public/, plus a trimmed
 * lookup table. Only icons actually reachable from the theme's name and
 * extension maps are copied, for files and folders alike — the pack ships far
 * more than any one project references.
 *
 *   bun run sync:icons
 */
import { mkdirSync, copyFileSync, writeFileSync, rmSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const pack = join(root, 'node_modules/material-icon-theme/dist')
const outIcons = join(root, 'public/file-icons')
const outMap = join(root, 'src/lib/file-icon-map.json')

const manifest = JSON.parse(readFileSync(join(pack, 'material-icons.json'), 'utf8'))
const { fileNames, fileExtensions, folderNames, folderNamesExpanded, iconDefinitions } = manifest
const fallback = manifest.file
const folderFallback = manifest.folder
const folderFallbackOpen = manifest.folderExpanded

const used = new Set([
  fallback,
  folderFallback,
  folderFallbackOpen,
  ...Object.values(fileNames),
  ...Object.values(fileExtensions),
  ...Object.values(folderNames),
  ...Object.values(folderNamesExpanded),
])

rmSync(outIcons, { recursive: true, force: true })
mkdirSync(outIcons, { recursive: true })

let copied = 0
const missing = []
for (const id of used) {
  const definition = iconDefinitions[id]
  if (!definition) {
    missing.push(id)
    continue
  }
  // iconPath is "./../icons/<name>.svg", relative to the manifest.
  const from = join(pack, definition.iconPath)
  try {
    copyFileSync(from, join(outIcons, `${id}.svg`))
    copied += 1
  } catch {
    missing.push(id)
  }
}

// The expanded folder icon is always the closed one with `-open` appended, so
// shipping that second map would be ~150KB of derivable data in the renderer
// bundle. Verified here rather than assumed: if a theme update ever breaks the
// rule, this fails the sync instead of quietly rendering the wrong icon.
const derivable = Object.keys(folderNames).every(
  (name) => folderNamesExpanded[name] === `${folderNames[name]}-open`
)
if (!derivable || folderFallbackOpen !== `${folderFallback}-open`) {
  throw new Error(
    'material-icon-theme no longer derives expanded folder icons as "<icon>-open"; ' +
      'ship folderNamesExpanded in the map and read it in file-icon.ts'
  )
}

writeFileSync(
  outMap,
  `${JSON.stringify(
    { fallback, folderFallback, fileNames, fileExtensions, folderNames },
    null,
    2
  )}\n`
)

console.log(`copied ${copied} icons -> public/file-icons`)
console.log(
  `map: ${Object.keys(fileNames).length} file names, ${Object.keys(fileExtensions).length} extensions, ` +
    `${Object.keys(folderNames).length} folder names`
)
if (missing.length) console.warn(`missing definitions: ${missing.join(', ')}`)
