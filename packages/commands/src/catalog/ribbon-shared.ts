/**
 * Ribbon building blocks shared by the apps (PLAN.md §1.4): the Clipboard, Font and Paragraph groups,
 * view-mode toggles over the core's `suite.view.mode`, zoom presets over `suite.view.zoom`, the Theme
 * and Macros groups, and the contextual tabs for tables, pictures and shapes.
 */
import {
  mhead,
  mitem,
  msep,
  type RibbonComboOption,
  type RibbonDataLeaf,
  type RibbonDataTab,
  type RibbonItemSize,
  type RibbonMenuEntry,
  rbutton,
  rcolor,
  rcombo,
  rdropdown,
  rgallery,
  rgroup,
  rrow,
  rsplit,
  rtab,
  rtoggle,
} from '../ribbon.ts';

const FONT_FAMILIES = [
  'Figtree',
  'Fraunces',
  'Aptos',
  'Calibri',
  'Cambria',
  'Arial',
  'Georgia',
  'Times New Roman',
  'Courier New',
];
const FONT_SIZES = ['8', '9', '10', '10.5', '11', '12', '14', '16', '18', '20', '24', '28', '36', '48', '72'];

/** Font family options (shown in their own face). */
export const FONT_FAMILY_OPTIONS: readonly RibbonComboOption[] = FONT_FAMILIES.map((f, i) => ({
  value: f,
  fontFamily: f,
  ...(i < 2 ? { group: 'Theme Fonts', hint: i === 0 ? '(Body)' : '(Headings)' } : { group: 'All Fonts' }),
}));
export const FONT_SIZE_OPTIONS: readonly RibbonComboOption[] = FONT_SIZES.map((value) => ({ value }));

/** Paste ▾ (Keep Source / Merge / Text only / Paste Special…). */
export const PASTE_MENU: readonly RibbonMenuEntry[] = [
  mhead('Paste Options'),
  mitem('suite.edit.paste_keep_source'),
  mitem('suite.edit.paste_merge_formatting'),
  mitem('suite.edit.paste_text_only'),
  msep(),
  mitem('suite.edit.paste_special'),
];

/** The Clipboard group every editor shares. */
export const clipboardGroup = (extra: readonly RibbonMenuEntry[] = []) =>
  rgroup(
    'clipboard',
    'Clipboard',
    [
      rsplit('suite.edit.paste', [...PASTE_MENU, ...extra], { size: 'large', keyTip: 'V' }),
      rbutton('suite.edit.cut', { size: 'medium', keyTip: 'X' }),
      rbutton('suite.edit.copy', { size: 'medium', keyTip: 'C' }),
      rbutton('suite.edit.format_painter', { size: 'medium', label: 'Format', keyTip: 'FP' }),
    ],
    { icon: 'Paste', priority: 3 },
  );

