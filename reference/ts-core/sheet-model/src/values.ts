/**
 * Cell values as stored in the sheet model, and their JSON-safe encoding.
 *
 * A stored value is `number | string | boolean | ErrorValue | null` (`null` = empty). Commands,
 * snapshots and journals carry {@link EncodedValue}s instead, where an error is `{ "e": "#N/A" }`, so
 * that everything the model serialises is plain JSON (and structured-clone safe).
 */
import { BLANK, ERROR_CODES, type ErrorCode, ErrorValue, type Scalar } from '@kiln-ref/formula';

export type CellValue = number | string | boolean | ErrorValue | null;

export type EncodedValue = number | string | boolean | null | { e: ErrorCode };

const ERROR_SET = new Set<string>(ERROR_CODES);
const ERROR_CACHE = new Map<string, ErrorValue>();

/** Shared ErrorValue instance for a code. */
export function errorValue(code: ErrorCode): ErrorValue {
  let e = ERROR_CACHE.get(code);
  if (!e) {
    e = new ErrorValue(code);
    ERROR_CACHE.set(code, e);
  }
  return e;
}

export function isErrorCode(s: string): s is ErrorCode {
  return ERROR_SET.has(s);
}

export function encodeValue(v: CellValue | undefined): EncodedValue {
  if (v === undefined || v === null) return null;
  if (v instanceof ErrorValue) return { e: v.code };
  if (typeof v === 'number' && !Number.isFinite(v)) return { e: '#NUM!' };
  return v;
}

export function decodeValue(v: EncodedValue | undefined): CellValue {
  if (v === undefined || v === null) return null;
  if (typeof v === 'object') {
    if (!isErrorCode(v.e)) throw new TypeError(`Unknown error code ${String(v.e)}`);
    return errorValue(v.e);
  }
  return v;
}

/** Converts a stored value to the formula engine's scalar (empty → undefined). */
export function toScalar(v: CellValue): Scalar | undefined {
  return v === null ? undefined : v;
}

/** Converts an engine scalar to a stored value (blank → null). */
export function fromScalar(v: Scalar | undefined): CellValue {
  if (v === undefined || v === BLANK) return null;
  return v as CellValue;
}

export function valuesEqual(a: CellValue, b: CellValue): boolean {
  if (a === b) return true;
  if (a instanceof ErrorValue && b instanceof ErrorValue) return a.code === b.code;
  return typeof a === 'number' && typeof b === 'number' && Number.isNaN(a) && Number.isNaN(b);
}
