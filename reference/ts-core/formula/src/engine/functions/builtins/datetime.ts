import { toNumber } from '../../../values/coerce.ts';
import { dateToSerial, jsDateToSerial } from '../../../values/dates.ts';
import { Errors, ErrorValue } from '../../../values/errors.ts';
import { MISSING, type Scalar } from '../../../values/types.ts';
import type { FunctionDescriptor } from '../registry.ts';

export const dateFunctions: FunctionDescriptor[] = [
  {
    name: 'TODAY',
    minArgs: 0,
    maxArgs: 0,
    volatile: true,
    impl: (_args, ctx) => Math.floor(jsDateToSerial(ctx.now(), ctx.date1904)),
    docs: {
      category: 'Date & Time',
      summary: "Returns today's date as a date serial number.",
      syntax: 'TODAY()',
      args: [],
    },
  },
  {
    name: 'NOW',
    minArgs: 0,
    maxArgs: 0,
    volatile: true,
    impl: (_args, ctx) => jsDateToSerial(ctx.now(), ctx.date1904),
    docs: {
      category: 'Date & Time',
      summary: 'Returns the current date and time as a serial number.',
      syntax: 'NOW()',
      args: [],
    },
  },
  {
    name: 'DATE',
    minArgs: 3,
    maxArgs: 3,
    lift: 'all',
    impl: (args, ctx) => {
      const parts: number[] = [];
      for (const a of args) {
        const v = a === MISSING ? 0 : toNumber(a as Scalar);
        if (v instanceof ErrorValue) return v;
        parts.push(Math.trunc(v));
      }
      const [y, m, d] = parts as [number, number, number];
      return dateToSerial(y, m, d, ctx.date1904) ?? Errors.NUM;
    },
    docs: {
      category: 'Date & Time',
      summary:
        'Returns the serial number of a date; months and days may overflow (month 13 is January next year).',
      syntax: 'DATE(year, month, day)',
      args: [
        { name: 'year', description: 'Year (0–1899 are added to 1900).' },
        { name: 'month', description: 'Month, 1–12 (other values roll over).' },
        { name: 'day', description: 'Day of the month (other values roll over).' },
      ],
      examples: [{ formula: '=DATE(2024,1,15)', result: '45306' }],
    },
  },
];
