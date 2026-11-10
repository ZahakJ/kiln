/**
 * The slide show's clock. The show asks the core for frames at the clock's time, so a test can
 * stop the clock and evaluate any transition or animation at a chosen moment (`ManualClock`);
 * the presenter's window uses the page's monotonic clock.
 */

export interface ShowClock {
  /** Milliseconds; any origin, never decreasing. */
  now(): number;
  /** Called when the time jumps (a manual clock was set); returns the unsubscribe function. */
  subscribe?(fn: () => void): () => void;
}

/** `performance.now()`. */
export const realClock: ShowClock = {
  now: () => (typeof performance === 'undefined' ? Date.now() : performance.now()),
};

/** A clock that only moves when it is told to (tests, screenshots at a given progress). */
export class ManualClock implements ShowClock {
  private t: number;
  private readonly listeners = new Set<() => void>();

  constructor(start = 0) {
    this.t = start;
  }

  now(): number {
    return this.t;
  }

  /** Sets the time (it never goes back). */
  set(ms: number): void {
    if (!Number.isFinite(ms)) return;
    this.t = Math.max(this.t, ms);
    for (const l of [...this.listeners]) l();
  }

  /** Moves the time on. */
  advance(ms: number): void {
    this.set(this.t + Math.max(0, ms));
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }
}
