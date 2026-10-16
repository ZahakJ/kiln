import { describe, expect, it } from 'vitest';
import { Engine } from '../../src/engine/engine.ts';
import { MemoryHost } from '../../src/engine/host.ts';
import { columnToLetters } from '../../src/parser/references.ts';
import { addr, fill, get, randInt, rng, sheet } from './_support.ts';

describe('incremental recalculation', () => {
  it('only recalculates what depends on a change', () => {
    const e = sheet({ A1: 1, B1: 10, A2: '=A1+1', A3: '=A2+1', B2: '=B1*2', C1: '=A3+B2' });
    expect(e.recalc().evaluated).toBe(4);
    fill(e, { B1: 20 });
    const r = e.recalc();
    expect(r.evaluated).toBe(2); // B2 and C1
    expect(get(e, 'C1')).toBe(43);
    expect(e.recalc().evaluated).toBe(0);
  });

  it('reports which formula cells changed value', () => {
    const e = sheet({ A1: 1, A2: '=A1*0', A3: '=A1+1' });
    e.recalc();
    fill(e, { A1: 5 });
    const r = e.recalc();
    expect(r.changed).toEqual([{ sheet: 'Sheet1', row: 2, col: 0 }]);
  });

  it('recalcAll re-evaluates everything', () => {
    const e = sheet({ A1: 1, A2: '=A1+1', A3: '=A2+1' });
    e.recalc();
    expect(e.recalcAll().evaluated).toBe(2);
  });

  it('formulas entered before their precedents still calculate in order', () => {
    const e = sheet({ A5: '=A4*2', A4: '=A3+1', A3: '=A2+1', A2: '=A1+1', A1: 1 });
    e.recalc();
    expect(get(e, 'A5')).toBe(8);
  });

  it('a range precedent is a single graph node', () => {
    const e = sheet();
    for (let r = 0; r < 50; r++) e.setCell('Sheet1', r, 1, { formula: '=SUM(A:A)' });
    expect(e.graphStats().rangeNodes).toBe(1);
    fill(e, { A7: 3 });
    e.recalc();
    expect(get(e, 'B50')).toBe(3);
    fill(e, { A1000: 4 });
    expect(e.recalc().evaluated).toBe(50);
    expect(get(e, 'B1')).toBe(7);
  });

  it('formulas reading a range recalculate when a formula inside the range changes', () => {
    const e = sheet({ Z1: 1, A1: '=Z1*10', A2: 5, B1: '=SUM(A1:A2)', C1: '=B1+1' });
    e.recalc();
    expect(get(e, 'C1')).toBe(16);
    fill(e, { Z1: 2 });
    e.recalc();
    expect(get(e, 'C1')).toBe(26);
  });

  it('clearing a formula updates its dependents', () => {
    const e = sheet({ A1: '=2+2', B1: '=A1*2' });
    e.recalc();
    e.clearCell('Sheet1', 0, 0);
    e.recalc();
    expect(get(e, 'B1')).toBe(0);
    expect(e.hasFormula('Sheet1', 0, 0)).toBe(false);
  });

  it('replacing a formula with a value updates dependents', () => {
    const e = sheet({ A1: '=1', B1: '=A1+1' });
    e.recalc();
    fill(e, { A1: 41 });
    e.recalc();
    expect(get(e, 'B1')).toBe(42);
    expect(e.getFormula('Sheet1', 0, 0)).toBeUndefined();
  });

  it('host-side value changes are picked up via cellChanged', () => {
    const host = new MemoryHost();
    const e = new Engine({ host });
    const s = e.addSheet('Sheet1');
    e.setCell('Sheet1', 0, 1, { formula: '=A1*3' });
    host.setCellValue(s, 0, 0, 2);
    e.cellChanged('Sheet1', 0, 0);
    e.recalc();
    expect(e.getValue('Sheet1', 0, 1)).toBe(6);
  });

  it('volatile functions recalculate every time, with their dependents', () => {
    let t = new Date(2024, 0, 15, 12, 0, 0);
    const e = new Engine({ now: () => t });
    e.addSheet('Sheet1');
    fill(e, { A1: '=NOW()', B1: '=A1+1', C1: 5, D1: '=C1*2' });
    e.recalc();
    expect(get(e, 'B1')).toBe(45307.5);
    t = new Date(2024, 0, 16, 12, 0, 0);
    const r = e.recalc();
    expect(r.evaluated).toBe(2);
    expect(get(e, 'B1')).toBe(45308.5);
  });

  it('INDIRECT and OFFSET see changes to cells they reach dynamically', () => {
    const e = sheet({ A1: 1, A2: 2, B1: '=INDIRECT("A"&C1)', C1: 1, D1: '=SUM(OFFSET(A1,0,0,C1,1))' });
    e.recalc();
    expect(get(e, 'B1')).toBe(1);
    fill(e, { C1: 2 });
    e.recalc();
    expect(get(e, 'B1')).toBe(2);
    expect(get(e, 'D1')).toBe(3);
    fill(e, { A2: 20 });
    e.recalc();
    expect(get(e, 'B1')).toBe(20);
  });

  it('throws a FormulaParseError for invalid formulas and keeps the old content', () => {
    const e = sheet({ A1: '=1+1' });
    expect(() => e.setCell('Sheet1', 0, 0, { formula: '=1+' })).toThrow(/end of formula/);
    expect(e.getFormula('Sheet1', 0, 0)).toBe('=1+1');
  });

  it('rejects out-of-range coordinates and unknown sheets', () => {
    const e = sheet();
    expect(() => e.setCell('Sheet1', -1, 0, { value: 1 })).toThrow(RangeError);
    expect(() => e.setCell('Sheet1', 0, 16_384, { value: 1 })).toThrow(RangeError);
    expect(() => e.setCell('Nope', 0, 0, { value: 1 })).toThrow(/Unknown sheet/);
  });

  it('accepts formulas without the leading =', () => {
    const e = sheet();
    e.setCell('Sheet1', 0, 0, { formula: '6*7' });
    e.recalc();
    expect(get(e, 'A1')).toBe(42);
    expect(e.getFormula('Sheet1', 0, 0)).toBe('=6*7');
  });
});

