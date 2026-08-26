import type { OnlyDiffsApi } from "../shared/contract";

declare global {
  interface Window {
    readonly onlydiffs: OnlyDiffsApi;
  }
}

export {};
