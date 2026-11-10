/**
 * The app storage of the web: the core's `HostOp::Store*` requests answered from the
 * Origin Private File System. Each value is one file, `kiln-store/<area>/<key>` (the key
 * URI-encoded); appending writes at the end of that file. The browser commits a file when its
 * writable stream closes, so a value is either written or not; a write the page did not finish
 * before it closed (a reload, a crash) is simply missing, and the core's journal format drops a
 * record cut short anyway.
 *
 * The requests arrive without waiting for each other, so the order the core gave them is kept
 * here: the writes of one journal (its checkpoint, log and marker, whatever the key) run one
 * after another in that order, as do the writes of any other value. A journal's marker is
 * therefore never committed ahead of the checkpoint it names.
 *
 * A journal belongs to one page at a time. The page holds a Web Lock (`kiln-journal:<key>`) for
 * each journal it writes or recovers, taken only if no other page has it (`ifAvailable`), until
 * the journal is removed or the page goes: writing a journal, reading one of its checkpoints
 * (Document Recovery ▸ Open) or removing its marker (Delete) fails with `Unavailable` while
 * another tab holds it, and `StoreList` leaves out the journals open in another tab or in another
 * app core of this page, so the Document Recovery pane never offers them. Locks end with the page,
 * so the journals of a page that crashed are listed again.
 *
 * **The write-ahead tail.** An OPFS write is asynchronous, and a page that goes takes its unfinished
 * writes with it. So every journal write is copied to `localStorage` first, in the task that
 * receives it from the core: that call is synchronous, and what it stored belongs to the browser,
 * not to the page, so it outlives a reload, a closed tab and a crash of the page. The core posts an
 * edit's journal writes before the frame that shows the edit, so what is on screen is in the tail
 * or already in the OPFS. An entry goes when its write has finished; the entries a page that went
 * left behind are replayed, in order, before the journals are listed.
 *
 * Appends never wait for anything: each goes to the tail when it arrives, whatever is being written.
 * A large checkpoint does not go in the tail at all, and needs not: the core moves the marker only
 * when the checkpoint is on the disk, and until then the marker's checkpoint and the logs after it
 * hold everything (`kiln_storage::journal`). A record too large for the tail (`TAIL_MAX_BYTES`)
 * leaves a stand-in saying a change is missing, so recovery can say so, and is counted in
 * {@link OpfsStore.unprotectedWrites}, as is a change that waits for a large checkpoint
 * (`k.unkept`): the page asks before it is left while there are any. Later records stay in the tail
 * and replay after the gap.
 *
 * The text of a cell still being typed (`k.draft`) changes with every key and is replaced, not
 * added to, so it lives in the tail alone while the page is alive, numbered in the journal's order
 * like every entry. The core sends an entry's record and the draft's removal together: the record
 * goes to the tail and the draft out of it in the same task, so no reload can find a draft older
 * than the record that ended it.
 *
 * Each entry names the tail format (`TAIL_FORMAT`). A journal with an entry this version cannot
 * read is left exactly as it is and is not listed: a later version finishes it. When the tail has
 * no room the page says so once ({@link TAIL_TROUBLE}), and journals go on without it.
 */
import type { HostReply, RecentFile, StoreArea } from '@kiln/protocol';
import { failed } from '@kiln/protocol';

const ROOT = 'kiln-store';
const LOCK_PREFIX = 'kiln-journal:';
const TAIL_PREFIX = 'kiln.journal-tail.';
/** The largest write the tail holds (a record, or the checkpoint of a small document). */
const TAIL_MAX_BYTES = 2 * 1024 * 1024;
/** The format of the tail's entries. An entry with none is this one's: it has not changed. */
const TAIL_FORMAT = 1;

/** Dispatched on `window` when the tail stops working (`detail.reason`), once until it works. */
export const TAIL_TROUBLE = 'kiln:journal-tail';

/** The Origin Private File System of the page (or none: private windows in some browsers). */
export type Directory = FileSystemDirectoryHandle;

/** Minimal Web Locks typings. */
interface LockManagerLike {
  request(
    name: string,
    options: { ifAvailable: boolean },
    callback: (lock: unknown) => Promise<void> | undefined,
  ): Promise<void>;
  query(): Promise<{ held?: { name?: string }[] }>;
}

/** A journal another tab holds. */
export class OpenElsewhere extends Error {
  constructor() {
    super('the document is open in another window');
    this.name = 'OpenElsewhere';
  }
}

