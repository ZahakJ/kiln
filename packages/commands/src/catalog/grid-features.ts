/**
 * Kiln Grid's features: the declarations of the core's sort, filter, conditional
 * formatting, validation, table, find, comment, page layout, name, auditing and data commands
 * and of the editor's dialogs and panes; how the UI builds each core command's typed `Command`
 * from its arguments; the ribbon's menus for them (Conditional Formatting, Cell Styles, Sort &
 * Filter, Find & Select, Margins, Orientation, Size, Print Area, Breaks, Calculation Options,
 * Data Validation, Group) and the Table Design tab; and the ids the Grid editor implements in its
 * chrome (`GRID_CHROME_COMMANDS`).
 */
import type {
  Command,
  GridArrows,
  GridBreakAction,
  GridCalcMode,
  GridCfRule,
  GridCfRuleKind,
  GridCfValue,
  GridColumnFilter,
  GridCommand,
  GridFindSpec,
  GridImport,
  GridLink,
  GridObjectFormat,
  GridPageSetup,
  GridPrintAreaAction,
  GridRange,
  GridSortOn,
  GridSortSpec,
  GridSpecial,
  GridTableOptions,
  GridTextToColumns,
  GridTotals,
  GridValidation,
} from '@kiln/protocol';
import {
  mhead,
  mitem,
  msep,
  msub,
  type RibbonDataTab,
  type RibbonGalleryEntry,
  type RibbonMenuEntry,
  rbutton,
  rcolor,
  rcombo,
  rdropdown,
  rgallery,
  rgroup,
  rnumber,
  rsplit,
  rtab,
  rtoggle,
} from '../ribbon.ts';
import type { CommandContext, CommandDeclaration } from '../types.ts';
import { c, GRID, section } from './define.ts';
import { GRID_ANALYSIS_CHROME_COMMANDS } from './grid-analysis.ts';

const G = { apps: GRID };
const hidden = { palette: false } as const;

/** Declarations of the core commands and chrome ids. */
export const gridFeatureCommands: readonly CommandDeclaration[] = [
  ...section('Data ▸ Sort & Filter', G, [
    c('grid.data.apply_sort', 'Sort', 'ArrowDownUp', hidden),
    c('grid.data.filter_column', 'Filter Column', undefined, hidden),
    c('grid.data.sort_filter_column', 'Sort Column', undefined, hidden),
  ]),
  ...section('Home ▸ Styles', G, [
    c('grid.styles.add_rule', 'Add Conditional Formatting Rule', undefined, hidden),
    c('grid.styles.set_rules', 'Set Conditional Formatting Rules', undefined, hidden),
    c('grid.styles.clear_rules', 'Clear Rules', 'ClearAll', { synonyms: ['remove conditional formatting'] }),
    c('ui.grid.cf_rule', 'New Formatting Rule…', 'ConditionalFormatting', {
      synonyms: ['highlight cells rules', 'top 10', 'above average', 'duplicate values'],
    }),
    c('ui.grid.cf_manage', 'Manage Rules…', 'ConditionalFormatting', {
      synonyms: ['conditional formatting rules manager'],
    }),
    c('grid.styles.apply_cell_style', 'Apply Cell Style', undefined, hidden),
    c('grid.clipboard.paint_format', 'Paint Format', undefined, hidden),
  ]),
  ...section('Data ▸ Data Tools', G, [
    c('grid.data.set_validation', 'Set Data Validation', undefined, hidden),
    c('grid.data.circle_invalid', 'Circle Invalid Data', 'DataValidation'),
    c('grid.data.split_column', 'Split Column', undefined, hidden),
    c('grid.data.dedupe', 'Remove Duplicate Rows', undefined, hidden),
    c('grid.data.import', 'Import Data', undefined, hidden),
    c('grid.data.group', 'Group Rows or Columns', undefined, hidden),
    c('grid.data.ungroup', 'Ungroup Rows or Columns', undefined, hidden),
    c('grid.data.outline_detail', 'Show or Hide Detail', 'ListTree'),
    c('ui.grid.export_csv', 'Export as CSV', 'Download', { synonyms: ['save as csv', 'download csv'] }),
    c('ui.grid.export_tsv', 'Export as TSV', 'Download', { synonyms: ['tab separated'] }),
    c('ui.grid.export_json', 'Export as JSON', 'FromJson', { synonyms: ['download json'] }),
  ]),
  ...section('Table Design', G, [
    c('grid.table.create', 'Create Table', undefined, hidden),
    c('grid.table.style', 'Table Styles', 'FormatAsTable', hidden),
    c('grid.table.options', 'Table Style Options', undefined, hidden),
    c('grid.table.totals', 'Total Row Function', undefined, hidden),
    c('grid.table.rename', 'Table Name', 'PencilLine', hidden),
    c('grid.table.resize', 'Resize Table', undefined, hidden),
    c('grid.table.convert_to_range', 'Convert to Range', 'Table', { synonyms: ['remove table'] }),
    c('ui.grid.resize_table', 'Resize Table…', 'Scaling'),
    c('ui.grid.convert_to_range', 'Convert to Range', 'Table', hidden),
    c('ui.grid.table_remove_duplicates', 'Remove Duplicates', 'RemoveDuplicates', hidden),
    c('ui.grid.table_totals', 'Total Row Function', 'Sigma', hidden),
    c('ui.grid.open_dropdown', 'Open List', 'ChevronDown', hidden),
    c('ui.grid.table_header_row', 'Header Row', 'Rows3', hidden),
    c('ui.grid.table_total_row', 'Total Row', 'Sigma', hidden),
    c('ui.grid.table_banded_rows', 'Banded Rows', 'Rows3', hidden),
    c('ui.grid.table_banded_columns', 'Banded Columns', 'Columns3', hidden),
    c('ui.grid.table_first_column', 'First Column', 'PanelLeft', hidden),
    c('ui.grid.table_last_column', 'Last Column', 'PanelRightClose', hidden),
    c('ui.grid.table_filter_button', 'Filter Button', 'FilterToggle', hidden),
  ]),
  ...section('Formulas ▸ Function Library', G, [
    c('ui.grid.function_args', 'Function Arguments', 'InsertFunction', hidden),
    c('grid.formulas.iterative', 'Iterative Calculation', undefined, hidden),
    c('grid.formulas.date_1904', 'Use 1904 Date System', undefined, hidden),
    c('grid.data.accept_invalid', 'Keep the Entry', undefined, hidden),
    c('ui.grid.iterative', 'Iterative Calculation…', 'CalculationOptions', {
      synonyms: ['circular references', 'maximum iterations'],
    }),
  ]),
  ...section('Insert ▸ Illustrations', G, [
    c('grid.insert.picture', 'Insert Picture', 'Image', hidden),
    c('grid.insert.shape', 'Insert Shape', 'Shapes', { synonyms: ['rectangle', 'oval', 'arrow', 'line'] }),
    c('grid.drawing.format', 'Format Object', undefined, hidden),
    c('grid.picture.reset', 'Reset Picture', 'RotateCcw', hidden),
    c('grid.drawing.arrange', 'Arrange', 'BringForward', hidden),
    c('ui.grid.reset_picture_size', 'Reset Picture & Size', 'RotateCcw', hidden),
    c('ui.grid.bring_to_front', 'Bring to Front', 'BringForward', hidden),
    c('ui.grid.send_to_back', 'Send to Back', 'SendBackward', hidden),
    c('ui.grid.object_height', 'Height', 'MoveVertical', { officeName: 'Shape Height', palette: false }),
    c('ui.grid.object_width', 'Width', 'MoveHorizontal', { officeName: 'Shape Width', palette: false }),
    c('ui.grid.shape_fill', 'Shape Fill', 'PaintBucket', hidden),
    c('ui.grid.shape_outline', 'Shape Outline', 'PenLine', hidden),
    c('ui.grid.outline_weight', 'Weight', 'Minus', hidden),
    c('ui.grid.shape_text', 'Edit Text…', 'Type', { synonyms: ['text in shape'] }),
    c('ui.grid.shape_text_color', 'Text Fill', 'FontColor', hidden),
    c('ui.grid.change_shape', 'Change Shape', 'Shapes', hidden),
    c('ui.grid.format_object', 'Size and Properties…', 'Scaling', { synonyms: ['move and size with cells'] }),
  ]),
  ...section('Home ▸ Editing', G, [
    c('grid.editing.replace', 'Replace', 'Replace', hidden),
    c('grid.editing.select_special', 'Select Special Cells', undefined, hidden),
    c('ui.grid.find', 'Find…', 'Search', { synonyms: ['find all'] }),
    c('ui.grid.go_to_special', 'Go To Special…', 'Crosshair', {
      synonyms: ['select blanks', 'select visible cells', 'select formulas'],
    }),
  ]),
  ...section('Review', G, [
    c('grid.review.set_note', 'Edit Note', 'NoteSticky', hidden),
    c('grid.review.show_note', 'Show/Hide Note', 'Eye'),
    c('grid.review.show_all_notes', 'Show All Notes', 'Eye', hidden),
    c('grid.review.add_comment', 'Add Comment', 'NewComment', hidden),
    c('grid.review.edit_comment', 'Edit Comment', undefined, hidden),
    c('grid.review.delete', 'Delete Comment', 'DeleteComment', hidden),
    c('grid.review.resolve_comment', 'Resolve Thread', undefined, hidden),
    c('grid.review.next', 'Next Comment', 'ChevronRight', hidden),
    c('grid.review.convert_notes', 'Convert Notes to Comments', undefined, hidden),
    c('grid.review.spell_change', 'Change Spelling', undefined, hidden),
    c('grid.review.spell_ignore', 'Ignore Spelling', undefined, hidden),
    c('grid.review.load_dictionary', 'Load Dictionary', undefined, hidden),
    c('ui.grid.edit_note', 'Edit Note', 'PencilLine'),
    c('ui.grid.delete_note', 'Delete Note', 'DeleteComment'),
    c('ui.grid.show_all_notes', 'Show All Notes', 'Eye'),
    c('ui.grid.convert_notes', 'Convert to Comments', 'NewComment'),
  ]),
  ...section('Insert ▸ Links', G, [
    c('grid.insert.set_link', 'Set Hyperlink', undefined, hidden),
    c('ui.grid.remove_link', 'Remove Hyperlink', 'Eraser'),
    c('ui.grid.open_link', 'Open Hyperlink', 'Link'),
  ]),
  ...section('Page Layout', G, [
    c('grid.page_layout.page_setup', 'Page Setup', 'Settings', hidden),
    c('grid.page_layout.set_print_area', 'Print Area', 'PrintArea', hidden),
    c('grid.page_layout.page_break', 'Page Break', 'Breaks', hidden),
    c('grid.page_layout.move_page_break', 'Move Page Break', undefined, hidden),
    c('grid.page_layout.print_gridlines', 'Print Gridlines', 'Printer'),
    c('grid.page_layout.print_headings', 'Print Headings', 'Printer'),
    c('ui.grid.print_area_set', 'Set Print Area', 'PrintArea'),
    c('ui.grid.custom_margins', 'Custom Margins…', 'Margins'),
  ]),
  ...section('Formulas', G, [
    c('grid.names.define', 'Define Name', undefined, hidden),
    c('grid.names.delete', 'Delete Name', undefined, hidden),
    c('grid.names.create', 'Create Names', undefined, hidden),
    c('grid.formulas.use_in_formula', 'Use in Formula', 'DefineName', { synonyms: ['paste name', 'F3'] }),
    c('grid.formulas.calculation_mode', 'Calculation Mode', undefined, hidden),
    c('grid.formulas.trace_formula', 'Trace Formula', undefined, hidden),
    c('grid.formulas.watch', 'Watch Cell', undefined, hidden),
    c('ui.grid.watch_add', 'Add Watch', 'Plus'),
    c('ui.grid.calc_automatic', 'Automatic', 'CalculationOptions', hidden),
    c('ui.grid.calc_automaticexcepttables', 'Automatic Except for Data Tables', 'CalculationOptions', hidden),
    c('ui.grid.calc_manual', 'Manual', 'CalculationOptions', { synonyms: ['manual calculation'] }),
  ]),
];

