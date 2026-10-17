/**
 * The grid surface (PLAN.md §3.6 "Grid specifics"): paints the core's Scene — cell tiles, header
 * strips and the corner, the overlay with the selection and the in-cell editor — into up to four
 * panes (frozen rows and columns stay while the rest scrolls), every pane from the same tile
 * cache. Scrolling is native (a transparent scroller over the canvas holds a spacer the size of
 * the sheet), so wheel, trackpad, touch and scroll bars behave as the platform does; each scroll
 * composites cached tiles at once and asks the core for a frame of the new view.
 *
 * Input: pointer gestures are hit-tested by the core (`Query::Grid(HitTest)`) and become
 * selection, fill-handle, resize and point-mode commands; keys go through `keys.ts`; a hidden
 * textarea is the input proxy for typing and IME (its text is the entry the core renders in the
 * cell). Nothing here lays out or formats anything (PLAN.md §3.7).
 */
import type {
  DrawOp,
  GridHit,
  GridLayout,
  GridState,
  KilnClient,
  SceneDelta,
  Viewport,
} from '@kiln/protocol';
import {
  cssColor,
  device,
  Painter,
  type PainterCanvas,
  paletteFor,
  resolvePaint,
  SceneCache,
} from '@kiln/surface';
import { gridKeyAction, type KeyAction, type KeyLike, typesText } from './keys.ts';
import type { GridEditorModel, GridSurfaceHandle } from './model.ts';

/** Tile layers of the Grid scene (`kiln_sheet_layout::scene`). */
export const LAYERS = { colHeader: 2, rowHeader: 3, corner: 4, cellsFrom: 16, drawings: 8192 } as const;

/** The DOM the controller drives. */
export interface GridElements {
  root: HTMLElement;
  canvas: HTMLCanvasElement;
  scroller: HTMLElement;
  spacer: HTMLElement;
  proxy: HTMLTextAreaElement;
  a11y: HTMLElement;
}

export interface GridSurfaceOptions {
  theme: 'light' | 'dark';
  /** Blink the editor's caret (screenshots turn it off). */
  blink: boolean;
}

/** A rectangle on the surface (CSS pixels). */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** One pane: where it is on the surface and which sheet position it shows at its top-left. */
export interface PaneSpec {
  rect: Rect;
  x: number;
  y: number;
  kind: 'cells' | 'colHeader' | 'rowHeader' | 'corner';
}

const DEFAULT_LAYOUT: GridLayout = {
  header_w: 44,
  header_h: 24,
  freeze_rows: 0,
  freeze_cols: 0,
  frozen_h: 0,
  frozen_w: 0,
  content_w: 4096,
  content_h: 4096,
  sheet_zoom: 1,
  split: false,
};

/**
 * The panes of a view (the twin of `SheetGeometry::pane`): cells, header strips and the corner,
 * with the sheet position each shows. Presentation arithmetic over the layout the core reports.
 */
export function panes(
  l: GridLayout,
  zoom: number,
  width: number,
  height: number,
  sx: number,
  sy: number,
): PaneSpec[] {
  const z = zoom;
  const hx = l.header_w * z;
  const hy = l.header_h * z;
  const fw = l.frozen_w * z;
  const fh = l.frozen_h * z;
  const right = Math.max(0, width - hx - fw);
  const bottom = Math.max(0, height - hy - fh);
  const out: PaneSpec[] = [];
  const add = (kind: PaneSpec['kind'], rect: Rect, x: number, y: number) => {
    if (rect.w > 0 && rect.h > 0) out.push({ kind, rect, x, y });
  };
  add('cells', { x: hx, y: hy, w: fw, h: fh }, 0, 0);
  add('cells', { x: hx + fw, y: hy, w: right, h: fh }, sx, 0);
  add('cells', { x: hx, y: hy + fh, w: fw, h: bottom }, 0, sy);
  add('cells', { x: hx + fw, y: hy + fh, w: right, h: bottom }, sx, sy);
  if (hy > 0) {
    add('colHeader', { x: hx, y: 0, w: fw, h: hy }, 0, -l.header_h);
    add('colHeader', { x: hx + fw, y: 0, w: right, h: hy }, sx, -l.header_h);
  }
  if (hx > 0) {
    add('rowHeader', { x: 0, y: hy, w: hx, h: fh }, -l.header_w, 0);
    add('rowHeader', { x: 0, y: hy + fh, w: hx, h: bottom }, -l.header_w, sy);
  }
  if (hx > 0 && hy > 0) add('corner', { x: 0, y: 0, w: hx, h: hy }, -l.header_w, -l.header_h);
  return out;
}

function layerOf(kind: PaneSpec['kind']): (layer: number) => boolean {
  switch (kind) {
    case 'cells':
      return (l) => l >= LAYERS.cellsFrom;
    case 'colHeader':
      return (l) => l === LAYERS.colHeader;
    case 'rowHeader':
      return (l) => l === LAYERS.rowHeader;
    default:
      return (l) => l === LAYERS.corner;
  }
}

const BLINK_MS = 530;
/** Pixels from the edge where a drag scrolls the sheet. */
const AUTOSCROLL_EDGE = 24;

type Drag =
  | { kind: 'cells'; point: boolean }
  | { kind: 'rows' }
  | { kind: 'cols' }
  | { kind: 'fill' }
  | { kind: 'resize'; axis: 'row' | 'col'; index: number; start: number; size: number; at: number };

/** Drives the grid surface of one document. */
/** Milliseconds of a frame spent rendering prefetched tiles (after the composite's own work). */
const PREFETCH_BUDGET_MS = 4;

