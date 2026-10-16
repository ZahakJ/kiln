/**
 * Formula binding: runs `@kiln-ref/formula`'s {@link Engine} over a {@link Workbook}.
 *
 * - {@link SheetModelHost} implements the engine's host interface over the sparse cell store: literal
 *   values, used bounds, hidden/filtered rows (SUBTOTAL/AGGREGATE) and tables for structured
 *   references.
 * - {@link WorkbookCalc} mirrors the workbook into the engine (sheets, formulas, defined names) and
 *   keeps it in sync by listening to workbook events, recalculates incrementally (automatically after
 *   every command in automatic mode, on demand in manual mode), exposes computed values and spill
 *   ranges and writes formula results back to the store as cached values.
 *
 * The engine runs in-process here; the grid wraps this in a worker later (WP-17).
 */
import {
  type CellAddress,
  Engine,
  type EngineHost,
  type ExplainStep,
  type RecalcResult,
  type Scalar,
  type TableDef,
  type Value,
} from '@kiln-ref/formula';
import { ModelError } from './address.ts';
import { type FormatOptions, type FormattedValue, formatCell } from './format.ts';
import type { Table } from './meta.ts';
import type { Sheet } from './sheet.ts';
import { type CellValue, fromScalar, toScalar } from './values.ts';
import type { Workbook, WorkbookEvent } from './workbook.ts';

function tableDef(t: Table, sheet: number): TableDef {
  return {
    name: t.name,
    sheet,
    r0: t.range.r0,
    c0: t.range.c0,
    r1: t.range.r1,
    c1: t.range.c1,
    headerRow: t.headerRow,
    totalsRow: t.totalsRow,
    columns: t.columns.map((c) => c.name),
  };
}

/** The engine host over the workbook's cell store. */
export class SheetModelHost implements EngineHost {
  private tableCache: Map<string, TableDef> | null = null;
  private sheetCache = new Map<number, Sheet>();

  constructor(readonly workbook: Workbook) {}

  /** Forgets cached sheet lookups (after sheets are added or removed). */
  invalidateSheets(): void {
    this.sheetCache.clear();
  }

  private sheet(id: number): Sheet | undefined {
    let s = this.sheetCache.get(id);
    if (!s) {
      s = this.workbook.sheetById(id);
      if (s) this.sheetCache.set(id, s);
    }
    return s;
  }

  getCellValue(sheet: number, row: number, col: number): Scalar | undefined {
    const s = this.sheet(sheet);
    if (!s) return undefined;
    const ch = s.cells.chunk(row);
    if (!ch) return undefined;
    const i = ch.find(col);
    if (i < 0 || ch.formulas[i] !== null) return undefined;
    return toScalar(ch.values[i] as CellValue);
  }

  getUsedBounds(sheet: number): { rows: number; cols: number } {
    const used = this.sheet(sheet)?.cells.getUsedRange(true);
    return used ? { rows: used.r1 + 1, cols: used.c1 + 1 } : { rows: 0, cols: 0 };
  }

  invalidateTables(): void {
    this.tableCache = null;
  }

  private tables(): Map<string, TableDef> {
    if (!this.tableCache) {
      const m = new Map<string, TableDef>();
      for (const s of this.workbook.sheets)
        for (const t of s.tables) m.set(t.name.toUpperCase(), tableDef(t, s.id));
      this.tableCache = m;
    }
    return this.tableCache;
  }

  getTable(name: string): TableDef | undefined {
    return this.tables().get(name.toUpperCase());
  }

  getTableAt(sheet: number, row: number, col: number): TableDef | undefined {
    const s = this.workbook.sheetById(sheet);
    const t = s?.tableAt(row, col);
    return t ? this.getTable(t.name) : undefined;
  }

  isRowHidden(sheet: number, row: number): boolean {
    return this.sheet(sheet)?.isRowHidden(row) ?? false;
  }

  isRowFiltered(sheet: number, row: number): boolean {
    return this.sheet(sheet)?.isRowFiltered(row) ?? false;
  }

  // The model has already shifted/removed its own cells when the engine reports these.
  applyStructuralEdit(): void {}
  removeSheet(): void {}
}

export interface WorkbookCalcOptions {
  /** Clock for NOW()/TODAY(). */
  now?: () => Date;
  /** Write formula results back into the store as cached values (default true). */
  writeBack?: boolean;
}

export interface CalcError {
  sheet: number;
  row: number;
  col: number;
  formula: string;
  message: string;
}

export class WorkbookCalc {
  readonly host: SheetModelHost;
  engine!: Engine;
  /** Formulas the engine could not parse (shown as #NAME?). */
  readonly errors = new Map<string, CalcError>();
  private readonly unsubscribe: () => void;
  private needsFull = false;
  private pending = false;
  private readonly opts: WorkbookCalcOptions;
  lastResult: RecalcResult | null = null;

