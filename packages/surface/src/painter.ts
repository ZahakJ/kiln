/**
 * The Canvas 2D painter: executes Scene ops. It is the twin of `kiln_raster::OpPainter` and uses
 * the same pixel rules, so the two agree pixel for pixel:
 *
 * - tiles render once into their own canvas of `ceil(w × scale) × ceil(h × scale)` device pixels
 *   and are composited at `round(x × scale) − round(scroll × scale)`;
 * - glyph quads are copied 1:1 (no resampling) from atlas pages tinted per colour, in device
 *   pixels relative to the tile's device origin;
 * - shadows are exact Gaussian box shadows computed per pixel (cached per geometry);
 * - the selection wash blends with `darken`/`lighten`, as the palette says.
 */

import type {
  Bounds,
  DrawOp,
  GlyphRun,
  Gradient,
  Palette,
  PathData,
  Shadow,
  ShadowLayer,
} from '@kiln/protocol';
import { channels, cssColor, resolvePaint } from './palette';
import type { AtlasPage, CachedImage, CachedTile, SceneCache } from './scene-cache';
import {
  blurredInterval,
  caretSpark,
  roundedRect,
  squiggle,
  teardrop,
  tracePath,
  underlineSegments,
} from './shapes';

/** The 2D contexts the painter draws with. */
export type Ctx2D = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

/** A canvas the painter can create and draw into. */
export interface PainterCanvas {
  width: number;
  height: number;
  getContext(kind: '2d'): Ctx2D | null;
}

/** Creates canvases (offscreen when available). */
export type CanvasFactory = (width: number, height: number) => PainterCanvas;

/** The default factory: `OffscreenCanvas`, or a detached `<canvas>`. */
export const defaultCanvasFactory: CanvasFactory = (width, height) => {
  if (typeof OffscreenCanvas !== 'undefined') {
    return new OffscreenCanvas(Math.max(1, width), Math.max(1, height)) as unknown as PainterCanvas;
  }
  const c = document.createElement('canvas');
  c.width = Math.max(1, width);
  c.height = Math.max(1, height);
  return c as unknown as PainterCanvas;
};

/** Largest bitmap side. */
export const MAX_SIDE = 16384;

/** Device position of a document coordinate (shared rounding with kiln-raster). */
export function device(v: number, scale: number): number {
  return Math.round(Math.fround(v * scale));
}

/** Device size of a tile's bitmap. */
export function tileDeviceSize(rect: Bounds, scale: number): [number, number] {
  const f = (v: number) => Math.min(MAX_SIDE, Math.max(1, Math.ceil(Math.fround(v * scale))));
  return [f(rect.w), f(rect.h)];
}

/** Options of a paint pass. */
export interface PaintState {
  /** Device pixels per document pixel. */
  scale: number;
  /** Device position glyph runs are relative to. */
  glyphOrigin: [number, number];
  /** Draw carets (the blink's on phase, and only while focused). */
  caretVisible: boolean;
  /** Draw touch handles. */
  touch: boolean;
}

/** Glyphs composed between two yields of a tile job (about a millisecond of work). */
const GLYPHS_PER_STEP = 400;

/** A band of a shadow layer at its device position in the tile. */
interface ShadowPiece {
  canvas: PainterCanvas;
  x: number;
  y: number;
}

/** Byte order of `Uint32Array` views (every platform Kiln runs on is little-endian; checked). */
const LITTLE_ENDIAN = new Uint8Array(new Uint32Array([1]).buffer)[0] === 1;

/**
 * Device-pixel bounds of overlay marks (with a margin for anti-aliasing and the caret's spark),
 * or `all` for marks without known bounds.
 */
function marksBounds(ops: readonly DrawOp[], scale: number, ox: number, oy: number): Bounds | 'all' | null {
  let x0 = Number.POSITIVE_INFINITY;
  let y0 = Number.POSITIVE_INFINITY;
  let x1 = Number.NEGATIVE_INFINITY;
  let y1 = Number.NEGATIVE_INFINITY;
  const add = (x: number, y: number, w: number, h: number) => {
    x0 = Math.min(x0, x);
    y0 = Math.min(y0, y);
    x1 = Math.max(x1, x + w);
    y1 = Math.max(y1, y + h);
  };
  for (const op of ops) {
    if (typeof op !== 'object') continue;
    if ('Caret' in op) {
      const r = op.Caret.rect;
      add(r.x - 6, r.y - 9, r.w + 12, r.h + 10);
    } else if ('Underline' in op) {
      const u = op.Underline;
      add(u.x, u.y, u.width, u.thickness);
    } else if ('Squiggle' in op) {
      const q = op.Squiggle;
      add(q.x, q.y - 3, q.width, 6);
    } else if ('Handle' in op) {
      const h = op.Handle;
      add(h.x - h.size * 1.5, h.y - 1, h.size * 3, h.size * 2);
    } else if ('FillRect' in op) {
      const r = op.FillRect.rect;
      add(r.x, r.y, r.w, r.h);
    } else {
      return 'all';
    }
  }
  if (!Number.isFinite(x0)) return null;
  const m = 2;
  const dx0 = Math.floor(x0 * scale - ox) - m;
  const dy0 = Math.floor(y0 * scale - oy) - m;
  return { x: dx0, y: dy0, w: Math.ceil(x1 * scale - ox) + m - dx0, h: Math.ceil(y1 * scale - oy) + m - dy0 };
}

