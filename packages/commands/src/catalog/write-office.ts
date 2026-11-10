/**
 * Kiln Write's commands that complete Word's ribbon: how the UI builds each core
 * command's typed `Command` from its arguments (symbol fonts, Alt+X, AutoCorrect, Page Movement,
 * hiding and grouping objects, balloons, drag and drop, Table Properties, Sort, the proofing
 * language, Shape Styles, gradients, effects, text direction, Crop to Shape, Corrections), their
 * declarations, and the list of ribbon ids the Write editor implements in its chrome.
 */
import type {
  AutoCorrectEntry,
  BalloonMode,
  Command,
  GradientSpec,
  PictureCorrection,
  Rect,
  ShapeEffect,
  ShapeStyleLevel,
  SortKind,
  TablePropertiesSpec,
  TextDirection,
  TextPos,
} from '@kiln/protocol';
import { mhead, mitem, msep, msub, type RibbonGalleryEntry, type RibbonMenuEntry } from '../ribbon.ts';
import type { CommandDeclaration } from '../types.ts';
import { c, section, WRITE } from './define.ts';

type Args = Readonly<Record<string, unknown>>;
type WriteCmd = Extract<Command, { Write: unknown }>['Write'];
type Builder = (args: Args) => Command;

class ArgsError extends Error {}

const isString = (v: unknown): v is string => typeof v === 'string';
const isBool = (v: unknown): v is boolean => typeof v === 'boolean';
const isNumber = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const isUint = (v: unknown): v is number => isNumber(v) && Number.isInteger(v) && v >= 0;
const isInt = (v: unknown): v is number => isNumber(v) && Number.isInteger(v);
const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);
const isTextPos = (v: unknown): v is TextPos => isRecord(v) && isUint(v.block) && isUint(v.offset);
const isEnum = (v: unknown): v is string => isString(v) && /^[A-Z][A-Za-z0-9]*$/.test(v);

function req<T>(a: Args, key: string, check: (v: unknown) => v is T, what: string): T {
  const v = a[key];
  if (!check(v)) throw new ArgsError(`"${key}" must be ${what}`);
  return v;
}
function opt<T>(a: Args, key: string, check: (v: unknown) => v is T, what: string): T | null {
  const v = a[key];
  if (v === undefined || v === null) return null;
  if (!check(v)) throw new ArgsError(`"${key}" must be ${what} or absent`);
  return v;
}

const w = (cmd: WriteCmd): Command => ({ Write: cmd });

function entries(v: unknown): AutoCorrectEntry[] {
  if (!Array.isArray(v)) throw new ArgsError('"entries" must be a list');
  return v.map((e) => {
    if (!isRecord(e) || !isString(e.from) || !isString(e.to)) {
      throw new ArgsError('each AutoCorrect entry has "from" and "to"');
    }
    return { from: e.from, to: e.to };
  });
}

