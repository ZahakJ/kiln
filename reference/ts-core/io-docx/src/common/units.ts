/**
 * WordprocessingML measurement parsing. Attributes are usually bare integers in a native unit (twips,
 * half-points, eighths of a point, EMU), but ECMA-376 2nd+ edition files (and Strict files) may use
 * `ST_UniversalMeasure` strings such as `"2.5cm"` or `"12pt"`, and table widths may be percentages.
 */
import type { TableWidth } from '@kiln-ref/doc-model';
import { parseUniversalMeasure } from '@kiln-ref/ooxml';

function parseNative(v: string | undefined, unitsPerPoint: number): number | undefined {
  if (v === undefined) return undefined;
  const s = v.trim();
  if (s === '') return undefined;
  const um = parseUniversalMeasure(s);
  if (um !== undefined) return Math.round(um * unitsPerPoint);
  const n = Number(s);
  return Number.isFinite(n) ? Math.round(n) : undefined;
}

/** `ST_TwipsMeasure` / `ST_SignedTwipsMeasure` → twips. */
export const parseTwips = (v: string | undefined): number | undefined => parseNative(v, 20);

/** `ST_HpsMeasure` / `ST_SignedHpsMeasure` → half-points. */
export const parseHalfPoints = (v: string | undefined): number | undefined => parseNative(v, 2);

/** `ST_Coordinate` / `ST_PositiveCoordinate` → EMU. */
export const parseEmu = (v: string | undefined): number | undefined => parseNative(v, 12700);

/** Plain decimal number (`ST_DecimalNumber`); `undefined` when absent or malformed. */
export function parseDecimal(v: string | undefined): number | undefined {
  if (v === undefined) return undefined;
  const s = v.trim();
  if (s === '') return undefined;
  const n = Number(s);
  return Number.isFinite(n) ? Math.trunc(n) : undefined;
}

/** `ST_OnOff` value (`true|false|1|0|on|off`); absent means `fallback`. */
export function parseOnOff(v: string | undefined, fallback = true): boolean {
  if (v === undefined) return fallback;
  switch (v.trim().toLowerCase()) {
    case 'false':
    case '0':
    case 'off':
    case 'none':
      return false;
    default:
      return true;
  }
}

/**
 * `ST_MeasurementOrPercent` with `w:type` (`w:tblW`, `w:tcW`, `w:tblInd`, cell margins): returns the
 * width in the model's units — twips for `dxa`, fiftieths of a percent for `pct` — whatever notation
 * the file used (`"2.5in"`, `"50%"`, `"2500"`).
 */
export function parseTableWidth(w: string | undefined, type: string | undefined): TableWidth | undefined {
  const t = (type ?? 'dxa') as TableWidth['type'];
  if (t === 'auto' || t === 'nil') return { type: t, value: Math.max(0, parseTwips(w) ?? 0) };
  if (w === undefined) return t === 'pct' ? { type: 'pct', value: 0 } : undefined;
  const s = w.trim();
  if (t === 'pct' || s.endsWith('%')) {
    if (s.endsWith('%')) {
      const n = Number(s.slice(0, -1));
      return Number.isFinite(n) ? { type: 'pct', value: Math.round(n * 50) } : undefined;
    }
    const n = Number(s);
    return Number.isFinite(n) ? { type: 'pct', value: Math.round(n) } : undefined;
  }
  const twips = parseTwips(s);
  return twips === undefined ? undefined : { type: 'dxa', value: twips };
}