export class GridSurfaceController implements GridSurfaceHandle {
  readonly cache = new SceneCache<PainterCanvas>();
  readonly painter: Painter;
  private readonly cleanups: Array<() => void> = [];
  private layout: GridLayout = DEFAULT_LAYOUT;
  private zoom = 1;
  private dpr = 1;
  private cssW = 0;
  private cssH = 0;
  private sx = 0;
  private sy = 0;
  private sent: Viewport | null = null;
  private inFlight = false;
  private frameWanted = false;
  private raf = 0;
  private disposed = false;
  private caretOn = true;
  private blinkTimer: ReturnType<typeof setInterval> | null = null;
  private focused = false;
  /** The entry is being typed (set at once on input, before the core's state says so). */
  private editing = false;
  /**
   * F2 or a double-click asked the core to edit the cell and its text has not arrived: keys typed
   * meanwhile wait in the proxy and join the cell's text when it does.
   */
  private pendingEdit = false;
  /** Keys pressed while `pendingEdit`, chained after the cell's text arrives. */
  private editStart: Promise<void> | null = null;
  /** Keys waiting in `editStart`'s chain. */
  private keysWaiting = 0;
  /** The last text sent to the core from the proxy. */
  private sentText = '';
  /**
   * Texts sent to the core and not yet seen back: a state that carries one of them is an echo of
   * typing (older than what the proxy holds now) and must not undo the keys typed since.
   */
  private echoes: string[] = [];
  /** Commits and cancels sent and not answered yet: the core's editor state may be stale. */
  private ending = 0;
  /**
   * The first frame resets the core's record of what the painter holds: another surface of the
   * window (the frame's placeholder before the editor mounted) may have taken earlier frames.
   */
  private fresh = true;
  private drag: Drag | null = null;
  private dragPoint: { x: number; y: number } | null = null;
  private dragBusy = false;
  private autoscroll = 0;
  private a11yTimer: ReturnType<typeof setTimeout> | null = null;
  private state: GridState | null = null;
  private suppressScroll = false;
  /** Frames painted (diagnostics, tests). */
  stats = { frames: 0, composites: 0, lastFrameMs: 0 };

  constructor(
    private readonly client: KilnClient,
    private readonly model: GridEditorModel,
    private readonly els: GridElements,
    private options: GridSurfaceOptions,
  ) {
    this.painter = new Painter(paletteFor(options.theme, 'grid'));
  }

  // ── lifecycle ───────────────────────────────────────────────────────────────────────────

  start(): void {
    const { scroller, proxy } = this.els;
    const on = <K extends keyof HTMLElementEventMap>(
      el: HTMLElement | Document,
      type: K,
      fn: (e: HTMLElementEventMap[K]) => void,
      opts?: AddEventListenerOptions,
    ) => {
      el.addEventListener(type, fn as EventListener, opts);
      this.cleanups.push(() => el.removeEventListener(type, fn as EventListener, opts));
    };
    on(scroller, 'scroll', () => this.onScroll(), { passive: true });
    on(scroller, 'wheel', (e) => this.onWheel(e), { passive: false });
    on(scroller, 'pointerdown', (e) => this.onPointerDown(e));
    on(scroller, 'pointermove', (e) => this.onPointerMove(e));
    on(scroller, 'pointerup', (e) => this.onPointerUp(e));
    on(scroller, 'pointercancel', (e) => this.onPointerUp(e));
    on(scroller, 'dblclick', (e) => this.onDoubleClick(e));
    on(scroller, 'contextmenu', (e) => this.onContextMenu(e));
    on(proxy, 'keydown', (e) => this.onKeyDown(e));
    on(proxy, 'input', () => this.onInput());
    on(proxy, 'paste', (e) => this.onPaste(e));
    on(proxy, 'focus', () => {
      this.focused = true;
      this.paint();
    });
    on(proxy, 'blur', () => {
      this.focused = false;
      this.paint();
    });
    on(document, 'selectionchange', () => this.onSelectionChange());
    const ro = new ResizeObserver(() => this.measure());
    ro.observe(this.els.root);
    this.cleanups.push(() => ro.disconnect());
    const dprQuery = () => {
      if (typeof matchMedia !== 'function') return;
      const mq = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
      const fn = () => {
        this.measure();
        dprQuery();
      };
      mq.addEventListener('change', fn, { once: true });
    };
    dprQuery();
    this.cleanups.push(this.model.onState((s) => this.onState(s)));
    this.measure();
    if (this.options.blink) {
      this.blinkTimer = setInterval(() => {
        if (!this.editing) return;
        this.caretOn = !this.caretOn;
        this.paint();
      }, BLINK_MS);
    }
  }

  dispose(): void {
    this.disposed = true;
    for (const c of this.cleanups.splice(0)) c();
    if (this.raf) cancelAnimationFrame(this.raf);
    if (this.prefetchRaf) cancelAnimationFrame(this.prefetchRaf);
    if (this.blinkTimer) clearInterval(this.blinkTimer);
    if (this.a11yTimer) clearTimeout(this.a11yTimer);
    if (this.autoscroll) cancelAnimationFrame(this.autoscroll);
  }

  setOptions(options: Partial<GridSurfaceOptions>): void {
    this.options = { ...this.options, ...options };
    const palette = paletteFor(this.options.theme, 'grid');
    if (palette.name !== this.painter.palette.name) {
      this.painter.setPalette(palette);
      this.cache.invalidateCanvases();
      this.paint();
    }
  }

  // ── GridSurfaceHandle ───────────────────────────────────────────────────────────────────

  focus(): void {
    this.els.proxy.focus({ preventScroll: true });
  }

  size(): { width: number; height: number } {
    return { width: this.cssW, height: this.cssH };
  }

  /** Scrolls to a sheet position (the core's scroll request). */
  reveal(x: number, y: number): void {
    const z = this.zoom;
    const left = Math.max(0, (x - this.layout.frozen_w) * z);
    const top = Math.max(0, (y - this.layout.frozen_h) * z);
    this.sx = x;
    this.sy = y;
    this.suppressScroll = true;
    this.els.scroller.scrollLeft = left;
    this.els.scroller.scrollTop = top;
    this.paint();
    this.requestFrame();
  }

  // ── state ───────────────────────────────────────────────────────────────────────────────

