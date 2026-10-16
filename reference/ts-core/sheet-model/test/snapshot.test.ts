import { describe, expect, it } from 'vitest';
import {
  CommandLog,
  errorValue,
  ModelError,
  renameTableColumnInFormula,
  renameTableInFormula,
  restoreWorkbook,
  snapshotWorkbook,
  structuredToA1,
  type Table,
  Workbook,
  WorkbookCalc,
  workbookFromBytes,
  workbookToBytes,
} from '../src/index.ts';
import { nn } from './helpers.ts';

function richWorkbook(): Workbook {
  const wb = new Workbook({ sheets: ['Data', 'Report'] });
  const log = new CommandLog(wb);
  const [d, r] = wb.sheets.map((s) => s.id) as [number, number];
  log.exec({
    type: 'setRange',
    sheet: d,
    row: 0,
    col: 0,
    values: [
      ['Item', 'Qty'],
      ['Pen', 3],
      ['Cup', 4],
      [true, { e: '#N/A' }],
    ],
  });
  log.exec({
    type: 'addTable',
    sheet: d,
    table: {
      name: 'Items',
      range: { r0: 0, c0: 0, r1: 2, c1: 1 },
      headerRow: true,
      totalsRow: false,
      columns: [{ name: 'Item' }, { name: 'Qty' }],
      style: { name: 'TableStyleMedium2', showRowStripes: true },
    },
  });
  log.exec({
    type: 'setCells',
    sheet: r,
    cells: [
      { row: 0, col: 0, formula: '=SUM(Items[Qty])' },
      { row: 1, col: 0, formula: '=Data!B2*Rate' },
    ],
  });
  log.exec({ type: 'defineName', name: 'Rate', formula: '1.5', scope: null, comment: 'VAT' });
  log.exec({
    type: 'setStyle',
    sheet: d,
    ranges: [{ r0: 0, c0: 0, r1: 0, c1: 1 }],
    patch: { font: { bold: true }, fill: { type: 'pattern', pattern: 'solid', fg: { theme: 4, tint: 0.4 } } },
  });
  log.exec({ type: 'merge', sheet: r, range: { r0: 5, c0: 0, r1: 5, c1: 3 } });
  log.exec({ type: 'setFreeze', sheet: d, rows: 1, cols: 0 });
  log.exec({ type: 'setRowProps', sheet: d, at: 2, count: 1, props: { height: 24, customHeight: true } });
  log.exec({ type: 'setColProps', sheet: d, at: 0, count: 1, props: { width: 18, customWidth: true } });
  log.exec({
    type: 'setSheetMeta',
    sheet: d,
    key: 'notes',
    value: [{ row: 1, col: 0, author: 'Ann', text: 'Check stock' }],
  });
  log.exec({
    type: 'setSheetMeta',
    sheet: r,
    key: 'hyperlinks',
    value: [{ ref: { r0: 0, c0: 1, r1: 0, c1: 1 }, location: 'Data!A1', display: 'Data' }],
  });
  log.exec({
    type: 'setSheetMeta',
    sheet: d,
    key: 'cf',
    value: [
      {
        ranges: [{ r0: 1, c0: 1, r1: 2, c1: 1 }],
        rules: [
          {
            id: 1,
            priority: 1,
            type: 'dataBar',
            min: { type: 'min' },
            max: { type: 'max' },
            color: { rgb: 'FF638EC6' },
          },
        ],
      },
    ],
  });
  log.exec({ type: 'setSheetMeta', sheet: d, key: 'tabColor', value: { rgb: 'FF00B050' } });
  log.exec({ type: 'setProps', patch: { calc: { mode: 'auto', iterate: true } } });
  return wb;
}

