/**
 * Cell-store benchmarks (`pnpm --filter @kiln-ref/sheet-model bench`).
 *
 * Targets (WP-16): fill 1,000,000 cells in under 2 s and iterate them in under 300 ms.
 */
import { expect, test } from 'vitest';
import { SparseStore } from '../src/index.ts';

const ROWS = 10_000;
const COLS = 100;
const CELLS = ROWS * COLS;
const FILL_TARGET_MS = 2000;
const ITERATE_TARGET_MS = 300;

function fillCellByCell(): SparseStore {
  const s = new SparseStore();
  for (let r = 0; r < ROWS; r++) for (let c = 0; c < COLS; c++) s.set(r, c, r * COLS + c);
  return s;
}

test('fill 1M cells one by one', async ({ bench }) => {
  let store: SparseStore | undefined;
  const result = await bench('fill 1M cells (set)', () => {
    store = fillCellByCell();
  }).run({ iterations: 5, time: 0, warmupIterations: 1, warmupTime: 0 });
  expect(store?.size).toBe(CELLS);
  console.log(
    `fill 1M cells (set): mean ${result.latency.mean.toFixed(1)} ms (target < ${FILL_TARGET_MS} ms)`,
  );
  expect(result.latency.mean).toBeLessThan(FILL_TARGET_MS);
});

test('fill 1M cells with setRange', async ({ bench }) => {
  const block = Array.from({ length: ROWS }, (_, r) => Array.from({ length: COLS }, (_, c) => r * COLS + c));
  let store: SparseStore | undefined;
  const result = await bench('fill 1M cells (setRange)', () => {
    store = new SparseStore();
    store.setRange(0, 0, block);
  }).run({ iterations: 5, time: 0, warmupIterations: 1, warmupTime: 0 });
  expect(store?.size).toBe(CELLS);
  console.log(
    `fill 1M cells (setRange): mean ${result.latency.mean.toFixed(1)} ms (target < ${FILL_TARGET_MS} ms)`,
  );
  expect(result.latency.mean).toBeLessThan(FILL_TARGET_MS);
});

test('fill 1M cells in random order', async ({ bench }) => {
  const order = new Int32Array(CELLS);
  for (let i = 0; i < CELLS; i++) order[i] = i;
  let seed = 12345;
  for (let i = CELLS - 1; i > 0; i--) {
    seed = (seed * 1103515245 + 12345) >>> 0;
    const j = seed % (i + 1);
    const t = order[i] as number;
    order[i] = order[j] as number;
    order[j] = t;
  }
  let store: SparseStore | undefined;
  const result = await bench('fill 1M cells (random order)', () => {
    store = new SparseStore();
    for (let i = 0; i < CELLS; i++) {
      const k = order[i] as number;
      store.set(Math.floor(k / COLS), k % COLS, k);
    }
  }).run({ iterations: 3, time: 0, warmupIterations: 1, warmupTime: 0 });
  expect(store?.size).toBe(CELLS);
  console.log(
    `fill 1M cells (random order): mean ${result.latency.mean.toFixed(1)} ms (target < ${FILL_TARGET_MS} ms)`,
  );
  expect(result.latency.mean).toBeLessThan(FILL_TARGET_MS);
});

test('iterate 1M cells', async ({ bench }) => {
  const store = fillCellByCell();
  let sum = 0;
  let n = 0;
  const result = await bench('iterate 1M cells (forEach)', () => {
    sum = 0;
    n = 0;
    store.forEach((_r, _c, v) => {
      sum += v as number;
      n++;
    });
  }).run({ iterations: 10, time: 0, warmupIterations: 2, warmupTime: 0 });
  expect(n).toBe(CELLS);
  expect(sum).toBe((CELLS * (CELLS - 1)) / 2);
  console.log(
    `iterate 1M cells: mean ${result.latency.mean.toFixed(1)} ms (target < ${ITERATE_TARGET_MS} ms)`,
  );
  expect(result.latency.mean).toBeLessThan(ITERATE_TARGET_MS);
});

test('random access over 1M cells', async ({ bench }) => {
  const store = fillCellByCell();
  let hits = 0;
  const result = await bench('1M random getValue', () => {
    hits = 0;
    let seed = 7;
    for (let i = 0; i < CELLS; i++) {
      seed = (seed * 1103515245 + 12345) >>> 0;
      if (store.getValue(seed % ROWS, (seed >>> 8) % COLS) !== null) hits++;
    }
  }).run({ iterations: 3, time: 0, warmupIterations: 1, warmupTime: 0 });
  expect(hits).toBe(CELLS);
  console.log(`1M random reads: mean ${result.latency.mean.toFixed(1)} ms`);
});
