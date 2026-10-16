import { Ban, Check, Palette } from 'lucide-react';
import { type KeyboardEvent, type ReactNode, useId, useRef, useState, useSyncExternalStore } from 'react';
import { HexColorInput, HexColorPicker } from 'react-colorful';
import { Icon } from '../icons/Icon.tsx';
import {
  addRecentColor,
  applyTint,
  type ColorChoice,
  getRecentColors,
  normalizeHex,
  OFFICE_THEME_COLORS,
  STANDARD_COLORS,
  subscribeRecentColors,
  type ThemeColorDef,
  themeVariants,
  tintLabel,
} from './color.ts';
import { cx } from './cx.ts';

export interface ColorPickerProps {
  /** Current colour; used to mark the matching swatch. */
  value?: ColorChoice;
  onChange: (choice: ColorChoice) => void;
  /** Document theme colours (10 slots). Defaults to the Office theme. */
  themeColors?: readonly ThemeColorDef[];
  /** Show "Automatic" (font colour). */
  allowAuto?: boolean;
  /** Show "No Color" (highlight, fill). */
  allowNone?: boolean;
  autoLabel?: string;
  noneLabel?: string;
  /** Extra footer items (e.g. "Gradient ▸"). */
  footer?: ReactNode;
  className?: string;
}

interface Swatch {
  key: string;
  hex: string;
  label: string;
  choice: ColorChoice;
}

const sameColor = (a: ColorChoice | undefined, b: ColorChoice) => {
  if (!a || a.type !== b.type) return false;
  if (a.type === 'theme' && b.type === 'theme') return a.slot === b.slot && Math.abs(a.tint - b.tint) < 1e-6;
  if (a.type === 'rgb' && b.type === 'rgb') return a.hex.toUpperCase() === b.hex.toUpperCase();
  return a.type === b.type;
};

/**
 * Office-style colour panel: Automatic / No Color, Theme Colors (10 × 6 with tints and shades),
 * Standard Colors, Recent Colors and More Colors (custom picker). Swatches form one 2-D grid navigated
 * with the arrow keys (Home/End per row); Enter/Space applies.
 */