describe('recalculation order (property-style)', () => {
  interface Spec {
    refs: string[];
    coef: number[];
  }

  function model(
    cells: string[],
    values: Map<string, number>,
    formulas: Map<string, Spec>,
    cell: string,
    memo: Map<string, number>,
  ): number {
    const m = memo.get(cell);
    if (m !== undefined) return m;
    const f = formulas.get(cell);
    let v: number;
    if (f) {
      v = 1;
      f.refs.forEach((r, i) => {
        v += model(cells, values, formulas, r, memo) * (f.coef[i] as number);
      });
    } else v = values.get(cell) ?? 0;
    memo.set(cell, v);
    return v;
  }

  for (const seed of [1, 2, 3, 4, 5, 6]) {
    it(`random DAG #${seed}: results match a reference evaluator after edits`, () => {
      const r = rng(seed * 7919);
      const cells: string[] = [];
      for (let c = 0; c < 8; c++)
        for (let row = 1; row <= 12; row++) cells.push(`${columnToLetters(c)}${row}`);
      // Random topological order, so the dependency order differs from sheet order.
      const order = [...cells].sort(() => r() - 0.5);
      const values = new Map<string, number>();
      const formulas = new Map<string, Spec>();
      const e = sheet();
      const setRandom = (cell: string, i: number) => {
        if (i < 5 || r() < 0.35) {
          const v = randInt(r, -9, 9);
          values.set(cell, v);
          formulas.delete(cell);
          fill(e, { [cell]: v });
        } else {
          const n = randInt(r, 1, 3);
          const refs = Array.from({ length: n }, () => order[randInt(r, 0, i - 1)] as string);
          const coef = refs.map(() => randInt(r, -2, 2));
          formulas.set(cell, { refs, coef });
          values.delete(cell);
          fill(e, { [cell]: `=1${refs.map((ref, k) => `+${ref}*${coef[k]}`).join('')}` });
        }
      };
      // Enter cells in a random order unrelated to the dependency order.
      const entry = order.map((c, i) => [c, i] as const).sort(() => r() - 0.5);
      for (const [c, i] of entry) setRandom(c, i);
      const check = () => {
        e.recalc();
        const memo = new Map<string, number>();
        for (const c of cells) expect(get(e, c), c).toBe(model(cells, values, formulas, c, memo));
      };
      check();
      for (let round = 0; round < 15; round++) {
        for (let k = 0; k < 3; k++) {
          const i = randInt(r, 0, order.length - 1);
          setRandom(order[i] as string, i);
        }
        check();
      }
    });
  }

  it('evaluates each dirty formula exactly once per recalc on a diamond', () => {
    const e = sheet({ A1: 1, B1: '=A1+1', B2: '=A1+2', C1: '=B1+B2', D1: '=C1+B1+B2' });
    e.recalc();
    fill(e, { A1: 2 });
    expect(e.recalc().evaluated).toBe(4);
    expect(get(e, 'D1')).toBe(14);
  });
});

