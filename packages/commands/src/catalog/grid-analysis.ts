/**
 * Kiln Grid's analysis features: the declarations of the core's PivotTable, sparkline,
 * Goal Seek, Subtotal, Flash Fill, protection and theme commands and of the editor's chrome ids;
 * how the UI builds each core command's typed `Command` from its arguments; and the PivotTable
 * Analyze, PivotTable Design and Sparkline contextual tabs, the Themes gallery and the Format
 * menu's Protection section.
 */
import type {
  Command,
  GridCommand,
  GridPivotEdit,
  GridSheetPermissions,
  GridSparkEdit,
  GridSparkKind,
  GridSubtotalSpec,
  GridThemePart,
} from '@kiln/protocol';
import {
  mhead,
  mitem,
  msep,
  type RibbonDataTab,
  type RibbonGalleryEntry,
  type RibbonMenuEntry,
  rbutton,
  rcolor,
  rcombo,
  rdropdown,
  rgallery,
  rgroup,
  rsplit,
  rtab,
  rtoggle,
} from '../ribbon.ts';
import type { CommandDeclaration } from '../types.ts';
import { DECK_THEMES } from './deck.ts';
import { c, GRID, section } from './define.ts';

const G = { apps: GRID };
const hidden = { palette: false } as const;

/** Declarations of the core commands and chrome ids. */
export const gridAnalysisCommands: readonly CommandDeclaration[] = [
  ...section('PivotTable', G, [
    c('grid.pivot.create', 'Create PivotTable', 'PivotTable', hidden),
    c('grid.pivot.edit', 'Change PivotTable', 'PivotTable', hidden),
    c('grid.pivot.refresh', 'Refresh', 'RefreshAll', { synonyms: ['refresh pivot table', 'update pivot'] }),
    c('grid.pivot.show_details', 'Show Details', 'TableProperties', hidden),
    c('ui.grid.pivot_refresh_all', 'Refresh All', 'RefreshAll', hidden),
    c('ui.grid.pivot_fields', 'Field List', 'List', { synonyms: ['pivottable fields', 'field list'] }),
    c('ui.grid.pivot_options', 'Options', 'Settings', { officeName: 'PivotTable Options' }),
    c('ui.grid.pivot_name', 'PivotTable Name', 'PencilLine', hidden),
    c('ui.grid.pivot_field_settings', 'Field Settings', 'TableProperties', {
      synonyms: ['value field settings', 'summarize values by', 'show values as'],
    }),
    c('ui.grid.pivot_change_source', 'Change Data Source', 'LiveGridRange', { synonyms: ['pivot source'] }),
    c('ui.grid.pivot_clear_all', 'Clear All', 'Eraser', hidden),
    c('ui.grid.pivot_clear_filters', 'Clear Filters', 'FunnelX', hidden),
    c('ui.grid.pivot_select', 'Entire PivotTable', 'SelectObjects', hidden),
    c('ui.grid.pivot_move', 'Move PivotTable', 'Move', {}),
    c('ui.grid.pivot_delete', 'Delete PivotTable', 'ClearAll', hidden),
    c('ui.grid.pivot_show_details', 'Show Details', 'TableProperties', hidden),
    c('ui.grid.pivot_subtotals_off', 'Do Not Show Subtotals', undefined, hidden),
    c('ui.grid.pivot_subtotals_bottom', 'Show All Subtotals at Bottom of Group', undefined, hidden),
    c('ui.grid.pivot_subtotals_top', 'Show All Subtotals at Top of Group', undefined, hidden),
    c('ui.grid.pivot_totals_off', 'Off for Rows and Columns', undefined, hidden),
    c('ui.grid.pivot_totals_on', 'On for Rows and Columns', undefined, hidden),
    c('ui.grid.pivot_totals_rows', 'On for Rows Only', undefined, hidden),
    c('ui.grid.pivot_totals_columns', 'On for Columns Only', undefined, hidden),
    c('ui.grid.pivot_layout_compact', 'Show in Compact Form', undefined, hidden),
    c('ui.grid.pivot_layout_outline', 'Show in Outline Form', undefined, hidden),
    c('ui.grid.pivot_layout_tabular', 'Show in Tabular Form', undefined, hidden),
    c('ui.grid.pivot_repeat_labels', 'Repeat All Item Labels', undefined, hidden),
    c('ui.grid.pivot_no_repeat_labels', 'Do Not Repeat Item Labels', undefined, hidden),
    c('ui.grid.pivot_blank_rows', 'Insert Blank Line after Each Item', undefined, hidden),
    c('ui.grid.pivot_no_blank_rows', 'Remove Blank Line after Each Item', undefined, hidden),
    c('ui.grid.pivot_row_headers', 'Row Headers', 'PanelLeft', hidden),
    c('ui.grid.pivot_col_headers', 'Column Headers', 'PanelTop', hidden),
    c('ui.grid.pivot_banded_rows', 'Banded Rows', 'Rows3', hidden),
    c('ui.grid.pivot_banded_cols', 'Banded Columns', 'Columns3', hidden),
    c('ui.grid.pivot_style', 'PivotTable Styles', 'FormatAsTable', hidden),
  ]),
  ...section('Sparkline', G, [
    c('grid.sparkline.insert', 'Insert Sparklines', 'SparklineLine', hidden),
    c('grid.sparkline.edit', 'Change Sparklines', 'SparklineLine', hidden),
    c('grid.data.show_outline_level', 'Show Outline Level', 'GroupRows', hidden),
    c('grid.data.toggle_outline_group', 'Show or Hide Group Detail', 'GroupRows', hidden),
    c('ui.grid.spark_edit_data', 'Edit Data', 'LiveGridRange', { officeName: 'Edit Group Location & Data' }),
    c('ui.grid.spark_hidden_empty', 'Hidden & Empty Cells…', 'EyeOff', hidden),
    c('ui.grid.spark_line', 'Line', 'SparklineLine', hidden),
    c('ui.grid.spark_column', 'Column', 'SparklineColumn', hidden),
    c('ui.grid.spark_win_loss', 'Win/Loss', 'SparklineWinLoss', hidden),
    c('ui.grid.spark_high', 'High Point', undefined, hidden),
    c('ui.grid.spark_low', 'Low Point', undefined, hidden),
    c('ui.grid.spark_negative', 'Negative Points', undefined, hidden),
    c('ui.grid.spark_first', 'First Point', undefined, hidden),
    c('ui.grid.spark_last', 'Last Point', undefined, hidden),
    c('ui.grid.spark_markers', 'Markers', undefined, hidden),
    c('ui.grid.spark_style', 'Sparkline Style', 'Palette', hidden),
    c('ui.grid.spark_color', 'Sparkline Color', 'FontColor', hidden),
    c('ui.grid.spark_marker_color', 'Marker Color', 'Palette', hidden),
    c('ui.grid.spark_show_axis', 'Show Axis', undefined, hidden),
    c('ui.grid.spark_axis_same_min', 'Same for All Sparklines (minimum)', undefined, hidden),
    c('ui.grid.spark_axis_same_max', 'Same for All Sparklines (maximum)', undefined, hidden),
    c('ui.grid.spark_right_to_left', 'Plot Data Right-to-Left', undefined, hidden),
    c('ui.grid.spark_group', 'Group', 'GroupObjects', hidden),
    c('ui.grid.spark_ungroup', 'Ungroup', 'Ungroup', hidden),
    c('ui.grid.spark_clear', 'Clear Selected Sparklines', 'Eraser', hidden),
    c('ui.grid.spark_clear_group', 'Clear Selected Sparkline Groups', 'Eraser', hidden),
  ]),
  ...section('Data', G, [
    c('grid.data.run_goal_seek', 'Goal Seek', 'GoalSeek', hidden),
    c('grid.data.apply_subtotal', 'Apply Subtotals', 'Subtotal', hidden),
  ]),
  ...section('Review ▸ Protect', G, [
    c('grid.review.set_sheet_protection', 'Set Sheet Protection', 'ProtectSheet', hidden),
    c('grid.review.set_workbook_protection', 'Set Workbook Protection', 'ProtectWorkbook', hidden),
    c('grid.cells.lock_cell', 'Lock Cell', 'Lock', hidden),
    c('ui.grid.lock_cell', 'Lock Cell', 'Lock', { synonyms: ['locked cells', 'unlock cells'] }),
  ]),
  ...section('Page Layout ▸ Themes', G, [
    c('grid.page_layout.apply_theme', 'Apply Theme', 'Palette', hidden),
    c('ui.grid.theme_colors', 'Colors', 'Palette', { officeName: 'Theme Colors' }),
    c('ui.grid.theme_fonts', 'Fonts', 'Type', { officeName: 'Theme Fonts' }),
  ]),
];

