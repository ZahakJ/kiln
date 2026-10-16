/**
 * Commands: every user-visible mutation of a workbook as a plain, JSON-serialisable object. Applying a
 * command validates it, mutates the model, emits change events and returns the *inverse* command
 * that undoes it exactly — the basis of {@link CommandLog}'s undo/redo, of journals and of
 * collaboration.
 *
 * Inverses of operations with workbook-wide side effects (structural edits, moves, sheet removal…)
 * are a `batch` of the opposite operation plus a `restore` of everything the forward operation
 * rewrote (formula texts, metadata, defined names), so undo is exact even where the opposite
 * operation alone would not be (for example references turned into `#REF!` by a deletion).
 */
import {
  copyFormula,
  ErrorValue,
  moveFormula,
  parse,
  removeSheetInFormula,
  renameSheetInFormula,
  shiftFormula,
  sortCompare,
} from '@officina/formula';
import {
  type CellRange,
  checkCell,
  checkRange,
  MAX_COLS,
  MAX_ROWS,
  ModelError,
  rangeInside,
  rangesIntersect,
} from './address.ts';
import type { ColProps, RowProps } from './axis.ts';
import type { CalcProps, DefinedName, Table, WorkbookProps } from './meta.ts';
import type { Sheet, SheetLayoutJSON, SheetMetaJSON } from './sheet.ts';
import { isInsert, isRowEdit, type StructuralEdit, shiftTable } from './shift.ts';
import { restoreSheet, type SheetSnapshot, snapshotSheet } from './snapshot.ts';
import type { StoredCell } from './store.ts';
import { renameTableColumnInFormula, renameTableInFormula, structuredToA1 } from './structured.ts';
import { canonicalKey, type StylePatch } from './styles.ts';
import { type CellValue, decodeValue, type EncodedValue, encodeValue } from './values.ts';
import { checkSheetName, type FormulaChange, type Workbook } from './workbook.ts';

// ── command types ─────────────────────────────────────────────────────────

/**
 * A cell edit. `formula` (with or without `=`) makes a formula cell (`value` is then its cached
 * result); a `value` alone replaces the content (removing any formula); `formula: null` clears the
 * content; `style` alone changes only the format.
 */
export interface CellInput {
  row: number;
  col: number;
  value?: EncodedValue;
  formula?: string | null;
  style?: number;
}

/** A cell's complete state (all-empty = no cell). */
export interface CellRecord {
  row: number;
  col: number;
  value: EncodedValue;
  formula: string | null;
  style: number;
}

export type ClearWhat = 'all' | 'contents' | 'formats' | 'notes' | 'hyperlinks';
export type PasteWhat = 'all' | 'values' | 'formulas' | 'formats';

export interface SortKey {
  /** Absolute column (or row, with `byColumns`) index to sort by. */
  index: number;
  descending?: boolean;
  /** Custom order (e.g. months); values not in the list sort after it. */
  customList?: string[];
}

/** Metadata fields settable through `setSheetMeta`. */
export type SheetMetaKey = Exclude<keyof SheetMetaJSON, 'tables' | 'merges'>;

export type Command =
  | { type: 'setCells'; sheet: number; cells: CellInput[] }
  | { type: 'setRange'; sheet: number; row: number; col: number; values: EncodedValue[][] }
  | { type: 'clearRange'; sheet: number; ranges: CellRange[]; what: ClearWhat }
  | { type: 'setStyle'; sheet: number; ranges: CellRange[]; patch: StylePatch }
  | { type: 'setStyleIndex'; sheet: number; ranges: CellRange[]; style: number }
  | {
      type: 'insertRows' | 'insertCols';
      sheet: number;
      at: number;
      count: number;
      /** Take the format of the row/column before (Excel's default) or after the insertion point. */
      formatFrom?: 'before' | 'after' | 'none';
    }
  | { type: 'deleteRows' | 'deleteCols'; sheet: number; at: number; count: number }
  | { type: 'moveRange'; sheet: number; range: CellRange; to: { sheet?: number; row: number; col: number } }
  | {
      type: 'copyRange';
      sheet: number;
      range: CellRange;
      to: { sheet?: number; range: CellRange };
      what?: PasteWhat;
      transpose?: boolean;
      skipBlanks?: boolean;
    }
  | {
      type: 'sort';
      sheet: number;
      range: CellRange;
      keys: SortKey[];
      header?: boolean;
      caseSensitive?: boolean;
      /** Sort left to right (columns) instead of top to bottom. */
      byColumns?: boolean;
    }
  | { type: 'addSheet'; name?: string; index?: number; id?: number }
  | { type: 'removeSheet'; sheet: number }
  | { type: 'restoreSheet'; snapshot: SheetSnapshot; index: number }
  | { type: 'renameSheet'; sheet: number; name: string }
  | { type: 'moveSheet'; sheet: number; index: number }
  | { type: 'setRowProps'; sheet: number; at: number; count: number; props: NullablePatch<RowProps> }
  | { type: 'setColProps'; sheet: number; at: number; count: number; props: NullablePatch<ColProps> }
  | { type: 'setAxis'; sheet: number; axis: 'rows' | 'cols'; entries: [number, RowProps | ColProps | null][] }
  | { type: 'merge'; sheet: number; range: CellRange; mode?: 'merge' | 'center' | 'across' }
  | { type: 'unmerge'; sheet: number; range: CellRange }
  | {
      type: 'defineName';
      name: string;
      formula: string;
      scope: number | null;
      hidden?: boolean;
      comment?: string;
    }
  | { type: 'deleteName'; name: string; scope: number | null }
  | { type: 'addTable'; sheet: number; table: Omit<Table, 'id'> & { id?: number } }
  | { type: 'removeTable'; name: string }
  | { type: 'updateTable'; name: string; patch: TablePatch }
  | { type: 'setFreeze'; sheet: number; rows: number; cols: number }
  | { type: 'setSheetMeta'; sheet: number; key: SheetMetaKey; value: unknown }
  | { type: 'setProps'; patch: PropsPatch }
  | { type: 'batch'; commands: Command[]; label?: string }
  | {
      type: 'restore';
      cells?: { sheet: number; records: CellRecord[] }[];
      meta?: { sheet: number; meta: SheetMetaJSON }[];
      layout?: { sheet: number; layout: SheetLayoutJSON }[];
      names?: DefinedName[];
    };

export type NullablePatch<T> = { [K in keyof T]?: T[K] | null };

export type TablePatch = Partial<
  Pick<
    Table,
    'name' | 'displayName' | 'range' | 'headerRow' | 'totalsRow' | 'style' | 'autoFilter' | 'comment'
  >
> & {
  columns?: Table['columns'];
};

export type PropsPatch = Partial<Omit<WorkbookProps, 'calc'>> & { calc?: Partial<CalcProps> };

export type CommandType = Command['type'];

// ── helpers ───────────────────────────────────────────────────────────────

const clone = <T>(v: T): T => structuredClone(v);

function normFormula(f: string): string {
  const t = f.replace(/^\s*=/, '');
  try {
    parse(t);
  } catch (e) {
    throw new ModelError(`Invalid formula "=${t}": ${(e as Error).message}`);
  }
  return t;
}

function recordOf(row: number, col: number, c: StoredCell | undefined): CellRecord {
  return c
    ? { row, col, value: encodeValue(c.value), formula: c.formula, style: c.style }
    : { row, col, value: null, formula: null, style: 0 };
}

function sheetOf(wb: Workbook, id: number): Sheet {
  return wb.sheet(id);
}

function checkStyle(wb: Workbook, style: number): void {
  if (!wb.styles.has(style)) throw new ModelError(`Style index ${style} does not exist`);
}

/** Writes full records and emits one `cells` event. */
function writeRecords(wb: Workbook, sheet: Sheet, records: readonly CellRecord[]): void {
  const coords: number[] = [];
  for (const r of records) {
    sheet.cells.set(r.row, r.col, decodeValue(r.value), r.formula, r.style);
    coords.push(r.row, r.col);
  }
  if (coords.length) wb.emit({ type: 'cells', sheet: sheet.id, coords });
}

function allMeta(wb: Workbook): { sheet: number; meta: SheetMetaJSON }[] {
  return wb.sheets.map((s) => ({ sheet: s.id, meta: s.metaToJSON() }));
}

function changesToRecords(changes: FormulaChange[]): Map<number, CellRecord[]> {
  const out = new Map<number, CellRecord[]>();
  for (const c of changes) {
    let list = out.get(c.sheet);
    if (!list) {
      list = [];
      out.set(c.sheet, list);
    }
    list.push(recordOf(c.row, c.col, c.before));
  }
  return out;
}

function restoreCellsFrom(map: Map<number, CellRecord[]>): { sheet: number; records: CellRecord[] }[] {
  return [...map].map(([sheet, records]) => ({ sheet, records: dedupeRecords(records) }));
}

/** Keeps the first record per cell. */
function dedupeRecords(records: CellRecord[]): CellRecord[] {
  const seen = new Set<number>();
  const out: CellRecord[] = [];
  for (const r of records) {
    const k = r.row * MAX_COLS + r.col;
    if (seen.has(k)) continue;
    seen.add(k);
    out.push(r);
  }
  return out;
}

function addRecords(map: Map<number, CellRecord[]>, sheet: number, records: CellRecord[]): void {
  const list = map.get(sheet);
  if (list) list.push(...records);
  else map.set(sheet, [...records]);
}

