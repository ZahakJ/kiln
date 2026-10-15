/**
 * The AST interpreter.
 *
 * References evaluate to {@link RefValue}s and are dereferenced only where a value is needed, so
 * functions such as ROW, OFFSET and INDEX and the reference operators (`:`, space, `,`) see real
 * references. Function arguments listed in a descriptor's `lazy` are passed unevaluated; arguments
 * listed in `lift` are lifted over arrays. Dynamic-array semantics apply unless `legacy` is set, in
 * which case ranges in single-value positions use implicit intersection.
 */
import type { BinaryNode, Node, RefNode, StructuredNode } from '../parser/ast.ts';
import { type ParseOptions, parse } from '../parser/parser.ts';
import { addressBounds, MAX_COLS, MAX_ROWS } from '../parser/references.ts';
import { type CoercionOptions, toNumber, toText } from '../values/coerce.ts';
import { compareScalars } from '../values/compare.ts';
import { Errors, ErrorValue, errorFromCode } from '../values/errors.ts';
import {
  type Area,
  Array2D,
  BLANK,
  type EvalValue,
  Lambda,
  MISSING,
  RefValue,
  type Scalar,
  Scope,
  type Value,
} from '../values/types.ts';
import { dimsOf, liftBinary, liftUnary, pick } from './arrays.ts';
import type { CallContext } from './context.ts';
import {
  type Arg,
  type FunctionDescriptor,
  getFunction,
  LazyArg,
  normalizeFunctionName,
} from './functions/registry.ts';
import type { TableDef } from './host.ts';
import { resolveStructured } from './structured.ts';

/** What the evaluator needs from the engine. */
export interface EvalHost {
  readonly date1904: boolean;
  now(): Date;
  readCell(sheet: number, row: number, col: number): Scalar;
  isFormulaCell(sheet: number, row: number, col: number): boolean;
  sheetId(name: string): number | undefined;
  sheetName(id: number): string | undefined;
  /** Sheet ids from `a` to `b` in workbook order (3-D references). */
  sheetsBetween(a: number, b: number): number[] | undefined;
  usedBounds(sheet: number): { rows: number; cols: number };
  lookupName(name: string, sheet: number): { ast: Node; scopeSheet: number | null } | undefined;
  getTable(name: string): TableDef | undefined;
  getTableAt(sheet: number, row: number, col: number): TableDef | undefined;
  /** Spill area of the formula anchored at the cell (evaluating it first if needed). */
  spillArea(sheet: number, row: number, col: number): Area | ErrorValue;
  /** Formula text of a cell, if it holds a formula. */
  formulaText?(sheet: number, row: number, col: number): string | undefined;
  /** Sheet ids in workbook order. */
  sheetOrderIds?(): number[];
  isRowHidden?(sheet: number, row: number): boolean;
  isRowFiltered?(sheet: number, row: number): boolean;
}

export interface TraceEntry {
  value: EvalValue;
  count: number;
}

const MAX_LAMBDA_DEPTH = 400;
const MAX_NAME_DEPTH = 32;

export class Evaluator implements CallContext {
  scope: Scope | null = null;
  /** Hyperlink target recorded by HYPERLINK while evaluating this cell. */
  hyperlink: string | undefined;
  private lambdaDepth = 0;
  private nameDepth = 0;
  readonly coercion: CoercionOptions;

  constructor(
    readonly host: EvalHost,
    readonly sheet: number,
    readonly row: number,
    readonly col: number,
    readonly legacy = false,
    readonly trace?: Map<Node, TraceEntry>,
  ) {
    this.coercion = { date1904: host.date1904 };
  }

  get date1904(): boolean {
    return this.host.date1904;
  }

  now(): Date {
    return this.host.now();
  }

  evaluate(node: Node, scope?: Scope | null): EvalValue {
    if (scope === undefined || scope === this.scope) return this.eval(node);
    const saved = this.scope;
    this.scope = scope;
    try {
      return this.eval(node);
    } finally {
      this.scope = saved;
    }
  }

