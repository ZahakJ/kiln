/**
 * The calculation engine: formulas, dependency graph, incremental recalculation, dynamic-array
 * spilling, defined names, structural edits and the Formula Lab's `explain`.
 *
 * Storage of literal values is delegated to an {@link EngineHost}; by default an in-memory
 * {@link MemoryHost}. Formula results are owned by the engine.
 *
 * Typical use:
 * ```ts
 * const engine = new Engine();
 * engine.addSheet('Sheet1');
 * engine.setCell('Sheet1', 0, 0, { value: 2 });
 * engine.setCell('Sheet1', 0, 1, { formula: '=A1*21' });
 * engine.recalc();
 * engine.getValue('Sheet1', 0, 1); // 42
 * ```
 */
import type { Node } from '../parser/ast.ts';
import { parse } from '../parser/parser.ts';
import { MAX_COLS, MAX_ROWS } from '../parser/references.ts';
import { isNameStart, matchA1Address, matchR1C1Address } from '../parser/tokenizer.ts';
import {
  moveFormula,
  removeSheetInFormula,
  renameSheetInFormula,
  type StructuralChange,
  shiftFormula,
} from '../parser/transform.ts';
import { Errors, ErrorValue } from '../values/errors.ts';
import { type Area, Array2D, BLANK, type Scalar, type Value } from '../values/types.ts';
import { type EvalHost, Evaluator } from './evaluator.ts';
import { buildExplainTree, type ExplainStep } from './explain.ts';
import { DependencyGraph, FormulaCell, type Precedents } from './graph.ts';
import { cellKey, type EngineHost, MemoryHost, type StructuralEdit, type TableDef } from './host.ts';
import { collectPrecedents } from './precedents.ts';
import type { Rect } from './rect-index.ts';
import { tarjan } from './scc.ts';

export type SheetRef = string | number;

export interface EngineOptions {
  host?: EngineHost;
  /** Use the 1904 date system. */
  date1904?: boolean;
  /** Iterative calculation for circular references (File › Options › Formulas). */
  iterative?: boolean | { enabled?: boolean; maxIterations?: number; maxChange?: number };
  /** Clock for NOW()/TODAY() (tests pass a fixed date). */
  now?: () => Date;
}

export interface CellInput {
  /** Formula text, with or without the leading `=`. */
  formula?: string | null;
  /** Literal value (null/undefined clears the cell). */
  value?: Scalar | null;
  /** Evaluate with pre-dynamic-array semantics (implicit intersection, no spilling). */
  legacy?: boolean;
}

export interface CellAddress {
  sheet: string;
  row: number;
  col: number;
}

export interface SpillRange extends CellAddress {
  /** Last row/column of the spill (inclusive). */
  row1: number;
  col1: number;
}

export interface RecalcResult {
  /** Number of formula evaluations performed. */
  evaluated: number;
  /** Formula cells whose result changed (spill anchors stand for their whole spill range). */
  changed: CellAddress[];
  /** Cells that are part of a circular reference. */
  circular: CellAddress[];
}

interface SheetInfo {
  id: number;
  name: string;
}

interface NameDef {
  name: string;
  text: string;
  ast: Node;
  /** Sheet id for a sheet-scoped name, null for workbook scope. */
  scope: number | null;
}

const MAX_PASSES = 64;

export class Engine implements EvalHost {
  readonly host: EngineHost;
  readonly date1904: boolean;
  private readonly clock: () => Date;
  private iterativeEnabled = false;
  private maxIterations = 100;
  private maxChange = 0.001;

  private readonly sheets = new Map<number, SheetInfo>();
  private readonly sheetByName = new Map<string, number>();
  private sheetOrder: number[] = [];
  private nextSheetId = 1;

  private readonly formulas = new Map<number, FormulaCell>();
  private readonly volatiles = new Set<FormulaCell>();
  private readonly dirty = new Set<FormulaCell>();
  private readonly graph = new DependencyGraph();
  private readonly names = new Map<string, NameDef>();
  private readonly formulaBounds = new Map<number, { rows: number; cols: number }>();

  private changed = new Set<FormulaCell>();
  private evaluatedCount = 0;

  constructor(opts: EngineOptions = {}) {
    this.host = opts.host ?? new MemoryHost();
    this.date1904 = opts.date1904 ?? false;
    this.clock = opts.now ?? (() => new Date());
    this.setIterative(opts.iterative ?? false);
  }

