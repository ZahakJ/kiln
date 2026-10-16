import { describe, expect, it } from 'vitest';
import { errorValue, MAX_COLS, MAX_ROWS, RowChunk, SparseStore } from '../src/index.ts';
import { int, rng } from './helpers.ts';

function collect(s: SparseStore): [number, number, unknown][] {
  const out: [number, number, unknown][] = [];
  s.forEach((r, c, v) => {
    out.push([r, c, v]);
  });
  return out;
}

describe('RowChunk', () => {
  it('keeps cells sorted by column whatever the insertion order', () => {
    const ch = new RowChunk();
    for (const c of [5, 1, 9, 3, 7]) ch.put(c, c * 10, null, 0);
    expect(Array.from(ch.cols.subarray(0, ch.size))).toEqual([1, 3, 5, 7, 9]);
    expect(ch.values).toEqual([10, 30, 50, 70, 90]);
  });

  it('finds cells through the binary search and the lazy hash index', () => {
    const ch = new RowChunk();
    for (let c = 0; c < 100; c += 2) ch.put(c, c, null, 0);
    for (let c = 0; c < 100; c++) expect(ch.find(c)).toBe(c % 2 === 0 ? c / 2 : -1);
    // A middle insertion invalidates and rebuilds the index.
    ch.put(51, 'x', null, 0);
    expect(ch.find(51)).toBe(26);
    expect(ch.find(52)).toBe(27);
  });

  it('overwrites in place without growing', () => {
    const ch = new RowChunk();
    expect(ch.put(3, 1, null, 0)).toBe(true);
    expect(ch.put(3, 2, 'A1', 4)).toBe(false);
    expect(ch.size).toBe(1);
    expect(ch.values[0]).toBe(2);
    expect(ch.formulas[0]).toBe('A1');
    expect(ch.styleIdx[0]).toBe(4);
  });

  it('removes slots and shifts columns', () => {
    const ch = new RowChunk();
    for (const c of [0, 1, 2, 3, 4]) ch.put(c, c, null, 0);
    ch.removeAt(2);
    expect(Array.from(ch.cols.subarray(0, ch.size))).toEqual([0, 1, 3, 4]);
    ch.shiftCols(1, 2);
    expect(Array.from(ch.cols.subarray(0, ch.size))).toEqual([0, 3, 5, 6]);
    ch.shiftCols(3, -2);
    expect(Array.from(ch.cols.subarray(0, ch.size))).toEqual([0, 3, 4]);
    expect(ch.values).toEqual([0, 3, 4]);
  });

  it('drops cells pushed past the last column', () => {
    const ch = new RowChunk();
    ch.put(MAX_COLS - 1, 'edge', null, 0);
    ch.put(0, 'a', null, 0);
    ch.shiftCols(0, 1);
    expect(ch.size).toBe(1);
    expect(ch.cols[0]).toBe(1);
  });
});