/** The parts of the browser the store uses (tests give their own). */
export interface StoreEnv {
  root: () => Promise<Directory>;
  locks?: LockManagerLike;
  /** The write-ahead tail: the browser's `localStorage`, or none when the page may not use it. */
  tail?: TailStorage | null;
  /** The tail has no room, or holds entries of a later version. */
  onTailTrouble?: (reason: 'full' | 'newer') => void;
}

/** The parts of `localStorage` the tail uses. */
export type TailStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem' | 'key' | 'length'>;

/** One journal write in the tail. */
interface TailEntry {
  /** The tail format. */
  v?: number;
  op: 'put' | 'append' | 'delete';
  key: string;
  /** Base64 of the bytes (`put`, `append`). */
  data?: string;
  /** Where an append starts: an append the log already reaches is not written twice. */
  at?: number;
  /** A draft's place among the journal's entries: its page and number there. */
  seq?: number;
  page?: string;
}

function toBase64(bytes: Uint8Array): string {
  let text = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(text);
}

function fromBase64(text: string): Uint8Array {
  return Uint8Array.from(atob(text), (c) => c.charCodeAt(0));
}

const isDraft = (area: StoreArea, key: string) => area === 'Journal' && key.endsWith('.draft');
const isUnkept = (area: StoreArea, key: string) => area === 'Journal' && key.endsWith('.unkept');

/** The browser's OPFS and Web Locks, when there are any. */
export function browserStoreEnv(): StoreEnv | null {
  const nav = (globalThis as { navigator?: Navigator }).navigator;
  const storage = nav?.storage;
  if (!storage?.getDirectory) return null;
  const locks = (nav as Navigator & { locks?: LockManagerLike }).locks;
  let tail: TailStorage | null = null;
  try {
    tail = globalThis.localStorage ?? null;
  } catch {
    // Site data is blocked: journals are written without a tail.
  }
  const onTailTrouble = (reason: 'full' | 'newer') =>
    globalThis.dispatchEvent?.(new CustomEvent(TAIL_TROUBLE, { detail: { reason } }));
  return { root: () => storage.getDirectory(), locks, tail, onTailTrouble };
}

/** The journal a key of the journal area belongs to (`k.meta`, `k.base.3` → `k`). */
export function journalOf(key: string): string {
  const dot = key.indexOf('.');
  return dot > 0 ? key.slice(0, dot) : key;
}

/** A lock this page asked for: whether it got it, and how to let it go. */
interface Held {
  granted: Promise<boolean>;
  release: () => void;
  /** The lock is free again. */
  gone: Promise<void>;
}

export class OpfsStore {
  private readonly queues = new Map<string, Promise<unknown>>();
  /** The journals this page holds (or asked for). */
  private readonly held = new Map<string, Held>();
  /** The next entry of the tail. */
  private tailSeq = 0;
  /**
   * This page among the pages that ever wrote the tail: entries are named and ordered by page,
   * then number, so a page never writes over the entries another page left.
   */
  private readonly page =
    `${Date.now().toString(36).padStart(9, '0')}${Math.random().toString(36).slice(2, 6)}`;
  /** The journals whose entries left by other pages this page has finished. */
  private readonly caughtUp = new Set<string>();
  /** The length each log will have once the writes given so far are done. */
  private readonly logSizes = new Map<string, number>();
  /** Each journal's structural writes in flight that are not in the tail. */
  private readonly heldBack = new Map<string, number>();
  /** Records in flight that are not in the tail. */
  private unkeptRecords = 0;
  /** The journals with a change waiting for its checkpoint. */
  private readonly waitingChanges = new Set<string>();
  /** The journals whose tail this version cannot read. */
  private readonly tooNew = new Set<string>();
  /** The tail said it was full, and has not taken a write since. */
  private tailFull = false;

  constructor(private readonly env: StoreEnv) {}

  /** Changes that would be lost if the page went now. */
  unprotectedWrites(): number {
    let total = this.unkeptRecords + this.waitingChanges.size;
    for (const n of this.heldBack.values()) total += n;
    return total;
  }

  /** Every write given so far has finished. */
  async written(): Promise<void> {
    while (this.queues.size > 0) await Promise.all(this.queues.values());
  }

  /** Stores an entry in the tail; `false` when it has no room. */
  private toTail(id: string, entry: TailEntry): boolean {
    const tail = this.env.tail;
    if (!tail) return false;
    try {
      tail.setItem(id, JSON.stringify({ ...entry, v: TAIL_FORMAT }));
      this.tailFull = false;
      return true;
    } catch {
      if (!this.tailFull) this.env.onTailTrouble?.('full');
      this.tailFull = true;
      return false;
    }
  }

