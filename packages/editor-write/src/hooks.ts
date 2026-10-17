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
