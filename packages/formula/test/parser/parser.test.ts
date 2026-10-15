import { describe, expect, it } from 'vitest';
import { type Node, stripSpans } from '../../src/parser/ast.ts';
import { FormulaParseError, parse, tryParse } from '../../src/parser/parser.ts';
import { print } from '../../src/parser/printer.ts';

/** Fully parenthesised rendering to check precedence/associativity. */
function shape(n: Node): string {
  switch (n.type) {
    case 'binary':
      return `(${shape(n.left)}${n.op === ' ' ? '∩' : n.op}${shape(n.right)})`;
    case 'unary':
      return `(${n.op}${shape(n.operand)})`;
    case 'postfix':
      return `(${shape(n.operand)}${n.op})`;
    case 'paren':
      return shape(n.expr);
    case 'call':
      return `${n.name}[${n.args.map(shape).join(';')}]`;
    case 'invoke':
      return `${shape(n.callee)}<${n.args.map(shape).join(';')}>`;
    case 'empty':
      return '∅';
    default:
      return print(n);
  }
}

const p = (s: string) => shape(parse(s));

describe('parser: precedence and associativity', () => {
  it.each([
    ['=1+2*3', '(1+(2*3))'],
    ['=(1+2)*3', '((1+2)*3)'],
    ['=-2^2', '((-2)^2)'],
    ['=2^-2', '(2^(-2))'],
    ['=2^3^2', '((2^3)^2)'],
    ['=-5%', '((-5)%)'],
    ['=5%^2', '((5%)^2)'],
    ['=1-2-3', '((1-2)-3)'],
    ['=8/4/2', '((8/4)/2)'],
    ['="a"&1+2', '("a"&(1+2))'],
    ['=1+2&"x"', '((1+2)&"x")'],
    ['=1&2=12', '((1&2)=12)'],
    ['=1<2=TRUE', '((1<2)=TRUE)'],
    ['=A1:B2 B1:C3', '(A1:B2∩B1:C3)'],
    ['=SUM(A1:B2 B2:C3)', 'SUM[(A1:B2∩B2:C3)]'],
    ['=(A1,B2)', '(A1,B2)'],
    ['=SUM((A1,B2),C3)', 'SUM[(A1,B2);C3]'],
    ['=A1:A3:C2', '(A1:A3:C2)'],
    ['=A1:INDEX(B:B,3)', '(A1:INDEX[B:B;3])'],
    ['=-A1:B2', '(-A1:B2)'],
    ['=@A1:A10', '(@A1:A10)'],
    ['=A1#', '(A1#)'],
    ['=SUM(A1#)', 'SUM[(A1#)]'],
    ['=+A1', '(+A1)'],
    ['=--A1', '(-(-A1))'],
    ['=1--1', '(1-(-1))'],
    ['=A1 :B2', '(A1:B2)'],
    ['= 1 + 2 ', '(1+2)'],
  ])('%s → %s', (src, expected) => {
    expect(p(src)).toBe(expected);
  });
});

describe('parser: calls and arguments', () => {
  it('parses empty argument lists and empty arguments', () => {
    expect(p('=NOW()')).toBe('NOW[]');
    expect(p('=IF(A1,,2)')).toBe('IF[A1;∅;2]');
    expect(p('=F(,)')).toBe('F[∅;∅]');
    expect(p('=F(1,)')).toBe('F[1;∅]');
  });

  it('upper-cases function names', () => {
    expect(p('=sum(1)')).toBe('SUM[1]');
  });

  it('parses lambda invocation', () => {
    expect(p('=LAMBDA(x,x+1)(2)')).toBe('LAMBDA[x;(x+1)]<2>');
    expect(p('=(LAMBDA(x,x))(3)')).toBe('LAMBDA[x;x]<3>');
  });

  it('parses nested calls', () => {
    expect(p('=IF(AND(A1>0,B1<1),SUM(C:C),"")')).toBe('IF[AND[(A1>0);(B1<1)];SUM[C:C];""]');
  });
});

describe('parser: literals and references', () => {
  it('parses array constants with negative numbers and mixed types', () => {
    const ast = parse('={1,-2;"a",TRUE;#N/A,0.5}');
    expect(stripSpans(ast)).toEqual({
      type: 'array',
      rows: [
        [
          { type: 'number', value: 1 },
          { type: 'number', value: -2 },
        ],
        [
          { type: 'string', value: 'a' },
          { type: 'boolean', value: true },
        ],
        [
          { type: 'error', code: '#N/A' },
          { type: 'number', value: 0.5 },
        ],
      ],
    });
  });

  it('parses quoted sheet names and 3-D references', () => {
    expect(stripSpans(parse("='Q1 Data'!B2"))).toMatchObject({ type: 'ref', prefix: { sheet: 'Q1 Data' } });
    expect(stripSpans(parse('=SUM(Jan:Mar!B2)'))).toMatchObject({
      args: [{ type: 'ref', prefix: { sheet: 'Jan', sheet2: 'Mar' } }],
    });
  });

  it('turns an overflowing number literal into #NUM!', () => {
    expect(stripSpans(parse('=1e400'))).toEqual({ type: 'error', code: '#NUM!' });
  });

  it('accepts formulas with or without the leading =', () => {
    expect(stripSpans(parse('1+1'))).toEqual(stripSpans(parse('=1+1')));
  });
});

describe('parser: spans', () => {
  it('records spans in the original text (after the =)', () => {
    const ast = parse('=SUM(A1, 2)');
    expect(ast.span).toEqual({ start: 1, end: 11 });
    if (ast.type !== 'call') throw new Error('expected call');
    expect(ast.args[0]?.span).toEqual({ start: 5, end: 7 });
    expect(ast.args[1]?.span).toEqual({ start: 9, end: 10 });
  });

  it('spans binary expressions from left to right operand', () => {
    const src = '=A1 + B1*2';
    const ast = parse(src);
    expect(src.slice(ast.span.start, ast.span.end)).toBe('A1 + B1*2');
  });

  it('places empty arguments right after the separator', () => {
    const ast = parse('=IF(1,,2)');
    if (ast.type !== 'call') throw new Error('expected call');
    expect(ast.args[1]).toEqual({ type: 'empty', span: { start: 6, end: 6 } });
  });
});

describe('parser: errors', () => {
  it.each([
    ['=', 'empty'],
    ['=1+', 'end'],
    ['=SUM(1', 'parenthesis'],
    ['=1)', 'closing parenthesis'],
    ['=(1', '")"'],
    ['={1,2;3}', 'same length'],
    ['={A1}', 'Array constants'],
    ['="abc', 'quotation'],
    ['=1 2 +', ''],
    ['=*3', 'Unexpected'],
  ])('rejects %s', (src, msg) => {
    expect(() => parse(src)).toThrow(FormulaParseError);
    try {
      parse(src);
    } catch (e) {
      expect((e as Error).message).toContain(msg);
    }
  });

  it('reports the offending span', () => {
    const r = tryParse('=1+*2');
    expect('error' in r && r.error.span).toEqual({ start: 3, end: 4 });
  });

  it('tryParse returns the AST on success', () => {
    expect('ast' in tryParse('=1')).toBe(true);
  });
});
