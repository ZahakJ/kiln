import { defaultStorage, readJson, type StorageLike, writeJson } from './storage.ts';

export interface RecentCommandsOptions {
  storage?: StorageLike;
  /** Storage key; one list per app is typical (`kiln.recentCommands.write`). */
  key?: string;
  /** Max entries kept. Default 8 (the palette shows them as "Recent"). */
  limit?: number;
}

/**
 * Most-recently-used command ids, newest first, for the palette's "Recent" section. Stored only on
 * this device; never transmitted (Kiln has no telemetry).
 */
export class RecentCommands {
  readonly #storage: StorageLike;
  readonly #key: string;
  readonly #limit: number;
  readonly #listeners = new Set<() => void>();
  #ids: string[];

  constructor(options: RecentCommandsOptions = {}) {
    this.#storage = options.storage ?? defaultStorage();
    this.#key = options.key ?? 'kiln.recentCommands';
    this.#limit = Math.max(1, options.limit ?? 8);
    const stored = readJson<unknown>(this.#storage, this.#key, []);
    this.#ids = Array.isArray(stored)
      ? stored.filter((x): x is string => typeof x === 'string').slice(0, this.#limit)
      : [];
  }

  /** Newest first. */
  list(): readonly string[] {
    return this.#ids;
  }

  record(id: string): void {
    // Opening the palette itself is not interesting as a "recent" entry.
    if (id === 'view.commandPalette') return;
    this.#ids = [id, ...this.#ids.filter((x) => x !== id)].slice(0, this.#limit);
    writeJson(this.#storage, this.#key, this.#ids);
    for (const l of this.#listeners) l();
  }

  remove(id: string): void {
    if (!this.#ids.includes(id)) return;
    this.#ids = this.#ids.filter((x) => x !== id);
    writeJson(this.#storage, this.#key, this.#ids);
    for (const l of this.#listeners) l();
  }

  clear(): void {
    this.#ids = [];
    try {
      this.#storage.removeItem(this.#key);
    } catch {
      // ignore
    }
    for (const l of this.#listeners) l();
  }

  subscribe(listener: () => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }
}