  /** Configures iterative calculation (default off; 100 iterations, maximum change 0.001). */
  setIterative(opt: boolean | { enabled?: boolean; maxIterations?: number; maxChange?: number }): void {
    if (typeof opt === 'boolean') this.iterativeEnabled = opt;
    else {
      this.iterativeEnabled = opt.enabled ?? true;
      if (opt.maxIterations !== undefined) this.maxIterations = Math.max(1, Math.floor(opt.maxIterations));
      if (opt.maxChange !== undefined) this.maxChange = Math.max(0, opt.maxChange);
    }
  }

  now(): Date {
    return this.clock();
  }

  // ── sheets ─────────────────────────────────────────────────────────────

  /** Adds a sheet and returns its id. Ids are stable across renames. */
  addSheet(name: string, id?: number): number {
    if (this.sheetByName.has(name.toUpperCase())) throw new Error(`A sheet named "${name}" already exists`);
    const sid = id ?? this.nextSheetId;
    if (this.sheets.has(sid)) throw new Error(`Sheet id ${sid} is already used`);
    this.nextSheetId = Math.max(this.nextSheetId, sid + 1);
    this.sheets.set(sid, { id: sid, name });
    this.sheetByName.set(name.toUpperCase(), sid);
    this.sheetOrder.push(sid);
    // Formulas that referred to this (previously missing) sheet can now bind.
    const waiting = [...this.formulas.values()].filter((f) => f.unresolvedSheets);
    for (const fc of waiting) this.reregister(fc);
    this.markDirty(waiting);
    return sid;
  }

  /** Removes a sheet; references to it become `#REF!`. */
  removeSheet(sheet: SheetRef): void {
    const sid = this.resolveSheet(sheet);
    const name = this.sheetNameOf(sid);
    for (const fc of [...this.formulas.values()]) {
      if (fc.sheet === sid) {
        this.graph.remove(fc);
        this.formulas.delete(fc.key);
        this.volatiles.delete(fc);
        this.dirty.delete(fc);
      }
    }
    for (const [k, def] of this.names) {
      if (def.scope === sid) this.names.delete(k);
    }
    this.sheets.delete(sid);
    this.sheetByName.delete(name.toUpperCase());
    this.sheetOrder = this.sheetOrder.filter((s) => s !== sid);
    this.formulaBounds.delete(sid);
    this.host.removeSheet?.(sid);
    this.rewriteAll(
      (text) => removeSheetInFormula(text, name),
      () => '',
    );
    this.rebuildAll(true);
  }

  renameSheet(sheet: SheetRef, newName: string): void {
    const sid = this.resolveSheet(sheet);
    const oldName = this.sheetNameOf(sid);
    const existing = this.sheetByName.get(newName.toUpperCase());
    if (existing !== undefined && existing !== sid)
      throw new Error(`A sheet named "${newName}" already exists`);
    this.sheetByName.delete(oldName.toUpperCase());
    this.sheetByName.set(newName.toUpperCase(), sid);
    (this.sheets.get(sid) as SheetInfo).name = newName;
    this.rewriteAll(
      (text) => renameSheetInFormula(text, oldName, newName),
      () => '',
    );
    this.rebuildAll(true);
  }

  /** Moves a sheet to a new position in the workbook order (affects 3-D references). */
  moveSheet(sheet: SheetRef, index: number): void {
    const sid = this.resolveSheet(sheet);
    this.sheetOrder = this.sheetOrder.filter((s) => s !== sid);
    this.sheetOrder.splice(Math.max(0, Math.min(index, this.sheetOrder.length)), 0, sid);
    this.rebuildAll(true);
  }

  sheetId(name: string): number | undefined {
    return this.sheetByName.get(name.toUpperCase());
  }

  sheetName(id: number): string | undefined {
    return this.sheets.get(id)?.name;
  }

  /** Sheet names in workbook order. */
  listSheets(): string[] {
    return this.sheetOrder.map((id) => this.sheetNameOf(id));
  }

  sheetsBetween(a: number, b: number): number[] | undefined {
    const i = this.sheetOrder.indexOf(a);
    const j = this.sheetOrder.indexOf(b);
    if (i < 0 || j < 0) return undefined;
    return this.sheetOrder.slice(Math.min(i, j), Math.max(i, j) + 1);
  }

  private resolveSheet(sheet: SheetRef): number {
    if (typeof sheet === 'number') {
      if (!this.sheets.has(sheet)) throw new Error(`Unknown sheet id ${sheet}`);
      return sheet;
    }
    const id = this.sheetByName.get(sheet.toUpperCase());
    if (id === undefined) throw new Error(`Unknown sheet "${sheet}"`);
    return id;
  }

  private sheetNameOf(id: number): string {
    return (this.sheets.get(id) as SheetInfo).name;
  }

  // ── cells ──────────────────────────────────────────────────────────────