describe('circular references', () => {
  it('flags a cycle and shows 0, like Excel', () => {
    const e = sheet({ A1: '=B1+1', B1: '=A1+1', C1: '=A1+100' });
    const r = e.recalc();
    expect(get(e, 'A1')).toBe(0);
    expect(get(e, 'B1')).toBe(0);
    expect(get(e, 'C1')).toBe(100);
    expect(r.circular).toHaveLength(2);
    expect(e.isCircular('Sheet1', 0, 0)).toBe(true);
    expect(e.isCircular('Sheet1', 0, 2)).toBe(false);
  });

  it('detects a self reference and a long cycle', () => {
    const e = sheet({ A1: '=A1+1' });
    e.recalc();
    expect(e.circularCells()).toEqual([{ sheet: 'Sheet1', row: 0, col: 0 }]);
    const big = sheet();
    for (let i = 1; i <= 5000; i++) fill(big, { [`A${i}`]: `=A${i === 5000 ? 1 : i + 1}+1` });
    big.recalc();
    expect(big.circularCells()).toHaveLength(5000);
  });

  it('clears the flag once the cycle is broken', () => {
    const e = sheet({ A1: '=B1+1', B1: '=A1+1' });
    e.recalc();
    fill(e, { B1: 5 });
    e.recalc();
    expect(get(e, 'A1')).toBe(6);
    expect(e.circularCells()).toEqual([]);
  });

  it('a cycle through a range is detected', () => {
    const e = sheet({ A1: 1, A2: '=SUM(A1:A3)' });
    e.recalc();
    expect(e.isCircular('Sheet1', 1, 0)).toBe(true);
  });

  it('iterative calculation converges', () => {
    const e = sheet(
      { A1: '=B1+1', B1: '=A1*0.5' },
      { iterative: { enabled: true, maxIterations: 200, maxChange: 1e-9 } },
    );
    e.recalc();
    expect(get(e, 'A1') as number).toBeCloseTo(2, 6);
    expect(get(e, 'B1') as number).toBeCloseTo(1, 6);
    expect(e.isCircular('Sheet1', 0, 0)).toBe(true);
  });

  it('iterative calculation stops at the iteration limit', () => {
    const e = sheet({ A1: '=A1+1' }, { iterative: { enabled: true, maxIterations: 7 } });
    e.recalc();
    expect(get(e, 'A1')).toBe(7);
    e.recalcAll();
    expect(get(e, 'A1')).toBe(14);
  });

  it('cells downstream of a cycle are computed after it', () => {
    const e = sheet(
      { A1: '=A1*0.5+5', B1: '=A1*2' },
      { iterative: { enabled: true, maxIterations: 100, maxChange: 1e-12 } },
    );
    e.recalc();
    expect(get(e, 'B1') as number).toBeCloseTo(20, 6);
  });
});

