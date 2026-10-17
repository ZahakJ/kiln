/** Kiln Grid commands and ribbon: PLAN.md §1.4 "Grid" and the Grid column of §1.7. */
import {
  mhead,
  mitem,
  msep,
  type RibbonComboOption,
  type RibbonData,
  type RibbonMenuEntry,
  rbutton,
  rcolor,
  rcombo,
  rdropdown,
  rgroup,
  rrow,
  rsplit,
  rtab,
  rtoggle,
} from '../ribbon.ts';
import { c, GRID, section } from './define.ts';
import {
  clipboardGroup,
  FONT_FAMILY_OPTIONS,
  FONT_SIZE_OPTIONS,
  macrosGroup,
  pictureFormatTab,
  tableTabs,
  themeGroup,
  viewMode,
  zoomMenu,
} from './ribbon-shared.ts';

const G = { apps: GRID };

export const gridCommands = [
  ...section('Home ▸ Clipboard', G, [
    c('grid.clipboard.paste_values', 'Paste Values', 'PasteValues', {
      synonyms: ['paste values only', 'paste as values'],
    }),
    c('grid.clipboard.paste_formulas', 'Paste Formulas', 'PasteFormulas'),
    c('grid.clipboard.paste_formatting', 'Paste Formatting', 'PasteFormatting'),
    c('grid.clipboard.paste_transpose', 'Transpose', 'PasteTranspose', {
      synonyms: ['paste transpose', 'swap rows and columns'],
    }),
  ]),
  ...section('Home ▸ Font', G, [
    c('grid.font.borders', 'Borders', 'BordersAll', { keyTip: 'B' }),
    c('grid.font.border_outline', 'Outside Borders', 'BordersOutside', {
      synonyms: ['outline border', 'box border'],
    }),
    c('grid.font.border_none', 'No Border', 'BordersNone', { synonyms: ['remove borders'] }),
    c('grid.font.fill_color', 'Fill Color', 'PaintBucket', {
      keyTip: 'H',
      synonyms: ['background colour', 'cell colour', 'shading'],
    }),
  ]),
  ...section('Home ▸ Alignment', G, [
    c('grid.alignment.align_top', 'Top Align', 'AlignTop'),
    c('grid.alignment.align_middle', 'Middle Align', 'AlignMiddle'),
    c('grid.alignment.align_bottom', 'Bottom Align', 'AlignBottom'),
    c('grid.alignment.orientation', 'Orientation', 'TextOrientation', {
      synonyms: ['rotate text', 'angle text'],
    }),
    c('grid.alignment.wrap_text', 'Wrap Text', 'WrapText', { keyTip: 'W' }),
    c('grid.alignment.merge_center', 'Merge & Center', 'MergeCenter', {
      keyTip: 'M',
      synonyms: ['merge and centre', 'merge cells'],
    }),
    c('grid.alignment.merge_across', 'Merge Across', 'MergeAcross'),
    c('grid.alignment.merge_cells', 'Merge Cells', 'MergeCells'),
    c('grid.alignment.unmerge_cells', 'Unmerge Cells', 'UnmergeCells'),
  ]),
  ...section('Home ▸ Number', G, [
    c('grid.number.number_format', 'Number Format', 'NumberFormat', { keyTip: 'N' }),
    c('grid.number.number', 'Number Format: Number', 'Hash', { officeName: 'Number' }),
    c('grid.number.currency', 'Number Format: Currency', 'DollarSign', { officeName: 'Currency' }),
    c('grid.number.percent', 'Percent Style', 'PercentStyle', { synonyms: ['percentage'] }),
    c('grid.number.accounting', 'Accounting Number Format', 'AccountingFormat'),
    c('grid.number.comma', 'Comma Style', 'CommaStyle', { synonyms: ['thousands separator'] }),
    c('grid.number.increase_decimal', 'Increase Decimal', 'IncreaseDecimal'),
    c('grid.number.decrease_decimal', 'Decrease Decimal', 'DecreaseDecimal'),
  ]),
  ...section('Home ▸ Styles', G, [
    c('grid.styles.conditional_formatting', 'Conditional Formatting', 'ConditionalFormatting', {
      synonyms: ['data bars', 'colour scales', 'icon sets', 'highlight cells'],
    }),
    c('grid.styles.format_as_table', 'Format as Table', 'FormatAsTable', {
      synonyms: ['create table', 'insert table'],
    }),
    c('grid.styles.cell_styles', 'Cell Styles', 'CellStyles'),
  ]),
  ...section('Home ▸ Cells', G, [
    c('grid.cells.insert_cells', 'Insert Cells', 'InsertCells', {
      synonyms: ['insert row', 'insert column'],
    }),
    c('grid.cells.delete_cells', 'Delete Cells', 'DeleteCells', {
      synonyms: ['delete row', 'delete column'],
    }),
    c('grid.cells.format_cells', 'Format Cells…', 'FormatCells', {
      description: 'Number, Alignment, Font, Border, Fill and Protection settings.',
    }),
    c('grid.cells.row_height', 'Row Height', 'RowHeight'),
    c('grid.cells.column_width', 'Column Width', 'ColumnWidth'),
    c('grid.cells.hide_rows', 'Hide Rows', 'EyeOff'),
    c('grid.cells.hide_columns', 'Hide Columns', 'EyeOff'),
    c('grid.cells.unhide_rows', 'Unhide Rows', 'Eye'),
    c('grid.cells.unhide_columns', 'Unhide Columns', 'Eye'),
    c('grid.cells.new_sheet', 'Insert Sheet', 'Plus', {
      synonyms: ['new sheet', 'add sheet', 'new worksheet'],
    }),
    c('grid.cells.rename_sheet', 'Rename Sheet', 'PencilLine'),
    c('grid.cells.previous_sheet', 'Previous Sheet', 'ChevronLeft'),
    c('grid.cells.next_sheet', 'Next Sheet', 'ChevronRight'),
  ]),
  ...section('Home ▸ Editing', G, [
    c('grid.editing.auto_sum', 'AutoSum', 'AutoSum', { keyTip: 'U', synonyms: ['sum', 'total'] }),
    c('grid.editing.fill', 'Fill', 'FillDown'),
    c('grid.editing.fill_down', 'Fill Down', 'FillDown'),
    c('grid.editing.fill_right', 'Fill Right', 'ArrowRight'),
    c('grid.editing.fill_selection', 'Fill Selection with Entry', 'FillDown'),
    c('grid.editing.flash_fill', 'Flash Fill', 'FlashFill', { tier: 'P1' }),
    c('grid.editing.clear', 'Clear', 'ClearAll'),
    c('grid.editing.sort_filter', 'Sort & Filter', 'SortFilter'),
    c('grid.editing.find_select', 'Find & Select', 'FindSelect'),
    c('grid.editing.edit_cell', 'Edit Cell', 'PencilLine', { description: 'Edit the active cell in place.' }),
    c('grid.editing.cancel_edit', 'Cancel Edit', 'X', { palette: false }),
    c('grid.editing.new_line_in_cell', 'New Line in Cell', 'CornerDownLeft', { palette: false }),
    c('grid.editing.array_formula', 'Enter Array Formula', 'Braces', { palette: false }),
    c('grid.editing.toggle_absolute', 'Toggle Absolute Reference', 'DollarSign', {
      synonyms: ['$', 'absolute reference', 'relative reference', 'F4'],
    }),
    c('grid.editing.open_dropdown', 'Open List', 'ChevronDown', {
      palette: false,
      description: 'Open the filter or autocomplete list.',
    }),
    c('grid.editing.select_row', 'Select Row', 'Rows3'),
    c('grid.editing.select_column', 'Select Column', 'Columns3'),
  ]),
  ...section('Insert', G, [
    c('grid.insert.pivot_table', 'PivotTable', 'PivotTable', { tier: 'P1', synonyms: ['pivot'] }),
    c('grid.insert.table', 'Table', 'Table'),
    c('grid.insert.recommended_charts', 'Recommended Charts', 'RecommendedCharts'),
    c('grid.insert.chart_column', 'Column or Bar Chart', 'ChartColumn'),
    c('grid.insert.chart_line', 'Line or Area Chart', 'ChartLine'),
    c('grid.insert.chart_pie', 'Pie or Doughnut Chart', 'ChartPie'),
    c('grid.insert.chart_scatter', 'Scatter or Bubble Chart', 'ChartScatter'),
    c('grid.insert.chart_hierarchy', 'Hierarchy Chart', 'ChartHierarchy', {
      synonyms: ['treemap', 'sunburst'],
    }),
    c('grid.insert.chart_statistic', 'Statistic Chart', 'ChartStatistic', {
      synonyms: ['histogram', 'box and whisker'],
    }),
    c('grid.insert.chart_waterfall', 'Waterfall or Funnel Chart', 'ChartWaterfall', { tier: 'P1' }),
    c('grid.insert.chart_combo', 'Combo Chart', 'ChartCombo'),
    c('grid.insert.sparkline_line', 'Line Sparkline', 'SparklineLine'),
    c('grid.insert.sparkline_column', 'Column Sparkline', 'SparklineColumn'),
    c('grid.insert.sparkline_win_loss', 'Win/Loss Sparkline', 'SparklineWinLoss', { tier: 'P1' }),
    c('grid.insert.slicer', 'Slicer', 'Slicer', { tier: 'P2' }),
    c('grid.insert.timeline', 'Timeline', 'Timeline', { tier: 'P2' }),
    c('grid.review.new_note', 'New Note', 'NoteSticky', { synonyms: ['cell note', 'legacy comment'] }),
  ]),
  ...section('Page Layout', G, [
    c('grid.page_layout.themes', 'Themes', 'Palette', { tier: 'P1' }),
    c('grid.page_layout.print_area', 'Print Area', 'PrintArea'),
    c('grid.page_layout.print_titles', 'Print Titles', 'PrintTitles'),
    c('grid.page_layout.background', 'Background', 'ImagePlus'),
    c('grid.page_layout.scale_to_fit', 'Scale to Fit', 'ScaleToFit'),
    c('grid.page_layout.headings', 'Headings', 'Headings'),
  ]),
  ...section('Formulas', G, [
    c('grid.formulas.insert_function', 'Insert Function', 'InsertFunction', {
      synonyms: ['fx', 'function wizard'],
    }),
    c('grid.formulas.recently_used_functions', 'Recently Used', 'History'),
    c('grid.formulas.name_manager', 'Name Manager', 'NameManager', { synonyms: ['named ranges'] }),
    c('grid.formulas.define_name', 'Define Name', 'DefineName'),
    c('grid.formulas.create_from_selection', 'Create from Selection', 'DefineName'),
    c('grid.formulas.trace_precedents', 'Trace Precedents', 'TracePrecedents'),
    c('grid.formulas.trace_dependents', 'Trace Dependents', 'TraceDependents'),
    c('grid.formulas.remove_arrows', 'Remove Arrows', 'RemoveArrows'),
    c('grid.formulas.show_formulas', 'Show Formulas', 'ShowFormulas'),
    c('grid.formulas.error_checking', 'Error Checking', 'ErrorChecking'),
    c('grid.formulas.evaluate_formula', 'Evaluate Formula', 'EvaluateFormula'),
    c('grid.formulas.watch_window', 'Watch Window', 'WatchWindow', { tier: 'P1' }),
    c('grid.formulas.calculation_options', 'Calculation Options', 'CalculationOptions', {
      synonyms: ['manual calculation', 'automatic calculation'],
    }),
    c('grid.formulas.calculate_now', 'Calculate Now', 'CalculateNow', { synonyms: ['recalculate'] }),
    c('grid.formulas.calculate_sheet', 'Calculate Sheet', 'CalculateSheet'),
    c('grid.formulas.formula_lab', 'Formula Lab', 'FormulaLab', {
      description: 'See how a formula is evaluated, step by step.',
      synonyms: ['explain formula'],
    }),
  ]),
  ...section('Data', G, [
    c('grid.data.from_text_csv', 'From Text/CSV', 'FromTextCsv', { synonyms: ['import csv'] }),
    c('grid.data.from_json', 'From JSON', 'FromJson'),
    c('grid.data.refresh_all', 'Refresh All', 'RefreshAll'),
    c('grid.data.sort_ascending', 'Sort A to Z', 'SortAscending'),
    c('grid.data.sort_descending', 'Sort Z to A', 'SortDescending'),
    c('grid.data.sort', 'Sort…', 'ArrowDownUp'),
    c('grid.data.toggle_filter', 'Filter', 'FilterToggle', { synonyms: ['autofilter', 'toggle filter'] }),
    c('grid.data.clear_filter', 'Clear Filter', 'FunnelX'),
    c('grid.data.reapply_filter', 'Reapply', 'RefreshCw'),
    c('grid.data.text_to_columns', 'Text to Columns', 'TextToColumns', { synonyms: ['split column'] }),
    c('grid.data.remove_duplicates', 'Remove Duplicates', 'RemoveDuplicates'),
    c('grid.data.data_validation', 'Data Validation', 'DataValidation', { synonyms: ['dropdown list'] }),
    c('grid.data.consolidate', 'Consolidate', 'Consolidate', { tier: 'P2' }),
    c('grid.data.what_if', 'What-If Analysis', 'WhatIfAnalysis'),
    c('grid.data.goal_seek', 'Goal Seek', 'GoalSeek', { tier: 'P1' }),
    c('grid.data.forecast_sheet', 'Forecast Sheet', 'ForecastSheet', { tier: 'P2' }),
    c('grid.data.group_rows', 'Group', 'GroupRows'),
    c('grid.data.ungroup_rows', 'Ungroup', 'UngroupRows'),
    c('grid.data.subtotal', 'Subtotal', 'Subtotal', { tier: 'P1' }),
    c('grid.data.show_outline_symbols', 'Show Outline Symbols', 'ListTree'),
  ]),
  ...section('Review', G, [
    c('grid.review.protect_sheet', 'Protect Sheet', 'ProtectSheet', { tier: 'P1' }),
    c('grid.review.protect_workbook', 'Protect Workbook', 'ProtectWorkbook', { tier: 'P1' }),
  ]),
  ...section('View', G, [
    c('grid.view.formula_bar', 'Formula Bar', 'SquareFunction'),
    c('grid.view.freeze_panes', 'Freeze Panes', 'FreezePanes'),
    c('grid.view.freeze_top_row', 'Freeze Top Row', 'FreezeTopRow'),
    c('grid.view.freeze_first_column', 'Freeze First Column', 'FreezeFirstColumn'),
    c('grid.view.zoom_to_selection', 'Zoom to Selection', 'ZoomToSelection'),
  ]),
];

