/**
 * parse(print(ast)) round-trips: a curated corpus covering every construct of the grammar plus a
 * seeded random generator (property-style), together well over 300 formulas.
 */
import { describe, expect, it } from 'vitest';
import { stripSpans } from '../../src/parser/ast.ts';
import { parse } from '../../src/parser/parser.ts';
import { print } from '../../src/parser/printer.ts';
import { columnToLetters } from '../../src/parser/references.ts';
import { randInt, rng } from '../engine/_support.ts';

const CURATED = [
  // literals
  '=1',
  '=0.5',
  '=.25',
  '=1E+21',
  '=1.5E-7',
  '=123456789012345',
  '="text"',
  '=""',
  '="quote "" inside"',
  '=TRUE',
  '=FALSE',
  '=#N/A',
  '=#DIV/0!',
  '=#REF!',
  '=#NAME?',
  '=#NULL!',
  '=#NUM!',
  '=#VALUE!',
  '=#SPILL!',
  '=#CALC!',
  // references
  '=A1',
  '=$A$1',
  '=A$1',
  '=$A1',
  '=XFD1048576',
  '=A1:B2',
  '=$A$1:$B$2',
  '=A:A',
  '=$A:$C',
  '=1:1',
  '=$2:$10',
  '=Sheet1!A1',
  '=Sheet1!$A$1:$C$9',
  "='My Sheet'!A1",
  "='It''s'!B2",
  "='2024'!A1",
  "='A1'!B2",
  "='R1C1'!B2",
  '=Sheet1:Sheet3!A1',
  "='Q 1:Q 4'!B2:C3",
  '=Jan:Dec!A:A',
  '=[1]Sheet1!A1',
  "='[Book 1.xlsx]Data'!A1",
  '=Sheet1!#REF!',
  '=Données!A1',
  '=Sheet1!Total',
  "='Sheet 2'!Sales",
  // names and structured references
  '=Sales',
  '=TaxRate*2',
  '=_total',
  '=Revenue.Q1',
  '=Table1',
  '=Table1[]',
  '=Table1[Amount]',
  '=Table1[[Unit Price]]',
  '=Table1[#All]',
  '=Table1[#Data]',
  '=Table1[#Headers]',
  '=Table1[#Totals]',
  '=Table1[[#Headers],[Amount]]',
  '=Table1[[#Headers],[#Data],[Jan]:[Mar]]',
  '=Table1[[#Data],[#Totals]]',
  '=Table1[[Jan]:[Mar]]',
  '=Table1[@]',
  '=Table1[@Amount]',
  '=Table1[@[Unit Price]]',
  '=Table1[@[Jan]:[Mar]]',
  '=Table1[[#This Row],[Amount]]',
  '=[@Qty]*[@Price]',
  '=[@[Unit Price]]',
  '=[Amount]',
  "=Table1[[Col'#1]]",
  "=Table1[[It''s]]",
  "=Table1[[a'[b']]]",
  '=SUM(Table1[Amount])',
  '=COUNTA(Table1[[#Headers],[#Data],[A]])',
  // array constants
  '={1,2,3}',
  '={1;2;3}',
  '={1,2;3,4}',
  '={-1,2.5;"a",TRUE}',
  '={#N/A,"x";FALSE,0}',
  '=SUM({1,2;3,4})',
  // operators
  '=1+2',
  '=1-2',
  '=2*3',
  '=6/3',
  '=2^3',
  '=2^3^2',
  '=-2^2',
  '=2^-1',
  '=5%',
  '=-5%',
  '=50%*A1',
  '="a"&"b"',
  '=A1&B1&C1',
  '=1=1',
  '=1<>2',
  '=1<2',
  '=1>2',
  '=1<=2',
  '=1>=2',
  '=--A1',
  '=+A1',
  '=-(-A1)',
  '=(1+2)*3',
  '=((1))',
  '=A1:B2 B1:C3',
  '=(A1,B2,C3)',
  '=SUM((A1:A3,C1:C3))',
  '=A1:A3:C2',
  '=A1:INDEX(B:B,5)',
  '=@A1:A10',
  '=@Table1[Amount]',
  '=A1#',
  "='My Sheet'!B2#",
  '=SUM(A1#)',
  '=COUNT(Sheet2!A1#)',
  // functions
  '=NOW()',
  '=TODAY()',
  '=PI()',
  '=SUM(1,2,3)',
  '=SUM(A1:A10,B1:B10)',
  '=IF(A1,,2)',
  '=IF(A1>0,"pos",IF(A1<0,"neg","zero"))',
  '=IFERROR(1/0,"")',
  '=VLOOKUP(A1,Sheet2!A:C,3,FALSE)',
  '=XLOOKUP(A1,B:B,C:C,"none",0,-1)',
  '=INDEX(A1:C10,MATCH(D1,B1:B10,0),3)',
  '=SUMPRODUCT((A1:A10>5)*(B1:B10))',
  '=TEXTJOIN(", ",TRUE,A1:A5)',
  '=LOG10(100)',
  '=ATAN2(1,1)',
  '=DEC2BIN(10)',
  '=_xlfn.XLOOKUP(1,A:A,B:B)',
  '=_xlfn._xlws.SORT(A1:A10)',
  '=STDEV.S(A1:A10)',
  '=F.DIST.RT(1,2,3)',
  '=LET(x,2,y,x*3,x+y)',
  '=LAMBDA(x,x*2)(21)',
  '=LAMBDA(a,b,a+b)(1,2)',
  '=LAMBDA(x,LAMBDA(y,x+y))(1)(2)',
  '=MAP(A1:A3,LAMBDA(v,v*2))',
  '=REDUCE(0,A1:A10,LAMBDA(acc,v,acc+v))',
  '=SCAN(,A1:A3,LAMBDA(a,b,a+b))',
  '=MAKEARRAY(3,3,LAMBDA(r,c,r*c))',
  '=BYROW(A1:C3,LAMBDA(r,SUM(r)))',
  '=SEQUENCE(10,2,0,0.5)',
  '=FILTER(A1:B10,B1:B10>5,"none")',
  '=SORT(UNIQUE(A1:A100),1,-1)',
  '=TRANSPOSE({1,2,3})',
  '=OFFSET(A1,1,1,2,2)',
  '=INDIRECT("Sheet2!A"&ROW())',
  '=SUM(INDIRECT("A1:A"&B1))',
  '=TRUE()',
  '=IF(TRUE(),FALSE(),1)',
  '=CONCAT("a",,"b")',
  '=SUM(Jan:Dec!B2)',
  '=MyLambda(1,2)',
  '=DATE(2024,1,15)+TIME(12,0,0)',
];

