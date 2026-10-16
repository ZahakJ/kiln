/**
 * Pratt parser for Excel formulas.
 *
 * Precedence (loosest → tightest): comparison (= <> < > <= >=), &, + -, * /, ^ (left-associative, as
 * in Excel), postfix %, prefix - + (so `-2^2` is 4 and `-5%` is -0.05), union `,` (only inside
 * parentheses), intersection (whitespace), range `:`, spill `#`. Implicit intersection `@` applies to
 * a whole range expression (`@A1:A10`).
 */
import type { ErrorCode } from '../values/errors.ts';
import type { ArrayElement, BinaryOp, Node, Span } from './ast.ts';
import { type Token, type TokenizeOptions, tokenize } from './tokenizer.ts';

export class FormulaParseError extends Error {
  /** Where the problem is (offsets into the formula text). */
  readonly span: Span;
  constructor(message: string, at: Span) {
    super(message);
    this.name = 'FormulaParseError';
    this.span = { start: at.start, end: at.end };
  }
}

export type ParseOptions = TokenizeOptions;

const BINARY_BP: Record<string, number> = {
  '=': 10,
  '<>': 10,
  '<': 10,
  '>': 10,
  '<=': 10,
  '>=': 10,
  '&': 20,
  '+': 30,
  '-': 30,
  '*': 40,
  '/': 40,
  '^': 50,
  ':': 100,
};
const PERCENT_BP = 60;
const PREFIX_BP = 70;
const UNION_BP = 80;
const INTERSECT_BP = 90;
const IMPLICIT_BP = 95;
const SPILL_BP = 110;
const CALL_BP = 120;

const OPERAND_START = new Set(['ref', 'name', 'function', 'structured', 'lparen']);

class Parser {
  private pos = 0;
  private readonly toks: Token[];

  constructor(
    tokens: Token[],
    private readonly endPos: number,
  ) {
    this.toks = tokens;
  }

  /** Index of the next non-whitespace token. */
  private nextIndex(): number {
    let i = this.pos;
    while (i < this.toks.length && (this.toks[i] as Token).kind === 'whitespace') i++;
    return i;
  }

  private peek(): Token | undefined {
    return this.toks[this.nextIndex()];
  }

  /** True when whitespace separates the previous token from the next significant one. */
  private wsBefore(): boolean {
    return this.toks[this.pos]?.kind === 'whitespace';
  }

  private take(): Token {
    const i = this.nextIndex();
    const t = this.toks[i];
    if (!t)
      throw new FormulaParseError('Unexpected end of formula', { start: this.endPos, end: this.endPos });
    this.pos = i + 1;
    return t;
  }

  private expect(kind: Token['kind'], what: string): Token {
    const t = this.peek();
    if (!t || t.kind !== kind) {
      const at = t ? { start: t.start, end: t.end } : { start: this.endPos, end: this.endPos };
      throw new FormulaParseError(`Expected ${what}`, at);
    }
    return this.take();
  }

  parseFormula(): Node {
    if (!this.peek())
      throw new FormulaParseError('The formula is empty', { start: this.endPos, end: this.endPos });
    const node = this.expression(0, false);
    const t = this.peek();
    if (t) throw this.unexpected(t);
    return node;
  }

  private unexpected(t: Token): FormulaParseError {
    if (t.kind === 'invalid') return new FormulaParseError(t.message ?? 'Invalid formula', t);
    if (t.kind === 'rparen') return new FormulaParseError('Unmatched closing parenthesis', t);
    return new FormulaParseError(`Unexpected "${t.text}"`, t);
  }

