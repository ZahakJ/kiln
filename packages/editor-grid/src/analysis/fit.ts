/**
 * Keeps a popup that opens at a point inside the window: one that would run past the right or
 * bottom edge moves left or opens above its point instead (as Excel's menus do).
 */
import { type RefObject, useLayoutEffect, useRef } from 'react';

export function useFitInWindow<T extends HTMLElement>(x: number, y: number): RefObject<T | null> {
  const ref = useRef<T | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || typeof window === 'undefined') return;
    const r = el.getBoundingClientRect();
    const w = window.innerWidth;
    const h = window.innerHeight;
    const left = Math.max(8, Math.min(x, w - r.width - 8));
    let top = y;
    if (y + r.height > h - 8) top = y - r.height > 8 ? y - r.height : Math.max(8, h - r.height - 8);
    el.style.left = `${left}px`;
    el.style.top = `${top}px`;
  }, [x, y]);
  return ref;
}
