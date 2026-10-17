/** Commands shared by the suite: File, editing basics, view chrome, review basics (PLAN.md §1.5, §1.7). */
import { c, EDITORS, section, TEXT_APPS, WRITE_DECK, WRITE_GRID } from './define.ts';

export const fileCommands = section('File', {}, [
  c('ui.backstage.open', 'File', 'Menu', {
    description: 'Open the File menu: new, open, save, print, share and export.',
    keyTip: 'F',
  }),
  c('ui.backstage.home', 'Home', 'House', { description: 'Recent files, pinned files and templates.' }),
  c('suite.file.new', 'New', 'FilePlus', {
    description: 'Create a new document from a blank page or a template.',
  }),
  c('suite.file.open', 'Open', 'FolderOpen', {
    description: 'Open a document from this device or a recent location.',
  }),
  c('ui.backstage.info', 'Info', 'Info', {
    description: 'Document properties, protection and compatibility.',
  }),
  c('suite.file.save', 'Save', 'Save', { description: 'Save the document.' }),
  c('suite.file.save_as', 'Save As', 'SaveAll', {
    description: 'Save under a new name, location or format.',
  }),
  c('suite.file.save_copy', 'Save a Copy', 'Copy', {
    description: 'Save a copy and keep working on the original.',
  }),
  c('suite.file.print', 'Print', 'Printer', { description: 'Print with a preview, or print to PDF.' }),
  c('suite.file.share', 'Share', 'Share2', {
    description: 'Share a link for live collaboration or send a copy.',
  }),
  c('suite.file.export', 'Export', 'FileOutput', {
    description: 'Export to PDF, Markdown, HTML or another format.',
  }),
  c('suite.file.export_pdf', 'Export as PDF', 'FileText', { officeName: 'Create PDF/XPS Document' }),
  c('suite.file.close', 'Close', 'X', { description: 'Close the document.' }),
  c('ui.backstage.account', 'Account', 'CircleUserRound', { description: 'Theme, language and sign-in.' }),
  c('ui.options.open', 'Options', 'Settings', {
    description: 'Preferences: general, display, saving, language and keyboard shortcuts.',
  }),
  c('suite.file.version_history', 'Version History', 'VersionHistory', {
    description: 'See and restore earlier versions of this document.',
    apps: EDITORS,
  }),
  c('suite.file.rename', 'Rename', 'PencilLine', { description: 'Rename the document.' }),
  c('suite.file.autosave', 'AutoSave', 'RefreshCw', {
    description: 'Save changes automatically as you work.',
  }),
  c('suite.file.check_compatibility', 'Check Compatibility', 'ShieldCheck', {
    officeName: 'Check Compatibility',
    apps: EDITORS,
    description: 'List features that will not survive saving in an Office format.',
  }),
  c('suite.file.protect', 'Protect Document', 'Lock', { apps: EDITORS, tier: 'P1' }),
]);

export const editCommands = section('Edit', { apps: EDITORS }, [
  c('suite.edit.undo', 'Undo', 'Undo2'),
  c('suite.edit.redo', 'Redo', 'Redo2'),
  c('suite.edit.repeat', 'Repeat', 'Repeat', { description: 'Repeat the last action.' }),
  c('suite.edit.cut', 'Cut', 'Scissors', { keyTip: 'X' }),
  c('suite.edit.copy', 'Copy', 'Copy', { keyTip: 'C' }),
  c('suite.edit.paste', 'Paste', 'Paste', { keyTip: 'V' }),
  c('suite.edit.paste_special', 'Paste Special…', 'PasteSpecial', {
    description: 'Choose how pasted content is formatted.',
  }),
  c('suite.edit.paste_keep_source', 'Keep Source Formatting', 'PasteKeepSource'),
  c('suite.edit.paste_merge_formatting', 'Merge Formatting', 'PasteMergeFormatting'),
  c('suite.edit.paste_text_only', 'Keep Text Only', 'PasteTextOnly', {
    synonyms: ['paste plain text', 'paste without formatting'],
  }),
  // Write and Notes select with the core's `write.caret.select_all`.
  c('suite.edit.select_all', 'Select All', 'TextSelect', { apps: ['grid', 'deck'] }),
  c('suite.edit.find', 'Find', 'Search', { keyTip: 'FD' }),
  c('suite.edit.replace', 'Replace', 'Replace', { keyTip: 'R' }),
  c('suite.edit.go_to', 'Go To', 'GoTo', { description: 'Jump to a page, cell, slide or bookmark.' }),
  c('suite.edit.select', 'Select', 'SelectObjects', { keyTip: 'SL' }),
  c('ui.edit.cancel', 'Cancel', 'X', {
    palette: false,
    description: 'Cancel the current action or close the pane.',
  }),
  c('suite.edit.format_painter', 'Format Painter', 'FormatPainter', {
    apps: ['write', 'grid', 'deck', 'notes'],
    keyTip: 'FP',
    description: 'Copy formatting from one place and apply it to another.',
  }),
  c('suite.format.copy_formatting', 'Copy Formatting', 'FormatPainter', { apps: WRITE_DECK }),
  c('suite.format.paste_formatting', 'Paste Formatting', 'FormatPainter', { apps: WRITE_DECK }),
]);