  private expression(minBp: number, union: boolean): Node {
    let left = this.prefix(union);
    for (;;) {
      const ws = this.wsBefore();
      const t = this.peek();
      if (!t) break;
      if (t.kind === 'operator') {
        const op = t.text;
        if (op === '%') {
          if (PERCENT_BP < minBp) break;
          this.take();
          left = { type: 'postfix', op: '%', operand: left, span: { start: left.span.start, end: t.end } };
          continue;
        }
        if (op === '#') {
          if (SPILL_BP < minBp || ws) break;
          this.take();
          left = { type: 'postfix', op: '#', operand: left, span: { start: left.span.start, end: t.end } };
          continue;
        }
        const bp = BINARY_BP[op];
        if (bp === undefined) {
          if (op === '@' && ws && INTERSECT_BP >= minBp) {
            left = this.intersect(left);
            continue;
          }
          throw this.unexpected(t);
        }
        if (bp < minBp) break;
        this.take();
        const right = this.expression(bp + 1, union);
        left = {
          type: 'binary',
          op: op as BinaryOp,
          left,
          right,
          span: { start: left.span.start, end: right.span.end },
        };
        continue;
      }
      if (t.kind === 'comma' && union) {
        if (UNION_BP < minBp) break;
        this.take();
        const right = this.expression(UNION_BP + 1, union);
        left = {
          type: 'binary',
          op: ',',
          left,
          right,
          span: { start: left.span.start, end: right.span.end },
        };
        continue;
      }
      if (t.kind === 'lparen' && !ws) {
        if (CALL_BP < minBp) break;
        const { args, end } = this.argumentList();
        left = { type: 'invoke', callee: left, args, span: { start: left.span.start, end } };
        continue;
      }
      if (ws && OPERAND_START.has(t.kind)) {
        if (INTERSECT_BP < minBp) break;
        left = this.intersect(left);
        continue;
      }
      break;
    }
    return left;
  }

  private intersect(left: Node): Node {
    const right = this.expression(INTERSECT_BP + 1, false);
    return { type: 'binary', op: ' ', left, right, span: { start: left.span.start, end: right.span.end } };
  }

  private prefix(union: boolean): Node {
    const t = this.take();
    const span = { start: t.start, end: t.end };
    switch (t.kind) {
      case 'number': {
        const value = t.literal as number;
        if (!Number.isFinite(value)) return { type: 'error', code: '#NUM!', span };
        return { type: 'number', value, span };
      }
      case 'string':
        return { type: 'string', value: t.literal as string, span };
      case 'bool':
        return { type: 'boolean', value: t.literal as boolean, span };
      case 'error':
        return t.prefix
          ? { type: 'error', code: t.error as ErrorCode, prefix: t.prefix, span }
          : { type: 'error', code: t.error as ErrorCode, span };
      case 'ref':
        return t.prefix
          ? { type: 'ref', prefix: t.prefix, address: t.address as NonNullable<Token['address']>, span }
          : { type: 'ref', address: t.address as NonNullable<Token['address']>, span };
      case 'name':
        return t.prefix
          ? { type: 'name', name: t.name as string, prefix: t.prefix, span }
          : { type: 'name', name: t.name as string, span };
      case 'structured': {
        const s = t.structured as NonNullable<Token['structured']>;
        return {
          type: 'structured',
          table: s.table,
          specials: s.specials,
          thisRow: s.thisRow,
          columns: s.columns,
          span,
        };
      }
      case 'function': {
        const { args, end } = this.argumentList();
        return { type: 'call', name: t.name as string, args, span: { start: t.start, end } };
      }
      case 'lparen': {
        const expr = this.expression(0, true);
        const close = this.expect('rparen', '")"');
        return { type: 'paren', expr, span: { start: t.start, end: close.end } };
      }
      case 'lbrace':
        return this.arrayConstant(t);
      case 'operator': {
        if (t.text === '-' || t.text === '+') {
          const operand = this.expression(PREFIX_BP, union);
          return { type: 'unary', op: t.text, operand, span: { start: t.start, end: operand.span.end } };
        }
        if (t.text === '@') {
          const operand = this.expression(IMPLICIT_BP, union);
          return { type: 'unary', op: '@', operand, span: { start: t.start, end: operand.span.end } };
        }
        throw this.unexpected(t);
      }
      default:
        throw this.unexpected(t);
    }
  }