function recordsIn(sheet: Sheet, r: CellRange): CellRecord[] {
  const out: CellRecord[] = [];
  sheet.cells.forEachInRange(r, (row, col, value, formula, style) =>
    out.push({ row, col, value: encodeValue(value), formula, style }),
  );
  return out;
}

function emitTablesIfChanged(wb: Workbook, before: string, after: string): void {
  if (before !== after) wb.emit({ type: 'tables' });
}

function tablesKey(wb: Workbook): string {
  return canonicalKey(wb.sheets.map((s) => s.tables));
}

// ── id resolution ─────────────────────────────────────────────────────────

/**
 * Fills in what a command would otherwise choose at apply time (new sheet ids, names and positions,
 * new table ids), so that re-applying it — redo, journal replay, collaboration — produces exactly the
 * same workbook.
 */
export function resolveCommand(wb: Workbook, cmd: Command): Command {
  const counters = wb.idCounters();
  const names = new Set(wb.sheets.map((s) => s.name.toUpperCase()));
  let sheetCount = wb.sheets.length;
  const walk = (c: Command): Command => {
    if (c.type === 'batch') return { ...c, commands: c.commands.map(walk) };
    if (c.type === 'addSheet') {
      let name = c.name;
      if (name === undefined)
        for (let i = 1; ; i++)
          if (!names.has(`SHEET${i}`)) {
            name = `Sheet${i}`;
            break;
          }
      names.add((name as string).toUpperCase());
      const out = { ...c, name: name as string, id: c.id ?? counters.sheet++, index: c.index ?? sheetCount };
      sheetCount++;
      return out;
    }
    if (c.type === 'addTable' && c.table.id === undefined)
      return { ...c, table: { ...c.table, id: counters.table++ } };
    return c;
  };
  return walk(cmd);
}

// ── apply ─────────────────────────────────────────────────────────────────

/** Applies a command and returns its inverse. Throws {@link ModelError} (changing nothing) when invalid. */
export function applyCommand(wb: Workbook, cmd: Command): Command {
  return wb.transact(() => applyInner(wb, cmd));
}

function applyInner(wb: Workbook, cmd: Command): Command {
  switch (cmd.type) {
    case 'setCells':
      return setCells(wb, cmd.sheet, cmd.cells);
    case 'setRange':
      return setRange(wb, cmd);
    case 'clearRange':
      return clearRange(wb, cmd);
    case 'setStyle':
      return setStyle(wb, cmd.sheet, cmd.ranges, (base, cache) => {
        let n = cache.get(base);
        if (n === undefined) {
          n = wb.styles.patch(base, cmd.patch);
          cache.set(base, n);
        }
        return n;
      });
    case 'setStyleIndex':
      checkStyle(wb, cmd.style);
      return setStyle(wb, cmd.sheet, cmd.ranges, () => cmd.style);
    case 'insertRows':
    case 'insertCols':
    case 'deleteRows':
    case 'deleteCols':
      return structural(wb, cmd);
    case 'moveRange':
      return moveRange(wb, cmd);
    case 'copyRange':
      return copyRange(wb, cmd);
    case 'sort':
      return sortRange(wb, cmd);
    case 'addSheet':
      return addSheet(wb, cmd);
    case 'removeSheet':
      return removeSheet(wb, cmd.sheet);
    case 'restoreSheet':
      return restoreSheetCmd(wb, cmd);
    case 'renameSheet':
      return renameSheet(wb, cmd.sheet, cmd.name);
    case 'moveSheet':
      return moveSheet(wb, cmd.sheet, cmd.index);
    case 'setRowProps':
      return setAxisProps(wb, cmd.sheet, 'rows', cmd.at, cmd.count, cmd.props);
    case 'setColProps':
      return setAxisProps(wb, cmd.sheet, 'cols', cmd.at, cmd.count, cmd.props);
    case 'setAxis':
      return setAxisEntries(wb, cmd.sheet, cmd.axis, cmd.entries);
    case 'merge':
      return mergeCells(wb, cmd);
    case 'unmerge':
      return unmergeCells(wb, cmd);
    case 'defineName':
      return defineName(wb, cmd);
    case 'deleteName':
      return deleteName(wb, cmd.name, cmd.scope);
    case 'addTable':
      return addTable(wb, cmd);
    case 'removeTable':
      return removeTable(wb, cmd.name);
    case 'updateTable':
      return updateTable(wb, cmd.name, cmd.patch);
    case 'setFreeze': {
      const sheet = sheetOf(wb, cmd.sheet);
      if (cmd.rows < 0 || cmd.cols < 0 || cmd.rows >= MAX_ROWS || cmd.cols >= MAX_COLS)
        throw new ModelError('Invalid freeze position');
      const view = clone(sheet.view);
      if (cmd.rows === 0 && cmd.cols === 0) delete view.freeze;
      else {
        view.freeze = { rows: cmd.rows, cols: cmd.cols };
        delete view.split;
      }
      return setSheetMeta(wb, cmd.sheet, 'view', view);
    }
    case 'setSheetMeta':
      return setSheetMeta(wb, cmd.sheet, cmd.key, cmd.value);
    case 'setProps':
      return setProps(wb, cmd.patch);
    case 'batch': {
      const inverses: Command[] = [];
      try {
        for (const c of cmd.commands) inverses.push(applyInner(wb, c));
      } catch (e) {
        // Roll back what was applied so a failing batch changes nothing.
        for (let i = inverses.length - 1; i >= 0; i--) applyInner(wb, inverses[i] as Command);
        throw e;
      }
      const inv: Command = { type: 'batch', commands: inverses.reverse() };
      if (cmd.label !== undefined) inv.label = cmd.label;
      return inv;
    }
    case 'restore':
      return restore(wb, cmd);
    default: {
      const t: never = cmd;
      throw new ModelError(`Unknown command ${(t as { type: string }).type}`);
    }
  }
}

// ── cells ─────────────────────────────────────────────────────────────────

function setCells(wb: Workbook, sheetId: number, cells: readonly CellInput[]): Command {
  const sheet = sheetOf(wb, sheetId);
  // Validate everything first.
  const prepared: CellRecord[] = [];
  for (const c of cells) {
    checkCell(c.row, c.col);
    if (c.style !== undefined) checkStyle(wb, c.style);
    const prev = sheet.cells.get(c.row, c.col);
    let value: EncodedValue;
    let formula: string | null;
    if (typeof c.formula === 'string') {
      formula = normFormula(c.formula);
      value = c.value ?? null;
    } else if (c.formula === null) {
      formula = null;
      value = c.value ?? null;
    } else if (c.value !== undefined) {
      formula = null;
      value = c.value;
    } else {
      formula = prev?.formula ?? null;
      value = encodeValue(prev?.value ?? null);
    }
    decodeValue(value); // validates error codes
    const style = c.style ?? (prev ? prev.style : sheet.styleAt(c.row, c.col));
    prepared.push({ row: c.row, col: c.col, value, formula, style });
  }
  const before = dedupeRecords(prepared.map((p) => recordOf(p.row, p.col, sheet.cells.get(p.row, p.col))));
  // Header cells of tables rename their column (Excel keeps the header text and column name in sync).
  const metaBefore = allMeta(wb);
  const namesBefore = clone(wb.names);
  const tablesBefore = tablesKey(wb);
  const formulaChanges: FormulaChange[] = [];
  writeRecords(wb, sheet, prepared);
  for (const p of prepared) {
    const t = sheet.tableAt(p.row, p.col);
    if (!t?.headerRow || p.row !== t.range.r0) continue;
    const idx = p.col - t.range.c0;
    const col = t.columns[idx];
    if (!col) continue;
    const v = decodeValue(p.value);
    const wanted = v === null ? '' : v instanceof ErrorValue ? v.code : String(v);
    if (wanted === col.name) continue;
    const others = t.columns.filter((_, i) => i !== idx).map((c) => c.name.toUpperCase());
    let name = wanted || `Column${idx + 1}`;
    for (let k = 2; others.includes(name.toUpperCase()); k++) name = `${wanted || 'Column'}${k}`;
    const old = col.name;
    col.name = name;
    if (name !== wanted) {
      sheet.cells.setValue(p.row, p.col, name);
      wb.emit({ type: 'cells', sheet: sheet.id, coords: [p.row, p.col] });
    }
    formulaChanges.push(
      ...wb.rewriteFormulas((text, fs, at) =>
        renameTableColumnInFormula(
          text,
          t.name,
          old,
          name,
          fs.toUpperCase() === sheet.name.toUpperCase() &&
            at !== undefined &&
            at.row >= t.range.r0 &&
            at.row <= t.range.r1 &&
            at.col >= t.range.c0 &&
            at.col <= t.range.c1,
        ),
      ),
    );
  }
  const tablesAfter = tablesKey(wb);
  if (tablesBefore === tablesAfter && formulaChanges.length === 0)
    return { type: 'setCells', sheet: sheetId, cells: before };
  emitTablesIfChanged(wb, tablesBefore, tablesAfter);
  emitFormulaChanges(wb, formulaChanges);
  wb.emit({ type: 'names' });
  const records = changesToRecords(formulaChanges);
  addRecords(records, sheetId, before);
  return { type: 'restore', cells: restoreCellsFrom(records), meta: metaBefore, names: namesBefore };
}

/** After a workbook-wide rewrite, tells listeners which cells' formulas changed. */
function emitFormulaChanges(wb: Workbook, changes: FormulaChange[]): void {
  const bySheet = new Map<number, number[]>();
  for (const c of changes) {
    let list = bySheet.get(c.sheet);
    if (!list) {
      list = [];
      bySheet.set(c.sheet, list);
    }
    list.push(c.row, c.col);
  }
  for (const [sheet, coords] of bySheet) wb.emit({ type: 'cells', sheet, coords });
}

