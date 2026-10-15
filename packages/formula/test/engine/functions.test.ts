import { describe, expect, it } from 'vitest';
import { CORE_FUNCTIONS } from '../../src/engine/functions/core.ts';
import {
  getFunction,
  listFunctions,
  registerFunction,
  unregisterFunction,
} from '../../src/engine/functions/registry.ts';
import { calc, get, sheet } from './_support.ts';

const DATA = {
  A1: 10,
  A2: 20,
  A3: 'text',
  A4: true,
  A5: 30,
  B1: 'apple',
  B2: 'Banana',
  B3: 'cherry',
  C1: 1,
  C2: 2,
  C3: 3,
  D1: 'a',
  D2: 'b',
  D3: 'c',
};

describe('aggregates', () => {
  it.each([
    ['=SUM(A1:A5)', 60],
    ['=SUM(A1:A5,5)', 65],
    ['=SUM(1,"2",TRUE)', 4],
    ['=SUM({1,"2",TRUE})', 1],
    ['=SUM(A1:A5,A1)', 70],
    ['=SUM(A9:A12)', 0],
    ['=AVERAGE(A1:A5)', 20],
    ['=AVERAGE(A9:A10)', '#DIV/0!'],
    ['=COUNT(A1:A5)', 3],
    ['=COUNT(1,"2","x",TRUE)', 3],
    ['=COUNTA(A1:A6)', 5],
    ['=COUNTA(1,"",FALSE)', 3],
    ['=MIN(A1:A5)', 10],
    ['=MAX(A1:A5,99)', 99],
    ['=MAX(A9:A10)', 0],
    ['=SUM(1,"x")', '#VALUE!'],
    ['=SUM(A1,1/0)', '#DIV/0!'],
    ['=SUM(A:A)', 60],
    ['=COUNT(1:1)', 2],
  ])('%s = %j', (f, v) => {
    expect(calc(f, DATA)).toBe(v);
  });
});

describe('logical', () => {
  it.each([
    ['=IF(TRUE,1,2)', 1],
    ['=IF(FALSE,1,2)', 2],
    ['=IF(FALSE,1)', false],
    ['=IF(TRUE,,2)', 0],
    ['=IF(FALSE,1,)', 0],
    ['=IF("TRUE",1,2)', 1],
    ['=IF("x",1,2)', '#VALUE!'],
    ['=IF(1/0,1,2)', '#DIV/0!'],
    ['=IF(TRUE,"ok",1/0)', 'ok'],
    ['=IF(0,1/0,"lazy")', 'lazy'],
    ['=IFS(FALSE,1,TRUE,2)', 2],
    ['=IFS(1>2,"a")', '#N/A'],
    ['=IFS(TRUE,"a",1/0,"b")', 'a'],
    ['=AND(TRUE,1,A1)', true],
    ['=AND(TRUE,0)', false],
    ['=AND(A3)', '#VALUE!'],
    ['=AND("x")', '#VALUE!'],
    ['=OR(FALSE,0,A4)', true],
    ['=OR(FALSE,1/0)', '#DIV/0!'],
    ['=AND({TRUE,FALSE})', false],
    ['=NOT(0)', true],
    ['=IFERROR(1/0,"none")', 'none'],
    ['=IFERROR(5,"none")', 5],
    ['=IFERROR(#N/A,)', 0],
  ])('%s = %j', (f, v) => {
    expect(calc(f, DATA)).toBe(v);
  });

  it('IF and IFERROR lift over arrays element-wise', () => {
    expect(calc('=IF({1,0,1},"y","n")')).toEqual([['y', 'n', 'y']]);
    expect(calc('=IF(C1:C3>1,D1:D3,"-")', DATA)).toEqual([['-'], ['b'], ['c']]);
    expect(calc('=IFERROR(1/{1,0,2},"x")')).toEqual([[1, 'x', 0.5]]);
    expect(calc('=IFS({1,0},"a",TRUE,"b")')).toEqual([['a', 'b']]);
    expect(calc('=NOT({TRUE,FALSE})')).toEqual([[false, true]]);
  });
});

describe('math', () => {
  it.each([
    ['=ROUND(2.345,2)', 2.35],
    ['=ROUND(-2.5,0)', -3],
    ['=ROUND(1234,-2)', 1200],
    ['=ROUND("1.25",1)', 1.3],
    ['=ABS(-4)', 4],
    ['=INT(-2.5)', -3],
    ['=INT(2.9)', 2],
    ['=MOD(10,3)', 1],
    ['=MOD(-3,2)', 1],
    ['=MOD(3,-2)', -1],
    ['=MOD(5,0)', '#DIV/0!'],
    ['=MOD(6,3)', 0],
    ['=ROUND({1.26,2.34},1)', [[1.3, 2.3]]],
  ])('%s = %j', (f, v) => {
    expect(calc(f)).toEqual(v);
  });
});