/**
 * Ids of the ribbon, keymaps and menus the Grid editor implements in its chrome (registered by
 * `@kiln/editor-grid`'s `registerGridUi` and `registerGridFeatures`); with the core's and the
 * `ui.*` ids they leave no Grid P0 or P1 item a reservation (the Grid gate).
 */
export const GRID_CHROME_COMMANDS: readonly string[] = Object.freeze([
  'grid.data.sort',
  'grid.data.data_validation',
  'grid.data.text_to_columns',
  'grid.data.remove_duplicates',
  'grid.data.from_text_csv',
  'grid.data.from_json',
  'grid.data.group_rows',
  'grid.data.ungroup_rows',
  'grid.insert.table',
  'grid.styles.format_as_table',
  'grid.formulas.name_manager',
  'grid.formulas.define_name',
  'grid.formulas.create_from_selection',
  'grid.formulas.use_in_formula',
  'grid.formulas.evaluate_formula',
  'grid.formulas.error_checking',
  'grid.formulas.formula_lab',
  'grid.formulas.watch_window',
  'grid.formulas.insert_function',
  'grid.page_layout.print_titles',
  'grid.review.new_note',
  'suite.edit.replace',
  'suite.edit.find',
  'suite.edit.go_to',
  'suite.edit.format_painter',
  'suite.insert.link',
  'suite.insert.symbol',
  'suite.insert.header_footer',
  'suite.layout.page_setup',
  'suite.review.new_comment',
  'suite.review.delete_comment',
  'suite.review.previous_comment',
  'suite.review.next_comment',
  'suite.review.show_comments',
  'suite.review.spelling',
  // The editor's own (`registerGridUi`, `registerGridFeatures`).
  'grid.cells.format_cells',
  'suite.edit.paste_special',
  'grid.editing.fill',
  'grid.data.what_if',
  'grid.insert.recommended_charts',
  'grid.insert.chart_column',
  'grid.insert.chart_line',
  'grid.insert.chart_pie',
  'grid.insert.chart_scatter',
  'grid.insert.chart_hierarchy',
  'grid.insert.chart_statistic',
  'grid.insert.chart_waterfall',
  'grid.insert.chart_combo',
  'grid.formulas.recently_used_functions',
  'grid.view.formula_bar',
  'grid.view.zoom_to_selection',
  'suite.view.ruler',
  'suite.insert.text_box',
  'suite.picture.alt_text',
  'suite.picture.crop',
  'suite.arrange.bring_forward',
  'suite.arrange.send_backward',
  ...GRID_ANALYSIS_CHROME_COMMANDS,
]);

// ── ribbon menus ─────────────────────────────────────────────────────────────────────────────

