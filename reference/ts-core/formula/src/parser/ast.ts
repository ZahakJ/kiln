/**
 * Formula AST. Every node carries a source span (offsets into the text given to the parser) so the
 * formula bar can highlight it and the Formula Lab can show each step against the source.
 */
import type { ErrorCode } from '../values/errors.ts';

export interface Span {
  start: number;
  end: number;
}

/** A sheet qualifier: `Sheet1!`, `'My Sheet'!`, `Sheet1:Sheet3!` (3-D) or `[1]Sheet1!` (external). */
export interface SheetPrefix {
  workbook?: string;
  sheet: string;
  /** Last sheet of a 3-D reference. */
  sheet2?: string;
}

/** Row and column indices are 0-based; `*Abs` flags record `$`. */
export type RefAddress =
  | { kind: 'cell'; row: number; col: number; rowAbs: boolean; colAbs: boolean }
  | {
      kind: 'area';
      r0: number;
      c0: number;
      r1: number;
      c1: number;
      r0Abs: boolean;
      c0Abs: boolean;
      r1Abs: boolean;
      c1Abs: boolean;
    }
  | { kind: 'rows'; r0: number; r1: number; r0Abs: boolean; r1Abs: boolean }
  | { kind: 'cols'; c0: number; c1: number; c0Abs: boolean; c1Abs: boolean };

export type StructuredSpecial = '#All' | '#Data' | '#Headers' | '#Totals';

export interface NumberNode {
  type: 'number';
  value: number;
  span: Span;
}
export interface StringNode {
  type: 'string';
  value: string;
  span: Span;
}
export interface BooleanNode {
  type: 'boolean';
  value: boolean;
  span: Span;
}
export interface ErrorNode {
  type: 'error';
  code: ErrorCode;
  /** Kept for `Sheet1!#REF!`. */
  prefix?: SheetPrefix;
  span: Span;
}
export interface RefNode {
  type: 'ref';
  prefix?: SheetPrefix;
  address: RefAddress;
  span: Span;
}
export interface NameNode {
  type: 'name';
  name: string;
  /** Sheet-scoped name: `Sheet1!Total`. */
  prefix?: SheetPrefix;
  span: Span;
}
export interface StructuredNode {
  type: 'structured';
  /** Null for an unqualified reference such as `[@Qty]` inside a table. */
  table: string | null;
  specials: StructuredSpecial[];
  /** `@` / `[#This Row]`. */
  thisRow: boolean;
  /** A single column or a column range (`[Jan]:[Mar]`); null for all columns. */
  columns: [string] | [string, string] | null;
  span: Span;
}
export type ArrayElement = NumberNode | StringNode | BooleanNode | ErrorNode;
export interface ArrayNode {
  type: 'array';
  rows: ArrayElement[][];
  span: Span;
}
export interface UnaryNode {
  type: 'unary';
  op: '-' | '+' | '@';
  operand: Node;
  span: Span;
}
export interface PostfixNode {
  type: 'postfix';
  op: '%' | '#';
  operand: Node;
  span: Span;
}
export type BinaryOp =
  | '+'
  | '-'
  | '*'
  | '/'
  | '^'
  | '&'
  | '='
  | '<>'
  | '<'
  | '>'
  | '<='
  | '>='
  | ':'
  | ' '
  | ',';
export interface BinaryNode {
  type: 'binary';
  op: BinaryOp;
  left: Node;
  right: Node;
  span: Span;
}
export interface CallNode {
  type: 'call';
  /** Upper-cased function name as written, including any `_xlfn.` prefix. */
  name: string;
  args: Node[];
  span: Span;
}
/** Calling the result of an expression: `LAMBDA(x,x+1)(2)`. */
export interface InvokeNode {
  type: 'invoke';
  callee: Node;
  args: Node[];
  span: Span;
}
export interface ParenNode {
  type: 'paren';
  expr: Node;
  span: Span;
}
/** An omitted function argument. */
export interface EmptyNode {
  type: 'empty';
  span: Span;
}

export type Node =
  | NumberNode
  | StringNode
  | BooleanNode
  | ErrorNode
  | RefNode
  | NameNode
  | StructuredNode
  | ArrayNode
  | UnaryNode
  | PostfixNode
  | BinaryNode
  | CallNode
  | InvokeNode
  | ParenNode
  | EmptyNode;

/** Direct children of a node, in source order. */
export function childrenOf(node: Node): Node[] {
  switch (node.type) {
    case 'unary':
    case 'postfix':
      return [node.operand];
    case 'binary':
      return [node.left, node.right];
    case 'call':
      return node.args;
    case 'invoke':
      return [node.callee, ...node.args];
    case 'paren':
      return [node.expr];
    case 'array':
      return node.rows.flat();
    default:
      return [];
  }
}

/** Depth-first pre-order walk. Return false from the visitor to skip a subtree. */
export function walk(node: Node, visit: (n: Node) => boolean | undefined): void {
  const stack: Node[] = [node];
  while (stack.length > 0) {
    const n = stack.pop() as Node;
    if (visit(n) === false) continue;
    const kids = childrenOf(n);
    for (let i = kids.length - 1; i >= 0; i--) stack.push(kids[i] as Node);
  }
}

/** A deep copy of a node with every span removed (useful for structural comparison). */
export function stripSpans(node: Node): unknown {
  return JSON.parse(JSON.stringify(node, (k, v) => (k === 'span' ? undefined : v)));
}
