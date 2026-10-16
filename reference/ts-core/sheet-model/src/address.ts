/**
 * Cell ranges: a rectangle of 0-based, inclusive row/column indices, with A1 parsing and printing.
 */
import { columnToLetters, lettersToColumn, MAX_COLS, MAX_ROWS } from '@kiln-ref/formula';

export { MAX_COLS, MAX_ROWS };

/** A rectangle of cells, 0-based and inclusive. */
export interface CellRange {
  r0: number;
  c0: number;
  r1: number;
  c1: number;
}

export interface CellRef {
  row: number;
  col: number;
}

export class ModelError extends Error {
  override name = 'ModelError';
}

export function range(r0: number, c0: number, r1 = r0, c1 = c0): CellRange {
  return {
    r0: Math.min(r0, r1),
    c0: Math.min(c0, c1),
    r1: Math.max(r0, r1),
    c1: Math.max(c0, c1),
  };
}

export function checkCell(row: number, col: number): void {
  if (!Number.isInteger(row) || row < 0 || row >= MAX_ROWS)
    throw new ModelError(`Row ${row} is out of range`);
  if (!Number.isInteger(col) || col < 0 || col >= MAX_COLS)
    throw new ModelError(`Column ${col} is out of range`);
}

export function checkRange(r: CellRange): void {
  checkCell(r.r0, r.c0);
  checkCell(r.r1, r.c1);
  if (r.r0 > r.r1 || r.c0 > r.c1) throw new ModelError('Range corners are reversed');
}

export function rangeContains(r: CellRange, row: number, col: number): boolean {
  return row >= r.r0 && row <= r.r1 && col >= r.c0 && col <= r.c1;
}

export function rangesIntersect(a: CellRange, b: CellRange): boolean {
  return a.r0 <= b.r1 && b.r0 <= a.r1 && a.c0 <= b.c1 && b.c0 <= a.c1;
}

export function rangeInside(inner: CellRange, outer: CellRange): boolean {
  return inner.r0 >= outer.r0 && inner.r1 <= outer.r1 && inner.c0 >= outer.c0 && inner.c1 <= outer.c1;
}

export function rangeIntersection(a: CellRange, b: CellRange): CellRange | null {
  if (!rangesIntersect(a, b)) return null;
  return {
    r0: Math.max(a.r0, b.r0),
    c0: Math.max(a.c0, b.c0),
    r1: Math.min(a.r1, b.r1),
    c1: Math.min(a.c1, b.c1),
  };
}

export function rangeEquals(a: CellRange, b: CellRange): boolean {
  return a.r0 === b.r0 && a.c0 === b.c0 && a.r1 === b.r1 && a.c1 === b.c1;
}

export function rangeSize(r: CellRange): { rows: number; cols: number; cells: number } {
  const rows = r.r1 - r.r0 + 1;
  const cols = r.c1 - r.c0 + 1;
  return { rows, cols, cells: rows * cols };
}

export function isSingleCell(r: CellRange): boolean {
  return r.r0 === r.r1 && r.c0 === r.c1;
}

/** `A1`, or `$A$1` with `absolute`. */
export function formatCellRef(row: number, col: number, absolute = false): string {
  const d = absolute ? '$' : '';
  return `${d}${columnToLetters(col)}${d}${row + 1}`;
}

/**
 * Prints a range: `A1`, `A1:B2`, whole columns `A:C` and whole rows `1:3`.
 */
export function formatRange(r: CellRange, absolute = false): string {
  const d = absolute ? '$' : '';
  if (r.r0 === 0 && r.r1 === MAX_ROWS - 1 && !(r.c0 === 0 && r.c1 === MAX_COLS - 1))
    return `${d}${columnToLetters(r.c0)}:${d}${columnToLetters(r.c1)}`;
  if (r.c0 === 0 && r.c1 === MAX_COLS - 1 && !(r.r0 === 0 && r.r1 === MAX_ROWS - 1))
    return `${d}${r.r0 + 1}:${d}${r.r1 + 1}`;
  if (isSingleCell(r)) return formatCellRef(r.r0, r.c0, absolute);
  return `${formatCellRef(r.r0, r.c0, absolute)}:${formatCellRef(r.r1, r.c1, absolute)}`;
}

const CELL_RE = /^\$?([A-Za-z]{1,3})\$?([1-9][0-9]{0,6})$/;
const COL_RE = /^\$?([A-Za-z]{1,3})$/;
const ROW_RE = /^\$?([1-9][0-9]{0,6})$/;

/** Parses `B7` (case-insensitive, `$` allowed). */
export function parseCellRef(text: string): CellRef | undefined {
  const m = CELL_RE.exec(text.trim());
  if (!m) return undefined;
  const col = lettersToColumn((m[1] as string).toUpperCase());
  const row = Number(m[2]) - 1;
  if (col >= MAX_COLS || row >= MAX_ROWS) return undefined;
  return { row, col };
}

/** Parses `A1`, `A1:C3`, `A:C`, `2:5` (sheet prefixes are not accepted). */
export function parseRange(text: string): CellRange | undefined {
  const parts = text.trim().split(':');
  if (parts.length === 1) {
    const c = parseCellRef(parts[0] as string);
    return c ? { r0: c.row, c0: c.col, r1: c.row, c1: c.col } : undefined;
  }
  if (parts.length !== 2) return undefined;
  const [a, b] = parts as [string, string];
  const ca = parseCellRef(a);
  const cb = parseCellRef(b);
  if (ca && cb) return range(ca.row, ca.col, cb.row, cb.col);
  const colA = COL_RE.exec(a);
  const colB = COL_RE.exec(b);
  if (colA && colB) {
    const x = lettersToColumn((colA[1] as string).toUpperCase());
    const y = lettersToColumn((colB[1] as string).toUpperCase());
    if (x >= MAX_COLS || y >= MAX_COLS) return undefined;
    return range(0, x, MAX_ROWS - 1, y);
  }
  const rowA = ROW_RE.exec(a);
  const rowB = ROW_RE.exec(b);
  if (rowA && rowB) {
    const x = Number(rowA[1]) - 1;
    const y = Number(rowB[1]) - 1;
    if (x >= MAX_ROWS || y >= MAX_ROWS) return undefined;
    return range(x, 0, y, MAX_COLS - 1);
  }
  return undefined;
}

/** Parses a space-separated list of ranges (an OOXML `sqref`). */
export function parseSqref(text: string): CellRange[] {
  const out: CellRange[] = [];
  for (const part of text.trim().split(/\s+/)) {
    if (!part) continue;
    const r = parseRange(part);
    if (!r) throw new ModelError(`Invalid range "${part}"`);
    out.push(r);
  }
  return out;
}

export function formatSqref(ranges: readonly CellRange[]): string {
  return ranges.map((r) => formatRange(r)).join(' ');
}

/** Iterates all cells of a range in row-major order. */
export function* cellsOf(r: CellRange): Generator<CellRef> {
  for (let row = r.r0; row <= r.r1; row++) for (let col = r.c0; col <= r.c1; col++) yield { row, col };
}
