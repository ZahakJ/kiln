/**
 * A worksheet: the sparse cell store, row/column properties and all per-sheet metadata (merges,
 * conditional formatting, validation, tables, hyperlinks, notes, views, print settings, AutoFilter,
 * drawing anchors).
 *
 * The methods here are *low-level*: they validate and mutate but record no undo information and do
 * not notify the calculation layer. Interactive edits go through commands (`Workbook.apply` /
 * `CommandLog`); file readers use these methods directly to build a workbook.
 */
import {
  type CellRange,
  checkRange,
  formatRange,
  ModelError,
  rangeContains,
  rangeInside,
  rangesIntersect,
} from './address.ts';
import {
  type Axis,
  BASE_COL_WIDTH,
  type ColProps,
  createColAxis,
  createRowAxis,
  DEFAULT_ROW_HEIGHT_PT,
  defaultColWidthFor,
  type RowProps,
} from './axis.ts';
import {
  type AutoFilter,
  type ConditionalFormat,
  type DataValidation,
  type DrawingAnchor,
  defaultPrintSettings,
  defaultSheetView,
  type Hyperlink,
  type Note,
  type PrintSettings,
  type SheetView,
  type SheetVisibility,
  type Table,
  type ThreadedComment,
} from './meta.ts';
import {
  isRowEdit,
  type StructuralEdit,
  shiftAutoFilter,
  shiftBreaks,
  shiftCell,
  shiftDrawing,
  shiftFrozenCount,
  shiftMerges,
  shiftRange,
  shiftRanges,
  shiftTable,
} from './shift.ts';
import { SparseStore } from './store.ts';
import type { Color } from './styles.ts';

/** Every piece of per-sheet metadata, as plain JSON (cells and row/column properties excluded). */
export interface SheetMetaJSON {
  merges: CellRange[];
  cf: ConditionalFormat[];
  validations: DataValidation[];
  tables: Table[];
  hyperlinks: Hyperlink[];
  notes: Note[];
  threads: ThreadedComment[];
  view: SheetView;
  print: PrintSettings;
  autoFilter: AutoFilter | null;
  charts: DrawingAnchor[];
  drawings: DrawingAnchor[];
  visibility: SheetVisibility;
  tabColor: Color | null;
  outline: { summaryBelow: boolean; summaryRight: boolean };
  /** Default row height in points. */
  defaultRowHeight: number;
  /** Default column width in SpreadsheetML character units (padding included). */
  defaultColWidth: number;
}

export interface SheetLayoutJSON {
  rows: [number, RowProps][];
  cols: [number, ColProps][];
}

const clone = <T>(v: T): T => structuredClone(v);

export class Sheet {
  readonly cells = new SparseStore();
  rows: Axis<RowProps>;
  cols: Axis<ColProps>;

  merges: CellRange[] = [];
  cf: ConditionalFormat[] = [];
  validations: DataValidation[] = [];
  tables: Table[] = [];
  hyperlinks: Hyperlink[] = [];
  notes: Note[] = [];
  threads: ThreadedComment[] = [];
  view: SheetView = defaultSheetView();
  print: PrintSettings = defaultPrintSettings();
  autoFilter: AutoFilter | null = null;
  charts: DrawingAnchor[] = [];
  drawings: DrawingAnchor[] = [];
  visibility: SheetVisibility = 'visible';
  tabColor: Color | null = null;
  outline = { summaryBelow: true, summaryRight: true };
  private defaultRowHeightPt = DEFAULT_ROW_HEIGHT_PT;
  private defaultColWidthValue: number;
  /** False while the default column width is derived from the font (no `defaultColWidth` set). */
  private customDefaultWidth = false;

  constructor(
    readonly id: number,
    public name: string,
    private mdw = 7,
  ) {
    this.defaultColWidthValue = defaultColWidthFor(BASE_COL_WIDTH, mdw);
    this.rows = createRowAxis(this.defaultRowHeightPt);
    this.cols = createColAxis(this.defaultColWidthValue, mdw);
  }

  // ── defaults ───────────────────────────────────────────────────────────

  get defaultRowHeight(): number {
    return this.defaultRowHeightPt;
  }

  set defaultRowHeight(pt: number) {
    if (!(pt > 0 && pt <= 409)) throw new ModelError('Row height must be between 0 and 409 points');
    this.defaultRowHeightPt = pt;
    const entries = this.rows.toJSON();
    this.rows = createRowAxis(pt);
    this.rows.load(entries);
  }

  get defaultColWidth(): number {
    return this.defaultColWidthValue;
  }

  set defaultColWidth(width: number) {
    if (!(width >= 0 && width <= 255)) throw new ModelError('Column width must be between 0 and 255');
    this.defaultColWidthValue = width;
    this.customDefaultWidth = true;
    this.rebuildColAxis();
  }

  /** Maximum digit width of the workbook default font (px), used for column widths. */
  get maxDigitWidth(): number {
    return this.mdw;
  }