  /** Evaluates a whole formula and reduces the result to what a cell holds. */
  evaluateFormula(node: Node): Value {
    try {
      return this.finalize(this.eval(node));
    } catch (e) {
      if (e instanceof RangeError) return new ErrorValue('#NUM!', 'The formula is nested too deeply');
      throw e;
    }
  }

  /** Converts an evaluation result to a cell value (dynamic arrays, or implicit intersection). */
  finalize(v: EvalValue): Value {
    if (v instanceof Lambda) return Errors.CALC;
    if (v === MISSING || v === BLANK) return 0;
    if (v instanceof RefValue) {
      if (this.legacy) return blankToZero(this.implicitIntersection(v));
      const value = this.toValue(v);
      return value instanceof Array2D ? value.map(blankToZero) : blankToZero(value as Scalar);
    }
    if (v instanceof Array2D) {
      if (this.legacy || v.size === 1) return blankToZero(v.get(0, 0));
      return v.map(blankToZero);
    }
    return v;
  }

  private eval(node: Node): EvalValue {
    const v = this.evalNode(node);
    if (this.trace) {
      const t = this.trace.get(node);
      if (t) {
        t.value = v;
        t.count++;
      } else this.trace.set(node, { value: v, count: 1 });
    }
    return v;
  }

  private evalNode(node: Node): EvalValue {
    switch (node.type) {
      case 'number':
        return node.value;
      case 'string':
        return node.value;
      case 'boolean':
        return node.value;
      case 'error':
        return errorFromCode(node.code) ?? Errors.VALUE;
      case 'empty':
        return MISSING;
      case 'ref':
        return this.evalRef(node);
      case 'name':
        return this.evalName(node.name, node.prefix?.sheet);
      case 'structured':
        return this.evalStructured(node);
      case 'array':
        return new Array2D(
          node.rows.map((r) =>
            r.map((e) =>
              e.type === 'error' ? (errorFromCode(e.code) ?? Errors.VALUE) : (e.value as Scalar),
            ),
          ),
        );
      case 'paren':
        return this.eval(node.expr);
      case 'unary': {
        if (node.op === '+') return this.eval(node.operand);
        const v = this.eval(node.operand);
        if (node.op === '@') {
          if (v instanceof RefValue) return this.implicitIntersection(v);
          if (v instanceof Array2D) return v.get(0, 0);
          return v;
        }
        return this.liftNumeric(v, (x) => -x);
      }
      case 'postfix': {
        if (node.op === '%') return this.liftNumeric(this.eval(node.operand), (x) => x / 100);
        const ref = this.eval(node.operand);
        if (!(ref instanceof RefValue) || !ref.isSingleCell) return Errors.REF;
        const a = ref.areas[0] as Area;
        const area = this.host.spillArea(a.sheet, a.r0, a.c0);
        return area instanceof ErrorValue ? area : RefValue.area(area);
      }
      case 'binary':
        return this.evalBinary(node);
      case 'call':
        return this.evalCall(node.name, node.args);
      case 'invoke': {
        const callee = this.eval(node.callee);
        if (callee instanceof ErrorValue) return callee;
        if (!(callee instanceof Lambda)) return Errors.VALUE;
        return this.invoke(
          callee,
          node.args.map((a) => this.eval(a)),
        );
      }
    }
  }

  // ── references ─────────────────────────────────────────────────────────

  private resolveSheets(node: {
    prefix?: { workbook?: string; sheet: string; sheet2?: string };
  }): number[] | ErrorValue {
    const p = node.prefix;
    if (!p) return [this.sheet];
    if (p.workbook !== undefined) return Errors.REF;
    const a = this.host.sheetId(p.sheet);
    if (a === undefined) return Errors.REF;
    if (p.sheet2 === undefined) return [a];
    const b = this.host.sheetId(p.sheet2);
    if (b === undefined) return Errors.REF;
    return this.host.sheetsBetween(a, b) ?? Errors.REF;
  }

  private evalRef(node: RefNode): EvalValue {
    const sheets = this.resolveSheets(node);
    if (sheets instanceof ErrorValue) return sheets;
    const b = addressBounds(node.address);
    if (sheets.length === 1) return RefValue.area({ sheet: sheets[0] as number, ...b });
    return new RefValue(sheets.map((sheet) => ({ sheet, ...b })));
  }