export const fontCommands = section('Home ▸ Font', { apps: EDITORS }, [
  c('suite.format.font_family', 'Font', 'Type', { keyTip: 'FF' }),
  c('suite.format.font_size', 'Font Size', 'ALargeSmall', { keyTip: 'FS' }),
  c('suite.format.grow_font', 'Increase Font Size', 'GrowFont', {
    keyTip: 'FG',
    officeName: 'Grow Font',
  }),
  c('suite.format.shrink_font', 'Decrease Font Size', 'ShrinkFont', {
    keyTip: 'FK',
    officeName: 'Shrink Font',
  }),
  c('suite.format.change_case', 'Change Case', 'ChangeCase', { apps: WRITE_DECK, keyTip: '7' }),
  c('suite.format.clear_formatting', 'Clear All Formatting', 'ClearFormatting', {
    apps: TEXT_APPS,
    keyTip: 'E',
  }),
  c('suite.format.bold', 'Bold', 'Bold', { keyTip: '1' }),
  c('suite.format.italic', 'Italic', 'Italic', { keyTip: '2' }),
  c('suite.format.underline', 'Underline', 'Underline', { keyTip: '3' }),
  c('suite.format.double_underline', 'Double Underline', 'DoubleUnderline', { apps: WRITE_DECK }),
  c('suite.format.strikethrough', 'Strikethrough', 'Strikethrough', { keyTip: '4' }),
  c('suite.format.subscript', 'Subscript', 'Subscript', { apps: WRITE_DECK, keyTip: '5' }),
  c('suite.format.superscript', 'Superscript', 'Superscript', { apps: WRITE_DECK, keyTip: '6' }),
  c('suite.format.all_caps', 'All Caps', 'CaseUpper', { apps: ['write'] }),
  c('suite.format.text_effects', 'Text Effects', 'TextEffects', { apps: WRITE_DECK, tier: 'P2' }),
  c('suite.format.highlight', 'Text Highlight Color', 'TextHighlight', { apps: TEXT_APPS, keyTip: 'I' }),
  c('suite.format.font_color', 'Font Color', 'FontColor', { keyTip: 'FC' }),
  c('suite.format.font_dialog', 'Font…', 'Type', {
    apps: TEXT_APPS,
    description: 'Open the Font dialog.',
    keyTip: 'FN',
  }),
  c('suite.format.character_spacing', 'Character Spacing', 'CharacterSpacing', { apps: ['deck'] }),
  c('suite.format.text_shadow', 'Text Shadow', 'TextShadow', { apps: ['deck'] }),
]);

export const paragraphCommands = section('Home ▸ Paragraph', { apps: TEXT_APPS }, [
  c('suite.format.bullets', 'Bullets', 'BulletList', { keyTip: 'U' }),
  c('suite.format.numbering', 'Numbering', 'NumberedList', { keyTip: 'N' }),
  c('suite.format.multilevel_list', 'Multilevel List', 'MultilevelList', { apps: ['write'], keyTip: 'M' }),
  c('suite.format.outdent', 'Decrease Indent', 'DecreaseIndent', { apps: EDITORS, keyTip: 'AO' }),
  c('suite.format.indent', 'Increase Indent', 'IncreaseIndent', { apps: EDITORS, keyTip: 'AI' }),
  c('suite.format.align_left', 'Align Left', 'AlignLeft', { apps: EDITORS, keyTip: 'AL' }),
  c('suite.format.align_center', 'Center', 'AlignCenter', {
    apps: EDITORS,
    keyTip: 'AC',
    synonyms: ['centre'],
  }),
  c('suite.format.align_right', 'Align Right', 'AlignRight', { apps: EDITORS, keyTip: 'AR' }),
  c('suite.format.justify', 'Justify', 'AlignJustify', { keyTip: 'AJ' }),
  c('suite.format.line_spacing', 'Line and Paragraph Spacing', 'LineSpacing', { keyTip: 'K' }),
  c('suite.format.paragraph_dialog', 'Paragraph…', 'Pilcrow', { description: 'Open the Paragraph dialog.' }),
]);

