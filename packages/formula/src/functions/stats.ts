/**
 * Statistical functions (descriptive statistics, regression, tests). Probability distributions are
 * in `distributions.ts`.
 */
import type { CallContext } from '../engine/context.ts';
import type { Arg, FunctionDescriptor } from '../engine/functions/registry.ts';
import { Errors, ErrorValue } from '../values/errors.ts';
import { round15 } from '../values/number.ts';
import { Array2D, MISSING, type Scalar } from '../values/types.ts';
import * as A from './agg.ts';
import {
  alias,
  arrayOf,
  devsq,
  fn,
  intOf,
  isErr,
  mean,
  num,
  numberOf,
  numbers,
  numbersA,
  numbersOf,
  numeric,
  numericPairs,
  sortedAsc,
  sum,
  VARIADIC,
} from './lib.ts';
import { chisqCdf, fCdf, gamma, invertCdf, logGamma, normCdf, normInv, normPdf, tCdf } from './special.ts';

type NumAgg = (xs: number[]) => number | ErrorValue;

/** A variadic function over numbers collected like SUM. */
function overNumbers(name: string, agg: NumAgg): FunctionDescriptor {
  return fn(name, 1, VARIADIC, (args, ctx) => {
    const xs = numbers(args, ctx);
    return xs instanceof ErrorValue ? xs : agg(xs);
  });
}

/** A variadic "A" function (text in ranges = 0, logicals = 1/0). */
function overNumbersA(name: string, agg: NumAgg): FunctionDescriptor {
  return fn(name, 1, VARIADIC, (args, ctx) => {
    const xs = numbersA(args, ctx);
    return xs instanceof ErrorValue ? xs : agg(xs);
  });
}

/** (array, k) functions where k is lifted: LARGE, SMALL, PERCENTILE, QUARTILE… */
function arrayK(name: string, f: (xs: number[], k: number) => number | ErrorValue): FunctionDescriptor {
  return fn(
    name,
    2,
    2,
    (args, ctx) => {
      const xs = numbersOf(args[0], ctx);
      if (xs instanceof ErrorValue) return xs;
      const k = num(args[1]);
      if (isErr(k)) return k;
      return f(xs, k);
    },
    { lift: [1] },
  );
}

/** Functions of two paired arrays (CORREL, SLOPE…). */
function paired(name: string, f: (xs: number[], ys: number[]) => number | ErrorValue): FunctionDescriptor {
  return fn(name, 2, 2, (args, ctx) => {
    const p = numericPairs(args[0], args[1], ctx);
    if (p instanceof ErrorValue) return p;
    return f(p.xs, p.ys);
  });
}

function covariance(xs: number[], ys: number[], sample: boolean): number | ErrorValue {
  const n = xs.length;
  if (n === 0 || (sample && n < 2)) return Errors.DIV0;
  const mx = mean(xs);
  const my = mean(ys);
  let s = 0;
  for (let i = 0; i < n; i++) s += ((xs[i] as number) - mx) * ((ys[i] as number) - my);
  return s / (sample ? n - 1 : n);
}

function correl(xs: number[], ys: number[]): number | ErrorValue {
  const n = xs.length;
  if (n < 2) return Errors.DIV0;
  const mx = mean(xs);
  const my = mean(ys);
  let sxy = 0;
  let sxx = 0;
  let syy = 0;
  for (let i = 0; i < n; i++) {
    const dx = (xs[i] as number) - mx;
    const dy = (ys[i] as number) - my;
    sxy += dx * dy;
    sxx += dx * dx;
    syy += dy * dy;
  }
  if (sxx === 0 || syy === 0) return Errors.DIV0;
  return sxy / Math.sqrt(sxx * syy);
}

/** Simple regression of ys on xs: slope, intercept, sums. */
function regression(ys: number[], xs: number[]) {
  const n = xs.length;
  const mx = mean(xs);
  const my = mean(ys);
  let sxy = 0;
  let sxx = 0;
  let syy = 0;
  for (let i = 0; i < n; i++) {
    const dx = (xs[i] as number) - mx;
    const dy = (ys[i] as number) - my;
    sxy += dx * dy;
    sxx += dx * dx;
    syy += dy * dy;
  }
  const slope = sxy / sxx;
  return { n, mx, my, sxx, sxy, syy, slope, intercept: my - slope * mx };
}

