/** Kiln Write commands and ribbon: PLAN.md §1.4 "Write" and the Write column of §1.7. */

import {
  mhead,
  mitem,
  msep,
  msub,
  type RibbonData,
  type RibbonDataTab,
  type RibbonGalleryEntry,
  type RibbonMenuEntry,
  rbutton,
  rcolor,
  rdropdown,
  rgallery,
  rgroup,
  rnumber,
  rrow,
  rsplit,
  rtab,
  rtoggle,
} from '../ribbon.ts';
import type { CommandContext } from '../types.ts';
import { c, section, WRITE } from './define.ts';
import {
  clipboardGroup,
  fontGroup,
  macrosGroup,
  pictureFormatTab,
  shapeFormatTab,
  themeGroup,
  viewMode,
  zoomMenu,
} from './ribbon-shared.ts';
import {
  ACCEPT_MENU,
  ADD_TEXT_MENU,
  ALIGN_MENU,
  BACKWARD_MENU,
  CALLOUT_MENU,
  CODE_MENU,
  COVER_MENU,
  FORWARD_MENU,
  MARKUP_MENU,
  NEXT_NOTE_MENU,
  POSITION_MENU,
  REJECT_MENU,
  ROTATE_MENU,
  SHAPES_MENU,
  SHOW_MARKUP_MENU,
  TOC_MENU,
  UPDATE_TOC_MENU,
  WRAP_MENU,
  WRITE_FEATURE_CHECKED,
  WRITE_FEATURE_VALUES,
} from './write-features.ts';

const W = { apps: WRITE };

export const writeCommands = [
  ...section('Home ▸ Paragraph', W, [
    c('write.paragraph.sort', 'Sort', 'SortText', { keyTip: 'SO' }),
    c('write.paragraph.show_marks', 'Show/Hide ¶', 'ShowFormattingMarks', {
      keyTip: '8',
      officeName: 'Show/Hide ¶',
      synonyms: ['pilcrow', 'paragraph marks', 'formatting marks', 'hidden characters'],
    }),
    c('write.paragraph.shading', 'Shading', 'Shading', { keyTip: 'H' }),
    c('write.paragraph.borders', 'Borders', 'BordersAll', { keyTip: 'B' }),
    c('write.paragraph.move_paragraph_up', 'Move Paragraph Up', 'ArrowUpToLine'),
    c('write.paragraph.move_paragraph_down', 'Move Paragraph Down', 'ArrowDownToLine'),
    c('write.paragraph.promote', 'Promote', 'ArrowLeftToLine', {
      description: 'Raise the heading or list level.',
    }),
    c('write.paragraph.demote', 'Demote', 'ArrowRightToLine', {
      description: 'Lower the heading or list level.',
    }),
    c('write.paragraph.clear_paragraph_formatting', 'Clear Paragraph Formatting', 'ClearFormatting', {
      officeName: 'Reset Paragraph',
    }),
    c('write.paragraph.clear_character_formatting', 'Clear Character Formatting', 'ClearFormatting', {
      officeName: 'Reset Character Formatting',
    }),
  ]),
  ...section('Home ▸ Styles', W, [
    c('write.styles.apply', 'Styles', 'StylesPane', { keyTip: 'L' }),
    c('write.styles.pane', 'Styles Pane', 'StylesPane', { description: 'Show every style in a pane.' }),
    c('write.styles.normal', 'Normal', 'Pilcrow', { officeName: 'Normal Style' }),
    c('write.styles.heading_1', 'Heading 1', 'Heading1'),
    c('write.styles.heading_2', 'Heading 2', 'Heading2'),
    c('write.styles.heading_3', 'Heading 3', 'Heading3'),
    c('write.styles.title', 'Title', 'Heading'),
  ]),
  ...section('Home ▸ Focus', W, [
    c('write.view.focus_mode', 'Focus Mode', 'FocusMode', {
      description: 'Hide everything but the page.',
      synonyms: ['distraction free', 'zen'],
    }),
  ]),
  ...section('Insert ▸ Pages', W, [
    c('write.insert.cover_page', 'Cover Page', 'CoverPage'),
    c('write.insert.blank_page', 'Blank Page', 'BlankPage'),
    c('write.insert.page_break', 'Page Break', 'PageBreak'),
    c('write.insert.column_break', 'Column Break', 'Columns'),
  ]),
  ...section('Insert', W, [
    c('write.insert.convert_text_to_table', 'Convert Text to Table', 'ConvertTextToTable'),
    c('write.insert.paste_markdown_table', 'Paste Markdown Table', 'Table'),
    c('write.insert.bookmark', 'Bookmark', 'Bookmark'),
    c('write.insert.cross_reference', 'Cross-reference', 'CrossReference'),
    c('write.insert.header', 'Header', 'Header'),
    c('write.insert.footer', 'Footer', 'Footer'),
    c('write.insert.page_number', 'Page Number', 'PageNumber'),
    c('write.insert.quick_parts', 'Quick Parts', 'QuickParts', { synonyms: ['fields'] }),
    c('write.insert.drop_cap', 'Drop Cap', 'DropCap'),
    c('write.insert.table_of_contents_quick', 'Table of Contents', 'TableOfContents'),
    c('write.insert.callout', 'Callout', 'Callout', { description: 'A highlighted note block.' }),
    c('write.insert.update_fields', 'Update Fields', 'RefreshCw', { synonyms: ['update field'] }),
  ]),
  ...section('Layout', W, [
    c('suite.layout.margins', 'Margins', 'Margins', { apps: ['write', 'grid'] }),
    c('suite.layout.orientation', 'Orientation', 'Orientation', {
      apps: ['write', 'grid'],
      synonyms: ['landscape', 'portrait'],
    }),
    c('suite.layout.size', 'Size', 'PageSize', {
      apps: ['write', 'grid'],
      synonyms: ['paper size', 'a4', 'letter'],
    }),
    c('write.layout.columns', 'Columns', 'Columns'),
    c('suite.layout.breaks', 'Breaks', 'Breaks', { apps: ['write', 'grid'], synonyms: ['section break'] }),
    c('write.layout.line_numbers', 'Line Numbers', 'LineNumbers', { tier: 'P2' }),
    c('write.layout.hyphenation', 'Hyphenation', 'Hyphenation'),
    c('suite.layout.page_setup', 'Page Setup…', 'PageSize', {
      apps: ['write', 'grid'],
      description: 'Open the Page Setup dialog.',
    }),
    c('write.layout.indent_left', 'Indent Left', 'IndentLeft'),
    c('write.layout.indent_right', 'Indent Right', 'IndentRight'),
    c('write.layout.spacing_before', 'Spacing Before', 'SpacingBefore'),
    c('write.layout.spacing_after', 'Spacing After', 'SpacingAfter'),
    c('suite.arrange.position', 'Position', 'Position', { apps: ['write', 'grid', 'deck'] }),
    c('suite.arrange.wrap_text', 'Wrap Text', 'WrapSquare', { apps: ['write', 'grid', 'deck'] }),
    c('suite.arrange.bring_forward', 'Bring Forward', 'BringForward', { apps: ['write', 'grid', 'deck'] }),
    c('suite.arrange.send_backward', 'Send Backward', 'SendBackward', { apps: ['write', 'grid', 'deck'] }),
    c('suite.arrange.selection_pane', 'Selection Pane', 'SelectionPane', { apps: ['write', 'grid', 'deck'] }),
    c('suite.arrange.align', 'Align', 'AlignObjects', { apps: ['write', 'grid', 'deck'] }),
    c('suite.arrange.group', 'Group', 'GroupObjects', { apps: ['write', 'grid', 'deck'] }),
    c('suite.arrange.rotate', 'Rotate', 'RotateObject', { apps: ['write', 'grid', 'deck'] }),
  ]),
  ...section('References', W, [
    c('write.references.table_of_contents', 'Table of Contents', 'TableOfContents', { synonyms: ['toc'] }),
    c('write.references.add_text', 'Add Text', 'AddText'),
    c('write.references.update_table', 'Update Table', 'UpdateTable'),
    c('write.references.insert_footnote', 'Insert Footnote', 'InsertFootnote'),
    c('write.references.insert_endnote', 'Insert Endnote', 'InsertEndnote'),
    c('write.references.next_footnote', 'Next Footnote', 'NextFootnote'),
    c('write.references.show_notes', 'Show Notes', 'NotebookText'),
    c('write.references.insert_citation', 'Insert Citation', 'InsertCitation', { tier: 'P2' }),
    c('write.references.manage_sources', 'Manage Sources', 'ManageSources', { tier: 'P2' }),
    c('write.references.bibliography', 'Bibliography', 'Bibliography', { tier: 'P2' }),
    c('write.references.insert_caption', 'Insert Caption', 'InsertCaption'),
    c('write.references.table_of_figures', 'Insert Table of Figures', 'TableOfFigures', { tier: 'P1' }),
    c('write.references.mark_entry', 'Mark Entry', 'MarkEntry', { tier: 'P2', synonyms: ['index'] }),
  ]),
  ...section('Review', W, [
    c('write.review.track_changes', 'Track Changes', 'TrackChanges', { synonyms: ['revisions', 'redline'] }),
    c('write.review.show_markup', 'Show Markup', 'ShowMarkup'),
    c('write.review.reviewing_pane', 'Reviewing Pane', 'ReviewingPane'),
    c('write.review.accept', 'Accept', 'AcceptChange', { synonyms: ['accept change'] }),
    c('write.review.reject', 'Reject', 'RejectChange', { synonyms: ['reject change'] }),
    c('write.review.previous_change', 'Previous Change', 'PreviousChange'),
    c('write.review.next_change', 'Next Change', 'NextChange'),
    c('write.review.restrict_editing', 'Restrict Editing', 'RestrictEditing', { tier: 'P2' }),
  ]),
  ...section('View', W, [
    c('suite.view.immersive_reader', 'Immersive Reader', 'ImmersiveReader', { tier: 'P2' }),
    c('suite.view.vertical_pages', 'Vertical', 'VerticalPages'),
    c('suite.view.side_to_side', 'Side to Side', 'SideToSide'),
    c('suite.view.navigation_pane', 'Navigation Pane', 'NavigationPane'),
    c('suite.view.one_page', 'One Page', 'OnePage'),
    c('suite.view.multiple_pages', 'Multiple Pages', 'MultiplePages'),
    c('suite.view.page_width', 'Page Width', 'PageWidth'),
    c('suite.view.split', 'Split', 'SplitWindow', { apps: ['write', 'grid'] }),
    c('suite.view.side_by_side', 'View Side by Side', 'SideBySide', { apps: ['write', 'grid', 'deck'] }),
  ]),
];