const v = (kind: GridCfValue['kind'], value = ''): GridCfValue => ({ kind, value, gte: true });
const rule = (kind: GridCfRuleKind): { rule: GridCfRule } => ({
  rule: { id: 0, applies_to: '', kind, style: null, stop_if_true: false },
});

const BARS: readonly [string, string][] = [
  ['Blue', '638EC6'],
  ['Green', '63C384'],
  ['Red', 'FF555A'],
  ['Orange', 'FFB628'],
  ['Light Blue', '008AEF'],
  ['Purple', 'D6007B'],
];

const SCALES: readonly [string, string[]][] = [
  ['Green - Yellow - Red', ['F8696B', 'FFEB84', '63BE7B']],
  ['Red - Yellow - Green', ['63BE7B', 'FFEB84', 'F8696B']],
  ['Green - White - Red', ['F8696B', 'FCFCFF', '63BE7B']],
  ['Red - White - Green', ['63BE7B', 'FCFCFF', 'F8696B']],
  ['Blue - White - Red', ['F8696B', 'FCFCFF', '5A8AC6']],
  ['Red - White - Blue', ['5A8AC6', 'FCFCFF', 'F8696B']],
  ['White - Red', ['F8696B', 'FCFCFF']],
  ['Red - White', ['FCFCFF', 'F8696B']],
  ['Green - White', ['FCFCFF', '63BE7B']],
  ['White - Green', ['63BE7B', 'FCFCFF']],
  ['Green - Yellow', ['FFEF9C', '63BE7B']],
  ['Yellow - Green', ['63BE7B', 'FFEF9C']],
];

const ICON_SETS: readonly [string, string][] = [
  ['3 Arrows (Coloured)', '3Arrows'],
  ['3 Arrows (Grey)', '3ArrowsGray'],
  ['4 Arrows (Coloured)', '4Arrows'],
  ['5 Arrows (Coloured)', '5Arrows'],
  ['3 Traffic Lights', '3TrafficLights1'],
  ['3 Traffic Lights (Rimmed)', '3TrafficLights2'],
  ['3 Signs', '3Signs'],
  ['4 Traffic Lights', '4TrafficLights'],
  ['3 Symbols (Circled)', '3Symbols'],
  ['3 Flags', '3Flags'],
  ['3 Stars', '3Stars'],
  ['5 Ratings', '5Rating'],
  ['5 Quarters', '5Quarters'],
  ['5 Boxes', '5Boxes'],
];

const thresholds = (set: string): GridCfValue[] => {
  const n = Number(set[0]) || 3;
  return Array.from({ length: n - 1 }, (_, i) => v('Percent', String(Math.round(((i + 1) * 100) / n))));
};

const quick = (preset: string, label: string) => mitem('ui.grid.cf_rule', { label, args: { preset } });

/** Home ▸ Styles ▸ Conditional Formatting. */
export const CF_MENU: readonly RibbonMenuEntry[] = [
  msub(
    'Highlight Cells Rules',
    [
      quick('greater', 'Greater Than…'),
      quick('less', 'Less Than…'),
      quick('between', 'Between…'),
      quick('equal', 'Equal To…'),
      quick('text', 'Text that Contains…'),
      quick('date', 'A Date Occurring…'),
      quick('duplicate', 'Duplicate Values…'),
      msep(),
      mitem('ui.grid.cf_rule', { label: 'More Rules…' }),
    ],
    'ConditionalFormatting',
  ),
  msub('Top/Bottom Rules', [
    quick('top10', 'Top 10 Items…'),
    quick('top10pct', 'Top 10%…'),
    quick('bottom10', 'Bottom 10 Items…'),
    quick('bottom10pct', 'Bottom 10%…'),
    quick('above', 'Above Average…'),
    quick('below', 'Below Average…'),
  ]),
  msub('Data Bars', [
    mhead('Gradient Fill'),
    ...BARS.map(([name, color]) =>
      mitem('grid.styles.add_rule', {
        label: `${name} Data Bar`,
        args: rule({
          DataBar: {
            min: v('Automatic'),
            max: v('Automatic'),
            color,
            gradient: true,
            show_value: true,
            negative_color: 'FF0000',
            border_color: null,
          },
        }),
      }),
    ),
    mhead('Solid Fill'),
    ...BARS.map(([name, color]) =>
      mitem('grid.styles.add_rule', {
        label: `Solid ${name} Data Bar`,
        args: rule({
          DataBar: {
            min: v('Automatic'),
            max: v('Automatic'),
            color,
            gradient: false,
            show_value: true,
            negative_color: 'FF0000',
            border_color: null,
          },
        }),
      }),
    ),
  ]),
  msub(
    'Color Scales',
    SCALES.map(([name, colors]) =>
      mitem('grid.styles.add_rule', {
        label: `${name} Color Scale`,
        args: rule({
          ColorScale: {
            points: colors.length === 3 ? [v('Min'), v('Percentile', '50'), v('Max')] : [v('Min'), v('Max')],
            colors,
          },
        }),
      }),
    ),
  ),
  msub(
    'Icon Sets',
    ICON_SETS.map(([name, set]) =>
      mitem('grid.styles.add_rule', {
        label: name,
        args: rule({ IconSet: { set, thresholds: thresholds(set), reverse: false, show_value: true } }),
      }),
    ),
  ),
  msep(),
  mitem('ui.grid.cf_rule', { label: 'New Rule…' }),
  msub('Clear Rules', [
    mitem('grid.styles.clear_rules', { label: 'Clear Rules from Selected Cells', args: { sheet: false } }),
    mitem('grid.styles.clear_rules', { label: 'Clear Rules from Entire Sheet', args: { sheet: true } }),
  ]),
  mitem('ui.grid.cf_manage', { label: 'Manage Rules…' }),
];

/** Excel's built-in cell styles in the gallery's groups. */
const CELL_STYLE_GROUPS: readonly [string, readonly string[]][] = [
  ['Good, Bad and Neutral', ['Normal', 'Bad', 'Good', 'Neutral']],
  [
    'Data and Model',
    [
      'Calculation',
      'Check Cell',
      'Explanatory Text',
      'Input',
      'Linked Cell',
      'Note',
      'Output',
      'Warning Text',
    ],
  ],
  ['Titles and Headings', ['Heading 1', 'Heading 2', 'Heading 3', 'Heading 4', 'Title', 'Total']],
  [
    'Themed Cell Styles',
    [1, 2, 3, 4, 5, 6].flatMap((n) => [
      `20% - Accent${n}`,
      `40% - Accent${n}`,
      `60% - Accent${n}`,
      `Accent${n}`,
    ]),
  ],
  ['Number Format', ['Comma', 'Comma [0]', 'Currency', 'Currency [0]', 'Percent']],
];

export const CELL_STYLES_MENU: readonly RibbonMenuEntry[] = CELL_STYLE_GROUPS.map(([group, names]) =>
  msub(
    group,
    names.map((name) => mitem('grid.styles.apply_cell_style', { label: name, args: { name } })),
  ),
);

/** Home ▸ Editing ▸ Sort & Filter. */
export const SORT_FILTER_MENU: readonly RibbonMenuEntry[] = [
  mitem('grid.data.sort_ascending'),
  mitem('grid.data.sort_descending'),
  mitem('grid.data.sort', { label: 'Custom Sort…' }),
  msep(),
  mitem('grid.data.toggle_filter'),
  mitem('grid.data.clear_filter'),
  mitem('grid.data.reapply_filter'),
];

const special = (s: GridSpecial, label: string) =>
  mitem('grid.editing.select_special', { label, args: { special: s } });

