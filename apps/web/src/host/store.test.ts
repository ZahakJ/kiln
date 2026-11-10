import type { HostOp } from '@kiln/protocol';
import { describe, expect, it } from 'vitest';
import { answerStore, OpfsStore } from './store.ts';

/** An in-memory Origin Private File System: directories of files of bytes. */
class FakeDir {
  readonly dirs = new Map<string, FakeDir>();
  readonly files = new Map<string, Uint8Array>();
  /** The files in the order their writes were committed (a closed stream). */
  static committed: string[] = [];
  /** How long a write of a file takes, by name (a slow disk). */
  static delay = new Map<string, number>();
  /** Writes of these files never finish: the page goes first. */
  static stuck = new Set<string>();

  async getDirectoryHandle(name: string, opts?: { create?: boolean }): Promise<FakeDir> {
    let d = this.dirs.get(name);
    if (!d) {
      if (!opts?.create) throw new DOMException(name, 'NotFoundError');
      d = new FakeDir();
      this.dirs.set(name, d);
    }
    return d;
  }

  async getFileHandle(name: string, opts?: { create?: boolean }) {
    if (!this.files.has(name)) {
      if (!opts?.create) throw new DOMException(name, 'NotFoundError');
      this.files.set(name, new Uint8Array());
    }
    const files = this.files;
    return {
      getFile: async () => new Blob([files.get(name) as Uint8Array<ArrayBuffer>]),
      createWritable: async (o?: { keepExistingData?: boolean }) => {
        let data = o?.keepExistingData ? (files.get(name) ?? new Uint8Array()) : new Uint8Array();
        return {
          write: async (w: Uint8Array | { position: number; data: Uint8Array }) => {
            const [at, bytes] = 'data' in w ? [w.position, w.data] : [data.length, w];
            const next = new Uint8Array(Math.max(data.length, at + bytes.length));
            next.set(data);
            next.set(bytes, at);
            data = next;
          },
          // The browser commits the file when the stream closes.
          close: async () => {
            if (FakeDir.stuck.has(decodeURIComponent(name))) await new Promise(() => undefined);
            await new Promise((r) => setTimeout(r, FakeDir.delay.get(name) ?? 0));
            files.set(name, data);
            FakeDir.committed.push(decodeURIComponent(name));
          },
          abort: async () => undefined,
        };
      },
    };
  }

  async removeEntry(name: string) {
    if (!this.files.delete(name)) throw new DOMException(name, 'NotFoundError');
  }

  async *keys() {
    yield* this.files.keys();
  }
}

/** Web Locks, with the locks other tabs hold in `held` too. */
function fakeLocks() {
  const held = new Set<string>();
  return {
    held,
    request: async (
      name: string,
      options: { ifAvailable: boolean },
      cb: (lock: unknown) => Promise<void> | undefined,
    ) => {
      if (held.has(name) && options.ifAvailable) {
        await cb(null);
        return;
      }
      held.add(name);
      await cb({ name });
      held.delete(name);
    },
    query: async () => ({ held: [...held].map((name) => ({ name })) }),
  };
}

function store(locks = fakeLocks()) {
  const root = new FakeDir();
  return {
    root,
    locks,
    s: new OpfsStore({ root: async () => root as unknown as FileSystemDirectoryHandle, locks }),
  };
}

const bytes = (...b: number[]) => Uint8Array.from(b);

/** `localStorage`, as far as the store uses it. */
class FakeTail {
  readonly items = new Map<string, string>();
  /** Writes over this many characters fail, as a full `localStorage` does. */
  room = Number.POSITIVE_INFINITY;
  get length() {
    return this.items.size;
  }
  key(i: number) {
    return [...this.items.keys()][i] ?? null;
  }
  getItem(k: string) {
    return this.items.get(k) ?? null;
  }
  setItem(k: string, v: string) {
    if (v.length > this.room) throw new DOMException('full', 'QuotaExceededError');
    this.items.set(k, v);
  }
  removeItem(k: string) {
    this.items.delete(k);
  }
}