describe('snapshots', () => {
  it('round-trips a workbook through JSON exactly', () => {
    const wb = richWorkbook();
    const snap = snapshotWorkbook(wb);
    const json = JSON.parse(JSON.stringify(snap));
    expect(json).toEqual(snap);
    const back = restoreWorkbook(json);
    expect(snapshotWorkbook(back)).toEqual(snap);
    expect(nn(back.sheets[0]).cells.getValue(3, 1)).toEqual(errorValue('#N/A'));
    expect(back.styles.get(nn(back.sheets[0]).cells.getStyle(0, 0)).font.bold).toBe(true);
    expect(nn(back.sheets[0]).rows.sizePx(2)).toBe(32);
  });

  it('is structured-clone safe', () => {
    const snap = snapshotWorkbook(richWorkbook());
    expect(structuredClone(snap)).toEqual(snap);
  });

  it('restores a workbook that calculates the same', () => {
    const wb = richWorkbook();
    const a = new WorkbookCalc(wb);
    const back = restoreWorkbook(snapshotWorkbook(wb));
    const b = new WorkbookCalc(back);
    const r = nn(wb.sheetByName('Report')).id;
    expect(a.getValue(r, 0, 0)).toBe(7);
    expect(b.getValue(r, 0, 0)).toBe(7);
    expect(b.getValue(r, 1, 0)).toBe(4.5);
  });

  it('gzips and restores', () => {
    const wb = richWorkbook();
    const bytes = workbookToBytes(wb);
    expect(bytes[0]).toBe(0x1f);
    expect(bytes[1]).toBe(0x8b);
    const back = workbookFromBytes(bytes);
    expect(snapshotWorkbook(back)).toEqual(snapshotWorkbook(wb));
  });

  it('compresses large sheets well', () => {
    const wb = new Workbook();
    const log = new CommandLog(wb);
    const values = Array.from({ length: 2000 }, (_, r) => Array.from({ length: 10 }, (_, c) => r * 10 + c));
    log.exec({ type: 'setRange', sheet: nn(wb.sheets[0]).id, row: 0, col: 0, values });
    const json = JSON.stringify(snapshotWorkbook(wb));
    const bytes = workbookToBytes(wb);
    expect(bytes.length).toBeLessThan(json.length / 3);
  });

  it('keeps id counters so new sheets and tables do not collide after restore', () => {
    const wb = richWorkbook();
    const back = restoreWorkbook(snapshotWorkbook(wb));
    const s = back.addSheet();
    expect(wb.sheets.map((x) => x.id)).not.toContain(s.id);
    expect(back.allocTableId()).toBeGreaterThan(1);
  });

  it('rejects foreign or newer data', () => {
    expect(() => restoreWorkbook({ format: 'x' } as never)).toThrow(ModelError);
    const snap = snapshotWorkbook(new Workbook());
    expect(() => restoreWorkbook({ ...snap, version: 99 })).toThrow(/newer/);
    expect(() => workbookFromBytes(new Uint8Array([1, 2, 3]))).toThrow(ModelError);
  });
});

describe('structured reference maintenance', () => {
  const table: Table = {
    id: 1,
    name: 'Sales',
    range: { r0: 1, c0: 1, r1: 5, c1: 3 },
    headerRow: true,
    totalsRow: true,
    columns: [{ name: 'Region' }, { name: 'Qty' }, { name: 'Unit Price' }],
  };

  it.each([
    ['SUM(Sales[Qty])', 'SUM(Revenue[Qty])'],
    ['sales[[#Totals],[Qty]] + Other[Qty]', 'Revenue[[#Totals],[Qty]] + Other[Qty]'],
    ['SUM( Sales[Qty] )', 'SUM( Revenue[Qty] )'],
  ])('renames tables: %s', (text, expected) => {
    expect(renameTableInFormula(text, 'Sales', 'Revenue')).toBe(expected);
  });

  it.each([
    ['SUM(Sales[Qty])', false, 'SUM(Sales[Units])'],
    ['[@Qty]*2', true, '[@Units]*2'],
    ['[@Qty]*2', false, '[@Qty]*2'],
    ['SUM(Sales[[Region]:[Qty]])', false, 'SUM(Sales[[Region]:[Units]])'],
  ] as const)('renames columns: %s', (text, inside, expected) => {
    expect(renameTableColumnInFormula(text, 'Sales', 'Qty', 'Units', inside)).toBe(expected);
  });

  it.each([
    ['SUM(Sales[Qty])', 'Data', undefined, 'SUM($C$3:$C$5)'],
    ['SUM(Sales[Qty])', 'Other', undefined, 'SUM(Data!$C$3:$C$5)'],
    ['Sales[#All]', 'Data', undefined, '$B$2:$D$6'],
    ['Sales[#Headers]', 'Data', undefined, '$B$2:$D$2'],
    ['Sales[[#Totals],[Qty]]', 'Data', undefined, '$C$6'],
    ['[@Qty]*[@[Unit Price]]', 'Data', { row: 3, col: 3 }, '$C4*$D4'],
    ['Sales[[#Data],[Region]:[Qty]]', 'Data', undefined, '$B$3:$C$5'],
    ['Sales[Missing]', 'Data', undefined, '#REF!'],
  ] as const)('converts %s to A1 (formula on %s)', (text, sheet, at, expected) => {
    expect(structuredToA1(text, table, 'Data', sheet, at)).toBe(expected);
  });
});
