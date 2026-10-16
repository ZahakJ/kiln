/**
 * Formula tokenizer.
 *
 * Never throws: malformed input produces `invalid` tokens so the formula bar can still colour an
 * incomplete formula while the user types. The parser turns invalid tokens into syntax errors.
 */
import { ERROR_CODES, type ErrorCode } from '../values/errors.ts';
import type { RefAddress, SheetPrefix, StructuredSpecial } from './ast.ts';
import { lettersToColumn, MAX_COLS, MAX_ROWS } from './references.ts';

export type TokenKind =
  | 'number'
  | 'string'
  | 'bool'
  | 'error'
  | 'ref'
  | 'name'
  | 'function'
  | 'structured'
  | 'operator'
  | 'lparen'
  | 'rparen'
  | 'lbrace'
  | 'rbrace'
  | 'comma'
  | 'semicolon'
  | 'whitespace'
  | 'invalid';

export interface StructuredData {
  table: string | null;
  specials: StructuredSpecial[];
  thisRow: boolean;
  columns: [string] | [string, string] | null;
}

export interface Token {
  kind: TokenKind;
  /** Source text of the token. */
  text: string;
  start: number;
  end: number;
  /** number | string | boolean for literals. */
  literal?: number | string | boolean;
  /** Error literals (and `Sheet1!#REF!`). */
  error?: ErrorCode;
  /** Sheet qualifier of refs, names and `Sheet!#REF!`. */
  prefix?: SheetPrefix;
  /** Parsed address of a `ref` token. */
  address?: RefAddress;
  /** Name of a `name` or `function` token (functions upper-cased). */
  name?: string;
  structured?: StructuredData;
  /** Why an `invalid` token is invalid. */
  message?: string;
}

export interface TokenizeOptions {
  /** Reference style. R1C1 references are converted to absolute indices using `anchor`. */
  style?: 'A1' | 'R1C1';
  /** The cell the formula belongs to (needed for relative R1C1 references). */
  anchor?: { row: number; col: number };
}

const SORTED_ERRORS = [...ERROR_CODES].sort((a, b) => b.length - a.length);
const SPECIALS: Record<string, StructuredSpecial | '#This Row'> = {
  '#ALL': '#All',
  '#DATA': '#Data',
  '#HEADERS': '#Headers',
  '#TOTALS': '#Totals',
  '#THIS ROW': '#This Row',
};

function isDigit(c: number): boolean {
  return c >= 48 && c <= 57;
}
function isLetter(c: number): boolean {
  return (c >= 65 && c <= 90) || (c >= 97 && c <= 122);
}
export function isNameStart(c: number): boolean {
  return isLetter(c) || c === 95 /* _ */ || c === 92 /* \ */ || c >= 0xa1;
}
export function isNameChar(c: number): boolean {
  return isNameStart(c) || isDigit(c) || c === 46 /* . */ || c === 63 /* ? */;
}
function isSheetChar(c: number): boolean {
  return isNameStart(c) || isDigit(c) || c === 46;
}
function isSpace(c: number): boolean {
  return c === 32 || c === 9 || c === 10 || c === 13 || c === 0xa0;
}

/** Characters that may not directly follow a reference. */
function blocksRefEnd(c: number): boolean {
  return Number.isNaN(c) ? false : isNameChar(c) || c === 40 /* ( */ || c === 91 /* [ */ || c === 33; /* ! */
}

interface Scanned<T> {
  value: T;
  end: number;
}

