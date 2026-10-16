import { describe, expect, it } from 'vitest';
import {
  applyCommand,
  type Command,
  CommandLog,
  MAX_COLS,
  MAX_ROWS,
  ModelError,
  parseCommand,
  type Sheet,
  serializeCommand,
  Workbook,
} from '../src/index.ts';
import { nn, stateOf } from './helpers.ts';

function setup() {
  const wb = new Workbook();
  const log = new CommandLog(wb);
  const sheet = wb.sheets[0] as Sheet;
  return { wb, log, sheet, s: sheet.id };
}

function grid(sheet: Sheet, r0: number, c0: number, r1: number, c1: number): unknown[][] {
  const out: unknown[][] = [];
  for (let r = r0; r <= r1; r++) {
    const line: unknown[] = [];
    for (let c = c0; c <= c1; c++) {
      const f = sheet.cells.getFormula(r, c);
      line.push(f !== null ? `=${f}` : sheet.cells.getValue(r, c));
    }
    out.push(line);
  }
  return out;
}

describe('setCells and setRange', () => {
  it('sets values, formulas and styles with exact undo', () => {
    const { wb, log, sheet, s } = setup();
    const before = stateOf(wb);
    log.exec({
      type: 'setCells',
      sheet: s,
      cells: [
        { row: 0, col: 0, value: 1 },
        { row: 0, col: 1, formula: '=A1+1' },
      ],
    });
    expect(grid(sheet, 0, 0, 0, 1)).toEqual([[1, '=A1+1']]);
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 0, col: 1, value: 'text' }] });
    expect(sheet.cells.getFormula(0, 1)).toBeNull();
    log.undo();
    expect(sheet.cells.getFormula(0, 1)).toBe('A1+1');
    log.undo();
    expect(stateOf(wb)).toEqual(before);
  });

  it('rejects invalid formulas, styles and positions without changing anything', () => {
    const { wb, log, s } = setup();
    const before = stateOf(wb);
    expect(() =>
      log.exec({
        type: 'setCells',
        sheet: s,
        cells: [
          { row: 0, col: 0, value: 1 },
          { row: 0, col: 1, formula: '=SUM(' },
        ],
      }),
    ).toThrow(ModelError);
    expect(() => log.exec({ type: 'setCells', sheet: s, cells: [{ row: 0, col: 0, style: 99 }] })).toThrow(
      ModelError,
    );
    expect(() =>
      log.exec({ type: 'setCells', sheet: s, cells: [{ row: MAX_ROWS, col: 0, value: 1 }] }),
    ).toThrow(ModelError);
    expect(() => log.exec({ type: 'setCells', sheet: 999, cells: [] })).toThrow(ModelError);
    expect(stateOf(wb)).toEqual(before);
    expect(log.canUndo).toBe(false);
  });

  it('gives new cells the format of their row or column', () => {
    const { wb, log, sheet, s } = setup();
    log.exec({
      type: 'setStyle',
      sheet: s,
      ranges: [{ r0: 0, c0: 2, r1: MAX_ROWS - 1, c1: 2 }],
      patch: { font: { bold: true } },
    });
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 50, col: 2, value: 'x' }] });
    expect(wb.styles.get(sheet.cells.getStyle(50, 2)).font.bold).toBe(true);
  });

  it('writes 2-D blocks', () => {
    const { log, sheet, s } = setup();
    log.exec({
      type: 'setRange',
      sheet: s,
      row: 1,
      col: 1,
      values: [
        [1, 2],
        [3, { e: '#N/A' }],
      ],
    });
    expect(grid(sheet, 1, 1, 2, 2)).toEqual([
      [1, 2],
      [3, sheet.cells.getValue(2, 2)],
    ]);
    expect((sheet.cells.getValue(2, 2) as { code: string }).code).toBe('#N/A');
    log.undo();
    expect(sheet.cells.size).toBe(0);
  });
});

describe('clearRange', () => {
  it.each(['all', 'contents', 'formats', 'notes', 'hyperlinks'] as const)(
    'clears %s and undoes exactly',
    (what) => {
      const { wb, log, sheet, s } = setup();
      log.exec({
        type: 'setCells',
        sheet: s,
        cells: [{ row: 0, col: 0, value: 5, style: wb.styles.patch(0, { font: { italic: true } }) }],
      });
      log.exec({ type: 'merge', sheet: s, range: { r0: 2, c0: 0, r1: 2, c1: 1 } });
      log.exec({
        type: 'setSheetMeta',
        sheet: s,
        key: 'notes',
        value: [{ row: 0, col: 0, author: 'A', text: 'n' }],
      });
      log.exec({
        type: 'setSheetMeta',
        sheet: s,
        key: 'hyperlinks',
        value: [{ ref: { r0: 0, c0: 0, r1: 0, c1: 0 }, target: 'https://x' }],
      });
      const before = stateOf(wb);
      log.exec({ type: 'clearRange', sheet: s, ranges: [{ r0: 0, c0: 0, r1: 5, c1: 5 }], what });
      const c = sheet.cells.get(0, 0);
      if (what === 'all') {
        expect(c).toBeUndefined();
        expect(sheet.notes).toEqual([]);
        expect(sheet.merges).toEqual([]);
      }
      if (what === 'contents') expect(c?.value).toBeNull();
      if (what === 'formats') {
        expect(c?.style).toBe(0);
        expect(c?.value).toBe(5);
        expect(sheet.merges).toEqual([]);
      }
      if (what === 'notes') expect(sheet.notes).toEqual([]);
      if (what === 'hyperlinks') expect(sheet.hyperlinks).toEqual([]);
      log.undo();
      expect(stateOf(wb)).toEqual(before);
    },
  );
});