  private evalName(name: string, prefixSheet?: string): EvalValue {
    if (prefixSheet === undefined && this.scope) {
      const local = this.scope.lookup(name);
      if (local !== undefined) return local;
    }
    let sheet = this.sheet;
    if (prefixSheet !== undefined) {
      const id = this.host.sheetId(prefixSheet);
      if (id === undefined) return Errors.REF;
      sheet = id;
    }
    const def = this.host.lookupName(name, sheet);
    if (def) {
      if (this.nameDepth >= MAX_NAME_DEPTH)
        return new ErrorValue('#NAME?', 'A defined name refers to itself');
      this.nameDepth++;
      const saved = this.scope;
      this.scope = null;
      try {
        return this.eval(def.ast);
      } finally {
        this.scope = saved;
        this.nameDepth--;
      }
    }
    if (prefixSheet === undefined) {
      const table = this.host.getTable(name);
      if (table) return this.tableRef(table, [], false, null);
    }
    return new ErrorValue('#NAME?', `"${name}" is not a defined name or function`);
  }

  private evalStructured(node: StructuredNode): EvalValue {
    const table = node.table
      ? this.host.getTable(node.table)
      : this.host.getTableAt(this.sheet, this.row, this.col);
    if (!table) return Errors.REF;
    return this.tableRef(table, node.specials, node.thisRow, node.columns);
  }

  private tableRef(
    t: TableDef,
    specials: readonly string[],
    thisRow: boolean,
    columns: readonly string[] | null,
  ): RefValue | ErrorValue {
    const area = resolveStructured(t, specials, thisRow, columns, this);
    return area instanceof ErrorValue ? area : RefValue.area(area);
  }

  // ── operators ──────────────────────────────────────────────────────────

  /** Evaluates and dereferences (fast path for plain cell references). */
  private evalValue(node: Node): Value {
    if (node.type === 'ref' && !node.prefix && node.address.kind === 'cell') {
      const v = this.readCell(this.sheet, node.address.row, node.address.col);
      if (this.trace) this.eval(node);
      return v;
    }
    return this.toValue(this.eval(node));
  }

  private evalBinary(node: BinaryNode): EvalValue {
    const op = node.op;
    if (op === ':' || op === ' ' || op === ',') {
      const l = this.eval(node.left);
      if (l instanceof ErrorValue) return l;
      const r = this.eval(node.right);
      if (r instanceof ErrorValue) return r;
      if (!(l instanceof RefValue) || !(r instanceof RefValue)) return Errors.VALUE;
      if (op === ',') return new RefValue([...l.areas, ...r.areas]);
      if (op === ':') return rangeUnion(l, r);
      return intersect(l, r);
    }
    const l = this.evalValue(node.left);
    const r = this.evalValue(node.right);
    if (l instanceof Lambda || r instanceof Lambda) return Errors.VALUE;
    switch (op) {
      case '&':
        return liftBinary(l, r, (x, y) => {
          const a = toText(x);
          if (a instanceof ErrorValue) return a;
          const b = toText(y);
          if (b instanceof ErrorValue) return b;
          return a + b;
        });
      case '=':
      case '<>':
      case '<':
      case '>':
      case '<=':
      case '>=':
        return liftBinary(l, r, (x, y) => {
          const c = compareScalars(x, y);
          if (c instanceof ErrorValue) return c;
          switch (op) {
            case '=':
              return c === 0;
            case '<>':
              return c !== 0;
            case '<':
              return c < 0;
            case '>':
              return c > 0;
            case '<=':
              return c <= 0;
            default:
              return c >= 0;
          }
        });
      default:
        return liftBinary(l, r, (x, y) => arithmetic(op, x, y, this.coercion));
    }
  }

  private liftNumeric(v: EvalValue, fn: (x: number) => number): EvalValue {
    const value = this.toValue(v);
    if (value instanceof Lambda) return Errors.VALUE;
    return liftUnary(value, (x) => {
      const n = toNumber(x, this.coercion);
      return n instanceof ErrorValue ? n : finite(fn(n));
    });
  }

