// Boot payload injected by Rust before the page loads (window.__NL_BOOT__),
// so the first paint needs no IPC round trip.

import type { AuthStatus_Serialize, LibrarySnapshot_Serialize, SettingValue } from './bindings';

export type Snapshot = LibrarySnapshot_Serialize;

export interface BootData {
  snapshot: Snapshot;
  settings: Record<string, SettingValue>;
  /** Account state before any network call (restoring / signed out). */
  auth: AuthStatus_Serialize;
  imgBase: string;
  bench: boolean;
  benchScroll: boolean;
  /** Dev tooling (`--dev-route`): open a view/popover for screenshots. */
  devRoute: string | null;
  bootError: string | null;
}

declare global {
  interface Window {
    __NL_BOOT__?: BootData;
  }
}

const EMPTY: BootData = {
  snapshot: { games: [], quickLaunch: [], recent: [], activeDownload: null, account: null },
  settings: {},
  auth: { state: 'signedOut', reason: null },
  imgBase: 'http://img.localhost',
  bench: false,
  benchScroll: false,
  devRoute: null,
  bootError: null,
};

/** Read once at startup. Outside Tauri (plain `vite dev` in a browser) this is empty. */
export const boot: BootData = window.__NL_BOOT__ ?? EMPTY;

/** specta types f64 as `number | null` (NaN serialises to null). */
export function num(v: number | null | undefined): number {
  return typeof v === 'number' && Number.isFinite(v) ? v : 0;
}
