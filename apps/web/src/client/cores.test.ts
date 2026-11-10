import type { Command, DocKind, Event, EventListener, HostOp, HostReply, KilnClient } from '@kiln/protocol';
import { KilnError, Listeners } from '@kiln/protocol';
import { describe, expect, it, vi } from 'vitest';
import { type CoreApp, CoreRouter, coreOfDoc, coreOfHint, formatOfPath, OPEN_FILTERS } from './cores';

const TAG: Record<CoreApp, number> = { write: 1, grid: 2, deck: 3 };

/** A core that numbers documents like the per-app bridge and records what it was asked. */
function fakeCore(app: CoreApp) {
  const listeners = new Listeners();
  let next = TAG[app];
  const calls: string[] = [];
  const summary = (kind: DocKind) => {
    const doc = next;
    next += 4;
    return { doc, kind, title: `${app}${doc}` } as never;
  };
  const core: KilnClient & { calls: string[]; emit(e: Event): void } = {
    bridge: 'wasm',
    calls,
    emit: (e) => listeners.emit(e),
    ping: async () => ({ version: app, protocol: 1, target: 'wasm32' }) as never,
    execute: async (doc, command) => {
      calls.push(`execute ${doc} ${JSON.stringify(command)}`);
      return { outcome: 'Applied', summary: null, value: 'None' };
    },
    query: async (doc) => {
      calls.push(`query ${doc}`);
      return 'Ok' as never;
    },
    open: async (_bytes, hint) => {
      calls.push(`open ${hint.name}`);
      return summary(app === 'write' ? 'Write' : app === 'grid' ? 'Grid' : 'Deck');
    },
    new: async (kind) => {
      calls.push(`new ${kind}`);
      return summary(kind);
    },
    setViewport: async (doc) => void calls.push(`viewport ${doc}`),
    frame: async (doc) => {
      calls.push(`frame ${doc}`);
      return {} as never;
    },
    a11y: async (doc) => {
      calls.push(`a11y ${doc}`);
      return {} as never;
    },
    subscribe: (l: EventListener) => listeners.add(l),
    hostReply: async (id) => void calls.push(`reply ${id}`),
    dispose: () => calls.push('dispose'),
  };
  return core;
}

function setup(pick: HostReply = { Path: 'handle:1/Budget.xlsx' }) {
  const cores = new Map<CoreApp, ReturnType<typeof fakeCore>>();
  const start = vi.fn(async (app: CoreApp) => {
    const c = fakeCore(app);
    cores.set(app, c);
    return c;
  });
  const asked: HostOp[] = [];
  const host = {
    handle: async (op: HostOp) => {
      asked.push(op);
      return pick;
    },
  };
  return { router: new CoreRouter(start, host), cores, start, asked };
}