/** Font: family, size, grow/shrink, case, clear; B I U S x₂ x² colour, highlight. */
export const fontGroup = (options: { effects?: boolean; spacing?: boolean } = {}) =>
  rgroup(
    'font',
    'Font',
    [
      rrow(
        rcombo('suite.format.font_family', FONT_FAMILY_OPTIONS, {
          placeholder: 'Font',
          width: 122,
          keyTip: 'FF',
          allowCustomValue: true,
        }),
        rcombo('suite.format.font_size', FONT_SIZE_OPTIONS, {
          placeholder: 'Size',
          width: 54,
          keyTip: 'FS',
          allowCustomValue: true,
          accepts: 'number',
        }),
        rbutton('suite.format.grow_font', { keyTip: 'FG' }),
        rbutton('suite.format.shrink_font', { keyTip: 'FK' }),
        rdropdown(
          'suite.format.change_case',
          [
            mitem('suite.format.change_case', { label: 'Sentence case.', args: { case: 'sentence' } }),
            mitem('suite.format.change_case', { label: 'lowercase', args: { case: 'lower' } }),
            mitem('suite.format.change_case', { label: 'UPPERCASE', args: { case: 'upper' } }),
            mitem('suite.format.change_case', { label: 'Capitalize Each Word', args: { case: 'title' } }),
            mitem('suite.format.change_case', { label: 'tOGGLE cASE', args: { case: 'toggle' } }),
          ],
          { size: 'small', keyTip: '7', hideInSimplified: true },
        ),
        rbutton('suite.format.clear_formatting', { keyTip: 'E', hideInSimplified: true }),
      ),
      rrow(
        rtoggle('suite.format.bold', { keyTip: '1', priority: 5 }),
        rtoggle('suite.format.italic', { keyTip: '2', priority: 5 }),
        rsplit(
          'suite.format.underline',
          [mitem('suite.format.underline'), mitem('suite.format.double_underline')],
          {
            size: 'small',
            keyTip: '3',
            priority: 5,
          },
        ),
        rtoggle('suite.format.strikethrough', { keyTip: '4' }),
        rtoggle('suite.format.subscript', { keyTip: '5', hideInSimplified: true }),
        rtoggle('suite.format.superscript', { keyTip: '6', hideInSimplified: true }),
        ...(options.effects
          ? [rbutton('suite.format.text_effects', { keyTip: 'TE', tier: 'P2' as const })]
          : []),
        ...(options.spacing
          ? [
              rbutton('suite.format.character_spacing', { keyTip: 'SP' }),
              rtoggle('suite.format.text_shadow', { keyTip: 'S' }),
            ]
          : []),
        rcolor('suite.format.font_color', {
          defaultColor: '#C8352B',
          allowAuto: true,
          keyTip: 'FC',
          priority: 4,
        }),
        rcolor('suite.format.highlight', { defaultColor: '#FFE066', allowNone: true, keyTip: 'I' }),
      ),
    ],
    { rows: 2, launcher: { command: 'suite.format.font_dialog' }, icon: 'Type', priority: 4 },
  );

/** Paragraph for text apps: lists, indents, alignment, spacing. */
export const paragraphGroup = (
  extra: { multilevel?: boolean; first?: readonly RibbonDataLeaf[]; second?: readonly RibbonDataLeaf[] } = {},
) =>
  rgroup(
    'paragraph',
    'Paragraph',
    [
      rrow(
        rsplit('suite.format.bullets', [mitem('suite.format.bullets')], {
          size: 'small',
          keyTip: 'U',
          priority: 3,
        }),
        rsplit('suite.format.numbering', [mitem('suite.format.numbering')], {
          size: 'small',
          keyTip: 'N',
          priority: 3,
        }),
        ...(extra.multilevel
          ? [rbutton('suite.format.multilevel_list', { keyTip: 'M', hideInSimplified: true })]
          : []),
        rbutton('suite.format.outdent', { keyTip: 'AO' }),
        rbutton('suite.format.indent', { keyTip: 'AI' }),
        ...(extra.first ?? []),
      ),
      rrow(
        rtoggle('suite.format.align_left', { keyTip: 'AL', priority: 2 }),
        rtoggle('suite.format.align_center', { keyTip: 'AC', priority: 2 }),
        rtoggle('suite.format.align_right', { keyTip: 'AR' }),
        rtoggle('suite.format.justify', { keyTip: 'AJ' }),
        rdropdown(
          'suite.format.line_spacing',
          [
            mitem('suite.format.line_spacing', { label: '1.0', args: { lineSpacing: 1 } }),
            mitem('suite.format.line_spacing', { label: '1.15', args: { lineSpacing: 1.15 } }),
            mitem('suite.format.line_spacing', { label: '1.5', args: { lineSpacing: 1.5 } }),
            mitem('suite.format.line_spacing', { label: '2.0', args: { lineSpacing: 2 } }),
            msep(),
            mitem('suite.format.paragraph_dialog', { label: 'Line Spacing Options…' }),
          ],
          { size: 'small', keyTip: 'K' },
        ),
        ...(extra.second ?? []),
      ),
    ],
    { rows: 2, launcher: { command: 'suite.format.paragraph_dialog' }, icon: 'Pilcrow', priority: 3 },
  );