/** Distance from a point to a rectangle (0 inside). */
function dist(r: Bounds, x: number, y: number): number {
  const dx = Math.max(r.x - x, 0, x - (r.x + r.w));
  const dy = Math.max(r.y - y, 0, y - (r.y + r.h));
  return Math.hypot(dx, dy);
}

/** Whether quad `i` of a glyph run lies inside its atlas page. */
function validQuad(q: readonly number[], i: number, page: AtlasPage): boolean {
  const sx = q[i] as number;
  const sy = q[i + 1] as number;
  const w = q[i + 2] as number;
  const h = q[i + 3] as number;
  return w > 0 && h > 0 && sx >= 0 && sy >= 0 && sx + w <= page.width && sy + h <= page.height;
}

function makePath(p: PathData): Path2D | null {
  if (typeof Path2D === 'undefined') return null;
  const path = new Path2D();
  return tracePath(p, path) ? path : null;
}

/** Executes ops and composites tiles. Holds derived canvases (tinted atlas pages, shadows). */
export class Painter {
  palette: Palette;
  private readonly tinted = new WeakMap<AtlasPage, Map<number, { version: number; canvas: PainterCanvas }>>();
  private readonly colourPages = new WeakMap<AtlasPage, { version: number; canvas: PainterCanvas }>();
  private readonly imageCanvases = new WeakMap<CachedImage, PainterCanvas>();
  private readonly shadows = new Map<string, ShadowPiece[]>();
  private readonly jobs = new WeakMap<object, Generator<void, PainterCanvas | null>>();
  /** Tiles rendered since creation (diagnostics). */
  tilesRendered = 0;

  constructor(
    palette: Palette,
    private readonly factory: CanvasFactory = defaultCanvasFactory,
  ) {
    this.palette = palette;
  }

  /** Switches the palette; rendered tiles must be invalidated by the caller. */
  setPalette(palette: Palette): void {
    if (palette.name !== this.palette.name) {
      this.palette = palette;
      this.shadows.clear();
    }
  }

  /** Whether a tile's canvas is rendered for the current palette. */
  isReady<C extends PainterCanvas>(t: CachedTile<C>): boolean {
    return t.canvas !== null && t.palette === this.palette.name;
  }

  /** Renders a tile into its canvas now if it has none for the current palette (finishing a
   * job already under way). */
  ensureTile<C extends PainterCanvas>(cache: SceneCache<C>, t: CachedTile<C>): C | null {
    if (this.isReady(t)) return t.canvas;
    const job = (this.jobs.get(t) as Generator<void, C | null> | undefined) ?? this.tileJob(cache, t);
    this.jobs.delete(t);
    let step = job.next();
    while (!step.done) step = job.next();
    return step.value;
  }

  /**
   * Spends time until `deadline` (a `performance.now()` value) rendering tiles that are not ready,
   * nearest to `around` first, in resumable steps. Returns whether any work remains.
   */
  prefetch<C extends PainterCanvas>(cache: SceneCache<C>, around: Bounds, deadline: number): boolean {
    const cx = around.x + around.w / 2;
    const cy = around.y + around.h / 2;
    const pending = [...cache.tiles.values()]
      .filter((t) => !this.isReady(t))
      .sort((a, b) => dist(a.tile.rect, cx, cy) - dist(b.tile.rect, cx, cy));
    for (const t of pending) {
      let job = this.jobs.get(t) as Generator<void, C | null> | undefined;
      if (!job) {
        job = this.tileJob(cache, t);
        this.jobs.set(t, job);
      }
      while (performance.now() < deadline) {
        if (job.next().done) {
          this.jobs.delete(t);
          break;
        }
      }
      if (performance.now() >= deadline) return true;
    }
    return false;
  }