describe('text', () => {
  it.each([
    ['=LEN("Officina")', 8],
    ['=LEN(12.5)', 4],
    ['=LEN(TRUE)', 4],
    ['=LEFT("Officina",3)', 'Off'],
    ['=LEFT("Officina")', 'O'],
    ['=LEFT("abc",)', ''],
    ['=LEFT("abc",10)', 'abc'],
    ['=LEFT("abc",-1)', '#VALUE!'],
    ['=RIGHT("Officina",4)', 'cina'],
    ['=RIGHT("abc",0)', ''],
    ['=MID("Officina",2,3)', 'ffi'],
    ['=MID("abc",5,2)', ''],
    ['=MID("abc",0,2)', '#VALUE!'],
    ['=CONCAT("a",1,TRUE)', 'a1TRUE'],
    ['=CONCAT(D1:D3)', 'abc'],
    ['=CONCAT(D1:D3,#N/A)', '#N/A'],
    ['=TEXTJOIN(", ",TRUE,"a","","b")', 'a, b'],
    ['=TEXTJOIN("-",FALSE,"a","","b")', 'a--b'],
    ['=TEXTJOIN({"+","-"},TRUE,1,2,3,4)', '1+2-3+4'],
    ['=TEXTJOIN(",",TRUE,D1:D3,C1:C3)', 'a,b,c,1,2,3'],
    ['="a"&1.5&TRUE', 'a1.5TRUE'],
    ['=LEN({"ab","cde"})', [[2, 3]]],
  ])('%s = %j', (f, v) => {
    expect(calc(f, DATA)).toEqual(v);
  });
});

describe('lookup and reference', () => {
  const T = {
    A1: 1,
    A2: 3,
    A3: 5,
    A4: 7,
    B1: 'one',
    B2: 'three',
    B3: 'five',
    B4: 'seven',
    C1: 'Apple',
    C2: 'Banana',
    C3: 'Cherry',
    C4: 'Date',
  };
  it.each([
    ['=VLOOKUP(5,A1:B4,2,FALSE)', 'five'],
    ['=VLOOKUP(4,A1:B4,2,FALSE)', '#N/A'],
    ['=VLOOKUP(4,A1:B4,2)', 'three'],
    ['=VLOOKUP(4,A1:B4,2,TRUE)', 'three'],
    ['=VLOOKUP(0,A1:B4,2)', '#N/A'],
    ['=VLOOKUP(99,A1:B4,2)', 'seven'],
    ['=VLOOKUP(5,A1:B4,3,FALSE)', '#REF!'],
    ['=VLOOKUP(5,A1:B4,0,FALSE)', '#VALUE!'],
    ['=VLOOKUP("ban*",C1:C4,1,FALSE)', 'Banana'],
    ['=VLOOKUP(2,{1,"a";2,"b"},2,FALSE)', 'b'],
    ['=XLOOKUP(5,A1:A4,B1:B4)', 'five'],
    ['=XLOOKUP(4,A1:A4,B1:B4)', '#N/A'],
    ['=XLOOKUP(4,A1:A4,B1:B4,"none")', 'none'],
    ['=XLOOKUP(4,A1:A4,B1:B4,,-1)', 'three'],
    ['=XLOOKUP(4,A1:A4,B1:B4,,1)', 'five'],
    ['=XLOOKUP("c*",C1:C4,A1:A4,,2)', 5],
    ['=XLOOKUP(5,A1:A4,B1:B4,,0,-1)', 'five'],
    ['=XLOOKUP(6,A1:A4,B1:B4,,-1,2)', 'five'],
    ['=XLOOKUP(6,A1:A4,B1:B4,,1,2)', 'seven'],
    ['=XLOOKUP(7,A1:A4,B1:B4,,0,2)', 'seven'],
    ['=XLOOKUP(5,{1,3,5},{"a","b","c"})', 'c'],
    ['=XLOOKUP(1,A1:A4,B1:B3)', '#VALUE!'],
    ['=MATCH(5,A1:A4,0)', 3],
    ['=MATCH(6,A1:A4)', 3],
    ['=MATCH(6,A1:A4,1)', 3],
    ['=MATCH(0,A1:A4,1)', '#N/A'],
    ['=MATCH(4,{7,5,3,1},-1)', 2],
    ['=MATCH("b?nana",C1:C4,0)', 2],
    ['=MATCH("BANANA",C1:C4,0)', 2],
    ['=MATCH("x~*",{"x*","xy"},0)', 1],
    ['=INDEX(A1:B4,3,2)', 'five'],
    ['=INDEX(A1:A4,2)', 3],
    ['=INDEX({1,2,3},2)', 2],
    ['=INDEX(A1:B4,5,1)', '#REF!'],
    ['=INDEX(A1:B4,-1,1)', '#VALUE!'],
    ['=SUM(INDEX(A1:B4,0,1))', 16],
    ['=SUM(INDEX(A1:B4,2,0))', 3],
    ['=INDEX((A1:A4,C1:C4),2,1,2)', 'Banana'],
    ['=ROW()', 1],
    ['=ROW(C7)', 7],
    ['=COLUMN(D2)', 4],
    ['=COLUMN()', 1],
    ['=ROW(OFFSET(A1,2,0))', 3],
    ['=SUM(OFFSET(A1,0,0,4,1))', 16],
    ['=SUM(OFFSET(A4,0,0,-2,1))', 12],
    ['=OFFSET(A1,-1,0)', '#REF!'],
    ['=OFFSET(A1,0,0,0,1)', '#REF!'],
    ['=OFFSET(1,0,0)', '#VALUE!'],
    ['=INDIRECT("B2")', 'three'],
    ['=SUM(INDIRECT("A1:A"&4))', 16],
    ['=INDIRECT("R3C2",FALSE)', 'five'],
    ['=INDIRECT("nonsense")', '#REF!'],
    ['=INDIRECT("1+1")', '#REF!'],
  ])('%s = %j', (f, v) => {
    expect(calc(f, T)).toEqual(v);
  });

  it('INDEX returns a reference usable by the range operator', () => {
    expect(calc('=SUM(A1:INDEX(A1:A4,3))', T)).toBe(9);
  });

  it('XLOOKUP returns whole rows and references', () => {
    expect(calc('=XLOOKUP(3,A1:A4,B1:C4)', T)).toEqual([['three', 'Banana']]);
    expect(calc('=SUM(XLOOKUP(1,A1:A4,A1:A4):XLOOKUP(5,A1:A4,A1:A4))', T)).toBe(9);
  });

  it('ROW and COLUMN of ranges spill arrays', () => {
    expect(calc('=ROW(A2:A4)')).toEqual([[2], [3], [4]]);
    expect(calc('=COLUMN(B1:D1)')).toEqual([[2, 3, 4]]);
  });

  it('lookups lift over an array of lookup values', () => {
    expect(calc('=XLOOKUP({1,7},A1:A4,B1:B4)', T)).toEqual([['one', 'seven']]);
    expect(calc('=MATCH({5,1},A1:A4,0)', T)).toEqual([[3, 1]]);
    expect(calc('=INDEX(A1:A4,{1;3})', T)).toEqual([[1], [5]]);
  });
});

