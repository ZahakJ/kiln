/**
 * `id + args → Command`: the typed protocol command for every id of `catalog.json`. The chrome only
 * checks the *shape* of the arguments (PLAN.md §3.7 thinness: "validate form input shape"); the core
 * validates their meaning and answers with an `ErrorInfo` when it disagrees.
 *
 * A unit test fails when `catalog.json` gains a command without a builder here, so a work package
 * that adds a core command also says how the UI addresses it.
 */
import type {
  CaretMotion,
  ChartAxisTarget,
  ChartBound,
  ChartElementKind,
  ChartKind,
  ChartLabelPosition,
  ChartLegendPosition,
  ChartMoveTarget,
  Command,
  DocKind,
  FileFormat,
  Point,
  PrintSettings,
  SelectUnit,
  TextPos,
  ViewMode,
} from '@kiln/protocol';
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
