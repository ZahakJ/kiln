/**
 * Structured-reference maintenance: renaming tables and table columns inside formula text, and
 * converting a table's structured references to plain A1 references (Table Design › Convert to
 * Range). Like the formula package's reference rewriting, edits are spliced into the original text so
 * the user's spacing is preserved.
 */
import {
  formatAddress,
  formatPrefix,
  formatStructured,
  type Node,
  parse,
  type StructuredNode,
  walk,
} from '@officina/formula';
import type { CellRange } from './address.ts';
import type { Table } from './meta.ts';

function splice(text: string, edits: { start: number; end: number; text: string }[]): string {
  if (edits.length === 0) return text;
  edits.sort((a, b) => b.start - a.start);
  let out = text;
  for (const e of edits) out = out.slice(0, e.start) + e.text + out.slice(e.end);
  return out;
}

function rewriteStructured(text: string, fn: (n: StructuredNode) => string | undefined): string {
  const ast: Node = parse(text);
  const edits: { start: number; end: number; text: string }[] = [];
  walk(ast, (n) => {
    if (n.type === 'structured') {
      const r = fn(n);
      if (r !== undefined) edits.push({ start: n.span.start, end: n.span.end, text: r });
    }
    return undefined;
  });
  return splice(text, edits);
}

const same = (a: string | null, b: string) => a !== null && a.toUpperCase() === b.toUpperCase();

/** Renames a table in structured references. */
export function renameTableInFormula(text: string, oldName: string, newName: string): string {
  return rewriteStructured(text, (n) =>
    same(n.table, oldName) ? formatStructured({ ...n, table: newName }) : undefined,
  );
}

/**
 * Renames a column of `table` in structured references. Unqualified references (`[@Qty]`) are only
 * rewritten for formulas inside that table (`insideTable`).
 */
export function renameTableColumnInFormula(
  text: string,
  table: string,
  oldColumn: string,
  newColumn: string,
  insideTable: boolean,
): string {
  return rewriteStructured(text, (n) => {
    const mine = n.table === null ? insideTable : same(n.table, table);
    if (!mine || !n.columns) return undefined;
    const cols = n.columns.map((c) => (c.toUpperCase() === oldColumn.toUpperCase() ? newColumn : c)) as
      | [string]
      | [string, string];
    if (cols.every((c, i) => c === n.columns?.[i])) return undefined;
    return formatStructured({ ...n, columns: cols });
  });
}

/** The A1 area a structured reference denotes (null = #REF!). */
export function structuredArea(t: Table, n: StructuredNode, atRow: number | undefined): CellRange | null {
  let c0 = t.range.c0;
  let c1 = t.range.c1;
  if (n.columns) {
    const idx = n.columns.map((name) =>
      t.columns.findIndex((c) => c.name.toUpperCase() === name.toUpperCase()),
    );
    if (idx.some((i) => i < 0)) return null;
    const a = idx[0] as number;
    const b = (idx[1] ?? a) as number;
    c0 = t.range.c0 + Math.min(a, b);
    c1 = t.range.c0 + Math.max(a, b);
  }
  const dataR0 = t.range.r0 + (t.headerRow ? 1 : 0);
  const dataR1 = t.range.r1 - (t.totalsRow ? 1 : 0);
  if (n.thisRow) {
    if (atRow === undefined || atRow < dataR0 || atRow > dataR1) return null;
    return { r0: atRow, c0, r1: atRow, c1 };
  }
  let r0 = Number.POSITIVE_INFINITY;
  let r1 = Number.NEGATIVE_INFINITY;
  for (const s of n.specials.length === 0 ? ['#Data'] : n.specials) {
    let a: number;
    let b: number;
    if (s === '#All') [a, b] = [t.range.r0, t.range.r1];
    else if (s === '#Data') [a, b] = [dataR0, dataR1];
    else if (s === '#Headers') {
      if (!t.headerRow) return null;
      [a, b] = [t.range.r0, t.range.r0];
    } else {
      if (!t.totalsRow) return null;
      [a, b] = [t.range.r1, t.range.r1];
    }
    r0 = Math.min(r0, a);
    r1 = Math.max(r1, b);
  }
  return r1 < r0 ? null : { r0, c0, r1, c1 };
}

/**
 * Replaces structured references to `table` (on `tableSheet`) with absolute A1 references, as Excel
 * does when a table is converted to a normal range. `at` is the formula's own cell (for `[@Col]`).
 */
export function structuredToA1(
  text: string,
  table: Table,
  tableSheet: string,
  formulaSheet: string,
  at?: { row: number; col: number },
): string {
  const insideTable =
    at !== undefined &&
    formulaSheet.toUpperCase() === tableSheet.toUpperCase() &&
    at.row >= table.range.r0 &&
    at.row <= table.range.r1 &&
    at.col >= table.range.c0 &&
    at.col <= table.range.c1;
  return rewriteStructured(text, (n) => {
    const mine = n.table === null ? insideTable : same(n.table, table.name);
    if (!mine) return undefined;
    const area = structuredArea(table, n, at?.row);
    if (!area) return '#REF!';
    const single = area.r0 === area.r1 && area.c0 === area.c1;
    const thisRow = n.thisRow;
    const addr = formatAddress(
      single
        ? { kind: 'cell', row: area.r0, col: area.c0, rowAbs: !thisRow, colAbs: true }
        : {
            kind: 'area',
            r0: area.r0,
            c0: area.c0,
            r1: area.r1,
            c1: area.c1,
            r0Abs: !thisRow,
            c0Abs: true,
            r1Abs: !thisRow,
            c1Abs: true,
          },
    );
    const prefix =
      formulaSheet.toUpperCase() === tableSheet.toUpperCase() ? '' : formatPrefix({ sheet: tableSheet });
    return prefix + addr;
  });
}
