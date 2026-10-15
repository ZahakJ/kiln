/**
 * Date & Time functions on serial numbers in the 1900 date system (with the Lotus 1900-02-29) or the
 * 1904 system, following the workbook setting.
 */
import type { CallContext } from '../engine/context.ts';
import { force, isOmitted } from '../engine/functions/helpers.ts';
import type { Arg, FunctionDescriptor } from '../engine/functions/registry.ts';
import { parseDateTimeText, toNumber } from '../values/coerce.ts';
import {
  dateToSerial,
  daysInMonth,
  isLeapYear,
  MAX_DATE_SERIAL_1900,
  MAX_DATE_SERIAL_1904,
  serialToDate,
  serialWeekday,
} from '../values/dates.ts';
import { Errors, ErrorValue } from '../values/errors.ts';
import { BLANK, MISSING, type Scalar } from '../values/types.ts';
import { fn, int, isErr, num, scalarsOf, str } from './lib.ts';

/** A date serial from a lifted argument: numbers, date text, blanks (0). Range-checked. */
export function serialArg(a: Arg | undefined, ctx: CallContext, dflt?: number): number | ErrorValue {
  if (a === undefined && dflt !== undefined) return dflt;
  const v = a === undefined || a === MISSING ? BLANK : (a as Scalar);
  const n = toNumber(v, { date1904: ctx.date1904 });
  if (n instanceof ErrorValue) return n;
  const max = ctx.date1904 ? MAX_DATE_SERIAL_1904 : MAX_DATE_SERIAL_1900;
  if (n < 0 || n >= max + 1) return Errors.NUM;
  return n;
}

function partsOf(serial: number, ctx: CallContext) {
  return serialToDate(serial, ctx.date1904);
}

function serialOf(y: number, m: number, d: number, ctx: CallContext): number | ErrorValue {
  return dateToSerial(y, m, d, ctx.date1904) ?? Errors.NUM;
}

/** Hour, minute and second of a serial, rounded to the nearest second like Excel. */
function hms(serial: number): { h: number; m: number; s: number } {
  // Rounded to the nearest second, but never into the next day (0.999999 is 23:59:59).
  let secs = Math.round((serial - Math.floor(serial)) * 86_400);
  if (secs >= 86_400) secs = 86_399;
  const h = Math.floor(secs / 3600);
  secs -= h * 3600;
  const m = Math.floor(secs / 60);
  return { h, m, s: secs - m * 60 };
}

// ── EDATE / EOMONTH ──────────────────────────────────────────────────────

function addMonths(
  serial: number,
  months: number,
  ctx: CallContext,
  endOfMonth: boolean,
): number | ErrorValue {
  const p = partsOf(serial, ctx);
  const total = p.year * 12 + (p.month - 1) + months;
  const y = Math.floor(total / 12);
  const m = total - y * 12 + 1;
  if (y < 1900 || y > 9999) return Errors.NUM;
  const dim = daysInMonth(y, m);
  const d = endOfMonth ? dim : Math.min(p.day === 0 ? 1 : p.day, dim);
  // The fictitious 1900-02-29 only exists as serial 60.
  if (!ctx.date1904 && y === 1900 && m === 2 && d === 29) return 60;
  return serialOf(y, m, d, ctx);
}

// ── DATEDIF ──────────────────────────────────────────────────────────────

function datedif(args: Arg[], ctx: CallContext): Scalar {
  const s = serialArg(args[0], ctx);
  if (isErr(s)) return s;
  const e = serialArg(args[1], ctx);
  if (isErr(e)) return e;
  const unit = str(args[2]);
  if (isErr(unit)) return unit;
  const start = Math.floor(s);
  const end = Math.floor(e);
  if (start > end) return Errors.NUM;
  const a = partsOf(start, ctx);
  const b = partsOf(end, ctx);
  const months = (b.year - a.year) * 12 + (b.month - a.month) - (b.day < a.day ? 1 : 0);
  switch (unit.toUpperCase()) {
    case 'D':
      return end - start;
    case 'M':
      return months;
    case 'Y':
      return Math.floor(months / 12);
    case 'YM':
      return months % 12;
    case 'MD': {
      // Days left after removing whole months: the start date moved forward by the whole months
      // (clamped to the month end) subtracted from the end date.
      const mid = addMonths(start, months, ctx, false);
      return isErr(mid) ? mid : end - mid;
    }
    case 'YD': {
      const years = Math.floor(months / 12);
      const mid = addMonths(start, years * 12, ctx, false);
      return isErr(mid) ? mid : end - mid;
    }
    default:
      return Errors.NUM;
  }
}

// ── WEEKDAY / WEEKNUM ────────────────────────────────────────────────────

