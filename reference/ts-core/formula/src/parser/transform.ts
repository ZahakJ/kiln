/**
 * Reference rewriting: structural edits (insert/delete rows and columns), copy with relative
 * adjustment, cut/move, sheet renames, A1↔R1C1 conversion and the F4 absolute/relative cycle.
 *
 * Rewrites splice new reference text into the original formula, so the user's spacing, line breaks
 * and capitalisation outside the changed references are preserved.
 */
import type { ErrorNode, Node, RefAddress, RefNode, SheetPrefix } from './ast.ts';
import { walk } from './ast.ts';
import { type ParseOptions, parse } from './parser.ts';
import { formatAddressR1C1, type PrintOptions, print } from './printer.ts';
import { addressBounds, formatAddress, MAX_COLS, MAX_ROWS } from './references.ts';
import { type TokenizeOptions, tokenize } from './tokenizer.ts';

export type StructuralChange = {
  kind: 'insertRows' | 'deleteRows' | 'insertCols' | 'deleteCols';
  /** Sheet whose rows/columns change. */
  sheet: string;
  /** 0-based index of the first inserted/deleted row or column. */
  at: number;
  count: number;
};

type RefLike = RefNode | ErrorNode | Extract<Node, { type: 'name' }>;

/** Returns a replacement for a reference-like node, or undefined to keep it. */
export type RefMapper = (node: RefLike) => Node | undefined;

/**
 * Applies `mapper` to every reference, error and name node and splices the replacements into the
 * source text. Returns the original string when nothing changed.
 */
export function rewriteFormula(
  text: string,
  mapper: RefMapper,
  opts: { parse?: ParseOptions; print?: PrintOptions } = {},
): string {
  const ast = parse(text, opts.parse);
  const edits: { start: number; end: number; text: string }[] = [];
  walk(ast, (n) => {
    if (n.type === 'ref' || n.type === 'error' || n.type === 'name') {
      const r = mapper(n);
      if (r) edits.push({ start: n.span.start, end: n.span.end, text: print(r, opts.print) });
    }
    return undefined;
  });
  if (edits.length === 0) return text;
  edits.sort((a, b) => b.start - a.start);
  let out = text;
  for (const e of edits) out = out.slice(0, e.start) + e.text + out.slice(e.end);
  return out;
}

const sameSheet = (a: string, b: string) => a.toUpperCase() === b.toUpperCase();

/** The sheet a reference points to (its prefix or the formula's own sheet); undefined for 3-D/external. */
function targetSheet(prefix: SheetPrefix | undefined, formulaSheet: string): string | undefined {
  if (!prefix) return formulaSheet;
  if (prefix.workbook !== undefined || prefix.sheet2 !== undefined) return undefined;
  return prefix.sheet;
}

function refError(node: RefNode): ErrorNode {
  return node.prefix
    ? { type: 'error', code: '#REF!', prefix: node.prefix, span: node.span }
    : { type: 'error', code: '#REF!', span: node.span };
}

function shiftRange(lo: number, hi: number, at: number, count: number, insert: boolean, max: number) {
  if (insert) {
    const nlo = lo >= at ? lo + count : lo;
    if (nlo >= max) return null;
    const nhi = hi >= at ? Math.min(hi + count, max - 1) : hi;
    return [nlo, nhi] as const;
  }
  const end = at + count; // exclusive
  if (lo >= at && hi < end) return null;
  const nlo = lo < at ? lo : lo >= end ? lo - count : at;
  const nhi = hi >= end ? hi - count : hi >= at ? at - 1 : hi;
  if (nlo > nhi) return null;
  return [nlo, nhi] as const;
}