function skew(xs: number[], population: boolean): number | ErrorValue {
  const n = xs.length;
  if (n < (population ? 1 : 3)) return Errors.DIV0;
  const m = mean(xs);
  const ss = devsq(xs);
  const sd = Math.sqrt(ss / (population ? n : n - 1));
  if (sd === 0) return Errors.DIV0;
  let s3 = 0;
  for (const x of xs) s3 += ((x - m) / sd) ** 3;
  return population ? s3 / n : (n / ((n - 1) * (n - 2))) * s3;
}

function kurt(xs: number[]): number | ErrorValue {
  const n = xs.length;
  if (n < 4) return Errors.DIV0;
  const m = mean(xs);
  const sd = Math.sqrt(devsq(xs) / (n - 1));
  if (sd === 0) return Errors.DIV0;
  let s4 = 0;
  for (const x of xs) s4 += ((x - m) / sd) ** 4;
  return ((n * (n + 1)) / ((n - 1) * (n - 2) * (n - 3))) * s4 - (3 * (n - 1) ** 2) / ((n - 2) * (n - 3));
}

/** Excel's PERCENTRANK: interpolated rank, truncated (not rounded) to `sig` digits. */
function percentRank(xs: number[], x: number, sig: number, exclusive: boolean): number | ErrorValue {
  const n = xs.length;
  if (n === 0 || sig < 1) return sig < 1 ? Errors.NUM : Errors.NA;
  const s = sortedAsc(xs);
  if (x < (s[0] as number) || x > (s[n - 1] as number)) return Errors.NA;
  const rankAt = (i: number) => (exclusive ? (i + 1) / (n + 1) : n === 1 ? 1 : i / (n - 1));
  let r: number;
  let lo = 0;
  while (lo < n && (s[lo] as number) < x) lo++;
  if (lo < n && s[lo] === x) r = rankAt(lo);
  else {
    const a = s[lo - 1] as number;
    const b = s[lo] as number;
    const frac = (x - a) / (b - a);
    r = rankAt(lo - 1) + frac * (rankAt(lo) - rankAt(lo - 1));
  }
  const f = 10 ** Math.trunc(sig);
  return Math.floor(round15(r * f)) / f;
}

function rank(args: Arg[], ctx: CallContext, avg: boolean): Scalar {
  const x = num(args[0]);
  if (isErr(x)) return x;
  const xs = numbersOf(args[1], ctx);
  if (xs instanceof ErrorValue) return xs;
  const order = args.length > 2 ? num(args[2]) : 0;
  if (isErr(order)) return order;
  let better = 0;
  let equal = 0;
  for (const v of xs) {
    if (round15(v) === round15(x)) equal++;
    else if (order === 0 ? v > x : v < x) better++;
  }
  if (equal === 0) return Errors.NA;
  return avg ? better + (equal + 1) / 2 : better + 1;
}

function frequency(args: Arg[], ctx: CallContext): Scalar | Array2D {
  const data = numbersOf(args[0], ctx);
  if (data instanceof ErrorValue) return data;
  const bins = numbersOf(args[1], ctx);
  if (bins instanceof ErrorValue) return bins;
  const order = bins.map((b, i) => ({ b, i })).sort((p, q) => p.b - q.b);
  const counts = new Array(bins.length + 1).fill(0) as number[];
  for (const x of data) {
    let k = order.findIndex((o) => x <= o.b);
    k = k < 0 ? bins.length : (order[k] as { i: number }).i;
    counts[k] = (counts[k] as number) + 1;
  }
  return Array2D.column(counts);
}

function trimmean(args: Arg[], ctx: CallContext): Scalar {
  const xs = numbersOf(args[0], ctx);
  if (xs instanceof ErrorValue) return xs;
  const p = numberOf(args[1], ctx);
  if (isErr(p)) return p;
  if (p < 0 || p >= 1 || xs.length === 0) return Errors.NUM;
  const k = Math.floor((xs.length * p) / 2);
  const s = sortedAsc(xs).slice(k, xs.length - k);
  return mean(s);
}

