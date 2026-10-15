/**
 * Dynamic arrays: spilling, #SPILL! on collisions, the # spill-range operator, and how spills
 * interact with incremental recalculation.
 */
import { describe, expect, it } from 'vitest';
import { addr, fill, get, plain, sheet } from './_support.ts';

describe('dynamic-array spill', () => {
  it('spills an array result into neighbouring cells', () => {
    const e = sheet({ A1: '=SEQUENCE(3,2)' });
    e.recalc();
    expect(get(e, 'A1')).toBe(1);
    expect(get(e, 'B1')).toBe(2);
    expect(get(e, 'A3')).toBe(5);
    expect(get(e, 'B3')).toBe(6);
    expect(get(e, 'A4')).toBe(null);
    expect(e.getSpillRange('Sheet1', 0, 0)).toEqual({
      sheet: 'Sheet1',
      row: 0,
      col: 0,
      row1: 2,
      col1: 1,
      blocked: false,
    });
    expect(e.getSpillRange('Sheet1', 2, 1)).toMatchObject({ row: 0, col: 0 });
    expect(plain(e.getResult('Sheet1', 0, 0))).toEqual([
      [1, 2],
      [3, 4],
      [5, 6],
    ]);
  });

  it('spills ranges, array constants and lifted operations', () => {
    const e = sheet({ A1: 1, A2: 2, A3: 3, C1: '=A1:A3*10', D1: '={1,2,3}', D5: '=TRANSPOSE(A1:A3)' });
    e.recalc();
    expect([get(e, 'C1'), get(e, 'C2'), get(e, 'C3')]).toEqual([10, 20, 30]);
    expect([get(e, 'D1'), get(e, 'E1'), get(e, 'F1')]).toEqual([1, 2, 3]);
    expect([get(e, 'D5'), get(e, 'E5'), get(e, 'F5')]).toEqual([1, 2, 3]);
  });

  it('shows blanks inside a spilled range as 0', () => {
    const e = sheet({ A1: 1, A3: 3, C1: '=A1:A3' });
    e.recalc();
    expect(get(e, 'C2')).toBe(0);
  });

  it('a 1×1 array result does not spill', () => {
    const e = sheet({ A1: '=SEQUENCE(1)' });
    e.recalc();
    expect(get(e, 'A1')).toBe(1);
    expect(e.getSpillRange('Sheet1', 0, 0)).toBeUndefined();
  });

  it('cells reading inside a spill see the spilled values', () => {
    const e = sheet({ A1: '=SEQUENCE(5)', C1: '=A4*100', C2: '=SUM(A1:A5)' });
    e.recalc();
    expect(get(e, 'C1')).toBe(400);
    expect(get(e, 'C2')).toBe(15);
  });

  it('dependents of spilled cells update when the anchor changes', () => {
    const e = sheet({ B1: 3, A1: '=SEQUENCE(B1)', C1: '=SUM(A1:A10)', D1: '=A3' });
    e.recalc();
    expect(get(e, 'C1')).toBe(6);
    expect(get(e, 'D1')).toBe(3);
    fill(e, { B1: 5 });
    e.recalc();
    expect(get(e, 'C1')).toBe(15);
    fill(e, { B1: 2 });
    e.recalc();
    expect(get(e, 'C1')).toBe(3);
    expect(get(e, 'D1')).toBe(0);
    expect(get(e, 'A3')).toBe(null);
  });

  it('a dependent evaluated before a growing spill is corrected in the same recalc', () => {
    // D1 reads A4, which only becomes part of the spill when it grows.
    const e = sheet({ B1: 2, A1: '=SEQUENCE(B1)', D1: '=A4+1' });
    e.recalc();
    expect(get(e, 'D1')).toBe(1);
    fill(e, { B1: 6 });
    e.recalc();
    expect(get(e, 'D1')).toBe(5);
  });
});

