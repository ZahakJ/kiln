/** Kiln Notes commands and ribbon: PLAN.md §1.4 "Notes" (Tier 2, implemented by WP-86). */
import {
  mitem,
  type RibbonData,
  rbutton,
  rcolor,
  rcombo,
  rdropdown,
  rgroup,
  rrow,
  rtab,
  rtoggle,
} from '../ribbon.ts';
import { c, NOTES, section } from './define.ts';
import {
  clipboardGroup,
  FONT_FAMILY_OPTIONS,
  FONT_SIZE_OPTIONS,
  themeGroup,
  zoomMenu,
} from './ribbon-shared.ts';

const N = { apps: NOTES };

export const notesCommands = [
  ...section('Notes', N, [
    c('notes.notes.tag_to_do', 'To Do Tag', 'TagToDo', { keyTip: '1' }),
    c('notes.notes.tag_important', 'Important Tag', 'TagImportant', { keyTip: '2' }),
    c('notes.notes.tag_question', 'Question Tag', 'TagQuestion', { keyTip: '3' }),
    c('notes.notes.find_tags', 'Find Tags', 'Tags', { keyTip: 'G', synonyms: ['all tags', 'tag summary'] }),
    c('notes.notes.attach_file', 'File Attachment', 'AttachFile'),
    c('notes.notes.page_color', 'Page Color', 'PageColor'),
    c('notes.notes.rule_lines', 'Rule Lines', 'RuleLines'),
    c('notes.notes.new_page', 'New Page', 'FilePlus'),
    c('notes.notes.new_section', 'New Section', 'FolderPlus'),
    c('notes.notes.navigation_panes', 'Navigation Panes', 'PanelLeft'),
  ]),
  ...section('Notes ▸ Styles', N, [
    c('notes.styles.heading_1', 'Heading 1', 'Heading1'),
    c('notes.styles.heading_2', 'Heading 2', 'Heading2'),
    c('notes.styles.normal', 'Normal', 'Pilcrow'),
  ]),
  ...section('Notes ▸ Draw', { apps: NOTES, tier: 'P2' }, [
    c('notes.draw.pen', 'Pen', 'PenTool'),
    c('notes.draw.highlighter', 'Highlighter', 'HighlighterTool'),
    c('notes.draw.eraser', 'Eraser', 'Eraser'),
    c('notes.draw.lasso', 'Lasso Select', 'LassoSelect'),
  ]),
];

/** Kiln Notes' ribbon. */
export const notesRibbon: RibbonData = {
  app: 'notes',
  tabs: [
    rtab(
      'home',
      'Home',
      [
        clipboardGroup(),
        rgroup(
          'basicText',
          'Basic Text',
          [
            rrow(
              rcombo('suite.format.font_family', FONT_FAMILY_OPTIONS, {
                placeholder: 'Font',
                width: 120,
                keyTip: 'FF',
                allowCustomValue: true,
              }),
              rcombo('suite.format.font_size', FONT_SIZE_OPTIONS, {
                placeholder: 'Size',
                width: 56,
                keyTip: 'FS',
                allowCustomValue: true,
                accepts: 'number',
              }),
            ),
            rrow(
              rtoggle('suite.format.bold', { keyTip: '1' }),
              rtoggle('suite.format.italic', { keyTip: '2' }),
              rtoggle('suite.format.underline', { keyTip: '3' }),
              rcolor('suite.format.highlight', { defaultColor: '#FFE066', allowNone: true, keyTip: 'I' }),
              rtoggle('suite.format.bullets', { keyTip: 'U' }),
              rtoggle('suite.format.numbering', { keyTip: 'N' }),
            ),
          ],
          { rows: 2, launcher: { command: 'suite.format.font_dialog' }, icon: 'Type', priority: 3 },
        ),
        rgroup('styles', 'Styles', [
          rbutton('notes.styles.heading_1', { size: 'small', keyTip: 'H1' }),
          rbutton('notes.styles.heading_2', { size: 'small', keyTip: 'H2' }),
          rbutton('notes.styles.normal', { size: 'small', keyTip: 'NS' }),
        ]),
        rgroup('tags', 'Tags', [
          rtoggle('notes.notes.tag_to_do', { size: 'medium', label: 'To do' }),
          rtoggle('notes.notes.tag_important', { size: 'medium', label: 'Important' }),
          rtoggle('notes.notes.tag_question', { size: 'medium', label: 'Question' }),
          rbutton('notes.notes.find_tags', { size: 'large', label: 'All tags' }),
        ]),
      ],
      { keyTip: 'H' },
    ),
    rtab(
      'insert',
      'Insert',
      [
        rgroup('tables', 'Tables', [rbutton('suite.insert.table', { size: 'large', keyTip: 'T' })]),
        rgroup('files', 'Files', [rbutton('notes.notes.attach_file', { size: 'large', keyTip: 'F' })]),
        rgroup('images', 'Images', [rbutton('suite.insert.pictures', { size: 'large', keyTip: 'P' })]),
        rgroup('links', 'Links', [rbutton('suite.insert.link', { size: 'large', keyTip: 'I' })]),
        rgroup('timeStamp', 'Time Stamp', [
          rbutton('suite.insert.date_time', { size: 'large', keyTip: 'D' }),
        ]),
        rgroup('kiln', 'Kiln', [rbutton('suite.insert.code_block', { size: 'large', keyTip: 'ZK' })]),
        rgroup('symbols', 'Symbols', [
          rbutton('suite.insert.equation', { size: 'large', keyTip: 'E', tier: 'P1' }),
        ]),
      ],
      { keyTip: 'N' },
    ),
    rtab(
      'draw',
      'Draw',
      [
        rgroup('tools', 'Tools', [
          rbutton('notes.draw.lasso', { size: 'large' }),
          rbutton('notes.draw.eraser', { size: 'large' }),
          rbutton('notes.draw.pen', { size: 'large' }),
          rbutton('notes.draw.highlighter', { size: 'large' }),
        ]),
      ],
      { keyTip: 'D', tier: 'P2' },
    ),
    rtab(
      'view',
      'View',
      [
        rgroup('views', 'Views', [rtoggle('notes.notes.navigation_panes', { size: 'large', keyTip: 'N' })]),
        rgroup('pageSetup', 'Page Setup', [
          rdropdown(
            'notes.notes.page_color',
            ['None', 'Blue', 'Yellow', 'Green', 'Red', 'Purple'].map((color) =>
              mitem('notes.notes.page_color', { label: color, args: { color: color.toLowerCase() } }),
            ),
            { size: 'large', keyTip: 'PC' },
          ),
          rdropdown(
            'notes.notes.rule_lines',
            [
              mitem('notes.notes.rule_lines', { label: 'Narrow Ruled', args: { lines: 'narrow' } }),
              mitem('notes.notes.rule_lines', { label: 'College Ruled', args: { lines: 'college' } }),
              mitem('notes.notes.rule_lines', { label: 'Small Grid', args: { lines: 'grid' } }),
              mitem('notes.notes.rule_lines', { label: 'Dots', args: { lines: 'dots' } }),
              mitem('notes.notes.rule_lines', { label: 'None', args: { lines: 'none' } }),
            ],
            { size: 'large', keyTip: 'R' },
          ),
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
        ]),
        themeGroup(),
      ],
      { keyTip: 'W' },
    ),
  ],
};
