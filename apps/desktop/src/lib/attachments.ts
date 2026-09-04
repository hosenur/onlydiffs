import { MAX_IMAGE_BYTES } from '@shared/contract'

/**
 * The rules for getting a pasted image out of a clipboard and into a message.
 *
 * The bytes themselves never reach the message. The backend writes the image
 * down on the machine the repository is on and hands back a path there, and
 * that path is what the session is told about — so everything here is about
 * which files to take from a paste, and how to name them once they have landed.
 */

/**
 * The parts of a `DataTransfer` this reads, and no more. Named rather than
 * taken as the DOM type so the rule can be exercised without a document; a real
 * `DataTransfer` satisfies it.
 */
export interface PasteSource {
  files: ArrayLike<File>
  items?: ArrayLike<{ kind: string; type: string; getAsFile: () => File | null }>
}

function isImage(file: File): boolean {
  return file.type.startsWith('image/')
}

/**
 * The images in a paste, or nothing — which is the common case, and the one
 * that has to leave the ordinary text paste alone.
 */
export function pastedImages(source: PasteSource | null | undefined): File[] {
  if (!source) return []

  const files = Array.from(source.files ?? []).filter(isImage)
  if (files.length > 0) return files

  // `files` is the modern spelling and covers a screenshot, an image copied
  // from a page, and a file copied in Finder. `items` is the fallback for the
  // engines that fill one and not the other, and it is worth keeping: an image
  // that silently fails to paste is indistinguishable from a broken feature.
  return Array.from(source.items ?? [])
    .filter((item) => item.kind === 'file' && item.type.startsWith('image/'))
    .map((item) => item.getAsFile())
    .filter((file): file is File => file !== null && isImage(file))
}

function megabytes(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

/**
 * Why a file cannot be attached, or `null` when it can.
 *
 * The backend decides this too, and it is the one that matters — it reads the
 * bytes rather than trusting what the clipboard called them. This is only here
 * to stop a 200 MB paste being copied across the process boundary to be told
 * no; the wording matches so the same limit is not described two ways.
 */
export function rejection(file: { size: number }): string | null {
  if (file.size === 0) return 'The pasted image is empty.'
  if (file.size > MAX_IMAGE_BYTES) {
    return `The image is ${megabytes(file.size)} and the limit is ${megabytes(MAX_IMAGE_BYTES)}.`
  }
  return null
}

/**
 * What actually gets sent: the line the user clicked, what they typed about it,
 * and where any images they pasted are now sitting.
 *
 * The paths go on their own lines below the question rather than inline, so the
 * sentence reads as a sentence and the session can open the files without
 * having to pick them out of it.
 */
export function composeMessage(
  label: string,
  text: string,
  imagePaths: readonly string[]
): string {
  const said = text.trim()
  const opening = said ? `${label} ${said}` : label
  if (imagePaths.length === 0) return opening
  return [opening, '', ...imagePaths.map((path) => `Pasted image: ${path}`)].join('\n')
}
