import { describe, expect, it } from 'vitest';
import { tokenize } from '../../src/parser/tokenizer.ts';

const kinds = (s: string) => tokenize(s).map((t) => t.kind);
const texts = (s: string) => tokenize(s).map((t) => t.text);

describe('tokenizer', () => {
  it('splits a simple expression with spans', () => {
    const t = tokenize('A1+2', {}, 1);
    expect(t.map((x) => [x.kind, x.text, x.start, x.end])).toEqual([
      ['ref', 'A1', 1, 3],
      ['operator', '+', 3, 4],
      ['number', '2', 4, 5],
    ]);
  });

  it('recognises absolute and mixed references', () => {
    const [t] = tokenize('$B3');
    expect(t?.address).toEqual({ kind: 'cell', row: 2, col: 1, rowAbs: false, colAbs: true });
    expect(tokenize('B$3')[0]?.address).toMatchObject({ rowAbs: true, colAbs: false });
    expect(tokenize('$B$3')[0]?.address).toMatchObject({ rowAbs: true, colAbs: true });
  });

  it('reads areas, whole columns and whole rows as single tokens', () => {
    expect(tokenize('A1:B2')[0]?.address).toMatchObject({ kind: 'area', r0: 0, c0: 0, r1: 1, c1: 1 });
    expect(tokenize('$A:C')[0]?.address).toEqual({ kind: 'cols', c0: 0, c1: 2, c0Abs: true, c1Abs: false });
    expect(tokenize('2:$5')[0]?.address).toEqual({ kind: 'rows', r0: 1, r1: 4, r0Abs: false, r1Abs: true });
  });

  it('normalises reversed areas', () => {
    expect(tokenize('B5:A1')[0]?.address).toMatchObject({ kind: 'area', r0: 0, c0: 0, r1: 4, c1: 1 });
  });

  it('handles sheet prefixes, quoted names and 3-D ranges', () => {
    expect(tokenize('Sheet2!C3')[0]?.prefix).toEqual({ sheet: 'Sheet2' });
    expect(tokenize("'My Sheet'!A1")[0]?.prefix).toEqual({ sheet: 'My Sheet' });
    expect(tokenize("'It''s'!A1")[0]?.prefix).toEqual({ sheet: "It's" });
    expect(tokenize('Jan:Dec!B2')[0]?.prefix).toEqual({ sheet: 'Jan', sheet2: 'Dec' });
    expect(tokenize("'Q 1:Q 4'!B2")[0]?.prefix).toEqual({ sheet: 'Q 1', sheet2: 'Q 4' });
    expect(tokenize('[1]Sheet1!A1')[0]?.prefix).toEqual({ sheet: 'Sheet1', workbook: '1' });
    expect(tokenize("'[Book 2.xlsx]Data'!A1")[0]?.prefix).toEqual({ sheet: 'Data', workbook: 'Book 2.xlsx' });
  });

  it('treats an identifier followed by ( as a function even if it looks like a cell', () => {
    expect(kinds('LOG10(100)')).toEqual(['function', 'lparen', 'number', 'rparen']);
    expect(tokenize('log10(1)')[0]?.name).toBe('LOG10');
  });

  it('keeps dotted function names and _xlfn prefixes', () => {
    expect(tokenize('_xlfn.XLOOKUP(1)')[0]?.name).toBe('_XLFN.XLOOKUP');
    expect(tokenize('STDEV.S(1)')[0]?.name).toBe('STDEV.S');
  });

  it('distinguishes names, booleans and cell references', () => {
    expect(tokenize('Sales')[0]?.kind).toBe('name');
    expect(tokenize('true')[0]).toMatchObject({ kind: 'bool', literal: true });
    expect(tokenize('A1B')[0]?.kind).toBe('name');
    expect(tokenize('XFD1')[0]?.kind).toBe('ref');
    expect(tokenize('XFE1')[0]?.kind).toBe('name');
    expect(tokenize('A1048577')[0]?.kind).toBe('name');
    expect(tokenize('R1C1')[0]?.kind).toBe('name');
  });

  it('reads sheet-scoped names and Sheet!#REF!', () => {
    expect(tokenize('Sheet1!Total')[0]).toMatchObject({
      kind: 'name',
      name: 'Total',
      prefix: { sheet: 'Sheet1' },
    });
    expect(tokenize('Sheet1!#REF!')[0]).toMatchObject({ kind: 'error', error: '#REF!' });
  });

  it('reads strings with doubled quotes', () => {
    expect(tokenize('"say ""hi"""')[0]).toMatchObject({ kind: 'string', literal: 'say "hi"' });
  });

  it('reports an unterminated string as invalid instead of throwing', () => {
    expect(tokenize('"abc')[0]?.kind).toBe('invalid');
  });

  it('reads every error literal, case-insensitively', () => {
    for (const e of [
      '#NULL!',
      '#DIV/0!',
      '#VALUE!',
      '#REF!',
      '#NAME?',
      '#NUM!',
      '#N/A',
      '#SPILL!',
      '#CALC!',
    ]) {
      expect(tokenize(e)[0]).toMatchObject({ kind: 'error', error: e });
    }
    expect(tokenize('#n/a')[0]?.error).toBe('#N/A');
  });

  it('reads numbers in all their forms', () => {
    expect(tokenize('1.5e3')[0]?.literal).toBe(1500);
    expect(tokenize('.5')[0]?.literal).toBe(0.5);
    expect(tokenize('2.')[0]?.literal).toBe(2);
    expect(tokenize('1E-2')[0]?.literal).toBe(0.01);
  });

  it('tells the spill operator from an error literal', () => {
    expect(kinds('A1#')).toEqual(['ref', 'operator']);
    expect(kinds('A1 #N/A')).toEqual(['ref', 'whitespace', 'error']);
  });

  it('reads two-character comparison operators', () => {
    expect(texts('1<>2<=3>=4')).toEqual(['1', '<>', '2', '<=', '3', '>=', '4']);
  });

  it('reads structured references', () => {
    expect(tokenize('Sales[Amount]')[0]?.structured).toEqual({
      table: 'Sales',
      specials: [],
      thisRow: false,
      columns: ['Amount'],
    });
    expect(tokenize('[@Qty]')[0]?.structured).toEqual({
      table: null,
      specials: [],
      thisRow: true,
      columns: ['Qty'],
    });
    expect(tokenize('T[[#Headers],[#Data],[A]:[C]]')[0]?.structured).toEqual({
      table: 'T',
      specials: ['#Headers', '#Data'],
      thisRow: false,
      columns: ['A', 'C'],
    });
    expect(tokenize('T[[#This Row],[Unit Price]]')[0]?.structured).toMatchObject({
      thisRow: true,
      columns: ['Unit Price'],
    });
    expect(tokenize("T[[Col'#1]]")[0]?.structured?.columns).toEqual(['Col#1']);
    expect(tokenize('T[#All]')[0]?.structured?.specials).toEqual(['#All']);
    expect(tokenize('T[@]')[0]?.structured).toMatchObject({ thisRow: true, columns: null });
  });

  it('flags malformed structured references', () => {
    expect(tokenize('T[[#Bogus]]')[0]?.kind).toBe('invalid');
    expect(tokenize('T[[A]')[0]?.kind).toBe('invalid');
  });

  it('does not treat 1:2 inside an array constant as a row range', () => {
    expect(kinds('{1,2}')).toEqual(['lbrace', 'number', 'comma', 'number', 'rbrace']);
  });

  it('tokenizes R1C1 references relative to an anchor', () => {
    const t = tokenize('R[-1]C+RC[2]+R2C3', { style: 'R1C1', anchor: { row: 4, col: 4 } });
    expect(t[0]?.address).toEqual({ kind: 'cell', row: 3, col: 4, rowAbs: false, colAbs: false });
    expect(t[2]?.address).toEqual({ kind: 'cell', row: 4, col: 6, rowAbs: false, colAbs: false });
    expect(t[4]?.address).toEqual({ kind: 'cell', row: 1, col: 2, rowAbs: true, colAbs: true });
  });

  it('tokenizes R1C1 whole rows and columns and keeps names like RATE intact', () => {
    const o = { style: 'R1C1' as const, anchor: { row: 0, col: 0 } };
    expect(tokenize('R2:R3', o)[0]?.address).toMatchObject({ kind: 'rows', r0: 1, r1: 2 });
    expect(tokenize('C[1]', o)[0]?.address).toMatchObject({ kind: 'cols', c0: 1, c1: 1 });
    expect(tokenize('RATE(1)', o)[0]?.kind).toBe('function');
    expect(tokenize('Rate', o)[0]?.kind).toBe('name');
  });

  it('turns an out-of-sheet relative R1C1 reference into #REF!', () => {
    expect(tokenize('R[-1]C', { style: 'R1C1', anchor: { row: 0, col: 0 } })[0]).toMatchObject({
      kind: 'error',
      error: '#REF!',
    });
  });

  it('marks unknown characters invalid', () => {
    expect(tokenize('1 ~ 2').some((t) => t.kind === 'invalid')).toBe(true);
  });
});
