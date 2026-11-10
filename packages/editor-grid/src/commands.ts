/**
 * The Grid editor's chrome commands: the ribbon and keyboard ids that open a dialog or the find
 * bar, toggle the formula bar or fit the zoom to the selection. They are
 * presentation — every change to the workbook is a core command the dialogs run — and they
 * register on the window's command registry while the editor is mounted.
 */

import type { CommandDefinition, Disposable } from '@kiln/commands';
import { msg } from '@kiln/i18n';
import type { ChartKind } from '@kiln/protocol';
import { loadUserLists } from './home/customLists.ts';
import { sendCustomLists } from './home/HomeDialogs.tsx';
import type { GridDialog, GridEditorModel } from './model.ts';

interface DialogSpec {
  id: string;
  title: string;
  dialog: GridDialog;
  tab?: string;
}

/** Ids that open one of the editor's dialogs. */
export const GRID_DIALOG_COMMANDS: readonly DialogSpec[] = [
  { id: 'grid.cells.format_cells', title: msg('Format Cells…'), dialog: 'formatCells' },
  { id: 'ui.grid.format_cells', title: msg('Format Cells…'), dialog: 'formatCells' },
  { id: 'suite.edit.go_to', title: msg('Go To'), dialog: 'goTo' },
  { id: 'suite.edit.paste_special', title: msg('Paste Special…'), dialog: 'pasteSpecial' },
  { id: 'ui.grid.insert_cells', title: msg('Insert Cells…'), dialog: 'insertCells' },
  { id: 'ui.grid.delete_cells', title: msg('Delete Cells…'), dialog: 'deleteCells' },
  { id: 'ui.grid.row_height', title: msg('Row Height…'), dialog: 'rowHeight' },
  { id: 'ui.grid.column_width', title: msg('Column Width…'), dialog: 'columnWidth' },
  { id: 'ui.grid.rename_sheet', title: msg('Rename Sheet'), dialog: 'renameSheet' },
  { id: 'ui.grid.tab_color', title: msg('Tab Color…'), dialog: 'tabColor' },
  { id: 'ui.grid.move_copy_sheet', title: msg('Move or Copy Sheet…'), dialog: 'moveCopySheet' },
  { id: 'ui.grid.unhide_sheet', title: msg('Unhide Sheet…'), dialog: 'unhideSheet' },
  { id: 'grid.formulas.insert_function', title: msg('Insert Function'), dialog: 'insertFunction' },
  { id: 'grid.insert.recommended_charts', title: msg('Recommended Charts'), dialog: 'recommendedCharts' },
  { id: 'ui.grid.fill_series', title: msg('Series…'), dialog: 'fillSeries' },
  { id: 'ui.grid.custom_lists', title: msg('Edit Custom Lists…'), dialog: 'customLists' },
];

/** Insert ▸ Charts buttons: a chart of the selection in the first type of each family. */
export const GRID_CHART_BUTTONS: readonly { id: string; title: string; kind: ChartKind }[] = [
  { id: 'grid.insert.chart_column', title: msg('Column or Bar Chart'), kind: 'ColumnClustered' },
  { id: 'grid.insert.chart_line', title: msg('Line or Area Chart'), kind: 'Line' },
  { id: 'grid.insert.chart_pie', title: msg('Pie or Doughnut Chart'), kind: 'Pie' },
  { id: 'grid.insert.chart_scatter', title: msg('Scatter or Bubble Chart'), kind: 'ScatterMarkers' },
  { id: 'grid.insert.chart_hierarchy', title: msg('Hierarchy Chart'), kind: 'Treemap' },
  { id: 'grid.insert.chart_statistic', title: msg('Statistic Chart'), kind: 'Histogram' },
  { id: 'grid.insert.chart_waterfall', title: msg('Waterfall or Funnel Chart'), kind: 'Waterfall' },
  { id: 'grid.insert.chart_combo', title: msg('Combo Chart'), kind: 'ComboColumnLine' },
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
  for (const b of GRID_CHART_BUTTONS) {
    defs.push({
      id: b.id,
      title: b.title,
      run: () => model.chart({ ChartInsert: { kind: b.kind } }),
    });
  }
  defs.push({
    id: 'ui.grid.format_chart',
    title: msg('Format Chart Area'),
    run: () => model.set({ chartPane: true }),
  });
  const formulaBar = (id: string): CommandDefinition<unknown> => ({
    id,
    title: msg('Formula Bar'),
    isChecked: () => model.state.showFormulaBar,
    run: () => model.set({ showFormulaBar: !model.state.showFormulaBar }),
  });
  defs.push(
    formulaBar('grid.view.formula_bar'),
    formulaBar('ui.grid.formula_bar'),
    {
      // View ▸ Ruler: Page Layout view's rulers (Excel offers the box in that view only).
      id: 'suite.view.ruler',
      title: msg('Ruler'),
      isEnabled: () => model.state.status?.view === 'PageLayout',
      isChecked: () => model.state.showRuler,
      run: () => model.set({ showRuler: !model.state.showRuler }),
    },
    // Dropdowns whose entries are the commands (Home ▸ Fill, Data ▸ What-If Analysis): the
    // button itself only opens its menu.
    { id: 'grid.editing.fill', title: msg('Fill'), run: () => undefined },
    { id: 'grid.data.what_if', title: msg('What-If Analysis'), run: () => undefined },
    {
      id: 'suite.edit.find',
      title: msg('Find'),
      run: () => model.set({ find: { ...model.state.find, open: true } }),
    },
    {
      // Esc on the sheet: closes the find bar, else clears the copy marquee.
      id: 'ui.edit.cancel',
      title: msg('Cancel'),
      run: async () => {
        if (model.state.find.open) model.set({ find: { ...model.state.find, open: false } });
        else if (model.state.grid?.home?.pen) await model.core('grid.font.border_pen_off');
        else await model.core('grid.editing.cancel_edit');
        model.surface?.focus();
      },
    },
    {
      id: 'ui.grid.hide_sheet',
      title: msg('Hide Sheet'),
      run: async () => {
        const g = model.state.grid;
        if (g) await model.run('grid.sheet.hide', { index: g.active_sheet });
      },
    },
    {
      id: 'ui.grid.duplicate_sheet',
      title: msg('Duplicate Sheet'),
      run: async () => {
        const g = model.state.grid;
        if (g) await model.run('grid.sheet.duplicate', { index: g.active_sheet });
      },
    },
    {
      id: 'ui.grid.delete_sheet',
      title: msg('Delete Sheet'),
      run: async () => {
        const g = model.state.grid;
        if (g) await model.run('grid.sheet.delete', { index: g.active_sheet });
      },
    },
    {
      id: 'grid.view.zoom_to_selection',
      title: msg('Zoom to Selection'),
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
  // The user's custom fill lists reach the core with the workbook.
  const lists = loadUserLists();
  if (lists.length > 0) void sendCustomLists(model, lists);
  return {
    dispose() {
      for (const d of disposables) d.dispose();
      reg.setContext({ editor: undefined });
    },
  };
}
