/**
 * Text functions beyond the core set. TEXT, DOLLAR and FIXED format through `numfmt` (ECMA-376
 * number formats, en-US conventions), so `TEXT(1234.5,"#,##0.00")` is `1,234.50` as in Excel.
 */
import * as numfmt from 'numfmt';
import type { CallContext } from '../engine/context.ts';
import { wildcardRegex } from '../engine/functions/builtins/lookup.ts';
import { force, isOmitted } from '../engine/functions/helpers.ts';
import type { Arg, FunctionDescriptor } from '../engine/functions/registry.ts';
import { parseNumberText, toNumber, toText } from '../values/coerce.ts';
import { Errors, ErrorValue } from '../values/errors.ts';
import { excelRound, numberToText } from '../values/number.ts';
import { Array2D, BLANK, MISSING, type Scalar } from '../values/types.ts';
import { arrayOf, bool, fn, int, isErr, num, str, VARIADIC } from './lib.ts';

const MAX_TEXT = 32_767;

function capped(s: string): string | ErrorValue {
  return s.length > MAX_TEXT ? new ErrorValue('#VALUE!', 'Text longer than 32,767 characters') : s;
}

/** Text functions of fixed arity with every argument lifted. */
function textFn(
  name: string,
  min: number,
  max: number,
  impl: FunctionDescriptor['impl'],
): FunctionDescriptor {
  return fn(name, min, max, impl, { lift: 'all' });
}

// ── formatting ───────────────────────────────────────────────────────────

const FORMAT_OPTS = {
  locale: 'en-US',
  throws: true,
  dateErrorThrows: true,
  dateSpanLarge: false,
  leap1900: true,
} as const;

/** Formats a number with an Excel format code, or #VALUE! for invalid codes and out-of-range dates. */
export function formatNumber(value: number | string, pattern: string): string | ErrorValue {
  try {
    return numfmt.format(pattern, value, FORMAT_OPTS);
  } catch {
    return Errors.VALUE;
  }
}

function textImpl(args: Arg[]): Scalar {
  const v = args[0] === MISSING ? BLANK : (args[0] as Scalar);
  if (v instanceof ErrorValue) return v;
  const f = str(args[1]);
  if (isErr(f)) return f;
  if (typeof v === 'boolean') return v ? 'TRUE' : 'FALSE';
  let x: number | string;
  if (typeof v === 'number') x = v;
  else if (v === BLANK) x = 0;
  else {
    const n = parseNumberText(v as string);
    x = n === undefined ? (v as string) : n;
  }
  if (f === '') return '';
  if (typeof x === 'number' && /^general$/i.test(f.trim())) return numberToText(x);
  const out = formatNumber(x, f);
  return isErr(out) ? out : capped(out);
}

function decimalsPattern(decimals: number): string {
  return decimals > 0 ? `.${'0'.repeat(decimals)}` : '';
}

function dollar(args: Arg[]): Scalar {
  const x = num(args[0]);
  if (isErr(x)) return x;
  const d = int(args[1], 2);
  if (isErr(d)) return d;
  if (d > 127) return Errors.VALUE;
  const r = excelRound(x, d);
  const body = `$#,##0${decimalsPattern(d)}`;
  return formatNumber(r, `${body};(${body})`);
}

function fixed(args: Arg[]): Scalar {
  const x = num(args[0]);
  if (isErr(x)) return x;
  const d = int(args[1], 2);
  if (isErr(d)) return d;
  const noCommas = bool(args[2], false);
  if (isErr(noCommas)) return noCommas;
  if (d > 127) return Errors.VALUE;
  const r = excelRound(x, d);
  return formatNumber(r, `${noCommas ? '0' : '#,##0'}${decimalsPattern(d)}`);
}

// ── Windows-1252 for CHAR/CODE ───────────────────────────────────────────

