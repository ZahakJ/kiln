/**
 * Chrome demo: a Write-like window wired to the real command registry, keymap, ribbon spec,
 * Backstage, status bar, task panes, palette, mini toolbar, context menu and Key Tips. The document
 * is a contentEditable page whose formatting commands use the browser's editing commands, so every
 * ribbon state (Bold pressed, font name…) is live. Used for screenshots and manual review.
 */
import {
  type CommandDefinition,
  type CommandRegistry,
  createCommandRegistry,
  createDefaultKeymap,
  type Keymap,
} from '@kiln/commands';
import { type ReactNode, useEffect, useMemo, useRef, useState } from 'react';
import {
  AppFrame,
  button,
  type ColorChoice,
  ContextMenu,
  color,
  combobox,
  Dialog,
  DialogContent,
  dropdown,
  gallery,
  group,
  item,
  type MenuEntry,
  MiniToolbar,
  numberInput,
  type RibbonSpec,
  ribbon,
  row,
  StatusBar,
  separator,
  split,
  submenu,
  type TaskPane,
  TaskPaneHost,
  tab,
  toggle,
  useSelectionMiniToolbar,
  validateRibbonSpec,
} from '../src/index.ts';
import './chrome.css';

const FONTS = ['Figtree', 'Fraunces', 'Calibri', 'Cambria', 'Arial', 'Times New Roman', 'Courier New'];
const SIZES = ['8', '9', '10', '10.5', '11', '12', '14', '16', '18', '20', '24', '28', '36', '48', '72'];
const STYLE_ITEMS = [
  { id: 'p', label: 'Normal', preview: <span className="cx-style cx-style--normal">AaBbCcDd</span> },
  {
    id: 'nospacing',
    label: 'No Spacing',
    preview: <span className="cx-style cx-style--normal">AaBbCcDd</span>,
  },
  { id: 'h1', label: 'Heading 1', preview: <span className="cx-style cx-style--h1">AaBbCc</span> },
  { id: 'h2', label: 'Heading 2', preview: <span className="cx-style cx-style--h2">AaBbCcD</span> },
  { id: 'h3', label: 'Heading 3', preview: <span className="cx-style cx-style--h3">AaBbCcD</span> },
  { id: 'title', label: 'Title', preview: <span className="cx-style cx-style--title">AaB</span> },
  {
    id: 'subtitle',
    label: 'Subtitle',
    preview: <span className="cx-style cx-style--subtitle">AaBbCcD</span>,
  },
  { id: 'quote', label: 'Quote', preview: <span className="cx-style cx-style--quote">AaBbCcD</span> },
  {
    id: 'emphasis',
    label: 'Emphasis',
    preview: <span className="cx-style cx-style--emphasis">AaBbCcD</span>,
  },
  { id: 'strong', label: 'Strong', preview: <span className="cx-style cx-style--strong">AaBbCcD</span> },
];
const BLOCK_FOR: Record<string, string> = {
  p: 'p',
  nospacing: 'p',
  h1: 'h1',
  h2: 'h2',
  h3: 'h3',
  title: 'h1',
  subtitle: 'h2',
  quote: 'blockquote',
};

const pasteMenu: MenuEntry[] = [
  { kind: 'heading', label: 'Paste Options' },
  item('suite.edit.paste_keep_source'),
  item('suite.edit.paste_merge_formatting'),
  item('suite.edit.paste_text_only'),
  separator(),
  item('suite.edit.paste_special'),
];

