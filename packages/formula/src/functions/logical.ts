/**
 * Logical functions beyond the core set (TRUE, FALSE, IFNA, SWITCH, XOR) and the Information family.
 */
import type { CallContext } from '../engine/context.ts';
import { force, isOmitted } from '../engine/functions/helpers.ts';
import type { Arg, FunctionDescriptor } from '../engine/functions/registry.ts';
import { formatCell, quoteSheet, sheetNeedsQuotes } from '../parser/references.ts';
import { toBoolean, toNumber, toText } from '../values/coerce.ts';
import { lookupEquals } from '../values/compare.ts';
import { Errors, ErrorValue } from '../values/errors.ts';
import {
  Array2D,
  BLANK,
  type EvalValue,
  Lambda,
  MISSING,
  RefValue,
  type Scalar,
  typeNumber,
} from '../values/types.ts';
import { AFTER_FIRST, fn, VARIADIC } from './lib.ts';

function branch(a: Arg | undefined): EvalValue {
  if (isOmitted(a)) return 0;
  const v = force(a);
  return v === MISSING || v === BLANK ? 0 : v;
}

function xor(args: Arg[], ctx: CallContext): Scalar {
  let seen = false;
  let trues = 0;
  let err: ErrorValue | undefined;
  for (const a of args) {
    ctx.forEachScalar(force(a), (v, fromRef) => {
      let b: boolean | undefined;
      if (typeof v === 'boolean') b = v;
      else if (typeof v === 'number') b = v !== 0;
      else if (v instanceof ErrorValue) {
        err = v;
        return false;
      } else if (typeof v === 'string' && !fromRef) {
        const r = toBoolean(v);
        if (r instanceof ErrorValue) {
          err = r;
          return false;
        }
        b = r;
      }
      if (b !== undefined) {
        seen = true;
        if (b) trues++;
      }
      return undefined;
    });
    if (err) return err;
  }
  return seen ? trues % 2 === 1 : Errors.VALUE;
}

function switchImpl(args: Arg[], ctx: CallContext): EvalValue {
  const x = args[0] as Scalar;
  if (x instanceof ErrorValue) return x;
  const key = x === BLANK || x === MISSING ? 0 : x;
  const rest = args.length - 1;
  const pairs = Math.floor(rest / 2);
  for (let i = 0; i < pairs; i++) {
    const v = ctx.toScalar(force(args[1 + 2 * i]));
    if (v instanceof ErrorValue) return v;
    if (lookupEquals(key, v === BLANK ? 0 : v)) return branch(args[2 + 2 * i]);
  }
  if (rest % 2 === 1) return branch(args[args.length - 1]);
  return Errors.NA;
}

/** IS* functions: lifted over arrays, applied to the dereferenced value. */
function is(name: string, test: (v: Scalar) => boolean | ErrorValue): FunctionDescriptor {
  return fn(name, 1, 1, (args) => test(args[0] === MISSING ? BLANK : (args[0] as Scalar)), { lift: [0] });
}

function parity(v: Scalar, odd: boolean): boolean | ErrorValue {
  if (typeof v === 'boolean') return Errors.VALUE;
  const n = toNumber(v);
  if (n instanceof ErrorValue) return n;
  const k = Math.abs(Math.trunc(n)) % 2;
  return odd ? k === 1 : k === 0;
}

// ── CELL / INFO / SHEET ──────────────────────────────────────────────────

function refArg(a: Arg | undefined): RefValue | ErrorValue | undefined {
  if (a === undefined || isOmitted(a)) return undefined;
  const v = force(a);
  if (v instanceof ErrorValue) return v;
  if (!(v instanceof RefValue)) return Errors.VALUE;
  return v;
}

function cell(args: Arg[], ctx: CallContext): Scalar {
  const info = toText(ctx.toScalar(force(args[0])));
  if (info instanceof ErrorValue) return info;
  const ref = refArg(args[1]);
  if (ref instanceof ErrorValue) return ref;
  const area = ref ? ref.areas[0] : { sheet: ctx.sheet, r0: ctx.row, c0: ctx.col, r1: ctx.row, c1: ctx.col };
  if (!area) return Errors.VALUE;
  const value = () => ctx.readCell(area.sheet, area.r0, area.c0);
  switch (info.trim().toLowerCase()) {
    case 'address': {
      const a = formatCell(area.r0, area.c0, true, true);
      if (area.sheet === ctx.sheet) return a;
      const name = ctx.sheetName(area.sheet);
      return name ? `${sheetNeedsQuotes(name) ? quoteSheet(name) : name}!${a}` : a;
    }
    case 'col':
      return area.c0 + 1;
    case 'row':
      return area.r0 + 1;
    case 'contents': {
      const v = value();
      return v === BLANK ? 0 : v;
    }
    case 'type': {
      const v = value();
      return v === BLANK ? 'b' : typeof v === 'string' ? 'l' : 'v';
    }
    case 'filename':
      // A workbook that has not been saved has no file name; Officina does not expose paths.
      return '';
    default:
      return Errors.VALUE;
  }
}

function infoFn(args: Arg[], ctx: CallContext): Scalar {
  const t = toText(ctx.toScalar(force(args[0])));
  if (t instanceof ErrorValue) return t;
  switch (t.trim().toLowerCase()) {
    case 'numfile':
      return ctx.sheetIds().length;
    case 'recalc':
      return 'Automatic';
    case 'release':
      return '16.0';
    case 'system':
      return 'pcdos';
    case 'osversion':
      return 'Officina';
    case 'origin':
      return '$A:$A$1';
    case 'directory':
      return Errors.NA;
    default:
      return Errors.VALUE;
  }
}