/** Home ▸ Editing ▸ Find & Select. */
export const FIND_SELECT_MENU: readonly RibbonMenuEntry[] = [
  mitem('ui.grid.find', { label: 'Find…' }),
  mitem('suite.edit.replace', { label: 'Replace…' }),
  mitem('suite.edit.go_to', { label: 'Go To…' }),
  mitem('ui.grid.go_to_special', { label: 'Go To Special…' }),
  msep(),
  special('Formulas', 'Formulas'),
  special('Notes', 'Notes'),
  special('ConditionalFormats', 'Conditional Formatting'),
  special('Constants', 'Constants'),
  special('DataValidation', 'Data Validation'),
];

const margins = (
  label: string,
  m: { top: number; bottom: number; left: number; right: number; header: number; footer: number },
) => mitem('grid.page_layout.page_setup', { label, args: { setup: { margins: m } } });

export const MARGINS_MENU: readonly RibbonMenuEntry[] = [
  margins('Normal: Top 0.75" Bottom 0.75" Left 0.7" Right 0.7"', {
    top: 0.75,
    bottom: 0.75,
    left: 0.7,
    right: 0.7,
    header: 0.3,
    footer: 0.3,
  }),
  margins('Wide: Top 1" Bottom 1" Left 1" Right 1"', {
    top: 1,
    bottom: 1,
    left: 1,
    right: 1,
    header: 0.5,
    footer: 0.5,
  }),
  margins('Narrow: Top 0.75" Bottom 0.75" Left 0.25" Right 0.25"', {
    top: 0.75,
    bottom: 0.75,
    left: 0.25,
    right: 0.25,
    header: 0.3,
    footer: 0.3,
  }),
  msep(),
  mitem('ui.grid.custom_margins', { label: 'Custom Margins…' }),
];

export const ORIENTATION_MENU: readonly RibbonMenuEntry[] = [
  mitem('grid.page_layout.page_setup', { label: 'Portrait', args: { setup: { landscape: false } } }),
  mitem('grid.page_layout.page_setup', { label: 'Landscape', args: { setup: { landscape: true } } }),
];

export const SIZE_MENU: readonly RibbonMenuEntry[] = [
  ...(
    [
      [1, 'Letter 8.5" × 11"'],
      [5, 'Legal 8.5" × 14"'],
      [7, 'Executive 7.25" × 10.5"'],
      [3, 'Tabloid 11" × 17"'],
      [8, 'A3 297 × 420 mm'],
      [9, 'A4 210 × 297 mm'],
      [11, 'A5 148 × 210 mm'],
      [13, 'B5 182 × 257 mm'],
    ] as const
  ).map(([paper, label]) => mitem('grid.page_layout.page_setup', { label, args: { setup: { paper } } })),
  msep(),
  mitem('suite.layout.page_setup', { label: 'More Paper Sizes…' }),
];

const area = (action: GridPrintAreaAction, label: string) =>
  mitem('grid.page_layout.set_print_area', { label, args: { action } });

export const PRINT_AREA_MENU: readonly RibbonMenuEntry[] = [
  area('Set', 'Set Print Area'),
  area('Add', 'Add to Print Area'),
  area('Clear', 'Clear Print Area'),
];

const brk = (action: GridBreakAction, label: string) =>
  mitem('grid.page_layout.page_break', { label, args: { action } });

export const BREAKS_MENU: readonly RibbonMenuEntry[] = [
  brk('Insert', 'Insert Page Break'),
  brk('Remove', 'Remove Page Break'),
  brk('Reset', 'Reset All Page Breaks'),
];

const fitWidth = (n: number, label: string) =>
  mitem('grid.page_layout.page_setup', { label, args: { setup: { fit_width: n, fit_height: 0 } } });

export const SCALE_MENU: readonly RibbonMenuEntry[] = [
  mhead('Width'),
  mitem('grid.page_layout.page_setup', { label: 'Automatic', args: { setup: { fit: false, scale: 100 } } }),
  fitWidth(1, '1 page wide'),
  fitWidth(2, '2 pages wide'),
  mhead('Height'),
  mitem('grid.page_layout.page_setup', { label: '1 page tall', args: { setup: { fit_height: 1 } } }),
  mitem('grid.page_layout.page_setup', {
    label: 'Fit Sheet on One Page',
    args: { setup: { fit_width: 1, fit_height: 1 } },
  }),
  mhead('Scale'),
  ...[50, 75, 100, 125].map((scale) =>
    mitem('grid.page_layout.page_setup', { label: `${scale}%`, args: { setup: { scale, fit: false } } }),
  ),
  msep(),
  mitem('suite.layout.page_setup', { label: 'More Scaling Options…' }),
];

export const CALCULATION_MENU: readonly RibbonMenuEntry[] = [
  mitem('ui.grid.calc_automatic', { label: 'Automatic' }),
  mitem('ui.grid.calc_automaticexcepttables', { label: 'Automatic Except for Data Tables' }),
  mitem('ui.grid.calc_manual', { label: 'Manual' }),
  msep(),
  mitem('ui.grid.iterative', { label: 'Iterative Calculation…' }),
];

export const REMOVE_ARROWS_MENU: readonly RibbonMenuEntry[] = [
  mitem('grid.formulas.remove_arrows', { label: 'Remove Arrows', args: { which: 'All' } }),
  mitem('grid.formulas.remove_arrows', { label: 'Remove Precedent Arrows', args: { which: 'Precedents' } }),
  mitem('grid.formulas.remove_arrows', { label: 'Remove Dependent Arrows', args: { which: 'Dependents' } }),
];

export const VALIDATION_MENU: readonly RibbonMenuEntry[] = [
  mitem('grid.data.data_validation', { label: 'Data Validation…' }),
  mitem('grid.data.circle_invalid', { label: 'Circle Invalid Data', args: { on: true } }),
  mitem('grid.data.circle_invalid', { label: 'Clear Validation Circles', args: { on: false } }),
];

export const GROUP_MENU: readonly RibbonMenuEntry[] = [
  mitem('grid.data.group_rows', { label: 'Group…' }),
  mitem('grid.data.group', { label: 'Group Columns', args: { columns: true } }),
];

export const UNGROUP_MENU: readonly RibbonMenuEntry[] = [
  mitem('grid.data.ungroup_rows', { label: 'Ungroup…' }),
  mitem('grid.data.ungroup', { label: 'Ungroup Columns', args: { columns: true } }),
];

export const EXPORT_MENU: readonly RibbonMenuEntry[] = [
  mhead('The selection'),
  mitem('ui.grid.export_csv', { label: 'Selection as CSV (UTF-8)', args: { selection: true } }),
  mitem('ui.grid.export_tsv', { label: 'Selection as Tab-Separated Text', args: { selection: true } }),
  mitem('ui.grid.export_json', { label: 'Selection as JSON', args: { selection: true } }),
  mhead('The sheet'),
  mitem('ui.grid.export_csv', { label: 'Sheet as CSV (UTF-8)', args: { selection: false } }),
  mitem('ui.grid.export_tsv', { label: 'Sheet as Tab-Separated Text', args: { selection: false } }),
  mitem('ui.grid.export_json', { label: 'Sheet as JSON', args: { selection: false } }),
];

