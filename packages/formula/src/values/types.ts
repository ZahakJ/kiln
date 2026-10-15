/**
 * The engine's value model.
 *
 * Scalars use JavaScript primitives for numbers, text and booleans (fast, allocation free) plus two
 * singletons: {@link BLANK} for an empty cell and {@link MISSING} for an omitted function argument
 * (`=IF(A1,,2)`). Arrays are rectangular {@link Array2D}s; {@link Lambda} is the value of `LAMBDA(...)`.
 * During evaluation references are kept as {@link RefValue}s so functions such as `ROW`, `OFFSET`,
 * `INDEX` and the range operators can work on the reference rather than on the values.
 */
import type { Node } from '../parser/ast.ts';
import { ErrorValue } from './errors.ts';

/** The value of an empty cell. Prints as an empty string, coerces to 0 / "" / FALSE. */
export class Blank {
  static readonly instance = new Blank();
  private constructor() {}
  toString(): string {
    return '';
  }
}
export const BLANK: Blank = Blank.instance;

/** An omitted argument, e.g. the second argument of `IF(A1,,2)`. Behaves like a blank. */
export class Missing {
  static readonly instance = new Missing();
  private constructor() {}
  toString(): string {
    return '';
  }
}
export const MISSING: Missing = Missing.instance;

export type Scalar = number | string | boolean | ErrorValue | Blank;

/** A rectangular array value, row-major (`data[row][col]`). Always at least 1×1. */
export class Array2D {
  readonly rows: number;
  readonly cols: number;
  constructor(readonly data: Scalar[][]) {
    this.rows = data.length;
    this.cols = data.length > 0 ? (data[0] as Scalar[]).length : 0;
  }

  static filled(rows: number, cols: number, value: Scalar): Array2D {
    const data: Scalar[][] = new Array(rows);
    for (let r = 0; r < rows; r++) data[r] = new Array(cols).fill(value);
    return new Array2D(data);
  }

  static build(rows: number, cols: number, fn: (r: number, c: number) => Scalar): Array2D {
    const data: Scalar[][] = new Array(rows);
    for (let r = 0; r < rows; r++) {
      const row: Scalar[] = new Array(cols);
      for (let c = 0; c < cols; c++) row[c] = fn(r, c);
      data[r] = row;
    }
    return new Array2D(data);
  }

  /** A single row. */
  static row(values: Scalar[]): Array2D {
    return new Array2D([values]);
  }

  /** A single column. */
  static column(values: Scalar[]): Array2D {
    return new Array2D(values.map((v) => [v]));
  }

  get size(): number {
    return this.rows * this.cols;
  }

  get(r: number, c: number): Scalar {
    return (this.data[r] as Scalar[])[c] as Scalar;
  }

  map(fn: (v: Scalar, r: number, c: number) => Scalar): Array2D {
    return Array2D.build(this.rows, this.cols, (r, c) => fn(this.get(r, c), r, c));
  }

  /** Values in row-major order. */
  flat(): Scalar[] {
    const out: Scalar[] = [];
    for (const row of this.data) for (const v of row) out.push(v);
    return out;
  }

  transpose(): Array2D {
    return Array2D.build(this.cols, this.rows, (r, c) => this.get(c, r));
  }
}

/** Lexical bindings created by LET and lambda invocation. Names are matched case-insensitively. */
export class Scope {
  private readonly vars = new Map<string, EvalValue>();
  constructor(readonly parent: Scope | null = null) {}

  set(name: string, value: EvalValue): void {
    this.vars.set(name.toUpperCase(), value);
  }

  lookup(name: string): EvalValue | undefined {
    const key = name.toUpperCase();
    for (let s: Scope | null = this; s; s = s.parent) {
      const v = s.vars.get(key);
      if (v !== undefined) return v;
    }
    return undefined;
  }
}

/** A lambda function value: parameters, body and the scope it closed over. */
export class Lambda {
  constructor(
    readonly params: readonly string[],
    readonly body: Node,
    readonly closure: Scope | null,
  ) {}

  toString(): string {
    return `LAMBDA(${this.params.join(',')})`;
  }
}

export type Value = Scalar | Array2D | Lambda;

/** A rectangular area on one sheet (0-based, inclusive bounds). */
export interface Area {
  sheet: number;
  r0: number;
  c0: number;
  r1: number;
  c1: number;
}

/** A reference (one or more areas; several for unions and 3-D references). */
export class RefValue {
  constructor(readonly areas: readonly Area[]) {}

  static cell(sheet: number, row: number, col: number): RefValue {
    return new RefValue([{ sheet, r0: row, c0: col, r1: row, c1: col }]);
  }

  static area(a: Area): RefValue {
    return new RefValue([a]);
  }

  get single(): Area | undefined {
    return this.areas.length === 1 ? this.areas[0] : undefined;
  }

  get isSingleCell(): boolean {
    const a = this.single;
    return !!a && a.r0 === a.r1 && a.c0 === a.c1;
  }
}

/** Anything an expression can evaluate to. */
export type EvalValue = Value | RefValue | Missing;

export function isBlank(v: unknown): v is Blank {
  return v === BLANK;
}

export function isMissing(v: unknown): v is Missing {
  return v === MISSING;
}

export function isScalar(v: unknown): v is Scalar {
  const t = typeof v;
  return t === 'number' || t === 'string' || t === 'boolean' || v === BLANK || v instanceof ErrorValue;
}

/** Excel's TYPE() numbering: 1 number, 2 text, 4 logical, 16 error, 64 array, 128 lambda. Blank is 1. */
export function typeNumber(v: Value): number {
  if (typeof v === 'number' || v === BLANK) return 1;
  if (typeof v === 'string') return 2;
  if (typeof v === 'boolean') return 4;
  if (v instanceof ErrorValue) return 16;
  if (v instanceof Array2D) return 64;
  return 128;
}
