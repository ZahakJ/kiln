import * as RP from '@radix-ui/react-popover';
import { ChevronDown, ChevronUp } from 'lucide-react';
import {
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useId,
  useRef,
  useState,
} from 'react';
import { Icon } from '../icons/Icon.tsx';
import { cx } from './cx.ts';

export interface GalleryItem {
  id: string;
  /** Accessible name and tooltip ("Heading 1"). */
  label: string;
  /** Visual preview (e.g. "AaBbCcDd" in the style's font). Defaults to the label. */
  preview?: ReactNode;
  disabled?: boolean;
}

export interface GalleryProps {
  /** Accessible name, e.g. "Styles". */
  label: string;
  items: readonly GalleryItem[];
  /** Selected item id (shown with the accent outline). */
  value?: string;
  onSelect: (id: string) => void;
  /** Office Live Preview: called with the hovered/focused id, and `null` when it ends. */
  onPreview?: (id: string | null) => void;
  /** Items per in-ribbon row. Default 5. */
  columns?: number;
  /** Columns of the expanded panel. Default `columns`. */
  expandedColumns?: number;
  itemWidth?: number;
  itemHeight?: number;
  /** Extra commands under the expanded grid (MenuItem-like buttons: "Create a Style"…). */
  footer?: ReactNode;
  className?: string;
}

/**
 * Office in-ribbon gallery: one row of items with ▲/▼ row scrolling and a ▾ "More" button that expands
 * the full grid in a panel. Items are `option`s of a `listbox` with a roving tabindex: ←/→ move,
 * ↑/↓ change row, Home/End, Enter/Space apply; hovering/focusing previews via `onPreview`.
 */
