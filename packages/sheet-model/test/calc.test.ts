import { describe, expect, it } from 'vitest';
import { CommandLog, type Table, Workbook, WorkbookCalc } from '../src/index.ts';
import { expectEngineInSync, nn } from './helpers.ts';

function setup(sheets = ['Sheet1']) {
  const wb = new Workbook({ sheets });
  const calc = new WorkbookCalc(wb, { now: () => new Date(2026, 9, 15, 12, 0, 0) });
  const log = new CommandLog(wb);
  const id = (i = 0) => (wb.sheets[i] as { id: number }).id;
  const set = (sheet: number, row: number, col: number, v: string | number | boolean | null) => {
    if (typeof v === 'string' && v.startsWith('='))
      log.exec({ type: 'setCells', sheet, cells: [{ row, col, formula: v }] });
    else log.exec({ type: 'setCells', sheet, cells: [{ row, col, value: v }] });
  };
  return { wb, calc, log, id, set };
}

describe('formula binding', () => {
  it('inserting a row above a formula reference shifts the formula text and the dependency graph', () => {
    const { wb, calc, log, id, set } = setup();
    const s = id();
    set(s, 4, 0, 10); // A5
    set(s, 5, 0, 32); // A6
    set(s, 0, 2, '=A5+A6'); // C1
    set(s, 1, 2, '=SUM(A5:A6)'); // C2
    expect(calc.getValue(s, 0, 2)).toBe(42);
    log.exec({ type: 'insertRows', sheet: s, at: 2, count: 1 }); // above row 5
    const sheet = nn(wb.sheets[0]);
    // Formula text follows the moved cells…
    expect(sheet.cells.getFormula(0, 2)).toBe('A6+A7');
    expect(sheet.cells.getFormula(1, 2)).toBe('SUM(A6:A7)');
    expect(calc.engine.getFormula(s, 0, 2)).toBe('=A6+A7');
    // …the value is unchanged after recalc…
    calc.recalcAll();
    expect(calc.getValue(s, 0, 2)).toBe(42);
    expect(calc.getValue(s, 1, 2)).toBe(42);
    // …and the dependency graph points at the new cells: editing A6 (formerly A5) recalculates C1.
    set(s, 5, 0, 20);
    expect(calc.getValue(s, 0, 2)).toBe(52);
    expect(calc.getValue(s, 1, 2)).toBe(52);
    // The vacated row is no longer a precedent.
    set(s, 4, 0, 1000);
    expect(calc.getValue(s, 0, 2)).toBe(52);
    expectEngineInSync(wb, calc);
  });

  it('deleting a referenced row produces #REF! and undo restores the formula', () => {
    const { wb, calc, log, id, set } = setup();
    const s = id();
    set(s, 2, 0, 5);
    set(s, 0, 1, '=A3*2');
    log.exec({ type: 'deleteRows', sheet: s, at: 2, count: 1 });
    expect(nn(wb.sheets[0]).cells.getFormula(0, 1)).toBe('#REF!*2');
    expect((calc.getValue(s, 0, 1) as { code?: string })?.code).toBe('#REF!');
    log.undo();
    expect(nn(wb.sheets[0]).cells.getFormula(0, 1)).toBe('A3*2');
    expect(calc.getValue(s, 0, 1)).toBe(10);
    expectEngineInSync(wb, calc);
  });

  it('recalculates dependents incrementally', () => {
    const { calc, id, set } = setup();
    const s = id();
    set(s, 0, 0, 1);
    for (let r = 1; r < 50; r++) set(s, r, 0, `=A${r}+1`);
    expect(calc.getValue(s, 49, 0)).toBe(50);
    set(s, 0, 0, 100);
    expect(calc.lastResult?.evaluated).toBe(49);
    expect(calc.getValue(s, 49, 0)).toBe(149);
    set(s, 10, 1, 'unrelated');
    expect(calc.lastResult?.evaluated).toBe(0);
  });

  it('writes formula results back to the store as cached values', () => {
    const { wb, id, set } = setup();
    const s = id();
    set(s, 0, 0, 6);
    set(s, 0, 1, '=A1*7');
    expect(nn(wb.sheets[0]).cells.getValue(0, 1)).toBe(42);
  });

  it('spills dynamic arrays and reports #SPILL! when blocked', () => {
    const { calc, id, set } = setup();
    const s = id();
    set(s, 0, 0, '=SEQUENCE(4)');
    expect(calc.getValue(s, 3, 0)).toBe(4);
    expect(calc.getSpillRange(s, 2, 0)).toMatchObject({ row: 0, col: 0, row1: 3, col1: 0, blocked: false });
    set(s, 2, 0, 'x');
    expect((calc.getValue(s, 0, 0) as { code?: string })?.code).toBe('#SPILL!');
    set(s, 2, 0, null);
    expect(calc.getValue(s, 2, 0)).toBe(3);
    set(s, 0, 2, '=SUM(A1#)');
    expect(calc.getValue(s, 0, 2)).toBe(10);
  });

  it('resolves structured references through the tables model', () => {
    const { wb, calc, log, id } = setup();
    const s = id();
    log.exec({
      type: 'setRange',
      sheet: s,
      row: 0,
      col: 0,
      values: [
        ['Item', 'Qty', 'Price'],
        ['Pen', 3, 1.5],
        ['Book', 2, 12],
        ['Cup', 4, 5],
      ],
    });
    const table: Omit<Table, 'id'> = {
      name: 'Sales',
      range: { r0: 0, c0: 0, r1: 3, c1: 2 },
      headerRow: true,
      totalsRow: false,
      columns: [{ name: 'Item' }, { name: 'Qty' }, { name: 'Price' }],
    };
    log.exec({ type: 'addTable', sheet: s, table });
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 5, col: 0, formula: '=SUM(Sales[Qty])' }] });
    log.exec({
      type: 'setCells',
      sheet: s,
      cells: [{ row: 5, col: 1, formula: '=SUMPRODUCT(Sales[Qty],Sales[Price])' }],
    });
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 5, col: 2, formula: '=ROWS(Sales[#All])' }] });
    expect(calc.getValue(s, 5, 0)).toBe(9);
    expect(calc.getValue(s, 5, 1)).toBe(48.5);
    expect(calc.getValue(s, 5, 2)).toBe(4);
    // A calculated column with [@Col] references.
    log.exec({ type: 'insertCols', sheet: s, at: 3, count: 1 }); // after the table: unchanged
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 1, col: 3, formula: '=[@Qty]*[@Price]' }] });
    expect((calc.getValue(s, 1, 3) as { code?: string })?.code).toBe('#REF!'); // D2 is outside the table: no table to resolve against
    // Grow the table by a row: SUM picks it up.
    log.exec({ type: 'updateTable', name: 'Sales', patch: { range: { r0: 0, c0: 0, r1: 4, c1: 2 } } });
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 4, col: 1, value: 10 }] });
    // Row 5 moved? No: formulas are in row 6 and the table now ends on row 5.
    expect(calc.getValue(s, 5, 0)).toBe(19);
    // Renaming the table rewrites references.
    log.exec({ type: 'updateTable', name: 'Sales', patch: { name: 'Orders' } });
    expect(nn(wb.sheets[0]).cells.getFormula(5, 0)).toBe('SUM(Orders[Qty])');
    expect(calc.getValue(s, 5, 0)).toBe(19);
    // Renaming a column through its header cell rewrites references too.
    log.exec({ type: 'setCells', sheet: s, cells: [{ row: 0, col: 1, value: 'Units' }] });
    expect(nn(nn(nn(wb.sheets[0]).tables[0]).columns[1]).name).toBe('Units');
    expect(nn(wb.sheets[0]).cells.getFormula(5, 0)).toBe('SUM(Orders[Units])');
    expect(calc.getValue(s, 5, 0)).toBe(19);
    log.undo();
    expect(nn(wb.sheets[0]).cells.getFormula(5, 0)).toBe('SUM(Orders[Qty])');
    expect(nn(nn(nn(wb.sheets[0]).tables[0]).columns[1]).name).toBe('Qty');
    expectEngineInSync(wb, calc);
  });

  it('evaluates [@Col] inside a table and converts references when the table becomes a range', () => {
    const { wb, calc, log, id } = setup();
    const s = id();
    log.exec({
      type: 'setRange',
      sheet: s,
      row: 0,
      col: 0,
      values: [
        ['Qty', 'Price', 'Total'],
        [2, 3, null],
        [4, 5, null],
      ],
    });
    log.exec({
      type: 'addTable',
      sheet: s,
      table: {
        name: 'T',
        range: { r0: 0, c0: 0, r1: 2, c1: 2 },
        headerRow: true,
        totalsRow: false,
        columns: [{ name: 'Qty' }, { name: 'Price' }, { name: 'Total' }],
      },
    });
    log.exec({
      type: 'setCells',
      sheet: s,
      cells: [
        { row: 1, col: 2, formula: '=[@Qty]*[@Price]' },
        { row: 2, col: 2, formula: '=[@Qty]*[@Price]' },
        { row: 4, col: 0, formula: '=SUM(T[Total])' },
      ],
    });
    expect(calc.getValue(s, 1, 2)).toBe(6);
    expect(calc.getValue(s, 2, 2)).toBe(20);
    expect(calc.getValue(s, 4, 0)).toBe(26);
    log.exec({ type: 'removeTable', name: 'T' });
    const cells = nn(wb.sheets[0]).cells;
    expect(cells.getFormula(1, 2)).toBe('$A2*$B2');
    expect(cells.getFormula(4, 0)).toBe('SUM($C$2:$C$3)');
    expect(calc.getValue(s, 4, 0)).toBe(26);
    log.undo();
    expect(cells.getFormula(4, 0)).toBe('SUM(T[Total])');
    expect(calc.getValue(s, 4, 0)).toBe(26);
    expectEngineInSync(wb, calc);
  });

  it('uses defined names at workbook and sheet scope', () => {
    const { wb, calc, log, id, set } = setup(['Data', 'Calc']);
    const data = id(0);
    const c = id(1);
    set(data, 0, 0, 0.2);
    log.exec({ type: 'defineName', name: 'Rate', formula: '=Data!$A$1', scope: null });
    log.exec({ type: 'defineName', name: 'Rate', formula: '0.5', scope: c });
    set(data, 1, 0, '=Rate*100');
    set(c, 0, 0, '=Rate*100');
    expect(calc.getValue(data, 1, 0)).toBe(20);
    expect(calc.getValue(c, 0, 0)).toBe(50); // the local name wins
    set(data, 0, 0, 0.3);
    expect(calc.getValue(data, 1, 0)).toBe(30);
    // Inserting rows on Data shifts the name.
    log.exec({ type: 'insertRows', sheet: data, at: 0, count: 2 });
    expect(wb.findName('Rate', null)?.formula).toBe('Data!$A$3');
    expect(calc.getValue(data, 3, 0)).toBe(30);
    log.exec({ type: 'deleteName', name: 'Rate', scope: c });
    expect(calc.getValue(c, 0, 0)).toBe(30);
    expectEngineInSync(wb, calc);
  });

  it('follows sheet renames, moves and removal', () => {
    const { wb, calc, log, id, set } = setup(['A', 'B']);
    const a = id(0);
    const b = id(1);
    set(b, 0, 0, 7);
    set(a, 0, 0, '=B!A1*2');
    log.exec({ type: 'renameSheet', sheet: b, name: 'Budget 2026' });
    expect(nn(wb.sheets[0]).cells.getFormula(0, 0)).toBe("'Budget 2026'!A1*2");
    expect(calc.getValue(a, 0, 0)).toBe(14);
    log.exec({ type: 'moveSheet', sheet: b, index: 0 });
    expect(wb.sheets.map((s) => s.name)).toEqual(['Budget 2026', 'A']);
    log.exec({ type: 'removeSheet', sheet: b });
    expect(nn(wb.sheetById(a)).cells.getFormula(0, 0)).toBe('#REF!*2');
    expect((calc.getValue(a, 0, 0) as { code?: string })?.code).toBe('#REF!');
    log.undo();
    expect(nn(wb.sheetById(a)).cells.getFormula(0, 0)).toBe("'Budget 2026'!A1*2");
    expect(calc.getValue(a, 0, 0)).toBe(14);
    log.undo();
    log.undo();
    expect(nn(wb.sheetById(a)).cells.getFormula(0, 0)).toBe('B!A1*2');
    expect(calc.getValue(a, 0, 0)).toBe(14);
    expectEngineInSync(wb, calc);
  });

  it('binds formulas to a sheet added later and keeps them on undo', () => {
    const { wb, calc, log, id, set } = setup();
    const s = id();
    set(s, 0, 0, '=Later!A1+1');
    expect((calc.getValue(s, 0, 0) as { code?: string })?.code).toBe('#REF!');
    log.exec({ type: 'addSheet', name: 'Later' });
    const later = nn(wb.sheetByName('Later')).id;
    set(later, 0, 0, 41);
    expect(calc.getValue(s, 0, 0)).toBe(42);
    log.undo();
    log.undo();
    expect(wb.sheets.length).toBe(1);
    expect(nn(wb.sheets[0]).cells.getFormula(0, 0)).toBe('Later!A1+1');
    expectEngineInSync(wb, calc);
  });

  it('honours manual calculation mode', () => {
    const { calc, log, id, set } = setup();
    const s = id();
    set(s, 0, 0, 1);
    set(s, 0, 1, '=A1+1');
    log.exec({ type: 'setProps', patch: { calc: { mode: 'manual' } } });
    expect(calc.mode).toBe('manual');
    set(s, 0, 0, 10);
    expect(calc.isDirty).toBe(true);
    expect(calc.getValue(s, 0, 1)).toBe(2); // stale until F9
    calc.recalc();
    expect(calc.getValue(s, 0, 1)).toBe(11);
    expect(calc.isDirty).toBe(false);
  });

  it('recalculates one sheet on demand', () => {
    const { calc, log, id, set } = setup(['A', 'B']);
    const a = id(0);
    set(a, 0, 0, '=RAND()');
    log.exec({ type: 'setProps', patch: { calc: { mode: 'manual' } } });
    const before = calc.getValue(a, 0, 0);
    const res = calc.recalcSheet(a);
    expect(res.evaluated).toBeGreaterThan(0);
    expect(calc.getValue(a, 0, 0)).not.toBe(before);
  });

  it('switches date systems by rebuilding the engine', () => {
    const { calc, log, id, set } = setup();
    const s = id();
    set(s, 0, 0, '=DATE(2023,3,15)');
    expect(calc.getValue(s, 0, 0)).toBe(45000);
    log.exec({ type: 'setProps', patch: { date1904: true } });
    expect(calc.getValue(s, 0, 0)).toBe(45000 - 1462);
    expect(calc.getDisplay(s, 0, 0).text).toBe('43538');
  });

  it('formats displayed values with the cell number format', () => {
    const { calc, log, id, set } = setup();
    const s = id();
    set(s, 0, 0, '=1/3');
    log.exec({
      type: 'setStyle',
      sheet: s,
      ranges: [{ r0: 0, c0: 0, r1: 0, c1: 0 }],
      patch: { numFmt: '0.00%' },
    });
    expect(calc.getDisplay(s, 0, 0).text).toBe('33.33%');
    set(s, 1, 0, '=DATE(2026,10,15)');
    log.exec({
      type: 'setStyle',
      sheet: s,
      ranges: [{ r0: 1, c0: 0, r1: 1, c1: 0 }],
      patch: { numFmt: 'yyyy-mm-dd' },
    });
    expect(calc.getDisplay(s, 1, 0).text).toBe('2026-10-15');
  });

  it('skips rows hidden by a filter in SUBTOTAL after the layout changes', () => {
    const { calc, log, id, set } = setup();
    const s = id();
    for (let r = 0; r < 4; r++) set(s, r, 0, r + 1);
    set(s, 5, 0, '=SUBTOTAL(9,A1:A4)');
    expect(calc.getValue(s, 5, 0)).toBe(10);
    log.exec({ type: 'setRowProps', sheet: s, at: 1, count: 1, props: { hidden: true, filtered: true } });
    expect(calc.getValue(s, 5, 0)).toBe(8);
    log.undo();
    expect(calc.getValue(s, 5, 0)).toBe(10);
  });

  it('explains a cell formula step by step', () => {
    const { calc, id, set } = setup();
    const s = id();
    set(s, 0, 0, 4);
    set(s, 0, 1, '=IF(A1>2,A1*10,0)');
    const step = calc.explain(s, 0, 1);
    expect(step.value).toBe(40);
    expect(step.children.length).toBeGreaterThan(0);
    expect(() => calc.explain(s, 5, 5)).toThrow();
  });

  it('keeps moving ranges consistent between model and engine', () => {
    const { wb, calc, log, id, set } = setup();
    const s = id();
    set(s, 0, 0, 3);
    set(s, 1, 0, '=A1*2');
    set(s, 0, 2, '=A2+A1');
    log.exec({ type: 'moveRange', sheet: s, range: { r0: 0, c0: 0, r1: 1, c1: 0 }, to: { row: 5, col: 4 } });
    const cells = nn(wb.sheets[0]).cells;
    expect(cells.getFormula(6, 4)).toBe('E6*2');
    expect(cells.getFormula(0, 2)).toBe('E7+E6');
    expect(calc.getValue(s, 0, 2)).toBe(9);
    expectEngineInSync(wb, calc);
    log.undo();
    expect(cells.getFormula(0, 2)).toBe('A2+A1');
    expect(cells.getFormula(1, 0)).toBe('A1*2');
    expect(cells.has(6, 4)).toBe(false);
    expect(calc.getValue(s, 0, 2)).toBe(9);
    expectEngineInSync(wb, calc);
  });

  it('marks unparsable formulas from files as errors instead of failing', () => {
    const wb = new Workbook();
    nn(wb.sheets[0]).cells.setFormula(0, 0, 'SUM(');
    nn(wb.sheets[0]).cells.setFormula(0, 1, '1+1');
    const calc = new WorkbookCalc(wb);
    expect(calc.errors.size).toBe(1);
    expect((calc.getValue(nn(wb.sheets[0]).id, 0, 0) as { code?: string })?.code).toBe('#NAME?');
    expect(calc.getValue(nn(wb.sheets[0]).id, 0, 1)).toBe(2);
  });
});
