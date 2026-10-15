import { MAX_COLS, MAX_ROWS } from '../../../parser/references.ts';
import { toBoolean, toNumber, toText } from '../../../values/coerce.ts';
import { compareScalars, lookupEquals } from '../../../values/compare.ts';
import { Errors, ErrorValue } from '../../../values/errors.ts';
import {
  type Area,
  Array2D,
  BLANK,
  type EvalValue,
  MISSING,
  RefValue,
  type Scalar,
} from '../../../values/types.ts';
import type { CallContext } from '../../context.ts';
import { force, intArg, isOmitted } from '../helpers.ts';
import type { Arg, FunctionDescriptor } from '../registry.ts';

// ── matching primitives ─────────────────────────────────────────────────

const wildcardCache = new Map<string, RegExp>();

/** Excel wildcards: `*` any run, `?` one character, `~` escapes the next character. */
export function wildcardRegex(pattern: string): RegExp {
  let re = wildcardCache.get(pattern);
  if (re) return re;
  let src = '';
  for (let i = 0; i < pattern.length; i++) {
    const ch = pattern[i] as string;
    if (ch === '~' && i + 1 < pattern.length) {
      src += escapeRe(pattern[++i] as string);
    } else if (ch === '*') src += '[\\s\\S]*';
    else if (ch === '?') src += '[\\s\\S]';
    else src += escapeRe(ch);
  }
  re = new RegExp(`^${src}$`, 'i');
  if (wildcardCache.size > 500) wildcardCache.clear();
  wildcardCache.set(pattern, re);
  return re;
}

function escapeRe(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function hasWildcard(s: string): boolean {
  return /[*?~]/.test(s);
}

function sameType(a: Scalar, b: Scalar): boolean {
  return typeof a === typeof b && a !== BLANK && !(a instanceof ErrorValue);
}

/** Index of the first exact match (wildcards allowed for text when `wild`), or -1. */
export function findExact(values: readonly Scalar[], x: Scalar, wild: boolean, reverse = false): number {
  const re = wild && typeof x === 'string' && hasWildcard(x) ? wildcardRegex(x) : undefined;
  const n = values.length;
  for (let k = 0; k < n; k++) {
    const i = reverse ? n - 1 - k : k;
    const v = values[i] as Scalar;
    if (re) {
      if (typeof v === 'string' && re.test(v)) return i;
    } else if (lookupEquals(v, x)) return i;
  }
  return -1;
}

function cmp(a: Scalar, b: Scalar): number {
  const c = compareScalars(a, b);
  return typeof c === 'number' ? c : 0;
}

/** Binary search for the last value <= x (ascending data), ignoring values of another type. */
export function approxAscending(values: readonly Scalar[], x: Scalar): number {
  let lo = 0;
  let hi = values.length - 1;
  let ans = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    let m = mid;
    while (m >= lo && !sameType(values[m] as Scalar, x)) m--;
    if (m < lo) {
      lo = mid + 1;
      continue;
    }
    if (cmp(values[m] as Scalar, x) <= 0) {
      ans = m;
      lo = mid + 1;
    } else hi = m - 1;
  }
  return ans;
}

/** Binary search for the last value >= x in descending data. */
export function approxDescending(values: readonly Scalar[], x: Scalar): number {
  let lo = 0;
  let hi = values.length - 1;
  let ans = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    let m = mid;
    while (m >= lo && !sameType(values[m] as Scalar, x)) m--;
    if (m < lo) {
      lo = mid + 1;
      continue;
    }
    if (cmp(values[m] as Scalar, x) >= 0) {
      ans = m;
      lo = mid + 1;
    } else hi = m - 1;
  }
  return ans;
}

