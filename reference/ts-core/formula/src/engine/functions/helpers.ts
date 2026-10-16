/**
 * Small helpers shared by function implementations.
 */
import { parseNumberText, toBoolean, toNumber, toText } from '../../values/coerce.ts';
import { Errors, ErrorValue } from '../../values/errors.ts';
import { BLANK, type EvalValue, MISSING, type Scalar } from '../../values/types.ts';
import type { CallContext } from '../context.ts';
import { type Arg, LazyArg } from './registry.ts';

/** Every argument index (for functions whose arguments are all lazy). */
export const ALL_ARGS: readonly number[] = Array.from({ length: 255 }, (_, i) => i);

/** Evaluates a possibly-lazy argument. */
export function force(a: Arg | undefined): EvalValue {
  if (a === undefined) return MISSING;
  return a instanceof LazyArg ? a.force() : a;
}

/** True when an argument was omitted or left empty. */
export function isOmitted(a: Arg | undefined): boolean {
  return a === undefined || a === MISSING || (a instanceof LazyArg && a.isEmpty);
}

/** A scalar from an argument (implicit intersection for ranges). */
export function scalarArg(a: Arg | undefined, ctx: CallContext): Scalar {
  if (a === undefined || a === MISSING) return BLANK;
  return ctx.toScalar(force(a));
}

/** A number from an argument; `dflt` when omitted. */
export function numberArg(a: Arg | undefined, ctx: CallContext, dflt?: number): number | ErrorValue {
  if (a === undefined && dflt !== undefined) return dflt;
  const v = scalarArg(a, ctx);
  return toNumber(v, { date1904: ctx.date1904 });
}

/** An integer (truncated toward zero) from an argument. */
export function intArg(a: Arg | undefined, ctx: CallContext, dflt?: number): number | ErrorValue {
  const n = numberArg(a, ctx, dflt);
  return typeof n === 'number' ? Math.trunc(n) : n;
}

export function boolArg(a: Arg | undefined, ctx: CallContext, dflt: boolean): boolean | ErrorValue {
  if (a === undefined) return dflt;
  return toBoolean(scalarArg(a, ctx));
}

export function textArg(a: Arg | undefined, ctx: CallContext): string | ErrorValue {
  return toText(scalarArg(a, ctx));
}

/**
 * Collects numbers the way SUM/AVERAGE/MIN/MAX do: inside ranges and arrays only numbers count
 * (text, logicals and blanks are skipped); typed-in arguments also accept logicals and numeric text.
 * Errors propagate.
 */
export function collectNumbers(args: Arg[], ctx: CallContext): number[] | ErrorValue {
  const out: number[] = [];
  let err: ErrorValue | undefined;
  for (const a of args) {
    ctx.forEachScalar(force(a), (v, fromRef) => {
      if (typeof v === 'number') out.push(v);
      else if (v instanceof ErrorValue) {
        err = v;
        return false;
      } else if (!fromRef) {
        if (typeof v === 'boolean') out.push(v ? 1 : 0);
        else if (typeof v === 'string') {
          const n = parseNumberText(v, { date1904: ctx.date1904 });
          if (n === undefined) {
            err = Errors.VALUE;
            return false;
          }
          out.push(n);
        } else if (v === BLANK) out.push(0);
      }
      return undefined;
    });
    if (err) return err;
  }
  return out;
}

/** Flattens arguments to scalars (ranges and arrays row by row), stopping at the first error. */
export function flattenScalars(args: Arg[], ctx: CallContext): Scalar[] {
  const out: Scalar[] = [];
  for (const a of args) ctx.forEachScalar(force(a), (v) => void out.push(v));
  return out;
}

export function isErr(v: unknown): v is ErrorValue {
  return v instanceof ErrorValue;
}
