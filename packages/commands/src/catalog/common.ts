/** Commands shared by the suite: File, editing basics, view chrome, review basics (PLAN.md §1.5, §1.7). */
import { c, EDITORS, section, TEXT_APPS, WRITE_DECK, WRITE_GRID } from './define.ts';

export const fileCommands = section('File', {}, [
  c('file.backstage', 'File', 'Menu', {
    description: 'Open the File menu: new, open, save, print, share and export.',
    keyTip: 'F',
  }),
  c('file.home', 'Home', 'House', { description: 'Recent files, pinned files and templates.' }),
  c('file.new', 'New', 'FilePlus', { description: 'Create a new document from a blank page or a template.' }),
  c('file.open', 'Open', 'FolderOpen', {
    description: 'Open a document from this device or a recent location.',
  }),
  c('file.info', 'Info', 'Info', { description: 'Document properties, protection and compatibility.' }),
  c('file.save', 'Save', 'Save', { description: 'Save the document.' }),
  c('file.saveAs', 'Save As', 'SaveAll', { description: 'Save under a new name, location or format.' }),
  c('file.saveCopy', 'Save a Copy', 'Copy', { description: 'Save a copy and keep working on the original.' }),
  c('file.print', 'Print', 'Printer', { description: 'Print with a preview, or print to PDF.' }),
  c('file.share', 'Share', 'Share2', { description: 'Share a link for live collaboration or send a copy.' }),
  c('file.export', 'Export', 'FileOutput', {
    description: 'Export to PDF, Markdown, HTML or another format.',
  }),
  c('file.exportPdf', 'Export as PDF', 'FileText', { officeName: 'Create PDF/XPS Document' }),
  c('file.close', 'Close', 'X', { description: 'Close the document.' }),
  c('file.account', 'Account', 'CircleUserRound', { description: 'Theme, language and sign-in.' }),
  c('file.options', 'Options', 'Settings', {
    description: 'Preferences: general, display, saving, language and keyboard shortcuts.',
  }),
  c('file.versionHistory', 'Version History', 'VersionHistory', {
    description: 'See and restore earlier versions of this document.',
    apps: EDITORS,
  }),
  c('file.rename', 'Rename', 'PencilLine', { description: 'Rename the document.' }),
  c('file.toggleAutoSave', 'AutoSave', 'RefreshCw', {
    description: 'Save changes automatically as you work.',
  }),
  c('file.compatibilityChecker', 'Check Compatibility', 'ShieldCheck', {
    officeName: 'Check Compatibility',
    apps: EDITORS,
    description: 'List features that will not survive saving in an Office format.',
  }),
  c('file.protect', 'Protect Document', 'Lock', { apps: EDITORS, tier: 'P1' }),
]);

export const editCommands = section('Edit', { apps: EDITORS }, [
  c('edit.undo', 'Undo', 'Undo2'),
  c('edit.redo', 'Redo', 'Redo2'),
  c('edit.repeat', 'Repeat', 'Repeat', { description: 'Repeat the last action.' }),
  c('edit.cut', 'Cut', 'Scissors', { keyTip: 'X' }),
  c('edit.copy', 'Copy', 'Copy', { keyTip: 'C' }),
  c('edit.paste', 'Paste', 'Paste', { keyTip: 'V' }),
  c('edit.pasteSpecial', 'Paste Special…', 'PasteSpecial', {
    description: 'Choose how pasted content is formatted.',
  }),
  c('edit.pasteKeepSource', 'Keep Source Formatting', 'PasteKeepSource', { apps: WRITE_DECK }),
  c('edit.pasteMergeFormatting', 'Merge Formatting', 'PasteMergeFormatting', { apps: WRITE_DECK }),
  c('edit.pasteTextOnly', 'Keep Text Only', 'PasteTextOnly', {
    apps: WRITE_DECK,
    synonyms: ['paste plain text', 'paste without formatting'],
  }),
  c('edit.selectAll', 'Select All', 'TextSelect'),
  c('edit.find', 'Find', 'Search', { keyTip: 'FD' }),
  c('edit.replace', 'Replace', 'Replace', { keyTip: 'R' }),
  c('edit.goTo', 'Go To', 'GoTo', { description: 'Jump to a page, cell, slide or bookmark.' }),
  c('edit.select', 'Select', 'SelectObjects', { keyTip: 'SL' }),
  c('edit.cancel', 'Cancel', 'X', {
    palette: false,
    description: 'Cancel the current action or close the pane.',
  }),
  c('edit.formatPainter', 'Format Painter', 'FormatPainter', {
    apps: ['write', 'grid', 'deck', 'notes'],
    keyTip: 'FP',
    description: 'Copy formatting from one place and apply it to another.',
  }),
  c('format.copyFormatting', 'Copy Formatting', 'FormatPainter', { apps: WRITE_DECK }),
  c('format.pasteFormatting', 'Paste Formatting', 'FormatPainter', { apps: WRITE_DECK }),
]);