  /**
   * Sets a cell to a formula or a literal value. Formulas are parsed immediately (a
   * `FormulaParseError` is thrown for invalid syntax) and calculated by the next `recalc()`.
   */
  setCell(sheet: SheetRef, row: number, col: number, input: CellInput): void {
    const sid = this.resolveSheet(sheet);
    checkCell(row, col);
    const key = cellKey(sid, row, col);
    const existing = this.formulas.get(key);
    if (input.formula !== undefined && input.formula !== null) {
      const text = /^\s*=/.test(input.formula) ? input.formula : `=${input.formula}`;
      const ast = parse(text);
      if (existing) this.removeFormula(existing);
      this.host.setCellValue?.(sid, row, col, undefined);
      const fc = new FormulaCell(sid, row, col, text, ast, input.legacy ?? false);
      this.formulas.set(key, fc);
      this.register(fc);
      this.growBounds(sid, row, col);
      this.markDirty([fc]);
      return;
    }
    if (existing) this.removeFormula(existing);
    const value = input.value ?? undefined;
    if (this.host.setCellValue) this.host.setCellValue(sid, row, col, value);
    else if (value !== undefined) throw new Error('The engine host does not accept values');
    this.cellChanged(sid, row, col);
  }

  clearCell(sheet: SheetRef, row: number, col: number): void {
    this.setCell(sheet, row, col, {});
  }

  /** Tells the engine a literal value changed in host storage (when not set through `setCell`). */
  cellChanged(sheet: SheetRef, row: number, col: number): void {
    const sid = this.resolveSheet(sheet);
    const out: FormulaCell[] = [];
    this.graph.dependentsAt(sid, row, col, out);
    this.markDirty(out);
  }

  /** Tells the engine the host's tables changed (structured references are re-resolved). */
  tablesChanged(): void {
    const using = [...this.formulas.values()].filter((f) => f.usesTables);
    for (const fc of using) this.reregister(fc);
    this.markDirty(using);
  }

  /** Formula text of a cell (with `=`), or undefined. */
  getFormula(sheet: SheetRef, row: number, col: number): string | undefined {
    return this.formulas.get(cellKey(this.resolveSheet(sheet), row, col))?.text;
  }

  hasFormula(sheet: SheetRef, row: number, col: number): boolean {
    return this.formulas.has(cellKey(this.resolveSheet(sheet), row, col));
  }

  /**
   * Displayed value of a cell: a formula's result (the top-left value for a spilling anchor,
   * `#SPILL!` when blocked), a value inside a spill range, or the host's literal value. Values are as
   * of the last `recalc()`.
   */
  getValue(sheet: SheetRef, row: number, col: number): Scalar {
    const sid = this.resolveSheet(sheet);
    const fc = this.formulas.get(cellKey(sid, row, col));
    if (fc) return anchorValue(fc);
    const spilled = this.spillValueAt(sid, row, col);
    if (spilled !== undefined) return spilled;
    return this.host.getCellValue(sid, row, col) ?? BLANK;
  }

  /** The full result of a formula (an array for dynamic-array formulas). */
  getResult(sheet: SheetRef, row: number, col: number): Value | undefined {
    return this.formulas.get(cellKey(this.resolveSheet(sheet), row, col))?.value;
  }

  /**
   * The spill range containing a cell (the anchor or any cell it spilled into), or undefined. A
   * blocked anchor reports the range it would occupy with `blocked: true`.
   */
  getSpillRange(sheet: SheetRef, row: number, col: number): (SpillRange & { blocked: boolean }) | undefined {
    const sid = this.resolveSheet(sheet);
    let anchor = this.formulas.get(cellKey(sid, row, col));
    if (!anchor?.claim) {
      anchor = undefined;
      this.graph.claimsAt(sid, row, col, (c) => {
        if (c.anchor.spill === 'ok') anchor = c.anchor;
      });
    }
    if (!anchor?.claim) return undefined;
    const c = anchor.claim;
    return {
      sheet: this.sheetNameOf(sid),
      row: c.r0,
      col: c.c0,
      row1: c.r1,
      col1: c.c1,
      blocked: anchor.spill === 'blocked',
    };
  }

  isCircular(sheet: SheetRef, row: number, col: number): boolean {
    return this.formulas.get(cellKey(this.resolveSheet(sheet), row, col))?.circular ?? false;
  }

  circularCells(): CellAddress[] {
    return [...this.formulas.values()].filter((f) => f.circular).map((f) => this.addressOf(f));
  }

  /** Number of formula cells. */
  get formulaCount(): number {
    return this.formulas.size;
  }

