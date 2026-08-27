/**
 * The domain types live in `src/shared/contract.ts`, mirroring
 * `src-tauri/src/contract.rs`, so both sides agree on one definition. This
 * re-export keeps the `@/types` import path the components already use.
 */
export type {
  ChangeStatus,
  Commit,
  FileChange,
  FullFileContents,
  RepoDiff,
} from '@shared/contract'
