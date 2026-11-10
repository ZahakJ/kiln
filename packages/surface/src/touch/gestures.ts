/**
 * Touch gestures for the document surfaces: a tap, a double tap, a long press, a
 * swipe and a two-finger pinch, recognised from touch events. The recogniser is plain logic over
 * points and times (`GestureRecognizer`), so it is tested without a browser; `attachGestures`
 * feeds it a DOM element's touch events. Mouse and pen input never reach it: the surfaces keep
 * their pointer handling, and touch is added on top.
 */

/** A point in client CSS px. */
export interface GesturePoint {
  x: number;
  y: number;
}

export interface GestureHandlers {
  /** One finger, down and up again in place. */
  onTap?: (p: GesturePoint) => void;
  /** A second tap close to the first, soon after it (zoom to width). */
  onDoubleTap?: (p: GesturePoint) => void;
  /** One finger held still (the context menu). */
  onLongPress?: (p: GesturePoint, target: EventTarget | null) => void;
  /** A quick one-finger flick, mostly along one axis. */
  onSwipe?: (direction: 'left' | 'right' | 'up' | 'down', from: GesturePoint) => void;
  /** Two fingers went down. */
  onPinchStart?: (center: GesturePoint) => void;
  /** The fingers moved: `scale` is their distance over the starting distance. */
  onPinch?: (scale: number, center: GesturePoint) => void;
  /** A finger lifted: the pinch is over at this scale. */
  onPinchEnd?: (scale: number, center: GesturePoint) => void;
}

/** How far a finger may wander and still tap or hold, in CSS px. */
export const TAP_SLOP = 10;
/** A press held this long is a long press (ms). */
export const LONG_PRESS_MS = 500;
/** Two taps within this time (ms) and distance (px) are a double tap. */
export const DOUBLE_TAP_MS = 320;
export const DOUBLE_TAP_SLOP = 32;
/** A swipe covers this distance (px) within this time (ms). */
export const SWIPE_MIN_PX = 48;
export const SWIPE_MAX_MS = 600;

/** The timers the recogniser uses (a test passes its own). */
export interface GestureTimers {
  now(): number;
  setTimeout(fn: () => void, ms: number): unknown;
  clearTimeout(handle: unknown): void;
}

const realTimers: GestureTimers = {
  now: () => (typeof performance !== 'undefined' ? performance.now() : Date.now()),
  setTimeout: (fn, ms) => setTimeout(fn, ms),
  clearTimeout: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
};

interface Press {
  start: GesturePoint;
  last: GesturePoint;
  t: number;
  target: EventTarget | null;
  moved: boolean;
  held: boolean;
  timer: unknown;
}

/** Recognises gestures from the fingers on a surface (one set of touches at a time). */
export class GestureRecognizer {
  private press: Press | null = null;
  private pinch: { dist: number; center: GesturePoint; scale: number } | null = null;
  private lastTap: { p: GesturePoint; t: number } | null = null;
  /** The fingers of this gesture lifted after a pinch: no tap until all are up. */
  private spent = false;

  constructor(
    private readonly handlers: GestureHandlers,
    private readonly timers: GestureTimers = realTimers,
  ) {}

  /** Whether a pinch is under way. */
  get pinching(): boolean {
    return this.pinch !== null;
  }

  /** The fingers now on the surface, after a touchstart. */
  start(touches: readonly GesturePoint[], target: EventTarget | null = null): void {
    if (touches.length >= 2) {
      this.cancelPress();
      const [a, b] = touches as [GesturePoint, GesturePoint];
      const center = mid(a, b);
      if (!this.pinch) this.handlers.onPinchStart?.(center);
      this.pinch = { dist: Math.max(1, dist(a, b)), center, scale: this.pinch?.scale ?? 1 };
      this.spent = true;
      return;
    }
    const p = touches[0];
    if (!p || this.spent) return;
    const t = this.timers.now();
    const press: Press = { start: p, last: p, t, target, moved: false, held: false, timer: null };
    press.timer = this.timers.setTimeout(() => {
      if (this.press !== press || press.moved) return;
      press.held = true;
      this.lastTap = null;
      this.handlers.onLongPress?.(press.start, press.target);
    }, LONG_PRESS_MS);
    this.press = press;
  }

  /** The fingers moved. */
  move(touches: readonly GesturePoint[]): void {
    if (this.pinch && touches.length >= 2) {
      const [a, b] = touches as [GesturePoint, GesturePoint];
      const center = mid(a, b);
      // The scale is the distance now over the distance when the pinch (re)started.
      const scale = (dist(a, b) / this.pinch.dist) * (this.pinch.scale || 1);
      this.pinch.center = center;
      this.handlers.onPinch?.(scale, center);
      // Keep the running scale for a pinch that loses and regains a finger.
      this.pinch = { ...this.pinch, dist: Math.max(1, dist(a, b)), scale };
      return;
    }
    const press = this.press;
    const p = touches[0];
    if (!press || !p) return;
    press.last = p;
    if (!press.moved && dist(p, press.start) > TAP_SLOP) {
      press.moved = true;
      this.timers.clearTimeout(press.timer);
    }
  }