export const fontCommands = section('Home ▸ Font', { apps: EDITORS }, [
  c('format.fontFamily', 'Font', 'Type', { keyTip: 'FF' }),
  c('format.fontSize', 'Font Size', 'ALargeSmall', { keyTip: 'FS' }),
  c('format.growFont', 'Increase Font Size', 'GrowFont', {
    apps: TEXT_APPS,
    keyTip: 'FG',
    officeName: 'Grow Font',
  }),
  c('format.shrinkFont', 'Decrease Font Size', 'ShrinkFont', {
    apps: TEXT_APPS,
    keyTip: 'FK',
    officeName: 'Shrink Font',
  }),
  c('format.changeCase', 'Change Case', 'ChangeCase', { apps: WRITE_DECK, keyTip: '7' }),
  c('format.clearFormatting', 'Clear All Formatting', 'ClearFormatting', { apps: TEXT_APPS, keyTip: 'E' }),
  c('format.bold', 'Bold', 'Bold', { keyTip: '1' }),
  c('format.italic', 'Italic', 'Italic', { keyTip: '2' }),
  c('format.underline', 'Underline', 'Underline', { keyTip: '3' }),
  c('format.doubleUnderline', 'Double Underline', 'DoubleUnderline', { apps: ['write'] }),
  c('format.strikethrough', 'Strikethrough', 'Strikethrough', { keyTip: '4' }),
  c('format.subscript', 'Subscript', 'Subscript', { apps: WRITE_DECK, keyTip: '5' }),
  c('format.superscript', 'Superscript', 'Superscript', { apps: WRITE_DECK, keyTip: '6' }),
  c('format.allCaps', 'All Caps', 'CaseUpper', { apps: ['write'] }),
  c('format.textEffects', 'Text Effects', 'TextEffects', { apps: WRITE_DECK, tier: 'P2' }),
  c('format.highlight', 'Text Highlight Color', 'TextHighlight', { apps: TEXT_APPS, keyTip: 'I' }),
  c('format.fontColor', 'Font Color', 'FontColor', { keyTip: 'FC' }),
  c('format.fontDialog', 'Font…', 'Type', {
    apps: WRITE_DECK,
    description: 'Open the Font dialog.',
    keyTip: 'FN',
  }),
  c('format.characterSpacing', 'Character Spacing', 'CharacterSpacing', { apps: ['deck'] }),
  c('format.textShadow', 'Text Shadow', 'TextShadow', { apps: ['deck'] }),
]);

export const paragraphCommands = section('Home ▸ Paragraph', { apps: TEXT_APPS }, [
  c('format.bullets', 'Bullets', 'BulletList', { keyTip: 'U' }),
  c('format.numbering', 'Numbering', 'NumberedList', { keyTip: 'N' }),
  c('format.multilevelList', 'Multilevel List', 'MultilevelList', { apps: ['write'], keyTip: 'M' }),
  c('format.outdent', 'Decrease Indent', 'DecreaseIndent', { keyTip: 'AO' }),
  c('format.indent', 'Increase Indent', 'IncreaseIndent', { keyTip: 'AI' }),
  c('format.alignLeft', 'Align Left', 'AlignLeft', { keyTip: 'AL' }),
  c('format.alignCenter', 'Center', 'AlignCenter', { keyTip: 'AC', synonyms: ['centre'] }),
  c('format.alignRight', 'Align Right', 'AlignRight', { keyTip: 'AR' }),
  c('format.justify', 'Justify', 'AlignJustify', { keyTip: 'AJ' }),
  c('format.lineSpacing', 'Line and Paragraph Spacing', 'LineSpacing', { keyTip: 'K' }),
  c('format.paragraphDialog', 'Paragraph…', 'Pilcrow', { description: 'Open the Paragraph dialog.' }),
]);

export const insertCommonCommands = section('Insert', { apps: EDITORS }, [
  c('insert.link', 'Link', 'Link', { officeName: 'Hyperlink' }),
  c('insert.date', 'Insert Date', 'CalendarDays', { apps: WRITE_GRID }),
  c('insert.time', 'Insert Time', 'Clock', { apps: WRITE_GRID }),
  c('insert.dateTime', 'Date & Time', 'DateTime', { apps: WRITE_DECK }),
  c('insert.table', 'Table', 'InsertTable', { apps: ['write', 'deck', 'notes'] }),
  c('insert.pictures', 'Pictures', 'Image'),
  c('insert.shapes', 'Shapes', 'Shapes'),
  c('insert.icons', 'Icons', 'Smile'),
  c('insert.chart', 'Chart', 'ChartColumn'),
  c('insert.textBox', 'Text Box', 'TextBox'),
  c('insert.symbol', 'Symbol', 'SymbolOmega'),
  c('insert.equation', 'Equation', 'Equation', { tier: 'P1' }),
  c('insert.headerFooter', 'Header & Footer', 'Header'),
  c('insert.wordArt', 'WordArt', 'WordArt', { tier: 'P2' }),
  c('insert.object', 'Object', 'Box', { tier: 'P2' }),
  c('insert.codeBlock', 'Code Block', 'CodeBlock', {
    apps: ['write', 'deck', 'notes'],
    description: 'A block of code with syntax highlighting.',
  }),
  c('insert.screenshot', 'Screenshot', 'Screenshot', { apps: WRITE_DECK }),
]);

