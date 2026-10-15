/**
 * Lookup & Reference and dynamic-array shaping functions beyond the core set.
 */
import type { CallContext } from '../engine/context.ts';
import { approxAscending, findExact, xmatch } from '../engine/functions/builtins/lookup.ts';
import { force, isOmitted } from '../engine/functions/helpers.ts';
import type { Arg, FunctionDescriptor } from '../engine/functions/registry.ts';
import { columnToLetters, quoteSheet, sheetNeedsQuotes } from '../parser/references.ts';
import { toBoolean, toNumber } from '../values/coerce.ts';
import { sortCompare } from '../values/compare.ts';
import { Errors, ErrorValue } from '../values/errors.ts';
import { Array2D, BLANK, type EvalValue, Lambda, MISSING, RefValue, type Scalar } from '../values/types.ts';
import { AFTER_FIRST, arrayOf, bool, fn, int, intOf, isErr, str, VARIADIC } from './lib.ts';

function calcEmpty(): ErrorValue {
  return new ErrorValue('#CALC!', 'The result is an empty array');
}

/** Dimensions of a range or array argument without reading it. */
function dims(v: EvalValue, ctx: CallContext): { rows: number; cols: number } | ErrorValue {
  if (v instanceof RefValue) {
    const a = v.single;
    if (!a) return Errors.REF;
    return { rows: a.r1 - a.r0 + 1, cols: a.c1 - a.c0 + 1 };
  }
  if (v instanceof ErrorValue) return v;
  if (v instanceof Lambda) return Errors.VALUE;
  const arr = ctx.toArray(v);
  if (arr instanceof ErrorValue) return arr;
  return { rows: arr.rows, cols: arr.cols };
}

function address(args: Arg[]): Scalar {
  const row = int(args[0]);
  if (isErr(row)) return row;
  const col = int(args[1]);
  if (isErr(col)) return col;
  const abs = int(args[2], 1, 1);
  if (isErr(abs)) return abs;
  const a1 = bool(args[3], true);
  if (isErr(a1)) return a1;
  if (row < 1 || col < 1 || row > 1_048_576 || col > 16_384 || abs < 1 || abs > 4) return Errors.VALUE;
  let out: string;
  const rowAbs = abs === 1 || abs === 2;
  const colAbs = abs === 1 || abs === 3;
  if (a1) out = `${colAbs ? '$' : ''}${columnToLetters(col - 1)}${rowAbs ? '$' : ''}${row}`;
  else out = `${rowAbs ? `R${row}` : `R[${row}]`}${colAbs ? `C${col}` : `C[${col}]`}`;
  if (args[4] !== undefined && args[4] !== MISSING) {
    const sheet = str(args[4]);
    if (isErr(sheet)) return sheet;
    if (sheet !== '')
      out = `${/^\[.*\]/.test(sheet) || !sheetNeedsQuotes(sheet) ? sheet : quoteSheet(sheet)}!${out}`;
  }
  return out;
}

function choose(args: Arg[]): EvalValue {
  const idx = toNumber(args[0] as Scalar);
  if (idx instanceof ErrorValue) return idx;
  const i = Math.trunc(idx);
  if (i < 1 || i >= args.length) return Errors.VALUE;
  const a = args[i];
  if (isOmitted(a)) return 0;
  return force(a);
}

function pickIndices(n: number, args: Arg[], ctx: CallContext): number[] | ErrorValue {
  const out: number[] = [];
  for (const a of args) {
    const arr = arrayOf(a, ctx);
    if (isErr(arr)) return arr;
    for (const v of arr.flat()) {
      const k = toNumber(v);
      if (k instanceof ErrorValue) return k;
      const i = Math.trunc(k);
      if (i === 0 || Math.abs(i) > n) return Errors.VALUE;
      out.push(i > 0 ? i - 1 : n + i);
    }
  }
  return out;
}

function chooseRowsCols(args: Arg[], ctx: CallContext, cols: boolean): Scalar | Array2D {
  const arr = arrayOf(args[0], ctx);
  if (isErr(arr)) return arr;
  const src = cols ? arr.transpose() : arr;
  const idx = pickIndices(src.rows, args.slice(1), ctx);
  if (isErr(idx)) return idx;
  const out = new Array2D(idx.map((i) => src.data[i] as Scalar[]));
  return cols ? out.transpose() : out;
}