/** A choice of the core's view mode (`suite.view.mode`), pressed while the core reports that view. */
export const viewMode = (
  mode: string,
  label: string,
  icon: string,
  keyTip: string,
  size: RibbonItemSize = 'large',
) =>
  rtoggle('suite.view.mode', {
    id: `view.${mode}`,
    args: { mode },
    checkedValue: mode,
    label,
    icon,
    keyTip,
    size,
  });

/** Zoom ▾ with Office's presets (the core clamps and applies them). */
export const zoomMenu = (size: RibbonItemSize = 'large') =>
  rdropdown(
    'suite.view.zoom',
    [50, 75, 100, 125, 150, 200].map((pct) =>
      mitem('suite.view.zoom', { label: `${pct}%`, args: { zoom: pct / 100 } }),
    ),
    { id: 'view.zoom', size, keyTip: 'Q' },
  );

/** View ▸ Theme: the chrome theme (system, light, dark, high contrast). */
export const themeGroup = () =>
  rgroup(
    'theme',
    'Theme',
    [
      rdropdown(
        undefined,
        [
          mitem('ui.theme.system', { label: 'Use System Setting' }),
          mitem('ui.theme.light', { label: 'Light' }),
          mitem('ui.theme.dark', { label: 'Dark' }),
          mitem('ui.theme.high_contrast', { label: 'High Contrast' }),
        ],
        { id: 'view.theme', label: 'Theme', icon: 'SunMoon', size: 'large', keyTip: 'TH' },
      ),
    ],
    { icon: 'SunMoon' },
  );

/** View ▸ Macros: Kiln never runs macros; the button explains why (PLAN.md §1.4, §9). */
export const macrosGroup = () =>
  rgroup('macros', 'Macros', [rbutton('ui.macros.explain', { size: 'large', keyTip: 'M' })], {
    icon: 'Macros',
  });

const TABLE_STYLES = [
  { id: 'Plain', label: 'Plain Table', look: { swatch: ['#FFFFFF', '#2A211C'] } },
  { id: 'Grid', label: 'Table Grid', look: { swatch: ['#F3ECE3', '#3A2F28'] } },
  { id: 'Accent', label: 'Accent Rows', look: { swatch: ['#DCE6FB', '#1F2A44'], accent: true } },
  { id: 'Banded', label: 'Banded Rows', look: { swatch: ['#EFE6DA', '#2E2621'] } },
] as const;

