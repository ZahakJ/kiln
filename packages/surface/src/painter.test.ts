import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { type DrawOp, decodeAs, type SceneDelta } from '@kiln/protocol';
import { beforeAll, describe, expect, it } from 'vitest';
import { device, Painter, type PainterCanvas, tileDeviceSize } from './painter';
import { DARK, LIGHT } from './palette';
import { SceneCache } from './scene-cache';
import { installPath2D, RecordingCanvas, type RecordingContext } from './test-support';

const FRAME = fileURLToPath(new URL('../../../fixtures/golden/scene/lorem-frame.bin', import.meta.url));

beforeAll(() => installPath2D());

function setup(delta: SceneDelta) {
  const cache = new SceneCache<PainterCanvas>();
  cache.apply(delta);
  const made: RecordingCanvas[] = [];
  const painter = new Painter(LIGHT, (w, h) => {
    const c = new RecordingCanvas(w, h);
    made.push(c);
    return c as unknown as PainterCanvas;
  });
  const screen = new RecordingCanvas(1000, 900);
  return { cache, painter, made, screen, ctx: screen.ctx as unknown as CanvasRenderingContext2D };
}

const base = (ops: DrawOp[], overlay: DrawOp[] = []): SceneDelta => ({
  revision: 1,
  reset: true,
  scale: 2,
  extent: { x: 0, y: 0, w: 900, h: 2000 },
  background: { Role: 'Canvas' },
  tiles: [{ id: { layer: 0, index: 0 }, rect: { x: 10.25, y: 20, w: 100, h: 50 }, hash: 7, ops }],
  removed: [],
  atlas: [{ page: 0, format: 'A8', width: 4, height: 2, y: 0, rows: 2, data: [255, 128, 0, 0, 0, 0, 0, 64] }],
  images: [],
  overlay: { hash: 1, ops: overlay },
});