export function ColorPicker({
  value,
  onChange,
  themeColors = OFFICE_THEME_COLORS,
  allowAuto = false,
  allowNone = false,
  autoLabel = 'Automatic',
  noneLabel = 'No Color',
  footer,
  className,
}: ColorPickerProps) {
  const recent = useSyncExternalStore(subscribeRecentColors, getRecentColors, getRecentColors);
  const [custom, setCustom] = useState<string | null>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const titleId = useId();

  const themeRows: Swatch[][] = [
    themeColors.map((c) => ({
      key: `${c.slot}:0`,
      hex: c.hex,
      label: c.name,
      choice: { type: 'theme', slot: c.slot, tint: 0, hex: c.hex },
    })),
  ];
  for (let row = 0; row < 5; row++) {
    themeRows.push(
      themeColors.map((c) => {
        const tint = themeVariants(c.hex)[row] ?? 0;
        const hex = applyTint(c.hex, tint);
        return {
          key: `${c.slot}:${tint}`,
          hex,
          label: `${c.name}, ${tintLabel(tint)}`,
          choice: { type: 'theme', slot: c.slot, tint, hex },
        };
      }),
    );
  }
  const standard: Swatch[] = STANDARD_COLORS.map((c) => ({
    key: `std:${c.hex}`,
    hex: c.hex,
    label: c.name,
    choice: { type: 'rgb', hex: c.hex },
  }));
  const recents: Swatch[] = recent.map((hex) => ({
    key: `recent:${hex}`,
    hex,
    label: `Recent colour ${hex}`,
    choice: { type: 'rgb', hex },
  }));

  const rows: Array<{ section: string; swatches: Swatch[] }> = [
    ...themeRows.map((s, i) => ({ section: i === 0 ? 'theme' : 'theme-variants', swatches: s })),
    { section: 'standard', swatches: standard },
    ...(recents.length ? [{ section: 'recent', swatches: recents }] : []),
  ];

  const selectedKey = rows.flatMap((r) => r.swatches).find((s) => sameColor(value, s.choice))?.key;
  const firstKey = rows[0]?.swatches[0]?.key;
  const [focusKey, setFocusKey] = useState<string | undefined>(selectedKey ?? firstKey);
  const tabStop = rows.flatMap((r) => r.swatches).some((s) => s.key === focusKey) ? focusKey : firstKey;

  const choose = (choice: ColorChoice) => onChange(choice);

  const onGridKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    const r = Number(target.dataset.row);
    const c = Number(target.dataset.col);
    if (Number.isNaN(r) || Number.isNaN(c)) return;
    let nr = r;
    let nc = c;
    const rtl = getComputedStyle(target).direction === 'rtl';
    switch (e.key) {
      case 'ArrowRight':
        nc = c + (rtl ? -1 : 1);
        break;
      case 'ArrowLeft':
        nc = c + (rtl ? 1 : -1);
        break;
      case 'ArrowDown':
        nr = r + 1;
        break;
      case 'ArrowUp':
        nr = r - 1;
        break;
      case 'Home':
        nc = 0;
        break;
      case 'End':
        nc = (rows[r]?.swatches.length ?? 1) - 1;
        break;
      default:
        return;
    }
    e.preventDefault();
    nr = Math.max(0, Math.min(rows.length - 1, nr));
    nc = Math.max(0, Math.min((rows[nr]?.swatches.length ?? 1) - 1, nc));
    const next = gridRef.current?.querySelector<HTMLButtonElement>(`[data-row="${nr}"][data-col="${nc}"]`);
    if (next) {
      setFocusKey(next.dataset.key);
      next.focus();
    }
  };

  const renderRow = (rowIndex: number) => {
    const row = rows[rowIndex];
    if (!row) return null;
    return (
      <div
        key={`${row.section}-${rowIndex}`}
        className={cx('kiln-color__row', row.section === 'theme-variants' && 'kiln-color__row--variants')}
      >
        {row.swatches.map((s, col) => (
          <button
            key={s.key}
            type="button"
            className="kiln-color__swatch"
            style={{ ['--swatch' as string]: s.hex }}
            data-row={rowIndex}
            data-col={col}
            data-key={s.key}
            tabIndex={s.key === tabStop ? 0 : -1}
            aria-label={s.label}
            aria-pressed={s.key === selectedKey}
            title={s.label}
            onFocus={() => setFocusKey(s.key)}
            onClick={() => choose(s.choice)}
          />
        ))}
      </div>
    );
  };

  const themeEnd = themeRows.length;
  return (
    <div className={cx('kiln-color', className)}>
      {allowAuto || allowNone ? (
        <div className="kiln-color__specials">
          {allowAuto ? (
            <button
              type="button"
              className="kiln-menu__item kiln-color__special"
              aria-pressed={value?.type === 'auto'}
              onClick={() => choose({ type: 'auto' })}
            >
              <span className="kiln-color__auto-chip" aria-hidden="true" />
              {autoLabel}
              {value?.type === 'auto' ? <Icon icon={Check} size={14} className="kiln-color__check" /> : null}
            </button>
          ) : null}
          {allowNone ? (
            <button
              type="button"
              className="kiln-menu__item kiln-color__special"
              aria-pressed={value?.type === 'none'}
              onClick={() => choose({ type: 'none' })}
            >
              <Icon icon={Ban} size={16} />
              {noneLabel}
              {value?.type === 'none' ? <Icon icon={Check} size={14} className="kiln-color__check" /> : null}
            </button>
          ) : null}
        </div>
      ) : null}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: arrow-key navigation delegated from the swatch buttons */}
      <div ref={gridRef} onKeyDown={onGridKeyDown}>
        <div className="kiln-color__heading" id={`${titleId}-theme`}>
          Theme Colors
        </div>
        {/* biome-ignore lint/a11y/useSemanticElements: a group of swatch buttons */}
        <div role="group" aria-labelledby={`${titleId}-theme`} className="kiln-color__section">
          {Array.from({ length: themeEnd }, (_, i) => renderRow(i))}
        </div>
        <div className="kiln-color__heading" id={`${titleId}-std`}>
          Standard Colors
        </div>
        {/* biome-ignore lint/a11y/useSemanticElements: a group of swatch buttons */}
        <div role="group" aria-labelledby={`${titleId}-std`} className="kiln-color__section">
          {renderRow(themeEnd)}
        </div>
        {recents.length ? (
          <>
            <div className="kiln-color__heading" id={`${titleId}-recent`}>
              Recent Colors
            </div>
            {/* biome-ignore lint/a11y/useSemanticElements: a group of swatch buttons */}
            <div role="group" aria-labelledby={`${titleId}-recent`} className="kiln-color__section">
              {renderRow(themeEnd + 1)}
            </div>
          </>
        ) : null}
      </div>
      <div className="kiln-menu__separator" />
      {custom === null ? (
        <button
          type="button"
          className="kiln-menu__item kiln-color__special"
          onClick={() =>
            setCustom(value && value.type !== 'auto' && value.type !== 'none' ? value.hex : '#4472C4')
          }
        >
          <Icon icon={Palette} size={16} />
          More Colors…
        </button>
      ) : (
        <div className="kiln-color__custom">
          <HexColorPicker color={custom} onChange={(c) => setCustom(c.toUpperCase())} />
          <div className="kiln-color__custom-row">
            <span className="kiln-color__preview" style={{ background: custom }} aria-hidden="true" />
            <span className="kiln-color__hex">
              <span aria-hidden="true">Hex</span>
              <HexColorInput
                className="kiln-field kiln-field--sm kiln-color__hex-input"
                color={custom}
                prefixed
                onChange={(c) => setCustom(c.toUpperCase())}
                aria-label="Hex colour"
              />
            </span>
            <button
              type="button"
              className="kiln-btn kiln-btn--primary kiln-btn--sm"
              onClick={() => {
                const hex = normalizeHex(custom);
                if (!hex) return;
                addRecentColor(hex);
                setCustom(null);
                choose({ type: 'rgb', hex });
              }}
            >
              <span className="kiln-btn__label">OK</span>
            </button>
          </div>
        </div>
      )}
      {footer}
    </div>
  );
}