function sheetFn(args: Arg[], ctx: CallContext): Scalar {
  const ids = ctx.sheetIds();
  const indexOf = (id: number) => {
    const i = ids.indexOf(id);
    return i < 0 ? Errors.NA : i + 1;
  };
  if (args.length === 0 || isOmitted(args[0])) return indexOf(ctx.sheet);
  const v = force(args[0]);
  if (v instanceof RefValue) return indexOf((v.areas[0] as { sheet: number }).sheet);
  if (v instanceof ErrorValue) return v;
  const s = ctx.toScalar(v);
  if (typeof s !== 'string') return Errors.NA;
  const id = ctx.sheetIdOf(s);
  return id === undefined ? Errors.NA : indexOf(id);
}

function sheetsFn(args: Arg[], ctx: CallContext): Scalar {
  if (args.length === 0 || isOmitted(args[0])) return ctx.sheetIds().length;
  const v = force(args[0]);
  if (v instanceof ErrorValue) return v;
  if (!(v instanceof RefValue)) return Errors.VALUE;
  return new Set(v.areas.map((a) => a.sheet)).size;
}

function typeFn(args: Arg[], ctx: CallContext): Scalar {
  const v = force(args[0]);
  if (v instanceof RefValue) {
    const a = v.areas[0];
    if (!a || !v.isSingleCell) return 64;
    return typeNumber(ctx.readCell(a.sheet, a.r0, a.c0));
  }
  if (v === MISSING) return 1;
  if (v instanceof Lambda) return 128;
  return typeNumber(v);
}

function isFormula(args: Arg[], ctx: CallContext): Scalar | Array2D {
  const v = force(args[0]);
  if (v instanceof ErrorValue) return v;
  if (!(v instanceof RefValue)) return Errors.VALUE;
  const a = v.areas[0];
  if (!a) return Errors.VALUE;
  if (v.isSingleCell) return ctx.isFormulaCell(a.sheet, a.r0, a.c0);
  const c = ctx.clampArea(a);
  return Array2D.build(c.r1 - c.r0 + 1, c.c1 - c.c0 + 1, (r, col) =>
    ctx.isFormulaCell(a.sheet, c.r0 + r, c.c0 + col),
  );
}

function formulaText(args: Arg[], ctx: CallContext): Scalar {
  const v = force(args[0]);
  if (v instanceof ErrorValue) return v;
  if (!(v instanceof RefValue)) return Errors.NA;
  const a = v.areas[0];
  if (!a) return Errors.NA;
  return ctx.formulaText(a.sheet, a.r0, a.c0) ?? Errors.NA;
}

export const logicalFunctions: FunctionDescriptor[] = [
  fn('TRUE', 0, 0, () => true),
  fn('FALSE', 0, 0, () => false),
  fn(
    'IFNA',
    2,
    2,
    (args) => {
      const v = args[0] as Scalar;
      if (v instanceof ErrorValue && v.code === '#N/A') return branch(args[1]);
      return v === BLANK ? 0 : v;
    },
    { lazy: [1], lift: [0, 1] },
  ),
  fn('SWITCH', 3, 254, switchImpl, { lazy: AFTER_FIRST, lift: [0] }),
  fn('XOR', 1, VARIADIC, xor),
];

export const informationFunctions: FunctionDescriptor[] = [
  is('ISBLANK', (v) => v === BLANK),
  is('ISERR', (v) => v instanceof ErrorValue && v.code !== '#N/A'),
  is('ISERROR', (v) => v instanceof ErrorValue),
  is('ISNA', (v) => v instanceof ErrorValue && v.code === '#N/A'),
  is('ISLOGICAL', (v) => typeof v === 'boolean'),
  is('ISNUMBER', (v) => typeof v === 'number'),
  is('ISTEXT', (v) => typeof v === 'string'),
  is('ISNONTEXT', (v) => typeof v !== 'string'),
  is('ISEVEN', (v) => parity(v, false)),
  is('ISODD', (v) => parity(v, true)),
  fn('ISREF', 1, 1, (args) => {
    const v = force(args[0]);
    return v instanceof RefValue;
  }),
  fn('ISFORMULA', 1, 1, isFormula),
  fn('FORMULATEXT', 1, 1, formulaText),
  fn(
    'N',
    1,
    1,
    (args) => {
      const v = args[0] as Scalar;
      if (typeof v === 'number' || v instanceof ErrorValue) return v;
      if (typeof v === 'boolean') return v ? 1 : 0;
      return 0;
    },
    { lift: [0] },
  ),
  fn('NA', 0, 0, () => Errors.NA),
  fn(
    'ERROR.TYPE',
    1,
    1,
    (args) => {
      const v = args[0];
      return v instanceof ErrorValue ? v.typeNumber : Errors.NA;
    },
    { lift: [0] },
  ),
  fn('TYPE', 1, 1, typeFn),
  fn('CELL', 1, 2, cell),
  fn('INFO', 1, 1, infoFn),
  fn('SHEET', 0, 1, sheetFn),
  fn('SHEETS', 0, 1, sheetsFn),
];