/** Text editing the core implements (WP-62/WP-64): typing, deleting, caret motion and IME. */
export const writeCoreCommands = section('Editing', { apps: ['write', 'notes'] }, [
  c('write.edit.insert_text', 'Type Text', undefined, { palette: false }),
  c('write.edit.delete_backward', 'Delete Backward', undefined, { palette: false }),
  c('write.edit.delete_forward', 'Delete Forward', undefined, { palette: false }),
  c('write.edit.delete_word_backward', 'Delete Previous Word', undefined, { palette: false }),
  c('write.edit.delete_word_forward', 'Delete Next Word', undefined, { palette: false }),
  c('write.caret.move', 'Move the Insertion Point', undefined, { palette: false }),
  c('write.caret.select_to', 'Extend the Selection', undefined, { palette: false }),
  c('write.caret.select_all', 'Select All', 'TextSelect', {
    officeName: 'Select All',
    synonyms: ['select everything'],
    description: 'Select everything in the document.',
  }),
  c('write.caret.select_at', 'Select at a Point', undefined, { palette: false }),
  c('write.ime.set_composition', 'Compose Text', undefined, { palette: false }),
  c('write.ime.commit_composition', 'Commit Composition', undefined, { palette: false }),
  c('write.ime.cancel_composition', 'Cancel Composition', undefined, { palette: false }),
  // WP-64: the Write editor's own commands (the shared ones are declared in common.ts).
  c('ui.write.bookmark', 'Bookmark…', 'Bookmark', {
    officeName: 'Bookmark',
    description: 'Name a place in the document to link or go to.',
  }),
  c('suite.edit.paste_as', 'Paste As', 'PasteSpecial', { palette: false, apps: ['write', 'notes'] }),
  c('write.edit.tab', 'Tab', undefined, { palette: false }),
  c('write.edit.line_break', 'Line Break', 'CornerDownLeft', {
    description: 'Start a new line in the same paragraph (Shift+Enter).',
    synonyms: ['soft return', 'manual line break'],
  }),
  c('write.format.paragraph', 'Paragraph Settings', undefined, { palette: false }),
  c('write.format.font', 'Font Settings', undefined, { palette: false }),
  c('write.format.tabs', 'Tabs…', 'Ruler', { synonyms: ['tab stops', 'leaders'] }),
  c('write.format.small_caps', 'Small Caps', 'ChangeCase', { synonyms: ['small capitals'] }),
  c('write.format.underline_style', 'Underline Style', undefined, { palette: false }),
  c('write.styles.update_from_selection', 'Update Style to Match Selection', 'RefreshCw', {
    officeName: 'Update Style to Match Selection',
  }),
  c('write.styles.new_from_selection', 'Create a Style', 'StylesPane', {
    officeName: 'Create a Style',
    synonyms: ['new style'],
  }),
  c('write.styles.modify', 'Modify Style…', 'StylesPane', { officeName: 'Modify Style' }),
  c('write.list.set_level', 'Change List Level', 'MultilevelList', { palette: false }),
  c('write.list.restart', 'Restart at 1', 'NumberedList', {
    officeName: 'Restart at 1',
    synonyms: ['restart numbering'],
  }),
  c('write.list.continue', 'Continue Numbering', 'NumberedList', { officeName: 'Continue Numbering' }),
  c('write.list.set_value', 'Set Numbering Value…', 'NumberedList', { palette: false }),
  c('write.layout.section_break', 'Section Break', 'Breaks', {
    synonyms: ['next page', 'continuous', 'even page', 'odd page'],
  }),
  c('write.layout.page_setup', 'Page Setup Settings', undefined, { palette: false }),
  c('write.insert.link', 'Insert Hyperlink', undefined, { palette: false }),
  c('write.insert.remove_link', 'Remove Hyperlink', 'Eraser', { officeName: 'Remove Hyperlink' }),
  c('write.insert.date_time', 'Insert Date and Time', undefined, { palette: false }),
  c('write.insert.field', 'Insert Field', undefined, { palette: false }),
  c('write.insert.picture', 'Insert Picture', undefined, { palette: false }),
  c('write.table.insert', 'Insert Table', undefined, { palette: false }),
  c('write.table.autofit', 'AutoFit', 'Columns', {
    synonyms: ['autofit contents', 'autofit window', 'fixed width'],
  }),
  c('write.table.align', 'Table Alignment', 'AlignCenter', { palette: false }),
  c('write.table.split', 'Split Table', 'TableCellsSplit', { officeName: 'Split Table' }),
  c('write.table.select', 'Select in Table', 'TextSelect', { palette: false }),
  c('write.table.cell_size', 'Cell Size', 'Table', { palette: false }),
  c('write.find.next', 'Find Next', 'Search', { palette: false }),
  c('write.find.replace', 'Replace Next', 'Replace', { palette: false }),
  c('write.find.replace_all', 'Replace All', 'Replace', { palette: false }),
  c('write.find.go_to', 'Go to a Place', undefined, { palette: false }),
]);