function prob(args: Arg[], ctx: CallContext): Scalar {
  const xa = arrayOf(args[0], ctx);
  if (isErr(xa)) return xa;
  const pa = arrayOf(args[1], ctx);
  if (isErr(pa)) return pa;
  const xs = xa.flat();
  const ps = pa.flat();
  if (xs.length !== ps.length) return Errors.NA;
  const lo = numberOf(args[2], ctx);
  if (isErr(lo)) return lo;
  const hi = args[3] === undefined || args[3] === MISSING ? lo : numberOf(args[3], ctx);
  if (isErr(hi)) return hi;
  let total = 0;
  let acc = 0;
  for (let i = 0; i < xs.length; i++) {
    const p = ps[i];
    const x = xs[i];
    if (p instanceof ErrorValue) return p;
    if (x instanceof ErrorValue) return x;
    if (typeof p !== 'number') continue;
    if (p < 0 || p > 1) return Errors.NUM;
    total += p;
    if (typeof x === 'number' && x >= lo && x <= hi) acc += p;
  }
  if (Math.abs(total - 1) > 1e-7) return Errors.NUM;
  return acc;
}

// ── LINEST family ────────────────────────────────────────────────────────

interface Fit {
  coef: number[]; // [m_k … m_1, b] in Excel's output order
  se?: number[];
  r2: number;
  sey: number;
  F: number;
  df: number;
  ssreg: number;
  ssresid: number;
}

/** Ordinary least squares of y on the columns of X (with an intercept when `withConst`). */
function leastSquares(y: number[], X: number[][], withConst: boolean, wantStats: boolean): Fit | ErrorValue {
  const n = y.length;
  const k = (X[0] as number[]).length;
  const p = k + (withConst ? 1 : 0);
  if (n < 1) return Errors.VALUE;
  // With a constant, regress on centred data (numerically far better), then recover the intercept.
  const mx = new Array(k).fill(0) as number[];
  let my = 0;
  if (withConst) {
    for (let i = 0; i < n; i++) {
      my += y[i] as number;
      for (let j = 0; j < k; j++) mx[j] = (mx[j] as number) + ((X[i] as number[])[j] as number);
    }
    my /= n;
    for (let j = 0; j < k; j++) mx[j] = (mx[j] as number) / n;
  }
  const xc = X.map((row) => row.map((v, j) => v - (mx[j] as number)));
  const yc = y.map((v) => v - my);
  const xtx: number[][] = Array.from({ length: k }, () => new Array(k).fill(0));
  const xty: number[] = new Array(k).fill(0);
  for (let i = 0; i < n; i++) {
    const r = xc[i] as number[];
    for (let a = 0; a < k; a++) {
      xty[a] = (xty[a] as number) + (r[a] as number) * (yc[i] as number);
      for (let b = 0; b < k; b++)
        (xtx[a] as number[])[b] = ((xtx[a] as number[])[b] as number) + (r[a] as number) * (r[b] as number);
    }
  }
  const inv = invert(xtx);
  if (!inv) return Errors.NUM;
  const slopesFwd = inv.map((row) => row.reduce((acc, v, j) => acc + v * (xty[j] as number), 0));
  const intercept = withConst ? my - slopesFwd.reduce((acc, b, j) => acc + b * (mx[j] as number), 0) : 0;
  let ssresid = 0;
  let sstot = 0;
  for (let i = 0; i < n; i++) {
    const r = xc[i] as number[];
    let fit = 0;
    for (let a = 0; a < k; a++) fit += (r[a] as number) * (slopesFwd[a] as number);
    ssresid += ((yc[i] as number) - fit) ** 2;
    sstot += (yc[i] as number) ** 2;
  }
  if (ssresid <= sstot * 1e-20) ssresid = 0;
  const df = n - p;
  const ssreg = sstot - ssresid;
  const r2 = sstot === 0 ? 1 : ssreg / sstot;
  const sey = df > 0 ? Math.sqrt(ssresid / df) : 0;
  const F = df > 0 && ssresid > 0 ? ssreg / k / (ssresid / df) : Number.NaN;
  const coef = [...[...slopesFwd].reverse(), intercept];
  const fitOut: Fit = { coef, r2, sey, F, df, ssreg, ssresid };
  if (wantStats) {
    const s2 = df > 0 ? ssresid / df : 0;
    const seSlopes = inv.map((row, i) => Math.sqrt(Math.max(0, (row[i] as number) * s2)));
    let seB = Number.NaN;
    if (withConst) {
      let q = 0;
      for (let a = 0; a < k; a++)
        for (let b = 0; b < k; b++)
          q += (mx[a] as number) * ((inv[a] as number[])[b] as number) * (mx[b] as number);
      seB = Math.sqrt(Math.max(0, s2 * (1 / n + q)));
    }
    fitOut.se = [...seSlopes.reverse(), seB];
  }
  return fitOut;
}

