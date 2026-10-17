/**
 * Kiln Write's advanced features (WP-66): the presentation of the core's comment, track changes,
 * reference, object, Format Painter, AutoFormat, block and spelling commands, how the UI builds
 * each one's typed `Command` from its arguments, the pressed states the ribbon reads from the
 * core's `FormatState.features`, and the menus of the Review, References, Insert and Layout tabs.
 */
import type {
  AutoFormatOptions,
  CalloutKind,
  CaptionSpec,
  ChangeScope,
  Command,
  CoverDesign,
  CrossRefSpec,
  FeatureState,
  MarkupKind,
  MarkupMode,
  NoteOptionsSpec,
  ObjectAlign,
  ObjectLayoutSpec,
  PositionPreset,
  PropertiesEdit,
  Rect,
  RevisionRef,
  RotateOp,
  TextPos,
  TocSpec,
  WrapMode,
  ZOrder,
} from '@kiln/protocol';
import { mhead, mitem, msep, type RibbonMenuEntry } from '../ribbon.ts';
import type { CommandContext } from '../types.ts';
import { c, section, WRITE } from './define.ts';

type Args = Readonly<Record<string, unknown>>;
type WriteCmd = Extract<Command, { Write: unknown }>['Write'];
type Builder = (args: Args) => Command;

/** The arguments do not have the shape a command needs (the registry reports it as failed). */
class ArgsError extends Error {}

const isString = (v: unknown): v is string => typeof v === 'string';
const isBool = (v: unknown): v is boolean => typeof v === 'boolean';
const isNumber = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const isUint = (v: unknown): v is number => isNumber(v) && Number.isInteger(v) && v >= 0;
const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);
const isTextPos = (v: unknown): v is TextPos => isRecord(v) && isUint(v.block) && isUint(v.offset);
const isRect = (v: unknown): v is Rect =>
  isRecord(v) && isNumber(v.x) && isNumber(v.y) && isNumber(v.width) && isNumber(v.height);
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
const unit =
  (name: string): Builder =>
  () =>
    w(name as WriteCmd);

/** The Table of Contents gallery's tables. */
export const TOC_AUTOMATIC_1: TocSpec = {
  levels: 3,
  hyperlinks: true,
  page_numbers: true,
  right_align: true,
  leader: 'Dot',
  title: 'Contents',
};
export const TOC_AUTOMATIC_2: TocSpec = { ...TOC_AUTOMATIC_1, title: 'Table of Contents' };

