/**
 * Unit helpers for the Write model. The primitive conversions (twips, half-points, eighths, EMU) live
 * in `@officina/ooxml` and are re-exported here so editor code can import everything from doc-model.
 */
import type { LineRule, Spacing } from './types.ts';

export {
  EMU_PER_INCH,
  EMU_PER_PX,
  eighthsToPt,
  emuToPt,
  emuToPx,
  emuToTwips,
  halfPointsToPt,
  ptToEighths,
  ptToEmu,
  ptToHalfPoints,
  ptToTwips,
  pxToEmu,
  pxToTwips,
  TWIPS_PER_INCH,
  TWIPS_PER_POINT,
  twipsToEmu,
  twipsToPt,
  twipsToPx,
} from '@officina/ooxml';

/** Twips per centimetre (1440 / 2.54). */
export const TWIPS_PER_CM = 1440 / 2.54;
/** `lineRule="auto"` unit: 240 = one line. */
export const LINE_UNITS = 240;

export const twipsToCm = (twips: number): number => twips / TWIPS_PER_CM;
export const cmToTwips = (cm: number): number => Math.round(cm * TWIPS_PER_CM);
export const twipsToInch = (twips: number): number => twips / 1440;
export const inchToTwips = (inch: number): number => Math.round(inch * 1440);
export const halfPointsToPx = (hp: number): number => (hp / 2) * (96 / 72);
export const ptToPx = (pt: number): number => pt * (96 / 72);
export const pxToPt = (px: number): number => px * (72 / 96);

/** Converts a line-spacing multiple (1.08) into `w:line` 240ths (259). */
export const lineMultipleToUnits = (multiple: number): number => Math.round(multiple * LINE_UNITS);
/** Converts `w:line` 240ths into a multiple (`259` → `1.0791…`). */
export const lineUnitsToMultiple = (units: number): number => units / LINE_UNITS;

/**
 * A CSS `line-height` for resolved spacing: unitless multiple for `auto`, points for exact/atLeast.
 * (Word's "multiple" applies to the font's natural line height, which CSS approximates with a unitless
 * multiplier of the font size × the font's ascent+descent; the editor applies font metrics on top.)
 */
export function cssLineHeight(spacing: Spacing | undefined): string {
  const line = spacing?.line;
  const rule: LineRule = spacing?.lineRule ?? 'auto';
  if (line === undefined) return 'normal';
  if (rule === 'auto') return String(Math.round((line / LINE_UNITS) * 10000) / 10000);
  return `${line / 20}pt`;
}

/** Formats twips for display in the user's measurement unit (Word shows 1 decimal for cm/in). */
export function formatLength(twips: number, unit: 'cm' | 'in' | 'pt' | 'mm'): string {
  const round = (n: number, d: number) => {
    const f = 10 ** d;
    return String(Math.round(n * f) / f);
  };
  switch (unit) {
    case 'cm':
      return `${round(twipsToCm(twips), 2)} cm`;
    case 'mm':
      return `${round(twipsToCm(twips) * 10, 1)} mm`;
    case 'in':
      return `${round(twipsToInch(twips), 2)}"`;
    case 'pt':
      return `${round(twips / 20, 1)} pt`;
  }
}
