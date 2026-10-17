/**
 * The Grid editor's view model: UI state only (dialogs, the find bar, menus, the formula bar's
 * height) plus the latest answer of the core's `Query::Grid(State)` — sheet tabs, name box,
 * formula bar text, mode, the in-cell editor with its suggestions and hint, the active cell's
 * format and the pane layout. Nothing is computed from the workbook: after every
 * `Event::Changed` of the window's document the model asks the core again (one round trip per
 * animation frame) and stores what it says (PLAN.md §3.7).
 */
import { buildCommand, type CommandRegistry } from '@kiln/commands';
import type {
  ChartKind,
  DocId,
  GridFormatCells,
  GridHit,
  GridQuery,
  GridQueryResult,
  GridState,
  KilnClient,
  StatusBar,
  SuiteCommand,
} from '@kiln/protocol';
import { createStore, type StoreApi } from 'zustand/vanilla';

/** The dialogs of the Grid editor. */
export type GridDialog =
  | 'formatCells'
  | 'goTo'
  | 'pasteSpecial'
  | 'insertCells'
  | 'deleteCells'
  | 'rowHeight'
  | 'columnWidth'
  | 'renameSheet'
  | 'tabColor'
  | 'insertFunction'
  | 'recommendedCharts';

/** A context menu: where and for what. */
export interface GridMenu {
  x: number;
  y: number;
  kind: 'cell' | 'row' | 'column' | 'tab';
  /** The sheet of a tab menu. */
  sheet?: number;
}

export interface GridUiState {
  /** The core's answer to `Query::Grid(State)`. */
  grid: GridState | null;
  /** The status bar fields of the last change (zoom, view). */
  status: StatusBar | null;
  dialog: { id: GridDialog; tab?: string; arg?: number } | null;
  /** The find bar (Ctrl+F). */
  find: { open: boolean; text: string; matchCase: boolean; wholeCell: boolean; inFormulas: boolean };
  /** The formula bar shows several lines. */
  formulaBarExpanded: boolean;
  /** View ▸ Formula Bar. */
  showFormulaBar: boolean;
  menu: GridMenu | null;
  /** A message for the user (a refused entry, a failed command). */
  notice: string | null;
  /** The Format Chart pane is open (while a chart is selected). */
  chartPane: boolean;
}

export interface GridModelDeps {
  client: KilnClient;
  doc: DocId;
  registry: CommandRegistry;
}

const PREF_KEY = 'kiln.grid.ui';

function loadPrefs(): { showFormulaBar?: boolean; formulaBarExpanded?: boolean } {
  try {
    const v = JSON.parse(localStorage.getItem(PREF_KEY) ?? '{}') as Record<string, unknown>;
    return {
      ...(typeof v.showFormulaBar === 'boolean' ? { showFormulaBar: v.showFormulaBar } : {}),
      ...(typeof v.formulaBarExpanded === 'boolean' ? { formulaBarExpanded: v.formulaBarExpanded } : {}),
    };
  } catch {
    return {};
  }
}

/** Unwraps a `QueryResult::Grid`. */
export function gridResult(r: unknown): GridQueryResult | null {
  return typeof r === 'object' && r !== null && 'Grid' in r ? (r as { Grid: GridQueryResult }).Grid : null;
}

/** The message of a rejected command. */
export function messageOf(error: unknown): string {
  if (error && typeof error === 'object' && 'message' in error) {
    const m = (error as { message: unknown }).message;
    if (typeof m === 'string') return m.replace(/^invalid input: /, '');
  }
  return 'The command could not run.';
}

/** The surface the model drives (so it can scroll, focus and paint). */
export interface GridSurfaceHandle {
  focus(): void;
  /** Applies a scroll request of the core. */
  reveal(x: number, y: number): void;
  /** The surface's size in CSS pixels. */
  size(): { width: number; height: number };
}

export class GridEditorModel {
  readonly store: StoreApi<GridUiState>;
  readonly client: KilnClient;
  readonly doc: DocId;
  readonly registry: CommandRegistry;
  surface: GridSurfaceHandle | null = null;
  /** The ids the surface sent straight to the core, newest last (the last 200; e2e tests). */
  readonly sent: string[] = [];
  private pending: { cancel(): void } | null = null;
  private lastReveal = -1;
  /** The last state given to the chrome, without the scrollable size (see `fetch`). */
  private stateKey = '';
  private disposed = false;
  private readonly unsubscribe: () => void;
  /** Listeners told about every new state (the surface follows the layout). */
  private readonly listeners = new Set<(s: GridState, asked: number) => void>();
  /** Numbers the state requests (a listener can tell a state asked before something it did). */
  asked = 0;

  constructor({ client, doc, registry }: GridModelDeps) {
    this.client = client;
    this.doc = doc;
    this.registry = registry;
    const prefs = loadPrefs();
    this.store = createStore<GridUiState>(() => ({
      grid: null,
      status: null,
      dialog: null,
      find: { open: false, text: '', matchCase: false, wholeCell: false, inFormulas: false },
      formulaBarExpanded: prefs.formulaBarExpanded ?? false,
      showFormulaBar: prefs.showFormulaBar ?? true,
      menu: null,
      notice: null,
      chartPane: false,
    }));
    this.unsubscribe = client.subscribe((e) => {
      if (typeof e !== 'object' || !('Changed' in e) || e.Changed.summary.doc !== this.doc) return;
      const status = e.Changed.status;
      if (JSON.stringify(status) !== JSON.stringify(this.state.status)) this.store.setState({ status });
      this.refresh();
    });
    this.refresh();
    client.query(doc, 'Status').then(
      (r) => {
        if (!this.disposed && typeof r === 'object' && r !== null && 'Status' in r)
          this.store.setState({ status: r.Status });
      },
      () => undefined,
    );
  }