/** XMATCH/XLOOKUP matching. Returns a 0-based index or -1. */
export function xmatch(values: readonly Scalar[], x: Scalar, matchMode: number, searchMode: number): number {
  if (searchMode === 2 || searchMode === -2) {
    const asc = searchMode === 2;
    let lo = 0;
    let hi = values.length - 1;
    let exact = -1;
    let below = -1; // largest < x
    let above = -1; // smallest > x
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      const c = cmp(values[mid] as Scalar, x);
      if (c === 0 && sameType(values[mid] as Scalar, x)) {
        exact = mid;
        if (asc) hi = mid - 1;
        else hi = mid - 1;
        continue;
      }
      if (c < 0) {
        below = mid;
        if (asc) lo = mid + 1;
        else hi = mid - 1;
      } else {
        above = mid;
        if (asc) hi = mid - 1;
        else lo = mid + 1;
      }
    }
    if (exact >= 0) return exact;
    if (matchMode === -1) return below;
    if (matchMode === 1) return above;
    return -1;
  }
  const reverse = searchMode === -1;
  const exact = findExact(values, x, matchMode === 2, reverse);
  if (exact >= 0 || matchMode === 0 || matchMode === 2) return exact;
  let best = -1;
  const n = values.length;
  for (let k = 0; k < n; k++) {
    const i = reverse ? n - 1 - k : k;
    const v = values[i] as Scalar;
    if (!sameType(v, x)) continue;
    const c = cmp(v, x);
    if (matchMode === -1 && c < 0 && (best < 0 || cmp(v, values[best] as Scalar) > 0)) best = i;
    if (matchMode === 1 && c > 0 && (best < 0 || cmp(v, values[best] as Scalar) < 0)) best = i;
  }
  return best;
}

// ── helpers ─────────────────────────────────────────────────────────────

function lookupKey(v: Scalar): Scalar {
  return v === BLANK ? 0 : v;
}

/** A one-dimensional list of values from a range/array, plus its orientation. */
function vector(v: EvalValue, ctx: CallContext): { values: Scalar[]; vertical: boolean } | ErrorValue {
  const arr = ctx.toArray(v);
  if (arr instanceof ErrorValue) return arr;
  if (arr.cols === 1) return { values: arr.data.map((r) => r[0] as Scalar), vertical: true };
  if (arr.rows === 1) return { values: arr.data[0] as Scalar[], vertical: false };
  return Errors.NA;
}

function subArea(a: Area, r0: number, c0: number, r1: number, c1: number): RefValue {
  return RefValue.area({ sheet: a.sheet, r0: a.r0 + r0, c0: a.c0 + c0, r1: a.r0 + r1, c1: a.c0 + c1 });
}

function sliceArray(arr: Array2D, r0: number, c0: number, r1: number, c1: number): EvalValue {
  if (r0 === r1 && c0 === c1) return arr.get(r0, c0);
  return Array2D.build(r1 - r0 + 1, c1 - c0 + 1, (r, c) => arr.get(r0 + r, c0 + c));
}

function num(v: Arg | undefined, dflt: number): number | ErrorValue {
  if (v === undefined || v === MISSING) return dflt;
  const n = toNumber(v as Scalar);
  return n instanceof ErrorValue ? n : Math.trunc(n);
}

// ── implementations ─────────────────────────────────────────────────────