// ── Ribbon (PLAN.md §1.4 "Write"; mockup docs/design/mockups/write-*.png) ─────────────────────────

/** Write's quick style set (Home ▸ Styles), previews as typography hints. */
export const WRITE_STYLES: readonly RibbonGalleryEntry[] = [
  { id: 'Normal', label: 'Normal', sample: 'Aa Bb', look: { font: 'ui', size: 17 } },
  { id: 'Title', label: 'Title', sample: 'Aa Bb', look: { font: 'display', size: 18, weight: 800 } },
  {
    id: 'Heading1',
    label: 'Heading 1',
    sample: 'Aa Bb',
    look: { font: 'display', size: 18, weight: 700, accent: true },
  },
  { id: 'Heading2', label: 'Heading 2', sample: 'Aa Bb', look: { font: 'display', size: 17, weight: 700 } },
  {
    id: 'Heading3',
    label: 'Heading 3',
    sample: 'Aa Bb',
    look: { font: 'ui', size: 16, weight: 700, accent: true },
  },
  {
    id: 'Quote',
    label: 'Quote',
    sample: '“Aa”',
    look: { font: 'display', size: 17, italic: true, muted: true },
  },
  { id: 'NoSpacing', label: 'No Spacing', sample: 'Aa Bb', look: { font: 'ui', size: 15 } },
  { id: 'Subtitle', label: 'Subtitle', sample: 'Aa Bb', look: { font: 'ui', size: 15, muted: true } },
  {
    id: 'SubtleEmphasis',
    label: 'Subtle Emphasis',
    sample: 'Aa Bb',
    look: { font: 'ui', size: 15, italic: true, muted: true },
  },
  { id: 'Emphasis', label: 'Emphasis', sample: 'Aa Bb', look: { font: 'ui', size: 15, italic: true } },
  {
    id: 'IntenseEmphasis',
    label: 'Intense Emphasis',
    sample: 'Aa Bb',
    look: { font: 'ui', size: 15, italic: true, accent: true },
  },
  { id: 'Strong', label: 'Strong', sample: 'Aa Bb', look: { font: 'ui', size: 15, weight: 700 } },
  {
    id: 'IntenseQuote',
    label: 'Intense Quote',
    sample: '“Aa”',
    look: { font: 'display', size: 16, italic: true, accent: true },
  },
  {
    id: 'SubtleReference',
    label: 'Subtle Reference',
    sample: 'AA BB',
    look: { font: 'ui', size: 13, caps: true, muted: true },
  },
  {
    id: 'IntenseReference',
    label: 'Intense Reference',
    sample: 'AA BB',
    look: { font: 'ui', size: 13, caps: true, weight: 700, accent: true },
  },
  {
    id: 'BookTitle',
    label: 'Book Title',
    sample: 'Aa Bb',
    look: { font: 'serif', size: 15, italic: true, weight: 700 },
  },
  { id: 'ListParagraph', label: 'List Paragraph', sample: '• Aa Bb', look: { font: 'ui', size: 15 } },
];

// ── Write's own groups (WP-64): menus with the arguments the core's commands take ──────────────

const PAGE_NUMBER_MENU: readonly RibbonMenuEntry[] = [
  mhead('Top of Page'),
  mitem('write.insert.page_number', { label: 'Top Left', args: { position: 'TopLeft' } }),
  mitem('write.insert.page_number', { label: 'Top Center', args: { position: 'TopCenter' } }),
  mitem('write.insert.page_number', { label: 'Top Right', args: { position: 'TopRight' } }),
  mhead('Bottom of Page'),
  mitem('write.insert.page_number', { label: 'Bottom Left', args: { position: 'BottomLeft' } }),
  mitem('write.insert.page_number', { label: 'Bottom Center', args: { position: 'BottomCenter' } }),
  mitem('write.insert.page_number', { label: 'Bottom Right', args: { position: 'BottomRight' } }),
  msep(),
  mitem('write.insert.page_number', { label: 'Current Position', args: { position: 'Caret' } }),
];

const BREAKS_MENU: readonly RibbonMenuEntry[] = [
  mhead('Page Breaks'),
  mitem('write.insert.page_break', { label: 'Page' }),
  mitem('write.insert.column_break', { label: 'Column' }),
  mitem('write.edit.line_break', { label: 'Text Wrapping' }),
  mhead('Section Breaks'),
  mitem('write.layout.section_break', { label: 'Next Page', args: { kind: 'NextPage' } }),
  mitem('write.layout.section_break', { label: 'Continuous', args: { kind: 'Continuous' } }),
  mitem('write.layout.section_break', { label: 'Even Page', args: { kind: 'EvenPage' } }),
  mitem('write.layout.section_break', { label: 'Odd Page', args: { kind: 'OddPage' } }),
];