/** Table Design and Table Layout (contextual, while the selection is in a table). */
export const tableTabs = (app: 'write' | 'grid' | 'deck'): RibbonDataTab[] => [
  rtab(
    'tableDesign',
    'Table Design',
    [
      rgroup('styleOptions', 'Table Style Options', [
        rtoggle('suite.table.header_row', { size: 'small' }),
        rtoggle('suite.table.total_row', { size: 'small' }),
        rtoggle('suite.table.banded_rows', { size: 'small' }),
        rtoggle('suite.table.first_column', { size: 'small' }),
        rtoggle('suite.table.banded_columns', { size: 'small' }),
      ]),
      rgroup('tableStyles', 'Table Styles', [
        rgallery('suite.table.styles', TABLE_STYLES, { columns: 4, itemWidth: 64, itemHeight: 48 }),
        rbutton('suite.table.shading', { size: 'medium' }),
      ]),
      rgroup('borders', 'Borders', [rbutton('suite.table.borders', { size: 'large' })]),
    ],
    { keyTip: 'JT', contextual: { set: 'tableTools', label: 'Table Tools', context: 'table' } },
  ),
  ...(app !== 'grid'
    ? [
        rtab(
          'tableLayout',
          'Layout',
          [
            rgroup('rowsColumns', 'Rows & Columns', [
              rbutton('suite.table.delete', { size: 'large' }),
              rbutton('suite.table.insert_row_above', { size: 'large' }),
              rbutton('suite.table.insert_row_below', { size: 'small' }),
              rbutton('suite.table.insert_column_left', { size: 'small' }),
              rbutton('suite.table.insert_column_right', { size: 'small' }),
            ]),
            rgroup('merge', 'Merge', [
              rbutton('suite.table.merge_cells', { size: 'medium' }),
              rbutton('suite.table.split_cells', { size: 'medium' }),
            ]),
            rgroup('cellSize', 'Cell Size', [
              rbutton('suite.table.distribute_rows', { size: 'medium' }),
              rbutton('suite.table.distribute_columns', { size: 'medium' }),
            ]),
            rgroup('alignment', 'Alignment', [rbutton('suite.table.cell_alignment', { size: 'large' })]),
            ...(app === 'write'
              ? [
                  rgroup('data', 'Data', [
                    rtoggle('suite.table.repeat_header_rows', { size: 'medium' }),
                    rbutton('suite.table.convert_to_text', { size: 'medium' }),
                    rbutton('suite.table.formula', { size: 'medium' }),
                  ]),
                ]
              : []),
          ],
          { keyTip: 'JL', contextual: { set: 'tableTools', label: 'Table Tools', context: 'table' } },
        ),
      ]
    : []),
];

/** Picture Format (contextual, while a picture is selected). */
export const pictureFormatTab = () =>
  rtab(
    'pictureFormat',
    'Picture Format',
    [
      rgroup('adjust', 'Adjust', [
        rbutton('suite.picture.corrections', { size: 'large' }),
        rbutton('suite.picture.reset', { size: 'medium' }),
      ]),
      rgroup('accessibility', 'Accessibility', [rbutton('suite.picture.alt_text', { size: 'large' })]),
      rgroup('arrange', 'Arrange', [
        rbutton('suite.arrange.position', { size: 'medium' }),
        rbutton('suite.arrange.wrap_text', { size: 'medium' }),
        rbutton('suite.arrange.bring_forward', { size: 'medium' }),
        rbutton('suite.arrange.send_backward', { size: 'medium' }),
      ]),
      rgroup('size', 'Size', [rbutton('suite.picture.crop', { size: 'large' })]),
    ],
    { keyTip: 'JP', contextual: { set: 'pictureTools', label: 'Picture Tools', context: 'picture' } },
  );

/** Shape Format (contextual, while a shape is selected). */
export const shapeFormatTab = () =>
  rtab(
    'shapeFormat',
    'Shape Format',
    [
      rgroup('insertShapes', 'Insert Shapes', [rbutton('suite.insert.shapes', { size: 'large' })]),
      rgroup('shapeStyles', 'Shape Styles', [
        rbutton('suite.shape.styles', { size: 'large' }),
        rbutton('suite.shape.fill', { size: 'medium' }),
        rbutton('suite.shape.outline', { size: 'medium' }),
        rbutton('suite.shape.effects', { size: 'medium' }),
      ]),
      rgroup('arrange', 'Arrange', [
        rbutton('suite.arrange.bring_forward', { size: 'medium' }),
        rbutton('suite.arrange.send_backward', { size: 'medium' }),
        rbutton('suite.arrange.align', { size: 'medium' }),
        rbutton('suite.arrange.group', { size: 'medium' }),
      ]),
      rgroup('size', 'Size', [
        rbutton('suite.shape.height', { size: 'medium' }),
        rbutton('suite.shape.width', { size: 'medium' }),
      ]),
    ],
    { keyTip: 'JD', contextual: { set: 'drawingTools', label: 'Drawing Tools', context: 'shape' } },
  );
