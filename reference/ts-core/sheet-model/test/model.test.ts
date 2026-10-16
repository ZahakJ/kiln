import { describe, expect, it } from 'vitest';
import {
  BUILTIN_NUMFMTS,
  builtinNumFmtId,
  charsToColWidth,
  colWidthToChars,
  colWidthToPx,
  createColAxis,
  createRowAxis,
  DEFAULT_FONT_METRICS,
  defaultCellStyle,
  defaultColWidthFor,
  formatRange,
  MAX_COLS,
  MAX_ROWS,
  ModelError,
  parseCellRef,
  parseRange,
  parseSqref,
  pxToColWidth,
  rgb,
  rowHeightToPx,
  Sheet,
  StyleTable,
  Workbook,
} from '../src/index.ts';
import { nn } from './helpers.ts';

describe('A1 ranges', () => {
  it.each([
    ['A1', { r0: 0, c0: 0, r1: 0, c1: 0 }],
    ['$B$3', { r0: 2, c0: 1, r1: 2, c1: 1 }],
    ['C3:A1', { r0: 0, c0: 0, r1: 2, c1: 2 }],
    ['b2:d10', { r0: 1, c0: 1, r1: 9, c1: 3 }],
    ['A:C', { r0: 0, c0: 0, r1: MAX_ROWS - 1, c1: 2 }],
    ['2:5', { r0: 1, c0: 0, r1: 4, c1: MAX_COLS - 1 }],
    ['XFD1048576', { r0: MAX_ROWS - 1, c0: MAX_COLS - 1, r1: MAX_ROWS - 1, c1: MAX_COLS - 1 }],
  ])('parses %s', (text, expected) => {
    expect(parseRange(text)).toEqual(expected);
  });

  it.each(['', 'A0', 'XFE1', 'A1048577', 'A1:B2:C3', 'Sheet1!A1', '1A'])('rejects %j', (text) => {
    expect(parseRange(text)).toBeUndefined();
  });

  it('prints ranges in their shortest form', () => {
    expect(formatRange({ r0: 0, c0: 0, r1: 0, c1: 0 })).toBe('A1');
    expect(formatRange({ r0: 0, c0: 0, r1: 9, c1: 2 }, true)).toBe('$A$1:$C$10');
    expect(formatRange(parseRange('B:D') as never)).toBe('B:D');
    expect(formatRange(parseRange('3:4') as never)).toBe('3:4');
    expect(parseCellRef('AA10')).toEqual({ row: 9, col: 26 });
  });

  it('parses and validates sqref lists', () => {
    expect(parseSqref('A1:B2 D4')).toHaveLength(2);
    expect(() => parseSqref('A1 nope')).toThrow(ModelError);
  });
});