function invert(m: number[][]): number[][] | null {
  const n = m.length;
  const a = m.map((r) => [...r]);
  const inv: number[][] = m.map((_, i) => m.map((__, j) => (i === j ? 1 : 0)));
  for (let c = 0; c < n; c++) {
    let piv = c;
    for (let r = c + 1; r < n; r++)
      if (Math.abs((a[r] as number[])[c] as number) > Math.abs((a[piv] as number[])[c] as number)) piv = r;
    const pv = (a[piv] as number[])[c] as number;
    if (Math.abs(pv) < 1e-300) return null;
    [a[piv], a[c]] = [a[c] as number[], a[piv] as number[]];
    [inv[piv], inv[c]] = [inv[c] as number[], inv[piv] as number[]];
    const ac = a[c] as number[];
    const ic = inv[c] as number[];
    for (let j = 0; j < n; j++) {
      ac[j] = (ac[j] as number) / pv;
      ic[j] = (ic[j] as number) / pv;
    }
    for (let r = 0; r < n; r++) {
      if (r === c) continue;
      const f = (a[r] as number[])[c] as number;
      if (f === 0) continue;
      const ar = a[r] as number[];
      const ir = inv[r] as number[];
      for (let j = 0; j < n; j++) {
        ar[j] = (ar[j] as number) - f * (ac[j] as number);
        ir[j] = (ir[j] as number) - f * (ic[j] as number);
      }
    }
  }
  return inv;
}

/** Reads known_y's / known_x's into a vector and a design matrix (columns = predictors). */
function regressionInputs(
  args: Arg[],
  ctx: CallContext,
): { y: number[]; X: number[][]; yShape: 'col' | 'row'; k: number } | ErrorValue {
  const ya = arrayOf(args[0], ctx);
  if (isErr(ya)) return ya;
  const y: number[] = [];
  for (const v of ya.flat()) {
    if (v instanceof ErrorValue) return v;
    if (typeof v !== 'number') return Errors.VALUE;
    y.push(v);
  }
  const n = y.length;
  const yShape = ya.cols === 1 ? 'col' : 'row';
  let X: number[][];
  if (args[1] === undefined || args[1] === MISSING) {
    X = y.map((_, i) => [i + 1]);
  } else {
    const xa = arrayOf(args[1], ctx);
    if (isErr(xa)) return xa;
    for (const v of xa.flat()) {
      if (v instanceof ErrorValue) return v;
      if (typeof v !== 'number') return Errors.VALUE;
    }
    if (ya.rows === 1 || ya.cols === 1) {
      if (xa.size === n) X = (xa.flat() as number[]).map((v) => [v]);
      else if (yShape === 'col' && xa.rows === n) X = xa.data.map((r) => r as number[]);
      else if (yShape === 'row' && xa.cols === n) X = xa.transpose().data.map((r) => r as number[]);
      else return Errors.REF;
    } else {
      if (xa.rows !== ya.rows || xa.cols !== ya.cols) return Errors.REF;
      X = (xa.flat() as number[]).map((v) => [v]);
    }
  }
  return { y, X, yShape, k: (X[0] as number[]).length };
}

