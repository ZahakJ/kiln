/**
 * Live announcements (WCAG 4.1.3): one polite and one assertive live region for the whole window,
 * created on first use and kept for the page's life (a region inserted together with its text is
 * not read by every screen reader). The same text twice in a row is read twice: the regions
 * alternate between two text nodes.
 */
export type Politeness = 'polite' | 'assertive';

const regions = new Map<Politeness, HTMLElement>();
let last: { text: string; at: number } | null = null;
const clearing = new Map<Politeness, ReturnType<typeof setTimeout>>();
/** A message stays in its region this long, then the region empties (it has been read). */
const KEEP_MS = 5000;

function region(politeness: Politeness): HTMLElement | null {
  if (typeof document === 'undefined' || !document.body) return null;
  const have = regions.get(politeness);
  if (have?.isConnected) return have;
  const el = document.createElement('div');
  el.className = 'kiln-visually-hidden';
  el.setAttribute('aria-live', politeness);
  el.setAttribute('aria-atomic', 'true');
  el.setAttribute('role', politeness === 'assertive' ? 'alert' : 'status');
  el.dataset.kilnAnnouncer = politeness;
  document.body.append(el);
  regions.set(politeness, el);
  return el;
}

/** Makes sure both regions exist before the first announcement (so it is not lost). */
export function prepareAnnouncer(): void {
  region('polite');
  region('assertive');
}

/** Reads `text` to screen reader users. Empty text is ignored. */
export function announce(text: string, politeness: Politeness = 'polite'): void {
  const t = text.trim();
  if (!t) return;
  const el = region(politeness);
  if (!el) return;
  const now = Date.now();
  // The same message within 100 ms (two components reporting one change) is read once.
  if (last && last.text === t && now - last.at < 100) return;
  last = { text: t, at: now };
  // A no-break space toggled at the end makes a repeated message a change the reader reports.
  const repeat = el.textContent?.replace(/ $/, '') === t;
  el.textContent = repeat && !el.textContent?.endsWith(' ') ? `${t} ` : t;
  const timer = clearing.get(politeness);
  if (timer) clearTimeout(timer);
  clearing.set(
    politeness,
    setTimeout(() => {
      el.textContent = '';
    }, KEEP_MS),
  );
}

/** The text of the last announcement in a region (tests). */
export function lastAnnouncement(politeness: Politeness = 'polite'): string {
  return (regions.get(politeness)?.textContent ?? '').replace(/ $/, '');
}
