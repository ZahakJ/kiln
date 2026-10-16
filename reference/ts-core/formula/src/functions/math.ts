/**
 * Math & Trig functions (the ones not in the engine's core set).
 */
import type { CallContext } from '../engine/context.ts';
import { force } from '../engine/functions/helpers.ts';
import type { Arg, FunctionDescriptor } from '../engine/functions/registry.ts';
import { toNumber } from '../values/coerce.ts';
import { Errors, ErrorValue } from '../values/errors.ts';
import { excelRound, excelTrunc, round15 } from '../values/number.ts';
import { Array2D, BLANK, MISSING, RefValue, type Scalar } from '../values/types.ts';
import * as A from './agg.ts';
import { refGrid } from './grid.ts';
import {
  arrayOf,
  fn,
  int,
  intOf,
  isErr,
  num,
  numberOf,
  numbers,
  numeric,
  numericPairs,
  str,
  sum,
  VARIADIC,
} from './lib.ts';
import { choose, factorial } from './special.ts';

const TRIG_LIMIT = 134_217_728; // 2^27: Excel's SIN/COS/TAN give #NUM! beyond this.

/** x / step rounded to 15 digits, so 1.1/0.1 is 11 rather than 11.000000000000002. */
function ratio(x: number, step: number): number {
  return round15(x / step);
}

function ceilingClassic(x: number, sig: number): number | ErrorValue {
  if (sig === 0 || x === 0) return 0;
  if (x > 0 && sig < 0) return Errors.NUM;
  return Math.ceil(ratio(x, sig)) * sig;
}

function floorClassic(x: number, sig: number): number | ErrorValue {
  if (x === 0) return 0;
  if (sig === 0) return Errors.DIV0;
  if (x > 0 && sig < 0) return Errors.NUM;
  return Math.floor(ratio(x, sig)) * sig;
}

function ceilingMath(x: number, sig: number, mode: number): number {
  const s = Math.abs(sig);
  if (s === 0 || x === 0) return 0;
  const q = ratio(x, s);
  return (x < 0 && mode !== 0 ? Math.floor(q) : Math.ceil(q)) * s;
}

function floorMath(x: number, sig: number, mode: number): number {
  const s = Math.abs(sig);
  if (s === 0 || x === 0) return 0;
  const q = ratio(x, s);
  return (x < 0 && mode !== 0 ? Math.ceil(q) : Math.floor(q)) * s;
}

/** Fixes the binary noise of `k * step` (0.1*3 → 0.3). */
function snap(x: number): number {
  return round15(x);
}

function roundUp(x: number, digits: number): number {
  const d = Math.trunc(digits);
  const sign = x < 0 ? -1 : 1;
  const a = Math.abs(x);
  if (d >= 0) {
    const f = 10 ** d;
    return (sign * Math.ceil(round15(a * f))) / f;
  }
  const f = 10 ** -d;
  return sign * Math.ceil(round15(a / f)) * f;
}

function evenOdd(x: number, odd: boolean): number {
  const sign = x < 0 ? -1 : 1;
  let n = Math.ceil(round15(Math.abs(x)));
  if (odd) {
    if (n % 2 === 0) n += 1;
  } else if (n % 2 === 1) n += 1;
  return sign * n;
}

function gcd2(a: number, b: number): number {
  let x = a;
  let y = b;
  while (y) [x, y] = [y, x % y];
  return x;
}

function integerArgs(args: Arg[], ctx: CallContext): number[] | ErrorValue {
  const xs = numbers(args, ctx);
  if (xs instanceof ErrorValue) return xs;
  const out: number[] = [];
  for (const x of xs) {
    if (x < 0 || x >= 2 ** 53) return Errors.NUM;
    out.push(Math.trunc(x));
  }
  return out;
}

// ── ROMAN / ARABIC / BASE / DECIMAL ──────────────────────────────────────

const ROMAN_CHARS = ['M', 'D', 'C', 'L', 'X', 'V', 'I'];
const ROMAN_VALUES = [1000, 500, 100, 50, 10, 5, 1];