function vlookup(args: Arg[], ctx: CallContext): EvalValue {
  const x = args[0] as Scalar;
  if (x instanceof ErrorValue) return x;
  const col = num(args[2], 1);
  if (col instanceof ErrorValue) return col;
  let approx = true;
  if (args.length > 3) {
    const b = args[3] === MISSING ? false : toBoolean(args[3] as Scalar);
    if (b instanceof ErrorValue) return b;
    approx = b;
  }
  if (col < 1) return Errors.VALUE;
  const table = force(args[1]);
  let firstCol: Scalar[];
  let width: number;
  let read: (row: number) => Scalar;
  if (table instanceof RefValue) {
    const a0 = table.single;
    if (!a0) return Errors.REF;
    const a = ctx.clampArea(a0);
    width = a0.c1 - a0.c0 + 1;
    if (col > width) return Errors.REF;
    firstCol = [];
    for (let r = a.r0; r <= a.r1; r++) firstCol.push(ctx.readCell(a.sheet, r, a.c0));
    read = (i) => ctx.readCell(a.sheet, a.r0 + i, a.c0 + col - 1);
  } else {
    const arr = ctx.toArray(table);
    if (arr instanceof ErrorValue) return arr;
    width = arr.cols;
    if (col > width) return Errors.REF;
    firstCol = arr.data.map((r) => r[0] as Scalar);
    read = (i) => arr.get(i, col - 1);
  }
  const key = lookupKey(x);
  const i = approx ? approxAscending(firstCol, key) : findExact(firstCol, key, true);
  return i < 0 ? Errors.NA : read(i);
}

function match(args: Arg[], ctx: CallContext): EvalValue {
  const x = args[0] as Scalar;
  if (x instanceof ErrorValue) return x;
  const type = num(args[2], args.length > 2 ? 0 : 1);
  if (type instanceof ErrorValue) return type;
  const vec = vector(force(args[1]), ctx);
  if (vec instanceof ErrorValue) return vec;
  const key = lookupKey(x);
  const i =
    type === 0
      ? findExact(vec.values, key, true)
      : type > 0
        ? approxAscending(vec.values, key)
        : approxDescending(vec.values, key);
  return i < 0 ? Errors.NA : i + 1;
}

function xlookup(args: Arg[], ctx: CallContext): EvalValue {
  const x = args[0] as Scalar;
  if (x instanceof ErrorValue) return x;
  const matchMode = num(args[4], 0);
  if (matchMode instanceof ErrorValue) return matchMode;
  const searchMode = num(args[5], 1);
  if (searchMode instanceof ErrorValue) return searchMode;
  if (![0, -1, 1, 2].includes(matchMode) || ![1, -1, 2, -2].includes(searchMode)) return Errors.VALUE;
  const vec = vector(force(args[1]), ctx);
  if (vec instanceof ErrorValue) return Errors.VALUE;
  const ret = force(args[2]);
  const n = vec.values.length;
  let retRows: number;
  let retCols: number;
  let retArr: Array2D | undefined;
  let retArea: Area | undefined;
  if (ret instanceof RefValue) {
    if (!ret.single) return Errors.VALUE;
    retArea = ret.single;
    retRows = retArea.r1 - retArea.r0 + 1;
    retCols = retArea.c1 - retArea.c0 + 1;
  } else {
    const a = ctx.toArray(ret);
    if (a instanceof ErrorValue) return a;
    retArr = a;
    retRows = a.rows;
    retCols = a.cols;
  }
  const vertical = vec.vertical;
  if (vertical ? retRows !== n : retCols !== n) return Errors.VALUE;
  const i = xmatch(vec.values, lookupKey(x), matchMode, searchMode);
  if (i < 0) {
    if (args.length > 3 && !isOmitted(args[3])) return force(args[3]);
    return Errors.NA;
  }
  if (retArea) {
    return vertical ? subArea(retArea, i, 0, i, retCols - 1) : subArea(retArea, 0, i, retRows - 1, i);
  }
  const arr = retArr as Array2D;
  return vertical ? sliceArray(arr, i, 0, i, retCols - 1) : sliceArray(arr, 0, i, retRows - 1, i);
}

