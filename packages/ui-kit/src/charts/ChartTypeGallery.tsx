import { type KeyboardEvent, useId, useRef, useState } from 'react';
import { cx } from '../primitives/cx.ts';
import { Tooltip } from '../primitives/Tooltip.tsx';
import { ChartThumb } from './ChartThumb.tsx';
import { CHART_FAMILIES, CHART_KIND_HINTS, CHART_KIND_LABELS, type ChartFamily } from './kinds.ts';
import type { ChartKind } from './types.ts';

export interface ChartTypeGalleryProps {
  /** Families to show (Insert ▸ Column/Bar shows two); all by default. */
  families?: readonly ChartFamily[];
  /** The current type (outlined). */
  value?: ChartKind | null;
  onSelect: (kind: ChartKind) => void;
  /** Live preview while hovering or focusing (Office): the kind, then `null` when it ends. */
  onPreview?: (kind: ChartKind | null) => void;
  /** Thumbnail width in px. Default 56. */
  thumbSize?: number;
  /** Accessible name. Default "Chart types". */
  label?: string;
  className?: string;
}

/**
 * The chart type picker of Insert ▸ Charts and Change Chart Type: one labelled row of thumbnails per
 * family. A single `listbox` with a roving tab stop: ←/→ move within a row, ↑/↓ move between rows
 * (keeping the column), Home/End, Enter/Space choose.
 */
export function ChartTypeGallery({
  families = CHART_FAMILIES,
  value,
  onSelect,
  onPreview,
  thumbSize = 56,
  label = 'Chart types',
  className,
}: ChartTypeGalleryProps) {
  const id = useId();
  const flat = families.flatMap((f, row) => f.kinds.map((kind, col) => ({ kind, row, col })));
  const initial = Math.max(
    0,
    flat.findIndex((e) => e.kind === value),
  );
  const [focus, setFocus] = useState(initial);
  const refs = useRef<(HTMLDivElement | null)[]>([]);

  const move = (to: number) => {
    const next = Math.min(flat.length - 1, Math.max(0, to));
    setFocus(next);
    refs.current[next]?.focus();
    const entry = flat[next];
    if (entry) onPreview?.(entry.kind);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const cur = flat[focus];
    if (!cur) return;
    const inRow = (row: number) => flat.filter((x) => x.row === row);
    switch (e.key) {
      case 'ArrowRight':
        move(focus + 1);
        break;
      case 'ArrowLeft':
        move(focus - 1);
        break;
      case 'ArrowDown':
      case 'ArrowUp': {
        const row = cur.row + (e.key === 'ArrowDown' ? 1 : -1);
        const target = inRow(row);
        if (target.length === 0) return;
        const pick = target[Math.min(cur.col, target.length - 1)];
        if (pick) move(flat.indexOf(pick));
        break;
      }
      case 'Home':
        move(0);
        break;
      case 'End':
        move(flat.length - 1);
        break;
      case 'Enter':
      case ' ':
        onSelect(cur.kind);
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  return (
    <div
      role="listbox"
      aria-label={label}
      tabIndex={-1}
      className={cx('kiln-chart-gallery', className)}
      onKeyDown={onKeyDown}
      onMouseLeave={() => onPreview?.(null)}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) onPreview?.(null);
      }}
    >
      {families.map((f, row) => (
        // biome-ignore lint/a11y/useSemanticElements: an option group of the listbox.
        <div key={f.id} role="group" aria-labelledby={`${id}-${f.id}`} className="kiln-chart-gallery__family">
          <div id={`${id}-${f.id}`} className="kiln-chart-gallery__heading">
            {f.label}
          </div>
          <div className="kiln-chart-gallery__row">
            {f.kinds.map((kind) => {
              const index = flat.findIndex((e) => e.kind === kind && e.row === row);
              const selected = kind === value;
              return (
                <Tooltip key={kind} content={CHART_KIND_LABELS[kind]} description={CHART_KIND_HINTS[kind]}>
                  <div
                    ref={(el) => {
                      refs.current[index] = el;
                    }}
                    role="option"
                    aria-selected={selected}
                    aria-label={CHART_KIND_LABELS[kind]}
                    tabIndex={index === focus ? 0 : -1}
                    data-kind={kind}
                    className={cx('kiln-chart-gallery__item', selected && 'is-selected')}
                    onClick={() => onSelect(kind)}
                    onKeyDown={() => {}}
                    onMouseEnter={() => onPreview?.(kind)}
                    onFocus={() => {
                      setFocus(index);
                      onPreview?.(kind);
                    }}
                  >
                    <ChartThumb kind={kind} size={thumbSize} />
                  </div>
                </Tooltip>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );
}
