/**
 * The surface controller: everything the `<Surface>` component does outside React's render cycle
 * (frames, compositing, scrolling, zoom, pointer and keyboard input, the IME proxy).
 *
 * Scrolling is native: a transparent scroller above the canvases holds a spacer the size of the
 * document at the current zoom, so wheel, trackpad, touch panning and scrollbars behave as the
 * platform does. On every scroll the cached tiles are composited at the new offset in the next
 * animation frame, and a frame for the new viewport is requested from the core (at most one in
 * flight); the core prefetches one viewport around the visible one, so scrolling paints from
 * cached tiles and never waits on IPC.
 */

import type {
  A11yTree,
  Command,
  DocId,
  DrawOp,
  KilnClient,
  Event as KilnEvent,
  Rect,
  Selection,
  TextPos,
  Viewport,
} from '@kiln/protocol';
import {
  autoscrollSpeed,
  charCount,
  commandForInput,
  commandForKey,
  isApplePlatform,
  unitForClicks,
} from './input';
import { type Ctx2D, Painter, type PainterCanvas } from './painter';
import { paletteFor } from './palette';
import { SceneCache } from './scene-cache';

/** The DOM the controller drives (created by `<Surface>`). */
export interface SurfaceElements {
  root: HTMLElement;
  scroller: HTMLElement;
  spacer: HTMLElement;
  content: HTMLCanvasElement;
  overlay: HTMLCanvasElement;
  proxy: HTMLElement;
}

/** Options of a surface. */
export interface SurfaceOptions {
  /** Colour theme; `system` follows `prefers-color-scheme`. */
  theme: 'light' | 'dark' | 'system';
  /** Blink the caret (tests and screenshots turn it off). */
  blink: boolean;
  /** Receives the accessibility tree of the visible pages. */
  onA11y?: (tree: A11yTree) => void;
  /** Receives errors the surface could not handle (bridge failures). */
  onError?: (error: unknown) => void;
  /** Receives the zoom factor whenever it changes (pinch, Ctrl+wheel, fit to width). */
  onZoom?: (zoom: number) => void;
  /**
   * Ctrl+= / Ctrl+- / Ctrl+0 zoom the surface (default). Editors whose shortcut table gives these
   * keys other meanings (Word: subscript, superscript) turn this off and zoom from their chrome.
   */
  zoomKeys?: boolean;
  /** Receives the scroll position, zoom and size after a frame that changed them (rulers). */
  onViewChange?: (view: {
    scrollX: number;
    scrollY: number;
    zoom: number;
    width: number;
    height: number;
  }) => void;
}

/** Counters for diagnostics and tests. */
export interface SurfaceStats {
  frames: number;
  composites: number;
  tiles: number;
  tilesRendered: number;
  revision: number;
  zoom: number;
  scrollX: number;
  scrollY: number;
}

const ZOOM_MIN = 0.1;
const ZOOM_MAX = 5;
const BLINK_MS = 530;
/** Time per animation frame spent rendering prefetched tiles (of a 16.7 ms frame). */
const PREFETCH_BUDGET_MS = 3;
/** Touch targets of the selection handles (44 × 44 dp). */
const HANDLE_TARGET = 44;

function rgbFor(prefersDark: boolean, theme: SurfaceOptions['theme']): 'light' | 'dark' {
  if (theme === 'system') return prefersDark ? 'dark' : 'light';
  return theme;
}

function sameViewport(a: Viewport | null, b: Viewport): boolean {
  return (
    a !== null &&
    a.width_px === b.width_px &&
    a.height_px === b.height_px &&
    a.scroll.x === b.scroll.x &&
    a.scroll.y === b.scroll.y &&
    a.zoom === b.zoom &&
    a.dpr === b.dpr &&
    a.mode === b.mode
  );
}

/** Drives one document surface. */
export class SurfaceController {
  readonly cache = new SceneCache<PainterCanvas>();
  readonly painter: Painter;
  private options: SurfaceOptions;
  private zoom = 1;
  private dpr = 1;
  /** The view the core reports (Print Layout, Web Layout, Read Mode, Draft — WP-64). */
  private mode: Viewport['mode'] = 'PrintLayout';
  private cssW = 0;
  private cssH = 0;
  /** Document x at the spacer's left edge (negative when the page is centred). */
  private originX = 0;
  private scrollX = 0;
  private scrollY = 0;
  private sentViewport: Viewport | null = null;
  /** The core still believes an earlier painter's cache is ours. */
  private needsRepaint = true;
  private frameWanted = false;
  private inFlight = false;
  private zoomTimer: ReturnType<typeof setTimeout> | null = null;
  private raf = 0;
  private blinkTimer: ReturnType<typeof setInterval> | null = null;
  private caretOn = true;
  private focused = false;
  private touch = false;
  private composing = false;
  private selection: Selection | null = null;
  private selectionKey = '';
  private revision = -1;
  private selectedText = '';
  private revealCaret = false;
  private queue: Promise<unknown> = Promise.resolve();
  /** A command ran since the last frame: its frame is composited as soon as it arrives. */
  private typed = false;
  /** Commands sent with their frame requested right behind them, whose change events are due. */
  private pipelined = 0;
  private drag: { unit: 'Char' | 'Word' | 'Paragraph'; x: number; y: number; pointer: number } | null = null;
  private selectBusy = false;
  private selectNext: Command | null = null;
  private autoscroll = 0;
  private a11yTimer: ReturnType<typeof setTimeout> | null = null;
  private pinch: { dist: number; zoom: number } | null = null;
  private touchStart: {
    x: number;
    y: number;
    t: number;
    id: number;
    long: ReturnType<typeof setTimeout> | null;
  } | null = null;
  private handles: HTMLElement[] = [];
  private disposed = false;
  private readonly cleanups: Array<() => void> = [];
  private stats: SurfaceStats = {
    frames: 0,
    composites: 0,
    tiles: 0,
    tilesRendered: 0,
    revision: 0,
    zoom: 1,
    scrollX: 0,
    scrollY: 0,
  };
  private readonly darkQuery: MediaQueryList | null;

