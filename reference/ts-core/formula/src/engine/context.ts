/**
 * What a function implementation can do with its evaluation context.
 */
import type { Node } from '../parser/ast.ts';
import type { ErrorValue } from '../values/errors.ts';
import type { Area, Array2D, EvalValue, Lambda, RefValue, Scalar, Scope, Value } from '../values/types.ts';
import type { TableDef } from './host.ts';

export interface CallContext {
  /** Cell being calculated (0-based). */
  readonly sheet: number;
  readonly row: number;
  readonly col: number;
  /** Legacy (pre-dynamic-array) semantics: implicit intersection instead of lifting/spilling. */
  readonly legacy: boolean;
  readonly date1904: boolean;
  readonly scope: Scope | null;
  /** Current time for NOW/TODAY. */
  now(): Date;
  /** Evaluates a node in this context (optionally in another lexical scope). */
  evaluate(node: Node, scope?: Scope | null): EvalValue;
  /** Dereferences a reference to its value (single cell → scalar, area → array). Missing → blank. */
  toValue(v: EvalValue): Value;
  /** Reduces to one scalar (implicit intersection for ranges, top-left for arrays). */
  toScalar(v: EvalValue): Scalar;
  /** Converts to an array (scalars become 1×1). Lambdas and multi-area references give an error. */
  toArray(v: EvalValue): Array2D | ErrorValue;
  /**
   * Visits every scalar of a value. `fromRef` is true for values that came from a range or array
   * (where Excel's aggregate functions ignore text and logicals). Return `false` to stop.
   */
  forEachScalar(v: EvalValue, cb: (value: Scalar, fromRef: boolean) => boolean | undefined): void;
  /** Value of one cell (evaluating it first if it is a pending formula). */
  readCell(sheet: number, row: number, col: number): Scalar;
  /** Calls a lambda with already evaluated arguments. */
  invoke(fn: Lambda, args: EvalValue[]): EvalValue;
  /** Resolves reference text (INDIRECT). */
  resolveReferenceText(text: string, a1: boolean): RefValue | ErrorValue;
  /** Used extent of a sheet, for clamping whole-column ranges. */
  usedBounds(sheet: number): { rows: number; cols: number };
  /** Area clamped to the used extent (at least one cell). */
  clampArea(area: Area): Area;
  sheetName(sheet: number): string | undefined;
  /** Sheet id for a sheet name (case-insensitive), or undefined. */
  sheetIdOf(name: string): number | undefined;
  /** Sheet ids in workbook order (SHEET, SHEETS). */
  sheetIds(): number[];
  getTable(name: string): TableDef | undefined;
  /** True when the cell holds a formula. */
  isFormulaCell(sheet: number, row: number, col: number): boolean;
  /** Formula text of a cell (with `=`), or undefined for literal and empty cells. */
  formulaText(sheet: number, row: number, col: number): string | undefined;
  /**
   * Visibility of a row for SUBTOTAL/AGGREGATE: `filtered` when hidden by an AutoFilter or advanced
   * filter, `hidden` when hidden manually (or collapsed by an outline), otherwise `visible`.
   */
  rowVisibility(sheet: number, row: number): 'visible' | 'hidden' | 'filtered';
  /** Records a hyperlink target for the cell being calculated (HYPERLINK). */
  setHyperlink(target: string): void;
}
