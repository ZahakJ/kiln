/**
 * Coercion rules (Excel, en-US).
 *
 * - Text → number accepts what Excel's arithmetic accepts: spaces, sign, `$`, thousands separators,
 *   accounting parentheses `(5)`, exponents, a trailing `%`, mixed fractions `1 1/2`, and dates and
 *   times in en-US forms (`2024-01-15`, `1/15/2024`, `15-Jan-2024`, `January 15, 2024`, `13:30`,
 *   `1:30 PM`, `1/15/2024 13:30`). Anything else is `#VALUE!`.
 * - Booleans are 1/0 in arithmetic; blanks are 0, "" and FALSE.
 */
import { dateToSerial, daysInMonth, timeToFraction } from './dates.ts';
import { Errors, ErrorValue } from './errors.ts';
import { numberToText } from './number.ts';
import { BLANK, type Scalar } from './types.ts';

export interface CoercionOptions {
  date1904?: boolean;
  /** Year assumed for dates typed without one (`1/15`). Defaults to the current year. */
  currentYear?: number;
}

const NUMBER_RE = /^(?:(\d{1,3}(?:,\d{3})+)|(\d*))(\.\d*)?(?:[eE]([+-]?\d+))?$/;

/** Parses a number written as text the way Excel's arithmetic does, or returns undefined. */
export function parseNumberText(text: string, opts: CoercionOptions = {}): number | undefined {
  const s = text.trim();
  if (s === '') return undefined;
  const plain = parsePlainNumber(s);
  if (plain !== undefined) return plain;
  const frac = /^([+-]?)(\d+)\s+(\d+)\/(\d+)$/.exec(s);
  if (frac) {
    const den = Number(frac[4]);
    if (den === 0) return undefined;
    const v = Number(frac[2]) + Number(frac[3]) / den;
    return frac[1] === '-' ? -v : v;
  }
  return parseDateTimeText(s, opts);
}

function parsePlainNumber(input: string): number | undefined {
  let s = input;
  let negative = false;
  let percent = false;
  // Accounting negative: (123) or ($123)
  if (s.startsWith('(') && s.endsWith(')')) {
    negative = true;
    s = s.slice(1, -1).trim();
  }
  if (s.endsWith('%')) {
    percent = true;
    s = s.slice(0, -1).trim();
  }
  let sawSign = false;
  let sawCurrency = false;
  for (;;) {
    const ch = s[0];
    if ((ch === '+' || ch === '-') && !sawSign && !negative) {
      sawSign = true;
      if (ch === '-') negative = true;
      s = s.slice(1).trimStart();
    } else if (ch === '$' && !sawCurrency) {
      sawCurrency = true;
      s = s.slice(1).trimStart();
    } else break;
  }
  if (s.endsWith('$') && !sawCurrency) s = s.slice(0, -1).trimEnd();
  const m = NUMBER_RE.exec(s);
  if (!m) return undefined;
  const intPart = (m[1] ?? m[2] ?? '').replace(/,/g, '');
  const fracPart = m[3] ?? '';
  if (intPart === '' && fracPart.length <= 1) return undefined;
  let v = Number(
    `${intPart || '0'}${fracPart === '.' ? '' : fracPart}${m[4] !== undefined ? `e${m[4]}` : ''}`,
  );
  if (!Number.isFinite(v)) return undefined;
  if (percent) v /= 100;
  return negative ? -v : v;
}

const MONTHS = [
  'january',
  'february',
  'march',
  'april',
  'may',
  'june',
  'july',
  'august',
  'september',
  'october',
  'november',
  'december',
];

function monthFromName(name: string): number | undefined {
  const n = name.toLowerCase();
  if (n.length < 3) return undefined;
  if (n === 'sept') return 9;
  const i = MONTHS.findIndex((m) => m === n || m.slice(0, 3) === n);
  return i < 0 ? undefined : i + 1;
}

function fullYear(y: string): number {
  const n = Number(y);
  if (y.length <= 2) return n < 30 ? 2000 + n : 1900 + n;
  return n;
}

const TIME_RE = /^(\d{1,5}):(\d{1,2})(?::(\d{1,2}(?:\.\d*)?))?(?:\s*([AaPp])[Mm]?)?$/;
const TIME_MS_RE = /^(\d{1,4}):(\d{1,2}\.\d+)$/; // mm:ss.0
const HOUR_AMPM_RE = /^(\d{1,2})\s*([AaPp])[Mm]?$/;

function parseTimeText(s: string): number | undefined {
  let m = TIME_MS_RE.exec(s);
  if (m) {
    const sec = Number(m[2]);
    if (sec >= 60) return undefined;
    return timeToFraction(0, Number(m[1]), sec);
  }
  let hour: number;
  let minute = 0;
  let second = 0;
  let ampm: string | undefined;
  m = TIME_RE.exec(s);
  if (m) {
    hour = Number(m[1]);
    minute = Number(m[2]);
    second = m[3] !== undefined ? Number(m[3]) : 0;
    ampm = m[4];
  } else {
    m = HOUR_AMPM_RE.exec(s);
    if (!m) return undefined;
    hour = Number(m[1]);
    ampm = m[2];
  }
  if (minute >= 60 || second >= 60) return undefined;
  if (ampm) {
    if (hour < 1 || hour > 12) return undefined;
    const pm = ampm.toLowerCase() === 'p';
    if (hour === 12) hour = pm ? 12 : 0;
    else if (pm) hour += 12;
  } else if (hour > 9999) return undefined;
  return timeToFraction(hour, minute, second);
}