const FUNCS = ['SUM', 'IF', 'MAX', 'CONCAT', 'INDEX', 'XLOOKUP', 'LEN', 'ROUND', 'F.DIST', 'LOG10'];
const NAMES = ['Sales', 'TaxRate', '_x', 'Revenue.Q1', 'λ'];
const SHEETS = ['Sheet1', 'My Sheet', "O'Brien", 'Data2'];
const BIN = ['+', '-', '*', '/', '^', '&', '=', '<>', '<', '>', '<=', '>='];
const ERRORS = ['#N/A', '#DIV/0!', '#VALUE!', '#REF!'];

function quoteSheet(s: string): string {
  return /^[A-Za-z_][A-Za-z0-9_.]*$/.test(s) ? s : `'${s.replace(/'/g, "''")}'`;
}

function genFormulas(seed: number, count: number): string[] {
  const r = rng(seed);
  const pick = <T>(xs: readonly T[]): T => xs[Math.floor(r() * xs.length)] as T;
  const cell = () =>
    `${r() < 0.3 ? '$' : ''}${columnToLetters(randInt(r, 0, 700))}${r() < 0.3 ? '$' : ''}${randInt(r, 1, 5000)}`;
  const prefix = () => {
    const x = r();
    if (x < 0.6) return '';
    if (x < 0.85) return `${quoteSheet(pick(SHEETS))}!`;
    return `${quoteSheet(`${pick(SHEETS)}:${pick(SHEETS)}`)}!`.replace(/^([^']*:[^']*)!$/, '$1!');
  };
  const ref = (): string => {
    const x = r();
    if (x < 0.45) return prefix() + cell();
    if (x < 0.8) return `${prefix()}${cell()}:${cell()}`;
    if (x < 0.9)
      return `${prefix()}${columnToLetters(randInt(r, 0, 30))}:${columnToLetters(randInt(r, 0, 30))}`;
    return `${prefix()}${randInt(r, 1, 99)}:${randInt(r, 1, 99)}`;
  };
  const num = () => {
    const x = r();
    if (x < 0.5) return String(randInt(r, 0, 1000));
    if (x < 0.8) return String(randInt(r, 0, 10000) / 100);
    if (x < 0.9) return `${randInt(r, 1, 9)}E+${randInt(r, 21, 99)}`;
    return `${randInt(r, 1, 9)}E-${randInt(r, 7, 99)}`;
  };
  const atom = (d: number): string => {
    const x = r();
    if (x < 0.15) return num();
    if (x < 0.22) return `"${pick(['a', 'b c', 'say ""hi""', ''])}"`;
    if (x < 0.25) return pick(['TRUE', 'FALSE']);
    if (x < 0.28) return pick(ERRORS);
    if (x < 0.5) return ref();
    if (x < 0.55) return pick(NAMES);
    if (x < 0.6) return pick(['T[Amount]', '[@Qty]', 'T[[#Headers],[A]:[B]]', 'T[#All]', 'T[@[Unit Price]]']);
    if (x < 0.63) return `{${randInt(r, -5, 5)},"x";TRUE,#N/A}`;
    if (x < 0.66) return `${cell()}#`;
    if (d <= 0) return num();
    if (x < 0.75) {
      const n = randInt(r, 0, 4);
      const args = Array.from({ length: n }, () => (r() < 0.15 ? '' : expr(d - 1)));
      return `${pick(FUNCS)}(${args.join(',')})`;
    }
    if (x < 0.82) return `(${expr(d - 1)})`;
    if (x < 0.86) return `-${atom(d - 1)}`;
    if (x < 0.88) return `@${ref()}`;
    if (x < 0.9) return `${atom(d - 1)}%`;
    if (x < 0.93) return `(${ref()},${ref()})`;
    if (x < 0.96) return `${ref()} ${ref()}`;
    return `LAMBDA(x,${expr(d - 1)})(${expr(d - 1)})`;
  };
  const expr = (d: number): string => {
    let s = atom(d);
    const n = randInt(r, 0, 3);
    for (let i = 0; i < n; i++) s += pick(BIN) + atom(d - 1);
    return s;
  };
  return Array.from({ length: count }, () => `=${expr(3)}`);
}

