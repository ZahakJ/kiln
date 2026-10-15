/**
 * WP-09 acceptance spot checks plus behaviour that needs a workbook (whole-column criteria ranges,
 * hidden rows, nested subtotals, HYPERLINK targets, CELL/SHEET information).
 */
import { describe, expect, it } from 'vitest';
import { listFunctions } from '../../src/engine/functions/registry.ts';
import { MemoryHost } from '../../src/engine/host.ts';
import { parseCriterion } from '../../src/functions/criteria.ts';
import { ErrorValue } from '../../src/values/errors.ts';
import { BLANK } from '../../src/values/types.ts';
import { engine } from './_support.ts';

function at(a1: string): { row: number; col: number } {
  const m = /^([A-Z]+)(\d+)$/.exec(a1) as RegExpExecArray;
  let col = 0;
  for (const ch of m[1] as string) col = col * 26 + ch.charCodeAt(0) - 64;
  return { row: Number(m[2]) - 1, col: col - 1 };
}

describe('WP-09 acceptance spot checks', () => {
  const e = engine();
  const ev = (f: string) => e.evaluate(f);

  it('registers at least 350 functions', () => {
    expect(listFunctions().length).toBeGreaterThanOrEqual(350);
  });

  it('TEXT formats with Excel number formats', () => {
    expect(ev('=TEXT(1234.5,"#,##0.00")')).toBe('1,234.50');
  });

  it('DATEDIF "md" counts the days left after whole months', () => {
    expect(ev('=DATEDIF(DATE(2020,1,31),DATE(2020,3,1),"md")')).toBe(1);
  });

  it('XLOOKUP finds an exact match', () => {
    expect(ev('=XLOOKUP(5,{1,3,5},{"a","b","c"})')).toBe('c');
  });

  it('compares numbers after rounding to 15 significant digits', () => {
    expect(ev('=0.1+0.2=0.3')).toBe(true);
  });

  it('IRR solves the cash-flow equation', () => {
    const r = ev('=IRR({-100,30,40,50})') as number;
    // The rate that makes the net present value zero (the acceptance text's "0.0894" is a typo:
    // NPV at 8.94% is -0.084; Excel and LibreOffice both return 0.0889633947).
    expect(Math.abs(r - 0.0889633947)).toBeLessThan(1e-7);
    const npv = -100 + 30 / (1 + r) + 40 / (1 + r) ** 2 + 50 / (1 + r) ** 3;
    expect(Math.abs(npv)).toBeLessThan(1e-9);
  });

  it('SUMIFS takes whole-column ranges and date criteria written as text', () => {
    const w = engine();
    const rows: [number, string][] = [
      [10, '2023-12-31'],
      [20, '2024-01-01'],
      [30, '2024-06-30'],
      [40, '2025-02-01'],
    ];
    rows.forEach(([amount, date], i) => {
      w.setCell('Sheet1', i, 0, { value: amount });
      w.setCell('Sheet1', i, 1, { formula: `=DATEVALUE("${date}")` });
    });
    w.setCell('Sheet1', 0, 3, { formula: '=SUMIFS(A:A,B:B,">=2024-01-01")' });
    w.setCell('Sheet1', 1, 3, { formula: '=SUMIFS(A:A,B:B,">=2024-01-01",B:B,"<"&DATE(2025,1,1))' });
    w.setCell('Sheet1', 2, 3, { formula: '=COUNTIFS(B:B,">=1/1/2024",A:A,">15")' });
    w.setCell('Sheet1', 3, 3, { formula: '=AVERAGEIFS(A:A,B:B,"<=2024-06-30")' });
    w.recalc();
    expect(w.getValue('Sheet1', 0, 3)).toBe(90);
    expect(w.getValue('Sheet1', 1, 3)).toBe(50);
    expect(w.getValue('Sheet1', 2, 3)).toBe(3);
    expect(w.getValue('Sheet1', 3, 3)).toBe(20);
  });
});

