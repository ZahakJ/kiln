import { describe, expect, it, vi } from 'vitest';
import type { Event } from './generated/Event';
import type { HostReply } from './generated/HostReply';
import type { HostHandler } from './host';
import { WasmWorkerClient, type WorkerLike } from './wasm-worker-client';
import { serveRequest } from './worker-host';
import type { WasmSession, WorkerMessage, WorkerRequest } from './worker-protocol';

/** A stand-in for the wasm `KilnSession` with just enough behaviour to exercise the bridge. */
class FakeSession implements WasmSession {
  events: Event[] = [];
  text = '';
  replies: [number, string][] = [];
  execute(doc: number | null | undefined, cmd: string): string {
    const command = JSON.parse(cmd);
    if (command.Suite === 'Ping') {
      return JSON.stringify({
        outcome: 'Unchanged',
        summary: null,
        value: { Pong: { version: '0.1.0', protocol: 1, target: 'wasm32-unknown' } },
      });
    }
    if (command.Suite?.Open) {
      this.events.push({ HostRequest: { id: 7, op: { ReadFile: { path: command.Suite.Open.path } } } });
      return JSON.stringify({ outcome: { Pending: 7 }, summary: null, value: 'None' });
    }
    if (doc === 99) throw new Error(JSON.stringify({ kind: 'NotFound', message: 'doc#99 not found' }));
    this.text += command.Write.InsertText.text;
    this.events.push({ Closed: { doc: doc ?? 0 } });
    return JSON.stringify({ outcome: 'Applied', summary: null, value: 'None' });
  }
  query(): string {
    return JSON.stringify({ Paragraphs: [{ id: 1, text: this.text }] });
  }
  open(bytes: Uint8Array, hint: string): string {
    return JSON.stringify({ bytes: bytes.length, hint: JSON.parse(hint) });
  }
  newDocument(kind: string): string {
    return JSON.stringify({ kind: JSON.parse(kind) });
  }
  setViewport(): void {}
  frame(): Uint8Array {
    // An empty SceneDelta for revision 300 (see postcard.test.ts).
    return Uint8Array.of(
      0xac,
      0x02,
      0,
      0,
      0,
      0x80,
      0x3f,
      ...new Array<number>(16).fill(0),
      0,
      0,
      0,
      0,
      0,
      0,
      0,
    );
  }
  a11y(): string {
    return JSON.stringify({ nodes: [], active: null, announce: null });
  }
  poll(): string {
    const out = JSON.stringify(this.events);
    this.events = [];
    return out;
  }
  hostReply(id: number, reply: string): void {
    this.replies.push([id, reply]);
  }
}

/** A Worker whose "other side" runs serveRequest synchronously on a microtask. */
function fakeWorker(session: WasmSession) {
  const listeners: ((e: MessageEvent<WorkerMessage>) => void)[] = [];
  const errorListeners: ((e: ErrorEvent) => void)[] = [];
  const send = (msg: WorkerMessage) => {
    queueMicrotask(() => {
      for (const l of listeners) l({ data: msg } as MessageEvent<WorkerMessage>);
    });
  };
  const worker: WorkerLike & { send: typeof send; crash: (m: string) => void } = {
    postMessage: (req: WorkerRequest) => serveRequest(session, req, { postMessage: (m) => send(m) }),
    addEventListener: ((type: string, l: never) => {
      if (type === 'message') listeners.push(l);
      else errorListeners.push(l);
    }) as WorkerLike['addEventListener'],
    terminate: vi.fn(),
    send,
    crash: (message) => {
      for (const l of errorListeners) l({ message } as ErrorEvent);
    },
  };
  return worker;
}

const noHost: HostHandler = { handle: async () => 'Done' };

