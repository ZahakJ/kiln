/** Kiln Write commands and ribbon: PLAN.md §1.4 "Write" and the Write column of §1.7. */
import {
  mitem,
  msep,
  type RibbonData,
  type RibbonGalleryEntry,
  rbutton,
  rdropdown,
  rgallery,
  rgroup,
  rnumber,
  rsplit,
  rtab,
  rtoggle,
} from '../ribbon.ts';
import { c, section, WRITE } from './define.ts';
import {
  clipboardGroup,
  fontGroup,
  macrosGroup,
  paragraphGroup,
  pictureFormatTab,
  shapeFormatTab,
  tableTabs,
  themeGroup,
  viewMode,
  zoomMenu,
} from './ribbon-shared.ts';

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
        paragraphGroup({
          multilevel: true,
          first: [
            rbutton('write.paragraph.sort', { hideInSimplified: true }),
            rtoggle('write.paragraph.show_marks', { keyTip: '8' }),
          ],
          second: [
            rbutton('write.paragraph.shading', { hideInSimplified: true }),
            rbutton('write.paragraph.borders', { hideInSimplified: true }),
          ],
        }),
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
            rbutton('write.view.focus_mode', { size: 'large', keyTip: 'FM' }),
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
          rbutton('write.insert.cover_page', { size: 'medium', keyTip: 'V' }),
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
          rbutton('suite.insert.shapes', { size: 'medium', keyTip: 'SH' }),
          rbutton('suite.insert.icons', { size: 'medium', keyTip: 'Y1' }),
          rbutton('suite.chart.insert', { size: 'medium', keyTip: 'C' }),
          rbutton('suite.insert.screenshot', { size: 'medium', keyTip: 'SC' }),
        ]),
        rgroup('links', 'Links', [
          rbutton('suite.insert.link', { size: 'large', keyTip: 'I' }),
          rbutton('write.insert.bookmark', { size: 'medium', keyTip: 'K' }),
          rbutton('write.insert.cross_reference', { size: 'medium', keyTip: 'RF' }),
        ]),
        rgroup('comments', 'Comments', [
          rbutton('suite.review.new_comment', { size: 'large', label: 'Comment', keyTip: 'L' }),
        ]),
        rgroup('headerFooter', 'Header & Footer', [
          rbutton('write.insert.header', { size: 'medium', keyTip: 'H' }),
          rbutton('write.insert.footer', { size: 'medium', keyTip: 'O' }),
          rbutton('write.insert.page_number', { size: 'medium', keyTip: 'NU' }),
        ]),
        rgroup('text', 'Text', [
          rbutton('suite.insert.text_box', { size: 'large', keyTip: 'X' }),
          rbutton('write.insert.quick_parts', { size: 'small', keyTip: 'Q' }),
          rbutton('suite.insert.word_art', { size: 'small', keyTip: 'W', tier: 'P2' }),
          rbutton('write.insert.drop_cap', { size: 'small', keyTip: 'RC' }),
          rbutton('suite.insert.date_time', { size: 'small', keyTip: 'D' }),
          rbutton('suite.insert.object', { size: 'small', keyTip: 'J', tier: 'P2' }),
        ]),
        rgroup('symbols', 'Symbols', [
          rbutton('suite.insert.equation', { size: 'medium', keyTip: 'E', tier: 'P1' }),
          rbutton('suite.insert.symbol', { size: 'medium', keyTip: 'U' }),
        ]),
        rgroup('smart', 'Smart', [
          rbutton('write.insert.table_of_contents_quick', { size: 'medium', keyTip: 'ZT' }),
          rbutton('write.insert.callout', { size: 'medium', keyTip: 'ZC' }),
          rbutton('suite.insert.code_block', { size: 'medium', keyTip: 'ZK' }),
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
            rbutton('suite.layout.breaks', { size: 'medium', keyTip: 'B' }),
            rbutton('write.layout.line_numbers', { size: 'medium', keyTip: 'LN', tier: 'P2' }),
            rbutton('write.layout.hyphenation', { size: 'medium', keyTip: 'H' }),
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
          rbutton('suite.arrange.position', { size: 'large', keyTip: 'PO' }),
          rbutton('suite.arrange.wrap_text', { size: 'large', keyTip: 'TW' }),
          rbutton('suite.arrange.bring_forward', { size: 'medium', keyTip: 'AF' }),
          rbutton('suite.arrange.send_backward', { size: 'medium', keyTip: 'AE' }),
          rbutton('suite.arrange.selection_pane', { size: 'medium', keyTip: 'AP' }),
          rbutton('suite.arrange.align', { size: 'small', keyTip: 'AA' }),
          rbutton('suite.arrange.group', { size: 'small', keyTip: 'G' }),
          rbutton('suite.arrange.rotate', { size: 'small', keyTip: 'AY' }),
        ]),
      ],
      { keyTip: 'P' },
    ),
    rtab(
      'references',
      'References',
      [
        rgroup('toc', 'Table of Contents', [
          rbutton('write.references.table_of_contents', { size: 'large', keyTip: 'T' }),
          rbutton('write.references.add_text', { size: 'medium', keyTip: 'A' }),
          rbutton('write.references.update_table', { size: 'medium', keyTip: 'U' }),
        ]),
        rgroup('footnotes', 'Footnotes', [
          rbutton('write.references.insert_footnote', { size: 'large', keyTip: 'F' }),
          rbutton('write.references.insert_endnote', { size: 'medium', keyTip: 'E' }),
          rbutton('write.references.next_footnote', { size: 'medium', keyTip: 'O' }),
          rbutton('write.references.show_notes', { size: 'medium', keyTip: 'H' }),
        ]),
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
          rbutton('write.references.insert_caption', { size: 'large', keyTip: 'P' }),
          rbutton('write.references.table_of_figures', { size: 'medium', keyTip: 'G', tier: 'P1' }),
          rbutton('write.insert.cross_reference', { size: 'medium', keyTip: 'RF' }),
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
        ]),
        rgroup('tracking', 'Tracking', [
          rtoggle('write.review.track_changes', { size: 'large', keyTip: 'G' }),
          rbutton('write.review.show_markup', { size: 'medium', keyTip: 'TM' }),
          rbutton('write.review.reviewing_pane', { size: 'medium', keyTip: 'TP' }),
        ]),
        rgroup('changes', 'Changes', [
          rbutton('write.review.accept', { size: 'large', keyTip: 'A2' }),
          rbutton('write.review.reject', { size: 'medium', keyTip: 'J' }),
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
          rbutton('write.view.focus_mode', { size: 'large', keyTip: 'O' }),
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
    ...tableTabs('write'),
    pictureFormatTab(),
    shapeFormatTab(),
    rtab(
      'headerFooter',
      'Header & Footer',
      [
        rgroup('headerFooter', 'Header & Footer', [
          rbutton('write.insert.header', { size: 'large' }),
          rbutton('write.insert.footer', { size: 'large' }),
          rbutton('write.insert.page_number', { size: 'large' }),
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
