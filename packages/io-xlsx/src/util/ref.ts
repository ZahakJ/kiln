/** Fast A1 reference helpers for the cell hot paths. */
import { type CellRange, formatRange, MAX_COLS, MAX_ROWS, parseRange } from '@officina/sheet-model';

const LETTERS: string[] = [];

/** Column letters for a 0-based column (cached). */
export function colLetters(col: number): string {
  let s = LETTERS[col];
  if (s !== undefined) return s;
  s = '';
  let n = col + 1;
  while (n > 0) {
    const m = (n - 1) % 26;
    s = String.fromCharCode(65 + m) + s;
    n = Math.floor((n - 1) / 26);
  }
  LETTERS[col] = s;
  return s;
}

/** `A1` text for a 0-based cell. */
export function cellRef(row: number, col: number): string {
  return colLetters(col) + (row + 1);
}

/**
 * Parses a cell reference such as `AB12` (no `$`, no sheet) into 0-based indices written to `out`.
 * Returns false when the text is not a cell reference.
 */
export function parseCellRefInto(s: string, out: { row: number; col: number }): boolean {
  let i = 0;
  let col = 0;
  const n = s.length;
  if (i < n && s.charCodeAt(i) === 36) i++; // $
  const start = i;
  while (i < n) {
    const c = s.charCodeAt(i);
    if (c >= 65 && c <= 90) col = col * 26 + (c - 64);
    else if (c >= 97 && c <= 122) col = col * 26 + (c - 96);
    else break;
    i++;
  }
  if (i === start || i - start > 3) return false;
  if (i < n && s.charCodeAt(i) === 36) i++;
  let row = 0;
  const rs = i;
  while (i < n) {
    const c = s.charCodeAt(i);
    if (c < 48 || c > 57) return false;
    row = row * 10 + (c - 48);
    i++;
  }
  if (i === rs || row < 1 || row > MAX_ROWS || col > MAX_COLS) return false;
  out.row = row - 1;
  out.col = col - 1;
  return true;
}

/** Parses `A1` / `A1:B2` / `A:A` / `1:1` (with optional `$`); undefined when invalid. */
export function parseRef(text: string): CellRange | undefined {
  return parseRange(text.replace(/\$/g, ''));
}

/** Parses an sqref list, skipping invalid parts instead of failing. */
export function parseSqrefLoose(text: string | undefined): CellRange[] {
  if (!text) return [];
  const out: CellRange[] = [];
  for (const part of text.trim().split(/\s+/)) {
    const r = part ? parseRef(part) : undefined;
    if (r) out.push(r);
  }
  return out;
}

export function formatRef(r: CellRange): string {
  return formatRange(r);
}

export function formatSqrefList(ranges: readonly CellRange[]): string {
  return ranges.map((r) => formatRange(r)).join(' ');
}