function validDate(y: number, m: number, d: number, opts: CoercionOptions): number | undefined {
  if (m < 1 || m > 12 || d < 1 || y < 1900 || y > 9999) return undefined;
  if (d > daysInMonth(y, m)) return undefined;
  return dateToSerial(y, m, d, opts.date1904);
}

function parseDateText(s: string, opts: CoercionOptions): number | undefined {
  const year = () => opts.currentYear ?? new Date().getFullYear();
  let m = /^(\d{4})[-/.](\d{1,2})[-/.](\d{1,2})$/.exec(s);
  if (m) return validDate(Number(m[1]), Number(m[2]), Number(m[3]), opts);
  m = /^(\d{1,2})[-/](\d{1,2})[-/](\d{4}|\d{1,2})$/.exec(s);
  if (m) return validDate(fullYear(m[3] as string), Number(m[1]), Number(m[2]), opts);
  m = /^(\d{1,2})[-/](\d{4})$/.exec(s);
  if (m) return validDate(Number(m[2]), Number(m[1]), 1, opts);
  m = /^(\d{1,2})[-/](\d{1,2})$/.exec(s);
  if (m) return validDate(year(), Number(m[1]), Number(m[2]), opts);
  m = /^(\d{1,2})[-\s]+([A-Za-z]+)\.?(?:[-\s,]+(\d{4}|\d{2}))?$/.exec(s);
  if (m) {
    const mon = monthFromName(m[2] as string);
    if (mon === undefined) return undefined;
    return validDate(m[3] !== undefined ? fullYear(m[3]) : year(), mon, Number(m[1]), opts);
  }
  m = /^([A-Za-z]+)\.?[-\s]+(\d{1,2})(?:(?:,\s*|[-\s]+)(\d{4}|\d{2}))?$/.exec(s);
  if (m) {
    const mon = monthFromName(m[1] as string);
    if (mon === undefined) return undefined;
    return validDate(m[3] !== undefined ? fullYear(m[3]) : year(), mon, Number(m[2]), opts);
  }
  m = /^([A-Za-z]+)\.?[-\s,]+(\d{4})$/.exec(s);
  if (m) {
    const mon = monthFromName(m[1] as string);
    if (mon === undefined) return undefined;
    return validDate(Number(m[2]), mon, 1, opts);
  }
  return undefined;
}

/** Parses an en-US date, time or date-time written as text into a serial. */
export function parseDateTimeText(text: string, opts: CoercionOptions = {}): number | undefined {
  const s = text.trim().replace(/\s+/g, ' ');
  if (s === '') return undefined;
  const t = parseTimeText(s);
  if (t !== undefined) return t;
  const d = parseDateText(s, opts);
  if (d !== undefined) return d;
  const split = /^(.+?)(?:\s+|T)(\d{1,5}:\d.*|\d{1,2}\s*[AaPp][Mm]?)$/.exec(s);
  if (split) {
    const date = parseDateText(split[1] as string, opts);
    const time = parseTimeText(split[2] as string);
    if (date !== undefined && time !== undefined) return date + time;
  }
  return undefined;
}

/** Converts a scalar to a number for arithmetic: TRUE→1, blank→0, numeric text parsed, else #VALUE!. */
export function toNumber(v: Scalar, opts?: CoercionOptions): number | ErrorValue {
  switch (typeof v) {
    case 'number':
      return v;
    case 'boolean':
      return v ? 1 : 0;
    case 'string': {
      const n = parseNumberText(v, opts);
      return n === undefined ? Errors.VALUE : n;
    }
    default:
      if (v === BLANK) return 0;
      return v as ErrorValue;
  }
}

/** Converts a scalar to text: numbers in General format, TRUE/FALSE, blank → "". */
export function toText(v: Scalar): string | ErrorValue {
  switch (typeof v) {
    case 'string':
      return v;
    case 'number':
      return numberToText(v);
    case 'boolean':
      return v ? 'TRUE' : 'FALSE';
    default:
      if (v === BLANK) return '';
      return v as ErrorValue;
  }
}

/** Converts a scalar to a logical: numbers ≠ 0 are TRUE, "TRUE"/"FALSE" text, blank → FALSE. */
export function toBoolean(v: Scalar): boolean | ErrorValue {
  switch (typeof v) {
    case 'boolean':
      return v;
    case 'number':
      return v !== 0;
    case 'string': {
      const u = v.trim().toUpperCase();
      if (u === 'TRUE') return true;
      if (u === 'FALSE') return false;
      return Errors.VALUE;
    }
    default:
      if (v === BLANK) return false;
      return v as ErrorValue;
  }
}

/** Display text for any scalar (errors show their code). */
export function displayText(v: Scalar): string {
  if (v instanceof ErrorValue) return v.code;
  return toText(v) as string;
}
