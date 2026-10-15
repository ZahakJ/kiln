import { parseNumberText } from '../../../values/coerce.ts';
import { Errors, ErrorValue } from '../../../values/errors.ts';
import { BLANK, MISSING } from '../../../values/types.ts';
import { collectNumbers, force } from '../helpers.ts';
import type { FunctionDescriptor } from '../registry.ts';

const VARIADIC = Number.POSITIVE_INFINITY;

export const aggregateFunctions: FunctionDescriptor[] = [
  {
    name: 'SUM',
    minArgs: 1,
    maxArgs: VARIADIC,
    impl: (args, ctx) => {
      const nums = collectNumbers(args, ctx);
      if (nums instanceof ErrorValue) return nums;
      let s = 0;
      for (const n of nums) s += n;
      return Number.isFinite(s) ? s : Errors.NUM;
    },
    docs: {
      category: 'Math & Trig',
      summary: 'Adds numbers.',
      syntax: 'SUM(number1, [number2], ...)',
      args: [
        { name: 'number1', description: 'A number, cell reference or range to add.' },
        { name: 'number2', description: 'Further numbers or ranges.', optional: true, repeating: true },
      ],
      examples: [{ formula: '=SUM(1,2,3)', result: '6' }],
    },
  },
  {
    name: 'AVERAGE',
    minArgs: 1,
    maxArgs: VARIADIC,
    impl: (args, ctx) => {
      const nums = collectNumbers(args, ctx);
      if (nums instanceof ErrorValue) return nums;
      if (nums.length === 0) return Errors.DIV0;
      let s = 0;
      for (const n of nums) s += n;
      return s / nums.length;
    },
    docs: {
      category: 'Statistical',
      summary: 'Returns the arithmetic mean of its arguments.',
      syntax: 'AVERAGE(number1, [number2], ...)',
      args: [
        { name: 'number1', description: 'A number, cell reference or range.' },
        { name: 'number2', description: 'Further numbers or ranges.', optional: true, repeating: true },
      ],
      examples: [{ formula: '=AVERAGE(2,4,9)', result: '5' }],
    },
  },
  {
    name: 'MIN',
    minArgs: 1,
    maxArgs: VARIADIC,
    impl: (args, ctx) => {
      const nums = collectNumbers(args, ctx);
      if (nums instanceof ErrorValue) return nums;
      if (nums.length === 0) return 0;
      let m = Number.POSITIVE_INFINITY;
      for (const n of nums) if (n < m) m = n;
      return m;
    },
    docs: {
      category: 'Statistical',
      summary: 'Returns the smallest number in a set of values.',
      syntax: 'MIN(number1, [number2], ...)',
      args: [
        { name: 'number1', description: 'A number, cell reference or range.' },
        { name: 'number2', description: 'Further numbers or ranges.', optional: true, repeating: true },
      ],
      examples: [{ formula: '=MIN(4,-2,7)', result: '-2' }],
    },
  },
  {
    name: 'MAX',
    minArgs: 1,
    maxArgs: VARIADIC,
    impl: (args, ctx) => {
      const nums = collectNumbers(args, ctx);
      if (nums instanceof ErrorValue) return nums;
      if (nums.length === 0) return 0;
      let m = Number.NEGATIVE_INFINITY;
      for (const n of nums) if (n > m) m = n;
      return m;
    },
    docs: {
      category: 'Statistical',
      summary: 'Returns the largest number in a set of values.',
      syntax: 'MAX(number1, [number2], ...)',
      args: [
        { name: 'number1', description: 'A number, cell reference or range.' },
        { name: 'number2', description: 'Further numbers or ranges.', optional: true, repeating: true },
      ],
      examples: [{ formula: '=MAX(4,-2,7)', result: '7' }],
    },
  },
  {
    name: 'COUNT',
    minArgs: 1,
    maxArgs: VARIADIC,
    impl: (args, ctx) => {
      let n = 0;
      for (const a of args) {
        ctx.forEachScalar(force(a), (v, fromRef) => {
          if (typeof v === 'number') n++;
          else if (!fromRef) {
            if (typeof v === 'boolean') n++;
            else if (typeof v === 'string' && parseNumberText(v, { date1904: ctx.date1904 }) !== undefined)
              n++;
          }
          return undefined;
        });
      }
      return n;
    },
    docs: {
      category: 'Statistical',
      summary: 'Counts the cells that contain numbers.',
      syntax: 'COUNT(value1, [value2], ...)',
      args: [
        { name: 'value1', description: 'A value, cell reference or range to count numbers in.' },
        { name: 'value2', description: 'Further values or ranges.', optional: true, repeating: true },
      ],
      examples: [{ formula: '=COUNT(1,"a",TRUE)', result: '2' }],
    },
  },
  {
    name: 'COUNTA',
    minArgs: 1,
    maxArgs: VARIADIC,
    impl: (args, ctx) => {
      let n = 0;
      for (const a of args) {
        const v = force(a);
        if (v === MISSING) {
          n++;
          continue;
        }
        ctx.forEachScalar(v, (x, fromRef) => {
          if (!fromRef || x !== BLANK) n++;
          return undefined;
        });
      }
      return n;
    },
    docs: {
      category: 'Statistical',
      summary: 'Counts the cells that are not empty.',
      syntax: 'COUNTA(value1, [value2], ...)',
      args: [
        { name: 'value1', description: 'A value, cell reference or range.' },
        { name: 'value2', description: 'Further values or ranges.', optional: true, repeating: true },
      ],
      examples: [{ formula: '=COUNTA(1,"a",TRUE)', result: '3' }],
    },
  },
];