/** First day of the week (0 = Sunday) and numbering for WEEKDAY return types. */
const WEEKDAY_TYPES: Record<number, { first: number; base: number }> = {
  1: { first: 0, base: 1 },
  2: { first: 1, base: 1 },
  3: { first: 1, base: 0 },
  11: { first: 1, base: 1 },
  12: { first: 2, base: 1 },
  13: { first: 3, base: 1 },
  14: { first: 4, base: 1 },
  15: { first: 5, base: 1 },
  16: { first: 6, base: 1 },
  17: { first: 0, base: 1 },
};

const WEEKNUM_FIRST: Record<number, number> = { 1: 0, 2: 1, 11: 1, 12: 2, 13: 3, 14: 4, 15: 5, 16: 6, 17: 0 };

function isoWeekNum(serial: number, ctx: CallContext): number {
  const n = Math.floor(serial);
  const wd = (serialWeekday(n, ctx.date1904) + 6) % 7; // Monday = 0
  const thursday = n - wd + 3;
  const year = partsOf(thursday, ctx).year;
  const jan1 = dateToSerial(year, 1, 1, ctx.date1904) as number;
  return Math.floor((thursday - jan1) / 7) + 1;
}

// ── working days ─────────────────────────────────────────────────────────

/** A 7-element weekend mask indexed by weekday (0 = Sunday). */
function weekendMask(a: Arg | undefined, ctx: CallContext): boolean[] | ErrorValue {
  const v = a === undefined || isOmitted(a) ? 1 : ctx.toScalar(force(a));
  if (v instanceof ErrorValue) return v;
  if (typeof v === 'string') {
    if (!/^[01]{7}$/.test(v) || v === '1111111') return Errors.VALUE;
    // Positions are Monday … Sunday.
    const mask = new Array(7).fill(false) as boolean[];
    for (let i = 0; i < 7; i++) mask[(i + 1) % 7] = v[i] === '1';
    return mask;
  }
  const n = toNumber(v === BLANK ? 1 : v);
  if (n instanceof ErrorValue) return n;
  const code = Math.trunc(n);
  const mask = new Array(7).fill(false) as boolean[];
  if (code >= 1 && code <= 7) {
    // 1: Sat+Sun, 2: Sun+Mon, … 7: Fri+Sat
    const first = (code + 5) % 7; // Saturday for code 1
    mask[first] = true;
    mask[(first + 1) % 7] = true;
    return mask;
  }
  if (code >= 11 && code <= 17) {
    mask[code - 11] = true; // 11: Sunday, 12: Monday … 17: Saturday
    return mask;
  }
  return Errors.NUM;
}

function holidaySet(a: Arg | undefined, ctx: CallContext): Set<number> | ErrorValue {
  const out = new Set<number>();
  if (a === undefined || isOmitted(a)) return out;
  for (const v of scalarsOf(a, ctx)) {
    if (v === BLANK) continue;
    if (v instanceof ErrorValue) return v;
    const n = toNumber(v, { date1904: ctx.date1904 });
    if (n instanceof ErrorValue) return n;
    if (n < 0) return Errors.NUM;
    out.add(Math.floor(n));
  }
  return out;
}

function networkdays(args: Arg[], ctx: CallContext, intl: boolean): Scalar {
  const s = serialArg(ctx.toScalar(force(args[0])), ctx);
  if (isErr(s)) return s;
  const e = serialArg(ctx.toScalar(force(args[1])), ctx);
  if (isErr(e)) return e;
  const mask = intl ? weekendMask(args[2], ctx) : weekendMask(undefined, ctx);
  if (isErr(mask)) return mask;
  const hol = holidaySet(intl ? args[3] : args[2], ctx);
  if (isErr(hol)) return hol;
  let a = Math.floor(s);
  let b = Math.floor(e);
  const sign = a <= b ? 1 : -1;
  if (sign < 0) [a, b] = [b, a];
  let count = 0;
  for (let d = a; d <= b; d++) {
    if (!mask[serialWeekday(d, ctx.date1904)] && !hol.has(d)) count++;
  }
  return sign * count;
}

function workday(args: Arg[], ctx: CallContext, intl: boolean): Scalar {
  const s = serialArg(ctx.toScalar(force(args[0])), ctx);
  if (isErr(s)) return s;
  const n = toNumber(ctx.toScalar(force(args[1])), { date1904: ctx.date1904 });
  if (isErr(n)) return n;
  const mask = intl ? weekendMask(args[2], ctx) : weekendMask(undefined, ctx);
  if (isErr(mask)) return mask;
  const hol = holidaySet(intl ? args[3] : args[2], ctx);
  if (isErr(hol)) return hol;
  let days = Math.trunc(n);
  let d = Math.floor(s);
  const step = days >= 0 ? 1 : -1;
  const max = ctx.date1904 ? MAX_DATE_SERIAL_1904 : MAX_DATE_SERIAL_1900;
  while (days !== 0) {
    d += step;
    if (d < 0 || d > max) return Errors.NUM;
    if (!mask[serialWeekday(d, ctx.date1904)] && !hol.has(d)) days -= step;
  }
  return d;
}

