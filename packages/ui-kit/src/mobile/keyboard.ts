/**
 * Keyboard docking. The on-screen keyboard shrinks the visual viewport (Chrome on
 * Android, Safari) or the whole web view (the Android app, `adjustResize`). The part of the window
 * the keyboard covers is written to `--kiln-keyboard-inset` on the frame, so the bottom toolbar
 * sits on the keyboard's top edge; where `visualViewport` is missing, the stylesheet falls back to
 * `env(keyboard-inset-height)` (the VirtualKeyboard API).
 */
import { type RefObject, useEffect } from 'react';

/** Below this the visual viewport only moved (pinch zoom, the address bar), not a keyboard. */
const KEYBOARD_MIN_PX = 60;

export interface ViewportLike {
  height: number;
  offsetTop: number;
  scale?: number;
}

/** The height the keyboard covers at the bottom of a layout viewport `innerHeight` tall. */
export function keyboardInset(viewport: ViewportLike, innerHeight: number): number {
  // Zoomed in, the visual viewport is smaller for another reason: nothing to dock to.
  if (viewport.scale !== undefined && viewport.scale > 1.01) return 0;
  const covered = Math.round(innerHeight - viewport.height - viewport.offsetTop);
  return covered >= KEYBOARD_MIN_PX ? covered : 0;
}

/**
 * Follows the on-screen keyboard while `enabled`: sets `--kiln-keyboard-inset` (px) and
 * `data-keyboard="open"` on the element, and clears them when the keyboard goes.
 */
export function useKeyboardInset(target: RefObject<HTMLElement | null>, enabled: boolean): void {
  useEffect(() => {
    const el = target.current;
    const viewport = typeof window !== 'undefined' ? window.visualViewport : null;
    if (!el || !enabled || !viewport) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      const inset = keyboardInset(viewport, window.innerHeight);
      el.style.setProperty('--kiln-keyboard-inset', `${inset}px`);
      if (inset > 0) el.dataset.keyboard = 'open';
      else delete el.dataset.keyboard;
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    update();
    viewport.addEventListener('resize', schedule);
    viewport.addEventListener('scroll', schedule);
    window.addEventListener('resize', schedule);
    return () => {
      if (frame) cancelAnimationFrame(frame);
      viewport.removeEventListener('resize', schedule);
      viewport.removeEventListener('scroll', schedule);
      window.removeEventListener('resize', schedule);
      el.style.removeProperty('--kiln-keyboard-inset');
      delete el.dataset.keyboard;
    };
  }, [target, enabled]);
}
