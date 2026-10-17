import { useEffect, useState } from 'react';
import { useStore } from 'zustand';
import type { WriteEditorModel, WriteUiState } from './model.ts';

/** Subscribes a component to part of the editor's UI state. */
export function useWrite<T>(model: WriteEditorModel, select: (s: WriteUiState) => T): T {
  return useStore(model.store, select);
}

/** Reads a CSS custom property (brand tokens) from an element, with a fallback. */
export function cssVar(el: Element | null, name: string, fallback: string): string {
  if (!el || typeof getComputedStyle !== 'function') return fallback;
  const v = getComputedStyle(el).getPropertyValue(name).trim();
  return v || fallback;
}

/** Points ⇄ document pixels (96 dpi) ⇄ centimetres and inches, for display. */
export const PX_PER_PT = 4 / 3;
export const ptToCm = (pt: number) => (pt / 72) * 2.54;
export const cmToPt = (cm: number) => (cm / 2.54) * 72;

/** Whether the UI shows inches (US English) or centimetres. */
export function usesInches(): boolean {
  const lang = typeof navigator !== 'undefined' ? navigator.language : 'en-US';
  return /^en-(US|PH)$|^en$/i.test(lang) || /-(US|LR|MM)$/i.test(lang);
}

/**
 * A number that changes whenever the colour theme may have changed: the OS scheme
 * (`prefers-color-scheme`, which `system` follows) or the `data-theme`/`data-app`/`class`/`style`
 * of `<html>` (the theme the user picks). Canvases that read `--kiln-*` colours when they draw (the
 * rulers) list it among their effect's dependencies so they repaint in the new colours.
 */
export function useThemeKey(): number {
  const [key, setKey] = useState(0);
  useEffect(() => {
    const bump = () => setKey((k) => k + 1);
    const query = typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: dark)') : null;
    query?.addEventListener('change', bump);
    const root = typeof document !== 'undefined' ? document.documentElement : null;
    const observer = root && typeof MutationObserver === 'function' ? new MutationObserver(bump) : null;
    if (root && observer)
      observer.observe(root, {
        attributes: true,
        attributeFilter: ['data-theme', 'data-app', 'class', 'style'],
      });
    return () => {
      query?.removeEventListener('change', bump);
      observer?.disconnect();
    };
  }, []);
  return key;
}

/** Whether a media query matches, kept current (compact layouts: the chrome's 639 px breakpoint). */
export function useMediaQuery(query: string): boolean {
  const get = () => (typeof matchMedia === 'function' ? matchMedia(query).matches : false);
  const [matches, setMatches] = useState(get);
  useEffect(() => {
    if (typeof matchMedia !== 'function') return;
    const q = matchMedia(query);
    const on = () => setMatches(q.matches);
    on();
    q.addEventListener('change', on);
    return () => q.removeEventListener('change', on);
  }, [query]);
  return matches;
}
