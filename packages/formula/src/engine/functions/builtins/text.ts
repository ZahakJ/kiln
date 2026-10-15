import { toBoolean, toNumber, toText } from '../../../values/coerce.ts';
import { Errors, ErrorValue } from '../../../values/errors.ts';
import { BLANK, MISSING, type Scalar } from '../../../values/types.ts';
import { force } from '../helpers.ts';
import type { FunctionDescriptor } from '../registry.ts';

const MAX_TEXT = 32_767;
const VARIADIC = Number.POSITIVE_INFINITY;

function str(v: unknown): string | ErrorValue {
  if (v === undefined || v === MISSING) return '';
  return toText(v as Scalar);
}

function count(v: unknown, dflt: number): number | ErrorValue {
  if (v === undefined) return dflt;
  if (v === MISSING) return 0;
  const n = toNumber(v as Scalar);
  return n instanceof ErrorValue ? n : Math.trunc(n);
}

function capped(s: string): string | ErrorValue {
  return s.length > MAX_TEXT ? new ErrorValue('#VALUE!', 'Text longer than 32,767 characters') : s;
}

export const textFunctions: FunctionDescriptor[] = [
  {
    name: 'LEN',
    minArgs: 1,
    maxArgs: 1,
    lift: 'all',
    impl: (args) => {
      const s = str(args[0]);
      return s instanceof ErrorValue ? s : s.length;
    },
    docs: {
      category: 'Text',
      summary: 'Returns the number of characters in a text string.',
      syntax: 'LEN(text)',
      args: [{ name: 'text', description: 'The text whose length you want.' }],
      examples: [{ formula: '=LEN("Officina")', result: '8' }],
    },
  },
  {
    name: 'LEFT',
    minArgs: 1,
    maxArgs: 2,
    lift: 'all',
    impl: (args) => {
      const s = str(args[0]);
      if (s instanceof ErrorValue) return s;
      const k = count(args[1], 1);
      if (k instanceof ErrorValue) return k;
      if (k < 0) return Errors.VALUE;
      return s.slice(0, k);
    },
    docs: {
      category: 'Text',
      summary: 'Returns the first characters of a text string.',
      syntax: 'LEFT(text, [num_chars])',
      args: [
        { name: 'text', description: 'The text.' },
        { name: 'num_chars', description: 'How many characters (default 1).', optional: true },
      ],
      examples: [{ formula: '=LEFT("Officina",3)', result: '"Off"' }],
    },
  },
  {
    name: 'RIGHT',
    minArgs: 1,
    maxArgs: 2,
    lift: 'all',
    impl: (args) => {
      const s = str(args[0]);
      if (s instanceof ErrorValue) return s;
      const k = count(args[1], 1);
      if (k instanceof ErrorValue) return k;
      if (k < 0) return Errors.VALUE;
      return k === 0 ? '' : s.slice(-k);
    },
    docs: {
      category: 'Text',
      summary: 'Returns the last characters of a text string.',
      syntax: 'RIGHT(text, [num_chars])',
      args: [
        { name: 'text', description: 'The text.' },
        { name: 'num_chars', description: 'How many characters (default 1).', optional: true },
      ],
      examples: [{ formula: '=RIGHT("Officina",4)', result: '"cina"' }],
    },
  },
  {
    name: 'MID',
    minArgs: 3,
    maxArgs: 3,
    lift: 'all',
    impl: (args) => {
      const s = str(args[0]);
      if (s instanceof ErrorValue) return s;
      const start = count(args[1], 1);
      if (start instanceof ErrorValue) return start;
      const k = count(args[2], 0);
      if (k instanceof ErrorValue) return k;
      if (start < 1 || k < 0) return Errors.VALUE;
      return s.slice(start - 1, start - 1 + k);
    },
    docs: {
      category: 'Text',
      summary: 'Returns characters from the middle of a text string.',
      syntax: 'MID(text, start_num, num_chars)',
      args: [
        { name: 'text', description: 'The text.' },
        { name: 'start_num', description: 'Position of the first character (1 is the first).' },
        { name: 'num_chars', description: 'How many characters.' },
      ],
      examples: [{ formula: '=MID("Officina",2,3)', result: '"ffi"' }],
    },
  },
  {
    name: 'CONCAT',
    minArgs: 1,
    maxArgs: VARIADIC,
    impl: (args, ctx) => {
      let out = '';
      let err: ErrorValue | undefined;
      for (const a of args) {
        ctx.forEachScalar(force(a), (v) => {
          const t = toText(v);
          if (t instanceof ErrorValue) {
            err = t;
            return false;
          }
          out += t;
          return undefined;
        });
        if (err) return err;
      }
      return capped(out);
    },
    docs: {
      category: 'Text',
      summary: 'Joins text from several values and ranges.',
      syntax: 'CONCAT(text1, [text2], ...)',
      args: [
        { name: 'text1', description: 'Text, a number or a range to join.' },
        { name: 'text2', description: 'Further items.', optional: true, repeating: true },
      ],
      examples: [{ formula: '=CONCAT("a",1,TRUE)', result: '"a1TRUE"' }],
    },
  },
  {
    name: 'TEXTJOIN',
    minArgs: 3,
    maxArgs: VARIADIC,
    impl: (args, ctx) => {
      const delims: string[] = [];
      let err: ErrorValue | undefined;
      ctx.forEachScalar(force(args[0]), (v) => {
        const t = toText(v);
        if (t instanceof ErrorValue) {
          err = t;
          return false;
        }
        delims.push(t);
        return undefined;
      });
      if (err) return err;
      if (delims.length === 0) delims.push('');
      const ignore = toBoolean(ctx.toScalar(force(args[1])));
      if (ignore instanceof ErrorValue) return ignore;
      const parts: string[] = [];
      for (const a of args.slice(2)) {
        ctx.forEachScalar(force(a), (v) => {
          if (ignore && (v === BLANK || v === '')) return undefined;
          const t = toText(v);
          if (t instanceof ErrorValue) {
            err = t;
            return false;
          }
          parts.push(t);
          return undefined;
        });
        if (err) return err;
      }
      let out = '';
      for (let i = 0; i < parts.length; i++) {
        if (i > 0) out += delims[(i - 1) % delims.length];
        out += parts[i];
      }
      return capped(out);
    },
    docs: {
      category: 'Text',
      summary: 'Joins text with a delimiter, optionally skipping empty values.',
      syntax: 'TEXTJOIN(delimiter, ignore_empty, text1, [text2], ...)',
      args: [
        { name: 'delimiter', description: 'Text placed between items (a range cycles through several).' },
        { name: 'ignore_empty', description: 'TRUE to skip empty cells.' },
        { name: 'text1', description: 'Text, a number or a range to join.' },
        { name: 'text2', description: 'Further items.', optional: true, repeating: true },
      ],
      examples: [{ formula: '=TEXTJOIN(", ",TRUE,"a","","b")', result: '"a, b"' }],
    },
  },
];