describe('Painter', () => {
  it('rounds device positions and tile sizes like kiln-raster', () => {
    expect(device(10.25, 2)).toBe(21);
    expect(device(-28, 1)).toBe(-28);
    expect(tileDeviceSize({ x: 0, y: 0, w: 100.2, h: 50 }, 2)).toEqual([201, 100]);
    expect(tileDeviceSize({ x: 0, y: 0, w: 0, h: 1e9 }, 1)).toEqual([1, 16384]);
  });

  it('composites tiles at integer device offsets on the background', () => {
    const { cache, painter, made, ctx, screen } = setup(base([]));
    const drawn = painter.composite(ctx, cache, {
      width: 1000,
      height: 900,
      scrollX: 5,
      scrollY: 0,
      viewScale: 2,
    });
    expect(drawn).toBe(1);
    const calls = screen.ctx.calls;
    expect(calls[0]?.op).toBe('fillRect');
    expect(calls[0]?.fill).toBe('rgb(246 239 230)');
    const blit = calls.find((c) => c.op === 'drawImage');
    expect(blit?.args.slice(1)).toEqual([21 - 10, 40]);
    expect(made[0]).toMatchObject({ width: 200, height: 100 });
    // Rendered once: a second composite reuses the tile canvas.
    painter.composite(ctx, cache, { width: 1000, height: 900, scrollX: 6, scrollY: 0, viewScale: 2 });
    expect(painter.tilesRendered).toBe(1);
    // A pinch in progress stretches the cached tiles.
    painter.composite(ctx, cache, { width: 1000, height: 900, scrollX: 0, scrollY: 0, viewScale: 4 });
    const stretched = screen.ctx.calls.filter((c) => c.op === 'drawImage').at(-1);
    expect(stretched?.args.slice(1)).toEqual([41, 80, 400, 200]);
  });

  it('composes glyph quads 1:1 from the atlas into one layer in device pixels', () => {
    const { cache, painter, made, ctx } = setup(
      base([
        { Glyphs: { page: 0, paint: { Role: 'Ink' }, quads: [0, 0, 2, 1, 7, 9, 3, 1, 1, 1, 12, 9] } },
        // Outside its page or on a missing page: skipped.
        { Glyphs: { page: 0, paint: { Rgba: 0xff0000ff }, quads: [0, 0, 9, 9, 0, 0] } },
        { Glyphs: { page: 5, paint: { Role: 'Ink' }, quads: [0, 0, 1, 1, 0, 0] } },
      ]),
    );
    painter.composite(ctx, cache, { width: 1000, height: 900, scrollX: 0, scrollY: 0, viewScale: 2 });
    const tileCtx = made.find((c) => c.width === 200)?.ctx as RecordingContext;
    const blits = tileCtx.calls.filter((c) => c.op === 'drawImage');
    // One layer covering the glyphs' device bounds, drawn 1:1 without transform.
    expect(blits.map((b) => b.args.slice(1))).toEqual([[0, 0, 6, 1, 7, 9, 6, 1]]);
    expect(blits[0]?.transform).toEqual([1, 0, 0, 1, 0, 0]);
    const layer = made.find((c) => c.ctx.calls.some((k) => k.op === 'putImageData'));
    const put = layer?.ctx.calls.find((c) => c.op === 'putImageData');
    if (!put) throw new Error('no glyph layer');
    const px = (put.args[0] as { data: Uint8ClampedArray }).data;
    // Ink colour, alpha = coverage: (0,0) full, (1,0) half, (5,0) from atlas (3,1) = 64.
    expect([...px.slice(0, 8)]).toEqual([0x2b, 0x1c, 0x14, 255, 0x2b, 0x1c, 0x14, 128]);
    expect(px[5 * 4 + 3]).toBe(64);
    expect(px[2 * 4 + 3]).toBe(0);
  });

  it('blends the selection wash into the content and paints the other marks on the overlay', () => {
    const overlay: DrawOp[] = [
      { SelectionRects: { rects: [{ x: 0, y: 0, w: 40, h: 20 }], paint: { Role: 'Selection' } } },
      { Caret: { rect: { x: 60, y: 20, w: 2, h: 20 }, paint: { Role: 'Caret' }, spark: true } },
      { Handle: { x: 30, y: 60, size: 16, side: 'End', paint: { Role: 'Caret' } } },
    ];
    const { cache, painter, screen, ctx } = setup(base([], overlay));
    const view = { width: 1000, height: 900, scrollX: 0, scrollY: 0, viewScale: 2 };
    painter.composite(ctx, cache, view);
    const wash = screen.ctx.calls.filter((c) => c.op === 'fill');
    expect(wash.map((f) => [f.fill, f.composite])).toEqual([['rgb(255 217 184)', 'darken']]);
    screen.ctx.calls = [];
    painter.paintOverlay(ctx, cache, view, { caretVisible: true, touch: false });
    const fills = screen.ctx.calls.filter((c) => c.op === 'fill');
    expect(fills.map((f) => f.fill)).toEqual(['rgb(49 89 198)', 'rgb(255 106 38)']);
    expect(fills.every((f) => f.composite === 'source-over')).toBe(true);
    screen.ctx.calls = [];
    painter.paintOverlay(ctx, cache, view, { caretVisible: false, touch: true });
    expect(screen.ctx.calls.filter((c) => c.op === 'fill').map((f) => f.fill)).toEqual(['rgb(49 89 198)']);
    painter.setPalette(DARK);
    screen.ctx.calls = [];
    painter.composite(ctx, cache, view);
    expect(screen.ctx.calls.find((c) => c.op === 'fill')?.composite).toBe('lighten');
  });

  it('nests clips and transforms, ignoring unbalanced pops', () => {
    const { cache, painter, made, ctx } = setup(
      base([
        { PushTransform: { matrix: [1, 0, 0, 1, 5, 0] } },
        { PushClip: { rect: { x: 0, y: 0, w: 10, h: 10 }, path: null } },
        { FillRect: { rect: { x: 0, y: 0, w: 10, h: 10 }, paint: { Rgba: 0xff0000ff }, radius: 0 } },
        'PopClip',
        { Glyphs: { page: 0, paint: { Role: 'Ink' }, quads: [0, 0, 1, 1, 3, 3] } },
        'PopTransform',
        'PopTransform',
        'PopClip',
      ]),
    );
    painter.composite(ctx, cache, { width: 1000, height: 900, scrollX: 0, scrollY: 0, viewScale: 2 });
    const tc = made.find((c) => c.width === 200)?.ctx as RecordingContext;
    const fill = tc.calls.find((c) => c.op === 'fill');
    expect(fill?.transform).toEqual([2, 0, 0, 2, 10, 0]);
    expect(tc.calls.some((c) => c.op === 'clip')).toBe(true);
    // The glyph moves by the transform's device translation (5 × 2), unscaled.
    expect(tc.calls.find((c) => c.op === 'drawImage')?.args.slice(5, 7)).toEqual([13, 3]);
  });

  it('computes page shadows once per geometry and only outside the page', () => {
    const shadow: DrawOp = { Shadow: { rect: { x: 60, y: 30, w: 100, h: 100 }, radius: 0, style: 'Page' } };
    const d = base([shadow]);
    d.scale = 1;
    const tiles = d.tiles;
    d.tiles = [
      { ...(tiles[0] as SceneDelta['tiles'][number]), rect: { x: 0, y: 0, w: 220, h: 220 } },
      {
        ...(tiles[0] as SceneDelta['tiles'][number]),
        id: { layer: 0, index: 1 },
        rect: { x: 0, y: 300, w: 220, h: 220 },
        hash: 8,
      },
    ];
    const { cache, painter, made, ctx } = setup(d);
    painter.composite(ctx, cache, { width: 1000, height: 900, scrollX: 0, scrollY: 0, viewScale: 1 });
    // Two light shadow layers in four bands each, computed once and reused by the second page.
    const pieces = made.filter((c) => c.ctx.calls.some((k) => k.op === 'putImageData'));
    expect(pieces).toHaveLength(8);
    const tileCanvases = made.filter((c) => c.ctx.calls.some((k) => k.op === 'drawImage'));
    expect(tileCanvases.map((c) => c.ctx.calls.filter((k) => k.op === 'drawImage').length)).toEqual([8, 8]);
    // No band covers the page's interior; the band below the page has shadow.
    const tile = tileCanvases[0] as RecordingCanvas;
    const placed = tile.ctx.calls
      .filter((k) => k.op === 'drawImage')
      .map((k) => ({ c: k.args[0] as RecordingCanvas, x: k.args[1] as number, y: k.args[2] as number }));
    for (const p of placed) {
      const inside = p.x < 160 && p.x + p.c.width > 60 && p.y < 130 && p.y + p.c.height > 30;
      const overlapsInterior = p.x + p.c.width > 61 && p.x < 159 && p.y + p.c.height > 31 && p.y < 129;
      expect(inside && overlapsInterior && p.c.width > 2 && p.c.height > 2).toBe(false);
    }
    const below = placed
      .filter((p) => p.y >= 129 && p.y + p.c.height > 140 && p.x <= 110 && p.x + p.c.width > 110)
      .sort((a, b) => b.c.height - a.c.height)[0];
    if (!below) throw new Error('no band below the page');
    const img = below.c.ctx.calls.find((k) => k.op === 'putImageData')?.args[0] as {
      data: Uint8ClampedArray;
    };
    const alphaAt = (x: number, y: number) =>
      img.data[((y - below.y) * below.c.width + (x - below.x)) * 4 + 3];
    expect(alphaAt(110, 140)).toBeGreaterThan(0);
  });

  it('paints a real frame of lorem.txt without errors', () => {
    const delta = decodeAs<SceneDelta>('SceneDelta', new Uint8Array(readFileSync(FRAME)));
    const { cache, painter, made, ctx } = setup(delta);
    const drawn = painter.composite(ctx, cache, {
      width: 1000,
      height: 900,
      scrollX: -28,
      scrollY: 0,
      viewScale: 1,
    });
    expect(drawn).toBeGreaterThan(0);
    // The page's text went through one glyph layer with thousands of inked pixels.
    const inked = made
      .flatMap((c) => c.ctx.calls.filter((k) => k.op === 'putImageData'))
      .map(
        (k) => (k.args[0] as { data: Uint8ClampedArray }).data.filter((_, i) => i % 4 === 3 && _ > 0).length,
      );
    expect(Math.max(...inked)).toBeGreaterThan(5000);
  });
});
