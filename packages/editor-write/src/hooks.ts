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

/**
 * A counter that changes whenever the brand tokens may resolve differently: the OS colour scheme
 * or contrast preference changes (the theme follows it while on "system"), or the theme root's
 * `data-theme`/`data-app`/`class`/`style` changes (an explicit choice in Options or the title
 * bar). Canvases that read `--kiln-*` colours at draw time (the rulers) list it as a dependency so
 * they repaint in the new theme.
 */
export function useThemeKey(): number {
  const [key, setKey] = useState(0);
  useEffect(() => {
    const bump = () => setKey((k) => k + 1);
    const queries =
      typeof matchMedia === 'function'
        ? ['(prefers-color-scheme: dark)', '(prefers-contrast: more)', '(forced-colors: active)'].map((q) =>
            matchMedia(q),
          )
        : [];
    for (const q of queries) q.addEventListener('change', bump);
    const observer =
      typeof MutationObserver === 'function' && typeof document !== 'undefined'
        ? new MutationObserver(bump)
        : null;
    observer?.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme', 'data-app', 'class', 'style'],
    });
    return () => {
      for (const q of queries) q.removeEventListener('change', bump);
      observer?.disconnect();
    };
  }, []);
  return key;
}

/** Whether a media query matches, following changes (false where `matchMedia` is missing). */
export function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() => typeof matchMedia === 'function' && matchMedia(query).matches);
  useEffect(() => {
    if (typeof matchMedia !== 'function') return;
    const q = matchMedia(query);
    const update = () => setMatches(q.matches);
    update();
    q.addEventListener('change', update);
    return () => q.removeEventListener('change', update);
  }, [query]);
  return matches;
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