describe('SparseStore', () => {
  it('stores values, formulas and styles independently', () => {
    const s = new SparseStore();
    s.setValue(0, 0, 42);
    s.setStyle(0, 0, 3);
    s.setFormula(1, 1, 'A1*2', 84);
    expect(s.getValue(0, 0)).toBe(42);
    expect(s.getStyle(0, 0)).toBe(3);
    expect(s.getFormula(1, 1)).toBe('A1*2');
    expect(s.getValue(1, 1)).toBe(84);
    expect(s.get(5, 5)).toBeUndefined();
    expect(s.size).toBe(2);
  });

  it('removes a cell when value, formula and style are all empty', () => {
    const s = new SparseStore();
    s.set(2, 2, 'x', null, 1);
    s.setValue(2, 2, null);
    expect(s.has(2, 2)).toBe(true); // still styled
    s.setStyle(2, 2, 0);
    expect(s.has(2, 2)).toBe(false);
    expect(s.size).toBe(0);
    expect(s.rowCount).toBe(0);
  });

  it('accepts every value type including errors and booleans', () => {
    const s = new SparseStore();
    s.setValue(0, 0, true);
    s.setValue(0, 1, false);
    s.setValue(0, 2, errorValue('#N/A'));
    s.setValue(0, 3, '');
    s.setValue(0, 4, -0.5);
    expect(collect(s).map((x) => x[2])).toEqual([true, false, errorValue('#N/A'), '', -0.5]);
  });

  it('iterates in row-major order regardless of insertion order', () => {
    const s = new SparseStore();
    const r = rng(7);
    const cells = new Set<string>();
    for (let i = 0; i < 500; i++) {
      const row = int(r, 0, 60);
      const col = int(r, 0, 30);
      s.setValue(row, col, row * 100 + col);
      cells.add(`${row},${col}`);
    }
    const seen = collect(s);
    expect(seen.length).toBe(cells.size);
    for (let i = 1; i < seen.length; i++) {
      const [r0, c0] = seen[i - 1] as [number, number, unknown];
      const [r1, c1] = seen[i] as [number, number, unknown];
      expect(r0 < r1 || (r0 === r1 && c0 < c1)).toBe(true);
    }
    for (const [row, col, v] of seen) expect(v).toBe(row * 100 + col);
  });

  it('agrees with a Map model under random operations', () => {
    const s = new SparseStore();
    const model = new Map<string, number>();
    const r = rng(99);
    for (let i = 0; i < 3000; i++) {
      const row = int(r, 0, 20);
      const col = int(r, 0, 40);
      const k = `${row},${col}`;
      if (r() < 0.3) {
        s.delete(row, col);
        model.delete(k);
      } else {
        const v = int(r, 0, 1000);
        s.setValue(row, col, v);
        model.set(k, v);
      }
    }
    expect(s.size).toBe(model.size);
    for (const [k, v] of model) {
      const [row, col] = k.split(',').map(Number) as [number, number];
      expect(s.getValue(row, col)).toBe(v);
    }
  });

  it('reports the used range, with and without style-only cells', () => {
    const s = new SparseStore();
    expect(s.getUsedRange()).toBeNull();
    s.setValue(3, 4, 1);
    s.setValue(10, 1, 2);
    s.setStyle(20, 30, 5);
    expect(s.getUsedRange()).toEqual({ r0: 3, c0: 1, r1: 20, c1: 30 });
    expect(s.getUsedRange(true)).toEqual({ r0: 3, c0: 1, r1: 10, c1: 4 });
    s.delete(20, 30);
    expect(s.getUsedRange()).toEqual({ r0: 3, c0: 1, r1: 10, c1: 4 });
  });

  it('visits only cells inside a range (dense and sparse paths)', () => {
    const s = new SparseStore();
    for (let r = 0; r < 50; r++) for (let c = 0; c < 10; c++) s.setValue(r * 1000, c, r * 10 + c);
    const small: number[] = [];
    s.forEachInRange({ r0: 2000, c0: 3, r1: 3000, c1: 4 }, (_r, _c, v) => small.push(v as number));
    expect(small).toEqual([23, 24, 33, 34]);
    const big: number[] = [];
    s.forEachInRange({ r0: 0, c0: 9, r1: MAX_ROWS - 1, c1: 9 }, (_r, _c, v) => big.push(v as number));
    expect(big.length).toBe(50);
    expect(s.countInRange({ r0: 0, c0: 0, r1: 999, c1: MAX_COLS - 1 })).toBe(10);
  });

  it('writes 2-D blocks with setRange, skipping undefined and keeping styles', () => {
    const s = new SparseStore();
    s.setStyle(1, 1, 7);
    s.setFormula(0, 2, 'X1');
    s.setRange(0, 0, [
      [1, 2, 3],
      [4, null, undefined],
    ]);
    expect(s.getValue(0, 2)).toBe(3);
    expect(s.getFormula(0, 2)).toBeNull(); // values replace formulas
    expect(s.has(1, 1)).toBe(true);
    expect(s.getStyle(1, 1)).toBe(7);
    expect(s.getValue(1, 1)).toBeNull();
    expect(s.has(1, 2)).toBe(false);
    s.setRange(5, 5, [['a']], [['B1']]);
    expect(s.getFormula(5, 5)).toBe('B1');
  });

  it('clears a range', () => {
    const s = new SparseStore();
    s.setRange(0, 0, [
      [1, 2, 3],
      [4, 5, 6],
    ]);
    s.clearRange({ r0: 0, c0: 1, r1: 1, c1: 1 });
    expect(collect(s).map((x) => x[2])).toEqual([1, 3, 4, 6]);
  });

  it('shifts rows for inserts and deletes', () => {
    const s = new SparseStore();
    for (let r = 0; r < 6; r++) s.setValue(r, 0, r);
    s.shiftRows(2, 3);
    expect(collect(s).map((x) => [x[0], x[2]])).toEqual([
      [0, 0],
      [1, 1],
      [5, 2],
      [6, 3],
      [7, 4],
      [8, 5],
    ]);
    s.shiftRows(1, -5);
    expect(collect(s).map((x) => [x[0], x[2]])).toEqual([
      [0, 0],
      [1, 3],
      [2, 4],
      [3, 5],
    ]);
    expect(s.size).toBe(4);
  });

  it('drops rows pushed past the last row', () => {
    const s = new SparseStore();
    s.setValue(MAX_ROWS - 1, 0, 'last');
    s.setValue(0, 0, 'first');
    s.shiftRows(0, 1);
    expect(s.size).toBe(1);
    expect(s.getValue(1, 0)).toBe('first');
  });

  it('shifts columns of every row', () => {
    const s = new SparseStore();
    s.setRange(0, 0, [
      [1, 2, 3],
      [4, 5, 6],
    ]);
    s.shiftCols(1, -1);
    expect(collect(s)).toEqual([
      [0, 0, 1],
      [0, 1, 3],
      [1, 0, 4],
      [1, 1, 6],
    ]);
    s.shiftCols(0, 2);
    expect(s.getValue(1, 3)).toBe(6);
    s.shiftCols(0, -10);
    expect(s.size).toBe(0);
    expect(s.rowCount).toBe(0);
  });

  it('writes cached values only to formula cells', () => {
    const s = new SparseStore();
    s.setValue(0, 0, 1);
    s.setFormula(0, 1, 'A1+1');
    s.setCachedValue(0, 0, 99);
    s.setCachedValue(0, 1, 2);
    s.setCachedValue(5, 5, 3);
    expect(s.getValue(0, 0)).toBe(1);
    expect(s.getValue(0, 1)).toBe(2);
    expect(s.has(5, 5)).toBe(false);
  });

  it('clones deeply', () => {
    const s = new SparseStore();
    s.setValue(1, 1, 'a');
    const c = s.clone();
    c.setValue(1, 1, 'b');
    c.setValue(2, 2, 'c');
    expect(s.getValue(1, 1)).toBe('a');
    expect(s.size).toBe(1);
    expect(c.size).toBe(2);
  });

  it('handles rows with thousands of cells (hash index path)', () => {
    const s = new SparseStore();
    for (let c = MAX_COLS - 1; c >= 0; c -= 3) s.setValue(0, c, c);
    let n = 0;
    for (let c = 0; c < MAX_COLS; c++) {
      const v = s.getValue(0, c);
      if (v !== null) {
        n++;
        expect(v).toBe(c);
      }
    }
    expect(n).toBe(s.size);
  });
});