  constructor(
    readonly workbook: Workbook,
    opts: WorkbookCalcOptions = {},
  ) {
    this.opts = opts;
    this.host = new SheetModelHost(workbook);
    this.rebuild();
    this.unsubscribe = workbook.on((e) => this.onEvent(e));
  }

  dispose(): void {
    this.unsubscribe();
  }

  /** Calculation mode from the workbook properties. */
  get mode(): 'auto' | 'manual' {
    return this.workbook.props.calc.mode === 'manual' ? 'manual' : 'auto';
  }

  /** True when edits are waiting for a recalculation (manual mode shows "Calculate" in the status bar). */
  get isDirty(): boolean {
    return this.pending || this.engine.graphStats().dirty > 0;
  }

  /** Re-creates the engine from the workbook (on load and when the date system changes). */
  rebuild(): void {
    const wb = this.workbook;
    const c = wb.props.calc;
    const engineOpts: ConstructorParameters<typeof Engine>[0] = {
      host: this.host,
      date1904: wb.props.date1904,
      iterative: { enabled: c.iterate, maxIterations: c.iterateCount, maxChange: c.iterateDelta },
    };
    if (this.opts.now) engineOpts.now = this.opts.now;
    this.engine = new Engine(engineOpts);
    this.host.invalidateTables();
    this.host.invalidateSheets();
    this.errors.clear();
    for (const s of wb.sheets) this.engine.addSheet(s.name, s.id);
    this.syncNames();
    for (const s of wb.sheets) {
      s.cells.forEach((row, col, _v, formula) => {
        if (formula !== null) this.setFormula(s.id, row, col, formula);
      });
    }
    this.pending = true;
    if (this.mode === 'auto') this.recalc();
  }

  private key(sheet: number, row: number, col: number): string {
    return `${sheet}:${row}:${col}`;
  }

  private setFormula(sheet: number, row: number, col: number, formula: string): void {
    try {
      this.engine.setCell(sheet, row, col, { formula });
      this.errors.delete(this.key(sheet, row, col));
    } catch (e) {
      this.errors.set(this.key(sheet, row, col), { sheet, row, col, formula, message: (e as Error).message });
      if (this.engine.hasFormula(sheet, row, col)) this.engine.setCell(sheet, row, col, {});
    }
  }

  private syncCell(sheet: number, row: number, col: number): void {
    const s = this.workbook.sheetById(sheet);
    if (!s) return;
    const formula = s.cells.getFormula(row, col);
    if (formula !== null) {
      const current = this.engine.getFormula(sheet, row, col);
      if (current === undefined || current.slice(1) !== formula) this.setFormula(sheet, row, col, formula);
      return;
    }
    this.errors.delete(this.key(sheet, row, col));
    if (this.engine.hasFormula(sheet, row, col)) this.engine.setCell(sheet, row, col, {});
    else this.engine.cellChanged(sheet, row, col);
  }

  private syncNames(): void {
    const wb = this.workbook;
    const wanted = new Map<string, { name: string; formula: string; scope: number | null }>();
    for (const n of wb.names) wanted.set(`${n.scope ?? '*'}|${n.name.toUpperCase()}`, n);
    for (const n of this.engine.listNames()) {
      const scope = n.scope === null ? null : (this.engine.sheetId(n.scope) ?? null);
      const k = `${scope ?? '*'}|${n.name.toUpperCase()}`;
      const w = wanted.get(k);
      if (!w || `=${w.formula}` !== n.formula) {
        this.engine.removeName(n.name, scope ?? undefined);
      } else wanted.delete(k);
    }
    for (const n of wanted.values()) {
      try {
        this.engine.defineName(n.name, n.formula, n.scope ?? undefined);
      } catch {
        // Names the engine cannot evaluate (external references, reserved names) are skipped.
      }
    }
  }