describe('dates', () => {
  it('DATE builds serials with overflow and the Lotus bug', () => {
    expect(calc('=DATE(2024,1,15)')).toBe(45306);
    expect(calc('=DATE(2023,14,1)')).toBe(45323);
    expect(calc('=DATE(1900,2,29)')).toBe(60);
    expect(calc('=DATE(124,1,15)')).toBe(45306);
    expect(calc('=DATE(-1,1,1)')).toBe('#NUM!');
    expect(calc('=DATE(2024,{1,2},1)')).toEqual([[45292, 45323]]);
  });

  it('TODAY and NOW use the engine clock', () => {
    expect(calc('=TODAY()')).toBe(45306);
    expect(calc('=NOW()')).toBe(45306.5);
  });

  it('dates in text take part in arithmetic', () => {
    expect(calc('="2024-01-16"-DATE(2024,1,15)')).toBe(1);
  });
});

describe('operators', () => {
  it.each([
    ['=0.1+0.2=0.3', true],
    ['=-2^2', 4],
    ['=2^3^2', 64],
    ['=2^-1', 0.5],
    ['=0^0', '#NUM!'],
    ['=(-8)^(1/3)', '#NUM!'],
    ['=0^-1', '#DIV/0!'],
    ['=1/0', '#DIV/0!'],
    ['=5%', 0.05],
    ['=200%%', 0.02],
    ['="3"+"4"', 7],
    ['="1,234"+1', 1235],
    ['="50%"*2', 1],
    ['="abc"+1', '#VALUE!'],
    ['=TRUE+TRUE', 2],
    ['="a"="A"', true],
    ['="a"<"B"', true],
    ['=1<"a"', true],
    ['="z"<TRUE', true],
    ['=A1=0', true],
    ['=A1=""', true],
    ['=#N/A=1', '#N/A'],
    ['=10^308*10', '#NUM!'],
    ['=+"text"', 'text'],
    ['=-"2"', -2],
    [
      '={1,2,3}+{10;20}',
      [
        [11, 12, 13],
        [21, 22, 23],
      ],
    ],
    ['={1,2,3}+{1,2}', [[2, 4, '#N/A']]],
    ['={1,2}&"x"', [['1x', '2x']]],
    ['=-{1,2}', [[-1, -2]]],
    ['={10,20}%', [[0.1, 0.2]]],
  ])('%s = %j', (f, v) => {
    expect(calc(f)).toEqual(v);
  });

  it('reference operators', () => {
    const cells = { A1: 1, A2: 2, B1: 3, B2: 4, C3: 9 };
    expect(calc('=SUM(A1:B2 B1:C3)', cells)).toBe(7);
    expect(calc('=A1:A2 C1:C2', cells)).toBe('#NULL!');
    expect(calc('=SUM((A1,C3))', cells)).toBe(10);
    expect(calc('=SUM(A1:A2:B2)', cells)).toBe(10);
    expect(calc('=(A1,B1)', cells)).toBe('#VALUE!');
    expect(calc('=1+(A1,B1)', cells)).toBe('#VALUE!');
  });

  it('unknown functions and names are #NAME?', () => {
    expect(calc('=NOPE(1)')).toBe('#NAME?');
    expect(calc('=Unknown+1')).toBe('#NAME?');
  });

  it('wrong argument counts are #VALUE!', () => {
    expect(calc('=ROUND(1)')).toBe('#VALUE!');
    expect(calc('=NOT(1,2)')).toBe('#VALUE!');
  });

  it('_xlfn prefixes are ignored when calling functions', () => {
    expect(calc('=_xlfn.XLOOKUP(2,{1,2},{"a","b"})')).toBe('b');
  });
});