/** Every WP-66 core command id and how its `Command` is built. */
export const WRITE_FEATURE_BUILDERS: Readonly<Record<string, Builder>> = Object.freeze({
  // Comments
  'write.comment.new': (a) => w({ NewComment: { text: opt(a, 'text', isString, 'text') ?? '' } }),
  'write.comment.reply': (a) =>
    w({
      ReplyComment: {
        parent: req(a, 'parent', isUint, 'a comment id'),
        text: req(a, 'text', isString, 'text'),
      },
    }),
  'write.comment.edit': (a) =>
    w({ EditComment: { id: req(a, 'id', isUint, 'a comment id'), text: req(a, 'text', isString, 'text') } }),
  'write.comment.resolve': (a) =>
    w({
      ResolveComment: {
        id: req(a, 'id', isUint, 'a comment id'),
        resolved: opt(a, 'resolved', isBool, 'true or false') ?? true,
      },
    }),
  'write.comment.delete': (a) => w({ DeleteComment: { id: opt(a, 'id', isUint, 'a comment id') } }),
  'write.comment.delete_all': unit('DeleteAllComments'),
  'write.comment.select': (a) => w({ SelectComment: { id: req(a, 'id', isUint, 'a comment id') } }),
  // Track changes
  'write.review.track_changes': unit('TrackChanges'),
  'write.review.set_reviewer': (a) => w({ SetReviewer: { name: req(a, 'name', isString, 'a name') } }),
  'write.review.markup_mode': (a) =>
    w({ MarkupMode: { mode: req(a, 'mode', isEnum, 'a markup mode') as MarkupMode } }),
  'write.review.show_markup': (a) =>
    w({ ShowMarkup: { kind: req(a, 'kind', isEnum, 'a markup kind') as MarkupKind } }),
  'write.review.accept': (a) =>
    w({ AcceptChange: { scope: (opt(a, 'scope', isEnum, 'a scope') ?? 'CurrentAndNext') as ChangeScope } }),
  'write.review.reject': (a) =>
    w({ RejectChange: { scope: (opt(a, 'scope', isEnum, 'a scope') ?? 'CurrentAndNext') as ChangeScope } }),
  'write.review.resolve': (a) =>
    w({
      ResolveRevision: {
        target: req(a, 'target', isRecord, 'a change') as unknown as RevisionRef,
        accept: req(a, 'accept', isBool, 'true or false'),
      },
    }),
  'write.review.previous_change': unit('PreviousChange'),
  'write.review.next_change': unit('NextChange'),
  // References
  'write.references.table_of_contents': (a) =>
    w({
      InsertToc: {
        spec: (opt(a, 'spec', isRecord, 'a table of contents') ?? TOC_AUTOMATIC_1) as unknown as TocSpec,
      },
    }),
  'write.insert.table_of_contents_quick': unit('InsertQuickToc'),
  'write.references.remove_toc': unit('RemoveToc'),
  'write.references.update_table': (a) =>
    w({ UpdateToc: { entire: opt(a, 'entire', isBool, 'true or false') ?? false } }),
  'write.references.add_text': (a) => w({ AddText: { level: req(a, 'level', isUint, 'a level') } }),
  'write.references.table_of_figures': (a) =>
    w({ InsertTableOfFigures: { label: opt(a, 'label', isString, 'a caption label') ?? 'Figure' } }),
  'write.references.insert_footnote': unit('InsertFootnote'),
  'write.references.insert_endnote': unit('InsertEndnote'),
  'write.references.next_footnote': (a) =>
    w({ NextNote: { backward: opt(a, 'backward', isBool, 'true or false') ?? false } }),
  'write.references.show_notes': unit('ShowNotes'),
  'write.references.note_options': (a) =>
    w({ NoteOptions: { spec: req(a, 'spec', isRecord, 'note options') as unknown as NoteOptionsSpec } }),
  'write.references.convert_notes': (a) =>
    w({ ConvertNotes: { to_endnotes: req(a, 'to_endnotes', isBool, 'true or false') } }),
  'write.references.insert_caption': (a) =>
    w({ InsertCaption: { spec: req(a, 'spec', isRecord, 'a caption') as unknown as CaptionSpec } }),
  'write.insert.cross_reference': (a) =>
    w({
      InsertCrossReference: {
        spec: req(a, 'spec', isRecord, 'a cross-reference') as unknown as CrossRefSpec,
      },
    }),
  'write.insert.follow_link': unit('FollowLink'),
  // Objects
  'write.arrange.select': (a) => w({ SelectObject: { id: req(a, 'id', isUint, 'an object id') } }),
  'write.arrange.position': (a) =>
    w({ ObjectPosition: { preset: req(a, 'preset', isEnum, 'a position') as PositionPreset } }),
  'write.arrange.wrap_text': (a) =>
    w({ ObjectWrap: { wrap: req(a, 'wrap', isEnum, 'a wrapping') as WrapMode } }),
  'write.arrange.transform': (a) =>
    w({
      ObjectTransform: {
        id: req(a, 'id', isUint, 'an object id'),
        rect: req(a, 'rect', isRect, 'a rectangle'),
      },
    }),
  'write.arrange.layout': (a) =>
    w({
      ObjectLayout: {
        id: opt(a, 'id', isUint, 'an object id'),
        spec: req(a, 'spec', isRecord, 'layout options') as unknown as ObjectLayoutSpec,
      },
    }),
  'write.arrange.order': (a) => w({ ObjectOrder: { order: req(a, 'order', isEnum, 'an order') as ZOrder } }),
  'write.arrange.rotate': (a) => w({ ObjectRotate: { op: req(a, 'op', isEnum, 'a rotation') as RotateOp } }),
  'write.arrange.align': (a) =>
    w({ ObjectAlign: { align: req(a, 'align', isEnum, 'an alignment') as ObjectAlign } }),
  'write.insert.text_box': (a) => w({ InsertTextBox: { text: opt(a, 'text', isString, 'text') } }),
  'write.insert.shape': (a) =>
    w({
      InsertShape: {
        preset: opt(a, 'preset', isString, 'a shape') ?? opt(a, 'value', isString, 'a shape') ?? 'rect',
        text: opt(a, 'text', isString, 'text'),
      },
    }),
  // Format Painter
  'write.format.painter': (a) =>
    w({ FormatPainter: { sticky: opt(a, 'sticky', isBool, 'true or false') ?? false } }),
  'write.format.copy_format': unit('CopyFormat'),
  'write.format.paste_format': unit('PasteFormat'),
  'write.format.cancel_painter': unit('CancelPainter'),
  // Editing options and blocks
  'write.options.autoformat': (a) =>
    w({
      SetAutoFormat: {
        options: req(a, 'options', isRecord, 'AutoFormat options') as unknown as AutoFormatOptions,
      },
    }),
  'write.insert.paste_markdown': (a) =>
    w({ PasteMarkdown: { text: req(a, 'text', isString, 'Markdown text') } }),
  'write.view.focus_mode': unit('FocusMode'),
  'write.view.word_goal': (a) => w({ WordGoal: { words: req(a, 'words', isUint, 'a number of words') } }),
  'write.layout.hyphenation': unit('Hyphenation'),
  'write.insert.code_block': (a) =>
    w({ InsertCodeBlock: { language: opt(a, 'language', isString, 'a language') } }),
  'write.insert.callout': (a) =>
    w({ InsertCallout: { kind: (opt(a, 'kind', isEnum, 'a callout kind') ?? 'Note') as CalloutKind } }),
  'write.insert.cover_page': (a) =>
    w({
      InsertCoverPage: {
        design: (opt(a, 'design', isEnum, 'a design') ??
          opt(a, 'value', isEnum, 'a design') ??
          'Kiln') as CoverDesign,
      },
    }),
  'write.file.properties': (a) =>
    w({ SetProperties: { props: req(a, 'props', isRecord, 'properties') as unknown as PropertiesEdit } }),
  // Spelling
  'write.spell.check_as_you_type': (a) => w({ SpellCheck: { on: req(a, 'on', isBool, 'true or false') } }),
  'write.spell.user_dictionary': (a) => {
    const words = a.words;
    if (!Array.isArray(words) || !words.every(isString))
      throw new ArgsError('"words" must be a list of words');
    return w({ SetUserDictionary: { words: words as string[] } });
  },
  'write.spell.add_word': (a) => w({ SpellAddWord: { word: req(a, 'word', isString, 'a word') } }),
  'write.spell.ignore_all': (a) => w({ SpellIgnoreAll: { word: req(a, 'word', isString, 'a word') } }),
  'write.spell.ignore': (a) => w({ SpellIgnore: { pos: req(a, 'pos', isTextPos, 'a position') } }),
  'write.spell.replace': (a) =>
    w({
      SpellReplace: {
        pos: req(a, 'pos', isTextPos, 'a position'),
        with: req(a, 'with', isString, 'a correction'),
        all: opt(a, 'all', isBool, 'true or false') ?? false,
      },
    }),
  'write.spell.next': unit('SpellNext'),
  'write.spell.load_dictionary': (a) =>
    w({
      SpellDictionary: {
        aff: req(a, 'aff', isString, 'an affix file'),
        dic: req(a, 'dic', isString, 'a word list'),
      },
    }),
});

