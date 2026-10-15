import { describe, expect, it } from 'vitest';
import { Engine } from '../../src/engine/engine.ts';
import { MemoryHost } from '../../src/engine/host.ts';
import { columnToLetters } from '../../src/parser/references.ts';
import { addr, fill, get, plain, randInt, rng, sheet } from './_support.ts';

const formulaAt = (e: Engine, a1: string, s = 'Sheet1') => {
  const { row, col } = addr(a1);
  return e.getFormula(s, row, col);
};

describe('insert and delete rows/columns', () => {
  it('moves cells and adjusts references when inserting rows', () => {
    const e = sheet({ A1: 1, A2: 2, A3: '=A1+A2', B1: '=SUM(A1:A2)' });
    e.recalc();
    e.insertRows('Sheet1', 1, 2);
    e.recalc();
    expect(get(e, 'A1')).toBe(1);
    expect(get(e, 'A4')).toBe(2);
    expect(formulaAt(e, 'A5')).toBe('=A1+A4');
    expect(get(e, 'A5')).toBe(3);
    expect(formulaAt(e, 'B1')).toBe('=SUM(A1:A4)');
    expect(get(e, 'B1')).toBe(3);
    expect(formulaAt(e, 'A3')).toBeUndefined();
  });

  it('turns references to deleted rows into #REF!', () => {
    const e = sheet({ A1: 1, A2: 2, A3: 3, B1: '=A2*10', B4: '=SUM(A1:A3)' });
    e.recalc();
    e.deleteRows('Sheet1', 1);
    e.recalc();
    expect(formulaAt(e, 'B1')).toBe('=#REF!*10');
    expect(get(e, 'B1')).toBe('#REF!');
    expect(formulaAt(e, 'B3')).toBe('=SUM(A1:A2)');
    expect(get(e, 'B3')).toBe(4);
  });

  it('removes formulas in deleted rows and updates their dependents', () => {
    const e = sheet({ A2: '=5', B1: '=A2+1', A5: '=A2*2' });
    e.recalc();
    e.deleteRows('Sheet1', 1);
    e.recalc();
    expect(formulaAt(e, 'B1')).toBe('=#REF!+1');
    expect(formulaAt(e, 'A4')).toBe('=#REF!*2');
  });

  it('inserts and deletes columns', () => {
    const e = sheet({ A1: 2, B1: 3, C1: '=A1*B1', D1: '=SUM(A:B)' });
    e.recalc();
    e.insertCols('Sheet1', 1);
    e.recalc();
    expect(formulaAt(e, 'D1')).toBe('=A1*C1');
    expect(formulaAt(e, 'E1')).toBe('=SUM(A:C)');
    expect(get(e, 'D1')).toBe(6);
    e.deleteCols('Sheet1', 0);
    e.recalc();
    expect(formulaAt(e, 'C1')).toBe('=#REF!*B1');
    expect(formulaAt(e, 'D1')).toBe('=SUM(A:B)');
    expect(get(e, 'D1')).toBe(3);
  });

  it('adjusts references from other sheets and in defined names', () => {
    const e = sheet({ A5: 7 });
    e.addSheet('Report');
    e.setCell('Report', 0, 0, { formula: '=Sheet1!A5*2' });
    e.defineName('Seven', '=Sheet1!$A$5');
    e.setCell('Report', 1, 0, { formula: '=Seven' });
    e.recalc();
    e.insertRows('Sheet1', 0, 3);
    e.recalc();
    expect(e.getFormula('Report', 0, 0)).toBe('=Sheet1!A8*2');
    expect(e.getValue('Report', 0, 0)).toBe(14);
    expect(e.listNames()[0]?.formula).toBe('=Sheet1!$A$8');
    expect(e.getValue('Report', 1, 0)).toBe(7);
  });

  it('keeps spills working after a structural change', () => {
    const e = sheet({ A1: '=SEQUENCE(3)', C1: '=SUM(A1#)' });
    e.recalc();
    e.insertRows('Sheet1', 0);
    e.recalc();
    expect(get(e, 'A2')).toBe(1);
    expect(get(e, 'A4')).toBe(3);
    expect(get(e, 'C2')).toBe(6);
  });

  it('property: inserting rows/columns never changes computed values', () => {
    const r = rng(99);
    for (let trial = 0; trial < 8; trial++) {
      const e = sheet();
      const cells: string[] = [];
      for (let c = 0; c < 5; c++)
        for (let row = 1; row <= 10; row++) cells.push(`${columnToLetters(c)}${row}`);
      for (const c of cells) {
        if (r() < 0.6) fill(e, { [c]: randInt(r, -50, 50) });
        else {
          const a = cells[randInt(r, 0, cells.length - 1)] as string;
          const b = cells[randInt(r, 0, cells.length - 1)] as string;
          fill(e, { [c]: r() < 0.5 ? `=IFERROR(${a}*2+${b},0)` : `=SUM(${a}:${b})` });
        }
      }
      e.setIterative(false);
      e.recalc();
      const before = new Map<string, unknown>();
      for (const c of cells) before.set(c, get(e, c));
      const rowsOp = r() < 0.5;
      const at = randInt(r, 0, rowsOp ? 10 : 5);
      const n = randInt(r, 1, 3);
      if (rowsOp) e.insertRows('Sheet1', at, n);
      else e.insertCols('Sheet1', at, n);
      e.recalc();
      for (const c of cells) {
        const { row, col } = addr(c);
        const nr = rowsOp && row >= at ? row + n : row;
        const nc = !rowsOp && col >= at ? col + n : col;
        expect(plain(e.getValue('Sheet1', nr, nc)), `${c} after insert at ${at}`).toEqual(before.get(c));
      }
    }
  });
});

