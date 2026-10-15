/**
 * Building blocks shared by the function library: argument coercion for lifted (scalar) arguments,
 * descriptor builders, and collectors that reproduce how Excel's aggregate functions treat text,
 * logicals and blanks in ranges versus typed-in arguments.
 */
import type { CallContext } from '../engine/context.ts';
import { force, isOmitted } from '../engine/functions/helpers.ts';
import type { Arg, FunctionDescriptor } from '../engine/functions/registry.ts';
import { parseNumberText, toBoolean, toNumber, toText } from '../values/coerce.ts';
import { Errors, ErrorValue } from '../values/errors.ts';
import { Array2D, BLANK, Lambda, MISSING, RefValue, type Scalar } from '../values/types.ts';

export const VARIADIC = 255;

/** Every argument index after the first (lazy CHOOSE/SWITCH branches). */
export const AFTER_FIRST: readonly number[] = Array.from({ length: 254 }, (_, i) => i + 1);

export type Impl = FunctionDescriptor['impl'];

/** Options for {@link fn}. */
export interface FnOptions {
  lazy?: readonly number[];
  lift?: readonly number[] | 'all';
  volatile?: boolean;
}

/** A descriptor without docs (docs are attached from `docs/*.json`). */
export function fn(name: string, minArgs: number, maxArgs: number, impl: Impl, opts: FnOptions = {}) {
  const d: FunctionDescriptor = { name, minArgs, maxArgs, impl };
  if (opts.lazy) d.lazy = opts.lazy;
  if (opts.lift) d.lift = opts.lift;
  if (opts.volatile) d.volatile = true;
  return d;
}

/** Same implementation registered under another name (compatibility aliases such as STDEV/STDEV.S). */
export function alias(name: string, of: FunctionDescriptor): FunctionDescriptor {
  const d: FunctionDescriptor = { ...of, name };
  delete d.docs;
  return d;
}

export function isErr(v: unknown): v is ErrorValue {
  return v instanceof ErrorValue;
}

/** A finite number, or #NUM! for overflow / NaN. */
export function finite(x: number): number | ErrorValue {
  return Number.isFinite(x) ? x : Errors.NUM;
}

// ── lifted (scalar) argument helpers ─────────────────────────────────────

/**
 * A number from a lifted argument. `undefined` (not passed) gives `dflt`; an empty argument
 * (`MISSING`) gives `emptyAs` (default 0, as Excel treats `ROUND(2.5,)`).
 */
export function num(a: Arg | undefined, dflt?: number, emptyAs = 0): number | ErrorValue {
  if (a === undefined) return dflt ?? 0;
  if (a === MISSING) return emptyAs;
  return toNumber(a as Scalar);
}

/** An integer (truncated toward zero). */
export function int(a: Arg | undefined, dflt?: number, emptyAs = 0): number | ErrorValue {
  const n = num(a, dflt, emptyAs);
  return typeof n === 'number' ? Math.trunc(n) : n;
}

/** Text from a lifted argument (omitted → ""). */
export function str(a: Arg | undefined): string | ErrorValue {
  if (a === undefined || a === MISSING) return '';
  return toText(a as Scalar);
}

/** A logical from a lifted argument. */
export function bool(a: Arg | undefined, dflt: boolean): boolean | ErrorValue {
  if (a === undefined) return dflt;
  if (a === MISSING) return false;
  return toBoolean(a as Scalar);
}

/** True when the argument was not passed at all or left empty. */
export function omitted(a: Arg | undefined): boolean {
  return isOmitted(a);
}

/**
 * A numeric function of fixed arity with every argument lifted over arrays and coerced to a number.
 * `optional` gives defaults for trailing arguments. Non-finite results become #NUM!.
 */
export function numeric(
  name: string,
  minArgs: number,
  maxArgs: number,
  body: (...xs: number[]) => number | ErrorValue,
  defaults: readonly number[] = [],
): FunctionDescriptor {
  return fn(
    name,
    minArgs,
    maxArgs,
    (args) => {
      const xs: number[] = [];
      for (let i = 0; i < maxArgs; i++) {
        const a = args[i];
        const n = num(a, defaults[i - minArgs] ?? 0);
        if (n instanceof ErrorValue) return n;
        xs.push(n);
      }
      const r = body(...xs);
      return typeof r === 'number' ? finite(r) : r;
    },
    { lift: 'all' },
  );
}

// ── collectors ───────────────────────────────────────────────────────────

/**
 * Collects numbers like SUM: in ranges/arrays only numbers count; typed-in logicals and numeric text
 * count; other typed-in text is #VALUE!. Errors propagate.
 */
export function numbers(args: readonly Arg[], ctx: CallContext): number[] | ErrorValue {
  const out: number[] = [];
  let err: ErrorValue | undefined;
  for (const a of args) {
    const v = force(a);
    if (v === MISSING) {
      out.push(0); // an empty argument (`SUM(1,,2)`) counts as zero
      continue;
    }
    ctx.forEachScalar(v, (x, fromRef) => {
      if (typeof x === 'number') out.push(x);
      else if (x instanceof ErrorValue) {
        err = x;
        return false;
      } else if (!fromRef) {
        if (typeof x === 'boolean') out.push(x ? 1 : 0);
        else if (typeof x === 'string') {
          const n = parseNumberText(x, { date1904: ctx.date1904 });
          if (n === undefined) {
            err = Errors.VALUE;
            return false;
          }
          out.push(n);
        } else if (x === BLANK) out.push(0);
      }
      return undefined;
    });
    if (err) return err;
  }
  return out;
}

