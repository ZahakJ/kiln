/** Kiln Grid commands and ribbon, and the Grid column of the shortcut table. */

import type {
  CellShift,
  ClipboardData,
  Command,
  GridAutoFillChoice,
  GridBorders,
  GridClear,
  GridCommand,
  GridDateUnit,
  GridDirection,
  GridFill,
  GridFormat,
  GridHAlign,
  GridLineStyle,
  GridPaste,
  GridPasteOp,
  GridPenMode,
  GridSeries,
  GridSeriesType,
} from '@kiln/protocol';
import {
  mhead,
  mitem,
  msep,
  msub,
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
import type { CommandContext } from '../types.ts';
import { c, GRID, section } from './define.ts';
import {
  FORMAT_PROTECTION_MENU,
  GRID_ANALYSIS_BUILDERS,
  gridPivotAnalyzeTab,
  gridPivotDesignTab,
  gridSparklineTab,
  gridThemesGroup,
} from './grid-analysis.ts';
import {
  BREAKS_MENU,
  CALCULATION_MENU,
  CELL_STYLES_MENU,
  CF_MENU,
  EXPORT_MENU,
  FIND_SELECT_MENU,
  GRID_FEATURE_BUILDERS,
  GRID_FEATURE_STATE,
  GROUP_MENU,
  gridPictureFormatTab,
  gridShapeFormatTab,
  gridTableDesignTab,
  LINK_MENU,
  MARGINS_MENU,
  NAMES_MENU,
  NOTES_MENU,
  ORIENTATION_MENU,
  PRINT_AREA_MENU,
  REMOVE_ARROWS_MENU,
  SCALE_MENU,
  SHAPES_MENU,
  SIZE_MENU,
  SORT_FILTER_MENU,
  UNGROUP_MENU,
  VALIDATION_MENU,
} from './grid-features.ts';
import { GRID_FUNCTION_MENUS } from './grid-functions.ts';
import { GRID_SURFACE_BUILDERS } from './grid-surface.ts';
import {
  accessibilityGroup,
  clipboardGroup,
  FONT_FAMILY_OPTIONS,
  FONT_SIZE_OPTIONS,
  macrosGroup,
  themeGroup,
  viewMode,
  zoomMenu,
} from './ribbon-shared.ts';

const G = { apps: GRID };

/** A Function Library menu: the category's functions (each opens Function Arguments). */
const functionMenu = (category: string): RibbonMenuEntry[] => [
  ...(GRID_FUNCTION_MENUS[category] ?? []).map((name) =>
    mitem('ui.grid.function_args', { label: name, args: { name } }),
  ),
  msep(),
  mitem('grid.formulas.insert_function', { label: 'Insert Function…', args: { category } }),
];

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
    c('grid.clipboard.paste_link', 'Paste Link', 'Link', {
      synonyms: ['link to source', 'paste reference'],
      description: 'Paste formulas that refer to the copied cells.',
    }),
  ]),
  ...section('Home ▸ Editing ▸ Fill', G, [
    c('ui.grid.fill_series', 'Series…', 'List', {
      synonyms: ['fill series', 'number series', 'date series', 'step value', 'stop value'],
      description: 'Fill the selection with a linear, growth or date series, with a step and a stop value.',
    }),
    c('grid.editing.fill_series', 'Fill Series', 'List', { palette: false }),
    c('grid.editing.auto_fill_option', 'Auto Fill Options', undefined, { palette: false }),
    c('ui.grid.custom_lists', 'Edit Custom Lists…', 'ListTree', {
      synonyms: ['custom list', 'fill list', 'sort list'],
      description: 'Lists the fill handle continues and the Sort dialog offers.',
    }),
    c('grid.editing.custom_lists', 'Set Custom Lists', 'ListTree', { palette: false }),
    c('grid.editing.import_custom_list', 'Import Custom List', undefined, { palette: false }),
  ]),
  ...section('Home ▸ Font ▸ Borders', G, [
    c('grid.font.border_pen', 'Draw Border', 'PencilLine', {
      synonyms: ['border pen', 'draw borders', 'erase border', 'draw border grid'],
      description: 'Draw borders with the pen: a drag draws round the cells, a click the edge you point at.',
    }),
    c('grid.font.border_pen_off', 'Put the Border Pen Away', undefined, { palette: false }),
    c('grid.font.draw_border', 'Border Pen Stroke', undefined, { palette: false }),
  ]),
  ...section('Home ▸ Number', G, [
    c('grid.number.delete_format', 'Delete Number Format', undefined, { palette: false }),
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
    c('grid.editing.flash_fill', 'Flash Fill', 'FlashFill'),
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
    c('grid.insert.pivot_table', 'PivotTable', 'PivotTable', { synonyms: ['pivot'] }),
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
    c('grid.insert.sparkline_win_loss', 'Win/Loss Sparkline', 'SparklineWinLoss'),
    c('grid.insert.slicer', 'Slicer', 'Slicer', { tier: 'P2' }),
    c('grid.insert.timeline', 'Timeline', 'Timeline', { tier: 'P2' }),
    c('grid.review.new_note', 'New Note', 'NoteSticky', { synonyms: ['cell note', 'legacy comment'] }),
  ]),
  ...section('Page Layout', G, [
    c('grid.page_layout.themes', 'Themes', 'Palette'),
    c('grid.page_layout.print_area', 'Print Area', 'PrintArea'),
    c('grid.page_layout.print_titles', 'Print Titles', 'PrintTitles'),
    c('grid.page_layout.background', 'Background', 'ImagePlus'),
    c('grid.page_layout.scale_to_fit', 'Scale to Fit', 'ScaleToFit'),
    c('grid.page_layout.headings', 'Headings', 'Headings'),
    c('grid.page_layout.sheet_right_to_left', 'Sheet Right-to-Left', 'TextDirection', { keyTip: 'SR' }),
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
    c('grid.formulas.watch_window', 'Watch Window', 'WatchWindow'),
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
    c('grid.data.goal_seek', 'Goal Seek', 'GoalSeek'),
    c('grid.data.forecast_sheet', 'Forecast Sheet', 'ForecastSheet', { tier: 'P2' }),
    c('grid.data.group_rows', 'Group', 'GroupRows'),
    c('grid.data.ungroup_rows', 'Ungroup', 'UngroupRows'),
    c('grid.data.subtotal', 'Subtotal', 'Subtotal'),
    c('grid.data.show_outline_symbols', 'Show Outline Symbols', 'ListTree'),
  ]),
  ...section('Review', G, [
    c('grid.review.protect_sheet', 'Protect Sheet', 'ProtectSheet'),
    c('grid.review.protect_workbook', 'Protect Workbook', 'ProtectWorkbook'),
  ]),
  ...section('Grid editing', G, [
    c('grid.nav.move', 'Move', undefined, { palette: false }),
    c('grid.nav.jump', 'Jump to the Edge of the Data', undefined, { palette: false }),
    c('grid.nav.page', 'Page Up or Down', undefined, { palette: false }),
    c('grid.nav.row_start', 'Go to the Start of the Row', undefined, { palette: false }),
    c('grid.nav.advance', 'Next Cell', undefined, { palette: false }),
    c('grid.nav.select_cell', 'Select Cell', undefined, { palette: false }),
    c('grid.nav.select_range', 'Select Range', undefined, { palette: false }),
    c('grid.nav.select_rows', 'Select Rows', undefined, { palette: false }),
    c('grid.nav.select_columns', 'Select Columns', undefined, { palette: false }),
    c('grid.nav.select_all', 'Select All', 'LassoSelect', {
      keyTip: 'SA',
      synonyms: ['current region', 'select sheet'],
    }),
    c('grid.nav.go_to', 'Go To', 'Crosshair', { palette: false }),
    c('grid.sheet.activate', 'Show Sheet', undefined, { palette: false }),
    c('grid.sheet.delete', 'Delete Sheet', 'X', { palette: false }),
    c('grid.sheet.move', 'Move Sheet', undefined, { palette: false }),
    c('grid.sheet.duplicate', 'Duplicate Sheet', 'Copy', { palette: false }),
    c('grid.sheet.tab_color', 'Tab Color', 'Palette', { palette: false }),
    c('grid.sheet.hide', 'Hide Sheet', 'EyeOff', { palette: false }),
    c('grid.sheet.unhide', 'Unhide Sheet', 'Eye', { palette: false }),
    c('grid.editing.begin', 'Start Typing', undefined, { palette: false }),
    c('grid.editing.put_back_draft', 'Put it back', undefined, { palette: false }),
    c('grid.editing.set_text', 'Edit Text', undefined, { palette: false }),
    c('grid.editing.commit', 'Enter', 'CornerDownLeft', { palette: false }),
    c('grid.editing.accept_completion', 'Accept Suggestion', undefined, { palette: false }),
    c('grid.editing.move_completion', 'Next Suggestion', undefined, { palette: false }),
    c('grid.editing.point_ref', 'Point at a Cell', undefined, { palette: false }),
    c('grid.editing.point_move', 'Point with the Arrows', undefined, { palette: false }),
    c('grid.editing.clear_contents', 'Clear Contents', 'Eraser', { synonyms: ['delete contents'] }),
    c('grid.editing.fill_up', 'Fill Up', 'ArrowUpToLine'),
    c('grid.editing.fill_left', 'Fill Left', 'ArrowLeftToLine'),
    c('grid.editing.drag_fill', 'Drag the Fill Handle', undefined, { palette: false }),
    c('grid.editing.auto_fill', 'Fill Series', 'Repeat', { synonyms: ['autofill', 'series'] }),
    c('grid.editing.insert_date_time', 'Insert Date or Time', 'CalendarDays', { palette: false }),
    c('grid.editing.find_next', 'Find Next', 'Search', { palette: false }),
    c('grid.cells.insert_rows', 'Insert Sheet Rows', 'BetweenHorizontalStart'),
    c('grid.cells.insert_columns', 'Insert Sheet Columns', 'BetweenVerticalStart'),
    c('grid.cells.delete_rows', 'Delete Sheet Rows', 'Rows3'),
    c('grid.cells.delete_columns', 'Delete Sheet Columns', 'Columns3'),
    c('grid.cells.resize_rows', 'Resize Rows', undefined, { palette: false }),
    c('grid.cells.resize_columns', 'Resize Columns', undefined, { palette: false }),
    c('grid.cells.autofit_rows', 'AutoFit Row Height', 'MoveVertical'),
    c('grid.cells.autofit_columns', 'AutoFit Column Width', 'MoveHorizontal'),
    c('grid.alignment.horizontal', 'Horizontal Alignment', undefined, { palette: false }),
    c('grid.format.apply', 'Apply Cell Format', undefined, { palette: false }),
    c('grid.view.split', 'Split', 'Columns3', { keyTip: 'S' }),
    c('grid.view.gridlines', 'Gridlines', 'Table', { keyTip: 'VG' }),
    c('grid.clipboard.paste_special', 'Paste Special…', 'Copy', { synonyms: ['paste special'] }),
    c('ui.grid.format_cells', 'Format Cells…', 'FormatCells', { palette: false }),
    c('ui.grid.row_height', 'Row Height…', 'RowHeight'),
    c('ui.grid.column_width', 'Column Width…', 'ColumnWidth'),
    c('ui.grid.rename_sheet', 'Rename Sheet', 'PencilLine'),
    c('ui.grid.tab_color', 'Tab Color…', 'Palette', { palette: false }),
    c('ui.grid.move_copy_sheet', 'Move or Copy Sheet…', 'Copy', { officeName: 'Move or Copy Sheet' }),
    c('ui.grid.unhide_sheet', 'Unhide Sheet…', 'Eye', { officeName: 'Unhide Sheet' }),
    c('ui.grid.hide_sheet', 'Hide Sheet', 'EyeOff'),
    c('ui.grid.duplicate_sheet', 'Duplicate Sheet', 'Copy', { synonyms: ['copy sheet', 'move or copy'] }),
    c('ui.grid.insert_cells', 'Insert Cells…', 'InsertCells', { palette: false }),
    c('ui.grid.delete_cells', 'Delete Cells…', 'DeleteCells', { palette: false }),
    c('ui.grid.delete_sheet', 'Delete Sheet', 'X', { synonyms: ['remove sheet', 'delete worksheet'] }),
    c('ui.grid.formula_bar', 'Formula Bar', 'SquareFunction', { palette: false }),
    // Charts on the sheet: selection, move/resize and delete come from the
    // surface; the theme is the chrome's.
    c('grid.drawing.select', 'Select Chart', undefined, { palette: false }),
    c('grid.drawing.place', 'Move or Resize Chart', undefined, { palette: false }),
    c('grid.drawing.delete', 'Delete Chart', 'X', { palette: false }),
    c('grid.view.theme', 'Theme', undefined, { palette: false }),
    c('ui.grid.format_chart', 'Format Chart Area', 'ChartBarIncreasing', { palette: false }),
  ]),
  ...section('View', G, [
    c('grid.view.formula_bar', 'Formula Bar', 'SquareFunction'),
    c('grid.view.freeze_panes', 'Freeze Panes', 'FreezePanes'),
    c('grid.view.freeze_top_row', 'Freeze Top Row', 'FreezeTopRow'),
    c('grid.view.freeze_first_column', 'Freeze First Column', 'FreezeFirstColumn'),
    c('grid.view.zoom_to_selection', 'Zoom to Selection', 'ZoomToSelection'),
  ]),
];

