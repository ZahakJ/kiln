/**
 * Financial functions: time value of money, cash-flow analysis, depreciation and securities.
 *
 * Coupon schedules follow Excel's rules (coupon dates are stepped back from maturity in whole months,
 * keeping month-end maturities on month ends); day counts follow the five `basis` conventions.
 * DOLLARDE/DOLLARFR use decimal arithmetic (decimal.js) so fractional prices convert exactly.
 */
import Decimal from 'decimal.js';
import type { CallContext } from '../engine/context.ts';
import type { Arg, FunctionDescriptor } from '../engine/functions/registry.ts';
import { parseNumberText, toNumber } from '../values/coerce.ts';
import { dateToSerial, daysInMonth, isLeapYear, serialToDate } from '../values/dates.ts';
import { Errors, ErrorValue } from '../values/errors.ts';
import { BLANK, type Scalar } from '../values/types.ts';
import { yearfrac } from './datetime.ts';
import { arrayOf, fn, isErr, num, numberOf, numbers, numeric, VARIADIC } from './lib.ts';

type R = number | ErrorValue;

// ── time value of money ──────────────────────────────────────────────────

export function pmt(r: number, n: number, pv: number, fv = 0, type = 0): number {
  if (r === 0) return -(pv + fv) / n;
  const f = (1 + r) ** n;
  return -(r * (fv + pv * f)) / ((1 + r * type) * (f - 1));
}

export function fv(r: number, n: number, p: number, pv = 0, type = 0): number {
  if (r === 0) return -(pv + p * n);
  const f = (1 + r) ** n;
  return -(pv * f + (p * (1 + r * type) * (f - 1)) / r);
}

export function pv(r: number, n: number, p: number, fvv = 0, type = 0): number {
  if (r === 0) return -(fvv + p * n);
  const f = (1 + r) ** n;
  return -(fvv + (p * (1 + r * type) * (f - 1)) / r) / f;
}

function nper(r: number, p: number, pvv: number, fvv = 0, type = 0): R {
  if (r === 0) return p === 0 ? Errors.NUM : -(pvv + fvv) / p;
  const num1 = p * (1 + r * type) - fvv * r;
  const den = p * (1 + r * type) + pvv * r;
  if (den === 0 || num1 / den <= 0) return Errors.NUM;
  return Math.log(num1 / den) / Math.log(1 + r);
}

function ipmt(r: number, per: number, n: number, pvv: number, fvv = 0, type = 0): R {
  if (per < 1 || per > n) return Errors.NUM;
  const p = pmt(r, n, pvv, fvv, type);
  if (type === 1 && per === 1) return 0;
  let i = fv(r, per - 1, p, pvv, type) * r;
  if (type === 1) i /= 1 + r;
  return i;
}

function rate(n: number, p: number, pvv: number, fvv = 0, type = 0, guess = 0.1): R {
  if (n <= 0) return Errors.NUM;
  if (pvv + p * n + fvv === 0) return 0;
  const f = (r: number) => {
    if (Math.abs(r) < 1e-12) return pvv + p * n + fvv;
    const g = (1 + r) ** n;
    return pvv * g + (p * (1 + r * type) * (g - 1)) / r + fvv;
  };
  let r = guess;
  for (let i = 0; i < 200; i++) {
    const y = f(r);
    const h = Math.max(1e-7, Math.abs(r) * 1e-7);
    const d = (f(r + h) - f(r - h)) / (2 * h);
    if (d === 0 || !Number.isFinite(d)) break;
    const next = r - y / d;
    if (!Number.isFinite(next) || next <= -1) break;
    if (Math.abs(next - r) < 1e-12) return next;
    r = next;
  }
  return Math.abs(f(r)) < 1e-7 ? r : Errors.NUM;
}

// ── cash flows ───────────────────────────────────────────────────────────

function npv(r: number, values: number[]): number {
  let s = 0;
  for (let i = 0; i < values.length; i++) s += (values[i] as number) / (1 + r) ** (i + 1);
  return s;
}

