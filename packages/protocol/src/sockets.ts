/**
 * The WebSockets and timers of shared documents: the core asks for them with
 * `HostOp.SocketOpen`, `SocketSend`, `SocketReceive`, `SocketClose` and `Timer`; in a browser the
 * page answers with the browser's `WebSocket` and `setTimeout`. `SocketReceive` is answered with
 * the next binary message, or `Empty` once the socket has closed, and the core asks again after
 * each message. A socket that has not connected after `OPEN_TIMEOUT_MS` fails. When the browser
 * goes offline every socket closes, opening ones too, so edits wait in the core and merge when it
 * reconnects.
 */

import type { HostOp } from './generated/HostOp';
import type { HostReply } from './generated/HostReply';

/** How long a connection may take to open. */
export const OPEN_TIMEOUT_MS = 10_000;

function failed(message: string): HostReply {
  return { Failed: { kind: 'Io', message } };
}

/** The parts of `WebSocket` used here (a fake in tests). */
export interface SocketLike {
  binaryType: string;
  onopen: ((e: unknown) => void) | null;
  onmessage: ((e: { data: unknown }) => void) | null;
  onclose: ((e: unknown) => void) | null;
  onerror: ((e: unknown) => void) | null;
  send(data: Uint8Array): void;
  close(): void;
}

/** Where the sockets come from and whether the browser is online. */
export interface SocketEnv {
  open(url: string): SocketLike;
  online(): boolean;
  /** Calls `f` whenever the browser goes offline; returns the unsubscribe function. */
  onOffline(f: () => void): () => void;
  setTimeout(f: () => void, ms: number): unknown;
}

interface Entry {
  ws: SocketLike;
  frames: Uint8Array[];
  /** Still connecting (sends fail until it has opened). */
  opening: boolean;
  closed: boolean;
  waiting: ((reply: HostReply) => void) | null;
}

/** The browser's sockets, network state and timers. */
export function browserSocketEnv(win: (Window & typeof globalThis) | undefined): SocketEnv | undefined {
  if (!win || typeof win.WebSocket !== 'function') return undefined;
  return {
    open: (url) => new win.WebSocket(url) as unknown as SocketLike,
    online: () => win.navigator?.onLine !== false,
    onOffline: (f) => {
      win.addEventListener('offline', f);
      return () => win.removeEventListener('offline', f);
    },
    setTimeout: (f, ms) => win.setTimeout(f, ms),
  };
}

/** Answers the socket and timer operations; `undefined` for every other one. */
export class HostSockets {
  private readonly sockets = new Map<number, Entry>();
  private stopOffline: (() => void) | null = null;

  constructor(private readonly env: SocketEnv) {}

  handle(op: HostOp): Promise<HostReply> | undefined {
    if (typeof op !== 'object') return undefined;
    if ('SocketOpen' in op) return this.open(op.SocketOpen.socket, op.SocketOpen.url);
    if ('SocketSend' in op) return Promise.resolve(this.send(op.SocketSend.socket, op.SocketSend.bytes));
    if ('SocketReceive' in op) return this.receive(op.SocketReceive.socket);
    if ('SocketClose' in op) return Promise.resolve(this.close(op.SocketClose.socket));
    if ('Timer' in op) {
      const ms = Math.max(0, op.Timer.ms);
      return new Promise((resolve) => this.env.setTimeout(() => resolve('Done'), ms));
    }
    return undefined;
  }

  private open(socket: number, url: string): Promise<HostReply> {
    if (!this.env.online()) return Promise.resolve(failed('the browser is offline'));
    this.stopOffline ??= this.env.onOffline(() => this.closeAll());
    let ws: SocketLike;
    try {
      ws = this.env.open(url);
    } catch (error) {
      return Promise.resolve(failed(error instanceof Error ? error.message : String(error)));
    }
    ws.binaryType = 'arraybuffer';
    const entry: Entry = { ws, frames: [], opening: true, closed: false, waiting: null };
    // Known while it connects, so that going offline or SocketClose reaches it.
    this.sockets.set(socket, entry);
    return new Promise((resolve) => {
      let settled = false;
      const fail = (why: string) => {
        if (settled) return;
        settled = true;
        entry.opening = false;
        entry.closed = true;
        if (this.sockets.get(socket) === entry) this.sockets.delete(socket);
        resolve(failed(why));
      };
      this.env.setTimeout(() => {
        if (settled) return;
        fail(`could not connect to ${url} in time`);
        try {
          ws.close();
        } catch {
          // Closing a socket that never opened.
        }
      }, OPEN_TIMEOUT_MS);
      ws.onopen = () => {
        if (settled) return;
        settled = true;
        entry.opening = false;
        resolve('Done');
      };
      ws.onmessage = (e) => {
        const data = e.data;
        if (!(data instanceof ArrayBuffer)) return;
        const bytes = new Uint8Array(data);
        const w = entry.waiting;
        if (w) {
          entry.waiting = null;
          w({ Bytes: Array.from(bytes) });
        } else {
          entry.frames.push(bytes);
        }
      };
      const end = () => {
        if (!settled) {
          fail(`could not connect to ${url}`);
          return;
        }
        this.ended(socket, entry);
      };
      ws.onclose = end;
      ws.onerror = end;
    });
  }

  /** The socket closed: a waiting receive ends empty, and the entry goes once it is read out. */
  private ended(socket: number, entry: Entry): void {
    entry.closed = true;
    const w = entry.waiting;
    entry.waiting = null;
    w?.('Empty');
    if (entry.frames.length === 0 && this.sockets.get(socket) === entry) this.sockets.delete(socket);
  }

  private send(socket: number, bytes: number[]): HostReply {
    const entry = this.sockets.get(socket);
    if (!entry || entry.closed || entry.opening) return failed('the connection is closed');
    try {
      entry.ws.send(Uint8Array.from(bytes));
      return 'Done';
    } catch (error) {
      return failed(error instanceof Error ? error.message : String(error));
    }
  }

  private receive(socket: number): Promise<HostReply> {
    const entry = this.sockets.get(socket);
    if (!entry) return Promise.resolve('Empty');
    const next = entry.frames.shift();
    if (next) return Promise.resolve({ Bytes: Array.from(next) });
    if (entry.closed) {
      this.sockets.delete(socket);
      return Promise.resolve('Empty');
    }
    return new Promise((resolve) => {
      // One receive at a time per socket; an older one (none in practice) ends empty.
      entry.waiting?.('Empty');
      entry.waiting = resolve;
    });
  }

  private close(socket: number): HostReply {
    const entry = this.sockets.get(socket);
    if (entry) {
      this.sockets.delete(socket);
      try {
        entry.ws.close();
      } catch {
        // Closing a socket that never opened.
      }
      this.ended(socket, entry);
    }
    return 'Done';
  }

  /** How many sockets are open or opening. */
  get size(): number {
    return this.sockets.size;
  }

  /** The browser went offline: every connection ends, opening ones too (the core reconnects
   * later). */
  private closeAll(): void {
    for (const socket of [...this.sockets.keys()]) this.close(socket);
  }
}