  private onState(s: GridState): void {
    const prev = this.state;
    this.state = s;
    const zoom = this.model.state.status?.zoom ?? 1;
    const layoutChanged = JSON.stringify(prev?.layout) !== JSON.stringify(s.layout) || zoom !== this.zoom;
    this.layout = s.layout;
    if (zoom !== this.zoom) this.zoom = zoom;
    if (layoutChanged) this.sizeSpacer();
    // The editor: the core's text wins when it differs from what was typed (F4, a suggestion,
    // a pointed reference, the formula bar).
    const ed = s.editor;
    const wasEditing = this.editing;
    const proxy = this.els.proxy;
    if (this.ending > 0) {
      // An answer from before the entry ended: the entry is over here already (the next keys
      // start a new one), whatever this state still says.
    } else if (ed && !ed.formula_bar && this.pendingEdit) {
      // F2's text arrived: what was typed meanwhile goes in at the caret.
      this.pendingEdit = false;
      this.editing = true;
      const typed = proxy.value;
      const text = ed.text.slice(0, ed.start) + typed + ed.text.slice(ed.end);
      const caret = ed.start + typed.length;
      proxy.value = text;
      this.sentText = text;
      this.echoes = typed ? [text] : [];
      try {
        proxy.setSelectionRange(caret, caret);
      } catch {
        // Detached in tests.
      }
      if (typed)
        void this.model.core('grid.editing.set_text', { text, start: caret, end: caret, formula_bar: false });
      this.placeProxy(ed.rect);
    } else if (ed && !ed.formula_bar) {
      this.editing = true;
      const echo = wasEditing ? this.echoes.indexOf(ed.text) : -1;
      if (echo >= 0) this.echoes.splice(0, echo + 1);
      else if (ed.text !== proxy.value || !wasEditing) {
        this.echoes = [];
        proxy.value = ed.text;
        this.sentText = ed.text;
        try {
          proxy.setSelectionRange(ed.start, ed.end);
        } catch {
          // Detached in tests.
        }
        this.lastSel = this.selKey();
      }
      this.placeProxy(ed.rect);
    } else {
      this.editing = s.mode !== 'Ready';
      if (!ed) {
        if (proxy.value !== '' && !this.editing) proxy.value = '';
        this.sentText = '';
      }
    }
    this.caretOn = true;
    this.requestFrame();
  }

  private placeProxy(r: { x: number; y: number; width: number; height: number }): void {
    const p = this.els.proxy.style;
    p.left = `${Math.round(r.x)}px`;
    p.top = `${Math.round(r.y)}px`;
    p.width = `${Math.max(8, Math.round(r.width))}px`;
    p.height = `${Math.max(8, Math.round(r.height))}px`;
  }

  // ── geometry ────────────────────────────────────────────────────────────────────────────

  private measure(): void {
    const { scroller, canvas } = this.els;
    // The scroll bars take room: the canvas covers what the scroller shows.
    const w = scroller.clientWidth || this.els.root.clientWidth;
    const h = scroller.clientHeight || this.els.root.clientHeight;
    const dpr = typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 1;
    if (w === this.cssW && h === this.cssH && dpr === this.dpr) return;
    this.cssW = w;
    this.cssH = h;
    if (dpr !== this.dpr) this.cache.invalidateCanvases();
    this.dpr = dpr;
    canvas.width = Math.max(1, Math.round(w * dpr));
    canvas.height = Math.max(1, Math.round(h * dpr));
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
    this.sizeSpacer();
    this.paint();
    this.requestFrame();
  }

  private sizeSpacer(): void {
    const l = this.layout;
    const z = this.zoom;
    this.els.spacer.style.width = `${Math.ceil((l.content_w + l.header_w) * z)}px`;
    this.els.spacer.style.height = `${Math.ceil((l.content_h + l.header_h) * z)}px`;
    this.syncScroll();
  }

  private syncScroll(): void {
    const z = this.zoom;
    this.sx = this.layout.frozen_w + this.els.scroller.scrollLeft / z;
    this.sy = this.layout.frozen_h + this.els.scroller.scrollTop / z;
  }

  private viewport(): Viewport {
    return {
      width_px: this.cssW,
      height_px: this.cssH,
      scroll: { x: this.sx, y: this.sy },
      zoom: this.zoom,
      dpr: this.dpr,
      mode: 'Normal',
    };
  }

  /** The panes of the current view. */
  panes(): PaneSpec[] {
    return panes(this.layout, this.zoom, this.cssW, this.cssH, this.sx, this.sy);
  }

