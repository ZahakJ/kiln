import { describe, expect, it } from 'vitest';
import type { HandleStore } from './handles.ts';
import { AppHost, FILE_PERMISSION } from './index.ts';

/** A picked file's handle as IndexedDB gives it back after a reload. */
function handle(name: string, permission: PermissionState, bytes: Uint8Array = new Uint8Array([1, 2, 3])) {
  let data = bytes;
  return {
    name,
    getFile: async () =>
      new File([data as Uint8Array<ArrayBuffer>], name, { lastModified: 1_792_000_000_000 }),
    createWritable: async () => ({
      write: async (b: Uint8Array) => {
        data = b;
      },
      close: async () => undefined,
    }),
    queryPermission: async () => permission,
    requestPermission: async () => permission,
  };
}

/** The kept handles of one path. */
function kept(path: string, h: ReturnType<typeof handle>): HandleStore {
  return {
    find: async (p: string) => (p === path ? h : null),
    get: async (p: string) => (p === path && (await h.queryPermission()) === 'granted' ? h : null),
    granted: async (p: string) => (p === path && (await h.queryPermission()) === 'granted' ? h : null),
    put: async () => undefined,
    remove: async () => undefined,
    same: async () => null,
    requestWrite: async () => false,
  } as unknown as HandleStore;
}

describe('the web host', () => {
  it('answers the stamp of a picked file, and a missing one as gone', async () => {
    const path = 'handle:abc/Plan.docx';
    const host = new AppHost({ store: null, handles: kept(path, handle('Plan.docx', 'granted')) });
    expect(await host.handle({ StatFile: { path } })).toEqual({
      Stamp: { size: 3, modified_ms: 1_792_000_000_000 },
    });
    expect(await host.handle({ StatFile: { path: 'somewhere/else.docx' } })).toMatchObject({
      Failed: { kind: 'Unsupported' },
    });
  });

  it('fails a write the browser has not allowed, and says so, instead of writing elsewhere', async () => {
    const path = 'handle:abc/Plan.docx';
    const host = new AppHost({ store: null, handles: kept(path, handle('Plan.docx', 'prompt')) });
    const asked: string[] = [];
    const listen = (e: Event) => asked.push((e as CustomEvent<{ path: string }>).detail.path);
    globalThis.addEventListener(FILE_PERMISSION, listen);
    const reply = await host.handle({ WriteFile: { path, bytes: [9] } });
    globalThis.removeEventListener(FILE_PERMISSION, listen);
    expect(reply).toEqual({
      Failed: { kind: 'Unavailable', message: 'Kiln needs your permission to save to Plan.docx' },
    });
    expect(asked).toEqual([path]);
  });
});
