/** Length units used in Office dialogs. Points are the canonical unit (1 in = 72 pt). */
export type LengthUnit = 'pt' | 'px' | 'cm' | 'mm' | 'in';

export const POINTS_PER_UNIT: Record<LengthUnit, number> = {
  pt: 1,
  px: 0.75, // CSS px: 96 per inch
  in: 72,
  cm: 72 / 2.54,
  mm: 72 / 25.4,
};

const ALIASES: Record<string, LengthUnit> = {
  pt: 'pt',
  pts: 'pt',
  point: 'pt',
  points: 'pt',
  px: 'px',
  in: 'in',
  inch: 'in',
  inches: 'in',
  '"': 'in',
  '″': 'in',
  cm: 'cm',
  mm: 'mm',
};

export function convertLength(value: number, from: LengthUnit, to: LengthUnit): number {
  return (value * POINTS_PER_UNIT[from]) / POINTS_PER_UNIT[to];
}

/**
 * Parses user input such as `2.5 cm`, `1"`, `12pt`, `0,5 in` or a bare number (taken in `unit`) and
 * returns the value converted to `unit`, or `null` when it is not a number.
 */
export function parseLength(input: string, unit: LengthUnit): number | null {
  const m = /^\s*([-+]?(?:\d+(?:[.,]\d*)?|[.,]\d+))\s*([a-zA-Z"″]*)\s*$/.exec(input);
  if (!m?.[1]) return null;
  const n = Number(m[1].replace(',', '.'));
  if (!Number.isFinite(n)) return null;
  const suffix = (m[2] ?? '').toLowerCase();
  if (!suffix) return n;
  const from = ALIASES[suffix];
  if (!from) return null;
  return convertLength(n, from, unit);
}

/** Rounds to `precision` decimals and drops trailing zeros: 2.50 → "2.5". */
export function formatNumber(value: number, precision: number): string {
  const f = 10 ** precision;
  return String(Math.round(value * f) / f);
}