export const insertCommonCommands = section('Insert', { apps: EDITORS }, [
  c('suite.insert.link', 'Link', 'Link', { officeName: 'Hyperlink' }),
  c('suite.insert.date', 'Insert Date', 'CalendarDays', { apps: WRITE_GRID }),
  c('suite.insert.time', 'Insert Time', 'Clock', { apps: WRITE_GRID }),
  c('suite.insert.date_time', 'Date & Time', 'DateTime', { apps: TEXT_APPS }),
  c('suite.insert.table', 'Table', 'InsertTable', { apps: ['write', 'deck', 'notes'] }),
  c('suite.insert.pictures', 'Pictures', 'Image'),
  c('suite.insert.shapes', 'Shapes', 'Shapes'),
  c('suite.insert.icons', 'Icons', 'Smile'),
  c('suite.chart.insert', 'Chart', 'ChartColumn'),
  c('suite.insert.text_box', 'Text Box', 'TextBox'),
  c('suite.insert.symbol', 'Symbol', 'SymbolOmega'),
  c('suite.insert.equation', 'Equation', 'Equation', { tier: 'P1' }),
  c('suite.insert.header_footer', 'Header & Footer', 'Header'),
  c('suite.insert.word_art', 'WordArt', 'WordArt', { tier: 'P2' }),
  c('suite.insert.object', 'Object', 'Box', { tier: 'P2' }),
  c('suite.insert.code_block', 'Code Block', 'CodeBlock', {
    apps: ['write', 'deck', 'notes'],
    description: 'A block of code with syntax highlighting.',
  }),
  c('suite.insert.screenshot', 'Screenshot', 'Screenshot', { apps: WRITE_DECK }),
]);

export const reviewCommonCommands = section('Review', { apps: EDITORS }, [
  c('suite.review.spelling', 'Spelling & Grammar', 'Spelling', { officeName: 'Spelling & Grammar' }),
  c('suite.review.thesaurus', 'Thesaurus', 'Thesaurus', { apps: ['write'], tier: 'P2' }),
  c('suite.review.word_count', 'Word Count', 'WordCount', { apps: TEXT_APPS }),
  c('suite.review.new_comment', 'New Comment', 'NewComment'),
  c('suite.review.delete_comment', 'Delete Comment', 'DeleteComment'),
  c('suite.review.previous_comment', 'Previous Comment', 'PreviousComment'),
  c('suite.review.next_comment', 'Next Comment', 'NextComment'),
  c('suite.review.show_comments', 'Show Comments', 'ShowComments'),
  c('suite.review.translate', 'Translate', 'Translate', { apps: ['write'] }),
  c('suite.review.language', 'Language', 'Language'),
  c('suite.review.compare', 'Compare', 'CompareDocuments', { apps: WRITE_DECK, tier: 'P2' }),
]);