  /** Surface coordinates of a pointer event. */
  private point(e: { clientX: number; clientY: number }): { x: number; y: number } {
    const r = this.els.root.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  // ── frames ──────────────────────────────────────────────────────────────────────────────

  /** Asks the core for the next frame (one in flight; prefetch frames follow while they bring tiles). */
  requestFrame(): void {
    if (this.disposed) return;
    if (this.inFlight) {
      this.frameWanted = true;
      return;
    }
    if (this.cssW <= 0 || this.cssH <= 0) return;
    this.inFlight = true;
    void this.fetchFrame();
  }

  private async fetchFrame(): Promise<void> {
    const t0 = performance.now();
    let delta: SceneDelta | null = null;
    try {
      if (this.fresh) {
        this.fresh = false;
        await this.client.execute(this.model.doc, { Suite: 'Repaint' }).catch(() => undefined);
      }
      // A disposed controller (React's development double mount) must not take the frame:
      // the core would count its tiles as painted.
      if (this.disposed) return;
      const vp = this.viewport();
      if (!sameViewport(this.sent, vp)) {
        await this.client.setViewport(this.model.doc, vp);
        this.sent = vp;
      }
      if (this.disposed) return;
      delta = await this.client.frame(this.model.doc);
      const applied = performance.now();
      this.cache.apply(delta);
      try {
        performance.measure('kiln:grid-apply', { start: applied, end: performance.now() });
        if (this.stats.frames % 600 === 0) performance.clearMeasures('kiln:grid-apply');
      } catch {
        // As in composite().
      }
      this.stats.frames += 1;
      this.stats.lastFrameMs = performance.now() - t0;
      // On the element too, for tests of production builds (which have no window handles).
      const ds = this.els.root.dataset;
      ds.kilnDoc = String(this.model.doc);
      ds.kilnFrames = String(this.stats.frames);
      ds.kilnFrameMs = this.stats.lastFrameMs.toFixed(2);
    } catch (error) {
      // A malformed delta or a lost painter: start over with a full frame.
      this.cache.tiles.clear();
      void this.client.execute(this.model.doc, { Suite: 'Repaint' }).catch(() => undefined);
      console.warn('kiln grid: frame failed', error);
    } finally {
      this.inFlight = false;
    }
    if (this.disposed) return;
    this.paint();
    this.scheduleA11y();
    // More prefetched tiles may follow; keep asking while frames bring tiles.
    const more = delta !== null && (delta.tiles.length > 0 || delta.reset);
    if (this.frameWanted || more) {
      this.frameWanted = false;
      requestAnimationFrame(() => this.requestFrame());
    }
  }

  private scheduleA11y(): void {
    if (this.a11yTimer) clearTimeout(this.a11yTimer);
    this.a11yTimer = setTimeout(() => {
      this.a11yTimer = null;
      void this.client.a11y(this.model.doc).then(
        (tree) => {
          if (!this.disposed) renderA11y(this.els.a11y, tree, this.els.proxy);
        },
        () => undefined,
      );
    }, 120);
  }

  // ── painting ────────────────────────────────────────────────────────────────────────────

  /** Composites on the next animation frame. */
  paint(): void {
    if (this.raf || this.disposed) return;
    this.raf = requestAnimationFrame(() => {
      this.raf = 0;
      this.composite();
    });
  }

  /** Composites every pane now. */
  composite(): void {
    const ctx = this.els.canvas.getContext('2d');
    if (!ctx) return;
    const begin = performance.now();
    const dpr = this.dpr;
    const vs = this.zoom * dpr;
    const s = this.cache.scale;
    const exact = Math.abs(vs - s) < 1e-6;
    const palette = this.painter.palette;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.globalCompositeOperation = 'source-over';
    ctx.fillStyle = cssColor(palette.page);
    ctx.fillRect(0, 0, this.els.canvas.width, this.els.canvas.height);
    const all = this.panes();
    for (const pane of all) {
      const r = pane.rect;
      const px = Math.round(r.x * dpr);
      const py = Math.round(r.y * dpr);
      const pw = Math.round((r.x + r.w) * dpr) - px;
      const ph = Math.round((r.y + r.h) * dpr) - py;
      ctx.save();
      ctx.beginPath();
      ctx.rect(px, py, pw, ph);
      ctx.clip();
      if (pane.kind !== 'cells') {
        ctx.fillStyle = cssColor(palette.panel);
        ctx.fillRect(px, py, pw, ph);
      }
      const region = { x: pane.x, y: pane.y, w: r.w / this.zoom, h: r.h / this.zoom };
      const accept = layerOf(pane.kind);
      const tiles = this.cache.tilesIn(region).filter((t) => accept(t.tile.id.layer));
      const ox = device(pane.x, s);
      const oy = device(pane.y, s);
      for (const t of tiles) {
        const canvas = this.painter.ensureTile(this.cache, t);
        if (!canvas) continue;
        const src = canvas as unknown as CanvasImageSource;
        if (exact) {
          ctx.drawImage(src, device(t.tile.rect.x, s) - ox + px, device(t.tile.rect.y, s) - oy + py);
        } else {
          const ratio = vs / s;
          ctx.drawImage(
            src,
            px + (t.tile.rect.x - pane.x) * vs,
            py + (t.tile.rect.y - pane.y) * vs,
            canvas.width * ratio,
            canvas.height * ratio,
          );
        }
      }
      if (pane.kind === 'cells') {
        // Drawings (charts) float above the cells.
        for (const t of this.cache.tilesIn(region).filter((x) => x.tile.id.layer >= LAYERS.drawings)) {
          const canvas = this.painter.ensureTile(this.cache, t);
          if (canvas)
            ctx.drawImage(
              canvas as unknown as CanvasImageSource,
              px + (t.tile.rect.x - pane.x) * vs,
              py + (t.tile.rect.y - pane.y) * vs,
              canvas.width * (vs / s),
              canvas.height * (vs / s),
            );
        }
        this.paintOverlay(ctx, this.cache.overlay, px - device(pane.x, vs), py - device(pane.y, vs), vs);
      }
      ctx.restore();
    }
    // The frozen panes' edges.
    const l = this.layout;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = cssColor(palette.guide);
    const line = Math.max(1, Math.round(dpr));
    if (l.frozen_w > 0) {
      const x = Math.round((l.header_w + l.frozen_w) * this.zoom * dpr);
      ctx.fillRect(x - line, 0, line, this.els.canvas.height);
    }
    if (l.frozen_h > 0) {
      const y = Math.round((l.header_h + l.frozen_h) * this.zoom * dpr);
      ctx.fillRect(0, y - line, this.els.canvas.width, line);
    }
    // A resize in progress: the guide where the border will go.
    if (this.drag?.kind === 'resize') {
      ctx.fillStyle = cssColor(resolvePaint(palette, { Role: 'AppFill' }));
      const at = Math.round(this.drag.at * dpr);
      if (this.drag.axis === 'col') ctx.fillRect(at - line, 0, line * 2, this.els.canvas.height);
      else ctx.fillRect(0, at - line, this.els.canvas.width, line * 2);
    }
    this.stats.composites += 1;
    // Spare time of this frame: render the prefetched tiles nearest the view in small steps, so
    // scrolling onto them costs a copy, never a whole tile's text.
    this.schedulePrefetch(begin);
    // The surface's work in this animation frame (B6 is measured on it, as for the page surface).
    try {
      performance.measure('kiln:grid-frame', { start: begin, end: performance.now() });
      // Observers see every entry; the buffer keeps only the recent ones.
      if (this.stats.composites % 600 === 0) performance.clearMeasures('kiln:grid-frame');
    } catch {
      // Engines without measure options: the stats still count frames.
    }
  }

  private prefetchRaf = 0;

  private schedulePrefetch(frameStart: number): void {
    if (this.disposed) return;
    const l = this.layout;
    const around = {
      x: this.sx,
      y: this.sy,
      w: Math.max(1, this.cssW / this.zoom - l.header_w - l.frozen_w),
      h: Math.max(1, this.cssH / this.zoom - l.header_h - l.frozen_h),
    };
    const more = this.painter.prefetch(this.cache, around, frameStart + PREFETCH_BUDGET_MS);
    if (more && !this.prefetchRaf) {
      this.prefetchRaf = requestAnimationFrame((t) => {
        this.prefetchRaf = 0;
        // A composite of this frame prefetches itself; otherwise the frame's time is free.
        if (!this.raf) this.schedulePrefetch(Math.max(t, performance.now()));
      });
    }
  }

  private paintOverlay(
    ctx: CanvasRenderingContext2D,
    ops: readonly DrawOp[],
    tx: number,
    ty: number,
    vs: number,
  ): void {
    if (ops.length === 0) return;
    ctx.setTransform(vs, 0, 0, vs, tx, ty);
    this.painter.paintOps(ctx, ops, this.cache, {
      scale: vs,
      glyphOrigin: [tx, ty],
      caretVisible: this.editing && this.focused && this.caretOn,
      touch: false,
    });
    ctx.setTransform(1, 0, 0, 1, 0, 0);
  }

  // ── scrolling ───────────────────────────────────────────────────────────────────────────

  private onScroll(): void {
    this.syncScroll();
    this.paint();
    if (this.suppressScroll) {
      this.suppressScroll = false;
    }
    this.requestFrame();
  }

  private onWheel(e: WheelEvent): void {
    if (!(e.ctrlKey || e.metaKey)) return;
    // Ctrl+wheel zooms (10 %–400 %), as Excel.
    e.preventDefault();
    const z = Math.min(4, Math.max(0.1, Math.round(this.zoom * (e.deltaY < 0 ? 1.1 : 1 / 1.1) * 100) / 100));
    void this.model.run('suite.view.zoom', { zoom: z });
  }

  // ── pointer ─────────────────────────────────────────────────────────────────────────────

  private async onPointerDown(e: PointerEvent): Promise<void> {
    if (e.button === 2) return;
    if (e.button !== 0) return;
    e.preventDefault();
    this.focus();
    const p = this.point(e);
    const hit = await this.model.hitTest(p.x, p.y);
    const shift = e.shiftKey;
    const add = e.ctrlKey || e.metaKey;
    const editing = this.editing;
    this.els.scroller.setPointerCapture?.(e.pointerId);
    if (hit === 'None') return;
    if (hit === 'Corner') {
      await this.model.core('grid.nav.select_range', { r0: 0, c0: 0, r1: 1048575, c1: 16383 });
      return;
    }
    if (hit === 'FillHandle') {
      this.drag = { kind: 'fill' };
      return;
    }
    if ('Cell' in hit) {
      const { row, col } = hit.Cell;
      if (editing) {
        // A formula takes the reference; otherwise the entry is committed and the cell selected.
        await this.model.core('grid.editing.point_ref', { row, col, extend: shift });
        this.drag = { kind: 'cells', point: true };
      } else {
        await this.model.core('grid.nav.select_cell', { row, col, extend: shift, add });
        this.drag = { kind: 'cells', point: false };
      }
      return;
    }
    if ('ColumnHeader' in hit) {
      const col = hit.ColumnHeader.col;
      await this.model.core('grid.nav.select_columns', { from: col, to: col, extend: shift, add });
      this.drag = { kind: 'cols' };
      return;
    }
    if ('RowHeader' in hit) {
      const row = hit.RowHeader.row;
      await this.model.core('grid.nav.select_rows', { from: row, to: row, extend: shift, add });
      this.drag = { kind: 'rows' };
      return;
    }
    if ('ColumnBorder' in hit) {
      const b = hit.ColumnBorder;
      this.drag = { kind: 'resize', axis: 'col', index: b.col, start: b.start, size: b.size, at: p.x };
      this.paint();
      return;
    }
    if ('RowBorder' in hit) {
      const b = hit.RowBorder;
      this.drag = { kind: 'resize', axis: 'row', index: b.row, start: b.start, size: b.size, at: p.y };
      this.paint();
    }
  }

  private onPointerMove(e: PointerEvent): void {
    const p = this.point(e);
    if (!this.drag) {
      void this.updateCursor(p);
      return;
    }
    if (this.drag.kind === 'resize') {
      this.drag.at = this.drag.axis === 'col' ? p.x : p.y;
      this.paint();
      return;
    }
    this.dragPoint = p;
    this.startAutoscroll();
    void this.dragTo();
  }

  private cursorBusy = false;
  private async updateCursor(p: { x: number; y: number }): Promise<void> {
    if (this.cursorBusy) return;
    this.cursorBusy = true;
    try {
      const hit = await this.model.hitTest(p.x, p.y);
      this.els.scroller.style.cursor = cursorFor(hit);
    } finally {
      this.cursorBusy = false;
    }
  }

  /** Extends the drag to the cell under the pointer (one request at a time). */
  private async dragTo(): Promise<void> {
    if (this.dragBusy || !this.drag || !this.dragPoint) return;
    this.dragBusy = true;
    const p = this.dragPoint;
    try {
      // Clamp to the cell area so dragging over the headers keeps selecting.
      const l = this.layout;
      const x = Math.min(Math.max(p.x, l.header_w * this.zoom + 1), this.cssW - 1);
      const y = Math.min(Math.max(p.y, l.header_h * this.zoom + 1), this.cssH - 1);
      const hit = await this.model.hitTest(x, y);
      const cell = hit !== 'None' && typeof hit === 'object' && 'Cell' in hit ? hit.Cell : null;
      const d = this.drag;
      if (!d) return;
      if (d.kind === 'cells' && cell) {
        await this.model.core(d.point ? 'grid.editing.point_ref' : 'grid.nav.select_cell', {
          row: cell.row,
          col: cell.col,
          extend: true,
        });
      } else if (d.kind === 'fill' && cell) {
        await this.model.core('grid.editing.drag_fill', { row: cell.row, col: cell.col });
      } else if (d.kind === 'cols') {
        const h = await this.model.hitTest(x, (l.header_h * this.zoom) / 2);
        const col = typeof h === 'object' && 'ColumnHeader' in h ? h.ColumnHeader.col : cell?.col;
        if (col !== undefined)
          await this.model.core('grid.nav.select_columns', { from: col, to: col, extend: true });
      } else if (d.kind === 'rows') {
        const h = await this.model.hitTest((l.header_w * this.zoom) / 2, y);
        const row = typeof h === 'object' && 'RowHeader' in h ? h.RowHeader.row : cell?.row;
        if (row !== undefined)
          await this.model.core('grid.nav.select_rows', { from: row, to: row, extend: true });
      }
    } finally {
      this.dragBusy = false;
    }
    if (this.dragPoint !== p) void this.dragTo();
  }

  private startAutoscroll(): void {
    if (this.autoscroll) return;
    const step = () => {
      this.autoscroll = 0;
      const p = this.dragPoint;
      if (!this.drag || !p || this.drag.kind === 'resize') return;
      const l = this.layout;
      const top = (l.header_h + l.frozen_h) * this.zoom;
      const left = (l.header_w + l.frozen_w) * this.zoom;
      let dx = 0;
      let dy = 0;
      if (p.x > this.cssW - AUTOSCROLL_EDGE) dx = p.x - (this.cssW - AUTOSCROLL_EDGE);
      else if (p.x < left + AUTOSCROLL_EDGE && p.x < left + 4) dx = p.x - (left + AUTOSCROLL_EDGE);
      if (p.y > this.cssH - AUTOSCROLL_EDGE) dy = p.y - (this.cssH - AUTOSCROLL_EDGE);
      else if (p.y < top + 4) dy = p.y - (top + AUTOSCROLL_EDGE);
      if (dx === 0 && dy === 0) return;
      this.els.scroller.scrollLeft += Math.sign(dx) * Math.min(40, Math.abs(dx) * 0.6);
      this.els.scroller.scrollTop += Math.sign(dy) * Math.min(40, Math.abs(dy) * 0.6);
      void this.dragTo();
      this.autoscroll = requestAnimationFrame(step);
    };
    this.autoscroll = requestAnimationFrame(step);
  }

  private async onPointerUp(e: PointerEvent): Promise<void> {
    const d = this.drag;
    this.drag = null;
    this.dragPoint = null;
    this.els.scroller.releasePointerCapture?.(e.pointerId);
    if (!d) return;
    if (d.kind === 'resize') {
      const px = Math.max(0, (d.at - d.start) / this.zoom);
      const id = d.axis === 'col' ? 'grid.cells.resize_columns' : 'grid.cells.resize_rows';
      if (Math.abs(px - d.size) >= 1)
        await this.model.run(id, { from: d.index, to: d.index, px: Math.round(px) });
      this.paint();
      return;
    }
    if (d.kind === 'fill') {
      const p = this.point(e);
      const hit = await this.model.hitTest(p.x, p.y);
      if (typeof hit === 'object' && 'Cell' in hit) {
        await this.model.run('grid.editing.auto_fill', {
          row: hit.Cell.row,
          col: hit.Cell.col,
          mode: 'Auto',
        });
      } else {
        await this.model.core('grid.editing.cancel_edit');
      }
    }
  }

  private async onDoubleClick(e: MouseEvent): Promise<void> {
    const p = this.point(e);
    const hit = await this.model.hitTest(p.x, p.y);
    if (hit === 'None') return;
    if (typeof hit === 'object' && 'ColumnBorder' in hit) {
      const c = hit.ColumnBorder.col;
      await this.model.run('grid.cells.autofit_columns', { from: c, to: c });
    } else if (typeof hit === 'object' && 'RowBorder' in hit) {
      const r = hit.RowBorder.row;
      await this.model.run('grid.cells.autofit_rows', { from: r, to: r });
    } else if (typeof hit === 'object' && 'Cell' in hit && !this.editing) {
      this.startEdit();
    }
  }

  private async onContextMenu(e: MouseEvent): Promise<void> {
    e.preventDefault();
    const p = this.point(e);
    const hit = await this.model.hitTest(p.x, p.y);
    const kind =
      typeof hit === 'object' && 'ColumnHeader' in hit
        ? 'column'
        : typeof hit === 'object' && 'RowHeader' in hit
          ? 'row'
          : 'cell';
    // Right-clicking outside the selection selects what is under the pointer first (Excel).
    if (typeof hit === 'object' && 'Cell' in hit && !this.inSelection(hit.Cell.row, hit.Cell.col))
      await this.model.core('grid.nav.select_cell', { row: hit.Cell.row, col: hit.Cell.col });
    if (typeof hit === 'object' && 'ColumnHeader' in hit && !this.state?.whole_columns)
      await this.model.core('grid.nav.select_columns', {
        from: hit.ColumnHeader.col,
        to: hit.ColumnHeader.col,
      });
    if (typeof hit === 'object' && 'RowHeader' in hit && !this.state?.whole_rows)
      await this.model.core('grid.nav.select_rows', { from: hit.RowHeader.row, to: hit.RowHeader.row });
    const r = this.els.root.getBoundingClientRect();
    this.model.set({ menu: { x: p.x + r.left, y: p.y + r.top, kind } });
  }

  private inSelection(row: number, col: number): boolean {
    const sel = this.model.registry.context.core as { selection?: { kind?: unknown } } | undefined;
    const kind = sel?.selection?.kind as
      | { Cells?: { ranges: { start: { row: number; col: number }; end: { row: number; col: number } }[] } }
      | undefined;
    const ranges = kind?.Cells?.ranges ?? [];
    return ranges.some(
      (r) =>
        Math.min(r.start.row, r.end.row) <= row &&
        row <= Math.max(r.start.row, r.end.row) &&
        Math.min(r.start.col, r.end.col) <= col &&
        col <= Math.max(r.start.col, r.end.col),
    );
  }

  // ── keys and typing ─────────────────────────────────────────────────────────────────────

  private onKeyDown(e: KeyboardEvent): void {
    if (e.isComposing) return;
    if (this.editStart) {
      // F2's text has not arrived. Plain typing goes into the proxy and joins the text when it
      // does; every other key (and typing after one) waits for it, in order, and is decided by
      // the mode it then finds, so "F2, Alt+Enter, Enter, Up" behaves as it would slowly.
      if (!this.keysWaiting && typesText(e) && !e.altKey) {
        e.stopPropagation();
        return;
      }
      if (gridKeyAction(e, 'Edit', null) === null && !typesText(e)) return; // the window's shortcut
      e.preventDefault();
      if (!e.altKey) e.stopPropagation();
      const k: KeyLike = {
        key: e.key,
        shiftKey: e.shiftKey,
        ctrlKey: e.ctrlKey,
        metaKey: e.metaKey,
        altKey: e.altKey,
      };
      this.keysWaiting += 1;
      this.afterEditStart(
        this.editStart.then(() => {
          this.keysWaiting -= 1;
          this.handleKey(k, null);
        }),
      );
      return;
    }
    this.handleKey(e, e);
  }

  /**
   * A key in the mode the editor is in now. `e` is the live event (null for a key that waited
   * for F2's text: it can no longer be left to the browser, so typing and caret keys are
   * replayed here).
   */
  private handleKey(k: KeyLike, e: KeyboardEvent | null): void {
    const ed = this.state?.editor ?? null;
    const mode = this.pendingEdit
      ? 'Edit'
      : this.editing
        ? this.state?.mode && this.state.mode !== 'Ready'
          ? this.state.mode
          : 'Enter'
        : 'Ready';
    // Paste with the clipboard's data: let the browser raise the paste event (Ctrl+V).
    if (
      e?.isTrusted &&
      !this.editing &&
      (k.ctrlKey || k.metaKey) &&
      !k.altKey &&
      k.key.toLowerCase() === 'v' &&
      !k.shiftKey
    ) {
      e.stopPropagation();
      return;
    }
    if (mode === 'Ready' && k.key === 'F2' && !k.ctrlKey && !k.metaKey && !k.altKey && !k.shiftKey) {
      e?.preventDefault();
      e?.stopPropagation();
      this.startEdit();
      return;
    }
    const action = gridKeyAction(
      k,
      mode,
      ed ? { completions: ed.completions.length, pickList: !!ed.pick_list } : null,
    );
    if (!action) return;
    if (action.kind === 'native') {
      if (e) {
        // Typing goes into the proxy; the window's shortcuts must not see it.
        e.stopPropagation();
      } else {
        this.replayNative(k);
      }
      return;
    }
    e?.preventDefault();
    // A consumed Alt chord still reaches the window (which skips handled keys), so releasing Alt
    // after Alt+Enter does not count as a lone Alt and open the Key Tips.
    if (!k.altKey) e?.stopPropagation();
    this.applyKey(action);
  }

  /** What the textarea would have done with a key that waited (typing, caret movement). */
  private replayNative(k: KeyLike): void {
    const p = this.els.proxy;
    const start = p.selectionStart ?? p.value.length;
    const end = p.selectionEnd ?? start;
    if (typesText(k) && k.key.length > 0 && k.key !== 'Dead' && k.key !== 'Process') {
      p.value = `${p.value.slice(0, start)}${k.key}${p.value.slice(end)}`;
      p.setSelectionRange(start + k.key.length, start + k.key.length);
      this.onInput();
      return;
    }
    let caret: number | null = null;
    if (k.key === 'ArrowLeft') caret = Math.max(0, start === end ? start - 1 : start);
    else if (k.key === 'ArrowRight') caret = Math.min(p.value.length, start === end ? end + 1 : end);
    else if (k.key === 'Home') caret = 0;
    else if (k.key === 'End') caret = p.value.length;
    else if (k.key === 'Backspace' || k.key === 'Delete') {
      const back = k.key === 'Backspace';
      const from = start === end ? (back ? Math.max(0, start - 1) : start) : start;
      const to = start === end ? (back ? start : Math.min(p.value.length, end + 1)) : end;
      p.value = `${p.value.slice(0, from)}${p.value.slice(to)}`;
      p.setSelectionRange(from, from);
      this.onInput();
      return;
    }
    if (caret !== null) {
      p.setSelectionRange(caret, caret);
      this.onSelectionChange();
    }
  }

  /** Carries out a grid key that is not plain typing. */
  private applyKey(action: Exclude<KeyAction, { kind: 'native' }>): void {
    const ed = this.state?.editor ?? null;
    if (action.kind === 'newline') {
      const p = this.els.proxy;
      const start = p.selectionStart ?? p.value.length;
      const end = p.selectionEnd ?? start;
      p.value = `${p.value.slice(0, start)}\n${p.value.slice(end)}`;
      p.setSelectionRange(start + 1, start + 1);
      this.onInput();
      return;
    }
    const args = { ...(action.args ?? {}) };
    if (action.id === 'grid.editing.accept_completion') args.index = ed?.completion_index ?? 0;
    if (
      action.id === 'grid.editing.commit' ||
      action.id === 'grid.editing.cancel_edit' ||
      action.id === 'grid.editing.fill_selection' ||
      action.id === 'grid.editing.array_formula'
    ) {
      this.editing = false;
      this.pendingEdit = false;
      this.sentText = '';
      this.echoes = [];
      this.els.proxy.value = '';
      this.ending += 1;
      void this.model.core(action.id, args).then((ok) => {
        this.ending -= 1;
        // A refused entry (data validation) stays for correction: the next state brings it back.
        if (!ok) this.model.refresh();
      });
      return;
    }
    if (action.id === 'grid.editing.begin') this.editing = true;
    void this.model.core(action.id, args);
  }

  /** F2 / double-click: edit the active cell in place. */
  private startEdit(): void {
    this.pendingEdit = true;
    this.els.proxy.value = '';
    const started = this.model.core('grid.editing.edit_cell').then(async (ok) => {
      // The state with the cell's text, at once (not on the next frame): it ends `pendingEdit`.
      if (ok) await this.model.fetch();
      if (!ok || this.pendingEdit) this.pendingEdit = false;
    });
    this.afterEditStart(started);
  }

  /** Makes `p` the end of the chain of keys waiting for F2's text (cleared when it settles last). */
  private afterEditStart(p: Promise<void>): void {
    const chain: Promise<void> = p
      .catch(() => undefined)
      .finally(() => {
        if (this.editStart === chain) this.editStart = null;
      });
    this.editStart = chain;
  }

  private onInput(): void {
    if (this.pendingEdit) return;
    const p = this.els.proxy;
    const text = p.value;
    if (!this.editing) {
      if (text === '') return;
      this.editing = true;
      this.sentText = text;
      this.lastSel = this.selKey();
      this.echoes.push(text);
      void this.model.core('grid.editing.begin', { text });
      return;
    }
    this.sentText = text;
    // The browser's selectionchange for this keystroke comes later: it must not send this text
    // again after a key that changed it in the core (F4, a suggestion).
    this.lastSel = this.selKey();
    this.echoes.push(text);
    if (this.echoes.length > 64) this.echoes.splice(0, this.echoes.length - 64);
    void this.model.core('grid.editing.set_text', {
      text,
      start: p.selectionStart ?? text.length,
      end: p.selectionEnd ?? text.length,
      formula_bar: false,
    });
  }

  private lastSel = '';
  /** The proxy's caret and length, as `onSelectionChange` compares them. */
  private selKey(): string {
    const p = this.els.proxy;
    return `${p.selectionStart}:${p.selectionEnd}:${p.value.length}`;
  }

  private onSelectionChange(): void {
    if (!this.editing || document.activeElement !== this.els.proxy) return;
    const p = this.els.proxy;
    const key = this.selKey();
    if (key === this.lastSel || p.value !== this.sentText) return;
    this.lastSel = key;
    void this.model.core('grid.editing.set_text', {
      text: p.value,
      start: p.selectionStart ?? 0,
      end: p.selectionEnd ?? 0,
      formula_bar: false,
    });
  }

  private onPaste(e: ClipboardEvent): void {
    if (this.editing) return;
    const d = e.clipboardData;
    if (!d) return;
    e.preventDefault();
    const text = d.getData('text/plain');
    const html = d.getData('text/html');
    const kiln = d.getData('application/x-kiln-fragment');
    void this.model.run('suite.edit.paste', { data: { text, html: html || null, kiln: kiln || null } });
  }
}

function sameViewport(a: Viewport | null, b: Viewport): boolean {
  return (
    a !== null &&
    a.width_px === b.width_px &&
    a.height_px === b.height_px &&
    a.scroll.x === b.scroll.x &&
    a.scroll.y === b.scroll.y &&
    a.zoom === b.zoom &&
    a.dpr === b.dpr
  );
}

/** The pointer cursor over a hit target (Excel's: a thick cross on cells, arrows on headers). */
export function cursorFor(hit: GridHit): string {
  if (hit === 'None') return 'default';
  if (hit === 'Corner') return 'default';
  if (hit === 'FillHandle') return 'crosshair';
  if ('ColumnBorder' in hit) return 'col-resize';
  if ('RowBorder' in hit) return 'row-resize';
  if ('ColumnHeader' in hit) return 's-resize';
  if ('RowHeader' in hit) return 'e-resize';
  return 'cell';
}

/** Ids of the core's accessibility nodes (`kiln_sheet_layout::a11y`). */
const CELL_BASE = 2 ** 40;
const ROW_BASE = 2 ** 39;

/**
 * Mirrors the core's accessibility tree into an off-screen ARIA grid: rows and cells with their
 * indices (decoded from the node ids), names and values; the proxy points at the active cell and
 * the polite live region announces it.
 */
export function renderA11y(
  host: HTMLElement,
  tree: import('@kiln/protocol').A11yTree,
  proxy: HTMLElement,
): void {
  const grid = tree.nodes[0];
  if (!grid) {
    host.replaceChildren();
    return;
  }
  const byId = new Map(tree.nodes.map((n) => [n.id, n]));
  const [rows, cols] = (grid.text ?? '1048576 16384').split(' ').map(Number);
  const table = document.createElement('div');
  table.setAttribute('role', 'grid');
  table.setAttribute('aria-label', grid.name ?? 'Sheet');
  table.setAttribute('aria-rowcount', String((rows ?? 0) + 1));
  table.setAttribute('aria-colcount', String((cols ?? 0) + 1));
  table.setAttribute('aria-multiselectable', 'true');
  for (const rowId of grid.children) {
    const row = byId.get(rowId);
    if (!row) continue;
    const tr = document.createElement('div');
    tr.setAttribute('role', 'row');
    const header = rowId === 2 ** 38;
    const rowIndex = header ? 1 : rowId - ROW_BASE + 2;
    tr.setAttribute('aria-rowindex', String(rowIndex));
    if (!header) {
      const th = document.createElement('div');
      th.setAttribute('role', 'rowheader');
      th.setAttribute('aria-colindex', '1');
      th.textContent = row.name ?? '';
      tr.append(th);
    }
    for (const cellId of row.children) {
      const c = byId.get(cellId);
      if (!c) continue;
      const td = document.createElement('div');
      if (header) {
        td.setAttribute('role', 'columnheader');
        td.setAttribute('aria-colindex', String(cellId - (2 ** 38 + 1) + 2));
        td.textContent = c.name ?? '';
      } else {
        td.setAttribute('role', 'gridcell');
        td.setAttribute('aria-colindex', String(((cellId - CELL_BASE) % 32768) + 2));
        td.id = `kiln-grid-cell-${cellId}`;
        td.setAttribute('aria-label', `${c.name ?? ''} ${c.text ?? ''}`.trim());
        td.textContent = c.text ?? '';
        if (cellId === tree.active) td.setAttribute('aria-selected', 'true');
      }
      tr.append(td);
    }
    table.append(tr);
  }
  const live = document.createElement('div');
  live.setAttribute('aria-live', 'polite');
  live.setAttribute('role', 'status');
  live.textContent = tree.announce ?? '';
  host.replaceChildren(table, live);
  if (tree.active !== null) proxy.setAttribute('aria-activedescendant', `kiln-grid-cell-${tree.active}`);
}