const CP1252_HIGH: Record<number, number> = {
  128: 0x20ac,
  130: 0x201a,
  131: 0x0192,
  132: 0x201e,
  133: 0x2026,
  134: 0x2020,
  135: 0x2021,
  136: 0x02c6,
  137: 0x2030,
  138: 0x0160,
  139: 0x2039,
  140: 0x0152,
  142: 0x017d,
  145: 0x2018,
  146: 0x2019,
  147: 0x201c,
  148: 0x201d,
  149: 0x2022,
  150: 0x2013,
  151: 0x2014,
  152: 0x02dc,
  153: 0x2122,
  154: 0x0161,
  155: 0x203a,
  156: 0x0153,
  158: 0x017e,
  159: 0x0178,
};
const CP1252_REVERSE = new Map<number, number>(Object.entries(CP1252_HIGH).map(([k, v]) => [v, Number(k)]));

// ── search helpers ───────────────────────────────────────────────────────

function findImpl(args: Arg[], insensitive: boolean): Scalar {
  const needle = str(args[0]);
  if (isErr(needle)) return needle;
  const hay = str(args[1]);
  if (isErr(hay)) return hay;
  const start = int(args[2], 1);
  if (isErr(start)) return start;
  if (start < 1 || start > hay.length + 1) return Errors.VALUE;
  if (needle === '') return start;
  if (!insensitive) {
    const i = hay.indexOf(needle, start - 1);
    return i < 0 ? Errors.VALUE : i + 1;
  }
  if (/[*?~]/.test(needle)) {
    const re = wildcardRegex(needle);
    const src = re.source.slice(1, -1);
    const m = new RegExp(src, 'i');
    const sub = hay.slice(start - 1);
    const hit = m.exec(sub);
    return hit ? hit.index + start : Errors.VALUE;
  }
  const i = hay.toLowerCase().indexOf(needle.toLowerCase(), start - 1);
  return i < 0 ? Errors.VALUE : i + 1;
}

// ── TEXTBEFORE / TEXTAFTER / TEXTSPLIT ───────────────────────────────────

function delimiters(a: Arg | undefined, ctx: CallContext): string[] | ErrorValue {
  const v = force(a);
  const out: string[] = [];
  let err: ErrorValue | undefined;
  ctx.forEachScalar(v === MISSING ? '' : v, (x) => {
    const t = toText(x);
    if (t instanceof ErrorValue) {
      err = t;
      return false;
    }
    out.push(t);
    return undefined;
  });
  return err ?? out;
}

/** All non-overlapping delimiter occurrences, scanning from the left. */
function occurrences(text: string, delims: string[], insensitive: boolean): [number, number][] {
  const hay = insensitive ? text.toLowerCase() : text;
  const ds = delims.filter((d) => d !== '').map((d) => (insensitive ? d.toLowerCase() : d));
  const out: [number, number][] = [];
  let i = 0;
  while (i <= hay.length) {
    let best = -1;
    let bestLen = 0;
    for (const d of ds) {
      const k = hay.indexOf(d, i);
      if (k >= 0 && (best < 0 || k < best || (k === best && d.length > bestLen))) {
        best = k;
        bestLen = d.length;
      }
    }
    if (best < 0) break;
    out.push([best, best + bestLen]);
    i = best + bestLen;
  }
  return out;
}