function linestImpl(args: Arg[], ctx: CallContext, log: boolean): Scalar | Array2D {
  const inp = regressionInputs(args, ctx);
  if (isErr(inp)) return inp;
  const withConst = args[2] === undefined || args[2] === MISSING ? true : numberOf(args[2], ctx);
  if (isErr(withConst)) return withConst;
  const stats = args[3] === undefined || args[3] === MISSING ? false : numberOf(args[3], ctx);
  if (isErr(stats)) return stats;
  let y = inp.y;
  if (log) {
    if (y.some((v) => v <= 0)) return Errors.NUM;
    y = y.map(Math.log);
  }
  const f = leastSquares(y, inp.X, !!withConst, !!stats);
  if (isErr(f)) return f;
  const row1 = f.coef.map((c) => (log ? Math.exp(c) : c));
  if (!stats) return Array2D.row(row1);
  const width = row1.length;
  const pad = (vals: Scalar[]) => [...vals, ...new Array(width - vals.length).fill(Errors.NA)];
  const se = (f.se as number[]).map((v, i) => (i === width - 1 && !withConst ? Errors.NA : v)) as Scalar[];
  return new Array2D([
    row1,
    se,
    pad([f.r2, f.sey]),
    pad([Number.isFinite(f.F) ? f.F : Errors.NUM, f.df]),
    pad([f.ssreg, f.ssresid]),
  ]);
}

function trendImpl(args: Arg[], ctx: CallContext, log: boolean): Scalar | Array2D {
  const inp = regressionInputs(args, ctx);
  if (isErr(inp)) return inp;
  const withConst = args[3] === undefined || args[3] === MISSING ? true : numberOf(args[3], ctx);
  if (isErr(withConst)) return withConst;
  let y = inp.y;
  if (log) {
    if (y.some((v) => v <= 0)) return Errors.NUM;
    y = y.map(Math.log);
  }
  const f = leastSquares(y, inp.X, !!withConst, false);
  if (isErr(f)) return f;
  const k = inp.k;
  const slopes = f.coef.slice(0, k).reverse();
  const b = f.coef[k] as number;
  let newX: Array2D;
  if (args[2] === undefined || args[2] === MISSING) {
    newX = new Array2D(inp.X.map((r) => r as Scalar[]));
    if (k === 1)
      newX =
        inp.yShape === 'col'
          ? Array2D.column(inp.X.map((r) => r[0] as number))
          : Array2D.row(inp.X.map((r) => r[0] as number));
  } else {
    const nx = arrayOf(args[2], ctx);
    if (isErr(nx)) return nx;
    newX = nx;
  }
  const pred = (xs: number[]) => {
    let v = b;
    for (let i = 0; i < k; i++) v += (slopes[i] as number) * (xs[i] as number);
    return log ? Math.exp(v) : v;
  };
  if (k === 1) {
    return newX.map((v) => (typeof v === 'number' ? pred([v]) : v instanceof ErrorValue ? v : Errors.VALUE));
  }
  // Multiple predictors: one prediction per row (or column) of new_x.
  const byRow = newX.cols === k;
  const m = byRow ? newX.rows : newX.cols;
  const out: Scalar[] = [];
  for (let i = 0; i < m; i++) {
    const xs: number[] = [];
    for (let j = 0; j < k; j++) {
      const v = byRow ? newX.get(i, j) : newX.get(j, i);
      if (typeof v !== 'number') return Errors.VALUE;
      xs.push(v);
    }
    out.push(pred(xs));
  }
  return byRow ? Array2D.column(out) : Array2D.row(out);
}

// ── tests ────────────────────────────────────────────────────────────────

function ttest(args: Arg[], ctx: CallContext): Scalar {
  const tails = intOf(args[2], ctx);
  if (isErr(tails)) return tails;
  const type = intOf(args[3], ctx);
  if (isErr(type)) return type;
  if ((tails !== 1 && tails !== 2) || type < 1 || type > 3) return Errors.NUM;
  let t: number;
  let df: number;
  if (type === 1) {
    const p = numericPairs(args[0], args[1], ctx);
    if (isErr(p)) return p;
    const d = p.xs.map((x, i) => x - (p.ys[i] as number));
    const n = d.length;
    if (n < 2) return Errors.DIV0;
    const sd = Math.sqrt(devsq(d) / (n - 1));
    if (sd === 0) return Errors.DIV0;
    t = mean(d) / (sd / Math.sqrt(n));
    df = n - 1;
  } else {
    const a = numbersOf(args[0], ctx);
    if (isErr(a)) return a;
    const b = numbersOf(args[1], ctx);
    if (isErr(b)) return b;
    const n1 = a.length;
    const n2 = b.length;
    if (n1 < 2 || n2 < 2) return Errors.DIV0;
    const v1 = devsq(a) / (n1 - 1);
    const v2 = devsq(b) / (n2 - 1);
    if (type === 2) {
      const sp = ((n1 - 1) * v1 + (n2 - 1) * v2) / (n1 + n2 - 2);
      if (sp === 0) return Errors.DIV0;
      t = (mean(a) - mean(b)) / Math.sqrt(sp * (1 / n1 + 1 / n2));
      df = n1 + n2 - 2;
    } else {
      const s = v1 / n1 + v2 / n2;
      if (s === 0) return Errors.DIV0;
      t = (mean(a) - mean(b)) / Math.sqrt(s);
      df = (s * s) / ((v1 / n1) ** 2 / (n1 - 1) + (v2 / n2) ** 2 / (n2 - 1));
    }
  }
  const one = 1 - tCdf(Math.abs(t), df);
  return tails === 1 ? one : 2 * one;
}