describe('setStyle', () => {
  it('formats cells and reuses style indices', () => {
    const { wb, log, sheet, s } = setup();
    log.exec({
      type: 'setStyle',
      sheet: s,
      ranges: [{ r0: 0, c0: 0, r1: 9, c1: 9 }],
      patch: { fill: { type: 'pattern', pattern: 'solid', fg: { rgb: 'FFFFFF00' } } },
    });
    expect(sheet.cells.size).toBe(100);
    const st = new Set<number>();
    sheet.cells.forEach((_r, _c, _v, _f, x) => {
      st.add(x);
    });
    expect(st.size).toBe(1);
    expect(wb.styles.count).toBe(2);
    log.undo();
    expect(sheet.cells.size).toBe(0);
  });

  it('formats whole columns through column properties', () => {
    const { wb, log, sheet, s } = setup();
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 100, col: 1, value: 1 }] });
    log.exec({
      type: 'setStyle',
      sheet: s,
      ranges: [{ r0: 0, c0: 1, r1: MAX_ROWS - 1, c1: 1 }],
      patch: { numFmt: '0.00' },
    });
    expect(sheet.cells.size).toBe(1);
    expect(wb.styles.get(sheet.cols.get(1)?.style ?? 0).numFmt).toBe('0.00');
    expect(wb.styles.get(sheet.cells.getStyle(100, 1)).numFmt).toBe('0.00');
    expect(wb.styles.get(sheet.styleAt(5000, 1)).numFmt).toBe('0.00');
    log.undo();
    expect(sheet.cols.get(1)).toBeUndefined();
    expect(sheet.cells.getStyle(100, 1)).toBe(0);
  });

  it('formats whole rows and their intersections with formatted columns', () => {
    const { wb, log, sheet, s } = setup();
    log.exec({
      type: 'setStyle',
      sheet: s,
      ranges: [{ r0: 0, c0: 3, r1: MAX_ROWS - 1, c1: 3 }],
      patch: { font: { bold: true } },
    });
    log.exec({
      type: 'setStyle',
      sheet: s,
      ranges: [{ r0: 4, c0: 0, r1: 4, c1: MAX_COLS - 1 }],
      patch: { font: { italic: true } },
    });
    const cross = wb.styles.get(sheet.styleAt(4, 3)).font;
    expect(cross.bold && cross.italic).toBe(true);
    expect(wb.styles.get(sheet.styleAt(4, 100)).font.italic).toBe(true);
  });

  it('applies a style index (Format Painter)', () => {
    const { wb, log, sheet, s } = setup();
    const st = wb.styles.patch(0, { numFmt: '0%' });
    log.exec({ type: 'setStyleIndex', sheet: s, ranges: [{ r0: 0, c0: 0, r1: 1, c1: 0 }], style: st });
    expect(sheet.cells.getStyle(1, 0)).toBe(st);
    expect(() =>
      log.exec({ type: 'setStyleIndex', sheet: s, ranges: [{ r0: 0, c0: 0, r1: 0, c1: 0 }], style: 1234 }),
    ).toThrow(ModelError);
  });
});