describe('criteria', () => {
  const cases: [unknown, unknown, boolean][] = [
    ['>5', 6, true],
    ['>5', 5, false],
    ['>5', '6', false],
    ['<>5', 'x', true],
    ['<>5', BLANK, true],
    ['=5', '5', true],
    [5, 5, true],
    ['apple', 'APPLE', true],
    ['a*', 'avocado', true],
    ['a?', 'ab', true],
    ['a?', 'abc', false],
    ['~*', '*', true],
    ['~*', 'x', false],
    ['', BLANK, true],
    ['', '', true],
    ['', 0, false],
    ['=', BLANK, true],
    ['<>', BLANK, false],
    ['<>', 'x', true],
    ['<>', '', true],
    ['>b', 'c', true],
    ['>b', 5, false],
    ['TRUE', true, true],
    ['TRUE', 'TRUE', false],
    ['#N/A', new ErrorValue('#N/A'), true],
    ['>=2024-01-01', 45292, true],
    ['>=2024-01-01', 45291, false],
    ['<>apple', 'Apple', false],
    ['>=50%', 0.5, true],
  ];
  it.each(cases)('%j matches %j → %s', (crit, value, expected) => {
    expect(parseCriterion(crit as never)(value as never)).toBe(expected);
  });
});

describe('SUBTOTAL and AGGREGATE with hidden rows', () => {
  function sheet() {
    const host = new MemoryHost();
    const e = engine({ host });
    for (let r = 0; r < 6; r++) e.setCell('Sheet1', r, 0, { value: (r + 1) * 10 }); // 10..60
    host.setRowHidden(1, 1, true); // A2 hidden manually (Sheet1 has id 1)
    host.setRowHidden(1, 3, true, true); // A4 hidden by a filter
    e.setCell('Sheet1', 6, 0, { formula: '=SUBTOTAL(9,A1:A6)' }); // A7: nested subtotal
    return e;
  }

  it('SUBTOTAL 1–11 skips filtered rows, 101–111 skips every hidden row', () => {
    const e = sheet();
    e.recalc();
    expect(e.getValue('Sheet1', 6, 0)).toBe(210 - 40);
    expect(e.evaluate('=SUBTOTAL(109,A1:A6)')).toBe(210 - 40 - 20);
    expect(e.evaluate('=SUBTOTAL(2,A1:A6)')).toBe(5);
    expect(e.evaluate('=SUBTOTAL(102,A1:A6)')).toBe(4);
  });

  it('ignores nested SUBTOTAL and AGGREGATE results', () => {
    const e = sheet();
    e.recalc();
    expect(e.evaluate('=SUBTOTAL(9,A1:A7)')).toBe(170);
    expect(e.evaluate('=AGGREGATE(9,0,A1:A7)')).toBe(210);
    expect(e.evaluate('=AGGREGATE(9,4,A1:A7)')).toBe(210 + 170);
  });

  it('AGGREGATE options ignore hidden rows and errors', () => {
    const e = sheet();
    e.setCell('Sheet1', 7, 0, { formula: '=1/0' }); // A8
    e.recalc();
    expect(e.evaluate('=AGGREGATE(9,5,A1:A6)')).toBe(150);
    expect(e.evaluate('=AGGREGATE(9,6,A1:A8)')).toBe(210 + 170);
    expect(e.evaluate('=AGGREGATE(4,7,A1:A8)')).toBe(170);
    expect((e.evaluate('=AGGREGATE(9,4,A1:A8)') as ErrorValue).code).toBe('#DIV/0!');
    expect(e.evaluate('=AGGREGATE(14,6,A1:A8,2)')).toBe(60);
  });
});

