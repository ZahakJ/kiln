import { ChevronDown, ChevronUp } from 'lucide-react';
import { type KeyboardEvent, useEffect, useId, useRef, useState } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { cx } from './cx.ts';
import { formatNumber, type LengthUnit, parseLength } from './units.ts';

export interface NumberInputProps {
  value: number;
  onValueChange: (value: number) => void;
  /** Accessible name. */
  'aria-label'?: string;
  'aria-labelledby'?: string;
  /** Unit of `value`, shown after the number (`1.27 cm`). Typed values in other units are converted. */
  unit?: LengthUnit | '%' | '°' | '';
  min?: number;
  max?: number;
  /** Arrow-key / spin step. Default 1 (0.1 for cm and in). */
  step?: number;
  /** Decimals kept. Default 2. */
  precision?: number;
  disabled?: boolean;
  width?: number | string;
  size?: 'sm' | 'md';
  className?: string;
  id?: string;
}

const LENGTH_UNITS = new Set(['pt', 'px', 'cm', 'mm', 'in']);

/**
 * Spin box (`role="spinbutton"`): ↑/↓ step, PageUp/PageDown ×10, Home/End to min/max, Enter commits,
 * Esc reverts. Accepts typed units ("2 cm" in an inch field becomes 0.79 in), clamps to [min, max].
 */
export function NumberInput({
  value,
  onValueChange,
  unit = '',
  min = Number.NEGATIVE_INFINITY,
  max = Number.POSITIVE_INFINITY,
  step,
  precision = 2,
  disabled,
  width,
  size = 'md',
  className,
  id,
  ...aria
}: NumberInputProps) {
  const effectiveStep = step ?? (unit === 'cm' || unit === 'in' ? 0.1 : 1);
  const format = (v: number) => (unit ? `${formatNumber(v, precision)} ${unit}` : formatNumber(v, precision));
  const [text, setText] = useState(format(value));
  const [invalid, setInvalid] = useState(false);
  const editing = useRef(false);
  const inputId = useId();

  useEffect(() => {
    if (!editing.current) setText(format(value));
  }, [value, unit, precision]);

  const clamp = (v: number) => Math.min(max, Math.max(min, v));
  const round = (v: number) => Number(formatNumber(v, precision));

  const set = (v: number) => {
    const next = round(clamp(v));
    editing.current = false;
    setInvalid(false);
    setText(format(next));
    if (next !== value) onValueChange(next);
  };

  const parse = (raw: string): number | null => {
    const t = raw.trim().replace(/%$|°$/, '');
    if (LENGTH_UNITS.has(unit)) return parseLength(t, unit as LengthUnit);
    return parseLength(t.replace(/[a-z]+$/i, ''), 'pt');
  };

  const commit = () => {
    const parsed = parse(text);
    if (parsed === null) {
      setInvalid(true);
      return false;
    }
    set(parsed);
    return true;
  };

  const stepBy = (delta: number) => {
    const base = editing.current ? (parse(text) ?? value) : value;
    // Snap to the step grid like Office: 1.13 + 0.1 → 1.2, 1.13 − 0.1 → 1.1.
    const s = effectiveStep;
    const onGrid = Math.abs(Math.round(base / s) * s - base) < 1e-9;
    const next = onGrid
      ? base + delta * s
      : delta > 0
        ? Math.ceil(base / s) * s + (delta - 1) * s
        : Math.floor(base / s) * s + (delta + 1) * s;
    set(next);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    switch (e.key) {
      case 'ArrowUp':
        e.preventDefault();
        stepBy(1);
        break;
      case 'ArrowDown':
        e.preventDefault();
        stepBy(-1);
        break;
      case 'PageUp':
        e.preventDefault();
        stepBy(10);
        break;
      case 'PageDown':
        e.preventDefault();
        stepBy(-10);
        break;
      case 'Home':
        if (Number.isFinite(min)) {
          e.preventDefault();
          set(min);
        }
        break;
      case 'End':
        if (Number.isFinite(max)) {
          e.preventDefault();
          set(max);
        }
        break;
      case 'Enter':
        if (!commit()) e.preventDefault();
        break;
      case 'Escape':
        if (editing.current) {
          e.preventDefault();
          e.stopPropagation();
          editing.current = false;
          setInvalid(false);
          setText(format(value));
        }
        break;
      default:
    }
  };

  return (
    <div
      className={cx(
        'kiln-field',
        'kiln-number',
        `kiln-field--${size}`,
        invalid && 'is-invalid',
        disabled && 'is-disabled',
        className,
      )}
      style={width !== undefined ? { width } : undefined}
    >
      <input
        id={id ?? inputId}
        className="kiln-number__input"
        role="spinbutton"
        inputMode="decimal"
        autoComplete="off"
        spellCheck={false}
        aria-label={aria['aria-label']}
        aria-labelledby={aria['aria-labelledby']}
        aria-valuenow={value}
        aria-valuemin={Number.isFinite(min) ? min : undefined}
        aria-valuemax={Number.isFinite(max) ? max : undefined}
        aria-valuetext={format(value)}
        aria-invalid={invalid || undefined}
        disabled={disabled}
        value={text}
        onChange={(e) => {
          editing.current = true;
          setInvalid(false);
          setText(e.target.value);
        }}
        onKeyDown={onKeyDown}
        onFocus={(e) => e.target.select()}
        onBlur={() => {
          if (editing.current && !commit()) {
            editing.current = false;
            setInvalid(false);
            setText(format(value));
          }
        }}
      />
      <span className="kiln-number__spin" aria-hidden="true">
        <button
          type="button"
          tabIndex={-1}
          disabled={disabled || value >= max}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => stepBy(1)}
        >
          <Icon icon={ChevronUp} size={12} />
        </button>
        <button
          type="button"
          tabIndex={-1}
          disabled={disabled || value <= min}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => stepBy(-1)}
        >
          <Icon icon={ChevronDown} size={12} />
        </button>
      </span>
    </div>
  );
}