/**
 * Collects values like the "A" functions (AVERAGEA, MAXA, STDEVA…): in ranges text counts as 0 and
 * logicals as 1/0, blanks are skipped; typed-in values behave as in {@link numbers}.
 */
export function numbersA(args: readonly Arg[], ctx: CallContext): number[] | ErrorValue {
  const out: number[] = [];
  let err: ErrorValue | undefined;
  for (const a of args) {
    const v = force(a);
    if (v === MISSING) {
      out.push(0); // an empty argument (`SUM(1,,2)`) counts as zero
      continue;
    }
    ctx.forEachScalar(v, (x, fromRef) => {
      if (typeof x === 'number') out.push(x);
      else if (typeof x === 'boolean') out.push(x ? 1 : 0);
      else if (x instanceof ErrorValue) {
        err = x;
        return false;
      } else if (typeof x === 'string') {
        if (fromRef) out.push(0);
        else {
          const n = parseNumberText(x, { date1904: ctx.date1904 });
          if (n === undefined) {
            err = Errors.VALUE;
            return false;
          }
          out.push(n);
        }
      } else if (x === BLANK && !fromRef) out.push(0);
      return undefined;
    });
    if (err) return err;
  }
  return out;
}

/** Numbers of a single range/array argument (text, logicals, blanks skipped; errors propagate). */
export function numbersOf(a: Arg | undefined, ctx: CallContext): number[] | ErrorValue {
  return numbers(a === undefined ? [] : [a], ctx);
}

/** Every scalar of an argument in row-major order. */
export function scalarsOf(a: Arg | undefined, ctx: CallContext): Scalar[] {
  const out: Scalar[] = [];
  if (a === undefined) return out;
  const v = force(a);
  if (v === MISSING) return out;
  ctx.forEachScalar(v, (x) => void out.push(x));
  return out;
}

/** An argument as a 2-D array of values (references dereferenced, scalars as 1×1). */
export function arrayOf(a: Arg | undefined, ctx: CallContext): Array2D | ErrorValue {
  if (a === undefined) return Errors.VALUE;
  const v = force(a);
  if (v instanceof RefValue && v.areas.length > 1) return Errors.VALUE;
  if (v instanceof Lambda) return Errors.VALUE;
  return ctx.toArray(v);
}

/**
 * Two equally sized arrays as paired numbers, keeping only positions where both are numbers (CORREL,
 * SLOPE, COVARIANCE…). Different sizes are #N/A; errors propagate.
 */
export function numericPairs(
  a: Arg | undefined,
  b: Arg | undefined,
  ctx: CallContext,
): { xs: number[]; ys: number[] } | ErrorValue {
  const x = arrayOf(a, ctx);
  if (x instanceof ErrorValue) return x;
  const y = arrayOf(b, ctx);
  if (y instanceof ErrorValue) return y;
  const fx = x.flat();
  const fy = y.flat();
  if (fx.length !== fy.length) return Errors.NA;
  const xs: number[] = [];
  const ys: number[] = [];
  for (let i = 0; i < fx.length; i++) {
    const p = fx[i] as Scalar;
    const q = fy[i] as Scalar;
    if (p instanceof ErrorValue) return p;
    if (q instanceof ErrorValue) return q;
    if (typeof p === 'number' && typeof q === 'number') {
      xs.push(p);
      ys.push(q);
    }
  }
  return { xs, ys };
}

/** A scalar from a non-lifted argument (implicit intersection for ranges). */
export function scalar(a: Arg | undefined, ctx: CallContext): Scalar {
  if (a === undefined || a === MISSING) return BLANK;
  const v = force(a);
  if (v === MISSING) return BLANK;
  return ctx.toScalar(v);
}

export function numberOf(a: Arg | undefined, ctx: CallContext, dflt?: number): number | ErrorValue {
  if (a === undefined || (dflt !== undefined && isOmitted(a))) return dflt ?? 0;
  return toNumber(scalar(a, ctx), { date1904: ctx.date1904 });
}

export function intOf(a: Arg | undefined, ctx: CallContext, dflt?: number): number | ErrorValue {
  const n = numberOf(a, ctx, dflt);
  return typeof n === 'number' ? Math.trunc(n) : n;
}

export function textOf(a: Arg | undefined, ctx: CallContext): string | ErrorValue {
  return toText(scalar(a, ctx));
}

export function boolOf(a: Arg | undefined, ctx: CallContext, dflt: boolean): boolean | ErrorValue {
  if (a === undefined || isOmitted(a)) return dflt;
  return toBoolean(scalar(a, ctx));
}

// ── numeric helpers ──────────────────────────────────────────────────────

export function sum(xs: readonly number[]): number {
  // Neumaier summation keeps long sums of decimals as close to Excel's as possible.
  let s = 0;
  let c = 0;
  for (const x of xs) {
    const t = s + x;
    if (Math.abs(s) >= Math.abs(x)) c += s - t + x;
    else c += x - t + s;
    s = t;
  }
  return s + c;
}

export function mean(xs: readonly number[]): number {
  return sum(xs) / xs.length;
}

/** Sum of squared deviations from the mean. */
export function devsq(xs: readonly number[]): number {
  const m = mean(xs);
  let s = 0;
  for (const x of xs) s += (x - m) * (x - m);
  return s;
}

export function sortedAsc(xs: readonly number[]): number[] {
  return [...xs].sort((a, b) => a - b);
}

/** Builds a column array of scalars. */
export function column(values: Scalar[]): Array2D {
  return Array2D.column(values);
}
