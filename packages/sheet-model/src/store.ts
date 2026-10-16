/**
 * The sparse cell store (PLAN.md §3.3): `Map<rowIndex, RowChunk>`, where a {@link RowChunk} keeps the
 * row's cells in parallel arrays sorted by column — `cols: Int32Array`, `values`, `formulas` and
 * `styleIdx: Int32Array` — for compact storage and fast row-major iteration. Random access is O(1)
 * through a per-row `Map<col, slot>` built lazily once a row has more than a handful of cells (smaller
 * rows use a binary search over `cols`, which is faster than hashing for them).
 *
 * A cell exists while it has a value, a formula or a non-default style (style index 0 is the
 * workbook's default `cellXfs` entry); setting all three to empty removes it.
 *
 * For a formula cell, `values` holds the last known result (as read from a file or written back by the
 * calculation layer); the formula text is authoritative.
 */
import { type CellRange, MAX_COLS, MAX_ROWS } from './address.ts';
import type { CellValue } from './values.ts';

/** Rows with more cells than this get a hash index for O(1) lookup. */
const INDEX_THRESHOLD = 16;

export type CellVisitor = (
  row: number,
  col: number,
  value: CellValue,
  formula: string | null,
  style: number,
) => void;

/** One stored row: parallel arrays sorted by column. */
export class RowChunk {
  cols: Int32Array;
  values: CellValue[];
  formulas: (string | null)[];
  styleIdx: Int32Array;
  /** Number of used slots. */
  size = 0;
  private index: Map<number, number> | null = null;
  private readsSinceChange = 0;

  constructor(capacity = 4) {
    this.cols = new Int32Array(capacity);
    this.styleIdx = new Int32Array(capacity);
    this.values = [];
    this.formulas = [];
  }

  /** Slot of a column, or -1. */
  find(col: number): number {
    const n = this.size;
    if (n === 0) return -1;
    // Fast paths: last cell (append-heavy fills) and the lazily built hash index.
    if (this.cols[n - 1] === col) return n - 1;
    if (this.index) return this.index.get(col) ?? -1;
    // Build the index lazily, once enough reads have happened since the last insertion/removal to
    // pay for it (keeps interleaved insert/read workloads from rebuilding it on every call).
    if (n > INDEX_THRESHOLD && ++this.readsSinceChange > n >>> 3) {
      this.buildIndex();
      return (this.index as unknown as Map<number, number>).get(col) ?? -1;
    }
    const i = this.lowerBound(col);
    return i < n && this.cols[i] === col ? i : -1;
  }

  /** First slot whose column is >= col. */
  lowerBound(col: number): number {
    let lo = 0;
    let hi = this.size;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if ((this.cols[mid] as number) < col) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }

  private buildIndex(): void {
    const m = new Map<number, number>();
    for (let i = 0; i < this.size; i++) m.set(this.cols[i] as number, i);
    this.index = m;
  }

  private grow(min: number): void {
    const cap = Math.max(min, this.cols.length * 2, 4);
    const cols = new Int32Array(cap);
    cols.set(this.cols.subarray(0, this.size));
    const st = new Int32Array(cap);
    st.set(this.styleIdx.subarray(0, this.size));
    this.cols = cols;
    this.styleIdx = st;
  }

  /** Inserts or overwrites a cell; returns true when a new slot was created. */
  put(col: number, value: CellValue, formula: string | null, style: number): boolean {
    const n = this.size;
    // Append (the common case when filling left to right).
    if (n === 0 || (this.cols[n - 1] as number) < col) {
      if (n === this.cols.length) this.grow(n + 1);
      this.cols[n] = col;
      this.styleIdx[n] = style;
      this.values[n] = value;
      this.formulas[n] = formula;
      this.size = n + 1;
      this.index?.set(col, n);
      return true;
    }
    const at = this.index ? (this.index.get(col) ?? -1) : -1;
    if (at >= 0) {
      this.values[at] = value;
      this.formulas[at] = formula;
      this.styleIdx[at] = style;
      return false;
    }
    const i = this.lowerBound(col);
    if (i < n && this.cols[i] === col) {
      this.values[i] = value;
      this.formulas[i] = formula;
      this.styleIdx[i] = style;
      return false;
    }
    if (n === this.cols.length) this.grow(n + 1);
    this.cols.copyWithin(i + 1, i, n);
    this.styleIdx.copyWithin(i + 1, i, n);
    this.values.splice(i, 0, value);
    this.formulas.splice(i, 0, formula);
    this.cols[i] = col;
    this.styleIdx[i] = style;
    this.size = n + 1;
    this.index = null;
    this.readsSinceChange = 0;
    return true;
  }

