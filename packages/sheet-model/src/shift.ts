/**
 * Pure functions that adjust ranges and sheet metadata for structural edits (insert/delete rows or
 * columns). The rules mirror both Excel and the formula engine's reference shifting
 * (`shiftFormula`): inserting before or at the first index moves a range, inserting strictly inside
 * grows it, deleting shrinks it and a range whose every row/column is deleted disappears. Ranges that
 * span the whole axis (`A:A` for rows) are never changed.
 */
import { type CellRange, MAX_COLS, MAX_ROWS, ModelError } from './address.ts';
import type { AnchorPoint, AutoFilter, DrawingAnchor, Table } from './meta.ts';

export interface StructuralEdit {
  kind: 'insertRows' | 'deleteRows' | 'insertCols' | 'deleteCols';
  at: number;
  count: number;
}

export function isRowEdit(e: StructuralEdit): boolean {
  return e.kind === 'insertRows' || e.kind === 'deleteRows';
}

export function isInsert(e: StructuralEdit): boolean {
  return e.kind === 'insertRows' || e.kind === 'insertCols';
}

function axisMax(e: StructuralEdit): number {
  return isRowEdit(e) ? MAX_ROWS : MAX_COLS;
}

/** Shifts a single index; null when it is deleted or pushed off the sheet. */
export function shiftIndex(i: number, e: StructuralEdit): number | null {
  if (i < e.at) return i;
  if (isInsert(e)) {
    const n = i + e.count;
    return n < axisMax(e) ? n : null;
  }
  if (i < e.at + e.count) return null;
  return i - e.count;
}

/** Shifts an inclusive span [lo, hi]; null when all of it is deleted (or pushed off the sheet). */
export function shiftSpan(lo: number, hi: number, e: StructuralEdit): [number, number] | null {
  const max = axisMax(e);
  if (lo === 0 && hi === max - 1) return [lo, hi];
  if (isInsert(e)) {
    const nlo = lo >= e.at ? lo + e.count : lo;
    if (nlo >= max) return null;
    const nhi = hi >= e.at ? Math.min(hi + e.count, max - 1) : hi;
    return [nlo, nhi];
  }
  const end = e.at + e.count;
  if (lo >= e.at && hi < end) return null;
  const nlo = lo < e.at ? lo : lo >= end ? lo - e.count : e.at;
  const nhi = hi >= end ? hi - e.count : hi >= e.at ? e.at - 1 : hi;
  if (nlo > nhi) return null;
  return [nlo, nhi];
}

export function shiftRange(r: CellRange, e: StructuralEdit): CellRange | null {
  if (isRowEdit(e)) {
    const s = shiftSpan(r.r0, r.r1, e);
    return s ? { r0: s[0], c0: r.c0, r1: s[1], c1: r.c1 } : null;
  }
  const s = shiftSpan(r.c0, r.c1, e);
  return s ? { r0: r.r0, c0: s[0], r1: r.r1, c1: s[1] } : null;
}

export function shiftRanges(list: readonly CellRange[], e: StructuralEdit): CellRange[] {
  const out: CellRange[] = [];
  for (const r of list) {
    const n = shiftRange(r, e);
    if (n) out.push(n);
  }
  return out;
}

/** Shifts a cell position; null when deleted. */
export function shiftCell(row: number, col: number, e: StructuralEdit): { row: number; col: number } | null {
  if (isRowEdit(e)) {
    const r = shiftIndex(row, e);
    return r === null ? null : { row: r, col };
  }
  const c = shiftIndex(col, e);
  return c === null ? null : { row, col: c };
}

/** Merged regions: shifted like ranges; a merge reduced to one cell is dropped. */
export function shiftMerges(list: readonly CellRange[], e: StructuralEdit): CellRange[] {
  return shiftRanges(list, e).filter((r) => r.r0 !== r.r1 || r.c0 !== r.c1);
}

/** A count of leading frozen rows/columns. */
export function shiftFrozenCount(n: number, e: StructuralEdit): number {
  if (n <= 0 || e.at >= n) return n;
  if (isInsert(e)) return Math.min(n + e.count, axisMax(e) - 1);
  const removed = Math.min(n, e.at + e.count) - e.at;
  return n - removed;
}

/** Page-break positions (a break before index i). */
export function shiftBreaks(list: readonly number[], e: StructuralEdit): number[] {
  const out: number[] = [];
  for (const i of list) {
    const n = shiftIndex(i, e);
    if (n !== null && n > 0) out.push(n);
  }
  return [...new Set(out)].sort((a, b) => a - b);
}

/** Unique default column names (`Column1`, `Column2`…) not already used. */
export function freshColumnNames(existing: readonly string[], count: number): string[] {
  const used = new Set(existing.map((n) => n.toUpperCase()));
  const out: string[] = [];
  let k = 1;
  while (out.length < count) {
    const name = `Column${k++}`;
    if (!used.has(name.toUpperCase())) {
      used.add(name.toUpperCase());
      out.push(name);
    }
  }
  return out;
}

/**
 * Shifts a table. Rows: the range moves/grows/shrinks; deleting the header row (without the whole
 * table) is refused like Excel does; deleting the totals row turns it off. Columns: inserted columns
 * get fresh names, deleted ones are dropped from the column list. Returns null when the whole table
 * is deleted.
 */
