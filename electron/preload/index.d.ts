import type { CashewApi } from "../shared/contract";

declare global {
  interface Window {
    readonly cashew: CashewApi;
  }
}

export {};