  /** Removes the cell in a slot. */
  removeAt(i: number): void {
    const n = this.size;
    this.cols.copyWithin(i, i + 1, n);
    this.styleIdx.copyWithin(i, i + 1, n);
    this.values.splice(i, 1);
    this.formulas.splice(i, 1);
    this.size = n - 1;
    this.index = null;
    this.readsSinceChange = 0;
  }

  /** Shifts columns for an insert (count > 0) or delete (count < 0) at `at`; drops deleted/overflowing cells. */
  shiftCols(at: number, count: number, maxCols = MAX_COLS): void {
    if (count === 0) return;
    let w = 0;
    const del = count < 0 ? -count : 0;
    for (let i = 0; i < this.size; i++) {
      let c = this.cols[i] as number;
      if (c >= at) {
        if (del > 0) {
          if (c < at + del) continue;
          c -= del;
        } else {
          c += count;
          if (c >= maxCols) continue;
        }
      }
      this.cols[w] = c;
      this.styleIdx[w] = this.styleIdx[i] as number;
      this.values[w] = this.values[i] as CellValue;
      this.formulas[w] = this.formulas[i] as string | null;
      w++;
    }
    this.values.length = w;
    this.formulas.length = w;
    this.size = w;
    this.index = null;
  }

  clone(): RowChunk {
    const c = new RowChunk(Math.max(this.size, 1));
    c.cols.set(this.cols.subarray(0, this.size));
    c.styleIdx.set(this.styleIdx.subarray(0, this.size));
    c.values = this.values.slice(0, this.size);
    c.formulas = this.formulas.slice(0, this.size);
    c.size = this.size;
    return c;
  }
}

export interface StoredCell {
  value: CellValue;
  formula: string | null;
  style: number;
}

function isEmptyCell(value: CellValue, formula: string | null, style: number): boolean {
  return value === null && formula === null && style === 0;
}

/** The sparse cell store of one sheet. */
export class SparseStore {
  private readonly chunks = new Map<number, RowChunk>();
  private sortedRows: number[] | null = [];
  private count = 0;
  /** Cached used ranges ([all cells, content only]); `undefined` = stale. */
  private usedCache: [CellRange | null | undefined, CellRange | null | undefined] = [null, null];

  /** Number of stored cells. */
  get size(): number {
    return this.count;
  }

  /** Number of rows with at least one cell. */
  get rowCount(): number {
    return this.chunks.size;
  }

  private touch(): void {
    this.usedCache = [undefined, undefined];
  }

  chunk(row: number): RowChunk | undefined {
    return this.chunks.get(row);
  }

  has(row: number, col: number): boolean {
    const ch = this.chunks.get(row);
    return ch !== undefined && ch.find(col) >= 0;
  }

  getValue(row: number, col: number): CellValue {
    const ch = this.chunks.get(row);
    if (!ch) return null;
    const i = ch.find(col);
    return i < 0 ? null : (ch.values[i] as CellValue);
  }

  getFormula(row: number, col: number): string | null {
    const ch = this.chunks.get(row);
    if (!ch) return null;
    const i = ch.find(col);
    return i < 0 ? null : (ch.formulas[i] as string | null);
  }

  getStyle(row: number, col: number): number {
    const ch = this.chunks.get(row);
    if (!ch) return 0;
    const i = ch.find(col);
    return i < 0 ? 0 : (ch.styleIdx[i] as number);
  }

  get(row: number, col: number): StoredCell | undefined {
    const ch = this.chunks.get(row);
    if (!ch) return undefined;
    const i = ch.find(col);
    if (i < 0) return undefined;
    return {
      value: ch.values[i] as CellValue,
      formula: ch.formulas[i] as string | null,
      style: ch.styleIdx[i] as number,
    };
  }

  /** Replaces a cell completely (all-empty removes it). */
  set(row: number, col: number, value: CellValue, formula: string | null = null, style = 0): void {
    if (isEmptyCell(value, formula, style)) {
      this.delete(row, col);
      return;
    }
    let ch = this.chunks.get(row);
    if (!ch) {
      ch = new RowChunk();
      this.chunks.set(row, ch);
      this.sortedRows = null;
    }
    if (ch.put(col, value, formula, style)) this.count++;
    this.touch();
  }