describe('StyleTable', () => {
  it('starts with the Normal style at index 0 and the two mandatory fills', () => {
    const t = new StyleTable();
    expect(t.count).toBe(1);
    expect(t.get(0).font.name).toBe('Calibri');
    expect(t.get(0).font.size).toBe(11);
    expect(t.get(0).numFmt).toBe('General');
    expect(t.toJSON().fills).toEqual([
      { type: 'pattern', pattern: 'none' },
      { type: 'pattern', pattern: 'gray125' },
    ]);
  });

  it('deduplicates equal formats', () => {
    const t = new StyleTable();
    const a = t.patch(0, { font: { bold: true } });
    const b = t.patch(0, { font: { bold: true } });
    expect(a).toBe(b);
    expect(a).not.toBe(0);
    // Toggling back off returns to Normal.
    expect(t.patch(a, { font: { bold: false } })).toBe(0);
    expect(t.count).toBe(2);
  });

  it('treats default-valued members as absent when hashing', () => {
    const t = new StyleTable();
    expect(t.patch(0, { alignment: { horizontal: 'general', vertical: 'bottom', wrapText: false } })).toBe(0);
    expect(t.patch(0, { protection: { locked: true, hidden: false } })).toBe(0);
    expect(t.patch(0, { border: { left: { style: 'none' } } })).toBe(0);
  });

  it('merges patches field by field and removes with null', () => {
    const t = new StyleTable();
    let s = t.patch(0, { font: { bold: true, color: rgb('FF0000') }, numFmt: '0.00' });
    s = t.patch(s, { font: { italic: true } });
    const st = t.get(s);
    expect(st.font).toMatchObject({ bold: true, italic: true, color: { rgb: 'FFFF0000' } });
    expect(st.numFmt).toBe('0.00');
    s = t.patch(s, {
      border: { top: { style: 'thin' }, bottom: { style: 'double', color: rgb('#0000ff') } },
    });
    s = t.patch(s, { border: { top: null } });
    expect(t.get(s).border).toEqual({ bottom: { style: 'double', color: { rgb: 'FF0000FF' } } });
    s = t.patch(s, { alignment: { horizontal: 'center', wrapText: true, indent: 2 } });
    s = t.patch(s, { alignment: { indent: null } });
    expect(t.get(s).alignment).toEqual({ horizontal: 'center', wrapText: true });
    s = t.patch(s, { fill: { type: 'pattern', pattern: 'solid', fg: rgb('FFFF00') } });
    expect(t.get(s).fill).toMatchObject({ pattern: 'solid' });
    s = t.patch(s, { fill: null });
    expect(t.get(s).fill).toEqual({ type: 'pattern', pattern: 'none' });
  });

  it('shares font, fill and border components between formats', () => {
    const t = new StyleTable();
    const a = t.patch(0, { font: { bold: true }, numFmt: '0' });
    const b = t.patch(0, { font: { bold: true }, numFmt: '0.0' });
    expect(t.getXf(a).font).toBe(t.getXf(b).font);
    expect(t.toJSON().fonts).toHaveLength(2);
  });

  it('interns differential formats', () => {
    const t = new StyleTable();
    const a = t.internDxf({
      font: { color: rgb('9C0006') },
      fill: { type: 'pattern', pattern: 'solid', bg: rgb('FFC7CE') },
    });
    const b = t.internDxf({
      fill: { type: 'pattern', pattern: 'solid', bg: rgb('FFC7CE') },
      font: { color: rgb('9C0006') },
    });
    expect(a).toBe(b);
    expect(t.dxfCount).toBe(1);
  });

  it('round-trips through JSON keeping indices', () => {
    const t = new StyleTable();
    const s = t.patch(0, { font: { underline: 'double' }, protection: { locked: false } });
    const u = StyleTable.fromParts(JSON.parse(JSON.stringify(t.toJSON())));
    expect(u.get(s)).toEqual(t.get(s));
    expect(u.patch(0, { font: { underline: 'double' }, protection: { locked: false } })).toBe(s);
  });

  it('keeps duplicate entries read from files', () => {
    const json = new StyleTable().toJSON();
    json.xfs.push({ ...nn(json.xfs[0]) });
    const t = StyleTable.fromParts(json);
    expect(t.count).toBe(2);
    expect(t.get(1)).toEqual(t.get(0));
  });

  it('rejects broken component references and unknown indices', () => {
    const json = new StyleTable().toJSON();
    json.xfs.push({ ...nn(json.xfs[0]), font: 5 });
    expect(() => StyleTable.fromParts(json)).toThrow(ModelError);
    expect(() => new StyleTable().get(3)).toThrow(ModelError);
    expect(() => rgb('nothex')).toThrow(ModelError);
  });

  it('knows the built-in number formats', () => {
    expect(BUILTIN_NUMFMTS[14]).toBe('m/d/yyyy');
    expect(builtinNumFmtId('0.00%')).toBe(10);
    expect(builtinNumFmtId('0.000')).toBeUndefined();
    expect(defaultCellStyle().numFmt).toBe('General');
  });
});

