/**
 * Resolution of structured references (`Table1[[#Headers],[Qty]]`, `[@Qty]`) to areas.
 */
import { Errors, type ErrorValue } from '../values/errors.ts';
import type { Area } from '../values/types.ts';
import type { TableDef } from './host.ts';

export function resolveStructured(
  t: TableDef,
  specials: readonly string[],
  thisRow: boolean,
  columns: readonly string[] | null,
  at: { sheet: number; row: number },
): Area | ErrorValue {
  let c0 = t.c0;
  let c1 = t.c1;
  if (columns) {
    const idx = columns.map((name) => t.columns.findIndex((c) => c.toUpperCase() === name.toUpperCase()));
    if (idx.some((i) => i < 0)) return Errors.REF;
    const a = idx[0] as number;
    const b = (idx[1] ?? a) as number;
    c0 = t.c0 + Math.min(a, b);
    c1 = t.c0 + Math.max(a, b);
  }
  const header = t.headerRow !== false;
  const totals = t.totalsRow === true;
  const dataR0 = t.r0 + (header ? 1 : 0);
  const dataR1 = t.r1 - (totals ? 1 : 0);
  if (thisRow) {
    if (at.sheet !== t.sheet || at.row < dataR0 || at.row > dataR1) return Errors.VALUE;
    return { sheet: t.sheet, r0: at.row, r1: at.row, c0, c1 };
  }
  let r0 = Number.POSITIVE_INFINITY;
  let r1 = Number.NEGATIVE_INFINITY;
  const add = (a: number, b: number) => {
    r0 = Math.min(r0, a);
    r1 = Math.max(r1, b);
  };
  for (const s of specials.length === 0 ? ['#Data'] : specials) {
    if (s === '#All') add(t.r0, t.r1);
    else if (s === '#Data') add(dataR0, dataR1);
    else if (s === '#Headers') {
      if (!header) return Errors.REF;
      add(t.r0, t.r0);
    } else if (s === '#Totals') {
      if (!totals) return Errors.REF;
      add(t.r1, t.r1);
    }
  }
  if (r1 < r0) return Errors.REF;
  return { sheet: t.sheet, r0, r1, c0, c1 };
}
