/** Shared test helpers: a seeded PRNG and model/engine consistency checks. */
import { expect } from 'vitest';
import { type CellValue, snapshotWorkbook, type Workbook, type WorkbookCalc } from '../src/index.ts';

/** mulberry32: small, fast, deterministic. */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function int(r: () => number, lo: number, hi: number): number {
  return lo + Math.floor(r() * (hi - lo + 1));
}

export function pick<T>(r: () => number, list: readonly T[]): T {
  return list[Math.floor(r() * list.length)] as T;
}

/** The workbook state that must round-trip through undo (style table growth is allowed). */
export function stateOf(wb: Workbook): unknown {
  const s = snapshotWorkbook(wb);
  return { props: s.props, names: s.names, sheets: s.sheets, ids: undefined };
}

/** Every formula in the store is in the engine with the same text, and vice versa. */
export function expectEngineInSync(wb: Workbook, calc: WorkbookCalc): void {
  let count = 0;
  for (const s of wb.sheets) {
    expect(calc.engine.sheetName(s.id)).toBe(s.name);
    s.cells.forEach((row, col, _v, f) => {
      if (f === null) return;
      count++;
      expect(calc.engine.getFormula(s.id, row, col), `${s.name}!R${row}C${col}`).toBe(`=${f}`);
    });
  }
  expect(calc.engine.formulaCount).toBe(count - calc.errors.size);
  expect(calc.engine.listSheets()).toEqual(wb.sheets.map((s) => s.name));
}

export function values(
  wb: Workbook,
  calc: WorkbookCalc,
  sheet: number,
  r0: number,
  c0: number,
  r1: number,
  c1: number,
): CellValue[][] {
  const out: CellValue[][] = [];
  for (let r = r0; r <= r1; r++) {
    const line: CellValue[] = [];
    for (let c = c0; c <= c1; c++) line.push(calc.getValue(sheet, r, c));
    out.push(line);
  }
  void wb;
  return out;
}

/** Non-null assertion that fails the test with a clear message. */
export function nn<T>(v: T | null | undefined): T {
  if (v === null || v === undefined) throw new Error('expected a value');
  return v;
}