describe('defined names', () => {
  it('evaluates names referring to ranges, constants and formulas', () => {
    const e = sheet({ A1: 1, A2: 2, A3: 3 });
    e.defineName('Data', '=Sheet1!$A$1:$A$3');
    e.defineName('Rate', '=0.5');
    e.defineName('Doubled', '=SUM(Data)*2');
    fill(e, { B1: '=SUM(Data)*Rate', B2: '=Doubled', B3: '=Missing' });
    e.recalc();
    expect(get(e, 'B1')).toBe(3);
    expect(get(e, 'B2')).toBe(12);
    expect(get(e, 'B3')).toBe('#NAME?');
  });

  it('recalculates users when a name is defined, redefined or removed', () => {
    const e = sheet({ A1: '=Rate*10' });
    e.recalc();
    expect(get(e, 'A1')).toBe('#NAME?');
    e.defineName('Rate', '=2');
    e.recalc();
    expect(get(e, 'A1')).toBe(20);
    e.defineName('Rate', '=3');
    e.recalc();
    expect(get(e, 'A1')).toBe(30);
    e.removeName('Rate');
    e.recalc();
    expect(get(e, 'A1')).toBe('#NAME?');
  });

  it('tracks cells a name refers to', () => {
    const e = sheet({ A1: 1 });
    e.defineName('Base', '=Sheet1!$A$1');
    fill(e, { B1: '=Base+1' });
    e.recalc();
    fill(e, { A1: 10 });
    e.recalc();
    expect(get(e, 'B1')).toBe(11);
  });

  it('sheet-scoped names win over workbook names on their sheet', () => {
    const e = sheet();
    e.addSheet('Other');
    e.defineName('Tax', '=0.1');
    e.defineName('Tax', '=0.2', 'Other');
    e.setCell('Sheet1', 0, 0, { formula: '=Tax' });
    e.setCell('Other', 0, 0, { formula: '=Tax' });
    e.setCell('Sheet1', 0, 1, { formula: '=Other!Tax' });
    e.recalc();
    expect(e.getValue('Sheet1', 0, 0)).toBe(0.1);
    expect(e.getValue('Other', 0, 0)).toBe(0.2);
    expect(e.getValue('Sheet1', 0, 1)).toBe(0.2);
    expect(e.listNames()).toHaveLength(2);
  });

  it('a name that refers to itself is #NAME? rather than a hang', () => {
    const e = sheet();
    e.defineName('Loop', '=Loop+1');
    expect(String(e.evaluate('=Loop'))).toBe('#NAME?');
  });

  it('validates names', () => {
    const e = sheet();
    expect(() => e.defineName('A1', '=1')).toThrow(/valid name/);
    expect(() => e.defineName('R2C3', '=1')).toThrow(/valid name/);
    expect(() => e.defineName('has space', '=1')).toThrow(/valid name/);
    expect(() => e.defineName('TRUE', '=1')).toThrow(/valid name/);
    expect(() => e.defineName('_ok.Name\\2', '=1')).not.toThrow();
  });
});

describe('sheets and 3-D references', () => {
  it('reads other sheets and 3-D ranges in workbook order', () => {
    const e = new Engine();
    for (const s of ['Jan', 'Feb', 'Mar', 'Summary']) e.addSheet(s);
    e.setCell('Jan', 0, 0, { value: 1 });
    e.setCell('Feb', 0, 0, { value: 2 });
    e.setCell('Mar', 0, 0, { value: 4 });
    e.setCell('Summary', 0, 0, { formula: '=SUM(Jan:Mar!A1)' });
    e.setCell('Summary', 0, 1, { formula: '=Feb!A1*10' });
    e.recalc();
    expect(e.getValue('Summary', 0, 0)).toBe(7);
    expect(e.getValue('Summary', 0, 1)).toBe(20);
    e.setCell('Feb', 0, 0, { value: 3 });
    e.recalc();
    expect(e.getValue('Summary', 0, 0)).toBe(8);
    e.moveSheet('Mar', 3);
    e.moveSheet('Summary', 0);
    e.recalc();
    expect(e.listSheets()).toEqual(['Summary', 'Jan', 'Feb', 'Mar']);
  });

  it('a reference to a missing sheet is #REF! until the sheet is added', () => {
    const e = sheet({ A1: '=Later!A1+1' });
    e.recalc();
    expect(get(e, 'A1')).toBe('#REF!');
    e.addSheet('Later');
    e.setCell('Later', 0, 0, { value: 41 });
    e.recalc();
    expect(get(e, 'A1')).toBe(42);
  });

  it('rejects duplicate sheet names', () => {
    const e = sheet();
    expect(() => e.addSheet('sheet1')).toThrow(/already exists/);
  });
});