/** Every core command id and how its `Command` is built. */
export const WRITE_OFFICE_BUILDERS: Readonly<Record<string, Builder>> = Object.freeze({
  'write.insert.symbol_char': (a) =>
    w({
      InsertSymbol: {
        font: req(a, 'font', isString, 'a symbol font'),
        code: req(a, 'code', isUint, 'a character code'),
      },
    }),
  'write.edit.toggle_unicode': () => w('ToggleUnicode'),
  'write.options.autocorrect': (a) => w({ SetAutoCorrect: { entries: entries(a.entries) } }),
  'write.view.page_movement': (a) =>
    w({ PageMovement: { side_to_side: req(a, 'side_to_side', isBool, 'true or false') } }),
  'write.view.split_region': (a) => {
    const r = a.region;
    const ok =
      isRecord(r) && [r.x, r.y, r.width, r.height].every((v) => typeof v === 'number' && Number.isFinite(v));
    if (r !== null && r !== undefined && !ok) throw new TypeError('region: expected a rectangle or null');
    return w({ SplitRegion: { region: ok ? (r as unknown as Rect) : null } });
  },
  'write.arrange.visible': (a) =>
    w({
      ObjectVisible: {
        id: opt(a, 'id', isUint, 'an object id'),
        visible: req(a, 'visible', isBool, 'true or false'),
      },
    }),
  'write.review.balloons': (a) =>
    w({ Balloons: { mode: req(a, 'mode', isEnum, 'a balloon mode') as BalloonMode } }),
  'write.edit.move_selection': (a) =>
    w({
      MoveSelection: {
        to: req(a, 'to', isTextPos, 'a text position'),
        copy: opt(a, 'copy', isBool, 'true or false') ?? false,
      },
    }),
  'write.table.properties': (a) =>
    w({
      TableProperties: {
        spec: req(a, 'spec', isRecord, 'table properties') as unknown as TablePropertiesSpec,
      },
    }),
  'write.arrange.group': (a) =>
    w({ GroupObjects: { ungroup: opt(a, 'ungroup', isBool, 'true or false') ?? false } }),
  'write.file.text_encoding': (a) =>
    w({
      TextEncoding: {
        encoding: req(a, 'encoding', isString, 'an encoding name'),
        reload: opt(a, 'reload', isBool, 'true or false') ?? false,
      },
    }),
  'write.shape.style': (a) => {
    // The gallery passes its tile id, "Level:colour".
    const tile = opt(a, 'value', isString, 'a gallery tile');
    const [lv, col] = tile ? tile.split(':') : [];
    const level = (lv ?? req(a, 'level', isEnum, 'a style level')) as ShapeStyleLevel;
    const color = col !== undefined ? Number(col) : req(a, 'color', isUint, 'a theme colour 0–6');
    if (!isEnum(level) || !isUint(color)) throw new ArgsError('"value" must be Level:colour');
    return w({ ShapeStyle: { preset: { color, level } } });
  },
  'write.shape.gradient': (a) => {
    const g = opt(a, 'gradient', isRecord, 'a gradient');
    return w({
      ShapeGradient: {
        gradient: g
          ? ({
              from: req(g, 'from', isString, 'RRGGBB'),
              to: req(g, 'to', isString, 'RRGGBB'),
              angle: opt(g, 'angle', isNumber, 'degrees') ?? 90,
            } satisfies GradientSpec)
          : null,
      },
    });
  },
  'write.shape.effect': (a) =>
    w({ ShapeEffect: { effect: req(a, 'effect', isEnum, 'an effect') as ShapeEffect } }),
  'write.shape.text_direction': (a) =>
    w({
      ShapeTextDirection: { direction: req(a, 'direction', isEnum, 'a direction') as TextDirection },
    }),
  'write.picture.crop_to_shape': (a) => w({ CropToShape: { preset: req(a, 'preset', isString, 'a shape') } }),
  'write.picture.corrections': (a) =>
    w({
      PictureCorrections: {
        correction: {
          brightness: req(a, 'brightness', isInt, 'a percentage'),
          contrast: req(a, 'contrast', isInt, 'a percentage'),
        } satisfies PictureCorrection,
      },
    }),
  'write.review.set_language': (a) =>
    w({
      SetLanguage: {
        lang: req(a, 'lang', isString, 'a language tag'),
        no_proof: opt(a, 'no_proof', isBool, 'true or false') ?? false,
      },
    }),
  'write.insert.equation': (a) =>
    w({ InsertEquation: { linear: req(a, 'linear', isString, 'an equation') } }),
  'write.paragraph.sort_by': (a) =>
    w({
      Sort: {
        spec: {
          kind: (opt(a, 'kind', isEnum, 'Text, Number or Date') ?? 'Text') as SortKind,
          descending: opt(a, 'descending', isBool, 'true or false') ?? false,
          column: opt(a, 'column', isUint, 'a column') ?? 0,
          header: opt(a, 'header', isBool, 'true or false') ?? false,
          case_sensitive: opt(a, 'case_sensitive', isBool, 'true or false') ?? false,
        },
      },
    }),
});

const W = { apps: WRITE };