  /** Graph statistics (for diagnostics and tests). */
  graphStats(): { formulas: number; rangeNodes: number; cellEdges: number; dirty: number } {
    return {
      formulas: this.formulas.size,
      rangeNodes: this.graph.rangeNodeCount,
      cellEdges: this.graph.cellEdgeCount,
      dirty: this.dirty.size,
    };
  }

  // ── names ──────────────────────────────────────────────────────────────

  /** Defines (or redefines) a name. `scope` makes it local to a sheet. */
  defineName(name: string, formula: string, scope?: SheetRef): void {
    if (!isValidName(name)) throw new Error(`"${name}" is not a valid name`);
    const sid = scope === undefined ? null : this.resolveSheet(scope);
    const text = /^\s*=/.test(formula) ? formula : `=${formula}`;
    const ast = parse(text);
    this.names.set(nameKey(name, sid), { name, text, ast, scope: sid });
    this.nameChanged(name);
  }

  removeName(name: string, scope?: SheetRef): void {
    const sid = scope === undefined ? null : this.resolveSheet(scope);
    if (this.names.delete(nameKey(name, sid))) this.nameChanged(name);
  }

  /** Defined names with their formulas. */
  listNames(): { name: string; formula: string; scope: string | null }[] {
    return [...this.names.values()].map((d) => ({
      name: d.name,
      formula: d.text,
      scope: d.scope === null ? null : this.sheetNameOf(d.scope),
    }));
  }

  lookupName(name: string, sheet: number): { ast: Node; scopeSheet: number | null } | undefined {
    const def = this.names.get(nameKey(name, sheet)) ?? this.names.get(nameKey(name, null));
    return def ? { ast: def.ast, scopeSheet: def.scope } : undefined;
  }

  private nameChanged(name: string): void {
    const users = this.graph.formulasUsingName(name);
    for (const fc of users) this.reregister(fc);
    this.markDirty(users);
  }

  // ── structural edits ───────────────────────────────────────────────────

  insertRows(sheet: SheetRef, at: number, count = 1): void {
    this.structural(sheet, { kind: 'insertRows', at, count });
  }

  deleteRows(sheet: SheetRef, at: number, count = 1): void {
    this.structural(sheet, { kind: 'deleteRows', at, count });
  }

  insertCols(sheet: SheetRef, at: number, count = 1): void {
    this.structural(sheet, { kind: 'insertCols', at, count });
  }

  deleteCols(sheet: SheetRef, at: number, count = 1): void {
    this.structural(sheet, { kind: 'deleteCols', at, count });
  }

  private structural(sheet: SheetRef, edit: StructuralEdit): void {
    const sid = this.resolveSheet(sheet);
    if (edit.count <= 0) return;
    const change: StructuralChange = { ...edit, sheet: this.sheetNameOf(sid) };
    this.rewriteAll(
      (text, f) => shiftFormula(text, change, f),
      (def) => (def.scope === null ? '\u0000' : this.sheetNameOf(def.scope)),
    );
    const rows = edit.kind === 'insertRows' || edit.kind === 'deleteRows';
    const insert = edit.kind === 'insertRows' || edit.kind === 'insertCols';
    const all = [...this.formulas.values()];
    for (const fc of all) this.graph.remove(fc);
    this.formulas.clear();
    for (const fc of all) {
      if (fc.sheet === sid) {
        const idx = rows ? fc.row : fc.col;
        if (idx >= edit.at) {
          if (!insert && idx < edit.at + edit.count) continue; // deleted
          const n = insert ? idx + edit.count : idx - edit.count;
          if (n >= (rows ? MAX_ROWS : MAX_COLS)) continue;
          if (rows) fc.row = n;
          else fc.col = n;
        }
      }
      fc.spill = fc.spill === 'none' ? 'none' : 'blocked';
      this.formulas.set(fc.key, fc);
    }
    this.host.applyStructuralEdit?.(sid, edit);
    this.recomputeBounds();
    this.rebuildAll(false);
  }