/** Shifts an address for an insert/delete of rows or columns. Null means the reference is deleted. */
export function shiftAddress(addr: RefAddress, change: Omit<StructuralChange, 'sheet'>): RefAddress | null {
  const rows = change.kind === 'insertRows' || change.kind === 'deleteRows';
  const insert = change.kind === 'insertRows' || change.kind === 'insertCols';
  const max = rows ? MAX_ROWS : MAX_COLS;
  switch (addr.kind) {
    case 'cell': {
      const v = rows ? addr.row : addr.col;
      const r = shiftRange(v, v, change.at, change.count, insert, max);
      if (!r) return null;
      if (r[0] === v) return addr;
      return rows ? { ...addr, row: r[0] } : { ...addr, col: r[0] };
    }
    case 'area': {
      const r = rows
        ? shiftRange(addr.r0, addr.r1, change.at, change.count, insert, max)
        : shiftRange(addr.c0, addr.c1, change.at, change.count, insert, max);
      if (!r) return null;
      return rows ? { ...addr, r0: r[0], r1: r[1] } : { ...addr, c0: r[0], c1: r[1] };
    }
    case 'rows': {
      if (!rows) return addr;
      const r = shiftRange(addr.r0, addr.r1, change.at, change.count, insert, max);
      return r ? { ...addr, r0: r[0], r1: r[1] } : null;
    }
    case 'cols': {
      if (rows) return addr;
      const r = shiftRange(addr.c0, addr.c1, change.at, change.count, insert, max);
      return r ? { ...addr, c0: r[0], c1: r[1] } : null;
    }
  }
}

function sameAddress(a: RefAddress, b: RefAddress): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** Rewrites a formula living on `formulaSheet` for a structural change. */
export function shiftFormula(text: string, change: StructuralChange, formulaSheet: string): string {
  return rewriteFormula(text, (n) => {
    if (n.type !== 'ref') return undefined;
    const sheet = targetSheet(n.prefix, formulaSheet);
    if (sheet === undefined || !sameSheet(sheet, change.sheet)) return undefined;
    const next = shiftAddress(n.address, change);
    if (!next) return refError(n);
    return sameAddress(next, n.address) ? undefined : { ...n, address: next };
  });
}

/** Moves the relative parts of an address by (dRow, dCol). Null when it leaves the sheet. */
export function offsetRelative(addr: RefAddress, dRow: number, dCol: number): RefAddress | null {
  const okR = (r: number) => r >= 0 && r < MAX_ROWS;
  const okC = (c: number) => c >= 0 && c < MAX_COLS;
  switch (addr.kind) {
    case 'cell': {
      const row = addr.rowAbs ? addr.row : addr.row + dRow;
      const col = addr.colAbs ? addr.col : addr.col + dCol;
      return okR(row) && okC(col) ? { ...addr, row, col } : null;
    }
    case 'area': {
      const r0 = addr.r0Abs ? addr.r0 : addr.r0 + dRow;
      const r1 = addr.r1Abs ? addr.r1 : addr.r1 + dRow;
      const c0 = addr.c0Abs ? addr.c0 : addr.c0 + dCol;
      const c1 = addr.c1Abs ? addr.c1 : addr.c1 + dCol;
      if (!okR(r0) || !okR(r1) || !okC(c0) || !okC(c1)) return null;
      return normalizeArea({ ...addr, r0, r1, c0, c1 });
    }
    case 'rows': {
      const r0 = addr.r0Abs ? addr.r0 : addr.r0 + dRow;
      const r1 = addr.r1Abs ? addr.r1 : addr.r1 + dRow;
      if (!okR(r0) || !okR(r1)) return null;
      return r0 <= r1
        ? { ...addr, r0, r1 }
        : { kind: 'rows', r0: r1, r1: r0, r0Abs: addr.r1Abs, r1Abs: addr.r0Abs };
    }
    case 'cols': {
      const c0 = addr.c0Abs ? addr.c0 : addr.c0 + dCol;
      const c1 = addr.c1Abs ? addr.c1 : addr.c1 + dCol;
      if (!okC(c0) || !okC(c1)) return null;
      return c0 <= c1
        ? { ...addr, c0, c1 }
        : { kind: 'cols', c0: c1, c1: c0, c0Abs: addr.c1Abs, c1Abs: addr.c0Abs };
    }
  }
}

function normalizeArea(a: Extract<RefAddress, { kind: 'area' }>): RefAddress {
  let { r0, r1, c0, c1, r0Abs, r1Abs, c0Abs, c1Abs } = a;
  if (r0 > r1) [r0, r1, r0Abs, r1Abs] = [r1, r0, r1Abs, r0Abs];
  if (c0 > c1) [c0, c1, c0Abs, c1Abs] = [c1, c0, c1Abs, c0Abs];
  return { kind: 'area', r0, c0, r1, c1, r0Abs, c0Abs, r1Abs, c1Abs };
}