function index(args: Arg[], ctx: CallContext): EvalValue {
  const src = force(args[0]);
  if (src instanceof ErrorValue) return src;
  const rowArg = ctx.toValue(force(args[1]));
  const colArg = args.length > 2 ? ctx.toValue(force(args[2])) : undefined;
  if (rowArg instanceof Array2D || colArg instanceof Array2D) {
    const parts = [rowArg, colArg ?? 0] as (Scalar | Array2D)[];
    const rows = Math.max(...parts.map((p) => (p instanceof Array2D ? p.rows : 1)));
    const cols = Math.max(...parts.map((p) => (p instanceof Array2D ? p.cols : 1)));
    return Array2D.build(rows, cols, (r, c) => {
      const pickOne = (p: Scalar | Array2D) =>
        p instanceof Array2D ? p.get(Math.min(r, p.rows - 1), Math.min(c, p.cols - 1)) : p;
      const one = indexOne(
        src,
        pickOne(rowArg as Scalar | Array2D),
        colArg === undefined ? undefined : pickOne(colArg as Scalar | Array2D),
        args[3],
        ctx,
      );
      return ctx.toScalar(one);
    });
  }
  return indexOne(src, rowArg as Scalar, colArg as Scalar | undefined, args[3], ctx);
}

function indexOne(
  src: EvalValue,
  rowV: Scalar,
  colV: Scalar | undefined,
  areaArg: Arg | undefined,
  ctx: CallContext,
): EvalValue {
  const toInt = (v: Scalar): number | ErrorValue => {
    const n = toNumber(v);
    return n instanceof ErrorValue ? n : Math.trunc(n);
  };
  let r = toInt(rowV === MISSING ? 0 : rowV);
  if (r instanceof ErrorValue) return r;
  let c: number | ErrorValue | undefined = colV === undefined ? undefined : toInt(colV);
  if (c instanceof ErrorValue) return c;
  let area: Area | undefined;
  let arr: Array2D | undefined;
  if (src instanceof RefValue) {
    let k = 1;
    if (areaArg !== undefined && !isOmitted(areaArg)) {
      const kk = intArg(areaArg, ctx);
      if (kk instanceof ErrorValue) return kk;
      k = kk;
    }
    if (k < 1 || k > src.areas.length) return Errors.REF;
    area = src.areas[k - 1] as Area;
  } else {
    const a = ctx.toArray(src);
    if (a instanceof ErrorValue) return a;
    arr = a;
  }
  const rows = area ? area.r1 - area.r0 + 1 : (arr as Array2D).rows;
  const cols = area ? area.c1 - area.c0 + 1 : (arr as Array2D).cols;
  if (c === undefined) {
    if (rows === 1 && cols > 1) {
      c = r;
      r = 1;
    } else c = cols === 1 ? 1 : 0;
  }
  if (r < 0 || c < 0) return Errors.VALUE;
  if (r > rows || c > cols) return Errors.REF;
  const [r0, r1] = r === 0 ? [0, rows - 1] : [r - 1, r - 1];
  const [c0, c1] = c === 0 ? [0, cols - 1] : [c - 1, c - 1];
  if (area) return subArea(area, r0, c0, r1, c1);
  return sliceArray(arr as Array2D, r0, c0, r1, c1);
}

function rowOrColumn(args: Arg[], ctx: CallContext, isRow: boolean): EvalValue {
  if (args.length === 0 || isOmitted(args[0])) return (isRow ? ctx.row : ctx.col) + 1;
  const ref = force(args[0]);
  if (ref instanceof ErrorValue) return ref;
  if (!(ref instanceof RefValue) || !ref.single) return Errors.VALUE;
  const a = ctx.clampArea(ref.single);
  const lo = isRow ? a.r0 : a.c0;
  const hi = isRow ? a.r1 : a.c1;
  if (lo === hi || ctx.legacy) return lo + 1;
  const values: Scalar[] = [];
  for (let i = lo; i <= hi; i++) values.push(i + 1);
  return isRow ? Array2D.column(values) : Array2D.row(values);
}

