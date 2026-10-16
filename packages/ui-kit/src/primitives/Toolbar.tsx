import { type ComponentPropsWithRef, type KeyboardEvent, useCallback, useLayoutEffect, useRef } from 'react';
import { cx } from './cx.ts';

const FOCUSABLE =
  'button, input, select, textarea, a[href], [role="slider"], [role="combobox"], [role="spinbutton"], [data-toolbar-item]';

export interface ToolbarProps extends ComponentPropsWithRef<'div'> {
  /** Accessible name, e.g. "Font". Required by WAI-ARIA when more than one toolbar is present. */
  'aria-label'?: string;
  orientation?: 'horizontal' | 'vertical';
  /** Wrap from the last item to the first. Default true. */
  loop?: boolean;
}

function isTextEntry(el: Element): boolean {
  if (el instanceof HTMLTextAreaElement) return true;
  if (!(el instanceof HTMLInputElement)) return false;
  return !['button', 'checkbox', 'radio', 'range', 'color', 'submit', 'reset'].includes(el.type);
}

/**
 * WAI-ARIA toolbar with a roving tabindex: the toolbar is one Tab stop; ←/→ (↑/↓ when vertical) move
 * between items, Home/End jump to the ends, focus is remembered. Text fields keep their arrow keys,
 * and keys already handled by an item (sliders, open menus) are left alone. RTL flips ←/→.
 * Opt an element out with `data-toolbar-skip` (on it or an ancestor inside the toolbar).
 */
export function Toolbar({
  orientation = 'horizontal',
  loop = true,
  className,
  children,
  onKeyDown,
  onFocus,
  ref,
  ...rest
}: ToolbarProps) {
  const own = useRef<HTMLDivElement>(null);
  const active = useRef(0);

  const items = useCallback((): HTMLElement[] => {
    const root = own.current;
    if (!root) return [];
    return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
      (el) =>
        el.closest('[role="toolbar"]') === root &&
        !el.closest('[data-toolbar-skip]') &&
        !(el as HTMLButtonElement).disabled &&
        el.getAttribute('aria-hidden') !== 'true',
    );
  }, []);

  const sync = useCallback(() => {
    const list = items();
    if (list.length === 0) return;
    if (active.current >= list.length) active.current = list.length - 1;
    list.forEach((el, i) => {
      el.tabIndex = i === active.current ? 0 : -1;
    });
  }, [items]);

  // Re-apply after every render: items may have been added, removed or disabled.
  useLayoutEffect(sync);

  const focusAt = (i: number) => {
    const list = items();
    if (list.length === 0) return;
    const n = list.length;
    const idx = loop ? (i + n) % n : Math.max(0, Math.min(n - 1, i));
    active.current = idx;
    sync();
    list[idx]?.focus();
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    onKeyDown?.(e);
    if (e.defaultPrevented || e.altKey || e.ctrlKey || e.metaKey) return;
    const target = e.target as HTMLElement;
    const list = items();
    const index = list.indexOf(target);
    if (index < 0) return;
    const rtl = getComputedStyle(e.currentTarget).direction === 'rtl';
    const horizontal = orientation === 'horizontal';
    const textEntry = isTextEntry(target);
    let next: number | null = null;
    if (horizontal && !textEntry && e.key === 'ArrowRight') next = index + (rtl ? -1 : 1);
    else if (horizontal && !textEntry && e.key === 'ArrowLeft') next = index + (rtl ? 1 : -1);
    else if (!horizontal && e.key === 'ArrowDown') next = index + 1;
    else if (!horizontal && e.key === 'ArrowUp') next = index - 1;
    else if (!textEntry && e.key === 'Home') next = 0;
    else if (!textEntry && e.key === 'End') next = list.length - 1;
    if (next === null) return;
    e.preventDefault();
    if (!loop && (next < 0 || next >= list.length)) return;
    focusAt(next);
  };

  return (
    <div
      ref={(node) => {
        own.current = node;
        if (typeof ref === 'function') ref(node);
        else if (ref) ref.current = node;
      }}
      role="toolbar"
      aria-orientation={orientation}
      className={cx('kiln-toolbar', `kiln-toolbar--${orientation}`, className)}
      onKeyDown={handleKeyDown}
      onFocus={(e) => {
        onFocus?.(e);
        const i = items().indexOf(e.target as HTMLElement);
        if (i >= 0 && i !== active.current) {
          active.current = i;
          sync();
        }
      }}
      {...rest}
    >
      {children}
    </div>
  );
}