/**
 * Copy/fill semantics: relative references move with the formula. A formula copied from (r, c) to
 * (r + dRow, c + dCol); references pushed off the sheet become `#REF!`.
 */
export function copyFormula(text: string, dRow: number, dCol: number): string {
  if (dRow === 0 && dCol === 0) return text;
  return rewriteFormula(text, (n) => {
    if (n.type !== 'ref') return undefined;
    const next = offsetRelative(n.address, dRow, dCol);
    if (!next) return refError(n);
    return sameAddress(next, n.address) ? undefined : { ...n, address: next };
  });
}

export interface MoveSpec {
  /** Sheet of the moved block. */
  sheet: string;
  r0: number;
  c0: number;
  r1: number;
  c1: number;
  dRow: number;
  dCol: number;
  /** Destination sheet when moving to another sheet. */
  toSheet?: string;
}

function inside(
  b: { r0: number; c0: number; r1: number; c1: number },
  m: { r0: number; c0: number; r1: number; c1: number },
) {
  return b.r0 >= m.r0 && b.r1 <= m.r1 && b.c0 >= m.c0 && b.c1 <= m.c1;
}

/**
 * Cut/paste semantics for a formula on `formulaSheet`: references that lie entirely inside the moved
 * block follow it (absolute or not); references to cells the block lands on become `#REF!`.
 * The moved formulas themselves keep their text (call this on them too so references into their own
 * block follow).
 */
export function moveFormula(text: string, move: MoveSpec, formulaSheet: string): string {
  const src = { r0: move.r0, c0: move.c0, r1: move.r1, c1: move.c1 };
  const dest = {
    r0: move.r0 + move.dRow,
    c0: move.c0 + move.dCol,
    r1: move.r1 + move.dRow,
    c1: move.c1 + move.dCol,
  };
  const toSheet = move.toSheet ?? move.sheet;
  return rewriteFormula(text, (n) => {
    if (n.type !== 'ref') return undefined;
    const sheet = targetSheet(n.prefix, formulaSheet);
    if (sheet === undefined) return undefined;
    const b = addressBounds(n.address);
    if (n.address.kind === 'cell' || n.address.kind === 'area') {
      if (sameSheet(sheet, move.sheet) && inside(b, src)) {
        const a = n.address;
        const next: RefAddress =
          a.kind === 'cell'
            ? { ...a, row: a.row + move.dRow, col: a.col + move.dCol }
            : {
                ...a,
                r0: a.r0 + move.dRow,
                r1: a.r1 + move.dRow,
                c0: a.c0 + move.dCol,
                c1: a.c1 + move.dCol,
              };
        let prefix = n.prefix;
        if (!sameSheet(toSheet, move.sheet)) {
          prefix = sameSheet(toSheet, formulaSheet) ? undefined : { sheet: toSheet };
        }
        const node: RefNode = prefix
          ? { type: 'ref', prefix, address: next, span: n.span }
          : { type: 'ref', address: next, span: n.span };
        return node;
      }
      if (sameSheet(sheet, toSheet) && inside(b, dest)) return refError(n);
    }
    return undefined;
  });
}

/** Updates sheet qualifiers after a sheet rename. */
export function renameSheetInFormula(text: string, oldName: string, newName: string): string {
  return rewriteFormula(text, (n) => {
    if (!n.prefix || n.prefix.workbook !== undefined) return undefined;
    const p = n.prefix;
    const hit1 = sameSheet(p.sheet, oldName);
    const hit2 = p.sheet2 !== undefined && sameSheet(p.sheet2, oldName);
    if (!hit1 && !hit2) return undefined;
    const prefix: SheetPrefix = { sheet: hit1 ? newName : p.sheet };
    if (p.sheet2 !== undefined) prefix.sheet2 = hit2 ? newName : p.sheet2;
    return { ...n, prefix } as Node;
  });
}

