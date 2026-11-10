/**
 * The UI state of Kiln Grid's features: the open feature dialog, the open pane (Formula
 * Lab, Watch Window, Comments, Spelling), the AutoFilter dropdown and the note card under the
 * pointer, Format Painter's brush, and the user's name for notes and comments. Every change to
 * the workbook is a core command (`exec`); what the dialogs show are the core's answers
 * (`ask`).
 */
import type {
  GridCommand,
  GridFeatureState,
  GridQuery,
  GridQueryResult,
  GridRange,
  Selection,
} from '@kiln/protocol';
import { createStore, type StoreApi } from 'zustand/vanilla';
import { type GridEditorModel, gridResult, messageOf } from '../model.ts';

/** The feature dialogs. */
export type FeatureDialog =
  | 'sort'
  | 'cfRule'
  | 'cfManage'
  | 'validation'
  | 'createTable'
  | 'resizeTable'
  | 'find'
  | 'goToSpecial'
  | 'nameManager'
  | 'defineName'
  | 'createNames'
  | 'pasteName'
  | 'link'
  | 'pageSetup'
  | 'textToColumns'
  | 'removeDuplicates'
  | 'import'
  | 'evaluate'
  | 'errorChecking'
  | 'customFilter'
  | 'top10'
  | 'symbol'
  | 'note'
  | 'altText'
  | 'crop'
  | 'formatObject'
  | 'shapeText'
  | 'functionArgs'
  | 'iterative'
  | 'circular'
  | 'validationAlert';

export type FeaturePane = 'lab' | 'watch' | 'comments' | 'spelling';

export interface FeatureUiState {
  dialog: { id: FeatureDialog; arg?: unknown } | null;
  pane: FeaturePane | null;
  /** The AutoFilter dropdown: its header cell and where it opens (client px). */
  filterMenu: { row: number; col: number; x: number; y: number } | null;
  /** Format Painter: the source range; `sticky` keeps painting (double-click). */
  painter: { from: GridRange; sticky: boolean } | null;
  /** The note or comment card shown on hover: cell and client px. */
  hover: { row: number; col: number; x: number; y: number } | null;
  /** The comment composer of the Comments pane: a new thread or a reply. */
  composer: { reply: string | null } | null;
  /** The total row's function list of a table column (Alt+Down or its button; client px). */
  totalsMenu: { col: number; x: number; y: number } | null;
}

const INSTANCES = new WeakMap<GridEditorModel, GridFeatures>();

/** The features model of a Grid editor model (one per editor). */
export function featuresOf(model: GridEditorModel): GridFeatures {
  let f = INSTANCES.get(model);
  if (!f) {
    f = new GridFeatures(model);
    INSTANCES.set(model, f);
  }
  return f;
}

/** The features part of the core's state. */
export function gridFeatures(model: GridEditorModel): GridFeatureState | null {
  return model.state.grid?.features ?? null;
}

const AUTHOR_KEY = 'kiln.user.name';

/** The name notes and comments are written under (Options ▸ General ▸ User name). */
export function authorName(): string {
  try {
    const v = localStorage.getItem(AUTHOR_KEY);
    if (v?.trim()) return v.trim();
  } catch {
    // Private mode.
  }
  return 'Author';
}

const RECENT_KEY = 'kiln.grid.recent_functions';
/** Excel's starting list of Most Recently Used functions. */
const RECENT_DEFAULT = ['SUM', 'AVERAGE', 'IF', 'HYPERLINK', 'COUNT', 'MAX', 'SIN', 'SUMIF', 'PMT', 'STDEV'];

/** The ten functions inserted last, newest first (Formulas ▸ Recently Used). */
export function recentFunctions(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(RECENT_KEY) ?? 'null') as unknown;
    if (Array.isArray(v) && v.every((x) => typeof x === 'string')) return v as string[];
  } catch {
    // Private mode or a damaged entry.
  }
  return [...RECENT_DEFAULT];
}

/** Puts a function at the top of the recent list. */
export function noteRecentFunction(name: string): void {
  const list = [name, ...recentFunctions().filter((n) => n !== name)].slice(0, 10);
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(list));
  } catch {
    // Private mode: the list lasts for the page.
  }
}

export class GridFeatures {
  readonly model: GridEditorModel;
  readonly store: StoreApi<FeatureUiState>;

  constructor(model: GridEditorModel) {
    this.model = model;
    this.store = createStore<FeatureUiState>(() => ({
      dialog: null,
      pane: null,
      filterMenu: null,
      painter: null,
      hover: null,
      composer: null,
      totalsMenu: null,
    }));
  }

  get state(): FeatureUiState {
    return this.store.getState();
  }

  set(patch: Partial<FeatureUiState>): void {
    this.store.setState(patch);
  }

  /** Runs a Grid core command, then shows the new state; a refusal becomes the notice. */
  async exec(command: GridCommand): Promise<boolean> {
    try {
      await this.model.client.execute(this.model.doc, { Grid: command });
      await this.model.fetch();
      const message = gridFeatures(this.model)?.message;
      if (message) this.model.set({ notice: message });
      return true;
    } catch (error) {
      this.model.set({ notice: messageOf(error) });
      return false;
    }
  }