describe('#SPILL! on collisions', () => {
  it('is #SPILL! when a value blocks the spill range', () => {
    const e = sheet({ A1: '=SEQUENCE(3)', A3: 'x' });
    e.recalc();
    expect(get(e, 'A1')).toBe('#SPILL!');
    expect(get(e, 'A2')).toBe(null);
    expect(get(e, 'A3')).toBe('x');
    expect(e.getSpillRange('Sheet1', 0, 0)).toMatchObject({ blocked: true, row1: 2 });
  });

  it('recovers when the blocking value is cleared', () => {
    const e = sheet({ A1: '=SEQUENCE(3)', A3: 'x' });
    e.recalc();
    e.clearCell('Sheet1', addr('A3').row, addr('A3').col);
    e.recalc();
    expect(get(e, 'A1')).toBe(1);
    expect(get(e, 'A3')).toBe(3);
  });

  it('turns into #SPILL! when a value is typed into the spill range', () => {
    const e = sheet({ A1: '=SEQUENCE(3)', B1: '=SUM(A1:A3)' });
    e.recalc();
    expect(get(e, 'B1')).toBe(6);
    fill(e, { A2: 'hello' });
    const r = e.recalc();
    expect(get(e, 'A1')).toBe('#SPILL!');
    expect(get(e, 'B1')).toBe('#SPILL!');
    expect(r.changed.map((c) => `${c.row},${c.col}`)).toContain('0,0');
  });

  it('is blocked by another formula', () => {
    const e = sheet({ A1: '=SEQUENCE(3)', A2: '=1+1' });
    e.recalc();
    expect(get(e, 'A1')).toBe('#SPILL!');
    expect(get(e, 'A2')).toBe(2);
  });

  it('is blocked by another spill, and takes over when that spill shrinks', () => {
    const e = sheet({ Z1: 3, A1: '=SEQUENCE(Z1,2)', B3: 5 });
    e.recalc();
    expect(get(e, 'A1')).toBe('#SPILL!');
    const e2 = sheet({ Z1: 3, A1: '=SEQUENCE(1,Z1)', B1: 7 });
    e2.recalc();
    expect(get(e2, 'A1')).toBe('#SPILL!');
    fill(e2, { Z1: 1 });
    e2.recalc();
    expect(get(e2, 'A1')).toBe(1);

    const e3 = sheet({ Z1: 3, A1: '=SEQUENCE(Z1)', B2: '=SEQUENCE(1,3,10)' });
    e3.recalc();
    expect(get(e3, 'B2')).toBe(10);
    // Grow A1's spill sideways into B2's spill: A1 is blocked because B2 already spills there.
    e3.setCell('Sheet1', 0, 0, { formula: '=SEQUENCE(Z1,3)' });
    e3.recalc();
    expect(get(e3, 'A1')).toBe('#SPILL!');
    expect(get(e3, 'C2')).toBe(11);
    // Remove B2: A1 can now spill.
    e3.clearCell('Sheet1', 1, 1);
    e3.recalc();
    expect(get(e3, 'A1')).toBe(1);
    expect(get(e3, 'C3')).toBe(9);
  });

  it('is #SPILL! when the result runs off the sheet', () => {
    const e = sheet();
    e.setCell('Sheet1', 1_048_575, 0, { formula: '=SEQUENCE(2)' });
    e.recalc();
    expect(plain(e.getValue('Sheet1', 1_048_575, 0))).toBe('#SPILL!');
  });

  it('legacy formulas never spill (top-left value / implicit intersection)', () => {
    const e = sheet({ A1: 5, A2: 6, A3: 7 });
    e.setCell('Sheet1', 1, 2, { formula: '=A1:A3', legacy: true });
    e.setCell('Sheet1', 0, 3, { formula: '=SEQUENCE(3)', legacy: true });
    e.recalc();
    expect(get(e, 'C2')).toBe(6);
    expect(get(e, 'D1')).toBe(1);
    expect(get(e, 'D2')).toBe(null);
  });
});

describe('the # spill-range operator', () => {
  it('refers to the whole spill range of an anchor', () => {
    const e = sheet({ A1: '=SEQUENCE(4)', C1: '=SUM(A1#)', D1: '=A1#*2' });
    e.recalc();
    expect(get(e, 'C1')).toBe(10);
    expect([get(e, 'D1'), get(e, 'D4')]).toEqual([2, 8]);
  });

  it('follows the spill as it resizes', () => {
    const e = sheet({ B1: 3, A1: '=SEQUENCE(B1)', C1: '=SUM(A1#)', D1: '=COUNT(A1#)' });
    e.recalc();
    expect(get(e, 'C1')).toBe(6);
    fill(e, { B1: 10 });
    e.recalc();
    expect(get(e, 'C1')).toBe(55);
    expect(get(e, 'D1')).toBe(10);
  });

  it('works across sheets and with quoted sheet names', () => {
    const e = sheet();
    e.addSheet('My Data');
    e.setCell('My Data', 0, 0, { formula: '=SEQUENCE(3)' });
    e.setCell('Sheet1', 0, 0, { formula: "=SUM('My Data'!A1#)" });
    e.recalc();
    expect(get(e, 'A1')).toBe(6);
  });

  it('returns the anchor error when the anchor is blocked', () => {
    const e = sheet({ A1: '=SEQUENCE(3)', A2: 'x', C1: '=SUM(A1#)' });
    e.recalc();
    expect(get(e, 'C1')).toBe('#SPILL!');
  });

  it('is #REF! on a cell without a formula', () => {
    const e = sheet({ A1: 5, C1: '=SUM(A1#)' });
    e.recalc();
    expect(get(e, 'C1')).toBe('#REF!');
  });

  it('chains spills: a spill of a spill', () => {
    const e = sheet({ A1: '=SEQUENCE(3)', B1: '=A1#^2', C1: '=SUM(B1#)' });
    e.recalc();
    expect([get(e, 'B1'), get(e, 'B2'), get(e, 'B3')]).toEqual([1, 4, 9]);
    expect(get(e, 'C1')).toBe(14);
  });

  it('works with FILTER, SORT and UNIQUE results', () => {
    const e = sheet({
      A1: 3,
      A2: 1,
      A3: 3,
      A4: 2,
      C1: '=SORT(UNIQUE(A1:A4))',
      D1: '=FILTER(C1#,C1#>1)',
      E1: '=SUM(D1#)',
    });
    e.recalc();
    expect([get(e, 'C1'), get(e, 'C2'), get(e, 'C3')]).toEqual([1, 2, 3]);
    expect([get(e, 'D1'), get(e, 'D2')]).toEqual([2, 3]);
    expect(get(e, 'E1')).toBe(5);
  });
});