describe('the web core router', () => {
  it('maps ids, kinds, formats and paths to cores', () => {
    expect([1, 2, 3, 4, 5, 10, 11].map(coreOfDoc)).toEqual([
      'write',
      'grid',
      'deck',
      null,
      'write',
      'grid',
      'deck',
    ]);
    expect(formatOfPath('opfs:documents/Q3 Report.DOCX')).toBe('Docx');
    expect(formatOfPath('page.htm')).toBe('Html');
    expect(formatOfPath('README')).toBeNull();
    expect(coreOfHint({ name: 'a.csv', path: null, format: null, read_only: false })).toBe('grid');
    expect(coreOfHint({ name: 'a.bin', path: null, format: 'Potx', read_only: false })).toBe('deck');
    expect(
      coreOfHint({ name: 'notes.txt', path: null, format: null, read_only: false, kind: 'Scratch' }),
    ).toBe('write');
    expect(coreOfHint({ name: 'a.ods', path: null, format: null, read_only: false })).toBeNull();
    expect(OPEN_FILTERS[0]?.extensions).toEqual([
      'txt',
      'md',
      'docx',
      'dotx',
      'docm',
      'dotm',
      'xlsx',
      'xltx',
      'xlsm',
      'csv',
      'pptx',
      'potx',
      'pptm',
      'ppsx',
      'kwrite',
      'kgrid',
      'kdeck',
    ]);
    // Kiln's own files open in their app's core.
    expect(coreOfHint({ name: 'Plan.kgrid', path: null, format: null, read_only: false })).toBe('grid');
  });

  it('reads the recent files without a core and opens a recovered document in its app', async () => {
    const files = [
      {
        path: 'opfs:documents/Plan.kwrite',
        name: 'Plan.kwrite',
        kind: 'Write',
        opened_at_ms: 1,
        pinned: false,
        thumbnail: null,
      },
    ];
    const { router, start, asked, cores } = setup({ Recents: files } as HostReply);
    expect(await router.query(null, 'Recents')).toEqual({ Recents: files });
    expect(asked).toEqual(['Recents']);
    expect(start).not.toHaveBeenCalled();
    await router.execute(null, { Suite: { RecoveryOpen: { key: 'k1', kind: 'Grid' } } });
    expect(router.started()).toEqual(['grid']);
    expect(
      cores.get('grid')?.calls.some((c) => c.startsWith('execute null') && c.includes('RecoveryOpen')),
    ).toBe(true);
  });

  it('starts only the cores that are needed, once each', async () => {
    const { router, start } = setup();
    expect(router.started()).toEqual([]);
    const w = await router.new('Write');
    const g = await router.new('Grid');
    await router.new('Write');
    expect([w.doc, g.doc]).toEqual([1, 2]);
    expect(start.mock.calls.map(([app]) => app)).toEqual(['write', 'grid']);
  });

  it('sends each document call to the core that holds the document', async () => {
    const { router, cores } = setup();
    await router.new('Deck');
    await router.new('Grid');
    await router.frame(3);
    await router.setViewport(2, {} as never);
    await router.execute(2, { Grid: 'Recalculate' } as unknown as Command);
    await router.query(3, 'Status');
    await router.a11y(2);
    expect(cores.get('deck')?.calls).toEqual(['new Deck', 'frame 3', 'query 3']);
    expect(cores.get('grid')?.calls).toEqual([
      'new Grid',
      'viewport 2',
      'execute 2 {"Grid":"Recalculate"}',
      'a11y 2',
    ]);
    await expect(router.frame(4)).rejects.toMatchObject({ kind: 'NotFound' });
  });

  it('opens bytes in the core of their format and refuses formats no web core reads', async () => {
    const { router, cores } = setup();
    const s = await router.open(new Uint8Array(), {
      name: 'Deck.pptx',
      path: null,
      format: null,
      read_only: false,
    });
    expect(s.doc).toBe(3);
    expect(cores.get('deck')?.calls).toEqual(['open Deck.pptx']);
    await expect(
      router.open(new Uint8Array(), { name: 'x.odt', path: null, format: null, read_only: false }),
    ).rejects.toMatchObject({ kind: 'Unsupported' });
    await expect(router.new('Notes')).rejects.toMatchObject({ kind: 'Unsupported' });
  });

  it('shows File ▸ Open itself and lets the right core open the chosen file', async () => {
    const { router, cores, asked } = setup();
    router.prefer('write');
    await router.execute(1, { Suite: { Open: { path: null } } });
    expect(asked).toEqual([{ PickOpen: { filters: OPEN_FILTERS } }]);
    expect(cores.get('grid')?.calls).toEqual([
      'execute null {"Suite":{"Open":{"path":"handle:1/Budget.xlsx"}}}',
    ]);
    expect(cores.has('write')).toBe(false);
    // A recent file goes straight to its core.
    await router.execute(null, { Suite: { Open: { path: 'opfs:documents/Talk.pptx' } } });
    expect(cores.get('deck')?.calls).toEqual([
      'execute null {"Suite":{"Open":{"path":"opfs:documents/Talk.pptx"}}}',
    ]);
    await expect(router.execute(null, { Suite: { Open: { path: 'a.pdf' } } })).rejects.toMatchObject({
      kind: 'Unsupported',
    });
  });

  it('treats a cancelled Open dialog as nothing done', async () => {
    const { router, start } = setup('Cancelled');
    const r = await router.execute(null, { Suite: { Open: { path: null } } });
    expect(r.outcome).toBe('Unchanged');
    expect(start).not.toHaveBeenCalled();
  });

  it('sends session calls to the preferred core and app commands to their app', async () => {
    const { router, cores } = setup();
    router.prefer('grid');
    expect((await router.ping()).version).toBe('grid');
    await router.execute(null, { Suite: { New: { kind: 'Deck' } } });
    expect(cores.get('deck')?.calls).toEqual(['execute null {"Suite":{"New":{"kind":"Deck"}}}']);
    await router.execute(null, { Write: 'FollowLink' } as unknown as Command);
    expect(cores.get('write')?.calls).toHaveLength(1);
  });

  it('merges the events of every core and replies to host requests on the core that asked', async () => {
    const { router, cores } = setup();
    const seen: Event[] = [];
    router.subscribe((e) => seen.push(e));
    await router.new('Write');
    await router.new('Grid');
    const opened = { Closed: { doc: 2 } } as Event;
    cores.get('grid')?.emit(opened);
    cores.get('grid')?.emit({ HostRequest: { id: 7, op: 'ClipboardRead' } } as Event);
    expect(seen).toEqual([opened, { HostRequest: { id: 7, op: 'ClipboardRead' } }]);
    await router.hostReply(7, 'Done');
    expect(cores.get('grid')?.calls.at(-1)).toBe('reply 7');
    expect(cores.get('write')?.calls.at(-1)).toBe('new Write');
  });

  it('retries a core that failed to start and disposes the started ones', async () => {
    let fail = true;
    const made: ReturnType<typeof fakeCore>[] = [];
    const router = new CoreRouter(
      async (app) => {
        if (fail) throw new KilnError({ kind: 'Unsupported', message: 'not built' });
        const c = fakeCore(app);
        made.push(c);
        return c;
      },
      { handle: async () => 'Cancelled' },
    );
    await expect(router.new('Grid')).rejects.toMatchObject({ kind: 'Unsupported' });
    fail = false;
    await expect(router.new('Grid')).resolves.toMatchObject({ doc: 2 });
    router.dispose();
    await Promise.resolve();
    await Promise.resolve();
    expect(made[0]?.calls.at(-1)).toBe('dispose');
    await expect(router.new('Grid')).rejects.toMatchObject({ kind: 'Closed' });
  });
});