/** The declarations of the core and chrome commands. */
export const writeOfficeCommands: readonly CommandDeclaration[] = [
  ...section('Insert ▸ Symbols', W, [
    c('write.insert.symbol_char', 'Insert Symbol Character', 'SymbolOmega', { palette: false }),
    c('write.insert.equation', 'Insert Equation', 'Equation', { palette: false, tier: 'P1' }),
    c('write.edit.toggle_unicode', 'Toggle Character Code', 'SymbolOmega', {
      officeName: 'Toggle Unicode (Alt+X)',
      description: 'Turn the hexadecimal code before the cursor into its character, or back.',
      synonyms: ['unicode', 'alt x', 'character code'],
    }),
  ]),
  ...section('Options', W, [
    c('write.options.autocorrect', 'AutoCorrect List', 'Spelling', { palette: false }),
  ]),
  ...section('View', W, [
    c('write.view.page_movement', 'Page Movement', 'VerticalPages', { palette: false }),
    c('write.view.split_region', 'Split Pane Region', 'SplitWindow', { palette: false }),
    c('ui.write.zoom_dialog', 'Zoom…', 'Zoom100', {
      officeName: 'Zoom',
      description: 'Choose a zoom level or fit the page, its width or several pages.',
      synonyms: ['magnify', 'zoom level', 'percent'],
      keyTip: 'Q',
    }),
    c('ui.write.split_remove', 'Remove Split', 'SplitWindow', { officeName: 'Remove Split', palette: false }),
    c('ui.write.header_distance', 'Header from Top', 'Header', {
      officeName: 'Header from Top',
      palette: false,
    }),
    c('ui.write.footer_distance', 'Footer from Bottom', 'Footer', {
      officeName: 'Footer from Bottom',
      palette: false,
    }),
  ]),
  ...section('Layout ▸ Arrange', W, [
    c('write.arrange.visible', 'Show or Hide Object', 'SelectionPane', { palette: false }),
    c('write.arrange.group', 'Group Objects', 'GroupObjects', { palette: false }),
    c('write.file.text_encoding', 'Text Encoding', 'Save', { palette: false }),
    c('ui.write.ungroup', 'Ungroup', 'GroupObjects', { officeName: 'Ungroup' }),
    c('ui.write.distribute', 'Distribute Objects', 'AlignObjects', { palette: false }),
    c('ui.write.gradient', 'Gradient', 'ShapeFill', { palette: false }),
    c('ui.write.corrections_dialog', 'Picture Corrections Options…', 'Contrast', {
      officeName: 'Picture Corrections Options',
    }),
  ]),
  ...section('Review', W, [
    c('write.review.balloons', 'Balloons', 'ShowMarkup', { palette: false }),
    c('write.review.set_language', 'Set Proofing Language', 'Language', { palette: false }),
    c('ui.write.language_dialog', 'Language…', 'Language', {
      officeName: 'Set Proofing Language',
      synonyms: ['proofing language', 'do not check spelling'],
    }),
  ]),
  ...section('Editing', W, [
    c('write.edit.move_selection', 'Move Selection', 'Position', { palette: false }),
  ]),
  ...section('Table Layout', W, [
    c('write.table.properties', 'Table Properties', 'InsertTable', { palette: false }),
    c('ui.write.table_properties_dialog', 'Table Properties…', 'InsertTable', {
      officeName: 'Table Properties',
      synonyms: ['row height', 'column width', 'cell alignment', 'text wrapping table'],
    }),
  ]),
  ...section('Shape Format', W, [
    c('write.shape.style', 'Shape Style', 'QuickStyles', { palette: false }),
    c('write.shape.gradient', 'Shape Gradient', 'ShapeFill', { palette: false }),
    c('write.shape.effect', 'Shape Effect', 'ShapeEffects', { palette: false }),
    c('write.shape.text_direction', 'Text Direction', 'TextDirection', { palette: false }),
  ]),
  ...section('Picture Format', W, [
    c('write.picture.crop_to_shape', 'Crop to Shape', 'Crop', { palette: false }),
    c('write.picture.corrections', 'Picture Corrections', 'Contrast', { palette: false }),
  ]),
  ...section('Home ▸ Paragraph', W, [
    c('write.paragraph.sort_by', 'Sort Paragraphs or Rows', 'SortText', { palette: false }),
    c('ui.write.sort_dialog', 'Sort…', 'SortText', {
      officeName: 'Sort Text',
      synonyms: ['alphabetise', 'order lines', 'sort table'],
    }),
  ]),
];

// ── Ribbon menus and galleries ───────────────────────────────────────────────────────────────

/** Word's Office theme colours (the gallery's swatches): text, then accents 1–6. */
const THEME_COLOURS = ['#000000', '#4472C4', '#ED7D31', '#A5A5A5', '#FFC000', '#5B9BD5', '#70AD47'] as const;
const COLOUR_NAMES = [
  'Dark 1',
  'Accent 1',
  'Accent 2',
  'Accent 3',
  'Accent 4',
  'Accent 5',
  'Accent 6',
] as const;

/** Mixes a colour with white (`t` > 0) or black (`t` < 0) for a swatch. */
function tint(hex: string, t: number): string {
  const n = Number.parseInt(hex.slice(1), 16);
  const ch = (s: number) => {
    const v = (n >> s) & 0xff;
    const out = t >= 0 ? v + (255 - v) * t : v * (1 + t);
    return Math.round(out).toString(16).padStart(2, '0');
  };
  return `#${ch(16)}${ch(8)}${ch(0)}`.toUpperCase();
}

