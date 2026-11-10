/**
 * What a finger selected: the surfaces (Write's page, Grid's cells, Deck's slide)
 * publish the selection a touch made, and the frame's floating context toolbar shows over it.
 * A mouse or a key ends it; so does anything that hides the selection (a scroll moves it with the
 * rectangle the surface publishes again).
 */

/** A rectangle in client CSS px. */
export interface ClientRect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface TouchSelection {
  /** What the finger selected: text, cells, or a shape or picture. */
  kind: 'text' | 'cells' | 'object';
  /** Where it is on the screen (the toolbar keeps clear of it). */
  rect: ClientRect;
  /** The element that gets the context menu (the toolbar's More). */
  target: HTMLElement;
  /** The point the context menu opens at. */
  point: { x: number; y: number };
  /** The surface's part of the screen: the toolbar stays inside it, off the chrome around it. */
  bounds?: ClientRect;
}

type Listener = () => void;

let current: TouchSelection | null = null;
const listeners = new Set<Listener>();

function same(a: TouchSelection | null, b: TouchSelection | null): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return (
    a.kind === b.kind &&
    a.target === b.target &&
    Math.round(a.rect.left) === Math.round(b.rect.left) &&
    Math.round(a.rect.top) === Math.round(b.rect.top) &&
    Math.round(a.rect.right) === Math.round(b.rect.right) &&
    Math.round(a.rect.bottom) === Math.round(b.rect.bottom)
  );
}

/** The touch selection the floating context toolbar shows over. */
export const touchSelection = {
  get(): TouchSelection | null {
    return current;
  },
  /** Publishes (or, with `null`, ends) the selection a finger made. */
  set(next: TouchSelection | null): void {
    if (same(current, next)) return;
    current = next;
    for (const l of [...listeners]) l();
  },
  /** Ends the selection `target` published (another surface's stays). */
  clear(target?: HTMLElement): void {
    if (!current || (target && current.target !== target)) return;
    current = null;
    for (const l of [...listeners]) l();
  },
  subscribe(listener: Listener): () => void {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
};

/** The rectangle around `rects`, or null when there are none. */
export function unionRect(rects: readonly ClientRect[]): ClientRect | null {
  if (rects.length === 0) return null;
  let { left, top, right, bottom } = rects[0] as ClientRect;
  for (const r of rects) {
    left = Math.min(left, r.left);
    top = Math.min(top, r.top);
    right = Math.max(right, r.right);
    bottom = Math.max(bottom, r.bottom);
  }
  return { left, top, right, bottom };
}