const W = { apps: WRITE };

/** Presentation of the WP-66 core commands the ribbon does not declare already. */
export const writeFeatureCommands = [
  ...section('Review ▸ Comments', W, [
    c('write.comment.new', 'Add Comment', 'NewComment', { palette: false }),
    c('write.comment.reply', 'Reply to Comment', 'NewComment', { palette: false }),
    c('write.comment.edit', 'Edit Comment', 'NewComment', { palette: false }),
    c('write.comment.resolve', 'Resolve Comment', 'ShowComments', { palette: false }),
    c('write.comment.delete', 'Delete Comment Here', 'DeleteComment', { palette: false }),
    c('write.comment.delete_all', 'Delete All Comments in Document', 'DeleteComment', {
      officeName: 'Delete All Comments in Document',
      synonyms: ['remove comments', 'clear comments'],
    }),
    c('write.comment.select', 'Go to Comment', 'NextComment', { palette: false }),
    c('ui.write.comments_pane', 'Comments Pane', 'ShowComments', {
      description: 'Every comment thread in one list, with replies.',
      synonyms: ['comments list'],
    }),
  ]),
  ...section('Review ▸ Tracking', W, [
    c('write.review.set_reviewer', 'Reviewer Name', 'TrackChanges', { palette: false }),
    c('write.review.markup_mode', 'Display for Review', 'ShowMarkup', {
      officeName: 'Display for Review',
      synonyms: ['simple markup', 'all markup', 'no markup', 'original'],
    }),
    c('write.review.resolve', 'Accept or Reject This Change', 'AcceptChange', { palette: false }),
  ]),
  ...section('References', W, [
    c('write.references.remove_toc', 'Remove Table of Contents', 'TableOfContents', {
      officeName: 'Remove Table of Contents',
    }),
    c('write.references.note_options', 'Footnote and Endnote Options', 'InsertFootnote', { palette: false }),
    c('write.references.convert_notes', 'Convert Notes', 'InsertEndnote', {
      description: 'Convert every footnote to an endnote, or back.',
    }),
    c('write.insert.follow_link', 'Follow Link', 'Link', {
      description: 'Go to the place a link in the document points to (Ctrl+click).',
      palette: false,
    }),
    c('ui.write.toc_dialog', 'Custom Table of Contents…', 'TableOfContents', {
      officeName: 'Custom Table of Contents',
    }),
    c('ui.write.caption_dialog', 'Insert Caption…', 'InsertCaption', { officeName: 'Insert Caption' }),
    c('ui.write.cross_reference_dialog', 'Cross-reference…', 'CrossReference', {
      officeName: 'Cross-reference',
    }),
    c('ui.write.footnote_dialog', 'Footnote and Endnote…', 'InsertFootnote', {
      officeName: 'Footnote and Endnote',
    }),
  ]),
  ...section('Layout ▸ Arrange', W, [
    c('write.arrange.select', 'Select Object', 'SelectObjects', { palette: false }),
    c('write.arrange.position', 'Position Object', 'Position', { palette: false }),
    c('write.arrange.wrap_text', 'Wrap Text Around Object', 'WrapText', { palette: false }),
    c('write.arrange.transform', 'Move or Resize Object', 'Position', { palette: false }),
    c('write.arrange.layout', 'Object Layout', 'Position', { palette: false }),
    c('write.arrange.order', 'Arrange Object', 'BringForward', { palette: false }),
    c('write.arrange.rotate', 'Rotate or Flip Object', 'RotateObject', { palette: false }),
    c('write.arrange.align', 'Align Object', 'AlignObjects', { palette: false }),
    c('ui.write.layout_dialog', 'More Layout Options…', 'Position', {
      officeName: 'More Layout Options',
      synonyms: ['picture position', 'text wrapping', 'layout dialog'],
    }),
  ]),
  ...section('Insert', W, [
    c('write.insert.text_box', 'Draw Text Box', 'TextBox', { palette: false }),
    c('write.insert.shape', 'Insert Shape', 'Shapes', { palette: false }),
    c('write.insert.code_block', 'Code Block', 'CodeBlock', { palette: false }),
    c('write.insert.paste_markdown', 'Paste as Markdown', 'Paste', {
      description: 'Paste Markdown as headings, lists, tables and code.',
      synonyms: ['markdown', 'paste markdown'],
    }),
    c('write.file.properties', 'Document Properties', 'Info', { palette: false }),
    c('ui.write.properties_dialog', 'Properties…', 'Info', {
      officeName: 'Advanced Properties',
      synonyms: ['title', 'author', 'keywords', 'document properties'],
    }),
  ]),
  ...section('Home ▸ Clipboard', W, [
    c('write.format.painter', 'Format Painter', 'FormatPainter', { palette: false }),
    c('write.format.copy_format', 'Copy Formatting', 'FormatPainter', { palette: false }),
    c('write.format.paste_format', 'Paste Formatting', 'PasteFormatting', { palette: false }),
    c('write.format.cancel_painter', 'Stop Format Painter', 'FormatPainter', { palette: false }),
  ]),
  ...section('Options', W, [
    c('write.options.autoformat', 'AutoFormat As You Type', 'Settings', { palette: false }),
    c('write.view.word_goal', 'Word Count Goal', 'WordCount', { palette: false }),
    c('ui.write.word_goal_dialog', 'Set Word Goal…', 'WordCount', {
      description: 'A target word count with progress in the status bar.',
      synonyms: ['word target', 'goal'],
    }),
  ]),
  ...section('Review ▸ Proofing', W, [
    c('write.spell.check_as_you_type', 'Check Spelling as You Type', 'Spelling', { palette: false }),
    c('write.spell.user_dictionary', 'User Dictionary', 'Spelling', { palette: false }),
    c('write.spell.add_word', 'Add to Dictionary', 'Spelling', { palette: false }),
    c('write.spell.ignore_all', 'Ignore All', 'Spelling', { palette: false }),
    c('write.spell.ignore', 'Ignore Once', 'Spelling', { palette: false }),
    c('write.spell.replace', 'Change Spelling', 'Spelling', { palette: false }),
    c('write.spell.next', 'Next Misspelling', 'Spelling', { palette: false }),
    c('write.spell.load_dictionary', 'Load Spelling Dictionary', 'Spelling', { palette: false }),
    c('ui.write.spell_add', 'Add to Dictionary', 'Spelling', { palette: false }),
  ]),
];

