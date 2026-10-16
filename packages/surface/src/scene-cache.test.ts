import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { decode, decodeAs, SCHEMA, type SceneDelta, type Tile } from '@kiln/protocol';
import { describe, expect, it } from 'vitest';
import { DeltaError, glyphPages, SceneCache, tileKey } from './scene-cache';

const VECTORS = fileURLToPath(new URL('../../../crates/kiln-protocol/tests/vectors', import.meta.url));
const FRAME = fileURLToPath(new URL('../../../fixtures/golden/scene/lorem-frame.bin', import.meta.url));

function lorem(): SceneDelta {
  return decodeAs<SceneDelta>('SceneDelta', new Uint8Array(readFileSync(FRAME)));
}

const tile = (index: number, hash: number, y = index * 1000): Tile => ({
  id: { layer: 0, index },
  rect: { x: 0, y, w: 800, h: 1000 },
  hash,
  ops: [{ FillRect: { rect: { x: 0, y: 0, w: 10, h: 10 }, paint: { Role: 'Page' }, radius: 0 } }],
});

const delta = (over: Partial<SceneDelta>): SceneDelta => ({
  revision: 1,
  reset: false,
  scale: 1,
  extent: { x: 0, y: 0, w: 900, h: 5000 },
  background: { Role: 'Canvas' },
  tiles: [],
  removed: [],
  atlas: [],
  images: [],
  overlay: null,
  ...over,
});

describe('decoding frames', () => {
  it('decodes the Rust SceneDelta vectors with the protocol decoder', () => {
    const vector = JSON.parse(readFileSync(`${VECTORS}/SceneDelta.json`, 'utf8')) as {
      format: unknown;
      values: SceneDelta[];
    };
    const bytes = new Uint8Array(readFileSync(`${VECTORS}/SceneDelta.bin`));
    const values = decode({ SEQ: vector.format as never }, bytes, SCHEMA, {
      canonicalF32: true,
    }) as SceneDelta[];
    expect(values).toEqual(vector.values);
    const full = values[1] as SceneDelta;
    const cache = new SceneCache();
    cache.apply(full);
    expect(cache.tiles.size).toBe(1);
    expect(cache.atlas.get(1)?.rows).toBe(3);
    expect(cache.images.get(3)?.data).toEqual(Uint8ClampedArray.of(255, 106, 38, 255));
    expect(cache.overlay).toHaveLength(1);
  });

  it('decodes a real frame of the scratch document (lorem.txt) quickly', () => {
    const bytes = new Uint8Array(readFileSync(FRAME));
    decodeAs<SceneDelta>('SceneDelta', bytes); // warm-up (JIT)
    let perFrame = Number.POSITIVE_INFINITY;
    for (let i = 0; i < 5; i++) {
      const t0 = performance.now();
      decodeAs<SceneDelta>('SceneDelta', bytes);
      perFrame = Math.min(perFrame, performance.now() - t0);
    }
    const d = lorem();
    expect(d.reset).toBe(true);
    expect(d.tiles.length).toBeGreaterThan(0);
    const ops = d.tiles[0]?.ops ?? [];
    expect(ops.some((op) => typeof op === 'object' && 'Shadow' in op)).toBe(true);
    const glyphs = ops.filter((op) => typeof op === 'object' && 'Glyphs' in op);
    expect(glyphs.length).toBeGreaterThan(0);
    expect(d.atlas[0]?.format).toBe('A8');
    // A full first frame decodes in a few milliseconds (PLAN.md §3.8: 1–3 ms per viewport); CI
    // runs this under coverage instrumentation, which is several times slower.
    expect(perFrame).toBeLessThan(process.env.CI ? 150 : 25);
  });
});

describe('SceneCache', () => {
  it('applies, replaces, keeps identical canvases and removes tiles', () => {
    const c = new SceneCache<string>();
    expect(c.apply(delta({ reset: true, tiles: [tile(0, 1), tile(1, 2)] }))).toBe(true);
    expect([...c.tiles.keys()]).toEqual(['0:0', '0:1']);
    const t0 = c.tiles.get('0:0');
    if (!t0) throw new Error('missing');
    t0.canvas = 'rendered';
    t0.palette = 'light';
    c.apply(delta({ tiles: [tile(0, 1)] }));
    expect(c.tiles.get('0:0')?.canvas).toBe('rendered');
    c.apply(delta({ tiles: [tile(0, 9)] }));
    expect(c.tiles.get('0:0')?.canvas).toBeNull();
    c.apply(delta({ removed: [{ layer: 0, index: 1 }] }));
    expect(c.tiles.has('0:1')).toBe(false);
    expect(c.apply(delta({}))).toBe(false);
    c.apply(delta({ reset: true }));
    expect(c.tiles.size).toBe(0);
  });

  it('writes atlas rows into pages and validates updates', () => {
    const c = new SceneCache();
    c.apply(
      delta({
        atlas: [
          { page: 0, format: 'A8', width: 4, height: 4, y: 0, rows: 2, data: [1, 2, 3, 4, 5, 6, 7, 8] },
        ],
      }),
    );
    c.apply(
      delta({ atlas: [{ page: 0, format: 'A8', width: 4, height: 4, y: 2, rows: 1, data: [9, 9, 9, 9] }] }),
    );
    const page = c.atlas.get(0);
    expect(page?.rows).toBe(3);
    expect(page?.version).toBe(2);
    expect([...(page?.data ?? [])].slice(0, 12)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 9, 9, 9]);
    const bad = [
      delta({
        atlas: [{ page: 0, format: 'A8', width: 4, height: 4, y: 3, rows: 2, data: new Array(8).fill(0) }],
      }),
      delta({
        atlas: [{ page: 0, format: 'A8', width: 8, height: 4, y: 0, rows: 1, data: new Array(8).fill(0) }],
      }),
      delta({ images: [{ id: 1, width: 2, height: 2, data: [1, 2, 3] }] }),
      delta({ scale: 0 }),
    ];
    for (const d of bad) expect(() => c.apply(d)).toThrow(DeltaError);
    expect(c.atlas.get(0)?.version).toBe(2);
  });

  it('culls tiles to a region in paint order', () => {
    const c = new SceneCache();
    c.apply(delta({ reset: true, tiles: [tile(2, 1), tile(0, 1), tile(1, 1)] }));
    expect(c.tilesIn({ x: 0, y: 500, w: 800, h: 1000 }).map((t) => tileKey(t.tile.id))).toEqual([
      '0:0',
      '0:1',
    ]);
    expect(c.tilesIn({ x: 900, y: 0, w: 10, h: 10 })).toEqual([]);
    expect(glyphPages([{ Glyphs: { page: 3, paint: { Role: 'Ink' }, quads: [] } }, 'PopClip'])).toEqual([3]);
  });
});
