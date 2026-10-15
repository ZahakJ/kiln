/**
 * Builds the Formula Lab's step tree from an evaluation trace.
 */
import { childrenOf, type Node, type Span } from '../parser/ast.ts';
import { formatStructured } from '../parser/printer.ts';
import { formatAddress, formatPrefix } from '../parser/references.ts';
import { toText } from '../values/coerce.ts';
import { ErrorValue } from '../values/errors.ts';
import {
  type Area,
  Array2D,
  BLANK,
  type EvalValue,
  Lambda,
  MISSING,
  RefValue,
  type Scalar,
  type Value,
} from '../values/types.ts';
import type { Evaluator, TraceEntry } from './evaluator.ts';
import { getFunction } from './functions/registry.ts';

export interface ExplainStep {
  /** Node type (`call`, `binary`, `ref`, …). */
  kind: Node['type'];
  /** Short label: the function name, operator, reference or literal. */
  label: string;
  /** Source text of this sub-expression. */
  text: string;
  span: Span;
  /** False for sub-expressions that were never evaluated (the untaken branch of IF…). */
  evaluated: boolean;
  /** How many times it was evaluated (more than 1 inside LAMBDA/MAP…). */
  count: number;
  /** Value (references are dereferenced). */
  value?: Value;
  /** Address text when the step evaluated to a reference. */
  reference?: string;
  /** Display text of the value, e.g. `42`, `"abc"`, `{1,2;3,4}`, `#N/A`. */
  display: string;
  /** Function summary from the registry docs, for `call` steps. */
  description?: string;
  children: ExplainStep[];
}

const MAX_ARRAY_PREVIEW = 6;

export function formatScalar(v: Scalar): string {
  if (v instanceof ErrorValue) return v.code;
  if (v === BLANK) return '(blank)';
  if (typeof v === 'string') return `"${v.replace(/"/g, '""')}"`;
  return toText(v) as string;
}

/** Compact display of any value: arrays are previewed as `{1,2,…;3,4,…}`. */
export function formatValue(v: Value | undefined): string {
  if (v === undefined) return '';
  if (v instanceof Array2D) {
    const rows = v.data.slice(0, MAX_ARRAY_PREVIEW).map((r) => {
      const cells = r.slice(0, MAX_ARRAY_PREVIEW).map(formatScalar);
      if (r.length > MAX_ARRAY_PREVIEW) cells.push('…');
      return cells.join(',');
    });
    if (v.rows > MAX_ARRAY_PREVIEW) rows.push('…');
    return `{${rows.join(';')}}`;
  }
  if (v instanceof Lambda) return v.toString();
  return formatScalar(v);
}

function label(n: Node, source: string): string {
  switch (n.type) {
    case 'call':
      return n.name;
    case 'binary':
      return n.op === ' ' ? '∩' : n.op;
    case 'unary':
    case 'postfix':
      return n.op;
    case 'paren':
      return '( )';
    case 'empty':
      return '(omitted)';
    case 'invoke':
      return 'call';
    case 'structured':
      return formatStructured(n);
    default:
      return source.slice(n.span.start, n.span.end);
  }
}

export function buildExplainTree(
  source: string,
  ast: Node,
  trace: Map<Node, TraceEntry>,
  result: Value,
  ev: Evaluator,
  sheetName: (id: number) => string | undefined,
): ExplainStep {
  const describeRef = (r: RefValue): string =>
    r.areas
      .map((a: Area) => {
        const addr =
          a.r0 === a.r1 && a.c0 === a.c1
            ? formatAddress({ kind: 'cell', row: a.r0, col: a.c0, rowAbs: false, colAbs: false })
            : formatAddress({
                kind: 'area',
                r0: a.r0,
                c0: a.c0,
                r1: a.r1,
                c1: a.c1,
                r0Abs: false,
                c0Abs: false,
                r1Abs: false,
                c1Abs: false,
              });
        const name = sheetName(a.sheet);
        return a.sheet === ev.sheet || !name ? addr : formatPrefix({ sheet: name }) + addr;
      })
      .join(',');

  const build = (n: Node, isRoot: boolean): ExplainStep => {
    const t = trace.get(n);
    let value: Value | undefined;
    let reference: string | undefined;
    if (isRoot) value = result;
    else if (t) {
      const raw: EvalValue = t.value;
      if (raw instanceof RefValue) {
        reference = describeRef(raw);
        value = raw.areas.length === 1 ? ev.toValue(raw) : undefined;
      } else value = raw === MISSING ? BLANK : (raw as Value);
    }
    const step: ExplainStep = {
      kind: n.type,
      label: label(n, source),
      text: source.slice(n.span.start, n.span.end),
      span: n.span,
      evaluated: isRoot || t !== undefined,
      count: t?.count ?? (isRoot ? 1 : 0),
      display: reference ? `${reference} → ${formatValue(value)}` : formatValue(value),
      children: childrenOf(n).map((c) => build(c, false)),
    };
    if (value !== undefined) step.value = value;
    if (reference !== undefined) step.reference = reference;
    if (n.type === 'call') {
      const d = getFunction(n.name)?.docs?.summary;
      if (d) step.description = d;
    }
    return step;
  };
  return build(ast, true);
}
