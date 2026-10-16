/**
 * Number-format bridge: turns a cell value and its Excel format code into display text, through
 * `numfmt` (ECMA-376 format codes), plus Excel's own **General** format which `numfmt` does not
 * reproduce exactly.
 *
 * General (cell display, standard column width): at most 11 characters for the number itself (the
 * sign is extra), rounding rather than truncating; integers up to 11 digits print in full, larger
 * magnitudes switch to scientific notation with as many significant digits as fit (`1.23457E+11`);
 * small numbers stay decimal while every significant digit fits in 11 decimals, else scientific
 * (`1.234E-07`).
 *
 * The result carries *runs* for the renderer: `fill` runs repeat a character to fill the cell (`*`
 * in a code, used by accounting formats to push the currency symbol to the left edge) and `pad` runs
 * reserve the width of a character (`_)`, which aligns positive numbers with parenthesised negatives).
 */

import { ErrorValue, excelRound } from '@kiln-ref/formula';
import * as numfmt from 'numfmt';
import type { CellValue } from './values.ts';

export type FormatRun =
  | { type: 'text'; text: string }
  | { type: 'fill'; char: string }
  | { type: 'pad'; char: string };

export interface FormattedValue {
  /** Plain display text: fills omitted, padding as a space (what Copy or TEXT() would give). */
  text: string;
  runs: FormatRun[];
  /** CSS colour from a `[Red]`/`[Color n]` section, as `#RRGGBB`. */
  color?: string;
  /** Horizontal alignment used when the cell's alignment is General. */
  align: 'left' | 'center' | 'right';
  kind: 'empty' | 'number' | 'date' | 'text' | 'boolean' | 'error';
  /** The value cannot be shown with this format (negative or too-large dates): fill the cell with `#`. */
  overflow?: boolean;
}

export interface FormatOptions {
  /** BCP 47 locale for separators, month and day names (default en-US). */
  locale?: string;
  /** Workbook uses the 1904 date system. */
  date1904?: boolean;
  /** Characters available to General (default 11, Excel's standard column). */
  generalWidth?: number;
}

const FILL = '\u0001';
const PAD = '\u0002';

/** Excel's named format colours. */
const NAMED_COLORS: Record<string, string> = {
  black: '#000000',
  blue: '#0000FF',
  cyan: '#00FFFF',
  green: '#00FF00',
  magenta: '#FF00FF',
  red: '#FF0000',
  white: '#FFFFFF',
  yellow: '#FFFF00',
};

/** Largest date serial Excel can display (9999-12-31 23:59:59). */
const MAX_DATE = 2_958_465.999_988_426;
const DATE1904_OFFSET = 1462;

const isGeneral = (code: string) => code === '' || /^general$/i.test(code.trim());

function decimalOf(locale: string | undefined): string {
  if (!locale) return '.';
  return numfmt.getLocale(locale)?.decimal ?? '.';
}

function trimZeros(s: string): string {
  if (!s.includes('.')) return s;
  return s.replace(/0+$/, '').replace(/\.$/, '');
}

function scientific(a: number, width: number): string {
  // Mantissa digits that fit: width - "E+XX" - "." (exponent grows to 3 digits past 1E+99).
  for (let sig = Math.max(1, width - 5); sig >= 1; sig--) {
    const [m, e] = a.toExponential(sig - 1).split('e') as [string, string];
    const exp = Number(e);
    const expText = `${exp < 0 ? '-' : '+'}${String(Math.abs(exp)).padStart(2, '0')}`;
    const out = `${trimZeros(m)}E${expText}`;
    if (out.length <= width || sig === 1) return out;
  }
  return a.toExponential(0).toUpperCase();
}

/**
 * Excel's General format for a number (`width` = characters for the digits, decimal point and
 * exponent; the minus sign is not counted).
 */
