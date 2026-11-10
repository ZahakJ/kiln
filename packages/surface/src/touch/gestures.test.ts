import { describe, expect, it } from 'vitest';
import { GestureRecognizer, type GestureTimers, LONG_PRESS_MS } from './gestures';
import { touchSelection, unionRect } from './selection';
import { doubleTapZoom } from './zoom';

/** A clock the test moves by hand. */
function clock() {
  let t = 0;
  const due: { at: number; fn: () => void; id: number }[] = [];
  let next = 1;
  const timers: GestureTimers = {
    now: () => t,
    setTimeout: (fn, ms) => {
      const id = next++;
      due.push({ at: t + ms, fn, id });
      return id;
    },
    clearTimeout: (h) => {
      const i = due.findIndex((d) => d.id === h);
      if (i >= 0) due.splice(i, 1);
    },
  };
  const advance = (ms: number) => {
    t += ms;
    for (const d of [...due].sort((a, b) => a.at - b.at))
      if (d.at <= t) {
        due.splice(due.indexOf(d), 1);
        d.fn();
      }
  };
  return { timers, advance };
}

function recorder() {
  const log: string[] = [];
  const g = (c = clock()) =>
    ({
      c,
      r: new GestureRecognizer(
        {
          onTap: (p) => log.push(`tap ${p.x},${p.y}`),
          onDoubleTap: (p) => log.push(`double ${p.x},${p.y}`),
          onLongPress: (p) => log.push(`long ${p.x},${p.y}`),
          onSwipe: (d) => log.push(`swipe ${d}`),
          onPinchStart: () => log.push('pinch start'),
          onPinch: (s) => log.push(`pinch ${s.toFixed(2)}`),
          onPinchEnd: (s) => log.push(`pinch end ${s.toFixed(2)}`),
        },
        c.timers,
      ),
    }) as const;
  return { log, g };
}

describe('GestureRecognizer', () => {
  it('a tap, then a second tap soon after is a double tap', () => {
    const { log, g } = recorder();
    const { c, r } = g();
    r.start([{ x: 100, y: 100 }]);
    c.advance(80);
    r.end([], { x: 102, y: 101 });
    c.advance(150);
    r.start([{ x: 104, y: 99 }]);
    c.advance(70);
    r.end([], { x: 104, y: 99 });
    expect(log).toEqual(['tap 100,100', 'double 104,99']);
  });

  it('taps far apart in time or place are two taps', () => {
    const { log, g } = recorder();
    const { c, r } = g();
    r.start([{ x: 10, y: 10 }]);
    r.end([]);
    c.advance(600);
    r.start([{ x: 10, y: 10 }]);
    r.end([]);
    c.advance(100);
    r.start([{ x: 200, y: 10 }]);
    r.end([]);
    expect(log).toEqual(['tap 10,10', 'tap 10,10', 'tap 200,10']);
  });

  it('a finger held still is a long press, and lifting it then is no tap', () => {
    const { log, g } = recorder();
    const { c, r } = g();
    r.start([{ x: 50, y: 60 }]);
    r.move([{ x: 53, y: 62 }]);
    c.advance(LONG_PRESS_MS + 10);
    r.end([]);
    expect(log).toEqual(['long 50,60']);
  });

  it('a finger that moves scrolls: no long press, no tap; a quick flick is a swipe', () => {
    const { log, g } = recorder();
    const { c, r } = g();
    r.start([{ x: 300, y: 200 }]);
    r.move([{ x: 260, y: 204 }]);
    c.advance(120);
    r.move([{ x: 180, y: 206 }]);
    c.advance(LONG_PRESS_MS);
    r.end([], { x: 180, y: 206 });
    expect(log).toEqual([]);
    r.start([{ x: 100, y: 200 }]);
    c.advance(90);
    r.move([{ x: 180, y: 202 }]);
    r.end([], { x: 200, y: 203 });
    expect(log).toEqual(['swipe right']);
  });

  it('two fingers pinch: the scale is the distance over the starting distance; no tap follows', () => {
    const { log, g } = recorder();
    const { r } = g();
    r.start([{ x: 100, y: 100 }]);
    r.start([
      { x: 100, y: 100 },
      { x: 200, y: 100 },
    ]);
    r.move([
      { x: 80, y: 100 },
      { x: 220, y: 100 },
    ]);
    r.move([
      { x: 50, y: 100 },
      { x: 250, y: 100 },
    ]);
    r.end([{ x: 50, y: 100 }]);
    r.end([]);
    expect(log).toEqual(['pinch start', 'pinch 1.40', 'pinch 2.00', 'pinch end 2.00']);
  });
});

describe('touch selection and double-tap zoom', () => {
  it('publishes once per change, and a surface clears only its own selection', () => {
    const a = {} as HTMLElement;
    const b = {} as HTMLElement;
    let calls = 0;
    const stop = touchSelection.subscribe(() => calls++);
    const sel = {
      kind: 'text' as const,
      rect: { left: 1, top: 2, right: 30, bottom: 20 },
      target: a,
      point: { x: 5, y: 5 },
    };
    touchSelection.set(sel);
    touchSelection.set({ ...sel, rect: { ...sel.rect } });
    expect(calls).toBe(1);
    touchSelection.clear(b);
    expect(touchSelection.get()).not.toBeNull();
    touchSelection.clear(a);
    expect(touchSelection.get()).toBeNull();
    expect(calls).toBe(2);
    stop();
  });

  it('unions rectangles', () => {
    expect(
      unionRect([
        { left: 10, top: 10, right: 20, bottom: 20 },
        { left: 5, top: 15, right: 12, bottom: 40 },
      ]),
    ).toEqual({ left: 5, top: 10, right: 20, bottom: 40 });
    expect(unionRect([])).toBeNull();
  });

  it('fits the width first, then looks closer, then fits again', () => {
    expect(doubleTapZoom(1, 0.42)).toBe(0.42);
    expect(doubleTapZoom(0.42, 0.42)).toBe(0.84);
    expect(doubleTapZoom(0.84, 0.42)).toBe(0.42);
    expect(doubleTapZoom(3, 3, 4)).toBe(4);
  });
});