/** Finds a root of f near `guess` (Newton, then a bracketing scan with bisection). */
function solve(f: (x: number) => number, df: (x: number) => number, guess: number): R {
  let x = guess;
  for (let i = 0; i < 100; i++) {
    const y = f(x);
    const d = df(x);
    if (!Number.isFinite(y) || !Number.isFinite(d) || d === 0) break;
    const next = x - y / d;
    if (!Number.isFinite(next) || next <= -1) break;
    if (Math.abs(next - x) <= 1e-12 * Math.max(1, Math.abs(next))) return next;
    x = next;
  }
  // Bracket a sign change scanning outward from the guess, then bisect.
  const grid: number[] = [];
  for (let k = -0.99; k < 10; k += 0.01) grid.push(k);
  grid.sort((a, b) => Math.abs(a - guess) - Math.abs(b - guess));
  for (const a of grid) {
    const b = a + 0.01;
    const fa = f(a);
    const fb = f(b);
    if (Number.isFinite(fa) && Number.isFinite(fb) && fa * fb <= 0) {
      let lo = a;
      let hi = b;
      let flo = fa;
      for (let i = 0; i < 200; i++) {
        const m = (lo + hi) / 2;
        const fm = f(m);
        if (fm === 0) return m;
        if (flo * fm < 0) hi = m;
        else {
          lo = m;
          flo = fm;
        }
      }
      return (lo + hi) / 2;
    }
  }
  return Errors.NUM;
}

function irr(values: number[], guess: number): R {
  if (!values.some((v) => v > 0) || !values.some((v) => v < 0)) return Errors.NUM;
  const f = (r: number) => {
    let s = 0;
    for (let i = 0; i < values.length; i++) s += (values[i] as number) / (1 + r) ** i;
    return s;
  };
  const df = (r: number) => {
    let s = 0;
    for (let i = 1; i < values.length; i++) s -= (i * (values[i] as number)) / (1 + r) ** (i + 1);
    return s;
  };
  return solve(f, df, guess);
}

function xnpv(r: number, values: number[], dates: number[]): number {
  const d0 = dates[0] as number;
  let s = 0;
  for (let i = 0; i < values.length; i++)
    s += (values[i] as number) / (1 + r) ** (((dates[i] as number) - d0) / 365);
  return s;
}

/** Values and dates for XNPV/XIRR: equal lengths, numbers only, dates truncated and ≥ the first. */
function flowsAndDates(
  va: Arg | undefined,
  da: Arg | undefined,
  ctx: CallContext,
): { values: number[]; dates: number[] } | ErrorValue {
  const v = arrayOf(va, ctx);
  if (isErr(v)) return v;
  const d = arrayOf(da, ctx);
  if (isErr(d)) return d;
  const vs = v.flat();
  const ds = d.flat();
  if (vs.length !== ds.length) return Errors.NUM;
  const values: number[] = [];
  const dates: number[] = [];
  for (let i = 0; i < vs.length; i++) {
    const x = vs[i] as Scalar;
    const t = ds[i] as Scalar;
    if (x instanceof ErrorValue) return x;
    if (t instanceof ErrorValue) return t;
    if (typeof x !== 'number') return Errors.VALUE;
    const tn = typeof t === 'string' ? parseNumberText(t) : typeof t === 'number' ? t : undefined;
    if (tn === undefined) return Errors.VALUE;
    values.push(x);
    dates.push(Math.floor(tn));
  }
  const d0 = dates[0] as number;
  if (dates.some((t) => t < d0 || t < 0)) return Errors.NUM;
  return { values, dates };
}

// ── depreciation ─────────────────────────────────────────────────────────

function ddb(cost: number, salvage: number, life: number, period: number, factor: number): number {
  let r = factor / life;
  let oldValue: number;
  if (r >= 1) {
    r = 1;
    oldValue = period === 1 ? cost : 0;
  } else oldValue = cost * (1 - r) ** (period - 1);
  const newValue = cost * (1 - r) ** period;
  const d = newValue < salvage ? oldValue - salvage : oldValue - newValue;
  return d < 0 ? 0 : d;
}

function interVdb(
  cost: number,
  salvage: number,
  life: number,
  life1: number,
  period: number,
  factor: number,
): number {
  let vdb = 0;
  const intEnd = Math.ceil(period - 1e-12);
  let sln = 0;
  let remaining = cost - salvage;
  let nowSln = false;
  for (let i = 1; i <= intEnd; i++) {
    let term: number;
    if (!nowSln) {
      const d = ddb(cost, salvage, life, i, factor);
      sln = remaining / (life1 - (i - 1));
      if (sln > d) {
        term = sln;
        nowSln = true;
      } else {
        term = d;
        remaining -= d;
      }
    } else term = sln;
    if (i === intEnd) term *= period + 1 - intEnd;
    vdb += term;
  }
  return vdb;
}

