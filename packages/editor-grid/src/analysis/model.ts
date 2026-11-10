/**
 * The UI state of Kiln Grid's analysis features: the open analysis dialog, the
 * PivotTable Fields pane, a PivotTable field's menu, and what the core reports about the
 * PivotTable, sparklines, Goal Seek, Flash Fill and protection at the active cell. Every change to
 * the workbook is a core command.
 */
import type {
  GridAnalysisState,
  GridCommand,
  GridPivotEdit,
  GridPivotInfo,
  GridSparkEdit,
} from '@kiln/protocol';
import { createStore, type StoreApi } from 'zustand/vanilla';
import { featuresOf, type GridFeatures } from '../features/model.ts';
import type { GridEditorModel } from '../model.ts';

export type AnalysisDialog =
  | 'createPivot'
  | 'valueSettings'
  | 'fieldSettings'
  | 'pivotOptions'
  | 'pivotMove'
  | 'pivotSource'
  | 'sparklines'
  | 'sparkData'
  | 'sparkHidden'
  | 'goalSeek'
  | 'goalSeekStatus'
  | 'subtotal'
  | 'protectSheet'
  | 'unprotectSheet'
  | 'protectWorkbook'
  | 'unprotectWorkbook'
  | 'confirmPassword'
  | 'sparkColor'
  | 'alert';

/** A PivotTable field's menu: from a field button on the sheet or a field in the pane. */
export interface FieldMenu {
  field: number;
  /** Where it opens (client px). */
  x: number;
  y: number;
  /** Opened from a field button: the field's sort and filter list. */
  filter: boolean;
}

export interface AnalysisUiState {
  dialog: { id: AnalysisDialog; arg?: unknown } | null;
  /** The Fields pane: shown while the active cell is in a PivotTable unless closed. */
  fieldsClosed: boolean;
  fieldMenu: FieldMenu | null;
}

const INSTANCES = new WeakMap<GridEditorModel, GridAnalysis>();

export function analysisOf(model: GridEditorModel): GridAnalysis {
  let a = INSTANCES.get(model);
  if (!a) {
    a = new GridAnalysis(model);
    INSTANCES.set(model, a);
  }
  return a;
}

/** The analysis part of the core's state. */
export function gridAnalysis(model: GridEditorModel): GridAnalysisState | null {
  return model.state.grid?.analysis ?? null;
}

export class GridAnalysis {
  readonly model: GridEditorModel;
  readonly features: GridFeatures;
  readonly store: StoreApi<AnalysisUiState>;

  constructor(model: GridEditorModel) {
    this.model = model;
    this.features = featuresOf(model);
    this.store = createStore<AnalysisUiState>(() => ({ dialog: null, fieldsClosed: false, fieldMenu: null }));
  }

  get state(): AnalysisUiState {
    return this.store.getState();
  }

  set(patch: Partial<AnalysisUiState>): void {
    this.store.setState(patch);
  }

  /** The PivotTable at the active cell. */
  pivot(): GridPivotInfo | null {
    return gridAnalysis(this.model)?.pivot ?? null;
  }

  /** Runs a core command; a refusal shows as the notice. */
  exec(c: GridCommand): Promise<boolean> {
    return this.features.exec(c);
  }

  /** Runs a core command; the refusal is returned (dialogs show it). */
  tryExec(c: GridCommand): Promise<string | null> {
    return this.features.tryExec(c);
  }

  /** A change to the PivotTable at the active cell. */
  edit(edit: GridPivotEdit): Promise<boolean> {
    return this.exec({ Pivot: { edit } });
  }

  /** A change to the sparklines at the selection. */
  spark(edit: GridSparkEdit): Promise<boolean> {
    return this.exec({ Sparklines: { edit } });
  }

  openDialog(id: AnalysisDialog, arg?: unknown): void {
    this.model.set({ menu: null });
    this.store.setState({ dialog: { id, ...(arg !== undefined ? { arg } : {}) }, fieldMenu: null });
  }

  /** Closes the dialog and gives the keys back to the sheet. */
  closeDialog(): void {
    this.store.setState({ dialog: null });
    this.model.surface?.focus();
  }
}
