/**
 * Office hands the keys back to the document after a command from a menu, popover or picker.
 * `armReturnToDocument` marks that such a command just ran; the menu or popover then closes
 * straight to the document (`kiln:return-to-document`) instead of returning focus to its
 * trigger, so even the very next keystroke types in the document.
 */
let armedAt = Number.NEGATIVE_INFINITY;

export function armReturnToDocument(): void {
  armedAt = typeof performance !== 'undefined' ? performance.now() : Date.now();
}

/** Wraps a Radix `onCloseAutoFocus`: closes to the document after a command. */
export function closeToDocument<E extends Event>(user?: (e: E) => void): (e: E) => void {
  return (e: E) => {
    user?.(e);
    if (e.defaultPrevented) return;
    const modal =
      typeof document !== 'undefined'
        ? document.querySelector('[role="dialog"]:not(.kiln-popover), [role="alertdialog"]')
        : null;
    // A dialog the command opened has taken the keys already: the menu must not return them to its
    // trigger (Radix's default) either. A menu inside a dialog, whose focus is still in the menu,
    // returns to its trigger as usual.
    if (modal?.contains(document.activeElement)) {
      e.preventDefault();
      return;
    }
    // A dialog still taking the keys: as before, Radix returns them to the trigger first.
    if (modal) return;
    const now = typeof performance !== 'undefined' ? performance.now() : Date.now();
    if (now - armedAt > 800) return;
    armedAt = Number.NEGATIVE_INFINITY;
    e.preventDefault();
    // The user has opened another menu, list or popover since (it has the focus): the keys stay
    // there, or the document taking them would close it.
    const a = typeof document !== 'undefined' ? (document.activeElement as HTMLElement | null) : null;
    const other = a?.closest('[role="menu"], [role="listbox"], .kiln-popover');
    if (other && !other.closest('.kiln-ribbon')) return;
    if (typeof window !== 'undefined') window.dispatchEvent(new CustomEvent('kiln:return-to-document'));
  };
}
