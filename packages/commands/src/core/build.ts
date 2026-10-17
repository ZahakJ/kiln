/**
 * `id + args → Command`: the typed protocol command for every id of `catalog.json`. The chrome only
 * checks the *shape* of the arguments (PLAN.md §3.7 thinness: "validate form input shape"); the core
 * validates their meaning and answers with an `ErrorInfo` when it disagrees.
 *
 * A unit test fails when `catalog.json` gains a command without a builder here, so a work package
 * that adds a core command also says how the UI addresses it.
 */
import type {
  BorderPreset,
  CaretMotion,
  CellVerticalAlign,
  ChartAxisTarget,
  ChartBound,
  ChartElementKind,
  ChartKind,
  ChartLabelPosition,
  ChartLegendPosition,
  ChartMoveTarget,
  ClipboardData,
  Command,
  DocKind,
  DropCapPlacement,
  FileFormat,
  FindQuery,
  FontFormat,
  GoToTarget,
  ListNumberStyle,
  PageNumberPosition,
  PageSetupSpec,
  ParagraphFormat,
  PasteMode,
  Point,
  PrintSettings,
  SectionStart,
  SelectUnit,
  StyleSpec,
  TableAutofit,
  TableDeleteTarget,
  TableSelectTarget,
  TabStopSpec,
  TextAlign,
  TextCase,
  TextPos,
  TextSeparator,
  UnderlineKind,
  ViewMode,
} from '@kiln/protocol';
import { GRID_BUILDERS } from '../catalog/grid.ts';
import type { AppId } from '../types.ts';

/** What a builder may use besides the arguments. */
export interface BuildContext {
  /** The app of the window (New creates a document of this app). */
  app?: AppId;
}

export type CoreBuilder = (args: Args, ctx: BuildContext) => Command;

/** Arguments of a command, as given by a ribbon item, binding or caller. */
export type Args = Readonly<Record<string, unknown>>;

/** The arguments do not have the shape the command needs. */
export class CommandArgsError extends Error {
  readonly id: string;
  constructor(id: string, message: string) {
    super(`${id}: ${message}`);
    this.name = 'CommandArgsError';
    this.id = id;
  }
}

// ── Shape checks ─────────────────────────────────────────────────────────────────────────────────

class ShapeError extends Error {}

function req<T>(args: Args, key: string, check: (v: unknown) => v is T, what: string): T {
  const v = args[key];
  if (!check(v)) throw new ShapeError(`"${key}" must be ${what}`);
  return v;
}

function opt<T>(args: Args, key: string, check: (v: unknown) => v is T, what: string): T | null {
  const v = args[key];
  if (v === undefined || v === null) return null;
  if (!check(v)) throw new ShapeError(`"${key}" must be ${what} or absent`);
  return v;
}

const isString = (v: unknown): v is string => typeof v === 'string';
const isBool = (v: unknown): v is boolean => typeof v === 'boolean';
const isNumber = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const isUint = (v: unknown): v is number => isNumber(v) && Number.isInteger(v) && v >= 0;
const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);
const isPoint = (v: unknown): v is Point => isRecord(v) && isNumber(v.x) && isNumber(v.y);
const isTextPos = (v: unknown): v is TextPos => isRecord(v) && isUint(v.block) && isUint(v.offset);
const isBound = (v: unknown): v is ChartBound => v === 'Auto' || (isRecord(v) && isNumber(v.Fixed));
const isMoveTarget = (v: unknown): v is ChartMoveTarget =>
  isRecord(v) &&
  ((isRecord(v.NewSheet) && isString(v.NewSheet.name)) ||
    (isRecord(v.ObjectIn) && isString(v.ObjectIn.sheet)));

/** Enum values are strings; the core rejects unknown variants with a typed error. */
function oneOf<T extends string>(): (v: unknown) => v is T {
  return (v: unknown): v is T => isString(v) && v.length > 0;
}

const DOC_KIND: Readonly<Partial<Record<AppId, DocKind>>> = {
  write: 'Write',
  grid: 'Grid',
  deck: 'Deck',
  notes: 'Notes',
};