function ftest(args: Arg[], ctx: CallContext): Scalar {
  const a = numbersOf(args[0], ctx);
  if (isErr(a)) return a;
  const b = numbersOf(args[1], ctx);
  if (isErr(b)) return b;
  if (a.length < 2 || b.length < 2) return Errors.DIV0;
  const v1 = devsq(a) / (a.length - 1);
  const v2 = devsq(b) / (b.length - 1);
  if (v1 === 0 || v2 === 0) return Errors.DIV0;
  const f = v1 / v2;
  const c = fCdf(f, a.length - 1, b.length - 1);
  return 2 * Math.min(c, 1 - c);
}

function chitest(args: Arg[], ctx: CallContext): Scalar {
  const act = arrayOf(args[0], ctx);
  if (isErr(act)) return act;
  const exp = arrayOf(args[1], ctx);
  if (isErr(exp)) return exp;
  if (act.rows !== exp.rows || act.cols !== exp.cols) return Errors.NA;
  let chi = 0;
  for (let r = 0; r < act.rows; r++)
    for (let c = 0; c < act.cols; c++) {
      const a = act.get(r, c);
      const e = exp.get(r, c);
      if (a instanceof ErrorValue) return a;
      if (e instanceof ErrorValue) return e;
      if (typeof a !== 'number' || typeof e !== 'number') continue;
      if (e === 0) return Errors.DIV0;
      chi += (a - e) ** 2 / e;
    }
  const df = act.rows > 1 && act.cols > 1 ? (act.rows - 1) * (act.cols - 1) : act.rows * act.cols - 1;
  if (df < 1) return Errors.NA;
  return 1 - chisqCdf(chi, df);
}

function ztest(args: Arg[], ctx: CallContext): Scalar {
  const xs = numbersOf(args[0], ctx);
  if (isErr(xs)) return xs;
  const x = numberOf(args[1], ctx);
  if (isErr(x)) return x;
  const n = xs.length;
  if (n === 0) return Errors.NA;
  let sigma: number;
  if (args[2] !== undefined && args[2] !== MISSING) {
    const s = numberOf(args[2], ctx);
    if (isErr(s)) return s;
    sigma = s;
  } else {
    if (n < 2) return Errors.DIV0;
    sigma = Math.sqrt(devsq(xs) / (n - 1));
  }
  if (sigma === 0) return Errors.DIV0;
  return 1 - normCdf((mean(xs) - x) / (sigma / Math.sqrt(n)));
}

// ── descriptors ──────────────────────────────────────────────────────────

const STDEV_S = overNumbers('STDEV.S', A.stdevS);
const STDEV_P = overNumbers('STDEV.P', A.stdevP);
const VAR_S = overNumbers('VAR.S', A.varS);
const VAR_P = overNumbers('VAR.P', A.varP);
const MODE_SNGL = overNumbers('MODE.SNGL', A.modeSngl);
const PERCENTILE_INC = arrayK('PERCENTILE.INC', A.percentileInc);
const QUARTILE_INC = arrayK('QUARTILE.INC', A.quartileInc);
const COVARIANCE_P = paired('COVARIANCE.P', (x, y) => covariance(x, y, false));