  setValue(row: number, col: number, value: CellValue): void {
    const c = this.get(row, col);
    this.set(row, col, value, c?.formula ?? null, c?.style ?? 0);
  }

  setFormula(row: number, col: number, formula: string | null, cachedValue: CellValue = null): void {
    const c = this.get(row, col);
    this.set(row, col, cachedValue, formula, c?.style ?? 0);
  }

  setStyle(row: number, col: number, style: number): void {
    const c = this.get(row, col);
    this.set(row, col, c?.value ?? null, c?.formula ?? null, style);
  }

  /**
   * Writes the cached result of a formula cell without changing anything else (no-op for cells that
   * are not formulas). Used by the calculation layer.
   */
  setCachedValue(row: number, col: number, value: CellValue): void {
    const ch = this.chunks.get(row);
    if (!ch) return;
    const i = ch.find(col);
    if (i >= 0 && ch.formulas[i] !== null) ch.values[i] = value;
  }

  delete(row: number, col: number): boolean {
    const ch = this.chunks.get(row);
    if (!ch) return false;
    const i = ch.find(col);
    if (i < 0) return false;
    ch.removeAt(i);
    this.count--;
    if (ch.size === 0) {
      this.chunks.delete(row);
      this.sortedRows = null;
    }
    this.touch();
    return true;
  }

  clear(): void {
    this.chunks.clear();
    this.sortedRows = [];
    this.count = 0;
    this.touch();
  }

  /** Stored row indices in ascending order. */
  rows(): readonly number[] {
    if (!this.sortedRows) {
      const keys = Array.from(this.chunks.keys());
      this.sortedRows = keys.sort((a, b) => a - b);
    }
    return this.sortedRows;
  }

  /** Visits every stored cell in row-major order. */
  forEach(visit: CellVisitor): void {
    for (const row of this.rows()) {
      const ch = this.chunks.get(row) as RowChunk;
      const { cols, values, formulas, styleIdx } = ch;
      for (let i = 0; i < ch.size; i++)
        visit(
          row,
          cols[i] as number,
          values[i] as CellValue,
          formulas[i] as string | null,
          styleIdx[i] as number,
        );
    }
  }