function setRange(wb: Workbook, cmd: Extract<Command, { type: 'setRange' }>): Command {
  const sheet = sheetOf(wb, cmd.sheet);
  const rows = cmd.values.length;
  const cols = Math.max(0, ...cmd.values.map((r) => r.length));
  if (rows === 0 || cols === 0) return { type: 'batch', commands: [] };
  checkRange({ r0: cmd.row, c0: cmd.col, r1: cmd.row + rows - 1, c1: cmd.col + cols - 1 });
  const before: CellRecord[] = [];
  const coords: number[] = [];
  for (let i = 0; i < rows; i++) {
    const line = cmd.values[i] as EncodedValue[];
    for (let j = 0; j < line.length; j++) {
      const v = line[j];
      if (v === undefined) continue;
      const row = cmd.row + i;
      const col = cmd.col + j;
      const prev = sheet.cells.get(row, col);
      before.push(recordOf(row, col, prev));
      sheet.cells.set(row, col, decodeValue(v), null, prev ? prev.style : sheet.styleAt(row, col));
      coords.push(row, col);
    }
  }
  if (coords.length) wb.emit({ type: 'cells', sheet: sheet.id, coords });
  return { type: 'setCells', sheet: cmd.sheet, cells: before };
}

function clearRange(wb: Workbook, cmd: Extract<Command, { type: 'clearRange' }>): Command {
  const sheet = sheetOf(wb, cmd.sheet);
  for (const r of cmd.ranges) checkRange(r);
  const meta = sheet.metaToJSON();
  const before: CellRecord[] = [];
  const coords: number[] = [];
  const touchesCells = cmd.what === 'all' || cmd.what === 'contents' || cmd.what === 'formats';
  if (touchesCells) {
    for (const r of cmd.ranges) {
      for (const rec of recordsIn(sheet, r)) {
        before.push(rec);
        const value = cmd.what === 'formats' ? decodeValue(rec.value) : null;
        const formula = cmd.what === 'formats' ? rec.formula : null;
        const style = cmd.what === 'contents' ? rec.style : 0;
        sheet.cells.set(rec.row, rec.col, value, formula, style);
        coords.push(rec.row, rec.col);
      }
    }
  }
  let metaChanged = false;
  const inAny = (row: number, col: number) =>
    cmd.ranges.some((r) => row >= r.r0 && row <= r.r1 && col >= r.c0 && col <= r.c1);
  if (cmd.what === 'all' || cmd.what === 'notes') {
    const n0 = sheet.notes.length + sheet.threads.length;
    sheet.notes = sheet.notes.filter((n) => !inAny(n.row, n.col));
    sheet.threads = sheet.threads.filter((t) => !inAny(t.row, t.col));
    metaChanged ||= n0 !== sheet.notes.length + sheet.threads.length;
  }
  if (cmd.what === 'all' || cmd.what === 'hyperlinks') {
    const n0 = sheet.hyperlinks.length;
    sheet.hyperlinks = sheet.hyperlinks.filter((h) => !cmd.ranges.some((r) => rangeInside(h.ref, r)));
    metaChanged ||= n0 !== sheet.hyperlinks.length;
  }
  if (cmd.what === 'all' || cmd.what === 'formats') {
    for (const r of cmd.ranges) metaChanged = sheet.removeMerges(r).length > 0 || metaChanged;
  }
  if (coords.length) wb.emit({ type: 'cells', sheet: sheet.id, coords });
  if (metaChanged) wb.emit({ type: 'meta', sheet: sheet.id, key: 'clear' });
  return {
    type: 'restore',
    cells: [{ sheet: sheet.id, records: before }],
    ...(metaChanged ? { meta: [{ sheet: sheet.id, meta }] } : {}),
  };
}

const isWholeCols = (r: CellRange) => r.r0 === 0 && r.r1 === MAX_ROWS - 1;
const isWholeRows = (r: CellRange) => r.c0 === 0 && r.c1 === MAX_COLS - 1;

/**
 * Applies a style mapping to ranges. Whole columns/rows set the column/row format (plus every existing
 * cell in them), like Excel; other ranges format each cell.
 */
function setStyle(
  wb: Workbook,
  sheetId: number,
  ranges: readonly CellRange[],
  map: (base: number, cache: Map<number, number>) => number,
): Command {
  const sheet = sheetOf(wb, sheetId);
  for (const r of ranges) checkRange(r);
  const cache = new Map<number, number>();
  const before = new Map<number, CellRecord>();
  const coords: number[] = [];
  const axisBefore: { rows: [number, RowProps | null][]; cols: [number, ColProps | null][] } = {
    rows: [],
    cols: [],
  };
  const touch = (row: number, col: number, base: number) => {
    const k = row * MAX_COLS + col;
    if (before.has(k)) return;
    const prev = sheet.cells.get(row, col);
    before.set(k, recordOf(row, col, prev));
    const next = map(base, cache);
    sheet.cells.set(row, col, prev?.value ?? null, prev?.formula ?? null, next);
    coords.push(row, col);
  };
  for (const r of ranges) {
    const cols = isWholeCols(r);
    const rows = isWholeRows(r);
    if (cols && !rows) {
      for (let c = r.c0; c <= r.c1; c++) {
        const p = sheet.cols.get(c);
        axisBefore.cols.push([c, p ? { ...p } : null]);
        sheet.cols.patch(c, { style: map(p?.style ?? 0, cache) });
      }
      // Cells at the intersection with formatted rows take the new format too.
      for (const [row, p] of sheet.rows.entries())
        if (p.style !== undefined) for (let c = r.c0; c <= r.c1; c++) touch(row, c, sheet.styleAt(row, c));
      sheet.cells.forEachInRange(r, (row, col, _v, _f, style) => touch(row, col, style));
    } else if (rows) {
      for (let i = r.r0; i <= r.r1; i++) {
        const p = sheet.rows.get(i);
        axisBefore.rows.push([i, p ? { ...p } : null]);
        sheet.rows.patch(i, { style: map(p?.style ?? 0, cache) });
      }
      if (!cols)
        for (const [col, p] of sheet.cols.entries())
          if (p.style !== undefined)
            for (let i = r.r0; i <= r.r1; i++)
              touch(i, col, sheet.cells.has(i, col) ? sheet.cells.getStyle(i, col) : (p.style ?? 0));
      sheet.cells.forEachInRange(r, (row, col, _v, _f, style) => touch(row, col, style));
    } else {
      for (let row = r.r0; row <= r.r1; row++)
        for (let col = r.c0; col <= r.c1; col++) touch(row, col, sheet.styleAt(row, col));
    }
  }
  if (coords.length) wb.emit({ type: 'cells', sheet: sheet.id, coords, styleOnly: true });
  const inv: Command[] = [{ type: 'setCells', sheet: sheetId, cells: [...before.values()] }];
  if (axisBefore.rows.length) {
    wb.emit({ type: 'layout', sheet: sheetId, axis: 'rows' });
    inv.push({ type: 'setAxis', sheet: sheetId, axis: 'rows', entries: axisBefore.rows });
  }
  if (axisBefore.cols.length) {
    wb.emit({ type: 'layout', sheet: sheetId, axis: 'cols' });
    inv.push({ type: 'setAxis', sheet: sheetId, axis: 'cols', entries: axisBefore.cols });
  }
  return inv.length === 1 ? (inv[0] as Command) : { type: 'batch', commands: inv };
}

// ── structural edits ──────────────────────────────────────────────────────

