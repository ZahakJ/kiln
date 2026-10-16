import { Minus, Plus } from 'lucide-react';
import { type KeyboardEvent, type PointerEvent, useRef } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { cx } from './cx.ts';

export interface SliderProps {
  value: number;
  onValueChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  /** Larger PageUp/PageDown step. Default 10 × step. */
  pageStep?: number;
  'aria-label'?: string;
  'aria-labelledby'?: string;
  /** Human-readable value for assistive tech, e.g. `120%`. */
  formatValue?: (value: number) => string;
  /** Maps between value and track position 0–1 (non-linear sliders such as zoom). */
  toPosition?: (value: number) => number;
  fromPosition?: (position: number) => number;
  /** Tick mark positions (values) drawn on the track, e.g. 100 % on the zoom slider. */
  marks?: number[];
  disabled?: boolean;
  width?: number | string;
  className?: string;
}

/**
 * Horizontal slider (`role="slider"`): drag, click the track, ←/→/↑/↓ step, PageUp/PageDown, Home/End.
 * Supports a non-linear value↔position mapping (the Office zoom slider is 10–100 % on the left half and
 * 100–500 % on the right).
 */
export function Slider({
  value,
  onValueChange,
  min = 0,
  max = 100,
  step = 1,
  pageStep,
  formatValue = String,
  toPosition,
  fromPosition,
  marks,
  disabled,
  width,
  className,
  ...aria
}: SliderProps) {
  const trackRef = useRef<HTMLDivElement>(null);
  const pos = (v: number) => (toPosition ? toPosition(v) : (v - min) / (max - min || 1));
  const fromPos = (p: number) => (fromPosition ? fromPosition(p) : min + p * (max - min));
  const snap = (v: number) => {
    const s = Math.round((v - min) / step) * step + min;
    return Math.min(max, Math.max(min, Number(s.toFixed(6))));
  };
  const commit = (v: number) => {
    const next = snap(v);
    if (next !== value) onValueChange(next);
  };

  const valueAtClientX = (clientX: number) => {
    const rect = trackRef.current?.getBoundingClientRect();
    if (!rect || rect.width === 0) return value;
    const rtl = trackRef.current ? getComputedStyle(trackRef.current).direction === 'rtl' : false;
    let p = (clientX - rect.left) / rect.width;
    if (rtl) p = 1 - p;
    return fromPos(Math.min(1, Math.max(0, p)));
  };

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (disabled || e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget.querySelector('[role="slider"]') as HTMLElement | null)?.focus();
    e.currentTarget.setPointerCapture?.(e.pointerId);
    commit(valueAtClientX(e.clientX));
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (disabled || !e.currentTarget.hasPointerCapture?.(e.pointerId)) return;
    commit(valueAtClientX(e.clientX));
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (disabled) return;
    const big = pageStep ?? step * 10;
    const rtl = getComputedStyle(e.currentTarget).direction === 'rtl';
    const map: Record<string, number | 'min' | 'max'> = {
      ArrowRight: rtl ? -step : step,
      ArrowLeft: rtl ? step : -step,
      ArrowUp: step,
      ArrowDown: -step,
      PageUp: big,
      PageDown: -big,
      Home: 'min',
      End: 'max',
    };
    const delta = map[e.key];
    if (delta === undefined) return;
    e.preventDefault();
    if (delta === 'min') commit(min);
    else if (delta === 'max') commit(max);
    else commit(value + delta);
  };

  const p = Math.min(1, Math.max(0, pos(value)));
  return (
    <div
      ref={trackRef}
      className={cx('kiln-slider', disabled && 'is-disabled', className)}
      style={{ ['--p' as string]: p, ...(width !== undefined ? { width } : {}) }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
    >
      <div className="kiln-slider__track" aria-hidden="true">
        <div className="kiln-slider__fill" style={{ inlineSize: `${p * 100}%` }} />
        {marks?.map((m) => (
          <span key={m} className="kiln-slider__mark" style={{ insetInlineStart: `${pos(m) * 100}%` }} />
        ))}
      </div>
      <div
        role="slider"
        tabIndex={disabled ? -1 : 0}
        className="kiln-slider__thumb"
        aria-label={aria['aria-label']}
        aria-labelledby={aria['aria-labelledby']}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={value}
        aria-valuetext={formatValue(value)}
        aria-orientation="horizontal"
        aria-disabled={disabled || undefined}
        onKeyDown={onKeyDown}
      />
    </div>
  );
}

/** Office zoom mapping: 10–100 % on the left half of the track, 100–500 % on the right half. */
export const zoomToPosition = (zoom: number) =>
  zoom <= 100 ? ((Math.max(10, zoom) - 10) / 90) * 0.5 : 0.5 + ((Math.min(500, zoom) - 100) / 400) * 0.5;
export const positionToZoom = (p: number) => (p <= 0.5 ? 10 + (p / 0.5) * 90 : 100 + ((p - 0.5) / 0.5) * 400);

/** Office zoom steps for the − / + buttons. */
export const ZOOM_STEPS = [
  10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 175, 200, 250, 300, 400, 500,
];

export interface ZoomSliderProps {
  /** Percent, 10–500. */
  value: number;
  onValueChange: (value: number) => void;
  /** Clicking the percentage label (Office opens the Zoom dialog). */
  onLabelClick?: () => void;
  className?: string;
}

/** Status-bar zoom control: − button, slider with a 100 % detent, + button and the percentage. */
export function ZoomSlider({ value, onValueChange, onLabelClick, className }: ZoomSliderProps) {
  const prev = [...ZOOM_STEPS].reverse().find((z) => z < value) ?? 10;
  const next = ZOOM_STEPS.find((z) => z > value) ?? 500;
  return (
    <div className={cx('kiln-zoom', className)}>
      <button
        type="button"
        className="kiln-btn kiln-btn--ghost kiln-btn--sm kiln-btn--icon-only"
        aria-label="Zoom out"
        disabled={value <= 10}
        onClick={() => onValueChange(prev)}
      >
        <Icon icon={Minus} size={14} />
      </button>
      <Slider
        value={value}
        onValueChange={(v) => onValueChange(Math.abs(v - 100) < 4 ? 100 : Math.round(v))}
        min={10}
        max={500}
        step={1}
        pageStep={10}
        aria-label="Zoom"
        formatValue={(v) => `${v}%`}
        toPosition={zoomToPosition}
        fromPosition={positionToZoom}
        marks={[100]}
        width={104}
      />
      <button
        type="button"
        className="kiln-btn kiln-btn--ghost kiln-btn--sm kiln-btn--icon-only"
        aria-label="Zoom in"
        disabled={value >= 500}
        onClick={() => onValueChange(next)}
      >
        <Icon icon={Plus} size={14} />
      </button>
      <button
        type="button"
        className="kiln-btn kiln-btn--ghost kiln-btn--sm kiln-zoom__label"
        onClick={onLabelClick}
        aria-label={`Zoom level ${value}%`}
      >
        <span className="kiln-btn__label">{value}%</span>
      </button>
    </div>
  );
}