describe('structured references', () => {
  function tableEngine() {
    const host = new MemoryHost();
    const e = new Engine({ host });
    const s = e.addSheet('Sheet1');
    // Table "Sales" at A1:D5: header row, 3 data rows, totals row.
    host.addTable({
      name: 'Sales',
      sheet: s,
      r0: 0,
      c0: 0,
      r1: 4,
      c1: 3,
      totalsRow: true,
      columns: ['Item', 'Qty', 'Unit Price', 'Total'],
    });
    fill(e, {
      A1: 'Item',
      B1: 'Qty',
      C1: 'Unit Price',
      D1: 'Total',
      A2: 'Pen',
      B2: 2,
      C2: 1.5,
      A3: 'Pad',
      B3: 3,
      C3: 4,
      A4: 'Ink',
      B4: 1,
      C4: 10,
    });
    return { e, host, s };
  }

  it('resolves column, special-item and this-row references', () => {
    const { e } = tableEngine();
    fill(e, {
      D2: '=[@Qty]*[@[Unit Price]]',
      D3: '=Sales[@Qty]*Sales[@[Unit Price]]',
      B5: '=SUM(Sales[Qty])',
      F1: '=COUNTA(Sales[#Headers])',
      F2: '=COUNTA(Sales[[#Headers],[#Data],[Item]])',
      F3: '=SUM(Sales[[Qty]:[Unit Price]])',
      F4: '=COUNTA(Sales[#All])',
      F5: '=SUM(Sales)',
      F6: '=Sales[[#Totals],[Qty]]',
      F7: '=Sales[Nope]',
      F8: '=Missing[Qty]',
    });
    e.recalc();
    expect(get(e, 'D2')).toBe(3);
    expect(get(e, 'D3')).toBe(12);
    expect(get(e, 'B5')).toBe(6);
    expect(get(e, 'F1')).toBe(4);
    expect(get(e, 'F2')).toBe(4);
    expect(get(e, 'F3')).toBe(21.5);
    expect(get(e, 'F4')).toBe(16);
    expect(get(e, 'F5')).toBe(36.5);
    expect(get(e, 'F6')).toBe(6);
    expect(get(e, 'F7')).toBe('#REF!');
    expect(get(e, 'F8')).toBe('#REF!');
  });

  it('recalculates when table data changes and when tables change', () => {
    const { e, host, s } = tableEngine();
    fill(e, { F1: '=SUM(Sales[Qty])' });
    e.recalc();
    expect(get(e, 'F1')).toBe(6);
    fill(e, { B3: 30 });
    e.recalc();
    expect(get(e, 'F1')).toBe(33);
    host.addTable({
      name: 'Sales',
      sheet: s,
      r0: 0,
      c0: 0,
      r1: 2,
      c1: 3,
      columns: ['Item', 'Qty', 'Unit Price', 'Total'],
    });
    e.tablesChanged();
    e.recalc();
    expect(get(e, 'F1')).toBe(32);
  });

  it('[@Col] outside the table data rows is #VALUE!, and unqualified outside a table is #REF!', () => {
    const { e } = tableEngine();
    fill(e, { E1: '=Sales[@Qty]', E2: '=[@Qty]' });
    e.recalc();
    expect(get(e, 'E1')).toBe('#VALUE!');
    expect(get(e, 'E2')).toBe('#REF!');
  });
});

describe('ad-hoc evaluation', () => {
  it('evaluates a formula at a position without storing it', () => {
    const e = sheet({ A1: 1, A2: 2, A3: 3 });
    e.recalc();
    expect(e.evaluate('=ROW()', { row: 9 })).toBe(10);
    expect(e.evaluate('=A1:A3*2', { sheet: 'Sheet1' })).toBeTruthy();
    expect(e.evaluate('=A1:A3', { row: 1, col: 3, legacy: true })).toBe(2);
    expect(e.formulaCount).toBe(0);
    expect(addr('B2')).toEqual({ row: 1, col: 1 });
  });

  it('pulls pending formulas when evaluating ad hoc', () => {
    const e = sheet({ A1: '=2*3' });
    expect(e.evaluate('=A1+1')).toBe(7);
  });
});