/** The demo ribbon: Write's Home tab in full, plus Insert, Layout, Review, View and a contextual set. */
export const demoRibbon: RibbonSpec = ribbon('write', [
  tab(
    'home',
    'Home',
    [
      group('clipboard', 'Clipboard', [
        split('suite.edit.paste', pasteMenu, { size: 'large' }),
        button('suite.edit.cut', { size: 'medium' }),
        button('suite.edit.copy', { size: 'medium' }),
        button('suite.edit.format_painter', { size: 'medium' }),
      ]),
      group(
        'font',
        'Font',
        [
          row(
            combobox(
              'suite.format.font_family',
              FONTS.map((f) => ({ value: f, style: { fontFamily: f } })),
              { width: 132 },
            ),
            combobox(
              'suite.format.font_size',
              SIZES.map((s) => ({ value: s })),
              { width: 56 },
            ),
            button('suite.format.grow_font'),
            button('suite.format.shrink_font'),
            dropdown(
              'suite.format.change_case',
              [
                item('suite.format.change_case', { label: 'Sentence case.' }),
                item('suite.format.change_case', { label: 'lowercase' }),
                item('suite.format.change_case', { label: 'UPPERCASE' }),
              ],
              { size: 'small' },
            ),
            button('suite.format.clear_formatting'),
          ),
          row(
            toggle('suite.format.bold'),
            toggle('suite.format.italic'),
            split(
              'suite.format.underline',
              [item('suite.format.underline'), item('suite.format.double_underline')],
              {
                size: 'small',
              },
            ),
            toggle('suite.format.strikethrough'),
            toggle('suite.format.subscript'),
            toggle('suite.format.superscript'),
            color('suite.format.highlight', { defaultColor: '#FFE066', allowNone: true }),
            color('suite.format.font_color', { defaultColor: '#C8352B', allowAuto: true }),
          ),
        ],
        { rows: 2, launcher: { command: 'suite.format.font_dialog' }, icon: 'Type' },
      ),
      group(
        'paragraph',
        'Paragraph',
        [
          row(
            split('suite.format.bullets', [item('suite.format.bullets'), item('suite.format.numbering')], {
              size: 'small',
            }),
            split('suite.format.numbering', [item('suite.format.numbering')], { size: 'small' }),
            button('suite.format.multilevel_list'),
            button('suite.format.outdent'),
            button('suite.format.indent'),
            button('write.paragraph.sort'),
            toggle('write.paragraph.show_marks'),
          ),
          row(
            toggle('suite.format.align_left'),
            toggle('suite.format.align_center'),
            toggle('suite.format.align_right'),
            toggle('suite.format.justify'),
            dropdown(
              'suite.format.line_spacing',
              [
                item('suite.format.line_spacing', { label: '1.0', args: { lineSpacing: 1 } }),
                item('suite.format.line_spacing', { label: '1.15', args: { lineSpacing: 1.15 } }),
                item('suite.format.line_spacing', { label: '1.5', args: { lineSpacing: 1.5 } }),
                item('suite.format.line_spacing', { label: '2.0', args: { lineSpacing: 2 } }),
              ],
              { size: 'small' },
            ),
            button('write.paragraph.shading'),
            button('write.paragraph.borders'),
          ),
        ],
        { rows: 2, launcher: { command: 'suite.format.paragraph_dialog' }, icon: 'Pilcrow' },
      ),
      group(
        'styles',
        'Styles',
        [
          gallery('write.styles.apply', STYLE_ITEMS, {
            columns: 5,
            itemWidth: 84,
            itemHeight: 62,
            footer: [item('suite.format.clear_formatting'), item('write.styles.pane')],
          }),
        ],
        { launcher: { command: 'write.styles.pane' }, icon: 'StylesPane', priority: 2 },
      ),
      group(
        'editing',
        'Editing',
        [
          split('suite.edit.find', [item('suite.edit.find'), item('suite.edit.go_to')], { size: 'medium' }),
          button('suite.edit.replace', { size: 'medium' }),
          dropdown(
            'suite.edit.select',
            [item('suite.edit.select_all'), item('suite.arrange.selection_pane')],
            {
              size: 'medium',
            },
          ),
        ],
        { priority: 1 },
      ),
      group(
        'focus',
        'Focus',
        [
          button('write.view.focus_mode', { size: 'large' }),
          button('ui.palette.open', { size: 'large', label: 'Command Palette' }),
        ],
        { priority: 0 },
      ),
    ],
    { keyTip: 'H' },
  ),
  tab(
    'insert',
    'Insert',
    [
      group('pages', 'Pages', [
        button('write.insert.cover_page', { size: 'medium' }),
        button('write.insert.blank_page', { size: 'medium' }),
        button('write.insert.page_break', { size: 'medium' }),
      ]),
      group('tables', 'Tables', [
        dropdown(
          'suite.insert.table',
          [
            item('suite.insert.table', { label: 'Insert Table…' }),
            item('write.insert.convert_text_to_table'),
            item('write.insert.paste_markdown_table'),
          ],
          { size: 'large' },
        ),
      ]),
      group('illustrations', 'Illustrations', [
        dropdown('suite.insert.pictures', [item('suite.insert.pictures', { label: 'This Device…' })], {
          size: 'large',
        }),
        button('suite.insert.shapes', { size: 'medium' }),
        button('suite.insert.icons', { size: 'medium' }),
        button('suite.chart.insert', { size: 'medium' }),
      ]),
      group('links', 'Links', [
        button('suite.insert.link', { size: 'large' }),
        button('write.insert.bookmark', { size: 'medium' }),
        button('write.insert.cross_reference', { size: 'medium' }),
      ]),
      group('comments', 'Comments', [
        button('suite.review.new_comment', { size: 'large', label: 'Comment' }),
      ]),
      group('headerFooter', 'Header & Footer', [
        button('write.insert.header', { size: 'medium' }),
        button('write.insert.footer', { size: 'medium' }),
        button('write.insert.page_number', { size: 'medium' }),
      ]),
      group('text', 'Text', [
        button('suite.insert.text_box', { size: 'large' }),
        button('write.insert.quick_parts', { size: 'small' }),
        button('suite.insert.word_art', { size: 'small' }),
        button('write.insert.drop_cap', { size: 'small' }),
        button('suite.insert.date_time', { size: 'small' }),
      ]),
      group('symbols', 'Symbols', [
        button('suite.insert.equation', { size: 'medium' }),
        button('suite.insert.symbol', { size: 'medium' }),
      ]),
    ],
    { keyTip: 'N' },
  ),
  tab(
    'layout',
    'Layout',
    [
      group(
        'pageSetup',
        'Page Setup',
        [
          dropdown(
            'suite.layout.margins',
            [
              submenu('Margins', [
                item('suite.layout.margins', { label: 'Normal' }),
                item('suite.layout.margins', { label: 'Narrow' }),
                item('suite.layout.margins', { label: 'Wide' }),
              ]),
            ],
            { size: 'large' },
          ),
          dropdown(
            'suite.layout.orientation',
            [
              item('suite.layout.orientation', { label: 'Portrait' }),
              item('suite.layout.orientation', { label: 'Landscape' }),
            ],
            { size: 'large' },
          ),
          dropdown(
            'suite.layout.size',
            [item('suite.layout.size', { label: 'Letter' }), item('suite.layout.size', { label: 'A4' })],
            { size: 'large' },
          ),
          dropdown(
            'write.layout.columns',
            [item('write.layout.columns', { label: 'One' }), item('write.layout.columns', { label: 'Two' })],
            { size: 'large' },
          ),
          button('suite.layout.breaks', { size: 'medium' }),
          button('write.layout.line_numbers', { size: 'medium' }),
          button('write.layout.hyphenation', { size: 'medium' }),
        ],
        { launcher: { command: 'suite.layout.page_setup' } },
      ),
      group(
        'paragraph',
        'Paragraph',
        [
          numberInput('write.layout.indent_left', { unit: 'cm', size: 'medium', min: 0, max: 50 }),
          numberInput('write.layout.indent_right', { unit: 'cm', size: 'medium', min: 0, max: 50 }),
          numberInput('write.layout.spacing_before', {
            unit: 'pt',
            size: 'medium',
            min: 0,
            max: 1584,
            step: 6,
          }),
          numberInput('write.layout.spacing_after', {
            unit: 'pt',
            size: 'medium',
            min: 0,
            max: 1584,
            step: 6,
          }),
        ],
        { rows: 2 },
      ),
      group('arrange', 'Arrange', [
        button('suite.arrange.position', { size: 'large' }),
        button('suite.arrange.wrap_text', { size: 'large' }),
        button('suite.arrange.bring_forward', { size: 'medium' }),
        button('suite.arrange.send_backward', { size: 'medium' }),
        button('suite.arrange.selection_pane', { size: 'medium' }),
        button('suite.arrange.align', { size: 'small' }),
        button('suite.arrange.group', { size: 'small' }),
        button('suite.arrange.rotate', { size: 'small' }),
      ]),
    ],
    { keyTip: 'P' },
  ),
  tab(
    'review',
    'Review',
    [
      group('proofing', 'Proofing', [
        button('suite.review.spelling', { size: 'large' }),
        button('suite.review.thesaurus', { size: 'medium' }),
        button('suite.review.word_count', { size: 'medium' }),
      ]),
      group('comments', 'Comments', [
        button('suite.review.new_comment', { size: 'large' }),
        button('suite.review.delete_comment', { size: 'medium' }),
        button('suite.review.previous_comment', { size: 'medium' }),
        button('suite.review.next_comment', { size: 'medium' }),
      ]),
      group('tracking', 'Tracking', [
        toggle('write.review.track_changes', { size: 'large' }),
        button('write.review.show_markup', { size: 'medium' }),
        button('write.review.reviewing_pane', { size: 'medium' }),
      ]),
      group('changes', 'Changes', [
        button('write.review.accept', { size: 'large' }),
        button('write.review.reject', { size: 'large' }),
        button('write.review.previous_change', { size: 'medium' }),
        button('write.review.next_change', { size: 'medium' }),
      ]),
      group('history', 'History', [button('suite.file.version_history', { size: 'large' })]),
    ],
    { keyTip: 'R' },
  ),
  tab(
    'view',
    'View',
    [
      group('views', 'Views', [
        toggle('suite.view.mode', {
          id: 'view.read',
          args: { mode: 'ReadMode' },
          checkedValue: 'ReadMode',
          label: 'Read Mode',
          icon: 'ReadMode',
          size: 'large',
        }),
        toggle('suite.view.mode', {
          id: 'view.print',
          args: { mode: 'PrintLayout' },
          checkedValue: 'PrintLayout',
          label: 'Print Layout',
          icon: 'PrintLayout',
          size: 'large',
        }),
        toggle('suite.view.mode', {
          id: 'view.web',
          args: { mode: 'WebLayout' },
          checkedValue: 'WebLayout',
          label: 'Web Layout',
          icon: 'WebLayout',
          size: 'large',
        }),
      ]),
      group('immersive', 'Immersive', [button('write.view.focus_mode', { size: 'large' })]),
      group('show', 'Show', [
        toggle('suite.view.ruler', { size: 'medium' }),
        toggle('suite.view.gridlines', { size: 'medium' }),
        toggle('suite.view.navigation_pane', { size: 'medium' }),
      ]),
      group('zoom', 'Zoom', [
        button('suite.view.zoom', { size: 'large' }),
        button('suite.view.zoom_100', { size: 'large' }),
        button('suite.view.one_page', { size: 'medium' }),
        button('suite.view.multiple_pages', { size: 'medium' }),
        button('suite.view.page_width', { size: 'medium' }),
      ]),
      group('ribbon', 'Ribbon', [
        toggle('ui.ribbon.classic', { size: 'medium' }),
        toggle('ui.ribbon.simplified', { size: 'medium' }),
        toggle('ui.ribbon.collapse', { size: 'medium' }),
      ]),
      group('theme', 'Theme', [
        toggle('ui.theme.light', { size: 'medium' }),
        toggle('ui.theme.dark', { size: 'medium' }),
        toggle('ui.theme.system', { size: 'medium' }),
      ]),
    ],
    { keyTip: 'W' },
  ),
  tab(
    'tableDesign',
    'Table Design',
    [
      group('styleOptions', 'Table Style Options', [
        toggle('suite.view.gridlines', { size: 'medium', label: 'Header Row' }),
      ]),
      group('tableStyles', 'Table Styles', [
        button('write.paragraph.shading', { size: 'large' }),
        button('write.paragraph.borders', { size: 'large' }),
      ]),
    ],
    { contextual: { set: 'table', label: 'Table Tools' }, when: (ctx) => ctx.inTable === true, keyTip: 'JT' },
  ),
  tab(
    'tableLayout',
    'Layout',
    [
      group('rowsCols', 'Rows & Columns', [
        button('grid.cells.insert_cells', { size: 'large', label: 'Insert Above' }),
        button('grid.cells.delete_cells', { size: 'large', label: 'Delete' }),
      ]),
    ],
    { contextual: { set: 'table', label: 'Table Tools' }, when: (ctx) => ctx.inTable === true, keyTip: 'JL' },
  ),
]);

