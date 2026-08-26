/**
 * Copies the file-type icons out of material-icon-theme into public/, plus a
 * trimmed lookup table. Only icons reachable from fileNames/fileExtensions are
 * copied — the folder icons are ~3/4 of the pack and nothing here uses them.
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
const { fileNames, fileExtensions, iconDefinitions } = manifest
const fallback = manifest.file

const used = new Set([fallback, ...Object.values(fileNames), ...Object.values(fileExtensions)])

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

writeFileSync(
  outMap,
  `${JSON.stringify({ fallback, fileNames, fileExtensions }, null, 2)}\n`
)

console.log(`copied ${copied} icons -> public/file-icons`)
console.log(`map: ${Object.keys(fileNames).length} names, ${Object.keys(fileExtensions).length} extensions`)
if (missing.length) console.warn(`missing definitions: ${missing.join(', ')}`)