  private onEvent(e: WorkbookEvent): void {
    const eng = this.engine;
    switch (e.type) {
      case 'cells':
        if (e.styleOnly) break;
        for (let i = 0; i < e.coords.length; i += 2)
          this.syncCell(e.sheet, e.coords[i] as number, e.coords[i + 1] as number);
        this.pending = true;
        break;
      case 'structure':
        eng[e.edit.kind](e.sheet, e.edit.at, e.edit.count);
        this.pending = true;
        break;
      case 'move':
        eng.moveRange(e.sheet, e.range, { sheet: e.to.sheet, row: e.to.row, col: e.to.col });
        this.pending = true;
        break;
      case 'sheetAdded': {
        this.host.invalidateSheets();
        const s = this.workbook.sheet(e.sheet);
        eng.addSheet(s.name, s.id);
        eng.moveSheet(s.id, this.workbook.sheetIndex(s.id));
        this.host.invalidateTables();
        s.cells.forEach((row, col, _v, f) => {
          if (f !== null) this.setFormula(s.id, row, col, f);
          else eng.cellChanged(s.id, row, col);
        });
        this.syncNames();
        this.pending = true;
        break;
      }
      case 'sheetRemoved':
        this.host.invalidateSheets();
        eng.removeSheet(e.sheet);
        this.host.invalidateTables();
        for (const k of [...this.errors.keys()]) if (k.startsWith(`${e.sheet}:`)) this.errors.delete(k);
        this.pending = true;
        break;
      case 'sheetRenamed':
        eng.renameSheet(e.sheet, e.to);
        this.pending = true;
        break;
      case 'sheetMoved':
        eng.moveSheet(e.sheet, e.index);
        this.pending = true;
        break;
      case 'names':
        this.syncNames();
        this.pending = true;
        break;
      case 'tables':
        this.host.invalidateTables();
        eng.tablesChanged();
        this.pending = true;
        break;
      case 'layout':
        // Hidden/filtered rows change SUBTOTAL/AGGREGATE results.
        if (e.axis === 'rows') {
          this.needsFull = true;
          this.pending = true;
        }
        break;
      case 'props': {
        const c = this.workbook.props.calc;
        if (this.workbook.props.date1904 !== eng.date1904) this.rebuild();
        else
          eng.setIterative({ enabled: c.iterate, maxIterations: c.iterateCount, maxChange: c.iterateDelta });
        break;
      }
      case 'commit':
        if (this.mode === 'auto' && this.pending) this.recalc();
        break;
      case 'meta':
        break;
    }
  }

  // ── calculation ────────────────────────────────────────────────────────

  /** Recalculates what changed (F9 in manual mode). */
  recalc(): RecalcResult {
    const res = this.needsFull ? this.engine.recalcAll() : this.engine.recalc();
    this.needsFull = false;
    this.pending = false;
    this.lastResult = res;
    if (this.opts.writeBack !== false) this.writeBack(res.changed);
    return res;
  }

  /** Recalculates every formula (Ctrl+Alt+F9). */
  recalcAll(): RecalcResult {
    this.needsFull = true;
    return this.recalc();
  }

  /** Recalculates one sheet (Shift+F9): its formulas and whatever depends on them. */
  recalcSheet(sheet: number | string): RecalcResult {
    const s = this.workbook.sheet(sheet);
    s.cells.forEach((row, col, _v, f) => {
      if (f !== null) this.setFormula(s.id, row, col, f);
    });
    return this.recalc();
  }

  private writeBack(changed: CellAddress[]): void {
    for (const c of changed) {
      const sid = this.engine.sheetId(c.sheet);
      if (sid === undefined) continue;
      const s = this.workbook.sheetById(sid);
      if (!s) continue;
      s.cells.setCachedValue(c.row, c.col, fromScalar(this.engine.getValue(sid, c.row, c.col)));
    }
  }

  // ── results ────────────────────────────────────────────────────────────

  /** Displayed value of a cell: a formula result, a spilled value or the literal value. */
  getValue(sheet: number | string, row: number, col: number): CellValue {
    const sid = this.sheetId(sheet);
    if (this.errors.has(this.key(sid, row, col))) return fromScalar(this.engine.evaluate('#NAME?') as Scalar);
    return fromScalar(this.engine.getValue(sid, row, col));
  }

  /** The full result of a formula (an array for dynamic arrays). */
  getResult(sheet: number | string, row: number, col: number): Value | undefined {
    return this.engine.getResult(this.sheetId(sheet), row, col);
  }

  /** The spill range containing the cell, if any. */
  getSpillRange(sheet: number | string, row: number, col: number) {
    return this.engine.getSpillRange(this.sheetId(sheet), row, col);
  }

  isCircular(sheet: number | string, row: number, col: number): boolean {
    return this.engine.isCircular(this.sheetId(sheet), row, col);
  }

  /** The value formatted with the cell's number format. */
  getDisplay(sheet: number | string, row: number, col: number, opts: FormatOptions = {}): FormattedValue {
    const s = this.workbook.sheet(sheet);
    const style = this.workbook.styles.get(s.styleAt(row, col));
    return formatCell(this.getValue(s.id, row, col), style.numFmt, {
      date1904: this.workbook.props.date1904,
      ...opts,
    });
  }

  /** Evaluates a formula as if entered in a cell, without storing it. */
  evaluate(formula: string, at?: { sheet?: number | string; row?: number; col?: number }): Value {
    const pos = at
      ? { ...at, sheet: at.sheet === undefined ? undefined : this.sheetId(at.sheet) }
      : undefined;
    return this.engine.evaluate(formula, pos as Parameters<Engine['evaluate']>[1]);
  }

  /** Step-by-step evaluation of a cell's formula for the Formula Lab / Evaluate Formula. */
  explain(sheet: number | string, row: number, col: number): ExplainStep {
    const sid = this.sheetId(sheet);
    const f = this.workbook.sheet(sid).cells.getFormula(row, col);
    if (f === null) throw new ModelError('The cell does not contain a formula');
    return this.engine.explain(`=${f}`, { sheet: sid, row, col });
  }

  private sheetId(sheet: number | string): number {
    return this.workbook.sheet(sheet).id;
  }
}