function countArg(a: Arg | undefined, ctx: CallContext): number | undefined | ErrorValue {
  if (a === undefined || isOmitted(a)) return undefined;
  const n = toNumber(ctx.toScalar(force(a)));
  return n instanceof ErrorValue ? n : Math.trunc(n);
}

function take(args: Arg[], ctx: CallContext, drop: boolean): Scalar | Array2D {
  const arr = arrayOf(args[0], ctx);
  if (isErr(arr)) return arr;
  const r = countArg(args[1], ctx);
  if (isErr(r)) return r;
  const c = countArg(args[2], ctx);
  if (isErr(c)) return c;
  const span = (n: number | undefined, size: number): [number, number] => {
    if (n === undefined) return [0, size];
    if (drop) return n >= 0 ? [Math.min(n, size), size] : [0, Math.max(0, size + n)];
    return n >= 0 ? [0, Math.min(n, size)] : [Math.max(0, size + n), size];
  };
  if (!drop && (r === 0 || c === 0)) return calcEmpty();
  const [r0, r1] = span(r, arr.rows);
  const [c0, c1] = span(c, arr.cols);
  if (r1 <= r0 || c1 <= c0) return calcEmpty();
  return Array2D.build(r1 - r0, c1 - c0, (i, j) => arr.get(r0 + i, c0 + j));
}

function expand(args: Arg[], ctx: CallContext): Scalar | Array2D {
  const arr = arrayOf(args[0], ctx);
  if (isErr(arr)) return arr;
  const r = countArg(args[1], ctx);
  if (isErr(r)) return r;
  const c = countArg(args[2], ctx);
  if (isErr(c)) return c;
  const rows = r ?? arr.rows;
  const cols = c ?? arr.cols;
  if (rows < arr.rows || cols < arr.cols) return Errors.VALUE;
  const pad: Scalar = args[3] === undefined || isOmitted(args[3]) ? Errors.NA : ctx.toScalar(force(args[3]));
  return Array2D.build(rows, cols, (i, j) => (i < arr.rows && j < arr.cols ? arr.get(i, j) : pad));
}

function stack(args: Arg[], ctx: CallContext, horizontal: boolean): Scalar | Array2D {
  const parts: Array2D[] = [];
  for (const a of args) {
    const arr = arrayOf(a, ctx);
    if (isErr(arr)) return arr;
    parts.push(arr);
  }
  if (horizontal) {
    const rows = Math.max(...parts.map((p) => p.rows));
    const cols = parts.reduce((s, p) => s + p.cols, 0);
    const data: Scalar[][] = Array.from({ length: rows }, () => []);
    for (const p of parts)
      for (let i = 0; i < rows; i++)
        for (let j = 0; j < p.cols; j++) (data[i] as Scalar[]).push(i < p.rows ? p.get(i, j) : Errors.NA);
    return cols === 0 ? calcEmpty() : new Array2D(data);
  }
  const cols = Math.max(...parts.map((p) => p.cols));
  const data: Scalar[][] = [];
  for (const p of parts)
    for (let i = 0; i < p.rows; i++)
      data.push(Array.from({ length: cols }, (_, j) => (j < p.cols ? p.get(i, j) : Errors.NA)));
  return new Array2D(data);
}

function toVector(args: Arg[], ctx: CallContext, column: boolean): Scalar | Array2D {
  const arr = arrayOf(args[0], ctx);
  if (isErr(arr)) return arr;
  const ignore = args[1] === undefined || isOmitted(args[1]) ? 0 : toNumber(ctx.toScalar(force(args[1])));
  if (isErr(ignore)) return ignore;
  const byCol = args[2] === undefined || isOmitted(args[2]) ? false : toBoolean(ctx.toScalar(force(args[2])));
  if (isErr(byCol)) return byCol;
  if (ignore < 0 || ignore > 3) return Errors.VALUE;
  const src = byCol ? arr.transpose() : arr;
  const vals = src.flat().filter((v) => {
    if ((ignore === 1 || ignore === 3) && v === BLANK) return false;
    if ((ignore === 2 || ignore === 3) && v instanceof ErrorValue) return false;
    return true;
  });
  if (vals.length === 0) return calcEmpty();
  return column ? Array2D.column(vals) : Array2D.row(vals);
}