function vdb(
  cost: number,
  salvage: number,
  life: number,
  start: number,
  end: number,
  factor: number,
  noSwitch: number,
): R {
  if (start < 0 || end < start || end > life || cost < 0 || salvage < 0 || factor <= 0 || life <= 0)
    return Errors.NUM;
  const intStart = Math.floor(start + 1e-12);
  const intEnd = Math.ceil(end - 1e-12);
  if (noSwitch) {
    let v = 0;
    for (let i = intStart + 1; i <= intEnd; i++) {
      let term = ddb(cost, salvage, life, i, factor);
      if (i === intStart + 1) term *= Math.min(end, intStart + 1) - start;
      else if (i === intEnd) term *= end + 1 - intEnd;
      v += term;
    }
    return v;
  }
  const life1 = Number.isInteger(life) ? life : Math.floor(life) + 1;
  let part = 0;
  if (start !== intStart) {
    const tempValue = cost - interVdb(cost, salvage, life, life1, intStart, factor);
    part += (start - intStart) * interVdb(tempValue, salvage, life, life - intStart, 1, factor);
  }
  if (end !== intEnd) {
    const tempStart = intEnd - 1;
    const tempValue = cost - interVdb(cost, salvage, life, life1, tempStart, factor);
    part += (intEnd - end) * interVdb(tempValue, salvage, life, life - tempStart, 1, factor);
  }
  const c = cost - interVdb(cost, salvage, life, life1, intStart, factor);
  return interVdb(c, salvage, life, life - intStart, intEnd - intStart, factor) - part;
}

function db(cost: number, salvage: number, life0: number, period0: number, month0: number): R {
  const life = Math.trunc(life0);
  const period = Math.trunc(period0);
  const month = Math.trunc(month0);
  if (cost < 0 || salvage < 0 || life <= 0 || period <= 0 || month < 1 || month > 12) return Errors.NUM;
  if (period > life + 1 || (month === 12 && period > life)) return Errors.NUM;
  if (cost === 0) return 0;
  const r = Math.round((1 - (salvage / cost) ** (1 / life)) * 1000) / 1000;
  let total = (cost * r * month) / 12;
  if (period === 1) return total;
  for (let p = 2; p <= period; p++) {
    const d = p === life + 1 ? ((cost - total) * r * (12 - month)) / 12 : (cost - total) * r;
    if (p === period) return d;
    total += d;
  }
  return 0;
}

// ── securities: day counts and coupon schedules ──────────────────────────

interface Ymd {
  y: number;
  m: number;
  d: number;
}

function ymd(serial: number, ctx: CallContext): Ymd {
  const p = serialToDate(serial, ctx.date1904);
  return { y: p.year, m: p.month, d: p.day };
}

function serial(p: Ymd, ctx: CallContext): number {
  return dateToSerial(p.y, p.m, p.d, ctx.date1904) as number;
}

/** A coupon date `months` from `base`, keeping end-of-month bases on month ends. */
function shiftMonths(base: Ymd, months: number): Ymd {
  const total = base.y * 12 + base.m - 1 + months;
  const y = Math.floor(total / 12);
  const m = total - y * 12 + 1;
  const eom = base.d >= daysInMonth(base.y, base.m);
  const d = eom ? daysInMonth(y, m) : Math.min(base.d, daysInMonth(y, m));
  return { y, m, d };
}

function cmpYmd(a: Ymd, b: Ymd): number {
  return a.y - b.y || a.m - b.m || a.d - b.d;
}

/** Previous coupon date (≤ settlement) and the next one, stepping from maturity. */
function couponDates(settle: Ymd, mat: Ymd, freq: number): { pcd: Ymd; ncd: Ymd; n: number } {
  const step = 12 / freq;
  let k = 0;
  // Number of whole periods between settlement and maturity, found by stepping back.
  let pcd = mat;
  while (cmpYmd(pcd, settle) > 0) {
    k++;
    pcd = shiftMonths(mat, -k * step);
  }
  const ncd = shiftMonths(mat, -(k - 1) * step);
  return { pcd, ncd, n: k };
}

/** Day count between two dates for a basis (30/360 variants or actual days). */
function dayCount(a: Ymd, b: Ymd, basis: number, ctx: CallContext): number {
  if (basis === 0 || basis === 4) {
    let d1 = a.d;
    let d2 = b.d;
    if (basis === 0) {
      const feb1 = a.m === 2 && d1 === daysInMonth(a.y, 2);
      const feb2 = b.m === 2 && d2 === daysInMonth(b.y, 2);
      if (feb1 && feb2) d2 = 30;
      if (feb1) d1 = 30;
      if (d2 === 31 && d1 >= 30) d2 = 30;
      if (d1 === 31) d1 = 30;
    } else {
      if (d1 === 31) d1 = 30;
      if (d2 === 31) d2 = 30;
    }
    return (b.y - a.y) * 360 + (b.m - a.m) * 30 + (d2 - d1);
  }
  return serial(b, ctx) - serial(a, ctx);
}