describe('structural edits', () => {
  it('inserts rows carrying the format of the row above', () => {
    const { wb, log, sheet, s } = setup();
    const bold = wb.styles.patch(0, { font: { bold: true } });
    log.exec({
      type: 'setCells',
      sheet: s,
      cells: [
        { row: 2, col: 0, value: 'h', style: bold },
        { row: 3, col: 0, value: 'x' },
      ],
    });
    log.exec({ type: 'insertRows', sheet: s, at: 3, count: 2 });
    expect(sheet.cells.getStyle(3, 0)).toBe(bold);
    expect(sheet.cells.getStyle(4, 0)).toBe(bold);
    expect(sheet.cells.getValue(5, 0)).toBe('x');
    log.exec({ type: 'insertRows', sheet: s, at: 0, count: 1, formatFrom: 'none' });
    expect(sheet.cells.has(0, 0)).toBe(false);
  });

  it('refuses inserts that would push data off the sheet', () => {
    const { log, s } = setup();
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: MAX_ROWS - 1, col: 0, value: 'end' }] });
    expect(() => log.exec({ type: 'insertRows', sheet: s, at: 0, count: 1 })).toThrow(/off the end/);
    expect(() => log.exec({ type: 'deleteRows', sheet: s, at: MAX_ROWS - 1, count: 2 })).toThrow(ModelError);
    expect(() => log.exec({ type: 'insertCols', sheet: s, at: 0, count: 0 })).toThrow(ModelError);
  });

  it('refuses to delete a table header row and leaves everything unchanged', () => {
    const { wb, log, s } = setup();
    log.exec({
      type: 'addTable',
      sheet: s,
      table: {
        name: 'Tbl',
        range: { r0: 2, c0: 0, r1: 5, c1: 1 },
        headerRow: true,
        totalsRow: false,
        columns: [{ name: 'A' }, { name: 'B' }],
      },
    });
    const before = stateOf(wb);
    expect(() => log.exec({ type: 'deleteRows', sheet: s, at: 2, count: 1 })).toThrow(/header row/);
    expect(stateOf(wb)).toEqual(before);
  });

  it('shifts names, conditional formats, validations and notes on other sheets too', () => {
    const wb = new Workbook({ sheets: ['A', 'B'] });
    const log = new CommandLog(wb);
    const a = wb.sheets[0] as Sheet;
    const b = wb.sheets[1] as Sheet;
    log.exec({ type: 'defineName', name: 'Input', formula: 'A!$C$5', scope: null });
    log.exec({
      type: 'setSheetMeta',
      sheet: b.id,
      key: 'cf',
      value: [
        {
          ranges: [{ r0: 0, c0: 0, r1: 0, c1: 0 }],
          rules: [{ id: 1, priority: 1, type: 'expression', formula: 'A!$C$5>0', dxf: 0 }],
        },
      ],
    });
    log.exec({
      type: 'setSheetMeta',
      sheet: a.id,
      key: 'validations',
      value: [{ id: 1, ranges: [{ r0: 9, c0: 0, r1: 9, c1: 0 }], type: 'list', formula1: '$C$5:$C$9' }],
    });
    log.exec({ type: 'insertRows', sheet: a.id, at: 0, count: 3 });
    expect(wb.names[0]?.formula).toBe('A!$C$8');
    const rule = b.cf[0]?.rules[0];
    expect(rule?.type === 'expression' && rule.formula).toBe('A!$C$8>0');
    expect(a.validations[0]?.formula1).toBe('$C$8:$C$12');
    expect(a.validations[0]?.ranges[0]).toEqual({ r0: 12, c0: 0, r1: 12, c1: 0 });
    log.undo();
    expect(wb.names[0]?.formula).toBe('A!$C$5');
    expect(a.validations[0]?.formula1).toBe('$C$5:$C$9');
  });

  it('deletes columns and restores everything on undo', () => {
    const { wb, log, sheet, s } = setup();
    log.exec({ type: 'setRange', sheet: s, row: 0, col: 0, values: [[1, 2, 3, 4]] });
    log.exec({
      type: 'setCells',
      sheet: s,
      cells: [
        { row: 1, col: 3, formula: '=SUM(A1:D1)' },
        { row: 1, col: 0, formula: '=C1' },
      ],
    });
    log.exec({ type: 'setColProps', sheet: s, at: 2, count: 1, props: { width: 20, customWidth: true } });
    const before = stateOf(wb);
    log.exec({ type: 'deleteCols', sheet: s, at: 1, count: 2 });
    expect(grid(sheet, 0, 0, 1, 1)).toEqual([
      [1, 4],
      ['=#REF!', '=SUM(A1:B1)'],
    ]);
    expect(sheet.cols.get(2)).toBeUndefined();
    log.undo();
    expect(stateOf(wb)).toEqual(before);
    log.redo();
    expect(grid(sheet, 1, 0, 1, 1)).toEqual([['=#REF!', '=SUM(A1:B1)']]);
  });
});