  get state(): GridUiState {
    return this.store.getState();
  }

  set(patch: Partial<GridUiState>): void {
    this.store.setState(patch);
    if ('showFormulaBar' in patch || 'formulaBarExpanded' in patch) {
      try {
        const s = this.state;
        localStorage.setItem(
          PREF_KEY,
          JSON.stringify({ showFormulaBar: s.showFormulaBar, formulaBarExpanded: s.formulaBarExpanded }),
        );
      } catch {
        // Private mode: the preference lasts for this window only.
      }
    }
  }

  /** Follows every new state (returns the unsubscribe function). */
  onState(listener: (s: GridState, asked: number) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Asks the core for the state (coalesced to one round trip per animation frame). */
  refresh(): void {
    if (this.pending) return;
    this.pending = nextFrame(() => {
      this.pending = null;
      void this.fetch();
    });
  }

  /** Asks for the state now. */
  async fetch(): Promise<GridState | null> {
    this.asked += 1;
    const asked = this.asked;
    const r = await this.query('State');
    if (this.disposed || !r || !('State' in r)) return null;
    const grid = r.State;
    // The scrollable size grows as the sheet scrolls down or right (as Excel's does): the surface
    // follows it (the listeners below), the chrome does not need to re-render for it.
    const key = JSON.stringify({ ...grid, layout: { ...grid.layout, content_w: 0, content_h: 0 } });
    if (key !== this.stateKey) {
      this.stateKey = key;
      this.store.setState({ grid });
      const editing = grid.mode !== 'Ready';
      this.registry.setContext({ gridState: grid, scopes: editing ? ['cell-edit'] : [] });
      if (!grid.drawing && this.state.chartPane) this.store.setState({ chartPane: false });
    }
    if (grid.reveal && grid.reveal.seq !== this.lastReveal) {
      this.lastReveal = grid.reveal.seq;
      this.surface?.reveal(grid.reveal.x, grid.reveal.y);
    }
    for (const l of [...this.listeners]) l(grid, asked);
    return grid;
  }

  async query(q: GridQuery): Promise<GridQueryResult | null> {
    try {
      return gridResult(await this.client.query(this.doc, { Grid: q }));
    } catch {
      return null;
    }
  }

  async hitTest(x: number, y: number): Promise<GridHit> {
    const r = await this.query({ HitTest: { x, y } });
    return r && 'Hit' in r ? r.Hit : 'None';
  }

  async formatCells(): Promise<GridFormatCells | null> {
    const r = await this.query('FormatCells');
    return r && 'FormatCells' in r ? r.FormatCells : null;
  }

  async preview(code: string): Promise<string> {
    const r = await this.query({ FormatPreview: { code } });
    return r && 'Preview' in r ? r.Preview : '';
  }

  /** Runs a command through the registry (the ribbon's path; the command log sees it). */
  async run(id: string, args?: unknown): Promise<boolean> {
    const r = await this.registry.execute(id, args);
    if (r.status === 'failed') {
      this.store.setState({ notice: messageOf(r.error) });
      return false;
    }
    return r.status === 'ran';
  }

  /**
   * Sends a core command straight to the core, in order with everything sent before (typing,
   * navigation): the surface's own input path, which must not wait for the registry's
   * availability check.
   */
  async core(id: string, args?: unknown): Promise<boolean> {
    this.sent.push(id);
    if (this.sent.length > 200) this.sent.splice(0, this.sent.length - 200);
    try {
      await this.client.execute(this.doc, buildCommand(id, args, { app: 'grid' }));
      return true;
    } catch (error) {
      const e = error as { kind?: unknown };
      if (e.kind !== 'Unavailable') this.store.setState({ notice: messageOf(error) });
      return false;
    }
  }

  /**
   * Runs a chart command (Chart Design, Format, the Format pane, Insert ▸ Charts) on the
   * selected chart or the selection, then shows the new state.
   */
  async chart(command: SuiteCommand): Promise<boolean> {
    try {
      await this.client.execute(this.doc, { Suite: command });
      await this.fetch();
      return true;
    } catch (error) {
      this.store.setState({ notice: messageOf(error) });
      return false;
    }
  }

  /** Recommended Charts for the selection, best first. */
  async recommendedCharts(): Promise<ChartKind[]> {
    const r = await this.query('RecommendedCharts');
    return r && 'Charts' in r ? r.Charts : [];
  }

  openDialog(id: GridDialog, tab?: string, arg?: number): void {
    this.store.setState({
      dialog: { id, ...(tab ? { tab } : {}), ...(arg !== undefined ? { arg } : {}) },
      menu: null,
    });
  }

  closeDialog(): void {
    this.store.setState({ dialog: null });
    this.surface?.focus();
  }

  dispose(): void {
    this.disposed = true;
    this.pending?.cancel();
    this.unsubscribe();
    this.listeners.clear();
    this.registry.setContext({ gridState: undefined, scopes: [] });
  }
}

/** Runs `fn` on the next animation frame (or a task where there is none). */
export function nextFrame(fn: () => void): { cancel(): void } {
  if (typeof requestAnimationFrame === 'function') {
    const id = requestAnimationFrame(fn);
    return { cancel: () => cancelAnimationFrame(id) };
  }
  const t = setTimeout(fn, 0);
  return { cancel: () => clearTimeout(t) };
}