interface Bond {
  settle: number;
  mat: number;
  s: Ymd;
  m: Ymd;
  freq: number;
  basis: number;
}

/** Validates the common settlement/maturity/frequency/basis arguments. */
function bondArgs(
  settleV: number,
  matV: number,
  freqV: number,
  basisV: number,
  ctx: CallContext,
): Bond | ErrorValue {
  const settle = Math.trunc(settleV);
  const mat = Math.trunc(matV);
  const freq = Math.trunc(freqV);
  const basis = Math.trunc(basisV);
  if (![1, 2, 4].includes(freq) || basis < 0 || basis > 4 || settle >= mat || settle < 0) return Errors.NUM;
  return { settle, mat, s: ymd(settle, ctx), m: ymd(mat, ctx), freq, basis };
}

function coupdays(b: Bond, ctx: CallContext): number {
  if (b.basis === 1) {
    const { pcd, ncd } = couponDates(b.s, b.m, b.freq);
    return serial(ncd, ctx) - serial(pcd, ctx);
  }
  return (b.basis === 3 ? 365 : 360) / b.freq;
}

function coupdaybs(b: Bond, ctx: CallContext): number {
  const { pcd } = couponDates(b.s, b.m, b.freq);
  return dayCount(pcd, b.s, b.basis, ctx);
}

function coupdaysnc(b: Bond, ctx: CallContext): number {
  if (b.basis !== 0 && b.basis !== 4) {
    const { ncd } = couponDates(b.s, b.m, b.freq);
    return serial(ncd, ctx) - b.settle;
  }
  return coupdays(b, ctx) - coupdaybs(b, ctx);
}

function price(b: Bond, rateV: number, yld: number, redemption: number, ctx: CallContext): number {
  const E = coupdays(b, ctx);
  const A = coupdaybs(b, ctx);
  const DSC = coupdaysnc(b, ctx);
  const N = couponDates(b.s, b.m, b.freq).n;
  const f = b.freq;
  const c = (100 * rateV) / f;
  if (N === 1) {
    return (redemption + c) / (1 + ((DSC / E) * yld) / f) - (c * A) / E;
  }
  const base = 1 + yld / f;
  let p = redemption / base ** (N - 1 + DSC / E);
  for (let k = 1; k <= N; k++) p += c / base ** (k - 1 + DSC / E);
  return p - (c * A) / E;
}

function yieldFn(b: Bond, rateV: number, pr: number, redemption: number, ctx: CallContext): R {
  const N = couponDates(b.s, b.m, b.freq).n;
  if (N === 1) {
    const E = coupdays(b, ctx);
    const A = coupdaybs(b, ctx);
    const DSR = E - A;
    const c = rateV / b.freq;
    const num1 = redemption / 100 + c - (pr / 100 + (A / E) * c);
    const den = pr / 100 + (A / E) * c;
    return ((num1 / den) * b.freq * E) / DSR;
  }
  const f = (y: number) => price(b, rateV, y, redemption, ctx) - pr;
  const df = (y: number) => (f(y + 1e-7) - f(y - 1e-7)) / 2e-7;
  return solve(f, df, 0.05);
}

/**
 * Macaulay duration in years: cash flows at (k − 1 + DSC/E) coupon periods from settlement, weighted
 * by their present values.
 */
function duration(b: Bond, coupon: number, yld: number, ctx: CallContext): number {
  const n = couponDates(b.s, b.m, b.freq).n;
  const first = coupdaysnc(b, ctx) / coupdays(b, ctx);
  const c = (coupon * 100) / b.freq;
  const y = 1 + yld / b.freq;
  let dur = 0;
  let p = 0;
  for (let k = 1; k <= n; k++) {
    const t = k - 1 + first;
    const cf = k === n ? c + 100 : c;
    const pvk = cf / y ** t;
    dur += t * pvk;
    p += pvk;
  }
  return dur / p / b.freq;
}

/** Year fraction between two serials with the basis; validates the basis. */
function yf(a: number, b: number, basis: number, ctx: CallContext): R {
  const bs = Math.trunc(basis);
  if (bs < 0 || bs > 4) return Errors.NUM;
  return yearfrac(Math.trunc(a), Math.trunc(b), bs, ctx);
}