/** Excel's ROMAN with forms 0 (classic) to 4 (most concise). */
export function toRoman(value: number, mode: number): string {
  let val = value;
  let out = '';
  const maxIndex = ROMAN_VALUES.length - 1;
  for (let i = 0; i <= maxIndex / 2; i++) {
    let index = 2 * i;
    const digit = Math.floor(val / (ROMAN_VALUES[index] as number));
    if (digit % 5 === 4) {
      const index2 = digit === 4 ? index - 1 : index - 2;
      let steps = 0;
      while (steps < mode && index < maxIndex) {
        steps++;
        if ((ROMAN_VALUES[index2] as number) - (ROMAN_VALUES[index + 1] as number) <= val) index++;
        else steps = mode;
      }
      out += (ROMAN_CHARS[index] as string) + (ROMAN_CHARS[index2] as string);
      val += (ROMAN_VALUES[index] as number) - (ROMAN_VALUES[index2] as number);
    } else {
      if (digit > 4) out += ROMAN_CHARS[index - 1];
      out += (ROMAN_CHARS[index] as string).repeat(digit % 5);
      val %= ROMAN_VALUES[index] as number;
    }
  }
  return out;
}

function fromRoman(text: string): number | ErrorValue {
  let s = text.trim().toUpperCase();
  if (s.length > 255) return Errors.VALUE;
  let sign = 1;
  if (s.startsWith('-')) {
    sign = -1;
    s = s.slice(1);
  }
  if (s === '') return 0;
  const map: Record<string, number> = { I: 1, V: 5, X: 10, L: 50, C: 100, D: 500, M: 1000 };
  let total = 0;
  for (let i = 0; i < s.length; i++) {
    const v = map[s[i] as string];
    if (v === undefined) return Errors.VALUE;
    const next = i + 1 < s.length ? map[s[i + 1] as string] : undefined;
    if (next !== undefined && next > v) total -= v;
    else total += v;
  }
  return sign * total;
}

// ── matrices ─────────────────────────────────────────────────────────────

function numericMatrix(a: Arg | undefined, ctx: CallContext): number[][] | ErrorValue {
  const arr = arrayOf(a, ctx);
  if (arr instanceof ErrorValue) return arr;
  const out: number[][] = [];
  for (const row of arr.data) {
    const r: number[] = [];
    for (const v of row) {
      if (v instanceof ErrorValue) return v;
      if (typeof v !== 'number') return Errors.VALUE;
      r.push(v);
    }
    out.push(r);
  }
  return out;
}

/** LU decomposition with partial pivoting; returns determinant and inverse (or null if singular). */
function luSolve(m: number[][], wantInverse: boolean): { det: number; inv: number[][] | null } {
  const n = m.length;
  const a = m.map((r) => [...r]);
  const inv: number[][] = wantInverse ? m.map((_, i) => m.map((__, j) => (i === j ? 1 : 0))) : [];
  let det = 1;
  for (let col = 0; col < n; col++) {
    let piv = col;
    for (let r = col + 1; r < n; r++)
      if (Math.abs((a[r] as number[])[col] as number) > Math.abs((a[piv] as number[])[col] as number))
        piv = r;
    const pv = (a[piv] as number[])[col] as number;
    if (pv === 0) return { det: 0, inv: null };
    if (piv !== col) {
      [a[piv], a[col]] = [a[col] as number[], a[piv] as number[]];
      if (wantInverse) [inv[piv], inv[col]] = [inv[col] as number[], inv[piv] as number[]];
      det = -det;
    }
    det *= pv;
    const rowC = a[col] as number[];
    const invC = inv[col] as number[];
    for (let j = 0; j < n; j++) {
      rowC[j] = (rowC[j] as number) / pv;
      if (wantInverse) invC[j] = (invC[j] as number) / pv;
    }
    for (let r = 0; r < n; r++) {
      if (r === col) continue;
      const row = a[r] as number[];
      const f = row[col] as number;
      if (f === 0) continue;
      for (let j = 0; j < n; j++) row[j] = (row[j] as number) - f * (rowC[j] as number);
      if (wantInverse) {
        const ir = inv[r] as number[];
        for (let j = 0; j < n; j++) ir[j] = (ir[j] as number) - f * (invC[j] as number);
      }
    }
  }
  return { det, inv: wantInverse ? inv : null };
}

// ── SUMPRODUCT and pair sums ─────────────────────────────────────────────

