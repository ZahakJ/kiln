/**
 * The chrome side of Kiln Grid's analysis features: the ribbon ids that open the
 * PivotTable, Sparklines, Goal Seek, Subtotal and protection dialogs, the PivotTable Analyze,
 * PivotTable Design and Sparkline tabs, the Themes gallery, Lock Cell, and double-click on a
 * PivotTable value (Show Details). Presentation only: every change is a core command.
 */

import type { CommandDefinition, Disposable } from '@kiln/commands';
import { GRID_THEMES } from '@kiln/commands';
import { msg } from '@kiln/i18n';
import type { GridPivotOptions, GridSparkOptions, GridThemePart } from '@kiln/protocol';
import { selectionRanges } from '../features/model.ts';
import { rangeA1 } from '../features/ui.tsx';
import type { GridEditorModel } from '../model.ts';
import { analysisOf, gridAnalysis } from './model.ts';

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null;
const arg = <T>(args: unknown, key: string): T | undefined =>
  isRecord(args) && key in args ? (args[key] as T) : undefined;

/** A colour argument of a colour picker (`#RRGGBB`, `{ hex }`). */
const colourOf = (args: unknown): string | null => {
  const v = arg<unknown>(args, 'color') ?? arg<unknown>(args, 'value');
  const raw = isRecord(v) && typeof v.hex === 'string' ? v.hex : v;
  return typeof raw === 'string' && /^#?[0-9a-f]{6}$/i.test(raw) ? raw.replace(/^#/, '').toUpperCase() : null;
};

/** One value object per colour: the ribbon compares command values by identity. */
const RGB_VALUES = new Map<string, { type: 'rgb'; hex: string }>();
function rgbValue(hex: string): { type: 'rgb'; hex: string } {
  let v = RGB_VALUES.get(hex);
  if (!v) {
    v = { type: 'rgb', hex: `#${hex}` };
    RGB_VALUES.set(hex, v);
  }
  return v;
}

/** Ids declared as reservations that this module implements. */
const CHROME = new Set([
  'grid.insert.pivot_table',
  'grid.insert.sparkline_line',
  'grid.insert.sparkline_column',
  'grid.insert.sparkline_win_loss',
  'grid.data.goal_seek',
  'grid.data.subtotal',
  'grid.review.protect_sheet',
  'grid.review.protect_workbook',
  'grid.page_layout.themes',
]);