function wrap(args: Arg[], ctx: CallContext, rowsFirst: boolean): Scalar | Array2D {
  const arr = arrayOf(args[0], ctx);
  if (isErr(arr)) return arr;
  if (arr.rows > 1 && arr.cols > 1) return Errors.VALUE;
  const n = intOf(args[1], ctx);
  if (isErr(n)) return n;
  if (n < 1) return Errors.NUM;
  const pad: Scalar = args[2] === undefined || isOmitted(args[2]) ? Errors.NA : ctx.toScalar(force(args[2]));
  const vals = arr.flat();
  const lines = Math.ceil(vals.length / n);
  const out = Array2D.build(lines, n, (i, j) => {
    const k = i * n + j;
    return k < vals.length ? (vals[k] as Scalar) : pad;
  });
  return rowsFirst ? out : out.transpose();
}

function hlookup(args: Arg[], ctx: CallContext): EvalValue {
  const x = args[0] as Scalar;
  if (x instanceof ErrorValue) return x;
  const row = int(args[2]);
  if (isErr(row)) return row;
  let approx = true;
  if (args.length > 3 && args[3] !== MISSING) {
    const b = toBoolean(args[3] as Scalar);
    if (b instanceof ErrorValue) return b;
    approx = b;
  }
  if (row < 1) return Errors.VALUE;
  const table = arrayOf(args[1], ctx);
  if (isErr(table)) return table;
  if (row > table.rows) return Errors.REF;
  const first = table.data[0] as Scalar[];
  const key = x === BLANK ? 0 : x;
  const i = approx ? approxAscending(first, key) : findExact(first, key, true);
  return i < 0 ? Errors.NA : table.get(row - 1, i);
}

function lookup(args: Arg[], ctx: CallContext): EvalValue {
  const x = args[0] as Scalar;
  if (x instanceof ErrorValue) return x;
  const vec = arrayOf(args[1], ctx);
  if (isErr(vec)) return vec;
  const key = x === BLANK ? 0 : x;
  let keys: Scalar[];
  let results: Scalar[];
  if (args.length > 2) {
    const res = arrayOf(args[2], ctx);
    if (isErr(res)) return res;
    keys = vec.rows === 1 ? (vec.data[0] as Scalar[]) : vec.data.map((r) => r[0] as Scalar);
    results = res.rows === 1 ? (res.data[0] as Scalar[]) : res.data.map((r) => r[0] as Scalar);
  } else if (vec.cols > vec.rows) {
    keys = vec.data[0] as Scalar[];
    results = vec.data[vec.rows - 1] as Scalar[];
  } else {
    keys = vec.data.map((r) => r[0] as Scalar);
    results = vec.data.map((r) => r[vec.cols - 1] as Scalar);
  }
  const i = approxAscending(keys, key);
  if (i < 0) return Errors.NA;
  return i < results.length ? (results[i] as Scalar) : Errors.NA;
}

function sortby(args: Arg[], ctx: CallContext): Scalar | Array2D {
  const arr = arrayOf(args[0], ctx);
  if (isErr(arr)) return arr;
  const keys: { values: Scalar[]; dir: number }[] = [];
  let byCol: boolean | undefined;
  for (let i = 1; i < args.length; i += 2) {
    const by = arrayOf(args[i], ctx);
    if (isErr(by)) return by;
    let col: boolean;
    if (by.cols === 1 && by.rows === arr.rows) col = false;
    else if (by.rows === 1 && by.cols === arr.cols) col = true;
    else return Errors.VALUE;
    if (byCol === undefined) byCol = col;
    else if (byCol !== col) return Errors.VALUE;
    const o =
      args[i + 1] === undefined || isOmitted(args[i + 1]) ? 1 : toNumber(ctx.toScalar(force(args[i + 1])));
    if (isErr(o)) return o;
    if (o !== 1 && o !== -1) return Errors.VALUE;
    keys.push({ values: by.flat(), dir: o });
  }
  const n = byCol ? arr.cols : arr.rows;
  const order = Array.from({ length: n }, (_, i) => i);
  order.sort((a, b) => {
    for (const k of keys) {
      const x = k.values[a] as Scalar;
      const y = k.values[b] as Scalar;
      if (x === BLANK || y === BLANK) {
        if (x === y) continue;
        return x === BLANK ? 1 : -1;
      }
      const c = sortCompare(x, y);
      if (c !== 0) return c * k.dir;
    }
    return a - b;
  });
  if (byCol) return Array2D.build(arr.rows, arr.cols, (r, c) => arr.get(r, order[c] as number));
  return new Array2D(order.map((i) => arr.data[i] as Scalar[]));
}