// ── DAYS360 / YEARFRAC ───────────────────────────────────────────────────

function lastDayOfFeb(y: number, m: number, d: number): boolean {
  return m === 2 && d === daysInMonth(y, 2);
}

export function days360(
  y1: number,
  m1: number,
  d1: number,
  y2: number,
  m2: number,
  d2: number,
  european: boolean,
): number {
  let a = d1;
  let b = d2;
  if (european) {
    if (a === 31) a = 30;
    if (b === 31) b = 30;
  } else {
    if (a === 31 || lastDayOfFeb(y1, m1, d1)) a = 30;
    if (b === 31 && a === 30) b = 30;
  }
  return (y2 - y1) * 360 + (m2 - m1) * 30 + (b - a);
}

/** US (NASD) 30/360 as used by YEARFRAC basis 0, where both month ends of February adjust. */
function days360US(y1: number, m1: number, d1: number, y2: number, m2: number, d2: number): number {
  let a = d1;
  let b = d2;
  const feb1 = lastDayOfFeb(y1, m1, d1);
  const feb2 = lastDayOfFeb(y2, m2, d2);
  if (feb1 && feb2) b = 30;
  if (feb1) a = 30;
  if (b === 31 && a >= 30) b = 30;
  if (a === 31) a = 30;
  return (y2 - y1) * 360 + (m2 - m1) * 30 + (b - a);
}

export function yearfrac(s: number, e: number, basis: number, ctx: CallContext): number | ErrorValue {
  let start = Math.floor(s);
  let end = Math.floor(e);
  if (start > end) [start, end] = [end, start];
  const a = partsOf(start, ctx);
  const b = partsOf(end, ctx);
  switch (basis) {
    case 0:
      return days360US(a.year, a.month, a.day, b.year, b.month, b.day) / 360;
    case 1: {
      if (start === end) return 0;
      const sameYear = a.year === b.year;
      const withinYear =
        sameYear || (b.year === a.year + 1 && (b.month < a.month || (b.month === a.month && b.day <= a.day)));
      if (withinYear) {
        let denom = 365;
        if (sameYear) denom = isLeapYear(a.year) ? 366 : 365;
        else if (
          (isLeapYear(a.year) && a.month < 3) ||
          (isLeapYear(b.year) && (b.month > 2 || (b.month === 2 && b.day === 29)))
        )
          denom = 366;
        return (end - start) / denom;
      }
      let total = 0;
      for (let y = a.year; y <= b.year; y++) total += isLeapYear(y) ? 366 : 365;
      return (end - start) / (total / (b.year - a.year + 1));
    }
    case 2:
      return (end - start) / 360;
    case 3:
      return (end - start) / 365;
    case 4:
      return days360(a.year, a.month, a.day, b.year, b.month, b.day, true) / 360;
    default:
      return Errors.NUM;
  }
}