const BULLETS: readonly [string, string][] = [
  ['•', 'Bullet'],
  ['○', 'Circle'],
  ['▪', 'Square'],
  ['❖', 'Diamonds'],
  ['➢', 'Arrow'],
  ['✓', 'Check mark'],
  ['–', 'Dash'],
];

const NUMBERING: readonly [string, string][] = [
  ['Decimal', '1. 2. 3.'],
  ['DecimalParen', '1) 2) 3)'],
  ['UpperRoman', 'I. II. III.'],
  ['UpperLetter', 'A. B. C.'],
  ['LowerLetterParen', 'a) b) c)'],
  ['LowerLetter', 'a. b. c.'],
  ['LowerRoman', 'i. ii. iii.'],
];

const BORDER_PRESETS: readonly [string, string, string][] = [
  ['Bottom', 'Bottom Border', 'BorderBottom'],
  ['Top', 'Top Border', 'BorderTop'],
  ['Left', 'Left Border', 'BorderLeft'],
  ['Right', 'Right Border', 'BorderRight'],
  ['None', 'No Border', 'BordersNone'],
  ['All', 'All Borders', 'BordersAll'],
  ['Outside', 'Outside Borders', 'BordersOutside'],
  ['Inside', 'Inside Borders', 'BordersInside'],
];

/** Home ▸ Paragraph for Write: the list galleries, Line Spacing, Shading and Borders menus. */
function writeParagraphGroup() {
  return rgroup(
    'paragraph',
    'Paragraph',
    [
      rrow(
        rsplit(
          'suite.format.bullets',
          [
            mhead('Bullet Library'),
            ...BULLETS.map(([glyph, label]) =>
              mitem('suite.format.bullets', { label: `${glyph}  ${label}`, args: { glyph } }),
            ),
          ],
          { size: 'small', keyTip: 'U', priority: 3 },
        ),
        rsplit(
          'suite.format.numbering',
          [
            mhead('Numbering Library'),
            ...NUMBERING.map(([style, label]) => mitem('suite.format.numbering', { label, args: { style } })),
            msep(),
            mitem('write.list.restart'),
            mitem('write.list.continue'),
          ],
          { size: 'small', keyTip: 'N', priority: 3 },
        ),
        rdropdown(
          'suite.format.multilevel_list',
          [
            mitem('suite.format.multilevel_list', { label: '1. 1.1. 1.1.1.', args: { legal: false } }),
            mitem('suite.format.multilevel_list', {
              label: 'Legal: 1.1.1 in every level',
              args: { legal: true },
            }),
            msep(),
            msub(
              'Change List Level',
              [0, 1, 2, 3, 4, 5, 6, 7, 8].map((level) =>
                mitem('write.list.set_level', { label: `Level ${level + 1}`, args: { level } }),
              ),
            ),
          ],
          { size: 'small', keyTip: 'M', hideInSimplified: true },
        ),
        rbutton('suite.format.outdent', { keyTip: 'AO' }),
        rbutton('suite.format.indent', { keyTip: 'AI' }),
        rbutton('write.paragraph.sort', { hideInSimplified: true }),
        rtoggle('write.paragraph.show_marks', { keyTip: '8' }),
      ),
      rrow(
        rtoggle('suite.format.align_left', { keyTip: 'AL', priority: 2 }),
        rtoggle('suite.format.align_center', { keyTip: 'AC', priority: 2 }),
        rtoggle('suite.format.align_right', { keyTip: 'AR' }),
        rtoggle('suite.format.justify', { keyTip: 'AJ' }),
        rdropdown(
          'suite.format.line_spacing',
          [
            ...[1, 1.15, 1.5, 2, 2.5, 3].map((n) =>
              mitem('suite.format.line_spacing', {
                label: n.toFixed(n % 1 ? 2 : 1).replace(/0$/, ''),
                args: { lineSpacing: n },
              }),
            ),
            msep(),
            mitem('suite.format.paragraph_dialog', { label: 'Line Spacing Options…' }),
            mitem('write.layout.spacing_before', { label: 'Add Space Before Paragraph', args: { pt: 12 } }),
            mitem('write.layout.spacing_after', { label: 'Remove Space After Paragraph', args: { pt: 0 } }),
          ],
          { size: 'small', keyTip: 'K' },
        ),
        rcolor('write.paragraph.shading', {
          allowNone: true,
          defaultColor: '#F2E6D8',
          keyTip: 'H',
          hideInSimplified: true,
        }),
        rdropdown(
          'write.paragraph.borders',
          [
            ...BORDER_PRESETS.map(([preset, label, icon]) =>
              mitem('write.paragraph.borders', { label, icon, args: { preset } }),
            ),
            msep(),
            mitem('suite.format.paragraph_dialog', { label: 'Borders and Shading…' }),
          ],
          { size: 'small', keyTip: 'B', hideInSimplified: true },
        ),
      ),
    ],
    { rows: 2, launcher: { command: 'suite.format.paragraph_dialog' }, icon: 'Pilcrow', priority: 3 },
  );
}

const TABLE_STYLES: readonly RibbonGalleryEntry[] = [
  { id: 'TableGrid', label: 'Table Grid', look: { swatch: ['#FFFFFF', '#2A211C'] } },
  { id: 'PlainTable1', label: 'Plain Table 1', look: { swatch: ['#F2F2F2', '#BFBFBF'] } },
  { id: 'GridTable4-Accent1', label: 'Grid Table 4', look: { swatch: ['#DCE6FB', '#3159C6'], accent: true } },
  { id: 'ListTable3-Accent1', label: 'List Table 3', look: { swatch: ['#FFFFFF', '#3159C6'] } },
];

const CELL_ALIGN: readonly [string, string, string][] = [
  ['Top', 'Left', 'Align Top Left'],
  ['Top', 'Center', 'Align Top Center'],
  ['Top', 'Right', 'Align Top Right'],
  ['Center', 'Left', 'Align Center Left'],
  ['Center', 'Center', 'Align Center'],
  ['Center', 'Right', 'Align Center Right'],
  ['Bottom', 'Left', 'Align Bottom Left'],
  ['Bottom', 'Center', 'Align Bottom Center'],
  ['Bottom', 'Right', 'Align Bottom Right'],
];

