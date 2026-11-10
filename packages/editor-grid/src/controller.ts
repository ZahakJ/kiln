/**
 * The grid surface: paints the core's Scene — cell tiles, header
 * strips and the corner, the overlay with the selection and the in-cell editor — into up to four
 * panes (frozen rows and columns stay while the rest scrolls), every pane from the same tile
 * cache. Scrolling is native (a transparent scroller over the canvas holds a spacer the size of
 * the sheet), so wheel, trackpad, touch and scroll bars behave as the platform does; each scroll
 * composites cached tiles at once and asks the core for a frame of the new view.
 *
 * Input: pointer gestures are hit-tested by the core (`Query::Grid(HitTest)`) and become
 * selection, fill-handle, resize and point-mode commands; keys go through `keys.ts`; a hidden
 * textarea is the input proxy for typing and IME (its text is the entry the core renders in the
 * cell). Nothing here lays out or formats anything.
 */
import { t } from '@kiln/i18n';
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
import { errorText, loadOptions } from '@kiln/ui-kit';
import { gridKeyAction, type KeyAction, type KeyLike, typesText } from './keys.ts';
import type { GridEditorModel, GridSurfaceHandle } from './model.ts';

/** How long the keys wait for the cell's text of an edit start before it is given up. */
const EDIT_START_MS = 3000;

/** Ctrl+Z, or Ctrl+Y and Ctrl+Shift+Z (Cmd on a Mac). */
function historyKey(e: KeyboardEvent): 'undo' | 'redo' | null {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return null;
  const k = e.key.toLowerCase();
  if (k === 'z') return e.shiftKey ? 'redo' : 'undo';
  if (k === 'y' && !e.shiftKey) return 'redo';
  return null;
}

/** Tile layers of the Grid scene (`kiln_sheet_layout::scene`). */
/** The views of a sheet (other apps' views reach the status bar before the first frame). */
const GRID_VIEWS: ReadonlySet<string> = new Set(['Normal', 'PageBreakPreview', 'PageLayout']);

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
  /**
   * The sheet x at the pane's left edge. A right-to-left sheet (`GridLayout.rtl`) is shown
   * mirrored (`kiln_sheet_layout::mirror`): its panes, tiles and overlay are in the mirrored
   * sheet, where a sheet x is −x.
   */
  x: number;
  y: number;
  kind: 'cells' | 'colHeader' | 'rowHeader' | 'corner';
}

