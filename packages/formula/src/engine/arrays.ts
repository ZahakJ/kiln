/**
 * Array broadcasting (Excel's "array lifting" of scalar operations).
 *
 * Two arrays combine element-wise; a single row or column is repeated to match the other operand;
 * positions outside a smaller operand are `#N/A`, exactly as Excel does for `{1,2,3}+{1,2}`.
 */
import { Errors } from '../values/errors.ts';
import { Array2D, type Scalar, type Value } from '../values/types.ts';

export function dimsOf(values: readonly (Scalar | Array2D)[]): { rows: number; cols: number } {
  let rows = 1;
  let cols = 1;
  for (const v of values) {
    if (v instanceof Array2D) {
      if (v.rows > rows) rows = v.rows;
      if (v.cols > cols) cols = v.cols;
    }
  }
  return { rows, cols };
}

/** Element (r, c) of a broadcast operand. */
export function pick(v: Scalar | Array2D, r: number, c: number): Scalar {
  if (!(v instanceof Array2D)) return v;
  const rr = v.rows === 1 ? 0 : r;
  const cc = v.cols === 1 ? 0 : c;
  if (rr >= v.rows || cc >= v.cols) return Errors.NA;
  return v.get(rr, cc);
}

export function liftUnary(a: Scalar | Array2D, fn: (x: Scalar) => Scalar): Value {
  if (!(a instanceof Array2D)) return fn(a);
  return a.map((x) => fn(x));
}

export function liftBinary(
  a: Scalar | Array2D,
  b: Scalar | Array2D,
  fn: (x: Scalar, y: Scalar) => Scalar,
): Value {
  if (!(a instanceof Array2D) && !(b instanceof Array2D)) return fn(a, b);
  const { rows, cols } = dimsOf([a, b]);
  return Array2D.build(rows, cols, (r, c) => {
    const x = pick(a, r, c);
    const y = pick(b, r, c);
    return fn(x, y);
  });
}