export function formatGeneral(x: number, width = 11, decimal = '.'): string {
  if (!Number.isFinite(x)) return '#NUM!';
  if (x === 0) return '0';
  const sign = x < 0 ? '-' : '';
  const a = Math.abs(x);
  const exp = Math.floor(Math.log10(a));
  let out: string | undefined;
  if (exp >= 0) {
    const intDigits = exp + 1;
    if (intDigits <= width) {
      const decimals = Math.max(0, width - intDigits - 1);
      const s = trimZeros(a.toFixed(Math.min(decimals, 20)));
      const intPart = s.split('.')[0] as string;
      if (intPart.length <= width)
        out = s.length <= width ? s : trimZeros(a.toFixed(Math.max(0, width - intPart.length - 1)));
    }
  } else if (exp >= -4) {
    out = trimZeros(a.toFixed(width - 2));
  } else {
    const s = trimZeros(a.toFixed(Math.min(width, 20)));
    if (s.length <= width && s !== '0') out = s;
  }
  if (out === undefined || out === '0') out = scientific(a, width);
  if (decimal !== '.') out = out.replace('.', decimal);
  return sign + out;
}

function parseRuns(raw: string): FormatRun[] {
  const runs: FormatRun[] = [];
  let text = '';
  for (let i = 0; i < raw.length; i++) {
    const ch = raw[i] as string;
    if ((ch === FILL || ch === PAD) && i + 1 < raw.length) {
      if (text) runs.push({ type: 'text', text });
      text = '';
      const next = raw[++i] as string;
      runs.push(ch === FILL ? { type: 'fill', char: next } : { type: 'pad', char: next });
    } else text += ch;
  }
  if (text) runs.push({ type: 'text', text });
  return runs;
}

function runsText(runs: FormatRun[]): string {
  return runs.map((r) => (r.type === 'text' ? r.text : r.type === 'pad' ? ' ' : '')).join('');
}

function colorOf(code: string, value: number | string): string | undefined {
  try {
    const c = numfmt.formatColor(code, value);
    if (c === null || c === undefined) return undefined;
    if (typeof c === 'number') return undefined;
    const named = NAMED_COLORS[c.toLowerCase()];
    if (named) return named;
    return /^#[0-9a-f]{6}$/i.test(c) ? c.toUpperCase() : undefined;
  } catch {
    return undefined;
  }
}

interface FormatInfo {
  valid: boolean;
  date: boolean;
  text: boolean;
  /** Fixed-point layout (number/grouped/percent) without conditions. */
  fixed: boolean;
  decimals: number;
  scale: number;
}

const formatCache = new Map<string, FormatInfo>();