/** The document's review and feature state from the selection's formatting. */
function features(ctx: CommandContext): FeatureState | undefined {
  const f = ctx.textFormat as { features?: FeatureState } | undefined;
  return f?.features;
}

/** Pressed states of the WP-66 core toggles. */
export const WRITE_FEATURE_CHECKED: Readonly<Record<string, (ctx: CommandContext) => boolean | undefined>> = {
  'write.review.track_changes': (ctx) => features(ctx)?.tracking,
  'write.view.focus_mode': (ctx) => features(ctx)?.focus,
  'write.layout.hyphenation': (ctx) => features(ctx)?.hyphenation,
};

/** Values of the WP-66 value commands (Display for Review shows the mode). */
export const WRITE_FEATURE_VALUES: Readonly<Record<string, (ctx: CommandContext) => unknown>> = {
  'write.review.markup_mode': (ctx) => features(ctx)?.markup,
};

// ── Ribbon menus ─────────────────────────────────────────────────────────────────────────────

export const MARKUP_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.review.markup_mode', { label: 'Simple Markup', args: { mode: 'Simple' } }),
  mitem('write.review.markup_mode', { label: 'All Markup', args: { mode: 'All' } }),
  mitem('write.review.markup_mode', { label: 'No Markup', args: { mode: 'None' } }),
  mitem('write.review.markup_mode', { label: 'Original', args: { mode: 'Original' } }),
];

