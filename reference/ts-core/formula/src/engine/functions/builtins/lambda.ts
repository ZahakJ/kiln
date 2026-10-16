import { Errors, ErrorValue } from '../../../values/errors.ts';
import {
  Array2D,
  type EvalValue,
  Lambda,
  MISSING,
  RefValue,
  type Scalar,
  Scope,
} from '../../../values/types.ts';
import { dimsOf, pick } from '../../arrays.ts';
import type { CallContext } from '../../context.ts';
import { ALL_ARGS, force, intArg } from '../helpers.ts';
import { type Arg, type FunctionDescriptor, LazyArg } from '../registry.ts';

function nameOf(a: Arg | undefined): string | undefined {
  if (!(a instanceof LazyArg)) return undefined;
  const n = a.node;
  return n.type === 'name' && !n.prefix ? n.name : undefined;
}

/** A lambda result reduced to a single array element (nested arrays are #CALC!). */
function element(v: EvalValue, ctx: CallContext): Scalar {
  if (v instanceof Array2D)
    return v.size === 1 ? v.get(0, 0) : new ErrorValue('#CALC!', 'Nested arrays are not supported');
  if (v instanceof RefValue && !v.isSingleCell) {
    const val = ctx.toValue(v);
    return val instanceof Array2D
      ? new ErrorValue('#CALC!', 'Nested arrays are not supported')
      : (val as Scalar);
  }
  if (v instanceof Lambda) return Errors.CALC;
  const s = ctx.toScalar(v);
  return s;
}

function lambdaArg(a: Arg | undefined, params: number): Lambda | ErrorValue {
  const v = force(a);
  if (v instanceof ErrorValue) return v;
  if (!(v instanceof Lambda)) return new ErrorValue('#VALUE!', 'Expected a LAMBDA');
  if (v.params.length !== params)
    return new ErrorValue('#VALUE!', `The LAMBDA must take ${params} parameter(s)`);
  return v;
}

function asArray(a: Arg | undefined, ctx: CallContext): Array2D | ErrorValue {
  return ctx.toArray(force(a));
}

function letImpl(args: Arg[], ctx: CallContext): EvalValue {
  if (args.length < 3 || args.length % 2 === 0)
    return new ErrorValue('#VALUE!', 'LET needs name/value pairs and a calculation');
  const first = args[0] as LazyArg;
  const scope = new Scope(first.scope);
  for (let i = 0; i < args.length - 1; i += 2) {
    const name = nameOf(args[i]);
    if (!name) return new ErrorValue('#NAME?', 'LET names must be valid names');
    const valueArg = args[i + 1] as LazyArg;
    const v = ctx.evaluate(valueArg.node, scope);
    scope.set(name, v === MISSING ? MISSING : v);
  }
  return ctx.evaluate((args[args.length - 1] as LazyArg).node, scope);
}

function lambdaImpl(args: Arg[]): EvalValue {
  const params: string[] = [];
  for (let i = 0; i < args.length - 1; i++) {
    const name = nameOf(args[i]);
    if (!name) return new ErrorValue('#VALUE!', 'LAMBDA parameters must be names');
    if (params.some((p) => p.toUpperCase() === name.toUpperCase())) return Errors.VALUE;
    params.push(name);
  }
  const body = args[args.length - 1] as LazyArg;
  return new Lambda(params, body.node, body.scope);
}

function mapImpl(args: Arg[], ctx: CallContext): EvalValue {
  const fn = lambdaArg(args[args.length - 1], args.length - 1);
  if (fn instanceof ErrorValue) return fn;
  const arrays: Array2D[] = [];
  for (const a of args.slice(0, -1)) {
    const arr = asArray(a, ctx);
    if (arr instanceof ErrorValue) return arr;
    arrays.push(arr);
  }
  const { rows, cols } = dimsOf(arrays);
  return Array2D.build(rows, cols, (r, c) =>
    element(
      ctx.invoke(
        fn,
        arrays.map((a) => pick(a, r, c)),
      ),
      ctx,
    ),
  );
}

