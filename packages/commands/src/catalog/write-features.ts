/**
 * Kiln Write's advanced features: the presentation of the core's comment, track changes,
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
  PictureCrop,
  PositionPreset,
  PropertiesEdit,
  Rect,
  RelativeTo,
  RevisionRef,
  RotateOp,
  TextAnchor,
  TextBoxDesign,
  TextPos,
  TocSpec,
  WrapMode,
  ZOrder,
} from '@kiln/protocol';
import {
  mhead,
  mitem,
  msep,
  msub,
  type RibbonMenuEntry,
  rbutton,
  rcolor,
  rdropdown,
  rgallery,
  rgroup,
  rnumber,
  rsplit,
  rtab,
  rtoggle,
} from '../ribbon.ts';
import type { CommandContext } from '../types.ts';
import { c, section, WRITE } from './define.ts';
import {
  BALLOONS_MENU,
  CORRECTIONS_MENU,
  CROP_TO_SHAPE_MENU,
  DISTRIBUTE_ENTRIES,
  GRADIENT_MENU,
  GROUP_MENU,
  SHAPE_EFFECTS_MENU,
  SHAPE_STYLE_GALLERY,
  TEXT_DIRECTION_MENU,
} from './write-office.ts';

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

/** A colour: `RRGGBB`, `#RRGGBB` or the colour picker's choice (`none`/`auto`: no colour). */
function colourArg(a: Args): string | null {
  const v = a.color;
  if (v === undefined || v === null) return null;
  if (isRecord(v) && (v.type === 'none' || v.type === 'auto')) return null;
  const raw = isRecord(v) && isString(v.hex) ? v.hex : v;
  if (!isString(raw) || !/^#?[0-9a-f]{6}$/i.test(raw)) throw new ArgsError('"color" must be RRGGBB');
  return raw.replace(/^#/, '').toUpperCase();
}

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

/** Every core command id and how its `Command` is built. */
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
  'write.review.show_reviewer': (a) =>
    w({
      ShowReviewer: {
        name: opt(a, 'name', isString, 'a reviewer'),
        show: req(a, 'show', isBool, 'true or false'),
      },
    }),
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
  'write.arrange.select': (a) =>
    w({
      SelectObject: {
        id: req(a, 'id', isUint, 'an object id'),
        // Shift or Ctrl+click adds the object to the selection.
        add: opt(a, 'add', isBool, 'true or false') ?? undefined,
      },
    }),
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
    w({
      ObjectAlign: {
        align: req(a, 'align', isEnum, 'an alignment') as ObjectAlign,
        relative_to: opt(a, 'relative_to', isEnum, 'Page or Margin') as RelativeTo | null,
      },
    }),
  'write.insert.text_box': (a) =>
    w({
      InsertTextBox: {
        text: opt(a, 'text', isString, 'text'),
        design: opt(a, 'design', isEnum, 'a text box design') as TextBoxDesign | null,
        rect: opt(a, 'rect', isRect, 'a rectangle'),
      },
    }),
  'write.insert.shape': (a) =>
    w({
      InsertShape: {
        preset: opt(a, 'preset', isString, 'a shape') ?? opt(a, 'value', isString, 'a shape') ?? 'rect',
        text: opt(a, 'text', isString, 'text'),
        rect: opt(a, 'rect', isRect, 'a rectangle'),
      },
    }),
  // Picture Format
  'write.picture.reset': (a) =>
    w({ ResetPicture: { size: opt(a, 'size', isBool, 'true or false') ?? false } }),
  'write.picture.change': (a) => {
    const bytes = a.bytes;
    if (!(bytes instanceof Uint8Array) && !(Array.isArray(bytes) && bytes.every(isUint)))
      throw new ArgsError('"bytes" must be the picture file');
    return w({
      ChangePicture: {
        name: req(a, 'name', isString, 'a file name'),
        bytes: Array.from(bytes as ArrayLike<number>),
      },
    });
  },
  'write.picture.crop': (a) => {
    const c = req(a, 'crop', isRecord, 'a crop');
    const side = (k: string) => {
      const v = c[k] ?? 0;
      if (!isNumber(v)) throw new ArgsError(`"crop.${k}" must be a percentage`);
      return v;
    };
    const crop: PictureCrop = {
      left: side('left'),
      top: side('top'),
      right: side('right'),
      bottom: side('bottom'),
    };
    return w({ CropPicture: { crop } });
  },
  'write.shape.fill': (a) => w({ ShapeFill: { color: colourArg(a) } }),
  'write.shape.outline': (a) => w({ ShapeOutline: { color: colourArg(a) } }),
  'write.shape.outline_weight': (a) =>
    w({ ShapeOutlineWeight: { points: req(a, 'points', isNumber, 'a weight in points') } }),
  'write.shape.text_layout': (a) => {
    const m = opt(a, 'margins', isRecord, 'margins');
    const side = (k: string) => {
      const v = m?.[k] ?? 0;
      if (!isNumber(v)) throw new ArgsError(`"margins.${k}" must be points`);
      return v;
    };
    return w({
      ShapeTextLayout: {
        anchor: opt(a, 'anchor', isEnum, 'Top, Middle or Bottom') as TextAnchor | null,
        margins: m
          ? { left: side('left'), top: side('top'), right: side('right'), bottom: side('bottom') }
          : null,
      },
    });
  },
  'write.picture.crop_aspect': (a) =>
    w({
      CropToAspect: {
        width: req(a, 'width', isUint, 'a whole number'),
        height: req(a, 'height', isUint, 'a whole number'),
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

/** Presentation of the core commands the ribbon does not declare already. */
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
    c('ui.write.reviewers', 'Choose Reviewers…', 'ShowMarkup', {
      officeName: 'Specific People',
      description: 'Show or hide each reviewer’s changes and comments.',
      synonyms: ['reviewers', 'filter markup by author'],
    }),
    c('write.review.show_reviewer', 'Show Markup for a Reviewer', 'ShowMarkup', {
      officeName: 'Specific People',
      palette: false,
    }),
  ]),
  ...section('Picture Format', W, [
    c('write.picture.reset', 'Reset Picture', 'RotateCcw', {
      officeName: 'Reset Picture',
      description: 'Remove the cropping, rotation and flips; Reset Picture & Size also restores its size.',
    }),
    c('write.picture.change', 'Change Picture', 'Replace', { palette: false }),
    c('write.picture.crop', 'Crop Picture', 'Crop', { palette: false }),
    c('write.picture.crop_aspect', 'Crop to Aspect Ratio', 'Crop', {
      officeName: 'Aspect Ratio',
      palette: false,
    }),
    c('ui.write.change_picture', 'Change Picture…', 'Replace', {
      officeName: 'Change Picture',
      description: 'Put another picture in this one’s place, keeping its size, position and alt text.',
      synonyms: ['replace picture', 'swap image'],
    }),
    c('write.shape.fill', 'Shape Fill', 'PaintBucket', {
      officeName: 'Shape Fill',
      description: 'Fill the shape or text box with a colour, or with nothing.',
    }),
    c('write.shape.outline', 'Shape Outline', 'PenLine', {
      officeName: 'Shape Outline',
      description: 'The colour of the shape’s outline, or no outline.',
    }),
    c('write.shape.outline_weight', 'Outline Weight', 'Minus', { officeName: 'Weight', palette: false }),
    c('write.shape.text_layout', 'Align Text', 'BetweenVerticalStart', {
      officeName: 'Align Text',
      description: 'Put the text at the top, middle or bottom of the shape.',
    }),
    c('ui.write.object_height', 'Height', 'MoveVertical', { officeName: 'Shape Height', palette: false }),
    c('ui.write.table_row_height', 'Height', 'MoveVertical', {
      officeName: 'Table Row Height',
      palette: false,
    }),
    c('ui.write.table_column_width', 'Width', 'MoveHorizontal', {
      officeName: 'Table Column Width',
      palette: false,
    }),
    c('ui.write.object_width', 'Width', 'MoveHorizontal', { officeName: 'Shape Width', palette: false }),
    c('ui.write.text_box_options', 'Text Box Options…', 'TextBox', {
      officeName: 'Format Shape ▸ Text Box',
      synonyms: ['internal margin', 'text inset', 'padding'],
    }),
    c('ui.write.crop_dialog', 'Crop…', 'Crop', {
      officeName: 'Crop',
      synonyms: ['trim picture', 'cut picture edges'],
    }),
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
    c('ui.write.align', 'Align Object', 'AlignObjects', { palette: false }),
    c('ui.write.align_to', 'Align to Page or Margin', 'AlignObjects', { palette: false }),
  ]),
  ...section('Insert', W, [
    c('write.insert.text_box', 'Insert Text Box', 'TextBox', { palette: false }),
    c('write.insert.shape', 'Insert Shape', 'Shapes', { palette: false }),
    c('ui.write.draw_text_box', 'Draw Text Box', 'TextBox', {
      officeName: 'Draw Text Box',
      description: 'Drag on the page to draw a text box.',
    }),
    c('ui.write.draw_shape', 'Draw Shape', 'Shapes', { palette: false }),
    c('ui.write.icons_dialog', 'Icons…', 'Smile', {
      officeName: 'Icons',
      description: 'Insert an icon from Kiln’s built-in set, in any colour.',
      synonyms: ['symbols', 'pictograms', 'glyphs'],
    }),
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
    c('ui.write.numbering_value_dialog', 'Set Numbering Value…', 'NumberedList', {
      officeName: 'Set Numbering Value',
    }),
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

/** Pressed states of the core toggles. */
export const WRITE_FEATURE_CHECKED: Readonly<Record<string, (ctx: CommandContext) => boolean | undefined>> = {
  'write.review.track_changes': (ctx) => features(ctx)?.tracking,
  'write.view.focus_mode': (ctx) => features(ctx)?.focus,
  'write.layout.hyphenation': (ctx) => features(ctx)?.hyphenation,
};

/** Values of the value commands (Display for Review shows the mode). */
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
  mhead('Specific People'),
  mitem('write.review.show_reviewer', { label: 'All Reviewers', args: { name: null, show: true } }),
  mitem('ui.write.reviewers', {
    label: 'Choose Reviewers…',
    description: 'Show or hide each reviewer’s changes and comments in the Reviewing pane',
  }),
  msep(),
  BALLOONS_MENU,
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
  mitem('ui.write.align', { label: 'Align Left', args: { align: 'Left' } }),
  mitem('ui.write.align', { label: 'Align Centre', args: { align: 'Center' } }),
  mitem('ui.write.align', { label: 'Align Right', args: { align: 'Right' } }),
  msep(),
  mitem('ui.write.align', { label: 'Align Top', args: { align: 'Top' } }),
  mitem('ui.write.align', { label: 'Align Middle', args: { align: 'Middle' } }),
  mitem('ui.write.align', { label: 'Align Bottom', args: { align: 'Bottom' } }),
  msep(),
  ...DISTRIBUTE_ENTRIES,
  msep(),
  mitem('ui.write.align_to', { label: 'Align to Page', args: { to: 'Page' } }),
  mitem('ui.write.align_to', { label: 'Align to Margin', args: { to: 'Margin' } }),
  mitem('ui.write.align_to', {
    label: 'Align Selected Objects',
    description: 'With several objects selected (Shift+click), line them up with each other',
    args: { to: 'Selected' },
  }),
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
/** Shapes ▾: picking one starts drawing it (drag on the page; a click places it at its usual
 * size). */
export const SHAPES_MENU: readonly RibbonMenuEntry[] = [
  mhead('Basic Shapes'),
  ...SHAPES.slice(0, 6).map(([preset, label]) => mitem('ui.write.draw_shape', { label, args: { preset } })),
  mhead('Arrows and Stars'),
  ...SHAPES.slice(6, 10).map(([preset, label]) => mitem('ui.write.draw_shape', { label, args: { preset } })),
  mhead('Callouts and More'),
  ...SHAPES.slice(10).map(([preset, label]) => mitem('ui.write.draw_shape', { label, args: { preset } })),
];

/** Text Box ▾: the built-in gallery and Draw Text Box. */
export const TEXT_BOX_MENU: readonly RibbonMenuEntry[] = [
  mhead('Built-In'),
  mitem('write.insert.text_box', {
    label: 'Simple Text Box',
    description: 'White with a thin outline',
    args: { design: 'Simple' },
  }),
  mitem('write.insert.text_box', {
    label: 'Sidebar',
    description: 'A tall tinted panel at the right margin',
    args: { design: 'Sidebar' },
  }),
  mitem('write.insert.text_box', {
    label: 'Pull Quote',
    description: 'Large centred text in an accent frame',
    args: { design: 'Quote' },
  }),
  msep(),
  mitem('ui.write.draw_text_box', { label: 'Draw Text Box' }),
];

/** Shape Outline ▸ Weight. */
export const OUTLINE_WEIGHT_MENU: readonly RibbonMenuEntry[] = [0.25, 0.5, 0.75, 1, 1.5, 2.25, 3, 4.5, 6].map(
  (points) => mitem('write.shape.outline_weight', { label: `${points} pt`, args: { points } }),
);

/** Align Text ▾ and the text box margins. */
export const ALIGN_TEXT_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.shape.text_layout', { label: 'Top', args: { anchor: 'Top' } }),
  mitem('write.shape.text_layout', { label: 'Middle', args: { anchor: 'Middle' } }),
  mitem('write.shape.text_layout', { label: 'Bottom', args: { anchor: 'Bottom' } }),
  msep(),
  mitem('ui.write.text_box_options', { label: 'Text Box Options…' }),
];

/** Reset Picture ▾. */
export const RESET_PICTURE_MENU: readonly RibbonMenuEntry[] = [
  mitem('write.picture.reset', { label: 'Reset Picture', args: { size: false } }),
  mitem('write.picture.reset', { label: 'Reset Picture & Size', args: { size: true } }),
];

/** Crop ▾: the Crop tab of the Layout dialog and Word's aspect ratios. */
export const CROP_MENU: readonly RibbonMenuEntry[] = [
  mitem('ui.write.crop_dialog', { label: 'Crop…' }),
  msub(
    'Aspect Ratio',
    (
      [
        [1, 1, 'Square'],
        [2, 3, 'Portrait'],
        [3, 4, 'Portrait'],
        [3, 5, 'Portrait'],
        [4, 5, 'Portrait'],
        [3, 2, 'Landscape'],
        [4, 3, 'Landscape'],
        [5, 3, 'Landscape'],
        [5, 4, 'Landscape'],
        [16, 9, 'Landscape'],
        [16, 10, 'Landscape'],
      ] as const
    ).map(([width, height, kind]) =>
      mitem('write.picture.crop_aspect', {
        label: `${width}:${height}`,
        description: kind,
        args: { width, height },
      }),
    ),
  ),
  CROP_TO_SHAPE_MENU,
  msep(),
  mitem('write.picture.reset', { label: 'Remove Cropping', args: { size: false } }),
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

/** Picture Format (contextual, while a picture is selected in Write). */
export const writePictureFormatTab = () =>
  rtab(
    'pictureFormat',
    'Picture Format',
    [
      rgroup('adjust', 'Adjust', [
        rdropdown(undefined, CORRECTIONS_MENU, {
          id: 'picture.corrections',
          label: 'Corrections',
          icon: 'Contrast',
          size: 'large',
          keyTip: 'R',
        }),
        rbutton('ui.write.change_picture', { size: 'medium', label: 'Change Picture' }),
        rsplit('write.picture.reset', RESET_PICTURE_MENU, { size: 'medium', keyTip: 'Q' }),
      ]),
      rgroup('accessibility', 'Accessibility', [
        rbutton('suite.picture.alt_text', { size: 'large', keyTip: 'AT' }),
      ]),
      rgroup('arrange', 'Arrange', [
        rdropdown('suite.arrange.position', POSITION_MENU, { size: 'medium', keyTip: 'PO' }),
        rdropdown('suite.arrange.wrap_text', WRAP_MENU, { size: 'medium', keyTip: 'TW' }),
        rsplit('suite.arrange.bring_forward', FORWARD_MENU, { size: 'medium', keyTip: 'AF' }),
        rsplit('suite.arrange.send_backward', BACKWARD_MENU, { size: 'medium', keyTip: 'AE' }),
        rtoggle('suite.arrange.selection_pane', { size: 'small', keyTip: 'AP' }),
        rdropdown('suite.arrange.align', ALIGN_MENU, { size: 'small', keyTip: 'AA' }),
        rdropdown(undefined, GROUP_MENU, {
          id: 'picture.group',
          label: 'Group',
          icon: 'GroupObjects',
          size: 'small',
          keyTip: 'G',
        }),
        rdropdown('suite.arrange.rotate', ROTATE_MENU, { size: 'small', keyTip: 'AY' }),
      ]),
      rgroup(
        'size',
        'Size',
        [
          rsplit('ui.write.crop_dialog', CROP_MENU, { size: 'large', keyTip: 'C', label: 'Crop' }),
          rnumber('ui.write.object_height', {
            unit: 'pt',
            size: 'medium',
            min: 1,
            max: 1584,
            step: 1,
            keyTip: 'H',
          }),
          rnumber('ui.write.object_width', {
            unit: 'pt',
            size: 'medium',
            min: 1,
            max: 1584,
            step: 1,
            keyTip: 'W',
          }),
        ],
        {
          launcher: { command: 'ui.write.layout_dialog', label: 'Size' },
        },
      ),
    ],
    { keyTip: 'JP', contextual: { set: 'pictureTools', label: 'Picture Tools', context: 'picture' } },
  );

/** Shape Format (contextual, while a shape or text box is selected in Write). */
export const writeShapeFormatTab = () =>
  rtab(
    'shapeFormat',
    'Shape Format',
    [
      rgroup('insertShapes', 'Insert Shapes', [
        rdropdown('suite.insert.shapes', SHAPES_MENU, { size: 'large', keyTip: 'SH' }),
        rbutton('ui.write.draw_text_box', { size: 'medium', keyTip: 'X' }),
      ]),
      rgroup('shapeStyles', 'Shape Styles', [
        rgallery('write.shape.style', SHAPE_STYLE_GALLERY, {
          columns: 7,
          itemWidth: 34,
          itemHeight: 26,
          keyTip: 'K',
        }),
        rcolor('write.shape.fill', {
          allowNone: true,
          defaultColor: '#C2481C',
          size: 'medium',
          keyTip: 'SI',
        }),
        rcolor('write.shape.outline', {
          allowNone: true,
          defaultColor: '#2B1C14',
          size: 'medium',
          keyTip: 'SO',
        }),
        rdropdown('write.shape.outline_weight', OUTLINE_WEIGHT_MENU, { size: 'medium', label: 'Weight' }),
        rdropdown(undefined, GRADIENT_MENU, {
          id: 'shape.gradient',
          label: 'Gradient',
          icon: 'ShapeFill',
          size: 'medium',
        }),
        rdropdown(undefined, SHAPE_EFFECTS_MENU, {
          id: 'shape.effects',
          label: 'Shape Effects',
          icon: 'ShapeEffects',
          size: 'medium',
          keyTip: 'SE',
        }),
      ]),
      rgroup('text', 'Text', [
        rdropdown(undefined, TEXT_DIRECTION_MENU, {
          id: 'shape.textDirection',
          label: 'Text Direction',
          icon: 'TextDirection',
          size: 'medium',
          keyTip: 'TD',
        }),
        rdropdown('write.shape.text_layout', ALIGN_TEXT_MENU, { size: 'medium', keyTip: 'AT' }),
      ]),
      rgroup('arrange', 'Arrange', [
        rdropdown('suite.arrange.position', POSITION_MENU, { size: 'medium', keyTip: 'PO' }),
        rdropdown('suite.arrange.wrap_text', WRAP_MENU, { size: 'medium', keyTip: 'TW' }),
        rsplit('suite.arrange.bring_forward', FORWARD_MENU, { size: 'medium', keyTip: 'AF' }),
        rsplit('suite.arrange.send_backward', BACKWARD_MENU, { size: 'medium', keyTip: 'AE' }),
        rtoggle('suite.arrange.selection_pane', { size: 'small', keyTip: 'AP' }),
        rdropdown('suite.arrange.align', ALIGN_MENU, { size: 'small', keyTip: 'AA' }),
        rdropdown(undefined, GROUP_MENU, {
          id: 'shape.group',
          label: 'Group',
          icon: 'GroupObjects',
          size: 'small',
          keyTip: 'G',
        }),
        rdropdown('suite.arrange.rotate', ROTATE_MENU, { size: 'small', keyTip: 'AY' }),
      ]),
      rgroup(
        'size',
        'Size',
        [
          rnumber('ui.write.object_height', {
            unit: 'pt',
            size: 'medium',
            min: 1,
            max: 1584,
            step: 1,
            keyTip: 'H',
          }),
          rnumber('ui.write.object_width', {
            unit: 'pt',
            size: 'medium',
            min: 1,
            max: 1584,
            step: 1,
            keyTip: 'W',
          }),
        ],
        { launcher: { command: 'ui.write.layout_dialog', label: 'Size' } },
      ),
    ],
    { keyTip: 'JD', contextual: { set: 'drawingTools', label: 'Drawing Tools', context: 'shape' } },
  );