describe('move and copy', () => {
  it('moves cells with notes, merges and links; references follow', () => {
    const { wb, log, sheet, s } = setup();
    log.exec({
      type: 'setRange',
      sheet: s,
      row: 0,
      col: 0,
      values: [
        [1, 2],
        [3, 4],
      ],
    });
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 5, col: 5, formula: '=SUM(A1:B2)' }] });
    log.exec({ type: 'merge', sheet: s, range: { r0: 3, c0: 0, r1: 3, c1: 1 } });
    log.exec({
      type: 'setSheetMeta',
      sheet: s,
      key: 'notes',
      value: [{ row: 1, col: 1, author: 'A', text: 'n' }],
    });
    const before = stateOf(wb);
    log.exec({ type: 'moveRange', sheet: s, range: { r0: 0, c0: 0, r1: 3, c1: 1 }, to: { row: 10, col: 2 } });
    expect(grid(sheet, 10, 2, 11, 3)).toEqual([
      [1, 2],
      [3, 4],
    ]);
    expect(sheet.cells.getFormula(5, 5)).toBe('SUM(C11:D12)');
    expect(sheet.merges).toEqual([{ r0: 13, c0: 2, r1: 13, c1: 3 }]);
    expect(sheet.notes[0]).toMatchObject({ row: 11, col: 3 });
    expect(sheet.cells.has(0, 0)).toBe(false);
    log.undo();
    expect(stateOf(wb)).toEqual(before);
  });

  it('moves to another sheet and overwrites the destination', () => {
    const wb = new Workbook({ sheets: ['A', 'B'] });
    const log = new CommandLog(wb);
    const [a, b] = wb.sheets as [Sheet, Sheet];
    log.exec({
      type: 'setCells',
      sheet: a.id,
      cells: [
        { row: 0, col: 0, value: 'moved' },
        { row: 0, col: 1, formula: '=A1' },
      ],
    });
    log.exec({
      type: 'setCells',
      sheet: b.id,
      cells: [
        { row: 0, col: 0, value: 'old' },
        { row: 1, col: 0, value: 'old2' },
      ],
    });
    const before = stateOf(wb);
    log.exec({
      type: 'moveRange',
      sheet: a.id,
      range: { r0: 0, c0: 0, r1: 0, c1: 0 },
      to: { sheet: b.id, row: 0, col: 0 },
    });
    expect(b.cells.getValue(0, 0)).toBe('moved');
    expect(a.cells.getFormula(0, 1)).toBe('B!A1');
    log.undo();
    expect(stateOf(wb)).toEqual(before);
  });

  it('refuses to move part of a table or a merge', () => {
    const { log, s } = setup();
    log.exec({ type: 'merge', sheet: s, range: { r0: 0, c0: 0, r1: 1, c1: 1 } });
    expect(() =>
      log.exec({
        type: 'moveRange',
        sheet: s,
        range: { r0: 0, c0: 0, r1: 0, c1: 0 },
        to: { row: 5, col: 5 },
      }),
    ).toThrow(ModelError);
  });

  it('copies with relative reference adjustment', () => {
    const { log, sheet, s } = setup();
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 0, col: 1, formula: '=A1*$A$1+A$1' }] });
    log.exec({
      type: 'copyRange',
      sheet: s,
      range: { r0: 0, c0: 1, r1: 0, c1: 1 },
      to: { range: { r0: 3, c0: 2, r1: 3, c1: 2 } },
    });
    expect(sheet.cells.getFormula(3, 2)).toBe('B4*$A$1+B$1');
  });

  it('tiles the source over a larger destination', () => {
    const { log, sheet, s } = setup();
    log.exec({ type: 'setRange', sheet: s, row: 0, col: 0, values: [[1, 2]] });
    log.exec({
      type: 'copyRange',
      sheet: s,
      range: { r0: 0, c0: 0, r1: 0, c1: 1 },
      to: { range: { r0: 2, c0: 0, r1: 4, c1: 3 } },
    });
    expect(grid(sheet, 2, 0, 4, 3)).toEqual([
      [1, 2, 1, 2],
      [1, 2, 1, 2],
      [1, 2, 1, 2],
    ]);
  });

  it('pastes values, formulas, formats and transposed blocks', () => {
    const { wb, log, sheet, s } = setup();
    const bold = wb.styles.patch(0, { font: { bold: true } });
    log.exec({
      type: 'setCells',
      sheet: s,
      cells: [
        { row: 0, col: 0, value: 2, style: bold },
        { row: 0, col: 1, formula: '=A1*3', value: 6 },
      ],
    });
    log.exec({
      type: 'copyRange',
      sheet: s,
      range: { r0: 0, c0: 0, r1: 0, c1: 1 },
      to: { range: { r0: 5, c0: 0, r1: 5, c1: 0 } },
      what: 'values',
    });
    expect(grid(sheet, 5, 0, 5, 1)).toEqual([[2, 6]]);
    expect(sheet.cells.getStyle(5, 0)).toBe(0);
    log.exec({
      type: 'copyRange',
      sheet: s,
      range: { r0: 0, c0: 0, r1: 0, c1: 0 },
      to: { range: { r0: 6, c0: 0, r1: 6, c1: 0 } },
      what: 'formats',
    });
    expect(sheet.cells.getStyle(6, 0)).toBe(bold);
    expect(sheet.cells.getValue(6, 0)).toBeNull();
    log.exec({
      type: 'copyRange',
      sheet: s,
      range: { r0: 0, c0: 0, r1: 0, c1: 1 },
      to: { range: { r0: 10, c0: 5, r1: 10, c1: 5 } },
      transpose: true,
    });
    expect(grid(sheet, 10, 5, 11, 5)).toEqual([[2], ['=E12*3']]);
  });

  it('skips blanks when asked', () => {
    const { log, sheet, s } = setup();
    log.exec({ type: 'setRange', sheet: s, row: 0, col: 0, values: [[1, null, 3]] });
    log.exec({ type: 'setRange', sheet: s, row: 2, col: 0, values: [[9, 9, 9]] });
    log.exec({
      type: 'copyRange',
      sheet: s,
      range: { r0: 0, c0: 0, r1: 0, c1: 2 },
      to: { range: { r0: 2, c0: 0, r1: 2, c1: 0 } },
      skipBlanks: true,
    });
    expect(grid(sheet, 2, 0, 2, 2)).toEqual([[1, 9, 3]]);
  });
});

