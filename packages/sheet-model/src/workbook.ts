/**
 * The workbook: ordered sheets, defined names, the style table, theme, workbook properties and an
 * opaque passthrough slot for the file layer, plus change notification for the calculation layer and
 * editors.
 *
 * Like {@link Sheet}, the methods here are low-level building blocks. User edits go through commands
 * (`applyCommand` / {@link CommandLog}), which record inverses for undo and emit change events.
 */
import { isValidName } from '@officina/formula';
import { officeTheme, type Theme } from '@officina/ooxml';
import { type CellRange, ModelError } from './address.ts';
import { DEFAULT_FONT_METRICS, type FontMetrics } from './axis.ts';
import {
  BUILTIN_NAMES,
  type CalcProps,
  type DefinedName,
  defaultCalcProps,
  type Table,
  type WorkbookProps,
} from './meta.ts';
import { Sheet } from './sheet.ts';
import type { StructuralEdit } from './shift.ts';
import type { StoredCell } from './store.ts';
import { DEFAULT_FONT, StyleTable } from './styles.ts';

/** Change notifications (consumed by `WorkbookCalc`, the grid renderer, collaboration…). */
export type WorkbookEvent =
  /** Cells changed: flat `[row, col, row, col, …]` pairs; `styleOnly` when only formats changed. */
  | { type: 'cells'; sheet: number; coords: number[]; styleOnly?: boolean }
  | { type: 'structure'; sheet: number; edit: StructuralEdit }
  | {
      type: 'move';
      sheet: number;
      range: CellRange;
      to: { sheet: number; row: number; col: number };
    }
  | { type: 'sheetAdded'; sheet: number }
  | { type: 'sheetRemoved'; sheet: number; name: string }
  | { type: 'sheetRenamed'; sheet: number; from: string; to: string }
  | { type: 'sheetMoved'; sheet: number; index: number }
  | { type: 'names' }
  | { type: 'tables' }
  /** Row/column properties changed (sizes, hidden, outline). */
  | { type: 'layout'; sheet: number; axis: 'rows' | 'cols' }
  /** Other sheet metadata changed (`key` names the field). */
  | { type: 'meta'; sheet: number; key: string }
  | { type: 'props' }
  /** End of a top-level command or transaction: a good moment to recalculate and repaint. */
  | { type: 'commit' };

export type WorkbookListener = (e: WorkbookEvent) => void;

/** A formula whose text changed during a workbook-wide rewrite (for undo). */
export interface FormulaChange {
  sheet: number;
  row: number;
  col: number;
  before: StoredCell;
}

/** Excel's sheet-name rules. */
export function checkSheetName(name: string): void {
  if (name.length === 0 || name.length > 31)
    throw new ModelError('A sheet name must have 1 to 31 characters');
  if (/[:\\/?*[\]]/.test(name)) throw new ModelError('A sheet name cannot contain : \\ / ? * [ or ]');
  if (name.startsWith("'") || name.endsWith("'"))
    throw new ModelError('A sheet name cannot begin or end with an apostrophe');
  if (name.toUpperCase() === 'HISTORY') throw new ModelError('"History" is a reserved sheet name');
}

/** True for defined-name text Excel accepts (including the reserved `_xlnm.` names). */
export function isDefinedNameValid(name: string): boolean {
  if (name.startsWith('_xlnm.'))
    return Object.values(BUILTIN_NAMES).some((b) => b.toUpperCase() === name.toUpperCase());
  return isValidName(name);
}

export interface WorkbookOptions {
  /** Create these sheets (default: one sheet named "Sheet1"). Pass [] for none. */
  sheets?: string[];
  metrics?: FontMetrics;
  theme?: Theme;
}

export class Workbook {
  sheets: Sheet[] = [];
  names: DefinedName[] = [];
  styles = new StyleTable(DEFAULT_FONT);
  theme: Theme | undefined;
  props: WorkbookProps = { date1904: false, calc: defaultCalcProps(), activeSheet: 0 };
  /** Opaque data owned by the file layer (unmodelled parts kept for round trip). Not snapshotted. */
  passthrough: unknown;
  metrics: FontMetrics;