/** Scans a sheet prefix (`Sheet1!`, `'a b'!`, `S1:S3!`, `[1]S!`, `'[b.xlsx]S'!`) at i. */
function scanPrefix(s: string, i: number): Scanned<SheetPrefix> | undefined {
  const c = s.charCodeAt(i);
  if (c === 39 /* ' */) {
    let j = i + 1;
    let content = '';
    for (;;) {
      if (j >= s.length) return undefined;
      if (s[j] === "'") {
        if (s[j + 1] === "'") {
          content += "'";
          j += 2;
          continue;
        }
        break;
      }
      content += s[j];
      j++;
    }
    if (s[j + 1] !== '!' || content === '') return undefined;
    let workbook: string | undefined;
    const ext = /^\[([^\]]+)\](.*)$/.exec(content);
    if (ext) {
      workbook = ext[1];
      content = ext[2] as string;
    }
    const colon = content.indexOf(':');
    const prefix: SheetPrefix =
      colon > 0 ? { sheet: content.slice(0, colon), sheet2: content.slice(colon + 1) } : { sheet: content };
    if (workbook !== undefined) prefix.workbook = workbook;
    if (prefix.sheet === '' || prefix.sheet2 === '') return undefined;
    return { value: prefix, end: j + 2 };
  }
  let j = i;
  let workbook: string | undefined;
  if (c === 91 /* [ */) {
    const close = s.indexOf(']', i);
    if (close < 0) return undefined;
    workbook = s.slice(i + 1, close);
    if (workbook === '' || workbook.includes('[')) return undefined;
    j = close + 1;
  }
  const readName = (k: number): number => {
    if (k >= s.length || !isNameStart(s.charCodeAt(k))) return k;
    let e = k + 1;
    while (e < s.length && isSheetChar(s.charCodeAt(e))) e++;
    return e;
  };
  const e1 = readName(j);
  if (e1 === j) return undefined;
  let sheet2: string | undefined;
  let end = e1;
  if (s[e1] === ':') {
    const e2 = readName(e1 + 1);
    if (e2 > e1 + 1 && s[e2] === '!') {
      sheet2 = s.slice(e1 + 1, e2);
      end = e2;
    }
  }
  if (s[end] !== '!') return undefined;
  const prefix: SheetPrefix = { sheet: s.slice(j, e1) };
  if (sheet2 !== undefined) prefix.sheet2 = sheet2;
  if (workbook !== undefined) prefix.workbook = workbook;
  return { value: prefix, end: end + 1 };
}

const A1_CELL = /\$?([A-Za-z]{1,3})\$?(\d{1,7})/y;
const A1_COLS = /(\$?)([A-Za-z]{1,3}):(\$?)([A-Za-z]{1,3})/y;
const A1_ROWS = /(\$?)(\d{1,7}):(\$?)(\d{1,7})/y;

interface CellMatch {
  row: number;
  col: number;
  rowAbs: boolean;
  colAbs: boolean;
  end: number;
}

function matchA1Cell(s: string, i: number): CellMatch | undefined {
  A1_CELL.lastIndex = i;
  const m = A1_CELL.exec(s);
  if (!m) return undefined;
  const col = lettersToColumn(m[1] as string);
  const row = Number(m[2]) - 1;
  if (col < 0 || col >= MAX_COLS || row < 0 || row >= MAX_ROWS) return undefined;
  const text = m[0];
  return { row, col, colAbs: text.startsWith('$'), rowAbs: text.indexOf('$', 1) > 0, end: i + text.length };
}

/** Matches an A1 address (cell, area, column range or row range) at i. */
export function matchA1Address(s: string, i: number): Scanned<RefAddress> | undefined {
  const a = matchA1Cell(s, i);
  if (a) {
    if (s[a.end] === ':') {
      const b = matchA1Cell(s, a.end + 1);
      if (b && !blocksRefEnd(s.charCodeAt(b.end))) {
        return { value: makeArea(a, b), end: b.end };
      }
    }
    if (!blocksRefEnd(s.charCodeAt(a.end))) {
      return {
        value: { kind: 'cell', row: a.row, col: a.col, rowAbs: a.rowAbs, colAbs: a.colAbs },
        end: a.end,
      };
    }
    return undefined;
  }
  A1_COLS.lastIndex = i;
  let m = A1_COLS.exec(s);
  if (m) {
    const end = i + m[0].length;
    const c0 = lettersToColumn(m[2] as string);
    const c1 = lettersToColumn(m[4] as string);
    if (c0 < MAX_COLS && c1 < MAX_COLS && !blocksRefEnd(s.charCodeAt(end))) {
      const [lo, hi] = c0 <= c1 ? [0, 1] : [1, 0];
      const cols = [c0, c1];
      const abs = [m[1] === '$', m[3] === '$'];
      return {
        value: {
          kind: 'cols',
          c0: cols[lo] as number,
          c1: cols[hi] as number,
          c0Abs: abs[lo] as boolean,
          c1Abs: abs[hi] as boolean,
        },
        end,
      };
    }
  }
  A1_ROWS.lastIndex = i;
  m = A1_ROWS.exec(s);
  if (m) {
    const end = i + m[0].length;
    const r0 = Number(m[2]) - 1;
    const r1 = Number(m[4]) - 1;
    if (r0 >= 0 && r1 >= 0 && r0 < MAX_ROWS && r1 < MAX_ROWS && !blocksRefEnd(s.charCodeAt(end))) {
      const [lo, hi] = r0 <= r1 ? [0, 1] : [1, 0];
      const rows = [r0, r1];
      const abs = [m[1] === '$', m[3] === '$'];
      return {
        value: {
          kind: 'rows',
          r0: rows[lo] as number,
          r1: rows[hi] as number,
          r0Abs: abs[lo] as boolean,
          r1Abs: abs[hi] as boolean,
        },
        end,
      };
    }
  }
  return undefined;
}