// ── Ribbon (PLAN.md §1.4 "Grid"; mockup docs/design/mockups/grid-*.png) ───────────────────────────

const NUMBER_FORMATS: readonly RibbonComboOption[] = [
  'General',
  'Number',
  'Currency',
  'Accounting',
  'Short Date',
  'Long Date',
  'Time',
  'Percentage',
  'Fraction',
  'Scientific',
  'Text',
].map((value) => ({ value }));

const BORDERS_MENU: readonly RibbonMenuEntry[] = [
  mitem('grid.font.borders', { label: 'All Borders', args: { borders: 'all' } }),
  mitem('grid.font.border_outline', { label: 'Outside Borders' }),
  mitem('grid.font.borders', { label: 'Thick Box Border', args: { borders: 'thickBox' } }),
  msep(),
  mitem('grid.font.borders', { label: 'Bottom Border', args: { borders: 'bottom' } }),
  mitem('grid.font.borders', { label: 'Top Border', args: { borders: 'top' } }),
  mitem('grid.font.borders', { label: 'Left Border', args: { borders: 'left' } }),
  mitem('grid.font.borders', { label: 'Right Border', args: { borders: 'right' } }),
  mitem('grid.font.border_none'),
  msep(),
  mitem('grid.cells.format_cells', { label: 'More Borders…', args: { tab: 'border' } }),
];

