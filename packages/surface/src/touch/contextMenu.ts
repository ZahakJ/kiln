/**
 * The long-press context menu: a finger held on the document opens the same menu as
 * a right-click there, by sending the element the `contextmenu` event a mouse would.
 */

/**
 * Surfaces that open their own menu on a long press carry this attribute: the browser's own
 * long-press `contextmenu` (Chrome on Android sends one) is dropped there, so the menu opens once,
 * after the surface has selected what is under the finger.
 */
export const OWN_LONG_PRESS = 'data-kiln-long-press';

let installed = false;

function install(): void {
  if (installed || typeof window === 'undefined') return;
  installed = true;
  window.addEventListener(
    'contextmenu',
    (e) => {
      const type = (e as PointerEvent).pointerType;
      if (!e.isTrusted || (type !== 'touch' && type !== 'pen')) return;
      const target = e.target instanceof Element ? e.target : null;
      if (!target?.closest(`[${OWN_LONG_PRESS}]`)) return;
      // A finger's menu on a surface with its own long press: the surface opens it.
      e.preventDefault();
      e.stopImmediatePropagation();
    },
    { capture: true },
  );
}

/** Marks `el` as a surface with its own long-press menu (see {@link OWN_LONG_PRESS}). */
export function ownLongPress(el: HTMLElement): () => void {
  install();
  el.setAttribute(OWN_LONG_PRESS, '');
  return () => el.removeAttribute(OWN_LONG_PRESS);
}

/** Opens the context menu of `target` at the client point (`x`, `y`), as a right-click there does. */
export function openContextMenuAt(target: Element, x: number, y: number): void {
  target.dispatchEvent(
    new MouseEvent('contextmenu', {
      bubbles: true,
      cancelable: true,
      composed: true,
      clientX: x,
      clientY: y,
      button: 2,
      buttons: 0,
      view: typeof window !== 'undefined' ? window : null,
    }),
  );
}

/**
 * A long press opened a menu under the finger: its lift must not choose the item it lands on.
 * The release (`pointerup`, `click`) is dropped until the finger is up, and a moment after.
 */
export function swallowRelease(): void {
  if (typeof window === 'undefined') return;
  const stop = (e: Event) => {
    if (e instanceof PointerEvent && e.pointerType === 'mouse') return;
    e.preventDefault();
    e.stopImmediatePropagation();
  };
  let timer: ReturnType<typeof setTimeout> | null = null;
  const remove = () => {
    if (timer) clearTimeout(timer);
    window.removeEventListener('pointerup', stop, true);
    window.removeEventListener('click', stop, true);
    window.removeEventListener('touchend', lifted, true);
    window.removeEventListener('touchcancel', lifted, true);
  };
  const lifted = (e: TouchEvent) => {
    if (e.touches.length > 0) return;
    if (timer) clearTimeout(timer);
    // The click that follows the lift comes after it.
    timer = setTimeout(remove, 350);
  };
  window.addEventListener('pointerup', stop, true);
  window.addEventListener('click', stop, true);
  window.addEventListener('touchend', lifted, true);
  window.addEventListener('touchcancel', lifted, true);
  timer = setTimeout(remove, 4000);
}
