import { useCallback, useEffect, useRef, useState } from 'react'
import { rejection } from '@/lib/attachments'
import { attachImage } from '@/lib/ipc'

/**
 * The images pasted into the composer, and everything that has to happen to one
 * on its way to a Claude session.
 *
 * A pasted image is written down as soon as it is pasted rather than when the
 * message is sent, so the wait — a copy over SSH, for a project on a host —
 * happens while the user is still typing instead of after they press Enter.
 */

/** One pasted image. */
export interface Attachment {
  id: number
  name: string
  /** Object URL behind the thumbnail. Each one pins the whole image in memory
   *  until it is revoked, which is why every path out of here revokes it. */
  preview: string
  /** Where the backend put it, once it has. `null` while that is in flight. */
  path: string | null
  error: string | null
}

export interface Attachments {
  items: readonly Attachment[]
  /**
   * Whether what is here can be sent as it stands.
   *
   * False while an image is still being written down — it has no path to name
   * yet — and false while one that could not be written is still in the row,
   * because sending would drop it from the message without saying so.
   */
  isReady: boolean
  /** Why an image could not be attached, or `null`. */
  error: string | null
  /** Where the images that made it are sitting, in the order they were
   *  pasted — which is the only part of any of this that gets sent. */
  paths: string[]
  add: (files: readonly File[]) => Promise<void>
  remove: (id: number) => void
  clear: () => void
}

export function useAttachments(): Attachments {
  const [items, setItems] = useState<Attachment[]>([])
  const nextId = useRef(0)
  // The list as the callbacks see it, so revoking an object URL never has to
  // happen inside a state updater — React is free to run one of those twice,
  // and a released URL is not something to release twice.
  const live = useRef<readonly Attachment[]>([])

  useEffect(() => {
    live.current = items
  }, [items])

  // The window outlives every composer that has been open in it. Without this,
  // one pasted image stays in memory for the life of the process.
  useEffect(
    () => () => {
      for (const item of live.current) URL.revokeObjectURL(item.preview)
    },
    []
  )

  /**
   * One image at a time on purpose: two five-megabyte screenshots pasted
   * together would otherwise both be in flight over the same SSH connection,
   * and the chips would settle in whichever order the host finished them.
   */
  const add = useCallback(async (files: readonly File[]) => {
    for (const file of files) {
      const id = (nextId.current += 1)
      const tooBig = rejection(file)
      setItems((previous) => [
        ...previous,
        {
          id,
          name: file.name || 'Pasted image',
          preview: URL.createObjectURL(file),
          path: null,
          error: tooBig,
        },
      ])
      if (tooBig) continue

      const settle = (change: Partial<Attachment>) =>
        setItems((previous) =>
          previous.map((item) => (item.id === id ? { ...item, ...change } : item))
        )
      try {
        settle({ path: await attachImage(await file.arrayBuffer()) })
      } catch (cause) {
        settle({ error: cause instanceof Error ? cause.message : String(cause) })
      }
    }
  }, [])

  const remove = useCallback((id: number) => {
    const gone = live.current.find((item) => item.id === id)
    if (gone) URL.revokeObjectURL(gone.preview)
    setItems((previous) => previous.filter((item) => item.id !== id))
  }, [])

  const clear = useCallback(() => {
    for (const item of live.current) URL.revokeObjectURL(item.preview)
    setItems([])
  }, [])

  const refused = items.find((item) => item.error !== null)
  const writing = items.some((item) => item.path === null && item.error === null)

  return {
    items,
    isReady: !writing && refused === undefined,
    error: refused?.error ?? null,
    paths: items.flatMap((item) => (item.path === null ? [] : [item.path])),
    add,
    remove,
    clear,
  }
}