const LEVELS: readonly [ShapeStyleLevel, string][] = [
  ['Outline', 'Colored Outline'],
  ['Fill', 'Colored Fill'],
  ['LightOutline', 'Light 1 Outline, Colored Fill'],
  ['Subtle', 'Subtle Effect'],
  ['Moderate', 'Moderate Effect'],
  ['Intense', 'Intense Effect'],
];

/** Shape Format ▸ Shape Styles: six rows of the seven theme colours, as in Word. */
export const SHAPE_STYLE_GALLERY: readonly RibbonGalleryEntry[] = LEVELS.flatMap(([level, name]) =>
  THEME_COLOURS.map((hex, i) => {
    const fill =
      level === 'Outline'
        ? '#FFFFFF'
        : level === 'Subtle'
          ? tint(hex, 0.8)
          : level === 'Moderate'
            ? tint(hex, 0.3)
            : hex;
    const line = level === 'Outline' || level === 'Subtle' ? hex : level === 'Fill' ? tint(hex, -0.5) : fill;
    return {
      id: `${level}:${i}`,
      label: `${name} – ${COLOUR_NAMES[i]}`,
      look: { swatch: [fill, line] },
    };
  }),
);

/** Shape Effects ▾. */
export const SHAPE_EFFECTS_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.shape.effect', { label: 'No Effect', args: { effect: 'None' } }),
  mitem('write.shape.effect', {
    label: 'Shadow',
    description: 'Outer, offset bottom right',
    args: { effect: 'Shadow' },
  }),
  mitem('write.shape.effect', {
    label: 'Glow',
    description: 'In the accent colour',
    args: { effect: 'Glow' },
  }),
  mitem('write.shape.effect', { label: 'Soft Edges', args: { effect: 'SoftEdges' } }),
];

/** Shape Fill ▸ Gradient: variations of the shape's colour. */
export const GRADIENT_MENU: readonly RibbonMenuEntry[] = [
  mhead('Light Variations'),
  mitem('ui.write.gradient', { label: 'Linear Down', args: { variant: 'light', angle: 90 } }),
  mitem('ui.write.gradient', { label: 'Linear Right', args: { variant: 'light', angle: 0 } }),
  mhead('Dark Variations'),
  mitem('ui.write.gradient', { label: 'Linear Down', args: { variant: 'dark', angle: 90 } }),
  mitem('ui.write.gradient', { label: 'Linear Diagonal', args: { variant: 'dark', angle: 45 } }),
  msep(),
  mitem('write.shape.gradient', { label: 'No Gradient', args: { gradient: null } }),
];

/** Text Direction ▾. */
export const TEXT_DIRECTION_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.shape.text_direction', { label: 'Horizontal', args: { direction: 'Horizontal' } }),
  mitem('write.shape.text_direction', { label: 'Rotate All Text 90°', args: { direction: 'Rotate90' } }),
  mitem('write.shape.text_direction', { label: 'Rotate All Text 270°', args: { direction: 'Rotate270' } }),
];

/** Group ▾. */
export const GROUP_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.arrange.group', { label: 'Group', args: { ungroup: false } }),
  mitem('write.arrange.group', { label: 'Ungroup', args: { ungroup: true } }),
];

/** Crop ▸ Crop to Shape. */
export const CROP_TO_SHAPE_MENU: RibbonMenuEntry = msub(
  'Crop to Shape',
  (
    [
      ['ellipse', 'Oval'],
      ['roundRect', 'Rounded Rectangle'],
      ['triangle', 'Triangle'],
      ['diamond', 'Diamond'],
      ['hexagon', 'Hexagon'],
      ['star5', '5-Point Star'],
      ['heart', 'Heart'],
      ['cloud', 'Cloud'],
      ['wedgeRoundRectCallout', 'Speech Bubble'],
      ['rect', 'Rectangle (no shape)'],
    ] as const
  ).map(([preset, label]) => mitem('write.picture.crop_to_shape', { label, args: { preset } })),
);

/** Corrections ▾: Word's brightness and contrast steps, and the options dialog. */
export const CORRECTIONS_MENU: readonly RibbonMenuEntry[] = [
  mhead('Brightness'),
  ...[-40, -20, 20, 40].map((b) =>
    mitem('write.picture.corrections', {
      label: `Brightness: ${b > 0 ? '+' : '−'}${Math.abs(b)}%`,
      args: { brightness: b, contrast: 0 },
    }),
  ),
  mhead('Contrast'),
  ...[-40, -20, 20, 40].map((c) =>
    mitem('write.picture.corrections', {
      label: `Contrast: ${c > 0 ? '+' : '−'}${Math.abs(c)}%`,
      args: { brightness: 0, contrast: c },
    }),
  ),
  msep(),
  mitem('write.picture.corrections', { label: 'Reset Corrections', args: { brightness: 0, contrast: 0 } }),
  mitem('ui.write.corrections_dialog', { label: 'Picture Corrections Options…' }),
];