  /** Visits the stored cells inside a range in row-major order. */
  forEachInRange(r: CellRange, visit: CellVisitor): void {
    const rows = this.rows();
    const span = r.r1 - r.r0 + 1;
    const visitRow = (row: number) => {
      const ch = this.chunks.get(row);
      if (!ch) return;
      const { cols, values, formulas, styleIdx } = ch;
      for (let i = ch.lowerBound(r.c0); i < ch.size; i++) {
        const c = cols[i] as number;
        if (c > r.c1) break;
        visit(row, c, values[i] as CellValue, formulas[i] as string | null, styleIdx[i] as number);
      }
    };
    if (span <= rows.length) {
      for (let row = r.r0; row <= r.r1; row++) visitRow(row);
      return;
    }
    let lo = 0;
    let hi = rows.length;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if ((rows[mid] as number) < r.r0) lo = mid + 1;
      else hi = mid;
    }
    // Copy: the visitor may mutate the store.
    const slice: number[] = [];
    for (let i = lo; i < rows.length && (rows[i] as number) <= r.r1; i++) slice.push(rows[i] as number);
    for (const row of slice) visitRow(row);
  }

  /** Number of stored cells inside a range. */
  countInRange(r: CellRange): number {
    let n = 0;
    this.forEachInRange(r, () => {
      n++;
    });
    return n;
  }

  /**
   * The smallest range containing every stored cell (or only cells with content when
   * `contentOnly`, ignoring style-only cells), or null when empty.
   */
  getUsedRange(contentOnly = false): CellRange | null {
    const slot = contentOnly ? 1 : 0;
    const hit = this.usedCache[slot];
    if (hit !== undefined) return hit === null ? null : { ...hit };
    let r0 = Number.POSITIVE_INFINITY;
    let r1 = -1;
    let c0 = Number.POSITIVE_INFINITY;
    let c1 = -1;
    for (const row of this.rows()) {
      const ch = this.chunks.get(row) as RowChunk;
      if (contentOnly) {
        for (let i = 0; i < ch.size; i++) {
          if (ch.values[i] === null && ch.formulas[i] === null) continue;
          const c = ch.cols[i] as number;
          if (row < r0) r0 = row;
          if (row > r1) r1 = row;
          if (c < c0) c0 = c;
          if (c > c1) c1 = c;
        }
      } else if (ch.size > 0) {
        if (row < r0) r0 = row;
        if (row > r1) r1 = row;
        const a = ch.cols[0] as number;
        const b = ch.cols[ch.size - 1] as number;
        if (a < c0) c0 = a;
        if (b > c1) c1 = b;
      }
    }
    const out = r1 < 0 ? null : { r0, c0, r1, c1 };
    this.usedCache[slot] = out ? { ...out } : null;
    return out;
  }

  /**
   * Bulk write of a 2-D block of values with its top-left at (row, col). `undefined` entries are
   * skipped; `null` clears the value (keeping the cell's style). Formulas in the block are removed
   * unless `formulas` supplies them (same shape; `null` = no formula).
   */
  setRange(
    row: number,
    col: number,
    values: readonly (readonly (CellValue | undefined)[])[],
    formulas?: readonly (readonly (string | null | undefined)[])[],
  ): void {
    for (let i = 0; i < values.length; i++) {
      const line = values[i] as readonly (CellValue | undefined)[];
      const fline = formulas?.[i];
      const r = row + i;
      let ch = this.chunks.get(r);
      for (let j = 0; j < line.length; j++) {
        const v = line[j];
        const f = fline?.[j];
        if (v === undefined && f === undefined) continue;
        const c = col + j;
        if (!ch) {
          if ((v ?? null) === null && (f ?? null) === null) continue;
          ch = new RowChunk(line.length);
          this.chunks.set(r, ch);
          this.sortedRows = null;
        }
        const at = ch.find(c);
        const style = at >= 0 ? (ch.styleIdx[at] as number) : 0;
        const value = v === undefined ? (at >= 0 ? (ch.values[at] as CellValue) : null) : v;
        const formula = f === undefined ? null : f;
        if (isEmptyCell(value, formula, style)) {
          if (at >= 0) {
            ch.removeAt(at);
            this.count--;
          }
          continue;
        }
        if (ch.put(c, value, formula, style)) this.count++;
      }
      if (ch && ch.size === 0) {
        this.chunks.delete(r);
        this.sortedRows = null;
      }
    }
    this.touch();
  }

  /** Removes every cell in a range. */
  clearRange(r: CellRange): void {
    const doomed: [number, number][] = [];
    this.forEachInRange(r, (row, col) => doomed.push([row, col]));
    for (const [row, col] of doomed) this.delete(row, col);
  }

  /** Shifts rows for an insert (count > 0) or delete (count < 0) at `at`. */
  shiftRows(at: number, count: number, maxRows = MAX_ROWS): void {
    if (count === 0) return;
    const del = count < 0 ? -count : 0;
    const moved: [number, RowChunk][] = [];
    for (const row of this.rows()) {
      if (row < at) continue;
      const ch = this.chunks.get(row) as RowChunk;
      this.chunks.delete(row);
      if (del > 0) {
        if (row < at + del) {
          this.count -= ch.size;
          continue;
        }
        moved.push([row - del, ch]);
      } else {
        const n = row + count;
        if (n >= maxRows) {
          this.count -= ch.size;
          continue;
        }
        moved.push([n, ch]);
      }
    }
    for (const [row, ch] of moved) this.chunks.set(row, ch);
    this.sortedRows = null;
    this.touch();
  }

  /** Shifts columns of every row for an insert (count > 0) or delete (count < 0) at `at`. */
  shiftCols(at: number, count: number, maxCols = MAX_COLS): void {
    if (count === 0) return;
    for (const row of this.rows()) {
      const ch = this.chunks.get(row) as RowChunk;
      const before = ch.size;
      ch.shiftCols(at, count, maxCols);
      this.count -= before - ch.size;
      if (ch.size === 0) this.chunks.delete(row);
    }
    this.sortedRows = null;
    this.touch();
  }

  /** Deep copy. */
  clone(): SparseStore {
    const s = new SparseStore();
    for (const [row, ch] of this.chunks) s.chunks.set(row, ch.clone());
    s.count = this.count;
    s.sortedRows = null;
    s.usedCache = [undefined, undefined];
    return s;
  }
}