  /** Fingers lifted; `remaining` are still down. */
  end(remaining: readonly GesturePoint[], lifted?: GesturePoint): void {
    if (this.pinch) {
      if (remaining.length < 2) {
        const { scale, center } = this.pinch;
        this.pinch = null;
        this.handlers.onPinchEnd?.(scale, center);
      }
      if (remaining.length === 0) this.spent = false;
      return;
    }
    const press = this.press;
    this.press = null;
    if (remaining.length === 0) this.spent = false;
    if (!press) return;
    this.timers.clearTimeout(press.timer);
    const end = lifted ?? press.last;
    const t = this.timers.now();
    if (press.held) return;
    if (press.moved || dist(end, press.start) > TAP_SLOP) {
      const dx = end.x - press.start.x;
      const dy = end.y - press.start.y;
      if (t - press.t <= SWIPE_MAX_MS && Math.max(Math.abs(dx), Math.abs(dy)) >= SWIPE_MIN_PX) {
        const horizontal = Math.abs(dx) >= Math.abs(dy) * 1.4;
        const vertical = Math.abs(dy) >= Math.abs(dx) * 1.4;
        if (horizontal) this.handlers.onSwipe?.(dx < 0 ? 'left' : 'right', press.start);
        else if (vertical) this.handlers.onSwipe?.(dy < 0 ? 'up' : 'down', press.start);
      }
      this.lastTap = null;
      return;
    }
    const last = this.lastTap;
    if (last && t - last.t <= DOUBLE_TAP_MS && dist(last.p, press.start) <= DOUBLE_TAP_SLOP) {
      this.lastTap = null;
      this.handlers.onDoubleTap?.(press.start);
      return;
    }
    this.lastTap = { p: press.start, t };
    this.handlers.onTap?.(press.start);
  }

  /** The browser took the touches (a scroll): nothing is recognised. */
  cancel(): void {
    this.cancelPress();
    if (this.pinch) {
      const { scale, center } = this.pinch;
      this.pinch = null;
      this.handlers.onPinchEnd?.(scale, center);
    }
    this.spent = false;
  }

  private cancelPress(): void {
    if (this.press) this.timers.clearTimeout(this.press.timer);
    this.press = null;
    this.lastTap = null;
  }
}

function dist(a: GesturePoint, b: GesturePoint): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

function mid(a: GesturePoint, b: GesturePoint): GesturePoint {
  return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
}

function points(list: TouchList): GesturePoint[] {
  const out: GesturePoint[] = [];
  for (let i = 0; i < list.length; i++) {
    const t = list.item(i);
    if (t) out.push({ x: t.clientX, y: t.clientY });
  }
  return out;
}

export interface AttachOptions {
  /** The pinch is the surface's: the browser does not zoom the page while two fingers are down. */
  ownPinch?: boolean;
  /**
   * A touch that starts on one of these is not a gesture of the surface (its handles, a button
   * over it).
   */
  ignore?: string;
}

/**
 * Recognises the gestures of the fingers on `el`. Returns a function that detaches it.
 */
export function attachGestures(
  el: HTMLElement,
  handlers: GestureHandlers,
  opts: AttachOptions = {},
): () => void {
  const g = new GestureRecognizer(handlers);
  const skip = (e: TouchEvent) =>
    !!opts.ignore && e.target instanceof Element && !!e.target.closest(opts.ignore);
  let ignoring = false;
  const onStart = (e: TouchEvent) => {
    if (e.touches.length === 1) ignoring = skip(e);
    // A second finger makes a pinch wherever the first one went down.
    else if (ignoring && e.touches.length >= 2) ignoring = false;
    if (ignoring) return;
    g.start(points(e.touches), e.target);
  };
  const onMove = (e: TouchEvent) => {
    if (ignoring) return;
    if (opts.ownPinch && e.touches.length >= 2 && e.cancelable) e.preventDefault();
    g.move(points(e.touches));
  };
  const onEnd = (e: TouchEvent) => {
    if (ignoring) {
      if (e.touches.length === 0) ignoring = false;
      return;
    }
    const lifted = e.changedTouches.item(0);
    g.end(points(e.touches), lifted ? { x: lifted.clientX, y: lifted.clientY } : undefined);
  };
  const onCancel = () => {
    ignoring = false;
    g.cancel();
  };
  el.addEventListener('touchstart', onStart, { passive: true });
  el.addEventListener('touchmove', onMove, { passive: !opts.ownPinch });
  el.addEventListener('touchend', onEnd, { passive: true });
  el.addEventListener('touchcancel', onCancel, { passive: true });
  return () => {
    g.cancel();
    el.removeEventListener('touchstart', onStart);
    el.removeEventListener('touchmove', onMove);
    el.removeEventListener('touchend', onEnd);
    el.removeEventListener('touchcancel', onCancel);
  };
}