  /**
   * Moves (cut & paste) a block of cells. Formulas anywhere that refer into the block follow it;
   * references to the cells it lands on become `#REF!`.
   */
  moveRange(
    sheet: SheetRef,
    from: { r0: number; c0: number; r1: number; c1: number },
    to: { row: number; col: number; sheet?: SheetRef },
  ): void {
    const sid = this.resolveSheet(sheet);
    const tid = to.sheet === undefined ? sid : this.resolveSheet(to.sheet);
    const dRow = to.row - from.r0;
    const dCol = to.col - from.c0;
    checkCell(from.r1 + dRow, from.c1 + dCol);
    checkCell(to.row, to.col);
    const spec = { sheet: this.sheetNameOf(sid), ...from, dRow, dCol, toSheet: this.sheetNameOf(tid) };
    this.rewriteAll(
      (text, f) => moveFormula(text, spec, f),
      (def) => (def.scope === null ? '\u0000' : this.sheetNameOf(def.scope)),
    );
    const inSrc = (f: FormulaCell) =>
      f.sheet === sid && f.row >= from.r0 && f.row <= from.r1 && f.col >= from.c0 && f.col <= from.c1;
    const inDest = (f: FormulaCell) =>
      f.sheet === tid &&
      f.row >= from.r0 + dRow &&
      f.row <= from.r1 + dRow &&
      f.col >= from.c0 + dCol &&
      f.col <= from.c1 + dCol;
    // Literal values move with the block.
    if (this.host.setCellValue) {
      const vals: [number, number, Scalar | undefined][] = [];
      for (let r = from.r0; r <= from.r1; r++)
        for (let c = from.c0; c <= from.c1; c++) {
          vals.push([r, c, this.host.getCellValue(sid, r, c)]);
          this.host.setCellValue(sid, r, c, undefined);
        }
      for (const [r, c, v] of vals) this.host.setCellValue(tid, r + dRow, c + dCol, v);
    }
    const all = [...this.formulas.values()];
    for (const fc of all) this.graph.remove(fc);
    this.formulas.clear();
    const moved = all.filter(inSrc);
    for (const fc of all) {
      if (inSrc(fc) || inDest(fc)) continue;
      this.formulas.set(fc.key, fc);
    }
    for (const fc of moved) {
      fc.sheet = tid;
      fc.row += dRow;
      fc.col += dCol;
      this.formulas.set(fc.key, fc);
    }
    for (const fc of this.formulas.values()) if (fc.spill !== 'none') fc.spill = 'blocked';
    this.recomputeBounds();
    this.rebuildAll(false);
  }

  // ── recalculation ──────────────────────────────────────────────────────

  /** Recalculates everything that changed since the last recalc (plus volatile formulas). */
  recalc(): RecalcResult {
    this.changed = new Set();
    this.evaluatedCount = 0;
    if (this.volatiles.size > 0) this.markDirty([...this.volatiles]);
    let passes = 0;
    while (this.dirty.size > 0 && passes < MAX_PASSES) {
      passes++;
      this.runPass([...this.dirty]);
    }
    const circular = [...this.formulas.values()].filter((f) => f.circular).map((f) => this.addressOf(f));
    return {
      evaluated: this.evaluatedCount,
      changed: [...this.changed].map((f) => this.addressOf(f)),
      circular,
    };
  }

  /** Recalculates every formula. */
  recalcAll(): RecalcResult {
    this.markDirty([...this.formulas.values()]);
    return this.recalc();
  }

  private runPass(nodes: FormulaCell[]): void {
    const graph = this.graph;
    for (const n of nodes) n.indeg = 0;
    const buf: FormulaCell[] = [];
    for (const n of nodes) {
      buf.length = 0;
      graph.dependentsOf(n, buf);
      const out: FormulaCell[] = [];
      for (const d of buf) {
        if (d.dirty) {
          out.push(d);
          d.indeg++;
        }
      }
      n.out = out;
    }
    const queue: FormulaCell[] = [];
    for (const n of nodes) if (n.indeg === 0) queue.push(n);
    for (let i = 0; i < queue.length; i++) {
      const n = queue[i] as FormulaCell;
      if (n.dirty) this.evaluateCell(n);
      for (const d of n.out as FormulaCell[]) {
        if (--d.indeg === 0) queue.push(d);
      }
    }
    if (queue.length < nodes.length) {
      const left = nodes.filter((n) => n.indeg > 0);
      for (const scc of tarjan(left)) {
        const only = scc[0] as FormulaCell;
        if (scc.length === 1 && !(only.out as FormulaCell[]).includes(only)) {
          if (only.dirty) this.evaluateCell(only);
        } else {
          this.resolveCycle(scc);
        }
      }
      for (const n of left) {
        n.indeg = 0;
        n.index = -1;
      }
    }
    for (const n of nodes) n.out = null;
  }

  private resolveCycle(members: FormulaCell[]): void {
    members.sort((a, b) => a.sheet - b.sheet || a.row - b.row || a.col - b.col);
    for (const m of members) {
      m.dirty = false;
      this.dirty.delete(m);
    }
    if (!this.iterativeEnabled) {
      for (const m of members) {
        this.applyResult(m, 0);
        m.circular = true;
      }
      return;
    }
    for (let it = 0; it < this.maxIterations; it++) {
      let maxDelta = 0;
      for (const m of members) {
        const before = numericOf(m.value);
        this.evaluateCell(m);
        const delta = Math.abs(numericOf(m.value) - before);
        if (delta > maxDelta || Number.isNaN(delta))
          maxDelta = Number.isNaN(delta) ? Number.POSITIVE_INFINITY : delta;
      }
      if (maxDelta <= this.maxChange) break;
    }
    for (const m of members) m.circular = true;
  }