/** Turns references to a deleted sheet into `#REF!`. */
export function removeSheetInFormula(text: string, sheetName: string): string {
  return rewriteFormula(text, (n) => {
    if (n.type === 'error' || !n.prefix || n.prefix.workbook !== undefined) return undefined;
    const p = n.prefix;
    if (p.sheet2 !== undefined) return undefined;
    if (!sameSheet(p.sheet, sheetName)) return undefined;
    return { type: 'error', code: n.type === 'ref' ? '#REF!' : '#NAME?', span: n.span };
  });
}

/** Converts an A1 formula on cell (row, col) to R1C1 notation. */
export function toR1C1(text: string, row: number, col: number): string {
  return rewriteFormula(text, (n) => (n.type === 'ref' ? n : undefined), {
    print: { style: 'R1C1', anchor: { row, col } },
  });
}

/** Converts an R1C1 formula on cell (row, col) to A1 notation. */
export function fromR1C1(text: string, row: number, col: number): string {
  return rewriteFormula(text, (n) => (n.type === 'ref' ? n : undefined), {
    parse: { style: 'R1C1', anchor: { row, col } },
  });
}

/** Formats a single address in R1C1 relative to (row, col). */
export function addressToR1C1(addr: RefAddress, row: number, col: number): string {
  return formatAddressR1C1(addr, { row, col });
}

/** Next state of the F4 cycle: A1 → $A$1 → A$1 → $A1 → A1. */
export function cycleAbsolute(addr: RefAddress): RefAddress {
  const next = (colAbs: boolean, rowAbs: boolean): [boolean, boolean] => {
    if (!colAbs && !rowAbs) return [true, true];
    if (colAbs && rowAbs) return [false, true];
    if (!colAbs && rowAbs) return [true, false];
    return [false, false];
  };
  switch (addr.kind) {
    case 'cell': {
      const [colAbs, rowAbs] = next(addr.colAbs, addr.rowAbs);
      return { ...addr, colAbs, rowAbs };
    }
    case 'area': {
      const [c, r] = next(addr.c0Abs, addr.r0Abs);
      return { ...addr, c0Abs: c, c1Abs: c, r0Abs: r, r1Abs: r };
    }
    case 'rows': {
      const v = !addr.r0Abs;
      return { ...addr, r0Abs: v, r1Abs: v };
    }
    case 'cols': {
      const v = !addr.c0Abs;
      return { ...addr, c0Abs: v, c1Abs: v };
    }
  }
}

export interface ToggleResult {
  text: string;
  selectionStart: number;
  selectionEnd: number;
}

/**
 * The F4 key in the formula bar: cycles `$` on the reference(s) at the caret or inside the selection.
 * Works on incomplete formulas (it only tokenizes). The returned selection covers the toggled
 * references, as Excel does.
 */
export function toggleAbsolute(
  text: string,
  selectionStart: number,
  selectionEnd: number = selectionStart,
  opts: TokenizeOptions = {},
): ToggleResult {
  const lead = /^\s*=/.exec(text);
  const offset = lead ? lead[0].length : 0;
  const tokens = tokenize(text.slice(offset), opts, offset).filter((t) => t.kind === 'ref');
  let hits = tokens.filter((t) => t.end > selectionStart && t.start < selectionEnd);
  if (hits.length === 0) {
    const atCaret = tokens.find((t) => t.start <= selectionStart && t.end >= selectionStart);
    hits = atCaret ? [atCaret] : [];
  }
  if (hits.length === 0) return { text, selectionStart, selectionEnd };
  let out = text;
  let delta = 0;
  let first = -1;
  let last = -1;
  for (const t of hits) {
    const bang = t.text.lastIndexOf('!');
    const prefixText = bang >= 0 ? t.text.slice(0, bang + 1) : '';
    const next = cycleAbsolute(t.address as RefAddress);
    const addrText =
      opts.style === 'R1C1'
        ? formatAddressR1C1(next, opts.anchor ?? { row: 0, col: 0 })
        : formatAddress(next);
    const replacement = prefixText + addrText;
    const s = t.start + delta;
    out = out.slice(0, s) + replacement + out.slice(t.end + delta);
    if (first < 0) first = s;
    last = s + replacement.length;
    delta += replacement.length - (t.end - t.start);
  }
  return { text: out, selectionStart: first, selectionEnd: last };
}
