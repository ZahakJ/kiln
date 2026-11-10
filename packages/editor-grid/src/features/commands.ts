/**
 * The chrome side of Kiln Grid's features: the ribbon, menu and keyboard ids that open a
 * feature dialog or pane, arm Format Painter, or need the user's name, the clock or the selection
 * before they become a core command. Presentation only: every change to the workbook is a core
 * command.
 */

import { type CommandDefinition, CommandNotice, type Disposable } from '@kiln/commands';
import { msg, t } from '@kiln/i18n';
import type {
  GridCalcMode,
  GridCommand,
  GridObjectFormat,
  GridTableOptions,
  GridTotals,
} from '@kiln/protocol';
import type { GridEditorModel } from '../model.ts';
import { PT_PER_PX } from './dialogs/ObjectDialogs.tsx';
import {
  authorName,
  type FeatureDialog,
  type FeaturePane,
  featuresOf,
  gridFeatures,
  selectionRanges,
} from './model.ts';

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null;
const arg = <T>(args: unknown, key: string): T | undefined =>
  isRecord(args) && key in args ? (args[key] as T) : undefined;

/** The ids that open a feature dialog (with the dialog's starting tab or preset). */
export const GRID_FEATURE_DIALOGS: readonly {
  id: string;
  title: string;
  dialog: FeatureDialog;
  arg?: string;
}[] = [
  { id: 'grid.data.sort', title: msg('Sort…'), dialog: 'sort' },
  { id: 'ui.grid.cf_rule', title: msg('New Formatting Rule…'), dialog: 'cfRule' },
  { id: 'ui.grid.cf_manage', title: msg('Manage Rules…'), dialog: 'cfManage' },
  { id: 'grid.data.data_validation', title: msg('Data Validation…'), dialog: 'validation' },
  { id: 'grid.insert.table', title: msg('Create Table'), dialog: 'createTable' },
  { id: 'ui.grid.resize_table', title: msg('Resize Table…'), dialog: 'resizeTable' },
  { id: 'suite.edit.replace', title: msg('Replace…'), dialog: 'find', arg: 'replace' },
  { id: 'ui.grid.find', title: msg('Find…'), dialog: 'find', arg: 'find' },
  { id: 'ui.grid.go_to_special', title: msg('Go To Special…'), dialog: 'goToSpecial' },
  { id: 'grid.formulas.name_manager', title: msg('Name Manager'), dialog: 'nameManager' },
  { id: 'grid.formulas.define_name', title: msg('Define Name…'), dialog: 'defineName' },
  {
    id: 'grid.formulas.create_from_selection',
    title: msg('Create Names from Selection'),
    dialog: 'createNames',
  },
  { id: 'grid.formulas.use_in_formula', title: msg('Paste Names…'), dialog: 'pasteName' },
  { id: 'suite.insert.link', title: msg('Insert Hyperlink'), dialog: 'link' },
  { id: 'suite.layout.page_setup', title: msg('Page Setup…'), dialog: 'pageSetup', arg: 'page' },
  { id: 'grid.page_layout.print_titles', title: msg('Print Titles…'), dialog: 'pageSetup', arg: 'sheet' },
  { id: 'suite.insert.header_footer', title: msg('Header & Footer…'), dialog: 'pageSetup', arg: 'header' },
  { id: 'ui.grid.custom_margins', title: msg('Custom Margins…'), dialog: 'pageSetup', arg: 'margins' },
  { id: 'grid.data.text_to_columns', title: msg('Text to Columns…'), dialog: 'textToColumns' },
  { id: 'grid.data.remove_duplicates', title: msg('Remove Duplicates…'), dialog: 'removeDuplicates' },
  { id: 'grid.formulas.evaluate_formula', title: msg('Evaluate Formula'), dialog: 'evaluate' },
  { id: 'grid.formulas.error_checking', title: msg('Error Checking'), dialog: 'errorChecking' },
  { id: 'suite.insert.symbol', title: msg('Symbol…'), dialog: 'symbol' },
  { id: 'suite.picture.alt_text', title: msg('Alt Text'), dialog: 'altText' },
  { id: 'ui.grid.iterative', title: msg('Iterative Calculation…'), dialog: 'iterative' },
  { id: 'suite.picture.crop', title: msg('Crop'), dialog: 'crop' },
  { id: 'ui.grid.format_object', title: msg('Size and Properties…'), dialog: 'formatObject' },
  { id: 'ui.grid.shape_text', title: msg('Edit Text…'), dialog: 'shapeText' },
];