function info(code: string): FormatInfo {
  let hit = formatCache.get(code);
  if (!hit) {
    const valid = numfmt.isValidFormat(code);
    const fi = valid ? numfmt.getFormatInfo(code) : undefined;
    hit = {
      valid,
      date: valid && numfmt.isDateFormat(code),
      text: valid && numfmt.isTextFormat(code),
      fixed:
        fi !== undefined &&
        (fi.type === 'number' || fi.type === 'grouped' || fi.type === 'percent') &&
        !/\[[<>=]/.test(code),
      decimals: fi?.maxDecimals ?? 0,
      scale: fi?.scale || 1,
    };
    if (formatCache.size > 2000) formatCache.clear();
    formatCache.set(code, hit);
  }
  return hit;
}

/** True when the format code displays dates or times. */
export function isDateFormat(code: string): boolean {
  return !isGeneral(code) && info(code).date;
}

export function isValidFormat(code: string): boolean {
  return isGeneral(code) || info(code).valid;
}

function result(raw: string, extra: Omit<FormattedValue, 'text' | 'runs'>): FormattedValue {
  const runs = parseRuns(raw);
  const out: FormattedValue = { text: runsText(runs), runs, ...extra };
  if (out.color === undefined) delete out.color;
  return out;
}

/**
 * Formats a cell value with a format code. Numbers honour sections, conditions (`[>100]`), colours,
 * dates/times, percentages, fractions, scientific and accounting layouts; text goes through the text
 * section (`@`); booleans and errors print as Excel shows them.
 */
export function formatCell(
  value: CellValue | undefined,
  code = 'General',
  opts: FormatOptions = {},
): FormattedValue {
  const locale = opts.locale ?? 'en-US';
  if (value === null || value === undefined) return { text: '', runs: [], align: 'left', kind: 'empty' };
  if (value instanceof ErrorValue)
    return { text: value.code, runs: [{ type: 'text', text: value.code }], align: 'center', kind: 'error' };
  if (typeof value === 'boolean') {
    const t = value ? 'TRUE' : 'FALSE';
    return { text: t, runs: [{ type: 'text', text: t }], align: 'center', kind: 'boolean' };
  }
  // numfmt's typings declare fillChar/skipChar as booleans, but they take the marker character.
  const nfOpts = {
    locale,
    fillChar: FILL,
    skipChar: PAD,
    throws: true,
    leap1900: true,
    dateSpanLarge: false,
  } as unknown as Parameters<typeof numfmt.format>[2];
  if (typeof value === 'string') {
    if (isGeneral(code) || !info(code).valid)
      return { text: value, runs: value ? [{ type: 'text', text: value }] : [], align: 'left', kind: 'text' };
    const raw = numfmt.format(code, value, nfOpts);
    return result(raw, { color: colorOf(code, value), align: 'left', kind: 'text' });
  }
  // Numbers.
  if (isGeneral(code) || !info(code).valid) {
    const t = formatGeneral(value, opts.generalWidth ?? 11, decimalOf(locale));
    return { text: t, runs: [{ type: 'text', text: t }], align: 'right', kind: 'number' };
  }
  const meta = info(code);
  if (meta.text) {
    // A text-only format shows numbers as General text, left-aligned like text.
    const raw = numfmt.format(code, formatGeneral(value, opts.generalWidth ?? 11, decimalOf(locale)), nfOpts);
    return result(raw, { color: colorOf(code, value), align: 'left', kind: 'text' });
  }
  let v = value;
  if (meta.date) {
    if (opts.date1904) v += DATE1904_OFFSET;
    if (v < 0 || v > MAX_DATE + (opts.date1904 ? DATE1904_OFFSET : 0))
      return { text: '#', runs: [{ type: 'fill', char: '#' }], align: 'right', kind: 'date', overflow: true };
  }
  // Excel rounds the 15-significant-digit *decimal* value half away from zero (1.005 shows as 1.01
  // with 0.00 although the double is 1.00499…); numfmt rounds the binary value. Pre-round plain
  // fixed-point layouts to the displayed decimals, as long as that cannot change which section applies.
  if (!meta.date && meta.fixed && v !== 0) {
    const r = excelRound(v * meta.scale, meta.decimals) / meta.scale;
    if (r !== 0 && Math.sign(r) === Math.sign(v)) v = r;
  }
  let raw: string;
  try {
    raw = numfmt.format(code, v, nfOpts);
    // Excel keeps the sign of a negative number that rounds to zero in a single-section format ("-0").
    if (v < 0 && !code.includes(';') && !/\[[<>=]/.test(code) && !raw.includes('-') && !/[1-9]/.test(raw))
      raw = `-${raw}`;
  } catch {
    if (meta.date)
      return { text: '#', runs: [{ type: 'fill', char: '#' }], align: 'right', kind: 'date', overflow: true };
    const t = formatGeneral(value, opts.generalWidth ?? 11, decimalOf(locale));
    return { text: t, runs: [{ type: 'text', text: t }], align: 'right', kind: 'number' };
  }
  return result(raw, { color: colorOf(code, v), align: 'right', kind: meta.date ? 'date' : 'number' });
}

/** Display text only (the TEXT()-like rendering of `formatCell`). */
export function formatValueText(
  value: CellValue | undefined,
  code = 'General',
  opts: FormatOptions = {},
): string {
  return formatCell(value, code, opts).text;
}