function makeArea(a: CellMatch, b: CellMatch): RefAddress {
  const [r0, r0Abs, r1, r1Abs] =
    a.row <= b.row ? [a.row, a.rowAbs, b.row, b.rowAbs] : [b.row, b.rowAbs, a.row, a.rowAbs];
  const [c0, c0Abs, c1, c1Abs] =
    a.col <= b.col ? [a.col, a.colAbs, b.col, b.colAbs] : [b.col, b.colAbs, a.col, a.colAbs];
  return { kind: 'area', r0, c0, r1, c1, r0Abs, c0Abs, r1Abs, c1Abs };
}

interface R1C1Part {
  present: boolean;
  abs: boolean;
  n: number; // absolute 1-based index or relative offset
  end: number;
}

const R1C1_PART = /([RrCc])(?:\[([+-]?\d+)\]|(\d+))?/y;

function matchR1C1Part(s: string, i: number, letter: 'R' | 'C'): R1C1Part | undefined {
  R1C1_PART.lastIndex = i;
  const m = R1C1_PART.exec(s);
  if (!m || (m[1] as string).toUpperCase() !== letter) return undefined;
  if (m[2] !== undefined) return { present: true, abs: false, n: Number(m[2]), end: i + m[0].length };
  if (m[3] !== undefined) return { present: true, abs: true, n: Number(m[3]), end: i + m[0].length };
  return { present: true, abs: false, n: 0, end: i + m[0].length };
}

/** Matches an R1C1 address at i, resolving relative parts against `anchor`. */
export function matchR1C1Address(
  s: string,
  i: number,
  anchor: { row: number; col: number },
): Scanned<RefAddress | 'invalid'> | undefined {
  const resolve = (p: R1C1Part, base: number, max: number): number | undefined => {
    const v = p.abs ? p.n - 1 : base + p.n;
    return v >= 0 && v < max ? v : undefined;
  };
  const rowPart = matchR1C1Part(s, i, 'R');
  if (rowPart) {
    const colPart = matchR1C1Part(s, rowPart.end, 'C');
    if (colPart) {
      const r = resolve(rowPart, anchor.row, MAX_ROWS);
      const c = resolve(colPart, anchor.col, MAX_COLS);
      if (s[colPart.end] === ':') {
        const r2p = matchR1C1Part(s, colPart.end + 1, 'R');
        const c2p = r2p ? matchR1C1Part(s, r2p.end, 'C') : undefined;
        if (r2p && c2p && !blocksRefEnd(s.charCodeAt(c2p.end))) {
          const r2 = resolve(r2p, anchor.row, MAX_ROWS);
          const c2 = resolve(c2p, anchor.col, MAX_COLS);
          if (r === undefined || c === undefined || r2 === undefined || c2 === undefined)
            return { value: 'invalid', end: c2p.end };
          const value = makeArea(
            { row: r, col: c, rowAbs: rowPart.abs, colAbs: colPart.abs, end: 0 },
            { row: r2, col: c2, rowAbs: r2p.abs, colAbs: c2p.abs, end: 0 },
          );
          return { value, end: c2p.end };
        }
      }
      if (blocksRefEnd(s.charCodeAt(colPart.end))) return undefined;
      if (r === undefined || c === undefined) return { value: 'invalid', end: colPart.end };
      return {
        value: { kind: 'cell', row: r, col: c, rowAbs: rowPart.abs, colAbs: colPart.abs },
        end: colPart.end,
      };
    }
    // Whole rows: R1, R[1], R, R1:R3
    let end = rowPart.end;
    let second = rowPart;
    if (s[end] === ':') {
      const p2 = matchR1C1Part(s, end + 1, 'R');
      if (p2 && !matchR1C1Part(s, p2.end, 'C')) {
        second = p2;
        end = p2.end;
      }
    }
    if (blocksRefEnd(s.charCodeAt(end))) return undefined;
    const a = resolve(rowPart, anchor.row, MAX_ROWS);
    const b = resolve(second, anchor.row, MAX_ROWS);
    if (a === undefined || b === undefined) return { value: 'invalid', end };
    const [r0, r0Abs, r1, r1Abs] = a <= b ? [a, rowPart.abs, b, second.abs] : [b, second.abs, a, rowPart.abs];
    return { value: { kind: 'rows', r0, r1, r0Abs, r1Abs }, end };
  }
  const colPart = matchR1C1Part(s, i, 'C');
  if (colPart) {
    let end = colPart.end;
    let second = colPart;
    if (s[end] === ':') {
      const p2 = matchR1C1Part(s, end + 1, 'C');
      if (p2) {
        second = p2;
        end = p2.end;
      }
    }
    if (blocksRefEnd(s.charCodeAt(end))) return undefined;
    const a = resolve(colPart, anchor.col, MAX_COLS);
    const b = resolve(second, anchor.col, MAX_COLS);
    if (a === undefined || b === undefined) return { value: 'invalid', end };
    const [c0, c0Abs, c1, c1Abs] = a <= b ? [a, colPart.abs, b, second.abs] : [b, second.abs, a, colPart.abs];
    return { value: { kind: 'cols', c0, c1, c0Abs, c1Abs }, end };
  }
  return undefined;
}