/** The ids that open or close a feature pane. */
export const GRID_FEATURE_PANES: readonly { id: string; title: string; pane: FeaturePane }[] = [
  { id: 'grid.formulas.formula_lab', title: msg('Formula Lab'), pane: 'lab' },
  { id: 'grid.formulas.watch_window', title: msg('Watch Window'), pane: 'watch' },
  { id: 'suite.review.show_comments', title: msg('Show Comments'), pane: 'comments' },
  { id: 'suite.review.spelling', title: msg('Spelling'), pane: 'spelling' },
];

/** Registers the feature commands; dispose to remove them. */
export function registerGridFeatures(model: GridEditorModel): Disposable {
  const features = featuresOf(model);
  const reg = model.registry;
  const exec = (c: GridCommand) => features.exec(c);
  const table = () => gridFeatures(model)?.table ?? null;
  const tableOption = (key: keyof GridTableOptions, title: string): CommandDefinition<unknown> => ({
    id: `ui.grid.table_${key}`,
    title,
    isEnabled: () => !!table(),
    isChecked: () => !!table()?.options[key],
    run: async () => {
      const t = table();
      if (!t) return;
      await exec({ TableOptions: { options: { ...t.options, [key]: !t.options[key] } } });
    },
  });
  const calc = (mode: GridCalcMode, title: string): CommandDefinition<unknown> => ({
    id: `ui.grid.calc_${mode.toLowerCase()}`,
    title,
    isChecked: () => gridFeatures(model)?.calc === mode,
    run: () => exec({ CalculationMode: { mode } }),
  });
  let lastPainter = 0;
  const object = () => model.state.grid?.drawing ?? null;
  const isObject = () => !!object()?.object;
  const isPicture = () => object()?.kind === 'Picture';
  const formatObject = (f: Partial<GridObjectFormat>) =>
    exec({
      FormatObject: {
        format: {
          name: null,
          alt_text: null,
          width: null,
          height: null,
          lock_aspect: null,
          crop: null,
          placement: null,
          preset: null,
          fill: null,
          line: null,
          line_width: null,
          text: null,
          text_color: null,
          text_size: null,
          ...f,
        },
      },
    });
  /** A colour argument of a colour picker (`#RRGGBB`, `{ hex }`, or `{ type: 'none' }`). */
  const colourOf = (args: unknown): string | null => {
    const v = arg<unknown>(args, 'color') ?? arg<unknown>(args, 'value');
    if (isRecord(v) && (v.type === 'none' || v.type === 'auto')) return 'none';
    const raw = isRecord(v) && typeof v.hex === 'string' ? v.hex : v;
    return typeof raw === 'string' && /^#?[0-9a-f]{6}$/i.test(raw)
      ? raw.replace(/^#/, '').toUpperCase()
      : null;
  };
  const objectDefs: CommandDefinition<unknown>[] = [
    {
      // Insert ▸ Text Box: a box at the active cell, its text typed at once.
      id: 'suite.insert.text_box',
      title: msg('Text Box'),
      run: async () => {
        await exec({ InsertShape: { preset: 'textBox', text: null } });
        if (model.state.grid?.drawing?.kind === 'Shape') features.openDialog('shapeText');
      },
    },
    {
      // A function of a Function Library menu: Function Arguments for it.
      id: 'ui.grid.function_args',
      title: msg('Function Arguments'),
      run: (_ctx, args) => {
        const name = arg<string>(args, 'name');
        if (name) features.openDialog('functionArgs', name.toUpperCase());
      },
    },
    {
      id: 'grid.formulas.recently_used_functions',
      title: msg('Recently Used'),
      run: () => model.openDialog('insertFunction', 'Most Recently Used'),
    },
    {
      id: 'ui.grid.reset_picture_size',
      title: msg('Reset Picture & Size'),
      isEnabled: isPicture,
      run: () => exec({ ResetPicture: { size: true } }),
    },
    {
      id: 'suite.picture.reset',
      title: msg('Reset Picture'),
      isEnabled: isPicture,
      run: () => exec({ ResetPicture: { size: false } }),
    },
    ...(
      [
        ['suite.arrange.bring_forward', msg('Bring Forward'), 'Forward'],
        ['ui.grid.bring_to_front', msg('Bring to Front'), 'Front'],
        ['suite.arrange.send_backward', msg('Send Backward'), 'Backward'],
        ['ui.grid.send_to_back', msg('Send to Back'), 'Back'],
      ] as const
    ).map(
      ([id, title, arrange]): CommandDefinition<unknown> => ({
        id,
        title,
        isEnabled: () => !!object(),
        run: () => exec({ ArrangeDrawing: { arrange } }),
      }),
    ),
    ...(['height', 'width'] as const).map(
      (side): CommandDefinition<unknown> => ({
        id: `ui.grid.object_${side}`,
        title: side === 'height' ? msg('Shape Height') : msg('Shape Width'),
        isEnabled: isObject,
        value: () => {
          const d = object();
          return d ? Math.round(d[side] * PT_PER_PX * 100) / 100 : undefined;
        },
        run: (_ctx, args) => {
          const v = arg<number>(args, 'value');
          if (typeof v !== 'number' || !(v >= 0)) return;
          return formatObject({ [side]: v / PT_PER_PX });
        },
      }),
    ),
    {
      id: 'ui.grid.shape_fill',
      title: msg('Shape Fill'),
      isEnabled: () => isObject() && !isPicture() && object()?.kind !== 'Line',
      run: (_ctx, args) => {
        const c = colourOf(args);
        if (c) return formatObject({ fill: c });
      },
    },
    {
      id: 'ui.grid.shape_outline',
      title: msg('Shape Outline'),
      isEnabled: isObject,
      run: (_ctx, args) => {
        const c = colourOf(args);
        if (c) return formatObject({ line: c });
      },
    },
    {
      id: 'ui.grid.shape_text_color',
      title: msg('Text Fill'),
      isEnabled: () => isObject() && !isPicture(),
      run: (_ctx, args) => {
        const c = colourOf(args);
        if (c && c !== 'none') return formatObject({ text_color: c });
      },
    },
    {
      id: 'ui.grid.outline_weight',
      title: msg('Weight'),
      isEnabled: isObject,
      run: (_ctx, args) => {
        const points = arg<number>(args, 'points');
        if (typeof points === 'number') return formatObject({ line_width: points });
      },
    },
    {
      id: 'ui.grid.change_shape',
      title: msg('Change Shape'),
      isEnabled: () => isObject() && !isPicture(),
      run: (_ctx, args) => {
        const preset = arg<string>(args, 'preset');
        if (preset) return formatObject({ preset });
      },
    },
  ];
  const defs: CommandDefinition<unknown>[] = [
    ...objectDefs,
    ...GRID_FEATURE_DIALOGS.map(
      (d): CommandDefinition<unknown> => ({
        id: d.id,
        title: d.title,
        run: (_ctx, args) => {
          // Text to Columns splits one column (Excel says so before its wizard opens).
          if (d.dialog === 'textToColumns') {
            const r = selectionRanges(model).at(-1);
            if (r && r.c1 > r.c0)
              throw new CommandNotice(
                t('Text to Columns'),
                t(
                  'Text to Columns converts one column at a time. Select cells in a single column, then try again.',
                ),
              );
          }
          features.openDialog(d.dialog, arg<string>(args, 'tab') ?? arg(args, 'preset') ?? d.arg);
        },
      }),
    ),
    ...GRID_FEATURE_PANES.map(
      (p): CommandDefinition<unknown> => ({
        id: p.id,
        title: p.title,
        isChecked: () => features.state.pane === p.pane,
        run: () => features.togglePane(p.pane),
      }),
    ),
    {
      // Ctrl+T, Ctrl+L and the Format as Table gallery (a style picked first).
      id: 'grid.styles.format_as_table',
      title: msg('Format as Table'),
      run: (_ctx, args) => {
        if (table()) {
          const style = arg<string>(args, 'value');
          if (style) return exec({ TableStyle: { style } });
        }
        features.openDialog('createTable', arg<string>(args, 'value') ?? 'TableStyleMedium2');
      },
    },
    {
      id: 'suite.edit.format_painter',
      title: msg('Format Painter'),
      isChecked: () => !!features.state.painter,
      run: () => {
        // Two clicks in quick succession keep the brush (Excel's double-click).
        const now = Date.now();
        const sticky = now - lastPainter < 450;
        lastPainter = now;
        if (features.state.painter && !sticky) {
          features.set({ painter: null });
          return;
        }
        const r = selectionRanges(model).at(-1);
        if (r) features.set({ painter: { from: r, sticky } });
        model.surface?.focus();
      },
    },
    {
      id: 'grid.review.new_note',
      title: msg('New Note'),
      run: () => features.openDialog('note'),
    },
    {
      id: 'ui.grid.edit_note',
      title: msg('Edit Note'),
      isEnabled: () => !!gridFeatures(model)?.note,
      run: () => features.openDialog('note'),
    },
    {
      id: 'ui.grid.delete_note',
      title: msg('Delete Note'),
      isEnabled: () => !!gridFeatures(model)?.note,
      run: () => exec({ SetNote: { text: null, author: '', cell: null } }),
    },
    {
      id: 'ui.grid.show_all_notes',
      title: msg('Show All Notes'),
      isChecked: () => (gridFeatures(model)?.cards.length ?? 0) > 0 && showAll,
      run: async () => {
        showAll = !showAll;
        await exec({ ShowAllNotes: { show: showAll } });
      },
    },
    {
      id: 'ui.grid.convert_notes',
      title: msg('Convert to Comments'),
      run: () => exec({ ConvertNotes: { author: authorName(), now_ms: Date.now() } }),
    },
    {
      id: 'suite.review.new_comment',
      title: msg('New Comment'),
      run: () => features.set({ pane: 'comments', composer: { reply: null } }),
    },
    {
      id: 'suite.review.delete_comment',
      title: msg('Delete Comment'),
      isEnabled: () => !!gridFeatures(model)?.comment || !!gridFeatures(model)?.note,
      run: () => exec({ DeleteComment: { id: null } }),
    },
    {
      id: 'suite.review.previous_comment',
      title: msg('Previous Comment'),
      run: () => exec({ NextComment: { backward: true } }),
    },
    {
      id: 'suite.review.next_comment',
      title: msg('Next Comment'),
      run: () => exec({ NextComment: { backward: false } }),
    },
    {
      id: 'ui.grid.remove_link',
      title: msg('Remove Hyperlink'),
      isEnabled: () => !!gridFeatures(model)?.link,
      run: () => exec({ SetLink: { link: null } }),
    },
    {
      id: 'ui.grid.open_link',
      title: msg('Open Hyperlink'),
      isEnabled: () => !!gridFeatures(model)?.link,
      run: async () => {
        const l = gridFeatures(model)?.link;
        if (!l) return;
        if (l.location) {
          await model.run('grid.nav.go_to', { reference: l.location });
        } else if (l.target && /^(https?:|mailto:)/i.test(l.target)) {
          window.open(l.target, '_blank', 'noopener,noreferrer');
        }
      },
    },
    {
      id: 'grid.data.group_rows',
      title: msg('Group'),
      run: () => exec({ Group: { columns: !!model.state.grid?.whole_columns } }),
    },
    {
      id: 'grid.data.ungroup_rows',
      title: msg('Ungroup'),
      run: () => exec({ Ungroup: { columns: !!model.state.grid?.whole_columns } }),
    },
    {
      id: 'grid.data.from_text_csv',
      title: msg('From Text/CSV'),
      run: () => pickImport(features, 'Text'),
    },
    {
      id: 'grid.data.from_json',
      title: msg('From JSON'),
      run: () => pickImport(features, 'Json'),
    },
    ...(['Csv', 'Tsv', 'Json'] as const).map(
      (format): CommandDefinition<unknown> => ({
        id: `ui.grid.export_${format.toLowerCase()}`,
        title: t('Export as {format}', { format: format.toUpperCase() }),
        run: async (_ctx, args) => {
          const r = await features.ask({
            Export: { format, selection: arg<boolean>(args, 'selection') ?? false },
          });
          if (!r || !('Export' in r)) {
            model.set({ notice: t('There is nothing to export.') });
            return;
          }
          const f = r.Export;
          const url = URL.createObjectURL(new Blob([new Uint8Array(f.bytes)], { type: f.mime }));
          const a = document.createElement('a');
          a.href = url;
          a.download = f.name;
          a.click();
          setTimeout(() => URL.revokeObjectURL(url), 10_000);
          model.surface?.focus();
        },
      }),
    ),
    {
      id: 'ui.grid.print_area_set',
      title: msg('Set Print Area'),
      run: () => exec({ PrintArea: { action: 'Set' } }),
    },
    {
      id: 'ui.grid.table_totals',
      title: msg('Total Row Function'),
      isEnabled: () => !!table()?.options.total_row,
      run: (_ctx, args) => {
        const t = table();
        const fn = arg<GridTotals>(args, 'fn');
        if (!t || !fn) return;
        return exec({ TableTotals: { col: t.range.c0 + t.column, totals: fn } });
      },
    },
    {
      // Alt+Down: the filter menu of a header, a total row's functions, or the cell's list.
      id: 'ui.grid.open_dropdown',
      title: msg('Open List'),
      run: () => features.openDropdown(),
    },
    {
      id: 'ui.grid.table_remove_duplicates',
      title: msg('Remove Duplicates'),
      isEnabled: () => !!table(),
      run: () => features.openDialog('removeDuplicates'),
    },
    {
      id: 'ui.grid.convert_to_range',
      title: msg('Convert to Range'),
      isEnabled: () => !!table(),
      run: () => exec('ConvertToRange'),
    },
    tableOption('header_row', msg('Header Row')),
    tableOption('total_row', msg('Total Row')),
    tableOption('banded_rows', msg('Banded Rows')),
    tableOption('banded_columns', msg('Banded Columns')),
    tableOption('first_column', msg('First Column')),
    tableOption('last_column', msg('Last Column')),
    tableOption('filter_button', msg('Filter Button')),
    calc('Automatic', msg('Automatic')),
    calc('AutomaticExceptTables', msg('Automatic Except for Data Tables')),
    calc('Manual', msg('Manual')),
    {
      id: 'ui.grid.watch_add',
      title: msg('Add Watch'),
      run: async () => {
        const g = model.state.grid;
        const r = selectionRanges(model).at(-1);
        if (!g || !r) return;
        for (let row = r.r0; row <= Math.min(r.r1, r.r0 + 50); row++)
          for (let col = r.c0; col <= Math.min(r.c1, r.c0 + 20); col++)
            await exec({ Watch: { add: true, sheet: g.active_sheet, row, col } });
      },
    },
  ];
  let showAll = false;
  const disposables: Disposable[] = [];
  for (const d of defs) {
    const s = reg.getState(d.id);
    if (s.exists && !s.pending && !d.id.startsWith('suite.') && !OURS.has(d.id)) continue;
    disposables.push(reg.register(d));
  }
  // Table Design follows the active cell; Picture Format and Shape Format the selected object.
  let shown = '';
  let circular = 0;
  const off = model.onState((s) => {
    // A new circular reference while iteration is off: Excel's warning.
    const c = s.features.circular.length;
    if (c > 0 && circular === 0 && !s.features.iterative[0] && !features.state.dialog)
      features.openDialog('circular');
    circular = c;
    // A refused entry: the rule's alert (the dialog says what the notice would).
    if (s.features.alert && !features.state.dialog) {
      model.set({ notice: null });
      features.openDialog('validationAlert');
    }
    const list: string[] = [];
    if (s.features.table && !s.drawing) list.push('table');
    // PivotTable Analyze and Design, and the Sparkline tab.
    if (s.analysis.pivot && !s.drawing) list.push('pivot');
    if (s.analysis.sparkline && !s.drawing) list.push('sparkline');
    if (s.drawing?.kind === 'Picture') list.push('picture');
    if (s.drawing?.kind === 'Shape' || s.drawing?.kind === 'Line') list.push('shape');
    const key = list.join(',');
    if (key !== shown) {
      shown = key;
      reg.setContext({ contexts: list });
    }
  });
  return {
    dispose() {
      off();
      for (const d of disposables) d.dispose();
      if (shown) reg.setContext({ contexts: [] });
    },
  };
}

/** Ids this module owns even when another editor declared them first. */
const OURS = new Set([
  'grid.formulas.recently_used_functions',
  'grid.styles.format_as_table',
  'grid.review.new_note',
  'grid.data.group_rows',
  'grid.data.ungroup_rows',
]);

/** The browser's file picker for an import, then the import dialog with the file. */
function pickImport(features: ReturnType<typeof featuresOf>, format: 'Text' | 'Json'): Promise<void> {
  return new Promise((resolve) => {
    if (typeof document === 'undefined') {
      resolve();
      return;
    }
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = format === 'Json' ? '.json,application/json' : '.csv,.tsv,.txt,text/csv,text/plain';
    input.addEventListener(
      'change',
      async () => {
        const f = input.files?.[0];
        if (f) {
          const bytes = new Uint8Array(await f.arrayBuffer());
          features.openDialog('import', { name: f.name, bytes: Array.from(bytes), format });
        }
        resolve();
      },
      { once: true },
    );
    input.addEventListener('cancel', () => resolve(), { once: true });
    input.click();
  });
}