function sumproduct(args: Arg[], ctx: CallContext): Scalar {
  const arrays: Array2D[] = [];
  for (const a of args) {
    const arr = arrayOf(a, ctx);
    if (arr instanceof ErrorValue) return arr;
    arrays.push(arr);
  }
  const first = arrays[0] as Array2D;
  for (const a of arrays) if (a.rows !== first.rows || a.cols !== first.cols) return Errors.VALUE;
  const terms: number[] = [];
  for (let r = 0; r < first.rows; r++) {
    for (let c = 0; c < first.cols; c++) {
      let p = 1;
      for (const a of arrays) {
        const v = a.get(r, c);
        if (v instanceof ErrorValue) return v;
        p *= typeof v === 'number' ? v : 0;
      }
      terms.push(p);
    }
  }
  return sum(terms);
}

function pairSum(args: Arg[], ctx: CallContext, f: (x: number, y: number) => number): Scalar {
  const p = numericPairs(args[0], args[1], ctx);
  if (p instanceof ErrorValue) return p;
  const terms = p.xs.map((x, i) => f(x, p.ys[i] as number));
  return sum(terms);
}

// ── SUBTOTAL / AGGREGATE ─────────────────────────────────────────────────

const NESTED_RE = /\b(?:SUBTOTAL|AGGREGATE)\s*\(/i;

const SUBTOTAL_AGGS: Record<number, A.Agg> = {
  1: A.average,
  2: A.count,
  3: A.count, // COUNTA handled specially
  4: A.maxOf,
  5: A.minOf,
  6: A.product,
  7: A.stdevS,
  8: A.stdevP,
  9: A.sumOf,
  10: A.varS,
  11: A.varP,
};

interface Collect {
  skipHidden: 'none' | 'filtered' | 'all';
  skipErrors: boolean;
  skipNested: boolean;
  counta: boolean;
}

/** Numbers (or non-blank count) from reference arguments honouring hidden rows and nested subtotals. */
function collectForSubtotal(args: Arg[], ctx: CallContext, opt: Collect): number[] | ErrorValue {
  const out: number[] = [];
  for (const a of args) {
    const v = force(a);
    if (v instanceof RefValue) {
      for (const area of v.areas) {
        const g = refGrid(area, ctx);
        for (let r = 0; r < g.usedRows; r++) {
          const row = area.r0 + r;
          if (opt.skipHidden !== 'none') {
            const vis = ctx.rowVisibility(area.sheet, row);
            if (vis === 'filtered' || (vis === 'hidden' && opt.skipHidden === 'all')) continue;
          }
          for (let c = 0; c < g.usedCols; c++) {
            const col = area.c0 + c;
            if (opt.skipNested) {
              const f = ctx.formulaText(area.sheet, row, col);
              if (f && NESTED_RE.test(f)) continue;
            }
            const x = ctx.readCell(area.sheet, row, col);
            if (x instanceof ErrorValue) {
              if (opt.skipErrors) continue;
              return x;
            }
            if (opt.counta) {
              if (x !== BLANK) out.push(1);
            } else if (typeof x === 'number') out.push(x);
          }
        }
      }
      continue;
    }
    if (v instanceof ErrorValue) {
      if (opt.skipErrors) continue;
      return v;
    }
    const arr = ctx.toArray(v);
    if (arr instanceof ErrorValue) return arr;
    for (const x of arr.flat()) {
      if (x instanceof ErrorValue) {
        if (opt.skipErrors) continue;
        return x;
      }
      if (opt.counta) {
        if (x !== BLANK) out.push(1);
      } else if (typeof x === 'number') out.push(x);
    }
  }
  return out;
}

function subtotal(args: Arg[], ctx: CallContext): Scalar {
  const f = intOf(args[0], ctx);
  if (f instanceof ErrorValue) return f;
  const base = f > 100 ? f - 100 : f;
  const agg = SUBTOTAL_AGGS[base];
  if (!agg || (f > 11 && f < 101) || f > 111) return Errors.VALUE;
  const xs = collectForSubtotal(args.slice(1), ctx, {
    skipHidden: f > 100 ? 'all' : 'filtered',
    skipErrors: false,
    skipNested: true,
    counta: base === 3,
  });
  if (xs instanceof ErrorValue) return xs;
  return agg(xs);
}

const AGGREGATE_REF: Record<number, A.Agg> = {
  1: A.average,
  2: A.count,
  3: A.count,
  4: A.maxOf,
  5: A.minOf,
  6: A.product,
  7: A.stdevS,
  8: A.stdevP,
  9: A.sumOf,
  10: A.varS,
  11: A.varP,
  12: A.median,
  13: A.modeSngl,
};

const AGGREGATE_K: Record<number, (xs: number[], k: number) => number | ErrorValue> = {
  14: A.large,
  15: A.small,
  16: A.percentileInc,
  17: A.quartileInc,
  18: A.percentileExc,
  19: A.quartileExc,
};

function aggregate(args: Arg[], ctx: CallContext): Scalar {
  const f = intOf(args[0], ctx);
  if (f instanceof ErrorValue) return f;
  const o = intOf(args[1], ctx, 0);
  if (o instanceof ErrorValue) return o;
  if (o < 0 || o > 7 || f < 1 || f > 19) return Errors.VALUE;
  const opt: Collect = {
    skipHidden: o === 1 || o === 3 || o === 5 || o === 7 ? 'all' : 'none',
    skipErrors: o === 2 || o === 3 || o === 6 || o === 7,
    skipNested: o <= 3,
    counta: f === 3,
  };
  if (f <= 13) {
    const xs = collectForSubtotal(args.slice(2), ctx, opt);
    if (xs instanceof ErrorValue) return xs;
    return (AGGREGATE_REF[f] as A.Agg)(xs);
  }
  if (args.length < 4) return Errors.VALUE;
  const k = numberOf(args[3], ctx);
  if (k instanceof ErrorValue) return k;
  const xs = collectForSubtotal([args[2] as Arg], ctx, opt);
  if (xs instanceof ErrorValue) return xs;
  return (AGGREGATE_K[f] as (xs: number[], k: number) => number | ErrorValue)(xs, k);
}

// ── RANDARRAY ────────────────────────────────────────────────────────────

function randarray(args: Arg[], ctx: CallContext): Scalar | Array2D {
  const rows = intOf(args[0], ctx, 1);
  if (rows instanceof ErrorValue) return rows;
  const cols = intOf(args[1], ctx, 1);
  if (cols instanceof ErrorValue) return cols;
  const lo = numberOf(args[2], ctx, 0);
  if (lo instanceof ErrorValue) return lo;
  const hi = numberOf(args[3], ctx, 1);
  if (hi instanceof ErrorValue) return hi;
  const whole = args[4] === undefined || args[4] === MISSING ? false : toNumber(ctx.toScalar(force(args[4])));
  if (whole instanceof ErrorValue) return whole;
  if (rows < 1 || cols < 1 || lo > hi) return rows === 0 || cols === 0 ? Errors.CALC : Errors.VALUE;
  if (whole && (!Number.isInteger(lo) || !Number.isInteger(hi))) return Errors.VALUE;
  return Array2D.build(rows, cols, () =>
    whole ? lo + Math.floor(Math.random() * (hi - lo + 1)) : lo + Math.random() * (hi - lo),
  );
}

export const mathFunctions: FunctionDescriptor[] = [
  numeric('ACOS', 1, 1, (x) => (x < -1 || x > 1 ? Errors.NUM : Math.acos(x))),
  numeric('ACOSH', 1, 1, (x) => (x < 1 ? Errors.NUM : Math.acosh(x))),
  numeric('ACOT', 1, 1, (x) => Math.PI / 2 - Math.atan(x)),
  numeric('ACOTH', 1, 1, (x) => (Math.abs(x) <= 1 ? Errors.NUM : 0.5 * Math.log((x + 1) / (x - 1)))),
  numeric('ASIN', 1, 1, (x) => (x < -1 || x > 1 ? Errors.NUM : Math.asin(x))),
  numeric('ASINH', 1, 1, (x) => Math.asinh(x)),
  numeric('ATAN', 1, 1, (x) => Math.atan(x)),
  numeric('ATAN2', 2, 2, (x, y) => (x === 0 && y === 0 ? Errors.DIV0 : Math.atan2(y, x))),
  numeric('ATANH', 1, 1, (x) => (Math.abs(x) >= 1 ? Errors.NUM : Math.atanh(x))),
  numeric('COS', 1, 1, (x) => (Math.abs(x) >= TRIG_LIMIT ? Errors.NUM : Math.cos(x))),
  numeric('COSH', 1, 1, (x) => Math.cosh(x)),
  numeric('COT', 1, 1, (x) =>
    x === 0 ? Errors.DIV0 : Math.abs(x) >= TRIG_LIMIT ? Errors.NUM : 1 / Math.tan(x),
  ),
  numeric('COTH', 1, 1, (x) => (x === 0 ? Errors.DIV0 : 1 / Math.tanh(x))),
  numeric('CSC', 1, 1, (x) =>
    x === 0 ? Errors.DIV0 : Math.abs(x) >= TRIG_LIMIT ? Errors.NUM : 1 / Math.sin(x),
  ),
  numeric('CSCH', 1, 1, (x) => (x === 0 ? Errors.DIV0 : 1 / Math.sinh(x))),
  numeric('SEC', 1, 1, (x) => (Math.abs(x) >= TRIG_LIMIT ? Errors.NUM : 1 / Math.cos(x))),
  numeric('SECH', 1, 1, (x) => 1 / Math.cosh(x)),
  numeric('SIN', 1, 1, (x) => (Math.abs(x) >= TRIG_LIMIT ? Errors.NUM : Math.sin(x))),
  numeric('SINH', 1, 1, (x) => Math.sinh(x)),
  numeric('TAN', 1, 1, (x) => (Math.abs(x) >= TRIG_LIMIT ? Errors.NUM : Math.tan(x))),
  numeric('TANH', 1, 1, (x) => Math.tanh(x)),
  numeric('DEGREES', 1, 1, (x) => (x * 180) / Math.PI),
  numeric('RADIANS', 1, 1, (x) => (x * Math.PI) / 180),
  fn('PI', 0, 0, () => Math.PI),
  numeric('EXP', 1, 1, (x) => Math.exp(x)),
  numeric('LN', 1, 1, (x) => (x <= 0 ? Errors.NUM : Math.log(x))),
  numeric('LOG10', 1, 1, (x) => (x <= 0 ? Errors.NUM : Math.log10(x))),
  numeric(
    'LOG',
    1,
    2,
    (x, b) => {
      if (x <= 0 || b <= 0) return Errors.NUM;
      if (b === 1) return Errors.DIV0;
      if (b === 10) return Math.log10(x);
      if (b === 2) return Math.log2(x);
      return round15(Math.log(x) / Math.log(b));
    },
    [10],
  ),
  numeric('SQRT', 1, 1, (x) => (x < 0 ? Errors.NUM : Math.sqrt(x))),
  numeric('SQRTPI', 1, 1, (x) => (x < 0 ? Errors.NUM : Math.sqrt(x * Math.PI))),
  numeric('POWER', 2, 2, (x, y) => {
    if (x === 0 && y === 0) return Errors.NUM;
    if (x === 0 && y < 0) return Errors.DIV0;
    const r = x ** y;
    return Number.isNaN(r) ? Errors.NUM : r;
  }),
  numeric('SIGN', 1, 1, (x) => Math.sign(x)),
  numeric('CEILING', 2, 2, (x, s) => ceilingClassic(x, s)),
  numeric('CEILING.MATH', 1, 3, (x, s, m) => ceilingMath(x, s, m), [1, 0]),
  numeric('CEILING.PRECISE', 1, 2, (x, s) => ceilingMath(x, s, 0), [1]),
  numeric('ISO.CEILING', 1, 2, (x, s) => ceilingMath(x, s, 0), [1]),
  numeric('FLOOR', 2, 2, (x, s) => floorClassic(x, s)),
  numeric('FLOOR.MATH', 1, 3, (x, s, m) => floorMath(x, s, m), [1, 0]),
  numeric('FLOOR.PRECISE', 1, 2, (x, s) => floorMath(x, s, 0), [1]),
  numeric('MROUND', 2, 2, (x, m) => {
    if (m === 0 || x === 0) return 0;
    if (Math.sign(x) !== Math.sign(m)) return Errors.NUM;
    return snap(excelRound(x / m, 0) * m);
  }),
  numeric('EVEN', 1, 1, (x) => evenOdd(x, false)),
  numeric('ODD', 1, 1, (x) => evenOdd(x, true)),
  numeric('ROUNDDOWN', 2, 2, (x, d) => excelTrunc(x, d)),
  numeric('ROUNDUP', 2, 2, (x, d) => roundUp(x, d)),
  numeric('TRUNC', 1, 2, (x, d) => excelTrunc(x, d), [0]),
  numeric('FACT', 1, 1, (x) => (x < 0 ? Errors.NUM : x >= 171 ? Errors.NUM : factorial(Math.trunc(x)))),
  numeric('FACTDOUBLE', 1, 1, (x) => {
    const n = Math.trunc(x);
    if (n < 0) return Errors.NUM;
    let r = 1;
    for (let k = n; k > 1; k -= 2) r *= k;
    return r;
  }),
  numeric('COMBIN', 2, 2, (n0, k0) => {
    const n = Math.trunc(n0);
    const k = Math.trunc(k0);
    if (n < 0 || k < 0 || n < k) return Errors.NUM;
    return choose(n, k);
  }),
  numeric('COMBINA', 2, 2, (n0, k0) => {
    const n = Math.trunc(n0);
    const k = Math.trunc(k0);
    if (n < 0 || k < 0 || n < k) return Errors.NUM;
    if (k === 0) return 1;
    return choose(n + k - 1, k);
  }),
  fn('GCD', 1, VARIADIC, (args, ctx) => {
    const xs = integerArgs(args, ctx);
    if (xs instanceof ErrorValue) return xs;
    return xs.reduce((a, b) => gcd2(a, b), 0);
  }),
  fn('LCM', 1, VARIADIC, (args, ctx) => {
    const xs = integerArgs(args, ctx);
    if (xs instanceof ErrorValue) return xs;
    let l = 1;
    for (const x of xs) {
      if (x === 0) return 0;
      l = (l / gcd2(l, x)) * x;
    }
    return Number.isFinite(l) && l < 2 ** 53 ? l : Errors.NUM;
  }),
  fn('MULTINOMIAL', 1, VARIADIC, (args, ctx) => {
    const xs = numbers(args, ctx);
    if (xs instanceof ErrorValue) return xs;
    let total = 0;
    let r = 1;
    for (const x0 of xs) {
      const x = Math.trunc(x0);
      if (x < 0) return Errors.NUM;
      for (let i = 1; i <= x; i++) r = (r * ++total) / i;
    }
    return Number.isFinite(r) ? Math.round(r) : Errors.NUM;
  }),
  fn('PRODUCT', 1, VARIADIC, (args, ctx) => {
    const xs = numbers(args, ctx);
    return xs instanceof ErrorValue ? xs : A.product(xs);
  }),
  numeric('QUOTIENT', 2, 2, (x, y) => (y === 0 ? Errors.DIV0 : Math.trunc(round15(x / y)))),
  fn('RAND', 0, 0, () => Math.random(), { volatile: true }),
  fn(
    'RANDBETWEEN',
    2,
    2,
    (args) => {
      const lo = num(args[0]);
      if (isErr(lo)) return lo;
      const hi = num(args[1]);
      if (isErr(hi)) return hi;
      const a = Math.ceil(lo);
      const b = Math.floor(hi);
      if (a > b) return Errors.NUM;
      return a + Math.floor(Math.random() * (b - a + 1));
    },
    { lift: 'all', volatile: true },
  ),
  fn('RANDARRAY', 0, 5, randarray, { volatile: true }),
  fn(
    'ROMAN',
    1,
    2,
    (args) => {
      const n = int(args[0]);
      if (isErr(n)) return n;
      let mode = 0;
      const m = args[1];
      if (m !== undefined && m !== MISSING) {
        if (typeof m === 'boolean') mode = m ? 0 : 4;
        else {
          const k = int(m);
          if (isErr(k)) return k;
          mode = k;
        }
      }
      if (n < 0 || n > 3999 || mode < 0 || mode > 4) return Errors.VALUE;
      return toRoman(n, mode);
    },
    { lift: 'all' },
  ),
  fn(
    'ARABIC',
    1,
    1,
    (args) => {
      const s = str(args[0]);
      return isErr(s) ? s : fromRoman(s);
    },
    { lift: 'all' },
  ),
  fn(
    'BASE',
    2,
    3,
    (args) => {
      const n = num(args[0]);
      if (isErr(n)) return n;
      const radix = int(args[1]);
      if (isErr(radix)) return radix;
      const len = int(args[2], 0);
      if (isErr(len)) return len;
      if (n < 0 || n >= 2 ** 53 || radix < 2 || radix > 36 || len < 0 || len > 255) return Errors.NUM;
      return Math.trunc(n).toString(radix).toUpperCase().padStart(len, '0');
    },
    { lift: 'all' },
  ),
  fn(
    'DECIMAL',
    2,
    2,
    (args) => {
      const s = str(args[0]);
      if (isErr(s)) return s;
      const radix = int(args[1]);
      if (isErr(radix)) return radix;
      if (radix < 2 || radix > 36 || s.length > 255) return Errors.NUM;
      let t = s.trim().toLowerCase();
      if (radix === 16 && t.startsWith('0x')) t = t.slice(2);
      if (radix === 2 && t.endsWith('b')) t = t.slice(0, -1);
      if (t === '') return 0;
      let v = 0;
      for (const ch of t) {
        const d = Number.parseInt(ch, 36);
        if (Number.isNaN(d) || d >= radix) return Errors.NUM;
        v = v * radix + d;
      }
      return v < 2 ** 53 ? v : Errors.NUM;
    },
    { lift: 'all' },
  ),
  fn('SUMSQ', 1, VARIADIC, (args, ctx) => {
    const xs = numbers(args, ctx);
    return xs instanceof ErrorValue ? xs : sum(xs.map((x) => x * x));
  }),
  fn('SUMX2MY2', 2, 2, (args, ctx) => pairSum(args, ctx, (x, y) => x * x - y * y)),
  fn('SUMX2PY2', 2, 2, (args, ctx) => pairSum(args, ctx, (x, y) => x * x + y * y)),
  fn('SUMXMY2', 2, 2, (args, ctx) => pairSum(args, ctx, (x, y) => (x - y) * (x - y))),
  fn('SUMPRODUCT', 1, VARIADIC, sumproduct),
  fn('SERIESSUM', 4, 4, (args, ctx) => {
    const x = numberOf(args[0], ctx);
    if (isErr(x)) return x;
    const n = numberOf(args[1], ctx);
    if (isErr(n)) return n;
    const m = numberOf(args[2], ctx);
    if (isErr(m)) return m;
    const arr = arrayOf(args[3], ctx);
    if (isErr(arr)) return arr;
    let s = 0;
    let i = 0;
    for (const c of arr.flat()) {
      if (c instanceof ErrorValue) return c;
      if (typeof c !== 'number') return Errors.VALUE;
      s += c * x ** (n + i * m);
      i++;
    }
    return Number.isFinite(s) ? s : Errors.NUM;
  }),
  fn('MDETERM', 1, 1, (args, ctx) => {
    const m = numericMatrix(args[0], ctx);
    if (isErr(m)) return m;
    if (m.length !== (m[0] as number[]).length) return Errors.VALUE;
    return luSolve(m, false).det;
  }),
  fn('MINVERSE', 1, 1, (args, ctx) => {
    const m = numericMatrix(args[0], ctx);
    if (isErr(m)) return m;
    if (m.length !== (m[0] as number[]).length) return Errors.VALUE;
    const { inv } = luSolve(m, true);
    if (!inv) return Errors.NUM;
    return new Array2D(inv.map((r) => r.map((v) => (Math.abs(v) < 1e-15 ? 0 : v))));
  }),
  fn('MMULT', 2, 2, (args, ctx) => {
    const a = numericMatrix(args[0], ctx);
    if (isErr(a)) return a;
    const b = numericMatrix(args[1], ctx);
    if (isErr(b)) return b;
    const n = (a[0] as number[]).length;
    if (n !== b.length) return Errors.VALUE;
    const p = (b[0] as number[]).length;
    return Array2D.build(a.length, p, (i, j) => {
      let s = 0;
      for (let k = 0; k < n; k++) s += ((a[i] as number[])[k] as number) * ((b[k] as number[])[j] as number);
      return s;
    });
  }),
  fn('MUNIT', 1, 1, (args, ctx) => {
    const n = intOf(args[0], ctx);
    if (isErr(n)) return n;
    if (n < 1) return Errors.VALUE;
    return Array2D.build(n, n, (r, c) => (r === c ? 1 : 0));
  }),
  fn('SUBTOTAL', 2, VARIADIC, subtotal),
  fn('AGGREGATE', 3, VARIADIC, aggregate),
];
