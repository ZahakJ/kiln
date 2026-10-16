import { Engine, type EngineOptions } from '../../src/engine/engine.ts';
import { parseCellAddress } from '../../src/parser/references.ts';
import { ErrorValue } from '../../src/values/errors.ts';
import { Array2D, BLANK, type Scalar, type Value } from '../../src/values/types.ts';

export function addr(a1: string): { row: number; col: number } {
  const p = parseCellAddress(a1);
  if (!p) throw new Error(`bad address ${a1}`);
  return p;
}

/** Creates an engine with Sheet1 and fills cells: strings starting with "=" are formulas. */
export function sheet(cells: Record<string, Scalar> = {}, opts: EngineOptions = {}): Engine {
  const e = new Engine({ now: () => new Date(2024, 0, 15, 12, 0, 0), ...opts });
  e.addSheet('Sheet1');
  fill(e, cells);
  return e;
}

export function fill(e: Engine, cells: Record<string, Scalar>, sheetName = 'Sheet1'): void {
  for (const [k, v] of Object.entries(cells)) {
    const { row, col } = addr(k);
    if (typeof v === 'string' && v.startsWith('=')) e.setCell(sheetName, row, col, { formula: v });
    else e.setCell(sheetName, row, col, { value: v });
  }
}

export function get(e: Engine, a1: string, sheetName = 'Sheet1'): unknown {
  const { row, col } = addr(a1);
  return plain(e.getValue(sheetName, row, col));
}

/** Converts values to plain JS for easy assertions: errors → code strings, arrays → nested arrays. */
export function plain(v: Value | undefined): unknown {
  if (v === undefined) return undefined;
  if (v instanceof ErrorValue) return v.code;
  if (v === BLANK) return null;
  if (v instanceof Array2D) return v.data.map((r) => r.map((x) => plain(x)));
  if (typeof v === 'object') return String(v);
  return v;
}

/** Evaluates a formula in a fresh (or given) engine and returns a plain value. */
export function calc(formula: string, cells: Record<string, Scalar> = {}): unknown {
  const e = sheet(cells);
  e.recalc();
  return plain(e.evaluate(formula));
}

/** Deterministic PRNG (mulberry32) for property-style tests. */
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

export function randInt(r: () => number, lo: number, hi: number): number {
  return lo + Math.floor(r() * (hi - lo + 1));
}