function percentRankFn(name: string, exclusive: boolean): FunctionDescriptor {
  return fn(
    name,
    2,
    3,
    (args, ctx) => {
      const xs = numbersOf(args[0], ctx);
      if (isErr(xs)) return xs;
      const x = num(args[1]);
      if (isErr(x)) return x;
      const sig = num(args[2], 3);
      if (isErr(sig)) return sig;
      return percentRank(xs, x, sig, exclusive);
    },
    { lift: [1, 2] },
  );
}

const PERCENTRANK_INC = percentRankFn('PERCENTRANK.INC', false);
const RANK_EQ = fn('RANK.EQ', 2, 3, (args, ctx) => rank(args, ctx, false), { lift: [0, 2] });

export const statisticalFunctions: FunctionDescriptor[] = [
  overNumbers('AVEDEV', (xs) => {
    if (xs.length === 0) return Errors.NUM;
    const m = mean(xs);
    return sum(xs.map((x) => Math.abs(x - m))) / xs.length;
  }),
  overNumbersA('AVERAGEA', A.average),
  overNumbersA('MAXA', A.maxOf),
  overNumbersA('MINA', A.minOf),
  overNumbers('MEDIAN', A.median),
  MODE_SNGL,
  alias('MODE', MODE_SNGL),
  fn('MODE.MULT', 1, VARIADIC, (args, ctx) => {
    const xs = numbers(args, ctx);
    if (isErr(xs)) return xs;
    const m = A.modes(xs);
    return m.length === 0 ? Errors.NA : Array2D.column(m);
  }),
  overNumbers('DEVSQ', (xs) => (xs.length === 0 ? Errors.NUM : devsq(xs))),
  overNumbers('GEOMEAN', (xs) => {
    if (xs.length === 0 || xs.some((x) => x <= 0)) return Errors.NUM;
    return Math.exp(sum(xs.map(Math.log)) / xs.length);
  }),
  overNumbers('HARMEAN', (xs) => {
    if (xs.length === 0 || xs.some((x) => x <= 0)) return Errors.NUM;
    return xs.length / sum(xs.map((x) => 1 / x));
  }),
  overNumbers('KURT', kurt),
  overNumbers('SKEW', (xs) => skew(xs, false)),
  overNumbers('SKEW.P', (xs) => skew(xs, true)),
  STDEV_S,
  alias('STDEV', STDEV_S),
  STDEV_P,
  alias('STDEVP', STDEV_P),
  overNumbersA('STDEVA', A.stdevS),
  overNumbersA('STDEVPA', A.stdevP),
  VAR_S,
  alias('VAR', VAR_S),
  VAR_P,
  alias('VARP', VAR_P),
  overNumbersA('VARA', A.varS),
  overNumbersA('VARPA', A.varP),
  arrayK('LARGE', A.large),
  arrayK('SMALL', A.small),
  PERCENTILE_INC,
  alias('PERCENTILE', PERCENTILE_INC),
  arrayK('PERCENTILE.EXC', A.percentileExc),
  QUARTILE_INC,
  alias('QUARTILE', QUARTILE_INC),
  arrayK('QUARTILE.EXC', A.quartileExc),
  PERCENTRANK_INC,
  alias('PERCENTRANK', PERCENTRANK_INC),
  percentRankFn('PERCENTRANK.EXC', true),
  RANK_EQ,
  alias('RANK', RANK_EQ),
  fn('RANK.AVG', 2, 3, (args, ctx) => rank(args, ctx, true), { lift: [0, 2] }),
  fn('TRIMMEAN', 2, 2, trimmean),
  fn('FREQUENCY', 2, 2, frequency),
  fn('PROB', 3, 4, prob),
  paired('CORREL', correl),
  paired('PEARSON', correl),
  paired('RSQ', (x, y) => {
    const r = correl(x, y);
    return typeof r === 'number' ? r * r : r;
  }),
  COVARIANCE_P,
  alias('COVAR', COVARIANCE_P),
  paired('COVARIANCE.S', (x, y) => covariance(x, y, true)),
  paired('SLOPE', (ys, xs) => {
    if (xs.length < 1) return Errors.DIV0;
    const r = regression(ys, xs);
    return r.sxx === 0 ? Errors.DIV0 : r.slope;
  }),
  paired('INTERCEPT', (ys, xs) => {
    if (xs.length < 1) return Errors.DIV0;
    const r = regression(ys, xs);
    return r.sxx === 0 ? Errors.DIV0 : r.intercept;
  }),
  paired('STEYX', (ys, xs) => {
    const n = xs.length;
    if (n < 3) return Errors.DIV0;
    const r = regression(ys, xs);
    if (r.sxx === 0) return Errors.DIV0;
    return Math.sqrt((r.syy - (r.sxy * r.sxy) / r.sxx) / (n - 2));
  }),
  ...['FORECAST', 'FORECAST.LINEAR'].map((name) =>
    fn(
      name,
      3,
      3,
      (args, ctx) => {
        const x = num(args[0]);
        if (isErr(x)) return x;
        const p = numericPairs(args[1], args[2], ctx);
        if (isErr(p)) return p;
        if (p.xs.length < 1) return Errors.DIV0;
        const r = regression(p.xs, p.ys);
        if (r.sxx === 0) return Errors.DIV0;
        return r.intercept + r.slope * x;
      },
      { lift: [0] },
    ),
  ),
  fn('LINEST', 1, 4, (args, ctx) => linestImpl(args, ctx, false)),
  fn('LOGEST', 1, 4, (args, ctx) => linestImpl(args, ctx, true)),
  fn('TREND', 1, 4, (args, ctx) => trendImpl(args, ctx, false)),
  fn('GROWTH', 1, 4, (args, ctx) => trendImpl(args, ctx, true)),
  numeric('STANDARDIZE', 3, 3, (x, m, sd) => (sd <= 0 ? Errors.NUM : (x - m) / sd)),
  numeric('FISHER', 1, 1, (x) => (x <= -1 || x >= 1 ? Errors.NUM : 0.5 * Math.log((1 + x) / (1 - x)))),
  numeric('FISHERINV', 1, 1, (y) => Math.tanh(y)),
  numeric('PERMUT', 2, 2, (n0, k0) => {
    const n = Math.trunc(n0);
    const k = Math.trunc(k0);
    if (n < 0 || k < 0 || n < k) return Errors.NUM;
    let r = 1;
    for (let i = 0; i < k; i++) r *= n - i;
    return r;
  }),
  numeric('PERMUTATIONA', 2, 2, (n0, k0) => {
    const n = Math.trunc(n0);
    const k = Math.trunc(k0);
    if (n < 0 || k < 0) return Errors.NUM;
    return n ** k;
  }),
  numeric('GAMMA', 1, 1, (x) => (x <= 0 && Number.isInteger(x) ? Errors.NUM : gamma(x))),
  ...['GAMMALN', 'GAMMALN.PRECISE'].map((name) =>
    numeric(name, 1, 1, (x) => (x <= 0 ? Errors.NUM : logGamma(x))),
  ),
  numeric('GAUSS', 1, 1, (z) => normCdf(z) - 0.5),
  numeric('PHI', 1, 1, (x) => normPdf(x)),
  ...['CONFIDENCE.NORM', 'CONFIDENCE'].map((name) =>
    numeric(name, 3, 3, (alpha, sd, n0) => {
      const n = Math.trunc(n0);
      if (alpha <= 0 || alpha >= 1 || sd <= 0 || n < 1) return Errors.NUM;
      return (normInv(1 - alpha / 2) * sd) / Math.sqrt(n);
    }),
  ),
  numeric('CONFIDENCE.T', 3, 3, (alpha, sd, n0) => {
    const n = Math.trunc(n0);
    if (alpha <= 0 || alpha >= 1 || sd <= 0 || n < 1) return Errors.NUM;
    if (n === 1) return Errors.DIV0;
    const t = invertCdf((x) => tCdf(x, n - 1), 1 - alpha / 2, 0, 1);
    return (t * sd) / Math.sqrt(n);
  }),
  ...['T.TEST', 'TTEST'].map((name) => fn(name, 4, 4, ttest)),
  ...['F.TEST', 'FTEST'].map((name) => fn(name, 2, 2, ftest)),
  ...['CHISQ.TEST', 'CHITEST'].map((name) => fn(name, 2, 2, chitest)),
  ...['Z.TEST', 'ZTEST'].map((name) => fn(name, 2, 3, ztest)),
];