describe('row and column geometry', () => {
  it('converts Excel column widths', () => {
    expect(charsToColWidth(8.43, 7)).toBe(9.140625);
    expect(colWidthToPx(9.140625, 7)).toBe(64);
    expect(colWidthToChars(9.140625, 7)).toBe(8.43);
    expect(pxToColWidth(64, 7)).toBe(9.140625);
    expect(defaultColWidthFor(8, 7)).toBe(9.140625);
    expect(colWidthToPx(defaultColWidthFor(8, 8), 8)).toBe(72);
    expect(colWidthToPx(charsToColWidth(20, 7), 7)).toBe(145);
    expect(rowHeightToPx(15)).toBe(20);
    expect(rowHeightToPx(12.75)).toBe(17);
  });

  it('derives the maximum digit width from the font', () => {
    expect(DEFAULT_FONT_METRICS.maxDigitWidth({ name: 'Calibri', size: 11 })).toBe(7);
    expect(DEFAULT_FONT_METRICS.maxDigitWidth({ name: 'Arial', size: 10 })).toBe(7);
    expect(DEFAULT_FONT_METRICS.maxDigitWidth({ name: 'Arial', size: 11 })).toBe(8);
    expect(DEFAULT_FONT_METRICS.maxDigitWidth({ name: 'Unknown Font', size: 11 })).toBeGreaterThan(0);
  });

  it('gives Excel default sizes: 20 px rows and 64 px columns', () => {
    const s = new Sheet(1, 'Sheet1', 7);
    expect(s.rows.sizePx(0)).toBe(20);
    expect(s.cols.sizePx(0)).toBe(64);
    expect(s.defaultColWidth).toBe(9.140625);
    expect(s.rows.offsetPx(10)).toBe(200);
  });

  it('computes offsets with custom and hidden sizes', () => {
    const rows = createRowAxis();
    rows.set(2, { height: 30, customHeight: true }); // 40 px
    rows.set(5, { hidden: true });
    expect(rows.sizePx(2)).toBe(40);
    expect(rows.sizePx(5)).toBe(0);
    expect(rows.offsetPx(3)).toBe(20 + 20 + 40);
    expect(rows.offsetPx(7)).toBe(6 * 20 + 20 - 20 + 0 + 20); // rows 0–6: 20,20,40,20,20,0,20
    expect(rows.indexAtPx(0)).toBe(0);
    expect(rows.indexAtPx(45)).toBe(2);
    expect(rows.indexAtPx(79)).toBe(2);
    expect(rows.indexAtPx(80)).toBe(3);
    // The hidden row 5 is skipped.
    expect(rows.indexAtPx(rows.offsetPx(5))).toBe(6);
    expect(rows.totalPx()).toBe(MAX_ROWS * 20 + 20 - 20);
  });

  it('shifts properties for inserts and deletes', () => {
    const cols = createColAxis(9.140625, 7);
    cols.set(3, { width: 20 });
    cols.set(10, { hidden: true });
    cols.shift(2, 2);
    expect(cols.get(5)?.width).toBe(20);
    expect(cols.isHidden(12)).toBe(true);
    cols.shift(4, -3);
    expect(cols.get(5)).toBeUndefined();
    expect(cols.isHidden(9)).toBe(true);
    expect(cols.count).toBe(1);
  });

  it('tracks outline levels and patches', () => {
    const rows = createRowAxis();
    rows.patch(4, { outlineLevel: 2 });
    rows.patch(4, { collapsed: true });
    expect(rows.outlineLevel(4)).toBe(2);
    expect(rows.get(4)).toEqual({ outlineLevel: 2, collapsed: true });
    rows.set(4, {});
    expect(rows.get(4)).toBeUndefined();
  });

  it('refreshes column pixels when the default font changes', () => {
    const wb = new Workbook();
    const s = wb.sheets[0] as Sheet;
    expect(s.cols.sizePx(0)).toBe(64);
    wb.refreshMetrics({ maxDigitWidth: () => 8 });
    expect(s.cols.sizePx(0)).toBe(colWidthToPx(s.defaultColWidth, 8));
  });
});

