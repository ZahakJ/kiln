/**
 * Files picked with the File System Access API, kept across reloads: each picked file's
 * `FileSystemFileHandle` is stored in IndexedDB under its `handle:<id>/<name>` path, so a recent
 * file opens again and AutoSave writes the same file after the page reloads. A file picked again
 * keeps the path it had (`same`), so its version history and AutoSave's lock follow it. The
 * browser asks the user again for permission to a handle the first time it is used after a
 * reload; it only asks during a click, so a write that needs it (AutoSave runs from a timer)
 * fails until the user allows it (`requestWrite`, from the click of the notice's button).
 */
import type { FileHandleLike } from '@kiln/protocol';

const DB = 'kiln-host';
const STORE = 'handles';

/** A handle with the permission calls of Chromium. */
interface PermissionHandle extends FileHandleLike {
  queryPermission?(options: { mode: 'read' | 'readwrite' }): Promise<PermissionState>;
  requestPermission?(options: { mode: 'read' | 'readwrite' }): Promise<PermissionState>;
  isSameEntry?(other: FileHandleLike): Promise<boolean>;
}

function request<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error);
  });
}

export class HandleStore {
  private db: Promise<IDBDatabase> | null = null;

  constructor(private readonly idb: IDBFactory | undefined = globalThis.indexedDB) {}

  private open(): Promise<IDBDatabase> {
    if (!this.idb) return Promise.reject(new Error('IndexedDB is not available'));
    this.db ??= new Promise((resolve, reject) => {
      const r = this.idb?.open(DB, 1);
      if (!r) return reject(new Error('IndexedDB is not available'));
      r.onupgradeneeded = () => {
        if (!r.result.objectStoreNames.contains(STORE)) r.result.createObjectStore(STORE);
      };
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => reject(r.error);
    });
    this.db.catch(() => {
      this.db = null;
    });
    return this.db;
  }

  /** Keeps a handle or a picked file (best effort: a browser that cannot store it keeps it for
   * this page only). */
  async put(path: string, handle: FileHandleLike | File): Promise<void> {
    try {
      const db = await this.open();
      await request(db.transaction(STORE, 'readwrite').objectStore(STORE).put(handle, path));
    } catch {
      // The handle still works until the page closes.
    }
  }

  /** The handle (or file) kept for a path, without asking for anything. */
  async find(path: string): Promise<PermissionHandle | File | null> {
    try {
      const db = await this.open();
      const kept = (await request(db.transaction(STORE).objectStore(STORE).get(path))) as
        | PermissionHandle
        | File
        | undefined;
      return kept ?? null;
    } catch {
      return null;
    }
  }

  /** The handle (or file) of a path, with permission to `mode` (asked for when needed). */
  async get(path: string, mode: 'read' | 'readwrite'): Promise<FileHandleLike | File | null> {
    const handle = await this.find(path);
    if (!handle) return null;
    if (handle instanceof File) return mode === 'read' ? handle : null;
    const state = (await handle.queryPermission?.({ mode })) ?? 'granted';
    if (state === 'granted') return handle;
    const asked = await handle.requestPermission?.({ mode }).catch(() => 'denied' as PermissionState);
    return asked === 'granted' ? handle : null;
  }

  /** The handle of a path if the page may already use it for `mode` (nothing is asked). */
  async granted(path: string, mode: 'read' | 'readwrite'): Promise<FileHandleLike | File | null> {
    const handle = await this.find(path);
    if (!handle || handle instanceof File) return handle;
    const state = await handle.queryPermission?.({ mode }).catch(() => 'denied' as PermissionState);
    return (state ?? 'granted') === 'granted' ? handle : null;
  }

  /** Asks the user for permission to write a kept file (call it from a click). */
  async requestWrite(path: string): Promise<boolean> {
    const handle = await this.find(path);
    if (!handle || handle instanceof File) return false;
    const state =
      (await handle.requestPermission?.({ mode: 'readwrite' }).catch(() => 'denied' as PermissionState)) ??
      'granted';
    return state === 'granted';
  }

  /** The path a handle was kept under before (the same file picked again), if any. */
  async same(handle: FileHandleLike, except: string): Promise<string | null> {
    const h = handle as PermissionHandle;
    if (!h.isSameEntry) return null;
    let entries: [IDBValidKey, unknown][] = [];
    try {
      const db = await this.open();
      const store = db.transaction(STORE).objectStore(STORE);
      const [keys, values] = await Promise.all([request(store.getAllKeys()), request(store.getAll())]);
      entries = keys.map((k, i) => [k, values[i]]);
    } catch {
      return null;
    }
    for (const [key, value] of entries) {
      if (typeof key !== 'string' || key === except || value instanceof File || !value) continue;
      if (await h.isSameEntry(value as FileHandleLike).catch(() => false)) return key;
    }
    return null;
  }

  /** Forgets a path. */
  async remove(path: string): Promise<void> {
    try {
      const db = await this.open();
      await request(db.transaction(STORE, 'readwrite').objectStore(STORE).delete(path));
    } catch {
      // Nothing kept.
    }
  }
}