  /**
   * Renders a tile in resumable steps: the generator yields between chunks of a few hundred
   * glyphs, so a caller can spread a tile over several frames (prefetched tiles are rendered in
   * the spare time of animation frames while scrolling). Returns the canvas when done.
   */
  *tileJob<C extends PainterCanvas>(cache: SceneCache<C>, t: CachedTile<C>): Generator<void, C | null> {
    const [w, h] = tileDeviceSize(t.tile.rect, cache.scale);
    const canvas = this.factory(w, h) as C;
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    const palette = this.palette.name;
    ctx.setTransform(cache.scale, 0, 0, cache.scale, 0, 0);
    yield* this.paintOpsSteps(ctx, t.tile.ops, cache as SceneCache<PainterCanvas>, {
      scale: cache.scale,
      glyphOrigin: [0, 0],
      caretVisible: true,
      touch: false,
    });
    if (palette !== this.palette.name) return null;
    t.canvas = canvas;
    t.palette = palette;
    this.tilesRendered += 1;
    return canvas;
  }

  /**
   * Paints the background and the cached tiles visible in a `width × height` device-pixel view
   * scrolled to `(scrollX, scrollY)` (document pixels) at `viewScale` device pixels per document
   * pixel. Tiles of a scene at another scale (during a pinch) are drawn stretched until the new
   * frame arrives. Returns the number of tiles drawn.
   */
  composite<C extends PainterCanvas>(
    ctx: Ctx2D,
    cache: SceneCache<C>,
    view: { width: number; height: number; scrollX: number; scrollY: number; viewScale: number },
  ): number {
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.globalCompositeOperation = 'source-over';
    ctx.fillStyle = cssColor(resolvePaint(this.palette, cache.background));
    ctx.fillRect(0, 0, view.width, view.height);
    const s = cache.scale;
    const vs = view.viewScale;
    const visible: Bounds = { x: view.scrollX, y: view.scrollY, w: view.width / vs, h: view.height / vs };
    let drawn = 0;
    const exact = Math.abs(vs - s) < 1e-6;
    const ox = device(view.scrollX, s);
    const oy = device(view.scrollY, s);
    for (const t of cache.tilesIn(visible)) {
      const canvas = this.ensureTile(cache, t);
      if (!canvas) continue;
      const src = canvas as unknown as CanvasImageSource;
      if (exact) {
        ctx.drawImage(src, device(t.tile.rect.x, s) - ox, device(t.tile.rect.y, s) - oy);
      } else {
        const ratio = vs / s;
        ctx.drawImage(
          src,
          (t.tile.rect.x - view.scrollX) * vs,
          (t.tile.rect.y - view.scrollY) * vs,
          canvas.width * ratio,
          canvas.height * ratio,
        );
      }
      drawn += 1;
    }
    // The selection wash goes onto the content (blended per the palette, so it reads as painted
    // under the text, exactly as kiln-raster paints it); the other overlay marks go onto the
    // overlay canvas, which blinks without recompositing the tiles.
    const wash = cache.overlay.filter((op) => typeof op === 'object' && 'SelectionRects' in op);
    if (wash.length > 0) {
      const ovx = device(view.scrollX, vs);
      const ovy = device(view.scrollY, vs);
      ctx.setTransform(vs, 0, 0, vs, -ovx, -ovy);
      this.paintOps(ctx, wash, cache, {
        scale: vs,
        glyphOrigin: [-ovx, -ovy],
        caretVisible: false,
        touch: false,
      });
      ctx.setTransform(1, 0, 0, 1, 0, 0);
    }
    return drawn;
  }

  /**
   * Paints the overlay marks (caret, composition underlines, squiggles, handles; the selection
   * wash is part of {@link Painter.composite}) over a view.
   */
  paintOverlay(
    ctx: Ctx2D,
    cache: SceneCache<PainterCanvas>,
    view: { width: number; height: number; scrollX: number; scrollY: number; viewScale: number },
    state: { caretVisible: boolean; touch: boolean },
  ): void {
    const vs = view.viewScale;
    const ox = device(view.scrollX, vs);
    const oy = device(view.scrollY, vs);
    const marks = cache.overlay.filter((op) => !(typeof op === 'object' && 'SelectionRects' in op));
    // Only the pixels the previous marks covered and the new ones will cover are touched: the
    // overlay is usually just a caret, and clearing a whole surface every frame is not free.
    const next = marksBounds(marks, vs, ox, oy);
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    const prev = this.overlayDirty;
    if (prev === 'all' || next === 'all') {
      ctx.clearRect(0, 0, view.width, view.height);
    } else if (prev) {
      ctx.clearRect(prev.x, prev.y, prev.w, prev.h);
    }
    this.overlayDirty = next;
    if (marks.length === 0) return;
    ctx.setTransform(vs, 0, 0, vs, -ox, -oy);
    this.paintOps(ctx, marks, cache, {
      scale: vs,
      glyphOrigin: [-ox, -oy],
      caretVisible: state.caretVisible,
      touch: state.touch,
    });
    ctx.setTransform(1, 0, 0, 1, 0, 0);
  }