export const dateTimeFunctions: FunctionDescriptor[] = [
  fn(
    'DATEVALUE',
    1,
    1,
    (args, ctx) => {
      const v = args[0];
      if (v instanceof ErrorValue) return v;
      if (typeof v !== 'string') return Errors.VALUE;
      const n = parseDateTimeText(v, { date1904: ctx.date1904, currentYear: ctx.now().getFullYear() });
      return n === undefined ? Errors.VALUE : Math.floor(n);
    },
    { lift: [0] },
  ),
  fn(
    'TIMEVALUE',
    1,
    1,
    (args, ctx) => {
      const v = args[0];
      if (v instanceof ErrorValue) return v;
      if (typeof v !== 'string') return Errors.VALUE;
      const n = parseDateTimeText(v, { date1904: ctx.date1904, currentYear: ctx.now().getFullYear() });
      return n === undefined ? Errors.VALUE : n - Math.floor(n);
    },
    { lift: [0] },
  ),
  ...(['YEAR', 'MONTH', 'DAY'] as const).map((name) =>
    fn(
      name,
      1,
      1,
      (args, ctx) => {
        const s = serialArg(args[0], ctx);
        if (isErr(s)) return s;
        const p = partsOf(s, ctx);
        return name === 'YEAR' ? p.year : name === 'MONTH' ? p.month : p.day;
      },
      { lift: 'all' },
    ),
  ),
  ...(['HOUR', 'MINUTE', 'SECOND'] as const).map((name) =>
    fn(
      name,
      1,
      1,
      (args, ctx) => {
        const v = args[0] === MISSING ? BLANK : (args[0] as Scalar);
        const n = toNumber(v, { date1904: ctx.date1904 });
        if (isErr(n)) return n;
        if (n < 0) return Errors.NUM;
        const t = hms(n);
        return name === 'HOUR' ? t.h : name === 'MINUTE' ? t.m : t.s;
      },
      { lift: 'all' },
    ),
  ),
  fn(
    'TIME',
    3,
    3,
    (args) => {
      const h = int(args[0]);
      if (isErr(h)) return h;
      const m = int(args[1]);
      if (isErr(m)) return m;
      const s = int(args[2]);
      if (isErr(s)) return s;
      if (h > 32767 || m > 32767 || s > 32767) return Errors.NUM;
      const total = h * 3600 + m * 60 + s;
      if (total < 0) return Errors.NUM;
      return (total % 86_400) / 86_400;
    },
    { lift: 'all' },
  ),
  fn(
    'WEEKDAY',
    1,
    2,
    (args, ctx) => {
      const s = serialArg(args[0], ctx);
      if (isErr(s)) return s;
      const t = int(args[1], 1);
      if (isErr(t)) return t;
      const spec = WEEKDAY_TYPES[t];
      if (!spec) return Errors.NUM;
      const wd = serialWeekday(Math.floor(s), ctx.date1904);
      return ((wd - spec.first + 7) % 7) + spec.base;
    },
    { lift: 'all' },
  ),
  fn(
    'WEEKNUM',
    1,
    2,
    (args, ctx) => {
      const s = serialArg(args[0], ctx);
      if (isErr(s)) return s;
      const t = int(args[1], 1);
      if (isErr(t)) return t;
      if (t === 21) return isoWeekNum(s, ctx);
      const first = WEEKNUM_FIRST[t];
      if (first === undefined) return Errors.NUM;
      const n = Math.floor(s);
      const year = partsOf(n, ctx).year;
      const jan1 = dateToSerial(year, 1, 1, ctx.date1904) as number;
      const offset = (serialWeekday(jan1, ctx.date1904) - first + 7) % 7;
      return Math.floor((n - jan1 + offset) / 7) + 1;
    },
    { lift: 'all' },
  ),
  fn(
    'ISOWEEKNUM',
    1,
    1,
    (args, ctx) => {
      const s = serialArg(args[0], ctx);
      return isErr(s) ? s : isoWeekNum(s, ctx);
    },
    { lift: 'all' },
  ),
  fn(
    'EDATE',
    2,
    2,
    (args, ctx) => {
      const s = serialArg(args[0], ctx);
      if (isErr(s)) return s;
      const m = int(args[1]);
      if (isErr(m)) return m;
      return addMonths(Math.floor(s), m, ctx, false);
    },
    { lift: 'all' },
  ),
  fn(
    'EOMONTH',
    2,
    2,
    (args, ctx) => {
      const s = serialArg(args[0], ctx);
      if (isErr(s)) return s;
      const m = int(args[1]);
      if (isErr(m)) return m;
      return addMonths(Math.floor(s), m, ctx, true);
    },
    { lift: 'all' },
  ),
  fn(
    'DAYS',
    2,
    2,
    (args, ctx) => {
      const e = serialArg(args[0], ctx);
      if (isErr(e)) return e;
      const s = serialArg(args[1], ctx);
      if (isErr(s)) return s;
      return Math.floor(e) - Math.floor(s);
    },
    { lift: 'all' },
  ),
  fn(
    'DAYS360',
    2,
    3,
    (args, ctx) => {
      const s = serialArg(args[0], ctx);
      if (isErr(s)) return s;
      const e = serialArg(args[1], ctx);
      if (isErr(e)) return e;
      const eu = args[2] === undefined || args[2] === MISSING ? 0 : num(args[2]);
      if (isErr(eu)) return eu;
      const a = partsOf(s, ctx);
      const b = partsOf(e, ctx);
      return days360(a.year, a.month, a.day, b.year, b.month, b.day, eu !== 0);
    },
    { lift: 'all' },
  ),
  fn(
    'YEARFRAC',
    2,
    3,
    (args, ctx) => {
      const s = serialArg(args[0], ctx);
      if (isErr(s)) return s;
      const e = serialArg(args[1], ctx);
      if (isErr(e)) return e;
      const b = int(args[2], 0);
      if (isErr(b)) return b;
      return yearfrac(s, e, b, ctx);
    },
    { lift: 'all' },
  ),
  fn('DATEDIF', 3, 3, datedif, { lift: 'all' }),
  fn('NETWORKDAYS', 2, 3, (args, ctx) => networkdays(args, ctx, false)),
  fn('NETWORKDAYS.INTL', 2, 4, (args, ctx) => networkdays(args, ctx, true)),
  fn('WORKDAY', 2, 3, (args, ctx) => workday(args, ctx, false)),
  fn('WORKDAY.INTL', 2, 4, (args, ctx) => workday(args, ctx, true)),
];
