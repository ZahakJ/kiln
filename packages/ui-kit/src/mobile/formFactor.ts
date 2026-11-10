/**
 * The window's form factor: a phone below 640 CSS px, a tablet from 640 to
 * 1023 px, the desktop layout from 1024 px (a tablet in landscape with a keyboard gets it too).
 */
import { useSyncExternalStore } from 'react';

export type FormFactor = 'phone' | 'tablet' | 'desktop';

/** The widest phone layout, in CSS px. */
export const PHONE_MAX_WIDTH = 639;
/** The widest tablet layout, in CSS px; the desktop layout starts one pixel wider. */
export const TABLET_MAX_WIDTH = 1023;

const PHONE_QUERY = `(max-width: ${PHONE_MAX_WIDTH}px)`;
const TABLET_QUERY = `(max-width: ${TABLET_MAX_WIDTH}px)`;

/** The form factor of a window this wide. */
export function formFactorOf(width: number): FormFactor {
  if (width <= PHONE_MAX_WIDTH) return 'phone';
  if (width <= TABLET_MAX_WIDTH) return 'tablet';
  return 'desktop';
}

function media(query: string): MediaQueryList | null {
  return typeof window !== 'undefined' && typeof window.matchMedia === 'function'
    ? window.matchMedia(query)
    : null;
}

function current(): FormFactor {
  if (media(PHONE_QUERY)?.matches) return 'phone';
  if (media(TABLET_QUERY)?.matches) return 'tablet';
  return 'desktop';
}

function subscribe(onChange: () => void): () => void {
  const lists = [media(PHONE_QUERY), media(TABLET_QUERY)].filter((l): l is MediaQueryList => !!l);
  for (const l of lists) l.addEventListener('change', onChange);
  return () => {
    for (const l of lists) l.removeEventListener('change', onChange);
  };
}

/** The window's form factor, following resizes and rotations (the desktop layout without a window). */
export function useFormFactor(): FormFactor {
  return useSyncExternalStore(subscribe, current, () => 'desktop');
}