function reduceOrScan(args: Arg[], ctx: CallContext, scan: boolean): EvalValue {
  const hasInit = args.length === 3;
  const fn = lambdaArg(args[args.length - 1], 2);
  if (fn instanceof ErrorValue) return fn;
  const arr = asArray(args[hasInit ? 1 : 0], ctx);
  if (arr instanceof ErrorValue) return arr;
  let acc: EvalValue = hasInit ? force(args[0]) : MISSING;
  if (acc instanceof RefValue) acc = ctx.toValue(acc);
  const out: Scalar[][] = [];
  for (let r = 0; r < arr.rows; r++) {
    const row: Scalar[] = [];
    for (let c = 0; c < arr.cols; c++) {
      acc = ctx.invoke(fn, [acc, arr.get(r, c)]);
      if (acc instanceof RefValue) acc = ctx.toValue(acc);
      if (scan) row.push(element(acc, ctx));
    }
    out.push(row);
  }
  if (scan) return new Array2D(out);
  return acc;
}

function byRowCol(args: Arg[], ctx: CallContext, byRow: boolean): EvalValue {
  const arr = asArray(args[0], ctx);
  if (arr instanceof ErrorValue) return arr;
  const fn = lambdaArg(args[1], 1);
  if (fn instanceof ErrorValue) return fn;
  if (byRow) {
    return Array2D.column(arr.data.map((row) => element(ctx.invoke(fn, [Array2D.row(row)]), ctx)));
  }
  const t = arr.transpose();
  return Array2D.row(t.data.map((col) => element(ctx.invoke(fn, [Array2D.column(col)]), ctx)));
}

function makeArray(args: Arg[], ctx: CallContext): EvalValue {
  const rows = intArg(args[0], ctx);
  if (rows instanceof ErrorValue) return rows;
  const cols = intArg(args[1], ctx);
  if (cols instanceof ErrorValue) return cols;
  if (rows < 1 || cols < 1) return Errors.VALUE;
  if (rows * cols > 1_048_576 * 16) return Errors.NUM;
  const fn = lambdaArg(args[2], 2);
  if (fn instanceof ErrorValue) return fn;
  return Array2D.build(rows, cols, (r, c) => element(ctx.invoke(fn, [r + 1, c + 1]), ctx));
}

const lambdaDoc = (
  summary: string,
  syntax: string,
  args: { name: string; description: string; optional?: boolean; repeating?: boolean }[],
  examples: { formula: string; result: string }[] = [],
) => ({
  category: 'Logical' as const,
  summary,
  syntax,
  args,
  examples,
});