function xmatchFn(args: Arg[], ctx: CallContext): Scalar {
  const x = args[0] as Scalar;
  if (x instanceof ErrorValue) return x;
  const mm = int(args[2], 0);
  if (isErr(mm)) return mm;
  const sm = int(args[3], 1);
  if (isErr(sm)) return sm;
  if (![0, -1, 1, 2].includes(mm) || ![1, -1, 2, -2].includes(sm)) return Errors.VALUE;
  const arr = arrayOf(args[1], ctx);
  if (isErr(arr)) return arr;
  if (arr.rows > 1 && arr.cols > 1) return Errors.VALUE;
  const values = arr.flat();
  const i = xmatch(values, x === BLANK ? 0 : x, mm, sm);
  return i < 0 ? Errors.NA : i + 1;
}

function hyperlink(args: Arg[], ctx: CallContext): Scalar {
  const link = str(args[0]);
  if (isErr(link)) return link;
  ctx.setHyperlink(link);
  if (args.length < 2 || args[1] === MISSING) return link;
  const v = args[1] as Scalar;
  return v === BLANK ? 0 : v;
}

export const referenceFunctions: FunctionDescriptor[] = [
  fn('ADDRESS', 2, 5, address, { lift: 'all' }),
  fn('AREAS', 1, 1, (args) => {
    const v = force(args[0]);
    if (v instanceof ErrorValue) return v;
    return v instanceof RefValue ? v.areas.length : Errors.VALUE;
  }),
  fn('CHOOSE', 2, 254, choose, { lazy: AFTER_FIRST, lift: [0] }),
  fn('CHOOSEROWS', 2, VARIADIC, (args, ctx) => chooseRowsCols(args, ctx, false)),
  fn('CHOOSECOLS', 2, VARIADIC, (args, ctx) => chooseRowsCols(args, ctx, true)),
  fn('ROWS', 1, 1, (args, ctx) => {
    const d = dims(force(args[0]), ctx);
    return isErr(d) ? d : d.rows;
  }),
  fn('COLUMNS', 1, 1, (args, ctx) => {
    const d = dims(force(args[0]), ctx);
    return isErr(d) ? d : d.cols;
  }),
  fn('TAKE', 2, 3, (args, ctx) => take(args, ctx, false)),
  fn('DROP', 2, 3, (args, ctx) => take(args, ctx, true)),
  fn('EXPAND', 2, 4, expand),
  fn('HSTACK', 1, VARIADIC, (args, ctx) => stack(args, ctx, true)),
  fn('VSTACK', 1, VARIADIC, (args, ctx) => stack(args, ctx, false)),
  fn('TOCOL', 1, 3, (args, ctx) => toVector(args, ctx, true)),
  fn('TOROW', 1, 3, (args, ctx) => toVector(args, ctx, false)),
  fn('WRAPROWS', 2, 3, (args, ctx) => wrap(args, ctx, true)),
  fn('WRAPCOLS', 2, 3, (args, ctx) => wrap(args, ctx, false)),
  fn('HLOOKUP', 3, 4, hlookup, { lift: [0, 2, 3] }),
  fn('LOOKUP', 2, 3, lookup, { lift: [0] }),
  fn('SORTBY', 2, VARIADIC, sortby),
  fn('XMATCH', 2, 4, xmatchFn, { lift: [0, 2, 3] }),
  fn('HYPERLINK', 1, 2, hyperlink, { lift: 'all' }),
];