  /**
   * Copies a journal write to the tail, synchronously. Returns what to do when the write has
   * finished. Records are always copied when they fit. A checkpoint too large to copy needs no
   * copy. A marker or a small checkpoint that could not be copied holds back the journal's other
   * such writes until it is done: they could not be replayed without it.
   */
  private ahead(entry: Omit<TailEntry, 'data'>, bytes?: Uint8Array): () => void {
    const journal = journalOf(entry.key);
    // Asked for before the entry is visible, so another page's replay leaves this journal alone.
    if (entry.op !== 'delete') void this.claim(journal);
    const tail = this.env.tail;
    const size = bytes?.length ?? 0;
    const fits = size <= TAIL_MAX_BYTES;
    if (!fits && entry.key.includes('.base')) return () => undefined;
    const record = entry.op === 'append';
    // Test builds only (`tools/chaos-planted.sh`): the review's planted bug, a tail that drops
    // the copies of records, which pwa-chaos must catch. Normal builds leave this out.
    if (import.meta.env.VITE_KILN_PLANT === 'tail-drops-records' && record) return () => undefined;
    const id = `${TAIL_PREFIX}${journal}.${this.page}.${this.tailSeq++}`;
    if (fits && (record || !this.heldBack.has(journal)) && (entry.at ?? 0) >= 0) {
      const full: TailEntry = bytes ? { ...entry, data: toBase64(bytes) } : entry;
      if (this.toTail(id, full)) {
        // Test builds only (`tools/chaos-planted.sh 16 tail-removed-early`): round 6's planted
        // bug W2, a record's copy taken off the tail as soon as its write starts, not when it
        // has finished. Normal builds leave this out.
        if (import.meta.env.VITE_KILN_PLANT === 'tail-removed-early' && record) {
          tail?.removeItem(id);
          return () => undefined;
        }
        return () => tail?.removeItem(id);
      }
    }
    if (record) {
      // Recovery will say a change is missing; the records after it go on.
      const standIn = this.toTail(id, { op: 'put', key: `${journal}.unkept`, data: '' });
      this.unkeptRecords += 1;
      return () => {
        this.unkeptRecords -= 1;
        if (standIn) tail?.removeItem(id);
      };
    }
    this.heldBack.set(journal, (this.heldBack.get(journal) ?? 0) + 1);
    return () => {
      const left = (this.heldBack.get(journal) ?? 1) - 1;
      if (left > 0) this.heldBack.set(journal, left);
      else this.heldBack.delete(journal);
    };
  }

  /** The tail's entries, each journal's in the order they were written: page by page. */
  private tailEntries(): Map<string, { id: string; page: string; seq: number }[]> {
    const found = new Map<string, { id: string; page: string; seq: number }[]>();
    const tail = this.env.tail;
    if (!tail) return found;
    for (let i = 0; i < tail.length; i += 1) {
      const id = tail.key(i);
      if (!id?.startsWith(TAIL_PREFIX)) continue;
      const rest = id.slice(TAIL_PREFIX.length);
      const dot = rest.indexOf('.');
      if (dot <= 0) continue;
      const journal = rest.slice(0, dot);
      const part = rest.slice(dot + 1);
      let page: string;
      let seq: number;
      if (part === 'draft') {
        // A draft keeps its page and number inside it (its key stays as it is replaced).
        const e = this.readEntry(id);
        page = e?.page ?? '';
        seq = e?.seq ?? -1;
      } else {
        const at = part.lastIndexOf('.');
        page = at > 0 ? part.slice(0, at) : '';
        seq = Number(part.slice(at + 1));
      }
      found.set(journal, [...(found.get(journal) ?? []), { id, page, seq }]);
    }
    for (const entries of found.values())
      entries.sort((a, b) => (a.page === b.page ? a.seq - b.seq : a.page < b.page ? -1 : 1));
    // Test builds only (`tools/chaos-planted.sh 16 tail-loses-last`): review 4's planted bug W1,
    // a tail whose last record entry of each journal is lost when it is read. Normal builds
    // leave this out.
    if (import.meta.env.VITE_KILN_PLANT === 'tail-loses-last')
      for (const entries of found.values()) {
        const ops = entries.map((e) => this.readEntry(e.id)?.op);
        const i = ops.lastIndexOf('append');
        if (i >= 0) entries.splice(i, 1);
      }
    return found;
  }

