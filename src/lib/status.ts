import type { BadgeProps } from '@/components/ui/badge'
import type { ChangeStatus } from '@/types'

export const STATUS_LABEL: Record<ChangeStatus, string> = {
  added: 'A',
  modified: 'M',
  deleted: 'D',
  renamed: 'R',
  untracked: 'U',
}

const INTENTS: Record<ChangeStatus, NonNullable<BadgeProps['intent']>> = {
  added: 'success',
  untracked: 'success',
  modified: 'warning',
  deleted: 'danger',
  renamed: 'info',
}

export function statusIntent(status: ChangeStatus) {
  return INTENTS[status]
}

export function splitPath(path: string) {
  const slash = path.lastIndexOf('/')
  return {
    dir: slash === -1 ? '' : path.slice(0, slash + 1),
    name: slash === -1 ? path : path.slice(slash + 1),
  }
}

/** Href for a single file's diff, matching the `/file/$` splat route. */
export function fileHref(path: string) {
  return `/file/${path}`
}
