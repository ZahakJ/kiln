/**
 * Slide thumbnails: the core renders them (`Query::Deck(Thumbnails)`, one tile per slide), the
 * painter turns each tile into a canvas, and this cache keeps one canvas per slide until the
 * slide's `version` changes. The rail, the slide sorter and the layout gallery draw from it.
 */
import type { DeckSlideInfo, SceneDelta } from '@kiln/protocol';
import { Painter, type PainterCanvas, paletteFor, SceneCache } from '@kiln/surface';
import type { DeckEditorModel } from './model.ts';

/** A rendered thumbnail. */
export interface Thumb {
  version: number;
  canvas: CanvasImageSource & { width: number; height: number };
}

/** Paints every tile of a self-contained scene; tile index → its canvas. */
export function paintScene(scene: SceneDelta, theme: 'light' | 'dark'): Map<number, Thumb['canvas']> {
  const cache = new SceneCache<PainterCanvas>();
  const out = new Map<number, Thumb['canvas']>();
  try {
    cache.apply(scene);
  } catch {
    return out;
  }
  const painter = new Painter(paletteFor(theme, 'deck'));
  for (const t of cache.tiles.values()) {
    const c = painter.ensureTile(cache, t);
    if (c) out.set(t.tile.id.index, c as unknown as Thumb['canvas']);
  }
  return out;
}

/** Runs of consecutive indices (`[from, count]`), at most `max` long. */
export function runs(indices: readonly number[], max = 16): [number, number][] {
  const sorted = [...new Set(indices)].sort((a, b) => a - b);
  const out: [number, number][] = [];
  for (const i of sorted) {
    const last = out.at(-1);
    if (last && last[0] + last[1] === i && last[1] < max) last[1] += 1;
    else out.push([i, 1]);
  }
  return out;
}

/** The thumbnails of a presentation's slides, by slide id. */
export class ThumbCache {
  private readonly thumbs = new Map<number, Thumb>();
  private readonly listeners = new Set<() => void>();
  private busy = false;
  private again = false;
  /** Bumped whenever a thumbnail arrives (React keys on it). */
  revision = 0;

  constructor(
    private readonly model: DeckEditorModel,
    private width: number,
    private theme: 'light' | 'dark',
  ) {}

  get(id: number): Thumb | undefined {
    return this.thumbs.get(id);
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  setTheme(theme: 'light' | 'dark'): void {
    if (theme === this.theme) return;
    this.theme = theme;
    this.thumbs.clear();
  }

  setWidth(width: number): void {
    if (Math.abs(width - this.width) < 1) return;
    this.width = width;
    this.thumbs.clear();
  }

  /** Asks for the thumbnails of the slides whose version changed. */
  async update(slides: readonly DeckSlideInfo[]): Promise<void> {
    if (this.busy) {
      this.again = true;
      return;
    }
    this.busy = true;
    try {
      const live = new Set(slides.map((s) => s.id));
      for (const id of [...this.thumbs.keys()]) if (!live.has(id)) this.thumbs.delete(id);
      const stale = slides.flatMap((s, i) => (this.thumbs.get(s.id)?.version === s.version ? [] : [i]));
      const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
      for (const [from, count] of runs(stale)) {
        const scene = await this.model.thumbnails(from, count, this.width, dpr);
        if (!scene) continue;
        for (const [index, canvas] of paintScene(scene, this.theme)) {
          const s = slides[index];
          if (s) this.thumbs.set(s.id, { version: s.version, canvas });
        }
        this.revision += 1;
        for (const l of [...this.listeners]) l();
      }
    } finally {
      this.busy = false;
    }
    if (this.again) {
      this.again = false;
      const s = this.model.state.deck;
      if (s) await this.update(s.slides);
    }
  }
}

/**
 * Data URLs of the bundled themes' samples (`Query::Deck(ThemeThumbnails)`), by theme name, for
 * the Design ▸ Themes gallery.
 */
export async function themeImages(
  model: DeckEditorModel,
  names: readonly string[],
  theme: 'light' | 'dark',
): Promise<Record<string, string>> {
  if (typeof document === 'undefined') return {};
  const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
  const scene = await model.themeThumbnails(72, dpr);
  if (!scene) return {};
  const out: Record<string, string> = {};
  for (const [index, canvas] of paintScene(scene, theme)) {
    const name = names[index];
    if (!name) continue;
    const el = document.createElement('canvas');
    el.width = canvas.width;
    el.height = canvas.height;
    const ctx = el.getContext('2d');
    if (!ctx) continue;
    ctx.drawImage(canvas, 0, 0);
    try {
      out[name] = el.toDataURL('image/png');
    } catch {
      // A canvas the browser will not export: the swatch stays.
    }
  }
  return out;
}
