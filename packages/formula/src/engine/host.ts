/**
 * The host interface: the engine owns formulas, dependencies and computed results, but literal cell
 * values live in the host's storage (the sheet model). {@link MemoryHost} is a complete in-memory
 * implementation used by tests, benchmarks and headless evaluation.
 */
import { BLANK, type Scalar } from '../values/types.ts';

/** An Excel table (ListObject) as needed for structured references. */
export interface TableDef {
  name: string;
  sheet: number;
  /** Full table range including header and totals rows (0-based, inclusive). */
  r0: number;
  c0: number;
  r1: number;
  c1: number;
  /** Defaults to true. */
  headerRow?: boolean;
  /** Defaults to false. */
  totalsRow?: boolean;
  /** Column names, left to right. */
  columns: string[];
}

export interface StructuralEdit {
  kind: 'insertRows' | 'deleteRows' | 'insertCols' | 'deleteCols';
  at: number;
  count: number;
}

export interface EngineHost {
  /** Literal (non-formula) value of a cell, or undefined when empty. */
  getCellValue(sheet: number, row: number, col: number): Scalar | undefined;
  /** Writes a literal value (undefined clears). Required for `Engine.setCell({ value })`. */
  setCellValue?(sheet: number, row: number, col: number, value: Scalar | undefined): void;
  /** Extent of the sheet's literal values (exclusive). Lets whole-column references stay cheap. */
  getUsedBounds?(sheet: number): { rows: number; cols: number };
  getTable?(name: string): TableDef | undefined;
  /** The table containing a cell (for unqualified `[@Col]` references). */
  getTableAt?(sheet: number, row: number, col: number): TableDef | undefined;
  /** Notified after the engine applied an insert/delete so the host can shift its own cells. */
  applyStructuralEdit?(sheet: number, edit: StructuralEdit): void;
  /** Notified when a sheet is removed. */
  removeSheet?(sheet: number): void;
}

const SHEET_STRIDE = 17_179_869_184; // 2^34
const ROW_STRIDE = 16_384;

/** Packs (sheet, row, col) into one safe integer. */
export function cellKey(sheet: number, row: number, col: number): number {
  return sheet * SHEET_STRIDE + row * ROW_STRIDE + col;
}

export function unpackKey(key: number): { sheet: number; row: number; col: number } {
  const sheet = Math.floor(key / SHEET_STRIDE);
  const rest = key - sheet * SHEET_STRIDE;
  const row = Math.floor(rest / ROW_STRIDE);
  return { sheet, row, col: rest - row * ROW_STRIDE };
}

/** In-memory host: a sparse map of literal values plus a table registry. */
export class MemoryHost implements EngineHost {
  private readonly cells = new Map<number, Scalar>();
  private readonly bounds = new Map<number, { rows: number; cols: number }>();
  private readonly tables = new Map<string, TableDef>();

  getCellValue(sheet: number, row: number, col: number): Scalar | undefined {
    return this.cells.get(cellKey(sheet, row, col));
  }

  setCellValue(sheet: number, row: number, col: number, value: Scalar | undefined): void {
    const k = cellKey(sheet, row, col);
    if (value === undefined || value === BLANK) {
      this.cells.delete(k);
      return;
    }
    this.cells.set(k, value);
    const b = this.bounds.get(sheet);
    if (!b) this.bounds.set(sheet, { rows: row + 1, cols: col + 1 });
    else {
      if (row >= b.rows) b.rows = row + 1;
      if (col >= b.cols) b.cols = col + 1;
    }
  }

  getUsedBounds(sheet: number): { rows: number; cols: number } {
    return this.bounds.get(sheet) ?? { rows: 0, cols: 0 };
  }

  addTable(table: TableDef): void {
    this.tables.set(table.name.toUpperCase(), table);
  }

  removeTable(name: string): void {
    this.tables.delete(name.toUpperCase());
  }

  getTable(name: string): TableDef | undefined {
    return this.tables.get(name.toUpperCase());
  }

  getTableAt(sheet: number, row: number, col: number): TableDef | undefined {
    for (const t of this.tables.values()) {
      if (t.sheet === sheet && row >= t.r0 && row <= t.r1 && col >= t.c0 && col <= t.c1) return t;
    }
    return undefined;
  }

  applyStructuralEdit(sheet: number, edit: StructuralEdit): void {
    const rows = edit.kind === 'insertRows' || edit.kind === 'deleteRows';
    const insert = edit.kind === 'insertRows' || edit.kind === 'insertCols';
    const moved: [number, Scalar][] = [];
    for (const [k, v] of this.cells) {
      const p = unpackKey(k);
      if (p.sheet !== sheet) continue;
      const idx = rows ? p.row : p.col;
      if (idx < edit.at) continue;
      this.cells.delete(k);
      if (!insert && idx < edit.at + edit.count) continue;
      const n = insert ? idx + edit.count : idx - edit.count;
      moved.push([rows ? cellKey(sheet, n, p.col) : cellKey(sheet, p.row, n), v]);
    }
    for (const [k, v] of moved) this.cells.set(k, v);
    const b = this.bounds.get(sheet);
    if (b && insert) {
      if (rows) b.rows += edit.count;
      else b.cols += edit.count;
    }
  }

  removeSheet(sheet: number): void {
    for (const k of [...this.cells.keys()]) if (unpackKey(k).sheet === sheet) this.cells.delete(k);
    this.bounds.delete(sheet);
    for (const [n, t] of this.tables) if (t.sheet === sheet) this.tables.delete(n);
  }
}
