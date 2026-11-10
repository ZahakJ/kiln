/**
 * The slide stage: paints the core's Scene of the current slide —
 * slide tiles on the pasteboard, the overlay with the selection outline, the eight handles and the
 * rotation handle, smart guides while dragging, the text caret and selection — at the zoom that
 * fits the window (or the one chosen), centred. Every gesture is the core's: pointer down, move
 * and up go to it in slide coordinates (`deck.stage.pointer_*`), and it decides whether they
 * select, move, resize, rotate, draw or place the caret, with Shift, Ctrl and Alt as PowerPoint
 * has them. The cursor over a point is the core's answer to `Query::Deck(HitTest)`.
 *
 * Keys go through `keys.ts`; a hidden textarea is the input proxy for typing and IME. Nothing
 * here lays out, measures or snaps anything.
 */
import type { DeckCursor, DeckState, KilnClient, SceneDelta, ViewMode, Viewport } from '@kiln/protocol';
import { device, Painter, type PainterCanvas, paletteFor, SceneCache } from '@kiln/surface';
import { deckKeyAction, type KeyLike, type StageMode, typesText } from './keys.ts';
import type { DeckEditorModel, DeckStageHandle, StateFreshness } from './model.ts';

/** The DOM the controller drives. */
export interface StageElements {
  root: HTMLElement;
  canvas: HTMLCanvasElement;
  proxy: HTMLTextAreaElement;
  a11y: HTMLElement;
}

export interface StageOptions {
  theme: 'light' | 'dark';
  /** Blink the caret (screenshots turn it off). */
  blink: boolean;
}

/** CSS px kept around the slide when it is zoomed to fit. */
export const FIT_MARGIN = 28;
const BLINK_MS = 530;
/** Presses this close in time (ms) and place (CSS px) make a double or triple click. */
const CLICK_MS = 500;
const CLICK_SLOP = 4;
const MIN_ZOOM = 0.1;
const MAX_ZOOM = 4;

/** The zoom at which a slide fits a view with the margin, and the scroll that centres it. */
export function fitView(
  slide: { w: number; h: number },
  view: { w: number; h: number },
): { zoom: number; x: number; y: number } {
  // Phones keep a thin margin so the slide takes the width.
  const margin = view.w < 560 ? 6 : FIT_MARGIN;
  const zoom = Math.max(
    MIN_ZOOM,
    Math.min(MAX_ZOOM, (view.w - 2 * margin) / slide.w, (view.h - 2 * margin) / slide.h),
  );
  return { zoom, x: (slide.w - view.w / zoom) / 2, y: (slide.h - view.h / zoom) / 2 };
}

/**
 * The scroll of a view at `zoom`: centred on an axis where the slide fits, else kept within the
 * slide and a margin of pasteboard.
 */
export function clampScroll(
  x: number,
  y: number,
  zoom: number,
  slide: { w: number; h: number },
  view: { w: number; h: number },
): { x: number; y: number } {
  const axis = (v: number, side: number, len: number) => {
    const visible = len / zoom;
    const margin = FIT_MARGIN / zoom;
    if (visible >= side + 2 * margin) return (side - visible) / 2;
    return Math.max(-margin, Math.min(side + margin - visible, v));
  };
  return { x: axis(x, slide.w, view.w), y: axis(y, slide.h, view.h) };
}