  // ── functions and lambdas ──────────────────────────────────────────────

  private evalCall(rawName: string, argNodes: readonly Node[]): EvalValue {
    const desc = getFunction(rawName);
    if (!desc) {
      const name = normalizeFunctionName(rawName);
      const target = this.scope?.lookup(name) ?? this.evalName(name);
      if (target instanceof Lambda)
        return this.invoke(
          target,
          argNodes.map((a) => this.eval(a)),
        );
      if (target instanceof ErrorValue && target.code !== '#NAME?') return target;
      return new ErrorValue('#NAME?', `Unknown function ${name}`);
    }
    if (argNodes.length < desc.minArgs || argNodes.length > desc.maxArgs) {
      return new ErrorValue('#VALUE!', `${desc.name} takes ${describeArity(desc)}`);
    }
    const lazy = desc.lazy;
    const args: Arg[] = new Array(argNodes.length);
    for (let i = 0; i < argNodes.length; i++) {
      const n = argNodes[i] as Node;
      args[i] = lazy?.includes(i) ? new LazyArg(n, this, this.scope) : this.eval(n);
    }
    if (desc.lift) return this.callLifted(desc, args);
    return desc.impl(args, this);
  }

  private callLifted(desc: FunctionDescriptor, args: Arg[]): EvalValue {
    const lift = desc.lift as readonly number[] | 'all';
    const isLifted = (i: number) => lift === 'all' || lift.includes(i);
    let anyArray = false;
    for (let i = 0; i < args.length; i++) {
      if (!isLifted(i)) continue;
      const a = args[i] as Arg;
      if (a instanceof LazyArg || a === MISSING) continue;
      const v = this.liftable(a as EvalValue);
      args[i] = v;
      if (v instanceof Array2D) anyArray = true;
    }
    if (!anyArray) return desc.impl(args, this);
    // Force lifted lazy arguments once; they take part in the broadcast.
    for (let i = 0; i < args.length; i++) {
      const a = args[i] as Arg;
      if (isLifted(i) && a instanceof LazyArg) args[i] = this.liftable(a.force());
    }
    const operands = args.filter((a, i) => isLifted(i) && a instanceof Array2D) as Array2D[];
    const { rows, cols } = dimsOf(operands);
    return Array2D.build(rows, cols, (r, c) => {
      const call = args.map((a, i) => (isLifted(i) && a instanceof Array2D ? pick(a, r, c) : a));
      const out = desc.impl(call, this);
      return this.toScalar(out);
    });
  }

  /** Dereference for a lifted position: a scalar or an array (implicit intersection when legacy). */
  private liftable(v: EvalValue): Scalar | Array2D {
    if (v === MISSING) return BLANK;
    if (v instanceof RefValue) {
      if (this.legacy) return this.implicitIntersection(v);
      const val = this.toValue(v);
      return val instanceof Lambda ? Errors.VALUE : val;
    }
    if (v instanceof Lambda) return Errors.VALUE;
    if (v instanceof Array2D && v.size === 1) return v.get(0, 0);
    return v as Scalar | Array2D;
  }

  invoke(fn: Lambda, args: EvalValue[]): EvalValue {
    if (args.length > fn.params.length) {
      return new ErrorValue('#VALUE!', `The LAMBDA expects ${fn.params.length} argument(s)`);
    }
    if (this.lambdaDepth >= MAX_LAMBDA_DEPTH) return new ErrorValue('#NUM!', 'Too many nested LAMBDA calls');
    const scope = new Scope(fn.closure);
    for (let i = 0; i < fn.params.length; i++)
      scope.set(fn.params[i] as string, i < args.length ? (args[i] as EvalValue) : MISSING);
    this.lambdaDepth++;
    try {
      return this.evaluate(fn.body, scope);
    } finally {
      this.lambdaDepth--;
    }
  }

  // ── CallContext helpers ────────────────────────────────────────────────

  readCell(sheet: number, row: number, col: number): Scalar {
    return this.host.readCell(sheet, row, col);
  }

  isFormulaCell(sheet: number, row: number, col: number): boolean {
    return this.host.isFormulaCell(sheet, row, col);
  }