export function shiftTable(t: Table, e: StructuralEdit): Table | null {
  const r = shiftRange(t.range, e);
  if (!r) return null;
  if (isRowEdit(e)) {
    if (isInsert(e)) return { ...t, range: r };
    const end = e.at + e.count;
    if (t.headerRow && t.range.r0 >= e.at && t.range.r0 < end)
      throw new ModelError(`Cannot delete the header row of table "${t.name}" without deleting the table`);
    const totalsDeleted = t.totalsRow && t.range.r1 >= e.at && t.range.r1 < end;
    const next: Table = { ...t, range: r, totalsRow: t.totalsRow && !totalsDeleted };
    // A table needs at least one data row (header + 1).
    const minRows = 1 + (next.headerRow ? 1 : 0) + (next.totalsRow ? 1 : 0);
    if (r.r1 - r.r0 + 1 < minRows) {
      if (next.headerRow) next.range = { ...r, r1: r.r0 + minRows - 1 };
      else return null;
    }
    return next;
  }
  // Columns.
  const c0 = t.range.c0;
  const columns = [...t.columns];
  if (isInsert(e)) {
    if (e.at > c0 && e.at <= t.range.c1) {
      const names = freshColumnNames(
        columns.map((c) => c.name),
        e.count,
      );
      columns.splice(e.at - c0, 0, ...names.map((name) => ({ name })));
    }
    // Columns pushed off the sheet are lost.
    columns.length = Math.min(columns.length, r.c1 - r.c0 + 1);
    return { ...t, range: r, columns, autoFilter: shiftFilterColumns(t.autoFilter, c0, e) };
  }
  const end = e.at + e.count;
  const kept = columns.filter((_, i) => {
    const c = c0 + i;
    return c < e.at || c >= end;
  });
  if (kept.length === 0) return null;
  return { ...t, range: r, columns: kept, autoFilter: shiftFilterColumns(t.autoFilter, c0, e) };
}

/** Re-indexes filter columns (`colId` is relative to the filter's first column `c0`). */
function shiftFilterColumns<T extends Omit<AutoFilter, 'range'>>(
  f: T | undefined,
  c0: number,
  e: StructuralEdit,
): T | undefined {
  if (!f) return f;
  const columns = [];
  for (const fc of f.columns) {
    const abs = c0 + fc.colId;
    const n = shiftIndex(abs, e);
    if (n === null) continue;
    const newC0 = shiftIndex(c0, e) ?? e.at;
    columns.push({ ...fc, colId: n - newC0 });
  }
  return { ...f, columns };
}

export function shiftAutoFilter(f: AutoFilter, e: StructuralEdit): AutoFilter | null {
  const r = shiftRange(f.range, e);
  if (!r) return null;
  if (isRowEdit(e)) {
    const sort = f.sort ? shiftSortState(f.sort, e) : undefined;
    const out: AutoFilter = { ...f, range: r };
    if (sort) out.sort = sort;
    else delete out.sort;
    return out;
  }
  const shifted = shiftFilterColumns(f, f.range.c0, e) as AutoFilter;
  const out: AutoFilter = { ...shifted, range: r };
  const sort = f.sort ? shiftSortState(f.sort, e) : undefined;
  if (sort) out.sort = sort;
  else delete out.sort;
  return out;
}

function shiftSortState(s: NonNullable<AutoFilter['sort']>, e: StructuralEdit) {
  const range = shiftRange(s.range, e);
  if (!range) return undefined;
  const conditions = s.conditions
    .map((c) => {
      const r = shiftRange(c.range, e);
      return r ? { ...c, range: r } : null;
    })
    .filter((c): c is NonNullable<typeof c> => c !== null);
  return { ...s, range, conditions };
}

function shiftAnchorPoint(p: AnchorPoint, e: StructuralEdit): AnchorPoint {
  const rows = isRowEdit(e);
  const i = rows ? p.row : p.col;
  let n: number;
  let off = rows ? p.rowOff : p.colOff;
  if (i < e.at) n = i;
  else if (isInsert(e)) n = Math.min(i + e.count, axisMax(e) - 1);
  else if (i < e.at + e.count) {
    n = e.at;
    off = 0;
  } else n = i - e.count;
  return rows ? { ...p, row: n, rowOff: off } : { ...p, col: n, colOff: off };
}

/**
 * Drawing anchors move (and, for "move and size with cells", resize) with the cells under them.
 * Absolute anchors never move.
 */
export function shiftDrawing(d: DrawingAnchor, e: StructuralEdit): DrawingAnchor {
  const a = d.anchor;
  if (a.type === 'absolute') return d;
  if (a.type === 'oneCell') return { ...d, anchor: { ...a, from: shiftAnchorPoint(a.from, e) } };
  if (a.editAs === 'absolute') return d;
  const from = shiftAnchorPoint(a.from, e);
  let to: AnchorPoint;
  if (a.editAs === 'oneCell') {
    // Move but don't size: keep the extent in cells.
    const rows = isRowEdit(e);
    const d0 = rows ? from.row - a.from.row : from.col - a.from.col;
    to = rows
      ? { ...a.to, row: Math.min(a.to.row + d0, MAX_ROWS - 1) }
      : { ...a.to, col: Math.min(a.to.col + d0, MAX_COLS - 1) };
  } else to = shiftAnchorPoint(a.to, e);
  return { ...d, anchor: { ...a, from, to } };
}
