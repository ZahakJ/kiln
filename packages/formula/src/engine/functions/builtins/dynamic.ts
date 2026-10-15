import { toBoolean, toNumber } from '../../../values/coerce.ts';
import { lookupEquals, sortCompare } from '../../../values/compare.ts';
import { Errors, ErrorValue } from '../../../values/errors.ts';
import { Array2D, BLANK, type EvalValue, Lambda, type Scalar } from '../../../values/types.ts';
import type { CallContext } from '../../context.ts';
import { boolArg, force, intArg, isOmitted, numberArg } from '../helpers.ts';
import type { Arg, FunctionDescriptor } from '../registry.ts';

const MAX_CELLS = 1_048_576 * 16;

function calcEmpty(): ErrorValue {
  return new ErrorValue('#CALC!', 'The result is an empty array');
}

/** Numbers from a scalar-or-array argument (SORT's sort_index / sort_order). */
function numberList(a: Arg | undefined, ctx: CallContext, dflt: number): number[] | ErrorValue {
  if (a === undefined || isOmitted(a)) return [dflt];
  const v = ctx.toValue(force(a));
  if (v instanceof Lambda) return Errors.VALUE;
  const items = v instanceof Array2D ? v.flat() : [v];
  const out: number[] = [];
  for (const x of items) {
    const n = toNumber(x);
    if (n instanceof ErrorValue) return n;
    out.push(Math.trunc(n));
  }
  return out;
}

function sequence(args: Arg[], ctx: CallContext): EvalValue {
  const rows = intArg(args[0], ctx);
  if (rows instanceof ErrorValue) return rows;
  const cols = isOmitted(args[1]) ? 1 : intArg(args[1], ctx);
  if (cols instanceof ErrorValue) return cols;
  const start = isOmitted(args[2]) ? 1 : numberArg(args[2], ctx);
  if (start instanceof ErrorValue) return start;
  const step = isOmitted(args[3]) ? 1 : numberArg(args[3], ctx);
  if (step instanceof ErrorValue) return step;
  if (rows < 0 || cols < 0) return Errors.VALUE;
  if (rows === 0 || cols === 0) return calcEmpty();
  if (rows * cols > MAX_CELLS) return Errors.NUM;
  return Array2D.build(rows, cols, (r, c) => start + (r * cols + c) * step);
}

function filter(args: Arg[], ctx: CallContext): EvalValue {
  const arr = ctx.toArray(force(args[0]));
  if (arr instanceof ErrorValue) return arr;
  const inc = ctx.toArray(force(args[1]));
  if (inc instanceof ErrorValue) return inc;
  let byRows: boolean;
  if (inc.cols === 1 && inc.rows === arr.rows) byRows = true;
  else if (inc.rows === 1 && inc.cols === arr.cols) byRows = false;
  else return Errors.VALUE;
  const keep: boolean[] = [];
  for (const v of inc.flat()) {
    if (v instanceof ErrorValue) return v;
    const b = v === BLANK ? false : toBoolean(v);
    if (b instanceof ErrorValue) return b;
    keep.push(b);
  }
  if (!keep.some(Boolean)) {
    if (args.length > 2 && !isOmitted(args[2])) return force(args[2]);
    return calcEmpty();
  }
  if (byRows) return new Array2D(arr.data.filter((_, i) => keep[i]));
  return new Array2D(arr.data.map((row) => row.filter((_, i) => keep[i])));
}

function sort(args: Arg[], ctx: CallContext): EvalValue {
  const arr = ctx.toArray(force(args[0]));
  if (arr instanceof ErrorValue) return arr;
  const byCol = boolArg(isOmitted(args[3]) ? undefined : args[3], ctx, false);
  if (byCol instanceof ErrorValue) return byCol;
  const keys = numberList(args[1], ctx, 1);
  if (keys instanceof ErrorValue) return keys;
  const orders = numberList(args[2], ctx, 1);
  if (orders instanceof ErrorValue) return orders;
  const src = byCol ? arr.transpose() : arr;
  if (keys.some((k) => k < 1 || k > src.cols)) return Errors.VALUE;
  if (orders.some((o) => o !== 1 && o !== -1)) return Errors.VALUE;
  const rows = [...src.data];
  rows.sort((a, b) => {
    for (let i = 0; i < keys.length; i++) {
      const k = (keys[i] as number) - 1;
      const dir = (orders[Math.min(i, orders.length - 1)] as number) ?? 1;
      const x = a[k] as Scalar;
      const y = b[k] as Scalar;
      if (x === BLANK || y === BLANK) {
        if (x === y) continue;
        return x === BLANK ? 1 : -1; // blanks last in both directions
      }
      const c = sortCompare(x, y);
      if (c !== 0) return c * dir;
    }
    return 0;
  });
  const out = new Array2D(rows);
  return byCol ? out.transpose() : out;
}