describe('sort', () => {
  const data = () => [
    ['Name', 'Score'],
    ['bob', 70],
    ['Ann', 90],
    ['carl', null],
    ['dave', 70],
    ['Eve', 'n/a'],
  ];

  it('sorts rows by one key with the header excluded, blanks last', () => {
    const { log, sheet, s } = setup();
    log.exec({ type: 'setRange', sheet: s, row: 0, col: 0, values: data() });
    log.exec({
      type: 'sort',
      sheet: s,
      range: { r0: 0, c0: 0, r1: 5, c1: 1 },
      keys: [{ index: 1 }],
      header: true,
    });
    expect(grid(sheet, 0, 0, 5, 1)).toEqual([
      ['Name', 'Score'],
      ['bob', 70],
      ['dave', 70],
      ['Ann', 90],
      ['Eve', 'n/a'],
      ['carl', null],
    ]);
  });

  it('sorts descending with blanks still last and breaks ties with a second key', () => {
    const { log, sheet, s } = setup();
    log.exec({ type: 'setRange', sheet: s, row: 0, col: 0, values: data() });
    log.exec({
      type: 'sort',
      sheet: s,
      range: { r0: 0, c0: 0, r1: 5, c1: 1 },
      keys: [
        { index: 1, descending: true },
        { index: 0, descending: true },
      ],
      header: true,
    });
    expect(grid(sheet, 1, 0, 5, 0).flat()).toEqual(['Eve', 'Ann', 'dave', 'bob', 'carl']);
  });

  it('sorts text case-insensitively and by custom lists', () => {
    const { log, sheet, s } = setup();
    log.exec({ type: 'setRange', sheet: s, row: 0, col: 0, values: [['Mar'], ['jan'], ['Feb'], ['Apr']] });
    log.exec({ type: 'sort', sheet: s, range: { r0: 0, c0: 0, r1: 3, c1: 0 }, keys: [{ index: 0 }] });
    expect(grid(sheet, 0, 0, 3, 0).flat()).toEqual(['Apr', 'Feb', 'jan', 'Mar']);
    log.exec({
      type: 'sort',
      sheet: s,
      range: { r0: 0, c0: 0, r1: 3, c1: 0 },
      keys: [{ index: 0, customList: ['Jan', 'Feb', 'Mar', 'Apr'] }],
    });
    expect(grid(sheet, 0, 0, 3, 0).flat()).toEqual(['jan', 'Feb', 'Mar', 'Apr']);
  });

  it('adjusts relative references in moved formulas and undoes exactly', () => {
    const { wb, log, sheet, s } = setup();
    log.exec({ type: 'setRange', sheet: s, row: 0, col: 0, values: [[3], [1], [2]] });
    log.exec({
      type: 'setCells',
      sheet: s,
      cells: [0, 1, 2].map((r) => ({ row: r, col: 1, formula: `=A${r + 1}*10`, value: [30, 10, 20][r] })),
    });
    const before = stateOf(wb);
    log.exec({ type: 'sort', sheet: s, range: { r0: 0, c0: 0, r1: 2, c1: 1 }, keys: [{ index: 0 }] });
    expect(grid(sheet, 0, 0, 2, 1)).toEqual([
      [1, '=A1*10'],
      [2, '=A2*10'],
      [3, '=A3*10'],
    ]);
    log.undo();
    expect(stateOf(wb)).toEqual(before);
  });

  it('sorts columns left to right', () => {
    const { log, sheet, s } = setup();
    log.exec({
      type: 'setRange',
      sheet: s,
      row: 0,
      col: 0,
      values: [
        [3, 1, 2],
        ['c', 'a', 'b'],
      ],
    });
    log.exec({
      type: 'sort',
      sheet: s,
      range: { r0: 0, c0: 0, r1: 1, c1: 2 },
      keys: [{ index: 0 }],
      byColumns: true,
    });
    expect(grid(sheet, 0, 0, 1, 2)).toEqual([
      [1, 2, 3],
      ['a', 'b', 'c'],
    ]);
  });

  it('refuses keys outside the range and partial merges', () => {
    const { log, s } = setup();
    expect(() =>
      log.exec({ type: 'sort', sheet: s, range: { r0: 0, c0: 0, r1: 3, c1: 0 }, keys: [{ index: 5 }] }),
    ).toThrow(ModelError);
    log.exec({ type: 'merge', sheet: s, range: { r0: 1, c0: 0, r1: 2, c1: 0 } });
    expect(() =>
      log.exec({ type: 'sort', sheet: s, range: { r0: 0, c0: 0, r1: 3, c1: 0 }, keys: [{ index: 0 }] }),
    ).toThrow(/merged/);
  });
});

