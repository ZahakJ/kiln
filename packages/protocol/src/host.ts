/**
 * Hosts answer the core's requests for the outside world (files, dialogs, clipboard, preferences)
 * on the main thread. The core asks with `Event.HostRequest`; clients pass the operation to a
 * {@link HostHandler} and send its reply back.
 */

import type { ErrorKind } from './generated/ErrorKind';
import type { FileFilter } from './generated/FileFilter';
import type { HostOp } from './generated/HostOp';
import type { HostReply } from './generated/HostReply';

/** Performs host operations for the core. */
export interface HostHandler {
  handle(op: HostOp): Promise<HostReply>;
}

/** A `Failed` reply. */
export function failed(kind: ErrorKind, message: string): HostReply {
  return { Failed: { kind, message } };
}

const PREF_PREFIX = 'kiln.pref.';

/** Minimal storage interface (window.localStorage in browsers and webviews). */
export interface PrefStore {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

/** URL schemes `OpenExternal` may open. */
const EXTERNAL_SCHEMES = new Set(['http:', 'https:', 'mailto:']);

/** Whether `url` is an absolute http(s) or mailto URL. */
export function isExternalUrl(url: string): boolean {
  try {
    return EXTERNAL_SCHEMES.has(new URL(url).protocol);
  } catch {
    return false;
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * The operations every webview can serve the same way: preferences (`localStorage`), clipboard
 * (`navigator.clipboard`) and recents (none yet: WP-83 keeps them in `kiln-storage`). Returns
 * `undefined` for operations the concrete host must handle.
 */
export async function handleCommon(
  op: HostOp,
  env: {
    prefs: PrefStore | undefined;
    clipboard: Clipboard | undefined;
    openUrl: (url: string) => Promise<void>;
  },
): Promise<HostReply | undefined> {
  if (op === 'Recents') return { Recents: [] };
  if (op === 'ClipboardRead') {
    if (!env.clipboard) return failed('Unsupported', 'the clipboard is not available');
    try {
      return { Clipboard: { text: await env.clipboard.readText(), html: null } };
    } catch (error) {
      return failed('Io', `clipboard: ${errorMessage(error)}`);
    }
  }
  if (typeof op !== 'object') return undefined;
  if ('StorePref' in op) {
    if (!env.prefs) return failed('Unsupported', 'preferences are not available');
    try {
      env.prefs.setItem(PREF_PREFIX + op.StorePref.key, op.StorePref.value);
      return 'Done';
    } catch (error) {
      return failed('Io', `preferences: ${errorMessage(error)}`);
    }
  }
  if ('LoadPref' in op) {
    const value = env.prefs?.getItem(PREF_PREFIX + op.LoadPref.key) ?? null;
    return value === null ? 'Empty' : { Text: value };
  }
  if ('ClipboardWrite' in op) {
    if (!env.clipboard) return failed('Unsupported', 'the clipboard is not available');
    const { text, html } = op.ClipboardWrite;
    try {
      if (html !== null && typeof ClipboardItem !== 'undefined') {
        await env.clipboard.write([
          new ClipboardItem({
            'text/plain': new Blob([text], { type: 'text/plain' }),
            'text/html': new Blob([html], { type: 'text/html' }),
          }),
        ]);
      } else {
        await env.clipboard.writeText(text);
      }
      return 'Done';
    } catch (error) {
      return failed('Io', `clipboard: ${errorMessage(error)}`);
    }
  }
  if ('OpenExternal' in op) {
    const { url } = op.OpenExternal;
    if (!isExternalUrl(url)) return failed('InvalidInput', `refusing to open ${url}`);
    try {
      await env.openUrl(url);
      return 'Done';
    } catch (error) {
      return failed('Io', `open ${url}: ${errorMessage(error)}`);
    }
  }
  return undefined;
}

/** `accept` map of the File System Access API from Kiln's filters. */
function pickerTypes(filters: FileFilter[]): { description: string; accept: Record<string, string[]> }[] {
  return filters.map((f) => ({
    description: f.name,
    accept: { 'application/octet-stream': f.extensions.map((e) => `.${e}`) },
  }));
}

/** Minimal File System Access API typings (Chromium; not in the DOM lib yet). */
interface WritableFileStream {
  write(data: Uint8Array): Promise<void>;
  close(): Promise<void>;
}
interface FileHandleLike {
  readonly name: string;
  getFile(): Promise<File>;
  createWritable?(): Promise<WritableFileStream>;
}
interface PickerWindow {
  showOpenFilePicker?(options: object): Promise<FileHandleLike[]>;
  showSaveFilePicker?(options: object): Promise<FileHandleLike>;
}

/** The scheme of paths that live in the Origin Private File System. */
export const OPFS_PREFIX = 'opfs:';
const HANDLE_PREFIX = 'handle:';

/**
 * The browser host (web/PWA): files through the File System Access API where the browser has it
 * (Chromium), otherwise an `<input type=file>` for opening and the Origin Private File System for
 * saving (`opfs:documents/<name>`); picked files are referred to by opaque `handle:<n>/<name>`
 * paths. Recents, printing and richer storage arrive with WP-83.
 */
export class WebHost implements HostHandler {
  private readonly handles = new Map<string, FileHandleLike | File>();
  private next = 1;

  constructor(
    private readonly win: (Window & PickerWindow) | undefined = typeof window === 'undefined'
      ? undefined
      : (window as Window & PickerWindow),
  ) {}

  private remember(item: FileHandleLike | File): string {
    const path = `${HANDLE_PREFIX}${this.next++}/${item.name}`;
    this.handles.set(path, item);
    return path;
  }

  async handle(op: HostOp): Promise<HostReply> {
    const common = await handleCommon(op, {
      prefs: this.safeLocalStorage(),
      clipboard: this.win?.navigator?.clipboard,
      openUrl: async (url) => {
        this.win?.open(url, '_blank', 'noopener,noreferrer');
      },
    });
    if (common !== undefined) return common;
    if (typeof op !== 'object') return failed('Unsupported', `${String(op)} is not available on the web`);
    try {
      if ('PickOpen' in op) return await this.pickOpen(op.PickOpen.filters);
      if ('PickSave' in op) return await this.pickSave(op.PickSave.suggested_name, op.PickSave.filters);
      if ('ReadFile' in op) return { Bytes: Array.from(await this.read(op.ReadFile.path)) };
      if ('WriteFile' in op) {
        await this.write(op.WriteFile.path, Uint8Array.from(op.WriteFile.bytes));
        return 'Done';
      }
    } catch (error) {
      if (error instanceof DOMException && error.name === 'AbortError') return 'Cancelled';
      const kind: ErrorKind =
        error instanceof DOMException && error.name === 'NotFoundError' ? 'NotFound' : 'Io';
      return failed(kind, errorMessage(error));
    }
    return failed('Unsupported', 'unknown host operation');
  }

  private safeLocalStorage(): PrefStore | undefined {
    try {
      return this.win?.localStorage;
    } catch {
      return undefined; // storage disabled (private mode, sandboxed frame)
    }
  }

  private async pickOpen(filters: FileFilter[]): Promise<HostReply> {
    if (this.win?.showOpenFilePicker) {
      const [handle] = await this.win.showOpenFilePicker({ types: pickerTypes(filters), multiple: false });
      return handle ? { Path: this.remember(handle) } : 'Cancelled';
    }
    const doc = this.win?.document;
    if (!doc) return failed('Unsupported', 'no document to show a file picker in');
    const input = doc.createElement('input');
    input.type = 'file';
    input.accept = filters.flatMap((f) => f.extensions.map((e) => `.${e}`)).join(',');
    const file = await new Promise<File | null>((resolve) => {
      input.addEventListener('change', () => resolve(input.files?.[0] ?? null), { once: true });
      input.addEventListener('cancel', () => resolve(null), { once: true });
      input.click();
    });
    return file ? { Path: this.remember(file) } : 'Cancelled';
  }

  private async pickSave(suggestedName: string, filters: FileFilter[]): Promise<HostReply> {
    if (this.win?.showSaveFilePicker) {
      const handle = await this.win.showSaveFilePicker({ suggestedName, types: pickerTypes(filters) });
      return { Path: this.remember(handle) };
    }
    return { Path: `${OPFS_PREFIX}documents/${suggestedName}` };
  }

  private async opfsFile(path: string, create: boolean): Promise<FileSystemFileHandle> {
    const storage = this.win?.navigator?.storage;
    if (!storage?.getDirectory) throw new Error('the Origin Private File System is not available');
    const parts = path
      .slice(OPFS_PREFIX.length)
      .split('/')
      .filter((p) => p !== '' && p !== '.' && p !== '..');
    const name = parts.pop();
    if (!name) throw new Error(`invalid path ${path}`);
    let dir = await storage.getDirectory();
    for (const part of parts) dir = await dir.getDirectoryHandle(part, { create });
    return dir.getFileHandle(name, { create });
  }

  private async read(path: string): Promise<Uint8Array> {
    const item = this.handles.get(path);
    if (item) {
      const file = item instanceof File ? item : await item.getFile();
      return new Uint8Array(await file.arrayBuffer());
    }
    if (path.startsWith(OPFS_PREFIX)) {
      const file = await (await this.opfsFile(path, false)).getFile();
      return new Uint8Array(await file.arrayBuffer());
    }
    throw new DOMException(`${path} is not a file this page can read`, 'NotFoundError');
  }

  private async write(path: string, bytes: Uint8Array): Promise<void> {
    const item = this.handles.get(path);
    if (item && !(item instanceof File) && item.createWritable) {
      const stream = await item.createWritable();
      await stream.write(bytes);
      await stream.close();
      return;
    }
    const target = path.startsWith(OPFS_PREFIX)
      ? path
      : `${OPFS_PREFIX}documents/${path.split('/').pop() ?? 'document'}`;
    const handle = await this.opfsFile(target, true);
    const stream = await handle.createWritable();
    await stream.write(bytes as Uint8Array<ArrayBuffer>);
    await stream.close();
  }
}