function offset(args: Arg[], ctx: CallContext): EvalValue {
  const ref = force(args[0]);
  if (ref instanceof ErrorValue) return ref;
  if (!(ref instanceof RefValue)) return Errors.VALUE;
  const a = ref.single;
  if (!a) return Errors.VALUE;
  const dr = intArg(args[1], ctx);
  if (dr instanceof ErrorValue) return dr;
  const dc = intArg(args[2], ctx);
  if (dc instanceof ErrorValue) return dc;
  const h = isOmitted(args[3]) ? a.r1 - a.r0 + 1 : intArg(args[3], ctx);
  if (h instanceof ErrorValue) return h;
  const w = isOmitted(args[4]) ? a.c1 - a.c0 + 1 : intArg(args[4], ctx);
  if (w instanceof ErrorValue) return w;
  if (h === 0 || w === 0) return Errors.REF;
  let r0 = a.r0 + dr;
  let c0 = a.c0 + dc;
  let r1 = r0 + h - 1;
  let c1 = c0 + w - 1;
  if (h < 0) [r0, r1] = [r0 + h + 1, r0];
  if (w < 0) [c0, c1] = [c0 + w + 1, c0];
  if (r0 < 0 || c0 < 0 || r1 >= MAX_ROWS || c1 >= MAX_COLS) return Errors.REF;
  return RefValue.area({ sheet: a.sheet, r0, c0, r1, c1 });
}

function indirect(args: Arg[], ctx: CallContext): EvalValue {
  const t = toText(ctx.toScalar(force(args[0])));
  if (t instanceof ErrorValue) return t;
  let a1 = true;
  if (args.length > 1 && !isOmitted(args[1])) {
    const b = toBoolean(ctx.toScalar(force(args[1])));
    if (b instanceof ErrorValue) return b;
    a1 = b;
  }
  return ctx.resolveReferenceText(t, a1);
}

