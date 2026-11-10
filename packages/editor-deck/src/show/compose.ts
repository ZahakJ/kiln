/**
 * Paints a show frame: the core's composition (groups of layers, each a tile of the frame's scene
 * or a colour, with a matrix, an opacity and a clip) over a black screen with the slide fitted
 * and centred, then the ink. Tiles are painted by the shared {@link Painter} into canvases once;
 * a frame only draws them where the composition says. A group fading with several layers is
 * composed on its own canvas first, so overlapping shapes fade as one picture (as PowerPoint
 * fades a slide).
 */
import type {
  DeckShowFrame,
  DeckShowGroup,
  DeckShowLayer,
  DeckShowRamp,
  Paint,
  PathData,
} from '@kiln/protocol';
import {
  type Ctx2D,
  cssColor,
  type Painter,
  type PainterCanvas,
  resolvePaint,
  type SceneCache,
  tileKey,
  tracePath,
} from '@kiln/surface';

/** Where the slide goes on a screen of `width` × `height` device pixels. */
export interface SlideFit {
  /** Device pixels per slide px. */
  k: number;
  /** Left and top of the slide, whole device pixels. */
  ox: number;
  oy: number;
}

/** The slide fitted into the screen and centred (letterboxed). */
export function fitSlide(sw: number, sh: number, width: number, height: number): SlideFit {
  const k = sw > 0 && sh > 0 ? Math.min(width / sw, height / sh) : 1;
  return { k, ox: Math.round((width - sw * k) / 2), oy: Math.round((height - sh * k) / 2) };
}

/** A slide point of a client point on the screen element (letterboxing undone). */
export function toSlidePoint(
  fit: SlideFit,
  dpr: number,
  rect: { left: number; top: number },
  client: { x: number; y: number },
): { x: number; y: number } {
  return {
    x: ((client.x - rect.left) * dpr - fit.ox) / fit.k,
    y: ((client.y - rect.top) * dpr - fit.oy) / fit.k,
  };
}

type M = [number, number, number, number, number, number];

/** `a × b` in canvas `transform()` order. */
export function multiply(a: readonly number[], b: readonly number[]): M {
  const [a0 = 1, a1 = 0, a2 = 0, a3 = 1, a4 = 0, a5 = 0] = a;
  const [b0 = 1, b1 = 0, b2 = 0, b3 = 1, b4 = 0, b5 = 0] = b;
  return [
    a0 * b0 + a2 * b1,
    a1 * b0 + a3 * b1,
    a0 * b2 + a2 * b3,
    a1 * b2 + a3 * b3,
    a0 * b4 + a2 * b5 + a4,
    a1 * b4 + a3 * b5 + a5,
  ];
}

function setMatrix(ctx: Ctx2D, m: readonly number[]): void {
  const [a = 1, b = 0, c = 0, d = 1, e = 0, f = 0] = m;
  ctx.setTransform(a, b, c, d, e, f);
}

function clipTo(ctx: Ctx2D, path: PathData): void {
  if (typeof Path2D === 'undefined') return;
  const p = new Path2D();
  if (tracePath(path, p)) ctx.clip(p);
}

/**
 * A canvas the size of the screen for composing a group (or a layer) on its own before it is
 * faded or masked; `slot` tells the two apart (both can be in use at once). Kept between frames.
 */
export type ScratchFactory = (width: number, height: number, slot: 'group' | 'layer') => PainterCanvas | null;

/** A ramp with offsets beyond 0–1 folded into its line (gradient stops must lie in 0–1). */
export function normalRamp(r: DeckShowRamp): DeckShowRamp {
  const offsets = r.stops.map(([o]) => o);
  const lo = Math.min(0, ...offsets);
  const hi = Math.max(1, ...offsets);
  if (lo === 0 && hi === 1) return r;
  const span = hi - lo;
  const at = (t: number): [number, number] => [
    r.from[0] + (r.to[0] - r.from[0]) * t,
    r.from[1] + (r.to[1] - r.from[1]) * t,
  ];
  return { from: at(lo), to: at(hi), stops: r.stops.map(([o, a]) => [(o - lo) / span, a]) };
}

/** Keeps what `ctx` holds only as far as the ramp's opacity (`m` maps the ramp's coordinates). */
function mask(ctx: Ctx2D, ramp: DeckShowRamp, m: readonly number[]): void {
  const r = normalRamp(ramp);
  ctx.save();
  setMatrix(ctx, m);
  const g = ctx.createLinearGradient(r.from[0], r.from[1], r.to[0], r.to[1]);
  for (const [o, a] of r.stops)
    g.addColorStop(Math.max(0, Math.min(1, o)), `rgba(0,0,0,${Math.max(0, Math.min(1, a))})`);
  ctx.globalCompositeOperation = 'destination-in';
  ctx.globalAlpha = 1;
  ctx.fillStyle = g;
  ctx.fillRect(-1e5, -1e5, 2e5, 2e5);
  ctx.restore();
}

