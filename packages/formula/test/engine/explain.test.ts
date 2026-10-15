import { describe, expect, it } from 'vitest';
import { formatValue } from '../../src/engine/explain.ts';
import { Array2D, BLANK } from '../../src/values/types.ts';
import { sheet } from './_support.ts';

describe('explain (Formula Lab step tree)', () => {
  it('returns every sub-expression with its value and source span', () => {
    const e = sheet({ A1: 3 });
    e.recalc();
    const src = '=IF(A1>1, A1*2, 1/0)';
    const t = e.explain(src);
    expect(t.kind).toBe('call');
    expect(t.label).toBe('IF');
    expect(t.display).toBe('6');
    expect(t.description).toMatch(/condition/);
    const [cond, yes, no] = t.children;
    expect(cond?.text).toBe('A1>1');
    expect(cond?.display).toBe('TRUE');
    expect(cond?.children[0]?.reference).toBe('A1');
    expect(cond?.children[0]?.display).toBe('A1 → 3');
    expect(yes?.display).toBe('6');
    expect(src.slice(yes?.span.start, yes?.span.end)).toBe('A1*2');
    expect(no?.evaluated).toBe(false);
    expect(no?.children.every((c) => !c.evaluated)).toBe(true);
  });

  it('counts repeated evaluation inside lambdas', () => {
    const e = sheet();
    const t = e.explain('=MAP({1,2,3},LAMBDA(x,x*10))');
    const lambda = t.children[1];
    const body = lambda?.children[1];
    expect(body?.count).toBe(3);
    expect(t.display).toBe('{10,20,30}');
  });

  it('previews ranges and arrays compactly', () => {
    const e = sheet({ A1: 1, A2: 'x' });
    e.recalc();
    const t = e.explain('=SUM(A1:A3)');
    expect(t.children[0]?.display).toBe('A1:A3 → {1;"x";(blank)}');
    expect(formatValue(Array2D.build(10, 10, (r, c) => r * 10 + c))).toMatch(/^\{0,1,2,3,4,5,…;/);
    expect(formatValue(BLANK)).toBe('(blank)');
  });

  it('labels operators, errors and omitted arguments', () => {
    const e = sheet();
    const t = e.explain('=IFERROR(1/0,)');
    expect(t.display).toBe('0');
    expect(t.children[0]?.label).toBe('/');
    expect(t.children[0]?.display).toBe('#DIV/0!');
    expect(t.children[1]?.label).toBe('(omitted)');
  });

  it('shows sheet-qualified references for other sheets', () => {
    const e = sheet();
    e.addSheet('Data 2');
    e.setCell('Data 2', 0, 0, { value: 5 });
    const t = e.explain("='Data 2'!A1+1");
    expect(t.children[0]?.reference).toBe("'Data 2'!A1");
    expect(t.display).toBe('6');
  });
});