export const lookupFunctions: FunctionDescriptor[] = [
  {
    name: 'VLOOKUP',
    minArgs: 3,
    maxArgs: 4,
    lift: [0, 2, 3],
    impl: vlookup,
    docs: {
      category: 'Lookup & Reference',
      summary: 'Looks for a value in the first column of a table and returns a value from the same row.',
      syntax: 'VLOOKUP(lookup_value, table_array, col_index_num, [range_lookup])',
      args: [
        { name: 'lookup_value', description: 'The value to find in the first column.' },
        { name: 'table_array', description: 'The table to search.' },
        { name: 'col_index_num', description: 'Column of the table to return (1 is the first).' },
        {
          name: 'range_lookup',
          description: 'FALSE for an exact match; TRUE or omitted for the closest match in sorted data.',
          optional: true,
        },
      ],
      examples: [{ formula: '=VLOOKUP(2,{1,"a";2,"b"},2,FALSE)', result: '"b"' }],
    },
  },
  {
    name: 'XLOOKUP',
    minArgs: 3,
    maxArgs: 6,
    lift: [0, 4, 5],
    impl: xlookup,
    docs: {
      category: 'Lookup & Reference',
      summary: 'Searches a range or array and returns the matching item from another range or array.',
      syntax:
        'XLOOKUP(lookup_value, lookup_array, return_array, [if_not_found], [match_mode], [search_mode])',
      args: [
        { name: 'lookup_value', description: 'The value to search for.' },
        { name: 'lookup_array', description: 'The row or column to search.' },
        { name: 'return_array', description: 'The range or array to return from.' },
        {
          name: 'if_not_found',
          description: 'Returned when nothing matches (default #N/A).',
          optional: true,
        },
        {
          name: 'match_mode',
          description: '0 exact (default), -1 exact or next smaller, 1 exact or next larger, 2 wildcard.',
          optional: true,
        },
        {
          name: 'search_mode',
          description:
            '1 first to last (default), -1 last to first, 2 binary ascending, -2 binary descending.',
          optional: true,
        },
      ],
      examples: [{ formula: '=XLOOKUP(5,{1,3,5},{"a","b","c"})', result: '"c"' }],
    },
  },
  {
    name: 'MATCH',
    minArgs: 2,
    maxArgs: 3,
    lift: [0, 2],
    impl: match,
    docs: {
      category: 'Lookup & Reference',
      summary: 'Returns the relative position of a value in a row or column.',
      syntax: 'MATCH(lookup_value, lookup_array, [match_type])',
      args: [
        { name: 'lookup_value', description: 'The value to find.' },
        { name: 'lookup_array', description: 'The row or column to search.' },
        {
          name: 'match_type',
          description:
            '1 (default) largest value ≤ lookup_value in ascending data, 0 exact, -1 smallest value ≥ in descending data.',
          optional: true,
        },
      ],
      examples: [{ formula: '=MATCH("b",{"a","b","c"},0)', result: '2' }],
    },
  },
  {
    name: 'INDEX',
    minArgs: 2,
    maxArgs: 4,
    impl: index,
    docs: {
      category: 'Lookup & Reference',
      summary: 'Returns the value or reference at a row and column of a range or array.',
      syntax: 'INDEX(array, row_num, [column_num], [area_num])',
      args: [
        { name: 'array', description: 'A range or array.' },
        { name: 'row_num', description: 'Row to return (0 for the whole column).' },
        { name: 'column_num', description: 'Column to return (0 for the whole row).', optional: true },
        { name: 'area_num', description: 'Which area of a multi-area reference.', optional: true },
      ],
      examples: [{ formula: '=INDEX({1,2;3,4},2,1)', result: '3' }],
    },
  },
  {
    name: 'ROW',
    minArgs: 0,
    maxArgs: 1,
    impl: (args, ctx) => rowOrColumn(args, ctx, true),
    docs: {
      category: 'Lookup & Reference',
      summary: 'Returns the row number of a reference (or of the formula cell).',
      syntax: 'ROW([reference])',
      args: [{ name: 'reference', description: 'A cell or range.', optional: true }],
      examples: [{ formula: '=ROW(C7)', result: '7' }],
    },
  },
  {
    name: 'COLUMN',
    minArgs: 0,
    maxArgs: 1,
    impl: (args, ctx) => rowOrColumn(args, ctx, false),
    docs: {
      category: 'Lookup & Reference',
      summary: 'Returns the column number of a reference (or of the formula cell).',
      syntax: 'COLUMN([reference])',
      args: [{ name: 'reference', description: 'A cell or range.', optional: true }],
      examples: [{ formula: '=COLUMN(C7)', result: '3' }],
    },
  },
  {
    name: 'OFFSET',
    minArgs: 3,
    maxArgs: 5,
    volatile: true,
    impl: offset,
    docs: {
      category: 'Lookup & Reference',
      summary: 'Returns a reference offset from a starting reference by rows and columns.',
      syntax: 'OFFSET(reference, rows, cols, [height], [width])',
      args: [
        { name: 'reference', description: 'The starting reference.' },
        { name: 'rows', description: 'Rows to move down (negative moves up).' },
        { name: 'cols', description: 'Columns to move right (negative moves left).' },
        { name: 'height', description: 'Height of the result (default: same as reference).', optional: true },
        { name: 'width', description: 'Width of the result (default: same as reference).', optional: true },
      ],
      examples: [{ formula: '=ROW(OFFSET(A1,2,0))', result: '3' }],
    },
  },
  {
    name: 'INDIRECT',
    minArgs: 1,
    maxArgs: 2,
    volatile: true,
    impl: indirect,
    docs: {
      category: 'Lookup & Reference',
      summary: 'Returns the reference named by a text string.',
      syntax: 'INDIRECT(ref_text, [a1])',
      args: [
        { name: 'ref_text', description: 'Text such as "B2", "Sheet2!A1:A10" or a defined name.' },
        { name: 'a1', description: 'TRUE or omitted for A1 style, FALSE for R1C1.', optional: true },
      ],
      examples: [{ formula: '=ROW(INDIRECT("C5"))', result: '5' }],
    },
  },
];