function structural(
  wb: Workbook,
  cmd: Extract<Command, { type: 'insertRows' | 'insertCols' | 'deleteRows' | 'deleteCols' }>,
): Command {
  const sheet = sheetOf(wb, cmd.sheet);
  const edit: StructuralEdit = { kind: cmd.type, at: cmd.at, count: cmd.count };
  const rows = isRowEdit(edit);
  const max = rows ? MAX_ROWS : MAX_COLS;
  if (!Number.isInteger(edit.count) || edit.count <= 0)
    throw new ModelError('Count must be a positive integer');
  if (!Number.isInteger(edit.at) || edit.at < 0 || edit.at >= max)
    throw new ModelError('Position out of range');
  const insert = isInsert(edit);
  if (!insert && edit.at + edit.count > max) throw new ModelError('Cannot delete past the end of the sheet');
  // Refuse edits that would push content off the sheet (Excel: "Microsoft Excel can't insert new cells
  // because it would push non-empty cells off the end of the worksheet").
  if (insert) {
    const lost: CellRange = rows
      ? { r0: max - edit.count, c0: 0, r1: max - 1, c1: MAX_COLS - 1 }
      : { r0: 0, c0: max - edit.count, r1: MAX_ROWS - 1, c1: max - 1 };
    let content = false;
    sheet.cells.forEachInRange(lost, (_r, _c, v, f) => {
      if (v !== null || f !== null) content = true;
    });
    if (content) throw new ModelError('Inserting would push non-empty cells off the end of the worksheet');
  }
  const metaBefore = allMeta(wb);
  const namesBefore = clone(wb.names);
  const layoutBefore = sheet.layoutToJSON();
  const tablesBefore = tablesKey(wb);
  // Validate table constraints before changing anything (throws on a header-row deletion).
  for (const t of sheet.tables) shiftTable(t, edit);
  const removed: CellRange = insert
    ? rows
      ? { r0: max - edit.count, c0: 0, r1: max - 1, c1: MAX_COLS - 1 }
      : { r0: 0, c0: max - edit.count, r1: MAX_ROWS - 1, c1: max - 1 }
    : rows
      ? { r0: edit.at, c0: 0, r1: edit.at + edit.count - 1, c1: MAX_COLS - 1 }
      : { r0: 0, c0: edit.at, r1: MAX_ROWS - 1, c1: edit.at + edit.count - 1 };
  const removedRecords = recordsIn(sheet, removed);
  const change = { ...edit, sheet: sheet.name };
  const formulaChanges = wb.rewriteFormulas((text, fs) => shiftFormula(text, change, fs));
  sheet.applyStructural(edit);
  // Inserted rows/columns take the format of their neighbour.
  const formatCoords: number[] = [];
  const from = insert ? ((cmd as { formatFrom?: string }).formatFrom ?? 'before') : 'none';
  if (from !== 'none') {
    const src = from === 'before' ? edit.at - 1 : edit.at + edit.count;
    if (src >= 0 && src < max) {
      if (rows) {
        const p = sheet.rows.get(src);
        for (let i = edit.at; i < edit.at + edit.count; i++)
          if (p?.style !== undefined) sheet.rows.patch(i, { style: p.style });
        const ch = sheet.cells.chunk(src);
        if (ch)
          for (let k = 0; k < ch.size; k++) {
            const st = ch.styleIdx[k] as number;
            if (st === 0) continue;
            const col = ch.cols[k] as number;
            for (let i = edit.at; i < edit.at + edit.count; i++) {
              sheet.cells.set(i, col, null, null, st);
              formatCoords.push(i, col);
            }
          }
      } else {
        const p = sheet.cols.get(src);
        for (let i = edit.at; i < edit.at + edit.count; i++)
          if (p?.style !== undefined) sheet.cols.patch(i, { style: p.style });
        const styled: [number, number][] = [];
        sheet.cells.forEachInRange({ r0: 0, c0: src, r1: MAX_ROWS - 1, c1: src }, (row, _c, _v, _f, st) => {
          if (st !== 0) styled.push([row, st]);
        });
        for (const [row, st] of styled)
          for (let i = edit.at; i < edit.at + edit.count; i++) {
            sheet.cells.set(row, i, null, null, st);
            formatCoords.push(row, i);
          }
      }
    }
  }
  wb.emit({ type: 'structure', sheet: sheet.id, edit });
  if (formatCoords.length) wb.emit({ type: 'cells', sheet: sheet.id, coords: formatCoords });
  wb.emit({ type: 'names' });
  emitTablesIfChanged(wb, tablesBefore, tablesKey(wb));
  wb.emit({ type: 'layout', sheet: sheet.id, axis: rows ? 'rows' : 'cols' });
  const opposite: Command = insert
    ? { type: rows ? 'deleteRows' : 'deleteCols', sheet: sheet.id, at: edit.at, count: edit.count }
    : {
        type: rows ? 'insertRows' : 'insertCols',
        sheet: sheet.id,
        at: edit.at,
        count: edit.count,
        formatFrom: 'none',
      };
  const records = changesToRecords(formulaChanges);
  addRecords(records, sheet.id, removedRecords);
  return {
    type: 'batch',
    commands: [
      opposite,
      {
        type: 'restore',
        cells: restoreCellsFrom(records),
        meta: metaBefore,
        layout: [{ sheet: sheet.id, layout: layoutBefore }],
        names: namesBefore,
      },
    ],
  };
}

// ── move / copy ───────────────────────────────────────────────────────────

function moveRange(wb: Workbook, cmd: Extract<Command, { type: 'moveRange' }>): Command {
  const src = sheetOf(wb, cmd.sheet);
  const dst = cmd.to.sheet === undefined ? src : sheetOf(wb, cmd.to.sheet);
  const r = cmd.range;
  checkRange(r);
  const dRow = cmd.to.row - r.r0;
  const dCol = cmd.to.col - r.c0;
  const dest: CellRange = { r0: r.r0 + dRow, c0: r.c0 + dCol, r1: r.r1 + dRow, c1: r.c1 + dCol };
  checkRange(dest);
  if (dRow === 0 && dCol === 0 && src === dst) return { type: 'batch', commands: [] };
  for (const s of [src, dst]) {
    const t = s.tables.find(
      (t) =>
        (s === src && rangesIntersect(t.range, r) && !rangeInside(t.range, r)) ||
        (s === dst && rangesIntersect(t.range, dest) && !(s === src && rangeInside(t.range, r))),
    );
    if (t) throw new ModelError(`The move would change part of table "${t.name}"`);
    const m = s.merges.find((m) => s === src && rangesIntersect(m, r) && !rangeInside(m, r));
    if (m) throw new ModelError('Cannot move part of a merged cell');
  }
  const metaBefore = allMeta(wb);
  const namesBefore = clone(wb.names);
  const srcRecords = recordsIn(src, r);
  const dstRecords = recordsIn(dst, dest);
  const spec = { sheet: src.name, ...r, dRow, dCol, toSheet: dst.name };
  const formulaChanges = wb.rewriteFormulas((text, fs) => moveFormula(text, spec, fs));
  // Read the (rewritten) block, clear source and destination, write the block.
  const block = recordsIn(src, r);
  src.cells.clearRange(r);
  dst.cells.clearRange(dest);
  for (const b of block) dst.cells.set(b.row + dRow, b.col + dCol, decodeValue(b.value), b.formula, b.style);
  // Metadata attached to cells travels with them.
  const inSrc = (row: number, col: number) => row >= r.r0 && row <= r.r1 && col >= r.c0 && col <= r.c1;
  const inDst = (row: number, col: number) =>
    row >= dest.r0 && row <= dest.r1 && col >= dest.c0 && col <= dest.c1;
  const movingNotes = src.notes.filter((n) => inSrc(n.row, n.col));
  src.notes = src.notes.filter((n) => !inSrc(n.row, n.col));
  dst.notes = dst.notes.filter((n) => !inDst(n.row, n.col));
  dst.notes.push(...movingNotes.map((n) => ({ ...n, row: n.row + dRow, col: n.col + dCol })));
  const movingThreads = src.threads.filter((n) => inSrc(n.row, n.col));
  src.threads = src.threads.filter((n) => !inSrc(n.row, n.col));
  dst.threads = dst.threads.filter((n) => !inDst(n.row, n.col));
  dst.threads.push(...movingThreads.map((n) => ({ ...n, row: n.row + dRow, col: n.col + dCol })));
  const shiftR = (x: CellRange): CellRange => ({
    r0: x.r0 + dRow,
    c0: x.c0 + dCol,
    r1: x.r1 + dRow,
    c1: x.c1 + dCol,
  });
  const movingMerges = src.merges.filter((m) => rangeInside(m, r));
  src.merges = src.merges.filter((m) => !rangeInside(m, r));
  dst.merges = dst.merges.filter((m) => !rangesIntersect(m, dest));
  dst.merges.push(...movingMerges.map(shiftR));
  const movingLinks = src.hyperlinks.filter((h) => rangeInside(h.ref, r));
  src.hyperlinks = src.hyperlinks.filter((h) => !rangeInside(h.ref, r));
  dst.hyperlinks = dst.hyperlinks.filter((h) => !rangeInside(h.ref, dest));
  dst.hyperlinks.push(...movingLinks.map((h) => ({ ...h, ref: shiftR(h.ref) })));
  const movingTables = src.tables.filter((t) => rangeInside(t.range, r));
  if (movingTables.length) {
    src.tables = src.tables.filter((t) => !rangeInside(t.range, r));
    dst.tables.push(...movingTables.map((t) => ({ ...t, range: shiftR(t.range) })));
    wb.emit({ type: 'tables' });
  }
  wb.emit({
    type: 'move',
    sheet: src.id,
    range: { ...r },
    to: { sheet: dst.id, row: cmd.to.row, col: cmd.to.col },
  });
  wb.emit({ type: 'names' });
  wb.emit({ type: 'meta', sheet: src.id, key: 'move' });
  const records = changesToRecords(formulaChanges);
  addRecords(records, src.id, withEmpties(src, r, srcRecords));
  addRecords(records, dst.id, withEmpties(dst, dest, dstRecords));
  return { type: 'restore', cells: restoreCellsFrom(records), meta: metaBefore, names: namesBefore };
}

/** `before` plus empty records for cells that exist in `r` now but did not before (so undo removes them). */
function withEmpties(sheet: Sheet, r: CellRange, before: CellRecord[]): CellRecord[] {
  const had = new Set(before.map((b) => b.row * MAX_COLS + b.col));
  const out = [...before];
  sheet.cells.forEachInRange(r, (row, col) => {
    if (!had.has(row * MAX_COLS + col)) out.push({ row, col, value: null, formula: null, style: 0 });
  });
  return out;
}

