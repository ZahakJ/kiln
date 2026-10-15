/**
 * Database functions (DSUM, DAVERAGE, DCOUNT…) and Web functions (ENCODEURL).
 *
 * A database is a range whose first row holds field labels. Criteria are a range whose first row holds
 * labels and whose further rows hold conditions: conditions in one row must all hold (AND), any row
 * may match (OR). As in Excel, a plain text condition matches values that *begin* with the text,
 * `=text` matches exactly, and comparison operators work as in COUNTIF.
 */
import type { CallContext } from '../engine/context.ts';
import { isOmitted } from '../engine/functions/helpers.ts';
import type { Arg, FunctionDescriptor } from '../engine/functions/registry.ts';
import { toText } from '../values/coerce.ts';
import { textEquals } from '../values/compare.ts';
import { Errors, ErrorValue } from '../values/errors.ts';
import { type Array2D, BLANK, type Scalar } from '../values/types.ts';
import * as A from './agg.ts';
import { type Predicate, parseCriterion } from './criteria.ts';
import { arrayOf, fn, isErr, scalar, str } from './lib.ts';

function fieldIndex(db: Array2D, a: Arg | undefined, ctx: CallContext): number | ErrorValue | undefined {
  if (a === undefined || isOmitted(a)) return undefined;
  const v = scalar(a, ctx);
  if (v instanceof ErrorValue) return v;
  if (typeof v === 'number') {
    const i = Math.trunc(v);
    return i < 1 || i > db.cols ? Errors.VALUE : i - 1;
  }
  const label = toText(v);
  if (isErr(label)) return label;
  const header = db.data[0] as Scalar[];
  const i = header.findIndex((h) => typeof h === 'string' && textEquals(h.trim(), label.trim()));
  return i < 0 ? Errors.VALUE : i;
}

function conditionPredicate(c: Scalar, date1904: boolean): Predicate | undefined {
  if (c === BLANK || c === '') return undefined;
  if (typeof c === 'string' && !/^(<=|>=|<>|=|<|>)/.test(c)) {
    // Plain text: "begins with" (unless it reads as a number or logical, which match exactly).
    const p = parseCriterion(c, date1904);
    const prefix = parseCriterion(`${c}*`, date1904);
    return (v) => (typeof v === 'string' ? prefix(v) : p(v));
  }
  return parseCriterion(c, date1904);
}

/** Row indices (1-based within the database) that satisfy the criteria range. */
function matchingRows(db: Array2D, crit: Array2D, ctx: CallContext): number[] | ErrorValue {
  const header = db.data[0] as Scalar[];
  const critHeader = crit.data[0] as Scalar[];
  const cols: (number | undefined)[] = critHeader.map((h) => {
    if (h === BLANK) return undefined;
    const t = toText(h);
    if (isErr(t)) return undefined;
    const i = header.findIndex((x) => typeof x === 'string' && textEquals(x.trim(), t.trim()));
    return i < 0 ? -1 : i;
  });
  const rules: { col: number; pred: Predicate }[][] = [];
  for (let r = 1; r < crit.rows; r++) {
    const row: { col: number; pred: Predicate }[] = [];
    for (let c = 0; c < crit.cols; c++) {
      const p = conditionPredicate(crit.get(r, c), ctx.date1904);
      if (!p) continue;
      const col = cols[c];
      if (col === undefined || col < 0) {
        row.push({ col: -1, pred: () => false });
        continue;
      }
      row.push({ col, pred: p });
    }
    rules.push(row);
  }
  const out: number[] = [];
  for (let r = 1; r < db.rows; r++) {
    const hit =
      rules.length === 0 ||
      rules.some((row) => row.every((rule) => rule.col >= 0 && rule.pred(db.get(r, rule.col))));
    if (hit) out.push(r);
  }
  return out;
}

type Mode = 'numbers' | 'counta' | 'count' | 'get';

function dfunc(name: string, mode: Mode, agg?: A.Agg): FunctionDescriptor {
  return fn(name, 3, 3, (args, ctx) => {
    const db = arrayOf(args[0], ctx);
    if (isErr(db)) return db;
    const crit = arrayOf(args[2], ctx);
    if (isErr(crit)) return crit;
    if (db.rows < 1 || crit.rows < 1) return Errors.VALUE;
    const field = fieldIndex(db, args[1], ctx);
    if (isErr(field)) return field;
    const rows = matchingRows(db, crit, ctx);
    if (isErr(rows)) return rows;
    if (field === undefined) {
      if (mode === 'count' || mode === 'counta') return rows.length;
      return Errors.VALUE;
    }
    if (mode === 'get') {
      if (rows.length === 0) return Errors.VALUE;
      if (rows.length > 1) return Errors.NUM;
      const v = db.get(rows[0] as number, field);
      return v === BLANK ? 0 : v;
    }
    if (mode === 'counta') return rows.filter((r) => db.get(r, field) !== BLANK).length;
    const xs: number[] = [];
    for (const r of rows) {
      const v = db.get(r, field);
      if (typeof v === 'number') xs.push(v);
    }
    if (mode === 'count') return xs.length;
    return (agg as A.Agg)(xs);
  });
}

/** RFC 3986 percent-encoding of everything but unreserved characters (as Excel's ENCODEURL). */
export function encodeUrl(s: string): string {
  let out = '';
  for (const byte of new TextEncoder().encode(s)) {
    const ch = String.fromCharCode(byte);
    out += /[A-Za-z0-9\-_.~]/.test(ch) ? ch : `%${byte.toString(16).toUpperCase().padStart(2, '0')}`;
  }
  return out;
}

export const databaseFunctions: FunctionDescriptor[] = [
  dfunc('DAVERAGE', 'numbers', A.average),
  dfunc('DCOUNT', 'count'),
  dfunc('DCOUNTA', 'counta'),
  dfunc('DGET', 'get'),
  dfunc('DMAX', 'numbers', A.maxOf),
  dfunc('DMIN', 'numbers', A.minOf),
  dfunc('DPRODUCT', 'numbers', A.product),
  dfunc('DSTDEV', 'numbers', A.stdevS),
  dfunc('DSTDEVP', 'numbers', A.stdevP),
  dfunc('DSUM', 'numbers', A.sumOf),
  dfunc('DVAR', 'numbers', A.varS),
  dfunc('DVARP', 'numbers', A.varP),
];

export const webFunctions: FunctionDescriptor[] = [
  fn(
    'ENCODEURL',
    1,
    1,
    (args) => {
      const s = str(args[0]);
      return isErr(s) ? s : encodeUrl(s);
    },
    { lift: 'all' },
  ),
];