/** Table Design and Table Layout for Write (contextual, while the caret is in a table). */
function writeTableTabs(): RibbonDataTab[] {
  return [
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
          rcolor('suite.table.shading', { allowNone: true, defaultColor: '#DCE6FB', size: 'medium' }),
        ]),
        rgroup('borders', 'Borders', [
          rdropdown(
            'suite.table.borders',
            [
              ...BORDER_PRESETS.map(([preset, label, icon]) =>
                mitem('suite.table.borders', { label, icon, args: { preset } }),
              ),
              mitem('suite.table.borders', {
                label: 'Inside Horizontal Border',
                args: { preset: 'InsideHorizontal' },
              }),
              mitem('suite.table.borders', {
                label: 'Inside Vertical Border',
                args: { preset: 'InsideVertical' },
              }),
            ],
            { size: 'large' },
          ),
        ]),
      ],
      { keyTip: 'JT', contextual: { set: 'tableTools', label: 'Table Tools', context: 'table' } },
    ),
    rtab(
      'tableLayout',
      'Layout',
      [
        rgroup('table', 'Table', [
          rdropdown(
            'write.table.select',
            [
              mitem('write.table.select', { label: 'Select Cell', args: { what: 'Cell' } }),
              mitem('write.table.select', { label: 'Select Column', args: { what: 'Column' } }),
              mitem('write.table.select', { label: 'Select Row', args: { what: 'Row' } }),
              mitem('write.table.select', { label: 'Select Table', args: { what: 'Table' } }),
            ],
            { size: 'large', label: 'Select', icon: 'TextSelect' },
          ),
        ]),
        rgroup('rowsColumns', 'Rows & Columns', [
          rdropdown(
            'suite.table.delete',
            [
              mitem('suite.table.delete', { label: 'Delete Cell Contents', args: { what: 'CellContents' } }),
              mitem('suite.table.delete', { label: 'Delete Columns', args: { what: 'Columns' } }),
              mitem('suite.table.delete', { label: 'Delete Rows', args: { what: 'Rows' } }),
              mitem('suite.table.delete', { label: 'Delete Table', args: { what: 'Table' } }),
            ],
            { size: 'large' },
          ),
          rbutton('suite.table.insert_row_above', { size: 'large' }),
          rbutton('suite.table.insert_row_below', { size: 'small' }),
          rbutton('suite.table.insert_column_left', { size: 'small' }),
          rbutton('suite.table.insert_column_right', { size: 'small' }),
        ]),
        rgroup('merge', 'Merge', [
          rbutton('suite.table.merge_cells', { size: 'medium' }),
          rdropdown(
            'suite.table.split_cells',
            [2, 3, 4]
              .map((cols) =>
                mitem('suite.table.split_cells', {
                  label: `Split into ${cols} Columns`,
                  args: { rows: 1, cols },
                }),
              )
              .concat([
                mitem('suite.table.split_cells', { label: 'Split into 2 Rows', args: { rows: 2, cols: 1 } }),
              ]),
            { size: 'medium' },
          ),
          rbutton('write.table.split', { size: 'medium' }),
        ]),
        rgroup('cellSize', 'Cell Size', [
          rdropdown(
            'write.table.autofit',
            [
              mitem('write.table.autofit', { label: 'AutoFit Contents', args: { mode: 'Contents' } }),
              mitem('write.table.autofit', { label: 'AutoFit Window', args: { mode: 'Window' } }),
              mitem('write.table.autofit', { label: 'Fixed Column Width', args: { mode: 'Fixed' } }),
            ],
            { size: 'large' },
          ),
          rbutton('suite.table.distribute_rows', { size: 'medium' }),
          rbutton('suite.table.distribute_columns', { size: 'medium' }),
        ]),
        rgroup('alignment', 'Alignment', [
          rdropdown(
            'suite.table.cell_alignment',
            CELL_ALIGN.map(([vertical, horizontal, label]) =>
              mitem('suite.table.cell_alignment', { label, args: { vertical, horizontal } }),
            ),
            { size: 'large' },
          ),
          rdropdown(
            'write.table.align',
            [
              mitem('write.table.align', { label: 'Table Left', args: { align: 'Left' } }),
              mitem('write.table.align', { label: 'Table Centered', args: { align: 'Center' } }),
              mitem('write.table.align', { label: 'Table Right', args: { align: 'Right' } }),
            ],
            { size: 'medium', label: 'Position' },
          ),
        ]),
        rgroup('data', 'Data', [
          rtoggle('suite.table.repeat_header_rows', { size: 'medium' }),
          rbutton('suite.table.convert_to_text', { size: 'medium' }),
          rbutton('suite.table.formula', { size: 'medium' }),
        ]),
      ],
      { keyTip: 'JL', contextual: { set: 'tableTools', label: 'Table Tools', context: 'table' } },
    ),
  ];
}

/**
 * Pressed states and values of the ribbon's core commands in Write, read from the selection's
 * formatting the core reports (`Query::Write(Format)`, put in the registry context as
 * `textFormat` by the editor). Nothing is computed: each entry reads one field.
 */
type Fmt = import('@kiln/protocol').FormatState;
const fmt = (ctx: CommandContext): Fmt | undefined => ctx.textFormat as Fmt | undefined;
/**
 * Colour values are objects, and the ribbon compares values by identity: one object per colour,
 * so an unchanged colour never looks like a change (which would re-render forever).
 */
const colours = new Map<string, { type: 'rgb'; hex: string } | { type: 'auto' } | { type: 'none' }>();
function colour(key: string, make: () => { type: 'rgb'; hex: string } | { type: 'auto' } | { type: 'none' }) {
  let v = colours.get(key);
  if (!v) {
    v = make();
    colours.set(key, v);
  }
  return v;
}
const rgb = (hex: string | null | undefined) =>
  hex
    ? colour(`rgb:${hex}`, () => ({ type: 'rgb', hex: `#${hex}` }))
    : colour('auto', () => ({ type: 'auto' }));
const cm = (pt: number) => Math.round((pt / 72) * 2.54 * 100) / 100;