export const viewCommonCommands = section('View', {}, [
  c('ui.palette.open', 'Command Palette', 'CommandPalette', {
    officeName: 'Search (Tell Me)',
    description: 'Find any command by name, including its Office name.',
  }),
  c('ui.keytips.show', 'Key Tips', 'Keyboard', {
    description: 'Show letters for reaching every ribbon command from the keyboard.',
    palette: false,
  }),
  c('ui.ribbon.collapse', 'Collapse the Ribbon', 'PanelTopClose', {
    officeName: 'Collapse the Ribbon',
    description: 'Show only the tab names to make more room.',
  }),
  c('ui.ribbon.classic', 'Classic Ribbon', 'PanelTop', { synonyms: ['ribbon layout', 'full ribbon'] }),
  c('ui.ribbon.simplified', 'Simplified Ribbon', 'PanelTopDashed', {
    synonyms: ['single line ribbon', 'compact ribbon'],
  }),
  c('ui.shortcuts.show', 'Keyboard Shortcuts', 'Keyboard', {
    description: 'A cheat sheet of the shortcuts for this app.',
  }),
  c('suite.view.zoom', 'Zoom', 'ZoomIn', { apps: EDITORS }),
  c('suite.view.zoom_in', 'Zoom In', 'ZoomIn', { apps: EDITORS }),
  c('suite.view.zoom_out', 'Zoom Out', 'ZoomOut', { apps: EDITORS }),
  c('suite.view.zoom_100', '100%', 'Zoom100', { apps: EDITORS }),
  c('suite.view.ruler', 'Ruler', 'Ruler', { apps: ['write', 'grid', 'deck'] }),
  c('suite.view.gridlines', 'Gridlines', 'Gridlines', { apps: ['write', 'grid', 'deck'] }),
  c('suite.view.new_window', 'New Window', 'NewWindow', { apps: EDITORS }),
  c('ui.theme.light', 'Light Theme', 'Sun', { synonyms: ['light mode'] }),
  c('ui.theme.dark', 'Dark Theme', 'Moon', { synonyms: ['dark mode', 'night mode'] }),
  c('ui.theme.system', 'Use System Theme', 'Monitor'),
  c('ui.theme.high_contrast', 'High Contrast Theme', 'Contrast', { synonyms: ['accessibility', 'contrast'] }),
  c('ui.macros.explain', 'Macros', 'Macros', {
    apps: ['write', 'grid', 'deck'],
    synonyms: ['vba', 'visual basic', 'record macro'],
    description: 'Macros are never run in Kiln; they are kept in the file so it is not damaged.',
  }),
  c('ui.pane.close', 'Close Pane', 'PanelRightClose', { palette: false }),
  c('suite.help.assistant', 'Ask the Assistant', 'Sparkles', {
    tier: 'P1',
    synonyms: ['ai', 'copilot', 'summarise', 'rewrite'],
  }),
]);

/**
 * Grid's keyboard navigation (PLAN.md §1.7: Ctrl+Arrows jump to the edge of the data). Write moves
 * the caret with the core's `write.caret.move`, so it needs no ids of its own here.
 */
export const navigationCommands = section('Navigation', { apps: ['grid'] }, [
  ...(['up', 'down', 'left', 'right'] as const).flatMap((d) => [
    c(`grid.nav.jump_${d}`, `Jump ${d}`, undefined, {
      palette: false,
      description: 'Move to the edge of the data region.',
    }),
    c(`grid.nav.extend_${d}`, `Extend selection ${d}`, undefined, { palette: false }),
  ]),
  c('grid.nav.first_cell', 'Go to A1', undefined, { palette: false }),
  c('grid.nav.last_cell', 'Go to the last used cell', undefined, { palette: false }),
]);

/** View commands the core implements that the ribbon reaches through other items (zoom slider, view buttons). */
export const coreViewCommands = section('View', { apps: EDITORS }, [
  c('suite.view.mode', 'Change View', 'LayoutPanelTop', {
    palette: false,
    description: 'Switch between the views of the document (Print Layout, Normal, Slide Sorter…).',
  }),
  c('suite.view.repaint', 'Repaint', 'RefreshCw', { palette: false }),
]);

/** Diagnostics: round trips to the core (bridge tests, the About box). */
export const devCommands = section('Diagnostics', {}, [
  c('suite.dev.ping', 'Ping the Core', 'Activity', { palette: false }),
  c('suite.dev.echo', 'Echo', 'Activity', { palette: false }),
]);

/** Chart editing (WP-73): the Chart Design and Format contextual tabs drive these. */
export const chartCommands = section('Chart', { apps: ['write', 'grid', 'deck'] }, [
  c('suite.chart.change_type', 'Change Chart Type', 'ChartColumn', { officeName: 'Change Chart Type' }),
  c('suite.chart.switch_row_column', 'Switch Row/Column', 'ArrowLeftRight'),
  c('suite.chart.select_data', 'Select Data', 'TableProperties'),
  c('suite.chart.element', 'Add Chart Element', 'ChartBarIncreasing', {
    synonyms: ['axis title', 'gridlines'],
  }),
  c('suite.chart.legend', 'Legend', 'List', { palette: false }),
  c('suite.chart.data_labels', 'Data Labels', 'Tag', { palette: false }),
  c('suite.chart.quick_layout', 'Quick Layout', 'LayoutDashboard'),
  c('suite.chart.style', 'Chart Styles', 'Palette'),
  c('suite.chart.colors', 'Change Colors', 'Paintbrush'),
  c('suite.chart.title', 'Chart Title', 'Heading', { palette: false }),
  c('suite.chart.move', 'Move Chart', 'Move'),
  c('suite.chart.format_series', 'Format Data Series', 'PaintBucket', { palette: false }),
  c('suite.chart.format_axis', 'Format Axis', 'Ruler', { palette: false }),
]);