  private evaluateCell(fc: FormulaCell): void {
    fc.evaluating = true;
    fc.circular = false;
    let value: Value;
    try {
      value = new Evaluator(this, fc.sheet, fc.row, fc.col, fc.legacy).evaluateFormula(fc.ast);
    } finally {
      fc.evaluating = false;
    }
    fc.dirty = false;
    this.dirty.delete(fc);
    this.evaluatedCount++;
    this.applyResult(fc, value);
  }

  private applyResult(fc: FormulaCell, value: Value): void {
    const oldValue = fc.value;
    const oldSpill = fc.spill;
    const oldClaim: Rect | null = fc.claim ? { ...fc.claim } : null;
    if (value instanceof Array2D && value.size > 1) {
      const rect = {
        r0: fc.row,
        c0: fc.col,
        r1: Math.min(fc.row + value.rows - 1, MAX_ROWS - 1),
        c1: Math.min(fc.col + value.cols - 1, MAX_COLS - 1),
      };
      const outOfSheet = fc.row + value.rows > MAX_ROWS || fc.col + value.cols > MAX_COLS;
      if (!oldClaim || !sameRect(oldClaim, rect)) this.graph.setClaim(fc, rect);
      fc.spill = outOfSheet || this.spillBlocked(fc, rect) ? 'blocked' : 'ok';
      fc.value = value;
      if (fc.spill === 'ok') this.growBounds(fc.sheet, rect.r1, rect.c1);
    } else {
      if (fc.claim) this.graph.setClaim(fc, null);
      fc.spill = 'none';
      fc.value = value;
    }
    const valueChanged = oldSpill !== fc.spill || !valuesEqual(oldValue, fc.value);
    if (valueChanged) this.changed.add(fc);
    const newClaim: Rect | null = fc.claim ? { ...fc.claim } : null;
    if (!oldClaim && !newClaim) return;
    const rectChanged = !oldClaim || !newClaim || !sameRect(oldClaim, newClaim) || oldSpill !== fc.spill;
    if (!valueChanged && !rectChanged) return;
    const affected: FormulaCell[] = [];
    for (const r of [oldClaim, newClaim]) {
      if (!r) continue;
      this.graph.dependentsInRect(fc.sheet, r, affected);
      if (rectChanged) this.graph.claimsOverlapping(fc.sheet, r, (c) => affected.push(c.anchor));
    }
    const stale = affected.filter((a) => a !== fc && !a.dirty && !a.evaluating);
    if (stale.length > 0) this.markDirty(stale);
  }

  private spillBlocked(fc: FormulaCell, rect: Rect): boolean {
    for (let r = rect.r0; r <= rect.r1; r++) {
      for (let c = rect.c0; c <= rect.c1; c++) {
        if (r === fc.row && c === fc.col) continue;
        if (this.formulas.has(cellKey(fc.sheet, r, c))) return true;
        const v = this.host.getCellValue(fc.sheet, r, c);
        if (v !== undefined && v !== BLANK) return true;
      }
    }
    let blocked = false;
    this.graph.claimsOverlapping(fc.sheet, rect, (c) => {
      if (c.anchor !== fc && c.anchor.spill === 'ok') blocked = true;
    });
    return blocked;
  }

  private markDirty(start: Iterable<FormulaCell>): void {
    const stack: FormulaCell[] = [];
    for (const s of start) {
      if (!s.dirty && this.formulas.get(s.key) === s) {
        s.dirty = true;
        this.dirty.add(s);
        stack.push(s);
      }
    }
    const buf: FormulaCell[] = [];
    while (stack.length > 0) {
      const fc = stack.pop() as FormulaCell;
      buf.length = 0;
      this.graph.dependentsOf(fc, buf);
      for (const d of buf) {
        if (!d.dirty) {
          d.dirty = true;
          this.dirty.add(d);
          stack.push(d);
        }
      }
    }
  }

  // ── EvalHost ───────────────────────────────────────────────────────────