  /** Device rectangle the last overlay pass painted (`all` when unknown). */
  private overlayDirty: Bounds | 'all' | null = 'all';

  /** Paints ops with the context's current transform as the base. */
  paintOps(ctx: Ctx2D, ops: readonly DrawOp[], cache: SceneCache<PainterCanvas>, state: PaintState): void {
    const steps = this.paintOpsSteps(ctx, ops, cache, state);
    while (!steps.next().done) {
      // drain
    }
  }

  /** {@link Painter.paintOps} in resumable steps (see {@link Painter.tileJob}). */
  *paintOpsSteps(
    ctx: Ctx2D,
    ops: readonly DrawOp[],
    cache: SceneCache<PainterCanvas>,
    state: PaintState,
  ): Generator<void, void> {
    let depth = 0;
    const base = ctx.getTransform();
    // Consecutive glyph runs outside clips and transforms are composed on the CPU into one layer
    // (see `glyphLayer`).
    let batch: GlyphRun[] = [];
    for (const op of ops) {
      if (depth === 0 && typeof op === 'object' && 'Glyphs' in op) {
        batch.push(op.Glyphs);
        continue;
      }
      if (batch.length > 0) {
        yield* this.glyphLayer(ctx, batch, cache, state);
        batch = [];
      }
      if (op === 'PopClip' || op === 'PopTransform') {
        if (depth > 0) {
          ctx.restore();
          depth -= 1;
        }
        continue;
      }
      if ('PushClip' in op) {
        ctx.save();
        depth += 1;
        const { rect, path } = op.PushClip;
        const p = path ? makePath(path) : null;
        if (p) {
          ctx.clip(p);
        } else {
          ctx.beginPath();
          if (!path) ctx.rect(rect.x, rect.y, rect.w, rect.h);
          ctx.clip();
        }
        continue;
      }
      if ('PushTransform' in op) {
        ctx.save();
        depth += 1;
        const m = op.PushTransform.matrix;
        if (m.every(Number.isFinite)) ctx.transform(m[0], m[1], m[2], m[3], m[4], m[5]);
        continue;
      }
      this.paintOp(ctx, op, cache, state, base.e, base.f);
    }
    if (batch.length > 0) yield* this.glyphLayer(ctx, batch, cache, state);
    while (depth > 0) {
      ctx.restore();
      depth -= 1;
    }
  }

  private fill(ctx: Ctx2D, rgba: number): void {
    ctx.fillStyle = cssColor(rgba);
  }

  private fillBounds(ctx: Ctx2D, b: Bounds, radius: number, rgba: number): void {
    if (!(b.w > 0 && b.h > 0)) return;
    const p = makePath(roundedRect(b, radius));
    if (!p) return;
    this.fill(ctx, rgba);
    ctx.fill(p);
  }

