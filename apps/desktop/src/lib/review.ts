import { indexChanges } from './file-tree'
import type { FileChange } from '@/types'

/**
 * Reviewing, in this app, is staging: you read what changed in a file and add
 * it to the index to say you are done with it. That gesture already existed —
 * this is the one place that reads it back out, so the tree and the toolbar
 * cannot end up disagreeing about what "done" means.
 */

/**
 * A path counts as reviewed when every row it has is staged.
 *
 * Being staged is not enough on its own. A path staged and then edited again
 * has two rows in the diff, and the second one is a change nobody has read yet;
 * calling that file reviewed would file it away with something still on it.
 */
export function isReviewed(changes: readonly FileChange[]): boolean {
  return changes.length > 0 && changes.every((change) => change.staged)
}

export interface ReviewProgress {
  reviewed: number
  /** Distinct paths, not diff rows — a path staged and edited again is one
   *  file to review, not two. */
  total: number
}

export function reviewProgress(files: readonly FileChange[]): ReviewProgress {
  let reviewed = 0
  const byPath = indexChanges(files)
  for (const changes of byPath.values()) {
    if (isReviewed(changes)) reviewed += 1
  }
  return { reviewed, total: byPath.size }
}