/** A fresh scratch context, cleared, with nothing set. */
function scratchContext(
  scratch: ScratchFactory | undefined,
  width: number,
  height: number,
  slot: 'group' | 'layer',
) {
  const canvas = scratch?.(width, height, slot);
  const ctx = canvas ? (canvas.getContext('2d') as Ctx2D | null) : null;
  if (!canvas || !ctx) return null;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = 1;
  ctx.globalCompositeOperation = 'source-over';
  ctx.clearRect(0, 0, width, height);
  return { canvas, ctx };
}

export interface ComposeTarget {
  ctx: Ctx2D;
  width: number;
  height: number;
}

function drawLayer(
  t: ComposeTarget,
  painter: Painter,
  cache: SceneCache<PainterCanvas>,
  base: M,
  group: DeckShowGroup,
  layer: DeckShowLayer,
  alpha: number,
  sw: number,
  sh: number,
  scratch?: ScratchFactory,
): void {
  const opacity = Math.max(0, Math.min(1, layer.opacity * alpha));
  if (opacity <= 0) return;
  const m = multiply(multiply(base, group.matrix), layer.matrix);
  if (layer.ramp) {
    // A soft edge: the layer on its own, masked, then laid on.
    const s = scratchContext(scratch, t.width, t.height, 'layer');
    if (s) {
      drawLayer(
        { ctx: s.ctx, width: t.width, height: t.height },
        painter,
        cache,
        base,
        group,
        { ...layer, ramp: null, opacity: 1 },
        1,
        sw,
        sh,
      );
      mask(s.ctx, layer.ramp, m);
      t.ctx.save();
      t.ctx.setTransform(1, 0, 0, 1, 0, 0);
      t.ctx.globalAlpha = opacity;
      t.ctx.drawImage(s.canvas as unknown as CanvasImageSource, 0, 0);
      t.ctx.restore();
      return;
    }
  }
  const { ctx } = t;
  ctx.save();
  setMatrix(ctx, m);
  if (layer.clip) clipTo(ctx, layer.clip);
  ctx.globalAlpha = opacity;
  if (layer.tile) {
    const cached = cache.tiles.get(tileKey(layer.tile));
    const canvas = cached ? painter.ensureTile(cache, cached) : null;
    if (cached && canvas) {
      const r = cached.tile.rect;
      // One canvas pixel per device pixel at rest: its size over the scene's scale.
      ctx.drawImage(
        canvas as unknown as CanvasImageSource,
        r.x,
        r.y,
        canvas.width / cache.scale,
        canvas.height / cache.scale,
      );
    }
  } else if (layer.fill) {
    ctx.fillStyle = cssColor(resolvePaint(painter.palette, layer.fill as Paint));
    ctx.fillRect(0, 0, sw, sh);
  }
  ctx.restore();
}

/**
 * Paints `frame` on a screen. `scratch` gives a second canvas of the screen's size for groups
 * that fade with several layers (without one they fade layer by layer).
 */
export function composeFrame(
  target: ComposeTarget,
  painter: Painter,
  cache: SceneCache<PainterCanvas>,
  frame: DeckShowFrame,
  scratch?: ScratchFactory,
): SlideFit {
  const { ctx, width, height } = target;
  const sw = frame.slide_width;
  const sh = frame.slide_height;
  const fit = fitSlide(sw, sh, width, height);
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = 1;
  ctx.globalCompositeOperation = 'source-over';
  ctx.fillStyle = '#000';
  ctx.fillRect(0, 0, width, height);
  const base: M = [fit.k, 0, 0, fit.k, fit.ox, fit.oy];
  ctx.save();
  setMatrix(ctx, base);
  ctx.beginPath();
  ctx.rect(0, 0, sw, sh);
  ctx.clip();
  for (const g of frame.groups) {
    const opacity = Math.max(0, Math.min(1, g.opacity));
    if (opacity <= 0) continue;
    const alone = (opacity < 1 && g.layers.length > 1) || g.ramp !== null;
    const s = alone ? scratchContext(scratch, width, height, 'group') : null;
    if (s) {
      const t2: ComposeTarget = { ctx: s.ctx, width, height };
      s.ctx.save();
      if (g.clip) {
        setMatrix(s.ctx, base);
        clipTo(s.ctx, g.clip);
      }
      for (const l of g.layers) drawLayer(t2, painter, cache, base, g, l, 1, sw, sh, scratch);
      s.ctx.restore();
      if (g.ramp) mask(s.ctx, g.ramp, base);
      ctx.save();
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.globalAlpha = opacity;
      ctx.drawImage(s.canvas as unknown as CanvasImageSource, 0, 0);
      ctx.restore();
      continue;
    }
    ctx.save();
    if (g.clip) {
      setMatrix(ctx, base);
      clipTo(ctx, g.clip);
    }
    for (const l of g.layers) drawLayer(target, painter, cache, base, g, l, opacity, sw, sh, scratch);
    ctx.restore();
  }
  if (frame.ink.length > 0) {
    setMatrix(ctx, base);
    painter.paintOps(ctx, frame.ink, cache, {
      scale: fit.k,
      glyphOrigin: [fit.ox, fit.oy],
      caretVisible: false,
      touch: false,
    });
  }
  ctx.restore();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = 1;
  return fit;
}