function textBeforeAfter(args: Arg[], ctx: CallContext, after: boolean): Scalar {
  const text = toText(ctx.toScalar(force(args[0])));
  if (text instanceof ErrorValue) return text;
  const delims = delimiters(args[1], ctx);
  if (delims instanceof ErrorValue) return delims;
  const inst = isOmitted(args[2]) ? 1 : toNumber(ctx.toScalar(force(args[2])));
  if (inst instanceof ErrorValue) return inst;
  const mode = isOmitted(args[3]) ? 0 : toNumber(ctx.toScalar(force(args[3])));
  if (mode instanceof ErrorValue) return mode;
  const end = isOmitted(args[4]) ? 0 : toNumber(ctx.toScalar(force(args[4])));
  if (end instanceof ErrorValue) return end;
  const n = Math.trunc(inst);
  if (n === 0 || Math.abs(n) > text.length + 1) return Errors.VALUE;
  const notFound = (): Scalar =>
    args.length > 5 && !isOmitted(args[5]) ? ctx.toScalar(force(args[5])) : Errors.NA;
  if (delims.every((d) => d === '')) {
    // An empty delimiter matches at the very start (or end for negative instances).
    if (n > 0) return after ? text : '';
    return after ? '' : text;
  }
  const occ = occurrences(text, delims, mode === 1);
  if (end) {
    if (n > 0) occ.push([text.length, text.length]);
    else occ.unshift([0, 0]);
  }
  const k = n > 0 ? n - 1 : occ.length + n;
  const hit = occ[k];
  if (!hit || k < 0) return notFound();
  return after ? text.slice(hit[1]) : text.slice(0, hit[0]);
}

function textSplit(args: Arg[], ctx: CallContext): Scalar | Array2D {
  const text = toText(ctx.toScalar(force(args[0])));
  if (text instanceof ErrorValue) return text;
  const colD = isOmitted(args[1]) ? [] : delimiters(args[1], ctx);
  if (colD instanceof ErrorValue) return colD;
  const rowD = isOmitted(args[2]) ? [] : delimiters(args[2], ctx);
  if (rowD instanceof ErrorValue) return rowD;
  if (colD.every((d) => d === '') && rowD.every((d) => d === '')) return Errors.VALUE;
  const ignoreEmpty = isOmitted(args[3]) ? false : toNumber(ctx.toScalar(force(args[3])));
  if (ignoreEmpty instanceof ErrorValue) return ignoreEmpty;
  const mode = isOmitted(args[4]) ? 0 : toNumber(ctx.toScalar(force(args[4])));
  if (mode instanceof ErrorValue) return mode;
  const pad: Scalar = isOmitted(args[5]) ? Errors.NA : ctx.toScalar(force(args[5]));
  const split = (s: string, ds: string[]): string[] => {
    if (ds.every((d) => d === '')) return [s];
    const occ = occurrences(s, ds, mode === 1);
    const parts: string[] = [];
    let i = 0;
    for (const [a, b] of occ) {
      parts.push(s.slice(i, a));
      i = b;
    }
    parts.push(s.slice(i));
    return ignoreEmpty ? parts.filter((p) => p !== '') : parts;
  };
  const rows = split(text, rowD).map((r) => split(r, colD));
  const kept = ignoreEmpty ? rows.filter((r) => r.length > 0) : rows;
  if (kept.length === 0) return Errors.CALC;
  const width = Math.max(...kept.map((r) => r.length));
  return Array2D.build(kept.length, width, (r, c) => {
    const row = kept[r] as string[];
    return c < row.length ? (row[c] as string) : pad;
  });
}

// ── VALUETOTEXT / ARRAYTOTEXT ────────────────────────────────────────────

function valueText(v: Scalar, strict: boolean): string {
  if (typeof v === 'string') return strict ? `"${v.replace(/"/g, '""')}"` : v;
  if (v instanceof ErrorValue) return v.code;
  return toText(v) as string;
}

// ── NUMBERVALUE ──────────────────────────────────────────────────────────