  private paintOp(
    ctx: Ctx2D,
    op: Exclude<DrawOp, 'PopClip' | 'PopTransform'>,
    cache: SceneCache<PainterCanvas>,
    state: PaintState,
    baseE: number,
    baseF: number,
  ): void {
    const pal = this.palette;
    if ('FillRect' in op) {
      this.fillBounds(ctx, op.FillRect.rect, op.FillRect.radius, resolvePaint(pal, op.FillRect.paint));
    } else if ('StrokeRect' in op) {
      const p = makePath(roundedRect(op.StrokeRect.rect, op.StrokeRect.radius));
      if (!p) return;
      ctx.strokeStyle = cssColor(resolvePaint(pal, op.StrokeRect.paint));
      ctx.lineWidth = Math.max(0, op.StrokeRect.width);
      ctx.lineJoin = 'miter';
      ctx.lineCap = 'butt';
      ctx.setLineDash([]);
      ctx.stroke(p);
    } else if ('Path' in op) {
      const { path, fill, stroke, rule } = op.Path;
      const p = makePath(path);
      if (!p) return;
      if (fill) {
        this.fill(ctx, resolvePaint(pal, fill));
        ctx.fill(p, rule === 'EvenOdd' ? 'evenodd' : 'nonzero');
      }
      if (stroke) {
        ctx.strokeStyle = cssColor(resolvePaint(pal, stroke.paint));
        ctx.lineWidth = Math.max(0, stroke.width);
        ctx.lineJoin = stroke.join === 'Round' ? 'round' : stroke.join === 'Bevel' ? 'bevel' : 'miter';
        ctx.lineCap = stroke.cap === 'Round' ? 'round' : stroke.cap === 'Square' ? 'square' : 'butt';
        ctx.setLineDash(stroke.dash.length >= 2 ? stroke.dash : []);
        ctx.stroke(p);
        ctx.setLineDash([]);
      }
    } else if ('Glyphs' in op) {
      this.glyphs(ctx, op.Glyphs, cache, state, baseE, baseF);
    } else if ('OutlineGlyphs' in op) {
      const p = makePath(op.OutlineGlyphs.path);
      if (!p) return;
      this.fill(ctx, resolvePaint(pal, op.OutlineGlyphs.paint));
      ctx.fill(p);
    } else if ('Image' in op) {
      const { image, src, dst, opacity } = op.Image;
      const img = cache.images.get(image);
      if (!img || !(src.w > 0 && src.h > 0 && dst.w > 0 && dst.h > 0)) return;
      const canvas = this.imageCanvas(img);
      if (!canvas) return;
      ctx.globalAlpha = Math.min(1, Math.max(0, opacity));
      ctx.drawImage(
        canvas as unknown as CanvasImageSource,
        src.x,
        src.y,
        src.w,
        src.h,
        dst.x,
        dst.y,
        dst.w,
        dst.h,
      );
      ctx.globalAlpha = 1;
    } else if ('Gradient' in op) {
      this.gradient(ctx, op.Gradient.rect, op.Gradient.radius, op.Gradient.gradient);
    } else if ('Shadow' in op) {
      this.shadow(ctx, op.Shadow);
    } else if ('Squiggle' in op) {
      const p = makePath(squiggle(op.Squiggle.x, op.Squiggle.y, op.Squiggle.width));
      if (!p) return;
      ctx.strokeStyle = cssColor(resolvePaint(pal, op.Squiggle.paint));
      ctx.lineWidth = 1;
      ctx.lineJoin = 'miter';
      ctx.lineCap = 'butt';
      ctx.stroke(p);
    } else if ('Underline' in op) {
      const u = op.Underline;
      const rgba = resolvePaint(pal, u.paint);
      for (const [x, w] of underlineSegments(u.x, u.width, u.thickness, u.style)) {
        this.fillBounds(ctx, { x, y: u.y, w, h: u.thickness }, 0, rgba);
      }
    } else if ('Caret' in op) {
      if (!state.caretVisible) return;
      this.fillBounds(ctx, op.Caret.rect, 0, resolvePaint(pal, op.Caret.paint));
      if (op.Caret.spark) {
        const p = makePath(caretSpark(op.Caret.rect));
        if (p) {
          this.fill(ctx, pal.spark);
          ctx.fill(p);
        }
      }
    } else if ('SelectionRects' in op) {
      const rgba = resolvePaint(pal, op.SelectionRects.paint);
      ctx.globalCompositeOperation = pal.selection_blend === 'Darken' ? 'darken' : 'lighten';
      for (const r of op.SelectionRects.rects) this.fillBounds(ctx, r, 0, rgba);
      ctx.globalCompositeOperation = 'source-over';
    } else if ('Guide' in op) {
      const g = op.Guide;
      const w = 1 / Math.max(0.01, state.scale);
      ctx.strokeStyle = cssColor(resolvePaint(pal, g.paint));
      ctx.lineWidth = w;
      ctx.setLineDash(g.dashed ? [4 * w, 3 * w] : []);
      ctx.beginPath();
      ctx.moveTo(g.x0, g.y0);
      ctx.lineTo(g.x1, g.y1);
      ctx.stroke();
      ctx.setLineDash([]);
    } else if ('Handle' in op) {
      if (!state.touch) return;
      const h = op.Handle;
      const p = makePath(teardrop(h.x, h.y, h.size, h.side));
      if (!p) return;
      this.fill(ctx, resolvePaint(pal, h.paint));
      ctx.fill(p);
    }
  }

  private glyphs(
    ctx: Ctx2D,
    run: GlyphRun,
    cache: SceneCache<PainterCanvas>,
    state: PaintState,
    baseE: number,
    baseF: number,
  ): void {
    const page = cache.atlas.get(run.page);
    if (!page) return;
    const src = this.atlasCanvas(page, resolvePaint(this.palette, run.paint));
    if (!src) return;
    // Glyphs are device-positioned: a transform pushed inside the pass only moves them.
    const m = ctx.getTransform();
    const ox = state.glyphOrigin[0] + Math.round(m.e - baseE);
    const oy = state.glyphOrigin[1] + Math.round(m.f - baseF);
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    const q = run.quads;
    const image = src as unknown as CanvasImageSource;
    for (let i = 0; i + 5 < q.length; i += 6) {
      const sx = q[i] as number;
      const sy = q[i + 1] as number;
      const w = q[i + 2] as number;
      const h = q[i + 3] as number;
      if (w <= 0 || h <= 0 || sx < 0 || sy < 0 || sx + w > page.width || sy + h > page.height) continue;
      ctx.drawImage(image, sx, sy, w, h, ox + (q[i + 4] as number), oy + (q[i + 5] as number), w, h);
    }
    ctx.restore();
  }