  /** Runs a Grid core command and returns the error message instead of showing it (dialogs). */
  async tryExec(command: GridCommand): Promise<string | null> {
    try {
      await this.model.client.execute(this.model.doc, { Grid: command });
      await this.model.fetch();
      const message = gridFeatures(this.model)?.message;
      if (message) this.model.set({ notice: message });
      return null;
    } catch (error) {
      return messageOf(error);
    }
  }

  /** Asks the core. */
  async ask(q: GridQuery): Promise<GridQueryResult | null> {
    try {
      return gridResult(await this.model.client.query(this.model.doc, { Grid: q }));
    } catch {
      return null;
    }
  }

  openDialog(id: FeatureDialog, arg?: unknown): void {
    this.model.set({ menu: null });
    this.store.setState({ dialog: { id, ...(arg !== undefined ? { arg } : {}) }, filterMenu: null });
  }

  /** Closes the dialog and gives the keys back to the sheet. */
  closeDialog(): void {
    this.store.setState({ dialog: null });
    this.model.surface?.focus();
  }

  togglePane(pane: FeaturePane): void {
    const open = this.state.pane === pane ? null : pane;
    this.store.setState({ pane: open });
    if (pane === 'lab' || this.state.pane === null) {
      void this.exec({ FormulaLab: { open: open === 'lab' } });
    }
    if (!open) this.model.surface?.focus();
  }

  closePane(): void {
    const was = this.state.pane;
    this.store.setState({ pane: null });
    if (was === 'lab') void this.exec({ FormulaLab: { open: false } });
    this.model.surface?.focus();
  }

  /**
   * Alt+Down (or a cell's dropdown button), as in Excel: on an AutoFilter header its filter
   * menu, on a table's total row the list of functions, elsewhere the cell's list (validation or
   * the column's values, the core's pick list). Never starts editing a header.
   */
  async openDropdown(): Promise<void> {
    // The keys can outrun the state the editor keeps (it is fetched a frame after each change):
    // an arrow, then Alt+Down at once. The core answers in order, after the commands before.
    const [fresh, selection] = await Promise.all([
      this.model.fetch(),
      this.model.client.query(this.model.doc, 'Selection').catch(() => null),
    ]);
    const g = fresh ?? this.model.state.grid;
    const a = activeCellOf(selection) ?? activeCell(this.model);
    if (!g || g.editor || !a) {
      await this.model.core('grid.editing.open_dropdown');
      return;
    }
    const [x, y, w, h] = g.features.cell_rect;
    const origin = (typeof document === 'undefined'
      ? null
      : document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.getBoundingClientRect()) ?? {
      left: 0,
      top: 0,
    };
    const left = origin.left + x;
    const below = origin.top + y + h;
    const menu = await this.ask({ FilterMenu: { row: a.row, col: a.col } });
    if (menu && 'FilterMenu' in menu) {
      this.store.setState({
        filterMenu: { row: a.row, col: a.col, x: left + Math.max(0, w - 24), y: below },
      });
      return;
    }
    const t = g.features.table;
    if (t?.options.total_row && a.row === t.range.r1 && a.col >= t.range.c0 && a.col <= t.range.c1) {
      this.store.setState({ totalsMenu: { col: a.col, x: left, y: below } });
      return;
    }
    await this.model.core('grid.editing.open_dropdown');
  }

  /** After a selection by the pointer ends: paints when the brush is armed. */
  async afterSelect(): Promise<void> {
    const p = this.state.painter;
    if (!p) return;
    await this.exec({ PaintFormat: { from: p.from } });
    if (!p.sticky) this.store.setState({ painter: null });
  }
}

/** The active cell the core reports. */
/** The active cell of the core's answer to `Selection`. */
function activeCellOf(answer: unknown): { row: number; col: number } | null {
  const sel =
    answer && typeof answer === 'object' && 'Selection' in answer
      ? (answer as { Selection: Selection }).Selection
      : null;
  const kind = sel?.kind;
  if (!kind || typeof kind !== 'object' || !('Cells' in kind)) return null;
  return { row: kind.Cells.active.row, col: kind.Cells.active.col };
}

export function activeCell(model: GridEditorModel): { row: number; col: number } | null {
  const sel = (model.registry.context.core as { selection?: Selection } | undefined)?.selection;
  const kind = sel?.kind;
  if (!kind || typeof kind !== 'object' || !('Cells' in kind)) return null;
  return { row: kind.Cells.active.row, col: kind.Cells.active.col };
}

/** The selection the core reports (ranges as rows and columns, the active cell's last). */
export function selectionRanges(model: GridEditorModel): GridRange[] {
  const sel = (model.registry.context.core as { selection?: Selection } | undefined)?.selection;
  const kind = sel?.kind;
  if (!kind || typeof kind !== 'object' || !('Cells' in kind)) return [];
  return kind.Cells.ranges.map((r) => ({
    r0: Math.min(r.start.row, r.end.row),
    c0: Math.min(r.start.col, r.end.col),
    r1: Math.max(r.start.row, r.end.row),
    c1: Math.max(r.start.col, r.end.col),
  }));
}