export const SHOW_MARKUP_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.review.show_markup', { label: 'Comments', args: { kind: 'Comments' } }),
  mitem('write.review.show_markup', {
    label: 'Insertions and Deletions',
    args: { kind: 'InsertionsDeletions' },
  }),
  mitem('write.review.show_markup', { label: 'Formatting', args: { kind: 'Formatting' } }),
];

const scoped = (command: string, verb: string): RibbonMenuEntry[] => [
  mitem(command, { label: `${verb} and Move to Next`, args: { scope: 'CurrentAndNext' } }),
  mitem(command, { label: `${verb} This Change`, args: { scope: 'Current' } }),
  mitem(command, { label: `${verb} All Changes Shown`, args: { scope: 'AllShown' } }),
  mitem(command, { label: `${verb} All Changes`, args: { scope: 'All' } }),
  mitem(command, { label: `${verb} All Changes and Stop Tracking`, args: { scope: 'AllAndStop' } }),
];
export const ACCEPT_MENU = scoped('write.review.accept', 'Accept');
export const REJECT_MENU = scoped('write.review.reject', 'Reject');

export const TOC_MENU: readonly RibbonMenuEntry[] = [
  mhead('Built-In'),
  mitem('write.references.table_of_contents', {
    label: 'Automatic Table 1',
    description: '“Contents”, three levels, dotted leaders, linked',
    args: { spec: TOC_AUTOMATIC_1 },
  }),
  mitem('write.references.table_of_contents', {
    label: 'Automatic Table 2',
    description: '“Table of Contents”, three levels, dotted leaders, linked',
    args: { spec: TOC_AUTOMATIC_2 },
  }),
  msep(),
  mitem('ui.write.toc_dialog', { label: 'Custom Table of Contents…' }),
  mitem('write.references.remove_toc', { label: 'Remove Table of Contents' }),
];