function copyRange(wb: Workbook, cmd: Extract<Command, { type: 'copyRange' }>): Command {
  const src = sheetOf(wb, cmd.sheet);
  const dst = cmd.to.sheet === undefined ? src : sheetOf(wb, cmd.to.sheet);
  const r = cmd.range;
  checkRange(r);
  checkRange(cmd.to.range);
  const what = cmd.what ?? 'all';
  const srcRows = r.r1 - r.r0 + 1;
  const srcCols = r.c1 - r.c0 + 1;
  const bh = cmd.transpose ? srcCols : srcRows; // block height at the destination
  const bw = cmd.transpose ? srcRows : srcCols;
  const t = cmd.to.range;
  const th = t.r1 - t.r0 + 1;
  const tw = t.c1 - t.c0 + 1;
  // Excel tiles the block when the destination is a whole multiple of it; otherwise pastes one copy.
  const tilesV = th % bh === 0 ? th / bh : 1;
  const tilesH = tw % bw === 0 ? tw / bw : 1;
  const dest: CellRange = { r0: t.r0, c0: t.c0, r1: t.r0 + bh * tilesV - 1, c1: t.c0 + bw * tilesH - 1 };
  checkRange(dest);
  const tbl = dst.tables.find((x) => rangesIntersect(x.range, dest) && !rangeInside(dest, x.range));
  if (tbl && (what === 'all' || what === 'formats') && src.merges.some((m) => rangeInside(m, r)))
    throw new ModelError(`Cannot paste merged cells into table "${tbl.name}"`);
  // Snapshot the source first (source and destination may overlap).
  const srcCells = new Map<number, StoredCell>();
  src.cells.forEachInRange(r, (row, col, value, formula, style) =>
    srcCells.set((row - r.r0) * srcCols + (col - r.c0), { value, formula, style }),
  );
  const srcStyle = (i: number, j: number) => src.styleAt(r.r0 + i, r.c0 + j);
  const metaBefore = dst.metaToJSON();
  const before = new Map<number, CellRecord>();
  const coords: number[] = [];
  for (let ti = 0; ti < tilesV; ti++)
    for (let tj = 0; tj < tilesH; tj++)
      for (let bi = 0; bi < bh; bi++)
        for (let bj = 0; bj < bw; bj++) {
          // Source offset (i, j) for destination offset (bi, bj).
          const i = cmd.transpose ? bj : bi;
          const j = cmd.transpose ? bi : bj;
          const row = dest.r0 + ti * bh + bi;
          const col = dest.c0 + tj * bw + bj;
          const s = srcCells.get(i * srcCols + j);
          if (cmd.skipBlanks && (!s || (s.value === null && s.formula === null))) continue;
          const prev = dst.cells.get(row, col);
          const k = row * MAX_COLS + col;
          if (!before.has(k)) before.set(k, recordOf(row, col, prev));
          let value: CellValue = prev?.value ?? null;
          let formula: string | null = prev?.formula ?? null;
          let style = prev ? prev.style : dst.styleAt(row, col);
          const sr = r.r0 + i;
          const sc = r.c0 + j;
          if (what === 'all' || what === 'formulas') {
            value = s?.value ?? null;
            formula = s?.formula ? copyFormula(s.formula, row - sr, col - sc) : null;
          } else if (what === 'values') {
            value = s?.value ?? null;
            formula = null;
          }
          if (what === 'all' || what === 'formats') style = s ? s.style : srcStyle(i, j);
          dst.cells.set(row, col, value, formula, style);
          coords.push(row, col);
        }
  let metaChanged = false;
  if (what === 'all' || what === 'formats') {
    const merges = src.merges.filter((m) => rangeInside(m, r));
    if (merges.length || dst.merges.some((m) => rangesIntersect(m, dest))) {
      dst.merges = dst.merges.filter((m) => !rangesIntersect(m, dest));
      for (let ti = 0; ti < tilesV; ti++)
        for (let tj = 0; tj < tilesH; tj++)
          for (const m of merges) {
            const oi = m.r0 - r.r0;
            const oj = m.c0 - r.c0;
            const h = m.r1 - m.r0;
            const w = m.c1 - m.c0;
            const [di, dj, dh, dw] = cmd.transpose ? [oj, oi, w, h] : [oi, oj, h, w];
            const r0 = dest.r0 + ti * bh + di;
            const c0 = dest.c0 + tj * bw + dj;
            dst.merges.push({ r0, c0, r1: r0 + dh, c1: c0 + dw });
          }
      metaChanged = true;
    }
  }
  if (what === 'all') {
    const notes = src.notes.filter((n) => n.row >= r.r0 && n.row <= r.r1 && n.col >= r.c0 && n.col <= r.c1);
    if (notes.length) {
      dst.notes = dst.notes.filter(
        (n) => !(n.row >= dest.r0 && n.row <= dest.r1 && n.col >= dest.c0 && n.col <= dest.c1),
      );
      for (let ti = 0; ti < tilesV; ti++)
        for (let tj = 0; tj < tilesH; tj++)
          for (const n of notes) {
            const oi = n.row - r.r0;
            const oj = n.col - r.c0;
            const [di, dj] = cmd.transpose ? [oj, oi] : [oi, oj];
            dst.notes.push({ ...n, row: dest.r0 + ti * bh + di, col: dest.c0 + tj * bw + dj });
          }
      metaChanged = true;
    }
  }
  if (coords.length) wb.emit({ type: 'cells', sheet: dst.id, coords });
  if (metaChanged) wb.emit({ type: 'meta', sheet: dst.id, key: 'paste' });
  return {
    type: 'restore',
    cells: [{ sheet: dst.id, records: [...before.values()] }],
    ...(metaChanged ? { meta: [{ sheet: dst.id, meta: metaBefore }] } : {}),
  };
}

// ── sort ──────────────────────────────────────────────────────────────────

function sortRange(wb: Workbook, cmd: Extract<Command, { type: 'sort' }>): Command {
  const sheet = sheetOf(wb, cmd.sheet);
  checkRange(cmd.range);
  if (cmd.keys.length === 0) throw new ModelError('Sort needs at least one key');
  const byCols = cmd.byColumns === true;
  const r = cmd.range;
  // "Lines" are the units being reordered (rows, or columns with byColumns).
  const first = (byCols ? r.c0 : r.r0) + (cmd.header ? 1 : 0);
  const last = byCols ? r.c1 : r.r1;
  if (last <= first) return { type: 'batch', commands: [] };
  for (const k of cmd.keys) {
    const lo = byCols ? r.r0 : r.c0;
    const hi = byCols ? r.r1 : r.c1;
    if (k.index < lo || k.index > hi) throw new ModelError('Sort key is outside the range');
  }
  const body: CellRange = byCols ? { ...r, c0: first } : { ...r, r0: first };
  const partial = sheet.merges.find((m) => rangesIntersect(m, body));
  if (partial) throw new ModelError('To sort, all merged cells need to be the same size');
  const valueAt = (line: number, idx: number): CellValue =>
    byCols ? sheet.cells.getValue(idx, line) : sheet.cells.getValue(line, idx);
  const lines: number[] = [];
  for (let i = first; i <= last; i++) lines.push(i);
  const cmpValues = (a: CellValue, b: CellValue, key: SortKey): number => {
    // Blanks always sort last, in either direction.
    const ab = a === null || a === '';
    const bb = b === null || b === '';
    if (ab || bb) return ab === bb ? 0 : ab ? 1 : -1;
    let c: number;
    if (key.customList) {
      const ia =
        typeof a === 'string' ? key.customList.findIndex((x) => x.toUpperCase() === a.toUpperCase()) : -1;
      const ib =
        typeof b === 'string' ? key.customList.findIndex((x) => x.toUpperCase() === b.toUpperCase()) : -1;
      if (ia >= 0 || ib >= 0) c = ia < 0 ? 1 : ib < 0 ? -1 : ia - ib;
      else c = sortCompare(a, b);
    } else if (
      cmd.caseSensitive &&
      typeof a === 'string' &&
      typeof b === 'string' &&
      a.toUpperCase() === b.toUpperCase()
    )
      // Excel's case-sensitive order puts lowercase before uppercase.
      c = a === b ? 0 : a < b ? 1 : -1;
    else c = sortCompare(a, b);
    return key.descending ? -c : c;
  };
  const order = [...lines].sort((x, y) => {
    for (const k of cmd.keys) {
      const c = cmpValues(valueAt(x, k.index), valueAt(y, k.index), k);
      if (c !== 0) return c;
    }
    return x - y; // stable
  });
  if (order.every((v, i) => v === lines[i])) return { type: 'batch', commands: [] };
  // Collect the body, then write it back reordered (formulas adjusted as if copied).
  const before = recordsIn(sheet, body);
  const byLine = new Map<number, CellRecord[]>();
  for (const rec of before) {
    const line = byCols ? rec.col : rec.row;
    let list = byLine.get(line);
    if (!list) {
      list = [];
      byLine.set(line, list);
    }
    list.push(rec);
  }
  sheet.cells.clearRange(body);
  const coords: number[] = [];
  const touched = new Map<number, CellRecord>();
  for (const rec of before) touched.set(rec.row * MAX_COLS + rec.col, rec);
  order.forEach((srcLine, i) => {
    const dstLine = first + i;
    for (const rec of byLine.get(srcLine) ?? []) {
      const row = byCols ? rec.row : dstLine;
      const col = byCols ? dstLine : rec.col;
      const formula = rec.formula ? copyFormula(rec.formula, row - rec.row, col - rec.col) : null;
      sheet.cells.set(row, col, decodeValue(rec.value), formula, rec.style);
      const k = row * MAX_COLS + col;
      if (!touched.has(k)) touched.set(k, { row, col, value: null, formula: null, style: 0 });
    }
  });
  for (const rec of touched.values()) coords.push(rec.row, rec.col);
  // Notes move with their cells.
  const meta = sheet.metaToJSON();
  const pos = new Map(order.map((srcLine, i) => [srcLine, first + i]));
  let metaChanged = false;
  sheet.notes = sheet.notes.map((n) => {
    const inBody = n.row >= body.r0 && n.row <= body.r1 && n.col >= body.c0 && n.col <= body.c1;
    if (!inBody) return n;
    const line = byCols ? n.col : n.row;
    const to = pos.get(line) ?? line;
    if (to === line) return n;
    metaChanged = true;
    return byCols ? { ...n, col: to } : { ...n, row: to };
  });
  wb.emit({ type: 'cells', sheet: sheet.id, coords });
  if (metaChanged) wb.emit({ type: 'meta', sheet: sheet.id, key: 'notes' });
  return {
    type: 'restore',
    cells: [{ sheet: sheet.id, records: [...touched.values()] }],
    ...(metaChanged ? { meta: [{ sheet: sheet.id, meta }] } : {}),
  };
}