export const reviewCommonCommands = section('Review', { apps: EDITORS }, [
  c('review.spelling', 'Spelling & Grammar', 'Spelling', { officeName: 'Spelling & Grammar' }),
  c('review.thesaurus', 'Thesaurus', 'Thesaurus', { apps: ['write'], tier: 'P2' }),
  c('review.wordCount', 'Word Count', 'WordCount', { apps: TEXT_APPS }),
  c('review.newComment', 'New Comment', 'NewComment'),
  c('review.deleteComment', 'Delete Comment', 'DeleteComment'),
  c('review.previousComment', 'Previous Comment', 'PreviousComment'),
  c('review.nextComment', 'Next Comment', 'NextComment'),
  c('review.showComments', 'Show Comments', 'ShowComments'),
  c('review.translate', 'Translate', 'Translate', { apps: ['write'] }),
  c('review.language', 'Language', 'Language'),
  c('review.compare', 'Compare', 'CompareDocuments', { apps: WRITE_DECK, tier: 'P2' }),
]);

export const viewCommonCommands = section('View', {}, [
  c('view.commandPalette', 'Command Palette', 'CommandPalette', {
    officeName: 'Search (Tell Me)',
    description: 'Find any command by name, including its Office name.',
  }),
  c('view.keyTips', 'Key Tips', 'Keyboard', {
    description: 'Show letters for reaching every ribbon command from the keyboard.',
    palette: false,
  }),
  c('view.toggleRibbon', 'Collapse the Ribbon', 'PanelTopClose', {
    officeName: 'Collapse the Ribbon',
    description: 'Show only the tab names to make more room.',
  }),
  c('view.ribbonClassic', 'Classic Ribbon', 'PanelTop', { synonyms: ['ribbon layout', 'full ribbon'] }),
  c('view.ribbonSimplified', 'Simplified Ribbon', 'PanelTopDashed', {
    synonyms: ['single line ribbon', 'compact ribbon'],
  }),
  c('view.shortcuts', 'Keyboard Shortcuts', 'Keyboard', {
    description: 'A cheat sheet of the shortcuts for this app.',
  }),
  c('view.zoom', 'Zoom', 'ZoomIn', { apps: EDITORS }),
  c('view.zoomIn', 'Zoom In', 'ZoomIn', { apps: EDITORS }),
  c('view.zoomOut', 'Zoom Out', 'ZoomOut', { apps: EDITORS }),
  c('view.zoom100', '100%', 'Zoom100', { apps: EDITORS }),
  c('view.ruler', 'Ruler', 'Ruler', { apps: ['write', 'grid', 'deck'] }),
  c('view.gridlines', 'Gridlines', 'Gridlines', { apps: ['write', 'grid', 'deck'] }),
  c('view.newWindow', 'New Window', 'NewWindow', { apps: EDITORS }),
  c('view.themeLight', 'Light Theme', 'Sun', { synonyms: ['light mode'] }),
  c('view.themeDark', 'Dark Theme', 'Moon', { synonyms: ['dark mode', 'night mode'] }),
  c('view.themeSystem', 'Use System Theme', 'Monitor'),
  c('view.themeHighContrast', 'High Contrast Theme', 'Contrast', { synonyms: ['accessibility', 'contrast'] }),
  c('view.macros', 'Macros', 'Macros', {
    apps: ['write', 'grid', 'deck'],
    description: 'Macros are never run in Kiln; they are kept in the file so it is not damaged.',
  }),
  c('view.taskPane', 'Close Pane', 'PanelRightClose', { palette: false }),
  c('help.assistant', 'Ask the Assistant', 'Sparkles', {
    tier: 'P1',
    synonyms: ['ai', 'copilot', 'summarise', 'rewrite'],
  }),
]);

export const navigationCommands = section('Navigation', { apps: EDITORS }, [
  ...(['Up', 'Down', 'Left', 'Right'] as const).flatMap((d) => [
    c(`nav.jump${d}`, `Jump ${d.toLowerCase()}`, undefined, {
      palette: false,
      description: 'Move by word or paragraph; in a sheet, to the edge of the data.',
    }),
    c(`nav.extendJump${d}`, `Extend selection ${d.toLowerCase()}`, undefined, { palette: false }),
  ]),
  c('nav.documentStart', 'Go to start', undefined, { palette: false }),
  c('nav.documentEnd', 'Go to end', undefined, { palette: false }),
]);
