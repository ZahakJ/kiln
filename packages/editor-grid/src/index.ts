/**
 * @kiln/editor-grid — Kiln Grid's editor views (PLAN.md §1.3, §1.4): the sheet surface (Rust
 * tiles, DOM overlays for the input proxy and the accessibility mirror), the formula bar with the
 * name box, fx and expand, the in-cell editor, formula autocomplete with inline docs, the Format
 * Cells, Go To, Paste Special, Insert/Delete Cells, Row Height, Column Width, Tab Color and Insert
 * Function dialogs, the sheet tab strip with its menu, the find bar, the context menus and the
 * status bar.
 *
 * Presentation only (§3.7): every edit is a core command, and everything shown is an answer of the
 * core's `Query::Grid`.
 */
import type { CommandRegistry } from '@kiln/commands';
import type { DocId, KilnClient } from '@kiln/protocol';
import { useEffect, useState } from 'react';
import { registerGridUi } from './commands.ts';
import { GridEditorModel } from './model.ts';

export const PACKAGE_NAME = '@kiln/editor-grid' as const;

export { Autocomplete, hintParts } from './Autocomplete.tsx';
export { GRID_CHART_BUTTONS, GRID_DIALOG_COMMANDS, registerGridUi } from './commands.ts';
export { cursorFor, type GridElements, GridSurfaceController, panes } from './controller.ts';
export { CATEGORIES, dashOf, FormatCellsDialog } from './dialogs/FormatCellsDialog.tsx';
export { FindBar } from './FindBar.tsx';
export { colourRuns, FormulaBar } from './FormulaBar.tsx';
export { FormatChartArea, RecommendedChartsForSelection, useChartHost, useChartTabs } from './GridCharts.tsx';
export { GridDialogs } from './GridDialogs.tsx';
export { GridEditor, type GridEditorProps } from './GridEditor.tsx';
export { GridMenus, gridMenuEntries } from './GridMenus.tsx';
export { GridStatusBar, modeItems, statItems } from './GridStatusBar.tsx';
export { GridSurface, type GridSurfaceProps } from './GridSurface.tsx';
export { useGrid } from './hooks.ts';
export { gridKeyAction, type KeyAction, typesText } from './keys.ts';
export {
  type GridDialog,
  GridEditorModel,
  type GridMenu,
  type GridModelDeps,
  type GridSurfaceHandle,
  type GridUiState,
  gridResult,
  messageOf,
} from './model.ts';
export { SheetTabs } from './SheetTabs.tsx';

/**
 * The editor model of a workbook, with the editor's chrome commands registered on the window's
 * registry for as long as the component is mounted (`null` until a workbook is shown).
 */
export function useGridEditorModel(
  deps: { client: KilnClient; doc: DocId; registry: CommandRegistry } | null,
): GridEditorModel | null {
  const client = deps?.client ?? null;
  const doc = deps?.doc ?? null;
  const registry = deps?.registry ?? null;
  const [model, setModel] = useState<GridEditorModel | null>(null);
  useEffect(() => {
    if (!client || doc === null || !registry) {
      setModel(null);
      return;
    }
    const m = new GridEditorModel({ client, doc, registry });
    const ui = registerGridUi(m);
    setModel(m);
    return () => {
      ui.dispose();
      m.dispose();
      setModel(null);
    };
  }, [client, doc, registry]);
  return model;
}