describe('sheets', () => {
  it('adds, renames, reorders and removes sheets with undo', () => {
    const { wb, log } = setup();
    const before = stateOf(wb);
    log.exec({ type: 'addSheet' });
    log.exec({ type: 'addSheet', name: 'Data', index: 0 });
    expect(wb.sheets.map((x) => x.name)).toEqual(['Data', 'Sheet1', 'Sheet2']);
    log.exec({ type: 'renameSheet', sheet: nn(wb.sheets[0]).id, name: 'Inputs' });
    log.exec({ type: 'moveSheet', sheet: nn(wb.sheets[0]).id, index: 2 });
    expect(wb.sheets.map((x) => x.name)).toEqual(['Sheet1', 'Sheet2', 'Inputs']);
    log.exec({ type: 'removeSheet', sheet: nn(wb.sheets[1]).id });
    expect(wb.sheets.map((x) => x.name)).toEqual(['Sheet1', 'Inputs']);
    while (log.undo());
    expect(stateOf(wb)).toEqual(before);
  });

  it('keeps at least one visible sheet', () => {
    const { wb, log, s } = setup();
    expect(() => log.exec({ type: 'removeSheet', sheet: s })).toThrow(/at least one/);
    expect(() => log.exec({ type: 'setSheetMeta', sheet: s, key: 'visibility', value: 'hidden' })).toThrow(
      /visible/,
    );
    log.exec({ type: 'addSheet' });
    log.exec({ type: 'setSheetMeta', sheet: s, key: 'visibility', value: 'hidden' });
    expect(() => log.exec({ type: 'removeSheet', sheet: nn(wb.sheets[1]).id })).toThrow(/visible/);
  });

  it('restores a removed sheet with its content, tables and local names', () => {
    const { wb, log } = setup();
    log.exec({ type: 'addSheet', name: 'Gone' });
    const g = nn(wb.sheetByName('Gone'));
    log.exec({ type: 'setRange', sheet: g.id, row: 0, col: 0, values: [['H'], [1]] });
    log.exec({
      type: 'addTable',
      sheet: g.id,
      table: {
        name: 'GT',
        range: { r0: 0, c0: 0, r1: 1, c1: 0 },
        headerRow: true,
        totalsRow: false,
        columns: [{ name: 'H' }],
      },
    });
    log.exec({ type: 'defineName', name: 'Local', formula: '1', scope: g.id });
    const before = stateOf(wb);
    log.exec({ type: 'removeSheet', sheet: g.id });
    expect(wb.names).toEqual([]);
    expect(wb.findTable('GT')).toBeUndefined();
    log.undo();
    expect(stateOf(wb)).toEqual(before);
  });

  it('rejects bad names', () => {
    const { log, s } = setup();
    expect(() => log.exec({ type: 'renameSheet', sheet: s, name: 'a:b' })).toThrow(ModelError);
    log.exec({ type: 'addSheet', name: 'Two' });
    expect(() => log.exec({ type: 'renameSheet', sheet: s, name: 'two' })).toThrow(/already exists/);
  });
});

describe('rows, columns, merges, views', () => {
  it('hides rows and sets heights with exact undo', () => {
    const { wb, log, sheet, s } = setup();
    const before = stateOf(wb);
    log.exec({ type: 'setRowProps', sheet: s, at: 3, count: 4, props: { hidden: true } });
    log.exec({ type: 'setRowProps', sheet: s, at: 4, count: 1, props: { height: 30, customHeight: true } });
    expect(sheet.isRowHidden(5)).toBe(true);
    expect(sheet.rows.get(4)).toEqual({ hidden: true, height: 30, customHeight: true });
    log.exec({ type: 'setRowProps', sheet: s, at: 3, count: 4, props: { hidden: null } });
    expect(sheet.rows.get(4)).toEqual({ height: 30, customHeight: true });
    expect(() =>
      log.exec({ type: 'setRowProps', sheet: s, at: 0, count: 1, props: { height: 500 } }),
    ).toThrow(ModelError);
    expect(() =>
      log.exec({ type: 'setColProps', sheet: s, at: 0, count: 1, props: { outlineLevel: 8 } }),
    ).toThrow(ModelError);
    while (log.undo());
    expect(stateOf(wb)).toEqual(before);
  });

  it('merges keeping the upper-left value and centres with Merge & Center', () => {
    const { wb, log, sheet, s } = setup();
    log.exec({
      type: 'setRange',
      sheet: s,
      row: 0,
      col: 0,
      values: [
        [null, 'b'],
        ['c', 'd'],
      ],
    });
    const before = stateOf(wb);
    log.exec({ type: 'merge', sheet: s, range: { r0: 0, c0: 0, r1: 1, c1: 1 }, mode: 'center' });
    expect(sheet.cells.getValue(0, 0)).toBe('b');
    expect(sheet.cells.getValue(1, 1)).toBeNull();
    expect(wb.styles.get(sheet.cells.getStyle(0, 0)).alignment.horizontal).toBe('center');
    expect(sheet.merges).toEqual([{ r0: 0, c0: 0, r1: 1, c1: 1 }]);
    log.undo();
    expect(stateOf(wb)).toEqual(before);
  });

  it('merges across rows separately', () => {
    const { log, sheet, s } = setup();
    log.exec({ type: 'merge', sheet: s, range: { r0: 0, c0: 0, r1: 2, c1: 3 }, mode: 'across' });
    expect(sheet.merges).toHaveLength(3);
    log.exec({ type: 'unmerge', sheet: s, range: { r0: 1, c0: 1, r1: 1, c1: 1 } });
    expect(sheet.merges).toHaveLength(2);
    expect(() => log.exec({ type: 'merge', sheet: s, range: { r0: 0, c0: 2, r1: 0, c1: 5 } })).toThrow(
      /partly overlaps/,
    );
  });

  it('freezes panes and changes the view', () => {
    const { log, sheet, s } = setup();
    log.exec({ type: 'setFreeze', sheet: s, rows: 1, cols: 2 });
    expect(sheet.view.freeze).toEqual({ rows: 1, cols: 2 });
    log.exec({
      type: 'setSheetMeta',
      sheet: s,
      key: 'view',
      value: { ...sheet.view, zoom: 150, showGridlines: false, rightToLeft: true },
    });
    expect(sheet.view.zoom).toBe(150);
    expect(() =>
      log.exec({ type: 'setSheetMeta', sheet: s, key: 'view', value: { ...sheet.view, zoom: 5 } }),
    ).toThrow(ModelError);
    log.undo();
    log.undo();
    expect(sheet.view.freeze).toBeUndefined();
  });

  it('refuses metadata keys that need dedicated commands', () => {
    const { log, s } = setup();
    expect(() => log.exec({ type: 'setSheetMeta', sheet: s, key: 'tables' as never, value: [] })).toThrow(
      ModelError,
    );
  });
});