/** Registers the analysis commands; dispose to remove them. */
export function registerGridAnalysis(model: GridEditorModel): Disposable {
  const a = analysisOf(model);
  const reg = model.registry;
  const pivot = () => a.pivot();
  const spark = () => gridAnalysis(model)?.sparkline ?? null;
  const prot = () => gridAnalysis(model);
  const options = (patch: Partial<GridPivotOptions>) => {
    const p = pivot();
    if (!p) return;
    return a.edit({ Options: { options: { ...p.options, ...patch } } });
  };
  const pivotOption = (
    id: string,
    title: string,
    checked: (o: GridPivotOptions) => boolean,
    patch: Partial<GridPivotOptions>,
  ): CommandDefinition<unknown> => ({
    id,
    title,
    isEnabled: () => !!pivot(),
    isChecked: () => {
      const p = pivot();
      return !!p && checked(p.options);
    },
    run: () => options(patch),
  });
  const sparkOptions = (patch: Partial<GridSparkOptions>) => {
    const s = spark();
    if (!s) return;
    return a.spark({ Options: { options: { ...s.options, ...patch } } });
  };
  const sparkFlag = (
    id: string,
    title: string,
    key: 'high' | 'low' | 'negative' | 'first' | 'last' | 'markers' | 'show_axis' | 'right_to_left',
  ): CommandDefinition<unknown> => ({
    id,
    title,
    isEnabled: () => !!spark(),
    isChecked: () => !!spark()?.options[key],
    run: () => {
      const s = spark();
      if (s) return sparkOptions({ [key]: !s.options[key] });
    },
  });
  const sparkKind = (
    id: string,
    title: string,
    kind: GridSparkOptions['kind'],
  ): CommandDefinition<unknown> => ({
    id,
    title,
    isEnabled: () => !!spark(),
    isChecked: () => spark()?.options.kind === kind,
    run: () => sparkOptions({ kind }),
  });
  const theme = (part: GridThemePart, args: unknown) => {
    const id = arg<string>(args, 'theme') ?? arg<string>(args, 'value');
    if (id) return a.exec({ SetTheme: { theme: id, part } });
  };
  const insertSparklines = (
    kind: GridSparkOptions['kind'],
    id: string,
    title: string,
  ): CommandDefinition<unknown> => ({
    id,
    title,
    run: () => a.openDialog('sparklines', kind),
  });
  const defs: CommandDefinition<unknown>[] = [
    {
      id: 'grid.insert.pivot_table',
      title: msg('PivotTable'),
      run: () => a.openDialog('createPivot'),
    },
    insertSparklines('Line', 'grid.insert.sparkline_line', msg('Line Sparkline')),
    insertSparklines('Column', 'grid.insert.sparkline_column', msg('Column Sparkline')),
    insertSparklines('WinLoss', 'grid.insert.sparkline_win_loss', msg('Win/Loss Sparkline')),
    { id: 'grid.data.goal_seek', title: msg('Goal Seek'), run: () => a.openDialog('goalSeek') },
    { id: 'grid.data.subtotal', title: msg('Subtotal'), run: () => a.openDialog('subtotal') },
    {
      id: 'grid.review.protect_sheet',
      title: msg('Protect Sheet'),
      isChecked: () => !!prot()?.sheet_protected,
      run: async () => {
        const s = prot();
        if (!s?.sheet_protected) {
          a.openDialog('protectSheet');
          return;
        }
        if (s.sheet_password) a.openDialog('unprotectSheet');
        else await a.exec({ ProtectSheet: { protect: false, password: '', allow: null } });
      },
    },
    {
      id: 'grid.review.protect_workbook',
      title: msg('Protect Workbook'),
      isChecked: () => !!prot()?.workbook_protected,
      run: async () => {
        const s = prot();
        if (!s?.workbook_protected) {
          a.openDialog('protectWorkbook');
          return;
        }
        if (s.workbook_password) a.openDialog('unprotectWorkbook');
        else await a.exec({ ProtectWorkbook: { protect: false, password: '' } });
      },
    },
    {
      id: 'ui.grid.lock_cell',
      title: msg('Lock Cell'),
      isChecked: () => !!prot()?.locked,
      run: () => a.exec({ LockCells: { locked: !prot()?.locked } }),
    },
    {
      id: 'grid.page_layout.themes',
      title: msg('Themes'),
      value: () => {
        const name = gridAnalysis(model)?.theme ?? '';
        const t = GRID_THEMES.find((x) => x.name === name || (x.id === 'office' && name === 'Office Theme'));
        return t?.id;
      },
      run: (_ctx, args) => theme('All', args),
    },
    { id: 'ui.grid.theme_colors', title: msg('Colors'), run: (_ctx, args) => theme('Colors', args) },
    { id: 'ui.grid.theme_fonts', title: msg('Fonts'), run: (_ctx, args) => theme('Fonts', args) },
    // ── PivotTable Analyze ──
    {
      id: 'ui.grid.pivot_fields',
      title: msg('Field List'),
      isEnabled: () => !!pivot(),
      isChecked: () => !!pivot() && !a.state.fieldsClosed,
      run: () => {
        a.set({ fieldsClosed: !a.state.fieldsClosed });
        if (a.state.fieldsClosed) model.surface?.focus();
      },
    },
    {
      id: 'ui.grid.pivot_name',
      title: msg('PivotTable Name'),
      isEnabled: () => !!pivot(),
      value: () => pivot()?.name,
      run: (_ctx, args) => {
        const name = arg<string>(args, 'value');
        if (name?.trim()) return a.edit({ Rename: { name } });
      },
    },
    {
      id: 'ui.grid.pivot_options',
      title: msg('Options'),
      isEnabled: () => !!pivot(),
      run: () => a.openDialog('pivotOptions'),
    },
    {
      id: 'ui.grid.pivot_field_settings',
      title: msg('Field Settings'),
      isEnabled: () => {
        const p = pivot();
        return !!p && (p.active_value !== null || p.active_field !== null);
      },
      run: () => {
        const p = pivot();
        if (!p) return;
        if (p.active_value !== null) a.openDialog('valueSettings', p.active_value);
        else if (p.active_field !== null) a.openDialog('fieldSettings', p.active_field);
      },
    },
    {
      id: 'ui.grid.pivot_refresh_all',
      title: msg('Refresh All'),
      run: () => a.exec({ RefreshPivots: { all: true } }),
    },
    {
      id: 'ui.grid.pivot_change_source',
      title: msg('Change Data Source'),
      isEnabled: () => !!pivot(),
      run: () => a.openDialog('pivotSource'),
    },
    {
      id: 'ui.grid.pivot_clear_all',
      title: msg('Clear All'),
      isEnabled: () => !!pivot(),
      run: () => a.edit('ClearAll'),
    },
    {
      id: 'ui.grid.pivot_clear_filters',
      title: msg('Clear Filters'),
      isEnabled: () => !!pivot(),
      run: () => a.edit('ClearFilters'),
    },
    {
      id: 'ui.grid.pivot_select',
      title: msg('Entire PivotTable'),
      isEnabled: () => !!pivot(),
      run: async () => {
        const p = pivot();
        if (p) await a.exec({ SelectRange: { range: p.range, add: false } });
        model.surface?.focus();
      },
    },
    {
      id: 'ui.grid.pivot_move',
      title: msg('Move PivotTable'),
      isEnabled: () => !!pivot(),
      run: () => a.openDialog('pivotMove'),
    },
    {
      id: 'ui.grid.pivot_delete',
      title: msg('Delete PivotTable'),
      isEnabled: () => !!pivot(),
      run: () => a.edit('Delete'),
    },
    {
      id: 'ui.grid.pivot_show_details',
      title: msg('Show Details'),
      isEnabled: () => !!pivot()?.on_value,
      run: async () => {
        const r = selectionRanges(model).at(-1);
        if (r) await a.exec({ PivotDetails: { row: r.r0, col: r.c0 } });
      },
    },
    // ── PivotTable Design ──
    pivotOption('ui.grid.pivot_subtotals_off', msg('Do Not Show Subtotals'), (o) => o.subtotals === 'Off', {
      subtotals: 'Off',
    }),
    pivotOption(
      'ui.grid.pivot_subtotals_bottom',
      msg('Show All Subtotals at Bottom of Group'),
      (o) => o.subtotals === 'Bottom',
      { subtotals: 'Bottom' },
    ),
    pivotOption(
      'ui.grid.pivot_subtotals_top',
      msg('Show All Subtotals at Top of Group'),
      (o) => o.subtotals === 'Top',
      {
        subtotals: 'Top',
      },
    ),
    pivotOption(
      'ui.grid.pivot_totals_off',
      msg('Off for Rows and Columns'),
      (o) => !o.row_totals && !o.col_totals,
      { row_totals: false, col_totals: false },
    ),
    pivotOption(
      'ui.grid.pivot_totals_on',
      msg('On for Rows and Columns'),
      (o) => o.row_totals && o.col_totals,
      {
        row_totals: true,
        col_totals: true,
      },
    ),
    // Excel's "rows only" shows the totals of the rows: the total column.
    pivotOption('ui.grid.pivot_totals_rows', msg('On for Rows Only'), (o) => o.row_totals && !o.col_totals, {
      row_totals: true,
      col_totals: false,
    }),
    pivotOption(
      'ui.grid.pivot_totals_columns',
      msg('On for Columns Only'),
      (o) => !o.row_totals && o.col_totals,
      {
        row_totals: false,
        col_totals: true,
      },
    ),
    pivotOption('ui.grid.pivot_layout_compact', msg('Show in Compact Form'), (o) => o.layout === 'Compact', {
      layout: 'Compact',
    }),
    pivotOption('ui.grid.pivot_layout_outline', msg('Show in Outline Form'), (o) => o.layout === 'Outline', {
      layout: 'Outline',
    }),
    pivotOption('ui.grid.pivot_layout_tabular', msg('Show in Tabular Form'), (o) => o.layout === 'Tabular', {
      layout: 'Tabular',
    }),
    pivotOption('ui.grid.pivot_repeat_labels', msg('Repeat All Item Labels'), (o) => o.repeat_labels, {
      repeat_labels: true,
    }),
    pivotOption('ui.grid.pivot_no_repeat_labels', msg('Do Not Repeat Item Labels'), (o) => !o.repeat_labels, {
      repeat_labels: false,
    }),
    pivotOption('ui.grid.pivot_blank_rows', msg('Insert Blank Line after Each Item'), (o) => o.blank_rows, {
      blank_rows: true,
    }),
    pivotOption(
      'ui.grid.pivot_no_blank_rows',
      msg('Remove Blank Line after Each Item'),
      (o) => !o.blank_rows,
      {
        blank_rows: false,
      },
    ),
    ...(
      [
        ['ui.grid.pivot_row_headers', msg('Row Headers'), 'row_headers'],
        ['ui.grid.pivot_col_headers', msg('Column Headers'), 'col_headers'],
        ['ui.grid.pivot_banded_rows', msg('Banded Rows'), 'row_stripes'],
        ['ui.grid.pivot_banded_cols', msg('Banded Columns'), 'col_stripes'],
      ] as const
    ).map(
      ([id, title, key]): CommandDefinition<unknown> => ({
        id,
        title,
        isEnabled: () => !!pivot(),
        isChecked: () => !!pivot()?.options[key],
        run: () => {
          const p = pivot();
          if (p) return options({ [key]: !p.options[key] });
        },
      }),
    ),
    {
      id: 'ui.grid.pivot_style',
      title: msg('PivotTable Styles'),
      isEnabled: () => !!pivot(),
      value: () => pivot()?.options.style,
      run: (_ctx, args) => {
        const style = arg<string>(args, 'value');
        if (style !== undefined) return options({ style });
      },
    },
    // ── Sparkline ──
    {
      id: 'ui.grid.spark_edit_data',
      title: msg('Edit Data'),
      isEnabled: () => !!spark(),
      run: () => a.openDialog('sparkData'),
    },
    {
      id: 'ui.grid.spark_hidden_empty',
      title: msg('Hidden & Empty Cells…'),
      isEnabled: () => !!spark(),
      run: () => a.openDialog('sparkHidden'),
    },
    sparkKind('ui.grid.spark_line', msg('Line'), 'Line'),
    sparkKind('ui.grid.spark_column', msg('Column'), 'Column'),
    sparkKind('ui.grid.spark_win_loss', msg('Win/Loss'), 'WinLoss'),
    sparkFlag('ui.grid.spark_high', msg('High Point'), 'high'),
    sparkFlag('ui.grid.spark_low', msg('Low Point'), 'low'),
    sparkFlag('ui.grid.spark_negative', msg('Negative Points'), 'negative'),
    sparkFlag('ui.grid.spark_first', msg('First Point'), 'first'),
    sparkFlag('ui.grid.spark_last', msg('Last Point'), 'last'),
    sparkFlag('ui.grid.spark_markers', msg('Markers'), 'markers'),
    sparkFlag('ui.grid.spark_show_axis', msg('Show Axis'), 'show_axis'),
    sparkFlag('ui.grid.spark_right_to_left', msg('Plot Data Right-to-Left'), 'right_to_left'),
    {
      id: 'ui.grid.spark_axis_same_min',
      title: msg('Same for All Sparklines (minimum)'),
      isEnabled: () => !!spark(),
      isChecked: () => spark()?.options.min === 'Group',
      run: () => sparkOptions({ min: spark()?.options.min === 'Group' ? 'Individual' : 'Group' }),
    },
    {
      id: 'ui.grid.spark_axis_same_max',
      title: msg('Same for All Sparklines (maximum)'),
      isEnabled: () => !!spark(),
      isChecked: () => spark()?.options.max === 'Group',
      run: () => sparkOptions({ max: spark()?.options.max === 'Group' ? 'Individual' : 'Group' }),
    },
    {
      id: 'ui.grid.spark_style',
      title: msg('Sparkline Style'),
      isEnabled: () => !!spark(),
      run: (_ctx, args) => {
        const v = Number(arg<string>(args, 'value'));
        if (Number.isInteger(v) && v >= 0) return a.spark({ Style: { index: v } });
      },
    },
    {
      id: 'ui.grid.spark_color',
      title: msg('Sparkline Color'),
      isEnabled: () => !!spark(),
      value: () => {
        const c = spark()?.options.colors[0];
        return c ? rgbValue(c) : undefined;
      },
      run: (_ctx, args) => {
        const s = spark();
        const c = colourOf(args);
        if (!s || !c) return;
        const colors = [...s.options.colors];
        colors[0] = c;
        return sparkOptions({ colors });
      },
    },
    {
      id: 'ui.grid.spark_marker_color',
      title: msg('Marker Color'),
      isEnabled: () => !!spark(),
      run: (_ctx, args) => {
        const which = arg<string>(args, 'which') ?? 'markers';
        a.openDialog('sparkColor', which);
      },
    },
    {
      id: 'ui.grid.spark_group',
      title: msg('Group'),
      isEnabled: () => !!spark(),
      run: () => a.spark('Group'),
    },
    {
      id: 'ui.grid.spark_ungroup',
      title: msg('Ungroup'),
      isEnabled: () => (spark()?.count ?? 0) > 1,
      run: () => a.spark('Ungroup'),
    },
    {
      id: 'ui.grid.spark_clear',
      title: msg('Clear Selected Sparklines'),
      isEnabled: () => !!spark(),
      run: () => a.spark({ Clear: { groups: false } }),
    },
    {
      id: 'ui.grid.spark_clear_group',
      title: msg('Clear Selected Sparkline Groups'),
      isEnabled: () => !!spark(),
      run: () => a.spark({ Clear: { groups: true } }),
    },
  ];
  const disposables: Disposable[] = [];
  for (const d of defs) {
    // The core's own commands stay the core's (connectCore registers them).
    const st = reg.getState(d.id);
    if (st.exists && !st.pending && !d.id.startsWith('ui.') && !CHROME.has(d.id)) continue;
    disposables.push(reg.register(d));
  }
  // Double-click on a PivotTable value: its source rows on a new sheet (Show Details).
  const onDouble = async (cell: { row: number; col: number }): Promise<boolean> => {
    const g = await model.fetch();
    const p = g?.analysis.pivot;
    if (!p) return false;
    if (p.on_value) await a.exec({ PivotDetails: { row: cell.row, col: cell.col } });
    return true;
  };
  model.cellDoubleClickHooks.push(onDouble);
  // A refused change: Excel's message box (each refusal once); a new PivotTable opens its
  // field list.
  let seq = model.state.grid?.analysis.alert_seq ?? 0;
  let pivotName: string | null = null;
  const off = model.onState((s) => {
    const msg = s.analysis.alert;
    if (msg && s.analysis.alert_seq !== seq && !a.state.dialog) a.openDialog('alert', msg);
    seq = s.analysis.alert_seq;
    const name = s.analysis.pivot?.name ?? null;
    if (name !== pivotName) {
      pivotName = name;
      if (name) a.set({ fieldsClosed: false });
    }
  });
  return {
    dispose() {
      off();
      model.cellDoubleClickHooks.splice(model.cellDoubleClickHooks.indexOf(onDouble), 1);
      for (const d of disposables) d.dispose();
    },
  };
}

/** The selection as A1 text (the first range of a Create PivotTable or Sparklines dialog). */
export function selectionText(model: GridEditorModel): string {
  const r = selectionRanges(model).at(-1);
  return r ? rangeA1(r) : '';
}