/** Show Markup ▸ Balloons. */
export const BALLOONS_MENU: RibbonMenuEntry = msub('Balloons', [
  mitem('write.review.balloons', { label: 'Show Revisions in Balloons', args: { mode: 'Revisions' } }),
  mitem('write.review.balloons', { label: 'Show All Revisions Inline', args: { mode: 'Inline' } }),
  mitem('write.review.balloons', {
    label: 'Show Only Comments and Formatting in Balloons',
    args: { mode: 'CommentsAndFormatting' },
  }),
]);

/** Review ▸ Language ▾. */
export const LANGUAGE_MENU: readonly RibbonMenuEntry[] = [
  mitem('ui.write.language_dialog', { label: 'Set Proofing Language…' }),
  mitem('ui.options.open', { label: 'Language Preferences…', args: { page: 'language' } }),
];

/** Align ▾ additions: distribute, and lining up the selected objects with each other. */
export const DISTRIBUTE_ENTRIES: readonly RibbonMenuEntry[] = [
  mitem('ui.write.align', { label: 'Distribute Horizontally', args: { align: 'DistributeHorizontally' } }),
  mitem('ui.write.align', { label: 'Distribute Vertically', args: { align: 'DistributeVertically' } }),
];

/**
 * Ribbon and keymap ids of Kiln Write that its editor implements in the chrome (they open a
 * dialog, a pane or a menu, or run core commands with arguments the chrome computes): the editor
 * registers each of them while a document is open (`@kiln/editor-write`'s tests assert it), so
 * none of them is a reservation in Write.
 */
export const WRITE_CHROME_COMMANDS: readonly string[] = Object.freeze([
  // Clipboard, Font, Paragraph, Editing.
  'suite.edit.find',
  'suite.edit.replace',
  'suite.edit.go_to',
  'suite.edit.select',
  'suite.edit.paste_special',
  'suite.edit.format_painter',
  'suite.format.font_dialog',
  'suite.format.paragraph_dialog',
  'suite.format.copy_formatting',
  'suite.format.paste_formatting',
  'write.styles.pane',
  'write.paragraph.sort',
  // Insert.
  'suite.insert.table',
  'suite.insert.link',
  'suite.insert.symbol',
  'suite.insert.date_time',
  'suite.insert.text_box',
  'suite.insert.shapes',
  'suite.insert.icons',
  'suite.insert.code_block',
  'suite.insert.equation',
  'suite.insert.screenshot',
  'write.insert.quick_parts',
  'write.insert.paste_markdown_table',
  // Layout.
  'suite.layout.page_setup',
  'suite.layout.breaks',
  'suite.arrange.position',
  'suite.arrange.wrap_text',
  'suite.arrange.bring_forward',
  'suite.arrange.send_backward',
  'suite.arrange.selection_pane',
  'suite.arrange.align',
  'suite.arrange.rotate',
  // Picture Format.
  'suite.picture.alt_text',
  'suite.picture.crop',
  'suite.picture.reset',
  // Review.
  'suite.review.new_comment',
  'suite.review.delete_comment',
  'suite.review.previous_comment',
  'suite.review.next_comment',
  'suite.review.show_comments',
  'suite.review.spelling',
  'suite.review.word_count',
  'write.review.reviewing_pane',
  // View.
  'suite.view.ruler',
  'suite.view.gridlines',
  'suite.view.navigation_pane',
  'suite.view.page_width',
  'suite.view.one_page',
  'suite.view.multiple_pages',
  'suite.view.vertical_pages',
  'suite.view.side_to_side',
  'suite.view.split',
  // Home ▸ Kiln.
  'suite.help.assistant',
]);

/** Ids the window frame implements for every app with a Print page (File ▸ Print, Export). */
export const FRAME_CHROME_COMMANDS: readonly string[] = Object.freeze([
  'suite.file.print',
  'suite.file.export_pdf',
]);

/**
 * the frame's Share dialog and Version History pane, for the apps whose documents can be
 * shared (Write, Deck and Notes; Grid's workbooks are not yet).
 */
export const COLLAB_CHROME_COMMANDS: readonly string[] = Object.freeze([
  'suite.file.share',
  'suite.file.version_history',
]);
