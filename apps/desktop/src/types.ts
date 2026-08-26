/**
 * The domain types live in `electron/shared/contract.ts` so the main process,
 * the preload bridge, and the renderer all agree on one definition. This
 * re-export keeps the `@/types` import path the components already use.
 */
export type {
  ChangeStatus,
  Commit,
  FileChange,
  FullFileContents,
  RepoDiff,
} from '@shared/contract'
