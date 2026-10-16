import { toNumber } from '../../../values/coerce.ts';
import { Errors, ErrorValue } from '../../../values/errors.ts';
import { excelRound } from '../../../values/number.ts';
import type { Scalar } from '../../../values/types.ts';
import { MISSING } from '../../../values/types.ts';
import type { FunctionDescriptor } from '../registry.ts';

function n(v: unknown): number | ErrorValue {
  if (v === undefined || v === MISSING) return 0;
  return toNumber(v as Scalar);
}

export const mathFunctions: FunctionDescriptor[] = [
  {
    name: 'ROUND',
    minArgs: 2,
    maxArgs: 2,
    lift: 'all',
    impl: (args) => {
      const x = n(args[0]);
      if (x instanceof ErrorValue) return x;
      const d = n(args[1]);
      if (d instanceof ErrorValue) return d;
      return excelRound(x, Math.trunc(d));
    },
    docs: {
      category: 'Math & Trig',
      summary: 'Rounds a number to a given number of digits (halves away from zero).',
      syntax: 'ROUND(number, num_digits)',
      args: [
        { name: 'number', description: 'The number to round.' },
        {
          name: 'num_digits',
          description: 'Digits after the decimal point; negative rounds to tens, hundreds…',
        },
      ],
      examples: [
        { formula: '=ROUND(2.345,2)', result: '2.35' },
        { formula: '=ROUND(1234,-2)', result: '1200' },
      ],
    },
  },
  {
    name: 'ABS',
    minArgs: 1,
    maxArgs: 1,
    lift: 'all',
    impl: (args) => {
      const x = n(args[0]);
      return x instanceof ErrorValue ? x : Math.abs(x);
    },
    docs: {
      category: 'Math & Trig',
      summary: 'Returns the absolute value of a number.',
      syntax: 'ABS(number)',
      args: [{ name: 'number', description: 'A real number.' }],
      examples: [{ formula: '=ABS(-4)', result: '4' }],
    },
  },
  {
    name: 'INT',
    minArgs: 1,
    maxArgs: 1,
    lift: 'all',
    impl: (args) => {
      const x = n(args[0]);
      return x instanceof ErrorValue ? x : Math.floor(x);
    },
    docs: {
      category: 'Math & Trig',
      summary: 'Rounds a number down to the nearest integer.',
      syntax: 'INT(number)',
      args: [{ name: 'number', description: 'The number to round down.' }],
      examples: [{ formula: '=INT(-2.5)', result: '-3' }],
    },
  },
  {
    name: 'MOD',
    minArgs: 2,
    maxArgs: 2,
    lift: 'all',
    impl: (args) => {
      const x = n(args[0]);
      if (x instanceof ErrorValue) return x;
      const d = n(args[1]);
      if (d instanceof ErrorValue) return d;
      if (d === 0) return Errors.DIV0;
      const r = x - d * Math.floor(x / d);
      // Snap results that are within rounding noise of the divisor or zero.
      if (Math.abs(r - d) < Math.abs(d) * 1e-15) return 0;
      return r;
    },
    docs: {
      category: 'Math & Trig',
      summary: 'Returns the remainder after division; the result has the sign of the divisor.',
      syntax: 'MOD(number, divisor)',
      args: [
        { name: 'number', description: 'The number to divide.' },
        { name: 'divisor', description: 'The number to divide by.' },
      ],
      examples: [{ formula: '=MOD(-3,2)', result: '1' }],
    },
  },
];