/** A page with a tail, over `root` and `tail`; the next page of the origin gets its own locks. */
function page(root = new FakeDir(), tail = new FakeTail()) {
  const s = new OpfsStore({
    root: async () => root as unknown as FileSystemDirectoryHandle,
    locks: fakeLocks(),
    tail,
  });
  return { s, root, tail };
}

const journalValue = async (s: OpfsStore, key: string) => Array.from((await s.get('Journal', key)) ?? []);

describe('the app storage of the web', () => {
  it('puts, appends, reads, lists and deletes values by area and key', async () => {
    const { s } = store();
    expect(await s.get('Recents', 'r/1')).toBeNull();
    await s.put('Recents', 'r/1', bytes(1, 2));
    await s.put('Recents', 'r/1', bytes(3), true);
    await s.put('Recents', 'r/1', bytes(4, 5), true);
    expect(Array.from((await s.get('Recents', 'r/1')) ?? [])).toEqual([1, 2, 3, 4, 5]);
    expect(await s.get('Prefs', 'r/1')).toBeNull();
    await s.put('Recents', 'q', bytes(9));
    expect(await s.list('Recents', '')).toEqual(['q', 'r/1']);
    expect(await s.list('Recents', 'r/')).toEqual(['r/1']);
    await s.delete('Recents', 'r/1');
    await s.delete('Recents', 'missing');
    expect(await s.list('Recents', '')).toEqual(['q']);
  });

  it('writes one value in order, even when the requests overlap', async () => {
    const { s } = store();
    await Promise.all(Array.from({ length: 20 }, (_, i) => s.put('Journal', 'k.log', bytes(i), true)));
    expect(Array.from((await s.get('Journal', 'k.log')) ?? [])).toEqual(
      Array.from({ length: 20 }, (_, i) => i),
    );
  });

  it("commits a journal's writes in the order the core gave them, whatever their keys", async () => {
    const { s } = store();
    FakeDir.committed = [];
    // The checkpoint is large and slow to write; the marker naming it is quick.
    FakeDir.delay = new Map([[encodeURIComponent('j.base.2'), 30]]);
    const writes = [
      s.put('Journal', 'j.base.2', bytes(1)),
      s.put('Journal', 'j.log.2', bytes()),
      s.put('Journal', 'j.meta', bytes(2)),
      s.delete('Journal', 'j.base.1'),
      s.put('Journal', 'j.log.2', bytes(3), true),
      s.put('Recents', 'r', bytes(4)),
    ];
    await Promise.all(writes);
    FakeDir.delay = new Map();
    expect(FakeDir.committed.filter((k) => k.startsWith('j.'))).toEqual([
      'j.base.2',
      'j.log.2',
      'j.meta',
      'j.log.2',
    ]);
    // Other values do not wait for the journal.
    expect(FakeDir.committed.indexOf('r')).toBeLessThan(FakeDir.committed.indexOf('j.base.2'));
  });

  it('refuses to write, open or delete a journal another tab holds', async () => {
    const { s, root, locks } = store();
    const dir = await (await root.getDirectoryHandle('kiln-store', { create: true })).getDirectoryHandle(
      'Journal',
      { create: true },
    );
    dir.files.set('theirs.meta', bytes(3));
    dir.files.set(encodeURIComponent('theirs.base.1'), bytes(4));
    locks.held.add('kiln-journal:theirs');
    for (const op of [
      { StoreGet: { area: 'Journal', key: 'theirs.base.1' } },
      { StoreDelete: { area: 'Journal', key: 'theirs.meta' } },
      { StoreAppend: { area: 'Journal', key: 'theirs.log.1', bytes: [1] } },
    ] as Exclude<HostOp, string>[]) {
      expect(await answerStore(s, op)).toEqual({
        Failed: { kind: 'Unavailable', message: 'the document is open in another window' },
      });
    }
    expect(dir.files.has('theirs.meta')).toBe(true);
    // Once that tab is gone, this one may open it.
    locks.held.delete('kiln-journal:theirs');
    expect(await answerStore(s, { StoreGet: { area: 'Journal', key: 'theirs.base.1' } })).toEqual({
      Bytes: [4],
    });
    expect(locks.held.has('kiln-journal:theirs')).toBe(true);
  });

  it('leaves out the journals open in another tab, and lets go of a journal when it is removed', async () => {
    const { s, root, locks } = store();
    await s.put('Journal', 'mine.meta', bytes(1));
    await s.put('Journal', 'mine.base.1', bytes(2));
    // Another tab's journal, which that tab holds.
    const dir = await (await root.getDirectoryHandle('kiln-store')).getDirectoryHandle('Journal');
    dir.files.set('theirs.meta', bytes(3));
    locks.held.add('kiln-journal:theirs');
    expect(await s.list('Journal', '')).toEqual([]);
    // That tab closes: its journal is listed.
    locks.held.delete('kiln-journal:theirs');
    expect(await s.list('Journal', '')).toEqual(['theirs.meta']);
    // Removing this page's journal (the marker first) lets go of it.
    await s.delete('Journal', 'mine.meta');
    await new Promise((r) => setTimeout(r, 0));
    expect(await s.list('Journal', '')).toEqual(['mine.base.1', 'theirs.meta']);
  });

  describe('the write-ahead tail', () => {
    const ignore = (p: Promise<unknown>) => void p.catch(() => undefined);

    it('finishes after a reload what the page that went had only started', async () => {
      const first = page();
      await first.s.put('Journal', 'j.base.1', bytes(1));
      await first.s.put('Journal', 'j.log.1', bytes());
      await first.s.put('Journal', 'j.meta', bytes(2));
      expect(first.tail.items.size).toBe(0);
      // The edit reaches the page; the page goes before the browser commits the file.
      FakeDir.stuck = new Set(['j.log.1']);
      ignore(first.s.put('Journal', 'j.log.1', bytes(7, 8), true));
      ignore(first.s.put('Journal', 'j.log.1', bytes(9), true));
      expect(first.tail.items.size).toBe(2);
      FakeDir.stuck = new Set();

      const next = page(first.root, first.tail);
      expect(await next.s.list('Journal', '')).toEqual(['j.base.1', 'j.log.1', 'j.meta']);
      expect(await journalValue(next.s, 'j.log.1')).toEqual([7, 8, 9]);
      expect(next.tail.items.size).toBe(0);
    });

    it('goes on through a checkpoint the page did not finish', async () => {
      const first = page();
      await first.s.put('Journal', 'j.base.1', bytes(1));
      await first.s.put('Journal', 'j.log.1', bytes());
      await first.s.put('Journal', 'j.meta', bytes(1));
      FakeDir.stuck = new Set(['j.base.2']);
      ignore(first.s.put('Journal', 'j.base.2', bytes(2)));
      ignore(first.s.put('Journal', 'j.log.2', bytes()));
      ignore(first.s.put('Journal', 'j.meta', bytes(2)));
      ignore(first.s.delete('Journal', 'j.base.1'));
      ignore(first.s.delete('Journal', 'j.log.1'));
      ignore(first.s.put('Journal', 'j.log.2', bytes(5), true));
      FakeDir.stuck = new Set();

      const next = page(first.root, first.tail);
      expect(await next.s.list('Journal', '')).toEqual(['j.base.2', 'j.log.2', 'j.meta']);
      expect(await journalValue(next.s, 'j.meta')).toEqual([2]);
      expect(await journalValue(next.s, 'j.log.2')).toEqual([5]);
    });

    it('does not write an append twice when the page finished it but not its entry', async () => {
      const first = page();
      await first.s.put('Journal', 'j.log.1', bytes());
      await first.s.put('Journal', 'j.log.1', bytes(1, 2), true);
      first.tail.items.set(
        'kiln.journal-tail.j.9',
        JSON.stringify({ op: 'append', key: 'j.log.1', at: 0, data: btoa('\u0001\u0002') }),
      );
      const next = page(first.root, first.tail);
      await next.s.list('Journal', '');
      expect(await journalValue(next.s, 'j.log.1')).toEqual([1, 2]);
    });

    it('leaves a journal alone while another page holds it', async () => {
      const root = new FakeDir();
      const tail = new FakeTail();
      const locks = fakeLocks();
      const env = { root: async () => root as unknown as FileSystemDirectoryHandle, locks, tail };
      tail.items.set(
        'kiln.journal-tail.theirs.0',
        JSON.stringify({ op: 'put', key: 'theirs.meta', data: btoa('x') }),
      );
      locks.held.add('kiln-journal:theirs');
      await new OpfsStore(env).list('Journal', '');
      expect(tail.items.size).toBe(1);
    });

    it('never holds a record back behind a large checkpoint', async () => {
      const first = page();
      await first.s.put('Journal', 'j.base.1', bytes(1));
      await first.s.put('Journal', 'j.log.1', bytes());
      await first.s.put('Journal', 'j.meta', bytes(1));
      FakeDir.stuck = new Set(['j.base.2']);
      ignore(first.s.put('Journal', 'j.log.2', bytes()));
      ignore(first.s.put('Journal', 'j.base.2', new Uint8Array(3 * 1024 * 1024)));
      ignore(first.s.put('Journal', 'j.log.2', bytes(5), true));
      ignore(first.s.put('Journal', 'j.draft', bytes(7)));
      // A checkpoint needs no copy, and nothing is at risk.
      expect(first.s.unprotectedWrites()).toBe(0);
      FakeDir.stuck = new Set();

      const next = page(first.root, first.tail);
      // The checkpoint's empty file is a leftover the core deletes.
      expect(await next.s.list('Journal', '')).toEqual([
        'j.base.1',
        'j.base.2',
        'j.draft',
        'j.log.1',
        'j.log.2',
        'j.meta',
      ]);
      expect(await journalValue(next.s, 'j.log.2')).toEqual([5]);
      expect(await journalValue(next.s, 'j.draft')).toEqual([7]);
    });

    it('asks while a change waits for its checkpoint', async () => {
      const { s } = page();
      await s.put('Journal', 'j.unkept', bytes());
      expect(s.unprotectedWrites()).toBe(1);
      await s.delete('Journal', 'j.unkept');
      expect(s.unprotectedWrites()).toBe(0);
    });

    it('goes past a record too large for the tail, and says one is missing', async () => {
      const first = page();
      await first.s.put('Journal', 'j.log.1', bytes());
      FakeDir.stuck = new Set(['j.log.1']);
      ignore(first.s.put('Journal', 'j.log.1', bytes(1), true));
      ignore(first.s.put('Journal', 'j.log.1', new Uint8Array(3 * 1024 * 1024), true));
      expect(first.s.unprotectedWrites()).toBe(1);
      ignore(first.s.put('Journal', 'j.log.1', bytes(2), true));
      await new Promise((r) => setTimeout(r, 5));
      FakeDir.stuck = new Set();

      const next = page(first.root, first.tail);
      expect(await next.s.list('Journal', '')).toEqual(['j.log.1', 'j.unkept']);
      expect(await journalValue(next.s, 'j.log.1')).toEqual([1, 2]);
    });

    it('says once when the tail has no room, and goes on', async () => {
      const { s, tail, root } = page();
      const trouble: string[] = [];
      const full = new OpfsStore({
        root: async () => root as unknown as FileSystemDirectoryHandle,
        locks: fakeLocks(),
        tail,
        onTailTrouble: (r) => trouble.push(r),
      });
      tail.room = 0;
      await full.put('Journal', 'j.meta', bytes(1));
      await full.put('Journal', 'j.meta', bytes(2));
      expect(trouble).toEqual(['full']);
      expect(await journalValue(full, 'j.meta')).toEqual([2]);
      expect(s.unprotectedWrites()).toBe(0);
    });

    it('leaves a journal whose tail it cannot read as it is, and says so', async () => {
      const root = new FakeDir();
      const tail = new FakeTail();
      const trouble: string[] = [];
      const env = {
        root: async () => root as unknown as FileSystemDirectoryHandle,
        locks: fakeLocks(),
        tail,
        onTailTrouble: (r: string) => trouble.push(r),
      };
      const dir = await (await root.getDirectoryHandle('kiln-store', { create: true })).getDirectoryHandle(
        'Journal',
        { create: true },
      );
      dir.files.set('later.meta', bytes(1));
      dir.files.set('now.meta', bytes(1));
      const later = JSON.stringify({ v: 2, op: 'frobnicate', key: 'later.meta' });
      tail.items.set('kiln.journal-tail.later.0', later);
      tail.items.set(
        'kiln.journal-tail.now.0',
        JSON.stringify({ op: 'put', key: 'now.base.1', data: btoa('x') }),
      );
      const s = new OpfsStore(env);
      expect(await s.list('Journal', '')).toEqual(['now.base.1', 'now.meta']);
      expect(tail.items.get('kiln.journal-tail.later.0')).toBe(later);
      expect(trouble).toEqual(['newer']);
    });

    it('keeps the text being typed in the tail alone, one value for the journal', async () => {
      const first = page();
      await first.s.put('Journal', 'j.meta', bytes(1));
      FakeDir.committed = [];
      await first.s.put('Journal', 'j.draft', bytes(1));
      await first.s.put('Journal', 'j.draft', bytes(1, 2));
      expect(FakeDir.committed).toEqual([]);
      expect(first.tail.items.size).toBe(1);

      const next = page(first.root, first.tail);
      expect(await next.s.list('Journal', '')).toEqual(['j.draft', 'j.meta']);
      expect(await journalValue(next.s, 'j.draft')).toEqual([1, 2]);
      await next.s.put('Journal', 'j.draft', bytes(3));
      await next.s.delete('Journal', 'j.draft');
      await next.s.written();
      expect(next.tail.items.size).toBe(0);
      expect(await next.s.list('Journal', 'j.d')).toEqual([]);
    });

    it('never brings back a draft older than the record that ended it', async () => {
      const first = page();
      await first.s.put('Journal', 'j.base.1', bytes(1));
      await first.s.put('Journal', 'j.log.1', bytes());
      await first.s.put('Journal', 'j.meta', bytes(1));
      await first.s.put('Journal', 'j.draft', bytes(7, 1));
      // Enter: the record, then the draft's removal, and the page goes before either is written.
      FakeDir.stuck = new Set(['j.log.1']);
      ignore(first.s.put('Journal', 'j.log.1', bytes(9), true));
      ignore(first.s.delete('Journal', 'j.draft'));
      expect([...first.tail.items.keys()].some((k) => k.endsWith('.draft'))).toBe(false);
      FakeDir.stuck = new Set();

      const next = page(first.root, first.tail);
      expect(await next.s.list('Journal', '')).toEqual(['j.base.1', 'j.log.1', 'j.meta']);
      expect(await journalValue(next.s, 'j.log.1')).toEqual([9]);
    });

    it('replays a draft in its place among the records', async () => {
      const first = page();
      await first.s.put('Journal', 'j.log.1', bytes());
      FakeDir.stuck = new Set(['j.log.1']);
      ignore(first.s.put('Journal', 'j.log.1', bytes(1), true));
      await first.s.put('Journal', 'j.draft', bytes(2));
      ignore(first.s.put('Journal', 'j.log.1', bytes(3), true));
      FakeDir.stuck = new Set();
      const next = page(first.root, first.tail);
      expect(await next.s.list('Journal', '')).toEqual(['j.draft', 'j.log.1']);
      expect(await journalValue(next.s, 'j.draft')).toEqual([2]);
      expect(await journalValue(next.s, 'j.log.1')).toEqual([1, 3]);
    });

    it('never writes over what another page left, and finishes it before using the journal', async () => {
      const first = page();
      await first.s.put('Journal', 'j.base.1', bytes(1));
      await first.s.put('Journal', 'j.log.1', bytes());
      await first.s.put('Journal', 'j.meta', bytes(1));
      FakeDir.stuck = new Set(['j.log.1']);
      ignore(first.s.put('Journal', 'j.log.1', bytes(5), true));
      ignore(first.s.put('Journal', 'j.log.1', bytes(6), true));
      FakeDir.stuck = new Set();
      const left = [...first.tail.items.keys()];
      // The next page cannot take the journal at first (the crashed page's lock is still there),
      // so its list leaves the entries; it opens the journal a moment later.
      const locks = fakeLocks();
      locks.held.add('kiln-journal:j');
      const next = new OpfsStore({
        root: async () => first.root as unknown as FileSystemDirectoryHandle,
        locks,
        tail: first.tail,
      });
      setTimeout(() => locks.held.delete('kiln-journal:j'), 50);
      expect(await next.list('Journal', '')).toEqual(['j.base.1', 'j.log.1', 'j.meta']);
      // Its own entries have names of their own.
      FakeDir.stuck = new Set(['j.log.2']);
      ignore(next.put('Journal', 'j.log.2', bytes()));
      ignore(next.put('Journal', 'j.log.2', bytes(7), true));
      FakeDir.stuck = new Set();
      const mine = [...first.tail.items.keys()].filter((k) => !left.includes(k));
      expect(mine).toHaveLength(2);
      const third = page(first.root, first.tail);
      await third.s.list('Journal', '');
      expect(await journalValue(third.s, 'j.log.1')).toEqual([5, 6]);
      expect(await journalValue(third.s, 'j.log.2')).toEqual([7]);
    });

    it('waits for the writes given so far', async () => {
      const { s } = page();
      FakeDir.delay = new Map([[encodeURIComponent('j.base.1'), 20]]);
      let done = false;
      void s.put('Journal', 'j.base.1', bytes(1)).then(() => {
        done = true;
      });
      await s.written();
      FakeDir.delay = new Map();
      expect(done).toBe(true);
    });
  });

  it('answers the core and orders the recent files as the core does', async () => {
    const { s } = store();
    const file = (path: string, opened: number, pinned: boolean) =>
      new TextEncoder().encode(
        JSON.stringify({ path, name: path, kind: 'Write', opened_at_ms: opened, pinned, thumbnail: null }),
      );
    await s.put('Recents', 'a', file('a.docx', 1, false));
    await s.put('Recents', 'b', file('b.docx', 3, false));
    await s.put('Recents', 'c', file('c.docx', 2, true));
    await s.put('Recents', 'bad', bytes(123));
    expect((await s.recents()).map((f) => f.path)).toEqual(['c.docx', 'b.docx', 'a.docx']);
    const put: HostOp = { StorePut: { area: 'Dictionary', key: 'user', bytes: [107, 105] } };
    expect(await answerStore(s, put)).toBe('Done');
    expect(await answerStore(s, { StoreGet: { area: 'Dictionary', key: 'user' } })).toEqual({
      Bytes: [107, 105],
    });
    expect(await answerStore(s, { StoreGet: { area: 'Dictionary', key: 'none' } })).toBe('Empty');
    expect(await answerStore(s, { StoreList: { area: 'Dictionary', prefix: '' } })).toEqual({
      Keys: ['user'],
    });
    expect(await answerStore(s, { ReadFile: { path: 'x' } })).toBeUndefined();
    expect(await answerStore(null, put)).toMatchObject({ Failed: { kind: 'Unsupported' } });
  });
});
