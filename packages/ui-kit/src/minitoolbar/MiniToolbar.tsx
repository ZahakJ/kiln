import { t } from '@kiln/i18n';
import {
  type ReactNode,
  type RefObject,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import { createPortal } from 'react-dom';
import { useOfficeOptions } from '../chrome/options.ts';
import { cx } from '../primitives/cx.ts';
import { Toolbar } from '../primitives/Toolbar.tsx';
import './minitoolbar.css';

export interface AnchorRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface MiniToolbarProps {
  /** Selection rectangle in viewport coordinates; `null` hides the toolbar. */
  anchor: AnchorRect | null;
  onDismiss: () => void;
  children: ReactNode;
  'aria-label'?: string;
  /** Fade out as the pointer moves away, dismissing at `fadeDistance` px (Office behaviour). Default true. */
  fadeWithDistance?: boolean;
  fadeDistance?: number;
  className?: string;
  /** Viewport rectangles the toolbar must not cover (open task panes): it moves beside them, or
   * is dismissed when there is no room. */
  avoid?: () => readonly DOMRect[];
}

const GAP = 8;

/**
 * Office mini toolbar: floats above the selection after a mouse selection, fades as the pointer moves
 * away, and disappears on Esc, typing or scrolling. Its controls form a roving-focus toolbar.
 */
export function MiniToolbar({
  anchor,
  onDismiss,
  children,
  'aria-label': ariaLabel = t('Formatting'),
  fadeWithDistance = true,
  fadeDistance = 180,
  className,
  avoid,
}: MiniToolbarProps) {
  const [options] = useOfficeOptions();
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number; below: boolean } | null>(null);
  const [opacity, setOpacity] = useState(1);
  const hovering = useRef(false);

  useLayoutEffect(() => {
    if (!anchor) {
      setPos(null);
      return;
    }
    setOpacity(1);
    const el = ref.current;
    const w = el?.offsetWidth ?? 0;
    const h = el?.offsetHeight ?? 0;
    const vw = window.innerWidth;
    const below = anchor.top - h - GAP < 4;
    const top = below ? anchor.top + anchor.height + GAP : anchor.top - h - GAP;
    let left = Math.min(Math.max(4, anchor.left + anchor.width / 2 - w / 2), Math.max(4, vw - w - 4));
    // Clear of the panes: beside the pane that would be covered, else not at all.
    const hits = (l: number) =>
      (avoid?.() ?? []).find((r) => l < r.right && l + w > r.left && top < r.bottom && top + h > r.top);
    const hit = hits(left);
    if (hit) {
      const before = hit.left - w - 4;
      const after = hit.right + 4;
      if (before >= 4 && !hits(before)) left = before;
      else if (after + w <= vw - 4 && !hits(after)) left = after;
      else {
        setPos(null);
        onDismiss();
        return;
      }
    }
    setPos({ left, top, below });
  }, [anchor, avoid, onDismiss]);

  useEffect(() => {
    if (!anchor) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof Node && ref.current?.contains(e.target)) {
        if (e.key === 'Escape') onDismiss();
        return;
      }
      // Typing in the document hides it; modifier presses alone do not.
      if (!['Shift', 'Control', 'Alt', 'Meta'].includes(e.key)) onDismiss();
    };
    const onMove = (e: PointerEvent) => {
      if (!fadeWithDistance || hovering.current || !ref.current) return;
      const r = ref.current.getBoundingClientRect();
      const dx = Math.max(r.left - e.clientX, 0, e.clientX - r.right);
      const dy = Math.max(r.top - e.clientY, 0, e.clientY - r.bottom);
      const d = Math.hypot(dx, dy);
      const o = d < 24 ? 1 : Math.max(0, 1 - (d - 24) / (fadeDistance - 24));
      setOpacity(o);
      if (o === 0) onDismiss();
    };
    const onScroll = () => onDismiss();
    window.addEventListener('keydown', onKey, true);
    window.addEventListener('pointermove', onMove);
    window.addEventListener('scroll', onScroll, true);
    return () => {
      window.removeEventListener('keydown', onKey, true);
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('scroll', onScroll, true);
    };
  }, [anchor, onDismiss, fadeWithDistance, fadeDistance]);

  // File ▸ Options ▸ General ▸ Show Mini Toolbar on selection.
  if (!anchor || !options.general.showMiniToolbar || typeof document === 'undefined') return null;
  return createPortal(
    // biome-ignore lint/a11y/noStaticElementInteractions: preserves the selection; the controls inside are the interactive elements
    <div
      ref={ref}
      className={cx('kiln-minitoolbar', pos?.below && 'kiln-minitoolbar--below', className)}
      style={{
        left: pos?.left ?? anchor.left,
        top: pos?.top ?? anchor.top,
        opacity,
        visibility: pos ? 'visible' : 'hidden',
      }}
      onPointerEnter={() => {
        hovering.current = true;
        setOpacity(1);
      }}
      onPointerLeave={() => {
        hovering.current = false;
      }}
      // Keep the document selection when clicking the toolbar.
      onMouseDown={(e) => {
        if (!(e.target as HTMLElement).closest('input, select, textarea, [role="combobox"]'))
          e.preventDefault();
      }}
    >
      <Toolbar aria-label={ariaLabel} className="kiln-minitoolbar__bar">
        {children}
      </Toolbar>
    </div>,
    document.body,
  );
}

export interface SelectionMiniToolbar {
  anchor: AnchorRect | null;
  dismiss: () => void;
}

/**
 * Shows the mini toolbar after a mouse selection inside `containerRef` (mouse-up with a non-empty
 * selection), anchored to the selection's bounding box.
 */
export function useSelectionMiniToolbar(
  containerRef: RefObject<HTMLElement | null>,
  enabled = true,
): SelectionMiniToolbar {
  const [anchor, setAnchor] = useState<AnchorRect | null>(null);
  const dismiss = useCallback(() => setAnchor(null), []);
  useEffect(() => {
    const el = containerRef.current;
    if (!el || !enabled) return;
    const onUp = (e: MouseEvent) => {
      if (e.button !== 0) return;
      // Wait for the browser to finalise the selection.
      requestAnimationFrame(() => {
        const sel = window.getSelection();
        if (!sel || sel.isCollapsed || sel.rangeCount === 0) return setAnchor(null);
        const range = sel.getRangeAt(0);
        if (!el.contains(range.commonAncestorContainer)) return setAnchor(null);
        const r = range.getBoundingClientRect();
        if (r.width === 0 && r.height === 0) return setAnchor(null);
        setAnchor({ left: r.left, top: r.top, width: r.width, height: r.height });
      });
    };
    const onDown = () => setAnchor(null);
    el.addEventListener('mouseup', onUp);
    el.addEventListener('mousedown', onDown);
    return () => {
      el.removeEventListener('mouseup', onUp);
      el.removeEventListener('mousedown', onDown);
    };
  }, [containerRef, enabled]);
  return { anchor, dismiss };
}