const CHARTS: readonly [string, string][] = [
  ['grid.insert.chart_column', 'C1'],
  ['grid.insert.chart_line', 'N1'],
  ['grid.insert.chart_pie', 'Q'],
  ['grid.insert.chart_scatter', 'D'],
  ['grid.insert.chart_hierarchy', 'I'],
  ['grid.insert.chart_statistic', 'S1'],
  ['grid.insert.chart_waterfall', 'W'],
  ['grid.insert.chart_combo', 'N2'],
];

/** Kiln Grid's ribbon. Chart Design and Chart Format come from `@kiln/ui-kit`'s `chartTabs` (WP-73). */
export const gridRibbon: RibbonData = {
  app: 'grid',
  tabs: [
    rtab(
      'home',
      'Home',
      [
        clipboardGroup([
          msep(),
          mitem('grid.clipboard.paste_values'),
          mitem('grid.clipboard.paste_formulas'),
          mitem('grid.clipboard.paste_formatting'),
          mitem('grid.clipboard.paste_transpose'),
        ]),
        rgroup(
          'font',
          'Font',
          [
            rrow(
              rcombo('suite.format.font_family', FONT_FAMILY_OPTIONS, {
                placeholder: 'Font',
                width: 132,
                keyTip: 'FF',
                allowCustomValue: true,
              }),
              rcombo('suite.format.font_size', FONT_SIZE_OPTIONS, {
                placeholder: 'Size',
                width: 56,
                keyTip: 'FS',
                allowCustomValue: true,
                accepts: 'number',
              }),
              rbutton('suite.format.grow_font', { keyTip: 'FG', hideInSimplified: true }),
              rbutton('suite.format.shrink_font', { keyTip: 'FK', hideInSimplified: true }),
            ),
            rrow(
              rtoggle('suite.format.bold', { keyTip: '1', priority: 5 }),
              rtoggle('suite.format.italic', { keyTip: '2', priority: 5 }),
              rtoggle('suite.format.underline', { keyTip: '3', priority: 5 }),
              rsplit('grid.font.borders', BORDERS_MENU, { size: 'small', keyTip: 'B' }),
              rcolor('grid.font.fill_color', { defaultColor: '#FFE066', allowNone: true, keyTip: 'H' }),
              rcolor('suite.format.font_color', { defaultColor: '#C8352B', allowAuto: true, keyTip: 'FC' }),
            ),
          ],
          {
            rows: 2,
            launcher: { command: 'grid.cells.format_cells', label: 'Format Cells: Font' },
            icon: 'Type',
            priority: 4,
          },
        ),
        rgroup(
          'alignment',
          'Alignment',
          [
            rrow(
              rtoggle('grid.alignment.align_top', { keyTip: 'AT', hideInSimplified: true }),
              rtoggle('grid.alignment.align_middle', { keyTip: 'AM', hideInSimplified: true }),
              rtoggle('grid.alignment.align_bottom', { keyTip: 'AB', hideInSimplified: true }),
              rtoggle('grid.alignment.wrap_text', { keyTip: 'W' }),
            ),
            rrow(
              rtoggle('suite.format.align_left', { keyTip: 'AL', priority: 3 }),
              rtoggle('suite.format.align_center', { keyTip: 'AC', priority: 3 }),
              rtoggle('suite.format.align_right', { keyTip: 'AR', priority: 3 }),
              rbutton('suite.format.outdent', { keyTip: '5' }),
              rbutton('suite.format.indent', { keyTip: '6' }),
              rdropdown(
                'grid.alignment.orientation',
                [
                  mitem('grid.alignment.orientation', {
                    label: 'Angle Counterclockwise',
                    args: { angle: 45 },
                  }),
                  mitem('grid.alignment.orientation', { label: 'Angle Clockwise', args: { angle: -45 } }),
                  mitem('grid.alignment.orientation', {
                    label: 'Vertical Text',
                    args: { angle: 'vertical' },
                  }),
                ],
                {
                  size: 'small',
                  keyTip: 'FQ',
                },
              ),
              rsplit(
                'grid.alignment.merge_center',
                [
                  mitem('grid.alignment.merge_center'),
                  mitem('grid.alignment.merge_across'),
                  mitem('grid.alignment.merge_cells'),
                  mitem('grid.alignment.unmerge_cells'),
                ],
                { size: 'small', keyTip: 'M' },
              ),
            ),
          ],
          {
            rows: 2,
            launcher: { command: 'grid.cells.format_cells', label: 'Format Cells: Alignment' },
            icon: 'AlignLeft',
            priority: 3,
          },
        ),
        rgroup(
          'number',
          'Number',
          [
            rrow(
              rcombo('grid.number.number_format', NUMBER_FORMATS, {
                width: 128,
                keyTip: 'N',
                placeholder: 'Number format',
              }),
            ),
            rrow(
              rsplit(
                'grid.number.accounting',
                [
                  mitem('grid.number.accounting', {
                    label: '£ English (United Kingdom)',
                    args: { currency: 'GBP' },
                  }),
                  mitem('grid.number.accounting', {
                    label: '$ English (United States)',
                    args: { currency: 'USD' },
                  }),
                  mitem('grid.number.accounting', { label: '€ Euro', args: { currency: 'EUR' } }),
                ],
                {
                  size: 'small',
                  keyTip: 'AN',
                },
              ),
              rbutton('grid.number.percent', { keyTip: 'P' }),
              rbutton('grid.number.comma', { keyTip: 'K' }),
              rbutton('grid.number.increase_decimal', { keyTip: '0' }),
              rbutton('grid.number.decrease_decimal', { keyTip: '9' }),
            ),
          ],
          {
            rows: 2,
            launcher: { command: 'grid.cells.format_cells', label: 'Format Cells: Number' },
            icon: 'Hash',
            priority: 2,
          },
        ),
        rgroup(
          'styles',
          'Styles',
          [
            rbutton('grid.styles.conditional_formatting', {
              size: 'medium',
              label: 'Conditional Formatting',
              keyTip: 'L',
            }),
            rbutton('grid.styles.format_as_table', { size: 'medium', keyTip: 'T' }),
            rbutton('grid.styles.cell_styles', { size: 'medium', keyTip: 'J' }),
          ],
          { icon: 'Palette', priority: 1 },
        ),
        rgroup(
          'cells',
          'Cells',
          [
            rsplit(
              'grid.cells.insert_cells',
              [mitem('grid.cells.insert_cells'), mitem('grid.cells.new_sheet')],
              { size: 'medium', label: 'Insert', keyTip: 'I' },
            ),
            rsplit('grid.cells.delete_cells', [mitem('grid.cells.delete_cells')], {
              size: 'medium',
              label: 'Delete',
              keyTip: 'D',
            }),
            rdropdown(
              'grid.cells.format_cells',
              [
                mhead('Cell Size'),
                mitem('grid.cells.row_height'),
                mitem('grid.cells.column_width'),
                mhead('Visibility'),
                mitem('grid.cells.hide_rows'),
                mitem('grid.cells.hide_columns'),
                mitem('grid.cells.unhide_rows'),
                mitem('grid.cells.unhide_columns'),
                mhead('Organize Sheets'),
                mitem('grid.cells.rename_sheet'),
                msep(),
                mitem('grid.cells.format_cells'),
              ],
              { size: 'medium', label: 'Format', keyTip: 'O' },
            ),
          ],
          { icon: 'TableCellsMerge', priority: 1 },
        ),
        rgroup(
          'editing',
          'Editing',
          [
            rsplit(
              'grid.editing.auto_sum',
              [
                mitem('grid.editing.auto_sum', { label: 'Sum', args: { fn: 'SUM' } }),
                mitem('grid.editing.auto_sum', { label: 'Average', args: { fn: 'AVERAGE' } }),
                mitem('grid.editing.auto_sum', { label: 'Count Numbers', args: { fn: 'COUNT' } }),
                mitem('grid.editing.auto_sum', { label: 'Max', args: { fn: 'MAX' } }),
                mitem('grid.editing.auto_sum', { label: 'Min', args: { fn: 'MIN' } }),
                msep(),
                mitem('grid.formulas.insert_function', { label: 'More Functions…' }),
              ],
              { size: 'medium', keyTip: 'U' },
            ),
            rdropdown(
              'grid.editing.sort_filter',
              [
                mitem('grid.data.sort_ascending'),
                mitem('grid.data.sort_descending'),
                mitem('grid.data.sort', { label: 'Custom Sort…' }),
                msep(),
                mitem('grid.data.toggle_filter'),
                mitem('grid.data.clear_filter'),
                mitem('grid.data.reapply_filter'),
              ],
              { size: 'medium', label: 'Sort & Filter', keyTip: 'S' },
            ),
            rdropdown(
              'grid.editing.find_select',
              [mitem('suite.edit.find'), mitem('suite.edit.replace'), mitem('suite.edit.go_to')],
              { size: 'medium', label: 'Find', keyTip: 'FD' },
            ),
            rdropdown(
              'grid.editing.fill',
              [
                mitem('grid.editing.fill_down'),
                mitem('grid.editing.fill_right'),
                mitem('grid.editing.flash_fill'),
              ],
              { size: 'small', keyTip: 'FI' },
            ),
            rbutton('grid.editing.clear', { size: 'small', keyTip: 'E' }),
          ],
          { icon: 'Sigma', priority: 1 },
        ),
        rgroup(
          'kiln',
          'Kiln',
          [
            rbutton('grid.formulas.formula_lab', {
              size: 'large',
              label: 'Explain',
              icon: 'Sparkles',
              keyTip: 'ZL',
            }),
          ],
          {
            icon: 'Sparkles',
          },
        ),
      ],
      { keyTip: 'H' },
    ),
    rtab(
      'insert',
      'Insert',
      [
        rgroup('tables', 'Tables', [
          rbutton('grid.insert.pivot_table', { size: 'large', keyTip: 'V', tier: 'P1' }),
          rbutton('grid.insert.table', { size: 'large', keyTip: 'T' }),
        ]),
        rgroup('illustrations', 'Illustrations', [
          rbutton('suite.insert.pictures', { size: 'large', keyTip: 'P' }),
          rbutton('suite.insert.shapes', { size: 'medium', keyTip: 'SH', tier: 'P1' }),
          rbutton('suite.insert.icons', { size: 'medium', keyTip: 'Y1' }),
        ]),
        rgroup(
          'charts',
          'Charts',
          [
            rbutton('grid.insert.recommended_charts', { size: 'large', keyTip: 'R' }),
            rbutton('suite.chart.insert', { size: 'large', keyTip: 'C' }),
            ...CHARTS.map(([id, keyTip]) => rbutton(id, { size: 'small' as const, keyTip })),
          ],
          { launcher: { command: 'grid.insert.recommended_charts', label: 'See All Charts' } },
        ),
        rgroup(
          'sparklines',
          'Sparklines',
          [
            rbutton('grid.insert.sparkline_line', { size: 'medium', keyTip: 'SL' }),
            rbutton('grid.insert.sparkline_column', { size: 'medium', keyTip: 'SO' }),
            rbutton('grid.insert.sparkline_win_loss', { size: 'medium', keyTip: 'SW' }),
          ],
          { tier: 'P1' },
        ),
        rgroup(
          'filters',
          'Filters',
          [
            rbutton('grid.insert.slicer', { size: 'large', keyTip: 'SF' }),
            rbutton('grid.insert.timeline', { size: 'large', keyTip: 'ST' }),
          ],
          { tier: 'P2' },
        ),
        rgroup('links', 'Links', [rbutton('suite.insert.link', { size: 'large', keyTip: 'I' })]),
        rgroup('comments', 'Comments', [
          rbutton('suite.review.new_comment', { size: 'large', label: 'Comment', keyTip: 'M' }),
          rbutton('grid.review.new_note', { size: 'medium', keyTip: 'N' }),
        ]),
        rgroup('text', 'Text', [
          rbutton('suite.insert.text_box', { size: 'medium', keyTip: 'X' }),
          rbutton('suite.insert.header_footer', { size: 'medium', keyTip: 'H' }),
          rbutton('suite.insert.word_art', { size: 'medium', keyTip: 'W', tier: 'P2' }),
        ]),
        rgroup('symbols', 'Symbols', [
          rbutton('suite.insert.equation', { size: 'medium', keyTip: 'E', tier: 'P2' }),
          rbutton('suite.insert.symbol', { size: 'medium', keyTip: 'U' }),
        ]),
      ],
      { keyTip: 'N' },
    ),
    rtab(
      'pageLayout',
      'Page Layout',
      [
        rgroup('themes', 'Themes', [rbutton('grid.page_layout.themes', { size: 'large', keyTip: 'TH' })], {
          tier: 'P1',
        }),
        rgroup(
          'pageSetup',
          'Page Setup',
          [
            rbutton('suite.layout.margins', { size: 'large', keyTip: 'M' }),
            rbutton('suite.layout.orientation', { size: 'large', keyTip: 'O' }),
            rbutton('suite.layout.size', { size: 'large', keyTip: 'SZ' }),
            rbutton('grid.page_layout.print_area', { size: 'medium', keyTip: 'R' }),
            rbutton('suite.layout.breaks', { size: 'medium', keyTip: 'B' }),
            rbutton('grid.page_layout.background', { size: 'medium', keyTip: 'G' }),
            rbutton('grid.page_layout.print_titles', { size: 'medium', keyTip: 'I' }),
          ],
          { launcher: { command: 'suite.layout.page_setup' } },
        ),
        rgroup('scaleToFit', 'Scale to Fit', [
          rbutton('grid.page_layout.scale_to_fit', { size: 'large', keyTip: 'SC' }),
        ]),
        rgroup('sheetOptions', 'Sheet Options', [
          rtoggle('suite.view.gridlines', { size: 'small', keyTip: 'VG' }),
          rtoggle('grid.page_layout.headings', { size: 'small', keyTip: 'VH' }),
        ]),
        rgroup('arrange', 'Arrange', [
          rbutton('suite.arrange.bring_forward', { size: 'medium', keyTip: 'AF' }),
          rbutton('suite.arrange.send_backward', { size: 'medium', keyTip: 'AE' }),
          rbutton('suite.arrange.selection_pane', { size: 'medium', keyTip: 'AP' }),
          rbutton('suite.arrange.align', { size: 'small', keyTip: 'AA' }),
          rbutton('suite.arrange.group', { size: 'small', keyTip: 'G2' }),
          rbutton('suite.arrange.rotate', { size: 'small', keyTip: 'AY' }),
        ]),
      ],
      { keyTip: 'P' },
    ),
    rtab(
      'formulas',
      'Formulas',
      [
        rgroup('functionLibrary', 'Function Library', [
          rbutton('grid.formulas.insert_function', { size: 'large', keyTip: 'F' }),
          rbutton('grid.editing.auto_sum', { size: 'large', keyTip: 'U' }),
          rbutton('grid.formulas.recently_used_functions', { size: 'large', keyTip: 'E' }),
          ...(
            [
              ['Financial', 'I', 'Financial'],
              ['Logical', 'L', 'Logical'],
              ['Text', 'T', 'Text'],
              ['Date & Time', 'E2', 'Date & Time'],
              ['Lookup & Reference', 'O', 'Lookup & Reference'],
              ['Math & Trig', 'G', 'Math & Trig'],
            ] as const
          ).map(([label, keyTip, category]) =>
            rbutton('grid.formulas.insert_function', {
              id: `fn.${keyTip}`,
              label,
              size: 'small' as const,
              keyTip,
              args: { category },
            }),
          ),
          rdropdown(
            'grid.formulas.insert_function',
            ['Statistical', 'Engineering', 'Information'].map((category) =>
              mitem('grid.formulas.insert_function', { label: category, args: { category } }),
            ),
            { id: 'fn.more', label: 'More Functions', size: 'small', keyTip: 'Q' },
          ),
        ]),
        rgroup('definedNames', 'Defined Names', [
          rbutton('grid.formulas.name_manager', { size: 'large', keyTip: 'N' }),
          rbutton('grid.formulas.define_name', { size: 'medium', keyTip: 'M' }),
          rbutton('grid.formulas.create_from_selection', { size: 'medium', keyTip: 'C' }),
        ]),
        rgroup('auditing', 'Formula Auditing', [
          rbutton('grid.formulas.trace_precedents', { size: 'medium', keyTip: 'P' }),
          rbutton('grid.formulas.trace_dependents', { size: 'medium', keyTip: 'D' }),
          rbutton('grid.formulas.remove_arrows', { size: 'medium', keyTip: 'A' }),
          rtoggle('grid.formulas.show_formulas', { size: 'medium', keyTip: 'H' }),
          rbutton('grid.formulas.error_checking', { size: 'medium', keyTip: 'K' }),
          rbutton('grid.formulas.evaluate_formula', { size: 'medium', keyTip: 'V' }),
          rbutton('grid.formulas.watch_window', { size: 'large', keyTip: 'W', tier: 'P1' }),
        ]),
        rgroup('calculation', 'Calculation', [
          rbutton('grid.formulas.calculation_options', { size: 'large', keyTip: 'X' }),
          rbutton('grid.formulas.calculate_now', { size: 'medium', keyTip: 'B' }),
          rbutton('grid.formulas.calculate_sheet', { size: 'medium', keyTip: 'J' }),
        ]),
        rgroup('formulaLab', 'Formula Lab', [
          rbutton('grid.formulas.formula_lab', { size: 'large', keyTip: 'ZL' }),
        ]),
      ],
      { keyTip: 'M' },
    ),
    rtab(
      'data',
      'Data',
      [
        rgroup('getTransform', 'Get & Transform Data', [
          rbutton('grid.data.from_text_csv', { size: 'large', keyTip: 'FT' }),
          rbutton('grid.data.from_json', { size: 'medium', keyTip: 'FJ' }),
          rbutton('grid.data.refresh_all', { size: 'large', keyTip: 'RA' }),
        ]),
        rgroup('sortFilter', 'Sort & Filter', [
          rbutton('grid.data.sort_ascending', { size: 'small', keyTip: 'SA' }),
          rbutton('grid.data.sort_descending', { size: 'small', keyTip: 'SD' }),
          rbutton('grid.data.sort', { size: 'large', keyTip: 'SS' }),
          rtoggle('grid.data.toggle_filter', { size: 'large', keyTip: 'T' }),
          rbutton('grid.data.clear_filter', { size: 'small', keyTip: 'C' }),
          rbutton('grid.data.reapply_filter', { size: 'small', keyTip: 'Y' }),
        ]),
        rgroup('dataTools', 'Data Tools', [
          rbutton('grid.data.text_to_columns', { size: 'large', keyTip: 'E' }),
          rbutton('grid.editing.flash_fill', { size: 'small', keyTip: 'FF' }),
          rbutton('grid.data.remove_duplicates', { size: 'small', keyTip: 'M' }),
          rbutton('grid.data.data_validation', { size: 'small', keyTip: 'V' }),
          rbutton('grid.data.consolidate', { size: 'small', keyTip: 'N', tier: 'P2' }),
        ]),
        rgroup('forecast', 'Forecast', [
          rdropdown('grid.data.what_if', [mitem('grid.data.goal_seek')], { size: 'large', keyTip: 'W' }),
          rbutton('grid.data.forecast_sheet', { size: 'large', keyTip: 'FC', tier: 'P2' }),
        ]),
        rgroup('outline', 'Outline', [
          rbutton('grid.data.group_rows', { size: 'medium', keyTip: 'G' }),
          rbutton('grid.data.ungroup_rows', { size: 'medium', keyTip: 'U' }),
          rbutton('grid.data.subtotal', { size: 'medium', keyTip: 'B', tier: 'P1' }),
          rtoggle('grid.data.show_outline_symbols', { size: 'small', keyTip: 'J' }),
        ]),
      ],
      { keyTip: 'A' },
    ),
    rtab(
      'review',
      'Review',
      [
        rgroup('proofing', 'Proofing', [rbutton('suite.review.spelling', { size: 'large', keyTip: 'S' })]),
        rgroup('comments', 'Comments', [
          rbutton('suite.review.new_comment', { size: 'large', label: 'New Comment', keyTip: 'C' }),
          rbutton('suite.review.delete_comment', { size: 'medium', keyTip: 'D' }),
          rbutton('suite.review.previous_comment', { size: 'medium', keyTip: 'V' }),
          rbutton('suite.review.next_comment', { size: 'medium', keyTip: 'N' }),
          rtoggle('suite.review.show_comments', { size: 'medium', keyTip: 'H' }),
        ]),
        rgroup('notes', 'Notes', [rbutton('grid.review.new_note', { size: 'large', keyTip: 'T' })]),
        rgroup(
          'protect',
          'Protect',
          [
            rbutton('grid.review.protect_sheet', { size: 'large', keyTip: 'PS' }),
            rbutton('grid.review.protect_workbook', { size: 'large', keyTip: 'PW' }),
          ],
          { tier: 'P1' },
        ),
        rgroup('history', 'History', [
          rbutton('suite.file.version_history', { size: 'large', keyTip: 'ZV' }),
        ]),
      ],
      { keyTip: 'R' },
    ),
    rtab(
      'view',
      'View',
      [
        rgroup('workbookViews', 'Workbook Views', [
          viewMode('Normal', 'Normal', 'NormalView', 'L'),
          viewMode('PageBreakPreview', 'Page Break Preview', 'PageBreakPreview', 'I'),
          viewMode('PageLayout', 'Page Layout', 'PageLayoutView', 'P'),
        ]),
        rgroup('show', 'Show', [
          rtoggle('suite.view.ruler', { size: 'small', keyTip: 'R' }),
          rtoggle('suite.view.gridlines', { size: 'small', keyTip: 'VG' }),
          rtoggle('grid.view.formula_bar', { size: 'small', keyTip: 'F' }),
          rtoggle('grid.page_layout.headings', { size: 'small', keyTip: 'H' }),
        ]),
        rgroup('zoom', 'Zoom', [
          zoomMenu(),
          rbutton('suite.view.zoom', {
            id: 'view.zoom100',
            size: 'large',
            label: '100%',
            icon: 'Zoom100',
            keyTip: 'J',
            args: { zoom: 1 },
          }),
          rbutton('grid.view.zoom_to_selection', { size: 'large', keyTip: 'G' }),
        ]),
        rgroup('window', 'Window', [
          rbutton('suite.view.new_window', { size: 'medium', keyTip: 'N' }),
          rsplit(
            'grid.view.freeze_panes',
            [
              mitem('grid.view.freeze_panes'),
              mitem('grid.view.freeze_top_row'),
              mitem('grid.view.freeze_first_column'),
            ],
            { size: 'large', keyTip: 'F' },
          ),
          rbutton('suite.view.split', { size: 'small', keyTip: 'S' }),
          rbutton('suite.view.side_by_side', { size: 'small', keyTip: 'B' }),
        ]),
        themeGroup(),
        macrosGroup(),
      ],
      { keyTip: 'W' },
    ),
    ...tableTabs('grid'),
    pictureFormatTab(),
  ],
};