function numberValue(args: Arg[]): Scalar {
  const t = str(args[0]);
  if (isErr(t)) return t;
  const dec = args[1] === undefined || args[1] === MISSING ? '.' : str(args[1]);
  if (isErr(dec)) return dec;
  const grp = args[2] === undefined || args[2] === MISSING ? ',' : str(args[2]);
  if (isErr(grp)) return grp;
  const d = dec[0] ?? '.';
  const g = grp[0] ?? ',';
  if (d === g) return Errors.VALUE;
  let s = t.replace(/\s+/g, '');
  if (s === '') return 0;
  let pct = 0;
  while (s.endsWith('%')) {
    pct++;
    s = s.slice(0, -1);
  }
  const di = s.indexOf(d);
  if (di >= 0 && s.indexOf(d, di + 1) >= 0) return Errors.VALUE;
  let intPart = di >= 0 ? s.slice(0, di) : s;
  const frac = di >= 0 ? s.slice(di + 1) : '';
  if (frac.includes(g)) return Errors.VALUE;
  intPart = intPart.split(g).join('');
  const normal = `${intPart}${di >= 0 ? `.${frac}` : ''}`;
  if (!/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(normal)) return Errors.VALUE;
  return Number(normal) / 100 ** pct;
}

// ── REGEX ────────────────────────────────────────────────────────────────

function regex(pattern: string, insensitive: boolean, global: boolean): RegExp | ErrorValue {
  try {
    return new RegExp(pattern, `${insensitive ? 'i' : ''}${global ? 'g' : ''}u`);
  } catch {
    return new ErrorValue('#VALUE!', 'Invalid regular expression');
  }
}

function regexArgs(args: Arg[], caseIndex: number): { text: string; re: string; ci: boolean } | ErrorValue {
  const text = str(args[0]);
  if (isErr(text)) return text;
  const re = str(args[1]);
  if (isErr(re)) return re;
  const c = int(args[caseIndex], 0);
  if (isErr(c)) return c;
  if (c !== 0 && c !== 1) return Errors.VALUE;
  return { text, re, ci: c === 1 };
}

