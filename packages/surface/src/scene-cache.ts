/**
 * The tile cache: the painter-side mirror of what the core's `DeltaTracker` believes the painter
 * holds. `apply` takes a decoded `SceneDelta` (tiles keyed by `TileId`, atlas page rows, images,
 * the overlay) exactly as `kiln_raster::Compositor::apply` does; the painter renders tiles from
 * here into their own canvases once and composites them on every frame.
 */

import type {
  AtlasUpdate,
  Bounds,
  DrawOp,
  ImageUpload,
  Paint,
  SceneDelta,
  Tile,
  TileId,
} from '@kiln/protocol';

/** A string key for a tile id (`layer:index`). */
export function tileKey(id: TileId): string {
  return `${id.layer}:${id.index}`;
}

/** An atlas page: its pixels, and a version bumped by every update so derived canvases refresh. */
export interface AtlasPage {
  format: 'A8' | 'Rgba8';
  width: number;
  height: number;
  /** A8: coverage per pixel; Rgba8: straight RGBA. */
  data: Uint8Array;
  /** Rows written so far (pages fill top-down). */
  rows: number;
  version: number;
}

/** An uploaded image (straight RGBA). */
export interface CachedImage {
  width: number;
  height: number;
  data: Uint8ClampedArray;
}

/**
 * A tile and its rendered canvas. Atlas updates never change pixels a rendered tile already uses
 * (pages only grow, and a tile arrives together with the rows its glyphs need), so a canvas stays
 * valid until the tile itself changes or the palette does.
 */
export interface CachedTile<C> {
  tile: Tile;
  canvas: C | null;
  /** Palette name the canvas was rendered with. */
  palette: string | null;
}

/** Why a delta was refused (the cache is unchanged). */
export class DeltaError extends Error {
  override readonly name = 'DeltaError';
}

function validAtlas(u: AtlasUpdate): boolean {
  const bpp = u.format === 'A8' ? 1 : 4;
  return u.width > 0 && u.height > 0 && u.y + u.rows <= u.height && u.data.length === u.rows * u.width * bpp;
}

function validImage(i: ImageUpload): boolean {
  return (
    i.width > 0 &&
    i.height > 0 &&
    i.width <= 16384 &&
    i.height <= 16384 &&
    i.data.length === i.width * i.height * 4
  );
}

/** The pages a tile's glyph runs use. */
export function glyphPages(ops: readonly DrawOp[]): number[] {
  const pages = new Set<number>();
  for (const op of ops) if (typeof op === 'object' && 'Glyphs' in op) pages.add(op.Glyphs.page);
  return [...pages].sort((a, b) => a - b);
}

/** Painter-side scene state. `C` is the painter's canvas type. */
export class SceneCache<C = unknown> {
  readonly tiles = new Map<string, CachedTile<C>>();
  readonly atlas = new Map<number, AtlasPage>();
  readonly images = new Map<number, CachedImage>();
  overlay: DrawOp[] = [];
  /** Hash of the overlay (changes when it does). */
  overlayHash = 0;
  scale = 1;
  extent: Bounds = { x: 0, y: 0, w: 0, h: 0 };
  background: Paint = { Rgba: 0 };
  revision = 0;
  /** Number of deltas applied. */
  applied = 0;

  /**
   * Applies a delta. Malformed deltas throw {@link DeltaError} and leave the cache unchanged.
   * Returns whether anything visible may have changed.
   */
  apply(delta: SceneDelta): boolean {
    if (!(Number.isFinite(delta.scale) && delta.scale > 0))
      throw new DeltaError('the frame scale is not positive');
    for (const u of delta.atlas) {
      if (!validAtlas(u)) throw new DeltaError(`atlas update for page ${u.page} is malformed`);
      const page = delta.reset ? undefined : this.atlas.get(u.page);
      if (page && (page.width !== u.width || page.height !== u.height || page.format !== u.format)) {
        throw new DeltaError(`atlas page ${u.page} changed shape`);
      }
    }
    for (const img of delta.images) {
      if (!validImage(img)) throw new DeltaError(`image ${img.id} is malformed`);
    }
    if (delta.reset) {
      this.tiles.clear();
      this.atlas.clear();
      this.images.clear();
      this.overlay = [];
      this.overlayHash = 0;
    }
    this.scale = delta.scale;
    this.extent = delta.extent;
    this.background = delta.background;
    this.revision = delta.revision;
    for (const u of delta.atlas) this.applyAtlas(u);
    for (const img of delta.images) {
      this.images.set(img.id, {
        width: img.width,
        height: img.height,
        data: Uint8ClampedArray.from(img.data),
      });
    }
    for (const id of delta.removed) this.tiles.delete(tileKey(id));
    for (const tile of delta.tiles) {
      const key = tileKey(tile.id);
      const old = this.tiles.get(key);
      // Identical pixels (same hash) keep their rendered canvas.
      if (old && old.tile.hash === tile.hash && !delta.reset) {
        old.tile = tile;
      } else {
        this.tiles.set(key, { tile, canvas: null, palette: null });
      }
    }
    if (delta.overlay) {
      this.overlay = delta.overlay.ops;
      this.overlayHash = delta.overlay.hash;
    }
    this.applied += 1;
    return !(
      delta.tiles.length === 0 &&
      delta.removed.length === 0 &&
      delta.atlas.length === 0 &&
      !delta.overlay &&
      !delta.reset
    );
  }

  private applyAtlas(u: AtlasUpdate): void {
    const bpp = u.format === 'A8' ? 1 : 4;
    let page = this.atlas.get(u.page);
    if (!page) {
      page = {
        format: u.format,
        width: u.width,
        height: u.height,
        data: new Uint8Array(u.width * u.height * bpp),
        rows: 0,
        version: 0,
      };
      this.atlas.set(u.page, page);
    }
    page.data.set(u.data, u.y * u.width * bpp);
    page.rows = Math.max(page.rows, u.y + u.rows);
    page.version += 1;
  }

  /** Cached tiles intersecting `region` (document pixels), in paint order. */
  tilesIn(region: Bounds): CachedTile<C>[] {
    const out: CachedTile<C>[] = [];
    for (const t of this.tiles.values()) {
      const r = t.tile.rect;
      if (
        r.x < region.x + region.w &&
        region.x < r.x + r.w &&
        r.y < region.y + region.h &&
        region.y < r.y + r.h
      ) {
        out.push(t);
      }
    }
    return out.sort((a, b) => a.tile.id.layer - b.tile.id.layer || a.tile.id.index - b.tile.id.index);
  }

  /** Drops every rendered canvas (palette or DPR change); tiles stay. */
  invalidateCanvases(): void {
    for (const t of this.tiles.values()) {
      t.canvas = null;
      t.palette = null;
    }
  }
}