  set maxDigitWidth(mdw: number) {
    this.mdw = mdw;
    if (!this.customDefaultWidth) this.defaultColWidthValue = defaultColWidthFor(BASE_COL_WIDTH, mdw);
    this.rebuildColAxis();
  }

  private rebuildColAxis(): void {
    const entries = this.cols.toJSON();
    this.cols = createColAxis(this.defaultColWidthValue, this.mdw);
    this.cols.load(entries);
  }

  // ── merges ─────────────────────────────────────────────────────────────

  /** The merged region containing a cell, if any. */
  mergeAt(row: number, col: number): CellRange | undefined {
    return this.merges.find((m) => rangeContains(m, row, col));
  }

  /** Merged regions intersecting a range. */
  mergesIn(r: CellRange): CellRange[] {
    return this.merges.filter((m) => rangesIntersect(m, r));
  }

  /** Validates a new merge: at least two cells, no overlap with other merges, not inside a table. */
  checkMerge(r: CellRange): void {
    checkRange(r);
    if (r.r0 === r.r1 && r.c0 === r.c1) throw new ModelError('A merged region needs at least two cells');
    const clash = this.merges.find((m) => rangesIntersect(m, r));
    if (clash) throw new ModelError(`${formatRange(r)} overlaps the merged cells ${formatRange(clash)}`);
    const table = this.tables.find((t) => rangesIntersect(t.range, r));
    if (table) throw new ModelError(`Cells in table "${table.name}" cannot be merged`);
  }

  addMerge(r: CellRange): void {
    this.checkMerge(r);
    this.merges.push({ r0: r.r0, c0: r.c0, r1: r.r1, c1: r.c1 });
  }

  /** Removes (splits) every merge intersecting the range; returns the removed regions. */
  removeMerges(r: CellRange): CellRange[] {
    const removed = this.merges.filter((m) => rangesIntersect(m, r));
    if (removed.length) this.merges = this.merges.filter((m) => !rangesIntersect(m, r));
    return removed;
  }

  /** Expands a range so that it fully contains every merge it touches (Excel's selection rule). */
  expandToMerges(r: CellRange): CellRange {
    const out = { ...r };
    let changed = true;
    while (changed) {
      changed = false;
      for (const m of this.merges) {
        if (!rangesIntersect(m, out) || rangeInside(m, out)) continue;
        out.r0 = Math.min(out.r0, m.r0);
        out.c0 = Math.min(out.c0, m.c0);
        out.r1 = Math.max(out.r1, m.r1);
        out.c1 = Math.max(out.c1, m.c1);
        changed = true;
      }
    }
    return out;
  }

  // ── tables ─────────────────────────────────────────────────────────────

  tableAt(row: number, col: number): Table | undefined {
    return this.tables.find((t) => rangeContains(t.range, row, col));
  }

  // ── notes and links ────────────────────────────────────────────────────

  noteAt(row: number, col: number): Note | undefined {
    return this.notes.find((n) => n.row === row && n.col === col);
  }

  hyperlinkAt(row: number, col: number): Hyperlink | undefined {
    return this.hyperlinks.find((h) => rangeContains(h.ref, row, col));
  }

  threadsAt(row: number, col: number): ThreadedComment[] {
    return this.threads.filter((t) => t.row === row && t.col === col);
  }

  /** Validation rule applying to a cell (the last one listed wins, as in Excel). */
  validationAt(row: number, col: number): DataValidation | undefined {
    for (let i = this.validations.length - 1; i >= 0; i--) {
      const v = this.validations[i] as DataValidation;
      if (v.ranges.some((r) => rangeContains(r, row, col))) return v;
    }
    return undefined;
  }

  /** Conditional-format rules applying to a cell, by ascending priority. */
  cfRulesAt(row: number, col: number): ConditionalFormat['rules'] {
    const out: ConditionalFormat['rules'] = [];
    for (const block of this.cf) {
      if (block.ranges.some((r) => rangeContains(r, row, col))) out.push(...block.rules);
    }
    return out.sort((a, b) => a.priority - b.priority);
  }

  // ── row/column helpers ─────────────────────────────────────────────────

  isRowHidden(row: number): boolean {
    return this.rows.isHidden(row);
  }

  isRowFiltered(row: number): boolean {
    return this.rows.get(row)?.filtered === true;
  }

  isColHidden(col: number): boolean {
    return this.cols.isHidden(col);
  }

  /** Effective style index of a cell: its own, else the row format, else the column format. */
  styleAt(row: number, col: number): number {
    if (this.cells.has(row, col)) return this.cells.getStyle(row, col);
    return this.rows.get(row)?.style ?? this.cols.get(col)?.style ?? 0;
  }

  // ── structural edits (cells, axes and metadata; formulas are rewritten by the workbook) ──