  /**
   * Draws glyph runs by composing their quads on the CPU into one transparent layer, uploaded
   * with a single `putImageData` and composited with one `drawImage`. Source-over is
   * associative, so this equals blitting every glyph in turn (what kiln-raster does), and costs a
   * small fraction of thousands of `drawImage` calls on software-rendered canvases. Coverage is
   * read straight from the A8 atlas pages, so no tinted copies are needed.
   */
  private *glyphLayer(
    ctx: Ctx2D,
    runs: GlyphRun[],
    cache: SceneCache<PainterCanvas>,
    state: PaintState,
  ): Generator<void, void> {
    // Batches never span pushed transforms, so glyphs sit exactly at the pass's glyph origin.
    const originX = state.glyphOrigin[0];
    const originY = state.glyphOrigin[1];
    const cw = ctx.canvas.width;
    const ch = ctx.canvas.height;
    let x0 = Number.POSITIVE_INFINITY;
    let y0 = Number.POSITIVE_INFINITY;
    let x1 = Number.NEGATIVE_INFINITY;
    let y1 = Number.NEGATIVE_INFINITY;
    for (const run of runs) {
      const page = cache.atlas.get(run.page);
      if (!page) continue;
      const q = run.quads;
      for (let i = 0; i + 5 < q.length; i += 6) {
        if (!validQuad(q, i, page)) continue;
        const dx = originX + (q[i + 4] as number);
        const dy = originY + (q[i + 5] as number);
        x0 = Math.min(x0, dx);
        y0 = Math.min(y0, dy);
        x1 = Math.max(x1, dx + (q[i + 2] as number));
        y1 = Math.max(y1, dy + (q[i + 3] as number));
      }
    }
    x0 = Math.max(0, x0);
    y0 = Math.max(0, y0);
    x1 = Math.min(cw, x1);
    y1 = Math.min(ch, y1);
    if (!(x1 > x0 && y1 > y0)) return;
    const w = x1 - x0;
    const h = y1 - y0;
    // A layer per batch (not a shared scratch canvas): a paused job must not see another job's
    // pixels.
    const layer = this.factory(w, h);
    const lctx = layer.getContext('2d');
    if (!lctx) return;
    const img = lctx.createImageData(w, h);
    let budget = GLYPHS_PER_STEP;
    const out = img.data;
    const out32 = new Uint32Array(out.buffer, out.byteOffset, out.length / 4);
    const little = LITTLE_ENDIAN;
    const pack = (r: number, g: number, b: number, a: number) =>
      little ? ((a << 24) | (b << 16) | (g << 8) | r) >>> 0 : ((r << 24) | (g << 16) | (b << 8) | a) >>> 0;
    const table = new Uint32Array(256);
    for (const run of runs) {
      const page = cache.atlas.get(run.page);
      if (!page) continue;
      const [r, g, b, a] = channels(resolvePaint(this.palette, run.paint));
      const colour = page.format === 'Rgba8';
      if (!colour) {
        // Coverage → packed straight-alpha pixel, as kiln-raster computes alpha.
        for (let c = 0; c < 256; c++) table[c] = pack(r, g, b, Math.floor((c * a + 127) / 255));
      }
      const src = page.data;
      const pw = page.width;
      const q = run.quads;
      for (let i = 0; i + 5 < q.length; i += 6) {
        if (--budget <= 0) {
          budget = GLYPHS_PER_STEP;
          yield;
        }
        if (!validQuad(q, i, page)) continue;
        const sx = q[i] as number;
        const sy = q[i + 1] as number;
        const gw = q[i + 2] as number;
        const gh = q[i + 3] as number;
        const dx = originX + (q[i + 4] as number) - x0;
        const dy = originY + (q[i + 5] as number) - y0;
        // Clip the quad to the layer once instead of per pixel.
        const c0 = Math.max(0, -dx);
        const c1 = Math.min(gw, w - dx);
        const r0 = Math.max(0, -dy);
        const r1 = Math.min(gh, h - dy);
        for (let row = r0; row < r1; row++) {
          const sRow = (sy + row) * pw + sx;
          const dRow = (dy + row) * w + dx;
          for (let col = c0; col < c1; col++) {
            let sr: number;
            let sg: number;
            let sb: number;
            let sa: number;
            const d = dRow + col;
            if (colour) {
              const si = (sRow + col) * 4;
              sa = src[si + 3] as number;
              if (sa === 0) continue;
              sr = src[si] as number;
              sg = src[si + 1] as number;
              sb = src[si + 2] as number;
              if (out32[d] === 0 || sa === 255) {
                out32[d] = pack(sr, sg, sb, sa);
                continue;
              }
            } else {
              const cov = src[sRow + col] as number;
              if (cov === 0) continue;
              const px = table[cov] as number;
              if (out32[d] === 0) {
                out32[d] = px;
                continue;
              }
              sr = r;
              sg = g;
              sb = b;
              sa = Math.floor((cov * a + 127) / 255);
              if (sa === 255) {
                out32[d] = px;
                continue;
              }
            }
            // Straight-alpha source-over within the layer (overlapping glyph edges).
            const di = d * 4;
            const da = out[di + 3] as number;
            const inv = 255 - sa;
            const oa = sa + (da * inv + 127) / 255;
            const k = (da * inv) / 255;
            out[di] = (sr * sa + (out[di] as number) * k) / oa;
            out[di + 1] = (sg * sa + (out[di + 1] as number) * k) / oa;
            out[di + 2] = (sb * sa + (out[di + 2] as number) * k) / oa;
            out[di + 3] = oa;
          }
        }
      }
    }
    lctx.putImageData(img, 0, 0);
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.drawImage(layer as unknown as CanvasImageSource, 0, 0, w, h, x0, y0, w, h);
    ctx.restore();
  }