describe('WasmWorkerClient', () => {
  it('round-trips commands, queries, frames and events', async () => {
    const session = new FakeSession();
    const worker = fakeWorker(session);
    const client = new WasmWorkerClient(worker, noHost);
    worker.send({ type: 'ready', version: '0.1.0' });
    expect(await client.ready).toBe('0.1.0');

    const info = await client.ping();
    expect(info.version).toBe('0.1.0');

    const seen: Event[] = [];
    const unsubscribe = client.subscribe((e) => seen.push(e));
    await client.execute(3, { Write: { InsertText: { text: 'Hi' } } });
    await client.execute(3, { Write: { InsertText: { text: '!' } } });
    const q = await client.query(3, 'Paragraphs');
    expect(q).toEqual({ Paragraphs: [{ id: 1, text: 'Hi!' }] });
    await Promise.resolve();
    expect(seen).toEqual([{ Closed: { doc: 3 } }, { Closed: { doc: 3 } }]);
    unsubscribe();

    expect(await client.frame(3)).toMatchObject({ revision: 300, reset: false, scale: 1, tiles: [] });
    expect(await client.new('Scratch')).toEqual({ kind: 'Scratch' });
    const bytes = Uint8Array.of(1, 2, 3);
    expect(
      await client.open(bytes, { name: 'a.txt', path: null, format: null, read_only: false }),
    ).toMatchObject({
      bytes: 3,
      hint: { name: 'a.txt' },
    });
    expect(bytes.length).toBe(3);
    await client.setViewport(3, {
      width_px: 1,
      height_px: 1,
      scroll: { x: 0, y: 0 },
      zoom: 1,
      dpr: 1,
      mode: 'PrintLayout',
    });
    expect((await client.a11y(3)).nodes).toEqual([]);
  });

  it('rejects with KilnError carrying the kind', async () => {
    const client = new WasmWorkerClient(fakeWorker(new FakeSession()), noHost);
    await expect(client.execute(99, { Suite: 'Undo' })).rejects.toMatchObject({
      name: 'KilnError',
      kind: 'NotFound',
      message: 'doc#99 not found',
    });
  });

  it('answers host requests on the main thread', async () => {
    const session = new FakeSession();
    const handle = vi.fn(async (): Promise<HostReply> => ({ Bytes: [104, 105] }));
    const client = new WasmWorkerClient(fakeWorker(session), { handle });
    await client.execute(null, { Suite: { Open: { path: '/a.txt' } } });
    await vi.waitFor(() => expect(session.replies).toEqual([[7, '{"Bytes":[104,105]}']]));
    expect(handle).toHaveBeenCalledWith({ ReadFile: { path: '/a.txt' } });
  });

  it('fails every call after a fatal error or disposal', async () => {
    const worker = fakeWorker(new FakeSession());
    const client = new WasmWorkerClient(worker, noHost);
    worker.send({ type: 'fatal', error: { kind: 'Unsupported', message: 'no wasm' } });
    await expect(client.ready).rejects.toMatchObject({ kind: 'Unsupported' });
    await expect(client.ping()).rejects.toMatchObject({ kind: 'Unsupported', message: 'no wasm' });

    const other = fakeWorker(new FakeSession());
    const second = new WasmWorkerClient(other, noHost);
    other.crash('boom');
    await expect(second.ping()).rejects.toMatchObject({ kind: 'Internal' });

    const third = new WasmWorkerClient(fakeWorker(new FakeSession()), noHost);
    third.dispose();
    await expect(third.ping()).rejects.toMatchObject({ kind: 'Closed' });
  });

  it('rejects a core that speaks another protocol', async () => {
    const session = new FakeSession();
    session.execute = () =>
      JSON.stringify({
        outcome: 'Unchanged',
        summary: null,
        value: { Pong: { version: '9', protocol: 99, target: 'x' } },
      });
    const client = new WasmWorkerClient(fakeWorker(session), noHost);
    await expect(client.ping()).rejects.toMatchObject({ kind: 'Unsupported' });
  });
});

describe('serveRequest', () => {
  it('transfers frame buffers and reports thrown errors', () => {
    const posted: [WorkerMessage, Transferable[]][] = [];
    const target = { postMessage: (m: WorkerMessage, t: Transferable[]) => posted.push([m, t]) };
    const session = new FakeSession();
    serveRequest(session, { id: 1, method: 'frame', args: [1] }, target);
    expect(posted[0]?.[1]).toHaveLength(1);
    serveRequest(
      session,
      { id: 2, method: 'execute', args: [99, '{"Write":{"InsertText":{"text":"x"}}}'] },
      target,
    );
    expect(posted[1]?.[0]).toEqual({
      type: 'reply',
      id: 2,
      ok: false,
      error: { kind: 'NotFound', message: 'doc#99 not found' },
    });
    session.poll = () => 'not json';
    serveRequest(session, { id: 3, method: 'hostReply', args: [1, '"Done"'] }, target);
    expect(posted.at(-1)?.[0]).toMatchObject({ type: 'events', events: [{ Error: { doc: null } }] });
  });
});
