/**
 * Plays the core's slide show on a canvas: asks for a frame at the clock's time
 * (`Query::Deck(ShowFrame)`), applies its scene delta, composes it, and asks again on the next
 * animation frame while something moves (a transition, an effect, a slide timing counting down)
 * or the presenter did something. Nothing about the show is decided here: steps, timing, ink and
 * the layers of every frame are the core's.
 */
import type { DeckShowFrame, DeckShowStatus, DocId, KilnClient } from '@kiln/protocol';
import { defaultCanvasFactory, Painter, type PainterCanvas, paletteFor, SceneCache } from '@kiln/surface';
import { deckResult } from '../model.ts';
import { realClock, type ShowClock } from './clock.ts';
import { composeFrame, type SlideFit, toSlidePoint } from './compose.ts';

export interface ShowEngineOptions {
  clock?: ShowClock;
  /** Every frame painted (the chrome reads the status from it). */
  onFrame?: (frame: DeckShowFrame) => void;
  /** The show is over (Esc, the end screen clicked, a preview done). */
  onEnd?: (status: DeckShowStatus) => void;
}

/** Drives one canvas. */
export class ShowEngine {
  readonly cache = new SceneCache<PainterCanvas>();
  readonly painter = new Painter(paletteFor('light', 'deck'));
  private readonly clock: ShowClock;
  private raf = 0;
  private busy = false;
  private wanted = true;
  private disposed = false;
  private fit: SlideFit = { k: 1, ox: 0, oy: 0 };
  private dpr = 1;
  private readonly scratch: { group: PainterCanvas | null; layer: PainterCanvas | null } = {
    group: null,
    layer: null,
  };
  private unsubscribeClock: (() => void) | null = null;
  private ro: ResizeObserver | null = null;
  private ended = false;
  /** The last frame painted. */
  last: DeckShowFrame | null = null;
  /** Frames painted (tests, diagnostics). */
  frames = 0;
  /** The clock's time of the last frame painted. */
  at = Number.NaN;

  constructor(
    private readonly client: KilnClient,
    private readonly doc: DocId,
    private readonly canvas: HTMLCanvasElement,
    private readonly options: ShowEngineOptions = {},
  ) {
    this.clock = options.clock ?? realClock;
  }

  start(): void {
    this.unsubscribeClock = this.clock.subscribe?.(() => this.kick()) ?? null;
    if (typeof ResizeObserver !== 'undefined') {
      this.ro = new ResizeObserver(() => this.kick());
      this.ro.observe(this.canvas);
    }
    this.kick();
  }

  dispose(): void {
    this.disposed = true;
    if (this.raf) cancelAnimationFrame(this.raf);
    this.unsubscribeClock?.();
    this.ro?.disconnect();
  }

  /** Asks for a frame (after a command, a resize, a clock change). */
  kick(): void {
    this.wanted = true;
    this.schedule();
  }

  /** The slide point under a client point. */
  slidePoint(clientX: number, clientY: number): { x: number; y: number } {
    const r = this.canvas.getBoundingClientRect();
    return toSlidePoint(this.fit, this.dpr, r, { x: clientX, y: clientY });
  }

  /** Where slide px are on the canvas element, CSS px: `left + x × k`, `top + y × k`. */
  cssFit(): { left: number; top: number; k: number } {
    const d = this.dpr || 1;
    return { left: this.fit.ox / d, top: this.fit.oy / d, k: this.fit.k / d };
  }

  private schedule(): void {
    if (this.disposed || this.raf) return;
    const run = () => {
      this.raf = 0;
      void this.tick();
    };
    this.raf =
      typeof requestAnimationFrame === 'function'
        ? requestAnimationFrame(run)
        : (setTimeout(run, 16) as unknown as number);
  }

  private async tick(): Promise<void> {
    if (this.disposed || this.busy) return;
    if (!this.wanted) return;
    this.wanted = false;
    this.busy = true;
    try {
      const rect = this.canvas.getBoundingClientRect();
      const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
      const width = Math.max(1, Math.round(rect.width));
      const height = Math.max(1, Math.round(rect.height));
      const at = this.clock.now();
      const r = deckResult(
        await this.client.query(this.doc, {
          Deck: { ShowFrame: { now_ms: at, width, height, dpr } },
        }),
      );
      if (this.disposed) return;
      if (!r || !('ShowFrame' in r)) {
        this.finish(null);
        return;
      }
      const frame = r.ShowFrame;
      this.paint(frame, width, height, dpr);
      this.at = at;
      this.options.onFrame?.(frame);
      if (!frame.status.running) {
        this.finish(frame.status);
        return;
      }
      if (frame.animating) this.wanted = true;
    } catch {
      this.finish(null);
      return;
    } finally {
      this.busy = false;
    }
    if (this.wanted) this.schedule();
  }

  private finish(status: DeckShowStatus | null): void {
    if (this.ended) return;
    this.ended = true;
    this.options.onEnd?.(
      status ?? {
        running: false,
        mode: 'Show',
        slide: 0,
        position: 0,
        count: 0,
        click: 0,
        clicks: 0,
        screen: 'Slide',
        tool: 'Arrow',
        typed: '',
        next: null,
        on_click: true,
        kind: 'Speaker',
        version: 0,
      },
    );
  }

  private paint(frame: DeckShowFrame, width: number, height: number, dpr: number): void {
    try {
      this.cache.apply(frame.delta);
    } catch {
      // A malformed delta: the next frame resets.
      return;
    }
    const w = Math.round(width * dpr);
    const h = Math.round(height * dpr);
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    const ctx = this.canvas.getContext('2d');
    if (!ctx) return;
    this.dpr = dpr;
    this.fit = composeFrame({ ctx, width: w, height: h }, this.painter, this.cache, frame, (sw, sh, slot) => {
      const have = this.scratch[slot];
      if (have && have.width === sw && have.height === sh) return have;
      const c = defaultCanvasFactory(sw, sh);
      this.scratch[slot] = c;
      return c;
    });
    // Tiles of the slide to come are painted in the frame's spare time.
    const deadline = (typeof performance === 'undefined' ? Date.now() : performance.now()) + 4;
    if (this.painter.prefetch(this.cache, frame.delta.extent, deadline)) this.wanted = true;
    this.last = frame;
    this.frames += 1;
  }
}
