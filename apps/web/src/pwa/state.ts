/**
 * What the installable web app shows about itself: an update waiting, the network, the
 * browser's install offer, and documents the page could not open from a launch. One small store
 * that `register.ts` and `launch.ts` write and `PwaLayer` reads (`useSyncExternalStore`).
 */

import type { UnsavedDocument } from '../client/unsaved.ts';

/** The browser's `beforeinstallprompt` event (Chromium), kept to show its prompt on a click. */
export interface InstallPrompt extends Event {
  prompt(): Promise<void>;
  userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>;
}

export interface PwaState {
  /** Unsaved documents still to answer for before an update's reload (the save-changes question). */
  asking: readonly UnsavedDocument[];
  /** The window itself says "Kiln has been updated" (a file of its old version is gone). */
  stale: boolean;
  /** A new version is installed and waits for the page to reload into it. */
  update: boolean;
  /** The browser is offline. */
  offline: boolean;
  /** The network came back a moment ago (the notice fades). */
  backOnline: boolean;
  /** The browser offers to install Kiln (and the user has not said no). */
  install: InstallPrompt | null;
  /** Names of launched or shared files that no web core opens. */
  unopened: string[];
  /** Every app now opens offline: shown once, the first time it becomes true. */
  offlineReady: boolean;
}

let state: PwaState = {
  asking: [],
  stale: false,
  update: false,
  offline: typeof navigator !== 'undefined' && navigator.onLine === false,
  backOnline: false,
  install: null,
  unopened: [],
  offlineReady: false,
};
const listeners = new Set<() => void>();

export function pwaState(): PwaState {
  return state;
}

export function setPwaState(change: Partial<PwaState>): void {
  state = { ...state, ...change };
  for (const listener of listeners) listener();
}

export function subscribePwa(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Per-browser choices (the install offer declined), kept when the browser allows it. */
export function remembered(key: string): string | null {
  try {
    return globalThis.localStorage?.getItem(`kiln.pwa.${key}`) ?? null;
  } catch {
    return null;
  }
}

export function remember(key: string, value: string): void {
  try {
    globalThis.localStorage?.setItem(`kiln.pwa.${key}`, value);
  } catch {
    // The choice holds for this visit only.
  }
}