// ── sheets ────────────────────────────────────────────────────────────────

function addSheet(wb: Workbook, cmd: Extract<Command, { type: 'addSheet' }>): Command {
  const sheet = wb.addSheet(cmd.name ?? wb.nextSheetName(), cmd.index ?? wb.sheets.length, cmd.id);
  wb.emit({ type: 'sheetAdded', sheet: sheet.id });
  // Formulas that referred to this name while it did not exist now bind; removing the sheet again
  // would turn them into #REF!, so the inverse restores their text afterwards.
  const pending = wb.rewriteFormulas((t) => removeSheetInFormula(t, sheet.name), true);
  const records = changesToRecords(pending.filter((c) => c.sheet !== sheet.id));
  return {
    type: 'batch',
    commands: [
      { type: 'removeSheet', sheet: sheet.id },
      {
        type: 'restore',
        cells: restoreCellsFrom(records),
        meta: allMeta(wb).filter((m) => m.sheet !== sheet.id),
        names: clone(wb.names),
      },
    ],
  };
}

function removeSheet(wb: Workbook, id: number): Command {
  const sheet = sheetOf(wb, id);
  const visible = wb.sheets.filter((s) => s.visibility === 'visible');
  if (wb.sheets.length === 1) throw new ModelError('A workbook must contain at least one sheet');
  if (sheet.visibility === 'visible' && visible.length === 1)
    throw new ModelError('A workbook must contain at least one visible sheet');
  const index = wb.sheetIndex(id);
  const snapshot = snapshotSheet(sheet);
  const metaBefore = allMeta(wb).filter((m) => m.sheet !== id);
  const namesBefore = clone(wb.names);
  const propsBefore = clone(wb.props);
  const tablesBefore = tablesKey(wb);
  // Formulas elsewhere that use the sheet's tables keep working as A1 references would not: Excel
  // turns them into #REF!, which the engine does for unknown tables anyway.
  const changes = wb
    .rewriteFormulas((t) => removeSheetInFormula(t, sheet.name))
    .filter((c) => c.sheet !== id);
  wb.sheets.splice(index, 1);
  wb.names = wb.names.filter((n) => n.scope !== id);
  if (wb.props.activeSheet >= wb.sheets.length) wb.props = { ...wb.props, activeSheet: wb.sheets.length - 1 };
  wb.emit({ type: 'sheetRemoved', sheet: id, name: sheet.name });
  wb.emit({ type: 'names' });
  emitTablesIfChanged(wb, tablesBefore, tablesKey(wb));
  return {
    type: 'batch',
    commands: [
      { type: 'restoreSheet', snapshot, index },
      {
        type: 'restore',
        cells: restoreCellsFrom(changesToRecords(changes)),
        meta: metaBefore,
        names: namesBefore,
      },
      { type: 'setProps', patch: { activeSheet: propsBefore.activeSheet } },
    ],
  };
}

function restoreSheetCmd(wb: Workbook, cmd: Extract<Command, { type: 'restoreSheet' }>): Command {
  const sheet = restoreSheet(cmd.snapshot, wb.maxDigitWidth);
  wb.insertSheetObject(sheet, cmd.index);
  wb.emit({ type: 'sheetAdded', sheet: sheet.id });
  if (sheet.tables.length) wb.emit({ type: 'tables' });
  return { type: 'removeSheet', sheet: sheet.id };
}

function renameSheet(wb: Workbook, id: number, name: string): Command {
  const sheet = sheetOf(wb, id);
  const old = sheet.name;
  if (old === name) return { type: 'batch', commands: [] };
  const clash = wb.sheetByName(name);
  if (clash && clash !== sheet) throw new ModelError(`A sheet named "${name}" already exists`);
  const metaBefore = allMeta(wb);
  const namesBefore = clone(wb.names);
  checkSheetName(name);
  const changes = wb.rewriteFormulas((t) => renameSheetInFormula(t, old, name));
  for (const s of wb.sheets)
    for (const h of s.hyperlinks)
      if (h.location) {
        try {
          h.location = renameSheetInFormula(h.location, old, name);
        } catch {
          // Not a reference (e.g. a defined name): keep.
        }
      }
  sheet.name = name;
  wb.emit({ type: 'sheetRenamed', sheet: id, from: old, to: name });
  wb.emit({ type: 'names' });
  return {
    type: 'batch',
    commands: [
      { type: 'renameSheet', sheet: id, name: old },
      {
        type: 'restore',
        cells: restoreCellsFrom(changesToRecords(changes)),
        meta: metaBefore,
        names: namesBefore,
      },
    ],
  };
}

function moveSheet(wb: Workbook, id: number, index: number): Command {
  const from = wb.sheetIndex(id);
  if (from < 0) throw new ModelError(`No sheet with id ${id}`);
  const to = Math.max(0, Math.min(index, wb.sheets.length - 1));
  if (from === to) return { type: 'batch', commands: [] };
  const [sheet] = wb.sheets.splice(from, 1);
  wb.sheets.splice(to, 0, sheet as Sheet);
  wb.emit({ type: 'sheetMoved', sheet: id, index: to });
  return { type: 'moveSheet', sheet: id, index: from };
}

// ── rows/columns ──────────────────────────────────────────────────────────

function setAxisProps(
  wb: Workbook,
  sheetId: number,
  axis: 'rows' | 'cols',
  at: number,
  count: number,
  patch: NullablePatch<RowProps> | NullablePatch<ColProps>,
): Command {
  const sheet = sheetOf(wb, sheetId);
  const max = axis === 'rows' ? MAX_ROWS : MAX_COLS;
  if (!Number.isInteger(at) || !Number.isInteger(count) || at < 0 || count <= 0 || at + count > max)
    throw new ModelError('Row/column span out of range');
  const p = patch as Record<string, unknown>;
  if (axis === 'rows' && typeof p.height === 'number' && !(p.height >= 0 && p.height <= 409))
    throw new ModelError('Row height must be between 0 and 409 points');
  if (axis === 'cols' && typeof p.width === 'number' && !(p.width >= 0 && p.width <= 255))
    throw new ModelError('Column width must be between 0 and 255 characters');
  if (typeof p.outlineLevel === 'number' && !(p.outlineLevel >= 0 && p.outlineLevel <= 7))
    throw new ModelError('Outline level must be between 0 and 7');
  if (typeof p.style === 'number') checkStyle(wb, p.style);
  const ax = axis === 'rows' ? sheet.rows : sheet.cols;
  const entries: [number, RowProps | ColProps | null][] = [];
  for (let i = at; i < at + count; i++) {
    const prev = ax.get(i);
    entries.push([i, prev ? { ...prev } : null]);
    const next: Record<string, unknown> = { ...(prev ?? {}) };
    for (const [k, v] of Object.entries(p)) {
      if (v === null || v === undefined || v === false) delete next[k];
      else next[k] = v;
    }
    (ax as { set(i: number, p: unknown): void }).set(i, next);
  }
  wb.emit({ type: 'layout', sheet: sheetId, axis });
  return { type: 'setAxis', sheet: sheetId, axis, entries };
}

function setAxisEntries(
  wb: Workbook,
  sheetId: number,
  axis: 'rows' | 'cols',
  entries: readonly [number, RowProps | ColProps | null][],
): Command {
  const sheet = sheetOf(wb, sheetId);
  const ax = axis === 'rows' ? sheet.rows : sheet.cols;
  const inv: [number, RowProps | ColProps | null][] = [];
  for (const [i, p] of entries) {
    const prev = ax.get(i);
    inv.push([i, prev ? { ...prev } : null]);
    (ax as { set(i: number, p: unknown): void }).set(i, p ? { ...p } : undefined);
  }
  wb.emit({ type: 'layout', sheet: sheetId, axis });
  return { type: 'setAxis', sheet: sheetId, axis, entries: inv };
}

// ── merges ────────────────────────────────────────────────────────────────