  private atlasCanvas(page: AtlasPage, rgba: number): PainterCanvas | null {
    if (page.format === 'Rgba8') {
      const cached = this.colourPages.get(page);
      if (cached && cached.version === page.version) return cached.canvas;
      const canvas = this.pixelsCanvas(page.width, page.rows, (data) => {
        data.set(page.data.subarray(0, page.width * page.rows * 4));
      });
      if (canvas) this.colourPages.set(page, { version: page.version, canvas });
      return canvas;
    }
    let byColour = this.tinted.get(page);
    if (!byColour) {
      byColour = new Map();
      this.tinted.set(page, byColour);
    }
    const cached = byColour.get(rgba);
    if (cached && cached.version === page.version) return cached.canvas;
    const [r, g, b, a] = channels(rgba);
    const canvas = this.pixelsCanvas(page.width, page.rows, (data) => {
      const cov = page.data;
      const n = page.width * page.rows;
      for (let i = 0; i < n; i++) {
        const c = cov[i] as number;
        if (c === 0) continue;
        const o = i * 4;
        data[o] = r;
        data[o + 1] = g;
        data[o + 2] = b;
        data[o + 3] = Math.floor((c * a + 127) / 255);
      }
    });
    if (canvas) {
      // A handful of text colours per page; bound the cache anyway.
      if (byColour.size > 16) byColour.clear();
      byColour.set(rgba, { version: page.version, canvas });
    }
    return canvas;
  }

  private pixelsCanvas(
    width: number,
    height: number,
    fill: (data: Uint8ClampedArray) => void,
  ): PainterCanvas | null {
    if (width <= 0 || height <= 0) return null;
    const canvas = this.factory(width, height);
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    const img = ctx.createImageData(width, height);
    fill(img.data);
    ctx.putImageData(img, 0, 0);
    return canvas;
  }

  private imageCanvas(img: CachedImage): PainterCanvas | null {
    const cached = this.imageCanvases.get(img);
    if (cached) return cached;
    const canvas = this.pixelsCanvas(img.width, img.height, (data) => data.set(img.data));
    if (canvas) this.imageCanvases.set(img, canvas);
    return canvas;
  }

  private gradient(ctx: Ctx2D, rect: Bounds, radius: number, g: Gradient): void {
    const p = makePath(roundedRect(rect, radius));
    if (!p || g.stops.length === 0) return;
    let grad: CanvasGradient;
    if ('Linear' in g.kind) {
      const { x0, y0, x1, y1 } = g.kind.Linear;
      grad = ctx.createLinearGradient(x0, y0, x1, y1);
    } else {
      const { cx, cy, r } = g.kind.Radial;
      grad = ctx.createRadialGradient(cx, cy, 0, cx, cy, Math.max(0, r));
    }
    for (const stop of g.stops) {
      if (Number.isFinite(stop.offset)) {
        grad.addColorStop(
          Math.min(1, Math.max(0, stop.offset)),
          cssColor(resolvePaint(this.palette, stop.paint)),
        );
      }
    }
    ctx.fillStyle = grad;
    ctx.fill(p);
  }

