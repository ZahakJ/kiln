/**
 * The Grid editor's chrome commands: the ribbon and keyboard ids that open a dialog or the find
 * bar, toggle the formula bar or fit the zoom to the selection (PLAN.md §1.4). They are
 * presentation — every change to the workbook is a core command the dialogs run — and they
 * register on the window's command registry while the editor is mounted.
 */
import type { CommandDefinition, Disposable } from '@kiln/commands';
import type { GridDialog, GridEditorModel } from './model.ts';

interface DialogSpec {
  id: string;
  title: string;
  dialog: GridDialog;
  tab?: string;
}

/** Ids that open one of the editor's dialogs. */
export const GRID_DIALOG_COMMANDS: readonly DialogSpec[] = [
  { id: 'grid.cells.format_cells', title: 'Format Cells…', dialog: 'formatCells' },
  { id: 'ui.grid.format_cells', title: 'Format Cells…', dialog: 'formatCells' },
  { id: 'suite.edit.go_to', title: 'Go To', dialog: 'goTo' },
  { id: 'suite.edit.paste_special', title: 'Paste Special…', dialog: 'pasteSpecial' },
  { id: 'ui.grid.insert_cells', title: 'Insert Cells…', dialog: 'insertCells' },
  { id: 'ui.grid.delete_cells', title: 'Delete Cells…', dialog: 'deleteCells' },
  { id: 'ui.grid.row_height', title: 'Row Height…', dialog: 'rowHeight' },
  { id: 'ui.grid.column_width', title: 'Column Width…', dialog: 'columnWidth' },
  { id: 'ui.grid.rename_sheet', title: 'Rename Sheet', dialog: 'renameSheet' },
  { id: 'ui.grid.tab_color', title: 'Tab Color…', dialog: 'tabColor' },
  { id: 'grid.formulas.insert_function', title: 'Insert Function', dialog: 'insertFunction' },
];

const str = (args: unknown, key: string): string | undefined => {
  if (typeof args !== 'object' || args === null || !(key in args)) return undefined;
  const v = (args as Record<string, unknown>)[key];
  return typeof v === 'string' ? v : undefined;
};

/** Registers the editor's chrome commands; dispose to remove them. */
export function registerGridUi(model: GridEditorModel): Disposable {
  const reg = model.registry;
  const defs: CommandDefinition<unknown>[] = GRID_DIALOG_COMMANDS.map((d) => ({
    id: d.id,
    title: d.title,
    run: (_ctx: unknown, args: unknown) => {
      model.openDialog(d.dialog, str(args, 'tab') ?? str(args, 'category') ?? d.tab);
    },
  }));
  const formulaBar = (id: string): CommandDefinition<unknown> => ({
    id,
    title: 'Formula Bar',
    isChecked: () => model.state.showFormulaBar,
    run: () => model.set({ showFormulaBar: !model.state.showFormulaBar }),
  });
  defs.push(
    formulaBar('grid.view.formula_bar'),
    formulaBar('ui.grid.formula_bar'),
    {
      id: 'suite.edit.find',
      title: 'Find',
      run: () => model.set({ find: { ...model.state.find, open: true } }),
    },
    {
      // Esc on the sheet (§1.7): closes the find bar, else clears the copy marquee.
      id: 'ui.edit.cancel',
      title: 'Cancel',
      run: async () => {
        if (model.state.find.open) model.set({ find: { ...model.state.find, open: false } });
        else await model.core('grid.editing.cancel_edit');
        model.surface?.focus();
      },
    },
    {
      id: 'ui.grid.delete_sheet',
      title: 'Delete Sheet',
      run: async () => {
        const g = model.state.grid;
        if (g) await model.run('grid.sheet.delete', { index: g.active_sheet });
      },
    },
    {
      id: 'grid.view.zoom_to_selection',
      title: 'Zoom to Selection',
      run: async () => {
        const size = model.surface?.size();
        if (!size) return;
        const r = await model.query({ ZoomToSelection: { width: size.width, height: size.height } });
        if (r && 'Zoom' in r) await model.run('suite.view.zoom', { zoom: r.Zoom });
      },
    },
  );
  // A declaration another editor in the window already implements stays.
  const disposables: Disposable[] = [];
  for (const d of defs) {
    const state = reg.getState(d.id);
    if (state.exists && !state.pending) continue;
    disposables.push(reg.register(d));
  }
  reg.setContext({ editor: 'grid' });
  return {
    dispose() {
      for (const d of disposables) d.dispose();
      reg.setContext({ editor: undefined });
    },
  };
}