export const textFunctions: FunctionDescriptor[] = [
  textFn('CHAR', 1, 1, (args) => {
    const n = int(args[0]);
    if (isErr(n)) return n;
    if (n < 1 || n > 255) return Errors.VALUE;
    return String.fromCharCode(CP1252_HIGH[n] ?? n);
  }),
  textFn('CODE', 1, 1, (args) => {
    const s = str(args[0]);
    if (isErr(s)) return s;
    if (s === '') return Errors.VALUE;
    const c = s.charCodeAt(0);
    if (c < 128 || (c >= 160 && c <= 255)) return c;
    return CP1252_REVERSE.get(c) ?? 63;
  }),
  textFn('UNICHAR', 1, 1, (args) => {
    const n = int(args[0]);
    if (isErr(n)) return n;
    if (n < 1 || n > 0x10ffff) return Errors.VALUE;
    if (n >= 0xd800 && n <= 0xdfff) return Errors.NA;
    return String.fromCodePoint(n);
  }),
  textFn('UNICODE', 1, 1, (args) => {
    const s = str(args[0]);
    if (isErr(s)) return s;
    if (s === '') return Errors.VALUE;
    return s.codePointAt(0) as number;
  }),
  textFn('CLEAN', 1, 1, (args) => {
    const s = str(args[0]);
    // biome-ignore lint/suspicious/noControlCharactersInRegex: CLEAN removes exactly these characters.
    return isErr(s) ? s : s.replace(/[\x00-\x1f]/g, '');
  }),
  textFn('TRIM', 1, 1, (args) => {
    const s = str(args[0]);
    return isErr(s) ? s : s.replace(/ +/g, ' ').replace(/^ | $/g, '');
  }),
  textFn('LOWER', 1, 1, (args) => {
    const s = str(args[0]);
    return isErr(s) ? s : s.toLowerCase();
  }),
  textFn('UPPER', 1, 1, (args) => {
    const s = str(args[0]);
    return isErr(s) ? s : s.toUpperCase();
  }),
  textFn('PROPER', 1, 1, (args) => {
    const s = str(args[0]);
    if (isErr(s)) return s;
    let out = '';
    let prevLetter = false;
    for (const ch of s) {
      const isLetter = ch.toLowerCase() !== ch.toUpperCase();
      out += isLetter ? (prevLetter ? ch.toLowerCase() : ch.toUpperCase()) : ch;
      prevLetter = isLetter;
    }
    return out;
  }),
  textFn('EXACT', 2, 2, (args) => {
    const a = str(args[0]);
    if (isErr(a)) return a;
    const b = str(args[1]);
    return isErr(b) ? b : a === b;
  }),
  textFn('FIND', 2, 3, (args) => findImpl(args, false)),
  textFn('FINDB', 2, 3, (args) => findImpl(args, false)),
  textFn('SEARCH', 2, 3, (args) => findImpl(args, true)),
  textFn('SEARCHB', 2, 3, (args) => findImpl(args, true)),
  ...['REPLACE', 'REPLACEB'].map((name) =>
    textFn(name, 4, 4, (args) => {
      const s = str(args[0]);
      if (isErr(s)) return s;
      const start = int(args[1]);
      if (isErr(start)) return start;
      const n = int(args[2]);
      if (isErr(n)) return n;
      const repl = str(args[3]);
      if (isErr(repl)) return repl;
      if (start < 1 || n < 0) return Errors.VALUE;
      return capped(s.slice(0, start - 1) + repl + s.slice(start - 1 + n));
    }),
  ),
  textFn('SUBSTITUTE', 3, 4, (args) => {
    const s = str(args[0]);
    if (isErr(s)) return s;
    const old = str(args[1]);
    if (isErr(old)) return old;
    const repl = str(args[2]);
    if (isErr(repl)) return repl;
    if (old === '') return s;
    if (args[3] === undefined) return capped(s.split(old).join(repl));
    const k = int(args[3]);
    if (isErr(k)) return k;
    if (k < 1) return Errors.VALUE;
    let idx = -1;
    let from = 0;
    for (let i = 0; i < k; i++) {
      idx = s.indexOf(old, from);
      if (idx < 0) return s;
      from = idx + old.length;
    }
    return capped(s.slice(0, idx) + repl + s.slice(idx + old.length));
  }),
  textFn('REPT', 2, 2, (args) => {
    const s = str(args[0]);
    if (isErr(s)) return s;
    const n = int(args[1]);
    if (isErr(n)) return n;
    if (n < 0 || s.length * n > MAX_TEXT) return Errors.VALUE;
    return s.repeat(n);
  }),
  textFn('T', 1, 1, (args) => {
    const v = args[0] as Scalar;
    if (v instanceof ErrorValue) return v;
    return typeof v === 'string' ? v : '';
  }),
  fn(
    'CONCATENATE',
    1,
    VARIADIC,
    (args) => {
      let out = '';
      for (const a of args) {
        const s = str(a);
        if (isErr(s)) return s;
        out += s;
      }
      return capped(out);
    },
    { lift: 'all' },
  ),
  textFn('TEXT', 2, 2, textImpl),
  textFn('DOLLAR', 1, 2, dollar),
  textFn('FIXED', 1, 3, fixed),
  textFn('VALUE', 1, 1, (args) => {
    const v = args[0] as Scalar;
    if (typeof v === 'number' || v instanceof ErrorValue) return v;
    if (v === BLANK || v === MISSING) return 0;
    if (typeof v === 'boolean') return Errors.VALUE;
    const n = parseNumberText(v as string);
    return n === undefined ? Errors.VALUE : n;
  }),
  textFn('NUMBERVALUE', 1, 3, numberValue),
  textFn('LENB', 1, 1, (args) => {
    const s = str(args[0]);
    return isErr(s) ? s : s.length;
  }),
  textFn('LEFTB', 1, 2, (args) => {
    const s = str(args[0]);
    if (isErr(s)) return s;
    const k = int(args[1], 1);
    if (isErr(k)) return k;
    return k < 0 ? Errors.VALUE : s.slice(0, k);
  }),
  textFn('RIGHTB', 1, 2, (args) => {
    const s = str(args[0]);
    if (isErr(s)) return s;
    const k = int(args[1], 1);
    if (isErr(k)) return k;
    return k < 0 ? Errors.VALUE : k === 0 ? '' : s.slice(-k);
  }),
  textFn('MIDB', 3, 3, (args) => {
    const s = str(args[0]);
    if (isErr(s)) return s;
    const start = int(args[1]);
    if (isErr(start)) return start;
    const k = int(args[2]);
    if (isErr(k)) return k;
    if (start < 1 || k < 0) return Errors.VALUE;
    return s.slice(start - 1, start - 1 + k);
  }),
  fn('TEXTBEFORE', 2, 6, (args, ctx) => textBeforeAfter(args, ctx, false)),
  fn('TEXTAFTER', 2, 6, (args, ctx) => textBeforeAfter(args, ctx, true)),
  fn('TEXTSPLIT', 2, 6, textSplit),
  textFn('VALUETOTEXT', 1, 2, (args) => {
    const v = args[0] === MISSING ? BLANK : (args[0] as Scalar);
    const f = int(args[1], 0);
    if (isErr(f)) return f;
    if (f !== 0 && f !== 1) return Errors.VALUE;
    return valueText(v, f === 1);
  }),
  fn('ARRAYTOTEXT', 1, 2, (args, ctx) => {
    const arr = arrayOf(args[0], ctx);
    if (isErr(arr)) return arr;
    const f = isOmitted(args[1]) ? 0 : toNumber(ctx.toScalar(force(args[1])));
    if (isErr(f)) return f;
    if (f !== 0 && f !== 1) return Errors.VALUE;
    if (f === 0)
      return capped(
        arr
          .flat()
          .map((v) => valueText(v, false))
          .join(', '),
      );
    return capped(`{${arr.data.map((r) => r.map((v) => valueText(v, true)).join(',')).join(';')}}`);
  }),
  textFn('REGEXTEST', 2, 3, (args) => {
    const a = regexArgs(args, 2);
    if (isErr(a)) return a;
    const re = regex(a.re, a.ci, false);
    return isErr(re) ? re : re.test(a.text);
  }),
  fn(
    'REGEXREPLACE',
    3,
    5,
    (args) => {
      const a = regexArgs(args, 4);
      if (isErr(a)) return a;
      const repl = str(args[2]);
      if (isErr(repl)) return repl;
      const occ = int(args[3], 0);
      if (isErr(occ)) return occ;
      const re = regex(a.re, a.ci, true);
      if (isErr(re)) return re;
      if (occ === 0) return capped(a.text.replace(re, repl));
      const all = [...a.text.matchAll(re)];
      const m = occ > 0 ? all[occ - 1] : all[all.length + occ];
      if (!m || m.index === undefined) return a.text;
      const one = new RegExp(re.source, re.flags.replace('g', ''));
      const replaced = m[0].replace(one, repl);
      return capped(a.text.slice(0, m.index) + replaced + a.text.slice(m.index + m[0].length));
    },
    { lift: [0, 1, 2, 3, 4] },
  ),
  fn(
    'REGEXEXTRACT',
    2,
    4,
    (args) => {
      const a = regexArgs(args, 3);
      if (isErr(a)) return a;
      const mode = int(args[2], 0);
      if (isErr(mode)) return mode;
      if (mode < 0 || mode > 2) return Errors.VALUE;
      const re = regex(a.re, a.ci, mode === 1);
      if (isErr(re)) return re;
      if (mode === 1) {
        const all = [...a.text.matchAll(re)].map((m) => m[0]);
        return all.length === 0 ? Errors.NA : all.length === 1 ? (all[0] as string) : Array2D.column(all);
      }
      const m = re.exec(a.text);
      if (!m) return Errors.NA;
      if (mode === 0) return m[0];
      const groups = m.slice(1).map((g) => g ?? '');
      return groups.length === 0
        ? Errors.NA
        : groups.length === 1
          ? (groups[0] as string)
          : Array2D.row(groups);
    },
    { lift: [0, 1, 3] },
  ),
];
