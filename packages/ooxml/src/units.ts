/**
 * OOXML measurement units. Office stores geometry in EMU (English Metric Units: 914400 per inch, 12700
 * per point, 360000 per centimetre), WordprocessingML lengths in twips (twentieths of a point), font sizes
 * in half-points, border widths in eighths of a point, and DrawingML percentages/angles in thousandths of
 * a percent and sixty-thousandths of a degree.
 */

export const EMU_PER_INCH = 914400;
export const EMU_PER_POINT = 12700;
export const EMU_PER_CM = 360000;
export const EMU_PER_MM = 36000;
/** CSS pixels are 1/96 inch. */
export const EMU_PER_PX = 9525;
export const TWIPS_PER_POINT = 20;
export const TWIPS_PER_INCH = 1440;
export const POINTS_PER_INCH = 72;

export const emuToPt = (emu: number): number => emu / EMU_PER_POINT;
export const ptToEmu = (pt: number): number => Math.round(pt * EMU_PER_POINT);
export const emuToPx = (emu: number): number => emu / EMU_PER_PX;
export const pxToEmu = (px: number): number => Math.round(px * EMU_PER_PX);
export const emuToInch = (emu: number): number => emu / EMU_PER_INCH;
export const inchToEmu = (inch: number): number => Math.round(inch * EMU_PER_INCH);
export const emuToCm = (emu: number): number => emu / EMU_PER_CM;
export const cmToEmu = (cm: number): number => Math.round(cm * EMU_PER_CM);
export const emuToMm = (emu: number): number => emu / EMU_PER_MM;
export const mmToEmu = (mm: number): number => Math.round(mm * EMU_PER_MM);

export const twipsToPt = (twips: number): number => twips / TWIPS_PER_POINT;
export const ptToTwips = (pt: number): number => Math.round(pt * TWIPS_PER_POINT);
export const twipsToEmu = (twips: number): number => Math.round(twips * (EMU_PER_POINT / TWIPS_PER_POINT));
export const emuToTwips = (emu: number): number => Math.round(emu / (EMU_PER_POINT / TWIPS_PER_POINT));
export const twipsToPx = (twips: number): number => (twips / TWIPS_PER_INCH) * 96;
export const pxToTwips = (px: number): number => Math.round((px / 96) * TWIPS_PER_INCH);

/** `w:sz` and friends: font size in half-points. */
export const halfPointsToPt = (hp: number): number => hp / 2;
export const ptToHalfPoints = (pt: number): number => Math.round(pt * 2);
/** `w:bdr/@w:sz`: border width in eighths of a point. */
export const eighthsToPt = (e: number): number => e / 8;
export const ptToEighths = (pt: number): number => Math.round(pt * 8);
/** DrawingML `a:rPr/@sz`: font size in hundredths of a point. */
export const centipointsToPt = (cp: number): number => cp / 100;
export const ptToCentipoints = (pt: number): number => Math.round(pt * 100);

/** DrawingML angle (ST_Angle): 60000ths of a degree. */
export const ANGLE_UNITS_PER_DEGREE = 60000;
export const angleToDegrees = (a: number): number => a / ANGLE_UNITS_PER_DEGREE;
export const degreesToAngle = (deg: number): number => Math.round(deg * ANGLE_UNITS_PER_DEGREE);
export const angleToRadians = (a: number): number => (a / ANGLE_UNITS_PER_DEGREE) * (Math.PI / 180);

/** DrawingML ST_Percentage: 100000 = 100 %. Returns a fraction (1 = 100 %). */
export const PERCENT_UNITS = 100000;

/**
 * Parses ST_Percentage / ST_PositivePercentage / ST_FixedPercentage, which may be an integer in
 * thousandths of a percent (`50000`) or, in Strict documents, a string with a percent sign (`50%`).
 * Returns a fraction (`0.5`), or `undefined` if unparsable.
 */
export function parsePercentage(v: string | undefined): number | undefined {
  if (v === undefined) return undefined;
  const s = v.trim();
  if (s.endsWith('%')) {
    const n = Number(s.slice(0, -1));
    return Number.isFinite(n) ? n / 100 : undefined;
  }
  const n = Number(s);
  return s !== '' && Number.isFinite(n) ? n / PERCENT_UNITS : undefined;
}

/** Formats a fraction as ST_Percentage thousandths (`0.5` → `"50000"`). */
export function formatPercentage(fraction: number): string {
  return String(Math.round(fraction * PERCENT_UNITS));
}

/**
 * WordprocessingML ST_DecimalNumberOrPercent / pct values used for table widths: fiftieths of a percent
 * (`5000` = 100 %) or a percent string. Returns a fraction.
 */
export function parseFiftiethsPercent(v: string | undefined): number | undefined {
  if (v === undefined) return undefined;
  const s = v.trim();
  if (s.endsWith('%')) {
    const n = Number(s.slice(0, -1));
    return Number.isFinite(n) ? n / 100 : undefined;
  }
  const n = Number(s);
  return s !== '' && Number.isFinite(n) ? n / 5000 : undefined;
}

const UNIVERSAL_MEASURE = /^\s*(-?\d+(?:\.\d+)?)\s*(mm|cm|in|pt|pc|pi)\s*$/;
const POINTS_PER_UNIT: Record<string, number> = {
  mm: 72 / 25.4,
  cm: 72 / 2.54,
  in: 72,
  pt: 1,
  pc: 12,
  pi: 12,
};

/**
 * Parses ST_UniversalMeasure (`"2.5cm"`, `"1in"`, `"12pt"`) into points. Returns `undefined` for bare
 * numbers, which callers interpret in the attribute's native unit.
 */
export function parseUniversalMeasure(v: string | undefined): number | undefined {
  if (v === undefined) return undefined;
  const m = UNIVERSAL_MEASURE.exec(v);
  if (!m) return undefined;
  return Number(m[1]) * (POINTS_PER_UNIT[m[2] as string] as number);
}

/**
 * Parses a length attribute that is either a bare number in `nativeUnitsPerPoint` units (twips = 20,
 * EMU = 12700, half-points = 2) or a universal measure. Returns points.
 */
export function parseLengthPt(v: string | undefined, nativeUnitsPerPoint: number): number | undefined {
  if (v === undefined) return undefined;
  const um = parseUniversalMeasure(v);
  if (um !== undefined) return um;
  const n = Number(v.trim());
  return v.trim() !== '' && Number.isFinite(n) ? n / nativeUnitsPerPoint : undefined;
}

/** Rounds to `digits` decimals and removes `-0`. */
export function roundTo(n: number, digits = 3): number {
  const f = 10 ** digits;
  const r = Math.round(n * f) / f;
  return Object.is(r, -0) ? 0 : r;
}