describe('names and tables', () => {
  it('validates names', () => {
    const { log, s } = setup();
    expect(() => log.exec({ type: 'defineName', name: 'A1', formula: '1', scope: null })).toThrow(ModelError);
    expect(() => log.exec({ type: 'defineName', name: 'has space', formula: '1', scope: null })).toThrow(
      ModelError,
    );
    expect(() => log.exec({ type: 'defineName', name: 'ok', formula: '=SUM(', scope: null })).toThrow(
      ModelError,
    );
    log.exec({
      type: 'defineName',
      name: '_xlnm.Print_Area',
      formula: 'Sheet1!$A$1:$D$20',
      scope: s,
      hidden: true,
    });
    expect(() => log.exec({ type: 'deleteName', name: 'missing', scope: null })).toThrow(ModelError);
  });

  it('validates tables', () => {
    const { log, s } = setup();
    const t = {
      name: 'T',
      range: { r0: 0, c0: 0, r1: 3, c1: 1 },
      headerRow: true,
      totalsRow: false,
      columns: [{ name: 'a' }, { name: 'b' }],
    };
    log.exec({ type: 'addTable', sheet: s, table: t });
    expect(() => log.exec({ type: 'addTable', sheet: s, table: { ...t, name: 't' } })).toThrow(
      /already exists/,
    );
    expect(() => log.exec({ type: 'addTable', sheet: s, table: { ...t, name: 'U' } })).toThrow(/overlap/);
    expect(() =>
      log.exec({
        type: 'addTable',
        sheet: s,
        table: { ...t, name: 'V', range: { r0: 10, c0: 0, r1: 12, c1: 0 } },
      }),
    ).toThrow(/column names/);
    expect(() =>
      log.exec({
        type: 'addTable',
        sheet: s,
        table: {
          ...t,
          name: 'W',
          range: { r0: 10, c0: 0, r1: 12, c1: 1 },
          columns: [{ name: 'x' }, { name: 'X' }],
        },
      }),
    ).toThrow(/Duplicate/);
    expect(() => log.exec({ type: 'defineName', name: 'T', formula: '1', scope: null })).toThrow(/table/);
    expect(() => log.exec({ type: 'merge', sheet: s, range: { r0: 1, c0: 0, r1: 1, c1: 1 } })).toThrow(
      /table/,
    );
  });

  it('writes header names into the header row', () => {
    const { log, sheet, s } = setup();
    log.exec({
      type: 'addTable',
      sheet: s,
      table: {
        name: 'T',
        range: { r0: 0, c0: 0, r1: 2, c1: 1 },
        headerRow: true,
        totalsRow: false,
        columns: [{ name: 'Qty' }, { name: 'Cost' }],
      },
    });
    expect(grid(sheet, 0, 0, 0, 1)).toEqual([['Qty', 'Cost']]);
    // Clearing a header cell gives the column a default name.
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 0, col: 1, value: null }] });
    expect(sheet.tables[0]?.columns[1]?.name).toBe('Column2');
    expect(sheet.cells.getValue(0, 1)).toBe('Column2');
    // A duplicate header name gets a number.
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 0, col: 1, value: 'qty' }] });
    expect(sheet.tables[0]?.columns[1]?.name).toBe('qty2');
  });
});

