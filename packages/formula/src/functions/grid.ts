/**
 * A uniform, lazily read 2-D view over a range argument (reference, array or scalar). References are
 * read cell by cell, only within the sheet's used extent, so whole-column criteria ranges (`A:A`) stay
 * cheap, while keeping the full declared shape for Excel's size checks.
 */
import type { CallContext } from '../engine/context.ts';
import { force } from '../engine/functions/helpers.ts';
import type { Arg } from '../engine/functions/registry.ts';
import { Errors, ErrorValue } from '../values/errors.ts';
import { type Area, Array2D, BLANK, Lambda, MISSING, RefValue, type Scalar } from '../values/types.ts';

export interface Grid {
  /** Declared shape. */
  rows: number;
  cols: number;
  /** Rows/columns that may hold data (≤ declared shape). */
  usedRows: number;
  usedCols: number;
  get(r: number, c: number): Scalar;
  /** The area for references (for hidden-row checks), undefined for arrays. */
  area?: Area;
}

export function gridOf(a: Arg | undefined, ctx: CallContext): Grid | ErrorValue {
  const v = force(a);
  if (v instanceof RefValue) {
    const area = v.single;
    if (!area) return Errors.VALUE;
    return refGrid(area, ctx);
  }
  if (v instanceof Lambda) return Errors.VALUE;
  if (v instanceof Array2D)
    return { rows: v.rows, cols: v.cols, usedRows: v.rows, usedCols: v.cols, get: (r, c) => v.get(r, c) };
  const s = v === MISSING ? BLANK : v;
  return { rows: 1, cols: 1, usedRows: 1, usedCols: 1, get: () => s as Scalar };
}

export function refGrid(area: Area, ctx: CallContext): Grid {
  const rows = area.r1 - area.r0 + 1;
  const cols = area.c1 - area.c0 + 1;
  const c = ctx.clampArea(area);
  return {
    rows,
    cols,
    usedRows: c.r1 - c.r0 + 1,
    usedCols: c.c1 - c.c0 + 1,
    area,
    get: (r, col) => ctx.readCell(area.sheet, area.r0 + r, area.c0 + col),
  };
}

/**
 * A grid with the shape of `like`, anchored at the top-left of `a` (SUMIF/AVERAGEIF resize their
 * sum range this way).
 */
export function resizedGrid(a: Arg | undefined, like: Grid, ctx: CallContext): Grid | ErrorValue {
  const v = force(a);
  if (v instanceof RefValue) {
    const s = v.single;
    if (!s) return Errors.VALUE;
    return refGrid(
      { sheet: s.sheet, r0: s.r0, c0: s.c0, r1: s.r0 + like.rows - 1, c1: s.c0 + like.cols - 1 },
      ctx,
    );
  }
  const g = gridOf(a, ctx);
  if (g instanceof ErrorValue) return g;
  return {
    rows: like.rows,
    cols: like.cols,
    usedRows: Math.min(like.rows, g.rows),
    usedCols: Math.min(like.cols, g.cols),
    get: (r, c) => (r < g.rows && c < g.cols ? g.get(r, c) : BLANK),
  };
}