function mergeCells(wb: Workbook, cmd: Extract<Command, { type: 'merge' }>): Command {
  const sheet = sheetOf(wb, cmd.sheet);
  const r = cmd.range;
  checkRange(r);
  const mode = cmd.mode ?? 'merge';
  const targets: CellRange[] =
    mode === 'across'
      ? Array.from({ length: r.r1 - r.r0 + 1 }, (_, i) => ({ ...r, r0: r.r0 + i, r1: r.r0 + i }))
      : [r];
  // Merges fully inside the range are absorbed; partial overlaps are refused.
  for (const m of sheet.merges)
    if (rangesIntersect(m, r) && !rangeInside(m, r))
      throw new ModelError('Cannot merge: the range partly overlaps merged cells');
  const table = sheet.tables.find((t) => rangesIntersect(t.range, r));
  if (table) throw new ModelError(`Cells in table "${table.name}" cannot be merged`);
  const meta = sheet.metaToJSON();
  sheet.merges = sheet.merges.filter((m) => !rangeInside(m, r));
  const before = new Map<number, CellRecord>();
  const remember = (row: number, col: number) => {
    const k = row * MAX_COLS + col;
    if (!before.has(k)) before.set(k, recordOf(row, col, sheet.cells.get(row, col)));
  };
  const coords: number[] = [];
  for (const t of targets) {
    if (t.r0 === t.r1 && t.c0 === t.c1) continue;
    // Keep only the upper-left-most content (Excel's rule), moved into the top-left cell.
    let keep: CellRecord | undefined;
    const cells: CellRecord[] = [];
    sheet.cells.forEachInRange(t, (row, col, value, formula, style) => {
      const rec = { row, col, value: encodeValue(value), formula, style };
      cells.push(rec);
      if (!keep && (value !== null || formula !== null)) keep = rec;
    });
    remember(t.r0, t.c0);
    for (const c of cells) {
      remember(c.row, c.col);
      if (c.row === t.r0 && c.col === t.c0) continue;
      sheet.cells.set(c.row, c.col, null, null, c.style);
      coords.push(c.row, c.col);
    }
    const k = keep as CellRecord | undefined;
    if (k && (k.row !== t.r0 || k.col !== t.c0)) {
      const formula = k.formula ? copyFormula(k.formula, t.r0 - k.row, t.c0 - k.col) : null;
      sheet.cells.set(t.r0, t.c0, decodeValue(k.value), formula, sheet.styleAt(t.r0, t.c0));
      coords.push(t.r0, t.c0);
    }
    sheet.merges.push({ ...t });
  }
  if (mode === 'center') {
    const cache = new Map<number, number>();
    for (let row = r.r0; row <= r.r1; row++)
      for (let col = r.c0; col <= r.c1; col++) {
        remember(row, col);
        const c = sheet.cells.get(row, col);
        const base = c ? c.style : sheet.styleAt(row, col);
        let st = cache.get(base);
        if (st === undefined) {
          st = wb.styles.patch(base, { alignment: { horizontal: 'center' } });
          cache.set(base, st);
        }
        sheet.cells.set(row, col, c?.value ?? null, c?.formula ?? null, st);
        coords.push(row, col);
      }
  }
  if (coords.length) wb.emit({ type: 'cells', sheet: sheet.id, coords });
  wb.emit({ type: 'meta', sheet: sheet.id, key: 'merges' });
  return {
    type: 'restore',
    cells: [{ sheet: sheet.id, records: [...before.values()] }],
    meta: [{ sheet: sheet.id, meta }],
  };
}

function unmergeCells(wb: Workbook, cmd: Extract<Command, { type: 'unmerge' }>): Command {
  const sheet = sheetOf(wb, cmd.sheet);
  checkRange(cmd.range);
  const meta = sheet.metaToJSON();
  const removed = sheet.removeMerges(cmd.range);
  if (removed.length === 0) return { type: 'batch', commands: [] };
  wb.emit({ type: 'meta', sheet: sheet.id, key: 'merges' });
  return { type: 'restore', meta: [{ sheet: sheet.id, meta }] };
}

// ── names ─────────────────────────────────────────────────────────────────

function defineName(wb: Workbook, cmd: Extract<Command, { type: 'defineName' }>): Command {
  wb.checkDefinedName(cmd.name, cmd.scope);
  const formula = normFormula(cmd.formula);
  const before = clone(wb.names);
  const def: DefinedName = { name: cmd.name, formula, scope: cmd.scope };
  if (cmd.hidden) def.hidden = true;
  if (cmd.comment) def.comment = cmd.comment;
  const i = wb.names.findIndex(
    (n) => n.scope === cmd.scope && n.name.toUpperCase() === cmd.name.toUpperCase(),
  );
  if (i >= 0) wb.names[i] = def;
  else wb.names.push(def);
  wb.emit({ type: 'names' });
  return { type: 'restore', names: before };
}

function deleteName(wb: Workbook, name: string, scope: number | null): Command {
  const before = clone(wb.names);
  const n0 = wb.names.length;
  wb.names = wb.names.filter((n) => !(n.scope === scope && n.name.toUpperCase() === name.toUpperCase()));
  if (wb.names.length === n0) throw new ModelError(`No name "${name}" in that scope`);
  wb.emit({ type: 'names' });
  return { type: 'restore', names: before };
}

// ── tables ────────────────────────────────────────────────────────────────

function addTable(wb: Workbook, cmd: Extract<Command, { type: 'addTable' }>): Command {
  const sheet = sheetOf(wb, cmd.sheet);
  checkRange(cmd.table.range);
  const table: Table = clone({ ...cmd.table, id: cmd.table.id ?? wb.allocTableId() }) as Table;
  if (wb.sheets.some((x) => x.tables.some((t) => t.id === table.id)))
    throw new ModelError(`Table id ${table.id} is already used`);
  wb.setIdCounters({ ...wb.idCounters(), table: Math.max(wb.idCounters().table, table.id + 1) });
  wb.checkTable(sheet, table);
  const meta = sheet.metaToJSON();
  const before: CellRecord[] = [];
  const coords: number[] = [];
  if (table.headerRow) {
    // Header cells show the column names.
    table.columns.forEach((c, i) => {
      const row = table.range.r0;
      const col = table.range.c0 + i;
      const prev = sheet.cells.get(row, col);
      if (prev?.value === c.name && prev.formula === null) return;
      before.push(recordOf(row, col, prev));
      sheet.cells.set(row, col, c.name, null, prev ? prev.style : sheet.styleAt(row, col));
      coords.push(row, col);
    });
  }
  sheet.tables.push(table);
  if (coords.length) wb.emit({ type: 'cells', sheet: sheet.id, coords });
  wb.emit({ type: 'tables' });
  return {
    type: 'restore',
    cells: [{ sheet: sheet.id, records: before }],
    meta: [{ sheet: sheet.id, meta }],
  };
}

/** Convert to Range: removes the table; structured references to it become A1 references. */
function removeTable(wb: Workbook, name: string): Command {
  const found = wb.findTable(name);
  if (!found) throw new ModelError(`No table named "${name}"`);
  const { sheet, table } = found;
  const metaBefore = allMeta(wb);
  const namesBefore = clone(wb.names);
  const changes = wb.rewriteFormulas((text, fs, at) => structuredToA1(text, table, sheet.name, fs, at));
  sheet.tables = sheet.tables.filter((t) => t !== table);
  if (sheet.autoFilter && sheet.autoFilter.range.r0 === table.range.r0) {
    // nothing: the table's own filter is part of the table
  }
  emitFormulaChanges(wb, changes);
  wb.emit({ type: 'tables' });
  wb.emit({ type: 'names' });
  return {
    type: 'restore',
    cells: restoreCellsFrom(changesToRecords(changes)),
    meta: metaBefore,
    names: namesBefore,
  };
}

function updateTable(wb: Workbook, name: string, patch: TablePatch): Command {
  const found = wb.findTable(name);
  if (!found) throw new ModelError(`No table named "${name}"`);
  const { sheet, table } = found;
  const next: Table = clone({ ...table, ...patch }) as Table;
  if (patch.range) {
    checkRange(patch.range);
    // Resizing keeps names of surviving columns and invents names for new ones.
    const width = patch.range.c1 - patch.range.c0 + 1;
    if (!patch.columns) {
      const cols = table.columns.slice(0, width);
      const extra = width - cols.length;
      if (extra > 0) {
        const used = new Set(cols.map((c) => c.name.toUpperCase()));
        for (let k = 1; cols.length < width; k++) {
          const n = `Column${k}`;
          if (!used.has(n.toUpperCase())) cols.push({ name: n });
        }
      }
      next.columns = cols;
    }
    if (patch.range.r0 !== table.range.r0 || patch.range.c0 !== table.range.c0)
      throw new ModelError('A table can only be resized from its bottom-right corner');
  }
  wb.checkTable(sheet, next, table.name);
  const metaBefore = allMeta(wb);
  const namesBefore = clone(wb.names);
  const changes: FormulaChange[] = [];
  if (next.name !== table.name)
    changes.push(...wb.rewriteFormulas((t) => renameTableInFormula(t, table.name, next.name)));
  const before: CellRecord[] = [];
  const coords: number[] = [];
  const idx = sheet.tables.indexOf(table);
  sheet.tables[idx] = next;
  if (next.headerRow) {
    next.columns.forEach((c, i) => {
      const old = table.columns[i];
      if (old && old.name !== c.name && i < table.columns.length)
        changes.push(
          ...wb.rewriteFormulas((t, fs, at) =>
            renameTableColumnInFormula(
              t,
              next.name,
              old.name,
              c.name,
              fs.toUpperCase() === sheet.name.toUpperCase() &&
                at !== undefined &&
                at.row >= next.range.r0 &&
                at.row <= next.range.r1 &&
                at.col >= next.range.c0 &&
                at.col <= next.range.c1,
            ),
          ),
        );
      const row = next.range.r0;
      const col = next.range.c0 + i;
      const prev = sheet.cells.get(row, col);
      if (prev?.value === c.name && prev.formula === null) return;
      before.push(recordOf(row, col, prev));
      sheet.cells.set(row, col, c.name, null, prev ? prev.style : sheet.styleAt(row, col));
      coords.push(row, col);
    });
  }
  if (coords.length) wb.emit({ type: 'cells', sheet: sheet.id, coords });
  emitFormulaChanges(wb, changes);
  wb.emit({ type: 'tables' });
  wb.emit({ type: 'names' });
  const records = changesToRecords(changes);
  addRecords(records, sheet.id, before);
  return { type: 'restore', cells: restoreCellsFrom(records), meta: metaBefore, names: namesBefore };
}