  private nextSheetId = 1;
  private nextTableId = 1;
  private nextRuleId = 1;
  private readonly listeners = new Set<WorkbookListener>();
  private depth = 0;
  private dirtyCommit = false;

  constructor(opts: WorkbookOptions = {}) {
    this.metrics = opts.metrics ?? DEFAULT_FONT_METRICS;
    this.theme = opts.theme ?? officeTheme();
    for (const name of opts.sheets ?? ['Sheet1']) this.addSheet(name);
  }

  // ── events ─────────────────────────────────────────────────────────────

  on(listener: WorkbookListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  emit(e: WorkbookEvent): void {
    if (e.type !== 'commit') this.dirtyCommit = true;
    for (const l of [...this.listeners]) l(e);
  }

  /** Runs `fn` as one unit: a single `commit` event is emitted when the outermost unit ends. */
  transact<T>(fn: () => T): T {
    this.depth++;
    try {
      return fn();
    } finally {
      this.depth--;
      if (this.depth === 0 && this.dirtyCommit) {
        this.dirtyCommit = false;
        this.emit({ type: 'commit' });
        this.dirtyCommit = false;
      }
    }
  }

  // ── ids ────────────────────────────────────────────────────────────────

  allocTableId(): number {
    return this.nextTableId++;
  }

  allocRuleId(): number {
    return this.nextRuleId++;
  }

  /** Counters, for snapshots. */
  idCounters(): { sheet: number; table: number; rule: number } {
    return { sheet: this.nextSheetId, table: this.nextTableId, rule: this.nextRuleId };
  }

  setIdCounters(c: { sheet: number; table: number; rule: number }): void {
    this.nextSheetId = Math.max(this.nextSheetId, c.sheet);
    this.nextTableId = Math.max(this.nextTableId, c.table);
    this.nextRuleId = Math.max(this.nextRuleId, c.rule);
  }

  // ── fonts ──────────────────────────────────────────────────────────────

  /** Maximum digit width (px) of the default font: the unit of column widths. */
  get maxDigitWidth(): number {
    return this.metrics.maxDigitWidth(this.styles.defaultFont);
  }

  /** Re-derives column pixel widths after the default font or metrics changed. */
  refreshMetrics(metrics: FontMetrics = this.metrics): void {
    this.metrics = metrics;
    const mdw = this.maxDigitWidth;
    for (const s of this.sheets) s.maxDigitWidth = mdw;
  }

  // ── sheets ─────────────────────────────────────────────────────────────

  get sheetCount(): number {
    return this.sheets.length;
  }

  sheetById(id: number): Sheet | undefined {
    return this.sheets.find((s) => s.id === id);
  }

  sheetByName(name: string): Sheet | undefined {
    const u = name.toUpperCase();
    return this.sheets.find((s) => s.name.toUpperCase() === u);
  }

  /** Resolves a sheet by id, name or object; throws when missing. */
  sheet(ref: number | string | Sheet): Sheet {
    if (ref instanceof Sheet) return ref;
    const s = typeof ref === 'number' ? this.sheetById(ref) : this.sheetByName(ref);
    if (!s) throw new ModelError(`No sheet ${typeof ref === 'number' ? `with id ${ref}` : `named "${ref}"`}`);
    return s;
  }

  sheetIndex(id: number): number {
    return this.sheets.findIndex((s) => s.id === id);
  }

  /** The next unused default name ("Sheet2", "Sheet3"…). */
  nextSheetName(prefix = 'Sheet'): string {
    for (let i = 1; ; i++) {
      const n = `${prefix}${i}`;
      if (!this.sheetByName(n)) return n;
    }
  }

  /** Adds a sheet (low-level: no event). */
  addSheet(name: string = this.nextSheetName(), index = this.sheets.length, id?: number): Sheet {
    checkSheetName(name);
    if (this.sheetByName(name)) throw new ModelError(`A sheet named "${name}" already exists`);
    const sid = id ?? this.nextSheetId;
    if (this.sheetById(sid)) throw new ModelError(`Sheet id ${sid} is already used`);
    this.nextSheetId = Math.max(this.nextSheetId, sid + 1);
    const sheet = new Sheet(sid, name, this.maxDigitWidth);
    this.sheets.splice(Math.max(0, Math.min(index, this.sheets.length)), 0, sheet);
    return sheet;
  }

  /** Inserts an existing sheet object (used when restoring a removed sheet). */
  insertSheetObject(sheet: Sheet, index: number): void {
    if (this.sheetById(sheet.id)) throw new ModelError(`Sheet id ${sheet.id} is already used`);
    if (this.sheetByName(sheet.name)) throw new ModelError(`A sheet named "${sheet.name}" already exists`);
    this.sheets.splice(Math.max(0, Math.min(index, this.sheets.length)), 0, sheet);
    this.nextSheetId = Math.max(this.nextSheetId, sheet.id + 1);
  }

  // ── names ──────────────────────────────────────────────────────────────

  findName(name: string, scope: number | null): DefinedName | undefined {
    const u = name.toUpperCase();
    return this.names.find((n) => n.scope === scope && n.name.toUpperCase() === u);
  }

  /** Resolves a name as seen from a sheet: a local name wins over a workbook-level one. */
  resolveName(name: string, fromSheet: number): DefinedName | undefined {
    return this.findName(name, fromSheet) ?? this.findName(name, null);
  }

  /** Throws when a name is not valid or clashes with a table. */
  checkDefinedName(name: string, scope: number | null): void {
    if (!isDefinedNameValid(name)) throw new ModelError(`"${name}" is not a valid name`);
    if (scope !== null && !this.sheetById(scope)) throw new ModelError(`No sheet with id ${scope}`);
    if (this.findTable(name)) throw new ModelError(`"${name}" is already used by a table`);
  }

  // ── tables ─────────────────────────────────────────────────────────────

  findTable(name: string): { sheet: Sheet; table: Table } | undefined {
    const u = name.toUpperCase();
    for (const sheet of this.sheets) {
      const table = sheet.tables.find((t) => t.name.toUpperCase() === u);
      if (table) return { sheet, table };
    }
    return undefined;
  }

  /** Validates a table definition for `sheet` (ignoring the table named `replacing`). */
  checkTable(sheet: Sheet, t: Table, replacing?: string): void {
    if (!isValidName(t.name)) throw new ModelError(`"${t.name}" is not a valid table name`);
    const other = this.findTable(t.name);
    if (other && other.table.name.toUpperCase() !== replacing?.toUpperCase())
      throw new ModelError(`A table named "${t.name}" already exists`);
    if (this.names.some((n) => n.name.toUpperCase() === t.name.toUpperCase()))
      throw new ModelError(`"${t.name}" is already used by a defined name`);
    const r = t.range;
    const width = r.c1 - r.c0 + 1;
    if (t.columns.length !== width)
      throw new ModelError(`Table "${t.name}" has ${t.columns.length} column names for ${width} columns`);
    const seen = new Set<string>();
    for (const c of t.columns) {
      const u = c.name.toUpperCase();
      if (!c.name) throw new ModelError('Table column names cannot be empty');
      if (seen.has(u)) throw new ModelError(`Duplicate table column name "${c.name}"`);
      seen.add(u);
    }
    const minRows = 1 + (t.headerRow ? 1 : 0) + (t.totalsRow ? 1 : 0);
    if (r.r1 - r.r0 + 1 < minRows) throw new ModelError('A table needs at least one data row');
    for (const o of sheet.tables) {
      if (o.name.toUpperCase() === replacing?.toUpperCase()) continue;
      if (o.range.r0 <= r.r1 && r.r0 <= o.range.r1 && o.range.c0 <= r.c1 && r.c0 <= o.range.c1)
        throw new ModelError(`Table "${t.name}" would overlap table "${o.name}"`);
    }
    const merge = sheet.merges.find((m) => m.r0 <= r.r1 && r.r0 <= m.r1 && m.c0 <= r.c1 && r.c0 <= m.c1);
    if (merge) throw new ModelError('A table cannot contain merged cells');
  }

  /** The next free default table name ("Table1", "Table2"…). */
  nextTableName(): string {
    for (let i = 1; ; i++) {
      const n = `Table${i}`;
      if (!this.findTable(n) && !this.names.some((d) => d.name.toUpperCase() === n.toUpperCase())) return n;
    }
  }

  // ── properties ─────────────────────────────────────────────────────────

  setCalcProps(patch: Partial<CalcProps>): void {
    this.props = { ...this.props, calc: { ...this.props.calc, ...patch } };
  }

  // ── workbook-wide formula rewriting ────────────────────────────────────

  /**
   * Rewrites every formula in the workbook — cells, defined names, conditional formats, validation
   * criteria and table column formulas — through `fn(text, formulaSheet, at)`, where `formulaSheet` is
   * the name of the sheet the formula lives on (`\u0000` for workbook-scoped names, whose unqualified
   * references are not tied to a sheet) and `at` is the cell of a cell formula. Formulas that cannot
   * be parsed are left alone. Returns the prior state of every cell whose formula changed. With
   * `dryRun`, nothing is modified (only the cell changes are reported).
   */
  rewriteFormulas(
    fn: (text: string, formulaSheet: string, at?: { row: number; col: number }) => string,
    dryRun = false,
  ): FormulaChange[] {
    const safe = (text: string, sheet: string, at?: { row: number; col: number }): string => {
      try {
        return fn(text, sheet, at);
      } catch {
        return text;
      }
    };
    const changes: FormulaChange[] = [];
    for (const sheet of this.sheets) {
      const updates: [number, number, string][] = [];
      sheet.cells.forEach((row, col, value, formula, style) => {
        if (formula === null) return;
        const next = safe(formula, sheet.name, { row, col });
        if (next !== formula) {
          changes.push({ sheet: sheet.id, row, col, before: { value, formula, style } });
          updates.push([row, col, next]);
        }
      });
      if (dryRun) continue;
      for (const [row, col, f] of updates) {
        const c = sheet.cells.get(row, col);
        sheet.cells.set(row, col, c?.value ?? null, f, c?.style ?? 0);
      }
      for (const block of sheet.cf) {
        const top = block.ranges[0];
        const at = top ? { row: top.r0, col: top.c0 } : undefined;
        for (const rule of block.rules) {
          if (rule.type === 'cellIs') rule.formulas = rule.formulas.map((f) => safe(f, sheet.name, at));
          else if (rule.type === 'expression') rule.formula = safe(rule.formula, sheet.name, at);
          const cfvos =
            rule.type === 'colorScale' || rule.type === 'iconSet'
              ? rule.cfvos
              : rule.type === 'dataBar'
                ? [rule.min, rule.max]
                : [];
          for (const v of cfvos) if (v.type === 'formula' && v.value) v.value = safe(v.value, sheet.name, at);
        }
      }
      for (const v of sheet.validations) {
        const top = v.ranges[0];
        const at = top ? { row: top.r0, col: top.c0 } : undefined;
        if (v.type === 'list' && v.formula1 && /^\s*"/.test(v.formula1)) continue;
        if (v.formula1) v.formula1 = safe(v.formula1, sheet.name, at);
        if (v.formula2) v.formula2 = safe(v.formula2, sheet.name, at);
      }
      for (const t of sheet.tables) {
        const at = { row: t.range.r0 + (t.headerRow ? 1 : 0), col: t.range.c0 };
        for (const c of t.columns) {
          if (c.calculatedFormula) c.calculatedFormula = safe(c.calculatedFormula, sheet.name, at);
          if (c.totalsFormula) c.totalsFormula = safe(c.totalsFormula, sheet.name, at);
        }
      }
    }
    if (dryRun) return changes;
    for (const n of this.names) {
      const scopeName = n.scope === null ? '\u0000' : (this.sheetById(n.scope)?.name ?? '\u0000');
      n.formula = safe(n.formula, scopeName);
    }
    return changes;
  }
}
