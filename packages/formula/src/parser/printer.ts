/**
 * Prints an AST back to formula text (canonical spacing: none, except the intersection operator).
 * `parse(print(ast))` reproduces the same tree.
 */
import type { Node, RefAddress, StructuredNode } from './ast.ts';
import { formatAddress, formatPrefix } from './references.ts';

export interface PrintOptions {
  /** Reference style; R1C1 needs `anchor`. */
  style?: 'A1' | 'R1C1';
  anchor?: { row: number; col: number };
  /** Prefix the output with `=`. */
  leadingEquals?: boolean;
}

/** Formats a number literal so that it re-parses to the same double (`1E+21`, `0.1`). */
export function formatNumberLiteral(n: number): string {
  const s = String(n);
  if (!s.includes('e')) return s;
  return s.replace('e', 'E');
}

function r1c1Part(letter: 'R' | 'C', index: number, abs: boolean, base: number): string {
  if (abs) return `${letter}${index + 1}`;
  const d = index - base;
  return d === 0 ? letter : `${letter}[${d}]`;
}

/** Formats a reference address in R1C1 notation relative to `anchor`. */
export function formatAddressR1C1(a: RefAddress, anchor: { row: number; col: number }): string {
  switch (a.kind) {
    case 'cell':
      return r1c1Part('R', a.row, a.rowAbs, anchor.row) + r1c1Part('C', a.col, a.colAbs, anchor.col);
    case 'area': {
      const first = r1c1Part('R', a.r0, a.r0Abs, anchor.row) + r1c1Part('C', a.c0, a.c0Abs, anchor.col);
      const second = r1c1Part('R', a.r1, a.r1Abs, anchor.row) + r1c1Part('C', a.c1, a.c1Abs, anchor.col);
      return `${first}:${second}`;
    }
    case 'rows': {
      const p0 = r1c1Part('R', a.r0, a.r0Abs, anchor.row);
      const p1 = r1c1Part('R', a.r1, a.r1Abs, anchor.row);
      return p0 === p1 ? p0 : `${p0}:${p1}`;
    }
    case 'cols': {
      const p0 = r1c1Part('C', a.c0, a.c0Abs, anchor.col);
      const p1 = r1c1Part('C', a.c1, a.c1Abs, anchor.col);
      return p0 === p1 ? p0 : `${p0}:${p1}`;
    }
  }
}

const SIMPLE_COLUMN =
  /^[^\t\n\r,:.[\]#'"{}$^&*+=\-<>/@ ]([^\t\n\r,:.[\]#'"{}$^&*+=\-<>/@]*[^\t\n\r,:.[\]#'"{}$^&*+=\-<>/@ ])?$/;

function escapeColumn(name: string): string {
  return name.replace(/['[\]#]/g, "'$&");
}

/** Prints a structured reference in Excel's canonical form. */
export function formatStructured(
  n: Pick<StructuredNode, 'table' | 'specials' | 'thisRow' | 'columns'>,
): string {
  const table = n.table ?? '';
  const cols = n.columns;
  const colItems = cols ? cols.map((c) => `[${escapeColumn(c)}]`).join(':') : '';
  if (n.thisRow) {
    if (!cols) return `${table}[@]`;
    if (cols.length === 1 && SIMPLE_COLUMN.test(cols[0])) return `${table}[@${cols[0]}]`;
    return `${table}[@${colItems}]`;
  }
  const items: string[] = n.specials.map((s) => `[${s}]`);
  if (items.length === 0) {
    if (!cols) return `${table}[]`;
    if (cols.length === 1 && SIMPLE_COLUMN.test(cols[0])) return `${table}[${cols[0]}]`;
    return `${table}[${colItems}]`;
  }
  if (!cols && items.length === 1) return `${table}[${n.specials[0]}]`;
  if (cols) items.push(colItems);
  return `${table}[${items.join(',')}]`;
}

function printString(s: string): string {
  return `"${s.replace(/"/g, '""')}"`;
}

/** Prints a node. */
export function print(node: Node, opts: PrintOptions = {}): string {
  const body = printNode(node, opts);
  return opts.leadingEquals ? `=${body}` : body;
}

function printNode(node: Node, opts: PrintOptions): string {
  switch (node.type) {
    case 'number':
      return formatNumberLiteral(node.value);
    case 'string':
      return printString(node.value);
    case 'boolean':
      return node.value ? 'TRUE' : 'FALSE';
    case 'error':
      return (node.prefix ? formatPrefix(node.prefix) : '') + node.code;
    case 'ref': {
      const addr =
        opts.style === 'R1C1'
          ? formatAddressR1C1(node.address, opts.anchor ?? { row: 0, col: 0 })
          : formatAddress(node.address);
      return (node.prefix ? formatPrefix(node.prefix) : '') + addr;
    }
    case 'name':
      return (node.prefix ? formatPrefix(node.prefix) : '') + node.name;
    case 'structured':
      return formatStructured(node);
    case 'array':
      return `{${node.rows.map((r) => r.map((e) => printNode(e, opts)).join(',')).join(';')}}`;
    case 'unary':
      return node.op + printNode(node.operand, opts);
    case 'postfix':
      return printNode(node.operand, opts) + node.op;
    case 'binary':
      return printNode(node.left, opts) + node.op + printNode(node.right, opts);
    case 'call':
      return `${node.name}(${node.args.map((a) => printNode(a, opts)).join(',')})`;
    case 'invoke':
      return `${printNode(node.callee, opts)}(${node.args.map((a) => printNode(a, opts)).join(',')})`;
    case 'paren':
      return `(${printNode(node.expr, opts)})`;
    case 'empty':
      return '';
  }
}
