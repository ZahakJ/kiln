/**
 * Recalculation micro-benchmarks (`pnpm --filter @kiln-ref/formula bench`).
 *
 * The headline case is a 100,000-cell dependent chain (A2 = A1 + 1, A3 = A2 + 1, …): changing A1
 * must recalculate every cell in topological order. Target (PLAN.md §3.9 / WP-07): under 1.5 s.
 */
import { expect, test } from 'vitest';
import { Engine } from '../src/index.ts';

const CHAIN = 100_000;
const TARGET_MS = 1500;

function buildChain(): Engine {
  const engine = new Engine();
  engine.addSheet('Sheet1');
  engine.setCell('Sheet1', 0, 0, { value: 1 });
  for (let r = 1; r < CHAIN; r++) engine.setCell('Sheet1', r, 0, { formula: `=A${r}+1` });
  engine.recalc();
  return engine;
}

test('100k-cell dependent chain: incremental recalc after editing the head', async ({ bench }) => {
  const engine = buildChain();
  let v = 1;
  const result = await bench('recalc 100k chain', () => {
    v++;
    engine.setCell('Sheet1', 0, 0, { value: v });
    engine.recalc();
  }).run({ iterations: 10, time: 0, warmupIterations: 2, warmupTime: 0 });
  expect(engine.getValue('Sheet1', CHAIN - 1, 0)).toBe(v + CHAIN - 1);
  console.log(`recalc 100k chain: mean ${result.latency.mean.toFixed(1)} ms (target < ${TARGET_MS} ms)`);
  expect(result.latency.mean).toBeLessThan(TARGET_MS);
});

test('100k-cell dependent chain: full recalc', async ({ bench }) => {
  const engine = buildChain();
  const result = await bench('recalcAll 100k chain', () => {
    engine.recalcAll();
  }).run({ iterations: 5, time: 0, warmupIterations: 1, warmupTime: 0 });
  console.log(`recalcAll 100k chain: mean ${result.latency.mean.toFixed(1)} ms`);
  expect(result.latency.mean).toBeLessThan(TARGET_MS);
});

test('SUM over a 100k-row column (10 formulas, one range node)', async ({ bench }) => {
  const engine = new Engine();
  engine.addSheet('Sheet1');
  for (let r = 0; r < CHAIN; r++) engine.setCell('Sheet1', r, 0, { value: r });
  for (let r = 0; r < 10; r++) engine.setCell('Sheet1', r, 2, { formula: '=SUM(A:A)' });
  engine.recalc();
  expect(engine.graphStats().rangeNodes).toBe(1);
  let v = 0;
  const result = await bench('edit + recalc 10 x SUM(A:A)', () => {
    engine.setCell('Sheet1', 5, 0, { value: ++v });
    engine.recalc();
  }).run({ iterations: 5, time: 0, warmupIterations: 1, warmupTime: 0 });
  console.log(`edit + 10 x SUM(A:A) over 100k rows: mean ${result.latency.mean.toFixed(1)} ms`);
});