// ── metadata / properties ─────────────────────────────────────────────────

const META_KEYS: readonly SheetMetaKey[] = [
  'cf',
  'validations',
  'hyperlinks',
  'notes',
  'threads',
  'view',
  'print',
  'autoFilter',
  'charts',
  'drawings',
  'visibility',
  'tabColor',
  'outline',
  'defaultRowHeight',
  'defaultColWidth',
];

const isInt = (v: unknown): v is number => Number.isInteger(v) && (v as number) >= 0;
const isCell = (o: { row?: unknown; col?: unknown }) =>
  isInt(o.row) && isInt(o.col) && (o.row as number) < MAX_ROWS && (o.col as number) < MAX_COLS;
const isRange = (r: unknown): boolean => {
  if (!r || typeof r !== 'object') return false;
  const x = r as CellRange;
  return (
    isInt(x.r0) &&
    isInt(x.c0) &&
    isInt(x.r1) &&
    isInt(x.c1) &&
    x.r0 <= x.r1 &&
    x.c0 <= x.c1 &&
    x.r1 < MAX_ROWS &&
    x.c1 < MAX_COLS
  );
};

/** Structural validation of metadata set through `setSheetMeta` (the shapes later code relies on). */
function validateMeta(key: SheetMetaKey, value: unknown): void {
  const bad = (what: string): never => {
    throw new ModelError(`Invalid ${what}`);
  };
  const list = (v: unknown): unknown[] => (Array.isArray(v) ? v : bad(`${key}: expected a list`));
  const obj = (v: unknown) => (v && typeof v === 'object' ? (v as Record<string, unknown>) : bad(key));
  switch (key) {
    case 'cf':
      for (const b of list(value)) {
        const o = obj(b);
        if (!Array.isArray(o.ranges) || o.ranges.length === 0 || !o.ranges.every(isRange))
          bad('conditional format ranges');
        for (const r of list(o.rules)) {
          const rule = obj(r);
          if (typeof rule.type !== 'string' || !Number.isFinite(rule.priority))
            bad('conditional format rule');
        }
      }
      break;
    case 'validations':
      for (const v of list(value)) {
        const o = obj(v);
        if (!Array.isArray(o.ranges) || o.ranges.length === 0 || !o.ranges.every(isRange))
          bad('validation ranges');
        if (typeof o.type !== 'string') bad('validation type');
      }
      break;
    case 'hyperlinks':
      for (const h of list(value)) {
        const o = obj(h);
        if (!isRange(o.ref) || (o.target === undefined && o.location === undefined)) bad('hyperlink');
      }
      break;
    case 'notes':
    case 'threads':
      for (const n of list(value)) if (!isCell(obj(n)) || typeof obj(n).text !== 'string') bad(key);
      break;
    case 'charts':
    case 'drawings':
      for (const d of list(value)) {
        const o = obj(d);
        if (typeof o.id !== 'string' || !o.anchor || typeof o.anchor !== 'object') bad('drawing anchor');
      }
      break;
    case 'view': {
      const v = obj(value) as unknown as SheetMetaJSON['view'];
      if (!(v.zoom >= 10 && v.zoom <= 400)) throw new ModelError('Zoom must be between 10% and 400%');
      if (!v.selection || !Array.isArray(v.selection.ranges) || !v.selection.ranges.every(isRange))
        bad('selection');
      if (v.freeze && !(isInt(v.freeze.rows) && isInt(v.freeze.cols))) bad('freeze panes');
      break;
    }
    case 'print': {
      const p = obj(value) as unknown as SheetMetaJSON['print'];
      if (!(p.scale >= 10 && p.scale <= 400)) bad('print scale (10–400%)');
      if (!Array.isArray(p.rowBreaks) || !Array.isArray(p.colBreaks)) bad('page breaks');
      break;
    }
    case 'autoFilter':
      if (value !== null && !isRange(obj(value).range)) bad('AutoFilter range');
      break;
    case 'visibility':
      if (value !== 'visible' && value !== 'hidden' && value !== 'veryHidden') bad('visibility');
      break;
    case 'defaultRowHeight':
      if (typeof value !== 'number' || !(value > 0 && value <= 409)) bad('default row height');
      break;
    case 'defaultColWidth':
      if (typeof value !== 'number' || !(value >= 0 && value <= 255)) bad('default column width');
      break;
    case 'outline':
      obj(value);
      break;
    case 'tabColor':
      if (value !== null) obj(value);
      break;
  }
}

function setSheetMeta(wb: Workbook, sheetId: number, key: SheetMetaKey, value: unknown): Command {
  const sheet = sheetOf(wb, sheetId);
  if (!META_KEYS.includes(key)) throw new ModelError(`"${String(key)}" cannot be set directly`);
  if (key === 'visibility' && value !== 'visible') {
    const others = wb.sheets.filter((s) => s !== sheet && s.visibility === 'visible');
    if (others.length === 0) throw new ModelError('A workbook must contain at least one visible sheet');
  }
  validateMeta(key, value);
  const meta = sheet.metaToJSON();
  const prev = clone(meta[key]);
  sheet.loadMeta({ ...meta, [key]: clone(value) } as SheetMetaJSON);
  wb.emit({ type: 'meta', sheet: sheetId, key });
  return { type: 'setSheetMeta', sheet: sheetId, key, value: prev };
}

function setProps(wb: Workbook, patch: PropsPatch): Command {
  const prev = clone(wb.props);
  const next: WorkbookProps = { ...wb.props, ...patch, calc: { ...wb.props.calc, ...(patch.calc ?? {}) } };
  if (
    !Number.isInteger(next.activeSheet) ||
    next.activeSheet < 0 ||
    next.activeSheet >= Math.max(1, wb.sheets.length)
  )
    throw new ModelError('Active sheet index out of range');
  wb.props = next;
  wb.emit({ type: 'props' });
  return { type: 'setProps', patch: prev };
}

// ── restore ───────────────────────────────────────────────────────────────

function restore(wb: Workbook, cmd: Extract<Command, { type: 'restore' }>): Command {
  const inv: Extract<Command, { type: 'restore' }> = { type: 'restore' };
  const tablesBefore = tablesKey(wb);
  if (cmd.meta) {
    inv.meta = [];
    for (const m of cmd.meta) {
      const sheet = wb.sheetById(m.sheet);
      if (!sheet) continue;
      inv.meta.push({ sheet: m.sheet, meta: sheet.metaToJSON() });
      sheet.loadMeta(m.meta);
      wb.emit({ type: 'meta', sheet: m.sheet, key: 'restore' });
    }
  }
  if (cmd.layout) {
    inv.layout = [];
    for (const l of cmd.layout) {
      const sheet = wb.sheetById(l.sheet);
      if (!sheet) continue;
      inv.layout.push({ sheet: l.sheet, layout: sheet.layoutToJSON() });
      sheet.loadLayout(l.layout);
      wb.emit({ type: 'layout', sheet: l.sheet, axis: 'rows' });
      wb.emit({ type: 'layout', sheet: l.sheet, axis: 'cols' });
    }
  }
  if (cmd.names) {
    inv.names = clone(wb.names);
    wb.names = clone(cmd.names);
    wb.emit({ type: 'names' });
  }
  emitTablesIfChanged(wb, tablesBefore, tablesKey(wb));
  if (cmd.cells) {
    inv.cells = [];
    for (const c of cmd.cells) {
      const sheet = wb.sheetById(c.sheet);
      if (!sheet) continue;
      inv.cells.push({
        sheet: c.sheet,
        records: c.records.map((r) => recordOf(r.row, r.col, sheet.cells.get(r.row, r.col))),
      });
      writeRecords(wb, sheet, c.records);
    }
  }
  return inv;
}

// ── serialisation ─────────────────────────────────────────────────────────

const COMMAND_TYPES = new Set<CommandType>([
  'setCells',
  'setRange',
  'clearRange',
  'setStyle',
  'setStyleIndex',
  'insertRows',
  'insertCols',
  'deleteRows',
  'deleteCols',
  'moveRange',
  'copyRange',
  'sort',
  'addSheet',
  'removeSheet',
  'restoreSheet',
  'renameSheet',
  'moveSheet',
  'setRowProps',
  'setColProps',
  'setAxis',
  'merge',
  'unmerge',
  'defineName',
  'deleteName',
  'addTable',
  'removeTable',
  'updateTable',
  'setFreeze',
  'setSheetMeta',
  'setProps',
  'batch',
  'restore',
]);

/** Serialises a command to JSON text (commands are plain data). */
export function serializeCommand(cmd: Command): string {
  return JSON.stringify(cmd);
}

/** Parses a serialised command, checking that it (and nested batch members) are known commands. */
export function parseCommand(text: string): Command {
  const v = JSON.parse(text) as unknown;
  const check = (c: unknown): Command => {
    if (!c || typeof c !== 'object' || !COMMAND_TYPES.has((c as { type: CommandType }).type))
      throw new ModelError('Not a sheet-model command');
    if ((c as Command).type === 'batch') (c as { commands: unknown[] }).commands.forEach(check);
    return c as Command;
  };
  return check(v);
}
