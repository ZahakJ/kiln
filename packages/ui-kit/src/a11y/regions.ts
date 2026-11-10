/**
 * F6 and Shift+F6 move between the window's regions as in Office: the document, the message bar,
 * the open task panes, the status bar, the title bar and the ribbon, in that order (Shift+F6
 * backwards). Each
 * region gets the keys where a user expects them: the ribbon its selected tab, the document its
 * caret (the editors take `kiln:return-to-document`), the others their first control.
 */
import { type RefObject, useEffect } from 'react';

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"]), [contenteditable="true"]';

export type RegionId = 'document' | 'panes' | 'statusbar' | 'titlebar' | 'ribbon';

/** The order F6 follows. */
export const REGION_ORDER: readonly RegionId[] = ['document', 'panes', 'statusbar', 'titlebar', 'ribbon'];

function visible(el: Element): boolean {
  if (el.closest('[hidden], [inert], [aria-hidden="true"]')) return false;
  const r = (el as HTMLElement).getBoundingClientRect?.();
  return !r || r.width > 0 || r.height > 0;
}

function firstFocusable(root: Element | null): HTMLElement | null {
  if (!root) return null;
  // A region that takes the keys itself (Deck's slide thumbnails, a list with its own arrows).
  if (root instanceof HTMLElement && root.matches(FOCUSABLE) && visible(root)) return root;
  for (const el of root.querySelectorAll<HTMLElement>(FOCUSABLE)) if (visible(el)) return el;
  return null;
}

/** The regions of the frame that exist now, each with its element. */
export function regionsOf(root: HTMLElement): { id: RegionId; el: HTMLElement }[] {
  const out: { id: RegionId; el: HTMLElement }[] = [];
  const doc = root.querySelector<HTMLElement>('.kiln-appframe__canvas');
  if (doc) out.push({ id: 'document', el: doc });
  const bar = root.querySelector<HTMLElement>('.kiln-messagebar');
  if (bar && firstFocusable(bar)) out.push({ id: 'panes', el: bar });
  // The frame's panes, and the panes an editor docks beside its page (Navigation, Styles).
  for (const pane of root.querySelectorAll<HTMLElement>(
    '.kiln-appframe__body > :not(.kiln-appframe__canvas), .kiln-appframe__canvas aside, .kiln-appframe__canvas [role="complementary"], .kiln-appframe__canvas [data-kiln-region="pane"]',
  ))
    if (
      visible(pane) &&
      firstFocusable(pane) &&
      !pane.parentElement?.closest('aside, [role="complementary"]')
    )
      out.push({ id: 'panes', el: pane });
  const status = root.querySelector<HTMLElement>('.kiln-statusbar');
  if (status && firstFocusable(status)) out.push({ id: 'statusbar', el: status });
  const title = root.querySelector<HTMLElement>('.kiln-titlebar');
  if (title) out.push({ id: 'titlebar', el: title });
  const ribbon = root.querySelector<HTMLElement>('.kiln-ribbon');
  if (ribbon && visible(ribbon)) out.push({ id: 'ribbon', el: ribbon });
  return out;
}

/** Moves the keys to a region. */
export function focusRegion(region: { id: RegionId; el: HTMLElement }): void {
  if (region.id === 'ribbon') {
    const tab =
      region.el.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]') ?? firstFocusable(region.el);
    tab?.focus();
    return;
  }
  if (region.id === 'document') {
    // The Accessible text view, when it is on, else the editor's own caret.
    const view = region.el.querySelector<HTMLElement>(
      '[data-kiln-text-view-focus], .kiln-textview td[tabindex="0"]',
    );
    if (view) {
      view.focus();
      return;
    }
    window.dispatchEvent(new CustomEvent('kiln:return-to-document'));
    if (!region.el.contains(document.activeElement)) firstFocusable(region.el)?.focus();
    return;
  }
  firstFocusable(region.el)?.focus();
}

/** Where the keys are now. */
function currentIndex(regions: readonly { el: HTMLElement }[]): number {
  const a = document.activeElement;
  // The innermost region holding the keys (a pane docked inside the document area).
  let best = -1;
  regions.forEach((r, i) => {
    if (r.el.contains(a) && (best < 0 || (regions[best] as { el: HTMLElement }).el.contains(r.el))) best = i;
  });
  return best;
}

/** The F6 handler of a frame: `backwards` for Shift+F6. Returns false when nothing moved. */
export function cycleRegions(root: HTMLElement, backwards: boolean): boolean {
  const regions = regionsOf(root);
  if (regions.length === 0) return false;
  const at = currentIndex(regions);
  const n = regions.length;
  const next = at < 0 ? (backwards ? n - 1 : 0) : (at + (backwards ? n - 1 : 1)) % n;
  const target = regions[next];
  if (!target) return false;
  focusRegion(target);
  return true;
}

/** Installs F6 / Shift+F6 on a frame (not while a dialog or the File menu holds the keys). */
export function useRegionCycle(rootRef: RefObject<HTMLElement | null>, enabled = true): void {
  useEffect(() => {
    if (!enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'F6' || e.ctrlKey || e.altKey || e.metaKey || e.defaultPrevented) return;
      const root = rootRef.current;
      if (!root) return;
      if (document.querySelector('[role="dialog"][aria-modal="true"], [role="alertdialog"]')) return;
      if (cycleRegions(root, e.shiftKey)) e.preventDefault();
    };
    // Before the focused control: F6 leaves any region, even one that keeps its other keys.
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [rootRef, enabled]);
}