function unique(args: Arg[], ctx: CallContext): EvalValue {
  const arr = ctx.toArray(force(args[0]));
  if (arr instanceof ErrorValue) return arr;
  const byCol = boolArg(isOmitted(args[1]) ? undefined : args[1], ctx, false);
  if (byCol instanceof ErrorValue) return byCol;
  const once = boolArg(isOmitted(args[2]) ? undefined : args[2], ctx, false);
  if (once instanceof ErrorValue) return once;
  const src = byCol ? arr.transpose() : arr;
  const groups: { row: Scalar[]; count: number }[] = [];
  const index = new Map<string, number[]>();
  const keyOf = (row: Scalar[]) =>
    row
      .map((v) =>
        typeof v === 'string'
          ? `s${v.toLowerCase()}`
          : v instanceof ErrorValue
            ? `e${v.code}`
            : `${typeof v}${String(v)}`,
      )
      .join('\u0001');
  for (const row of src.data) {
    const k = keyOf(row);
    const cands = index.get(k);
    const hit = cands?.find((g) =>
      (groups[g] as { row: Scalar[] }).row.every((v, i) => lookupEquals(v, row[i] as Scalar)),
    );
    if (hit !== undefined) (groups[hit] as { count: number }).count++;
    else {
      groups.push({ row, count: 1 });
      if (cands) cands.push(groups.length - 1);
      else index.set(k, [groups.length - 1]);
    }
  }
  const rows = groups.filter((g) => !once || g.count === 1).map((g) => g.row);
  if (rows.length === 0) return calcEmpty();
  const out = new Array2D(rows);
  return byCol ? out.transpose() : out;
}

export const dynamicFunctions: FunctionDescriptor[] = [
  {
    name: 'SEQUENCE',
    minArgs: 1,
    maxArgs: 4,
    impl: sequence,
    docs: {
      category: 'Dynamic Array',
      summary: 'Generates a list of sequential numbers in an array.',
      syntax: 'SEQUENCE(rows, [columns], [start], [step])',
      args: [
        { name: 'rows', description: 'Number of rows.' },
        { name: 'columns', description: 'Number of columns (default 1).', optional: true },
        { name: 'start', description: 'First number (default 1).', optional: true },
        { name: 'step', description: 'Increment (default 1).', optional: true },
      ],
      examples: [{ formula: '=SUM(SEQUENCE(4))', result: '10' }],
    },
  },
  {
    name: 'FILTER',
    minArgs: 2,
    maxArgs: 3,
    impl: filter,
    docs: {
      category: 'Dynamic Array',
      summary: 'Filters a range or array by a condition.',
      syntax: 'FILTER(array, include, [if_empty])',
      args: [
        { name: 'array', description: 'The range or array to filter.' },
        { name: 'include', description: 'A row or column of TRUE/FALSE values, one per row (or column).' },
        { name: 'if_empty', description: 'Returned when nothing passes the filter.', optional: true },
      ],
      examples: [{ formula: '=SUM(FILTER({1;2;3},{1;2;3}>1))', result: '5' }],
    },
  },
  {
    name: 'SORT',
    minArgs: 1,
    maxArgs: 4,
    impl: sort,
    docs: {
      category: 'Dynamic Array',
      summary: 'Sorts the contents of a range or array.',
      syntax: 'SORT(array, [sort_index], [sort_order], [by_col])',
      args: [
        { name: 'array', description: 'The range or array to sort.' },
        { name: 'sort_index', description: 'Column (or row) to sort by (default 1).', optional: true },
        { name: 'sort_order', description: '1 ascending (default), -1 descending.', optional: true },
        { name: 'by_col', description: 'TRUE to sort columns instead of rows.', optional: true },
      ],
      examples: [{ formula: '=INDEX(SORT({3;1;2}),1)', result: '1' }],
    },
  },
  {
    name: 'UNIQUE',
    minArgs: 1,
    maxArgs: 3,
    impl: unique,
    docs: {
      category: 'Dynamic Array',
      summary: 'Returns the unique rows (or columns) of a range or array.',
      syntax: 'UNIQUE(array, [by_col], [exactly_once])',
      args: [
        { name: 'array', description: 'The range or array.' },
        { name: 'by_col', description: 'TRUE to compare columns instead of rows.', optional: true },
        { name: 'exactly_once', description: 'TRUE to return only items that occur once.', optional: true },
      ],
      examples: [{ formula: '=COUNTA(UNIQUE({1;2;2;3}))', result: '3' }],
    },
  },
  {
    name: 'TRANSPOSE',
    minArgs: 1,
    maxArgs: 1,
    impl: (args, ctx) => {
      const v = ctx.toValue(force(args[0]));
      if (v instanceof Array2D) return v.transpose();
      return v;
    },
    docs: {
      category: 'Lookup & Reference',
      summary: 'Swaps the rows and columns of a range or array.',
      syntax: 'TRANSPOSE(array)',
      args: [{ name: 'array', description: 'The range or array to transpose.' }],
      examples: [{ formula: '=INDEX(TRANSPOSE({1,2,3}),3,1)', result: '3' }],
    },
  },
];
