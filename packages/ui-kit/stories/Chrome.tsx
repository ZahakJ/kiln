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
  item('edit.pasteKeepSource'),
  item('edit.pasteMergeFormatting'),
  item('edit.pasteTextOnly'),
  separator(),
  item('edit.pasteSpecial'),
];

/** The demo ribbon: Write's Home tab in full, plus Insert, Layout, Review, View and a contextual set. */
export const demoRibbon: RibbonSpec = ribbon('write', [
  tab(
    'home',
    'Home',
    [
      group('clipboard', 'Clipboard', [
        split('edit.paste', pasteMenu, { size: 'large' }),
        button('edit.cut', { size: 'medium' }),
        button('edit.copy', { size: 'medium' }),
        button('edit.formatPainter', { size: 'medium' }),
      ]),
      group(
        'font',
        'Font',
        [
          row(
            combobox(
              'format.fontFamily',
              FONTS.map((f) => ({ value: f, style: { fontFamily: f } })),
              { width: 132 },
            ),
            combobox(
              'format.fontSize',
              SIZES.map((s) => ({ value: s })),
              { width: 56 },
            ),
            button('format.growFont'),
            button('format.shrinkFont'),
            dropdown(
              'format.changeCase',
              [
                item('format.changeCase', { label: 'Sentence case.' }),
                item('format.changeCase', { label: 'lowercase' }),
                item('format.changeCase', { label: 'UPPERCASE' }),
              ],
              { size: 'small' },
            ),
            button('format.clearFormatting'),
          ),
          row(
            toggle('format.bold'),
            toggle('format.italic'),
            split('format.underline', [item('format.underline'), item('format.doubleUnderline')], {
              size: 'small',
            }),
            toggle('format.strikethrough'),
            toggle('format.subscript'),
            toggle('format.superscript'),
            color('format.highlight', { defaultColor: '#FFE066', allowNone: true }),
            color('format.fontColor', { defaultColor: '#C8352B', allowAuto: true }),
          ),
        ],
        { rows: 2, launcher: { command: 'format.fontDialog' }, icon: 'Type' },
      ),
      group(
        'paragraph',
        'Paragraph',
        [
          row(
            split('format.bullets', [item('format.bullets'), item('format.numbering')], { size: 'small' }),
            split('format.numbering', [item('format.numbering')], { size: 'small' }),
            button('format.multilevelList'),
            button('format.outdent'),
            button('format.indent'),
            button('write.sort'),
            toggle('view.showFormattingMarks'),
          ),
          row(
            toggle('format.alignLeft'),
            toggle('format.alignCenter'),
            toggle('format.alignRight'),
            toggle('format.justify'),
            dropdown(
              'format.lineSpacing',
              [
                item('format.lineSpacing', { label: '1.0', args: { lineSpacing: 1 } }),
                item('format.lineSpacing', { label: '1.15', args: { lineSpacing: 1.15 } }),
                item('format.lineSpacing', { label: '1.5', args: { lineSpacing: 1.5 } }),
                item('format.lineSpacing', { label: '2.0', args: { lineSpacing: 2 } }),
              ],
              { size: 'small' },
            ),
            button('format.shading'),
            button('format.borders'),
          ),
        ],
        { rows: 2, launcher: { command: 'format.paragraphDialog' }, icon: 'Pilcrow' },
      ),
      group(
        'styles',
        'Styles',
        [
          gallery('write.styles', STYLE_ITEMS, {
            columns: 5,
            itemWidth: 84,
            itemHeight: 62,
            footer: [item('format.clearFormatting'), item('write.stylesPane')],
          }),
        ],
        { launcher: { command: 'write.stylesPane' }, icon: 'StylesPane', priority: 2 },
      ),
      group(
        'editing',
        'Editing',
        [
          split('edit.find', [item('edit.find'), item('edit.goTo')], { size: 'medium' }),
          button('edit.replace', { size: 'medium' }),
          dropdown('edit.select', [item('edit.selectAll'), item('arrange.selectionPane')], {
            size: 'medium',
          }),
        ],
        { priority: 1 },
      ),
      group(
        'focus',
        'Focus',
        [
          button('view.focusMode', { size: 'large' }),
          button('view.commandPalette', { size: 'large', label: 'Command Palette' }),
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
        button('write.coverPage', { size: 'medium' }),
        button('write.blankPage', { size: 'medium' }),
        button('write.pageBreak', { size: 'medium' }),
      ]),
      group('tables', 'Tables', [
        dropdown(
          'insert.table',
          [
            item('insert.table', { label: 'Insert Table…' }),
            item('write.convertTextToTable'),
            item('write.pasteMarkdownTable'),
          ],
          { size: 'large' },
        ),
      ]),
      group('illustrations', 'Illustrations', [
        dropdown('insert.pictures', [item('insert.pictures', { label: 'This Device…' })], { size: 'large' }),
        button('insert.shapes', { size: 'medium' }),
        button('insert.icons', { size: 'medium' }),
        button('insert.chart', { size: 'medium' }),
      ]),
      group('links', 'Links', [
        button('insert.link', { size: 'large' }),
        button('write.bookmark', { size: 'medium' }),
        button('write.crossReference', { size: 'medium' }),
      ]),
      group('comments', 'Comments', [button('review.newComment', { size: 'large', label: 'Comment' })]),
      group('headerFooter', 'Header & Footer', [
        button('write.header', { size: 'medium' }),
        button('write.footer', { size: 'medium' }),
        button('write.pageNumber', { size: 'medium' }),
      ]),
      group('text', 'Text', [
        button('insert.textBox', { size: 'large' }),
        button('write.quickParts', { size: 'small' }),
        button('insert.wordArt', { size: 'small' }),
        button('write.dropCap', { size: 'small' }),
        button('insert.dateTime', { size: 'small' }),
      ]),
      group('symbols', 'Symbols', [
        button('insert.equation', { size: 'medium' }),
        button('insert.symbol', { size: 'medium' }),
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
            'layout.margins',
            [
              submenu('Margins', [
                item('layout.margins', { label: 'Normal' }),
                item('layout.margins', { label: 'Narrow' }),
                item('layout.margins', { label: 'Wide' }),
              ]),
            ],
            { size: 'large' },
          ),
          dropdown(
            'layout.orientation',
            [
              item('layout.orientation', { label: 'Portrait' }),
              item('layout.orientation', { label: 'Landscape' }),
            ],
            { size: 'large' },
          ),
          dropdown(
            'layout.size',
            [item('layout.size', { label: 'Letter' }), item('layout.size', { label: 'A4' })],
            { size: 'large' },
          ),
          dropdown(
            'layout.columns',
            [item('layout.columns', { label: 'One' }), item('layout.columns', { label: 'Two' })],
            { size: 'large' },
          ),
          button('layout.breaks', { size: 'medium' }),
          button('layout.lineNumbers', { size: 'medium' }),
          button('layout.hyphenation', { size: 'medium' }),
        ],
        { launcher: { command: 'layout.pageSetup' } },
      ),
      group(
        'paragraph',
        'Paragraph',
        [
          numberInput('layout.indentLeft', { unit: 'cm', size: 'medium', min: 0, max: 50 }),
          numberInput('layout.indentRight', { unit: 'cm', size: 'medium', min: 0, max: 50 }),
          numberInput('layout.spacingBefore', { unit: 'pt', size: 'medium', min: 0, max: 1584, step: 6 }),
          numberInput('layout.spacingAfter', { unit: 'pt', size: 'medium', min: 0, max: 1584, step: 6 }),
        ],
        { rows: 2 },
      ),
      group('arrange', 'Arrange', [
        button('arrange.position', { size: 'large' }),
        button('arrange.wrapText', { size: 'large' }),
        button('arrange.bringForward', { size: 'medium' }),
        button('arrange.sendBackward', { size: 'medium' }),
        button('arrange.selectionPane', { size: 'medium' }),
        button('arrange.align', { size: 'small' }),
        button('arrange.group', { size: 'small' }),
        button('arrange.rotate', { size: 'small' }),
      ]),
    ],
    { keyTip: 'P' },
  ),
  tab(
    'review',
    'Review',
    [
      group('proofing', 'Proofing', [
        button('review.spelling', { size: 'large' }),
        button('review.thesaurus', { size: 'medium' }),
        button('review.wordCount', { size: 'medium' }),
      ]),
      group('comments', 'Comments', [
        button('review.newComment', { size: 'large' }),
        button('review.deleteComment', { size: 'medium' }),
        button('review.previousComment', { size: 'medium' }),
        button('review.nextComment', { size: 'medium' }),
      ]),
      group('tracking', 'Tracking', [
        toggle('review.trackChanges', { size: 'large' }),
        button('review.showMarkup', { size: 'medium' }),
        button('review.reviewingPane', { size: 'medium' }),
      ]),
      group('changes', 'Changes', [
        button('review.acceptChange', { size: 'large' }),
        button('review.rejectChange', { size: 'large' }),
        button('review.previousChange', { size: 'medium' }),
        button('review.nextChange', { size: 'medium' }),
      ]),
      group('history', 'History', [button('file.versionHistory', { size: 'large' })]),
    ],
    { keyTip: 'R' },
  ),
  tab(
    'view',
    'View',
    [
      group('views', 'Views', [
        button('view.readMode', { size: 'large' }),
        button('view.printLayout', { size: 'large' }),
        button('view.webLayout', { size: 'large' }),
      ]),
      group('immersive', 'Immersive', [button('view.focusMode', { size: 'large' })]),
      group('show', 'Show', [
        toggle('view.ruler', { size: 'medium' }),
        toggle('view.gridlines', { size: 'medium' }),
        toggle('view.navigationPane', { size: 'medium' }),
      ]),
      group('zoom', 'Zoom', [
        button('view.zoom', { size: 'large' }),
        button('view.zoom100', { size: 'large' }),
        button('view.onePage', { size: 'medium' }),
        button('view.multiplePages', { size: 'medium' }),
        button('view.pageWidth', { size: 'medium' }),
      ]),
      group('ribbon', 'Ribbon', [
        toggle('view.ribbonClassic', { size: 'medium' }),
        toggle('view.ribbonSimplified', { size: 'medium' }),
        toggle('view.toggleRibbon', { size: 'medium' }),
      ]),
      group('theme', 'Theme', [
        toggle('view.themeLight', { size: 'medium' }),
        toggle('view.themeDark', { size: 'medium' }),
        toggle('view.themeSystem', { size: 'medium' }),
      ]),
    ],
    { keyTip: 'W' },
  ),
  tab(
    'tableDesign',
    'Table Design',
    [
      group('styleOptions', 'Table Style Options', [
        toggle('view.gridlines', { size: 'medium', label: 'Header Row' }),
      ]),
      group('tableStyles', 'Table Styles', [
        button('format.shading', { size: 'large' }),
        button('format.borders', { size: 'large' }),
      ]),
    ],
    { contextual: { set: 'table', label: 'Table Tools' }, when: (ctx) => ctx.inTable === true, keyTip: 'JT' },
  ),
  tab(
    'tableLayout',
    'Layout',
    [
      group('rowsCols', 'Rows & Columns', [
        button('grid.insertCells', { size: 'large', label: 'Insert Above' }),
        button('grid.deleteCells', { size: 'large', label: 'Delete' }),
      ]),
    ],
    { contextual: { set: 'table', label: 'Table Tools' }, when: (ctx) => ctx.inTable === true, keyTip: 'JL' },
  ),
]);

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
    { id: 'format.bold', title: 'Bold', run: () => exec('bold'), isChecked: () => queryState('bold') },
    {
      id: 'format.italic',
      title: 'Italic',
      run: () => exec('italic'),
      isChecked: () => queryState('italic'),
    },
    {
      id: 'format.underline',
      title: 'Underline',
      run: () => exec('underline'),
      isChecked: () => queryState('underline'),
    },
    {
      id: 'format.strikethrough',
      title: 'Strikethrough',
      run: () => exec('strikeThrough'),
      isChecked: () => queryState('strikeThrough'),
    },
    {
      id: 'format.subscript',
      title: 'Subscript',
      run: () => exec('subscript'),
      isChecked: () => queryState('subscript'),
    },
    {
      id: 'format.superscript',
      title: 'Superscript',
      run: () => exec('superscript'),
      isChecked: () => queryState('superscript'),
    },
    {
      id: 'format.alignLeft',
      title: 'Align Left',
      run: () => exec('justifyLeft'),
      isChecked: () => queryState('justifyLeft'),
    },
    {
      id: 'format.alignCenter',
      title: 'Center',
      run: () => exec('justifyCenter'),
      isChecked: () => queryState('justifyCenter'),
    },
    {
      id: 'format.alignRight',
      title: 'Align Right',
      run: () => exec('justifyRight'),
      isChecked: () => queryState('justifyRight'),
    },
    {
      id: 'format.justify',
      title: 'Justify',
      run: () => exec('justifyFull'),
      isChecked: () => queryState('justifyFull'),
    },
    {
      id: 'format.bullets',
      title: 'Bullets',
      run: () => exec('insertUnorderedList'),
      isChecked: () => queryState('insertUnorderedList'),
    },
    {
      id: 'format.numbering',
      title: 'Numbering',
      run: () => exec('insertOrderedList'),
      isChecked: () => queryState('insertOrderedList'),
    },
    { id: 'format.indent', title: 'Increase Indent', run: () => exec('indent') },
    { id: 'format.outdent', title: 'Decrease Indent', run: () => exec('outdent') },
    { id: 'format.clearFormatting', title: 'Clear All Formatting', run: () => exec('removeFormat') },
    { id: 'edit.undo', title: 'Undo', run: () => exec('undo') },
    { id: 'edit.redo', title: 'Redo', run: () => exec('redo') },
    { id: 'edit.cut', title: 'Cut', run: () => exec('cut') },
    { id: 'edit.copy', title: 'Copy', run: () => exec('copy') },
    {
      id: 'edit.paste',
      title: 'Paste',
      run: async () => exec('insertText', await navigator.clipboard.readText()),
    },
    {
      id: 'format.fontFamily',
      title: 'Font',
      run: (_c, a) => exec('fontName', (a as { value: string }).value),
      value: () => (document.queryCommandValue('fontName') || 'Calibri').replace(/["']/g, '').split(',')[0],
    },
    { id: 'format.fontSize', title: 'Font Size', run: () => {}, value: () => '11' },
    {
      id: 'format.highlight',
      title: 'Text Highlight Color',
      run: (_c, a) => exec('hiliteColor', colorOf(a)),
    },
    { id: 'format.fontColor', title: 'Font Color', run: (_c, a) => exec('foreColor', colorOf(a)) },
    {
      id: 'write.styles',
      title: 'Styles',
      run: (_c, a) => exec('formatBlock', BLOCK_FOR[(a as { value: string }).value] ?? 'p'),
      value: () => {
        const b = String(document.queryCommandValue('formatBlock') || 'p').toLowerCase();
        return b === 'blockquote' ? 'quote' : b === 'div' ? 'p' : b;
      },
    },
    { id: 'write.stylesPane', title: 'Styles Pane', run: () => env.openPane('styles') },
    { id: 'edit.find', title: 'Find', run: () => env.openPane('navigation') },
    {
      id: 'view.navigationPane',
      title: 'Navigation Pane',
      run: () => env.togglePane('navigation'),
      isChecked: () => false,
    },
    { id: 'review.newComment', title: 'New Comment', run: () => env.openPane('comments') },
    { id: 'format.fontDialog', title: 'Font…', run: () => env.openFontDialog() },
    { id: 'view.zoom100', title: '100%', run: () => env.zoom.set(100) },
    {
      id: 'view.printLayout',
      title: 'Print Layout',
      run: () => env.view.set('print'),
      isChecked: () => env.view.get() === 'print',
    },
    {
      id: 'view.webLayout',
      title: 'Web Layout',
      run: () => env.view.set('web'),
      isChecked: () => env.view.get() === 'web',
    },
    {
      id: 'view.readMode',
      title: 'Read Mode',
      run: () => env.view.set('read'),
      isChecked: () => env.view.get() === 'read',
    },
    {
      id: 'review.trackChanges',
      title: 'Track Changes',
      run: () => env.trackChanges.set(!env.trackChanges.get()),
      isChecked: () => env.trackChanges.get(),
    },
    {
      id: 'view.ruler',
      title: 'Ruler',
      run: () => env.ruler.set(!env.ruler.get()),
      isChecked: () => env.ruler.get(),
    },
    { id: 'file.save', title: 'Save', run: () => {} },
    { id: 'file.share', title: 'Share', run: () => {} },
    { id: 'layout.spacingBefore', title: 'Spacing Before', run: () => {}, value: () => 0 },
    { id: 'layout.spacingAfter', title: 'Spacing After', run: () => {}, value: () => 8 },
    { id: 'layout.indentLeft', title: 'Indent Left', run: () => {}, value: () => 0 },
    { id: 'layout.indentRight', title: 'Indent Right', run: () => {}, value: () => 0 },
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
        onClick={() => registry.execute('format.bold')}
      >
        <b>B</b>
      </button>
      <button
        type="button"
        className="kiln-btn kiln-btn--toolbar kiln-btn--icon-only cx-mini-btn"
        aria-label="Italic"
        onClick={() => registry.execute('format.italic')}
      >
        <i>I</i>
      </button>
      <button
        type="button"
        className="kiln-btn kiln-btn--toolbar kiln-btn--icon-only cx-mini-btn"
        aria-label="Underline"
        onClick={() => registry.execute('format.underline')}
      >
        <u>U</u>
      </button>
      <button
        type="button"
        className="kiln-btn kiln-btn--toolbar kiln-btn--sm"
        onClick={() => registry.execute('format.highlight', { color: { type: 'rgb', hex: '#FFE066' } })}
      >
        Highlight
      </button>
      <button
        type="button"
        className="kiln-btn kiln-btn--toolbar kiln-btn--sm"
        onClick={() => registry.execute('review.newComment')}
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
              item('edit.cut'),
              item('edit.copy'),
              item('edit.paste'),
              separator(),
              item('format.fontDialog'),
              item('format.paragraphDialog'),
              separator(),
              item('insert.link'),
              item('review.newComment'),
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