describe('Workbook and Sheet', () => {
  it('creates Sheet1 by default and names new sheets like Excel', () => {
    const wb = new Workbook();
    expect(wb.sheets.map((s) => s.name)).toEqual(['Sheet1']);
    wb.addSheet();
    wb.addSheet();
    expect(wb.sheets.map((s) => s.name)).toEqual(['Sheet1', 'Sheet2', 'Sheet3']);
    expect(new Set(wb.sheets.map((s) => s.id)).size).toBe(3);
  });

  it.each(['', 'a'.repeat(32), 'a/b', 'x[1]', "'quoted", 'History', 'what?'])(
    'rejects the sheet name %j',
    (n) => {
      expect(() => new Workbook().addSheet(n)).toThrow(ModelError);
    },
  );

  it('rejects duplicate sheet names case-insensitively', () => {
    const wb = new Workbook();
    expect(() => wb.addSheet('SHEET1')).toThrow(/already exists/);
  });

  it('validates merges', () => {
    const s = new Sheet(1, 'S', 7);
    s.addMerge({ r0: 0, c0: 0, r1: 1, c1: 1 });
    expect(() => s.addMerge({ r0: 1, c0: 1, r1: 2, c1: 2 })).toThrow(/overlaps/);
    expect(() => s.addMerge({ r0: 5, c0: 5, r1: 5, c1: 5 })).toThrow(/two cells/);
    expect(s.mergeAt(1, 0)).toEqual({ r0: 0, c0: 0, r1: 1, c1: 1 });
    expect(s.expandToMerges({ r0: 1, c0: 1, r1: 3, c1: 3 })).toEqual({ r0: 0, c0: 0, r1: 3, c1: 3 });
    expect(s.removeMerges({ r0: 1, c0: 1, r1: 1, c1: 1 })).toHaveLength(1);
    expect(s.merges).toEqual([]);
  });

  it('resolves effective styles from cell, row and column formats', () => {
    const s = new Sheet(1, 'S', 7);
    s.cols.set(2, { style: 5 });
    s.rows.set(3, { style: 6 });
    s.cells.setStyle(3, 2, 7);
    expect(s.styleAt(3, 2)).toBe(7);
    expect(s.styleAt(3, 1)).toBe(6);
    expect(s.styleAt(4, 2)).toBe(5);
    expect(s.styleAt(4, 4)).toBe(0);
  });

  it('looks up validation, conditional formats, notes and links per cell', () => {
    const s = new Sheet(1, 'S', 7);
    s.validations.push({ id: 1, ranges: [{ r0: 0, c0: 0, r1: 9, c1: 0 }], type: 'whole' });
    s.validations.push({ id: 2, ranges: [{ r0: 5, c0: 0, r1: 5, c1: 0 }], type: 'list', formula1: '"a,b"' });
    s.cf.push({
      ranges: [{ r0: 0, c0: 0, r1: 9, c1: 9 }],
      rules: [
        { id: 2, priority: 2, type: 'containsBlanks', dxf: 0 },
        { id: 1, priority: 1, type: 'expression', formula: 'A1>5', dxf: 0, stopIfTrue: true },
      ],
    });
    s.notes.push({ row: 1, col: 1, author: 'Ann', text: 'hi' });
    s.hyperlinks.push({ ref: { r0: 2, c0: 2, r1: 2, c1: 3 }, target: 'https://example.org' });
    expect(s.validationAt(5, 0)?.id).toBe(2);
    expect(s.validationAt(4, 0)?.id).toBe(1);
    expect(s.validationAt(4, 1)).toBeUndefined();
    expect(s.cfRulesAt(3, 3).map((r) => r.id)).toEqual([1, 2]);
    expect(s.noteAt(1, 1)?.text).toBe('hi');
    expect(s.hyperlinkAt(2, 3)?.target).toBe('https://example.org');
  });

  it('shifts all metadata on structural edits', () => {
    const s = new Sheet(1, 'S', 7);
    s.merges.push({ r0: 4, c0: 0, r1: 5, c1: 1 });
    s.notes.push({ row: 6, col: 0, author: 'a', text: 'n' });
    s.cf.push({ ranges: [{ r0: 2, c0: 0, r1: 8, c1: 0 }], rules: [] });
    s.view.freeze = { rows: 3, cols: 0 };
    s.print.rowBreaks = [10];
    s.rows.set(7, { height: 30 });
    s.cells.setValue(9, 0, 'x');
    s.applyStructural({ kind: 'insertRows', at: 1, count: 2 });
    expect(s.merges).toEqual([{ r0: 6, c0: 0, r1: 7, c1: 1 }]);
    expect(s.notes[0]?.row).toBe(8);
    expect(s.cf[0]?.ranges).toEqual([{ r0: 4, c0: 0, r1: 10, c1: 0 }]);
    expect(s.view.freeze).toEqual({ rows: 5, cols: 0 });
    expect(s.print.rowBreaks).toEqual([12]);
    expect(s.rows.get(9)?.height).toBe(30);
    expect(s.cells.getValue(11, 0)).toBe('x');
  });
});