const GENERATED = genFormulas(20261015, 420);

function roundTrip(src: string): void {
  const ast1 = parse(src);
  const printed = print(ast1, { leadingEquals: true });
  const ast2 = parse(printed);
  expect(stripSpans(ast2), `${src} → ${printed}`).toEqual(stripSpans(ast1));
  expect(print(ast2, { leadingEquals: true })).toBe(printed);
}

describe('parse(print(ast)) round-trips', () => {
  it('covers at least 300 formulas including structured refs, 3-D refs, array constants and quoted sheets', () => {
    const all = [...CURATED, ...GENERATED];
    expect(all.length).toBeGreaterThanOrEqual(300);
    expect(all.some((f) => f.includes('[#Headers]'))).toBe(true);
    expect(all.some((f) => f.includes('Sheet1:Sheet3!'))).toBe(true);
    expect(all.some((f) => f.includes('{'))).toBe(true);
    expect(all.some((f) => f.includes("'My Sheet'!"))).toBe(true);
    for (const f of all) roundTrip(f);
  });

  it.each(CURATED)('curated: %s', (src) => roundTrip(src));

  it('prints canonical text for the curated corpus (whitespace-free, upper-case functions)', () => {
    expect(print(parse('= sum( a1 , 2 )'), { leadingEquals: true })).toBe('=SUM(A1,2)');
    expect(print(parse('=Table1[[#This Row],[Amount]]'))).toBe('Table1[@Amount]');
    expect(print(parse("=T[[#Headers],[Col'#1]]"))).toBe("T[[#Headers],[Col'#1]]");
    expect(print(parse('=T[[A]]'))).toBe('T[A]');
    expect(print(parse("='Sheet1'!A1"))).toBe('Sheet1!A1');
  });

  it('generated formulas are diverse', () => {
    const set = new Set(GENERATED);
    expect(set.size).toBeGreaterThan(400);
  });
});