/** Finds the end of a bracketed structured reference starting at `[` (handles `'` escapes). */
function scanBrackets(s: string, i: number): number {
  let depth = 0;
  for (let j = i; j < s.length; j++) {
    const c = s[j];
    if (c === "'") {
      j++;
      continue;
    }
    if (c === '[') depth++;
    else if (c === ']') {
      depth--;
      if (depth === 0) return j + 1;
    }
  }
  return -1;
}

function unescapeColumn(s: string): string {
  return s.replace(/'(.)/g, '$1');
}

/** Parses the bracketed items of `[[#Headers],[A]:[B]]` (the text between the outer brackets). */
function parseItemList(text: string): Omit<StructuredData, 'table'> | string {
  const specials: StructuredSpecial[] = [];
  let thisRow = false;
  let columns: [string] | [string, string] | null = null;
  let i = 0;
  const skipSpaces = () => {
    while (i < text.length && text[i] === ' ') i++;
  };
  const readItem = (): string | undefined => {
    if (text[i] !== '[') return undefined;
    const end = scanBrackets(text, i);
    if (end < 0) return undefined;
    const inner = text.slice(i + 1, end - 1);
    i = end;
    return inner;
  };
  for (;;) {
    skipSpaces();
    const first = readItem();
    if (first === undefined) return 'Expected a bracketed table item';
    skipSpaces();
    if (first.startsWith('#')) {
      const sp = SPECIALS[first.toUpperCase()];
      if (!sp) return `Unknown table item ${first}`;
      if (sp === '#This Row') thisRow = true;
      else if (!specials.includes(sp)) specials.push(sp);
    } else {
      if (columns) return 'Only one column or column range is allowed';
      if (text[i] === ':') {
        i++;
        skipSpaces();
        const second = readItem();
        if (second === undefined || second.startsWith('#')) return 'Expected a column after ":"';
        columns = [unescapeColumn(first), unescapeColumn(second)];
        skipSpaces();
      } else {
        columns = [unescapeColumn(first)];
      }
    }
    if (i >= text.length) break;
    if (text[i] !== ',') return 'Expected "," between table items';
    i++;
  }
  return { specials, thisRow, columns };
}

/** Parses the inside of `Table1[ ... ]`. */
export function parseStructuredInner(inner: string): Omit<StructuredData, 'table'> | string {
  const c = inner.trim();
  if (c === '') return { specials: [], thisRow: false, columns: null };
  if (c.startsWith('@')) {
    const rest = c.slice(1).trim();
    if (rest === '') return { specials: [], thisRow: true, columns: null };
    if (rest.startsWith('[')) {
      const r = parseItemList(rest);
      if (typeof r === 'string') return r;
      if (r.specials.length > 0) return 'Table items cannot follow @';
      return { specials: [], thisRow: true, columns: r.columns };
    }
    return { specials: [], thisRow: true, columns: [unescapeColumn(rest)] };
  }
  if (c.startsWith('#')) {
    const sp = SPECIALS[c.toUpperCase()];
    if (!sp) return `Unknown table item ${c}`;
    return sp === '#This Row'
      ? { specials: [], thisRow: true, columns: null }
      : { specials: [sp], thisRow: false, columns: null };
  }
  if (c.startsWith('[')) return parseItemList(c);
  return { specials: [], thisRow: false, columns: [unescapeColumn(inner)] };
}

const OPERAND_END: ReadonlySet<TokenKind> = new Set<TokenKind>([
  'number',
  'string',
  'bool',
  'error',
  'ref',
  'name',
  'structured',
  'rparen',
  'rbrace',
]);

/**
 * Splits a formula into tokens. `offset` is added to every position (use 1 when the leading `=` was
 * stripped). The input should not include the leading `=`; if it does, it is tokenized as an operator.
 */
export function tokenize(src: string, opts: TokenizeOptions = {}, offset = 0): Token[] {
  const tokens: Token[] = [];
  const r1c1 = opts.style === 'R1C1';
  const anchor = opts.anchor ?? { row: 0, col: 0 };
  let braceDepth = 0;
  let i = 0;
  const push = (t: Omit<Token, 'start' | 'end' | 'text'>, start: number, end: number) => {
    tokens.push({ ...t, text: src.slice(start, end), start: start + offset, end: end + offset });
    i = end;
  };
  const prevKind = (): TokenKind | undefined => tokens[tokens.length - 1]?.kind;

  const tryReference = (start: number): boolean => {
    let j = start;
    const pre = scanPrefix(src, j);
    let prefix: SheetPrefix | undefined;
    if (pre) {
      prefix = pre.value;
      j = pre.end;
    }
    const addr = r1c1 ? matchR1C1Address(src, j, anchor) : matchA1Address(src, j);
    if (addr) {
      if (addr.value === 'invalid') {
        push({ kind: 'error', error: '#REF!', ...(prefix ? { prefix } : {}) }, start, addr.end);
      } else {
        push({ kind: 'ref', address: addr.value, ...(prefix ? { prefix } : {}) }, start, addr.end);
      }
      return true;
    }
    if (!prefix) return false;
    if (src.slice(j, j + 5).toUpperCase() === '#REF!') {
      push({ kind: 'error', error: '#REF!', prefix }, start, j + 5);
      return true;
    }
    if (j < src.length && isNameStart(src.charCodeAt(j))) {
      let e = j + 1;
      while (e < src.length && isNameChar(src.charCodeAt(e))) e++;
      push({ kind: 'name', name: src.slice(j, e), prefix }, start, e);
      return true;
    }
    push({ kind: 'invalid', message: 'Expected a reference after the sheet name' }, start, j);
    return true;
  };

  while (i < src.length) {
    const start = i;
    const ch = src[i] as string;
    const c = src.charCodeAt(i);

    if (isSpace(c)) {
      let e = i + 1;
      while (e < src.length && isSpace(src.charCodeAt(e))) e++;
      push({ kind: 'whitespace' }, start, e);
      continue;
    }
    if (ch === '"') {
      let e = i + 1;
      let value = '';
      let closed = false;
      while (e < src.length) {
        if (src[e] === '"') {
          if (src[e + 1] === '"') {
            value += '"';
            e += 2;
            continue;
          }
          closed = true;
          e++;
          break;
        }
        value += src[e];
        e++;
      }
      if (closed) push({ kind: 'string', literal: value }, start, e);
      else push({ kind: 'invalid', message: 'Missing closing quotation mark' }, start, e);
      continue;
    }
    if (ch === '#') {
      const pk = prevKind();
      if (pk && OPERAND_END.has(pk) && pk !== 'error' && tokens[tokens.length - 1]?.end === start + offset) {
        const upper = src.slice(i, i + 14).toUpperCase();
        if (!SORTED_ERRORS.some((code) => upper.startsWith(code))) {
          push({ kind: 'operator' }, start, i + 1);
          continue;
        }
      }
      const upper = src.slice(i, i + 14).toUpperCase();
      const code = SORTED_ERRORS.find((e) => upper.startsWith(e));
      if (code) push({ kind: 'error', error: code }, start, i + code.length);
      else if (pk && OPERAND_END.has(pk)) push({ kind: 'operator' }, start, i + 1);
      else push({ kind: 'invalid', message: 'Unknown error value' }, start, i + 1);
      continue;
    }
    if (ch === "'" || ch === '[') {
      if (tryReference(start)) continue;
      if (ch === '[') {
        const end = scanBrackets(src, i);
        if (end < 0) {
          push({ kind: 'invalid', message: 'Missing closing bracket' }, start, src.length);
          continue;
        }
        const parsed = parseStructuredInner(src.slice(i + 1, end - 1));
        if (typeof parsed === 'string') push({ kind: 'invalid', message: parsed }, start, end);
        else push({ kind: 'structured', structured: { table: null, ...parsed } }, start, end);
        continue;
      }
      push({ kind: 'invalid', message: 'Missing closing quote in sheet name' }, start, src.length);
      continue;
    }
    if (isDigit(c) || (ch === '.' && isDigit(src.charCodeAt(i + 1))) || ch === '$') {
      if (braceDepth === 0 && (ch === '$' || isDigit(c)) && tryReference(start)) continue;
      if (ch === '$') {
        push({ kind: 'invalid', message: 'Unexpected "$"' }, start, i + 1);
        continue;
      }
      const m = /(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?/y;
      m.lastIndex = i;
      const r = m.exec(src) as RegExpExecArray;
      push({ kind: 'number', literal: Number(r[0]) }, start, i + r[0].length);
      continue;
    }
    if (isNameStart(c)) {
      if (tryReference(start)) continue;
      let e = i + 1;
      while (e < src.length && isNameChar(src.charCodeAt(e))) e++;
      const word = src.slice(i, e);
      const next = src[e];
      if (next === '(') {
        push({ kind: 'function', name: word.toUpperCase() }, start, e);
        continue;
      }
      if (next === '[') {
        const end = scanBrackets(src, e);
        if (end < 0) {
          push({ kind: 'invalid', message: 'Missing closing bracket' }, start, src.length);
          continue;
        }
        const parsed = parseStructuredInner(src.slice(e + 1, end - 1));
        if (typeof parsed === 'string') push({ kind: 'invalid', message: parsed }, start, end);
        else push({ kind: 'structured', structured: { table: word, ...parsed } }, start, end);
        continue;
      }
      const upper = word.toUpperCase();
      if (upper === 'TRUE' || upper === 'FALSE') push({ kind: 'bool', literal: upper === 'TRUE' }, start, e);
      else push({ kind: 'name', name: word }, start, e);
      continue;
    }
    switch (ch) {
      case '(':
        push({ kind: 'lparen' }, start, i + 1);
        continue;
      case ')':
        push({ kind: 'rparen' }, start, i + 1);
        continue;
      case '{':
        braceDepth++;
        push({ kind: 'lbrace' }, start, i + 1);
        continue;
      case '}':
        braceDepth = Math.max(0, braceDepth - 1);
        push({ kind: 'rbrace' }, start, i + 1);
        continue;
      case ',':
        push({ kind: 'comma' }, start, i + 1);
        continue;
      case ';':
        push({ kind: 'semicolon' }, start, i + 1);
        continue;
      case '<':
        if (src[i + 1] === '=' || src[i + 1] === '>') push({ kind: 'operator' }, start, i + 2);
        else push({ kind: 'operator' }, start, i + 1);
        continue;
      case '>':
        push({ kind: 'operator' }, start, src[i + 1] === '=' ? i + 2 : i + 1);
        continue;
      case '+':
      case '-':
      case '*':
      case '/':
      case '^':
      case '&':
      case '=':
      case '%':
      case '@':
      case ':':
        push({ kind: 'operator' }, start, i + 1);
        continue;
      default:
        push({ kind: 'invalid', message: `Unexpected character "${ch}"` }, start, i + 1);
    }
  }
  return tokens;
}