describe('function registry', () => {
  it('lists the native core set with docs', () => {
    const names = listFunctions().map((f) => f.name);
    for (const n of [
      'SUM',
      'AVERAGE',
      'COUNT',
      'COUNTA',
      'MIN',
      'MAX',
      'IF',
      'IFS',
      'AND',
      'OR',
      'NOT',
      'IFERROR',
      'ROUND',
      'ABS',
      'INT',
      'MOD',
      'LEN',
      'LEFT',
      'RIGHT',
      'MID',
      'CONCAT',
      'TEXTJOIN',
      'VLOOKUP',
      'XLOOKUP',
      'INDEX',
      'MATCH',
      'ROW',
      'COLUMN',
      'OFFSET',
      'INDIRECT',
      'TODAY',
      'NOW',
      'DATE',
      'SEQUENCE',
      'FILTER',
      'SORT',
      'UNIQUE',
      'TRANSPOSE',
      'LET',
      'LAMBDA',
      'MAP',
      'REDUCE',
      'SCAN',
      'BYROW',
      'BYCOL',
      'MAKEARRAY',
    ]) {
      expect(names).toContain(n);
      expect(getFunction(n)?.docs?.summary, n).toBeTruthy();
    }
  });

  it('every documented example evaluates to its stated result', () => {
    const e = sheet();
    // The engine's own inline docs; the WP-09 library's JSON docs are checked in test/functions/docs.test.ts.
    for (const f of CORE_FUNCTIONS) {
      for (const ex of f.docs?.examples ?? []) {
        const v = e.evaluate(ex.formula);
        const shown =
          typeof v === 'string' ? `"${v}"` : typeof v === 'boolean' ? (v ? 'TRUE' : 'FALSE') : String(v);
        expect(shown, `${f.name}: ${ex.formula}`).toBe(ex.result);
      }
    }
  });

  it('marks volatile functions', () => {
    expect(getFunction('NOW')?.volatile).toBe(true);
    expect(getFunction('OFFSET')?.volatile).toBe(true);
    expect(getFunction('SUM')?.volatile).toBeFalsy();
  });

  it('accepts new registrations (the WP-09 extension point)', () => {
    registerFunction({
      name: 'DOUBLE.IT',
      minArgs: 1,
      maxArgs: 1,
      lift: 'all',
      impl: (args) => (typeof args[0] === 'number' ? args[0] * 2 : 0),
    });
    expect(calc('=DOUBLE.IT(21)')).toBe(42);
    expect(calc('=DOUBLE.IT({1,2})')).toEqual([[2, 4]]);
    unregisterFunction('DOUBLE.IT');
    expect(calc('=DOUBLE.IT(21)')).toBe('#NAME?');
  });

  it('values flow from cells', () => {
    const e = sheet({ A1: 4, A2: '=A1*2' });
    e.recalc();
    expect(get(e, 'A2')).toBe(8);
  });
});