  readCell(sheet: number, row: number, col: number): Scalar {
    const fc = this.formulas.get(cellKey(sheet, row, col));
    if (fc) {
      if (fc.evaluating) fc.circular = true;
      else if (fc.dirty) this.evaluateCell(fc);
      return anchorValue(fc);
    }
    const idx = this.graph.claims.get(sheet);
    if (idx && idx.size > 0) {
      const pending: FormulaCell[] = [];
      this.graph.claimsAt(sheet, row, col, (c) => {
        if (c.anchor.dirty && !c.anchor.evaluating) pending.push(c.anchor);
      });
      for (const a of pending) if (a.dirty) this.evaluateCell(a);
      const v = this.spillValueAt(sheet, row, col);
      if (v !== undefined) return v;
    }
    return this.host.getCellValue(sheet, row, col) ?? BLANK;
  }

  private spillValueAt(sheet: number, row: number, col: number): Scalar | undefined {
    let out: Scalar | undefined;
    this.graph.claimsAt(sheet, row, col, (c) => {
      const a = c.anchor;
      if (a.spill === 'ok' && a.value instanceof Array2D) out = a.value.get(row - a.row, col - a.col);
    });
    return out;
  }

  isFormulaCell(sheet: number, row: number, col: number): boolean {
    return this.formulas.has(cellKey(sheet, row, col));
  }

  usedBounds(sheet: number): { rows: number; cols: number } {
    const h = this.host.getUsedBounds?.(sheet) ?? { rows: MAX_ROWS, cols: MAX_COLS };
    const f = this.formulaBounds.get(sheet);
    if (!f) return h;
    return { rows: Math.max(h.rows, f.rows), cols: Math.max(h.cols, f.cols) };
  }

  getTable(name: string): TableDef | undefined {
    return this.host.getTable?.(name);
  }

  getTableAt(sheet: number, row: number, col: number): TableDef | undefined {
    return this.host.getTableAt?.(sheet, row, col);
  }

  spillArea(sheet: number, row: number, col: number): Area | ErrorValue {
    const fc = this.formulas.get(cellKey(sheet, row, col));
    if (!fc) return Errors.REF;
    if (fc.dirty && !fc.evaluating) this.evaluateCell(fc);
    if (fc.spill === 'ok' && fc.claim)
      return { sheet, r0: fc.claim.r0, c0: fc.claim.c0, r1: fc.claim.r1, c1: fc.claim.c1 };
    if (fc.spill === 'blocked') return anchorValue(fc) as ErrorValue;
    if (fc.value instanceof ErrorValue) return fc.value;
    return { sheet, r0: row, c0: col, r1: row, c1: col };
  }

  // ── ad-hoc evaluation and the Formula Lab ──────────────────────────────

  /** Evaluates a formula without storing it (as if it were in `at`, default the first sheet's A1). */
  evaluate(formula: string, at?: { sheet?: SheetRef; row?: number; col?: number; legacy?: boolean }): Value {
    const ast = parse(formula);
    const { sid, row, col } = this.position(at);
    return new Evaluator(this, sid, row, col, at?.legacy ?? false).evaluateFormula(ast);
  }

  /**
   * Evaluates a formula step by step for the Formula Lab: every sub-expression with its source text,
   * span and value; branches that were not evaluated (lazy IF arms) are marked `evaluated: false`.
   */
  explain(formula: string, at?: { sheet?: SheetRef; row?: number; col?: number }): ExplainStep {
    const ast = parse(formula);
    const { sid, row, col } = this.position(at);
    const trace = new Map();
    const ev = new Evaluator(this, sid, row, col, false, trace);
    const result = ev.evaluateFormula(ast);
    return buildExplainTree(formula, ast, trace, result, ev, (id) => this.sheetName(id));
  }

  private position(at?: { sheet?: SheetRef; row?: number; col?: number }): {
    sid: number;
    row: number;
    col: number;
  } {
    let sid: number;
    if (at?.sheet !== undefined) sid = this.resolveSheet(at.sheet);
    else if (this.sheetOrder.length > 0) sid = this.sheetOrder[0] as number;
    else sid = this.addSheet('Sheet1');
    return { sid, row: at?.row ?? 0, col: at?.col ?? 0 };
  }

  // ── internals ──────────────────────────────────────────────────────────

  private register(fc: FormulaCell): void {
    const p: Precedents = collectPrecedents(fc.ast, fc.sheet, fc.row, fc.col, this);
    this.graph.add(fc, p);
    if (p.volatile) this.volatiles.add(fc);
    else this.volatiles.delete(fc);
  }

  private reregister(fc: FormulaCell): void {
    this.graph.remove(fc, true);
    this.register(fc);
  }

  private removeFormula(fc: FormulaCell): void {
    const affected: FormulaCell[] = [];
    this.graph.dependentsOf(fc, affected);
    if (fc.claim) this.graph.claimsOverlapping(fc.sheet, fc.claim, (c) => affected.push(c.anchor));
    this.graph.remove(fc);
    this.formulas.delete(fc.key);
    this.volatiles.delete(fc);
    this.dirty.delete(fc);
    fc.dirty = false;
    this.markDirty(affected.filter((a) => a !== fc));
  }

