import { describe, expect, it, vi } from 'vitest';
import { handleCommon, isExternalUrl, OPFS_PREFIX, WebHost } from './host';

function memoryStore() {
  const map = new Map<string, string>();
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => {
      map.set(k, v);
    },
    map,
  };
}

describe('common host operations', () => {
  it('stores and loads preferences', async () => {
    const prefs = memoryStore();
    const env = { prefs, clipboard: undefined, openUrl: vi.fn(async () => {}) };
    expect(await handleCommon({ LoadPref: { key: 'ui.theme' } }, env)).toBe('Empty');
    expect(await handleCommon({ StorePref: { key: 'ui.theme', value: '"dark"' } }, env)).toBe('Done');
    expect(prefs.map.get('kiln.pref.ui.theme')).toBe('"dark"');
    expect(await handleCommon({ LoadPref: { key: 'ui.theme' } }, env)).toEqual({ Text: '"dark"' });
    expect(await handleCommon('Recents', env)).toEqual({ Recents: [] });
    expect(await handleCommon({ ReadFile: { path: '/a' } }, env)).toBeUndefined();
  });

  it('opens only web and mail links', async () => {
    const openUrl = vi.fn(async () => {});
    const env = { prefs: undefined, clipboard: undefined, openUrl };
    expect(await handleCommon({ OpenExternal: { url: 'https://zahakj.github.io/kiln/' } }, env)).toBe('Done');
    expect(openUrl).toHaveBeenCalledWith('https://zahakj.github.io/kiln/');
    const refused = await handleCommon({ OpenExternal: { url: 'file:///etc/passwd' } }, env);
    expect(refused).toEqual({
      Failed: { kind: 'InvalidInput', message: 'refusing to open file:///etc/passwd' },
    });
    expect(isExternalUrl('mailto:a@b.c')).toBe(true);
    expect(isExternalUrl('javascript:alert(1)')).toBe(false);
    expect(isExternalUrl('not a url')).toBe(false);
  });

  it('reports a missing clipboard and uses one that exists', async () => {
    const none = { prefs: undefined, clipboard: undefined, openUrl: async () => {} };
    expect(await handleCommon('ClipboardRead', none)).toMatchObject({ Failed: { kind: 'Unsupported' } });
    const clipboard = {
      readText: async () => 'copied',
      writeText: vi.fn(async () => {}),
    } as unknown as Clipboard;
    const env = { prefs: undefined, clipboard, openUrl: async () => {} };
    expect(await handleCommon('ClipboardRead', env)).toEqual({ Clipboard: { text: 'copied', html: null } });
    expect(await handleCommon({ ClipboardWrite: { text: 'x', html: null } }, env)).toBe('Done');
  });
});

describe('WebHost', () => {
  it('saves to the private file system without a save picker and reads it back', async () => {
    const files = new Map<string, Uint8Array>();
    const fileHandle = (name: string) => ({
      getFile: async () => {
        const data = files.get(name);
        if (!data) throw new DOMException('missing', 'NotFoundError');
        return new File([data as Uint8Array<ArrayBuffer>], name);
      },
      createWritable: async () => ({
        write: async (d: Uint8Array) => {
          files.set(name, d);
        },
        close: async () => {},
      }),
    });
    const dir = {
      getDirectoryHandle: async () => dir,
      getFileHandle: async (name: string) => fileHandle(name),
    };
    const win = { navigator: { storage: { getDirectory: async () => dir } } } as unknown as Window;
    const host = new WebHost(win);
    const pick = await host.handle({ PickSave: { suggested_name: 'Scratch1.txt', filters: [] } });
    expect(pick).toEqual({ Path: `${OPFS_PREFIX}documents/Scratch1.txt` });
    const path = (pick as { Path: string }).Path;
    expect(await host.handle({ WriteFile: { path, bytes: [104, 105] } })).toBe('Done');
    expect(await host.handle({ ReadFile: { path } })).toEqual({ Bytes: [104, 105] });
    expect(await host.handle({ ReadFile: { path: `${OPFS_PREFIX}documents/none.txt` } })).toMatchObject({
      Failed: { kind: 'NotFound' },
    });
    expect(await host.handle({ ReadFile: { path: '/etc/hosts' } })).toMatchObject({
      Failed: { kind: 'NotFound' },
    });
  });

  it('opens through the File System Access API and maps cancellation', async () => {
    const handle = { name: 'a.txt', getFile: async () => new File(['hey'], 'a.txt') };
    const showOpenFilePicker = vi
      .fn()
      .mockResolvedValueOnce([handle])
      .mockRejectedValueOnce(new DOMException('cancelled', 'AbortError'));
    const host = new WebHost({ showOpenFilePicker } as unknown as Window);
    const op = { PickOpen: { filters: [{ name: 'Text', extensions: ['txt'] }] } };
    const picked = await host.handle(op);
    expect(picked).toEqual({ Path: 'handle:1/a.txt' });
    expect(await host.handle({ ReadFile: { path: 'handle:1/a.txt' } })).toEqual({ Bytes: [104, 101, 121] });
    expect(await host.handle(op)).toBe('Cancelled');
    expect(showOpenFilePicker.mock.calls[0]?.[0]).toMatchObject({
      types: [{ description: 'Text', accept: { 'application/octet-stream': ['.txt'] } }],
    });
  });

  it('works without a window', async () => {
    const host = new WebHost(undefined);
    expect(await host.handle({ LoadPref: { key: 'x' } })).toBe('Empty');
    expect(await host.handle({ PickOpen: { filters: [] } })).toMatchObject({
      Failed: { kind: 'Unsupported' },
    });
  });
});
