import type { HostOp, HostReply } from '@kiln/protocol';
import { describe, expect, it } from 'vitest';
import { appHash, parseRoute } from './App.tsx';
import { DEFAULT_PREFS, loadPrefs, setPrefsHost, storePref } from './routes/app-frame/prefs.ts';

describe('routes', () => {
  it('parses Home, the app frames and documents', () => {
    expect(parseRoute('')).toEqual({ kind: 'home' });
    expect(parseRoute('#/')).toEqual({ kind: 'home' });
    expect(parseRoute('#/home')).toEqual({ kind: 'home' });
    expect(parseRoute('#/write')).toEqual({ kind: 'app', app: 'write', doc: null, nonce: null });
    expect(parseRoute('#/grid?doc=3')).toEqual({ kind: 'app', app: 'grid', doc: 3, nonce: null });
    expect(parseRoute('#/deck?doc=-1&new=5')).toEqual({ kind: 'app', app: 'deck', doc: null, nonce: '5' });
    expect(parseRoute('#/nope')).toEqual({ kind: 'unknown', hash: '#/nope' });
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