const suite = (c: Extract<Command, { Suite: unknown }>['Suite']): Command => ({ Suite: c });
const write = (c: Extract<Command, { Write: unknown }>['Write']): Command => ({ Write: c });
type EditCmd = Extract<Extract<Command, { Suite: unknown }>['Suite'], { Edit: unknown }>['Edit'];
const edit = (c: EditCmd): Command => ({ Suite: { Edit: c } });

// ── WP-64 argument shapes ──────────────────────────────────────────────────────────────────────

const isClipboard = (v: unknown): v is ClipboardData =>
  isRecord(v) &&
  isString(v.text) &&
  (v.html === undefined || v.html === null || isString(v.html)) &&
  (v.kiln === undefined || v.kiln === null || isString(v.kiln));
const clipboard = (a: Args): ClipboardData | null => {
  const d = opt(a, 'data', isClipboard, 'clipboard data');
  return d ? { text: d.text, html: d.html ?? null, kiln: d.kiln ?? null } : null;
};
/** `RRGGBB` from a colour choice of the colour pickers, a hex string, or `null` (automatic). */
function colorArg(a: Args, key = 'color'): string | null {
  const v = a[key];
  if (v === undefined || v === null) return null;
  if (isString(v)) {
    const hex = v.replace(/^#/, '');
    if (!/^[0-9a-f]{6}$/i.test(hex)) throw new ShapeError(`"${key}" must be RRGGBB`);
    return hex.toUpperCase();
  }
  if (isRecord(v) && (v.type === 'auto' || v.type === 'none')) return null;
  if (isRecord(v) && isString(v.hex)) return colorArg({ [key]: v.hex }, key);
  throw new ShapeError(`"${key}" must be a colour`);
}
/** A number given as a number or a numeric string (the font size box). */
function numberArg(a: Args, keys: readonly string[], what: string): number {
  for (const k of keys) {
    const v = a[k];
    if (isNumber(v)) return v;
    if (isString(v) && v.trim() !== '' && Number.isFinite(Number(v))) return Number(v);
  }
  throw new ShapeError(`"${keys[0]}" must be ${what}`);
}
const capital = <T extends string>(v: string): T => (v.charAt(0).toUpperCase() + v.slice(1)) as T;
const PAPER: Readonly<Record<string, string>> = {
  letter: 'Letter',
  legal: 'Legal',
  executive: 'Executive',
  a3: 'A3',
  a4: 'A4',
  a5: 'A5',
  b5: 'B5',
  tabloid: 'Tabloid',
};
const isQuery = (v: unknown): v is FindQuery => isRecord(v) && isString(v.text);
const query = (a: Args): FindQuery => {
  const q = req(a, 'query', isQuery, 'a find query');
  return {
    text: q.text,
    match_case: q.match_case === true,
    whole_word: q.whole_word === true,
    wildcards: q.wildcards === true,
    regex: q.regex === true,
  };
};
const isObject = <T>(v: unknown): v is T => isRecord(v);

/** Every core command id and how its `Command` is built. */
export const CORE_BUILDERS: Readonly<Record<string, CoreBuilder>> = Object.freeze({
  // Files
  'suite.file.open': (a) => suite({ Open: { path: opt(a, 'path', isString, 'a path') } }),
  'suite.file.new': (a, ctx) => {
    const kind =
      opt(a, 'kind', oneOf<DocKind>(), 'a document kind') ?? (ctx.app ? DOC_KIND[ctx.app] : undefined);
    if (!kind) throw new ShapeError('"kind" is needed outside an app window');
    return suite({ New: { kind } });
  },
  'suite.file.save': () => suite('Save'),
  'suite.file.save_as': (a) =>
    suite({
      SaveAs: {
        path: opt(a, 'path', isString, 'a path'),
        format: opt(a, 'format', oneOf<FileFormat>(), 'a file format'),
      },
    }),
  'suite.file.close': () => suite('Close'),
  // Print and PDF (WP-67): the settings come from the Print and Export panes as a whole.
  'suite.file.print_pdf': (a) =>
    suite({
      PrintPdf: { settings: req(a, 'settings', isRecord, 'print settings') as unknown as PrintSettings },
    }),
  'suite.file.save_pdf': (a) =>
    suite({
      SavePdf: {
        path: opt(a, 'path', isString, 'a path'),
        settings: req(a, 'settings', isRecord, 'print settings') as unknown as PrintSettings,
      },
    }),
  // Edit and view
  'suite.edit.undo': () => suite('Undo'),
  'suite.edit.redo': () => suite('Redo'),
  'suite.view.zoom': (a) => suite({ SetZoom: { zoom: req(a, 'zoom', isNumber, 'a number') } }),
  'suite.view.mode': (a) =>
    suite({ SetViewMode: { mode: req(a, 'mode', oneOf<ViewMode>(), 'a view mode') } }),
  'suite.view.repaint': () => suite('Repaint'),
  // Diagnostics
  'suite.dev.ping': () => suite('Ping'),
  'suite.dev.echo': (a) => suite({ Echo: { text: req(a, 'text', isString, 'text') } }),
  // Charts (WP-73)
  'suite.chart.insert': (a) =>
    suite({ ChartInsert: { kind: opt(a, 'kind', oneOf<ChartKind>(), 'a chart kind') } }),
  'suite.chart.change_type': (a) =>
    suite({ ChartChangeType: { kind: req(a, 'kind', oneOf<ChartKind>(), 'a chart kind') } }),
  'suite.chart.switch_row_column': () => suite('ChartSwitchRowColumn'),
  'suite.chart.select_data': (a) =>
    suite({ ChartSelectData: { range: req(a, 'range', isString, 'a range') } }),
  'suite.chart.element': (a) =>
    suite({
      ChartSetElement: {
        element: req(a, 'element', oneOf<ChartElementKind>(), 'a chart element'),
        on: req(a, 'on', isBool, 'true or false'),
      },
    }),
  'suite.chart.legend': (a) =>
    suite({
      ChartSetLegend: { position: opt(a, 'position', oneOf<ChartLegendPosition>(), 'a legend position') },
    }),
  'suite.chart.data_labels': (a) =>
    suite({
      ChartSetDataLabels: { position: opt(a, 'position', oneOf<ChartLabelPosition>(), 'a label position') },
    }),
  'suite.chart.quick_layout': (a) =>
    suite({ ChartQuickLayout: { layout: req(a, 'layout', isUint, 'a layout index') } }),
  'suite.chart.style': (a) => suite({ ChartSetStyle: { style: req(a, 'style', isUint, 'a style index') } }),
  'suite.chart.colors': (a) =>
    suite({ ChartSetColors: { palette: req(a, 'palette', isUint, 'a palette index') } }),
  'suite.chart.title': (a) => suite({ ChartSetTitle: { text: req(a, 'text', isString, 'text') } }),
  'suite.chart.move': (a) => suite({ ChartMove: { to: req(a, 'to', isMoveTarget, 'a move target') } }),
  'suite.chart.format_series': (a) =>
    suite({
      ChartFormatSeries: {
        series: req(a, 'series', isUint, 'a series index'),
        fill: opt(a, 'fill', isString, 'a colour'),
        line: opt(a, 'line', isString, 'a colour'),
        line_width: opt(a, 'line_width', isNumber, 'a width'),
        secondary: opt(a, 'secondary', isBool, 'true or false'),
      },
    }),
  'suite.chart.format_axis': (a) =>
    suite({
      ChartFormatAxis: {
        axis: req(a, 'axis', oneOf<ChartAxisTarget>(), 'an axis'),
        min: opt(a, 'min', isBound, 'a bound'),
        max: opt(a, 'max', isBound, 'a bound'),
        major_unit: opt(a, 'major_unit', isBound, 'a bound'),
        log_base: opt(a, 'log_base', isBound, 'a bound'),
        reversed: opt(a, 'reversed', isBool, 'true or false'),
        number_format: opt(a, 'number_format', isString, 'a format code'),
      },
    }),
  // Write text editing (WP-62/WP-64)
  'write.edit.insert_text': (a) => write({ InsertText: { text: req(a, 'text', isString, 'text') } }),
  'write.edit.delete_backward': () => write('DeleteBackward'),
  'write.edit.delete_forward': () => write('DeleteForward'),
  'write.edit.delete_word_backward': () => write('DeleteWordBackward'),
  'write.edit.delete_word_forward': () => write('DeleteWordForward'),
  'write.caret.move': (a) =>
    write({
      MoveCaret: {
        motion: req(a, 'motion', oneOf<CaretMotion>(), 'a caret motion'),
        extend: opt(a, 'extend', isBool, 'true or false') ?? false,
      },
    }),
  'write.caret.select_to': (a) =>
    write({
      SelectTo: {
        pos: req(a, 'pos', isTextPos, 'a text position'),
        extend: opt(a, 'extend', isBool, 'true or false') ?? false,
      },
    }),
  'write.caret.select_all': () => write('SelectAll'),
  'write.caret.select_at': (a) =>
    write({
      SelectAt: {
        point: req(a, 'point', isPoint, 'a point'),
        extend: opt(a, 'extend', isBool, 'true or false') ?? false,
        unit: opt(a, 'unit', oneOf<SelectUnit>(), 'a unit') ?? 'Char',
      },
    }),
  'write.ime.set_composition': (a) =>
    write({
      SetComposition: { text: req(a, 'text', isString, 'text'), caret: req(a, 'caret', isUint, 'an offset') },
    }),
  'write.ime.commit_composition': (a) =>
    write({ CommitComposition: { text: req(a, 'text', isString, 'text') } }),
  'write.ime.cancel_composition': () => write('CancelComposition'),
  // Shared editing commands (WP-64): the clipboard, formatting, lists, page setup, tables.
  'suite.edit.cut': () => edit('Cut'),
  'suite.edit.copy': () => edit('Copy'),
  'suite.edit.paste': (a) => edit({ Paste: { data: clipboard(a) } }),
  'suite.edit.paste_keep_source': (a) => edit({ PasteKeepSource: { data: clipboard(a) } }),
  'suite.edit.paste_merge_formatting': (a) => edit({ PasteMerge: { data: clipboard(a) } }),
  'suite.edit.paste_text_only': (a) => edit({ PasteTextOnly: { data: clipboard(a) } }),
  'suite.edit.paste_as': (a) =>
    edit({ PasteAs: { mode: req(a, 'mode', oneOf<PasteMode>(), 'a paste mode'), data: clipboard(a) } }),
  'suite.edit.repeat': () => edit('Repeat'),
  'suite.format.bold': () => edit('Bold'),
  'suite.format.italic': () => edit('Italic'),
  'suite.format.underline': () => edit('Underline'),
  'suite.format.double_underline': () => edit('DoubleUnderline'),
  'suite.format.strikethrough': () => edit('Strikethrough'),
  'suite.format.subscript': () => edit('Subscript'),
  'suite.format.superscript': () => edit('Superscript'),
  'suite.format.all_caps': () => edit('AllCaps'),
  'suite.format.font_family': (a) => {
    const family = opt(a, 'family', isString, 'a font name') ?? req(a, 'value', isString, 'a font name');
    return edit({ FontFamily: { family } });
  },
  'suite.format.font_size': (a) => edit({ FontSize: { size: numberArg(a, ['size', 'value'], 'a size') } }),
  'suite.format.grow_font': (a) =>
    edit({ GrowFont: { one_point: a.step !== undefined || a.one_point === true } }),
  'suite.format.shrink_font': (a) =>
    edit({ ShrinkFont: { one_point: a.step !== undefined || a.one_point === true } }),
  'suite.format.change_case': (a) => {
    const c = opt(a, 'case', isString, 'a case');
    return edit({ ChangeCase: { case: c ? capital<TextCase>(c) : null } });
  },
  'suite.format.clear_formatting': () => edit('ClearFormatting'),
  'suite.format.font_color': (a) => edit({ FontColor: { color: colorArg(a) } }),
  'suite.format.highlight': (a) => edit({ Highlight: { color: colorArg(a) } }),
  'suite.format.bullets': (a) => edit({ Bullets: { glyph: opt(a, 'glyph', isString, 'a bullet') } }),
  'suite.format.numbering': (a) =>
    edit({ Numbering: { style: opt(a, 'style', oneOf<ListNumberStyle>(), 'a numbering style') } }),
  'suite.format.multilevel_list': (a) => edit({ MultilevelList: { legal: a.legal === true } }),
  'suite.format.indent': () => edit('Indent'),
  'suite.format.outdent': () => edit('Outdent'),
  'suite.format.align_left': () => edit('AlignLeft'),
  'suite.format.align_center': () => edit('AlignCenter'),
  'suite.format.align_right': () => edit('AlignRight'),
  'suite.format.justify': () => edit('Justify'),
  'suite.format.line_spacing': (a) =>
    edit({ LineSpacing: { spacing: numberArg(a, ['lineSpacing', 'spacing', 'value'], 'a line spacing') } }),
  'suite.insert.date': () => edit('InsertDate'),
  'suite.insert.time': () => edit('InsertTime'),
  'suite.insert.pictures': () => edit('InsertPictures'),
  'suite.layout.margins': (a) =>
    edit({ Margins: { preset: capital(req(a, 'preset', isString, 'a margins preset')) } }),
  'suite.layout.orientation': (a) =>
    edit({ Orientation: { orientation: capital(req(a, 'orientation', isString, 'an orientation')) } }),
  'suite.layout.size': (a) => {
    const size = req(a, 'size', isString, 'a paper size');
    const paper = PAPER[size.toLowerCase()];
    if (!paper) throw new ShapeError(`"size" must be one of ${Object.keys(PAPER).join(', ')}`);
    return edit({ PageSize: { size: paper as never } });
  },
  'suite.table.insert_row_above': () => edit('TableInsertRowAbove'),
  'suite.table.insert_row_below': () => edit('TableInsertRowBelow'),
  'suite.table.insert_column_left': () => edit('TableInsertColumnLeft'),
  'suite.table.insert_column_right': () => edit('TableInsertColumnRight'),
  'suite.table.delete': (a) =>
    edit({ TableDelete: { what: opt(a, 'what', oneOf<TableDeleteTarget>(), 'a delete target') ?? 'Rows' } }),
  'suite.table.merge_cells': () => edit('TableMergeCells'),
  'suite.table.split_cells': (a) =>
    edit({
      TableSplitCells: {
        rows: opt(a, 'rows', isUint, 'a row count') ?? 1,
        cols: opt(a, 'cols', isUint, 'a column count') ?? 2,
      },
    }),
  'suite.table.distribute_rows': () => edit('TableDistributeRows'),
  'suite.table.distribute_columns': () => edit('TableDistributeColumns'),
  'suite.table.cell_alignment': (a) =>
    edit({
      TableCellAlignment: {
        vertical: opt(a, 'vertical', oneOf<CellVerticalAlign>(), 'a vertical alignment') ?? 'Center',
        horizontal: opt(a, 'horizontal', oneOf<TextAlign>(), 'an alignment') ?? 'Center',
      },
    }),
  'suite.table.repeat_header_rows': () => edit('TableRepeatHeaderRows'),
  'suite.table.convert_to_text': (a) =>
    edit({
      TableConvertToText: { separator: opt(a, 'separator', oneOf<TextSeparator>(), 'a separator') ?? 'Tabs' },
    }),
  'suite.table.header_row': () => edit('TableHeaderRow'),
  'suite.table.total_row': () => edit('TableTotalRow'),
  'suite.table.banded_rows': () => edit('TableBandedRows'),
  'suite.table.first_column': () => edit('TableFirstColumn'),
  'suite.table.banded_columns': () => edit('TableBandedColumns'),
  'suite.table.styles': (a) => {
    const style = opt(a, 'style', isString, 'a table style') ?? req(a, 'value', isString, 'a table style');
    return edit({ TableStyle: { style } });
  },
  'suite.table.shading': (a) => edit({ TableShading: { color: colorArg(a) } }),
  'suite.table.borders': (a) =>
    edit({ TableBorders: { preset: opt(a, 'preset', oneOf<BorderPreset>(), 'a border preset') ?? 'All' } }),
  // The Write editor (WP-64).
  'write.edit.tab': (a) => write({ Tab: { back: a.back === true } }),
  'write.edit.line_break': () => write('LineBreak'),
  'write.paragraph.show_marks': () => write('ShowMarks'),
  'write.paragraph.clear_paragraph_formatting': () => write('ClearParagraphFormatting'),
  'write.paragraph.clear_character_formatting': () => write('ClearCharacterFormatting'),
  'write.paragraph.move_paragraph_up': () => write('MoveParagraphUp'),
  'write.paragraph.move_paragraph_down': () => write('MoveParagraphDown'),
  'write.paragraph.promote': () => write('Promote'),
  'write.paragraph.demote': () => write('Demote'),
  'write.paragraph.shading': (a) => write({ ParagraphShading: { color: colorArg(a) } }),
  'write.paragraph.borders': (a) =>
    write({
      ParagraphBorders: { preset: opt(a, 'preset', oneOf<BorderPreset>(), 'a border preset') ?? 'Bottom' },
    }),
  'write.format.paragraph': (a) =>
    write({ SetParagraph: { format: req(a, 'format', isObject<ParagraphFormat>, 'paragraph settings') } }),
  'write.format.font': (a) =>
    write({ SetFont: { format: req(a, 'format', isObject<FontFormat>, 'font settings') } }),
  'write.format.tabs': (a) =>
    write({
      SetTabs: {
        tabs: req(a, 'tabs', (v): v is TabStopSpec[] => Array.isArray(v) && v.every(isRecord), 'tab stops'),
      },
    }),
  'write.format.small_caps': () => write('SmallCaps'),
  'write.format.underline_style': (a) =>
    write({ UnderlineStyle: { style: opt(a, 'style', oneOf<UnderlineKind>(), 'an underline style') } }),
  'write.styles.apply': (a) => {
    const style_id = opt(a, 'style_id', isString, 'a style id') ?? req(a, 'value', isString, 'a style id');
    return write({ ApplyStyle: { style_id } });
  },
  'write.styles.normal': () => write('StyleNormal'),
  'write.styles.heading_1': () => write('StyleHeading1'),
  'write.styles.heading_2': () => write('StyleHeading2'),
  'write.styles.heading_3': () => write('StyleHeading3'),
  'write.styles.title': () => write('StyleTitle'),
  'write.styles.update_from_selection': (a) =>
    write({ UpdateStyleFromSelection: { style_id: opt(a, 'style_id', isString, 'a style id') } }),
  'write.styles.new_from_selection': (a) =>
    write({ NewStyleFromSelection: { name: req(a, 'name', isString, 'a style name') } }),
  'write.styles.modify': (a) =>
    write({
      ModifyStyle: {
        style_id: req(a, 'style_id', isString, 'a style id'),
        spec: req(a, 'spec', isObject<StyleSpec>, 'style settings'),
      },
    }),
  'write.list.set_level': (a) => write({ ListLevel: { level: req(a, 'level', isUint, 'a level') } }),
  'write.list.restart': () => write('RestartNumbering'),
  'write.list.continue': () => write('ContinueNumbering'),
  'write.list.set_value': (a) => write({ SetNumberingValue: { value: req(a, 'value', isUint, 'a number') } }),
  'write.insert.page_break': () => write('PageBreak'),
  'write.insert.column_break': () => write('ColumnBreak'),
  'write.insert.blank_page': () => write('BlankPage'),
  'write.layout.section_break': (a) =>
    write({ SectionBreak: { kind: req(a, 'kind', oneOf<SectionStart>(), 'a section start') } }),
  'write.layout.columns': (a) =>
    write({ Columns: { count: numberArg(a, ['count', 'value'], 'a column count') } }),
  'write.layout.indent_left': (a) =>
    write({ IndentLeft: { cm: numberArg(a, ['cm', 'value'], 'centimetres') } }),
  'write.layout.indent_right': (a) =>
    write({ IndentRight: { cm: numberArg(a, ['cm', 'value'], 'centimetres') } }),
  'write.layout.spacing_before': (a) =>
    write({ SpacingBefore: { pt: numberArg(a, ['pt', 'value'], 'points') } }),
  'write.layout.spacing_after': (a) =>
    write({ SpacingAfter: { pt: numberArg(a, ['pt', 'value'], 'points') } }),
  'write.layout.page_setup': (a) =>
    write({ PageSetup: { setup: req(a, 'setup', isObject<PageSetupSpec>, 'page settings') } }),
  'write.insert.header': () => write('EditHeader'),
  'write.insert.footer': () => write('EditFooter'),
  'write.header_footer.close': () => write('CloseHeaderFooter'),
  'write.header_footer.different_first_page': () => write('DifferentFirstPage'),
  'write.header_footer.different_odd_even': () => write('DifferentOddEven'),
  'write.header_footer.link_to_previous': () => write('LinkToPrevious'),
  'write.insert.page_number': (a) =>
    write({
      PageNumber: {
        position: opt(a, 'position', oneOf<PageNumberPosition>(), 'a position') ?? 'BottomCenter',
      },
    }),
  'write.insert.link': (a) =>
    write({
      InsertLink: {
        url: req(a, 'url', isString, 'a web address'),
        text: opt(a, 'text', isString, 'text'),
        tooltip: opt(a, 'tooltip', isString, 'a ScreenTip'),
      },
    }),
  'write.insert.remove_link': () => write('RemoveLink'),
  'write.insert.bookmark': (a) =>
    write({ InsertBookmark: { name: req(a, 'name', isString, 'a bookmark name') } }),
  'write.insert.date_time': (a) =>
    write({
      InsertDateTime: {
        format: req(a, 'format', isString, 'a date format'),
        as_field: a.as_field === true,
      },
    }),
  'write.insert.field': (a) => write({ InsertField: { code: req(a, 'code', isString, 'a field code') } }),
  'write.insert.update_fields': () => write('UpdateFields'),
  'write.insert.drop_cap': (a) =>
    write({ DropCap: { placement: opt(a, 'placement', oneOf<DropCapPlacement>(), 'a placement') } }),
  'write.insert.picture': (a) =>
    write({
      InsertPicture: {
        name: req(a, 'name', isString, 'a file name'),
        bytes: req(
          a,
          'bytes',
          (v): v is number[] => Array.isArray(v) && v.every((b) => isUint(b) && b < 256),
          'bytes',
        ),
        floating: a.floating === true,
      },
    }),
  'write.table.insert': (a) =>
    write({
      InsertTable: {
        rows: req(a, 'rows', isUint, 'a row count'),
        cols: req(a, 'cols', isUint, 'a column count'),
        autofit: opt(a, 'autofit', oneOf<TableAutofit>(), 'an AutoFit mode') ?? 'Fixed',
      },
    }),
  'write.insert.convert_text_to_table': (a) =>
    write({
      ConvertTextToTable: { separator: opt(a, 'separator', oneOf<TextSeparator>(), 'a separator') ?? 'Tabs' },
    }),
  'write.table.autofit': (a) =>
    write({ TableAutofit: { mode: req(a, 'mode', oneOf<TableAutofit>(), 'a mode') } }),
  'write.table.align': (a) =>
    write({ TableAlign: { align: req(a, 'align', oneOf<TextAlign>(), 'an alignment') } }),
  'write.table.split': () => write('TableSplit'),
  'write.table.select': (a) =>
    write({ TableSelect: { what: req(a, 'what', oneOf<TableSelectTarget>(), 'a selection target') } }),
  'write.table.cell_size': (a) =>
    write({
      TableCellSize: {
        width: opt(a, 'width', isNumber, 'points'),
        height: opt(a, 'height', isNumber, 'points'),
      },
    }),
  'write.find.next': (a) => write({ FindNext: { query: query(a), backward: a.backward === true } }),
  'write.find.replace': (a) =>
    write({ Replace: { query: query(a), replacement: req(a, 'replacement', isString, 'a replacement') } }),
  'write.find.replace_all': (a) =>
    write({ ReplaceAll: { query: query(a), replacement: req(a, 'replacement', isString, 'a replacement') } }),
  'write.find.go_to': (a) =>
    write({ GoTo: { target: req(a, 'target', isObject<GoToTarget>, 'a Go To target') } }),
  // Grid (WP-71): built in its catalog module.
  ...GRID_BUILDERS,
} satisfies Record<string, CoreBuilder>);

/**
 * The typed `Command` for a core command id. Throws {@link CommandArgsError} for an id without a
 * builder or arguments of the wrong shape.
 */
export function buildCommand(id: string, args: unknown = {}, ctx: BuildContext = {}): Command {
  const builder = CORE_BUILDERS[id];
  if (!builder) throw new CommandArgsError(id, 'not a core command');
  const record: Args = args === undefined || args === null ? {} : isRecord(args) ? args : { value: args };
  try {
    return builder(record, ctx);
  } catch (error) {
    if (error instanceof ShapeError) throw new CommandArgsError(id, error.message);
    throw error;
  }
}