export const ADD_TEXT_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.references.add_text', { label: 'Do Not Show in Table of Contents', args: { level: 0 } }),
  mitem('write.references.add_text', { label: 'Level 1', args: { level: 1 } }),
  mitem('write.references.add_text', { label: 'Level 2', args: { level: 2 } }),
  mitem('write.references.add_text', { label: 'Level 3', args: { level: 3 } }),
];

export const UPDATE_TOC_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.references.update_table', { label: 'Update Page Numbers Only', args: { entire: false } }),
  mitem('write.references.update_table', { label: 'Update Entire Table', args: { entire: true } }),
];

export const NEXT_NOTE_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.references.next_footnote', { label: 'Next Footnote', args: { backward: false } }),
  mitem('write.references.next_footnote', { label: 'Previous Footnote', args: { backward: true } }),
];

const POSITIONS: readonly [PositionPreset, string][] = [
  ['TopLeft', 'Top Left'],
  ['TopCenter', 'Top Centre'],
  ['TopRight', 'Top Right'],
  ['MiddleLeft', 'Middle Left'],
  ['MiddleCenter', 'Middle Centre'],
  ['MiddleRight', 'Middle Right'],
  ['BottomLeft', 'Bottom Left'],
  ['BottomCenter', 'Bottom Centre'],
  ['BottomRight', 'Bottom Right'],
];
export const POSITION_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.arrange.position', { label: 'In Line with Text', args: { preset: 'InLine' } }),
  mhead('With Text Wrapping'),
  ...POSITIONS.map(([preset, label]) => mitem('write.arrange.position', { label, args: { preset } })),
  msep(),
  mitem('ui.write.layout_dialog', { label: 'More Layout Options…' }),
];

export const WRAP_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.arrange.wrap_text', { label: 'In Line with Text', args: { wrap: 'InLine' } }),
  mitem('write.arrange.wrap_text', { label: 'Square', args: { wrap: 'Square' } }),
  mitem('write.arrange.wrap_text', { label: 'Tight', args: { wrap: 'Tight' } }),
  mitem('write.arrange.wrap_text', { label: 'Through', args: { wrap: 'Through' } }),
  mitem('write.arrange.wrap_text', { label: 'Top and Bottom', args: { wrap: 'TopAndBottom' } }),
  mitem('write.arrange.wrap_text', { label: 'Behind Text', args: { wrap: 'Behind' } }),
  mitem('write.arrange.wrap_text', { label: 'In Front of Text', args: { wrap: 'InFront' } }),
  msep(),
  mitem('ui.write.layout_dialog', { label: 'More Layout Options…' }),
];