// ── Ribbon (mockup docs/design/mockups/grid-*.png) ───────────────────────────

// The value is the name the core matches; the label is the interface message.
const NUMBER_FORMATS: readonly RibbonComboOption[] = [
  { value: 'General', label: 'General' },
  { value: 'Number', label: 'Number' },
  { value: 'Currency', label: 'Currency' },
  { value: 'Accounting', label: 'Accounting' },
  { value: 'Short Date', label: 'Short Date' },
  { value: 'Long Date', label: 'Long Date' },
  { value: 'Time', label: 'Time' },
  { value: 'Percentage', label: 'Percentage' },
  { value: 'Fraction', label: 'Fraction' },
  { value: 'Scientific', label: 'Scientific' },
  { value: 'Text', label: 'Text' },
];

const PEN_COLORS: readonly [string, string][] = [
  ['Black', '000000'],
  ['Dark Red', 'C00000'],
  ['Red', 'FF0000'],
  ['Orange', 'FFC000'],
  ['Yellow', 'FFFF00'],
  ['Green', '00B050'],
  ['Blue', '0070C0'],
  ['Dark Blue', '002060'],
  ['Purple', '7030A0'],
  ['Clay', 'C8352B'],
];

const PEN_STYLES: readonly [GridLineStyle, string][] = [
  ['Thin', 'Thin'],
  ['Hair', 'Hair'],
  ['Dotted', 'Dotted'],
  ['Dashed', 'Dashed'],
  ['DashDot', 'Dash Dot'],
  ['DashDotDot', 'Dash Dot Dot'],
  ['Medium', 'Medium'],
  ['MediumDashed', 'Medium Dashed'],
  ['Thick', 'Thick'],
  ['Double', 'Double'],
];

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
  mhead('Draw Borders'),
  mitem('grid.font.border_pen', { label: 'Draw Border', args: { mode: 'Draw' }, icon: 'PencilLine' }),
  mitem('grid.font.border_pen', { label: 'Draw Border Grid', args: { mode: 'Grid' }, icon: 'Table' }),
  mitem('grid.font.border_pen', { label: 'Erase Border', args: { mode: 'Erase' }, icon: 'Eraser' }),
  msub(
    'Line Color',
    [
      mitem('grid.font.border_pen', { label: 'Automatic', args: { color: '' } }),
      ...PEN_COLORS.map(([label, color]) => mitem('grid.font.border_pen', { label, args: { color } })),
    ],
    'Palette',
  ),
  msub(
    'Line Style',
    PEN_STYLES.map(([style, label]) => mitem('grid.font.border_pen', { label, args: { style } })),
    'Minus',
  ),
  msep(),
  mitem('grid.cells.format_cells', { label: 'More Borders…', args: { tab: 'border' } }),
];