  /**
   * Applies an insert/delete to this sheet's cells, row/column properties and metadata. Throws (before
   * changing anything) when the edit would break a table header or split a merged region partially
   * out of the sheet.
   */
  applyStructural(e: StructuralEdit): void {
    if (e.count <= 0) throw new ModelError('Count must be positive');
    // Validate tables first so a refused edit changes nothing.
    const tables: Table[] = [];
    for (const t of this.tables) {
      const n = shiftTable(t, e);
      if (n) tables.push(n);
    }
    const rows = isRowEdit(e);
    const insert = e.kind === 'insertRows' || e.kind === 'insertCols';
    const delta = insert ? e.count : -e.count;
    if (rows) {
      this.cells.shiftRows(e.at, delta);
      this.rows.shift(e.at, delta);
    } else {
      this.cells.shiftCols(e.at, delta);
      this.cols.shift(e.at, delta);
    }
    this.tables = tables;
    this.merges = shiftMerges(this.merges, e);
    this.cf = this.cf
      .map((b) => ({ ...b, ranges: shiftRanges(b.ranges, e) }))
      .filter((b) => b.ranges.length > 0);
    this.validations = this.validations
      .map((v) => ({ ...v, ranges: shiftRanges(v.ranges, e) }))
      .filter((v) => v.ranges.length > 0);
    this.hyperlinks = this.hyperlinks
      .map((h) => {
        const ref = shiftRange(h.ref, e);
        return ref ? { ...h, ref } : null;
      })
      .filter((h): h is Hyperlink => h !== null);
    this.notes = this.notes
      .map((n) => {
        const p = shiftCell(n.row, n.col, e);
        return p ? { ...n, ...p } : null;
      })
      .filter((n): n is Note => n !== null);
    this.threads = this.threads
      .map((t) => {
        const p = shiftCell(t.row, t.col, e);
        return p ? { ...t, ...p } : null;
      })
      .filter((t): t is ThreadedComment => t !== null);
    this.autoFilter = this.autoFilter ? shiftAutoFilter(this.autoFilter, e) : null;
    this.charts = this.charts.map((d) => shiftDrawing(d, e));
    this.drawings = this.drawings.map((d) => shiftDrawing(d, e));
    // Views and print settings.
    const v = this.view;
    if (v.freeze) {
      v.freeze = rows
        ? { rows: shiftFrozenCount(v.freeze.rows, e), cols: v.freeze.cols }
        : { rows: v.freeze.rows, cols: shiftFrozenCount(v.freeze.cols, e) };
    }
    const sel = shiftRanges(v.selection.ranges, e);
    const active = shiftCell(v.selection.active.row, v.selection.active.col, e) ?? {
      row: rows ? Math.max(0, e.at - 1) : v.selection.active.row,
      col: rows ? v.selection.active.col : Math.max(0, e.at - 1),
    };
    v.selection = {
      ranges: sel.length > 0 ? sel : [{ r0: active.row, c0: active.col, r1: active.row, c1: active.col }],
      active,
    };
    if (rows) this.print.rowBreaks = shiftBreaks(this.print.rowBreaks, e);
    else this.print.colBreaks = shiftBreaks(this.print.colBreaks, e);
  }

  // ── snapshots of metadata ──────────────────────────────────────────────

  metaToJSON(): SheetMetaJSON {
    return clone({
      merges: this.merges,
      cf: this.cf,
      validations: this.validations,
      tables: this.tables,
      hyperlinks: this.hyperlinks,
      notes: this.notes,
      threads: this.threads,
      view: this.view,
      print: this.print,
      autoFilter: this.autoFilter,
      charts: this.charts,
      drawings: this.drawings,
      visibility: this.visibility,
      tabColor: this.tabColor,
      outline: this.outline,
      defaultRowHeight: this.defaultRowHeightPt,
      defaultColWidth: this.defaultColWidthValue,
    });
  }

  loadMeta(m: SheetMetaJSON): void {
    const c = clone(m);
    this.merges = c.merges;
    this.cf = c.cf;
    this.validations = c.validations;
    this.tables = c.tables;
    this.hyperlinks = c.hyperlinks;
    this.notes = c.notes;
    this.threads = c.threads;
    this.view = c.view;
    this.print = c.print;
    this.autoFilter = c.autoFilter;
    this.charts = c.charts;
    this.drawings = c.drawings;
    this.visibility = c.visibility;
    this.tabColor = c.tabColor;
    this.outline = c.outline;
    if (c.defaultRowHeight !== this.defaultRowHeightPt) this.defaultRowHeight = c.defaultRowHeight;
    if (c.defaultColWidth !== this.defaultColWidthValue) this.defaultColWidth = c.defaultColWidth;
  }

  layoutToJSON(): SheetLayoutJSON {
    return { rows: this.rows.toJSON(), cols: this.cols.toJSON() };
  }

  loadLayout(l: SheetLayoutJSON): void {
    this.rows.load(l.rows);
    this.cols.load(l.cols);
  }
}
