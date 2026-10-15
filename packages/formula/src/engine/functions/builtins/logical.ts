import { toBoolean } from '../../../values/coerce.ts';
import { Errors, ErrorValue } from '../../../values/errors.ts';
import { Array2D, BLANK, type EvalValue, MISSING, type Scalar } from '../../../values/types.ts';
import { dimsOf, pick } from '../../arrays.ts';
import type { CallContext } from '../../context.ts';
import { ALL_ARGS, force, isOmitted } from '../helpers.ts';
import type { Arg, FunctionDescriptor } from '../registry.ts';

const VARIADIC = Number.POSITIVE_INFINITY;

/** Value of an IF/IFERROR branch: an empty argument is 0. */
function branch(a: Arg | undefined): EvalValue {
  if (isOmitted(a)) return 0;
  const v = force(a);
  return v === MISSING || v === BLANK ? 0 : v;
}

function logicalFold(args: Arg[], ctx: CallContext, isAnd: boolean): EvalValue {
  let seen = false;
  let acc = isAnd;
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
        acc = isAnd ? acc && b : acc || b;
      }
      return undefined;
    });
    if (err) return err;
  }
  return seen ? acc : Errors.VALUE;
}

function ifsImpl(args: Arg[], ctx: CallContext): EvalValue {
  if (args.length % 2 !== 0) return new ErrorValue('#VALUE!', 'IFS needs condition/value pairs');
  // Scalar conditions: evaluate lazily in order, like Excel.
  const conds: (Scalar | Array2D)[] = [];
  for (let i = 0; i < args.length; i += 2) {
    const c = ctx.toValue(force(args[i]));
    if (c instanceof Array2D && c.size > 1) return ifsLifted(args, ctx);
    const s = c instanceof Array2D ? c.get(0, 0) : (c as Scalar);
    conds.push(s);
    const b = toBoolean(s);
    if (b instanceof ErrorValue) return b;
    if (b) return branch(args[i + 1]);
  }
  return Errors.NA;
}

function ifsLifted(args: Arg[], ctx: CallContext): EvalValue {
  const parts = args.map((a) => {
    const v = isOmitted(a) ? 0 : ctx.toValue(force(a));
    return v instanceof Array2D ? v : (v as Scalar);
  });
  const { rows, cols } = dimsOf(parts as (Scalar | Array2D)[]);
  return Array2D.build(rows, cols, (r, c) => {
    for (let i = 0; i < parts.length; i += 2) {
      const b = toBoolean(pick(parts[i] as Scalar | Array2D, r, c));
      if (b instanceof ErrorValue) return b;
      if (b) {
        const v = pick(parts[i + 1] as Scalar | Array2D, r, c);
        return v === BLANK ? 0 : v;
      }
    }
    return Errors.NA;
  });
}

export const logicalFunctions: FunctionDescriptor[] = [
  {
    name: 'IF',
    minArgs: 2,
    maxArgs: 3,
    lazy: [1, 2],
    lift: [0, 1, 2],
    impl: (args) => {
      const test = toBoolean(args[0] as Scalar);
      if (test instanceof ErrorValue) return test;
      if (test) return branch(args[1]);
      if (args.length < 3) return false;
      return branch(args[2]);
    },
    docs: {
      category: 'Logical',
      summary: 'Returns one value if a condition is TRUE and another if it is FALSE.',
      syntax: 'IF(logical_test, value_if_true, [value_if_false])',
      args: [
        { name: 'logical_test', description: 'The condition to check.' },
        { name: 'value_if_true', description: 'Result when the condition is TRUE.' },
        {
          name: 'value_if_false',
          description: 'Result when the condition is FALSE (FALSE if omitted).',
          optional: true,
        },
      ],
      examples: [{ formula: '=IF(2>1,"yes","no")', result: '"yes"' }],
    },
  },
  {
    name: 'IFS',
    minArgs: 2,
    maxArgs: 254,
    lazy: ALL_ARGS,
    impl: ifsImpl,
    docs: {
      category: 'Logical',
      summary: 'Returns the value for the first condition that is TRUE.',
      syntax: 'IFS(logical_test1, value_if_true1, [logical_test2, value_if_true2], ...)',
      args: [
        { name: 'logical_test1', description: 'The first condition.' },
        { name: 'value_if_true1', description: 'Result when logical_test1 is TRUE.' },
        {
          name: 'logical_test2, value_if_true2',
          description: 'Further pairs.',
          optional: true,
          repeating: true,
        },
      ],
      examples: [{ formula: '=IFS(1>2,"a",TRUE,"b")', result: '"b"' }],
    },
  },
  {
    name: 'AND',
    minArgs: 1,
    maxArgs: VARIADIC,
    impl: (args, ctx) => logicalFold(args, ctx, true),
    docs: {
      category: 'Logical',
      summary: 'Returns TRUE if all of its arguments are TRUE.',
      syntax: 'AND(logical1, [logical2], ...)',
      args: [
        { name: 'logical1', description: 'A condition, logical value or range.' },
        { name: 'logical2', description: 'Further conditions.', optional: true, repeating: true },
      ],
      examples: [{ formula: '=AND(TRUE,1>2)', result: 'FALSE' }],
    },
  },
  {
    name: 'OR',
    minArgs: 1,
    maxArgs: VARIADIC,
    impl: (args, ctx) => logicalFold(args, ctx, false),
    docs: {
      category: 'Logical',
      summary: 'Returns TRUE if any argument is TRUE.',
      syntax: 'OR(logical1, [logical2], ...)',
      args: [
        { name: 'logical1', description: 'A condition, logical value or range.' },
        { name: 'logical2', description: 'Further conditions.', optional: true, repeating: true },
      ],
      examples: [{ formula: '=OR(FALSE,1<2)', result: 'TRUE' }],
    },
  },
  {
    name: 'NOT',
    minArgs: 1,
    maxArgs: 1,
    lift: [0],
    impl: (args) => {
      const b = toBoolean(args[0] as Scalar);
      return b instanceof ErrorValue ? b : !b;
    },
    docs: {
      category: 'Logical',
      summary: 'Reverses a logical value.',
      syntax: 'NOT(logical)',
      args: [{ name: 'logical', description: 'A value or condition that is TRUE or FALSE.' }],
      examples: [{ formula: '=NOT(TRUE)', result: 'FALSE' }],
    },
  },
  {
    name: 'IFERROR',
    minArgs: 2,
    maxArgs: 2,
    lazy: [1],
    lift: [0, 1],
    impl: (args) => {
      const v = args[0] as Scalar;
      if (v instanceof ErrorValue) return branch(args[1]);
      return v === BLANK ? 0 : v;
    },
    docs: {
      category: 'Logical',
      summary: 'Returns a value you choose if a formula evaluates to an error; otherwise the formula result.',
      syntax: 'IFERROR(value, value_if_error)',
      args: [
        { name: 'value', description: 'The value or formula to check.' },
        { name: 'value_if_error', description: 'Result when value is an error.' },
      ],
      examples: [{ formula: '=IFERROR(1/0,"none")', result: '"none"' }],
    },
  },
];