export const NOTES_MENU: readonly RibbonMenuEntry[] = [
  mitem('grid.review.new_note', { label: 'New Note' }),
  mitem('ui.grid.edit_note', { label: 'Edit Note' }),
  mitem('ui.grid.delete_note', { label: 'Delete Note' }),
  msep(),
  mitem('grid.review.next', { label: 'Previous Note', args: { backward: true } }),
  mitem('grid.review.next', { label: 'Next Note', args: { backward: false } }),
  mitem('grid.review.show_note', { label: 'Show/Hide Note' }),
  mitem('ui.grid.show_all_notes', { label: 'Show All Notes' }),
  msep(),
  mitem('ui.grid.convert_notes', { label: 'Convert to Comments' }),
];

export const LINK_MENU: readonly RibbonMenuEntry[] = [
  mitem('suite.insert.link', { label: 'Insert Link…' }),
  mitem('ui.grid.open_link', { label: 'Open Hyperlink' }),
  mitem('ui.grid.remove_link', { label: 'Remove Hyperlink' }),
];

export const NAMES_MENU: readonly RibbonMenuEntry[] = [
  mitem('grid.formulas.define_name', { label: 'Define Name…' }),
  mitem('grid.formulas.use_in_formula', { label: 'Paste Names…' }),
];

// ── Pictures and shapes ──────────────────────────────────────────────────────────────────────

/** The shapes of Insert ▸ Shapes and Change Shape. */
export const GRID_SHAPES: readonly (readonly [string, string, string])[] = [
  ['line', 'Line', 'Lines'],
  ['straightConnector1', 'Arrow', 'Lines'],
  ['rect', 'Rectangle', 'Rectangles'],
  ['roundRect', 'Rounded Rectangle', 'Rectangles'],
  ['ellipse', 'Oval', 'Basic Shapes'],
  ['triangle', 'Triangle', 'Basic Shapes'],
  ['diamond', 'Diamond', 'Basic Shapes'],
  ['hexagon', 'Hexagon', 'Basic Shapes'],
  ['can', 'Cylinder', 'Basic Shapes'],
  ['rightArrow', 'Right Arrow', 'Block Arrows'],
  ['leftRightArrow', 'Left-Right Arrow', 'Block Arrows'],
  ['upArrow', 'Up Arrow', 'Block Arrows'],
  ['chevron', 'Chevron', 'Block Arrows'],
  ['flowChartProcess', 'Flowchart: Process', 'Flowchart'],
  ['flowChartDecision', 'Flowchart: Decision', 'Flowchart'],
  ['flowChartTerminator', 'Flowchart: Terminator', 'Flowchart'],
  ['star5', '5-Point Star', 'Stars and Banners'],
  ['wedgeRoundRectCallout', 'Speech Bubble', 'Callouts'],
  ['cloud', 'Cloud', 'Basic Shapes'],
  ['heart', 'Heart', 'Basic Shapes'],
];

const shapeMenu = (command: string): RibbonMenuEntry[] => {
  const out: RibbonMenuEntry[] = [];
  let group = '';
  for (const [preset, label, g] of GRID_SHAPES) {
    if (command === 'ui.grid.change_shape' && g === 'Lines') continue;
    if (g !== group) {
      out.push(mhead(g));
      group = g;
    }
    out.push(mitem(command, { label, args: { preset } }));
  }
  return out;
};

/** Insert ▸ Shapes ▾: a click places the shape at the active cell. */
export const SHAPES_MENU: readonly RibbonMenuEntry[] = shapeMenu('grid.insert.shape');
const CHANGE_SHAPE_MENU: readonly RibbonMenuEntry[] = shapeMenu('ui.grid.change_shape');

const RESET_MENU: readonly RibbonMenuEntry[] = [
  mitem('grid.picture.reset', { label: 'Reset Picture', args: { size: false } }),
  mitem('ui.grid.reset_picture_size', { label: 'Reset Picture & Size' }),
];
const FORWARD_MENU: readonly RibbonMenuEntry[] = [
  mitem('suite.arrange.bring_forward', { label: 'Bring Forward' }),
  mitem('ui.grid.bring_to_front', { label: 'Bring to Front' }),
];
const BACKWARD_MENU: readonly RibbonMenuEntry[] = [
  mitem('suite.arrange.send_backward', { label: 'Send Backward' }),
  mitem('ui.grid.send_to_back', { label: 'Send to Back' }),
];
const WEIGHT_MENU: readonly RibbonMenuEntry[] = [0.25, 0.5, 0.75, 1, 1.5, 2.25, 3, 4.5, 6].map((points) =>
  mitem('ui.grid.outline_weight', { label: `${points} pt`, args: { points } }),
);

const sizeGroup = () =>
  rgroup(
    'size',
    'Size',
    [
      rnumber('ui.grid.object_height', {
        unit: 'pt',
        size: 'medium',
        min: 0,
        max: 1584,
        step: 1,
        keyTip: 'H',
      }),
      rnumber('ui.grid.object_width', {
        unit: 'pt',
        size: 'medium',
        min: 1,
        max: 1584,
        step: 1,
        keyTip: 'W',
      }),
    ],
    { launcher: { command: 'ui.grid.format_object', label: 'Size and Properties' } },
  );

const arrangeGroup = () =>
  rgroup('arrange', 'Arrange', [
    rsplit('suite.arrange.bring_forward', FORWARD_MENU, { size: 'medium', keyTip: 'AF' }),
    rsplit('suite.arrange.send_backward', BACKWARD_MENU, { size: 'medium', keyTip: 'AE' }),
  ]);

/** Picture Format (contextual, while a picture is selected). */
export const gridPictureFormatTab = (): RibbonDataTab =>
  rtab(
    'pictureFormat',
    'Picture Format',
    [
      rgroup('adjust', 'Adjust', [
        rbutton('suite.picture.alt_text', { size: 'large', keyTip: 'AT' }),
        rsplit('grid.picture.reset', RESET_MENU, { size: 'medium', keyTip: 'Q', args: { size: false } }),
      ]),
      arrangeGroup(),
      rgroup('crop', 'Crop', [rbutton('suite.picture.crop', { size: 'large', keyTip: 'C' })]),
      sizeGroup(),
    ],
    { keyTip: 'JP', contextual: { set: 'pictureTools', label: 'Picture Tools', context: 'picture' } },
  );

/** Shape Format (contextual, while a shape or line is selected). */
export const gridShapeFormatTab = (): RibbonDataTab =>
  rtab(
    'shapeFormat',
    'Shape Format',
    [
      rgroup('insertShapes', 'Insert Shapes', [
        rdropdown('grid.insert.shape', SHAPES_MENU, { size: 'large', keyTip: 'SH', label: 'Shapes' }),
        rdropdown('ui.grid.change_shape', CHANGE_SHAPE_MENU, { size: 'medium', keyTip: 'E' }),
        rbutton('ui.grid.shape_text', { size: 'medium', keyTip: 'X' }),
      ]),
      rgroup('shapeStyles', 'Shape Styles', [
        rcolor('ui.grid.shape_fill', {
          allowNone: true,
          defaultColor: '#4472C4',
          size: 'medium',
          keyTip: 'SI',
        }),
        rcolor('ui.grid.shape_outline', {
          allowNone: true,
          defaultColor: '#1F3864',
          size: 'medium',
          keyTip: 'SO',
        }),
        rdropdown('ui.grid.outline_weight', WEIGHT_MENU, { size: 'medium', keyTip: 'SW' }),
        rcolor('ui.grid.shape_text_color', { defaultColor: '#FFFFFF', size: 'medium', keyTip: 'TF' }),
      ]),
      rgroup('accessibility', 'Accessibility', [
        rbutton('suite.picture.alt_text', { size: 'large', keyTip: 'AT' }),
      ]),
      arrangeGroup(),
      sizeGroup(),
    ],
    { keyTip: 'JD', contextual: { set: 'drawingTools', label: 'Drawing Tools', context: 'shape' } },
  );

