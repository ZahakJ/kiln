/**
 * Helpers for the function-library tests: evaluating formulas in a fresh engine and comparing a value
 * with an expected result written as a formula literal (`6`, `"text"`, `TRUE`, `#N/A`, `{1,2;3,4}`).
 */
import { Engine, type EngineOptions } from '../../src/engine/engine.ts';
import { ErrorValue } from '../../src/values/errors.ts';
import { Array2D, BLANK, type Scalar, type Value } from '../../src/values/types.ts';

export function engine(opts: EngineOptions = {}): Engine {
  const e = new Engine({ now: () => new Date(2024, 0, 15, 12, 0, 0), ...opts });
  e.addSheet('Sheet1');
  return e;
}

/** Splits an array literal's body into element texts (respecting quotes). */
function splitElements(body: string): { text: string; rowEnd: boolean }[] {
  const out: { text: string; rowEnd: boolean }[] = [];
  let cur = '';
  let inStr = false;
  for (let i = 0; i < body.length; i++) {
    const ch = body[i] as string;
    if (ch === '"') {
      if (inStr && body[i + 1] === '"') {
        cur += '""';
        i++;
        continue;
      }
      inStr = !inStr;
      cur += ch;
    } else if (!inStr && (ch === ',' || ch === ';')) {
      out.push({ text: cur.trim(), rowEnd: ch === ';' });
      cur = '';
    } else cur += ch;
  }
  out.push({ text: cur.trim(), rowEnd: true });
  return out;
}

export interface Expected {
  /** Rows of element literals. */
  rows: string[][];
  isArray: boolean;
}

export function parseExpected(text: string): Expected {
  const t = text.trim();
  if (!t.startsWith('{')) return { rows: [[t]], isArray: false };
  const rows: string[][] = [[]];
  for (const el of splitElements(t.slice(1, -1))) {
    (rows[rows.length - 1] as string[]).push(el.text);
    if (el.rowEnd) rows.push([]);
  }
  rows.pop();
  return { rows, isArray: true };
}

/** Tolerance implied by how many digits a number literal was written with. */
export function displayTolerance(lit: string): number {
  const m = /^[+-]?(\d*)(?:\.(\d*))?(?:[eE]([+-]?\d+))?$/.exec(lit.trim());
  if (!m) return 0;
  const intDigits = (m[1] ?? '').replace(/^0+/, '');
  const frac = m[2] ?? '';
  const exp = m[3] ? Number(m[3]) : 0;
  const sig = (intDigits + frac).replace(/^0+/, '').length;
  if (sig >= 15) return Math.abs(Number(lit)) * 1e-13;
  return 0.5 * 10 ** (exp - frac.length) * (1 + 1e-9);
}

function literalValue(lit: string): Scalar {
  if (/^".*"$/s.test(lit)) return lit.slice(1, -1).replace(/""/g, '"');
  const u = lit.toUpperCase();
  if (u === 'TRUE') return true;
  if (u === 'FALSE') return false;
  if (lit.startsWith('#')) return new ErrorValue(lit as ErrorValue['code']);
  if (lit === '') return BLANK;
  return Number(lit);
}

function describe(v: Value | Scalar): string {
  if (v instanceof ErrorValue) return v.code;
  if (v instanceof Array2D) return `{${v.data.map((r) => r.map((x) => describe(x)).join(',')).join(';')}}`;
  if (typeof v === 'string') return JSON.stringify(v);
  if (v === BLANK) return '(blank)';
  return String(v);
}

function scalarMatches(actual: Scalar, lit: string, tol: (lit: string) => number): boolean {
  const exp = literalValue(lit);
  if (typeof exp === 'number') {
    if (typeof actual !== 'number') return false;
    if (Number.isNaN(exp)) return false;
    return Math.abs(actual - exp) <= tol(lit);
  }
  if (exp instanceof ErrorValue) return actual instanceof ErrorValue && actual.code === exp.code;
  if (exp === BLANK) return actual === BLANK || actual === 0 || actual === '';
  return actual === exp;
}

/** Returns undefined when `actual` matches the expected literal, else a description of the mismatch. */
export function compare(
  actual: Value,
  expected: string,
  tol: (lit: string) => number = displayTolerance,
): string | undefined {
  const e = parseExpected(expected);
  if (!e.isArray) {
    const v = actual instanceof Array2D && actual.size === 1 ? actual.get(0, 0) : actual;
    if (v instanceof Array2D) return `expected ${expected}, got array ${describe(v)}`;
    return scalarMatches(v as Scalar, e.rows[0]?.[0] ?? '', tol)
      ? undefined
      : `expected ${expected}, got ${describe(v)}`;
  }
  const arr = actual instanceof Array2D ? actual : new Array2D([[actual as Scalar]]);
  const rows = e.rows.length;
  const cols = e.rows[0]?.length ?? 0;
  if (arr.rows !== rows || arr.cols !== cols)
    return `expected ${expected} (${rows}×${cols}), got ${describe(arr)}`;
  for (let r = 0; r < rows; r++)
    for (let c = 0; c < cols; c++)
      if (!scalarMatches(arr.get(r, c), (e.rows[r] as string[])[c] as string, tol))
        return `expected ${expected}, got ${describe(arr)}`;
  return undefined;
}
