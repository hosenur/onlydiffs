import { describe, expect, test } from 'bun:test'
import { composeMessage, pastedImages, rejection, type PasteSource } from './attachments'
import { MAX_IMAGE_BYTES } from '@shared/contract'

function file(name: string, type: string, bytes = 4): File {
  return new File([new Uint8Array(bytes)], name, { type })
}

/** A paste that filled `files`, which is what every current engine does. */
function paste(...files: File[]): PasteSource {
  return { files, items: [] }
}

/** A paste that filled only `items`, which is the case the fallback is for. */
function itemsOnly(...files: File[]): PasteSource {
  return {
    files: [],
    items: files.map((value) => ({
      kind: 'file',
      type: value.type,
      getAsFile: () => value,
    })),
  }
}

describe('pastedImages', () => {
  test('takes the images out of a paste', () => {
    const png = file('shot.png', 'image/png')

    expect(pastedImages(paste(png))).toEqual([png])
  })

  test('leaves a text paste alone', () => {
    // Nothing to attach means the field pastes text as it always has.
    expect(pastedImages(paste())).toEqual([])
    expect(pastedImages(null)).toEqual([])
  })

  test('ignores a file that is not an image', () => {
    expect(pastedImages(paste(file('notes.txt', 'text/plain')))).toEqual([])
  })

  test('takes every image when several are pasted at once', () => {
    const first = file('a.png', 'image/png')
    const second = file('b.jpg', 'image/jpeg')

    expect(pastedImages(paste(first, second))).toEqual([first, second])
  })

  test('falls back to items when the engine filled those instead', () => {
    const png = file('shot.png', 'image/png')

    expect(pastedImages(itemsOnly(png))).toEqual([png])
  })

  test('ignores a non-file item, which is how a plain text paste arrives', () => {
    const source: PasteSource = {
      files: [],
      items: [{ kind: 'string', type: 'text/plain', getAsFile: () => null }],
    }

    expect(pastedImages(source)).toEqual([])
  })
})

describe('rejection', () => {
  test('an image within the limit is accepted', () => {
    expect(rejection({ size: 1024 })).toBeNull()
  })

  test('an image past the limit is refused before it is copied anywhere', () => {
    expect(rejection({ size: MAX_IMAGE_BYTES + 1 })).toContain('limit')
  })

  test('an image exactly at the limit is allowed', () => {
    expect(rejection({ size: MAX_IMAGE_BYTES })).toBeNull()
  })

  test('an empty file is refused', () => {
    expect(rejection({ size: 0 })).not.toBeNull()
  })
})

describe('composeMessage', () => {
  test('a question about a line reads exactly as it did before images existed', () => {
    expect(composeMessage('src/app.tsx:42', 'why is this hidden?', [])).toBe(
      'src/app.tsx:42 why is this hidden?'
    )
  })

  test('an image is named on its own line, below the question', () => {
    expect(composeMessage('src/app.tsx:42', 'like this', ['/repo/.git/onlydiffs/pastes/1.png']))
      .toBe('src/app.tsx:42 like this\n\nPasted image: /repo/.git/onlydiffs/pastes/1.png')
  })

  test('an image with nothing typed still carries the line it is about', () => {
    // Worth sending: the screenshot is the question.
    expect(composeMessage('src/app.tsx:42', '   ', ['/repo/.git/onlydiffs/pastes/1.png'])).toBe(
      'src/app.tsx:42\n\nPasted image: /repo/.git/onlydiffs/pastes/1.png'
    )
  })

  test('several images each get a line', () => {
    const message = composeMessage('a.tsx:1', 'before and after', ['/tmp/a.png', '/tmp/b.png'])

    expect(message.split('\n').filter((line) => line.startsWith('Pasted image:'))).toHaveLength(2)
  })
})