  /** Re-registers every formula (after renames/structural edits) and marks everything dirty. */
  private rebuildAll(keepClaims: boolean): void {
    const all = [...this.formulas.values()];
    for (const fc of all) {
      this.graph.remove(fc, keepClaims);
      fc.dirty = false;
    }
    this.dirty.clear();
    this.volatiles.clear();
    for (const fc of all) this.register(fc);
    this.markDirty(all);
  }

  private rewriteAll(
    fn: (text: string, formulaSheet: string) => string,
    nameSheet: (def: NameDef) => string,
  ): void {
    for (const fc of this.formulas.values()) {
      const next = fn(fc.text, this.sheetNameOf(fc.sheet));
      if (next !== fc.text) {
        fc.text = next;
        fc.ast = parse(next);
      }
    }
    for (const def of this.names.values()) {
      const next = fn(def.text, nameSheet(def));
      if (next !== def.text) {
        def.text = next;
        def.ast = parse(next);
      }
    }
  }

  private growBounds(sheet: number, row: number, col: number): void {
    const b = this.formulaBounds.get(sheet);
    if (!b) this.formulaBounds.set(sheet, { rows: row + 1, cols: col + 1 });
    else {
      if (row >= b.rows) b.rows = row + 1;
      if (col >= b.cols) b.cols = col + 1;
    }
  }

  private recomputeBounds(): void {
    this.formulaBounds.clear();
    for (const fc of this.formulas.values()) this.growBounds(fc.sheet, fc.row, fc.col);
  }

  private addressOf(fc: FormulaCell): CellAddress {
    return { sheet: this.sheetNameOf(fc.sheet), row: fc.row, col: fc.col };
  }
}

function anchorValue(fc: FormulaCell): Scalar {
  const v = fc.value;
  if (fc.spill === 'blocked') return new ErrorValue('#SPILL!', 'The spill range is not blank');
  if (v instanceof Array2D) return v.get(0, 0);
  return v as Scalar;
}

function sameRect(a: Rect, b: Rect): boolean {
  return a.r0 === b.r0 && a.c0 === b.c0 && a.r1 === b.r1 && a.c1 === b.c1;
}

function scalarsEqual(x: Scalar, y: Scalar): boolean {
  return x === y || (x instanceof ErrorValue && y instanceof ErrorValue && x.code === y.code);
}

function valuesEqual(a: Value, b: Value): boolean {
  if (a === b) return true;
  if (a instanceof Array2D && b instanceof Array2D) {
    if (a.rows !== b.rows || a.cols !== b.cols) return false;
    for (let r = 0; r < a.rows; r++) {
      for (let c = 0; c < a.cols; c++) if (!scalarsEqual(a.get(r, c), b.get(r, c))) return false;
    }
    return true;
  }
  if (a instanceof Array2D || b instanceof Array2D) return false;
  if (typeof a === 'object' && !(a instanceof ErrorValue)) return false; // lambdas
  return scalarsEqual(a as Scalar, b as Scalar);
}

function numericOf(v: Value): number {
  if (typeof v === 'number') return v;
  if (typeof v === 'boolean') return v ? 1 : 0;
  if (v instanceof Array2D) return numericOf(v.get(0, 0));
  return 0;
}

function checkCell(row: number, col: number): void {
  if (!Number.isInteger(row) || row < 0 || row >= MAX_ROWS)
    throw new RangeError(`Row ${row} is out of range`);
  if (!Number.isInteger(col) || col < 0 || col >= MAX_COLS)
    throw new RangeError(`Column ${col} is out of range`);
}

function nameKey(name: string, scope: number | null): string {
  return `${scope ?? '*'}|${name.toUpperCase()}`;
}

/** Excel's rules for defined names: letter/_/\ start, no spaces, not a cell reference, not TRUE/FALSE. */
export function isValidName(name: string): boolean {
  if (name.length === 0 || name.length > 255) return false;
  if (!isNameStart(name.charCodeAt(0))) return false;
  if (!/^[^\s!"#$%&'()*+,\-/:;<=>@[\]^`{|}~]+$/.test(name)) return false;
  const u = name.toUpperCase();
  if (u === 'TRUE' || u === 'FALSE' || u === 'R' || u === 'C') return false;
  const a1 = matchA1Address(name, 0);
  if (a1 && a1.end === name.length) return false;
  const r1 = matchR1C1Address(name, 0, { row: 0, col: 0 });
  if (r1 && r1.end === name.length) return false;
  return true;
}