  private argumentList(): { args: Node[]; end: number } {
    const open = this.expect('lparen', '"("');
    const args: Node[] = [];
    let t = this.peek();
    if (t?.kind === 'rparen') {
      this.take();
      return { args, end: t.end };
    }
    let lastSepEnd = open.end;
    for (;;) {
      t = this.peek();
      if (!t)
        throw new FormulaParseError('Missing closing parenthesis', { start: open.start, end: this.endPos });
      if (t.kind === 'comma' || t.kind === 'rparen') {
        args.push({ type: 'empty', span: { start: lastSepEnd, end: lastSepEnd } });
      } else {
        args.push(this.expression(0, false));
      }
      t = this.peek();
      if (!t)
        throw new FormulaParseError('Missing closing parenthesis', { start: open.start, end: this.endPos });
      if (t.kind === 'comma') {
        this.take();
        lastSepEnd = t.end;
        continue;
      }
      if (t.kind === 'rparen') {
        this.take();
        return { args, end: t.end };
      }
      throw this.unexpected(t);
    }
  }

  private arrayConstant(open: Token): Node {
    const rows: ArrayElement[][] = [[]];
    for (;;) {
      const t = this.take();
      let el: ArrayElement;
      if (t.kind === 'operator' && (t.text === '-' || t.text === '+')) {
        const n = this.take();
        if (n.kind !== 'number') throw new FormulaParseError('Expected a number in the array constant', n);
        const v = n.literal as number;
        el = { type: 'number', value: t.text === '-' ? -v : v, span: { start: t.start, end: n.end } };
      } else if (t.kind === 'number') {
        el = { type: 'number', value: t.literal as number, span: { start: t.start, end: t.end } };
      } else if (t.kind === 'string') {
        el = { type: 'string', value: t.literal as string, span: { start: t.start, end: t.end } };
      } else if (t.kind === 'bool') {
        el = { type: 'boolean', value: t.literal as boolean, span: { start: t.start, end: t.end } };
      } else if (t.kind === 'error' && !t.prefix) {
        el = { type: 'error', code: t.error as ErrorCode, span: { start: t.start, end: t.end } };
      } else {
        throw new FormulaParseError(
          'Array constants may only contain numbers, text, logical values and errors',
          t,
        );
      }
      (rows[rows.length - 1] as ArrayElement[]).push(el);
      const sep = this.take();
      if (sep.kind === 'comma') continue;
      if (sep.kind === 'semicolon') {
        rows.push([]);
        continue;
      }
      if (sep.kind === 'rbrace') {
        const width = (rows[0] as ArrayElement[]).length;
        if (rows.some((r) => r.length !== width))
          throw new FormulaParseError('Every row of an array constant must have the same length', {
            start: open.start,
            end: sep.end,
          });
        return { type: 'array', rows, span: { start: open.start, end: sep.end } };
      }
      throw new FormulaParseError('Expected ",", ";" or "}" in the array constant', sep);
    }
  }
}

/**
 * Parses a formula. A leading `=` is optional. Spans are offsets into `text` as given.
 * Throws {@link FormulaParseError} with the offending span.
 */
export function parse(text: string, opts: ParseOptions = {}): Node {
  let body = text;
  let offset = 0;
  const lead = /^\s*=/.exec(text);
  if (lead) {
    offset = lead[0].length;
    body = text.slice(offset);
  }
  const tokens = tokenize(body, opts, offset);
  return new Parser(tokens, text.length).parseFormula();
}

/** Like {@link parse} but returns the error instead of throwing. */
export function tryParse(
  text: string,
  opts: ParseOptions = {},
): { ast: Node } | { error: FormulaParseError } {
  try {
    return { ast: parse(text, opts) };
  } catch (e) {
    if (e instanceof FormulaParseError) return { error: e };
    throw e;
  }
}
