/**
 * Pure aggregations over number lists, shared by the statistical functions, SUBTOTAL, AGGREGATE and
 * the database functions. Each returns a number or the Excel error for an invalid input.
 */
import { Errors, type ErrorValue } from '../values/errors.ts';
import { round15 } from '../values/number.ts';
import { devsq, mean, sortedAsc, sum } from './lib.ts';

export type Agg = (xs: number[]) => number | ErrorValue;

export const average: Agg = (xs) => (xs.length === 0 ? Errors.DIV0 : mean(xs));
export const count: Agg = (xs) => xs.length;
export const maxOf: Agg = (xs) => (xs.length === 0 ? 0 : xs.reduce((a, b) => (b > a ? b : a), -Infinity));
export const minOf: Agg = (xs) => (xs.length === 0 ? 0 : xs.reduce((a, b) => (b < a ? b : a), Infinity));
export const sumOf: Agg = (xs) => sum(xs);
export const product: Agg = (xs) => {
  if (xs.length === 0) return 0;
  let p = 1;
  for (const x of xs) p *= x;
  return Number.isFinite(p) ? p : Errors.NUM;
};
export const varS: Agg = (xs) => (xs.length < 2 ? Errors.DIV0 : devsq(xs) / (xs.length - 1));
export const varP: Agg = (xs) => (xs.length < 1 ? Errors.DIV0 : devsq(xs) / xs.length);
export const stdevS: Agg = (xs) => {
  const v = varS(xs);
  return typeof v === 'number' ? Math.sqrt(v) : v;
};
export const stdevP: Agg = (xs) => {
  const v = varP(xs);
  return typeof v === 'number' ? Math.sqrt(v) : v;
};

export const median: Agg = (xs) => {
  if (xs.length === 0) return Errors.NUM;
  const s = sortedAsc(xs);
  const m = s.length >> 1;
  return s.length % 2 ? (s[m] as number) : ((s[m - 1] as number) + (s[m] as number)) / 2;
};

/** Most frequent value; ties go to the value that appears first. #N/A when nothing repeats. */
export const modeSngl: Agg = (xs) => {
  const m = modes(xs);
  return m.length === 0 ? Errors.NA : (m[0] as number);
};

/** All values with the highest frequency (> 1), in order of first appearance. */
export function modes(xs: readonly number[]): number[] {
  const counts = new Map<number, number>();
  const order: number[] = [];
  for (const x of xs) {
    const k = round15(x);
    const c = counts.get(k);
    if (c === undefined) {
      counts.set(k, 1);
      order.push(k);
    } else counts.set(k, c + 1);
  }
  let best = 1;
  for (const c of counts.values()) if (c > best) best = c;
  if (best < 2) return [];
  return order.filter((k) => counts.get(k) === best);
}

/** k-th largest (1-based). */
export function large(xs: readonly number[], k: number): number | ErrorValue {
  const kk = Math.ceil(k);
  if (xs.length === 0 || kk < 1 || kk > xs.length) return Errors.NUM;
  return sortedAsc(xs)[xs.length - kk] as number;
}

export function small(xs: readonly number[], k: number): number | ErrorValue {
  const kk = Math.ceil(k);
  if (xs.length === 0 || kk < 1 || kk > xs.length) return Errors.NUM;
  return sortedAsc(xs)[kk - 1] as number;
}

export function percentileInc(xs: readonly number[], p: number): number | ErrorValue {
  if (xs.length === 0 || p < 0 || p > 1) return Errors.NUM;
  const s = sortedAsc(xs);
  const h = (s.length - 1) * p;
  const lo = Math.floor(h);
  const frac = h - lo;
  const a = s[lo] as number;
  return frac === 0 ? a : a + frac * ((s[lo + 1] as number) - a);
}

export function percentileExc(xs: readonly number[], p: number): number | ErrorValue {
  const n = xs.length;
  if (n === 0 || p <= 0 || p >= 1) return Errors.NUM;
  const h = (n + 1) * p;
  if (h < 1 || h > n) return Errors.NUM;
  const s = sortedAsc(xs);
  const lo = Math.floor(h);
  const frac = h - lo;
  const a = s[lo - 1] as number;
  return frac === 0 || lo >= n ? a : a + frac * ((s[lo] as number) - a);
}

export function quartileInc(xs: readonly number[], q: number): number | ErrorValue {
  const qq = Math.trunc(q);
  if (qq < 0 || qq > 4) return Errors.NUM;
  return percentileInc(xs, qq / 4);
}

export function quartileExc(xs: readonly number[], q: number): number | ErrorValue {
  const qq = Math.trunc(q);
  if (qq < 1 || qq > 3) return Errors.NUM;
  return percentileExc(xs, qq / 4);
}
