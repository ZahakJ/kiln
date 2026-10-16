import * as DM from '@radix-ui/react-dropdown-menu';
import {
  type CSSProperties,
  cloneElement,
  isValidElement,
  type KeyboardEvent,
  type MouseEvent,
  type ReactElement,
  type ReactNode,
  useState,
} from 'react';
import { createPortal } from 'react-dom';
import { MenuContent } from '../primitives/Menu.tsx';
import { Toolbar } from '../primitives/Toolbar.tsx';
import { CommandMenuItems, type MenuEntry } from './CommandMenuItems.tsx';
import '../minitoolbar/minitoolbar.css';
import './contextmenu.css';

export interface ContextMenuProps {
  /** The element that gets the menu (one element that accepts `onContextMenu`/`onKeyDown`). */
  children: ReactElement<{
    onContextMenu?: (e: MouseEvent) => void;
    onKeyDown?: (e: KeyboardEvent) => void;
  }>;
  /** Command entries; or pass `content` for free-form menu items. */
  entries?: readonly MenuEntry[];
  content?: ReactNode;
  /** Office's mini toolbar shown above the menu (font, size, bold… in Write and Deck). */
  miniToolbar?: ReactNode;
  /** Called before opening with the event target; return false to keep the browser's menu. */
  onBeforeOpen?: (target: EventTarget | null) => boolean | undefined;
  onOpenChange?: (open: boolean) => void;
  'aria-label'?: string;
}

const insideMini = (t: EventTarget | null) => t instanceof Element && !!t.closest('.kiln-contextmenu__mini');

interface Point {
  x: number;
  y: number;
}

/**
 * Right-click menu at the pointer (Shift+F10 / the Menu key at the focused element), built on the
 * Radix menu: arrow keys, typeahead, submenus, Esc, focus return. Optionally shows the mini toolbar.
 */
export function ContextMenu({
  children,
  entries,
  content,
  miniToolbar,
  onBeforeOpen,
  onOpenChange,
  'aria-label': ariaLabel = 'Context menu',
}: ContextMenuProps) {
  const [at, setAt] = useState<Point | null>(null);
  const open = at !== null;
  const setOpen = (o: boolean, p: Point | null = null) => {
    setAt(o ? p : null);
    onOpenChange?.(o);
  };

  if (!isValidElement(children)) return children;
  const child = cloneElement(children, {
    onContextMenu: (e: MouseEvent) => {
      children.props.onContextMenu?.(e);
      if (e.defaultPrevented) return;
      if (onBeforeOpen?.(e.target) === false) return;
      e.preventDefault();
      setOpen(true, { x: e.clientX, y: e.clientY });
    },
    onKeyDown: (e: KeyboardEvent) => {
      children.props.onKeyDown?.(e);
      if (e.defaultPrevented) return;
      if ((e.shiftKey && e.key === 'F10') || e.key === 'ContextMenu') {
        if (onBeforeOpen?.(e.target) === false) return;
        e.preventDefault();
        const r = (e.target as HTMLElement).getBoundingClientRect();
        setOpen(true, { x: r.left + Math.min(24, r.width / 2), y: r.top + Math.min(r.height, 24) });
      }
    },
  });

  const anchorStyle: CSSProperties = {
    position: 'fixed',
    left: at?.x ?? 0,
    top: at?.y ?? 0,
    width: 0,
    height: 0,
  };
  return (
    <>
      {child}
      {open && typeof document !== 'undefined'
        ? createPortal(
            <DM.Root open={open} onOpenChange={(o) => setOpen(o, at)} modal={!miniToolbar}>
              <DM.Trigger asChild>
                <span aria-hidden="true" style={anchorStyle} tabIndex={-1} />
              </DM.Trigger>
              <MenuContent
                aria-label={ariaLabel}
                side="bottom"
                align="start"
                sideOffset={2}
                onPointerDownOutside={(e) => {
                  if (insideMini(e.target)) e.preventDefault();
                }}
                onFocusOutside={(e) => {
                  if (insideMini(e.target)) e.preventDefault();
                }}
              >
                {content ?? (entries ? <CommandMenuItems entries={entries} /> : null)}
              </MenuContent>
              {miniToolbar ? (
                <div
                  className="kiln-contextmenu__mini"
                  style={{ left: at?.x ?? 0, top: Math.max(8, (at?.y ?? 0) - 52) }}
                  data-toolbar-skip
                >
                  <Toolbar aria-label="Formatting" className="kiln-minitoolbar__bar">
                    {miniToolbar}
                  </Toolbar>
                </div>
              ) : null}
            </DM.Root>,
            document.body,
          )
        : null}
    </>
  );
}
