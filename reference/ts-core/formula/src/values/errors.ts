/**
 * Excel error values. Errors are first-class values: they flow through formulas, propagate through
 * operators and most functions, and are compared by their code.
 */

export const ERROR_CODES = [
  '#NULL!',
  '#DIV/0!',
  '#VALUE!',
  '#REF!',
  '#NAME?',
  '#NUM!',
  '#N/A',
  '#GETTING_DATA',
  '#SPILL!',
  '#CONNECT!',
  '#BLOCKED!',
  '#UNKNOWN!',
  '#FIELD!',
  '#CALC!',
  '#BUSY!',
] as const;

export type ErrorCode = (typeof ERROR_CODES)[number];

/** An Excel error such as `#DIV/0!`. `message` is an optional human explanation (for tooltips). */
export class ErrorValue {
  constructor(
    readonly code: ErrorCode,
    readonly message?: string,
  ) {}

  /** The number `ERROR.TYPE` returns for this error. */
  get typeNumber(): number {
    return ERROR_CODES.indexOf(this.code) + 1;
  }

  toString(): string {
    return this.code;
  }
}

/** Shared singletons for the common errors (use `new ErrorValue(code, message)` to attach a reason). */
export const Errors = {
  NULL: new ErrorValue('#NULL!'),
  DIV0: new ErrorValue('#DIV/0!'),
  VALUE: new ErrorValue('#VALUE!'),
  REF: new ErrorValue('#REF!'),
  NAME: new ErrorValue('#NAME?'),
  NUM: new ErrorValue('#NUM!'),
  NA: new ErrorValue('#N/A'),
  GETTING_DATA: new ErrorValue('#GETTING_DATA'),
  SPILL: new ErrorValue('#SPILL!'),
  CONNECT: new ErrorValue('#CONNECT!'),
  BLOCKED: new ErrorValue('#BLOCKED!'),
  UNKNOWN: new ErrorValue('#UNKNOWN!'),
  FIELD: new ErrorValue('#FIELD!'),
  CALC: new ErrorValue('#CALC!'),
  BUSY: new ErrorValue('#BUSY!'),
} as const;

const BY_CODE = new Map<string, ErrorValue>(Object.values(Errors).map((e) => [e.code, e]));

/** Looks up an error literal case-insensitively (`#n/a` → `#N/A`). */
export function errorFromCode(text: string): ErrorValue | undefined {
  return BY_CODE.get(text.toUpperCase());
}

export function isError(v: unknown): v is ErrorValue {
  return v instanceof ErrorValue;
}