  private shadow(ctx: Ctx2D, shadow: Shadow): void {
    const layers: ShadowLayer[] =
      shadow.style === 'Page'
        ? this.palette.page_shadow
        : [
            {
              dx: shadow.style.Drop.dx,
              dy: shadow.style.Drop.dy,
              blur: shadow.style.Drop.blur,
              spread: shadow.style.Drop.spread,
              rgba: resolvePaint(this.palette, shadow.style.Drop.paint),
            },
          ];
    const t = ctx.getTransform();
    if (t.b !== 0 || t.c !== 0) return;
    const canvasW = ctx.canvas.width;
    const canvasH = ctx.canvas.height;
    const dev = (b: Bounds): [number, number, number, number] => {
      const x0 = Math.fround(b.x * t.a + t.e);
      const y0 = Math.fround(b.y * t.d + t.f);
      const x1 = Math.fround((b.x + b.w) * t.a + t.e);
      const y1 = Math.fround((b.y + b.h) * t.d + t.f);
      return [Math.min(x0, x1), Math.min(y0, y1), Math.max(x0, x1), Math.max(y0, y1)];
    };
    const caster = dev(shadow.rect);
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    for (const l of layers) {
      const r: Bounds = {
        x: shadow.rect.x + l.dx - l.spread,
        y: shadow.rect.y + l.dy - l.spread,
        w: Math.max(0, shadow.rect.w + 2 * l.spread),
        h: Math.max(0, shadow.rect.h + 2 * l.spread),
      };
      const sigma = (Math.max(0, l.blur) / 2) * Math.abs(t.a);
      const key = `${dev(r).join(',')}|${caster.join(',')}|${sigma}|${l.rgba}|${canvasW}x${canvasH}`;
      let pieces = this.shadows.get(key);
      if (pieces === undefined) {
        pieces = this.shadowPieces(dev(r), caster, sigma, l.rgba, canvasW, canvasH);
        if (this.shadows.size > 32) this.shadows.clear();
        this.shadows.set(key, pieces);
      }
      for (const piece of pieces)
        ctx.drawImage(piece.canvas as unknown as CanvasImageSource, piece.x, piece.y);
    }
    ctx.restore();
  }

  /**
   * One Gaussian shadow layer outside `caster`, as kiln-raster's `paint_shadow` computes it, in
   * up to four bands around the caster (the caster hides the rest), so a page shadow uploads and
   * blends a band of pixels rather than the whole tile.
   */
  private shadowPieces(
    rect: [number, number, number, number],
    caster: [number, number, number, number],
    sigma: number,
    rgba: number,
    width: number,
    height: number,
  ): ShadowPiece[] {
    const [r, g, b, a] = channels(rgba);
    if (a === 0) return [];
    const reach = Math.ceil(sigma * 3);
    const clamp = (v: number, hi: number) => Math.min(hi, Math.max(0, v));
    const x0 = clamp(Math.floor(rect[0] - reach), width);
    const x1 = clamp(Math.ceil(rect[2] + reach), width);
    const y0 = clamp(Math.floor(rect[1] - reach), height);
    const y1 = clamp(Math.ceil(rect[3] + reach), height);
    if (x1 <= x0 || y1 <= y0) return [];
    // Pixels whose centre lies in the caster are skipped: rows [cy0, cy1) × columns [cx0, cx1).
    const cy0 = Math.min(y1, Math.max(y0, Math.ceil(caster[1] - 0.5)));
    const cy1 = Math.min(y1, Math.max(cy0, Math.ceil(caster[3] - 0.5)));
    const cx0 = Math.min(x1, Math.max(x0, Math.ceil(caster[0] - 0.5)));
    const cx1 = Math.min(x1, Math.max(cx0, Math.ceil(caster[2] - 0.5)));
    const bands: Array<[number, number, number, number]> = [
      [x0, y0, x1, cy0],
      [x0, cy1, x1, y1],
      [x0, cy0, cx0, cy1],
      [cx1, cy0, x1, cy1],
    ];
    const pieces: ShadowPiece[] = [];
    for (const [bx0, by0, bx1, by1] of bands) {
      const w = bx1 - bx0;
      const h = by1 - by0;
      if (w <= 0 || h <= 0) continue;
      const cols = new Float64Array(w);
      for (let i = 0; i < w; i++) cols[i] = blurredInterval(bx0 + i + 0.5, rect[0], rect[2], sigma);
      let any = false;
      const canvas = this.pixelsCanvas(w, h, (data) => {
        for (let y = by0; y < by1; y++) {
          const cy = y + 0.5;
          const fy = blurredInterval(cy, rect[1], rect[3], sigma);
          if (fy <= 0) continue;
          const inRows = cy >= caster[1] && cy < caster[3];
          for (let i = 0; i < w; i++) {
            const cx = bx0 + i + 0.5;
            if (inRows && cx >= caster[0] && cx < caster[2]) continue;
            const alpha = Math.min(255, Math.max(0, Math.round(a * (cols[i] as number) * fy)));
            if (alpha === 0) continue;
            const o = ((y - by0) * w + i) * 4;
            data[o] = r;
            data[o + 1] = g;
            data[o + 2] = b;
            data[o + 3] = alpha;
            any = true;
          }
        }
      });
      if (canvas && any) pieces.push({ canvas, x: bx0, y: by0 });
    }
    return pieces;
  }
}