/**
 * Fraction of a year for the discount-security formulas (INTRATE, RECEIVED, PRICEDISC, ACCRINTM):
 * days / B, where for basis 1 (actual/actual) B is the length of the settlement year.
 */
function dayFraction(a: number, b: number, basis: number, ctx: CallContext): R {
  const bs = Math.trunc(basis);
  if (bs !== 1) return yf(a, b, bs, ctx);
  const start = Math.trunc(a);
  const year = serialToDate(start, ctx.date1904).year;
  return (Math.trunc(b) - start) / (isLeapYear(year) ? 366 : 365);
}

/** Securities functions: every argument lifted and numeric; dates as serials. */
function sec(
  name: string,
  min: number,
  max: number,
  body: (ctx: CallContext, ...xs: number[]) => R,
  defaults: readonly number[] = [],
): FunctionDescriptor {
  return fn(
    name,
    min,
    max,
    (args, ctx) => {
      const xs: number[] = [];
      for (let i = 0; i < max; i++) {
        const a = args[i];
        const v = a === undefined ? (defaults[i - min] ?? 0) : num(a);
        if (isErr(v)) return v;
        xs.push(v);
      }
      const r = body(ctx, ...xs);
      return typeof r === 'number' && !Number.isFinite(r) ? Errors.NUM : r;
    },
    { lift: 'all' },
  );
}

function tbillDays(s: number, m: number): R {
  const settle = Math.trunc(s);
  const mat = Math.trunc(m);
  if (settle >= mat || mat - settle > 365) return Errors.NUM;
  return mat - settle;
}

// ── DOLLARDE / DOLLARFR ──────────────────────────────────────────────────

function dollarConvert(x: number, fraction: number, toDecimal: boolean): R {
  const f = Math.trunc(fraction);
  if (f < 0) return Errors.NUM;
  if (f === 0) return Errors.DIV0;
  const sign = x < 0 ? -1 : 1;
  const ax = new Decimal(Math.abs(x));
  const intPart = ax.floor();
  const frac = ax.minus(intPart);
  const scale = new Decimal(10).pow(Math.ceil(Math.log10(f)));
  const res = toDecimal ? intPart.plus(frac.times(scale).div(f)) : intPart.plus(frac.times(f).div(scale));
  return sign * res.toNumber();
}

