/**
 * Comparison rules.
 *
 * Comparison operators: numbers < text < logicals; text is compared case-insensitively (accent
 * sensitive); numbers are compared after rounding to 15 significant digits (so `0.1+0.2=0.3` is
 * TRUE); a blank takes the type of the other operand (0, "" or FALSE). Errors propagate.
 */
import { ErrorValue } from './errors.ts';
import { round15 } from './number.ts';
import { BLANK, type Scalar } from './types.ts';

const collator = new Intl.Collator('en-US', { sensitivity: 'accent', numeric: false });

/** Case-insensitive, accent-sensitive text comparison (-1, 0, 1). */
export function compareText(a: string, b: string): number {
  if (a === b) return 0;
  const c = collator.compare(a, b);
  return c < 0 ? -1 : c > 0 ? 1 : 0;
}

/** True when two texts are equal ignoring case. */
export function textEquals(a: string, b: string): boolean {
  return a === b || compareText(a, b) === 0;
}

function typeRank(v: Scalar): number {
  switch (typeof v) {
    case 'number':
      return 0;
    case 'string':
      return 1;
    case 'boolean':
      return 2;
    default:
      return 3;
  }
}

/**
 * Compares two scalars with the comparison-operator rules. Returns -1, 0 or 1, or the first error.
 */
export function compareScalars(a: Scalar, b: Scalar): number | ErrorValue {
  if (a instanceof ErrorValue) return a;
  if (b instanceof ErrorValue) return b;
  let x = a;
  let y = b;
  if (x === BLANK && y === BLANK) return 0;
  if (x === BLANK) x = blankAs(y);
  else if (y === BLANK) y = blankAs(x);
  const tx = typeRank(x);
  const ty = typeRank(y);
  if (tx !== ty) return tx < ty ? -1 : 1;
  if (tx === 0) {
    const nx = round15(x as number);
    const ny = round15(y as number);
    return nx < ny ? -1 : nx > ny ? 1 : 0;
  }
  if (tx === 1) return compareText(x as string, y as string);
  return x === y ? 0 : x ? 1 : -1;
}

function blankAs(other: Scalar): Scalar {
  switch (typeof other) {
    case 'string':
      return '';
    case 'boolean':
      return false;
    default:
      return 0;
  }
}

/**
 * Total order used by SORT and friends: numbers < text < logicals < errors < blanks (blanks always
 * last regardless of direction is handled by the caller).
 */
export function sortCompare(a: Scalar, b: Scalar): number {
  const ra = a === BLANK ? 4 : typeRank(a);
  const rb = b === BLANK ? 4 : typeRank(b);
  if (ra !== rb) return ra - rb;
  switch (ra) {
    case 0:
      return (a as number) - (b as number);
    case 1:
      return compareText(a as string, b as string);
    case 2:
      return (a ? 1 : 0) - (b ? 1 : 0);
    case 3:
      return (a as ErrorValue).typeNumber - (b as ErrorValue).typeNumber;
    default:
      return 0;
  }
}

/** Equality used by lookups and UNIQUE: same type, numbers exactly (15 digits), text case-insensitive. */
export function lookupEquals(a: Scalar, b: Scalar): boolean {
  if (typeof a === 'number' && typeof b === 'number') return a === b || round15(a) === round15(b);
  if (typeof a === 'string' && typeof b === 'string') return textEquals(a, b);
  if (a instanceof ErrorValue && b instanceof ErrorValue) return a.code === b.code;
  return a === b;
}