/** The demo's views as the core's `ViewMode` values (`suite.view.mode`). */
const VIEW_OF: Record<string, string> = { ReadMode: 'read', PrintLayout: 'print', WebLayout: 'web' };
const MODE_OF: Record<string, string> = { read: 'ReadMode', print: 'PrintLayout', web: 'WebLayout' };

/** Live editing state of the demo page. */
function exec(command: string, value?: string) {
  // The deprecated but universally supported editing API is enough for a demo page.
  document.execCommand(command, false, value);
}
const queryState = (cmd: string) => {
  try {
    return document.queryCommandState(cmd);
  } catch {
    return false;
  }
};

function demoCommands(env: {
  openPane: (id: string) => void;
  togglePane: (id: string) => void;
  openFontDialog: () => void;
  zoom: { get: () => number; set: (z: number) => void };
  view: { get: () => string; set: (v: string) => void };
  trackChanges: { get: () => boolean; set: (v: boolean) => void };
  ruler: { get: () => boolean; set: (v: boolean) => void };
}): CommandDefinition<never>[] {
  const defs: CommandDefinition<unknown>[] = [
    { id: 'suite.format.bold', title: 'Bold', run: () => exec('bold'), isChecked: () => queryState('bold') },
    {
      id: 'suite.format.italic',
      title: 'Italic',
      run: () => exec('italic'),
      isChecked: () => queryState('italic'),
    },
    {
      id: 'suite.format.underline',
      title: 'Underline',
      run: () => exec('underline'),
      isChecked: () => queryState('underline'),
    },
    {
      id: 'suite.format.strikethrough',
      title: 'Strikethrough',
      run: () => exec('strikeThrough'),
      isChecked: () => queryState('strikeThrough'),
    },
    {
      id: 'suite.format.subscript',
      title: 'Subscript',
      run: () => exec('subscript'),
      isChecked: () => queryState('subscript'),
    },
    {
      id: 'suite.format.superscript',
      title: 'Superscript',
      run: () => exec('superscript'),
      isChecked: () => queryState('superscript'),
    },
    {
      id: 'suite.format.align_left',
      title: 'Align Left',
      run: () => exec('justifyLeft'),
      isChecked: () => queryState('justifyLeft'),
    },
    {
      id: 'suite.format.align_center',
      title: 'Center',
      run: () => exec('justifyCenter'),
      isChecked: () => queryState('justifyCenter'),
    },
    {
      id: 'suite.format.align_right',
      title: 'Align Right',
      run: () => exec('justifyRight'),
      isChecked: () => queryState('justifyRight'),
    },
    {
      id: 'suite.format.justify',
      title: 'Justify',
      run: () => exec('justifyFull'),
      isChecked: () => queryState('justifyFull'),
    },
    {
      id: 'suite.format.bullets',
      title: 'Bullets',
      run: () => exec('insertUnorderedList'),
      isChecked: () => queryState('insertUnorderedList'),
    },
    {
      id: 'suite.format.numbering',
      title: 'Numbering',
      run: () => exec('insertOrderedList'),
      isChecked: () => queryState('insertOrderedList'),
    },
    { id: 'suite.format.indent', title: 'Increase Indent', run: () => exec('indent') },
    { id: 'suite.format.outdent', title: 'Decrease Indent', run: () => exec('outdent') },
    { id: 'suite.format.clear_formatting', title: 'Clear All Formatting', run: () => exec('removeFormat') },
    { id: 'suite.edit.undo', title: 'Undo', run: () => exec('undo') },
    { id: 'suite.edit.redo', title: 'Redo', run: () => exec('redo') },
    { id: 'suite.edit.cut', title: 'Cut', run: () => exec('cut') },
    { id: 'suite.edit.copy', title: 'Copy', run: () => exec('copy') },
    {
      id: 'suite.edit.paste',
      title: 'Paste',
      run: async () => exec('insertText', await navigator.clipboard.readText()),
    },
    {
      id: 'suite.format.font_family',
      title: 'Font',
      run: (_c, a) => exec('fontName', (a as { value: string }).value),
      value: () => (document.queryCommandValue('fontName') || 'Calibri').replace(/["']/g, '').split(',')[0],
    },
    { id: 'suite.format.font_size', title: 'Font Size', run: () => {}, value: () => '11' },
    {
      id: 'suite.format.highlight',
      title: 'Text Highlight Color',
      run: (_c, a) => exec('hiliteColor', colorOf(a)),
    },
    { id: 'suite.format.font_color', title: 'Font Color', run: (_c, a) => exec('foreColor', colorOf(a)) },
    {
      id: 'write.styles.apply',
      title: 'Styles',
      run: (_c, a) => exec('formatBlock', BLOCK_FOR[(a as { value: string }).value] ?? 'p'),
      value: () => {
        const b = String(document.queryCommandValue('formatBlock') || 'p').toLowerCase();
        return b === 'blockquote' ? 'quote' : b === 'div' ? 'p' : b;
      },
    },
    { id: 'write.styles.pane', title: 'Styles Pane', run: () => env.openPane('styles') },
    { id: 'suite.edit.find', title: 'Find', run: () => env.openPane('navigation') },
    {
      id: 'suite.view.navigation_pane',
      title: 'Navigation Pane',
      run: () => env.togglePane('navigation'),
      isChecked: () => false,
    },
    { id: 'suite.review.new_comment', title: 'New Comment', run: () => env.openPane('comments') },
    { id: 'suite.format.font_dialog', title: 'Font…', run: () => env.openFontDialog() },
    { id: 'suite.view.zoom_100', title: '100%', run: () => env.zoom.set(100) },
    {
      id: 'suite.view.mode',
      title: 'Change View',
      run: (_ctx, args) =>
        env.view.set(VIEW_OF[(args as { mode?: string } | undefined)?.mode ?? ''] ?? 'print'),
      value: () => MODE_OF[env.view.get()],
    },
    {
      id: 'write.review.track_changes',
      title: 'Track Changes',
      run: () => env.trackChanges.set(!env.trackChanges.get()),
      isChecked: () => env.trackChanges.get(),
    },
    {
      id: 'suite.view.ruler',
      title: 'Ruler',
      run: () => env.ruler.set(!env.ruler.get()),
      isChecked: () => env.ruler.get(),
    },
    { id: 'suite.file.save', title: 'Save', run: () => {} },
    { id: 'suite.file.share', title: 'Share', run: () => {} },
    { id: 'write.layout.spacing_before', title: 'Spacing Before', run: () => {}, value: () => 0 },
    { id: 'write.layout.spacing_after', title: 'Spacing After', run: () => {}, value: () => 8 },
    { id: 'write.layout.indent_left', title: 'Indent Left', run: () => {}, value: () => 0 },
    { id: 'write.layout.indent_right', title: 'Indent Right', run: () => {}, value: () => 0 },
  ];
  return defs as CommandDefinition<never>[];
}

function colorOf(args: unknown): string {
  const c = (args as { color?: ColorChoice } | undefined)?.color;
  if (!c) return '#000000';
  if (c.type === 'rgb' || c.type === 'theme') return c.hex;
  return c.type === 'none' ? 'transparent' : '#000000';
}

function useDemoSetup(): { registry: CommandRegistry; keymap: Keymap } {
  return useMemo(
    () => ({ registry: createCommandRegistry({ app: 'write' }), keymap: createDefaultKeymap() }),
    [],
  );
}

const PANES: Record<string, Omit<TaskPane, 'id'>> = {
  navigation: {
    title: 'Navigation',
    icon: 'NavigationPane',
    content: (
      <div className="cx-pane">
        <input className="cx-pane__search" placeholder="Search document" aria-label="Search document" />
        <ul className="cx-outline">
          <li className="is-active">Quarterly report</li>
          <li className="cx-outline__l2">Highlights</li>
          <li className="cx-outline__l2">Revenue by region</li>
          <li className="cx-outline__l2">Outlook</li>
        </ul>
      </div>
    ),
  },
  styles: {
    title: 'Styles',
    icon: 'StylesPane',
    content: (
      <ul className="cx-styles">
        {STYLE_ITEMS.map((s) => (
          <li key={s.id}>
            <span>{s.label}</span>
            <span className="cx-styles__mark">¶</span>
          </li>
        ))}
      </ul>
    ),
  },
  comments: {
    title: 'Comments',
    icon: 'ShowComments',
    content: (
      <div className="cx-comment">
        <div className="cx-comment__head">
          <span className="cx-avatar">AL</span>
          <div>
            <strong>Ada Lovelace</strong>
            <div className="cx-muted">Just now</div>
          </div>
        </div>
        <p>Can we add last year’s figures for comparison?</p>
      </div>
    ),
  },
};

function BackstagePages(): Record<string, ReactNode> {
  return {
    home: (
      <div className="cx-bs">
        <h2 className="cx-bs__h2">New</h2>
        <div className="cx-bs__templates">
          {['Blank document', 'Report', 'Letter', 'Résumé'].map((t, i) => (
            <button key={t} type="button" className="cx-bs__template">
              <span className={`cx-bs__thumb cx-bs__thumb--${i}`} />
              <span>{t}</span>
            </button>
          ))}
        </div>
        <h2 className="cx-bs__h2">Recent</h2>
        <ul className="cx-bs__recent">
          {[
            ['Quarterly report.docx', 'Documents', 'Today, 09:12'],
            ['Project brief.docx', 'Desktop', 'Yesterday'],
            ['Meeting notes.docx', 'Documents › Team', 'Mon'],
          ].map(([n, l, d]) => (
            <li key={n}>
              <span className="cx-bs__file">{n}</span>
              <span className="cx-muted">{l}</span>
              <span className="cx-muted">{d}</span>
            </li>
          ))}
        </ul>
      </div>
    ),
    new: <p className="cx-muted">Templates appear here.</p>,
    open: <p className="cx-muted">Recent files, pinned files and this device.</p>,
    info: (
      <dl className="cx-bs__props">
        <dt>Size</dt>
        <dd>18.2 KB</dd>
        <dt>Pages</dt>
        <dd>1</dd>
        <dt>Words</dt>
        <dd>142</dd>
        <dt>Author</dt>
        <dd>You</dd>
      </dl>
    ),
    saveAs: <p className="cx-muted">Choose a location and a format (.docx, .odt, PDF, Markdown).</p>,
    print: <p className="cx-muted">Print preview and printer settings.</p>,
    share: <p className="cx-muted">Share a live link or send a copy.</p>,
    export: <p className="cx-muted">Export as PDF, Markdown or HTML.</p>,
    account: <p className="cx-muted">Theme, language and sign-in.</p>,
  };
}

/** The whole demo window. */
export function ChromeDemo() {
  const { registry, keymap } = useDemoSetup();
  const [title, setTitle] = useState('Quarterly report');
  const [autoSave, setAutoSave] = useState(true);
  const [zoom, setZoom] = useState(100);
  const [view, setView] = useState('print');
  const [track, setTrack] = useState(false);
  const [ruler, setRuler] = useState(true);
  const [panes, setPanes] = useState<string[]>([]);
  const [activePane, setActivePane] = useState<string | undefined>();
  const [fontDialog, setFontDialog] = useState(false);
  const [words, setWords] = useState(0);
  const docRef = useRef<HTMLDivElement>(null);
  const mini = useSelectionMiniToolbar(docRef);
  const state = useRef({ zoom, view, track, ruler });
  state.current = { zoom, view, track, ruler };

  useEffect(() => {
    const issues = validateRibbonSpec(demoRibbon, registry);
    if (issues.length) console.warn('Ribbon spec issues', issues);
    const d = registry.registerMany(
      demoCommands({
        openPane: (id) => {
          setPanes((p) => (p.includes(id) ? p : [...p, id]));
          setActivePane(id);
        },
        togglePane: (id) => setPanes((p) => (p.includes(id) ? p.filter((x) => x !== id) : [...p, id])),
        openFontDialog: () => setFontDialog(true),
        zoom: { get: () => state.current.zoom, set: setZoom },
        view: { get: () => state.current.view, set: setView },
        trackChanges: { get: () => state.current.track, set: setTrack },
        ruler: { get: () => state.current.ruler, set: setRuler },
      }),
    );
    const onSel = () => {
      const sel = window.getSelection();
      const inTable =
        !!sel?.anchorNode &&
        !!(sel.anchorNode instanceof Element ? sel.anchorNode : sel.anchorNode.parentElement)?.closest(
          'table',
        );
      if (registry.context.inTable !== inTable) registry.setContext({ inTable });
      else registry.invalidate();
    };
    document.addEventListener('selectionchange', onSel);
    return () => {
      d.dispose();
      document.removeEventListener('selectionchange', onSel);
    };
  }, [registry]);

  // These drive the toggle states shown in the ribbon.
  useEffect(() => registry.invalidate(), [registry, view, track, ruler, zoom]);

  useEffect(() => {
    const el = docRef.current;
    if (!el) return;
    const count = () => setWords((el.innerText.match(/\S+/g) ?? []).length);
    count();
    el.addEventListener('input', count);
    return () => el.removeEventListener('input', count);
  }, []);

  const params = new URLSearchParams(location.search);
  const taskPanes: TaskPane[] = panes.map((id) => ({ id, ...(PANES[id] as Omit<TaskPane, 'id'>) }));
  const miniToolbar = (
    <>
      <button
        type="button"
        className="kiln-btn kiln-btn--toolbar kiln-btn--icon-only cx-mini-btn"
        aria-label="Bold"
        onClick={() => registry.execute('suite.format.bold')}
      >
        <b>B</b>
      </button>
      <button
        type="button"
        className="kiln-btn kiln-btn--toolbar kiln-btn--icon-only cx-mini-btn"
        aria-label="Italic"
        onClick={() => registry.execute('suite.format.italic')}
      >
        <i>I</i>
      </button>
      <button
        type="button"
        className="kiln-btn kiln-btn--toolbar kiln-btn--icon-only cx-mini-btn"
        aria-label="Underline"
        onClick={() => registry.execute('suite.format.underline')}
      >
        <u>U</u>
      </button>
      <button
        type="button"
        className="kiln-btn kiln-btn--toolbar kiln-btn--sm"
        onClick={() => registry.execute('suite.format.highlight', { color: { type: 'rgb', hex: '#FFE066' } })}
      >
        Highlight
      </button>
      <button
        type="button"
        className="kiln-btn kiln-btn--toolbar kiln-btn--sm"
        onClick={() => registry.execute('suite.review.new_comment')}
      >
        Comment
      </button>
    </>
  );

  return (
    <div className="cx-shell">
      <AppFrame
        app="write"
        registry={registry}
        keymap={keymap}
        title={title}
        onRename={setTitle}
        saveStatus={autoSave ? 'Saved' : 'Not saved'}
        autoSave={{ enabled: autoSave, onChange: setAutoSave }}
        ribbon={demoRibbon}
        defaultTab={params.get('tab') ?? 'home'}
        backstage={BackstagePages()}
        presence={
          <span className="cx-avatar cx-avatar--presence" title="Ada Lovelace is editing">
            AL
          </span>
        }
        rightPane={
          taskPanes.length ? (
            <TaskPaneHost
              panes={taskPanes}
              activeId={activePane}
              onActiveChange={setActivePane}
              onClose={(id) => setPanes((p) => p.filter((x) => x !== id))}
            />
          ) : null
        }
        statusBar={
          <StatusBar
            storageKey="kiln.demo.statusbar"
            left={[
              { id: 'page', label: 'Page Number', content: 'Page 1 of 1' },
              {
                id: 'words',
                label: 'Word Count',
                content: `${words.toLocaleString()} words`,
                onClick: () => {},
              },
              { id: 'lang', label: 'Language', content: 'English (United States)' },
              {
                id: 'track',
                label: 'Track Changes',
                content: `Track Changes: ${track ? 'On' : 'Off'}`,
                onClick: () => setTrack(!track),
              },
            ]}
            views={{
              value: view,
              onChange: setView,
              options: [
                { id: 'read', label: 'Read Mode', icon: 'ReadMode' },
                { id: 'print', label: 'Print Layout', icon: 'PrintLayout' },
                { id: 'web', label: 'Web Layout', icon: 'WebLayout' },
              ],
            }}
            zoom={{
              value: zoom,
              onChange: setZoom,
              fits: [
                { id: 'width', label: 'Page width', onSelect: () => setZoom(130) },
                { id: 'page', label: 'Whole page', onSelect: () => setZoom(70) },
              ],
            }}
          />
        }
      >
        <div className={`cx-canvas cx-canvas--${view}`}>
          {ruler && view === 'print' ? <div className="cx-ruler" aria-hidden="true" /> : null}
          <ContextMenu
            entries={[
              item('suite.edit.cut'),
              item('suite.edit.copy'),
              item('suite.edit.paste'),
              separator(),
              item('suite.format.font_dialog'),
              item('suite.format.paragraph_dialog'),
              separator(),
              item('suite.insert.link'),
              item('suite.review.new_comment'),
            ]}
            miniToolbar={miniToolbar}
          >
            {/* biome-ignore lint/a11y/useSemanticElements: a rich-text editing surface, not a plain <textarea> */}
            <div
              ref={docRef}
              className="cx-page"
              style={{ transform: `scale(${zoom / 100})` }}
              contentEditable
              suppressContentEditableWarning
              spellCheck={false}
              role="textbox"
              aria-multiline="true"
              aria-label="Document"
              tabIndex={0}
            >
              <h1>Quarterly report</h1>
              <p>
                Revenue grew <b>12%</b> quarter on quarter, led by strong demand in EMEA. Operating costs
                stayed flat, so the margin improved to <i>18.4%</i>.
              </p>
              <h2>Highlights</h2>
              <ul>
                <li>Three new enterprise customers signed in September.</li>
                <li>Support response time fell below four hours.</li>
              </ul>
              <table className="cx-table">
                <tbody>
                  <tr>
                    <th>Region</th>
                    <th>Q2</th>
                    <th>Q3</th>
                  </tr>
                  <tr>
                    <td>EMEA</td>
                    <td>4.1</td>
                    <td>4.9</td>
                  </tr>
                  <tr>
                    <td>Americas</td>
                    <td>5.3</td>
                    <td>5.6</td>
                  </tr>
                </tbody>
              </table>
              <p>
                Select text to see the mini toolbar, right-click for the context menu, press Alt for Key Tips.
              </p>
            </div>
          </ContextMenu>
        </div>
      </AppFrame>
      <MiniToolbar anchor={mini.anchor} onDismiss={mini.dismiss}>
        {miniToolbar}
      </MiniToolbar>
      <Dialog open={fontDialog} onOpenChange={setFontDialog}>
        <DialogContent
          title="Font"
          footer={
            <button type="button" className="kiln-btn kiln-btn--primary" onClick={() => setFontDialog(false)}>
              OK
            </button>
          }
        >
          <p className="cx-muted">
            Office’s Font dialog (WP-12 builds the real one); opened by the group launcher and Ctrl+D.
          </p>
        </DialogContent>
      </Dialog>
    </div>
  );
}
