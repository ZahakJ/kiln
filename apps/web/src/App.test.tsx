import type { HostOp, HostReply } from '@kiln/protocol';
import { describe, expect, it } from 'vitest';
import { appHash, parseRoute } from './App.tsx';
import { DEFAULT_PREFS, loadPrefs, setPrefsHost, storePref } from './routes/app-frame/prefs.ts';

describe('routes', () => {
  it('parses Home, the app frames and documents', () => {
    expect(parseRoute('')).toEqual({ kind: 'home' });
    expect(parseRoute('#/')).toEqual({ kind: 'home' });
    expect(parseRoute('#/home')).toEqual({ kind: 'home' });
    expect(parseRoute('#/write')).toEqual({
      kind: 'app',
      app: 'write',
      doc: null,
      nonce: null,
      share: null,
      template: null,
    });
    expect(parseRoute('#/grid?doc=3')).toEqual({
      kind: 'app',
      app: 'grid',
      doc: 3,
      nonce: null,
      share: null,
      template: null,
    });
    expect(parseRoute('#/deck?doc=-1&new=5')).toEqual({
      kind: 'app',
      app: 'deck',
      doc: null,
      nonce: '5',
      share: null,
      template: null,
    });
    expect(parseRoute('#/nope')).toEqual({ kind: 'unknown', hash: '#/nope' });
    // a new document from a template.
    expect(parseRoute('#/grid?template=grid%2Fbudget.kgrid&new=2')).toMatchObject({
      kind: 'app',
      app: 'grid',
      doc: null,
      template: 'grid/budget.kgrid',
    });
  });

  it('routes only the apps of the release: Kiln Notes has no frame in v0.1', () => {
    expect(parseRoute('#/notes')).toEqual({ kind: 'unknown', hash: '#/notes' });
  });

  it('parses a shared document link', () => {
    expect(parseRoute('#/write?doc=k7p2q9xw&server=wss%3A%2F%2Fcollab.example.org&token=t')).toEqual({
      kind: 'app',
      app: 'write',
      doc: null,
      nonce: null,
      share: { room: 'k7p2q9xw', server: 'wss://collab.example.org', token: 't' },
      template: null,
    });
    // A number is an open document, and a room needs its server.
    expect(parseRoute('#/write?doc=5&server=ws%3A%2F%2Fx')).toMatchObject({ doc: 5, share: null });
    expect(parseRoute('#/deck?doc=abc')).toMatchObject({ share: null });
  });

  it('builds app hashes', () => {
    expect(appHash('hub')).toBe('#/home');
    expect(appHash('notes')).toBe('#/notes');
    expect(appHash('write', 7)).toBe('#/write?doc=7');
  });
});

describe('UI preferences go through the host', () => {
  it('stores and loads through StorePref / LoadPref, ignoring damaged values', async () => {
    const store = new Map<string, string>();
    const ops: HostOp[] = [];
    setPrefsHost({
      async handle(op: HostOp): Promise<HostReply> {
        ops.push(op);
        if (typeof op === 'object' && 'StorePref' in op) {
          store.set(op.StorePref.key, op.StorePref.value);
          return 'Done';
        }
        if (typeof op === 'object' && 'LoadPref' in op) {
          const v = store.get(op.LoadPref.key);
          return v === undefined ? 'Empty' : { Text: v };
        }
        return 'Empty';
      },
    });
    expect(await loadPrefs()).toEqual(DEFAULT_PREFS);
    await storePref('theme', 'dark');
    await storePref('ribbonLayout', 'simplified');
    await storePref('ribbonCollapsed', true);
    expect(ops).toContainEqual({ StorePref: { key: 'ui.theme', value: 'dark' } });
    expect(await loadPrefs()).toEqual({
      ...DEFAULT_PREFS,
      theme: 'dark',
      ribbonLayout: 'simplified',
      ribbonCollapsed: true,
    });
    store.set('ui.theme', 'neon');
    expect((await loadPrefs()).theme).toBe('system');
  });
});