  constructor(
    private readonly client: KilnClient,
    private readonly doc: DocId,
    private readonly els: SurfaceElements,
    options: Partial<SurfaceOptions> = {},
  ) {
    this.options = { theme: 'system', blink: true, ...options };
    this.darkQuery = typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: dark)') : null;
    this.painter = new Painter(paletteFor(rgbFor(this.darkQuery?.matches ?? false, this.options.theme)));
  }

  /** Attaches listeners, sizes the canvases and requests the first frame. */
  start(): void {
    const { scroller, proxy } = this.els;
    const on = <K extends keyof HTMLElementEventMap>(
      el: HTMLElement | Window,
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
    on(scroller, 'touchstart', (e) => this.onTouchStart(e), { passive: true });
    on(scroller, 'touchmove', (e) => this.onTouchMove(e), { passive: false });
    on(scroller, 'touchend', () => {
      this.pinch = null;
    });
    on(proxy, 'beforeinput', (e) => this.onBeforeInput(e as InputEvent));
    on(proxy, 'input', () => this.onInput());
    on(proxy, 'keydown', (e) => this.onKeyDown(e));
    on(proxy, 'compositionstart', () => {
      this.composing = true;
    });
    on(proxy, 'compositionupdate', (e) => this.onCompositionUpdate(e));
    on(proxy, 'compositionend', (e) => this.onCompositionEnd(e));
    on(proxy, 'paste', (e) => this.onPaste(e));
    on(proxy, 'copy', (e) => this.onCopy(e, false));
    on(proxy, 'cut', (e) => this.onCopy(e, true));
    on(proxy, 'focus', () => this.setFocused(true));
    on(proxy, 'blur', () => this.setFocused(false));
    if (this.darkQuery) {
      const onTheme = () => this.applyTheme();
      this.darkQuery.addEventListener('change', onTheme);
      this.cleanups.push(() => this.darkQuery?.removeEventListener('change', onTheme));
    }
    if (typeof ResizeObserver !== 'undefined') {
      const ro = new ResizeObserver(() => this.resize());
      ro.observe(scroller);
      this.cleanups.push(() => ro.disconnect());
    }
    this.watchDpr();
    const unsubscribe = this.client.subscribe((e) => this.onEvent(e));
    this.cleanups.push(unsubscribe);
    this.resize();
    void this.client.query(this.doc, 'Selection').then((r) => {
      if (typeof r === 'object' && 'Selection' in r) this.setSelection(r.Selection);
    }, this.report);
    this.requestFrame();
    this.scheduleA11y(0);
  }

  /** Detaches everything. */
  dispose(): void {
    this.disposed = true;
    for (const c of this.cleanups.splice(0)) c();
    if (this.raf) cancelAnimationFrame(this.raf);
    if (this.prefetchRaf) cancelAnimationFrame(this.prefetchRaf);
    if (this.blinkTimer) clearInterval(this.blinkTimer);
    if (this.zoomTimer) clearTimeout(this.zoomTimer);
    if (this.a11yTimer) clearTimeout(this.a11yTimer);
    if (this.autoscroll) cancelAnimationFrame(this.autoscroll);
    for (const h of this.handles) h.remove();
  }

  /** Diagnostics. */
  getStats(): SurfaceStats {
    return { ...this.stats, tiles: this.cache.tiles.size, tilesRendered: this.painter.tilesRendered };
  }

  /** Changes the theme. */
  setTheme(theme: SurfaceOptions['theme']): void {
    this.options.theme = theme;
    this.applyTheme();
  }

  /** Turns caret blinking on or off. */
  setBlink(blink: boolean): void {
    this.options.blink = blink;
    this.restartBlink();
  }

  /** The current zoom factor. */
  getZoom(): number {
    return this.zoom;
  }

  /** Sets the zoom (10 %–500 %) around the surface's centre. */
  setZoom(zoom: number): void {
    this.zoomAt(zoom, this.cssW / 2, this.cssH / 2);
  }

  /** Focuses the input proxy (without scrolling). */
  focus(): void {
    this.els.proxy.focus({ preventScroll: true });
  }

  /** Scrolls to a document y. */
  scrollToY(y: number): void {
    this.els.scroller.scrollTop = y * this.zoom;
  }

  // ----- core communication ---------------------------------------------------------------

  private readonly report = (error: unknown): void => {
    if (this.disposed) return;
    if (this.options.onError) this.options.onError(error);
    else console.error('Kiln surface:', error);
  };

  /**
   * Runs a command. It is sent at once: the client delivers commands in the order they are
   * called (`KilnClient.execute`), so typing stays in order with the shortcuts and ribbon
   * commands that other parts of the window send between two keys.
   */
  exec(command: Command, reveal = true): Promise<unknown> {
    if (reveal) this.revealCaret = true;
    this.typed = true;
    // The caret shows steadily from the frame that answers the command (`pump`); drawing it now
    // would ask the browser for a frame, and Chrome then holds the core's answer until after it.
    this.caretOn = true;
    const run = this.client.execute(this.doc, command);
    // The frame is requested right behind the command: the core answers both in one turn, in
    // order, instead of a second round trip once the command's answer arrives (budget B5). The
    // change event that follows then needs no frame of its own.
    this.pipelined += 1;
    this.requestFrame();
    const settled = run.then(
      (r) => {
        // Only an applied command is followed by a change event (a pending one changes the
        // document later, after the frame that was requested with it).
        if ((r as { outcome?: string } | undefined)?.outcome !== 'Applied')
          this.pipelined = Math.max(0, this.pipelined - 1);
      },
      (e: unknown) => {
        this.pipelined = Math.max(0, this.pipelined - 1);
        // Unavailable commands (nothing to undo, read-only) are expected; others are reported.
        const kind = (e as { kind?: string }).kind;
        if (kind !== 'Unavailable') this.report(e);
      },
    );
    this.queue = settled;
    return this.queue;
  }

  private onEvent(e: KilnEvent): void {
    if (this.disposed || typeof e !== 'object' || !('Changed' in e)) return;
    const { summary, selection, status } = e.Changed;
    if (summary.doc !== this.doc) return;
    this.followStatus(status);
    const revisionChanged = summary.revision !== this.revision;
    this.revision = summary.revision;
    const selectionChanged = this.setSelection(selection);
    // A command this surface sent asked for its frame already (`exec`).
    const covered = this.pipelined > 0;
    if (covered) this.pipelined -= 1;
    if (revisionChanged || selectionChanged) {
      if (!covered) this.requestFrame();
      this.scheduleA11y(120);
      if (this.revealCaret) {
        this.revealCaret = false;
        this.scrollCaretIntoView();
      }
    }
  }

  /**
   * Follows a view or zoom the core changed (View ▸ Read Mode, the zoom slider's commands):
   * the core's viewport is authoritative for the mode, and for a zoom it reports that differs
   * from the one this surface last sent (WP-64).
   */
  private followStatus(status: { view: Viewport['mode']; zoom: number } | undefined): void {
    if (!status) return;
    if (status.view && status.view !== this.mode) {
      this.mode = status.view;
      this.requestFrame();
    }
    const sent = this.sentViewport?.zoom;
    if (
      sent !== undefined &&
      Number.isFinite(status.zoom) &&
      status.zoom > 0 &&
      Math.abs(status.zoom - sent) > 1e-3 &&
      Math.abs(status.zoom - this.zoom) > 1e-3
    ) {
      this.setZoom(status.zoom);
    }
  }

  /** Scrolls the caret into view after the next change (commands run outside the surface). */
  reveal(): void {
    this.revealCaret = true;
    this.scrollCaretIntoView();
  }

  /** The client (viewport) rectangle of a rectangle in document coordinates. */
  clientRectOf(r: Rect): { left: number; top: number; width: number; height: number } {
    const base = this.els.scroller.getBoundingClientRect();
    return {
      left: base.left + (r.x - this.scrollX) * this.zoom,
      top: base.top + (r.y - this.scrollY) * this.zoom,
      width: r.width * this.zoom,
      height: r.height * this.zoom,
    };
  }

  /** The document point under client coordinates (rulers, context menus). */
  documentPoint(clientX: number, clientY: number): { x: number; y: number } {
    const p = this.docPoint(clientX, clientY);
    return { x: p.x, y: p.y };
  }

  /** Scroll position and zoom, for rulers drawn beside the surface. */
  viewState(): { scrollX: number; scrollY: number; zoom: number; width: number; height: number } {
    return {
      scrollX: this.scrollX,
      scrollY: this.scrollY,
      zoom: this.zoom,
      width: this.cssW,
      height: this.cssH,
    };
  }

  private setSelection(selection: Selection): boolean {
    const key = JSON.stringify(selection);
    if (key === this.selectionKey) return false;
    this.selectionKey = key;
    this.selection = selection;
    this.positionProxy();
    const kind = selection.kind;
    const nonEmpty =
      typeof kind === 'object' &&
      'Text' in kind &&
      JSON.stringify(kind.Text.anchor) !== JSON.stringify(kind.Text.head);
    if (nonEmpty) {
      void this.client.query(this.doc, 'SelectedText').then((r) => {
        if (typeof r === 'object' && 'Text' in r) this.selectedText = r.Text;
      }, this.report);
    } else {
      this.selectedText = '';
    }
    return true;
  }

  /** Asks for a frame; at most one is in flight, later requests coalesce. */
  requestFrame(): void {
    this.frameWanted = true;
    if (!this.inFlight) void this.pump();
  }

  private async pump(): Promise<void> {
    this.inFlight = true;
    try {
      while (this.frameWanted && !this.disposed) {
        this.frameWanted = false;
        const vp = this.viewport();
        if (vp.width_px <= 0 || vp.height_px <= 0) break;
        if (!sameViewport(this.sentViewport, vp)) {
          await this.client.setViewport(this.doc, vp);
          this.sentViewport = vp;
          if (this.disposed) return;
        }
        if (this.needsRepaint) {
          // A new painter has an empty cache: the core must resend everything. Sent right before
          // the frame so no other request of this surface can come in between.
          this.needsRepaint = false;
          await this.client.execute(this.doc, { Suite: 'Repaint' });
          if (this.disposed) return;
        }
        const delta = await this.client.frame(this.doc);
        if (this.disposed) return;
        const before = this.cache.extent;
        this.cache.apply(delta);
        this.stats.frames += 1;
        this.stats.revision = delta.revision;
        if (delta.overlay) {
          this.caretOn = true;
          this.restartBlink();
          this.positionProxy();
        }
        if (before.w !== this.cache.extent.w || before.h !== this.cache.extent.h) {
          if (before.w === 0 && this.cache.extent.w * this.zoom > this.cssW && this.cssW > 0) {
            // A page wider than the surface (phones): start zoomed to fit its width.
            this.zoom = Math.max(ZOOM_MIN, Math.floor((this.cssW / this.cache.extent.w) * 100) / 100);
            this.stats.zoom = this.zoom;
            this.options.onZoom?.(this.zoom);
          }
          this.layoutSpacer();
          this.syncScroll();
          // Centring depends on the extent: the viewport may have moved.
          if (!sameViewport(this.sentViewport, this.viewport())) this.frameWanted = true;
        }
        this.positionHandles();
        if (this.typed && delta.overlay) {
          // The answer to an edit is drawn now rather than at the next animation frame: the
          // browser presents it at the coming vsync either way, and drawing at once keeps a
          // keystroke's latency to the core's work plus this composite (budget B5).
          this.typed = false;
          if (this.raf) cancelAnimationFrame(this.raf);
          this.raf = 0;
          this.composite();
          // Typing latency (B5) is measured to here (e2e/specs/write.spec.ts observes the mark;
          // only the latest is kept, so long sessions do not accumulate entries).
          performance.clearMarks('kiln:input-paint');
          performance.mark('kiln:input-paint');
        } else {
          this.scheduleComposite();
        }
      }
    } catch (e) {
      this.report(e);
    } finally {
      this.inFlight = false;
    }
  }

  private viewport(): Viewport {
    return {
      width_px: this.cssW,
      height_px: this.cssH,
      scroll: { x: Math.fround(this.scrollX), y: Math.fround(this.scrollY) },
      zoom: Math.fround(this.zoom),
      dpr: Math.fround(this.dpr),
      mode: this.mode,
    };
  }

  private scheduleA11y(delay: number): void {
    if (!this.options.onA11y) return;
    if (this.a11yTimer) clearTimeout(this.a11yTimer);
    this.a11yTimer = setTimeout(() => {
      this.a11yTimer = null;
      void this.client.a11y(this.doc).then((tree) => {
        if (!this.disposed) this.options.onA11y?.(tree);
      }, this.report);
    }, delay);
  }

  // ----- geometry, scrolling and zoom ------------------------------------------------------------

  private resize(): void {
    const { scroller, content, overlay } = this.els;
    const w = scroller.clientWidth;
    const h = scroller.clientHeight;
    const dpr = typeof devicePixelRatio === 'number' && devicePixelRatio > 0 ? devicePixelRatio : 1;
    if (w === this.cssW && h === this.cssH && dpr === this.dpr) return;
    this.cssW = w;
    this.cssH = h;
    if (dpr !== this.dpr) {
      this.dpr = dpr;
      this.cache.invalidateCanvases();
    }
    for (const c of [content, overlay]) {
      c.width = Math.max(1, Math.round(w * dpr));
      c.height = Math.max(1, Math.round(h * dpr));
      c.style.width = `${w}px`;
      c.style.height = `${h}px`;
    }
    this.layoutSpacer();
    this.syncScroll();
    this.requestFrame();
    this.scheduleComposite();
  }

  private watchDpr(): void {
    if (typeof matchMedia !== 'function' || typeof devicePixelRatio !== 'number') return;
    const q = matchMedia(`(resolution: ${devicePixelRatio}dppx)`);
    const onChange = () => {
      this.resize();
      this.watchDpr();
    };
    q.addEventListener('change', onChange, { once: true });
    this.cleanups.push(() => q.removeEventListener('change', onChange));
  }

  /** Sizes the spacer to the document at the current zoom and works out the centring. */
  private layoutSpacer(): void {
    const ext = this.cache.extent;
    const docW = ext.w * this.zoom;
    const docH = ext.h * this.zoom;
    const narrow = docW <= this.cssW;
    this.originX = narrow ? -(this.cssW / this.zoom - ext.w) / 2 : 0;
    this.els.spacer.style.width = `${narrow ? this.cssW : docW}px`;
    this.els.spacer.style.height = `${Math.max(docH, this.cssH)}px`;
    this.positionProxy();
    this.positionHandles();
  }

  private syncScroll(): void {
    const { scroller } = this.els;
    this.scrollX = this.originX + scroller.scrollLeft / this.zoom;
    this.scrollY = scroller.scrollTop / this.zoom;
    this.stats.scrollX = this.scrollX;
    this.stats.scrollY = this.scrollY;
  }

  private onScroll(): void {
    this.syncScroll();
    this.scheduleComposite();
    this.requestFrame();
    this.scheduleA11y(250);
  }

  private zoomAt(next: number, anchorX: number, anchorY: number): void {
    const zoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, next));
    if (!Number.isFinite(zoom) || Math.abs(zoom - this.zoom) < 1e-4) return;
    const docX = this.scrollX + anchorX / this.zoom;
    const docY = this.scrollY + anchorY / this.zoom;
    this.zoom = zoom;
    this.stats.zoom = zoom;
    this.options.onZoom?.(zoom);
    this.layoutSpacer();
    const { scroller } = this.els;
    scroller.scrollLeft = Math.max(0, (docX - anchorX / zoom - this.originX) * zoom);
    scroller.scrollTop = Math.max(0, (docY - anchorY / zoom) * zoom);
    this.syncScroll();
    this.scheduleComposite();
    // Re-rasterising at every step of a continuous gesture is wasted work: wait for a pause.
    if (this.zoomTimer) clearTimeout(this.zoomTimer);
    this.zoomTimer = setTimeout(() => {
      this.zoomTimer = null;
      this.requestFrame();
    }, 140);
  }

  private onWheel(e: WheelEvent): void {
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    const rect = this.els.scroller.getBoundingClientRect();
    const dy = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaY;
    this.zoomAt(this.zoom * Math.exp(-dy * 0.0025), e.clientX - rect.left, e.clientY - rect.top);
  }

  private onTouchStart(e: TouchEvent): void {
    if (e.touches.length === 2) {
      const [a, b] = [e.touches[0] as Touch, e.touches[1] as Touch];
      this.pinch = { dist: Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY), zoom: this.zoom };
    }
  }

  private onTouchMove(e: TouchEvent): void {
    if (!this.pinch || e.touches.length !== 2) return;
    e.preventDefault();
    const [a, b] = [e.touches[0] as Touch, e.touches[1] as Touch];
    const dist = Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
    if (this.pinch.dist <= 0) return;
    const rect = this.els.scroller.getBoundingClientRect();
    this.zoomAt(
      (this.pinch.zoom * dist) / this.pinch.dist,
      (a.clientX + b.clientX) / 2 - rect.left,
      (a.clientY + b.clientY) / 2 - rect.top,
    );
  }

  /** Scrolls so the caret (selection head) is visible with a small margin. */
  private scrollCaretIntoView(): void {
    const caret = this.selection?.caret?.rect;
    if (!caret) return;
    const { scroller } = this.els;
    const margin = 24 / this.zoom;
    const top = this.scrollY;
    const bottom = top + this.cssH / this.zoom;
    if (caret.y - margin < top) scroller.scrollTop = Math.max(0, (caret.y - margin) * this.zoom);
    else if (caret.y + caret.height + margin > bottom) {
      scroller.scrollTop = (caret.y + caret.height + margin) * this.zoom - this.cssH;
    }
    const left = this.scrollX;
    const right = left + this.cssW / this.zoom;
    if (caret.x < left || caret.x > right) {
      scroller.scrollLeft = Math.max(0, (caret.x - this.originX) * this.zoom - this.cssW / 2);
    }
    this.syncScroll();
  }

  // ----- compositing ------------------------------------------------------------------------

  private scheduleComposite(): void {
    if (this.raf || this.disposed) return;
    this.raf = requestAnimationFrame(() => {
      this.raf = 0;
      this.composite();
    });
  }

  /**
   * One animation frame: composite the cached tiles and the overlay, then spend spare time on
   * prefetched tiles. The whole of the surface's work in the frame is recorded as the
   * `kiln:frame` performance measure (budget B6: ≤ 16.7 ms at p95; the painter itself targets
   * ≤ 4 ms, PLAN.md §3.8).
   */
  private composite(): void {
    const begin = performance.now();
    const ctx = this.els.content.getContext('2d') as Ctx2D | null;
    const octx = this.els.overlay.getContext('2d') as Ctx2D | null;
    if (!ctx || !octx || this.cssW <= 0) return;
    const view = {
      width: this.els.content.width,
      height: this.els.content.height,
      scrollX: this.scrollX,
      scrollY: this.scrollY,
      viewScale: this.zoom * this.dpr,
    };
    this.painter.composite(ctx, this.cache, view);
    this.paintOverlay();
    if (this.options.onViewChange) {
      const key = `${this.scrollX}|${this.scrollY}|${this.zoom}|${this.cssW}|${this.cssH}`;
      if (key !== this.viewKey) {
        this.viewKey = key;
        this.options.onViewChange(this.viewState());
      }
    }
    this.stats.composites += 1;
    // Spare time: render prefetched tiles in small steps, so scrolling onto them costs nothing
    // and no frame is ever spent on a whole tile.
    this.schedulePrefetch(begin);
    try {
      performance.measure('kiln:frame', { start: begin, end: performance.now() });
    } catch {
      // Engines without measure options: the stats still count frames.
    }
  }

  private prefetchRaf = 0;
  private viewKey = '';

  private schedulePrefetch(frameStart: number): void {
    const visible = {
      x: this.scrollX,
      y: this.scrollY,
      w: this.cssW / this.zoom,
      h: this.cssH / this.zoom,
    };
    const more = this.painter.prefetch(this.cache, visible, frameStart + PREFETCH_BUDGET_MS);
    if (more && !this.prefetchRaf && !this.disposed) {
      this.prefetchRaf = requestAnimationFrame((t) => {
        this.prefetchRaf = 0;
        this.schedulePrefetch(Math.max(t, performance.now()));
      });
    }
  }

  private paintOverlay(): void {
    const octx = this.els.overlay.getContext('2d') as Ctx2D | null;
    if (!octx) return;
    this.painter.paintOverlay(
      octx,
      this.cache,
      {
        width: this.els.overlay.width,
        height: this.els.overlay.height,
        scrollX: this.scrollX,
        scrollY: this.scrollY,
        viewScale: this.zoom * this.dpr,
      },
      { caretVisible: this.focused && this.caretOn, touch: this.touch },
    );
  }

  private restartBlink(): void {
    if (this.blinkTimer) {
      clearInterval(this.blinkTimer);
      this.blinkTimer = null;
    }
    this.caretOn = true;
    if (this.options.blink && this.focused && !this.disposed) {
      this.blinkTimer = setInterval(() => {
        this.caretOn = !this.caretOn;
        this.paintOverlay();
      }, BLINK_MS);
    }
    this.paintOverlay();
  }

  private setFocused(focused: boolean): void {
    this.focused = focused;
    this.restartBlink();
  }

  private applyTheme(): void {
    const palette = paletteFor(rgbFor(this.darkQuery?.matches ?? false, this.options.theme));
    if (palette.name === this.painter.palette.name) return;
    this.painter.setPalette(palette);
    this.cache.invalidateCanvases();
    this.scheduleComposite();
  }

  // ----- the input proxy --------------------------------------------------------------------

  /** Moves the proxy to the caret so IME candidate windows and soft keyboards appear there. */
  private positionProxy(): void {
    // The painted caret is the freshest geometry (frames follow every change); the selection's
    // caret covers non-empty selections, which paint no caret.
    let caret: Rect | undefined = this.selection?.caret?.rect;
    for (const op of this.cache.overlay) {
      if (typeof op === 'object' && 'Caret' in op) {
        const r = op.Caret.rect;
        caret = { x: r.x, y: r.y, width: r.w, height: r.h };
      }
    }
    const p = this.els.proxy;
    const x = caret ? (caret.x - this.originX) * this.zoom : 0;
    const y = caret ? caret.y * this.zoom : 0;
    const h = caret ? Math.max(1, caret.height * this.zoom) : 16;
    p.style.transform = `translate(${Math.round(x)}px, ${Math.round(y)}px)`;
    p.style.height = `${Math.round(h)}px`;
    p.style.fontSize = `${Math.max(1, Math.round(h * 0.8))}px`;
  }

  private clearProxy(): void {
    if (!this.composing && this.els.proxy.textContent !== '') this.els.proxy.textContent = '';
  }

  private onBeforeInput(e: InputEvent): void {
    if (e.isComposing || this.composing || e.inputType === 'insertCompositionText') return;
    e.preventDefault();
    let data = e.data;
    if (data === null && e.dataTransfer) data = e.dataTransfer.getData('text/plain');
    const cmd = commandForInput(e.inputType, data);
    if (cmd) void this.exec(cmd);
  }

  private onInput(): void {
    this.clearProxy();
  }

  private onKeyDown(e: KeyboardEvent): void {
    if (e.isComposing || e.keyCode === 229) return;
    const primary = isApplePlatform() ? e.metaKey : e.ctrlKey;
    if (
      this.options.zoomKeys !== false &&
      primary &&
      !e.altKey &&
      (e.key === '=' || e.key === '+' || e.key === '-' || e.key === '0')
    ) {
      e.preventDefault();
      if (e.key === '0') this.setZoom(1);
      else this.setZoom(Math.round((this.zoom + (e.key === '-' ? -0.1 : 0.1)) * 10) / 10);
      return;
    }
    const cmd = commandForKey(e);
    if (!cmd) return;
    e.preventDefault();
    if (this.touch) {
      this.touch = false;
      this.positionHandles();
    }
    void this.exec(cmd);
  }

  private onCompositionUpdate(e: CompositionEvent): void {
    this.composing = true;
    const text = e.data ?? '';
    void this.exec({ Write: { SetComposition: { text, caret: charCount(text) } } });
  }

  private onCompositionEnd(e: CompositionEvent): void {
    this.composing = false;
    void this.exec({ Write: { CommitComposition: { text: e.data ?? '' } } });
    // The browser inserted the committed text into the proxy; the document has it now.
    setTimeout(() => this.clearProxy(), 0);
  }

  private onPaste(e: ClipboardEvent): void {
    e.preventDefault();
    const text = e.clipboardData?.getData('text/plain') ?? '';
    if (text) void this.exec({ Write: { InsertText: { text } } });
  }

  private onCopy(e: ClipboardEvent, cut: boolean): void {
    e.preventDefault();
    if (!this.selectedText) return;
    e.clipboardData?.setData('text/plain', this.selectedText);
    if (cut) void this.exec({ Write: 'DeleteBackward' });
  }

  // ----- pointer selection -----------------------------------------------------------------

  /** Document point under client coordinates. */
  private docPoint(clientX: number, clientY: number): { x: number; y: number; inside: boolean } {
    const rect = this.els.scroller.getBoundingClientRect();
    const vx = clientX - rect.left;
    const vy = clientY - rect.top;
    return {
      x: this.scrollX + vx / this.zoom,
      y: this.scrollY + vy / this.zoom,
      inside: vx >= 0 && vy >= 0 && vx < this.els.scroller.clientWidth && vy < this.els.scroller.clientHeight,
    };
  }

  /** Sends a selection command; while one is in flight only the latest pointer position waits. */
  private select(cmd: Command): void {
    if (this.selectBusy) {
      this.selectNext = cmd;
      return;
    }
    this.selectBusy = true;
    void this.exec(cmd, false).finally(() => {
      this.selectBusy = false;
      const next = this.selectNext;
      this.selectNext = null;
      if (next) this.select(next);
    });
  }

  private onPointerDown(e: PointerEvent): void {
    if (e.button !== 0) return;
    const p = this.docPoint(e.clientX, e.clientY);
    if (!p.inside) return; // the scrollbars
    if (e.pointerType === 'touch') {
      this.touch = true;
      const long = setTimeout(() => {
        // Long press: select the word, as on every phone.
        if (this.touchStart) {
          this.select({ Write: { SelectAt: { point: { x: p.x, y: p.y }, extend: false, unit: 'Word' } } });
          this.touchStart = null;
          this.focus();
        }
      }, 500);
      this.touchStart = { x: e.clientX, y: e.clientY, t: performance.now(), id: e.pointerId, long };
      this.positionHandles();
      this.paintOverlay();
      return;
    }
    if (this.touch) {
      this.touch = false;
      this.positionHandles();
    }
    e.preventDefault();
    this.focus();
    const unit = unitForClicks(this.countClick(e));
    this.drag = { unit, x: p.x, y: p.y, pointer: e.pointerId };
    try {
      this.els.scroller.setPointerCapture(e.pointerId);
    } catch {
      // Synthetic events without an active pointer.
    }
    this.select({ Write: { SelectAt: { point: { x: p.x, y: p.y }, extend: e.shiftKey, unit } } });
  }

  private clicks = { n: 0, t: 0, x: 0, y: 0 };

  /**
   * The click count of a press (pointer events report no `detail`): presses within the
   * platform's double-click time (500 ms) and 4 px of the previous one count up, to 3.
   */
  private countClick(e: PointerEvent): number {
    const now = performance.now();
    const c = this.clicks;
    const near = Math.abs(e.clientX - c.x) <= 4 && Math.abs(e.clientY - c.y) <= 4;
    c.n = near && now - c.t < 500 ? Math.min(3, c.n + 1) : 1;
    c.t = now;
    c.x = e.clientX;
    c.y = e.clientY;
    return Math.max(c.n, e.detail || 0);
  }

  private onPointerMove(e: PointerEvent): void {
    if (this.touchStart && e.pointerId === this.touchStart.id) {
      if (Math.hypot(e.clientX - this.touchStart.x, e.clientY - this.touchStart.y) > 8) {
        if (this.touchStart.long) clearTimeout(this.touchStart.long);
        this.touchStart = null; // a pan, handled natively
      }
      return;
    }
    if (!this.drag || e.pointerId !== this.drag.pointer) return;
    const p = this.docPoint(e.clientX, e.clientY);
    this.drag.x = p.x;
    this.drag.y = p.y;
    this.select({ Write: { SelectAt: { point: { x: p.x, y: p.y }, extend: true, unit: this.drag.unit } } });
    const rect = this.els.scroller.getBoundingClientRect();
    const past = Math.max(rect.top - e.clientY, e.clientY - (rect.top + this.els.scroller.clientHeight));
    if (past > -4 && !this.autoscroll) this.runAutoscroll(e.clientY);
    else if (this.autoscroll) this.autoscrollY = e.clientY;
  }

  private autoscrollY = 0;

  private runAutoscroll(clientY: number): void {
    this.autoscrollY = clientY;
    const step = () => {
      if (!this.drag) {
        this.autoscroll = 0;
        return;
      }
      const rect = this.els.scroller.getBoundingClientRect();
      const above = rect.top + 4 - this.autoscrollY;
      const below = this.autoscrollY - (rect.top + this.els.scroller.clientHeight - 4);
      const speed = above > 0 ? -autoscrollSpeed(above) : autoscrollSpeed(below);
      if (speed === 0) {
        this.autoscroll = 0;
        return;
      }
      this.els.scroller.scrollTop += speed * this.zoom;
      this.syncScroll();
      const y = above > 0 ? this.scrollY : this.scrollY + this.cssH / this.zoom;
      this.select({
        Write: { SelectAt: { point: { x: this.drag.x, y }, extend: true, unit: this.drag.unit } },
      });
      this.autoscroll = requestAnimationFrame(step);
    };
    this.autoscroll = requestAnimationFrame(step);
  }

  private onPointerUp(e: PointerEvent): void {
    if (this.touchStart && e.pointerId === this.touchStart.id) {
      const t = this.touchStart;
      this.touchStart = null;
      if (t.long) clearTimeout(t.long);
      if (e.type === 'pointerup' && performance.now() - t.t < 500) {
        const p = this.docPoint(e.clientX, e.clientY);
        this.select({ Write: { SelectAt: { point: { x: p.x, y: p.y }, extend: false, unit: 'Char' } } });
        this.focus();
      }
      return;
    }
    if (!this.drag || e.pointerId !== this.drag.pointer) return;
    this.drag = null;
    if (this.autoscroll) {
      cancelAnimationFrame(this.autoscroll);
      this.autoscroll = 0;
    }
    try {
      this.els.scroller.releasePointerCapture(e.pointerId);
    } catch {
      // Not captured.
    }
  }

  // ----- touch selection handles ------------------------------------------------------------

  /** Places 44 × 44 hit targets over the teardrop handles of the overlay (touch only). */
  private positionHandles(): void {
    const ops: DrawOp[] = this.touch
      ? this.cache.overlay.filter((op) => typeof op === 'object' && 'Handle' in op)
      : [];
    while (this.handles.length > ops.length) this.handles.pop()?.remove();
    ops.forEach((op, i) => {
      if (typeof op !== 'object' || !('Handle' in op)) return;
      let el = this.handles[i];
      if (!el) {
        el = document.createElement('div');
        el.setAttribute('aria-hidden', 'true');
        el.dataset.kilnHandle = '';
        el.style.cssText = `position:absolute;left:0;top:0;width:${HANDLE_TARGET}px;height:${HANDLE_TARGET}px;touch-action:none;`;
        this.attachHandleDrag(el);
        this.els.spacer.appendChild(el);
        this.handles.push(el);
      }
      const h = op.Handle;
      el.dataset.side = h.side;
      const cx = (h.x - this.originX) * this.zoom;
      const cy = (h.y + h.size / 2) * this.zoom;
      el.style.transform = `translate(${Math.round(cx - HANDLE_TARGET / 2)}px, ${Math.round(cy - HANDLE_TARGET / 2)}px)`;
    });
  }

  private attachHandleDrag(el: HTMLElement): void {
    el.addEventListener('pointerdown', (e) => {
      e.preventDefault();
      e.stopPropagation();
      el.setPointerCapture(e.pointerId);
      const kind = this.selection?.kind;
      // Dragging the start handle: make the end the anchor first.
      if (el.dataset.side === 'Start' && typeof kind === 'object' && 'Text' in kind) {
        const { anchor, head } = kind.Text;
        const end: TextPos = this.after(anchor, head) ? anchor : head;
        void this.exec({ Write: { SelectTo: { pos: end, extend: false } } }, false);
      }
      const move = (ev: PointerEvent) => {
        const p = this.docPoint(ev.clientX, ev.clientY);
        const extend = el.dataset.side !== 'Caret';
        this.select({ Write: { SelectAt: { point: { x: p.x, y: p.y }, extend, unit: 'Char' } } });
      };
      const up = () => {
        el.removeEventListener('pointermove', move);
        el.removeEventListener('pointerup', up);
        el.removeEventListener('pointercancel', up);
      };
      el.addEventListener('pointermove', move);
      el.addEventListener('pointerup', up);
      el.addEventListener('pointercancel', up);
    });
  }

  /** Whether `a` comes after `b` in the document (same paragraph: by offset; else by order). */
  private after(a: TextPos, b: TextPos): boolean {
    if (a.block === b.block) return a.offset > b.offset;
    // Paragraph order is the core's; the anchor/head order of the Changed event's rects tells it.
    const rects = this.selection?.rects ?? [];
    const caret = this.selection?.caret?.rect;
    if (rects.length && caret) return caret.y <= (rects[0] as Rect).y + 0.5;
    return false;
  }
}
