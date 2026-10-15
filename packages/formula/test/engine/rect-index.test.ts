import { describe, expect, it } from 'vitest';
import { type Rect, RectIndex } from '../../src/engine/rect-index.ts';
import { randInt, rng } from './_support.ts';

interface Item extends Rect {
  id: number;
}

describe('RectIndex (interval treap)', () => {
  it('finds rectangles by stabbing and overlap, matching brute force', () => {
    const r = rng(42);
    const idx = new RectIndex<Item>();
    let live: Item[] = [];
    for (let i = 0; i < 2000; i++) {
      const r0 = randInt(r, 0, 1000);
      const c0 = randInt(r, 0, 50);
      const item: Item = { id: i, r0, c0, r1: r0 + randInt(r, 0, 200), c1: c0 + randInt(r, 0, 10) };
      idx.insert(item);
      live.push(item);
      if (r() < 0.3) {
        const victim = live[randInt(r, 0, live.length - 1)] as Item;
        expect(idx.remove(victim)).toBe(true);
        live = live.filter((x) => x !== victim);
      }
    }
    expect(idx.size).toBe(live.length);
    for (let q = 0; q < 300; q++) {
      const row = randInt(r, 0, 1200);
      const col = randInt(r, 0, 60);
      const got: number[] = [];
      idx.stab(row, col, (e) => got.push(e.id));
      const want = live
        .filter((e) => e.r0 <= row && e.r1 >= row && e.c0 <= col && e.c1 >= col)
        .map((e) => e.id);
      expect(got.sort((a, b) => a - b)).toEqual(want.sort((a, b) => a - b));
      const rect = { r0: row, r1: row + randInt(r, 0, 50), c0: col, c1: col + randInt(r, 0, 5) };
      const got2: number[] = [];
      idx.overlaps(rect, (e) => got2.push(e.id));
      const want2 = live
        .filter((e) => e.r0 <= rect.r1 && e.r1 >= rect.r0 && e.c0 <= rect.c1 && e.c1 >= rect.c0)
        .map((e) => e.id);
      expect(got2.sort((a, b) => a - b)).toEqual(want2.sort((a, b) => a - b));
    }
  });

  it('removing an unknown entry is a no-op', () => {
    const idx = new RectIndex<Item>();
    idx.insert({ id: 1, r0: 0, c0: 0, r1: 1, c1: 1 });
    expect(idx.remove({ id: 2, r0: 0, c0: 0, r1: 1, c1: 1 })).toBe(false);
    expect(idx.entries()).toHaveLength(1);
  });
});