/** The CSS cursor of the core's cursor over a point. */
export function cssCursor(c: DeckCursor): string {
  if (typeof c === 'object' && 'Resize' in c) {
    // Handles clockwise from the top-left: NW, N, NE, E, SE, S, SW, W (turned with the shape).
    const all = ['nwse-resize', 'ns-resize', 'nesw-resize', 'ew-resize'];
    return all[c.Resize % 4] ?? 'move';
  }
  switch (c) {
    case 'Move':
      return 'move';
    case 'Text':
      return 'text';
    case 'Rotate':
      return 'grab';
    case 'Crosshair':
      return 'crosshair';
    default:
      return 'default';
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

/** Drives the stage of one presentation. */
export class DeckStageController implements DeckStageHandle {
  readonly cache = new SceneCache<PainterCanvas>();
  readonly painter: Painter;
  private readonly cleanups: Array<() => void> = [];
  private zoom = 1;
  private dpr = 1;
  private cssW = 0;
  private cssH = 0;
  /** The view's top-left in slide px. */
  private sx = 0;
  private sy = 0;
  private slide = { w: 1280, h: 720 };
  private sent: Viewport | null = null;
  private inFlight = false;
  private frameWanted = false;
  private raf = 0;
  private disposed = false;
  private caretOn = true;
  private blinkTimer: ReturnType<typeof setInterval> | null = null;
  private focused = false;
  private fresh = true;
  private state: DeckState | null = null;
  /** A pointer is down on the stage. */
  private pressed: number | null = null;
  /** The latest drag position not yet sent (one move in flight at a time). */
  private moveWanted: {
    x: number;
    y: number;
    e: Pick<KeyLike, 'shiftKey' | 'ctrlKey' | 'metaKey' | 'altKey'>;
  } | null = null;
  private moving = false;
  private hoverTimer: ReturnType<typeof setTimeout> | null = null;
  private composing = false;
  /**
   * Typing started on a selected shape: the core is editing its text, whatever the last state
   * (asked before the typing) says, until a state shows the edit.
   */
  private expected: { mode: StageMode; after: number } | null = null;
  private a11yTimer: ReturnType<typeof setTimeout> | null = null;
  /** The slide the live region last named. */
  private a11ySlide: number | null = null;
  /** The zoom the stage last sent (a different one in the status is a zoom the user chose). */
  private sentZoom = 0;
  /** Frames painted (diagnostics, tests). */
  stats = { frames: 0, composites: 0, lastFrameMs: 0 };
  private readonly compositeListeners = new Set<() => void>();
  /**
   * A finger's zoom (a pinch, a double tap): the core's statuses from before it are stale and do
   * not take the view back; `null` with none under way.
   */
  private zoomHold: { zoom: number | null; until: number } | null = null;
  /** Where a finger pressed (a second finger is a pinch, not a press of its own). */
  private touchPress: { id: number; x: number; y: number } | null = null;

  constructor(
    private readonly client: KilnClient,
    private readonly model: DeckEditorModel,
    private readonly els: StageElements,
    private options: StageOptions,
  ) {
    this.painter = new Painter(paletteFor(options.theme, 'deck'));
  }

  // ── lifecycle ───────────────────────────────────────────────────────────────────────────

  start(): void {
    const { root, proxy } = this.els;
    const on = <K extends keyof HTMLElementEventMap>(
      el: HTMLElement | Document,
      type: K,
      fn: (e: HTMLElementEventMap[K]) => void,
      opts?: AddEventListenerOptions,
    ) => {
      el.addEventListener(type, fn as EventListener, opts);
      this.cleanups.push(() => el.removeEventListener(type, fn as EventListener, opts));
    };
    on(root, 'pointerdown', (e) => this.onPointerDown(e));
    on(root, 'pointermove', (e) => this.onPointerMove(e));
    on(root, 'pointerup', (e) => this.onPointerUp(e));
    on(root, 'pointercancel', (e) => this.onPointerUp(e));
    on(root, 'wheel', (e) => this.onWheel(e), { passive: false });
    on(root, 'contextmenu', (e) => e.preventDefault());
    on(proxy, 'keydown', (e) => this.onKeyDown(e));
    on(proxy, 'input', () => this.onInput());
    on(proxy, 'paste', (e) => this.onPaste(e));
    on(proxy, 'compositionstart', () => {
      this.composing = true;
    });
    on(proxy, 'compositionupdate', (e) => {
      void this.model.core('deck.text.composition', { text: (e as CompositionEvent).data ?? '' });
      this.requestFrame();
    });
    on(proxy, 'compositionend', (e) => {
      this.composing = false;
      const text = (e as CompositionEvent).data ?? '';
      proxy.value = '';
      void this.model.core('deck.text.composition', { text: '' });
      if (text) void this.model.core('deck.text.insert', { text });
      this.requestFrame();
    });
    on(proxy, 'focus', () => {
      this.focused = true;
      if (this.model.state.rail) this.model.set({ rail: false });
      this.paint();
    });
    on(proxy, 'blur', () => {
      this.focused = false;
      this.paint();
    });
    // Fonts the core fetched for text it could not draw (Arabic on the web): draw again.
    const onFonts = () => this.requestFrame();
    window.addEventListener('kiln:fonts-added', onFonts);
    this.cleanups.push(() => window.removeEventListener('kiln:fonts-added', onFonts));
    const ro = new ResizeObserver(() => this.measure());
    ro.observe(root);
    this.cleanups.push(() => ro.disconnect());
    this.cleanups.push(this.model.onState((s, fresh) => this.onState(s, fresh)));
    this.cleanups.push(
      this.model.store.subscribe((s, prev) => {
        if (s.status !== prev.status) this.onStatus();
        if (s.fit !== prev.fit && s.fit) this.fitToWindow();
      }),
    );
    this.measure();
    if (this.options.blink) {
      this.blinkTimer = setInterval(() => {
        if (!this.state?.editing) return;
        this.caretOn = !this.caretOn;
        this.paint();
      }, BLINK_MS);
    }
    const s = this.model.state.deck;
    if (s) this.onState(s);
  }

  dispose(): void {
    this.disposed = true;
    for (const c of this.cleanups.splice(0)) c();
    if (this.raf) cancelAnimationFrame(this.raf);
    if (this.blinkTimer) clearInterval(this.blinkTimer);
    if (this.hoverTimer) clearTimeout(this.hoverTimer);
    if (this.a11yTimer) clearTimeout(this.a11yTimer);
  }

  setOptions(options: Partial<StageOptions>): void {
    this.options = { ...this.options, ...options };
    const palette = paletteFor(this.options.theme, 'deck');
    if (palette.name !== this.painter.palette.name) {
      this.painter.setPalette(palette);
      this.cache.invalidateCanvases();
      this.paint();
    }
  }

  // ── DeckStageHandle ─────────────────────────────────────────────────────────────────────

  focus(): void {
    this.els.proxy.focus({ preventScroll: true });
  }

  size(): { width: number; height: number } {
    return { width: this.cssW, height: this.cssH };
  }

  fitToWindow(): void {
    if (!this.model.state.fit) {
      this.model.set({ fit: true });
      return;
    }
    if (this.cssW <= 0 || this.cssH <= 0) return;
    const f = fitView(this.slide, { w: this.cssW, h: this.cssH });
    this.zoom = f.zoom;
    this.sx = f.x;
    this.sy = f.y;
    this.paint();
    this.requestFrame();
  }

  /** The current view, for tests: zoom and the slide's top-left on the stage (CSS px). */
  view(): { zoom: number; left: number; top: number; width: number; height: number } {
    return {
      zoom: this.zoom,
      left: -this.sx * this.zoom,
      top: -this.sy * this.zoom,
      width: this.slide.w * this.zoom,
      height: this.slide.h * this.zoom,
    };
  }

  // ── state ───────────────────────────────────────────────────────────────────────────────

  private onState(s: DeckState, fresh?: StateFreshness): void {
    const prev = this.state;
    this.state = s;
    // A state that saw every command the expectation came from replaces it.
    const e = this.expected;
    if (e && (!fresh || (fresh.settled && fresh.sentBefore >= e.after))) this.expected = null;
    const size = { w: s.slide_width, h: s.slide_height };
    if (size.w !== this.slide.w || size.h !== this.slide.h) {
      this.slide = size;
      if (this.model.state.fit) this.fitToWindow();
    }
    if (s.editing !== prev?.editing || s.caret !== prev?.caret) this.placeProxy();
    this.caretOn = true;
    this.requestFrame();
  }

  /** A zoom chosen elsewhere (the status bar's slider, View ▸ Zoom) ends zoom-to-fit. */
  private onStatus(): void {
    const z = this.model.state.status?.zoom;
    // Until the stage has sent its view, the core's zoom is its default, not a choice.
    if (this.sentZoom === 0) return;
    // A finger is zooming: statuses until its zoom arrives are from before it.
    const hold = this.zoomHold;
    if (hold) {
      const done = hold.zoom !== null && z !== undefined && Math.abs(z - hold.zoom) < 5e-3;
      if (done || performance.now() > hold.until) this.zoomHold = null;
      return;
    }
    if (z === undefined || Math.abs(z - this.zoom) < 1e-4 || Math.abs(z - this.sentZoom) < 1e-4) return;
    const cx = this.sx + this.cssW / this.zoom / 2;
    const cy = this.sy + this.cssH / this.zoom / 2;
    this.zoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, z));
    const c = clampScroll(
      cx - this.cssW / this.zoom / 2,
      cy - this.cssH / this.zoom / 2,
      this.zoom,
      this.slide,
      {
        w: this.cssW,
        h: this.cssH,
      },
    );
    this.sx = c.x;
    this.sy = c.y;
    if (this.model.state.fit) this.model.set({ fit: false });
    this.paint();
    this.requestFrame();
  }

  /** The proxy sits at the caret (the IME's candidate window follows it). */
  private placeProxy(): void {
    const c = this.state?.caret;
    const p = this.els.proxy.style;
    if (!c) {
      p.left = '0px';
      p.top = '0px';
      return;
    }
    p.left = `${Math.round((c.x - this.sx) * this.zoom)}px`;
    p.top = `${Math.round((c.y - this.sy) * this.zoom)}px`;
    p.height = `${Math.max(8, Math.round(c.height * this.zoom))}px`;
  }

  // ── geometry ────────────────────────────────────────────────────────────────────────────

  private measure(): void {
    const { root, canvas } = this.els;
    const w = root.clientWidth;
    const h = root.clientHeight;
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
    if (this.model.state.fit) this.fitToWindow();
    else {
      const c = clampScroll(this.sx, this.sy, this.zoom, this.slide, { w, h });
      this.sx = c.x;
      this.sy = c.y;
    }
    this.paint();
    this.requestFrame();
  }

  private viewport(): Viewport {
    return {
      width_px: this.cssW,
      height_px: this.cssH,
      scroll: { x: this.sx, y: this.sy },
      zoom: this.zoom,
      dpr: this.dpr,
      // The stage stays under Reading View and Notes Page view: it keeps the view chosen.
      mode: deckView(this.model.state.status?.view),
    };
  }

  /** Slide coordinates of a pointer event. */
  toSlide(e: { clientX: number; clientY: number }): { x: number; y: number } {
    const r = this.els.root.getBoundingClientRect();
    return { x: this.sx + (e.clientX - r.left) / this.zoom, y: this.sy + (e.clientY - r.top) / this.zoom };
  }

  // ── frames ──────────────────────────────────────────────────────────────────────────────

  /** Asks the core for the next frame (one in flight). */
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
      if (this.disposed) return;
      const vp = this.viewport();
      if (!sameViewport(this.sent, vp)) {
        this.sentZoom = vp.zoom;
        await this.client.setViewport(this.model.doc, vp);
        this.sent = vp;
      }
      if (this.disposed) return;
      delta = await this.client.frame(this.model.doc);
      this.cache.apply(delta);
      this.stats.frames += 1;
      this.stats.lastFrameMs = performance.now() - t0;
      const ds = this.els.root.dataset;
      ds.kilnDoc = String(this.model.doc);
      ds.kilnFrames = String(this.stats.frames);
      ds.kilnFrameMs = this.stats.lastFrameMs.toFixed(2);
      try {
        performance.measure('kiln:deck-frame', { start: t0, end: performance.now() });
        if (this.stats.frames % 600 === 0) performance.clearMeasures('kiln:deck-frame');
      } catch {
        // Engines without measure options: the stats still count frames.
      }
    } catch (error) {
      this.cache.tiles.clear();
      void this.client.execute(this.model.doc, { Suite: 'Repaint' }).catch(() => undefined);
      console.warn('kiln deck: frame failed', error);
    } finally {
      this.inFlight = false;
    }
    if (this.disposed) return;
    this.paint();
    this.scheduleA11y();
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
          if (this.disposed) return;
          const root = tree.nodes[0];
          // Read when another slide is shown, not while its title is typed.
          if (root?.id !== this.a11ySlide) {
            this.a11ySlide = root?.id ?? null;
            this.els.a11y.textContent = root?.name ?? '';
          }
          const names = tree.nodes
            .slice(1)
            .map((n) => n.name)
            .filter((n): n is string => !!n);
          this.els.proxy.setAttribute('aria-description', names.join('; '));
        },
        () => undefined,
      );
    }, 150);
  }

  // ── painting ────────────────────────────────────────────────────────────────────────────

  paint(): void {
    if (this.raf || this.disposed) return;
    this.raf = requestAnimationFrame(() => {
      this.raf = 0;
      this.composite();
    });
  }

  composite(): void {
    const ctx = this.els.canvas.getContext('2d');
    if (!ctx) return;
    const vs = this.zoom * this.dpr;
    const view = {
      width: this.els.canvas.width,
      height: this.els.canvas.height,
      scrollX: this.sx,
      scrollY: this.sy,
      viewScale: vs,
    };
    this.painter.composite(ctx, this.cache, view);
    const marks = this.cache.overlay.filter((op) => !(typeof op === 'object' && 'SelectionRects' in op));
    if (marks.length > 0) {
      const tx = -device(this.sx, vs);
      const ty = -device(this.sy, vs);
      ctx.setTransform(vs, 0, 0, vs, tx, ty);
      this.painter.paintOps(ctx, marks, this.cache, {
        scale: vs,
        glyphOrigin: [tx, ty],
        caretVisible: this.focused && this.caretOn,
        touch: false,
      });
      ctx.setTransform(1, 0, 0, 1, 0, 0);
    }
    this.stats.composites += 1;
    for (const l of this.compositeListeners) l();
  }

  // ── touch ───────────────────────────────────────────────────────────────────────

  /** Called after every composite (the touch handles follow the slide). */
  onComposite(listener: () => void): () => void {
    this.compositeListeners.add(listener);
    return () => this.compositeListeners.delete(listener);
  }

  /** The stage's element. */
  get root(): HTMLElement {
    return this.els.root;
  }

  /** The zoom at which the slide fits the stage. */
  fitZoom(): number {
    return fitView(this.slide, { w: this.cssW, h: this.cssH }).zoom;
  }

  /**
   * Zooms to `zoom` about a client point (pinch, double tap), as Ctrl+wheel does; `commit` false
   * while the fingers still move (the zoom command follows when they lift).
   */
  zoomAbout(zoom: number, clientX: number, clientY: number, commit = true): void {
    const r = this.els.root.getBoundingClientRect();
    const p = this.toSlide({ clientX, clientY });
    const z = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, zoom));
    const c = clampScroll(p.x - (clientX - r.left) / z, p.y - (clientY - r.top) / z, z, this.slide, {
      w: this.cssW,
      h: this.cssH,
    });
    this.zoom = z;
    this.sx = c.x;
    this.sy = c.y;
    if (this.model.state.fit) this.model.set({ fit: false });
    const rounded = Math.round(z * 100) / 100;
    this.zoomHold = { zoom: commit ? rounded : null, until: performance.now() + 2000 };
    if (commit) void this.model.run('suite.view.zoom', { zoom: rounded });
    this.placeProxy();
    this.paint();
    this.requestFrame();
  }

  /** Pans the view by CSS px (a finger dragging a zoomed slide). */
  panBy(dx: number, dy: number): void {
    const c = clampScroll(this.sx - dx / this.zoom, this.sy - dy / this.zoom, this.zoom, this.slide, {
      w: this.cssW,
      h: this.cssH,
    });
    this.sx = c.x;
    this.sy = c.y;
    this.placeProxy();
    this.paint();
    this.requestFrame();
  }

  /** The context menu of what is at a client point (a long press), as a right-click opens it. */
  openMenuAt(clientX: number, clientY: number): void {
    const p = this.toSlide({ clientX, clientY });
    void this.contextMenu({ clientX, clientY }, p);
  }

  // ── input ───────────────────────────────────────────────────────────────────────────────

  private mods(e: Pick<KeyLike, 'shiftKey' | 'ctrlKey' | 'metaKey' | 'altKey'>) {
    return { shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey, alt: e.altKey };
  }

  private onPointerDown(e: PointerEvent): void {
    if (e.button !== 0 && e.button !== 2) return;
    // A second finger: a pinch (the touch layer's), and the first finger's press is let go.
    if (e.pointerType === 'touch') {
      if (this.touchPress && this.touchPress.id !== e.pointerId) {
        if (this.pressed === this.touchPress.id) {
          this.pressed = null;
          const at = this.toSlide({ clientX: this.touchPress.x, clientY: this.touchPress.y });
          void this.model.core('deck.stage.pointer_up', { ...at, shift: false, ctrl: false, alt: false });
        }
        return;
      }
      this.touchPress = { id: e.pointerId, x: e.clientX, y: e.clientY };
    }
    e.preventDefault();
    this.focus();
    const p = this.toSlide(e);
    if (e.button === 2) {
      // Right-click: select what is under the pointer (keeping a selection it is part of),
      // then the context menu for it.
      void this.contextMenu(e, p);
      return;
    }
    this.pressed = e.pointerId;
    this.expected = null;
    try {
      this.els.root.setPointerCapture(e.pointerId);
    } catch {
      // Synthetic events in tests.
    }
    void this.model.core('deck.stage.pointer_down', {
      ...p,
      ...this.mods(e),
      clicks: this.countClick(e),
    });
    this.requestFrame();
  }

  /**
   * The click count of a press (double- and triple-clicks select a word and a paragraph). Chromium
   * reports `detail` 0 on pointer events, so presses close in time and place are counted here.
   */
  countClick(e: { clientX: number; clientY: number; timeStamp: number; detail?: number }): number {
    const last = this.lastPress;
    const near =
      last !== null &&
      e.timeStamp - last.t < CLICK_MS &&
      Math.abs(e.clientX - last.x) <= CLICK_SLOP &&
      Math.abs(e.clientY - last.y) <= CLICK_SLOP;
    const counted = near && last ? Math.min(last.n + 1, 3) : 1;
    // Some engines report 0 or 1 for every press: the larger of theirs and ours.
    const n = Math.max(counted, e.detail ?? 0);
    this.lastPress = { x: e.clientX, y: e.clientY, t: e.timeStamp, n };
    return n;
  }

  private lastPress: { x: number; y: number; t: number; n: number } | null = null;

  private async contextMenu(
    e: { clientX: number; clientY: number },
    p: { x: number; y: number },
  ): Promise<void> {
    const cursor = await this.model.cursorAt(p.x, p.y);
    const s = this.state;
    if (cursor !== 'Default' && !s?.editing) {
      await this.model.core('deck.stage.pointer_down', {
        ...p,
        shift: false,
        ctrl: false,
        alt: false,
        clicks: 1,
      });
      await this.model.core('deck.stage.pointer_up', { ...p, shift: false, ctrl: false, alt: false });
    }
    const st = await this.model.fetch();
    const kind = st?.editing ? 'text' : st && st.selection.length > 0 ? 'shape' : 'slide';
    this.model.set({ menu: { x: e.clientX, y: e.clientY, kind } });
    this.requestFrame();
  }

  private onPointerMove(e: PointerEvent): void {
    const p = this.toSlide(e);
    if (this.pressed === null) {
      this.hover(p);
      return;
    }
    this.moveWanted = { ...p, e };
    void this.flushMove();
  }

  private async flushMove(): Promise<void> {
    if (this.moving) return;
    while (this.moveWanted) {
      const m = this.moveWanted;
      this.moveWanted = null;
      this.moving = true;
      await this.model.core('deck.stage.pointer_move', { x: m.x, y: m.y, ...this.mods(m.e) });
      this.moving = false;
      this.requestFrame();
    }
  }

  private onPointerUp(e: PointerEvent): void {
    if (this.touchPress?.id === e.pointerId) this.touchPress = null;
    if (this.pressed !== e.pointerId) return;
    this.pressed = null;
    try {
      this.els.root.releasePointerCapture(e.pointerId);
    } catch {
      // As in onPointerDown.
    }
    const p = this.toSlide(e);
    this.moveWanted = null;
    void this.model.core('deck.stage.pointer_up', { ...p, ...this.mods(e) }).then(async () => {
      if (this.model.state.painter) {
        // The Format Painter's click: what was clicked takes the copied formatting.
        this.model.set({ painter: false });
        await this.model.core('deck.clipboard.paste_format');
      } else if (this.model.state.animPainter) {
        // The Animation Painter's click: the shape clicked takes the effects picked up.
        this.model.set({ animPainter: false });
        if (
          (this.model.state.deck?.selection.length ?? 0) > 0 ||
          (await this.model.fetch())?.selection.length
        )
          await this.model.core('deck.animations.paste');
      }
      this.model.refresh();
      this.requestFrame();
    });
  }

  private hover(p: { x: number; y: number }): void {
    if (this.hoverTimer) return;
    this.hoverTimer = setTimeout(() => {
      this.hoverTimer = null;
      void this.model.cursorAt(p.x, p.y).then((c) => {
        if (!this.disposed)
          this.els.root.style.cursor =
            this.model.state.painter || this.model.state.animPainter ? 'copy' : cssCursor(c);
      });
    }, 40);
  }

  private onWheel(e: WheelEvent): void {
    e.preventDefault();
    if (e.ctrlKey || e.metaKey) {
      // Ctrl+wheel zooms about the pointer.
      const p = this.toSlide(e);
      const z = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, this.zoom * Math.exp(-e.deltaY / 400)));
      const r = this.els.root.getBoundingClientRect();
      const c = clampScroll(p.x - (e.clientX - r.left) / z, p.y - (e.clientY - r.top) / z, z, this.slide, {
        w: this.cssW,
        h: this.cssH,
      });
      this.zoom = z;
      this.sx = c.x;
      this.sy = c.y;
      if (this.model.state.fit) this.model.set({ fit: false });
      void this.model.run('suite.view.zoom', { zoom: Math.round(z * 100) / 100 });
    } else {
      const fits =
        this.slide.w * this.zoom + 2 * FIT_MARGIN <= this.cssW &&
        this.slide.h * this.zoom + 2 * FIT_MARGIN <= this.cssH;
      if (fits && !e.shiftKey) {
        // The whole slide is in view: the wheel goes through the slides, as in PowerPoint.
        if (Math.abs(e.deltaY) >= 4)
          void this.model.core(e.deltaY > 0 ? 'deck.slides.next_slide' : 'deck.slides.previous_slide');
        return;
      }
      const dx = e.shiftKey ? e.deltaY : e.deltaX;
      const dy = e.shiftKey ? 0 : e.deltaY;
      const c = clampScroll(this.sx + dx / this.zoom, this.sy + dy / this.zoom, this.zoom, this.slide, {
        w: this.cssW,
        h: this.cssH,
      });
      this.sx = c.x;
      this.sy = c.y;
    }
    this.placeProxy();
    this.paint();
    this.requestFrame();
  }

  private expect(mode: StageMode): void {
    this.expected = { mode, after: this.model.sentCount };
  }

  private mode(): StageMode {
    const s = this.state;
    if (this.expected) return this.expected.mode;
    if (s?.editing) return 'text';
    return s && s.selection.length > 0 ? 'object' : 'slide';
  }

  private onKeyDown(e: KeyboardEvent): void {
    // A key between presses starts a new click count.
    this.lastPress = null;
    if (e.isComposing || this.composing) return;
    // A painter armed (Format or Animation Painter): Tab picks the object, Enter paints it, as a
    // click on it does.
    if (
      e.key === 'Enter' &&
      !e.ctrlKey &&
      !e.metaKey &&
      !e.altKey &&
      this.mode() === 'object' &&
      (this.model.state.painter || this.model.state.animPainter)
    ) {
      e.preventDefault();
      e.stopPropagation();
      const anim = !this.model.state.painter;
      this.model.set({ painter: false, animPainter: false });
      void this.model.core(anim ? 'deck.animations.paste' : 'deck.clipboard.paste_format').then(() => {
        this.model.refresh();
        this.requestFrame();
      });
      return;
    }
    // A shape armed for drawing (Insert ▸ Shapes, Text Box): Enter places it in the middle of the
    // slide at its default size, as a click does where it lands.
    const armed = this.model.state.deck?.armed ?? null;
    if (armed && e.key === 'Enter' && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      e.stopPropagation();
      const d = this.model.state.deck;
      const [w, h] = armed === 'textBox' ? [240, 40] : [96, 96];
      const rect = {
        x: Math.max(0, ((d?.slide_width ?? 960) - w) / 2),
        y: Math.max(0, ((d?.slide_height ?? 540) - h) / 2),
        width: w,
        height: h,
      };
      void this.model
        .core('deck.select.escape')
        .then(() =>
          armed === 'textBox'
            ? this.model.core('deck.insert.text_box', { rect })
            : this.model.core('deck.insert.shape', { preset: armed, rect }),
        );
      this.requestFrame();
      return;
    }
    const action = deckKeyAction(e, this.mode());
    if (!action) return;
    if (action.kind === 'native') {
      // The character goes into the proxy and to the core from the input event.
      e.stopPropagation();
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    const mode = this.mode();
    void this.model.core(action.id, action.args);
    // Esc ends the edit (the shape stays selected) or clears the selection: keys pressed before
    // the core's next state go where it will be.
    if (action.id === 'deck.select.escape')
      this.expect(mode === 'text' && (this.state?.selection.length ?? 0) > 0 ? 'object' : 'slide');
    this.caretOn = true;
    this.requestFrame();
  }

  private onInput(): void {
    if (this.composing) return;
    const proxy = this.els.proxy;
    const text = proxy.value;
    if (!text) return;
    proxy.value = '';
    void this.model.core('deck.text.insert', { text });
    // Typing edits text (on a selected shape it starts editing its text): the keys that follow
    // are the text's until the core's next state says otherwise.
    this.expect('text');
    this.caretOn = true;
    this.requestFrame();
  }

  private onPaste(e: ClipboardEvent): void {
    e.preventDefault();
    const d = e.clipboardData;
    if (!d) {
      void this.model.run('suite.edit.paste', { data: null });
      return;
    }
    const text = d.getData('text/plain');
    const html = d.getData('text/html');
    const kiln = d.getData('application/x-kiln-fragment');
    void this.model.run('suite.edit.paste', { data: { text, html: html || null, kiln: kiln || null } });
  }
}

/** Whether a key would type into the proxy (exported for the editor's key routing). */
export { typesText };

/** The status bar's view when it is one of Deck's, else Normal. */
function deckView(view: ViewMode | undefined): ViewMode {
  return view === 'SlideSorter' || view === 'NotesPage' || view === 'ReadingView' ? view : 'SlideNormal';
}