export const lambdaFunctions: FunctionDescriptor[] = [
  {
    name: 'LET',
    minArgs: 3,
    maxArgs: 253,
    lazy: ALL_ARGS,
    impl: letImpl,
    docs: lambdaDoc(
      'Assigns names to calculation results so a formula can reuse them.',
      'LET(name1, value1, [name2, value2], ..., calculation)',
      [
        { name: 'name1', description: 'A name for the first value.' },
        { name: 'value1', description: 'The value or calculation to name.' },
        { name: 'calculation', description: 'The result, which can use the names.' },
      ],
      [{ formula: '=LET(x,2,y,3,x*y)', result: '6' }],
    ),
  },
  {
    name: 'LAMBDA',
    minArgs: 1,
    maxArgs: 254,
    lazy: ALL_ARGS,
    impl: lambdaImpl,
    docs: lambdaDoc(
      'Creates a reusable custom function; call it directly or give it a name.',
      'LAMBDA([parameter1, parameter2, ...], calculation)',
      [
        { name: 'parameter', description: 'A parameter name.', optional: true, repeating: true },
        { name: 'calculation', description: 'The formula to evaluate using the parameters.' },
      ],
      [{ formula: '=LAMBDA(x,x*2)(21)', result: '42' }],
    ),
  },
  {
    name: 'ISOMITTED',
    minArgs: 1,
    maxArgs: 1,
    impl: (args) => args[0] === MISSING,
    docs: {
      category: 'Information',
      summary: 'Checks whether a LAMBDA argument was omitted.',
      syntax: 'ISOMITTED(argument)',
      args: [{ name: 'argument', description: 'A LAMBDA parameter.' }],
      examples: [{ formula: '=LAMBDA(a,b,ISOMITTED(b))(1,)', result: 'TRUE' }],
    },
  },
  {
    name: 'MAP',
    minArgs: 2,
    maxArgs: 254,
    impl: mapImpl,
    docs: lambdaDoc(
      'Applies a LAMBDA to each value of one or more arrays and returns the results.',
      'MAP(array1, [array2, ...], lambda)',
      [
        { name: 'array1', description: 'An array to map.' },
        { name: 'lambda', description: 'A LAMBDA taking one parameter per array.' },
      ],
      [{ formula: '=SUM(MAP({1,2,3},LAMBDA(x,x*x)))', result: '14' }],
    ),
  },
  {
    name: 'REDUCE',
    minArgs: 2,
    maxArgs: 3,
    impl: (args, ctx) => reduceOrScan(args, ctx, false),
    docs: lambdaDoc(
      'Reduces an array to an accumulated value by applying a LAMBDA to each value.',
      'REDUCE([initial_value], array, lambda)',
      [
        { name: 'initial_value', description: 'Starting value of the accumulator.', optional: true },
        { name: 'array', description: 'The array to reduce.' },
        { name: 'lambda', description: 'LAMBDA(accumulator, value).' },
      ],
      [{ formula: '=REDUCE(0,{1,2,3},LAMBDA(a,b,a+b))', result: '6' }],
    ),
  },
  {
    name: 'SCAN',
    minArgs: 2,
    maxArgs: 3,
    impl: (args, ctx) => reduceOrScan(args, ctx, true),
    docs: lambdaDoc(
      'Scans an array applying a LAMBDA and returns every intermediate value.',
      'SCAN([initial_value], array, lambda)',
      [
        { name: 'initial_value', description: 'Starting value of the accumulator.', optional: true },
        { name: 'array', description: 'The array to scan.' },
        { name: 'lambda', description: 'LAMBDA(accumulator, value).' },
      ],
      [{ formula: '=INDEX(SCAN(0,{1,2,3},LAMBDA(a,b,a+b)),3)', result: '6' }],
    ),
  },
  {
    name: 'BYROW',
    minArgs: 2,
    maxArgs: 2,
    impl: (args, ctx) => byRowCol(args, ctx, true),
    docs: lambdaDoc('Applies a LAMBDA to each row and returns one value per row.', 'BYROW(array, lambda)', [
      { name: 'array', description: 'The array.' },
      { name: 'lambda', description: 'A LAMBDA taking a row.' },
    ]),
  },
  {
    name: 'BYCOL',
    minArgs: 2,
    maxArgs: 2,
    impl: (args, ctx) => byRowCol(args, ctx, false),
    docs: lambdaDoc(
      'Applies a LAMBDA to each column and returns one value per column.',
      'BYCOL(array, lambda)',
      [
        { name: 'array', description: 'The array.' },
        { name: 'lambda', description: 'A LAMBDA taking a column.' },
      ],
    ),
  },
  {
    name: 'MAKEARRAY',
    minArgs: 3,
    maxArgs: 3,
    impl: makeArray,
    docs: lambdaDoc(
      'Builds an array of the given size by calling a LAMBDA for each position.',
      'MAKEARRAY(rows, columns, lambda)',
      [
        { name: 'rows', description: 'Number of rows.' },
        { name: 'columns', description: 'Number of columns.' },
        { name: 'lambda', description: 'LAMBDA(row, column) returning each value.' },
      ],
      [{ formula: '=SUM(MAKEARRAY(2,2,LAMBDA(r,c,r*c)))', result: '9' }],
    ),
  },
];