/** The x of a tile's left edge where the surface draws it (mirrored on a right-to-left sheet). */
export function tileX(rect: { x: number; w: number }, rtl: boolean): number {
  return rtl ? -(rect.x + rect.w) : rect.x;
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
  rtl: false,
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
  // A right-to-left sheet: the mirror image, column A and the row headers at the right.
  if (l.rtl)
    for (const p of out) {
      p.x = -(p.x + p.rect.w / z);
      p.rect = { ...p.rect, x: width - p.rect.x - p.rect.w };
    }
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
  /** `target`: the cell the last drag hit test found, which the fill goes to on release. */
  | { kind: 'fill'; target?: { row: number; col: number } }
  /** A stroke of the border pen from `from` (surface CSS pixels). */
  | { kind: 'pen'; from: { x: number; y: number }; to: { x: number; y: number } }
  /**
   * The selection dragged by its border (Excel's drag and drop): `grab` is the cell it was taken
   * by, `at` the cell under the pointer, Ctrl copies, Shift inserts.
   */
  | {
      kind: 'move';
      grab: { row: number; col: number };
      at: { row: number; col: number };
      copy: boolean;
      insert: boolean;
      moved: boolean;
    }
  | { kind: 'resize'; axis: 'row' | 'col'; index: number; start: number; size: number; at: number }
  /** A chart moved (`handle` null) or resized by one of its eight handles; sheet pixels. */
  | {
      kind: 'drawing';
      index: number;
      handle: number | null;
      from: { x: number; y: number };
      rect: SheetRect;
      now: SheetRect;
    };

/** A rectangle in sheet pixels at 100 %. */
interface SheetRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * The rectangle of a drag of a chart by `dx`, `dy` sheet pixels: the whole chart, or the edges a
 * handle holds (0–7 clockwise from the top-left corner), never smaller than 24 px.
 */
export function dragRect(r: SheetRect, handle: number | null, dx: number, dy: number): SheetRect {
  if (handle === null) return { ...r, x: Math.max(0, r.x + dx), y: Math.max(0, r.y + dy) };
  const MIN = 24;
  let { x, y, w, h } = r;
  const left = handle === 0 || handle === 6 || handle === 7;
  const right = handle === 2 || handle === 3 || handle === 4;
  const top = handle === 0 || handle === 1 || handle === 2;
  const bottom = handle === 4 || handle === 5 || handle === 6;
  if (left) {
    const nx = Math.min(x + dx, x + w - MIN);
    w += x - nx;
    x = nx;
  }
  if (right) w = Math.max(MIN, w + dx);
  if (top) {
    const ny = Math.min(y + dy, y + h - MIN);
    h += y - ny;
    y = ny;
  }
  if (bottom) h = Math.max(MIN, h + dy);
  return { x: Math.max(0, x), y: Math.max(0, y), w, h };
}

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
  /** The edit start under way (a later one, or one given up, ignores the earlier's answer). */
  private editAttempt = 0;
  /**
   * The proxy holds text that came with no key the controller replays (an input method's, or
   * text inserted with no key) and has not been handled yet: a press or an edit start must not
   * clear it, or it is lost.
   */
  private held = false;
  /** An input method's composition is under way in the proxy: its text must not be replaced. */
  private composing = false;
  /** The cell's text arrived during a composition: it joins the composed text when that ends. */
  private deferredEdit: NonNullable<GridState['editor']> | null = null;
  /** The last text sent to the core from the proxy. */
  private sentText = '';
  /**
   * Texts sent to the core and not yet seen back: a state that carries one of them is an echo of
   * typing (older than what the proxy holds now) and must not undo the keys typed since.
   */
  private echoes: string[] = [];
  /**
   * Every text the proxy sent in this entry since the core last changed it on its own: a state
   * that carries one of them after its echo was taken is a repeat of an older state (a pushed
   * frame and a fetch answering the same version) and must not undo the keys typed since.
   */
  private sentTexts = new Set<string>();
  /** Commits and cancels sent and not answered yet: the core's editor state may be stale. */
  private ending = 0;
  /** The model's state request count when the last entry ended (older answers are stale). */
  private endedAsked = -1;
  /** The model's state request count when text was last sent from the proxy. */
  private typedAsked = -1;
  /**
   * The first frame resets the core's record of what the painter holds: another surface of the
   * window (the frame's placeholder before the editor mounted) may have taken earlier frames.
   */
  private fresh = true;
  private drag: Drag | null = null;
  /**
   * The chart sheet shown fitted to the window (Excel's Zoom with Window): its name, the zoom it
   * was fitted at (asked for, `pending` until the core's status has it) and the zoom of the sheet
   * shown before it, given back on leaving. A zoom the user picks there ends the fitting.
   */
  private chartFit: {
    sheet: string;
    zoom: number;
    pending: boolean;
    before: number;
    manual: boolean;
  } | null = null;
  /** The chart's extent (sheet pixels at 100 %) of each chart sheet seen with its chart selected. */
  private readonly chartExtent = new Map<string, { w: number; h: number }>();
  private dragPoint: { x: number; y: number } | null = null;
  /** A finger down on the sheet that has not moved yet (it selects where it lifts). */
  private tap: { id: number; x: number; y: number; t: number } | null = null;
  /** When a finger last touched the sheet (the browser's emulated mouse events follow). */
  private lastTouch = Number.NEGATIVE_INFINITY;
  private readonly compositeListeners = new Set<() => void>();
  private dragBusy = false;
  /** The drag step in flight, if any (its hit test may still name the fill's target). */
  private dragStep: Promise<void> | null = null;
  /** The button is up and the fill waits for its last drag step. */
  private releasing = false;
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

  /** Tells the core the app's theme (charts are coloured for it). */
  private sendTheme(): void {
    void this.model.core('grid.view.theme', { dark: this.options.theme === 'dark' });
  }

  start(): void {
    this.sendTheme();
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
    on(scroller, 'pointerdown', (e) => {
      // a finger pans the sheet; it selects where it lifts without having moved (a tap),
      // unless a chart or picture is selected (then it moves and sizes it, as a mouse does).
      if (e.pointerType === 'touch' && e.button === 0 && !this.model.state.grid?.drawing) {
        this.lastTouch = performance.now();
        this.tap = { id: e.pointerId, x: e.clientX, y: e.clientY, t: performance.now() };
        return;
      }
      // A quick click releases before the press's hit test returns: the release waits for it.
      this.press(e);
    });
    on(scroller, 'pointermove', (e) => {
      if (this.tap && e.pointerId === this.tap.id) {
        if (Math.hypot(e.clientX - this.tap.x, e.clientY - this.tap.y) > 10) this.tap = null;
        return;
      }
      this.onPointerMove(e);
    });
    on(scroller, 'pointerup', (e) => {
      if (this.tap && e.pointerId === this.tap.id) {
        const held = performance.now() - this.tap.t;
        this.tap = null;
        this.lastTouch = performance.now();
        // A long press: its context menu selected the cell (the touch layer opens it).
        if (held >= 500) return;
        // The tap: the press and its release where the finger lifted.
        this.press(e);
        this.release(e);
        return;
      }
      this.release(e);
    });
    on(scroller, 'pointercancel', (e) => {
      if (this.tap && e.pointerId === this.tap.id) {
        this.tap = null;
        return;
      }
      this.release(e);
    });
    // The browser's emulated mouse press after a tap would take the keys from the sheet.
    on(scroller, 'mousedown', (e) => {
      if (performance.now() - this.lastTouch < 1000) e.preventDefault();
    });
    on(scroller, 'dblclick', (e) => {
      // A finger's double tap zooms (the touch layer); a mouse's double-click edits the cell.
      if (performance.now() - this.lastTouch < 1000) return;
      this.inOrder(() => this.onDoubleClick(e));
    });
    on(scroller, 'contextmenu', (e) => this.onContextMenu(e));
    on(proxy, 'keydown', (e) => this.onKeyDown(e));
    on(proxy, 'input', () => this.onInput());
    on(proxy, 'compositionstart', () => {
      this.composing = true;
      this.held = true;
    });
    on(proxy, 'compositionend', () => {
      this.composing = false;
      // The composed text has been sent, unless it waits for its turn or for the cell's text.
      if (!this.pendingEdit && !this.pressing) this.held = false;
      const ed = this.deferredEdit;
      this.deferredEdit = null;
      if (ed && this.pendingEdit) this.joinEditText(ed);
    });
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
    // A key that moved the entry's selection (Shift+Left) tells the core on its release, before
    // the next shortcut (Ctrl+B on the selected characters) can reach the window's keymap.
    on(proxy, 'keyup', () => this.onSelectionChange());
    const ro = new ResizeObserver(() => this.measure());
    ro.observe(this.els.root);
    this.cleanups.push(() => ro.disconnect());
    // The core redraws the sheet with fonts the web fetched for it (Arabic text).
    if (typeof window !== 'undefined') {
      const onFonts = () => this.requestFrame();
      window.addEventListener('kiln:fonts-added', onFonts);
      this.cleanups.push(() => window.removeEventListener('kiln:fonts-added', onFonts));
    }
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
    this.cleanups.push(this.model.onState((s, asked) => this.onState(s, asked)));
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
    const dark = this.options.theme === 'dark';
    this.options = { ...this.options, ...options };
    if ((this.options.theme === 'dark') !== dark) this.sendTheme();
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

  holdInput(done: Promise<unknown>): void {
    this.inOrder(() => done.then(() => undefined));
  }

  size(): { width: number; height: number } {
    return { width: this.cssW, height: this.cssH };
  }

  // ── touch ───────────────────────────────────────────────────────────────────────

  /** The zoom the sheet is shown at. */
  getZoom(): number {
    return this.zoom;
  }

  /** The element that scrolls the sheet (a finger pans it). */
  get scroller(): HTMLElement {
    return this.els.scroller;
  }

  /** The surface's element (the touch layer sits over it). */
  get root(): HTMLElement {
    return this.els.root;
  }

  /** Called after every composite (the touch handles follow the sheet). */
  onComposite(listener: () => void): () => void {
    this.compositeListeners.add(listener);
    return () => this.compositeListeners.delete(listener);
  }

  /**
   * The selection's frame on the surface (CSS px from its top-left): the corners of the overlay's
   * selection outline (the 2.5 px frame the core draws round the selection), each where its pane
   * shows it; `null` while editing or with nothing selected. A corner scrolled out of its pane is
   * `null`.
   */
  selectionFrame(): {
    start: { x: number; y: number } | null;
    end: { x: number; y: number } | null;
  } | null {
    const op = this.cache.overlay.find(
      (o) =>
        typeof o === 'object' &&
        'StrokeRect' in o &&
        o.StrokeRect.width === 2.5 &&
        typeof o.StrokeRect.paint === 'object' &&
        'Role' in o.StrokeRect.paint &&
        o.StrokeRect.paint.Role === 'AppFill',
    );
    if (!op || typeof op !== 'object' || !('StrokeRect' in op)) return null;
    const r = op.StrokeRect.rect;
    const cells = this.panes().filter((p) => p.kind === 'cells');
    const z = this.zoom;
    const at = (x: number, y: number) => {
      for (const p of cells) {
        const w = p.rect.w / z;
        const h = p.rect.h / z;
        if (x >= p.x - 1 && x <= p.x + w + 1 && y >= p.y - 1 && y <= p.y + h + 1)
          return { x: p.rect.x + (x - p.x) * z, y: p.rect.y + (y - p.y) * z };
      }
      return null;
    };
    const x0 = r.x + 0.5;
    const y0 = r.y + 0.5;
    const x1 = r.x + r.w + 0.5;
    const y1 = r.y + r.h + 0.5;
    // A right-to-left sheet is drawn mirrored: its first column's corner is at the right.
    return this.layout.rtl ? { start: at(x1, y0), end: at(x0, y1) } : { start: at(x0, y0), end: at(x1, y1) };
  }

  /** Scrolls to a sheet position (the core's scroll request). */
  reveal(x: number, y: number): void {
    const z = this.zoom;
    const left = Math.max(0, (x - this.layout.frozen_w) * z);
    const top = Math.max(0, (y - this.layout.frozen_h) * z);
    this.sx = x;
    this.sy = y;
    this.suppressScroll = true;
    // A right-to-left scroller starts at the right: its scrollLeft runs from 0 to negative.
    this.els.scroller.scrollLeft = this.layout.rtl ? -left : left;
    this.els.scroller.scrollTop = top;
    this.paint();
    this.requestFrame();
  }

  // ── state ───────────────────────────────────────────────────────────────────────────────

  private onState(s: GridState, asked = Number.POSITIVE_INFINITY): void {
    // A state asked for before the last entry ended describes that entry: its editor is stale
    // (the keys typed since start a new one).
    if (asked <= this.endedAsked && s.editor) s = { ...s, editor: null, mode: 'Ready' };
    // A state asked before the text typed last was sent is older than the entry: keep typing.
    if (asked <= this.typedAsked && this.editing && !s.editor) return;
    const prev = this.state;
    this.state = s;
    const zoom = this.model.state.status?.zoom ?? 1;
    const layoutChanged = JSON.stringify(prev?.layout) !== JSON.stringify(s.layout) || zoom !== this.zoom;
    this.layout = s.layout;
    // The scroll bars of a right-to-left sheet: vertical at the left, horizontal from the right.
    const dir = s.layout.rtl ? 'rtl' : 'ltr';
    if (this.els.scroller.style.direction !== dir) this.els.scroller.style.direction = dir;
    if (zoom !== this.zoom) this.zoom = zoom;
    if (layoutChanged) this.sizeSpacer();
    // The editor: the core's text wins when it differs from what was typed (F4, a suggestion,
    // a pointed reference, the formula bar).
    const ed = s.editor;
    const wasEditing = this.editing;
    const proxy = this.els.proxy;
    // An edit in the Formula Bar is the core's change, not an echo of the proxy's typing.
    if (ed?.formula_bar) this.sentTexts.clear();
    if (this.ending > 0) {
      // An answer from before the entry ended: the entry is over here already (the next keys
      // start a new one), whatever this state still says.
    } else if (ed && !ed.formula_bar && this.pendingEdit && !this.accepting) {
      // F2's text arrived: what was typed meanwhile goes in at the caret, once a composition
      // under way has ended (setting the proxy's text would end it, and its text came again).
      if (this.composing) this.deferredEdit = ed;
      else this.joinEditText(ed);
      this.placeProxy(ed.rect);
    } else if (ed && !ed.formula_bar && this.composing) {
      // A composition is under way: its text reaches the core when it ends.
      this.editing = true;
      this.placeProxy(ed.rect);
    } else if (ed && !ed.formula_bar) {
      // An entry found open as the window opens (a recovered one): the keys are the editor's.
      if (!wasEditing && document.activeElement === document.body) this.focus();
      this.editing = true;
      const echo = wasEditing ? this.echoes.indexOf(ed.text) : -1;
      if (echo >= 0) this.echoes.splice(0, echo + 1);
      else if (wasEditing && ed.text !== proxy.value && this.sentTexts.has(ed.text)) {
        // A repeat of a state already taken: the proxy holds newer typing.
      } else if (ed.text !== proxy.value || !wasEditing) {
        this.sentTexts.clear();
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
        if (proxy.value !== '' && !this.editing && !this.held && !this.pendingEdit) proxy.value = '';
        this.sentText = '';
      }
    }
    this.caretOn = true;
    this.fitChartSheet();
    this.requestFrame();
  }

  /** The cell's text of an edit start arrived: what was typed meanwhile goes in at the caret. */
  private joinEditText(ed: NonNullable<GridState['editor']>): void {
    const proxy = this.els.proxy;
    this.pendingEdit = false;
    this.editing = true;
    this.held = false;
    const typed = proxy.value;
    const text = ed.text.slice(0, ed.start) + typed + ed.text.slice(ed.end);
    const caret = ed.start + typed.length;
    proxy.value = text;
    this.sentText = text;
    this.echoes = typed ? [text] : [];
    // A repeat of the state that brought the cell's text (a pushed frame and the fetch answering
    // the same version) must not take back what was typed meanwhile.
    this.sentTexts = new Set(typed ? [text, ed.text] : []);
    try {
      proxy.setSelectionRange(caret, caret);
    } catch {
      // Detached in tests.
    }
    if (typed)
      void this.model.core('grid.editing.set_text', { text, start: caret, end: caret, formula_bar: false });
  }

  /**
   * Excel fits a chart sheet's chart to the window: the zoom is set so all of it shows, again as
   * the window changes size, until the user zooms there themselves. Leaving the chart sheet gives
   * back the zoom of the sheet before it.
   */
  private fitChartSheet(): void {
    const s = this.state;
    if (!s || this.cssW <= 0 || this.cssH <= 0) return;
    const tab = s.sheets[s.active_sheet];
    const zoom = this.model.state.status?.zoom ?? 1;
    const fit = this.chartFit;
    if (!tab?.chart) {
      if (fit) {
        this.chartFit = null;
        if (zoom !== fit.before) void this.model.run('suite.view.zoom', { zoom: fit.before });
      }
      return;
    }
    if (s.drawing)
      this.chartExtent.set(tab.name, { w: s.drawing.x + s.drawing.width, h: s.drawing.y + s.drawing.height });
    // Excel's chart sheet page (Letter landscape less margins) when the chart is not selected.
    const ext = this.chartExtent.get(tab.name) ?? { w: 912, h: 660 };
    if (fit && fit.sheet === tab.name) {
      if (fit.pending) {
        if (zoom !== fit.zoom) return;
        fit.pending = false;
      } else if (zoom !== fit.zoom) fit.manual = true;
      if (fit.manual) return;
    }
    // Room for the chart, less a margin round it (the headers are hidden on a chart sheet).
    const pad = 12;
    const l = this.layout;
    const w = this.cssW - 2 * pad;
    const h = this.cssH - 2 * pad;
    const want = Math.min(w / (ext.w + l.header_w), h / (ext.h + l.header_h));
    const next = Math.min(4, Math.max(0.1, Math.floor(want * 100) / 100));
    const before = fit && fit.sheet !== tab.name ? fit.before : (fit?.before ?? zoom);
    if (fit && fit.sheet === tab.name && fit.zoom === next) return;
    this.chartFit = { sheet: tab.name, zoom: next, pending: zoom !== next, before, manual: false };
    if (zoom !== next) void this.model.run('suite.view.zoom', { zoom: next });
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
    this.fitChartSheet();
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
    this.sx = this.layout.frozen_w + Math.abs(this.els.scroller.scrollLeft) / z;
    this.sy = this.layout.frozen_h + this.els.scroller.scrollTop / z;
  }

  private viewport(): Viewport {
    return {
      width_px: this.cssW,
      height_px: this.cssH,
      scroll: { x: this.sx, y: this.sy },
      // The core's zoom as last reported: this copy follows it on the next state, and a viewport
      // sent before that must not put an older zoom back.
      zoom: this.model.state.status?.zoom ?? this.zoom,
      dpr: this.dpr,
      // The view the user chose (Page Break Preview, Page Layout) stays as the sheet scrolls.
      mode: GRID_VIEWS.has(this.model.state.status?.view ?? 'Normal')
        ? (this.model.state.status?.view ?? 'Normal')
        : 'Normal',
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
      // Tiles keep their left-to-right sheet rectangles; a mirrored pane shows the sheet
      // between −x − w and −x.
      const region = {
        x: this.layout.rtl ? -(pane.x + r.w / this.zoom) : pane.x,
        y: pane.y,
        w: r.w / this.zoom,
        h: r.h / this.zoom,
      };
      const accept = layerOf(pane.kind);
      const tiles = this.cache.tilesIn(region).filter((t) => accept(t.tile.id.layer));
      const ox = device(pane.x, s);
      const oy = device(pane.y, s);
      const rtl = this.layout.rtl;
      for (const t of tiles) {
        const canvas = this.painter.ensureTile(this.cache, t);
        if (!canvas) continue;
        const src = canvas as unknown as CanvasImageSource;
        const tx = tileX(t.tile.rect, rtl);
        if (exact) {
          ctx.drawImage(src, device(tx, s) - ox + px, device(t.tile.rect.y, s) - oy + py);
        } else {
          const ratio = vs / s;
          ctx.drawImage(
            src,
            px + (tx - pane.x) * vs,
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
              px + (tileX(t.tile.rect, rtl) - pane.x) * vs,
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
      const edge = (l.header_w + l.frozen_w) * this.zoom;
      const x = Math.round((l.rtl ? this.cssW - edge : edge) * dpr);
      ctx.fillRect(l.rtl ? x : x - line, 0, line, this.els.canvas.height);
    }
    if (l.frozen_h > 0) {
      const y = Math.round((l.header_h + l.frozen_h) * this.zoom * dpr);
      ctx.fillRect(0, y - line, this.els.canvas.width, line);
    }
    // A chart being moved or resized: its outline where it will go.
    if (this.drag?.kind === 'drawing') {
      const pane = all.filter((x) => x.kind === 'cells').at(-1);
      if (pane) {
        const r = this.drag.now;
        const sx = (pane.rect.x + (tileX(r, l.rtl) - pane.x) * this.zoom) * dpr;
        const sy = (pane.rect.y + (r.y - pane.y) * this.zoom) * dpr;
        ctx.save();
        ctx.strokeStyle = cssColor(resolvePaint(palette, { Role: 'AppFill' }));
        ctx.lineWidth = Math.max(1, Math.round(1.5 * dpr));
        ctx.setLineDash([4 * dpr, 3 * dpr]);
        ctx.strokeRect(
          Math.round(sx) + 0.5,
          Math.round(sy) + 0.5,
          Math.round(r.w * this.zoom * dpr),
          Math.round(r.h * this.zoom * dpr),
        );
        ctx.restore();
      }
    }
    // A resize in progress: the guide where the border will go.
    if (this.drag?.kind === 'resize') {
      ctx.fillStyle = cssColor(resolvePaint(palette, { Role: 'AppFill' }));
      const at = Math.round(this.drag.at * dpr);
      if (this.drag.axis === 'col') ctx.fillRect(at - line, 0, line * 2, this.els.canvas.height);
      else ctx.fillRect(0, at - line, this.els.canvas.width, line * 2);
    }
    this.stats.composites += 1;
    for (const l of this.compositeListeners) l();
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

  /**
   * Keeps the pointer's moves on the sheet. A finger's quick tap has already lifted by the time
   * the press's hit test answers, and capturing a pointer that is gone throws: the tap still
   * selects its cell.
   */
  private capture(pointer: number): void {
    try {
      this.els.scroller.setPointerCapture?.(pointer);
    } catch {
      // Released already (a tap).
    }
  }

  /** The last of the presses and keys waiting their turn (see `inOrder`). */
  private pressing: Promise<void> | null = null;
  /** A key that waited is being handled now: it has had its turn. */
  private inTurn = false;

  /**
   * Runs `job` after what the user did before it. A press takes the core's answer before it can
   * select anything, however long the core is busy, so presses, and the keys that follow one, are
   * handled strictly in the order they came: each key reaches the cell selected when it was
   * pressed. A job that started an in-place edit (a double-click, F2) is followed by the cell's
   * text: the jobs after it wait for that too, so a key typed a moment after a double-click goes
   * into the cell's entry, and the Enter after it commits that entry with the key in it.
   */
  private inOrder(job: () => Promise<void> | void): void {
    const turn = () => {
      this.inTurn = true;
      try {
        return job();
      } finally {
        this.inTurn = false;
      }
    };
    const run = (this.pressing ?? Promise.resolve())
      .then(() => this.editStart ?? undefined)
      .then(turn)
      .catch(() => undefined);
    const handled: Promise<void> = run.finally(() => {
      if (this.pressing === handled) this.pressing = null;
    });
    this.pressing = handled;
  }

  /** Handles a release after what came before it: it can select (a click on the selection's
   * border), and keys typed after it must find that selection. */
  private release(e: PointerEvent): void {
    this.inOrder(() => this.onPointerUp(e));
  }

  /** Handles a press after what came before it. */
  private press(e: PointerEvent): void {
    if (e.button !== 0) return;
    e.preventDefault();
    this.focus();
    this.inOrder(() => {
      // An entry that ended a moment ago may still be in the proxy; what is typed next starts anew
      // (text that came after the press, from an input method, is kept).
      if (!this.editing && !this.held) this.els.proxy.value = '';
      return this.onPointerDown(e);
    });
  }

  private async onPointerDown(e: PointerEvent): Promise<void> {
    if (e.button !== 0) return;
    const p = this.point(e);
    // A finger or pen is coarser than a mouse: the small outline buttons take a 24 px target.
    // The state is asked for with the hit test: both answer after every command sent before the
    // press, so a pen turned on or off a moment ago is seen (the chrome's copy can lag a frame).
    const [fresh, hit] = await Promise.all([
      this.model.fetch(),
      this.model.hitTest(p.x, p.y, e.pointerType === 'touch' || e.pointerType === 'pen'),
    ]);
    // Home ▸ Borders ▸ Draw Border: the press starts a stroke of the pen, not a selection.
    if ((fresh ?? this.model.state.grid)?.home?.pen) {
      if (this.editing) await this.model.core('grid.editing.commit', { advance: null });
      this.capture(e.pointerId);
      this.drag = { kind: 'pen', from: p, to: p };
      return;
    }
    const shift = e.shiftKey;
    const add = e.ctrlKey || e.metaKey;
    const editing = this.editing;
    if (this.model.hitHooks.some((h) => h(hit, { x: e.clientX, y: e.clientY }, e))) return;
    this.capture(e.pointerId);
    if (hit === 'None') return;
    if (hit === 'Corner') {
      await this.model.core('grid.nav.select_range', { r0: 0, c0: 0, r1: 1048575, c1: 16383 });
      return;
    }
    if (hit === 'FillHandle') {
      this.drag = { kind: 'fill' };
      return;
    }
    if (hit === 'ErrorButton') {
      // The error indicator's menu, under its button.
      const r = this.els.root.getBoundingClientRect();
      this.model.set({ menu: { x: p.x + r.left, y: p.y + r.top + 10, kind: 'error' } });
      return;
    }
    if (typeof hit !== 'object') return;
    if ('SelectionBorder' in hit) {
      const grab = { row: hit.SelectionBorder.row, col: hit.SelectionBorder.col };
      this.drag = { kind: 'move', grab, at: grab, copy: add, insert: shift, moved: false };
      return;
    }
    if ('Drawing' in hit || 'DrawingHandle' in hit) {
      const index = 'Drawing' in hit ? hit.Drawing.index : hit.DrawingHandle.index;
      const handle = 'DrawingHandle' in hit ? hit.DrawingHandle.handle : null;
      if (editing) await this.model.core('grid.editing.commit', { advance: null });
      await this.model.core('grid.drawing.select', { index });
      const g = (await this.model.fetch())?.drawing;
      if (!g || g.index !== index) return;
      const rect = { x: g.x, y: g.y, w: g.width, h: g.height };
      this.drag = { kind: 'drawing', index, handle, from: p, rect, now: rect };
      return;
    }
    if ('Cell' in hit) {
      const { row, col } = hit.Cell;
      if (editing) {
        // A formula takes the reference; otherwise the entry is committed and the cell selected.
        this.sentTexts.clear();
        await this.model.core('grid.editing.point_ref', { row, col, extend: shift });
        // Keys wait behind this press: they must find out here whether the entry went on.
        const after = await this.model.fetch();
        if (after && !after.editor) {
          this.editing = false;
          this.els.proxy.value = '';
          this.sentText = '';
        }
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
    if (this.releasing) return;
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
    if (this.drag.kind === 'pen') {
      this.drag.to = p;
      void this.penPreview();
      return;
    }
    if (this.drag.kind === 'drawing') {
      const d = this.drag;
      const dx = (p.x - d.from.x) / this.zoom;
      d.now = dragRect(d.rect, d.handle, this.layout.rtl ? -dx : dx, (p.y - d.from.y) / this.zoom);
      this.paint();
      return;
    }
    if (this.drag.kind === 'move') {
      this.drag.copy = e.ctrlKey || e.metaKey;
      this.drag.insert = e.shiftKey;
    }
    this.dragPoint = p;
    this.startAutoscroll();
    void this.dragTo();
  }

  private penBusy = false;
  /** Shows the cells the pen's stroke covers (one request at a time). */
  private async penPreview(): Promise<void> {
    const d = this.drag;
    if (this.penBusy || d?.kind !== 'pen') return;
    this.penBusy = true;
    try {
      await this.model.core('grid.font.draw_border', { x0: d.from.x, y0: d.from.y, x1: d.to.x, y1: d.to.y });
    } finally {
      this.penBusy = false;
    }
  }

  private cursorBusy = false;
  private async updateCursor(p: { x: number; y: number }): Promise<void> {
    const pen = this.model.state.grid?.home?.pen;
    if (pen) {
      // Excel's pencil and eraser: a cross-hair that reads on any fill.
      this.els.scroller.style.cursor = pen.mode === 'Erase' ? 'cell' : 'crosshair';
      return;
    }
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
  private dragTo(): Promise<void> {
    if (this.dragBusy || !this.drag || !this.dragPoint) return this.dragStep ?? Promise.resolve();
    this.dragStep = this.dragStepRun();
    return this.dragStep;
  }

  private async dragStepRun(): Promise<void> {
    this.dragBusy = true;
    const p = this.dragPoint as { x: number; y: number };
    // The drag this step belongs to: a fill keeps the step's target even if the button is released meanwhile.
    const drag = this.drag;
    try {
      // Clamp to the cell area so dragging over the headers keeps selecting.
      const l = this.layout;
      const x = this.clampX(p.x);
      const y = Math.min(Math.max(p.y, l.header_h * this.zoom + 1), this.cssH - 1);
      const hit = await this.model.hitTest(x, y);
      const cell = hit !== 'None' && typeof hit === 'object' && 'Cell' in hit ? hit.Cell : null;
      if (drag?.kind === 'fill' && cell) drag.target = { row: cell.row, col: cell.col };
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
      } else if (d.kind === 'move') {
        const at =
          cell ??
          (hit !== 'None' && typeof hit === 'object' && 'SelectionBorder' in hit
            ? hit.SelectionBorder
            : null);
        if (at) {
          d.at = { row: at.row, col: at.col };
          d.moved = d.moved || at.row !== d.grab.row || at.col !== d.grab.col;
          if (d.moved) await this.model.core('grid.editing.drag_cells', this.moveArgs(d, 'Preview'));
        }
      } else if (d.kind === 'cols') {
        const h = await this.model.hitTest(x, (l.header_h * this.zoom) / 2);
        const col = typeof h === 'object' && 'ColumnHeader' in h ? h.ColumnHeader.col : cell?.col;
        if (col !== undefined)
          await this.model.core('grid.nav.select_columns', { from: col, to: col, extend: true });
      } else if (d.kind === 'rows') {
        const hx = (l.header_w * this.zoom) / 2;
        const h = await this.model.hitTest(l.rtl ? this.cssW - hx : hx, y);
        const row = typeof h === 'object' && 'RowHeader' in h ? h.RowHeader.row : cell?.row;
        if (row !== undefined)
          await this.model.core('grid.nav.select_rows', { from: row, to: row, extend: true });
      }
    } finally {
      this.dragBusy = false;
      this.dragStep = null;
    }
    if (this.dragPoint && this.dragPoint !== p) await this.dragTo();
  }

  private startAutoscroll(): void {
    if (this.autoscroll) return;
    const step = () => {
      this.autoscroll = 0;
      const p = this.dragPoint;
      if (!this.drag || !p || this.drag.kind === 'resize' || this.drag.kind === 'drawing') return;
      const l = this.layout;
      const top = (l.header_h + l.frozen_h) * this.zoom;
      const left = (l.header_w + l.frozen_w) * this.zoom;
      let dx = 0;
      let dy = 0;
      // Measured on the left-to-right layout: a right-to-left sheet is its mirror image.
      const px = l.rtl ? this.cssW - p.x : p.x;
      if (px > this.cssW - AUTOSCROLL_EDGE) dx = px - (this.cssW - AUTOSCROLL_EDGE);
      else if (px < left + AUTOSCROLL_EDGE && px < left + 4) dx = px - (left + AUTOSCROLL_EDGE);
      if (l.rtl) dx = -dx;
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
    if (d?.kind === 'fill' && this.dragStep && !this.releasing) {
      // A hit test is in flight: let it, and one for the release point if the pointer moved since,
      // settle the target before the drag ends (the steps already queued, not an extra round trip).
      this.releasing = true;
      this.dragPoint = this.point(e);
      try {
        await this.dragStep;
      } finally {
        this.releasing = false;
      }
    }
    this.drag = null;
    this.dragPoint = null;
    try {
      this.els.scroller.releasePointerCapture?.(e.pointerId);
    } catch {
      // Not captured (a tap, whose press was handled when it lifted).
    }
    if (!d) return;
    if (d.kind === 'pen') {
      const to = this.point(e);
      await this.model.core('grid.font.draw_border', {
        x0: d.from.x,
        y0: d.from.y,
        x1: to.x,
        y1: to.y,
        done: true,
      });
      return;
    }
    if (d.kind === 'drawing') {
      const r = d.now;
      const moved =
        Math.abs(r.x - d.rect.x) +
        Math.abs(r.y - d.rect.y) +
        Math.abs(r.w - d.rect.w) +
        Math.abs(r.h - d.rect.h);
      if (moved >= 1)
        await this.model.core('grid.drawing.place', {
          index: d.index,
          x: r.x,
          y: r.y,
          width: r.w,
          height: r.h,
        });
      this.paint();
      return;
    }
    if (d.kind === 'resize') {
      // A column of a right-to-left sheet grows to the left.
      const along = d.axis === 'col' && this.layout.rtl ? d.start - d.at : d.at - d.start;
      const px = Math.max(0, along / this.zoom);
      const id = d.axis === 'col' ? 'grid.cells.resize_columns' : 'grid.cells.resize_rows';
      if (Math.abs(px - d.size) >= 1)
        await this.model.run(id, { from: d.index, to: d.index, px: Math.round(px) });
      this.paint();
      return;
    }
    if (d.kind === 'cells' && !d.point) {
      for (const h of this.model.selectedHooks) h();
      return;
    }
    if (d.kind === 'move') {
      if (this.dragStep) await this.dragStep;
      // The cell under the release point (a step in flight may have tested an earlier one).
      const at = await this.cellUnder(this.point(e));
      if (at && (at.row !== d.at.row || at.col !== d.at.col)) {
        d.at = at;
        d.moved = d.moved || at.row !== d.grab.row || at.col !== d.grab.col;
      }
      if (!d.moved) {
        // A click on the border without a drag selects the cell under it, as Excel does.
        await this.model.core('grid.nav.select_cell', {
          row: d.grab.row,
          col: d.grab.col,
          extend: e.shiftKey,
          add: e.ctrlKey || e.metaKey,
        });
        return;
      }
      d.copy = e.ctrlKey || e.metaKey;
      d.insert = e.shiftKey;
      await this.dropCells(d);
      return;
    }
    if (d.kind === 'fill') {
      // The fill goes to the last drag target (the preview the user saw), with no hit test of its own.
      const t = d.target;
      if (t) {
        await this.model.run('grid.editing.auto_fill', { row: t.row, col: t.col, mode: 'Auto' });
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
      for (const h of this.model.cellDoubleClickHooks) if (await h(hit.Cell)) return;
      this.startEdit();
    }
  }

  private async onContextMenu(e: MouseEvent): Promise<void> {
    e.preventDefault();
    const p = this.point(e);
    const hit = await this.model.hitTest(p.x, p.y);
    if (typeof hit === 'object' && ('Drawing' in hit || 'DrawingHandle' in hit)) {
      // A chart, picture or shape: selected first, then its own menu.
      const index = 'Drawing' in hit ? hit.Drawing.index : hit.DrawingHandle.index;
      await this.model.core('grid.drawing.select', { index });
      const g = (await this.model.fetch())?.drawing;
      const r = this.els.root.getBoundingClientRect();
      this.model.set({
        menu: { x: p.x + r.left, y: p.y + r.top, kind: g?.chart ? 'chart' : 'object' },
      });
      return;
    }
    const kind =
      typeof hit === 'object' && 'ColumnHeader' in hit
        ? 'column'
        : typeof hit === 'object' && 'RowHeader' in hit
          ? 'row'
          : 'cell';
    // Right-clicking outside the selection selects what is under the pointer first (Excel).
    if (typeof hit === 'object' && 'Cell' in hit && !this.inSelection(hit.Cell.row, hit.Cell.col))
      await this.model.core('grid.nav.select_cell', { row: hit.Cell.row, col: hit.Cell.col });
    if (
      typeof hit === 'object' &&
      'SelectionBorder' in hit &&
      !this.inSelection(hit.SelectionBorder.row, hit.SelectionBorder.col)
    )
      await this.model.core('grid.nav.select_cell', {
        row: hit.SelectionBorder.row,
        col: hit.SelectionBorder.col,
      });
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

  private moveArgs(
    d: Extract<Drag, { kind: 'move' }>,
    phase: 'Preview' | 'Drop' | 'Cancel',
    replace = false,
  ): Record<string, unknown> {
    return {
      phase,
      row: d.at.row,
      col: d.at.col,
      grab_row: d.grab.row,
      grab_col: d.grab.col,
      copy: d.copy,
      insert: d.insert,
      replace,
    };
  }

  /** Drops dragged cells; over cells with data it asks first, as Excel does. */
  private async dropCells(d: Extract<Drag, { kind: 'move' }>): Promise<void> {
    const state = await this.model.fetch();
    if (state?.surface?.drag?.occupied) {
      this.model.pendingDrop = this.moveArgs(d, 'Drop', true);
      await this.model.core('grid.editing.drag_cells', this.moveArgs(d, 'Cancel'));
      this.model.openDialog('replaceCells');
      return;
    }
    await this.model.core('grid.editing.drag_cells', this.moveArgs(d, 'Drop'));
  }

  /** A surface x clamped to the cells (the row headers are at the right of a right-to-left sheet). */
  private clampX(x: number): number {
    const hx = this.layout.header_w * this.zoom + 1;
    return this.layout.rtl
      ? Math.min(Math.max(x, 1), this.cssW - hx)
      : Math.min(Math.max(x, hx), this.cssW - 1);
  }

  /** The cell under a surface point, clamped to the cell area (a drag over the headers). */
  private async cellUnder(p: { x: number; y: number }): Promise<{ row: number; col: number } | null> {
    const l = this.layout;
    const x = this.clampX(p.x);
    const y = Math.min(Math.max(p.y, l.header_h * this.zoom + 1), this.cssH - 1);
    const hit = await this.model.hitTest(x, y);
    if (hit === 'None' || typeof hit !== 'object') return null;
    if ('Cell' in hit) return hit.Cell;
    if ('SelectionBorder' in hit) return hit.SelectionBorder;
    return null;
  }

  /** Esc during a drag of cells leaves them where they are. */
  cancelDrag(): boolean {
    const d = this.drag;
    if (d?.kind !== 'move') return false;
    this.drag = null;
    this.dragPoint = null;
    void this.model.core('grid.editing.drag_cells', this.moveArgs(d, 'Cancel'));
    return true;
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
    // An input method's key (or a dead key) starts a composition the browser makes in the
    // proxy: never held back, its text waits there for its turn (see `held`).
    if (e.key === 'Process' || e.key === 'Dead' || e.keyCode === 229) {
      e.stopPropagation();
      return;
    }
    const history = historyKey(e);
    if (e.key === 'Escape' && this.cancelDrag()) {
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    // A press has not landed: every key waits its turn behind it, typing too, so nothing reaches
    // the proxy that belongs to a cell the core has not selected yet.
    if (this.pressing) {
      if (history) {
        // Undo and Redo act on what is before them: they take their turn too.
        e.preventDefault();
        e.stopPropagation();
        this.inOrder(() => this.historyInTurn(history));
        return;
      }
      if (gridKeyAction(e, 'Edit', null) === null && !typesText(e)) return; // the window's shortcut
      e.preventDefault();
      if (!e.altKey) e.stopPropagation();
      const k = keyLike(e);
      this.inOrder(() => this.handleKey(k, null));
      return;
    }
    // F2's text has not arrived. Plain typing goes into the proxy and joins the text when it does;
    // every other key (and typing after one) waits for it, in order, and is decided by the mode it
    // then finds, so "F2, Alt+Enter, Enter, Up" behaves as it would slowly.
    if (this.editStart) {
      if (!this.keysWaiting && typesText(e) && !e.altKey) {
        e.stopPropagation();
        return;
      }
      if (history) {
        e.preventDefault();
        e.stopPropagation();
        this.keysWaiting += 1;
        this.afterEditStart(
          this.editStart.then(() => {
            this.keysWaiting -= 1;
            return this.historyInTurn(history);
          }),
        );
        return;
      }
      if (gridKeyAction(e, 'Edit', null) === null && !typesText(e)) return; // the window's shortcut
      e.preventDefault();
      if (!e.altKey) e.stopPropagation();
      const k = keyLike(e);
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
   * Ctrl+Z or Ctrl+Y that waited its turn, as it acts when pressed then: in an entry, Ctrl+Z
   * cancels it; otherwise the window's Undo or Redo, finished before the next key.
   */
  private async historyInTurn(kind: 'undo' | 'redo'): Promise<void> {
    if (kind === 'undo' && (this.editing || this.pendingEdit)) {
      this.applyKey({ kind: 'core', id: 'grid.editing.cancel_edit' });
      return;
    }
    await this.model.run(kind === 'undo' ? 'suite.edit.undo' : 'suite.edit.redo');
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
    // A chart or picture selected: Tab and Shift+Tab go to the next and the previous one, as in
    // Excel (Go To Special ▸ Objects selects the first).
    const drawing = this.state?.drawing ?? null;
    if (mode === 'Ready' && drawing && k.key === 'Tab' && !k.ctrlKey && !k.metaKey && !k.altKey) {
      e?.preventDefault();
      e?.stopPropagation();
      const next = drawing.index + (k.shiftKey ? -1 : 1);
      void this.model.core('grid.drawing.select', { index: Math.max(0, next) }).then((ok) => {
        if (!ok && next > 0) void this.model.core('grid.drawing.select', { index: 0 });
      });
      return;
    }
    if (mode === 'Ready' && k.key === 'F2' && !k.ctrlKey && !k.metaKey && !k.altKey && !k.shiftKey) {
      e?.preventDefault();
      e?.stopPropagation();
      this.startEdit();
      return;
    }
    const { enterMoves, enterDirection } = loadOptions().advanced;
    const action = gridKeyAction(
      k,
      mode,
      ed ? { completions: ed.completions.length, pickList: !!ed.pick_list } : null,
      { moves: enterMoves, direction: enterDirection },
    );
    if (!action) {
      // A window shortcut while typing (Ctrl+B on selected characters): the core gets the
      // entry's selection first, before the browser's selectionchange would bring it.
      if (this.editing) this.onSelectionChange();
      return;
    }
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
      this.held = false;
      this.deferredEdit = null;
      this.sentText = '';
      this.echoes = [];
      this.sentTexts.clear();
      this.els.proxy.value = '';
      this.endedAsked = this.model.asked;
      this.ending += 1;
      void this.model.core(action.id, args).then((ok) => {
        this.ending -= 1;
        // A refused entry (data validation) stays for correction: the next state brings it back.
        if (!ok) this.model.refresh();
      });
      return;
    }
    // The core changes the text itself (F4, a suggestion): an older text may rightly come back.
    this.sentTexts.clear();
    if (action.id === 'grid.editing.accept_completion') {
      // The suggestion's text comes from the core: keys typed before it arrives wait in the
      // proxy and go in at the caret after it (as after F2), or they would overwrite it.
      this.pendingEdit = true;
      this.accepting = true;
      this.els.proxy.value = '';
      const orCommit = action.orCommit;
      void this.model.core(action.id, args).then((ok) => {
        this.accepting = false;
        if (!ok && orCommit) {
          // No list in the core any more: Enter commits the entry, as it does without one (and
          // the refusal is not news to the user).
          this.pendingEdit = false;
          this.model.set({ notice: null });
          this.applyKey({ kind: 'core', id: 'grid.editing.commit', args: { advance: orCommit.advance } });
          return;
        }
        this.model.refresh();
      });
      return;
    }
    if (action.id === 'grid.editing.begin') this.editing = true;
    void this.model.core(action.id, args);
  }

  /** F2 / double-click: edit the active cell in place. */
  private startEdit(): void {
    this.pendingEdit = true;
    this.deferredEdit = null;
    // Text from an input method that came after the double-click stays: it goes in after the
    // cell's text.
    if (!this.held) this.els.proxy.value = '';
    const attempt = ++this.editAttempt;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const opened = this.model.core('grid.editing.edit_cell').then(async (ok) => {
      // The state with the cell's text, at once (not on the next frame): it ends `pendingEdit`.
      if (ok) await this.model.fetch();
      return ok;
    });
    // A core that does not answer is not waited for long: the keys after it would wait with it.
    const late = new Promise<'late'>((done) => {
      timer = setTimeout(() => done('late'), EDIT_START_MS);
    });
    const started = Promise.race([opened, late]).then((ok) => {
      clearTimeout(timer);
      if (attempt !== this.editAttempt) return;
      this.editAttempt += 1;
      if (ok !== true) {
        // Refused, or given up as a refused one is (with the notice a failed command gives): the
        // keys typed meanwhile make an entry of their own, before the keys that wait.
        if (!this.pendingEdit) return;
        this.pendingEdit = false;
        this.deferredEdit = null;
        if (ok === 'late') this.model.set({ notice: errorText({ kind: 'Internal' }) });
        if (this.held && this.els.proxy.value !== '') this.sendInput();
        return;
      }
      // A composition that holds the cell's text keeps the edit start open until it ends.
      if (this.pendingEdit && !this.deferredEdit) this.pendingEdit = false;
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
    if (this.pendingEdit) {
      // It joins the cell's text when that arrives.
      this.held = true;
      return;
    }
    if (this.pressing && !this.inTurn) {
      // Text that came with no key to hold back (an input method).
      this.held = true;
      this.inOrder(() => this.onInput());
      return;
    }
    this.sendInput();
  }

  /** Sends the proxy's text to the core: it starts the entry, or is its new text. */
  private sendInput(): void {
    this.held = this.composing;
    const p = this.els.proxy;
    const text = p.value;
    if (!this.editing) {
      if (text === '') return;
      this.editing = true;
      this.sentText = text;
      this.lastSel = this.selKey();
      this.echoes.push(text);
      this.sentTexts = new Set([text]);
      this.typedAsked = this.model.asked;
      void this.model.core('grid.editing.begin', { text });
      return;
    }
    this.sentText = text;
    this.typedAsked = this.model.asked;
    // The browser's selectionchange for this keystroke comes later: it must not send this text
    // again after a key that changed it in the core (F4, a suggestion).
    this.lastSel = this.selKey();
    this.echoes.push(text);
    this.sentTexts.add(text);
    if (this.echoes.length > 64) this.echoes.splice(0, this.echoes.length - 64);
    void this.model.core('grid.editing.set_text', {
      text,
      start: p.selectionStart ?? text.length,
      end: p.selectionEnd ?? text.length,
      formula_bar: false,
    });
  }

  /** A suggestion is being accepted (its text not yet back from the core). */
  private accepting = false;

  private lastSel = '';
  /** The proxy's caret and length, as `onSelectionChange` compares them. */
  private selKey(): string {
    const p = this.els.proxy;
    return `${p.selectionStart}:${p.selectionEnd}:${p.value.length}`;
  }

  private onSelectionChange(): void {
    if (!this.editing || document.activeElement !== this.els.proxy) return;
    // An entry in the Formula Bar is the bar's own: the keys coming back to the sheet (Esc,
    // Enter) move the proxy's caret, which is no edit of the cell.
    if (this.model.state.grid?.editor?.formula_bar) return;
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
    e.preventDefault();
    // No clipboard data: the core pastes Kiln's own last copy.
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

function keyLike(e: KeyboardEvent): KeyLike {
  return { key: e.key, shiftKey: e.shiftKey, ctrlKey: e.ctrlKey, metaKey: e.metaKey, altKey: e.altKey };
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
  if (hit === 'ListButton' || hit === 'ErrorButton') return 'pointer';
  if ('SelectionBorder' in hit) return 'move';
  if ('ColumnBorder' in hit) return 'col-resize';
  if ('RowBorder' in hit) return 'row-resize';
  if ('ColumnHeader' in hit) return 's-resize';
  if ('RowHeader' in hit) return 'e-resize';
  if ('Drawing' in hit) return 'move';
  if ('FilterButton' in hit) return 'pointer';
  if ('PageBreak' in hit) return hit.PageBreak.columns ? 'col-resize' : 'row-resize';
  if ('DrawingHandle' in hit)
    return ['nwse-resize', 'ns-resize', 'nesw-resize', 'ew-resize'][hit.DrawingHandle.handle % 4] ?? 'move';
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
  // The live region stays the same element, so its changes are read (a new region is not).
  let live = host.querySelector<HTMLElement>(':scope > [aria-live]');
  if (!live) {
    live = document.createElement('div');
    live.setAttribute('aria-live', 'polite');
    live.setAttribute('role', 'status');
  }
  const grid = tree.nodes[0];
  if (!grid) {
    host.replaceChildren(live);
    return;
  }
  const byId = new Map(tree.nodes.map((n) => [n.id, n]));
  const [rows, cols] = (grid.text ?? '1048576 16384').split(' ').map(Number);
  const table = document.createElement('div');
  table.setAttribute('role', 'grid');
  table.setAttribute('aria-label', grid.name ?? t('Sheet'));
  table.setAttribute('aria-rowcount', String((rows ?? 0) + 1));
  table.setAttribute('aria-colcount', String((cols ?? 0) + 1));
  table.setAttribute('aria-multiselectable', 'true');
  for (const rowId of grid.children) {
    const row = byId.get(rowId);
    if (!row) continue;
    const tr = document.createElement('div');
    tr.setAttribute('role', 'row');
    const header = rowId === 2 ** 38;
    tr.setAttribute('aria-rowindex', String(header ? 1 : rowId - ROW_BASE + 2));
    for (const cellId of row.children) {
      const c = byId.get(cellId);
      if (!c) continue;
      const td = document.createElement('div');
      if (c.role === 'ColumnHeader') {
        td.setAttribute('role', 'columnheader');
        td.setAttribute('aria-colindex', String(cellId - (2 ** 38 + 1) + 2));
        td.textContent = c.name ?? '';
      } else if (c.role === 'RowHeader') {
        td.setAttribute('role', 'rowheader');
        td.setAttribute('aria-colindex', '1');
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
  host.replaceChildren(table, live);
  if (live.textContent !== (tree.announce ?? '')) live.textContent = tree.announce ?? '';
  if (tree.active !== null) proxy.setAttribute('aria-activedescendant', `kiln-grid-cell-${tree.active}`);
}
