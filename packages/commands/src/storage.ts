/** The subset of the Web Storage API the command system persists to (user keymap, recent commands). */
export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/** In-memory storage for tests, workers and private windows. */
export class MemoryStorage implements StorageLike {
  readonly #map = new Map<string, string>();
  getItem(key: string): string | null {
    return this.#map.get(key) ?? null;
  }
  setItem(key: string, value: string): void {
    this.#map.set(key, String(value));
  }
  removeItem(key: string): void {
    this.#map.delete(key);
  }
}

/**
 * `localStorage` when it is usable (it throws in some sandboxed iframes and private modes),
 * otherwise an in-memory fallback so callers never have to care.
 */
export function defaultStorage(): StorageLike {
  try {
    const ls = (globalThis as { localStorage?: StorageLike }).localStorage;
    if (ls) {
      const probe = '__kiln_probe__';
      ls.setItem(probe, '1');
      ls.removeItem(probe);
      return ls;
    }
  } catch {
    // fall through
  }
  return new MemoryStorage();
}

/** Reads JSON, returning `fallback` on a missing key, a parse error or a storage error. */
export function readJson<T>(storage: StorageLike, key: string, fallback: T): T {
  try {
    const raw = storage.getItem(key);
    return raw == null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

export function writeJson(storage: StorageLike, key: string, value: unknown): void {
  try {
    storage.setItem(key, JSON.stringify(value));
  } catch {
    // Quota or access errors: preferences are best-effort.
  }
}