// ── Table Design (contextual) ─────────────────────────────────────────────────────────────────

/** Office theme colours of the six accents and dark 1 (the built-in styles' slots). */
const ACCENTS = ['000000', '4472C4', 'ED7D31', 'A5A5A5', 'FFC000', '5B9BD5', '70AD47'];

const tint = (hex: string, t: number): string => {
  const n = Number.parseInt(hex, 16);
  const ch = (s: number) => Math.round(((n >> s) & 255) + (255 - ((n >> s) & 255)) * t);
  return [16, 8, 0].map((s) => ch(s).toString(16).padStart(2, '0')).join('');
};

/** A table style's preview as an SVG picture (header, banded rows, lines). */
function tablePreview(family: 'Light' | 'Medium' | 'Dark', n: number): string {
  const slot = (n - 1) % 7;
  const accent = ACCENTS[slot] ?? '4472C4';
  const dark = slot === 0;
  const header =
    family === 'Light'
      ? n <= 7
        ? null
        : accent
      : family === 'Medium'
        ? n <= 14
          ? accent
          : dark
            ? '000000'
            : accent
        : dark
          ? '000000'
          : accent;
  const band =
    family === 'Light'
      ? tint(dark ? '7F7F7F' : accent, 0.8)
      : family === 'Medium'
        ? tint(dark ? '7F7F7F' : accent, n <= 7 ? 0.8 : n <= 14 ? 0.6 : 0.8)
        : tint(dark ? '404040' : accent, 0.4);
  const line = family === 'Light' ? accent : tint(accent, 0.4);
  const body = family === 'Dark' ? tint(dark ? '595959' : accent, 0.25) : 'FFFFFF';
  const rows = [0, 1, 2, 3].map((i) => {
    const y = 8 + i * 6;
    const fill = i % 2 === 0 ? band : body;
    return `<rect x="1" y="${y}" width="42" height="6" fill="#${fill}"/>`;
  });
  const head = header
    ? `<rect x="1" y="2" width="42" height="6" fill="#${header}"/>`
    : `<line x1="1" y1="8" x2="43" y2="8" stroke="#${accent}" stroke-width="1"/>`;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 44 34" width="44" height="34"><rect x="1" y="2" width="42" height="30" fill="#${body}" stroke="#${line}" stroke-width="0.6"/>${rows.join('')}${head}</svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

/** The 60 built-in table styles as gallery tiles. */
export const TABLE_STYLE_ENTRIES: readonly RibbonGalleryEntry[] = (
  [
    ['Light', 21],
    ['Medium', 28],
    ['Dark', 11],
  ] as const
).flatMap(([family, max]) =>
  Array.from({ length: max }, (_, i) => ({
    id: `TableStyle${family}${i + 1}`,
    // Excel's name, shown as the tile's tooltip.
    label: `Table Style ${family} ${i + 1}`,
    look: { image: tablePreview(family, i + 1) },
  })),
);

const TOTALS: readonly [GridTotals, string][] = [
  ['None', 'None'],
  ['Average', 'Average'],
  ['Count', 'Count'],
  ['CountNumbers', 'Count Numbers'],
  ['Max', 'Max'],
  ['Min', 'Min'],
  ['Sum', 'Sum'],
  ['StdDev', 'StdDev'],
  ['Var', 'Var'],
];

export const gridTableDesignTab = (): RibbonDataTab =>
  rtab(
    'gridTableDesign',
    'Table Design',
    [
      rgroup('properties', 'Properties', [
        rcombo('grid.table.rename', [], {
          width: 120,
          placeholder: 'Table name',
          allowCustomValue: true,
        }),
        rbutton('ui.grid.resize_table', { size: 'medium' }),
      ]),
      rgroup('tools', 'Tools', [
        rbutton('ui.grid.table_remove_duplicates', { size: 'medium', keyTip: 'M' }),
        rbutton('ui.grid.convert_to_range', { size: 'medium', keyTip: 'G' }),
        rdropdown(
          undefined,
          TOTALS.map(([fn, label]) => mitem('ui.grid.table_totals', { label, args: { fn } })),
          { id: 'table.totals', label: 'Total Function', icon: 'Sigma', size: 'medium' },
        ),
      ]),
      rgroup('styleOptions', 'Table Style Options', [
        rtoggle('ui.grid.table_header_row', { size: 'small', checkbox: true, keyTip: 'O' }),
        rtoggle('ui.grid.table_total_row', { size: 'small', checkbox: true, keyTip: 'T' }),
        rtoggle('ui.grid.table_banded_rows', { size: 'small', checkbox: true, keyTip: 'R' }),
        rtoggle('ui.grid.table_first_column', { size: 'small', checkbox: true, keyTip: 'F' }),
        rtoggle('ui.grid.table_last_column', { size: 'small', checkbox: true, keyTip: 'L' }),
        rtoggle('ui.grid.table_banded_columns', { size: 'small', checkbox: true, keyTip: 'B' }),
        rtoggle('ui.grid.table_filter_button', { size: 'small', checkbox: true, keyTip: 'W' }),
      ]),
      rgroup('tableStyles', 'Table Styles', [
        rgallery('grid.table.style', TABLE_STYLE_ENTRIES, {
          columns: 7,
          itemWidth: 52,
          itemHeight: 40,
          keyTip: 'S',
        }),
      ]),
    ],
    { keyTip: 'JT', contextual: { set: 'tableTools', label: 'Table Tools', context: 'table' } },
  );

// ── builders ─────────────────────────────────────────────────────────────────────────────────

type Args = Readonly<Record<string, unknown>>;

class FeatureArgsError extends Error {
  override readonly name = 'CommandArgsError';
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);
const isStr = (v: unknown): v is string => typeof v === 'string';
const isNum = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const isUint = (v: unknown): v is number => isNum(v) && Number.isInteger(v) && v >= 0;
const isBool = (v: unknown): v is boolean => typeof v === 'boolean';

function need<T>(id: string, a: Args, key: string, ok: (v: unknown) => v is T, what: string): T {
  const x = a[key];
  if (!ok(x)) throw new FeatureArgsError(`${id}: "${key}" must be ${what}`);
  return x;
}
function opt<T>(id: string, a: Args, key: string, ok: (v: unknown) => v is T, what: string): T | null {
  const x = a[key];
  if (x === undefined || x === null) return null;
  if (!ok(x)) throw new FeatureArgsError(`${id}: "${key}" must be ${what} or absent`);
  return x;
}
const flag = (id: string, a: Args, key: string, dflt = false) =>
  opt(id, a, key, isBool, 'true or false') ?? dflt;
const range = (id: string, a: Args, key: string): GridRange => {
  const r = need(id, a, key, isObj, 'a range');
  if (!isUint(r.r0) || !isUint(r.c0) || !isUint(r.r1) || !isUint(r.c1))
    throw new FeatureArgsError(`${id}: "${key}" must have r0, c0, r1 and c1`);
  return { r0: r.r0, c0: r.c0, r1: r.r1, c1: r.c1 };
};

const EMPTY_SETUP: GridPageSetup = {
  landscape: null,
  paper: null,
  margins: null,
  scale: null,
  fit_width: null,
  fit_height: null,
  fit: null,
  gridlines: null,
  headings: null,
  center_h: null,
  center_v: null,
  header: null,
  footer: null,
  first_page: null,
  over_then_down: null,
  black_and_white: null,
  print_area: null,
  title_rows: null,
  title_cols: null,
};

const g = (c: GridCommand): Command => ({ Grid: c });

const TABLE: Readonly<Record<string, (a: Args, id: string) => GridCommand>> = {
  'grid.data.sort_ascending': () => 'SortAscending',
  'grid.data.sort_descending': () => 'SortDescending',
  'grid.data.apply_sort': (a, id) => ({
    ApplySort: { spec: need(id, a, 'spec', isObj, 'a sort spec') as unknown as GridSortSpec },
  }),
  'grid.data.toggle_filter': () => 'ToggleFilter',
  'grid.data.filter_column': (a, id) => ({
    FilterColumn: {
      row: need(id, a, 'row', isUint, 'a row'),
      col: need(id, a, 'col', isUint, 'a column'),
      filter: opt(id, a, 'filter', isObj, 'a filter') as unknown as GridColumnFilter | null,
    },
  }),
  'grid.data.sort_filter_column': (a, id) => ({
    SortFilterColumn: {
      row: need(id, a, 'row', isUint, 'a row'),
      col: need(id, a, 'col', isUint, 'a column'),
      descending: flag(id, a, 'descending'),
      on: (opt(id, a, 'on', isStr, 'what to sort on') ?? 'Values') as GridSortOn,
      color: opt(id, a, 'color', isStr, 'a colour'),
    },
  }),
  'grid.data.clear_filter': () => 'ClearFilter',
  'grid.data.reapply_filter': () => 'ReapplyFilter',
  'grid.styles.add_rule': (a, id) => ({
    AddConditionalFormat: { rule: need(id, a, 'rule', isObj, 'a rule') as unknown as GridCfRule },
  }),
  'grid.styles.set_rules': (a, id) => ({
    SetConditionalFormats: {
      rules: need(id, a, 'rules', Array.isArray, 'a list of rules') as unknown as GridCfRule[],
      sheet: flag(id, a, 'sheet'),
    },
  }),
  'grid.styles.clear_rules': (a, id) => ({ ClearConditionalFormats: { sheet: flag(id, a, 'sheet') } }),
  'grid.data.set_validation': (a, id) => ({
    SetValidation: { rule: opt(id, a, 'rule', isObj, 'a rule') as unknown as GridValidation | null },
  }),
  'grid.data.circle_invalid': (a, id) => ({ CircleInvalid: { on: flag(id, a, 'on', true) } }),
  'grid.table.create': (a, id) => ({
    CreateTable: {
      range: a.range ? range(id, a, 'range') : null,
      header: flag(id, a, 'header', true),
      style: opt(id, a, 'style', isStr, 'a style') ?? 'TableStyleMedium2',
    },
  }),
  'grid.table.style': (a, id) => ({
    TableStyle: {
      style: opt(id, a, 'value', isStr, 'a style') ?? opt(id, a, 'style', isStr, 'a style') ?? '',
    },
  }),
  'grid.table.options': (a, id) => ({
    TableOptions: { options: need(id, a, 'options', isObj, 'table options') as unknown as GridTableOptions },
  }),
  'grid.table.totals': (a, id) => ({
    TableTotals: {
      col: need(id, a, 'col', isUint, 'a column'),
      totals: need(id, a, 'totals', isStr, 'a function') as GridTotals,
    },
  }),
  'grid.table.rename': (a, id) => ({
    RenameTable: { name: opt(id, a, 'value', isStr, 'a name') ?? need(id, a, 'name', isStr, 'a name') },
  }),
  'grid.table.resize': (a, id) => ({ ResizeTable: { range: range(id, a, 'range') } }),
  'grid.table.convert_to_range': () => 'ConvertToRange',
  'grid.styles.apply_cell_style': (a, id) => ({
    ApplyCellStyle: { name: opt(id, a, 'value', isStr, 'a style') ?? need(id, a, 'name', isStr, 'a style') },
  }),
  'grid.clipboard.paint_format': (a, id) => ({ PaintFormat: { from: range(id, a, 'from') } }),
  'grid.editing.replace': (a, id) => ({
    Replace: {
      spec: need(id, a, 'spec', isObj, 'find options') as unknown as GridFindSpec,
      with: need(id, a, 'with', isStr, 'the replacement'),
      all: flag(id, a, 'all'),
    },
  }),
  'grid.editing.select_special': (a, id) => ({
    SelectSpecial: { special: need(id, a, 'special', isStr, 'what to select') as GridSpecial },
  }),
  'grid.review.set_note': (a, id) => ({
    SetNote: {
      text: opt(id, a, 'text', isStr, 'text'),
      author: opt(id, a, 'author', isStr, 'a name') ?? 'Author',
      cell: opt(id, a, 'cell', isStr, 'a cell such as D4'),
    },
  }),
  'grid.review.show_note': (a, id) => ({ ShowNote: { show: opt(id, a, 'show', isBool, 'true or false') } }),
  'grid.review.show_all_notes': (a, id) => ({ ShowAllNotes: { show: flag(id, a, 'show', true) } }),
  'grid.review.add_comment': (a, id) => ({
    AddComment: {
      text: need(id, a, 'text', isStr, 'text'),
      author: opt(id, a, 'author', isStr, 'a name') ?? 'Author',
      reply_to: opt(id, a, 'reply_to', isStr, 'a comment id'),
      now_ms: opt(id, a, 'now_ms', isUint, 'a time') ?? Date.now(),
    },
  }),
  'grid.review.edit_comment': (a, id) => ({
    EditComment: { id: need(id, a, 'id', isStr, 'a comment id'), text: need(id, a, 'text', isStr, 'text') },
  }),
  'grid.review.delete': (a, id) => ({ DeleteComment: { id: opt(id, a, 'id', isStr, 'a comment id') } }),
  'grid.review.resolve_comment': (a, id) => ({
    ResolveComment: { id: need(id, a, 'id', isStr, 'a comment id'), done: flag(id, a, 'done', true) },
  }),
  'grid.review.next': (a, id) => ({ NextComment: { backward: flag(id, a, 'backward') } }),
  'grid.review.convert_notes': (a, id) => ({
    ConvertNotes: {
      author: opt(id, a, 'author', isStr, 'a name') ?? 'Author',
      now_ms: opt(id, a, 'now_ms', isUint, 'a time') ?? Date.now(),
    },
  }),
  'grid.insert.set_link': (a, id) => ({
    SetLink: { link: opt(id, a, 'link', isObj, 'a link') as unknown as GridLink | null },
  }),
  'grid.page_layout.page_setup': (a, id) => ({
    PageSetup: {
      setup: { ...EMPTY_SETUP, ...(need(id, a, 'setup', isObj, 'page setup') as Partial<GridPageSetup>) },
    },
  }),
  'grid.page_layout.set_print_area': (a, id) => ({
    PrintArea: { action: (opt(id, a, 'action', isStr, 'Set, Add or Clear') ?? 'Set') as GridPrintAreaAction },
  }),
  'grid.page_layout.page_break': (a, id) => ({
    PageBreak: {
      action: (opt(id, a, 'action', isStr, 'Insert, Remove or Reset') ?? 'Insert') as GridBreakAction,
    },
  }),
  'grid.page_layout.move_page_break': (a, id) => ({
    MovePageBreak: {
      columns: flag(id, a, 'columns'),
      from: need(id, a, 'from', isUint, 'a row or column'),
      to: need(id, a, 'to', isUint, 'a row or column'),
    },
  }),
  'grid.page_layout.print_gridlines': () => 'PrintGridlines',
  'grid.page_layout.print_headings': () => 'PrintHeadings',
  'grid.names.define': (a, id) => ({
    DefineName: {
      name: need(id, a, 'name', isStr, 'a name'),
      refers_to: need(id, a, 'refers_to', isStr, 'a reference'),
      scope: opt(id, a, 'scope', isUint, 'a sheet index'),
      comment: opt(id, a, 'comment', isStr, 'a comment') ?? '',
      previous: null,
    },
  }),
  'grid.names.delete': (a, id) => ({
    DeleteName: {
      name: need(id, a, 'name', isStr, 'a name'),
      scope: opt(id, a, 'scope', isUint, 'a sheet index'),
    },
  }),
  'grid.names.create': (a, id) => ({
    CreateNames: {
      top: flag(id, a, 'top', true),
      left: flag(id, a, 'left'),
      bottom: flag(id, a, 'bottom'),
      right: flag(id, a, 'right'),
    },
  }),
  'grid.formulas.trace_precedents': () => 'TracePrecedents',
  'grid.formulas.trace_dependents': () => 'TraceDependents',
  'grid.formulas.remove_arrows': (a, id) => ({
    RemoveArrows: {
      which: (opt(id, a, 'which', isStr, 'All, Precedents or Dependents') ?? 'All') as GridArrows,
    },
  }),
  'grid.formulas.calculation_mode': (a, id) => ({
    CalculationMode: { mode: need(id, a, 'mode', isStr, 'a calculation mode') as GridCalcMode },
  }),
  'grid.formulas.trace_formula': (a, id) => ({ FormulaLab: { open: flag(id, a, 'open', true) } }),
  'grid.formulas.watch': (a, id) => ({
    Watch: {
      add: flag(id, a, 'add', true),
      sheet: need(id, a, 'sheet', isUint, 'a sheet index'),
      row: need(id, a, 'row', isUint, 'a row'),
      col: need(id, a, 'col', isUint, 'a column'),
    },
  }),
  'grid.data.import': (a, id) => ({
    Import: { import: need(id, a, 'import', isObj, 'an import') as unknown as GridImport },
  }),
  'grid.data.refresh_all': () => 'RefreshAll',
  'grid.data.split_column': (a, id) => ({
    TextToColumns: { options: need(id, a, 'options', isObj, 'options') as unknown as GridTextToColumns },
  }),
  'grid.data.dedupe': (a, id) => ({
    RemoveDuplicates: {
      columns: (opt(id, a, 'columns', Array.isArray, 'a list of columns') ?? []) as number[],
      header: opt(id, a, 'header', isBool, 'true or false'),
    },
  }),
  'grid.insert.shape': (a, id) => ({
    InsertShape: {
      preset: opt(id, a, 'preset', isStr, 'a shape') ?? opt(id, a, 'value', isStr, 'a shape') ?? 'rect',
      text: opt(id, a, 'text', isStr, 'text'),
    },
  }),
  'grid.insert.picture': (a, id) => {
    const bytes = a.bytes;
    if (!(bytes instanceof Uint8Array) && !(Array.isArray(bytes) && bytes.every(isUint)))
      throw new FeatureArgsError(`${id}: "bytes" must be the picture file`);
    return {
      InsertPicture: {
        name: need(id, a, 'name', isStr, 'a file name'),
        bytes: Array.from(bytes as ArrayLike<number>),
      },
    };
  },
  'grid.drawing.format': (a, id) => ({
    FormatObject: { format: need(id, a, 'format', isObj, 'a format') as unknown as GridObjectFormat },
  }),
  'grid.data.accept_invalid': () => 'AcceptInvalid',
  'grid.formulas.iterative': (a, id) => ({
    IterativeCalculation: {
      enabled: flag(id, a, 'enabled', true),
      max_iterations: opt(id, a, 'max_iterations', isUint, 'a whole number') ?? 100,
      max_change: opt(id, a, 'max_change', isNum, 'a number') ?? 0.001,
    },
  }),
  'grid.formulas.date_1904': (a, id) => ({ DateSystem: { date1904: flag(id, a, 'date1904', true) } }),
  'grid.picture.reset': (a, id) => ({ ResetPicture: { size: flag(id, a, 'size') } }),
  'grid.drawing.arrange': (a, id) => {
    const v = need(id, a, 'arrange', isStr, 'Forward, Front, Backward or Back');
    if (!['Forward', 'Front', 'Backward', 'Back'].includes(v))
      throw new FeatureArgsError(`${id}: "arrange" must be Forward, Front, Backward or Back`);
    return { ArrangeDrawing: { arrange: v as 'Forward' | 'Front' | 'Backward' | 'Back' } };
  },
  'grid.data.group': (a, id) => ({ Group: { columns: flag(id, a, 'columns') } }),
  'grid.data.ungroup': (a, id) => ({ Ungroup: { columns: flag(id, a, 'columns') } }),
  'grid.data.outline_detail': (a, id) => ({ OutlineDetail: { show: flag(id, a, 'show', true) } }),
  'grid.data.show_outline_symbols': () => 'OutlineSymbols',
  'grid.review.spell_change': (a, id) => ({
    SpellChange: {
      sheet: need(id, a, 'sheet', isUint, 'a sheet index'),
      row: need(id, a, 'row', isUint, 'a row'),
      col: need(id, a, 'col', isUint, 'a column'),
      offset: need(id, a, 'offset', isUint, 'an offset'),
      word: need(id, a, 'word', isStr, 'a word'),
      with: need(id, a, 'with', isStr, 'a word'),
      all: flag(id, a, 'all'),
    },
  }),
  'grid.review.spell_ignore': (a, id) => ({
    SpellIgnore: { word: need(id, a, 'word', isStr, 'a word'), add: flag(id, a, 'add') },
  }),
  'grid.review.load_dictionary': (a, id) => ({
    LoadDictionary: { aff: need(id, a, 'aff', isStr, 'text'), dic: need(id, a, 'dic', isStr, 'text') },
  }),
};

/** The builders of the core commands (spread into `GRID_BUILDERS`). */
export const GRID_FEATURE_BUILDERS: Readonly<Record<string, (a: Args) => Command>> = Object.freeze(
  Object.fromEntries(Object.entries(TABLE).map(([id, build]) => [id, (a: Args) => g(build(a, id))] as const)),
);

type GridCtxState = import('@kiln/protocol').GridState;
const fs = (ctx: CommandContext) => (ctx.gridState as GridCtxState | undefined)?.features;

/** Pressed states and values of the ribbon items read from the core's Grid state. */
export const GRID_FEATURE_STATE: {
  checked: Readonly<Record<string, (ctx: CommandContext) => boolean | undefined>>;
  values: Readonly<Record<string, (ctx: CommandContext) => unknown>>;
} = {
  checked: {
    'grid.data.toggle_filter': (c) => fs(c)?.filter,
    'grid.page_layout.print_gridlines': (c) => fs(c)?.print_gridlines,
    'grid.page_layout.print_headings': (c) => fs(c)?.print_headings,
    'grid.data.show_outline_symbols': (c) => fs(c)?.outline_symbols,
    'grid.review.show_note': (c) => fs(c)?.cards.some((k) => !k.thread),
  },
  values: {
    'grid.table.style': (c) => fs(c)?.table?.style,
    'grid.table.rename': (c) => fs(c)?.table?.name,
  },
};