export const FORWARD_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.arrange.order', { label: 'Bring Forward', args: { order: 'Forward' } }),
  mitem('write.arrange.order', { label: 'Bring to Front', args: { order: 'ToFront' } }),
];
export const BACKWARD_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.arrange.order', { label: 'Send Backward', args: { order: 'Backward' } }),
  mitem('write.arrange.order', { label: 'Send to Back', args: { order: 'ToBack' } }),
];
export const ALIGN_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.arrange.align', { label: 'Align Left', args: { align: 'Left' } }),
  mitem('write.arrange.align', { label: 'Align Centre', args: { align: 'Center' } }),
  mitem('write.arrange.align', { label: 'Align Right', args: { align: 'Right' } }),
  msep(),
  mitem('write.arrange.align', { label: 'Align Top', args: { align: 'Top' } }),
  mitem('write.arrange.align', { label: 'Align Middle', args: { align: 'Middle' } }),
  mitem('write.arrange.align', { label: 'Align Bottom', args: { align: 'Bottom' } }),
];
export const ROTATE_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.arrange.rotate', { label: 'Rotate Right 90°', args: { op: 'Right90' } }),
  mitem('write.arrange.rotate', { label: 'Rotate Left 90°', args: { op: 'Left90' } }),
  mitem('write.arrange.rotate', { label: 'Flip Vertical', args: { op: 'FlipVertical' } }),
  mitem('write.arrange.rotate', { label: 'Flip Horizontal', args: { op: 'FlipHorizontal' } }),
  msep(),
  mitem('ui.write.layout_dialog', { label: 'More Rotation Options…' }),
];

const SHAPES: readonly [string, string][] = [
  ['rect', 'Rectangle'],
  ['roundRect', 'Rounded Rectangle'],
  ['ellipse', 'Oval'],
  ['triangle', 'Triangle'],
  ['diamond', 'Diamond'],
  ['hexagon', 'Hexagon'],
  ['rightArrow', 'Right Arrow'],
  ['leftRightArrow', 'Left-Right Arrow'],
  ['chevron', 'Chevron'],
  ['star5', '5-Point Star'],
  ['wedgeRoundRectCallout', 'Speech Bubble'],
  ['cloud', 'Cloud'],
  ['heart', 'Heart'],
  ['flowChartProcess', 'Flowchart: Process'],
  ['flowChartDecision', 'Flowchart: Decision'],
];
export const SHAPES_MENU: readonly RibbonMenuEntry[] = [
  mhead('Basic Shapes'),
  ...SHAPES.slice(0, 6).map(([preset, label]) => mitem('write.insert.shape', { label, args: { preset } })),
  mhead('Arrows and Stars'),
  ...SHAPES.slice(6, 10).map(([preset, label]) => mitem('write.insert.shape', { label, args: { preset } })),
  mhead('Callouts and More'),
  ...SHAPES.slice(10).map(([preset, label]) => mitem('write.insert.shape', { label, args: { preset } })),
];

export const CALLOUT_MENU: readonly RibbonMenuEntry[] = (
  [
    ['Note', 'Note'],
    ['Tip', 'Tip'],
    ['Warning', 'Warning'],
    ['Important', 'Important'],
  ] as const
).map(([kind, label]) => mitem('write.insert.callout', { label, args: { kind } }));

export const COVER_MENU: readonly RibbonMenuEntry[] = (
  [
    ['Kiln', 'Kiln', 'A large serif title on a calm page'],
    ['Banded', 'Banded', 'A band of colour across the top'],
    ['Ledger', 'Ledger', 'A rule under a left-aligned title'],
    ['Classic', 'Classic', 'Centred, between two rules'],
    ['Slab', 'Slab', 'A dark slab behind the title'],
    ['Quiet', 'Quiet', 'Title and date at the foot of the page'],
  ] as const
).map(([design, label, description]) =>
  mitem('write.insert.cover_page', { label, description, args: { design } }),
);

export const CODE_MENU: readonly RibbonMenuEntry[] = (
  ['Plain', 'Rust', 'Python', 'JavaScript', 'TypeScript', 'SQL', 'Shell'] as const
).map((lang) =>
  mitem('write.insert.code_block', {
    label: lang,
    args: { language: lang === 'Plain' ? null : lang.toLowerCase() },
  }),
);
