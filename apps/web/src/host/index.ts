/**
 * The web page's host for the cores: the browser host of `@kiln/protocol` plus the app
 * storage in the Origin Private File System (`store.ts`: autosave journals, versions of local
 * files, recent files, the user dictionary), picked files kept across reloads in IndexedDB
 * (`handles.ts`), and the recent files for Kiln Home (`HostOp::Recents`). The web cannot watch
 * files for changes by other programs, so the core's watch requests are answered as unsupported;
 * it answers the stamp of a file (`StatFile`: its size and `lastModified`), which the core
 * compares before it writes the file again, so another program's or another tab's change is never
 * written over. A write to a picked file the browser has not allowed (after a reload) fails, and
 * the page hears of it (`FILE_PERMISSION`), rather than going anywhere else.
 */
import {
  type FileHandleLike,
  failed,
  type HostOp,
  type HostReply,
  OPFS_PREFIX,
  WebHost,
} from '@kiln/protocol';
import { HandleStore } from './handles.ts';
import { answerStore, browserStoreEnv, OpfsStore, type StoreEnv } from './store.ts';

export { answerStore, browserStoreEnv, OpfsStore, type StoreEnv, TAIL_TROUBLE } from './store.ts';

const HANDLE_PREFIX = 'handle:';

/** Dispatched on `window` when a write needs the user's permission (`detail.path`). */
export const FILE_PERMISSION = 'kiln:file-permission';

/** The handles kept across reloads (one IndexedDB, shared by every host of the page). */
const keptHandles = new HandleStore();

/**
 * Asks the user for permission to write a picked file again (call it from the click that asks,
 * as the browser requires). `true` when it was given.
 */
export function requestFilePermission(path: string): Promise<boolean> {
  return keptHandles.requestWrite(path);
}

/** A random id for a picked file's path (unique across reloads). */
function newId(): string {
  const c = globalThis.crypto;
  if (c?.randomUUID) return c.randomUUID().slice(0, 13);
  return Math.random().toString(36).slice(2, 15);
}

export class AppHost extends WebHost {
  private readonly store: OpfsStore | null;
  private readonly kept: HandleStore;

  constructor(options: { store?: StoreEnv | null; handles?: HandleStore } = {}) {
    super();
    const env = options.store === undefined ? browserStoreEnv() : options.store;
    this.store = env ? new OpfsStore(env) : null;
    this.kept = options.handles ?? keptHandles;
  }

  /** Journal writes in flight that are not in the tail (`store.ts`). */
  unprotectedWrites(): number {
    return this.store?.unprotectedWrites() ?? 0;
  }

  /** Every app storage write given so far has finished. */
  async written(): Promise<void> {
    await this.store?.written();
  }

  override async handle(op: HostOp): Promise<HostReply> {
    if (op === 'Recents') {
      try {
        return { Recents: this.store ? await this.store.recents() : [] };
      } catch {
        return { Recents: [] };
      }
    }
    if (typeof op === 'object') {
      const stored = await answerStore(this.store, op);
      if (stored !== undefined) return stored;
      if ('WatchFile' in op) return failed('Unsupported', 'the web cannot watch files');
      if ('UnwatchFile' in op) return 'Done';
      if ('StatFile' in op) return this.stat(op.StatFile.path);
      // A file picked before the page reloaded.
      if ('ReadFile' in op) await this.restore(op.ReadFile.path, 'read');
      if ('WriteFile' in op) {
        const { path } = op.WriteFile;
        await this.restore(path, 'readwrite');
        if (path.startsWith(HANDLE_PREFIX) && !this.handles.has(path)) return this.permissionNeeded(path);
      }
      if ('PickOpen' in op || 'PickSave' in op) return this.picked(await super.handle(op));
    }
    return super.handle(op);
  }

  /** The stamp of a file: picked (with permission to read it already), or in the OPFS. */
  private async stat(path: string): Promise<HostReply> {
    try {
      let file: File | null = null;
      if (path.startsWith(HANDLE_PREFIX)) {
        const item = this.handles.get(path) ?? (await this.kept.granted(path, 'read'));
        if (!item) return failed('Unavailable', 'the browser has not allowed Kiln to read this file yet');
        file = item instanceof File ? item : await item.getFile();
      } else if (path.startsWith(OPFS_PREFIX)) {
        file = await (await this.opfsFile(path, false)).getFile();
      } else {
        return failed('Unsupported', 'not a file this page can reach');
      }
      return { Stamp: { size: file.size, modified_ms: file.lastModified } };
    } catch (error) {
      if (error instanceof DOMException && error.name === 'NotFoundError') return 'Empty';
      return failed('Io', error instanceof Error ? error.message : String(error));
    }
  }

  /** A write to a picked file the browser has not allowed: it fails, and the page offers to ask. */
  private permissionNeeded(path: string): HostReply {
    const name = path.slice(path.lastIndexOf('/') + 1);
    globalThis.dispatchEvent?.(new CustomEvent(FILE_PERMISSION, { detail: { path } }));
    return failed('Unavailable', `Kiln needs your permission to save to ${name}`);
  }

  /** A file picked again keeps the path it was kept under before. */
  private async picked(reply: HostReply): Promise<HostReply> {
    if (typeof reply !== 'object' || !('Path' in reply)) return reply;
    const fresh = reply.Path;
    const item = this.handles.get(fresh);
    if (!item || item instanceof File) return reply;
    for (const [path, other] of this.handles) {
      if (path === fresh || other instanceof File) continue;
      const same = (item as FileHandleLike & { isSameEntry?(o: FileHandleLike): Promise<boolean> })
        .isSameEntry;
      if (same && (await same.call(item, other).catch(() => false))) return this.adopt(fresh, path, item);
    }
    const kept = await this.kept.same(item, fresh);
    return kept ? this.adopt(fresh, kept, item) : reply;
  }

  private adopt(fresh: string, path: string, item: FileHandleLike): HostReply {
    this.handles.delete(fresh);
    this.handles.set(path, item);
    void this.kept.remove(fresh).then(() => this.kept.put(path, item));
    return { Path: path };
  }

  /**
   * a file the browser handed the page (an installed app's file handler, a share): kept
   * as a picked file is, so a core opens it by the returned path and Save writes back to a handle.
   */
  adoptLaunched(item: FileHandleLike | File): string {
    return this.remember(item);
  }

  /** A picked file's path that stays valid across reloads, and the handle kept with it. */
  protected override remember(item: FileHandleLike | File): string {
    const path = `${HANDLE_PREFIX}${newId()}/${item.name}`;
    this.handles.set(path, item);
    // A file from the file input is kept as it was picked; a handle reads the file as it is.
    void this.kept.put(path, item);
    return path;
  }

  private async restore(path: string, mode: 'read' | 'readwrite'): Promise<void> {
    if (!path.startsWith(HANDLE_PREFIX) || this.handles.has(path)) return;
    const handle = await this.kept.get(path, mode);
    if (handle) this.handles.set(path, handle);
  }
}
