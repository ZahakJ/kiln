/**
 * Conditional aggregates: COUNTIF(S), SUMIF(S), AVERAGEIF(S), MAXIFS, MINIFS, COUNTBLANK.
 *
 * Criteria arguments are lifted, so `COUNTIF(A:A,{"a","b"})` returns an array of counts as in Excel.
 */
import type { CallContext } from '../engine/context.ts';
import type { Arg, FunctionDescriptor } from '../engine/functions/registry.ts';
import { Errors, ErrorValue } from '../values/errors.ts';
import { BLANK, MISSING, type Scalar } from '../values/types.ts';
import { type Predicate, parseCriterion } from './criteria.ts';
import { type Grid, gridOf, resizedGrid } from './grid.ts';
import { fn, sum } from './lib.ts';

function criterionOf(a: Arg | undefined, ctx: CallContext): Predicate | ErrorValue {
  const c = a === undefined || a === MISSING ? BLANK : (a as Scalar);
  if (c instanceof ErrorValue) return (v: Scalar) => v instanceof ErrorValue && v.code === c.code;
  return parseCriterion(c === '' ? '' : c, ctx.date1904);
}

/**
 * Visits positions (r, c) where every (grid, predicate) pair matches. Positions beyond every grid's
 * used area hold blanks; they are not visited, and the number of them that match is returned.
 */
function matches(
  pairs: { grid: Grid; pred: Predicate }[],
  visit: (r: number, c: number) => void,
  target?: Grid,
): number | ErrorValue {
  const first = (pairs[0] as { grid: Grid }).grid;
  for (const p of pairs) if (p.grid.rows !== first.rows || p.grid.cols !== first.cols) return Errors.VALUE;
  // Positions outside every grid's used area are blank everywhere: test a blank once for those.
  let rows = target?.usedRows ?? 0;
  let cols = target?.usedCols ?? 0;
  for (const p of pairs) {
    rows = Math.max(rows, p.grid.usedRows);
    cols = Math.max(cols, p.grid.usedCols);
  }
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      let ok = true;
      for (const p of pairs) {
        if (!p.pred(p.grid.get(r, c))) {
          ok = false;
          break;
        }
      }
      if (ok) visit(r, c);
    }
  }
  const outside = first.rows * first.cols - rows * cols;
  return outside > 0 && pairs.every((p) => p.pred(BLANK)) ? outside : 0;
}

function countIfs(args: Arg[], ctx: CallContext, start: number): Scalar {
  const pairs: { grid: Grid; pred: Predicate }[] = [];
  for (let i = start; i < args.length; i += 2) {
    const g = gridOf(args[i], ctx);
    if (g instanceof ErrorValue) return g;
    const p = criterionOf(args[i + 1], ctx);
    if (p instanceof ErrorValue) return p;
    pairs.push({ grid: g, pred: p });
  }
  let n = 0;
  const outside = matches(pairs, () => void n++);
  return outside instanceof ErrorValue ? outside : n + outside;
}

type Fold = (values: number[]) => Scalar;

/** Aggregates numbers of `target` at positions matching all criteria (SUMIFS, AVERAGEIFS…). */
function foldIfs(target: Grid, pairs: { grid: Grid; pred: Predicate }[], fold: Fold): Scalar {
  if (pairs.some((p) => p.grid.rows !== target.rows || p.grid.cols !== target.cols)) return Errors.VALUE;
  const values: number[] = [];
  let err: ErrorValue | undefined;
  const e = matches(
    pairs,
    (r, c) => {
      const v = target.get(r, c);
      if (typeof v === 'number') values.push(v);
      else if (v instanceof ErrorValue && !err) err = v;
    },
    target,
  );
  if (e instanceof ErrorValue) return e;
  if (err) return err;
  return fold(values);
}

const sumFold: Fold = (xs) => sum(xs);
const avgFold: Fold = (xs) => (xs.length === 0 ? Errors.DIV0 : sum(xs) / xs.length);
const maxFold: Fold = (xs) => (xs.length === 0 ? 0 : Math.max(...xs));
const minFold: Fold = (xs) => (xs.length === 0 ? 0 : Math.min(...xs));

function ifSingle(args: Arg[], ctx: CallContext, fold: Fold): Scalar {
  const range = gridOf(args[0], ctx);
  if (range instanceof ErrorValue) return range;
  const pred = criterionOf(args[1], ctx);
  if (pred instanceof ErrorValue) return pred;
  const target = args.length > 2 && args[2] !== MISSING ? resizedGrid(args[2], range, ctx) : range;
  if (target instanceof ErrorValue) return target;
  return foldIfs(target, [{ grid: range, pred }], fold);
}

function ifMulti(args: Arg[], ctx: CallContext, fold: Fold): Scalar {
  if ((args.length - 1) % 2 !== 0) return Errors.VALUE;
  const target = gridOf(args[0], ctx);
  if (target instanceof ErrorValue) return target;
  const pairs: { grid: Grid; pred: Predicate }[] = [];
  for (let i = 1; i < args.length; i += 2) {
    const g = gridOf(args[i], ctx);
    if (g instanceof ErrorValue) return g;
    const p = criterionOf(args[i + 1], ctx);
    if (p instanceof ErrorValue) return p;
    pairs.push({ grid: g, pred: p });
  }
  return foldIfs(target, pairs, fold);
}

/** Odd indices from `start` (criteria positions) up to 255 arguments. */
function criteriaPositions(start: number): number[] {
  const out: number[] = [];
  for (let i = start; i < 255; i += 2) out.push(i);
  return out;
}

export const conditionalFunctions: FunctionDescriptor[] = [
  fn('COUNTIF', 2, 2, (args, ctx) => countIfs(args, ctx, 0), { lift: [1] }),
  fn('COUNTIFS', 2, 254, (args, ctx) => (args.length % 2 !== 0 ? Errors.VALUE : countIfs(args, ctx, 0)), {
    lift: criteriaPositions(1),
  }),
  fn('SUMIF', 2, 3, (args, ctx) => ifSingle(args, ctx, sumFold), { lift: [1] }),
  fn('SUMIFS', 3, 255, (args, ctx) => ifMulti(args, ctx, sumFold), { lift: criteriaPositions(2) }),
  fn('AVERAGEIF', 2, 3, (args, ctx) => ifSingle(args, ctx, avgFold), { lift: [1] }),
  fn('AVERAGEIFS', 3, 255, (args, ctx) => ifMulti(args, ctx, avgFold), { lift: criteriaPositions(2) }),
  fn('MAXIFS', 3, 255, (args, ctx) => ifMulti(args, ctx, maxFold), { lift: criteriaPositions(2) }),
  fn('MINIFS', 3, 255, (args, ctx) => ifMulti(args, ctx, minFold), { lift: criteriaPositions(2) }),
  fn('COUNTBLANK', 1, 1, (args, ctx) => {
    const g = gridOf(args[0], ctx);
    if (g instanceof ErrorValue) return g;
    let n = g.rows * g.cols - g.usedRows * g.usedCols;
    for (let r = 0; r < g.usedRows; r++)
      for (let c = 0; c < g.usedCols; c++) {
        const v = g.get(r, c);
        if (v === BLANK || v === '') n++;
      }
    return n;
  }),
];