/** Contextual tabs (PLAN.md §1.4): tables, pictures, shapes, headers and footers. */
export const contextualCommands = [
  ...section('Table Design', { apps: ['write', 'grid', 'deck'] }, [
    c('suite.table.styles', 'Table Styles', 'Table', { keyTip: 'S' }),
    c('suite.table.header_row', 'Header Row', 'PanelTop', { keyTip: 'H' }),
    c('suite.table.total_row', 'Total Row', 'PanelBottom', { keyTip: 'T' }),
    c('suite.table.banded_rows', 'Banded Rows', 'Rows3', { keyTip: 'R' }),
    c('suite.table.banded_columns', 'Banded Columns', 'Columns3', { keyTip: 'C' }),
    c('suite.table.first_column', 'First Column', 'PanelLeft', { keyTip: 'F' }),
    c('suite.table.shading', 'Shading', 'Shading', { keyTip: 'P' }),
    c('suite.table.borders', 'Borders', 'BordersAll', { keyTip: 'B' }),
  ]),
  ...section('Table Layout', { apps: ['write', 'deck'] }, [
    c('suite.table.insert_row_above', 'Insert Above', 'BetweenHorizontalStart', { keyTip: 'A' }),
    c('suite.table.insert_row_below', 'Insert Below', 'BetweenHorizontalEnd', { keyTip: 'BE' }),
    c('suite.table.insert_column_left', 'Insert Left', 'BetweenVerticalStart', { keyTip: 'L' }),
    c('suite.table.insert_column_right', 'Insert Right', 'BetweenVerticalEnd', { keyTip: 'R' }),
    c('suite.table.delete', 'Delete', 'TableCellsSplit', { keyTip: 'D' }),
    c('suite.table.merge_cells', 'Merge Cells', 'TableCellsMerge', { keyTip: 'M' }),
    c('suite.table.split_cells', 'Split Cells', 'TableCellsSplit', { keyTip: 'P' }),
    c('suite.table.distribute_rows', 'Distribute Rows', 'Rows3', { keyTip: 'HR' }),
    c('suite.table.distribute_columns', 'Distribute Columns', 'Columns3', { keyTip: 'HC' }),
    c('suite.table.cell_alignment', 'Alignment', 'AlignCenter', { keyTip: 'AL' }),
    c('suite.table.repeat_header_rows', 'Repeat Header Rows', 'Repeat', { apps: ['write'], keyTip: 'RH' }),
    c('suite.table.convert_to_text', 'Convert to Text', 'TextCursorInput', { apps: ['write'], keyTip: 'V' }),
    c('suite.table.formula', 'Formula', 'Sigma', { apps: ['write'], keyTip: 'UL' }),
  ]),
  ...section('Picture Format', { apps: ['write', 'grid', 'deck'] }, [
    c('suite.picture.corrections', 'Corrections', 'SunMedium', { keyTip: 'R' }),
    c('suite.picture.crop', 'Crop', 'Crop', { keyTip: 'C' }),
    c('suite.picture.alt_text', 'Alt Text', 'Accessibility', {
      keyTip: 'AT',
      synonyms: ['alternative text'],
    }),
    c('suite.picture.reset', 'Reset Picture', 'RotateCcw', { keyTip: 'Q' }),
  ]),
  ...section('Shape Format', { apps: ['write', 'grid', 'deck'] }, [
    c('suite.shape.styles', 'Shape Styles', 'Shapes', { keyTip: 'S' }),
    c('suite.shape.fill', 'Shape Fill', 'PaintBucket', { keyTip: 'SF' }),
    c('suite.shape.outline', 'Shape Outline', 'PenLine', { keyTip: 'SO' }),
    c('suite.shape.effects', 'Shape Effects', 'Sparkles', { keyTip: 'SE' }),
    c('suite.shape.height', 'Shape Height', 'MoveVertical', { keyTip: 'H' }),
    c('suite.shape.width', 'Shape Width', 'MoveHorizontal', { keyTip: 'W' }),
  ]),
  ...section('Header & Footer', { apps: ['write'] }, [
    c('write.header_footer.different_first_page', 'Different First Page', 'FileText', { keyTip: 'A' }),
    c('write.header_footer.different_odd_even', 'Different Odd & Even Pages', 'Files', { keyTip: 'O' }),
    c('write.header_footer.link_to_previous', 'Link to Previous', 'Link2', { keyTip: 'K' }),
    c('write.header_footer.close', 'Close Header and Footer', 'X', { keyTip: 'C' }),
  ]),
];