export const financialFunctions: FunctionDescriptor[] = [
  numeric('PMT', 3, 5, (r, n, p, f, t) => (n === 0 ? Errors.NUM : pmt(r, n, p, f, t ? 1 : 0)), [0, 0]),
  numeric('FV', 3, 5, (r, n, p, v, t) => fv(r, n, p, v, t ? 1 : 0), [0, 0]),
  numeric('PV', 3, 5, (r, n, p, f, t) => pv(r, n, p, f, t ? 1 : 0), [0, 0]),
  numeric('NPER', 3, 5, (r, p, v, f, t) => nper(r, p, v, f, t ? 1 : 0), [0, 0]),
  numeric('RATE', 3, 6, (n, p, v, f, t, g) => rate(n, p, v, f, t ? 1 : 0, g), [0, 0, 0.1]),
  numeric('IPMT', 4, 6, (r, per, n, v, f, t) => ipmt(r, per, n, v, f, t ? 1 : 0), [0, 0]),
  numeric(
    'PPMT',
    4,
    6,
    (r, per, n, v, f, t) => {
      const i = ipmt(r, per, n, v, f, t ? 1 : 0);
      return isErr(i) ? i : pmt(r, n, v, f, t ? 1 : 0) - i;
    },
    [0, 0],
  ),
  ...(['CUMIPMT', 'CUMPRINC'] as const).map((name) =>
    numeric(name, 6, 6, (r, n0, v, s0, e0, t) => {
      const n = Math.trunc(n0);
      const s = Math.trunc(s0);
      const e = Math.trunc(e0);
      if (r <= 0 || n <= 0 || v <= 0 || s < 1 || e < s || e > n || (t !== 0 && t !== 1)) return Errors.NUM;
      const p = pmt(r, n, v, 0, t);
      let acc = 0;
      for (let per = s; per <= e; per++) {
        const i = ipmt(r, per, n, v, 0, t) as number;
        acc += name === 'CUMIPMT' ? i : p - i;
      }
      return acc;
    }),
  ),
  numeric('ISPMT', 4, 4, (r, per, n, v) => (n === 0 ? Errors.DIV0 : v * r * (per / n - 1))),
  fn('NPV', 2, VARIADIC, (args, ctx) => {
    const r = numberOf(args[0], ctx);
    if (isErr(r)) return r;
    const xs = numbers(args.slice(1), ctx);
    if (isErr(xs)) return xs;
    if (r === -1) return Errors.DIV0;
    return npv(r, xs);
  }),
  fn('XNPV', 3, 3, (args, ctx) => {
    const r = numberOf(args[0], ctx);
    if (isErr(r)) return r;
    const fd = flowsAndDates(args[1], args[2], ctx);
    if (isErr(fd)) return fd;
    if (r <= -1) return Errors.NUM;
    return xnpv(r, fd.values, fd.dates);
  }),
  fn('IRR', 1, 2, (args, ctx) => {
    const v = numbers([args[0] as Arg], ctx);
    if (isErr(v)) return v;
    const g = numberOf(args[1], ctx, 0.1);
    if (isErr(g)) return g;
    return irr(v, g);
  }),
  fn('XIRR', 2, 3, (args, ctx) => {
    const fd = flowsAndDates(args[0], args[1], ctx);
    if (isErr(fd)) return fd;
    const g = numberOf(args[2], ctx, 0.1);
    if (isErr(g)) return g;
    const { values, dates } = fd;
    if (!values.some((v) => v > 0) || !values.some((v) => v < 0)) return Errors.NUM;
    const d0 = dates[0] as number;
    const f = (r: number) => xnpv(r, values, dates);
    const df = (r: number) => {
      let s = 0;
      for (let i = 0; i < values.length; i++) {
        const t = ((dates[i] as number) - d0) / 365;
        s -= (t * (values[i] as number)) / (1 + r) ** (t + 1);
      }
      return s;
    };
    return solve(f, df, g);
  }),
  fn('MIRR', 3, 3, (args, ctx) => {
    const v = numbers([args[0] as Arg], ctx);
    if (isErr(v)) return v;
    const fr = numberOf(args[1], ctx);
    if (isErr(fr)) return fr;
    const rr = numberOf(args[2], ctx);
    if (isErr(rr)) return rr;
    const n = v.length;
    const pos = v.map((x) => (x > 0 ? x : 0));
    const neg = v.map((x) => (x < 0 ? x : 0));
    if (!pos.some((x) => x > 0) || !neg.some((x) => x < 0)) return Errors.DIV0;
    const top = -npv(rr, pos) * (1 + rr) ** n;
    const bottom = npv(fr, neg) * (1 + fr);
    return (top / bottom) ** (1 / (n - 1)) - 1;
  }),
  fn('FVSCHEDULE', 2, 2, (args, ctx) => {
    const p = numberOf(args[0], ctx);
    if (isErr(p)) return p;
    const arr = arrayOf(args[1], ctx);
    if (isErr(arr)) return arr;
    let v = p;
    for (const x of arr.flat()) {
      if (x instanceof ErrorValue) return x;
      if (x === BLANK) continue;
      const r = toNumber(x);
      if (isErr(r)) return Errors.VALUE;
      v *= 1 + r;
    }
    return v;
  }),
  numeric('EFFECT', 2, 2, (nom, p0) => {
    const p = Math.trunc(p0);
    if (nom <= 0 || p < 1) return Errors.NUM;
    return (1 + nom / p) ** p - 1;
  }),
  numeric('NOMINAL', 2, 2, (eff, p0) => {
    const p = Math.trunc(p0);
    if (eff <= 0 || p < 1) return Errors.NUM;
    return p * ((1 + eff) ** (1 / p) - 1);
  }),
  numeric('PDURATION', 3, 3, (r, a, b) =>
    r <= 0 || a <= 0 || b <= 0 ? Errors.NUM : (Math.log(b) - Math.log(a)) / Math.log1p(r),
  ),
  numeric('RRI', 3, 3, (n, a, b) => (n <= 0 || a === 0 ? Errors.NUM : (b / a) ** (1 / n) - 1)),
  numeric('SLN', 3, 3, (c, s, l) => (l === 0 ? Errors.DIV0 : (c - s) / l)),
  numeric('SYD', 4, 4, (c, s, l, per) => {
    if (l <= 0 || per <= 0 || per > l || s < 0) return Errors.NUM;
    return ((c - s) * (l - per + 1) * 2) / (l * (l + 1));
  }),
  numeric('DB', 4, 5, db, [12]),
  numeric(
    'DDB',
    4,
    5,
    (c, s, l, per, f) => {
      if (c < 0 || s < 0 || l <= 0 || per <= 0 || per > l || f <= 0) return Errors.NUM;
      return ddb(c, s, l, per, f);
    },
    [2],
  ),
  numeric('VDB', 5, 7, vdb, [2, 0]),
  numeric('DOLLARDE', 2, 2, (x, f) => dollarConvert(x, f, true)),
  numeric('DOLLARFR', 2, 2, (x, f) => dollarConvert(x, f, false)),
  // securities
  sec(
    'ACCRINTM',
    3,
    5,
    (ctx, issue, s, r, par, basis) => {
      if (r <= 0 || par <= 0 || Math.trunc(issue) >= Math.trunc(s)) return Errors.NUM;
      const y = dayFraction(issue, s, basis, ctx);
      return isErr(y) ? y : par * r * y;
    },
    [1000, 0],
  ),
  sec(
    'ACCRINT',
    6,
    8,
    (ctx, issue, _first, s, r, par, freq, basis) => {
      const f = Math.trunc(freq);
      if (r <= 0 || par <= 0 || ![1, 2, 4].includes(f) || Math.trunc(issue) >= Math.trunc(s))
        return Errors.NUM;
      const y = yf(issue, s, basis, ctx);
      return isErr(y) ? y : par * r * y;
    },
    [0, 1],
  ),
  sec(
    'COUPDAYBS',
    3,
    4,
    (ctx, s, m, f, b) => {
      const bd = bondArgs(s, m, f, b, ctx);
      return isErr(bd) ? bd : coupdaybs(bd, ctx);
    },
    [0],
  ),
  sec(
    'COUPDAYS',
    3,
    4,
    (ctx, s, m, f, b) => {
      const bd = bondArgs(s, m, f, b, ctx);
      return isErr(bd) ? bd : coupdays(bd, ctx);
    },
    [0],
  ),
  sec(
    'COUPDAYSNC',
    3,
    4,
    (ctx, s, m, f, b) => {
      const bd = bondArgs(s, m, f, b, ctx);
      return isErr(bd) ? bd : coupdaysnc(bd, ctx);
    },
    [0],
  ),
  sec(
    'COUPNCD',
    3,
    4,
    (ctx, s, m, f, b) => {
      const bd = bondArgs(s, m, f, b, ctx);
      return isErr(bd) ? bd : serial(couponDates(bd.s, bd.m, bd.freq).ncd, ctx);
    },
    [0],
  ),
  sec(
    'COUPPCD',
    3,
    4,
    (ctx, s, m, f, b) => {
      const bd = bondArgs(s, m, f, b, ctx);
      return isErr(bd) ? bd : serial(couponDates(bd.s, bd.m, bd.freq).pcd, ctx);
    },
    [0],
  ),
  sec(
    'COUPNUM',
    3,
    4,
    (ctx, s, m, f, b) => {
      const bd = bondArgs(s, m, f, b, ctx);
      return isErr(bd) ? bd : couponDates(bd.s, bd.m, bd.freq).n;
    },
    [0],
  ),
  sec(
    'PRICE',
    6,
    7,
    (ctx, s, m, r, y, red, f, b) => {
      const bd = bondArgs(s, m, f, b, ctx);
      if (isErr(bd)) return bd;
      if (r < 0 || y < 0 || red <= 0) return Errors.NUM;
      return price(bd, r, y, red, ctx);
    },
    [0],
  ),
  sec(
    'YIELD',
    6,
    7,
    (ctx, s, m, r, pr, red, f, b) => {
      const bd = bondArgs(s, m, f, b, ctx);
      if (isErr(bd)) return bd;
      if (r < 0 || pr <= 0 || red <= 0) return Errors.NUM;
      return yieldFn(bd, r, pr, red, ctx);
    },
    [0],
  ),
  sec(
    'DURATION',
    5,
    6,
    (ctx, s, m, c, y, f, b) => {
      const bd = bondArgs(s, m, f, b, ctx);
      if (isErr(bd)) return bd;
      if (c < 0 || y < 0) return Errors.NUM;
      return duration(bd, c, y, ctx);
    },
    [0],
  ),
  sec(
    'MDURATION',
    5,
    6,
    (ctx, s, m, c, y, f, b) => {
      const bd = bondArgs(s, m, f, b, ctx);
      if (isErr(bd)) return bd;
      if (c < 0 || y < 0) return Errors.NUM;
      return duration(bd, c, y, ctx) / (1 + y / bd.freq);
    },
    [0],
  ),
  sec(
    'DISC',
    4,
    5,
    (ctx, s, m, pr, red, b) => {
      if (pr <= 0 || red <= 0 || Math.trunc(s) >= Math.trunc(m)) return Errors.NUM;
      const y = yf(s, m, b, ctx);
      return isErr(y) ? y : (red - pr) / red / y;
    },
    [0],
  ),
  sec(
    'INTRATE',
    4,
    5,
    (ctx, s, m, inv, red, b) => {
      if (inv <= 0 || red <= 0 || Math.trunc(s) >= Math.trunc(m)) return Errors.NUM;
      const y = dayFraction(s, m, b, ctx);
      return isErr(y) ? y : (red - inv) / inv / y;
    },
    [0],
  ),
  sec(
    'RECEIVED',
    4,
    5,
    (ctx, s, m, inv, disc, b) => {
      if (inv <= 0 || disc <= 0 || Math.trunc(s) >= Math.trunc(m)) return Errors.NUM;
      const y = dayFraction(s, m, b, ctx);
      if (isErr(y)) return y;
      const d = 1 - disc * y;
      return d === 0 ? Errors.NUM : inv / d;
    },
    [0],
  ),
  sec(
    'PRICEDISC',
    4,
    5,
    (ctx, s, m, disc, red, b) => {
      if (disc <= 0 || red <= 0 || Math.trunc(s) >= Math.trunc(m)) return Errors.NUM;
      const y = dayFraction(s, m, b, ctx);
      return isErr(y) ? y : red * (1 - disc * y);
    },
    [0],
  ),
  sec(
    'YIELDDISC',
    4,
    5,
    (ctx, s, m, pr, red, b) => {
      if (pr <= 0 || red <= 0 || Math.trunc(s) >= Math.trunc(m)) return Errors.NUM;
      const y = yf(s, m, b, ctx);
      return isErr(y) ? y : (red / pr - 1) / y;
    },
    [0],
  ),
  sec(
    'PRICEMAT',
    5,
    6,
    (ctx, s, m, issue, r, y, b) => {
      if (r < 0 || y < 0 || Math.trunc(s) >= Math.trunc(m)) return Errors.NUM;
      const im = yf(issue, m, b, ctx);
      const is = yf(issue, s, b, ctx);
      const sm = yf(s, m, b, ctx);
      if (isErr(im)) return im;
      if (isErr(is)) return is;
      if (isErr(sm)) return sm;
      return ((1 + im * r) / (1 + sm * y) - is * r) * 100;
    },
    [0],
  ),
  sec(
    'YIELDMAT',
    5,
    6,
    (ctx, s, m, issue, r, pr, b) => {
      if (r < 0 || pr <= 0 || Math.trunc(s) >= Math.trunc(m)) return Errors.NUM;
      const im = yf(issue, m, b, ctx);
      const is = yf(issue, s, b, ctx);
      const sm = yf(s, m, b, ctx);
      if (isErr(im)) return im;
      if (isErr(is)) return is;
      if (isErr(sm)) return sm;
      return ((1 + im * r) / (pr / 100 + is * r) - 1) / sm;
    },
    [0],
  ),
  sec('TBILLPRICE', 3, 3, (_ctx, s, m, disc) => {
    const dsm = tbillDays(s, m);
    if (isErr(dsm)) return dsm;
    if (disc <= 0) return Errors.NUM;
    const p = 100 * (1 - (disc * dsm) / 360);
    return p <= 0 ? Errors.NUM : p;
  }),
  sec('TBILLYIELD', 3, 3, (_ctx, s, m, pr) => {
    const dsm = tbillDays(s, m);
    if (isErr(dsm)) return dsm;
    if (pr <= 0) return Errors.NUM;
    return ((100 - pr) / pr) * (360 / dsm);
  }),
  sec('TBILLEQ', 3, 3, (_ctx, s, m, disc) => {
    const dsm = tbillDays(s, m);
    if (isErr(dsm)) return dsm;
    if (disc <= 0) return Errors.NUM;
    if (dsm <= 182) return (365 * disc) / (360 - disc * dsm);
    const p = 100 * (1 - (disc * dsm) / 360);
    if (p <= 0) return Errors.NUM;
    const t = dsm / 365;
    return (-2 * t + 2 * Math.sqrt(t * t - (2 * t - 1) * (1 - 100 / p))) / (2 * t - 1);
  }),
];