const CHARTS: readonly [string, string][] = [
  ['grid.insert.chart_column', 'C1'],
  ['grid.insert.chart_line', 'N1'],
  ['grid.insert.chart_pie', 'Q'],
  ['grid.insert.chart_scatter', 'D'],
  ['grid.insert.chart_hierarchy', ''],
  ['grid.insert.chart_statistic', 'S1'],
  ['grid.insert.chart_waterfall', 'W'],
  ['grid.insert.chart_combo', 'N2'],
];

/** Kiln Grid's ribbon. Chart Design and Chart Format come from `@kiln/ui-kit`'s `chartTabs`. */
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
          mitem('grid.clipboard.paste_link'),
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
            launcher: { command: 'grid.cells.format_cells', label: 'Format Cells: Font', keyTip: 'FN' },
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
            launcher: { command: 'grid.cells.format_cells', label: 'Format Cells: Alignment', keyTip: 'FA' },
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
            launcher: { command: 'grid.cells.format_cells', label: 'Format Cells: Number', keyTip: 'FM' },
            icon: 'Hash',
            priority: 2,
          },
        ),
        rgroup(
          'styles',
          'Styles',
          [
            rdropdown(undefined, CF_MENU, {
              id: 'styles.cf',
              size: 'medium',
              label: 'Conditional Formatting',
              icon: 'ConditionalFormatting',
              keyTip: 'L',
            }),
            rbutton('grid.styles.format_as_table', { size: 'medium', keyTip: 'T' }),
            rdropdown(undefined, CELL_STYLES_MENU, {
              id: 'styles.cell',
              size: 'medium',
              label: 'Cell Styles',
              icon: 'CellStyles',
              keyTip: 'J',
            }),
          ],
          { icon: 'Palette', priority: 1 },
        ),
        rgroup(
          'cells',
          'Cells',
          [
            rsplit(
              'grid.cells.insert_cells',
              [
                mitem('ui.grid.insert_cells', { label: 'Insert Cells…' }),
                mitem('grid.cells.insert_rows'),
                mitem('grid.cells.insert_columns'),
                mitem('grid.cells.new_sheet'),
              ],
              { size: 'medium', label: 'Insert', keyTip: 'I' },
            ),
            rsplit(
              'grid.cells.delete_cells',
              [
                mitem('ui.grid.delete_cells', { label: 'Delete Cells…' }),
                mitem('grid.cells.delete_rows'),
                mitem('grid.cells.delete_columns'),
                mitem('ui.grid.delete_sheet', { label: 'Delete Sheet' }),
              ],
              {
                size: 'medium',
                label: 'Delete',
                keyTip: 'D',
              },
            ),
            rdropdown(
              'grid.cells.format_cells',
              [
                mhead('Cell Size'),
                mitem('ui.grid.row_height', { label: 'Row Height…' }),
                mitem('grid.cells.autofit_rows', { label: 'AutoFit Row Height' }),
                mitem('ui.grid.column_width', { label: 'Column Width…' }),
                mitem('grid.cells.autofit_columns', { label: 'AutoFit Column Width' }),
                mhead('Visibility'),
                mitem('grid.cells.hide_rows'),
                mitem('grid.cells.hide_columns'),
                mitem('grid.cells.unhide_rows'),
                mitem('grid.cells.unhide_columns'),
                mitem('ui.grid.hide_sheet', { label: 'Hide Sheet' }),
                mitem('ui.grid.unhide_sheet', { label: 'Unhide Sheet…' }),
                mhead('Organize Sheets'),
                mitem('ui.grid.rename_sheet', { label: 'Rename Sheet' }),
                mitem('ui.grid.move_copy_sheet', { label: 'Move or Copy Sheet…' }),
                mitem('ui.grid.tab_color', { label: 'Tab Color…' }),
                ...FORMAT_PROTECTION_MENU,
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
            rdropdown(undefined, SORT_FILTER_MENU, {
              id: 'editing.sort_filter',
              size: 'medium',
              label: 'Sort & Filter',
              icon: 'SortFilter',
              keyTip: 'S',
            }),
            rdropdown(undefined, FIND_SELECT_MENU, {
              id: 'editing.find_select',
              size: 'medium',
              label: 'Find',
              icon: 'FindSelect',
              keyTip: 'FD',
            }),
            rdropdown(
              'grid.editing.fill',
              [
                mitem('grid.editing.fill_down'),
                mitem('grid.editing.fill_right'),
                mitem('grid.editing.fill_up'),
                mitem('grid.editing.fill_left'),
                mitem('ui.grid.fill_series', { label: 'Series…' }),
                msep(),
                mitem('grid.editing.flash_fill'),
                mitem('ui.grid.custom_lists'),
              ],
              { size: 'small', keyTip: 'FI' },
            ),
            rdropdown(
              'grid.editing.clear',
              [
                mitem('grid.editing.clear', { label: 'Clear All', args: { what: 'All' } }),
                mitem('grid.editing.clear', { label: 'Clear Formats', args: { what: 'Formats' } }),
                mitem('grid.editing.clear', { label: 'Clear Contents', args: { what: 'Contents' } }),
                mitem('grid.editing.clear', {
                  label: 'Clear Comments and Notes',
                  args: { what: 'Comments' },
                }),
                mitem('grid.editing.clear', { label: 'Clear Hyperlinks', args: { what: 'Hyperlinks' } }),
              ],
              { size: 'small', keyTip: 'E' },
            ),
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
          rbutton('grid.insert.pivot_table', { size: 'large', keyTip: 'V' }),
          rbutton('grid.insert.table', { size: 'large', keyTip: 'T' }),
        ]),
        rgroup('illustrations', 'Illustrations', [
          rbutton('suite.insert.pictures', { size: 'large', keyTip: 'P' }),
          rdropdown('grid.insert.shape', SHAPES_MENU, { size: 'medium', keyTip: 'SH', label: 'Shapes' }),
          rbutton('suite.insert.icons', { size: 'medium', keyTip: 'Y1', tier: 'P2' }),
        ]),
        rgroup(
          'charts',
          'Charts',
          [
            rbutton('grid.insert.recommended_charts', { size: 'large', keyTip: 'R' }),
            rbutton('suite.chart.insert', { size: 'large', keyTip: 'CH' }),
            ...CHARTS.map(([id, keyTip]) =>
              rbutton(id, { size: 'small' as const, ...(keyTip ? { keyTip } : {}) }),
            ),
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
          {},
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
        rgroup('links', 'Links', [rsplit('suite.insert.link', LINK_MENU, { size: 'large', keyTip: 'I' })]),
        rgroup('comments', 'Comments', [
          rbutton('suite.review.new_comment', { size: 'large', label: 'Comment', keyTip: 'M' }),
          rbutton('grid.review.new_note', { size: 'medium' }),
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
        gridThemesGroup(),
        rgroup(
          'pageSetup',
          'Page Setup',
          [
            rdropdown(undefined, MARGINS_MENU, {
              id: 'page.margins',
              label: 'Margins',
              icon: 'Margins',
              size: 'large',
              keyTip: 'M',
            }),
            rdropdown(undefined, ORIENTATION_MENU, {
              id: 'page.orientation',
              label: 'Orientation',
              icon: 'Orientation',
              size: 'large',
              keyTip: 'O',
            }),
            rdropdown(undefined, SIZE_MENU, {
              id: 'page.size',
              label: 'Size',
              icon: 'PageSize',
              size: 'large',
              keyTip: 'SZ',
            }),
            rdropdown(undefined, PRINT_AREA_MENU, {
              id: 'page.print_area',
              label: 'Print Area',
              icon: 'PrintArea',
              size: 'medium',
              keyTip: 'R',
            }),
            rdropdown(undefined, BREAKS_MENU, {
              id: 'page.breaks',
              label: 'Breaks',
              icon: 'Breaks',
              size: 'medium',
              keyTip: 'B',
            }),
            rbutton('grid.page_layout.background', { size: 'medium', keyTip: 'G', tier: 'P2' }),
            rbutton('grid.page_layout.print_titles', { size: 'medium', keyTip: 'I' }),
          ],
          { launcher: { command: 'suite.layout.page_setup' } },
        ),
        rgroup(
          'scaleToFit',
          'Scale to Fit',
          [
            rdropdown(undefined, SCALE_MENU, {
              id: 'page.scale',
              label: 'Scale to Fit',
              icon: 'ScaleToFit',
              size: 'large',
              keyTip: 'SC',
            }),
          ],
          { launcher: { command: 'suite.layout.page_setup', label: 'Page Setup: Page' } },
        ),
        rgroup(
          'sheetOptions',
          'Sheet Options',
          [
            rtoggle('grid.view.gridlines', {
              size: 'small',
              checkbox: true,
              label: 'Gridlines: View',
              keyTip: 'VG',
            }),
            rtoggle('grid.page_layout.print_gridlines', {
              size: 'small',
              checkbox: true,
              label: 'Gridlines: Print',
              keyTip: 'PG',
            }),
            rtoggle('grid.page_layout.headings', {
              size: 'small',
              checkbox: true,
              label: 'Headings: View',
              keyTip: 'VH',
            }),
            rtoggle('grid.page_layout.print_headings', {
              size: 'small',
              checkbox: true,
              label: 'Headings: Print',
              keyTip: 'PH',
            }),
            rtoggle('grid.page_layout.sheet_right_to_left', { size: 'large', keyTip: 'SR' }),
          ],
          { launcher: { command: 'grid.page_layout.print_titles', label: 'Page Setup: Sheet' } },
        ),
        rgroup('arrange', 'Arrange', [
          rbutton('suite.arrange.bring_forward', { size: 'medium', keyTip: 'AF' }),
          rbutton('suite.arrange.send_backward', { size: 'medium', keyTip: 'AE' }),
          rbutton('suite.arrange.selection_pane', { size: 'medium', keyTip: 'AP', tier: 'P2' }),
          rbutton('suite.arrange.align', { size: 'small', keyTip: 'AA', tier: 'P2' }),
          rbutton('suite.arrange.group', { size: 'small', keyTip: 'G2', tier: 'P2' }),
          rbutton('suite.arrange.rotate', { size: 'small', keyTip: 'AY', tier: 'P2' }),
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
              ['Financial', 'I', 'Financial', 'DollarSign'],
              ['Logical', 'L', 'Logical', 'Blend'],
              ['Text', 'T', 'Text', 'Type'],
              ['Date & Time', '', 'Date & Time', 'CalendarDays'],
              ['Lookup & Reference', 'O', 'Lookup & Reference', 'Search'],
              ['Math & Trig', 'G', 'Math & Trig', 'Sigma'],
            ] as const
          ).map(([label, keyTip, category, icon]) =>
            // Excel's categories, each with its own icon (labelled in their tooltips: at
            // labelled size the Formulas tab would fold its later groups at 1440 px).
            rdropdown(undefined, functionMenu(category), {
              id: `fn.${category}`,
              label,
              icon,
              size: 'small' as const,
              // Recently Used holds Excel's E; this one takes a letter of its own.
              keyTip: keyTip || undefined,
            }),
          ),
          rdropdown(
            undefined,
            [
              ...['Statistical', 'Engineering', 'Information', 'Compatibility', 'Web'].map((category) =>
                msub(category, functionMenu(category)),
              ),
            ],
            { id: 'fn.more', label: 'More Functions', icon: 'SquareFunction', size: 'small', keyTip: 'Q' },
          ),
        ]),
        rgroup('definedNames', 'Defined Names', [
          rbutton('grid.formulas.name_manager', { size: 'large', keyTip: 'N' }),
          rsplit('grid.formulas.define_name', NAMES_MENU, { size: 'medium', keyTip: 'M' }),
          rbutton('grid.formulas.use_in_formula', { size: 'medium', keyTip: 'S' }),
          rbutton('grid.formulas.create_from_selection', { size: 'medium', keyTip: 'C' }),
        ]),
        rgroup('auditing', 'Formula Auditing', [
          rbutton('grid.formulas.trace_precedents', { size: 'medium', keyTip: 'P' }),
          rbutton('grid.formulas.trace_dependents', { size: 'medium', keyTip: 'D' }),
          rsplit('grid.formulas.remove_arrows', REMOVE_ARROWS_MENU, { size: 'medium', keyTip: 'A' }),
          rtoggle('grid.formulas.show_formulas', { size: 'medium', keyTip: 'H' }),
          rbutton('grid.formulas.error_checking', { size: 'medium', keyTip: 'K' }),
          rbutton('grid.formulas.evaluate_formula', { size: 'medium', keyTip: 'V' }),
          rbutton('grid.formulas.watch_window', { size: 'large', keyTip: 'W' }),
        ]),
        rgroup('calculation', 'Calculation', [
          rdropdown(undefined, CALCULATION_MENU, {
            id: 'calc.options',
            label: 'Calculation Options',
            icon: 'CalculationOptions',
            size: 'large',
            keyTip: 'X',
          }),
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
          rdropdown(undefined, EXPORT_MENU, {
            id: 'data.export',
            label: 'Export',
            icon: 'FileDown',
            size: 'medium',
          }),
        ]),
        rgroup('sortFilter', 'Sort & Filter', [
          rbutton('grid.data.sort_ascending', { size: 'small', keyTip: 'SA' }),
          rbutton('grid.data.sort_descending', { size: 'small', keyTip: 'SD' }),
          rbutton('grid.data.sort', { size: 'large', keyTip: 'SS' }),
          rtoggle('grid.data.toggle_filter', { size: 'large', keyTip: 'T' }),
          rbutton('grid.data.clear_filter', { size: 'medium', keyTip: 'C' }),
          rbutton('grid.data.reapply_filter', { size: 'medium', keyTip: 'Y' }),
        ]),
        rgroup('dataTools', 'Data Tools', [
          rbutton('grid.data.text_to_columns', { size: 'large', keyTip: 'E' }),
          rbutton('grid.editing.flash_fill', { size: 'medium', keyTip: 'FF' }),
          rbutton('grid.data.remove_duplicates', { size: 'medium', keyTip: 'M' }),
          rsplit('grid.data.data_validation', VALIDATION_MENU, { size: 'medium', keyTip: 'V' }),
          rbutton('grid.data.consolidate', { size: 'small', keyTip: 'N', tier: 'P2' }),
        ]),
        rgroup('forecast', 'Forecast', [
          rdropdown('grid.data.what_if', [mitem('grid.data.goal_seek')], { size: 'large', keyTip: 'W' }),
          rbutton('grid.data.forecast_sheet', { size: 'large', keyTip: 'FC', tier: 'P2' }),
        ]),
        rgroup('outline', 'Outline', [
          rsplit('grid.data.group_rows', GROUP_MENU, { size: 'medium', keyTip: 'G' }),
          rsplit('grid.data.ungroup_rows', UNGROUP_MENU, { size: 'medium', keyTip: 'U' }),
          rbutton('grid.data.outline_detail', {
            id: 'outline.show',
            size: 'medium',
            label: 'Show Detail',
            icon: 'Plus',
            keyTip: 'SH',
            args: { show: true },
          }),
          rbutton('grid.data.outline_detail', {
            id: 'outline.hide',
            size: 'medium',
            label: 'Hide Detail',
            icon: 'Minus',
            keyTip: 'HI',
            args: { show: false },
          }),
          rbutton('grid.data.subtotal', { size: 'medium', keyTip: 'B' }),
          rtoggle('grid.data.show_outline_symbols', { size: 'medium', keyTip: 'J' }),
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
        rgroup('notes', 'Notes', [
          rdropdown(undefined, NOTES_MENU, {
            id: 'review.notes',
            label: 'Notes',
            icon: 'NoteSticky',
            size: 'large',
            keyTip: 'T',
          }),
        ]),
        rgroup(
          'protect',
          'Protect',
          [
            rbutton('grid.review.protect_sheet', { size: 'large', keyTip: 'PS' }),
            rbutton('grid.review.protect_workbook', { size: 'large', keyTip: 'PW' }),
          ],
          {},
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
          rtoggle('suite.view.ruler', { size: 'medium', checkbox: true, keyTip: 'R' }),
          rtoggle('grid.view.gridlines', { size: 'medium', checkbox: true, keyTip: 'VG' }),
          rtoggle('grid.view.formula_bar', { size: 'medium', checkbox: true }),
          rtoggle('grid.page_layout.headings', { size: 'medium', checkbox: true, keyTip: 'H' }),
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
          rbutton('suite.view.new_window', { size: 'medium', keyTip: 'N', tier: 'P2' }),
          rsplit(
            'grid.view.freeze_panes',
            [
              mitem('grid.view.freeze_panes'),
              mitem('grid.view.freeze_top_row'),
              mitem('grid.view.freeze_first_column'),
            ],
            { size: 'large', keyTip: 'F' },
          ),
          rtoggle('grid.view.split', { size: 'medium', keyTip: 'S' }),
          rbutton('suite.view.side_by_side', { size: 'medium', keyTip: 'B', tier: 'P2' }),
        ]),
        accessibilityGroup(),
        themeGroup(),
        macrosGroup(),
      ],
      { keyTip: 'W' },
    ),
    gridTableDesignTab(),
    gridPivotAnalyzeTab(),
    gridPivotDesignTab(),
    gridSparklineTab(),
    gridPictureFormatTab(),
    gridShapeFormatTab(),
  ],
};

// ── Builders: id + args → the core's typed Command ─────────────────────────────────

type Args = Readonly<Record<string, unknown>>;

/** The arguments of a Grid command do not have the shape it needs. */
export class GridArgsError extends Error {
  override readonly name = 'CommandArgsError';
  constructor(
    readonly id: string,
    message: string,
  ) {
    super(`${id}: ${message}`);
  }
}

const isStr = (v: unknown): v is string => typeof v === 'string';
const isNum = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const isUint = (v: unknown): v is number => isNum(v) && Number.isInteger(v) && v >= 0 && v <= 0xffffffff;
const isBool = (v: unknown): v is boolean => typeof v === 'boolean';
const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

function want<T>(id: string, a: Args, key: string, ok: (v: unknown) => v is T, what: string): T {
  const v = a[key];
  if (!ok(v)) throw new GridArgsError(id, `"${key}" must be ${what}`);
  return v;
}

function maybe<T>(id: string, a: Args, key: string, ok: (v: unknown) => v is T, what: string): T | null {
  const v = a[key];
  if (v === undefined || v === null) return null;
  if (!ok(v)) throw new GridArgsError(id, `"${key}" must be ${what} or absent`);
  return v;
}

const flag = (id: string, a: Args, key: string) => maybe(id, a, key, isBool, 'true or false') ?? false;

const DIRS: readonly GridDirection[] = ['Up', 'Down', 'Left', 'Right'];
const isDir = (v: unknown): v is GridDirection => isStr(v) && (DIRS as readonly string[]).includes(v);

const BORDER_PRESETS: Readonly<Record<string, GridBorders>> = {
  all: 'All',
  outside: 'Outside',
  thickBox: 'ThickBox',
  inside: 'Inside',
  top: 'Top',
  bottom: 'Bottom',
  left: 'Left',
  right: 'Right',
  bottomDouble: 'BottomDouble',
  bottomThick: 'BottomThick',
  topBottom: 'TopBottom',
  none: 'None',
};

/** `RRGGBB` from a colour choice of the colour pickers, a hex string, or `null`. */
function colorOf(id: string, v: unknown): string | null {
  if (v === undefined || v === null) return null;
  if (isStr(v)) {
    const hex = v.replace(/^#/, '');
    if (!/^[0-9a-f]{6}$/i.test(hex)) throw new GridArgsError(id, '"color" must be RRGGBB');
    return hex.toUpperCase();
  }
  if (isObj(v) && (v.type === 'auto' || v.type === 'none')) return null;
  if (isObj(v) && isStr(v.hex)) return colorOf(id, v.hex);
  throw new GridArgsError(id, '"color" must be a colour');
}

const isClip = (v: unknown): v is ClipboardData => isObj(v) && isStr(v.text);
function clipOf(id: string, a: Args): ClipboardData | null {
  const d = maybe(id, a, 'data', isClip, 'clipboard data');
  return d
    ? { text: d.text, html: isStr(d.html) ? d.html : null, kiln: isStr(d.kiln) ? d.kiln : null }
    : null;
}

const SERIES_TYPES: readonly GridSeriesType[] = ['Linear', 'Growth', 'Date', 'AutoFill'];
const DATE_UNITS: readonly GridDateUnit[] = ['Day', 'Weekday', 'Month', 'Year'];

/** Home ▸ Fill ▸ Series arguments: `{ columns?, kind?, unit?, step?, stop?, trend? }`. */
function seriesOf(id: string, a: Args): GridSeries {
  const kind = (maybe(id, a, 'kind', isStr, 'Linear, Growth, Date or AutoFill') ??
    'Linear') as GridSeriesType;
  if (!SERIES_TYPES.includes(kind)) throw new GridArgsError(id, `unknown series type "${kind}"`);
  const unit = (maybe(id, a, 'unit', isStr, 'Day, Weekday, Month or Year') ?? 'Day') as GridDateUnit;
  if (!DATE_UNITS.includes(unit)) throw new GridArgsError(id, `unknown date unit "${unit}"`);
  return {
    columns: maybe(id, a, 'columns', isBool, 'true or false'),
    kind,
    unit,
    step: maybe(id, a, 'step', isNum, 'a number') ?? 1,
    stop: maybe(id, a, 'stop', isNum, 'a number'),
    trend: flag(id, a, 'trend'),
  };
}

/** The local time zone, as minutes to add to UTC (the core has no zone of its own). */
function utcOffsetMinutes(): number {
  return -new Date().getTimezoneOffset();
}

type GridBuilder = (a: Args, id: string) => GridCommand;

const g = (c: GridCommand): Command => ({ Grid: c });

const simple =
  <T extends GridCommand & string>(c: T): GridBuilder =>
  () =>
    c;

/** A format that changes nothing: `grid.format.apply` takes the members it is given over it. */
export const EMPTY_GRID_FORMAT: GridFormat = Object.freeze({
  number_format: null,
  font_family: null,
  font_size: null,
  bold: null,
  italic: null,
  underline: null,
  strike: null,
  superscript: null,
  subscript: null,
  font_color: null,
  fill_color: null,
  pattern: null,
  pattern_color: null,
  h_align: null,
  v_align: null,
  wrap: null,
  shrink: null,
  indent: null,
  rotation: null,
  merge: null,
  border_left: null,
  border_right: null,
  border_top: null,
  border_bottom: null,
  border_inside_h: null,
  border_inside_v: null,
  border_diagonal_up: null,
  border_diagonal_down: null,
  locked: null,
  hidden: null,
}) as GridFormat;

const GRID_BUILDER_TABLE: Readonly<Record<string, GridBuilder>> = {
  'grid.nav.move': (a, id) => ({
    Move: { dir: want(id, a, 'dir', isDir, 'a direction'), extend: flag(id, a, 'extend') },
  }),
  'grid.nav.jump': (a, id) => ({
    Jump: { dir: want(id, a, 'dir', isDir, 'a direction'), extend: flag(id, a, 'extend') },
  }),
  'grid.nav.page': (a, id) => ({
    Page: { dir: want(id, a, 'dir', isDir, 'a direction'), extend: flag(id, a, 'extend') },
  }),
  'grid.nav.row_start': (a, id) => ({ RowStart: { extend: flag(id, a, 'extend') } }),
  'grid.nav.first_cell': (a, id) => ({ FirstCell: { extend: flag(id, a, 'extend') } }),
  'grid.nav.last_cell': (a, id) => ({ LastCell: { extend: flag(id, a, 'extend') } }),
  'grid.nav.advance': (a, id) => ({
    Advance: { backward: flag(id, a, 'backward'), across: flag(id, a, 'across') },
  }),
  'grid.nav.select_cell': (a, id) => ({
    SelectCell: {
      row: want(id, a, 'row', isUint, 'a row'),
      col: want(id, a, 'col', isUint, 'a column'),
      extend: flag(id, a, 'extend'),
      add: flag(id, a, 'add'),
    },
  }),
  'grid.nav.select_range': (a, id) => ({
    SelectRange: {
      range: {
        r0: want(id, a, 'r0', isUint, 'a row'),
        c0: want(id, a, 'c0', isUint, 'a column'),
        r1: want(id, a, 'r1', isUint, 'a row'),
        c1: want(id, a, 'c1', isUint, 'a column'),
      },
      add: flag(id, a, 'add'),
    },
  }),
  'grid.nav.select_rows': (a, id) => ({
    SelectRows: {
      from: want(id, a, 'from', isUint, 'a row'),
      to: want(id, a, 'to', isUint, 'a row'),
      extend: flag(id, a, 'extend'),
      add: flag(id, a, 'add'),
    },
  }),
  'grid.nav.select_columns': (a, id) => ({
    SelectColumns: {
      from: want(id, a, 'from', isUint, 'a column'),
      to: want(id, a, 'to', isUint, 'a column'),
      extend: flag(id, a, 'extend'),
      add: flag(id, a, 'add'),
    },
  }),
  'grid.nav.select_all': simple('SelectAll'),
  'grid.editing.select_row': simple('SelectRow'),
  'grid.editing.select_column': simple('SelectColumn'),
  'grid.nav.go_to': (a, id) => ({
    GoTo: { reference: want(id, a, 'reference', isStr, 'a reference'), define: flag(id, a, 'define') },
  }),
  'grid.sheet.activate': (a, id) => ({
    ActivateSheet: { index: want(id, a, 'index', isUint, 'a sheet index') },
  }),
  'grid.cells.previous_sheet': simple('PreviousSheet'),
  'grid.cells.next_sheet': simple('NextSheet'),
  'grid.cells.new_sheet': simple('InsertSheet'),
  'grid.cells.rename_sheet': (a, id) => ({
    RenameSheet: {
      index: want(id, a, 'index', isUint, 'a sheet index'),
      name: want(id, a, 'name', isStr, 'a name'),
    },
  }),
  'grid.sheet.delete': (a, id) => ({ DeleteSheet: { index: want(id, a, 'index', isUint, 'a sheet index') } }),
  'grid.sheet.move': (a, id) => ({
    MoveSheet: {
      from: want(id, a, 'from', isUint, 'a sheet index'),
      to: want(id, a, 'to', isUint, 'a sheet index'),
    },
  }),
  'grid.sheet.duplicate': (a, id) => ({
    DuplicateSheet: { index: want(id, a, 'index', isUint, 'a sheet index') },
  }),
  'grid.sheet.tab_color': (a, id) => ({
    SheetTabColor: { index: want(id, a, 'index', isUint, 'a sheet index'), color: colorOf(id, a.color) },
  }),
  'grid.sheet.hide': (a, id) => ({ HideSheet: { index: want(id, a, 'index', isUint, 'a sheet index') } }),
  'grid.sheet.unhide': (a, id) => ({ UnhideSheet: { index: want(id, a, 'index', isUint, 'a sheet index') } }),
  'grid.editing.edit_cell': simple('EditCell'),
  'grid.editing.begin': (a, id) => ({ BeginEdit: { text: want(id, a, 'text', isStr, 'text') } }),
  'grid.editing.put_back_draft': (a, id) => ({
    PutBackDraft: {
      sheet: want(id, a, 'sheet', isUint, 'a sheet id'),
      row: want(id, a, 'row', isUint, 'a row'),
      col: want(id, a, 'col', isUint, 'a column'),
      text: want(id, a, 'text', isStr, 'text'),
    },
  }),
  'grid.editing.set_text': (a, id) => ({
    SetEditText: {
      text: want(id, a, 'text', isStr, 'text'),
      start: want(id, a, 'start', isUint, 'an offset'),
      end: want(id, a, 'end', isUint, 'an offset'),
      formula_bar: flag(id, a, 'formula_bar'),
    },
  }),
  'grid.editing.commit': (a, id) => ({
    CommitEdit: { advance: maybe(id, a, 'advance', isDir, 'a direction') },
  }),
  'grid.editing.fill_selection': simple('FillSelection'),
  'grid.editing.array_formula': simple('ArrayFormula'),
  'grid.editing.cancel_edit': simple('CancelEdit'),
  'grid.editing.new_line_in_cell': simple('NewLineInCell'),
  'grid.editing.toggle_absolute': simple('ToggleAbsolute'),
  'grid.editing.accept_completion': (a, id) => ({
    AcceptCompletion: { index: want(id, a, 'index', isUint, 'an index') },
  }),
  'grid.editing.move_completion': (a, id) => ({
    MoveCompletion: { down: want(id, a, 'down', isBool, 'true or false') },
  }),
  'grid.editing.point_ref': (a, id) => ({
    PointRef: {
      row: want(id, a, 'row', isUint, 'a row'),
      col: want(id, a, 'col', isUint, 'a column'),
      extend: flag(id, a, 'extend'),
    },
  }),
  'grid.editing.point_move': (a, id) => ({
    PointMove: { dir: want(id, a, 'dir', isDir, 'a direction'), extend: flag(id, a, 'extend') },
  }),
  'grid.editing.open_dropdown': simple('OpenDropdown'),
  'grid.editing.clear_contents': simple('ClearContents'),
  'grid.editing.clear': (a, id) => ({
    Clear: {
      what: (maybe(id, a, 'what', isStr, 'All, Contents, Formats, Comments or Hyperlinks') ??
        'All') as GridClear,
    },
  }),
  'grid.editing.fill_down': simple('FillDown'),
  'grid.editing.fill_right': simple('FillRight'),
  'grid.editing.fill_up': simple('FillUp'),
  'grid.editing.fill_left': simple('FillLeft'),
  'grid.editing.drag_fill': (a, id) => ({
    DragFill: { row: want(id, a, 'row', isUint, 'a row'), col: want(id, a, 'col', isUint, 'a column') },
  }),
  'grid.editing.auto_fill': (a, id) => ({
    AutoFill: {
      row: want(id, a, 'row', isUint, 'a row'),
      col: want(id, a, 'col', isUint, 'a column'),
      mode: (maybe(id, a, 'mode', isStr, 'a fill option') ?? 'Auto') as GridFill,
    },
  }),
  'grid.editing.auto_sum': (a, id) => ({
    AutoSum: { function: maybe(id, a, 'fn', isStr, 'a function name') },
  }),
  'grid.editing.insert_date_time': (a, id) => ({
    InsertDateTime: { time: flag(id, a, 'time'), utc_offset_minutes: utcOffsetMinutes() },
  }),
  'grid.editing.find_next': (a, id) => ({
    FindNext: {
      text: want(id, a, 'text', isStr, 'text'),
      match_case: flag(id, a, 'match_case'),
      whole_cell: flag(id, a, 'whole_cell'),
      backward: flag(id, a, 'backward'),
      in_formulas: flag(id, a, 'in_formulas'),
    },
  }),
  'grid.formulas.calculate_now': simple('CalculateNow'),
  'grid.formulas.calculate_sheet': simple('CalculateSheet'),
  'grid.formulas.show_formulas': simple('ShowFormulas'),
  'grid.cells.insert_cells': (a, id) => ({
    InsertCells: { shift: (maybe(id, a, 'shift', isStr, 'a shift') ?? 'Auto') as CellShift },
  }),
  'grid.cells.delete_cells': (a, id) => ({
    DeleteCells: { shift: (maybe(id, a, 'shift', isStr, 'a shift') ?? 'Auto') as CellShift },
  }),
  'grid.cells.insert_rows': simple('InsertRows'),
  'grid.cells.insert_columns': simple('InsertColumns'),
  'grid.cells.delete_rows': simple('DeleteRows'),
  'grid.cells.delete_columns': simple('DeleteColumns'),
  'grid.cells.row_height': (a, id) => ({ RowHeight: { points: want(id, a, 'points', isNum, 'points') } }),
  'grid.cells.column_width': (a, id) => ({
    ColumnWidth: { chars: want(id, a, 'chars', isNum, 'characters') },
  }),
  'grid.cells.resize_rows': (a, id) => ({
    ResizeRows: {
      from: want(id, a, 'from', isUint, 'a row'),
      to: want(id, a, 'to', isUint, 'a row'),
      px: want(id, a, 'px', isNum, 'pixels'),
    },
  }),
  'grid.cells.resize_columns': (a, id) => ({
    ResizeColumns: {
      from: want(id, a, 'from', isUint, 'a column'),
      to: want(id, a, 'to', isUint, 'a column'),
      px: want(id, a, 'px', isNum, 'pixels'),
    },
  }),
  'grid.cells.autofit_rows': (a, id) => ({
    AutoFitRows: { from: maybe(id, a, 'from', isUint, 'a row'), to: maybe(id, a, 'to', isUint, 'a row') },
  }),
  'grid.cells.autofit_columns': (a, id) => ({
    AutoFitColumns: {
      from: maybe(id, a, 'from', isUint, 'a column'),
      to: maybe(id, a, 'to', isUint, 'a column'),
    },
  }),
  'grid.cells.hide_rows': simple('HideRows'),
  'grid.cells.hide_columns': simple('HideColumns'),
  'grid.cells.unhide_rows': simple('UnhideRows'),
  'grid.cells.unhide_columns': simple('UnhideColumns'),
  'grid.font.borders': (a, id) => {
    const name = maybe(id, a, 'borders', isStr, 'a border preset') ?? 'all';
    const preset = BORDER_PRESETS[name];
    if (!preset) throw new GridArgsError(id, `unknown border preset "${name}"`);
    return {
      Borders: {
        preset,
        style: maybe(id, a, 'style', isStr, 'a line style') as GridLineStyle | null,
        color: colorOf(id, a.color),
      },
    };
  },
  'grid.font.border_outline': simple('BorderOutline'),
  'grid.font.border_none': simple('BorderNone'),
  'grid.font.fill_color': (a, id) => ({ FillColor: { color: colorOf(id, a.color) } }),
  'grid.alignment.align_top': simple('AlignTop'),
  'grid.alignment.align_middle': simple('AlignMiddle'),
  'grid.alignment.align_bottom': simple('AlignBottom'),
  'grid.alignment.horizontal': (a, id) => ({
    AlignHorizontal: { align: want(id, a, 'align', isStr, 'an alignment') as GridHAlign },
  }),
  'grid.alignment.orientation': (a, id) => {
    const v = a.angle;
    const angle = v === 'vertical' ? 255 : isNum(v) && Number.isInteger(v) ? v : null;
    if (angle === null) throw new GridArgsError(id, '"angle" must be degrees or "vertical"');
    return { Orientation: { angle } };
  },
  'grid.alignment.wrap_text': simple('WrapText'),
  'grid.alignment.merge_center': simple('MergeCenter'),
  'grid.alignment.merge_across': simple('MergeAcross'),
  'grid.alignment.merge_cells': simple('MergeCells'),
  'grid.alignment.unmerge_cells': simple('UnmergeCells'),
  'grid.number.number_format': (a, id) => {
    const v = a.format ?? a.value;
    if (!isStr(v) || v.trim() === '') throw new GridArgsError(id, '"format" must be a format name or code');
    return { NumberFormat: { format: v } };
  },
  'grid.number.number': simple('NumberStyle'),
  'grid.number.currency': simple('CurrencyStyle'),
  'grid.number.percent': simple('PercentStyle'),
  'grid.number.accounting': (a, id) => ({
    AccountingStyle: { currency: maybe(id, a, 'currency', isStr, 'a currency code') },
  }),
  'grid.number.comma': simple('CommaStyle'),
  'grid.number.increase_decimal': simple('IncreaseDecimal'),
  'grid.number.decrease_decimal': simple('DecreaseDecimal'),
  'grid.format.apply': (a, id) => ({
    ApplyFormat: {
      format: { ...EMPTY_GRID_FORMAT, ...(want(id, a, 'format', isObj, 'a format') as Partial<GridFormat>) },
    },
  }),
  'grid.view.freeze_panes': simple('FreezePanes'),
  'grid.view.freeze_top_row': simple('FreezeTopRow'),
  'grid.view.freeze_first_column': simple('FreezeFirstColumn'),
  'grid.view.split': simple('Split'),
  'grid.view.gridlines': simple('Gridlines'),
  'grid.page_layout.headings': simple('Headings'),
  'grid.page_layout.sheet_right_to_left': simple('SheetRightToLeft'),
  'grid.clipboard.paste_special': (a, id) => ({
    PasteSpecial: {
      what: (maybe(id, a, 'what', isStr, 'what to paste') ?? 'All') as GridPaste,
      op: (maybe(id, a, 'op', isStr, 'an operation') ?? 'None') as GridPasteOp,
      skip_blanks: flag(id, a, 'skip_blanks'),
      transpose: flag(id, a, 'transpose'),
      data: clipOf(id, a),
    },
  }),
  'grid.clipboard.paste_values': (a, id) => ({ PasteValues: { data: clipOf(id, a) } }),
  'grid.clipboard.paste_formulas': (a, id) => ({ PasteFormulas: { data: clipOf(id, a) } }),
  'grid.clipboard.paste_formatting': (a, id) => ({ PasteFormatting: { data: clipOf(id, a) } }),
  'grid.clipboard.paste_transpose': (a, id) => ({ PasteTranspose: { data: clipOf(id, a) } }),
  'grid.clipboard.paste_link': (a, id) => ({ PasteLink: { data: clipOf(id, a) } }),
  'grid.editing.fill_series': (a, id) => ({ FillSeries: { series: seriesOf(id, a) } }),
  'grid.editing.auto_fill_option': (a, id) => {
    const v = want(id, a, 'choice', isStr, 'a fill option or FlashFill');
    const choice: GridAutoFillChoice = v === 'FlashFill' ? 'FlashFill' : { Fill: v as GridFill };
    return { AutoFillOption: { choice } };
  },
  'grid.editing.custom_lists': (a, id) => {
    const lists = a.lists;
    if (!Array.isArray(lists) || !lists.every((l) => Array.isArray(l) && l.every(isStr)))
      throw new GridArgsError(id, '"lists" must be lists of text');
    return { SetCustomLists: { lists: lists as string[][] } };
  },
  'grid.editing.import_custom_list': simple('ImportCustomList'),
  'grid.font.border_pen': (a, id) => {
    const mode = maybe(id, a, 'mode', isStr, 'Draw, Grid or Erase');
    if (mode !== null && !['Draw', 'Grid', 'Erase'].includes(mode))
      throw new GridArgsError(id, '"mode" must be Draw, Grid or Erase');
    const color = a.color === '' ? '' : colorOf(id, a.color);
    return {
      BorderPen: {
        mode: mode as GridPenMode | null,
        style: maybe(id, a, 'style', isStr, 'a line style') as GridLineStyle | null,
        color,
      },
    };
  },
  'grid.font.border_pen_off': simple('BorderPenOff'),
  'grid.font.draw_border': (a, id) => ({
    DrawBorder: {
      x0: want(id, a, 'x0', isNum, 'a number'),
      y0: want(id, a, 'y0', isNum, 'a number'),
      x1: want(id, a, 'x1', isNum, 'a number'),
      y1: want(id, a, 'y1', isNum, 'a number'),
      done: flag(id, a, 'done'),
    },
  }),
  'grid.number.delete_format': (a, id) => ({
    DeleteNumberFormat: { code: want(id, a, 'code', isStr, 'a number format code') },
  }),
  'grid.drawing.select': (a, id) => ({
    SelectDrawing: { index: maybe(id, a, 'index', isUint, 'a chart number') },
  }),
  'grid.drawing.place': (a, id) => ({
    PlaceDrawing: {
      index: want(id, a, 'index', isUint, 'a chart number'),
      x: want(id, a, 'x', isNum, 'pixels'),
      y: want(id, a, 'y', isNum, 'pixels'),
      width: want(id, a, 'width', isNum, 'pixels'),
      height: want(id, a, 'height', isNum, 'pixels'),
    },
  }),
  'grid.drawing.delete': simple('DeleteDrawing'),
  'grid.view.theme': (a, id) => ({ Theme: { dark: flag(id, a, 'dark') } }),
};

/** The builders of every Grid core command (spread into `CORE_BUILDERS`). */
export const GRID_BUILDERS: Readonly<Record<string, (a: Args) => Command>> = Object.freeze({
  ...Object.fromEntries(
    Object.entries(GRID_BUILDER_TABLE).map(([id, build]) => [id, (a: Args) => g(build(a, id))] as const),
  ),
  ...GRID_FEATURE_BUILDERS,
  ...GRID_ANALYSIS_BUILDERS,
  ...GRID_SURFACE_BUILDERS,
});

// ── Ribbon state ─────────────────────────────────────────────────────────────────────────────────

type GridCtxState = import('@kiln/protocol').GridState;
/** The core's Grid state the editor put in the registry context (`gridState`). */
const gs = (ctx: CommandContext): GridCtxState | undefined => ctx.gridState as GridCtxState | undefined;
const fmt = (ctx: CommandContext) => gs(ctx)?.format;
/** One object per colour (the ribbon compares values by identity). */
const colours = new Map<string, { type: 'rgb'; hex: string } | { type: 'auto' } | { type: 'none' }>();
function colour(key: string, make: () => { type: 'rgb'; hex: string } | { type: 'auto' } | { type: 'none' }) {
  let v = colours.get(key);
  if (!v) {
    v = make();
    colours.set(key, v);
  }
  return v;
}
const hexOr = (hex: string | null | undefined, none: 'auto' | 'none') =>
  hex ? colour(`rgb:${hex}`, () => ({ type: 'rgb', hex: `#${hex}` })) : colour(none, () => ({ type: none }));

/**
 * Pressed states and values of the ribbon in Grid, read from the active cell's format and the
 * view the core reports (`Query::Grid(State)`). Nothing is computed: each entry reads one field.
 */
export const GRID_CORE_STATE: {
  checked: Readonly<Record<string, (ctx: CommandContext) => boolean | undefined>>;
  values: Readonly<Record<string, (ctx: CommandContext) => unknown>>;
} = {
  checked: {
    'suite.format.bold': (c) => fmt(c)?.bold,
    'suite.format.italic': (c) => fmt(c)?.italic,
    'suite.format.underline': (c) => fmt(c)?.underline,
    'suite.format.strikethrough': (c) => fmt(c)?.strike,
    'suite.format.align_left': (c) => fmt(c)?.h_align === 'Left',
    'suite.format.align_center': (c) => fmt(c)?.h_align === 'Center',
    'suite.format.align_right': (c) => fmt(c)?.h_align === 'Right',
    'grid.alignment.align_top': (c) => fmt(c)?.v_align === 'Top',
    'grid.alignment.align_middle': (c) => fmt(c)?.v_align === 'Center',
    'grid.alignment.align_bottom': (c) => fmt(c)?.v_align === 'Bottom',
    'grid.alignment.wrap_text': (c) => fmt(c)?.wrap,
    'grid.alignment.merge_center': (c) => fmt(c)?.merged,
    'grid.view.gridlines': (c) => gs(c)?.gridlines,
    'grid.page_layout.headings': (c) => gs(c)?.headings,
    'grid.page_layout.sheet_right_to_left': (c) => gs(c)?.layout.rtl,
    'grid.formulas.show_formulas': (c) => gs(c)?.show_formulas,
    'grid.view.split': (c) => gs(c)?.layout.split,
    'grid.view.freeze_panes': (c) => {
      const l = gs(c)?.layout;
      return l ? l.freeze_rows > 0 || l.freeze_cols > 0 : undefined;
    },
    ...GRID_FEATURE_STATE.checked,
  },
  values: {
    'suite.format.font_family': (c) => fmt(c)?.font_family,
    'suite.format.font_size': (c) => {
      const s = fmt(c)?.font_size;
      return s === undefined ? undefined : String(Math.round(s * 2) / 2);
    },
    'suite.format.font_color': (c) => (fmt(c) ? hexOr(fmt(c)?.font_color, 'auto') : undefined),
    'grid.font.fill_color': (c) => (fmt(c) ? hexOr(fmt(c)?.fill_color, 'none') : undefined),
    'grid.number.number_format': (c) => {
      const f = fmt(c);
      if (!f) return undefined;
      return f.number_format_name === 'Custom' ? f.number_format : f.number_format_name;
    },
    ...GRID_FEATURE_STATE.values,
  },
};