  formulaText(sheet: number, row: number, col: number): string | undefined {
    return this.host.formulaText?.(sheet, row, col);
  }

  rowVisibility(sheet: number, row: number): 'visible' | 'hidden' | 'filtered' {
    if (!this.host.isRowHidden?.(sheet, row)) return 'visible';
    return this.host.isRowFiltered?.(sheet, row) ? 'filtered' : 'hidden';
  }

  setHyperlink(target: string): void {
    this.hyperlink = target;
  }

  usedBounds(sheet: number): { rows: number; cols: number } {
    return this.host.usedBounds(sheet);
  }

  sheetName(sheet: number): string | undefined {
    return this.host.sheetName(sheet);
  }

  sheetIdOf(name: string): number | undefined {
    return this.host.sheetId(name);
  }

  sheetIds(): number[] {
    return this.host.sheetOrderIds?.() ?? [this.sheet];
  }

  getTable(name: string): TableDef | undefined {
    return this.host.getTable(name);
  }

  clampArea(a: Area): Area {
    const whole = a.r0 === 0 && a.r1 === MAX_ROWS - 1;
    const wholeRow = a.c0 === 0 && a.c1 === MAX_COLS - 1;
    if (!whole && !wholeRow) return a;
    const b = this.host.usedBounds(a.sheet);
    const r1 = whole ? Math.max(a.r0, Math.min(a.r1, b.rows - 1)) : a.r1;
    const c1 = wholeRow ? Math.max(a.c0, Math.min(a.c1, b.cols - 1)) : a.c1;
    return { sheet: a.sheet, r0: a.r0, c0: a.c0, r1, c1 };
  }

  toValue(v: EvalValue): Value {
    if (v instanceof RefValue) {
      if (v.areas.length !== 1) return Errors.VALUE;
      const a = v.areas[0] as Area;
      if (a.r0 === a.r1 && a.c0 === a.c1) return this.readCell(a.sheet, a.r0, a.c0);
      const c = this.clampArea(a);
      return Array2D.build(c.r1 - c.r0 + 1, c.c1 - c.c0 + 1, (r, col) =>
        this.readCell(c.sheet, c.r0 + r, c.c0 + col),
      );
    }
    if (v === MISSING) return BLANK;
    return v as Value;
  }

  toScalar(v: EvalValue): Scalar {
    if (v instanceof RefValue) return this.implicitIntersection(v);
    if (v instanceof Array2D) return v.get(0, 0);
    if (v instanceof Lambda) return Errors.CALC;
    if (v === MISSING) return BLANK;
    return v as Scalar;
  }

  toArray(v: EvalValue): Array2D | ErrorValue {
    const val = this.toValue(v);
    if (val instanceof Array2D) return val;
    if (val instanceof Lambda) return Errors.VALUE;
    if (val instanceof ErrorValue && v instanceof RefValue && v.areas.length !== 1) return val;
    return new Array2D([[val]]);
  }

  forEachScalar(v: EvalValue, cb: (value: Scalar, fromRef: boolean) => boolean | undefined): void {
    if (v instanceof RefValue) {
      for (const area of v.areas) {
        const a = this.clampArea(area);
        for (let r = a.r0; r <= a.r1; r++) {
          for (let c = a.c0; c <= a.c1; c++) {
            if (cb(this.readCell(a.sheet, r, c), true) === false) return;
          }
        }
      }
      return;
    }
    if (v instanceof Array2D) {
      for (const row of v.data) for (const x of row) if (cb(x, true) === false) return;
      return;
    }
    if (v === MISSING) return;
    if (v instanceof Lambda) {
      cb(Errors.VALUE, false);
      return;
    }
    cb(v as Scalar, false);
  }

  /** Excel's implicit intersection of a range with the formula's row or column. */
  implicitIntersection(ref: RefValue): Scalar {
    if (ref.areas.length !== 1) return Errors.VALUE;
    const a = ref.areas[0] as Area;
    if (a.r0 === a.r1 && a.c0 === a.c1) return this.readCell(a.sheet, a.r0, a.c0);
    if (a.sheet !== this.sheet) return Errors.VALUE;
    if (a.c0 === a.c1 && this.row >= a.r0 && this.row <= a.r1) return this.readCell(a.sheet, this.row, a.c0);
    if (a.r0 === a.r1 && this.col >= a.c0 && this.col <= a.c1) return this.readCell(a.sheet, a.r0, this.col);
    return Errors.VALUE;
  }