export const WRITE_CORE_STATE: {
  checked: Readonly<Record<string, (ctx: CommandContext) => boolean | undefined>>;
  values: Readonly<Record<string, (ctx: CommandContext) => unknown>>;
} = {
  checked: {
    'suite.format.bold': (c) => fmt(c)?.bold,
    'suite.format.italic': (c) => fmt(c)?.italic,
    'suite.format.underline': (c) => !!fmt(c)?.underline,
    'suite.format.double_underline': (c) => fmt(c)?.underline === 'Double',
    'suite.format.strikethrough': (c) => fmt(c)?.strike,
    'suite.format.subscript': (c) => fmt(c)?.subscript,
    'suite.format.superscript': (c) => fmt(c)?.superscript,
    'suite.format.all_caps': (c) => fmt(c)?.all_caps,
    'write.format.small_caps': (c) => fmt(c)?.small_caps,
    'suite.format.align_left': (c) => fmt(c)?.alignment === 'Left',
    'suite.format.align_center': (c) => fmt(c)?.alignment === 'Center',
    'suite.format.align_right': (c) => fmt(c)?.alignment === 'Right',
    'suite.format.justify': (c) => fmt(c)?.alignment === 'Justify',
    'suite.format.bullets': (c) => fmt(c)?.list?.bullet === true,
    'suite.format.numbering': (c) => fmt(c)?.list?.bullet === false,
    'write.paragraph.show_marks': (c) => fmt(c)?.show_marks,
    'suite.table.header_row': (c) => fmt(c)?.table?.header_row,
    'suite.table.total_row': (c) => fmt(c)?.table?.total_row,
    'suite.table.banded_rows': (c) => fmt(c)?.table?.banded_rows,
    'suite.table.first_column': (c) => fmt(c)?.table?.first_column,
    'suite.table.banded_columns': (c) => fmt(c)?.table?.banded_columns,
    'suite.table.repeat_header_rows': (c) => fmt(c)?.table?.repeat_header,
    'write.header_footer.different_first_page': (c) => fmt(c)?.header_footer?.different_first,
    'write.header_footer.different_odd_even': (c) => fmt(c)?.header_footer?.different_odd_even,
    'write.header_footer.link_to_previous': (c) => fmt(c)?.header_footer?.linked,
    // WP-66: Track Changes, Focus, Hyphenation.
    ...WRITE_FEATURE_CHECKED,
  },
  values: {
    'suite.format.font_family': (c) => fmt(c)?.font,
    'suite.format.font_size': (c) => {
      const s = fmt(c)?.size;
      return s === undefined ? undefined : String(Math.round(s * 2) / 2);
    },
    'suite.format.font_color': (c) => (fmt(c) ? rgb(fmt(c)?.color) : undefined),
    'suite.format.highlight': (c) => {
      const h = fmt(c)?.highlight;
      return h ? rgb(h) : colour('none', () => ({ type: 'none' }));
    },
    'write.styles.apply': (c) => fmt(c)?.style_id,
    'suite.table.styles': (c) => fmt(c)?.table?.style ?? undefined,
    'write.layout.indent_left': (c) => (fmt(c) ? cm(fmt(c)?.indent_left ?? 0) : undefined),
    'write.layout.indent_right': (c) => (fmt(c) ? cm(fmt(c)?.indent_right ?? 0) : undefined),
    'write.layout.spacing_before': (c) => fmt(c)?.space_before,
    'write.layout.spacing_after': (c) => fmt(c)?.space_after,
    // WP-66: Display for Review.
    ...WRITE_FEATURE_VALUES,
  },
};