/** Ids of the ribbon items the Grid editor implements in its chrome. */
export const GRID_ANALYSIS_CHROME_COMMANDS: readonly string[] = Object.freeze([
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

// ── PivotTable Analyze and Design ─────────────────────────────────────────────────────────────

const ACCENTS = ['595959', '4472C4', 'ED7D31', 'A5A5A5', 'FFC000', '5B9BD5', '70AD47'];

const tint = (hex: string, t: number): string => {
  const n = Number.parseInt(hex, 16);
  const ch = (s: number) => {
    const v = (n >> s) & 255;
    return Math.round(t >= 0 ? v + (255 - v) * t : v * (1 + t));
  };
  return [16, 8, 0].map((s) => ch(s).toString(16).padStart(2, '0')).join('');
};

/** A PivotTable style's preview: header, subheading and grand-total bands in its accent. */
function pivotPreview(family: 'Light' | 'Medium' | 'Dark', n: number): string {
  const slot = (n - 1) % 7;
  const group = Math.floor((n - 1) / 7);
  const accent = ACCENTS[slot] ?? '4472C4';
  let header: string | null = null;
  let headerLine = accent;
  let body = 'FFFFFF';
  let sub = 'FFFFFF';
  let grand = 'FFFFFF';
  if (family === 'Light') {
    header = group === 1 ? accent : group === 2 ? tint(accent, 0.8) : null;
    sub = group === 3 ? tint(accent, 0.8) : 'FFFFFF';
    grand = group === 2 ? tint(accent, 0.8) : 'FFFFFF';
  } else if (family === 'Medium') {
    header = group % 2 === 0 ? accent : tint(accent, 0.6);
    sub = tint(accent, 0.8);
    grand = tint(accent, 0.6);
  } else {
    header = '262626';
    body = tint(accent, -0.25);
    sub = tint(accent, -0.5);
    grand = '262626';
    headerLine = body;
  }
  const rows = [
    `<rect x="1" y="9" width="42" height="5" fill="#${sub}"/>`,
    `<rect x="1" y="14" width="42" height="5" fill="#${body}"/>`,
    `<rect x="1" y="19" width="42" height="5" fill="#${sub}"/>`,
    `<rect x="1" y="24" width="42" height="5" fill="#${body}"/>`,
    `<rect x="1" y="29" width="42" height="4" fill="#${grand}"/>`,
    `<line x1="1" y1="29" x2="43" y2="29" stroke="#${headerLine}" stroke-width="0.6"/>`,
  ];
  const head = header
    ? `<rect x="1" y="2" width="42" height="7" fill="#${header}"/>`
    : `<line x1="1" y1="9" x2="43" y2="9" stroke="#${accent}" stroke-width="0.8"/>`;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 44 34" width="44" height="34"><rect x="1" y="2" width="42" height="31" fill="#${body}" stroke="#${tint(accent, 0.3)}" stroke-width="0.6"/>${rows.join('')}${head}</svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

/** The 84 built-in PivotTable styles as gallery tiles, and None. */
export const PIVOT_STYLE_ENTRIES: readonly RibbonGalleryEntry[] = [
  { id: '', label: 'None', sample: 'None', look: { muted: true } },
  ...(['Light', 'Medium', 'Dark'] as const).flatMap((family) =>
    Array.from({ length: 28 }, (_, i) => ({
      id: `PivotStyle${family}${i + 1}`,
      label: `Pivot Style ${family} ${i + 1}`,
      sample: '',
      look: { image: pivotPreview(family, i + 1) },
    })),
  ),
];

export const gridPivotAnalyzeTab = (): RibbonDataTab =>
  rtab(
    'gridPivotAnalyze',
    'PivotTable Analyze',
    [
      rgroup('pivotTable', 'PivotTable', [
        rcombo('ui.grid.pivot_name', [], {
          width: 120,
          placeholder: 'PivotTable name',
          allowCustomValue: true,
          keyTip: 'N',
        }),
        rbutton('ui.grid.pivot_options', { size: 'medium', keyTip: 'T' }),
      ]),
      rgroup('activeField', 'Active Field', [
        rbutton('ui.grid.pivot_field_settings', { size: 'large', keyTip: 'G' }),
      ]),
      rgroup('data', 'Data', [
        rsplit(
          'grid.pivot.refresh',
          [mitem('grid.pivot.refresh', { label: 'Refresh' }), mitem('ui.grid.pivot_refresh_all')],
          { size: 'large', keyTip: 'F5' },
        ),
        rbutton('ui.grid.pivot_change_source', { size: 'large', keyTip: 'I' }),
      ]),
      rgroup('actions', 'Actions', [
        rdropdown(undefined, [mitem('ui.grid.pivot_clear_all'), mitem('ui.grid.pivot_clear_filters')], {
          id: 'pivot.clear',
          label: 'Clear',
          icon: 'Eraser',
          size: 'medium',
          keyTip: 'C',
        }),
        rdropdown(undefined, [mitem('ui.grid.pivot_select')], {
          id: 'pivot.select',
          label: 'Select',
          icon: 'SelectObjects',
          size: 'medium',
          keyTip: 'S',
        }),
        rbutton('ui.grid.pivot_move', { size: 'medium', keyTip: 'M' }),
        rbutton('ui.grid.pivot_delete', { size: 'medium', label: 'Delete', keyTip: 'D' }),
      ]),
      rgroup('details', 'Details', [rbutton('ui.grid.pivot_show_details', { size: 'large', keyTip: 'H' })]),
      rgroup('show', 'Show', [rtoggle('ui.grid.pivot_fields', { size: 'large', keyTip: 'L' })]),
    ],
    { keyTip: 'JT', contextual: { set: 'pivotTools', label: 'PivotTable Tools', context: 'pivot' } },
  );

export const gridPivotDesignTab = (): RibbonDataTab =>
  rtab(
    'gridPivotDesign',
    'Design',
    [
      rgroup('layout', 'Layout', [
        rdropdown(
          undefined,
          [
            mitem('ui.grid.pivot_subtotals_off'),
            mitem('ui.grid.pivot_subtotals_bottom'),
            mitem('ui.grid.pivot_subtotals_top'),
          ],
          { id: 'pivot.subtotals', label: 'Subtotals', icon: 'Sigma', size: 'medium', keyTip: 'ST' },
        ),
        rdropdown(
          undefined,
          [
            mitem('ui.grid.pivot_totals_off'),
            mitem('ui.grid.pivot_totals_on'),
            mitem('ui.grid.pivot_totals_rows'),
            mitem('ui.grid.pivot_totals_columns'),
          ],
          {
            id: 'pivot.grand_totals',
            label: 'Grand Totals',
            icon: 'TableProperties',
            size: 'medium',
            keyTip: 'GT',
          },
        ),
        rdropdown(
          undefined,
          [
            mitem('ui.grid.pivot_layout_compact'),
            mitem('ui.grid.pivot_layout_outline'),
            mitem('ui.grid.pivot_layout_tabular'),
            msep(),
            mitem('ui.grid.pivot_repeat_labels'),
            mitem('ui.grid.pivot_no_repeat_labels'),
          ],
          {
            id: 'pivot.report_layout',
            label: 'Report Layout',
            icon: 'LayoutDashboard',
            size: 'medium',
            keyTip: 'RP',
          },
        ),
        rdropdown(undefined, [mitem('ui.grid.pivot_blank_rows'), mitem('ui.grid.pivot_no_blank_rows')], {
          id: 'pivot.blank_rows',
          label: 'Blank Rows',
          icon: 'Rows3',
          size: 'medium',
          keyTip: 'B',
        }),
      ]),
      rgroup('styleOptions', 'PivotTable Style Options', [
        rtoggle('ui.grid.pivot_row_headers', { size: 'small', checkbox: true }),
        rtoggle('ui.grid.pivot_col_headers', { size: 'small', checkbox: true, keyTip: 'C' }),
        rtoggle('ui.grid.pivot_banded_rows', { size: 'small', checkbox: true }),
        rtoggle('ui.grid.pivot_banded_cols', { size: 'small', checkbox: true }),
      ]),
      rgroup('pivotStyles', 'PivotTable Styles', [
        rgallery('ui.grid.pivot_style', PIVOT_STYLE_ENTRIES, {
          columns: 7,
          itemWidth: 52,
          itemHeight: 40,
        }),
      ]),
    ],
    { keyTip: 'JY', contextual: { set: 'pivotTools', label: 'PivotTable Tools', context: 'pivot' } },
  );

// ── Sparkline ─────────────────────────────────────────────────────────────────────────────────

const OFFICE_ACCENTS = ['4472C4', 'ED7D31', 'A5A5A5', 'FFC000', '5B9BD5', '70AD47'];
const SHADES = [-0.5, -0.25, 0, 0.4];

/** A sparkline style's preview: a little line in its colour with marked points. */
function sparkPreview(index: number): string {
  const accent = OFFICE_ACCENTS[index % 6] ?? '4472C4';
  const row = Math.floor(index / 6);
  const line = tint(accent, SHADES[row] ?? 0);
  const point = tint(accent, row >= 2 ? -0.5 : 0.4);
  const pts = [
    [4, 22],
    [12, 12],
    [20, 18],
    [28, 8],
    [36, 14],
  ];
  const path = pts.map(([x, y], i) => `${i ? 'L' : 'M'}${x} ${y}`).join(' ');
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 30" width="40" height="30"><rect width="40" height="30" fill="#FFFFFF"/><path d="${path}" fill="none" stroke="#${line}" stroke-width="1.6" stroke-linejoin="round"/><circle cx="28" cy="8" r="2" fill="#${point}"/><circle cx="4" cy="22" r="2" fill="#${point}"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

/** Twenty-four sparkline styles: four rows of the six accents, darker to lighter. */
export const SPARK_STYLE_ENTRIES: readonly RibbonGalleryEntry[] = Array.from({ length: 24 }, (_, i) => ({
  id: String(i),
  label: `Sparkline Style Accent ${(i % 6) + 1}, ${['Darker 50%', 'Darker 25%', 'Accent', 'Lighter 40%'][Math.floor(i / 6)]}`,
  sample: '',
  look: { image: sparkPreview(i) },
}));

const MARKER_COLORS: readonly RibbonMenuEntry[] = [
  mhead('Marker Color'),
  mitem('ui.grid.spark_marker_color', { label: 'Negative Points', args: { which: 'negative' } }),
  mitem('ui.grid.spark_marker_color', { label: 'Markers', args: { which: 'markers' } }),
  mitem('ui.grid.spark_marker_color', { label: 'High Point', args: { which: 'high' } }),
  mitem('ui.grid.spark_marker_color', { label: 'Low Point', args: { which: 'low' } }),
  mitem('ui.grid.spark_marker_color', { label: 'First Point', args: { which: 'first' } }),
  mitem('ui.grid.spark_marker_color', { label: 'Last Point', args: { which: 'last' } }),
];

export const gridSparklineTab = (): RibbonDataTab =>
  rtab(
    'gridSparkline',
    'Sparkline',
    [
      rgroup('sparkline', 'Sparkline', [
        rsplit(
          'ui.grid.spark_edit_data',
          [mitem('ui.grid.spark_edit_data'), mitem('ui.grid.spark_hidden_empty')],
          {
            size: 'large',
            keyTip: 'D',
          },
        ),
      ]),
      rgroup('type', 'Type', [
        rtoggle('ui.grid.spark_line', { size: 'large', keyTip: 'L' }),
        rtoggle('ui.grid.spark_column', { size: 'large', keyTip: 'C' }),
        rtoggle('ui.grid.spark_win_loss', { size: 'large', keyTip: 'W' }),
      ]),
      rgroup('show', 'Show', [
        rtoggle('ui.grid.spark_high', { size: 'small', checkbox: true, keyTip: 'H' }),
        rtoggle('ui.grid.spark_low', { size: 'small', checkbox: true, keyTip: 'O' }),
        rtoggle('ui.grid.spark_negative', { size: 'small', checkbox: true, keyTip: 'N' }),
        rtoggle('ui.grid.spark_first', { size: 'small', checkbox: true, keyTip: 'F' }),
        rtoggle('ui.grid.spark_last', { size: 'small', checkbox: true, keyTip: 'P' }),
        rtoggle('ui.grid.spark_markers', { size: 'small', checkbox: true, keyTip: 'M' }),
      ]),
      rgroup('style', 'Style', [
        rgallery('ui.grid.spark_style', SPARK_STYLE_ENTRIES, {
          columns: 6,
          itemWidth: 44,
          itemHeight: 34,
          keyTip: 'S',
        }),
        rcolor('ui.grid.spark_color', { size: 'small', keyTip: 'SC' }),
        rdropdown(undefined, MARKER_COLORS, {
          id: 'spark.marker_color',
          label: 'Marker Color',
          icon: 'Palette',
          size: 'medium',
          keyTip: 'K',
        }),
      ]),
      rgroup('group', 'Group', [
        rdropdown(
          undefined,
          [
            mitem('ui.grid.spark_show_axis'),
            mitem('ui.grid.spark_right_to_left'),
            msep(),
            mhead('Vertical Axis Minimum Value Options'),
            mitem('ui.grid.spark_axis_same_min'),
            mhead('Vertical Axis Maximum Value Options'),
            mitem('ui.grid.spark_axis_same_max'),
          ],
          { id: 'spark.axis', label: 'Axis', icon: 'Ruler', size: 'medium', keyTip: 'A' },
        ),
        rbutton('ui.grid.spark_group', { size: 'medium', keyTip: 'G' }),
        rbutton('ui.grid.spark_ungroup', { size: 'medium', keyTip: 'U' }),
        rdropdown(undefined, [mitem('ui.grid.spark_clear'), mitem('ui.grid.spark_clear_group')], {
          id: 'spark.clear',
          label: 'Clear',
          icon: 'Eraser',
          size: 'medium',
          keyTip: 'E',
        }),
      ]),
    ],
    { keyTip: 'JS', contextual: { set: 'sparklineTools', label: 'Sparkline Tools', context: 'sparkline' } },
  );

// ── Themes ────────────────────────────────────────────────────────────────────────────────────

/** Excel's Office theme and the twelve Kiln themes Deck shares (kiln_deck::themes). */
export const GRID_THEMES: readonly { id: string; name: string; colours: readonly string[] }[] = [
  { id: 'office', name: 'Office', colours: ['#FFFFFF', '#000000', '#4472C4'] },
  ...DECK_THEMES,
];

export const THEME_ENTRIES: readonly RibbonGalleryEntry[] = GRID_THEMES.map((t) => ({
  id: t.id,
  label: t.name,
  sample: t.name,
  look: { swatch: [...t.colours] },
}));

const themeMenu = (part: GridThemePart, command: string): RibbonMenuEntry[] =>
  GRID_THEMES.map((t) => mitem(command, { label: t.name, args: { theme: t.id, part } }));

export const THEME_COLORS_MENU: readonly RibbonMenuEntry[] = themeMenu('Colors', 'ui.grid.theme_colors');
export const THEME_FONTS_MENU: readonly RibbonMenuEntry[] = themeMenu('Fonts', 'ui.grid.theme_fonts');

export const gridThemesGroup = () =>
  rgroup('themes', 'Themes', [
    rgallery('grid.page_layout.themes', THEME_ENTRIES, {
      columns: 4,
      itemWidth: 64,
      itemHeight: 48,
      keyTip: 'TH',
    }),
    rdropdown(undefined, THEME_COLORS_MENU, {
      id: 'themes.colors',
      label: 'Colors',
      icon: 'Palette',
      size: 'medium',
      keyTip: 'TC',
    }),
    rdropdown(undefined, THEME_FONTS_MENU, {
      id: 'themes.fonts',
      label: 'Fonts',
      icon: 'Type',
      size: 'medium',
      keyTip: 'TF',
    }),
  ]);

/** The Protection section of Home ▸ Cells ▸ Format. */
export const FORMAT_PROTECTION_MENU: readonly RibbonMenuEntry[] = [
  mhead('Protection'),
  mitem('grid.review.protect_sheet', { label: 'Protect Sheet…' }),
  mitem('ui.grid.lock_cell'),
];

// ── builders ─────────────────────────────────────────────────────────────────────────────────

type Args = Readonly<Record<string, unknown>>;

class AnalysisArgsError extends Error {
  override readonly name = 'CommandArgsError';
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);
const isStr = (v: unknown): v is string => typeof v === 'string';
const isUint = (v: unknown): v is number => typeof v === 'number' && Number.isInteger(v) && v >= 0;
const isBool = (v: unknown): v is boolean => typeof v === 'boolean';

function need<T>(id: string, a: Args, key: string, ok: (v: unknown) => v is T, what: string): T {
  const x = a[key];
  if (!ok(x)) throw new AnalysisArgsError(`${id}: "${key}" must be ${what}`);
  return x;
}

const g = (c: GridCommand): Command => ({ Grid: c });
const KINDS: readonly GridSparkKind[] = ['Line', 'Column', 'WinLoss'];
const PARTS: readonly GridThemePart[] = ['All', 'Colors', 'Fonts'];

const TABLE: Readonly<Record<string, (a: Args, id: string) => GridCommand>> = {
  'grid.pivot.create': (a, id) => ({
    CreatePivot: {
      source: need(id, a, 'source', isStr, 'a range or a table name'),
      to: isStr(a.to) ? a.to : null,
    },
  }),
  'grid.pivot.edit': (a, id) => ({
    Pivot: { edit: need(id, a, 'edit', (v): v is GridPivotEdit => isObj(v) || isStr(v), 'an edit') },
  }),
  'grid.pivot.refresh': (a) => ({ RefreshPivots: { all: a.all === true } }),
  'grid.data.show_outline_level': (a) => ({
    OutlineLevel: { columns: a.columns === true, level: Number(a.level ?? 1) },
  }),
  'grid.data.toggle_outline_group': (a) => ({
    OutlineToggle: { columns: a.columns === true, index: Number(a.index ?? 0) },
  }),
  'grid.pivot.show_details': (a, id) => ({
    PivotDetails: { row: need(id, a, 'row', isUint, 'a row'), col: need(id, a, 'col', isUint, 'a column') },
  }),
  'grid.sparkline.insert': (a, id) => {
    const kind = a.kind === undefined ? 'Line' : a.kind;
    if (!KINDS.includes(kind as GridSparkKind))
      throw new AnalysisArgsError(`${id}: "kind" must be Line, Column or WinLoss`);
    return {
      InsertSparklines: {
        kind: kind as GridSparkKind,
        data: need(id, a, 'data', isStr, 'a range'),
        location: need(id, a, 'location', isStr, 'a range'),
      },
    };
  },
  'grid.sparkline.edit': (a, id) => ({
    Sparklines: { edit: need(id, a, 'edit', (v): v is GridSparkEdit => isObj(v) || isStr(v), 'an edit') },
  }),
  'grid.data.run_goal_seek': (a, id) => ({
    GoalSeek: {
      set_cell: need(id, a, 'set_cell', isStr, 'a cell'),
      to_value: need(id, a, 'to_value', isStr, 'a number'),
      by_changing: need(id, a, 'by_changing', isStr, 'a cell'),
    },
  }),
  'grid.data.apply_subtotal': (a) => ({
    Subtotal: { spec: isObj(a.spec) ? (a.spec as unknown as GridSubtotalSpec) : null },
  }),
  'grid.editing.flash_fill': () => 'FlashFill',
  'grid.review.set_sheet_protection': (a, id) => ({
    ProtectSheet: {
      protect: need(id, a, 'protect', isBool, 'true or false'),
      password: isStr(a.password) ? a.password : '',
      allow: isObj(a.allow) ? (a.allow as unknown as GridSheetPermissions) : null,
    },
  }),
  'grid.review.set_workbook_protection': (a, id) => ({
    ProtectWorkbook: {
      protect: need(id, a, 'protect', isBool, 'true or false'),
      password: isStr(a.password) ? a.password : '',
    },
  }),
  'grid.cells.lock_cell': (a, id) => ({
    LockCells: { locked: need(id, a, 'locked', isBool, 'true or false') },
  }),
  'grid.page_layout.apply_theme': (a, id) => {
    const theme = isStr(a.theme) ? a.theme : need(id, a, 'value', isStr, 'a theme');
    const part = a.part === undefined ? 'All' : a.part;
    if (!PARTS.includes(part as GridThemePart))
      throw new AnalysisArgsError(`${id}: "part" must be All, Colors or Fonts`);
    return { SetTheme: { theme, part: part as GridThemePart } };
  },
};

/** The builders of the core commands (spread into `GRID_BUILDERS`). */
export const GRID_ANALYSIS_BUILDERS: Readonly<Record<string, (a: Args) => Command>> = Object.freeze(
  Object.fromEntries(Object.entries(TABLE).map(([id, build]) => [id, (a: Args) => g(build(a, id))] as const)),
);