describe('information and links', () => {
  it('HYPERLINK shows the friendly name and records the target', () => {
    const e = engine();
    e.setCell('Sheet1', 0, 0, { formula: '=HYPERLINK("https://example.com/a b","Example")' });
    e.setCell('Sheet1', 1, 0, { formula: '=IF(FALSE,HYPERLINK("x"),"plain")' });
    e.recalc();
    expect(e.getValue('Sheet1', 0, 0)).toBe('Example');
    expect(e.getHyperlink('Sheet1', 0, 0)).toBe('https://example.com/a b');
    expect(e.getHyperlink('Sheet1', 1, 0)).toBeUndefined();
  });

  it('CELL reports address, row, column, contents and type', () => {
    const e = engine();
    e.addSheet('Data Sheet');
    e.setCell('Sheet1', 2, 1, { value: 'abc' }); // B3
    e.setCell('Sheet1', 3, 1, { value: 42 }); // B4
    e.recalc();
    expect(e.evaluate('=CELL("address",B3)')).toBe('$B$3');
    expect(e.evaluate('=CELL("address",\'Data Sheet\'!C9)')).toBe("'Data Sheet'!$C$9");
    expect(e.evaluate('=CELL("row",B3)')).toBe(3);
    expect(e.evaluate('=CELL("col",B3)')).toBe(2);
    expect(e.evaluate('=CELL("contents",B4)')).toBe(42);
    expect(e.evaluate('=CELL("type",B3)')).toBe('l');
    expect(e.evaluate('=CELL("type",B4)')).toBe('v');
    expect(e.evaluate('=CELL("type",B5)')).toBe('b');
    expect(e.evaluate('=CELL("filename",B3)')).toBe('');
    expect((e.evaluate('=CELL("color",B3)') as ErrorValue).code).toBe('#VALUE!');
    expect(e.evaluate('=CELL("row")', { row: 9, col: 0 })).toBe(10);
  });

  it('SHEET, SHEETS, ISFORMULA and FORMULATEXT read the workbook', () => {
    const e = engine();
    e.addSheet('Two');
    e.addSheet('Three');
    e.setCell('Sheet1', 0, 0, { formula: '=1+1' });
    e.recalc();
    expect(e.evaluate('=SHEETS()')).toBe(3);
    expect(e.evaluate('=SHEET("Three")')).toBe(3);
    expect(e.evaluate('=SHEET(Two!A1)')).toBe(2);
    expect(e.evaluate('=SHEETS(Sheet1:Three!A1)')).toBe(3);
    expect(e.evaluate('=ISFORMULA(A1)')).toBe(true);
    expect(e.evaluate('=FORMULATEXT(A1)')).toBe('=1+1');
    expect(e.evaluate('=INFO("numfile")')).toBe(3);
  });

  it('ENCODEURL percent-encodes everything but unreserved characters', () => {
    const e = engine();
    expect(e.evaluate('=ENCODEURL("a b&c=ü~-_.")')).toBe('a%20b%26c%3D%C3%BC~-_.');
  });
});

describe('dynamic arrays from library functions spill', () => {
  it('TEXTSPLIT, SORTBY and SEQUENCE results spill into the grid', () => {
    const e = engine();
    e.setCell('Sheet1', 0, 0, { formula: '=TEXTSPLIT("a,b,c",",")' });
    e.setCell('Sheet1', 2, 0, { formula: '=SORTBY({"x";"y";"z"},{3;1;2})' });
    e.recalc();
    expect(e.getValue('Sheet1', 0, 2)).toBe('c');
    expect(e.getValue('Sheet1', 3, 0)).toBe('z');
    expect(e.getSpillRange('Sheet1', 2, 0)).toMatchObject({ row1: 4, col1: 0 });
  });

  it('lifts scalar functions over ranges', () => {
    const e = engine();
    for (let r = 0; r < 3; r++) e.setCell('Sheet1', r, 0, { value: r + 1.5 });
    e.setCell('Sheet1', 0, 1, { formula: '=ROUNDUP(A1:A3,0)' });
    e.recalc();
    expect([0, 1, 2].map((r) => e.getValue('Sheet1', r, 1))).toEqual([2, 3, 4]);
    expect(at('B1')).toEqual({ row: 0, col: 1 });
  });
});
