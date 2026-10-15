/**
 * Cell address helpers (A1 notation).
 */
import type { RefAddress } from './ast.ts';

export const MAX_ROWS = 1_048_576;
export const MAX_COLS = 16_384;

/** 0 → "A", 25 → "Z", 26 → "AA". */
export function columnToLetters(col: number): string {
  let n = col + 1;
  let s = '';
  while (n > 0) {
    const r = (n - 1) % 26;
    s = String.fromCharCode(65 + r) + s;
    n = Math.floor((n - 1) / 26);
  }
  return s;
}

/** "A" → 0, "xfd" → 16383. Returns -1 for invalid input. */
export function lettersToColumn(letters: string): number {
  let n = 0;
  for (let i = 0; i < letters.length; i++) {
    const c = letters.charCodeAt(i) | 0x20; // lower-case
    if (c < 97 || c > 122) return -1;
    n = n * 26 + (c - 96);
  }
  return n - 1;
}

/** Formats a single cell such as `$B$3`. */
export function formatCell(row: number, col: number, rowAbs = false, colAbs = false): string {
  return `${colAbs ? '$' : ''}${columnToLetters(col)}${rowAbs ? '$' : ''}${row + 1}`;
}

/** Formats the address part of a reference (no sheet). */
export function formatAddress(a: RefAddress): string {
  switch (a.kind) {
    case 'cell':
      return formatCell(a.row, a.col, a.rowAbs, a.colAbs);
    case 'area':
      return `${formatCell(a.r0, a.c0, a.r0Abs, a.c0Abs)}:${formatCell(a.r1, a.c1, a.r1Abs, a.c1Abs)}`;
    case 'rows':
      return `${a.r0Abs ? '$' : ''}${a.r0 + 1}:${a.r1Abs ? '$' : ''}${a.r1 + 1}`;
    case 'cols':
      return `${a.c0Abs ? '$' : ''}${columnToLetters(a.c0)}:${a.c1Abs ? '$' : ''}${columnToLetters(a.c1)}`;
  }
}

/** The rectangle a reference address covers (0-based, inclusive). */
export function addressBounds(a: RefAddress): { r0: number; c0: number; r1: number; c1: number } {
  switch (a.kind) {
    case 'cell':
      return { r0: a.row, c0: a.col, r1: a.row, c1: a.col };
    case 'area':
      return { r0: a.r0, c0: a.c0, r1: a.r1, c1: a.c1 };
    case 'rows':
      return { r0: a.r0, c0: 0, r1: a.r1, c1: MAX_COLS - 1 };
    case 'cols':
      return { r0: 0, c0: a.c0, r1: MAX_ROWS - 1, c1: a.c1 };
  }
}

const CELL_RE = /^\$?([A-Za-z]{1,3})\$?(\d{1,7})$/;

/** Parses a plain cell address like `B3` or `$B$3` into 0-based indices. */
export function parseCellAddress(text: string): { row: number; col: number } | undefined {
  const m = CELL_RE.exec(text.trim());
  if (!m) return undefined;
  const col = lettersToColumn(m[1] as string);
  const row = Number(m[2]) - 1;
  if (col < 0 || col >= MAX_COLS || row < 0 || row >= MAX_ROWS) return undefined;
  return { row, col };
}

const PLAIN_SHEET_RE = /^[A-Za-z_\\¡-￿][A-Za-z0-9_.¡-￿]*$/;

/** True when a sheet name must be quoted in a formula (`'My Sheet'!A1`). */
export function sheetNeedsQuotes(name: string): boolean {
  if (!PLAIN_SHEET_RE.test(name)) return true;
  if (/^[A-Za-z]{1,3}\d+$/.test(name)) return true; // looks like a cell (A1, XFD2)
  if (/^[Rr]\d*[Cc]\d*$/.test(name) || /^[RrCc]\d*$/.test(name)) return true; // R1C1-like
  const u = name.toUpperCase();
  return u === 'TRUE' || u === 'FALSE';
}

export function quoteSheet(name: string): string {
  return `'${name.replace(/'/g, "''")}'`;
}

/** Formats a sheet prefix including the `!`. */
export function formatPrefix(p: { workbook?: string; sheet: string; sheet2?: string }): string {
  const sheets = p.sheet2 !== undefined ? `${p.sheet}:${p.sheet2}` : p.sheet;
  const needs = sheetNeedsQuotes(p.sheet) || (p.sheet2 !== undefined && sheetNeedsQuotes(p.sheet2));
  const book = p.workbook !== undefined ? `[${p.workbook}]` : '';
  return needs ? `${quoteSheet(book + sheets)}!` : `${book}${sheets}!`;
}