  resolveReferenceText(text: string, a1: boolean): RefValue | ErrorValue {
    const opts: ParseOptions = a1 ? {} : { style: 'R1C1', anchor: { row: this.row, col: this.col } };
    let ast: Node;
    try {
      ast = parse(text.trim().replace(/^=/, ''), opts);
    } catch {
      return Errors.REF;
    }
    if (!isReferenceExpression(ast)) return Errors.REF;
    const saved = this.scope;
    this.scope = null;
    try {
      const v = this.eval(ast);
      return v instanceof RefValue ? v : Errors.REF;
    } finally {
      this.scope = saved;
    }
  }
}

function isReferenceExpression(n: Node): boolean {
  switch (n.type) {
    case 'ref':
    case 'name':
    case 'structured':
      return true;
    case 'paren':
      return isReferenceExpression(n.expr);
    case 'binary':
      return (
        (n.op === ':' || n.op === ' ' || n.op === ',') &&
        isReferenceExpression(n.left) &&
        isReferenceExpression(n.right)
      );
    case 'postfix':
      return n.op === '#' && isReferenceExpression(n.operand);
    default:
      return false;
  }
}

function blankToZero(v: Scalar): Scalar {
  return v === BLANK ? 0 : v;
}

function finite(x: number): number | ErrorValue {
  return Number.isFinite(x) ? x : Errors.NUM;
}

export function arithmetic(op: string, a: Scalar, b: Scalar, opts?: CoercionOptions): Scalar {
  const x = toNumber(a, opts);
  if (x instanceof ErrorValue) return x;
  const y = toNumber(b, opts);
  if (y instanceof ErrorValue) return y;
  switch (op) {
    case '+':
      return finite(x + y);
    case '-':
      return finite(x - y);
    case '*':
      return finite(x * y);
    case '/':
      return y === 0 ? Errors.DIV0 : finite(x / y);
    case '^': {
      if (x === 0 && y === 0) return Errors.NUM;
      if (x === 0 && y < 0) return Errors.DIV0;
      const r = x ** y;
      return Number.isNaN(r) ? Errors.NUM : finite(r);
    }
    default:
      return Errors.VALUE;
  }
}

function rangeUnion(l: RefValue, r: RefValue): RefValue | ErrorValue {
  const all = [...l.areas, ...r.areas];
  const sheet = (all[0] as Area).sheet;
  if (all.some((a) => a.sheet !== sheet)) return Errors.VALUE;
  return RefValue.area({
    sheet,
    r0: Math.min(...all.map((a) => a.r0)),
    c0: Math.min(...all.map((a) => a.c0)),
    r1: Math.max(...all.map((a) => a.r1)),
    c1: Math.max(...all.map((a) => a.c1)),
  });
}

function intersect(l: RefValue, r: RefValue): RefValue | ErrorValue {
  const out: Area[] = [];
  for (const a of l.areas) {
    for (const b of r.areas) {
      if (a.sheet !== b.sheet) continue;
      const r0 = Math.max(a.r0, b.r0);
      const r1 = Math.min(a.r1, b.r1);
      const c0 = Math.max(a.c0, b.c0);
      const c1 = Math.min(a.c1, b.c1);
      if (r0 <= r1 && c0 <= c1) out.push({ sheet: a.sheet, r0, c0, r1, c1 });
    }
  }
  return out.length === 0 ? Errors.NULL : new RefValue(out);
}

function describeArity(d: FunctionDescriptor): string {
  if (d.minArgs === d.maxArgs) return `${d.minArgs} argument${d.minArgs === 1 ? '' : 's'}`;
  if (d.maxArgs === Number.POSITIVE_INFINITY)
    return `at least ${d.minArgs} argument${d.minArgs === 1 ? '' : 's'}`;
  return `${d.minArgs} to ${d.maxArgs} arguments`;
}