describe('workbook properties and batches', () => {
  it('sets properties with undo', () => {
    const { wb, log } = setup();
    log.exec({ type: 'setProps', patch: { calc: { mode: 'manual', iterate: true } } });
    expect(wb.props.calc).toMatchObject({ mode: 'manual', iterate: true, iterateCount: 100 });
    log.undo();
    expect(wb.props.calc.mode).toBe('auto');
    expect(() => log.exec({ type: 'setProps', patch: { activeSheet: 4 } })).toThrow(ModelError);
  });

  it('rolls back a failing batch completely', () => {
    const { wb, s } = setup();
    const before = stateOf(wb);
    const bad: Command = {
      type: 'batch',
      commands: [
        { type: 'setCells', sheet: s, cells: [{ row: 0, col: 0, value: 1 }] },
        { type: 'insertRows', sheet: s, at: 0, count: 2 },
        { type: 'setCells', sheet: s, cells: [{ row: 0, col: 0, formula: '=(' }] },
      ],
    };
    expect(() => applyCommand(wb, bad)).toThrow(ModelError);
    expect(stateOf(wb)).toEqual(before);
  });

  it('emits one commit per top-level command', () => {
    const { wb, log, s } = setup();
    const events: string[] = [];
    wb.on((e) => events.push(e.type));
    log.exec({
      type: 'batch',
      commands: [
        { type: 'setCells', sheet: s, cells: [{ row: 0, col: 0, value: 1 }] },
        { type: 'setCells', sheet: s, cells: [{ row: 1, col: 0, value: 2 }] },
      ],
    });
    expect(events.filter((e) => e === 'commit')).toHaveLength(1);
    expect(events.filter((e) => e === 'cells')).toHaveLength(2);
  });
});

describe('serialisation', () => {
  it('round-trips commands through JSON and replays them', () => {
    const { wb, log, s } = setup();
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 0, col: 0, value: { e: '#DIV/0!' } }] });
    log.exec({ type: 'insertRows', sheet: s, at: 0, count: 1 });
    log.exec({
      type: 'setStyle',
      sheet: s,
      ranges: [{ r0: 0, c0: 0, r1: 2, c1: 2 }],
      patch: { font: { bold: true } },
    });
    const wb2 = new Workbook();
    const log2 = new CommandLog(wb2);
    const replay = JSON.parse(log.serialize()) as { undo: { forward: Command }[] };
    for (const e of replay.undo) log2.exec(parseCommand(serializeCommand(e.forward)));
    expect(stateOf(wb2)).toEqual(stateOf(wb));
  });

  it('rejects unknown commands', () => {
    expect(() => parseCommand('{"type":"format-disk"}')).toThrow(ModelError);
    expect(() => parseCommand('{"type":"batch","commands":[{"type":"nope"}]}')).toThrow(ModelError);
  });

  it('restores a serialised history and can undo it', () => {
    const { wb, log, s } = setup();
    const before = stateOf(wb);
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 0, col: 0, value: 5 }] }, 'Typing');
    const json = log.serialize();
    const log2 = new CommandLog(wb);
    log2.load(json);
    expect(log2.undoLabel).toBe('Typing');
    log2.undo();
    expect(stateOf(wb)).toEqual(before);
  });
});

describe('CommandLog', () => {
  it('groups transactions into one undo step', () => {
    const { wb, log, s } = setup();
    const before = stateOf(wb);
    log.transaction('Fill', () => {
      for (let i = 0; i < 5; i++)
        log.exec({ type: 'setCells', sheet: s, cells: [{ row: i, col: 0, value: i }] });
      log.exec({ type: 'insertRows', sheet: s, at: 0, count: 1 });
    });
    expect(log.undoDepth).toBe(1);
    expect(log.undoLabel).toBe('Fill');
    log.undo();
    expect(stateOf(wb)).toEqual(before);
    log.redo();
    expect(nn(wb.sheets[0]).cells.getValue(5, 0)).toBe(4);
  });

  it('rolls back a transaction that throws', () => {
    const { wb, log, s } = setup();
    const before = stateOf(wb);
    expect(() =>
      log.transaction('Bad', () => {
        log.exec({ type: 'setCells', sheet: s, cells: [{ row: 0, col: 0, value: 1 }] });
        throw new Error('boom');
      }),
    ).toThrow('boom');
    expect(stateOf(wb)).toEqual(before);
    expect(log.canUndo).toBe(false);
  });

  it('supports nested transactions', () => {
    const { log, s } = setup();
    log.begin('outer');
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 0, col: 0, value: 1 }] });
    log.begin('inner');
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 1, col: 0, value: 2 }] });
    log.commit();
    expect(() => log.undo()).toThrow(ModelError);
    log.commit();
    expect(log.undoDepth).toBe(1);
    expect(() => log.commit()).toThrow(ModelError);
  });

  it('clears the redo stack on new edits and respects the limit', () => {
    const wb = new Workbook();
    const log = new CommandLog(wb, { limit: 3 });
    const s = nn(wb.sheets[0]).id;
    for (let i = 0; i < 5; i++)
      log.exec({ type: 'setCells', sheet: s, cells: [{ row: i, col: 0, value: i }] });
    expect(log.undoDepth).toBe(3);
    log.undo();
    expect(log.canRedo).toBe(true);
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 9, col: 0, value: 9 }] });
    expect(log.canRedo).toBe(false);
    expect(log.redo()).toBe(false);
    log.clear();
    expect(log.undo()).toBe(false);
  });

  it('notifies listeners', () => {
    const { log, s } = setup();
    const kinds: string[] = [];
    log.on((e) => kinds.push(e.kind));
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 0, col: 0, value: 1 }] });
    log.undo();
    log.redo();
    expect(kinds).toEqual(['exec', 'undo', 'redo']);
  });
});
