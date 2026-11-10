import { describe, expect, it } from 'vitest';
import { HostSockets, type SocketEnv, type SocketLike } from './sockets';

class FakeSocket implements SocketLike {
  binaryType = 'blob';
  onopen: ((e: unknown) => void) | null = null;
  onmessage: ((e: { data: unknown }) => void) | null = null;
  onclose: ((e: unknown) => void) | null = null;
  onerror: ((e: unknown) => void) | null = null;
  sent: number[][] = [];
  closed = false;
  send(data: Uint8Array): void {
    this.sent.push(Array.from(data));
  }
  close(): void {
    this.closed = true;
    this.onclose?.({});
  }
  receive(bytes: number[]): void {
    this.onmessage?.({ data: Uint8Array.from(bytes).buffer });
  }
}

function env() {
  const sockets: FakeSocket[] = [];
  let online = true;
  const offline: (() => void)[] = [];
  const timers: (() => void)[] = [];
  const e: SocketEnv = {
    open: () => {
      const s = new FakeSocket();
      sockets.push(s);
      return s;
    },
    online: () => online,
    onOffline: (f) => {
      offline.push(f);
      return () => {};
    },
    setTimeout: (f) => {
      timers.push(f);
    },
  };
  return {
    e,
    sockets,
    goOffline: () => {
      online = false;
      for (const f of offline) f();
    },
    /** Lets every timer so far run out. */
    elapse: () => {
      for (const f of timers.splice(0)) f();
    },
  };
}

describe('HostSockets', () => {
  it('opens, sends, and hands over messages in order', async () => {
    const { e, sockets } = env();
    const h = new HostSockets(e);
    const opening = h.handle({ SocketOpen: { socket: 1, url: 'ws://relay/rooms/a' } });
    sockets[0]?.onopen?.({});
    expect(await opening).toBe('Done');
    expect(sockets[0]?.binaryType).toBe('arraybuffer');
    expect(await h.handle({ SocketSend: { socket: 1, bytes: [1, 2] } })).toBe('Done');
    expect(sockets[0]?.sent).toEqual([[1, 2]]);
    sockets[0]?.receive([7]);
    expect(await h.handle({ SocketReceive: { socket: 1 } })).toEqual({ Bytes: [7] });
    const waiting = h.handle({ SocketReceive: { socket: 1 } });
    sockets[0]?.receive([8, 9]);
    expect(await waiting).toEqual({ Bytes: [8, 9] });
  });

  it('answers a waiting receive with Empty when the socket closes or the browser goes offline', async () => {
    const { e, sockets, goOffline } = env();
    const h = new HostSockets(e);
    const opening = h.handle({ SocketOpen: { socket: 2, url: 'ws://relay/rooms/b' } });
    sockets[0]?.onopen?.({});
    await opening;
    const waiting = h.handle({ SocketReceive: { socket: 2 } });
    goOffline();
    expect(await waiting).toBe('Empty');
    expect(sockets[0]?.closed).toBe(true);
    const again = await h.handle({ SocketOpen: { socket: 3, url: 'ws://relay/rooms/b' } });
    expect(again).toEqual({ Failed: { kind: 'Io', message: 'the browser is offline' } });
  });

  it('fails an open that never connects, and leaves other operations alone', async () => {
    const { e, sockets, elapse } = env();
    const h = new HostSockets(e);
    const opening = h.handle({ SocketOpen: { socket: 4, url: 'ws://nowhere' } });
    sockets[0]?.onerror?.({});
    expect(await opening).toEqual({ Failed: { kind: 'Io', message: 'could not connect to ws://nowhere' } });
    expect(h.size).toBe(0);
    const timer = h.handle({ Timer: { ms: 10 } });
    elapse();
    expect(await timer).toBe('Done');
    expect(h.handle('Recents')).toBeUndefined();
    expect(h.handle({ LoadPref: { key: 'x' } })).toBeUndefined();
  });

  it('gives up on an open that takes too long', async () => {
    const { e, sockets, elapse } = env();
    const h = new HostSockets(e);
    const opening = h.handle({ SocketOpen: { socket: 5, url: 'ws://slow' } });
    elapse();
    expect(await opening).toEqual({
      Failed: { kind: 'Io', message: 'could not connect to ws://slow in time' },
    });
    expect(sockets[0]?.closed).toBe(true);
    expect(h.size).toBe(0);
  });

  it('closes sockets still opening when the browser goes offline', async () => {
    const { e, sockets, goOffline } = env();
    const h = new HostSockets(e);
    const opening = h.handle({ SocketOpen: { socket: 6, url: 'ws://relay/rooms/c' } });
    goOffline();
    expect(sockets[0]?.closed).toBe(true);
    expect(await opening).toEqual({
      Failed: { kind: 'Io', message: 'could not connect to ws://relay/rooms/c' },
    });
    expect(h.size).toBe(0);
  });

  it('forgets a socket the other end closed once it is read out', async () => {
    const { e, sockets } = env();
    const h = new HostSockets(e);
    const opening = h.handle({ SocketOpen: { socket: 7, url: 'ws://relay/rooms/d' } });
    sockets[0]?.onopen?.({});
    await opening;
    sockets[0]?.receive([1]);
    sockets[0]?.onclose?.({});
    expect(h.size).toBe(1);
    expect(await h.handle({ SocketReceive: { socket: 7 } })).toEqual({ Bytes: [1] });
    expect(await h.handle({ SocketReceive: { socket: 7 } })).toBe('Empty');
    expect(h.size).toBe(0);
  });
});