export function Gallery({
  label,
  items,
  value,
  onSelect,
  onPreview,
  columns = 5,
  expandedColumns,
  itemWidth = 72,
  itemHeight = 56,
  footer,
  className,
}: GalleryProps) {
  const id = useId();
  const rowCount = Math.max(1, Math.ceil(items.length / columns));
  const selectedIndex = Math.max(
    0,
    items.findIndex((i) => i.id === value),
  );
  const [row, setRow] = useState(Math.floor(selectedIndex / columns));
  const [focusIndex, setFocusIndex] = useState(selectedIndex);
  const [expanded, setExpanded] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  // Keep the selected item's row visible when the value changes from outside.
  useEffect(() => {
    setRow(
      Math.floor(
        Math.max(
          0,
          items.findIndex((i) => i.id === value),
        ) / columns,
      ),
    );
  }, [value, items, columns]);

  const visible = items.slice(row * columns, row * columns + columns);
  const style = {
    ['--gallery-item-w' as string]: `${itemWidth}px`,
    ['--gallery-item-h' as string]: `${itemHeight}px`,
  } as CSSProperties;

  const apply = (item: GalleryItem) => {
    if (item.disabled) return;
    onPreview?.(null);
    onSelect(item.id);
  };

  const focusItem = (container: HTMLElement | null, index: number) => {
    container?.querySelector<HTMLElement>(`[data-index="${index}"]`)?.focus();
  };

  const navigate = (
    e: KeyboardEvent<HTMLElement>,
    cols: number,
    container: HTMLElement | null,
    inline: boolean,
  ) => {
    const current = Number((e.target as HTMLElement).dataset.index);
    if (Number.isNaN(current)) return;
    if (inline && e.altKey && e.key === 'ArrowDown') {
      e.preventDefault();
      setExpanded(true);
      return;
    }
    const rtl = getComputedStyle(e.currentTarget).direction === 'rtl';
    let next = current;
    switch (e.key) {
      case 'ArrowRight':
        next = current + (rtl ? -1 : 1);
        break;
      case 'ArrowLeft':
        next = current + (rtl ? 1 : -1);
        break;
      case 'ArrowDown':
        next = current + cols;
        break;
      case 'ArrowUp':
        next = current - cols;
        break;
      case 'Home':
        next = 0;
        break;
      case 'End':
        next = items.length - 1;
        break;
      case 'Enter':
      case ' ': {
        e.preventDefault();
        const item = items[current];
        if (item) apply(item);
        if (!inline) setExpanded(false);
        return;
      }
      default:
        return;
    }
    e.preventDefault();
    e.stopPropagation();
    next = Math.max(0, Math.min(items.length - 1, next));
    setFocusIndex(next);
    if (inline) {
      const r = Math.floor(next / columns);
      if (r !== row) setRow(r);
      requestAnimationFrame(() => focusItem(container, next));
    } else focusItem(container, next);
    const item = items[next];
    if (item) onPreview?.(item.id);
  };

  const renderItem = (item: GalleryItem, index: number, inline: boolean) => (
    <div
      key={item.id}
      id={`${id}-${inline ? 'i' : 'x'}-${index}`}
      role="option"
      aria-selected={item.id === value}
      aria-disabled={item.disabled || undefined}
      tabIndex={index === focusIndex ? 0 : -1}
      data-index={index}
      title={item.label}
      className="of-gallery__item"
      onClick={() => {
        apply(item);
        if (!inline) setExpanded(false);
      }}
      onKeyDown={(e) =>
        navigate(
          e,
          inline ? columns : (expandedColumns ?? columns),
          inline ? listRef.current : panelRef.current,
          inline,
        )
      }
      onFocus={() => {
        setFocusIndex(index);
        onPreview?.(item.id);
      }}
      onMouseEnter={() => onPreview?.(item.id)}
      onMouseLeave={() => onPreview?.(null)}
      onBlur={() => onPreview?.(null)}
    >
      <span className="of-gallery__preview">{item.preview ?? item.label}</span>
    </div>
  );

  return (
    <RP.Root open={expanded} onOpenChange={setExpanded}>
      <RP.Anchor asChild>
        <div className={cx('of-gallery', className)} style={style}>
          <div
            ref={listRef}
            role="listbox"
            aria-label={label}
            aria-orientation="horizontal"
            className="of-gallery__row"
          >
            {visible.map((item, i) => renderItem(item, row * columns + i, true))}
          </div>
          <div className="of-gallery__scroller">
            <button
              type="button"
              tabIndex={-1}
              data-toolbar-skip=""
              aria-label={`Previous row of ${label}`}
              disabled={row === 0}
              onClick={() => setRow((r) => Math.max(0, r - 1))}
            >
              <Icon icon={ChevronUp} size={12} />
            </button>
            <button
              type="button"
              tabIndex={-1}
              data-toolbar-skip=""
              aria-label={`Next row of ${label}`}
              disabled={row >= rowCount - 1}
              onClick={() => setRow((r) => Math.min(rowCount - 1, r + 1))}
            >
              <Icon icon={ChevronDown} size={12} />
            </button>
            <RP.Trigger asChild>
              <button type="button" aria-label={`More ${label}`} className="of-gallery__more">
                <Icon icon={ChevronDown} size={12} />
                <span className="of-gallery__more-bar" aria-hidden="true" />
              </button>
            </RP.Trigger>
          </div>
        </div>
      </RP.Anchor>
      <RP.Portal>
        <RP.Content
          className="of-popover of-popover--flush of-gallery__panel"
          align="start"
          sideOffset={-1}
          collisionPadding={8}
          style={style}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            requestAnimationFrame(() => focusItem(panelRef.current, focusIndex));
          }}
        >
          <div
            ref={panelRef}
            role="listbox"
            aria-label={label}
            className="of-gallery__grid"
            style={{ gridTemplateColumns: `repeat(${expandedColumns ?? columns}, var(--gallery-item-w))` }}
          >
            {items.map((item, i) => renderItem(item, i, false))}
          </div>
          {footer ? <div className="of-gallery__footer">{footer}</div> : null}
        </RP.Content>
      </RP.Portal>
    </RP.Root>
  );
}