describe('rename, remove and move sheets', () => {
  it('renames sheets inside formulas, quoting when needed', () => {
    const e = sheet({ A1: 5 });
    e.addSheet('Other');
    e.setCell('Other', 0, 0, { formula: '=Sheet1!A1+1' });
    e.recalc();
    e.renameSheet('Sheet1', 'My Data');
    e.recalc();
    expect(e.getFormula('Other', 0, 0)).toBe("='My Data'!A1+1");
    expect(e.getValue('Other', 0, 0)).toBe(6);
    expect(e.sheetId('my data')).toBeDefined();
    expect(() => e.renameSheet('Other', 'MY DATA')).toThrow(/already exists/);
  });

  it('removing a sheet turns references into #REF!', () => {
    const e = sheet();
    e.addSheet('Temp');
    e.setCell('Temp', 0, 0, { value: 3 });
    e.setCell('Sheet1', 0, 0, { formula: '=Temp!A1*2' });
    e.recalc();
    expect(get(e, 'A1')).toBe(6);
    e.removeSheet('Temp');
    e.recalc();
    expect(e.getFormula('Sheet1', 0, 0)).toBe('=#REF!*2');
    expect(get(e, 'A1')).toBe('#REF!');
  });

  it('moveRange cuts and pastes, retargeting references', () => {
    const e = sheet({ A1: 1, A2: 2, B1: '=A1+A2', D1: '=B1*10' });
    e.recalc();
    e.moveRange('Sheet1', { r0: 0, c0: 0, r1: 1, c1: 1 }, { row: 4, col: 2 });
    e.recalc();
    expect(get(e, 'C5')).toBe(1);
    expect(get(e, 'C6')).toBe(2);
    expect(formulaAt(e, 'D5')).toBe('=C5+C6');
    expect(formulaAt(e, 'D1')).toBe('=D5*10');
    expect(get(e, 'D1')).toBe(30);
    expect(get(e, 'A1')).toBe(null);
  });

  it('moveRange to another sheet', () => {
    const e = sheet({ A1: 4, B1: '=A1*2' });
    e.addSheet('Dest');
    e.recalc();
    e.moveRange('Sheet1', { r0: 0, c0: 0, r1: 0, c1: 0 }, { row: 0, col: 0, sheet: 'Dest' });
    e.recalc();
    expect(formulaAt(e, 'B1')).toBe('=Dest!A1*2');
    expect(get(e, 'B1')).toBe(8);
  });

  it('works with a host that does not store values (read-only host)', () => {
    const backing = new MemoryHost();
    const e = new Engine({ host: { getCellValue: (s, r, c) => backing.getCellValue(s, r, c) } });
    const s = e.addSheet('S');
    backing.setCellValue(s, 0, 0, 9);
    e.setCell('S', 0, 1, { formula: '=A1+1' });
    e.recalc();
    expect(e.getValue('S', 0, 1)).toBe(10);
    expect(() => e.setCell('S', 0, 0, { value: 1 })).toThrow(/does not accept values/);
  });
});