/** Kiln Write's ribbon. */
export const writeRibbon: RibbonData = {
  app: 'write',
  tabs: [
    rtab(
      'home',
      'Home',
      [
        clipboardGroup(),
        fontGroup({ effects: true }),
        writeParagraphGroup(),
        rgroup(
          'styles',
          'Styles',
          [
            rgallery('write.styles.apply', WRITE_STYLES, {
              columns: 5,
              itemWidth: 74,
              itemHeight: 58,
              keyTip: 'L',
              footer: [
                mitem('suite.format.clear_formatting'),
                mitem('write.styles.pane', { label: 'Apply Styles…' }),
              ],
            }),
          ],
          { launcher: { command: 'write.styles.pane' }, icon: 'StylesPane', priority: 2 },
        ),
        rgroup(
          'editing',
          'Editing',
          [
            rsplit(
              'suite.edit.find',
              [
                mitem('suite.edit.find'),
                mitem('suite.edit.replace', { label: 'Advanced Find…' }),
                mitem('suite.edit.go_to'),
              ],
              {
                size: 'medium',
                keyTip: 'FD',
              },
            ),
            rbutton('suite.edit.replace', { size: 'medium', keyTip: 'R' }),
            rdropdown(
              'suite.edit.select',
              [mitem('write.caret.select_all'), mitem('suite.arrange.selection_pane')],
              {
                size: 'medium',
                keyTip: 'SL',
              },
            ),
          ],
          { icon: 'Search', priority: 1 },
        ),
        rgroup(
          'kiln',
          'Kiln',
          [
            rtoggle('write.view.focus_mode', { size: 'large', keyTip: 'FM' }),
            rbutton('suite.help.assistant', {
              size: 'large',
              label: 'Rewrite',
              icon: 'Sparkles',
              keyTip: 'ZA',
            }),
          ],
          { icon: 'Sparkles', priority: 0 },
        ),
      ],
      { keyTip: 'H' },
    ),
    rtab(
      'insert',
      'Insert',
      [
        rgroup('pages', 'Pages', [
          rdropdown('write.insert.cover_page', COVER_MENU, { size: 'medium', keyTip: 'V' }),
          rbutton('write.insert.blank_page', { size: 'medium', keyTip: 'NP' }),
          rbutton('write.insert.page_break', { size: 'medium', keyTip: 'B' }),
        ]),
        rgroup('tables', 'Tables', [
          rdropdown(
            'suite.insert.table',
            [
              mitem('suite.insert.table', { label: 'Insert Table…' }),
              mitem('write.insert.convert_text_to_table'),
              mitem('write.insert.paste_markdown_table'),
            ],
            { size: 'large', keyTip: 'T' },
          ),
        ]),
        rgroup('illustrations', 'Illustrations', [
          rdropdown('suite.insert.pictures', [mitem('suite.insert.pictures', { label: 'This Device…' })], {
            size: 'large',
            keyTip: 'P',
          }),
          rdropdown('suite.insert.shapes', SHAPES_MENU, { size: 'medium', keyTip: 'SH' }),
          rbutton('suite.insert.icons', { size: 'medium', keyTip: 'Y1' }),
          rbutton('suite.chart.insert', { size: 'medium', keyTip: 'C' }),
          rbutton('suite.insert.screenshot', { size: 'medium', keyTip: 'SC' }),
        ]),
        rgroup('links', 'Links', [
          rbutton('suite.insert.link', { size: 'large', keyTip: 'I' }),
          rbutton('ui.write.bookmark', { size: 'medium', keyTip: 'K', label: 'Bookmark', icon: 'Bookmark' }),
          rbutton('ui.write.cross_reference_dialog', {
            size: 'medium',
            keyTip: 'RF',
            label: 'Cross-reference',
            icon: 'CrossReference',
          }),
        ]),
        rgroup('comments', 'Comments', [
          rbutton('suite.review.new_comment', { size: 'large', label: 'Comment', keyTip: 'L' }),
        ]),
        rgroup('headerFooter', 'Header & Footer', [
          rbutton('write.insert.header', { size: 'medium', keyTip: 'H' }),
          rbutton('write.insert.footer', { size: 'medium', keyTip: 'O' }),
          rdropdown('write.insert.page_number', PAGE_NUMBER_MENU, { size: 'medium', keyTip: 'NU' }),
        ]),
        rgroup('text', 'Text', [
          rbutton('suite.insert.text_box', { size: 'large', keyTip: 'X' }),
          rbutton('write.insert.quick_parts', { size: 'small', keyTip: 'Q' }),
          rbutton('suite.insert.word_art', { size: 'small', keyTip: 'W', tier: 'P2' }),
          rdropdown(
            'write.insert.drop_cap',
            [
              mitem('write.insert.drop_cap', { label: 'None', args: {} }),
              mitem('write.insert.drop_cap', { label: 'Dropped', args: { placement: 'Dropped' } }),
              mitem('write.insert.drop_cap', { label: 'In Margin', args: { placement: 'InMargin' } }),
            ],
            { size: 'small', keyTip: 'RC' },
          ),
          rbutton('suite.insert.date_time', { size: 'small', keyTip: 'D' }),
          rbutton('suite.insert.object', { size: 'small', keyTip: 'J', tier: 'P2' }),
        ]),
        rgroup('symbols', 'Symbols', [
          rbutton('suite.insert.equation', { size: 'medium', keyTip: 'E', tier: 'P1' }),
          rbutton('suite.insert.symbol', { size: 'medium', keyTip: 'U' }),
        ]),
        rgroup('smart', 'Smart', [
          rbutton('write.insert.table_of_contents_quick', { size: 'medium', keyTip: 'ZT' }),
          rdropdown('write.insert.callout', CALLOUT_MENU, { size: 'medium', keyTip: 'ZC' }),
          rsplit('suite.insert.code_block', CODE_MENU, { size: 'medium', keyTip: 'ZK' }),
        ]),
      ],
      { keyTip: 'N' },
    ),
    rtab(
      'layout',
      'Layout',
      [
        rgroup(
          'pageSetup',
          'Page Setup',
          [
            rdropdown(
              'suite.layout.margins',
              [
                mitem('suite.layout.margins', { label: 'Normal', args: { preset: 'normal' } }),
                mitem('suite.layout.margins', { label: 'Narrow', args: { preset: 'narrow' } }),
                mitem('suite.layout.margins', { label: 'Moderate', args: { preset: 'moderate' } }),
                mitem('suite.layout.margins', { label: 'Wide', args: { preset: 'wide' } }),
                msep(),
                mitem('suite.layout.page_setup', { label: 'Custom Margins…' }),
              ],
              { size: 'large', keyTip: 'M' },
            ),
            rdropdown(
              'suite.layout.orientation',
              [
                mitem('suite.layout.orientation', { label: 'Portrait', args: { orientation: 'portrait' } }),
                mitem('suite.layout.orientation', { label: 'Landscape', args: { orientation: 'landscape' } }),
              ],
              { size: 'large', keyTip: 'O' },
            ),
            rdropdown(
              'suite.layout.size',
              [
                mitem('suite.layout.size', { label: 'Letter', args: { size: 'letter' } }),
                mitem('suite.layout.size', { label: 'Legal', args: { size: 'legal' } }),
                mitem('suite.layout.size', { label: 'A4', args: { size: 'a4' } }),
                mitem('suite.layout.size', { label: 'A5', args: { size: 'a5' } }),
                msep(),
                mitem('suite.layout.page_setup', { label: 'More Paper Sizes…' }),
              ],
              { size: 'large', keyTip: 'SZ' },
            ),
            rdropdown(
              'write.layout.columns',
              [
                mitem('write.layout.columns', { label: 'One', args: { count: 1 } }),
                mitem('write.layout.columns', { label: 'Two', args: { count: 2 } }),
                mitem('write.layout.columns', { label: 'Three', args: { count: 3 } }),
              ],
              { size: 'large', keyTip: 'J' },
            ),
            rdropdown('suite.layout.breaks', BREAKS_MENU, { size: 'medium', keyTip: 'B' }),
            rbutton('write.layout.line_numbers', { size: 'medium', keyTip: 'LN', tier: 'P2' }),
            rtoggle('write.layout.hyphenation', { size: 'medium', keyTip: 'H' }),
          ],
          { launcher: { command: 'suite.layout.page_setup' }, icon: 'PageSize' },
        ),
        rgroup(
          'paragraph',
          'Paragraph',
          [
            rnumber('write.layout.indent_left', {
              unit: 'cm',
              size: 'medium',
              min: 0,
              max: 55,
              step: 0.25,
              keyTip: 'IL',
            }),
            rnumber('write.layout.indent_right', {
              unit: 'cm',
              size: 'medium',
              min: 0,
              max: 55,
              step: 0.25,
              keyTip: 'IR',
            }),
            rnumber('write.layout.spacing_before', {
              unit: 'pt',
              size: 'medium',
              min: 0,
              max: 1584,
              step: 6,
              keyTip: 'SB',
            }),
            rnumber('write.layout.spacing_after', {
              unit: 'pt',
              size: 'medium',
              min: 0,
              max: 1584,
              step: 6,
              keyTip: 'SA',
            }),
          ],
          { rows: 2, launcher: { command: 'suite.format.paragraph_dialog' }, icon: 'Pilcrow' },
        ),
        rgroup('arrange', 'Arrange', [
          rdropdown('suite.arrange.position', POSITION_MENU, { size: 'large', keyTip: 'PO' }),
          rdropdown('suite.arrange.wrap_text', WRAP_MENU, { size: 'large', keyTip: 'TW' }),
          rsplit('suite.arrange.bring_forward', FORWARD_MENU, { size: 'medium', keyTip: 'AF' }),
          rsplit('suite.arrange.send_backward', BACKWARD_MENU, { size: 'medium', keyTip: 'AE' }),
          rtoggle('suite.arrange.selection_pane', { size: 'medium', keyTip: 'AP' }),
          rdropdown('suite.arrange.align', ALIGN_MENU, { size: 'small', keyTip: 'AA' }),
          rbutton('suite.arrange.group', { size: 'small', keyTip: 'G' }),
          rdropdown('suite.arrange.rotate', ROTATE_MENU, { size: 'small', keyTip: 'AY' }),
        ]),
      ],
      { keyTip: 'P' },
    ),
    rtab(
      'references',
      'References',
      [
        rgroup('toc', 'Table of Contents', [
          rdropdown('write.references.table_of_contents', TOC_MENU, { size: 'large', keyTip: 'T' }),
          rdropdown('write.references.add_text', ADD_TEXT_MENU, { size: 'medium', keyTip: 'A' }),
          rdropdown('write.references.update_table', UPDATE_TOC_MENU, { size: 'medium', keyTip: 'U' }),
        ]),
        rgroup(
          'footnotes',
          'Footnotes',
          [
            rbutton('write.references.insert_footnote', { size: 'large', keyTip: 'F' }),
            rbutton('write.references.insert_endnote', { size: 'medium', keyTip: 'E' }),
            rsplit('write.references.next_footnote', NEXT_NOTE_MENU, { size: 'medium', keyTip: 'O' }),
            rbutton('write.references.show_notes', { size: 'medium', keyTip: 'H' }),
          ],
          { launcher: { command: 'ui.write.footnote_dialog' } },
        ),
        rgroup(
          'citations',
          'Citations & Bibliography',
          [
            rbutton('write.references.insert_citation', { size: 'large', keyTip: 'C' }),
            rbutton('write.references.manage_sources', { size: 'medium', keyTip: 'M' }),
            rbutton('write.references.bibliography', { size: 'medium', keyTip: 'B' }),
          ],
          { tier: 'P2' },
        ),
        rgroup('captions', 'Captions', [
          rbutton('ui.write.caption_dialog', {
            size: 'large',
            keyTip: 'P',
            label: 'Insert Caption',
            icon: 'InsertCaption',
          }),
          rbutton('write.references.table_of_figures', { size: 'medium', keyTip: 'G', tier: 'P1' }),
          rbutton('ui.write.cross_reference_dialog', {
            size: 'medium',
            keyTip: 'RF',
            label: 'Cross-reference',
            icon: 'CrossReference',
          }),
        ]),
        rgroup('index', 'Index', [rbutton('write.references.mark_entry', { size: 'large', keyTip: 'N' })], {
          tier: 'P2',
        }),
      ],
      { keyTip: 'S' },
    ),
    rtab(
      'review',
      'Review',
      [
        rgroup('proofing', 'Proofing', [
          rbutton('suite.review.spelling', { size: 'large', keyTip: 'C' }),
          rbutton('suite.review.thesaurus', { size: 'medium', keyTip: 'E', tier: 'P2' }),
          rbutton('suite.review.word_count', { size: 'medium', keyTip: 'W' }),
        ]),
        rgroup('language', 'Language', [
          rbutton('suite.review.translate', { size: 'large', keyTip: 'L' }),
          rbutton('suite.review.language', { size: 'medium', keyTip: 'U' }),
        ]),
        rgroup('comments', 'Comments', [
          rbutton('suite.review.new_comment', { size: 'large', label: 'New Comment', keyTip: 'C2' }),
          rbutton('suite.review.delete_comment', { size: 'medium', keyTip: 'D' }),
          rbutton('suite.review.previous_comment', { size: 'medium', keyTip: 'V' }),
          rbutton('suite.review.next_comment', { size: 'medium', keyTip: 'N' }),
          rtoggle('suite.review.show_comments', { size: 'medium', keyTip: 'K' }),
          rtoggle('ui.write.comments_pane', { size: 'medium', keyTip: 'CP', label: 'Comments Pane' }),
        ]),
        rgroup('tracking', 'Tracking', [
          rtoggle('write.review.track_changes', { size: 'large', keyTip: 'G' }),
          rdropdown('write.review.markup_mode', MARKUP_MENU, { size: 'medium', keyTip: 'TD' }),
          rdropdown('write.review.show_markup', SHOW_MARKUP_MENU, { size: 'medium', keyTip: 'TM' }),
          rtoggle('write.review.reviewing_pane', { size: 'medium', keyTip: 'TP' }),
        ]),
        rgroup('changes', 'Changes', [
          rsplit('write.review.accept', ACCEPT_MENU, { size: 'large', keyTip: 'A2' }),
          rsplit('write.review.reject', REJECT_MENU, { size: 'medium', keyTip: 'J' }),
          rbutton('write.review.previous_change', { size: 'medium', keyTip: 'F' }),
          rbutton('write.review.next_change', { size: 'medium', keyTip: 'H' }),
        ]),
        rgroup(
          'compare',
          'Compare',
          [rbutton('suite.review.compare', { size: 'large', keyTip: 'M', tier: 'P2' })],
          { tier: 'P2' },
        ),
        rgroup(
          'protect',
          'Protect',
          [rbutton('write.review.restrict_editing', { size: 'large', keyTip: 'PE' })],
          { tier: 'P2' },
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
        rgroup('views', 'Views', [
          viewMode('ReadMode', 'Read Mode', 'ReadMode', 'F'),
          viewMode('PrintLayout', 'Print Layout', 'PrintLayout', 'P'),
          viewMode('WebLayout', 'Web Layout', 'WebLayout', 'L'),
          { ...viewMode('Outline', 'Outline', 'OutlineView', 'U', 'medium'), tier: 'P1' },
          viewMode('Draft', 'Draft', 'DraftView', 'E', 'medium'),
        ]),
        rgroup('immersive', 'Immersive', [
          rtoggle('write.view.focus_mode', { size: 'large', keyTip: 'O' }),
          rbutton('ui.write.word_goal_dialog', { size: 'large', label: 'Word Goal', keyTip: 'WG' }),
          rbutton('suite.view.immersive_reader', { size: 'large', keyTip: 'IR', tier: 'P2' }),
        ]),
        rgroup('pageMovement', 'Page Movement', [
          rtoggle('suite.view.vertical_pages', { size: 'large', keyTip: 'VP' }),
          rtoggle('suite.view.side_to_side', { size: 'large', keyTip: 'SS' }),
        ]),
        rgroup('show', 'Show', [
          rtoggle('suite.view.ruler', { size: 'small', keyTip: 'R' }),
          rtoggle('suite.view.gridlines', { size: 'small', keyTip: 'G' }),
          rtoggle('suite.view.navigation_pane', { size: 'small', keyTip: 'K' }),
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
          rbutton('suite.view.one_page', { size: 'small', keyTip: '1' }),
          rbutton('suite.view.multiple_pages', { size: 'small', keyTip: 'M' }),
          rbutton('suite.view.page_width', { size: 'small', keyTip: 'I' }),
        ]),
        rgroup('window', 'Window', [
          rbutton('suite.view.new_window', { size: 'large', keyTip: 'N' }),
          rbutton('suite.view.split', { size: 'small', keyTip: 'Q2' }),
          rbutton('suite.view.side_by_side', { size: 'small', keyTip: 'B' }),
        ]),
        themeGroup(),
        macrosGroup(),
      ],
      { keyTip: 'W' },
    ),
    ...writeTableTabs(),
    pictureFormatTab(),
    shapeFormatTab(),
    rtab(
      'headerFooter',
      'Header & Footer',
      [
        rgroup('headerFooter', 'Header & Footer', [
          rbutton('write.insert.header', { size: 'large' }),
          rbutton('write.insert.footer', { size: 'large' }),
          rdropdown('write.insert.page_number', PAGE_NUMBER_MENU, { size: 'large' }),
        ]),
        rgroup('insert', 'Insert', [
          rbutton('suite.insert.date_time', { size: 'medium' }),
          rbutton('write.insert.quick_parts', { size: 'medium', label: 'Document Info' }),
        ]),
        rgroup('options', 'Options', [
          rtoggle('write.header_footer.different_first_page', { size: 'small' }),
          rtoggle('write.header_footer.different_odd_even', { size: 'small' }),
          rtoggle('write.header_footer.link_to_previous', { size: 'small' }),
        ]),
        rgroup('close', 'Close', [rbutton('write.header_footer.close', { size: 'large' })]),
      ],
      {
        keyTip: 'JH',
        contextual: { set: 'headerFooter', label: 'Header & Footer', context: 'header_footer' },
      },
    ),
  ],
};