  /** The entry, or `null` when this version cannot read it. */
  private readEntry(id: string): TailEntry | null {
    try {
      const entry = JSON.parse(this.env.tail?.getItem(id) ?? 'null') as TailEntry | null;
      if (!entry || (entry.v ?? TAIL_FORMAT) !== TAIL_FORMAT || typeof entry.key !== 'string') return null;
      return ['put', 'append', 'delete'].includes(entry.op) ? entry : null;
    } catch {
      return null;
    }
  }

  /**
   * Finishes the writes of pages that went: each journal no page holds gets the writes its tail
   * still has, in order, and is free again for whichever page opens it.
   */
  private async replayTail(): Promise<void> {
    if (!this.env.tail) return;
    for (const journal of this.tailEntries().keys()) {
      if (this.held.has(journal) || this.tooNew.has(journal)) continue;
      if (!(await this.claimSoon(journal))) continue;
      await this.serial('Journal', `${journal}.tail`, () => this.catchUp(journal)).catch(() => undefined);
      await this.letGo(journal);
    }
  }

  /**
   * Takes a journal's lock, waiting a moment for it: the lock of a page that crashed goes a
   * little after the page. A live page's never does.
   */
  private async claimSoon(journal: string): Promise<boolean> {
    for (let i = 0; i < 20; i += 1) {
      if (await this.claim(journal)) return true;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    return false;
  }

  /**
   * Replays the entries other pages left for `journal`, which this page holds: before this page
   * reads or writes it, so nothing it writes is ever followed by an older page's entry.
   */
  private async catchUp(journal: string): Promise<void> {
    const tail = this.env.tail;
    if (!tail || this.caughtUp.has(journal)) return;
    const theirs = (this.tailEntries().get(journal) ?? []).filter((e) => e.page !== this.page);
    const read = theirs.map((e) => ({ id: e.id, entry: this.readEntry(e.id) }));
    if (read.some((e) => e.entry === null)) {
      if (!this.tooNew.has(journal)) this.env.onTailTrouble?.('newer');
      this.tooNew.add(journal);
      throw new OpenElsewhere();
    }
    for (const { id, entry } of read) {
      if (entry) await this.replay(entry);
      tail.removeItem(id);
    }
    this.caughtUp.add(journal);
  }

  private async replay(entry: TailEntry): Promise<void> {
    const dir = await this.dir('Journal', true);
    if (!dir) return;
    const name = encodeURIComponent(entry.key);
    if (entry.op === 'delete') {
      await dir.removeEntry(name).catch((error: unknown) => {
        if (!(error instanceof DOMException && error.name === 'NotFoundError')) throw error;
      });
      return;
    }
    const bytes = fromBase64(entry.data ?? '');
    const handle = await dir.getFileHandle(name, { create: true });
    // An append the page finished itself is not written twice. One past the end of the log follows
    // a record too large for the tail: it goes on after the gap, a record being whole in itself.
    if (entry.op === 'append' && (await handle.getFile()).size >= (entry.at ?? 0) + bytes.length) return;
    await this.writeFile(handle, bytes, entry.op === 'append');
  }

  private async writeFile(handle: FileSystemFileHandle, bytes: Uint8Array, append: boolean): Promise<void> {
    const stream = await handle.createWritable({ keepExistingData: append });
    try {
      if (append) {
        const size = (await handle.getFile()).size;
        await stream.write({ type: 'write', position: size, data: bytes as Uint8Array<ArrayBuffer> });
      } else {
        await stream.write(bytes as Uint8Array<ArrayBuffer>);
      }
      await stream.close();
    } catch (error) {
      await stream.abort().catch(() => undefined);
      throw error;
    }
  }

  private async dir(area: StoreArea, create: boolean): Promise<Directory | null> {
    const root = await this.env.root();
    try {
      const top = await root.getDirectoryHandle(ROOT, { create });
      return await top.getDirectoryHandle(area, { create });
    } catch (error) {
      if (!create && error instanceof DOMException && error.name === 'NotFoundError') return null;
      throw error;
    }
  }

  /** Runs `job` after the earlier jobs on the same value (on the same journal, any of its keys). */
  private serial<T>(area: StoreArea, key: string, job: () => Promise<T>): Promise<T> {
    const id = area === 'Journal' ? `${area}/${journalOf(key)}` : `${area}/${key}`;
    const prev = this.queues.get(id) ?? Promise.resolve();
    const next = prev.then(job, job);
    const tail = next.catch(() => undefined);
    this.queues.set(id, tail);
    void tail.then(() => {
      if (this.queues.get(id) === tail) this.queues.delete(id);
    });
    return next;
  }

  /** Takes the lock of a journal unless another page has it; `false` when it does. */
  private claim(journal: string): Promise<boolean> {
    const locks = this.env.locks;
    if (!locks) return Promise.resolve(true);
    const known = this.held.get(journal);
    if (known) return known.granted;
    let release: () => void = () => undefined;
    const done = new Promise<void>((resolve) => {
      release = resolve;
    });
    let gone: Promise<void> = Promise.resolve();
    const granted = new Promise<boolean>((resolve) => {
      gone = locks
        .request(LOCK_PREFIX + journal, { ifAvailable: true }, (lock) => {
          resolve(!!lock);
          return lock ? done : undefined;
        })
        .catch(() => resolve(false));
    });
    this.held.set(journal, { granted, release, gone });
    void granted.then((ok) => {
      if (!ok && this.held.get(journal)?.granted === granted) this.held.delete(journal);
    });
    return granted;
  }

  /** Fails unless this page holds (or now takes) the journal of `key`. */
  private async own(key: string): Promise<void> {
    const journal = journalOf(key);
    if (!(await this.claim(journal))) throw new OpenElsewhere();
    await this.catchUp(journal);
  }

  private letGo(key: string): Promise<void> {
    const journal = journalOf(key);
    const held = this.held.get(journal);
    held?.release();
    this.held.delete(journal);
    this.caughtUp.delete(journal);
    return held?.gone ?? Promise.resolve();
  }

  async get(area: StoreArea, key: string): Promise<Uint8Array | null> {
    return this.serial(area, key, async () => {
      // Reading a journal's checkpoint means opening it (Document Recovery ▸ Open): it is this
      // page's from now on, unless another page has it.
      if (area === 'Journal' && key.includes('.base')) await this.own(key);
      const dir = await this.dir(area, false);
      if (!dir) return null;
      try {
        const file = await (await dir.getFileHandle(encodeURIComponent(key))).getFile();
        return new Uint8Array(await file.arrayBuffer());
      } catch (error) {
        if (error instanceof DOMException && error.name === 'NotFoundError') return null;
        throw error;
      }
    });
  }

  async put(area: StoreArea, key: string, bytes: Uint8Array, append = false): Promise<void> {
    if (isDraft(area, key) && this.keepDraft(key, bytes)) return;
    if (isUnkept(area, key)) this.waitingChanges.add(journalOf(key));
    const finished = area === 'Journal' ? this.aheadOfPut(key, bytes, append) : undefined;
    try {
      await this.serial(area, key, async () => {
        if (area === 'Journal') await this.own(key);
        const dir = await this.dir(area, true);
        if (!dir) throw new Error('the app storage is not available');
        const handle = await dir.getFileHandle(encodeURIComponent(key), { create: true });
        await this.writeFile(handle, bytes, append);
      });
    } finally {
      finished?.();
    }
  }

  private aheadOfPut(key: string, bytes: Uint8Array, append: boolean): () => void {
    if (!append) {
      this.logSizes.set(key, bytes.length);
      return this.ahead({ op: 'put', key }, bytes);
    }
    const at = this.logSizes.get(key) ?? -1;
    if (at >= 0) this.logSizes.set(key, at + bytes.length);
    return this.ahead({ op: 'append', key, at }, bytes);
  }

  /** The text of a cell being typed replaces the last in the tail; `false` without a tail. */
  private keepDraft(key: string, bytes: Uint8Array): boolean {
    if (bytes.length > TAIL_MAX_BYTES) return false;
    const journal = journalOf(key);
    void this.claim(journal);
    return this.toTail(`${TAIL_PREFIX}${journal}.draft`, {
      op: 'put',
      key,
      data: toBase64(bytes),
      seq: this.tailSeq++,
      page: this.page,
    });
  }

  async delete(area: StoreArea, key: string): Promise<void> {
    const marker = area === 'Journal' && key.endsWith('.meta');
    const journal = journalOf(key);
    // At once: the record that ended the entry is in the tail already.
    if (isDraft(area, key)) this.env.tail?.removeItem(`${TAIL_PREFIX}${journal}.draft`);
    if (isUnkept(area, key)) this.waitingChanges.delete(journal);
    const finished = area === 'Journal' ? this.ahead({ op: 'delete', key }) : undefined;
    try {
      await this.serial(area, key, async () => {
        // Removing a journal (or Document Recovery's Delete) needs it to be this page's.
        if (marker) await this.own(key);
        const dir = await this.dir(area, false);
        if (!dir) return;
        try {
          await dir.removeEntry(encodeURIComponent(key));
        } catch (error) {
          if (!(error instanceof DOMException && error.name === 'NotFoundError')) throw error;
        }
      });
    } finally {
      finished?.();
    }
    // A journal goes marker first: once its marker is gone it is no longer this page's.
    if (marker) void this.letGo(key);
  }

  async list(area: StoreArea, prefix: string): Promise<string[]> {
    if (area === 'Journal') await this.replayTail();
    const dir = await this.dir(area, false);
    if (!dir) return [];
    const keys: string[] = [];
    for await (const name of (dir as Directory & { keys(): AsyncIterable<string> }).keys()) {
      const key = decodeURIComponent(name);
      if (key.startsWith(prefix)) keys.push(key);
    }
    keys.sort();
    if (area !== 'Journal') return keys;
    const readable = keys.filter((k) => !this.tooNew.has(journalOf(k)));
    if (!this.env.locks) return readable;
    // Leave out the journals another tab or core holds (they are open, not lost).
    const state = await this.env.locks.query().catch(() => ({ held: [] }));
    const open = new Set(
      (state.held ?? [])
        .map((l) => l.name ?? '')
        .filter((n) => n.startsWith(LOCK_PREFIX))
        .map((n) => n.slice(LOCK_PREFIX.length)),
    );
    return readable.filter((k) => !open.has(journalOf(k)));
  }

  /** The recent files, pinned first, then the most recently opened (the core's order). */
  async recents(): Promise<RecentFile[]> {
    const files: RecentFile[] = [];
    for (const key of await this.list('Recents', '')) {
      const bytes = await this.get('Recents', key);
      if (!bytes) continue;
      try {
        files.push(JSON.parse(new TextDecoder().decode(bytes)) as RecentFile);
      } catch {
        // A damaged entry is left out.
      }
    }
    return files.sort(
      (a, b) =>
        Number(b.pinned) - Number(a.pinned) ||
        b.opened_at_ms - a.opened_at_ms ||
        a.name.localeCompare(b.name),
    );
  }
}

/** The bytes of a write: large ones arrive from the Worker as a transferred array. */
type Bytes = number[] | Uint8Array;
const toBytes = (bytes: Bytes) => (bytes instanceof Uint8Array ? bytes : Uint8Array.from(bytes));

/** Answers a `HostOp::Store*` request (`undefined` for other requests). */
export async function answerStore(store: OpfsStore | null, op: object): Promise<HostReply | undefined> {
  const isStore = ['StoreGet', 'StorePut', 'StoreAppend', 'StoreDelete', 'StoreList'].some((k) => k in op);
  if (!isStore) return undefined;
  if (!store) return failed('Unsupported', 'the app storage is not available in this browser');
  try {
    if ('StoreGet' in op) {
      const { area, key } = op.StoreGet as { area: StoreArea; key: string };
      const bytes = await store.get(area, key);
      return bytes ? { Bytes: Array.from(bytes) } : 'Empty';
    }
    if ('StorePut' in op) {
      const { area, key, bytes } = op.StorePut as { area: StoreArea; key: string; bytes: Bytes };
      await store.put(area, key, toBytes(bytes));
      return 'Done';
    }
    if ('StoreAppend' in op) {
      const { area, key, bytes } = op.StoreAppend as { area: StoreArea; key: string; bytes: Bytes };
      await store.put(area, key, toBytes(bytes), true);
      return 'Done';
    }
    if ('StoreDelete' in op) {
      const { area, key } = op.StoreDelete as { area: StoreArea; key: string };
      await store.delete(area, key);
      return 'Done';
    }
    const { area, prefix } = (op as { StoreList: { area: StoreArea; prefix: string } }).StoreList;
    return { Keys: await store.list(area, prefix) };
  } catch (error) {
    if (error instanceof OpenElsewhere) return failed('Unavailable', error.message);
    const quota = error instanceof DOMException && error.name === 'QuotaExceededError';
    const message = error instanceof Error ? error.message : String(error);
    return failed('Io', quota ? 'the browser has no more room for this site' : `app storage: ${message}`);
  }
}
