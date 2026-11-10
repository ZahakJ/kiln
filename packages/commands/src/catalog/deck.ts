/** Kiln Deck commands and ribbon, and the Deck column of the shortcut table. */
import type {
  CaretMotion,
  ClipboardData,
  Command,
  DeckCommand,
  DeckDirection,
  DeckEffect,
  DeckFill,
  DeckOutline,
  DeckOutlineEdit,
  DeckRect,
  DeckShowSettings,
  DeckSlideSize,
  DeckState,
} from '@kiln/protocol';
import {
  mhead,
  mitem,
  msep,
  msub,
  type RibbonData,
  type RibbonGalleryEntry,
  type RibbonMenuEntry,
  rbutton,
  rcolor,
  rcombo,
  rdropdown,
  rgallery,
  rgroup,
  rnumber,
  rsplit,
  rtab,
  rtoggle,
} from '../ribbon.ts';
import type { CommandContext } from '../types.ts';
import { c, DECK, section } from './define.ts';
import {
  accessibilityGroup,
  clipboardGroup,
  fontGroup,
  macrosGroup,
  paragraphGroup,
  tableTabs,
  themeGroup,
  viewMode,
  zoomMenu,
} from './ribbon-shared.ts';

const D = { apps: DECK };

export const deckCommands = [
  ...section('Home ▸ Slides', D, [
    c('deck.slides.new_slide', 'New Slide', 'NewSlide', { keyTip: 'I' }),
    c('deck.slides.layout', 'Layout', 'SlideLayout', { keyTip: 'L' }),
    c('deck.slides.reset_slide', 'Reset', 'ResetSlide'),
    c('deck.slides.section', 'Section', 'Section'),
    c('deck.slides.duplicate', 'Duplicate', 'CopyPlus', { synonyms: ['duplicate slide'] }),
    c('deck.slides.previous_slide', 'Previous Slide', 'ChevronUp'),
    c('deck.slides.next_slide', 'Next Slide', 'ChevronDown'),
    c('deck.slides.edit_text', 'Edit Text', 'PencilLine'),
    c('deck.slides.promote', 'Promote', 'ArrowLeftToLine'),
    c('deck.slides.demote', 'Demote', 'ArrowRightToLine'),
    c('deck.slides.delete', 'Delete Slide', 'X', { synonyms: ['remove slide'] }),
    c('deck.slides.move', 'Move Slides', 'MoveVertical', { palette: false }),
    c('deck.slides.select', 'Select Slide', 'Square', { palette: false }),
    c('deck.slides.rename_section', 'Rename Section', 'PencilLine', { palette: false }),
    c('deck.slides.remove_section', 'Remove Section', 'X', { palette: false }),
    c('deck.slides.move_section', 'Move Section', 'MoveVertical', { palette: false }),
    c('deck.notes.set_text', 'Speaker Notes', 'NotebookText', { palette: false }),
  ]),
  ...section('Stage', D, [
    c('deck.stage.pointer_down', 'Press on the Slide', 'Crosshair', { palette: false }),
    c('deck.stage.pointer_move', 'Drag on the Slide', 'Crosshair', { palette: false }),
    c('deck.stage.pointer_up', 'Release on the Slide', 'Crosshair', { palette: false }),
    c('deck.stage.arm_insert', 'Draw a Shape', 'Shapes', { palette: false }),
    c('deck.select.shapes', 'Select Shapes', 'SelectObjects', { palette: false }),
    c('deck.select.all', 'Select All Objects', 'SelectObjects', { palette: false }),
    c('deck.select.escape', 'Clear Selection', 'X', { palette: false }),
    c('deck.select.next', 'Select Next Object', 'SelectObjects', { palette: false }),
    c('deck.edit.delete', 'Delete', 'X', { palette: false }),
    c('deck.text.insert', 'Type', 'Type', { palette: false }),
    c('deck.text.delete', 'Delete Text', 'X', { palette: false }),
    c('deck.text.new_paragraph', 'New Paragraph', 'Pilcrow', { palette: false }),
    c('deck.text.line_break', 'Line Break', 'CornerDownLeft', { palette: false }),
    c('deck.text.tab', 'Tab', 'ArrowRightToLine', { palette: false }),
    c('deck.text.move', 'Move the Caret', 'TextCursorInput', { palette: false }),
    c('deck.text.composition', 'Composition', 'Languages', { palette: false }),
    c('deck.text.select', 'Select Text', 'TextSelect', { palette: false }),
    c('deck.edit.find', 'Find', 'Search', { palette: false }),
    c('deck.edit.replace', 'Replace', 'Replace', { palette: false }),
    c('deck.edit.replace_font', 'Replace Fonts', 'Replace', { synonyms: ['replace font'] }),
  ]),
  ...section('Deck', D, [
    c('ui.deck.format_background', 'Format Background…', 'FormatBackground'),
    c('ui.deck.slide_size', 'Custom Slide Size…', 'SlideSize'),
    c('ui.deck.selection_pane', 'Selection Pane…', 'SelectionPane'),
    c('ui.deck.replace_fonts', 'Replace Fonts…', 'Replace'),
    c('ui.deck.rename_section', 'Rename Section…', 'PencilLine'),
    c('ui.deck.remove_section', 'Remove Section', 'X'),
    c('ui.deck.alt_text', 'Alt Text…', 'Accessibility'),
    c('ui.deck.change_picture', 'Change Picture…', 'ImagePlus'),
    c('ui.deck.crop', 'Crop Picture…', 'Crop'),
    c('ui.deck.shape_height', 'Shape Height', 'MoveVertical', { palette: false }),
    c('ui.deck.shape_width', 'Shape Width', 'MoveHorizontal', { palette: false }),
    c('ui.deck.size_position', 'Size and Position…', 'Scaling'),
    c('ui.deck.change_shape', 'Change Shape…', 'Shapes'),
    c('ui.deck.hyperlink', 'Link…', 'Link', { palette: false }),
    c('ui.deck.advance_after', 'Advance After', 'Timer', { palette: false }),
    // charts.
    c('ui.deck.insert_chart', 'Chart', 'ChartColumn', {
      officeName: 'Insert Chart',
      synonyms: ['graph', 'column chart', 'pie chart'],
    }),
    c('ui.deck.edit_chart_data', 'Edit Data', 'Table', { synonyms: ['chart data', 'data sheet'] }),
  ]),
  ...section('Shape Format', D, [
    c('deck.format.size_position', 'Size and Position', 'Scaling', { synonyms: ['height', 'width'] }),
    c('deck.arrange.rotate', 'Rotate', 'RotateObject'),
    c('deck.arrange.flip', 'Flip', 'RotateObject', { synonyms: ['flip horizontal', 'flip vertical'] }),
    c('deck.arrange.align', 'Align Objects', 'AlignObjects'),
    c('deck.arrange.distribute', 'Distribute', 'AlignObjects'),
    c('deck.arrange.order', 'Order Objects', 'BringForward', {
      synonyms: ['bring to front', 'send to back'],
    }),
    c('deck.drawing.change_shape', 'Change Shape', 'Shapes'),
    c('deck.format.alt_text', 'Alt Text', 'Accessibility'),
    c('deck.format.autofit', 'Autofit Text', 'Scaling', { synonyms: ['shrink text on overflow'] }),
    c('deck.picture.crop', 'Crop by Values', 'Crop'),
    c('deck.picture.reset', 'Reset Picture', 'RotateCcw'),
    c('deck.paragraph.columns', 'Columns', 'Columns'),
    c('deck.font.character_spacing', 'Character Spacing', 'CharacterSpacing', { palette: false }),
    c('deck.font.text_shadow', 'Text Shadow', 'TextShadow', { palette: false }),
    c('deck.clipboard.copy_format', 'Copy Formatting', 'FormatPainter', { palette: false }),
    c('deck.clipboard.paste_format', 'Paste Formatting', 'FormatPainter', { palette: false }),
  ]),
  ...section('Insert', D, [
    c('deck.insert.shape', 'Insert Shape', 'Shapes', { palette: false }),
    c('deck.insert.text_box', 'Insert Text Box', 'TextBox', { palette: false }),
    c('deck.insert.picture', 'Insert Picture', 'Image', { palette: false }),
    c('deck.insert.table', 'Insert Table', 'InsertTable', { palette: false }),
    c('deck.insert.header_footer', 'Header & Footer', 'HeaderFooterDeck', { palette: false }),
    c('deck.insert.link', 'Link', 'Link', { palette: false }),
  ]),
  ...section('Home ▸ Paragraph', D, [
    c('deck.paragraph.text_direction', 'Text Direction', 'TextDirection'),
    c('deck.paragraph.align_text', 'Align Text', 'AlignText'),
    c('deck.paragraph.convert_to_diagram', 'Convert to List Diagram', 'ConvertToDiagram', {
      officeName: 'Convert to SmartArt',
      tier: 'P1',
    }),
  ]),
  ...section('Home ▸ Drawing', D, [
    c('deck.drawing.arrange', 'Arrange', 'Arrange'),
    c('deck.drawing.quick_styles', 'Quick Styles', 'QuickStyles'),
    c('deck.drawing.shape_fill', 'Shape Fill', 'ShapeFill'),
    c('deck.drawing.shape_outline', 'Shape Outline', 'ShapeOutline'),
    c('deck.drawing.shape_effects', 'Shape Effects', 'ShapeEffects'),
    c('deck.drawing.group', 'Group', 'GroupObjects'),
    c('deck.drawing.ungroup', 'Ungroup', 'Ungroup'),
    c('deck.drawing.nudge', 'Nudge', 'Move', { palette: false }),
    c('deck.drawing.resize', 'Resize', 'Scaling', { palette: false }),
    c('deck.drawing.design_ideas', 'Design Ideas', 'DesignIdeas', { officeName: 'Designer' }),
  ]),
  ...section('Insert', D, [
    c('deck.insert.reuse_slides', 'Reuse Slides', 'ReuseSlides', { tier: 'P1' }),
    c('deck.insert.video', 'Video', 'InsertVideo', { tier: 'P1' }),
    c('deck.insert.audio', 'Audio', 'InsertAudio', { tier: 'P1' }),
    c('deck.insert.slide_number', 'Slide Number', 'SlideNumber'),
    c('deck.insert.live_grid_range', 'Live Grid Range', 'LiveGridRange', { tier: 'P2' }),
  ]),
  ...section('Design', D, [
    c('deck.design.themes', 'Themes', 'Palette'),
    c('deck.design.variants', 'Variants', 'SwatchBook'),
    c('deck.design.slide_size', 'Slide Size', 'SlideSize', { synonyms: ['16:9', '4:3', 'widescreen'] }),
    c('deck.design.format_background', 'Format Background', 'FormatBackground'),
    c('deck.design.hide_background_graphics', 'Hide Background Graphics', 'EyeOff'),
    // the Variants menus and Browse for Themes.
    c('deck.design.colors', 'Theme Colors', 'Palette', { synonyms: ['colour scheme', 'color scheme'] }),
    c('deck.design.fonts', 'Theme Fonts', 'Type', { synonyms: ['font scheme'] }),
    c('deck.design.effects', 'Theme Effects', 'Sparkles'),
    c('deck.design.background_style', 'Background Styles', 'FormatBackground'),
    c('deck.design.browse_themes', 'Apply a Theme File', 'Palette', { palette: false }),
    c('ui.deck.browse_themes', 'Browse for Themes…', 'FolderOpen', { synonyms: ['thmx', 'theme file'] }),
  ]),
  ...section('Transitions', D, [
    c('deck.transitions.preview', 'Preview', 'TransitionPreview'),
    c('deck.transitions.gallery', 'Transition to This Slide', 'Blend'),
    c('deck.transitions.effect_options', 'Effect Options', 'Settings', {
      description: 'Change the direction or form of the transition.',
    }),
    c('deck.transitions.duration', 'Duration', 'Timer'),
    c('deck.transitions.apply_to_all', 'Apply To All', 'ApplyToAll'),
    c('deck.transitions.on_click', 'On Mouse Click', 'Square', {
      description: 'Advance to the next slide when you click.',
    }),
    c('deck.transitions.after', 'After', 'Timer', {
      description: 'Advance to the next slide after this many seconds.',
      synonyms: ['slide timing', 'auto advance'],
    }),
  ]),
  ...section('Animations', D, [
    c('deck.animations.preview', 'Preview Animations', 'Play'),
    c('deck.animations.gallery', 'Animation Styles', 'Sparkle'),
    c('deck.animations.effect_options', 'Effect Options', 'Settings', {
      description: 'Change the direction, amount or sequence of the animation.',
    }),
    c('deck.animations.add_animation', 'Add Animation', 'AddAnimation'),
    c('deck.animations.animation_pane', 'Animation Pane', 'AnimationPane'),
    c('deck.animations.animation_painter', 'Animation Painter', 'AnimationPainter'),
    c('deck.animations.start', 'Start', 'Play', { palette: false }),
    c('deck.animations.duration', 'Duration', 'Timer', { palette: false }),
    c('deck.animations.delay', 'Delay', 'Clock', { palette: false }),
    c('deck.animations.repeat', 'Repeat', 'Repeat', { palette: false }),
    c('deck.animations.remove', 'Remove Animation', 'X'),
    c('deck.animations.reorder', 'Reorder Animation', 'MoveVertical', { palette: false }),
    c('deck.animations.move', 'Move Animation', 'MoveVertical', { palette: false }),
    c('deck.animations.select', 'Select Animations', 'Sparkle', { palette: false }),
    c('deck.animations.copy', 'Pick Up Animation', 'AnimationPainter', { palette: false }),
    c('deck.animations.paste', 'Apply Animation', 'AnimationPainter', { palette: false }),
    c('deck.animations.badges', 'Effect Numbers', 'Hash', { palette: false }),
  ]),
  ...section('Slide Show', D, [
    c('deck.show.from_beginning', 'From Beginning', 'FromBeginning', {
      synonyms: ['present', 'start slide show', 'play'],
    }),
    c('deck.show.from_current', 'From Current Slide', 'FromCurrent'),
    c('deck.show.in_tab', 'Present in Browser Tab', 'PresentInTab'),
    c('deck.show.set_up', 'Set Up Slide Show', 'SetUpShow'),
    c('deck.show.hide_slide', 'Hide Slide', 'HideSlide'),
    c('deck.show.rehearse_timings', 'Rehearse Timings', 'RehearseTimings', { tier: 'P1' }),
    c('deck.show.presenter_view', 'Use Presenter View', 'PresenterView'),
  ]),
  ...section('Slide Show ▸ During the show', { apps: DECK }, [
    c('deck.show.next', 'Next', 'ChevronRight', { palette: false }),
    c('deck.show.previous', 'Previous', 'ChevronLeft', { palette: false }),
    c('deck.show.first', 'First Slide', 'ChevronFirst', { palette: false }),
    c('deck.show.last', 'Last Slide', 'ChevronLast', { palette: false }),
    c('deck.show.digit', 'Type Slide Number', 'Hash', { palette: false }),
    c('deck.show.go_to_typed', 'Go to Slide', 'CornerDownLeft', { palette: false }),
    c('deck.show.black', 'Black Screen', 'Blackout', { palette: false }),
    c('deck.show.white', 'White Screen', 'Square', { palette: false }),
    c('deck.show.laser', 'Laser Pointer', 'LaserPointer', { palette: false }),
    c('deck.show.pen', 'Pen', 'PenTool', { palette: false }),
    c('deck.show.highlighter', 'Highlighter', 'HighlighterTool', { palette: false }),
    c('deck.show.end', 'End Show', 'X', { palette: false }),
    c('deck.show.eraser', 'Eraser', 'Eraser', { palette: false }),
    c('deck.show.erase', 'Erase All Ink on Slide', 'Eraser', { palette: false }),
    c('deck.show.arrow', 'Arrow', 'Crosshair', { palette: false }),
    c('deck.show.go_to', 'See All Slides', 'Rows3', { palette: false }),
    c('deck.show.pointer', 'Pointer on the Show', 'Crosshair', { palette: false }),
    c('deck.show.start', 'Start Slide Show', 'FromBeginning', { palette: false }),
    c('deck.show.settings', 'Slide Show Settings', 'SetUpShow', { palette: false }),
    // Asked at the end of Rehearse Timings (P1).
    c('deck.show.keep_timings', 'Keep Slide Timings', 'RehearseTimings', { palette: false, tier: 'P1' }),
    c('suite.view.open_window', 'Open a Second Window', 'Monitor', { palette: false }),
  ]),
  ...section('Design Ideas, Outline View, Reuse Slides', D, [
    c('deck.designer.apply', 'Apply Design Idea', 'DesignIdeas', { palette: false }),
    c('deck.outline.edit', 'Edit the Outline', 'OutlineView', { palette: false }),
    c('deck.insert.reuse_slides_from', 'Insert Reused Slides', 'ReuseSlides', {
      palette: false,
      tier: 'P1',
    }),
    c('deck.insert.media', 'Insert Video or Audio', 'InsertVideo', { palette: false, tier: 'P1' }),
    c('deck.insert.equation', 'Insert Equation', 'Equation', { palette: false, tier: 'P1' }),
  ]),
  ...section('Charts', D, [c('deck.chart.set_data', 'Set Chart Data', 'Table', { palette: false })]),
  // Paste as Picture, icons, code blocks, symbols, crop, table cells.
  ...section('Objects', D, [
    c('deck.clipboard.paste_picture', 'Paste as Picture', 'Image', {
      keyTip: 'U',
      synonyms: ['paste picture', 'paste as image'],
    }),
    c('deck.insert.icon', 'Insert Icon', 'Smile', { palette: false }),
    c('deck.insert.code_block', 'Insert Code Block', 'CodeBlock', { palette: false }),
    c('deck.insert.symbol', 'Insert Symbol', 'SymbolOmega', { palette: false }),
    c('deck.picture.crop_mode', 'Crop', 'Crop', { synonyms: ['crop handles'] }),
    c('deck.picture.crop_to', 'Crop to Aspect Ratio', 'Crop', { synonyms: ['fill', 'fit', 'aspect ratio'] }),
    c('deck.table.select_cells', 'Select Cells', 'Table', { palette: false }),
    c('deck.table.cell_margins', 'Cell Margins', 'Table', { keyTip: 'M' }),
    c('ui.deck.format_shape', 'Format Shape…', 'ShapeFill', { synonyms: ['gradient fill', 'pattern fill'] }),
    c('ui.deck.split_cells', 'Split Cells…', 'TableCellsSplit'),
    c('ui.deck.cell_margins', 'Custom Margins…', 'Table'),
  ]),
  ...section('Slide Master', D, [
    c('deck.master.view', 'Close Master View', 'X', { palette: false }),
    c('deck.master.select', 'Show a Master or Layout', 'SlideMaster', { palette: false }),
    c('deck.master.insert_master', 'Insert Slide Master', 'SlideMaster'),
    c('deck.master.insert_layout', 'Insert Layout', 'SlideLayout'),
    c('deck.master.delete', 'Delete Master or Layout', 'X'),
    c('deck.master.rename', 'Rename Master or Layout', 'PencilLine', { palette: false }),
    c('deck.master.preserve', 'Preserve Master', 'Pin'),
    c('deck.master.insert_placeholder', 'Insert Placeholder', 'Square'),
    c('deck.master.title', 'Title', 'Heading', {
      description: 'Show or hide the title placeholder on this layout.',
    }),
    c('deck.master.footers', 'Footers', 'PanelBottom', {
      description: 'Show or hide the date, footer and slide number on this layout.',
    }),
    c('ui.deck.rename_master', 'Rename…', 'PencilLine'),
  ]),
  ...section('Review', D, [
    c('deck.comment.new', 'Post a Comment', 'NewComment', { palette: false }),
    c('deck.comment.reply', 'Reply to a Comment', 'NewComment', { palette: false }),
    c('deck.comment.edit', 'Edit a Comment', 'PencilLine', { palette: false }),
    c('deck.comment.resolve', 'Resolve a Thread', 'AcceptChange', { palette: false }),
    c('deck.comment.delete', 'Delete Comments', 'DeleteComment', { palette: false }),
    c('deck.comment.select', 'Select a Comment', 'ShowComments', { palette: false }),
    c('deck.comment.next', 'Go to a Comment', 'NextComment', { palette: false }),
    c('deck.spell.check', 'Check Spelling', 'Spelling', { palette: false }),
    c('deck.spell.change', 'Change the Word', 'Spelling', { palette: false }),
    c('deck.spell.ignore', 'Ignore the Word', 'Spelling', { palette: false }),
    c('deck.spell.load_dictionary', 'Load the Dictionary', 'Spelling', { palette: false }),
    c('deck.spell.user_dictionary', 'User Dictionary', 'Spelling', { palette: false }),
  ]),
  ...section('View', D, [
    c('deck.view.outline_view', 'Outline View', 'OutlineView'),
    c('deck.view.slide_master', 'Slide Master', 'SlideMaster'),
    c('deck.view.handout_master', 'Handout Master', 'HandoutMaster', { tier: 'P2' }),
    c('deck.view.notes_master', 'Notes Master', 'NotesMaster', { tier: 'P2' }),
    c('deck.view.guides', 'Guides', 'Crosshair'),
    c('deck.view.gridlines', 'Gridlines', 'Gridlines'),
    c('deck.view.notes', 'Notes', 'NotebookText'),
    c('deck.view.fit_to_window', 'Fit to Window', 'FitToWindow'),
    c('deck.view.color_grayscale', 'Color / Grayscale', 'Contrast', { tier: 'P2' }),
    // the guides and the ruler's indents.
    c('deck.view.add_guide', 'Add Guide', 'Crosshair', {
      synonyms: ['add vertical guide', 'add horizontal guide'],
    }),
    c('deck.view.move_guide', 'Move Guide', 'Crosshair', { palette: false }),
    c('deck.view.remove_guide', 'Remove Guide', 'X', { palette: false }),
    c('deck.paragraph.indent', 'Paragraph Indents', 'IncreaseIndent', { palette: false }),
  ]),
  ...section('Slide Show ▸ Audience', D, [
    c('deck.show.audience_laser', 'Laser Pointer', 'LaserPointer', {
      description: 'Point at the slide while presenting.',
    }),
    c('deck.show.audience_pen', 'Pen', 'PenTool'),
    c('deck.show.audience_highlighter', 'Highlighter', 'HighlighterTool'),
    c('deck.show.audience_blackout', 'Blackout', 'Blackout', {
      description: 'Black or white screen (B / W during the show).',
    }),
  ]),
];

/** Transition to This Slide, in PowerPoint's order (ids are the core's gallery ids). */
const TRANSITIONS: readonly RibbonGalleryEntry[] = [
  ['none', 'None'],
  // Morph is P2: a file's Morph still opens, saves and plays (as Fade).
  ['morph', 'Morph', 'P2'],
  ['fade', 'Fade'],
  ['push', 'Push'],
  ['wipe', 'Wipe'],
  ['split', 'Split'],
  ['reveal', 'Reveal'],
  ['cut', 'Cut'],
  ['randomBars', 'Random Bars'],
  ['shape', 'Shape'],
  ['uncover', 'Uncover'],
  ['cover', 'Cover'],
  ['flash', 'Flash'],
].map(([id, label, tier]) => ({
  id: id ?? '',
  label: label ?? '',
  sample: label,
  look: { font: 'ui' as const, size: 11 },
  ...(tier === 'P2' ? { tier: 'P2' as const } : {}),
}));

/** The Animation gallery: entrance, emphasis and exit effects (ids are the core's). */
export const DECK_ANIMATIONS: readonly {
  id: string;
  label: string;
  group: 'Entrance' | 'Emphasis' | 'Exit';
}[] = [
  { id: 'appear', label: 'Appear', group: 'Entrance' },
  { id: 'fade', label: 'Fade', group: 'Entrance' },
  { id: 'flyIn', label: 'Fly In', group: 'Entrance' },
  { id: 'floatIn', label: 'Float In', group: 'Entrance' },
  { id: 'split', label: 'Split', group: 'Entrance' },
  { id: 'wipe', label: 'Wipe', group: 'Entrance' },
  { id: 'zoom', label: 'Zoom', group: 'Entrance' },
  { id: 'growTurn', label: 'Grow & Turn', group: 'Entrance' },
  { id: 'pulse', label: 'Pulse', group: 'Emphasis' },
  { id: 'colorPulse', label: 'Colour Pulse', group: 'Emphasis' },
  { id: 'teeter', label: 'Teeter', group: 'Emphasis' },
  { id: 'spin', label: 'Spin', group: 'Emphasis' },
  { id: 'growShrink', label: 'Grow/Shrink', group: 'Emphasis' },
  { id: 'disappear', label: 'Disappear', group: 'Exit' },
  { id: 'fadeOut', label: 'Fade', group: 'Exit' },
  { id: 'flyOut', label: 'Fly Out', group: 'Exit' },
  { id: 'zoomOut', label: 'Zoom', group: 'Exit' },
];

const ANIMATIONS: readonly RibbonGalleryEntry[] = [
  { id: 'none', label: 'None', sample: 'None', look: { font: 'ui' as const, size: 11 } },
  ...DECK_ANIMATIONS.map((a) => ({
    id: a.id,
    label: a.group === 'Exit' ? `${a.label} (Exit)` : a.label,
    sample: a.label,
    look: { font: 'ui' as const, size: 11 },
  })),
];

/** Add Animation ▾: the effects by class. */
const ADD_ANIMATION = (['Entrance', 'Emphasis', 'Exit'] as const).flatMap((g, i) => [
  ...(i > 0 ? [msep()] : []),
  mhead(g),
  ...DECK_ANIMATIONS.filter((a) => a.group === g).map((a) =>
    mitem('deck.animations.add_animation', { label: a.label, args: { effect: a.id } }),
  ),
]);

const START_OPTIONS = [
  { value: 'OnClick', label: 'On Click' },
  { value: 'WithPrevious', label: 'With Previous' },
  { value: 'AfterPrevious', label: 'After Previous' },
];

/** The bundled themes in gallery order: background, text and accent of each. */
export const DECK_THEMES: readonly {
  id: string;
  name: string;
  colours: readonly [string, string, string];
}[] = [
  { id: 'kiln', name: 'Kiln', colours: ['#FFFAF3', '#2B1C14', '#E05A27'] },
  { id: 'tenmoku', name: 'Tenmoku', colours: ['#1E140F', '#F5EBDD', '#FF8A3D'] },
  { id: 'cobalt', name: 'Cobalt', colours: ['#FFFFFF', '#1A2238', '#2D4FB5'] },
  { id: 'jade', name: 'Jade', colours: ['#F6FAF7', '#17302A', '#1D8A60'] },
  { id: 'saffron', name: 'Saffron', colours: ['#FFF8EA', '#2E2312', '#E8A82C'] },
  { id: 'celadon', name: 'Celadon', colours: ['#EEF5F3', '#1F3534', '#5E9E95'] },
  { id: 'shino', name: 'Shino', colours: ['#FBF7F0', '#3A322C', '#C8553D'] },
  { id: 'raku', name: 'Raku', colours: ['#121212', '#EDE6DD', '#C87533'] },
  { id: 'porcelain', name: 'Porcelain', colours: ['#FFFFFF', '#1F2A37', '#2F6DB5'] },
  { id: 'slate', name: 'Slate', colours: ['#1C232B', '#E8EDF2', '#4FB3BF'] },
  { id: 'terracotta', name: 'Terracotta', colours: ['#F8EEE5', '#3B2418', '#B4462A'] },
  { id: 'ash', name: 'Ash', colours: ['#F5F4EF', '#2E2F2A', '#6B7F4E'] },
];

const THEMES: readonly RibbonGalleryEntry[] = DECK_THEMES.map((t) => ({
  id: t.id,
  label: t.name,
  sample: t.name,
  look: { swatch: [...t.colours] },
}));

const VARIANTS: readonly RibbonGalleryEntry[] = [0, 1, 2, 3].map((i) => ({
  id: String(i),
  label: `Variant ${i + 1}`,
  sample: `Variant ${i + 1}`,
  look: {
    swatch:
      [
        ['#FFFAF3', '#E05A27'],
        ['#FFFFFF', '#3159C6'],
        ['#F6FAF7', '#1D8A60'],
        ['#1E140F', '#FF8A3D'],
      ][i] ?? [],
  },
}));

/** The schemes of the Colors, Fonts and Effects menus: Office's, then each bundled theme's. */
const scheme = (id: string, extra: readonly string[] = []): RibbonMenuEntry[] =>
  ['Office', ...extra, ...DECK_THEMES.map((t) => t.name)].map((name) =>
    mitem(id, { label: name, args: { scheme: name } }),
  );

/** The twelve background styles, then Format Background. */
const BACKGROUND_STYLES: readonly RibbonMenuEntry[] = [
  ...Array.from({ length: 12 }, (_, i) =>
    mitem('deck.design.background_style', { label: `Style ${i + 1}`, args: { style: i + 1 } }),
  ),
  msep(),
  mitem('ui.deck.format_background', { label: 'Format Background…' }),
];

/** Design ▸ Variants ▾: Colors, Fonts, Effects and Background Styles, as in PowerPoint. */
const VARIANT_MENUS: readonly RibbonMenuEntry[] = [
  msub('Colors', scheme('deck.design.colors', ['Grayscale']), 'Palette'),
  msub('Fonts', scheme('deck.design.fonts'), 'Type'),
  msub('Effects', scheme('deck.design.effects'), 'Sparkles'),
  msub('Background Styles', BACKGROUND_STYLES, 'FormatBackground'),
];

/** The Shapes gallery in the ribbon (the full set is in the Shapes menu of the editor). */
export const DECK_QUICK_SHAPES: readonly { preset: string; label: string; sample: string }[] = [
  { preset: 'rect', label: 'Rectangle', sample: '▭' },
  { preset: 'ellipse', label: 'Oval', sample: '◯' },
  { preset: 'roundRect', label: 'Rectangle: Rounded Corners', sample: '▢' },
  { preset: 'triangle', label: 'Isosceles Triangle', sample: '△' },
  { preset: 'rtTriangle', label: 'Right Triangle', sample: '◺' },
  { preset: 'rightArrow', label: 'Arrow: Right', sample: '⇨' },
  { preset: 'line', label: 'Straight Connector', sample: '╱' },
  { preset: 'star5', label: 'Star: 5 Points', sample: '☆' },
  { preset: 'diamond', label: 'Diamond', sample: '◇' },
  { preset: 'hexagon', label: 'Hexagon', sample: '⬡' },
  { preset: 'wedgeRoundRectCallout', label: 'Speech Bubble: Rectangle with Corners Rounded', sample: '💬︎' },
  { preset: 'chevron', label: 'Arrow: Chevron', sample: '❯' },
  { preset: 'heart', label: 'Heart', sample: '♡' },
  { preset: 'flowChartProcess', label: 'Flowchart: Process', sample: '▭' },
];

const ACCENTS = ['#E05A27', '#3159C6', '#1D8A60', '#E3A126', '#B4462A', '#6B5CA5'];
const STYLE_ROWS = [
  'Colored Outline',
  'Colored Fill',
  'Light 1 Outline',
  'Subtle Effect',
  'Moderate Effect',
  'Intense Effect',
  'Light Fill',
];

const ORDER_MENU = [
  mitem('deck.arrange.order', { label: 'Bring to Front', icon: 'BringForward', args: { order: 'Front' } }),
  mitem('deck.arrange.order', { label: 'Bring Forward', icon: 'BringForward', args: { order: 'Forward' } }),
  mitem('deck.arrange.order', { label: 'Send Backward', icon: 'SendBackward', args: { order: 'Backward' } }),
  mitem('deck.arrange.order', { label: 'Send to Back', icon: 'SendBackward', args: { order: 'Back' } }),
];

const ALIGN_MENU = [
  mitem('deck.arrange.align', { label: 'Align Left', icon: 'AlignLeft', args: { how: 'Left' } }),
  mitem('deck.arrange.align', { label: 'Align Center', icon: 'AlignCenter', args: { how: 'Center' } }),
  mitem('deck.arrange.align', { label: 'Align Right', icon: 'AlignRight', args: { how: 'Right' } }),
  msep(),
  mitem('deck.arrange.align', { label: 'Align Top', icon: 'AlignTop', args: { how: 'Top' } }),
  mitem('deck.arrange.align', { label: 'Align Middle', icon: 'AlignMiddle', args: { how: 'Middle' } }),
  mitem('deck.arrange.align', { label: 'Align Bottom', icon: 'AlignBottom', args: { how: 'Bottom' } }),
  msep(),
  mitem('deck.arrange.distribute', { label: 'Distribute Horizontally', args: { horizontal: true } }),
  mitem('deck.arrange.distribute', { label: 'Distribute Vertically', args: { horizontal: false } }),
];

const ROTATE_MENU = [
  mitem('deck.arrange.rotate', { label: 'Rotate Right 90°', args: { degrees: 90 } }),
  mitem('deck.arrange.rotate', { label: 'Rotate Left 90°', args: { degrees: -90 } }),
  mitem('deck.arrange.flip', { label: 'Flip Vertical', args: { horizontal: false } }),
  mitem('deck.arrange.flip', { label: 'Flip Horizontal', args: { horizontal: true } }),
];

const GROUP_MENU = [mitem('deck.drawing.group'), mitem('deck.drawing.ungroup')];

const ARRANGE_MENU = [
  { kind: 'heading' as const, label: 'Order Objects' },
  ...ORDER_MENU,
  { kind: 'heading' as const, label: 'Group Objects' },
  ...GROUP_MENU,
  { kind: 'heading' as const, label: 'Position Objects' },
  msub('Align', ALIGN_MENU, 'AlignObjects'),
  msub('Rotate', ROTATE_MENU, 'RotateObject'),
  msep(),
  mitem('ui.deck.selection_pane', { label: 'Selection Pane…' }),
];

const EFFECTS_MENU = [
  mitem('deck.drawing.shape_effects', { label: 'Shadow', args: { kind: 'shadow' } }),
  mitem('deck.drawing.shape_effects', { label: 'No Shadow', args: { kind: 'noShadow' } }),
  mitem('deck.drawing.shape_effects', { label: 'Glow', args: { kind: 'glow' } }),
  mitem('deck.drawing.shape_effects', { label: 'Soft Edges', args: { kind: 'softEdges' } }),
  msep(),
  mitem('deck.drawing.shape_effects', { label: 'No Effects', args: { kind: 'none' } }),
];

const WEIGHT_MENU = [0.25, 0.5, 0.75, 1, 1.5, 2.25, 3, 4.5, 6].map((w) =>
  mitem('deck.drawing.shape_outline', { label: `${w} pt`, args: { weight: w } }),
);
const DASH_MENU = [
  ['solid', 'Solid'],
  ['sysDot', 'Round Dot'],
  ['sysDash', 'Square Dot'],
  ['dash', 'Dash'],
  ['dashDot', 'Dash Dot'],
  ['lgDash', 'Long Dash'],
].map(([dash, label]) => mitem('deck.drawing.shape_outline', { label: label ?? '', args: { dash } }));
const ARROW_MENU = [
  mitem('deck.drawing.shape_outline', { label: 'No Arrows', args: { head: 'none', tail: 'none' } }),
  mitem('deck.drawing.shape_outline', { label: 'Arrow at End', args: { head: 'none', tail: 'triangle' } }),
  mitem('deck.drawing.shape_outline', { label: 'Arrow at Start', args: { head: 'triangle', tail: 'none' } }),
  mitem('deck.drawing.shape_outline', {
    label: 'Arrows at Both Ends',
    args: { head: 'triangle', tail: 'triangle' },
  }),
];

const AUTOFIT_MENU = [
  mitem('deck.format.autofit', { label: 'Do Not Autofit', args: { mode: 'Off' } }),
  mitem('deck.format.autofit', { label: 'Shrink Text on Overflow', args: { mode: 'Shrink' } }),
  mitem('deck.format.autofit', { label: 'Resize Shape to Fit Text', args: { mode: 'ResizeShape' } }),
];

const ALIGN_TEXT_MENU = [
  mitem('deck.paragraph.align_text', { label: 'Top', icon: 'AlignTop', args: { anchor: 'Top' } }),
  mitem('deck.paragraph.align_text', { label: 'Middle', icon: 'AlignMiddle', args: { anchor: 'Middle' } }),
  mitem('deck.paragraph.align_text', { label: 'Bottom', icon: 'AlignBottom', args: { anchor: 'Bottom' } }),
];

const DIRECTION_MENU = [
  mitem('deck.paragraph.text_direction', { label: 'Horizontal', args: { direction: 'Horizontal' } }),
  mitem('deck.paragraph.text_direction', { label: 'Rotate all text 90°', args: { direction: 'Rotate90' } }),
  mitem('deck.paragraph.text_direction', { label: 'Rotate all text 270°', args: { direction: 'Rotate270' } }),
  mitem('deck.paragraph.text_direction', { label: 'Stacked', args: { direction: 'Stacked' } }),
];

const COLUMNS_MENU = [1, 2, 3].map((n) =>
  mitem('deck.paragraph.columns', {
    label: n === 1 ? 'One Column' : n === 2 ? 'Two Columns' : 'Three Columns',
    args: { count: n, spacing: n === 1 ? 0 : 18 },
  }),
);

const SLIDE_SIZE_MENU = [
  mitem('deck.design.slide_size', { label: 'Standard (4:3)', args: { ratio: '4:3' } }),
  mitem('deck.design.slide_size', { label: 'Widescreen (16:9)', args: { ratio: '16:9' } }),
  msep(),
  mitem('ui.deck.slide_size', { label: 'Custom Slide Size…' }),
];

/** Shape Format (contextual, while a shape is selected): PowerPoint's Drawing Tools. */
const shapeFormatTabDeck = () =>
  rtab(
    'shapeFormat',
    'Shape Format',
    [
      rgroup('insertShapes', 'Insert Shapes', [
        shapesGallery('SH'),
        rdropdown('deck.drawing.change_shape', [mitem('ui.deck.change_shape', { label: 'Change Shape…' })], {
          size: 'small',
          keyTip: 'E',
        }),
        rbutton('suite.insert.text_box', { size: 'small', keyTip: 'X' }),
      ]),
      rgroup(
        'shapeStyles',
        'Shape Styles',
        [
          quickStyles('deck.format_styles', 'K'),
          rcolor('deck.drawing.shape_fill', {
            id: 'deck.format_fill',
            allowNone: true,
            defaultColor: '#E05A27',
            keyTip: 'SF',
          }),
          rsplit(
            'deck.drawing.shape_outline',
            [
              mitem('deck.drawing.shape_outline', { label: 'No Outline', args: { none: true } }),
              msub('Weight', WEIGHT_MENU),
              msub('Dashes', DASH_MENU),
              msub('Arrows', ARROW_MENU),
            ],
            { id: 'deck.format_outline', size: 'small', keyTip: 'SO' },
          ),
          rdropdown('deck.drawing.shape_effects', EFFECTS_MENU, {
            id: 'deck.format_effects',
            size: 'small',
            keyTip: 'SE',
          }),
        ],
        { launcher: { command: 'ui.deck.format_shape', label: 'Format Shape…' } },
      ),
      rgroup('text', 'Text', [
        rdropdown('deck.paragraph.text_direction', DIRECTION_MENU, {
          id: 'deck.format_direction',
          size: 'small',
        }),
        rdropdown('deck.paragraph.align_text', ALIGN_TEXT_MENU, { id: 'deck.format_anchor', size: 'small' }),
        rdropdown('deck.format.autofit', AUTOFIT_MENU, { size: 'small', keyTip: 'AF' }),
      ]),
      rgroup('accessibility', 'Accessibility', [
        rbutton('ui.deck.alt_text', {
          label: 'Alt Text',
          icon: 'Accessibility',
          size: 'large',
          keyTip: 'AT',
        }),
      ]),
      rgroup('arrange', 'Arrange', [
        rsplit('deck.arrange.order', ORDER_MENU.slice(0, 2), {
          id: 'deck.bring_forward',
          label: 'Bring Forward',
          icon: 'BringForward',
          args: { order: 'Forward' },
          size: 'medium',
          keyTip: 'AF',
        }),
        rsplit('deck.arrange.order', ORDER_MENU.slice(2), {
          id: 'deck.send_backward',
          label: 'Send Backward',
          icon: 'SendBackward',
          args: { order: 'Backward' },
          size: 'medium',
          keyTip: 'AE',
        }),
        rbutton('ui.deck.selection_pane', {
          label: 'Selection Pane',
          icon: 'SelectionPane',
          size: 'medium',
          keyTip: 'AP',
        }),
        rdropdown('deck.arrange.align', ALIGN_MENU, { size: 'small', keyTip: 'AA' }),
        rdropdown('deck.drawing.group', GROUP_MENU, { id: 'deck.group', size: 'small', keyTip: 'G' }),
        rdropdown('deck.arrange.rotate', ROTATE_MENU, { size: 'small', keyTip: 'AY' }),
      ]),
      rgroup(
        'size',
        'Size',
        [
          rnumber('ui.deck.shape_height', {
            label: 'Height',
            icon: 'MoveVertical',
            unit: 'pt',
            min: 1,
            max: 4000,
            step: 1,
            keyTip: 'H',
          }),
          rnumber('ui.deck.shape_width', {
            label: 'Width',
            icon: 'MoveHorizontal',
            unit: 'pt',
            min: 1,
            max: 4000,
            step: 1,
            keyTip: 'W',
          }),
        ],
        { launcher: { command: 'ui.deck.size_position', label: 'Size and Position…' } },
      ),
    ],
    { keyTip: 'JD', contextual: { set: 'drawingTools', label: 'Drawing Tools', context: 'shape' } },
  );

/** Crop ▾: Crop to Shape, Aspect Ratio, Fill and Fit. */
const CROP_MENU: readonly RibbonMenuEntry[] = [
  mitem('suite.picture.crop'),
  msub(
    'Crop to Shape',
    DECK_QUICK_SHAPES.map((sh) =>
      mitem('deck.drawing.change_shape', { label: sh.label, args: { preset: sh.preset } }),
    ),
  ),
  msub(
    'Aspect Ratio',
    (
      [
        ['Square', [1, 1]],
        ['Portrait', [2, 3], [3, 4], [3, 5], [4, 5]],
        ['Landscape', [3, 2], [4, 3], [5, 3], [5, 4], [16, 9], [16, 10]],
      ] as const
    ).flatMap(([group, ...ratios]) => [
      mhead(group),
      ...ratios.map(([width, height]) =>
        mitem('deck.picture.crop_to', { label: `${width}:${height}`, args: { width, height } }),
      ),
    ]),
  ),
  msep(),
  mitem('deck.picture.crop_to', { label: 'Fill', args: { fit: 'Fill' } }),
  mitem('deck.picture.crop_to', { label: 'Fit', args: { fit: 'Fit' } }),
  msep(),
  mitem('ui.deck.crop', { label: 'Crop by Values…' }),
];

/** Convert to List Diagram ▾. */
const LIST_DIAGRAM_MENU: readonly RibbonMenuEntry[] = [
  mitem('deck.paragraph.convert_to_diagram', { label: 'Basic Block List', args: { layout: 'BlockList' } }),
  mitem('deck.paragraph.convert_to_diagram', { label: 'Basic Process', args: { layout: 'Process' } }),
  mitem('deck.paragraph.convert_to_diagram', {
    label: 'Vertical Bullet List',
    args: { layout: 'VerticalList' },
  }),
];

/** Cell alignment: PowerPoint's six buttons as nine positions. */
const CELL_ALIGN = (
  [
    ['Top', 'Top'],
    ['Center', 'Middle'],
    ['Bottom', 'Bottom'],
  ] as const
).flatMap(([vertical, v]) =>
  (['Left', 'Center', 'Right'] as const).map(
    (horizontal) =>
      [vertical, horizontal, `${v} ${horizontal === 'Center' ? 'Centre' : horizontal}`] as const,
  ),
);

/** Cell Margins ▾: PowerPoint's four presets (points), then the dialog. */
const CELL_MARGINS_MENU: readonly RibbonMenuEntry[] = [
  ...(
    [
      ['Normal', 7.2, 3.6],
      ['None', 0, 0],
      ['Narrow', 3.6, 3.6],
      ['Wide', 10.8, 10.8],
    ] as const
  ).map(([label, side, edge]) =>
    mitem('deck.table.cell_margins', { label, args: { left: side, right: side, top: edge, bottom: edge } }),
  ),
  msep(),
  mitem('ui.deck.cell_margins', { label: 'Custom Margins…' }),
];

/** Table Layout (contextual, while the caret is in a table). */
const tableLayoutTabDeck = () =>
  rtab(
    'tableLayout',
    'Layout',
    [
      rgroup('table', 'Table', [
        // PowerPoint's Select ▾ ▸ Select Table: every cell, for Table Layout to act on.
        rdropdown(
          undefined,
          [
            mitem('deck.table.select_cells', {
              label: 'Select Table',
              args: { row: 0, col: 0, rows: 0, cols: 0 },
            }),
          ],
          { id: 'deck.table_select', label: 'Select', icon: 'SelectObjects', size: 'large' },
        ),
      ]),
      rgroup('rowsColumns', 'Rows & Columns', [
        rdropdown(
          'suite.table.delete',
          [
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
        rbutton('ui.deck.split_cells', { label: 'Split Cells', size: 'medium', keyTip: 'P' }),
      ]),
      rgroup('cellSize', 'Cell Size', [
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
        rdropdown('deck.table.cell_margins', CELL_MARGINS_MENU, { size: 'large', keyTip: 'M' }),
      ]),
    ],
    { keyTip: 'JL', contextual: { set: 'tableTools', label: 'Table Tools', context: 'table' } },
  );

/** Picture Format (contextual, while a picture is selected). */
const pictureFormatTabDeck = () =>
  rtab(
    'pictureFormat',
    'Picture Format',
    [
      rgroup('adjust', 'Adjust', [
        // Corrections: the parity row is P2 (deck.picture-format.adjust).
        rbutton('suite.picture.corrections', { size: 'large', tier: 'P2' }),
        rbutton('deck.picture.reset', { size: 'medium', keyTip: 'Q' }),
        rbutton('ui.deck.change_picture', {
          label: 'Change Picture',
          icon: 'ImagePlus',
          size: 'medium',
          keyTip: 'CP',
        }),
      ]),
      rgroup('pictureStyles', 'Picture Styles', [
        rsplit(
          'deck.drawing.shape_outline',
          [
            mitem('deck.drawing.shape_outline', { label: 'No Outline', args: { none: true } }),
            msub('Weight', WEIGHT_MENU),
          ],
          { id: 'deck.picture_border', label: 'Picture Border', size: 'small', keyTip: 'SO' },
        ),
        rdropdown('deck.drawing.shape_effects', EFFECTS_MENU, {
          id: 'deck.picture_effects',
          label: 'Picture Effects',
          size: 'small',
          keyTip: 'SE',
        }),
      ]),
      rgroup('accessibility', 'Accessibility', [
        rbutton('ui.deck.alt_text', {
          label: 'Alt Text',
          icon: 'Accessibility',
          size: 'large',
          keyTip: 'AT',
        }),
      ]),
      rgroup('arrange', 'Arrange', [
        rsplit('deck.arrange.order', ORDER_MENU.slice(0, 2), {
          id: 'deck.picture_forward',
          label: 'Bring Forward',
          icon: 'BringForward',
          args: { order: 'Forward' },
          size: 'medium',
        }),
        rsplit('deck.arrange.order', ORDER_MENU.slice(2), {
          id: 'deck.picture_backward',
          label: 'Send Backward',
          icon: 'SendBackward',
          args: { order: 'Backward' },
          size: 'medium',
        }),
        rdropdown('deck.arrange.align', ALIGN_MENU, { id: 'deck.picture_align', size: 'small' }),
        rdropdown('deck.arrange.rotate', ROTATE_MENU, { id: 'deck.picture_rotate', size: 'small' }),
      ]),
      rgroup('size', 'Size', [
        rsplit('suite.picture.crop', CROP_MENU, { size: 'large', keyTip: 'C' }),
        rnumber('ui.deck.shape_height', {
          id: 'deck.picture_height',
          label: 'Height',
          icon: 'MoveVertical',
          unit: 'pt',
          min: 1,
          max: 4000,
          step: 1,
        }),
        rnumber('ui.deck.shape_width', {
          id: 'deck.picture_width',
          label: 'Width',
          icon: 'MoveHorizontal',
          unit: 'pt',
          min: 1,
          max: 4000,
          step: 1,
        }),
      ]),
    ],
    { keyTip: 'JP', contextual: { set: 'pictureTools', label: 'Picture Tools', context: 'picture' } },
  );

/** Shapes ▾: the common shapes (the next drag on the slide draws one) and the full gallery. */
const SHAPES_MENU = [
  ...DECK_QUICK_SHAPES.map((sh) =>
    mitem('deck.stage.arm_insert', { label: sh.label, args: { preset: sh.preset } }),
  ),
  msep(),
  mitem('suite.insert.shapes', { label: 'All Shapes…' }),
];

/** Quick Styles ▾: seven rows of the six theme accents. */
const QUICK_STYLES_MENU = STYLE_ROWS.map((row, r) =>
  msub(
    row,
    ACCENTS.map((_, a) =>
      mitem('deck.drawing.quick_styles', { label: `${row}, Accent ${a + 1}`, args: { index: r * 6 + a } }),
    ),
  ),
);

let shapesCount = 0;
const shapesGallery = (keyTip: string) =>
  rsplit('suite.insert.shapes', SHAPES_MENU, {
    id: `deck.shapes${shapesCount++ || ''}`,
    label: 'Shapes',
    icon: 'Shapes',
    size: 'large',
    keyTip,
  });

const quickStyles = (id: string, keyTip: string) =>
  rdropdown('deck.drawing.quick_styles', QUICK_STYLES_MENU, { id, size: 'medium', keyTip });

/**
 * Slide Master (View ▸ Slide Master): the tab in front while the view is on, with PowerPoint's
 * Edit Master, Master Layout, Edit Theme, Background, Size and Close groups.
 */
export const DECK_SLIDE_MASTER_TAB = rtab(
  'slideMaster',
  'Slide Master',
  [
    rgroup('editMaster', 'Edit Master', [
      rbutton('deck.master.insert_master', { size: 'large', keyTip: 'IM' }),
      rbutton('deck.master.insert_layout', { size: 'large', keyTip: 'IL' }),
      rbutton('deck.master.delete', { size: 'small', label: 'Delete', keyTip: 'D' }),
      rbutton('ui.deck.rename_master', { size: 'small', label: 'Rename', keyTip: 'R' }),
      rtoggle('deck.master.preserve', { size: 'small', label: 'Preserve', keyTip: 'P' }),
    ]),
    rgroup('masterLayout', 'Master Layout', [
      rdropdown(
        'deck.master.insert_placeholder',
        (['Content', 'Text', 'Picture', 'Chart', 'Table', 'Media'] as const).map((kind) =>
          mitem('deck.master.insert_placeholder', { label: kind, args: { kind } }),
        ),
        { size: 'large', keyTip: 'IP' },
      ),
      rtoggle('deck.master.title', { size: 'small', checkbox: true, keyTip: 'TI' }),
      rtoggle('deck.master.footers', { size: 'small', checkbox: true, keyTip: 'F' }),
    ]),
    rgroup('editTheme', 'Edit Theme', [
      rgallery('deck.design.themes', THEMES, {
        id: 'deck.master_themes',
        columns: 3,
        itemWidth: 64,
        itemHeight: 52,
        keyTip: 'TH',
      }),
      rdropdown('deck.design.colors', scheme('deck.design.colors', ['Grayscale']), {
        size: 'small',
        label: 'Colors',
        keyTip: 'TC',
      }),
      rdropdown('deck.design.fonts', scheme('deck.design.fonts'), {
        size: 'small',
        label: 'Fonts',
        keyTip: 'TF',
      }),
      rdropdown('deck.design.effects', scheme('deck.design.effects'), {
        size: 'small',
        label: 'Effects',
        keyTip: 'TE',
      }),
    ]),
    rgroup('background', 'Background', [
      rdropdown('deck.design.background_style', BACKGROUND_STYLES, {
        size: 'small',
        label: 'Background Styles',
        keyTip: 'B',
      }),
      rtoggle('deck.design.hide_background_graphics', {
        id: 'deck.master_hide_background',
        size: 'small',
        checkbox: true,
        keyTip: 'HB',
      }),
    ]),
    rgroup('size', 'Size', [
      rdropdown('deck.design.slide_size', SLIDE_SIZE_MENU, {
        id: 'deck.master_slide_size',
        size: 'large',
        keyTip: 'S',
      }),
    ]),
    rgroup('close', 'Close', [
      rbutton('deck.master.view', {
        size: 'large',
        label: 'Close Master View',
        args: { on: false },
        keyTip: 'C',
      }),
    ]),
  ],
  { keyTip: 'M' },
);

/** Kiln Deck's ribbon. The editor fills in the layouts of the presentation (New Slide, Layout). */
export const deckRibbon: RibbonData = {
  app: 'deck',
  tabs: [
    rtab(
      'home',
      'Home',
      [
        clipboardGroup([mitem('deck.clipboard.paste_picture', { label: 'Picture' })]),
        rgroup(
          'slides',
          'Slides',
          [
            rsplit(
              'deck.slides.new_slide',
              [
                mitem('deck.slides.new_slide'),
                mitem('deck.slides.duplicate'),
                msep(),
                mitem('deck.insert.reuse_slides'),
              ],
              { id: 'deck.new_slide', size: 'large', keyTip: 'I' },
            ),
            rdropdown('deck.slides.layout', [], { id: 'deck.layout', size: 'medium', keyTip: 'L' }),
            rbutton('deck.slides.reset_slide', { size: 'medium', keyTip: '8' }),
            rdropdown(
              'deck.slides.section',
              [
                mitem('deck.slides.section', { label: 'Add Section' }),
                mitem('ui.deck.rename_section', { label: 'Rename Section…' }),
                mitem('ui.deck.remove_section', { label: 'Remove Section' }),
              ],
              { size: 'medium', keyTip: 'T1' },
            ),
          ],
          { icon: 'NewSlide', priority: 3 },
        ),
        fontGroup({ spacing: true }),
        paragraphGroup({
          second: [
            rdropdown('deck.paragraph.text_direction', DIRECTION_MENU, {
              hideInSimplified: true,
              keyTip: 'AD',
            }),
            rdropdown('deck.paragraph.align_text', ALIGN_TEXT_MENU, { hideInSimplified: true, keyTip: 'AT' }),
            rdropdown('deck.paragraph.columns', COLUMNS_MENU, { hideInSimplified: true, keyTip: 'AM' }),
            rdropdown('deck.paragraph.convert_to_diagram', LIST_DIAGRAM_MENU, {
              hideInSimplified: true,
              keyTip: 'M',
            }),
          ],
        }),
        rgroup(
          'drawing',
          'Drawing',
          [
            shapesGallery('SH'),
            rdropdown(undefined, ARRANGE_MENU, {
              id: 'deck.arrange',
              label: 'Arrange',
              icon: 'Arrange',
              size: 'medium',
              keyTip: 'G',
            }),
            quickStyles('deck.quick_styles', 'Q'),
            rcolor('deck.drawing.shape_fill', {
              allowNone: true,
              defaultColor: '#E05A27',
              keyTip: 'SF',
              size: 'small',
            }),
            rcolor('deck.drawing.shape_outline', {
              allowNone: true,
              defaultColor: '#2B1C14',
              keyTip: 'SO',
              size: 'small',
            }),
            rdropdown('deck.drawing.shape_effects', EFFECTS_MENU, { size: 'small', keyTip: 'SE' }),
          ],
          {
            icon: 'Shapes',
            priority: 2,
            launcher: { command: 'ui.deck.format_shape', label: 'Format Shape…' },
          },
        ),
        rgroup(
          'editing',
          'Editing',
          [
            rbutton('suite.edit.find', { size: 'medium', keyTip: 'FD' }),
            rsplit(
              'suite.edit.replace',
              [mitem('suite.edit.replace'), mitem('ui.deck.replace_fonts', { label: 'Replace Fonts…' })],
              {
                size: 'medium',
                keyTip: 'R',
              },
            ),
            rdropdown(
              undefined,
              [mitem('suite.edit.select_all'), mitem('ui.deck.selection_pane', { label: 'Selection Pane…' })],
              { id: 'deck.select', label: 'Select', icon: 'SelectObjects', size: 'medium', keyTip: 'SL' },
            ),
          ],
          { icon: 'Search', priority: 0 },
        ),
        rgroup(
          'kiln',
          'Kiln',
          [rtoggle('deck.drawing.design_ideas', { size: 'large', label: 'Ideas', keyTip: 'DI' })],
          { icon: 'Sparkles' },
        ),
      ],
      { keyTip: 'H' },
    ),
    rtab(
      'insert',
      'Insert',
      [
        rgroup('slides', 'Slides', [
          rsplit('deck.slides.new_slide', [mitem('deck.slides.new_slide'), mitem('deck.slides.duplicate')], {
            id: 'deck.insert_new_slide',
            size: 'large',
            keyTip: 'I',
          }),
          rtoggle('deck.insert.reuse_slides', { size: 'medium', keyTip: 'RS' }),
        ]),
        rgroup('tables', 'Tables', [rbutton('suite.insert.table', { size: 'large', keyTip: 'T' })]),
        rgroup('images', 'Images', [
          rbutton('suite.insert.pictures', { size: 'large', keyTip: 'P' }),
          // Screenshot: the parity row is P2 (deck.insert.images.screenshot).
          rbutton('suite.insert.screenshot', { size: 'medium', keyTip: 'SC', tier: 'P2' }),
        ]),
        rgroup('illustrations', 'Illustrations', [
          shapesGallery('SH'),
          rbutton('suite.insert.icons', { size: 'medium', keyTip: 'Y1' }),
          rbutton('ui.deck.insert_chart', { size: 'medium', keyTip: 'C' }),
        ]),
        rgroup('links', 'Links', [rbutton('suite.insert.link', { size: 'large' })]),
        rgroup('comments', 'Comments', [
          rbutton('suite.review.new_comment', { size: 'large', label: 'Comment', keyTip: 'L' }),
        ]),
        rgroup('text', 'Text', [
          rbutton('suite.insert.text_box', { size: 'large', keyTip: 'X' }),
          rbutton('suite.insert.header_footer', { size: 'medium', keyTip: 'H' }),
          rbutton('suite.insert.date_time', { size: 'medium', keyTip: 'D' }),
          rbutton('deck.insert.slide_number', { size: 'medium', keyTip: 'SN' }),
          rbutton('suite.insert.word_art', { size: 'small', keyTip: 'W', tier: 'P2' }),
        ]),
        rgroup('symbols', 'Symbols', [
          rbutton('suite.insert.equation', { size: 'medium', keyTip: 'E', tier: 'P1' }),
          rbutton('suite.insert.symbol', { size: 'medium', keyTip: 'U' }),
        ]),
        rgroup('media', 'Media', [
          rbutton('deck.insert.video', { size: 'large', keyTip: 'V' }),
          rbutton('deck.insert.audio', { size: 'large', keyTip: 'O' }),
        ]),
        rgroup('kiln', 'Kiln', [
          rbutton('suite.insert.code_block', { size: 'medium', keyTip: 'ZK' }),
          rbutton('deck.insert.live_grid_range', { size: 'medium', keyTip: 'ZG' }),
        ]),
      ],
      { keyTip: 'N' },
    ),
    rtab(
      'design',
      'Design',
      [
        rgroup(
          'themes',
          'Themes',
          [
            rgallery('deck.design.themes', THEMES, {
              columns: 6,
              itemWidth: 76,
              itemHeight: 60,
              keyTip: 'H',
              footer: [mitem('ui.deck.browse_themes', { label: 'Browse for Themes…' })],
            }),
          ],
          { priority: 2 },
        ),
        rgroup('variants', 'Variants', [
          rgallery('deck.design.variants', VARIANTS, {
            columns: 4,
            itemWidth: 64,
            itemHeight: 52,
            keyTip: 'V',
            footer: VARIANT_MENUS,
          }),
        ]),
        rgroup('customize', 'Customize', [
          rdropdown('deck.design.slide_size', SLIDE_SIZE_MENU, { size: 'large', keyTip: 'S' }),
          rbutton('ui.deck.format_background', {
            label: 'Format Background',
            icon: 'FormatBackground',
            size: 'large',
            keyTip: 'B',
          }),
          rtoggle('deck.design.hide_background_graphics', { size: 'small', checkbox: true }),
        ]),
        rgroup('designer', 'Designer', [
          rtoggle('deck.drawing.design_ideas', { size: 'large', keyTip: 'DI' }),
        ]),
      ],
      { keyTip: 'G' },
    ),
    rtab(
      'transitions',
      'Transitions',
      [
        rgroup('preview', 'Preview', [rbutton('deck.transitions.preview', { size: 'large', keyTip: 'P' })]),
        rgroup(
          'transition',
          'Transition to This Slide',
          [
            rgallery('deck.transitions.gallery', TRANSITIONS, {
              columns: 7,
              itemWidth: 60,
              itemHeight: 58,
              keyTip: 'T',
            }),
            rdropdown('deck.transitions.effect_options', [], { size: 'large', keyTip: 'EO' }),
          ],
          { priority: 2 },
        ),
        rgroup('timing', 'Timing', [
          rnumber('deck.transitions.duration', {
            unit: '',
            size: 'medium',
            min: 0.01,
            max: 59,
            step: 0.25,
          }),
          rbutton('deck.transitions.apply_to_all', { size: 'medium', keyTip: 'L' }),
          rtoggle('deck.transitions.on_click', { size: 'small', checkbox: true, keyTip: 'M' }),
          rtoggle('ui.deck.advance_after', { size: 'small', checkbox: true, label: 'After', keyTip: 'AF' }),
          rnumber('deck.transitions.after', {
            unit: '',
            size: 'small',
            min: 0,
            max: 86399,
            step: 1,
            label: 'Seconds',
          }),
        ]),
      ],
      { keyTip: 'K' },
    ),
    rtab(
      'animations',
      'Animations',
      [
        rgroup('preview', 'Preview', [rbutton('deck.animations.preview', { size: 'large', keyTip: 'P' })]),
        rgroup(
          'animation',
          'Animation',
          [
            rgallery('deck.animations.gallery', ANIMATIONS, {
              columns: 7,
              itemWidth: 60,
              itemHeight: 58,
              keyTip: 'S',
            }),
            rdropdown('deck.animations.effect_options', [], { size: 'large', keyTip: 'EM' }),
          ],
          {
            priority: 2,
          },
        ),
        rgroup('advanced', 'Advanced Animation', [
          rdropdown('deck.animations.add_animation', ADD_ANIMATION, { size: 'large', keyTip: 'AA' }),
          rtoggle('deck.animations.animation_pane', { size: 'medium', keyTip: 'C' }),
          rtoggle('deck.animations.animation_painter', { size: 'medium' }),
        ]),
        rgroup('timing', 'Timing', [
          rcombo('deck.animations.start', START_OPTIONS, { width: 116, placeholder: 'Start' }),
          rnumber('deck.animations.duration', {
            unit: '',
            size: 'medium',
            min: 0.01,
            max: 59,
            step: 0.25,
            keyTip: 'D',
          }),
          rnumber('deck.animations.delay', {
            unit: '',
            size: 'medium',
            min: 0,
            max: 59,
            step: 0.25,
          }),
          rbutton('deck.animations.reorder', {
            size: 'medium',
            label: 'Move Earlier',
            icon: 'ChevronUp',
            args: { earlier: true },
            id: 'deck.animations.move_earlier',
            keyTip: 'ME',
          }),
          rbutton('deck.animations.reorder', {
            size: 'medium',
            label: 'Move Later',
            icon: 'ChevronDown',
            args: { earlier: false },
            id: 'deck.animations.move_later',
            keyTip: 'ML',
          }),
        ]),
      ],
      { keyTip: 'A' },
    ),
    rtab(
      'slideShow',
      'Slide Show',
      [
        rgroup('start', 'Start Slide Show', [
          rbutton('deck.show.from_beginning', { size: 'large', keyTip: 'B' }),
          rbutton('deck.show.from_current', { size: 'large', keyTip: 'C' }),
          rbutton('deck.show.in_tab', { size: 'medium', keyTip: 'ZT' }),
        ]),
        rgroup('setUp', 'Set Up', [
          rbutton('deck.show.set_up', { size: 'large', keyTip: 'S' }),
          rtoggle('deck.show.hide_slide', { size: 'medium', keyTip: 'H' }),
          rbutton('deck.show.rehearse_timings', { size: 'medium', keyTip: 'T', tier: 'P1' }),
        ]),
        rgroup('monitors', 'Monitors', [
          rtoggle('deck.show.presenter_view', { size: 'medium', keyTip: 'V' }),
        ]),
        rgroup('audience', 'Audience', [
          rbutton('deck.show.audience_laser', { size: 'medium', keyTip: 'ZL' }),
          rbutton('deck.show.audience_pen', { size: 'medium', keyTip: 'ZP' }),
          rbutton('deck.show.audience_highlighter', { size: 'medium', keyTip: 'ZH' }),
          rbutton('deck.show.audience_blackout', { size: 'medium', keyTip: 'ZB' }),
        ]),
      ],
      { keyTip: 'S' },
    ),
    rtab(
      'review',
      'Review',
      [
        rgroup('proofing', 'Proofing', [rbutton('suite.review.spelling', { size: 'large', keyTip: 'S' })]),
        rgroup('comments', 'Comments', [
          rbutton('suite.review.new_comment', { size: 'large', label: 'New Comment', keyTip: 'C' }),
          rsplit(
            'suite.review.delete_comment',
            [
              mitem('suite.review.delete_comment', { label: 'Delete' }),
              mitem('deck.comment.delete', {
                label: 'Delete All Comments on This Slide',
                args: { scope: 'Slide' },
              }),
              mitem('deck.comment.delete', {
                label: 'Delete All Comments in This Presentation',
                args: { scope: 'All' },
              }),
            ],
            { size: 'medium', keyTip: 'D' },
          ),
          rbutton('suite.review.previous_comment', { size: 'medium', keyTip: 'V' }),
          rbutton('suite.review.next_comment', { size: 'medium', keyTip: 'N' }),
          rtoggle('suite.review.show_comments', { size: 'medium', keyTip: 'H' }),
        ]),
        rgroup('compare', 'Compare', [rbutton('suite.review.compare', { size: 'large', keyTip: 'M' })], {
          tier: 'P2',
        }),
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
        rgroup('presentationViews', 'Presentation Views', [
          viewMode('SlideNormal', 'Normal', 'NormalView', 'N'),
          rtoggle('deck.view.outline_view', { size: 'large', keyTip: 'PO' }),
          viewMode('SlideSorter', 'Slide Sorter', 'SlideSorter', 'I'),
          viewMode('NotesPage', 'Notes Page', 'NotesPage', 'T'),
          viewMode('ReadingView', 'Reading View', 'ReadingView', 'D'),
        ]),
        rgroup('masterViews', 'Master Views', [
          rbutton('deck.view.slide_master', { size: 'large', keyTip: 'M' }),
          rbutton('deck.view.handout_master', { size: 'medium', keyTip: 'H' }),
          rbutton('deck.view.notes_master', { size: 'medium', keyTip: 'O' }),
        ]),
        rgroup('show', 'Show', [
          rtoggle('suite.view.ruler', { size: 'small', checkbox: true, keyTip: 'R' }),
          rtoggle('deck.view.gridlines', { size: 'small', checkbox: true, keyTip: 'G' }),
          rtoggle('deck.view.guides', { size: 'small', checkbox: true, keyTip: 'U' }),
          rtoggle('deck.view.notes', { size: 'small', checkbox: true }),
        ]),
        rgroup('zoom', 'Zoom', [
          zoomMenu(),
          rbutton('deck.view.fit_to_window', { size: 'large', keyTip: 'F' }),
        ]),
        rgroup(
          'color',
          'Color/Grayscale',
          [rbutton('deck.view.color_grayscale', { size: 'large', keyTip: 'C' })],
          { tier: 'P2' },
        ),
        // New Window and View Side by Side: a second window of the same presentation is the
        // desktop's, as in Write and Grid.
        rgroup(
          'window',
          'Window',
          [
            rbutton('suite.view.new_window', { size: 'large', keyTip: 'NW' }),
            rbutton('suite.view.side_by_side', { size: 'small', keyTip: 'B' }),
          ],
          { tier: 'P2' },
        ),
        accessibilityGroup(null),
        themeGroup(),
        macrosGroup(null),
      ],
      { keyTip: 'W' },
    ),
    shapeFormatTabDeck(),
    pictureFormatTabDeck(),
    ...tableTabs('deck').slice(0, 1),
    tableLayoutTabDeck(),
  ],
};

/**
 * The ids of Deck's ribbon that the Deck editor implements in its chrome (dialogs, panes, the
 * show, toggles over core commands); `@kiln/editor-deck` tests that it registers each of them.
 */
export const DECK_CHROME_COMMANDS: readonly string[] = [
  // Home
  'suite.edit.paste_special',
  'suite.edit.format_painter',
  'suite.format.copy_formatting',
  'suite.format.paste_formatting',
  'suite.edit.find',
  'suite.edit.replace',
  'suite.edit.select_all',
  'suite.format.font_dialog',
  'suite.format.paragraph_dialog',
  'suite.format.character_spacing',
  'suite.format.text_shadow',
  'deck.drawing.design_ideas',
  // Insert
  'deck.insert.reuse_slides',
  'deck.insert.video',
  'deck.insert.audio',
  'suite.insert.table',
  'suite.insert.shapes',
  'suite.insert.icons',
  'suite.insert.link',
  'suite.insert.text_box',
  'suite.insert.header_footer',
  'suite.insert.date_time',
  'suite.insert.equation',
  'suite.insert.symbol',
  'suite.insert.code_block',
  // Transitions, Animations, Slide Show
  'deck.transitions.preview',
  'deck.animations.preview',
  'deck.animations.animation_pane',
  'deck.animations.animation_painter',
  'deck.show.from_beginning',
  'deck.show.from_current',
  'deck.show.in_tab',
  'deck.show.set_up',
  'deck.show.rehearse_timings',
  'deck.show.presenter_view',
  'deck.show.audience_laser',
  'deck.show.audience_pen',
  'deck.show.audience_highlighter',
  'deck.show.audience_blackout',
  // Review
  'suite.review.spelling',
  'suite.review.new_comment',
  'suite.review.delete_comment',
  'suite.review.previous_comment',
  'suite.review.next_comment',
  'suite.review.show_comments',
  // View
  'deck.view.outline_view',
  'deck.view.slide_master',
  'deck.view.fit_to_window',
  'suite.view.ruler',
  // Picture Format
  'suite.picture.crop',
];

// ── Builders: id + args → the core's typed Command ─────────────────────────────────

type Args = Readonly<Record<string, unknown>>;

/** The arguments of a Deck command do not have the shape it needs. */
export class DeckArgsError extends Error {
  override readonly name = 'CommandArgsError';
  constructor(
    readonly id: string,
    message: string,
  ) {
    super(`${id}: ${message}`);
  }
}

const isStr = (v: unknown): v is string => typeof v === 'string';
const isNum = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const isUint = (v: unknown): v is number => isNum(v) && Number.isInteger(v) && v >= 0 && v <= 0xffffffff;
const isInt = (v: unknown): v is number => isNum(v) && Number.isInteger(v);
const isBool = (v: unknown): v is boolean => typeof v === 'boolean';
const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

function want<T>(id: string, a: Args, key: string, ok: (v: unknown) => v is T, what: string): T {
  const v = a[key];
  if (!ok(v)) throw new DeckArgsError(id, `"${key}" must be ${what}`);
  return v;
}

function maybe<T>(id: string, a: Args, key: string, ok: (v: unknown) => v is T, what: string): T | null {
  const v = a[key];
  if (v === undefined || v === null) return null;
  if (!ok(v)) throw new DeckArgsError(id, `"${key}" must be ${what} or absent`);
  return v;
}

const flag = (id: string, a: Args, key: string) => maybe(id, a, key, isBool, 'true or false') ?? false;

/** `key`, else the gallery's `value`. */
function wantOr<T>(id: string, a: Args, key: string, ok: (v: unknown) => v is T, what: string): T {
  return a[key] === undefined && a.value !== undefined
    ? want(id, a, 'value', ok, what)
    : want(id, a, key, ok, what);
}

/** One of `choices`, matched without regard to case (`up` and `Up`). */
function choice<T extends string>(id: string, a: Args, key: string, choices: readonly T[]): T {
  const v = a[key] ?? a.value;
  const hit = isStr(v) ? choices.find((c) => c.toLowerCase() === v.toLowerCase()) : undefined;
  if (!hit) throw new DeckArgsError(id, `"${key}" must be one of ${choices.join(', ')}`);
  return hit;
}

const isRect = (v: unknown): v is DeckRect =>
  isObj(v) && isNum(v.x) && isNum(v.y) && isNum(v.width) && isNum(v.height);

/**
 * A colour as the core takes it: `RRGGBB`, or a theme colour's scheme name (`accent1`, `tx1`)
 * with its brightness, from a colour picker's choice or a string.
 */
function colourOf(id: string, v: unknown): { color: string; brightness: number | null } | null {
  if (v === undefined || v === null) return null;
  if (isStr(v)) {
    const s = v.replace(/^#/, '');
    if (/^[0-9a-f]{6}$/i.test(s)) return { color: s.toUpperCase(), brightness: null };
    if (/^[a-z]+[0-9]?$/.test(s)) return { color: s, brightness: null };
    throw new DeckArgsError(id, '"color" must be RRGGBB or a theme colour');
  }
  if (isObj(v) && (v.type === 'auto' || v.type === 'none')) return null;
  if (isObj(v) && v.type === 'theme' && isStr(v.slot))
    return { color: v.slot, brightness: isNum(v.tint) ? Math.round(v.tint * 100) : null };
  if (isObj(v) && isStr(v.hex)) return colourOf(id, v.hex);
  throw new DeckArgsError(id, '"color" must be a colour');
}

function bytesOf(id: string, v: unknown): number[] {
  if (v instanceof Uint8Array) return Array.from(v);
  if (Array.isArray(v) && v.every((b) => isUint(b) && b < 256)) return v as number[];
  throw new DeckArgsError(id, '"bytes" must be the bytes of a file');
}

/** A fill from the chrome: `{ fill }` as the protocol has it, or `{ color }`, `{ none: true }`. */
function fillOf(id: string, a: Args): DeckFill {
  if (isObj(a.fill) || isStr(a.fill)) return a.fill as DeckFill;
  if (a.none === true) return 'None';
  if (isObj(a.color) && a.color.type === 'none') return 'None';
  if (isObj(a.gradient)) {
    const g = a.gradient;
    const from = colourOf(id, g.from);
    const to = colourOf(id, g.to);
    if (!from || !to) throw new DeckArgsError(id, 'a gradient needs two colours');
    return { Gradient: { from: from.color, to: to.color, angle: isNum(g.angle) ? g.angle : 90 } };
  }
  if (isStr(a.name) && a.bytes !== undefined)
    return { Picture: { name: a.name, bytes: bytesOf(id, a.bytes) } };
  const c = colourOf(id, a.color ?? a.value);
  if (!c) return 'None';
  return {
    Solid: {
      color: c.color,
      brightness: maybe(id, a, 'brightness', isNum, 'a number') ?? c.brightness,
      transparency: maybe(id, a, 'transparency', isNum, 'a percentage'),
    },
  };
}

function outlineOf(id: string, a: Args): DeckOutline {
  if (isObj(a.outline) || isStr(a.outline)) return a.outline as DeckOutline;
  if (a.none === true || (isObj(a.color) && a.color.type === 'none')) return 'None';
  if (isNum(a.weight)) return { Weight: { points: a.weight } };
  if (isStr(a.dash)) return { Dash: { dash: a.dash } };
  if (isStr(a.head) || isStr(a.tail))
    return { Arrows: { head: isStr(a.head) ? a.head : 'none', tail: isStr(a.tail) ? a.tail : 'none' } };
  const c = colourOf(id, a.color ?? a.value);
  return c ? { Color: { color: c.color } } : 'None';
}

function effectOf(id: string, a: Args): DeckEffect {
  if (isObj(a.effect) || isStr(a.effect)) return a.effect as DeckEffect;
  switch (choice(id, a, 'kind', ['none', 'shadow', 'noShadow', 'glow', 'softEdges'] as const)) {
    case 'none':
      return 'None';
    case 'noShadow':
      return 'NoShadow';
    case 'glow':
      return { Glow: { radius: maybe(id, a, 'radius', isNum, 'points') ?? 8 } };
    case 'softEdges':
      return { SoftEdges: { radius: maybe(id, a, 'radius', isNum, 'points') ?? 5 } };
    default:
      return {
        Shadow: {
          distance: maybe(id, a, 'distance', isNum, 'points') ?? 3,
          blur: maybe(id, a, 'blur', isNum, 'points') ?? 4,
          angle: maybe(id, a, 'angle', isNum, 'degrees') ?? 45,
        },
      };
  }
}

function slideSizeOf(id: string, a: Args): DeckSlideSize {
  if (isObj(a.size) || isStr(a.size)) return a.size as DeckSlideSize;
  if (a.ratio === '16:9') return 'Widescreen';
  if (a.ratio === '4:3') return 'Standard';
  return {
    Custom: {
      width_in: want(id, a, 'width_in', isNum, 'inches'),
      height_in: want(id, a, 'height_in', isNum, 'inches'),
    },
  };
}

const MOTIONS: readonly CaretMotion[] = [
  'Backward',
  'Forward',
  'WordBackward',
  'WordForward',
  'LineStart',
  'LineEnd',
  'LineUp',
  'LineDown',
  'DocumentStart',
  'DocumentEnd',
  'ParagraphBackward',
  'ParagraphForward',
  'PageUp',
  'PageDown',
];

const DIRECTIONS: readonly DeckDirection[] = ['Up', 'Down', 'Left', 'Right'];

type DeckBuilder = (a: Args, id: string) => DeckCommand;

const simple =
  <T extends DeckCommand & string>(c: T): DeckBuilder =>
  () =>
    c;

const pointer = (a: Args, id: string) => ({
  x: want(id, a, 'x', isNum, 'a slide x'),
  y: want(id, a, 'y', isNum, 'a slide y'),
  shift: flag(id, a, 'shift'),
  ctrl: flag(id, a, 'ctrl'),
  alt: flag(id, a, 'alt'),
});

const DECK_BUILDER_TABLE: Readonly<Record<string, DeckBuilder>> = {
  'deck.slides.new_slide': (a, id) => ({
    NewSlide: {
      layout: maybe(id, a, 'layout', isUint, 'a layout') ?? maybe(id, a, 'value', isUint, 'a layout'),
    },
  }),
  'deck.slides.duplicate': simple('Duplicate'),
  'deck.slides.delete': simple('DeleteSlides'),
  'deck.slides.move': (a, id) => ({ MoveSlides: { to: want(id, a, 'to', isUint, 'a slide index') } }),
  'deck.slides.select': (a, id) => ({
    SelectSlide: {
      index: want(id, a, 'index', isUint, 'a slide index'),
      extend: flag(id, a, 'extend'),
      toggle: flag(id, a, 'toggle'),
    },
  }),
  'deck.slides.previous_slide': simple('PreviousSlide'),
  'deck.slides.next_slide': simple('NextSlide'),
  'deck.slides.layout': (a, id) => ({ SetLayout: { layout: wantOr(id, a, 'layout', isUint, 'a layout') } }),
  'deck.slides.reset_slide': simple('ResetSlides'),
  'deck.show.hide_slide': (a, id) => ({
    HideSlides: { hidden: maybe(id, a, 'hidden', isBool, 'true or false') },
  }),
  'deck.slides.section': (a, id) => ({ AddSection: { name: maybe(id, a, 'name', isStr, 'a name') } }),
  'deck.slides.rename_section': (a, id) => ({
    RenameSection: {
      index: want(id, a, 'index', isUint, 'a section'),
      name: want(id, a, 'name', isStr, 'a name'),
    },
  }),
  'deck.slides.remove_section': (a, id) => ({
    RemoveSection: {
      index: want(id, a, 'index', isUint, 'a section'),
      with_slides: flag(id, a, 'with_slides'),
    },
  }),
  'deck.slides.move_section': (a, id) => ({
    MoveSection: {
      index: want(id, a, 'index', isUint, 'a section'),
      delta: want(id, a, 'delta', isInt, '-1 or 1'),
    },
  }),
  'deck.notes.set_text': (a, id) => ({ SetNotes: { text: want(id, a, 'text', isStr, 'text') } }),
  'deck.insert.shape': (a, id) => ({
    InsertShape: {
      preset: wantOr(id, a, 'preset', isStr, 'a shape'),
      rect: maybe(id, a, 'rect', isRect, 'a rectangle'),
    },
  }),
  'deck.insert.text_box': (a, id) => ({
    InsertTextBox: { rect: maybe(id, a, 'rect', isRect, 'a rectangle') },
  }),
  'deck.insert.picture': (a, id) => ({
    InsertPicture: {
      name: want(id, a, 'name', isStr, 'a file name'),
      bytes: bytesOf(id, a.bytes),
      rect: maybe(id, a, 'rect', isRect, 'a rectangle'),
    },
  }),
  'deck.insert.table': (a, id) => ({
    InsertTable: {
      rows: want(id, a, 'rows', isUint, 'a row count'),
      cols: want(id, a, 'cols', isUint, 'a column count'),
    },
  }),
  'deck.insert.slide_number': simple('InsertSlideNumber'),
  'deck.insert.header_footer': (a, id) => ({
    SetHeaderFooter: {
      date: flag(id, a, 'date'),
      fixed_date: maybe(id, a, 'fixed_date', isStr, 'a date'),
      slide_number: flag(id, a, 'slide_number'),
      footer: maybe(id, a, 'footer', isStr, 'text'),
      not_on_title: flag(id, a, 'not_on_title'),
      all: flag(id, a, 'all'),
    },
  }),
  'deck.insert.link': (a, id) => ({ Link: { url: maybe(id, a, 'url', isStr, 'an address') } }),
  'deck.stage.pointer_down': (a, id) => ({
    PointerDown: { ...pointer(a, id), clicks: maybe(id, a, 'clicks', isUint, 'a click count') ?? 1 },
  }),
  'deck.stage.pointer_move': (a, id) => ({ PointerMove: pointer(a, id) }),
  'deck.stage.pointer_up': (a, id) => ({ PointerUp: pointer(a, id) }),
  'deck.stage.arm_insert': (a, id) => ({
    ArmInsert: {
      preset: maybe(id, a, 'preset', isStr, 'a shape') ?? maybe(id, a, 'value', isStr, 'a shape'),
    },
  }),
  'deck.select.shapes': (a, id) => {
    const ids = a.ids;
    if (!Array.isArray(ids) || !ids.every(isUint)) throw new DeckArgsError(id, '"ids" must be shape ids');
    return { SelectShapes: { ids: ids as number[] } };
  },
  'deck.select.all': simple('SelectAll'),
  'deck.select.escape': simple('Escape'),
  'deck.select.next': (a, id) => ({ CycleSelection: { backward: flag(id, a, 'backward') } }),
  'deck.drawing.nudge': (a, id) => ({
    Nudge: { direction: choice(id, a, 'direction', DIRECTIONS), fine: flag(id, a, 'fine') },
  }),
  'deck.drawing.resize': (a, id) => ({
    Resize: { direction: choice(id, a, 'direction', DIRECTIONS), fine: flag(id, a, 'fine') },
  }),
  'deck.format.size_position': (a, id) => ({
    SetTransform: {
      x: maybe(id, a, 'x', isNum, 'points'),
      y: maybe(id, a, 'y', isNum, 'points'),
      width: maybe(id, a, 'width', isNum, 'points'),
      height: maybe(id, a, 'height', isNum, 'points'),
      rotation: maybe(id, a, 'rotation', isNum, 'degrees'),
      lock_aspect: flag(id, a, 'lock_aspect'),
    },
  }),
  'deck.arrange.rotate': (a, id) => ({ Rotate: { degrees: wantOr(id, a, 'degrees', isNum, 'degrees') } }),
  'deck.arrange.flip': (a, id) => ({ Flip: { horizontal: flag(id, a, 'horizontal') } }),
  'deck.arrange.align': (a, id) => ({
    Align: { how: choice(id, a, 'how', ['Left', 'Center', 'Right', 'Top', 'Middle', 'Bottom'] as const) },
  }),
  'deck.arrange.distribute': (a, id) => ({ Distribute: { horizontal: flag(id, a, 'horizontal') } }),
  'deck.drawing.group': simple('Group'),
  'deck.drawing.ungroup': simple('Ungroup'),
  'deck.arrange.order': (a, id) => ({
    Order: { order: choice(id, a, 'order', ['Front', 'Forward', 'Backward', 'Back'] as const) },
  }),
  'deck.edit.delete': simple('Delete'),
  'deck.drawing.shape_fill': (a, id) => ({ SetFill: { fill: fillOf(id, a) } }),
  'deck.drawing.shape_outline': (a, id) => ({ SetOutline: { outline: outlineOf(id, a) } }),
  'deck.drawing.shape_effects': (a, id) => ({ SetEffect: { effect: effectOf(id, a) } }),
  'deck.drawing.quick_styles': (a, id) => ({
    QuickStyle: {
      index:
        a.index === undefined && isStr(a.value) ? Number(a.value) : want(id, a, 'index', isUint, 'a style'),
    },
  }),
  'deck.drawing.change_shape': (a, id) => ({
    ChangeShape: { preset: wantOr(id, a, 'preset', isStr, 'a shape') },
  }),
  'deck.format.alt_text': (a, id) => ({ SetAltText: { text: want(id, a, 'text', isStr, 'text') } }),
  'deck.picture.crop': (a, id) => ({
    Crop: {
      left: maybe(id, a, 'left', isNum, 'a fraction') ?? 0,
      top: maybe(id, a, 'top', isNum, 'a fraction') ?? 0,
      right: maybe(id, a, 'right', isNum, 'a fraction') ?? 0,
      bottom: maybe(id, a, 'bottom', isNum, 'a fraction') ?? 0,
    },
  }),
  'deck.picture.reset': simple('ResetPicture'),
  'deck.picture.crop_mode': (a, id) => ({
    CropMode: { on: maybe(id, a, 'on', isBool, 'true or false') ?? true },
  }),
  'deck.picture.crop_to': (a, id) => {
    if (isNum(a.width) || isNum(a.height))
      return {
        CropTo: {
          fit: {
            Aspect: {
              width: want(id, a, 'width', isUint, 'a ratio'),
              height: want(id, a, 'height', isUint, 'a ratio'),
            },
          },
        },
      };
    return { CropTo: { fit: choice(id, a, 'fit', ['Fill', 'Fit'] as const) } };
  },
  'deck.clipboard.paste_picture': (a) => ({
    PastePicture: { data: isObj(a.data) && isStr(a.data.text) ? (a.data as unknown as ClipboardData) : null },
  }),
  'deck.insert.icon': (a, id) => {
    const paths = a.paths;
    if (!Array.isArray(paths) || !paths.every(isStr))
      throw new DeckArgsError(id, '"paths" must be SVG path data');
    return { InsertIcon: { name: want(id, a, 'name', isStr, 'a name'), paths: paths as string[] } };
  },
  'deck.insert.code_block': (a, id) => ({
    InsertCodeBlock: {
      code: want(id, a, 'code', isStr, 'code'),
      language: maybe(id, a, 'language', isStr, 'a language'),
    },
  }),
  'deck.insert.symbol': (a, id) => ({
    InsertSymbol: {
      text: want(id, a, 'text', isStr, 'characters'),
      font: maybe(id, a, 'font', isStr, 'a font'),
    },
  }),
  'deck.table.select_cells': (a, id) => ({
    SelectCells: {
      range: {
        row: want(id, a, 'row', isUint, 'a row'),
        col: want(id, a, 'col', isUint, 'a column'),
        rows: want(id, a, 'rows', isUint, 'a row count'),
        cols: want(id, a, 'cols', isUint, 'a column count'),
      },
    },
  }),
  'deck.table.cell_margins': (a, id) => ({
    CellMargins: {
      left: want(id, a, 'left', isNum, 'points'),
      top: want(id, a, 'top', isNum, 'points'),
      right: want(id, a, 'right', isNum, 'points'),
      bottom: want(id, a, 'bottom', isNum, 'points'),
    },
  }),
  'deck.paragraph.convert_to_diagram': (a, id) => ({
    ConvertToDiagram: { layout: choice(id, a, 'layout', ['BlockList', 'Process', 'VerticalList'] as const) },
  }),
  'deck.paragraph.align_text': (a, id) => ({
    AlignText: { anchor: choice(id, a, 'anchor', ['Top', 'Middle', 'Bottom'] as const) },
  }),
  'deck.paragraph.text_direction': (a, id) => ({
    TextDirection: {
      direction: choice(id, a, 'direction', ['Horizontal', 'Rotate90', 'Rotate270', 'Stacked'] as const),
    },
  }),
  'deck.paragraph.columns': (a, id) => ({
    Columns: {
      count: wantOr(id, a, 'count', isUint, 'a column count'),
      spacing: maybe(id, a, 'spacing', isNum, 'points') ?? 0,
    },
  }),
  'deck.format.autofit': (a, id) => ({
    Autofit: { mode: choice(id, a, 'mode', ['Off', 'Shrink', 'ResizeShape'] as const) },
  }),
  'deck.font.character_spacing': (a, id) => ({
    CharacterSpacing: { points: wantOr(id, a, 'points', isNum, 'points') },
  }),
  'deck.font.text_shadow': (a, id) => ({ TextShadow: { on: maybe(id, a, 'on', isBool, 'true or false') } }),
  'deck.clipboard.copy_format': simple('CopyFormat'),
  'deck.clipboard.paste_format': simple('PasteFormat'),
  'deck.slides.edit_text': simple('EditText'),
  'deck.text.insert': (a, id) => ({ InsertText: { text: want(id, a, 'text', isStr, 'text') } }),
  'deck.text.delete': (a, id) => ({
    DeleteText: { forward: flag(id, a, 'forward'), word: flag(id, a, 'word') },
  }),
  'deck.text.new_paragraph': simple('NewParagraph'),
  'deck.text.line_break': simple('LineBreak'),
  'deck.text.tab': (a, id) => ({ Tab: { shift: flag(id, a, 'shift') } }),
  'deck.text.move': (a, id) => ({
    MoveCaret: { motion: choice(id, a, 'motion', MOTIONS), extend: flag(id, a, 'extend') },
  }),
  'deck.text.composition': (a, id) => ({ SetComposition: { text: want(id, a, 'text', isStr, 'text') } }),
  'deck.text.select': (a, id) => ({
    SelectText: {
      shape: want(id, a, 'shape', isUint, 'a shape id'),
      anchor_para: want(id, a, 'anchor_para', isUint, 'a paragraph index'),
      anchor_offset: want(id, a, 'anchor_offset', isUint, 'an offset'),
      head_para: want(id, a, 'head_para', isUint, 'a paragraph index'),
      head_offset: want(id, a, 'head_offset', isUint, 'an offset'),
    },
  }),
  'deck.slides.promote': simple('Promote'),
  'deck.slides.demote': simple('Demote'),
  'deck.design.themes': (a, id) => ({ ApplyTheme: { name: wantOr(id, a, 'name', isStr, 'a theme') } }),
  'deck.design.variants': (a, id) => ({
    ApplyVariant: {
      index:
        a.index === undefined && isStr(a.value) ? Number(a.value) : want(id, a, 'index', isUint, 'a variant'),
    },
  }),
  'deck.design.slide_size': (a, id) => ({
    SlideSize: {
      size: slideSizeOf(id, a),
      mode: a.mode === undefined ? 'EnsureFit' : choice(id, a, 'mode', ['Maximize', 'EnsureFit'] as const),
    },
  }),
  'deck.design.format_background': (a, id) => ({
    SetBackground: { fill: a.reset === true ? null : fillOf(id, a), all: flag(id, a, 'all') },
  }),
  'deck.design.hide_background_graphics': (a, id) => ({
    HideBackgroundGraphics: { hide: maybe(id, a, 'hide', isBool, 'true or false') },
  }),
  'deck.view.guides': simple('ToggleGuides'),
  'deck.view.gridlines': simple('ToggleGridlines'),
  'deck.view.notes': simple('ToggleNotes'),
  'deck.edit.find': (a, id) => ({
    Find: {
      text: want(id, a, 'text', isStr, 'text'),
      match_case: flag(id, a, 'match_case'),
      whole_word: flag(id, a, 'whole_word'),
      backward: flag(id, a, 'backward'),
    },
  }),
  'deck.edit.replace': (a, id) => ({
    Replace: {
      text: want(id, a, 'text', isStr, 'text'),
      with: want(id, a, 'with', isStr, 'text'),
      match_case: flag(id, a, 'match_case'),
      whole_word: flag(id, a, 'whole_word'),
      all: flag(id, a, 'all'),
    },
  }),
  'deck.edit.replace_font': (a, id) => ({
    ReplaceFont: { from: want(id, a, 'from', isStr, 'a font'), to: want(id, a, 'to', isStr, 'a font') },
  }),
  // ── Transitions, Animations, Slide Show ──
  'deck.transitions.gallery': (a, id) => ({
    SetTransition: { effect: wantOr(id, a, 'effect', isStr, 'a transition') },
  }),
  'deck.transitions.effect_options': (a, id) => ({
    TransitionOption: { option: wantOr(id, a, 'option', isStr, 'an effect option') },
  }),
  'deck.transitions.duration': (a, id) => ({ TransitionDuration: { ms: msOf(id, a) } }),
  'deck.transitions.on_click': (a, id) => ({
    AdvanceOnClick: { on: maybe(id, a, 'on', isBool, 'true or false') },
  }),
  'deck.transitions.after': (a, id) => ({
    AdvanceAfter: { ms: a.ms === null || a.value === null || a.off === true ? null : msOf(id, a) },
  }),
  'deck.transitions.apply_to_all': simple('ApplyTransitionToAll'),
  'deck.animations.gallery': (a, id) => ({
    SetAnimation: { effect: wantOr(id, a, 'effect', isStr, 'an animation') },
  }),
  'deck.animations.add_animation': (a, id) => ({
    AddAnimation: { effect: wantOr(id, a, 'effect', isStr, 'an animation') },
  }),
  'deck.animations.effect_options': (a, id) => ({
    AnimationOption: { option: wantOr(id, a, 'option', isStr, 'an effect option') },
  }),
  'deck.animations.start': (a, id) => ({
    AnimationStart: { start: choice(id, a, 'start', ['OnClick', 'WithPrevious', 'AfterPrevious'] as const) },
  }),
  'deck.animations.duration': (a, id) => ({ AnimationDuration: { ms: msOf(id, a) } }),
  'deck.animations.delay': (a, id) => ({ AnimationDelay: { ms: msOf(id, a) } }),
  'deck.animations.repeat': (a, id) => ({
    AnimationRepeat: { count: wantOr(id, a, 'count', isUint, 'a count') },
  }),
  'deck.animations.remove': simple('RemoveAnimations'),
  'deck.animations.move': (a, id) => ({
    MoveAnimation: { id: want(id, a, 'id', isUint, 'an effect'), to: want(id, a, 'to', isUint, 'an index') },
  }),
  'deck.animations.reorder': (a, id) => ({ ReorderAnimation: { earlier: flag(id, a, 'earlier') } }),
  'deck.animations.select': (a, id) => {
    const ids = a.ids;
    if (!Array.isArray(ids) || !ids.every(isUint)) throw new DeckArgsError(id, '"ids" must be effect ids');
    return { SelectAnimations: { ids: ids as number[] } };
  },
  'deck.animations.copy': simple('CopyAnimation'),
  'deck.animations.paste': simple('PasteAnimation'),
  'deck.animations.badges': (a, id) => ({ MotionBadges: { on: want(id, a, 'on', isBool, 'true or false') } }),
  'deck.show.settings': (a, id) => ({ SetShowSettings: { settings: settingsOf(id, a) } }),
  'deck.show.start': (a, id) => ({
    ShowStart: {
      from: maybe(id, a, 'from', isUint, 'a slide index'),
      mode:
        a.mode === undefined
          ? 'Show'
          : choice(id, a, 'mode', ['Show', 'Rehearse', 'PreviewTransition', 'PreviewAnimations'] as const),
      // The system's reduced motion: transitions play as fades and effects appear at once.
      reduced_motion:
        a.reduced_motion === undefined
          ? (globalThis.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false)
          : want(id, a, 'reduced_motion', isBool, 'true or false'),
    },
  }),
  'deck.show.next': simple('ShowNext'),
  'deck.show.previous': simple('ShowPrevious'),
  'deck.show.first': simple('ShowFirst'),
  'deck.show.last': simple('ShowLast'),
  'deck.show.digit': (a, id) => {
    const d = want(id, a, 'digit', isUint, 'a digit');
    if (d > 9) throw new DeckArgsError(id, '"digit" must be 0 to 9');
    return { ShowDigit: { digit: d } };
  },
  'deck.show.go_to_typed': simple('ShowGoToTyped'),
  'deck.show.go_to': (a, id) => ({ ShowGoTo: { index: wantOr(id, a, 'index', isUint, 'a slide index') } }),
  'deck.show.black': simple('ShowBlack'),
  'deck.show.white': simple('ShowWhite'),
  'deck.show.laser': simple('ShowLaser'),
  'deck.show.pen': simple('ShowPen'),
  'deck.show.highlighter': simple('ShowHighlighter'),
  'deck.show.eraser': simple('ShowEraser'),
  'deck.show.arrow': simple('ShowArrow'),
  'deck.show.erase': simple('ShowErase'),
  'deck.show.pointer': (a, id) => ({
    ShowPointer: {
      phase: choice(id, a, 'phase', ['Down', 'Move', 'Up', 'Leave'] as const),
      x: want(id, a, 'x', isNum, 'a slide x'),
      y: want(id, a, 'y', isNum, 'a slide y'),
      touch: flag(id, a, 'touch'),
    },
  }),
  'deck.show.end': simple('ShowEnd'),
  'deck.show.keep_timings': simple('ShowKeepTimings'),
  // ── Design Ideas, Outline View, Reuse Slides ──
  'deck.designer.apply': (a, id) => ({
    ApplyDesignIdea: { id: wantOr(id, a, 'id', isStr, 'a design idea') },
  }),
  'deck.outline.edit': (a, id) => ({ EditOutline: { edit: outlineEditOf(id, a) } }),
  // ── the guides and the ruler ──
  'deck.view.add_guide': (a, id) => ({
    AddGuide: { horizontal: flag(id, a, 'horizontal'), pos: maybe(id, a, 'pos', isNum, 'slide px') },
  }),
  'deck.view.move_guide': (a, id) => ({
    MoveGuide: { index: want(id, a, 'index', isUint, 'a guide'), pos: want(id, a, 'pos', isNum, 'slide px') },
  }),
  'deck.view.remove_guide': (a, id) => ({ RemoveGuide: { index: want(id, a, 'index', isUint, 'a guide') } }),
  'deck.paragraph.indent': (a, id) => ({
    ParagraphIndent: {
      left: want(id, a, 'left', isNum, 'points'),
      first_line: want(id, a, 'first_line', isNum, 'points'),
    },
  }),
  // ── Slide Master view ──
  'deck.master.view': (a, id) => ({
    MasterView: { on: maybe(id, a, 'on', isBool, 'true or false') ?? true },
  }),
  'deck.master.select': (a, id) => ({
    SelectMasterItem: { id: want(id, a, 'id', isUint, 'a master or layout') },
  }),
  'deck.master.insert_master': simple('InsertSlideMaster'),
  'deck.master.insert_layout': simple('InsertLayout'),
  'deck.master.delete': simple('DeleteMasterItem'),
  'deck.master.rename': (a, id) => ({ RenameMasterItem: { name: want(id, a, 'name', isStr, 'a name') } }),
  'deck.master.preserve': (a, id) => ({
    PreserveMaster: { on: maybe(id, a, 'on', isBool, 'true or false') },
  }),
  'deck.master.insert_placeholder': (a, id) => ({
    InsertPlaceholder: {
      kind:
        a.kind === undefined && a.value === undefined
          ? 'Content'
          : choice(id, a, 'kind', ['Content', 'Text', 'Picture', 'Chart', 'Table', 'Media'] as const),
    },
  }),
  'deck.master.title': (a, id) => ({ LayoutTitle: { on: maybe(id, a, 'on', isBool, 'true or false') } }),
  'deck.master.footers': (a, id) => ({ LayoutFooters: { on: maybe(id, a, 'on', isBool, 'true or false') } }),
  // ── design ──
  'deck.design.colors': (a, id) => ({
    SetThemeColors: { scheme: wantOr(id, a, 'scheme', isStr, 'a scheme') },
  }),
  'deck.design.fonts': (a, id) => ({ SetThemeFonts: { scheme: wantOr(id, a, 'scheme', isStr, 'a scheme') } }),
  'deck.design.effects': (a, id) => ({
    SetThemeEffects: { scheme: wantOr(id, a, 'scheme', isStr, 'a scheme') },
  }),
  'deck.design.background_style': (a, id) => ({
    SetBackgroundStyle: { style: wantOr(id, a, 'style', isUint, 'a style from 1 to 12') },
  }),
  'deck.design.browse_themes': (a, id) => ({ ApplyThemeFile: { bytes: bytesOf(id, a.bytes) } }),
  // ── comments and spelling ──
  'deck.comment.new': (a, id) => ({
    NewComment: {
      text: want(id, a, 'text', isStr, 'text'),
      author: maybe(id, a, 'author', isStr, 'a name') ?? '',
      initials: maybe(id, a, 'initials', isStr, 'initials') ?? '',
    },
  }),
  'deck.comment.reply': (a, id) => ({
    ReplyComment: {
      id: want(id, a, 'id', isUint, 'a comment'),
      text: want(id, a, 'text', isStr, 'text'),
      author: maybe(id, a, 'author', isStr, 'a name') ?? '',
    },
  }),
  'deck.comment.edit': (a, id) => ({
    EditComment: { id: want(id, a, 'id', isUint, 'a comment'), text: want(id, a, 'text', isStr, 'text') },
  }),
  'deck.comment.resolve': (a, id) => ({
    ResolveComment: { id: want(id, a, 'id', isUint, 'a comment'), resolved: flag(id, a, 'resolved') },
  }),
  'deck.comment.delete': (a, id) => ({
    DeleteComment: {
      id: maybe(id, a, 'id', isUint, 'a comment'),
      scope: a.scope === undefined ? 'One' : choice(id, a, 'scope', ['One', 'Slide', 'All'] as const),
    },
  }),
  'deck.comment.select': (a, id) => ({ SelectComment: { id: maybe(id, a, 'id', isUint, 'a comment') } }),
  'deck.comment.next': (a, id) => ({ NextComment: { backward: flag(id, a, 'backward') } }),
  'deck.spell.check': simple('SpellCheck'),
  'deck.spell.change': (a, id) => ({
    SpellChange: { with: want(id, a, 'with', isStr, 'the replacement'), all: flag(id, a, 'all') },
  }),
  'deck.spell.ignore': (a, id) => ({ SpellIgnore: { all: flag(id, a, 'all'), add: flag(id, a, 'add') } }),
  'deck.spell.load_dictionary': (a, id) => ({
    LoadDictionary: {
      aff: want(id, a, 'aff', isStr, 'the .aff text'),
      dic: want(id, a, 'dic', isStr, 'the .dic text'),
    },
  }),
  'deck.spell.user_dictionary': (a, id) => {
    const words = a.words;
    if (!Array.isArray(words) || !words.every(isStr)) throw new DeckArgsError(id, '"words" must be words');
    return { SetUserDictionary: { words: words as string[] } };
  },
  // ── charts ──
  'deck.chart.set_data': (a, id) => {
    const cells = a.cells;
    if (!Array.isArray(cells) || !cells.every((r) => Array.isArray(r) && r.every(isStr)))
      throw new DeckArgsError(id, '"cells" must be rows of text');
    return { SetChartData: { cells: cells as string[][] } };
  },
  'deck.insert.equation': (a, id) => ({
    InsertEquation: { linear: want(id, a, 'linear', isStr, 'an equation') },
  }),
  'deck.insert.media': (a, id) => ({
    InsertMedia: {
      kind: choice(id, a, 'kind', ['Video', 'Audio'] as const),
      name: maybe(id, a, 'name', isStr, 'a file name') ?? '',
      mime: maybe(id, a, 'mime', isStr, 'a media type') ?? '',
      bytes: bytesOf(id, a.bytes),
      poster: bytesOf(id, a.poster),
      width: want(id, a, 'width', isUint, 'a width'),
      height: want(id, a, 'height', isUint, 'a height'),
    },
  }),
  'deck.insert.reuse_slides_from': (a, id) => ({
    ReuseSlides: {
      bytes: bytesOf(id, a.bytes),
      slides: Array.isArray(a.slides) ? a.slides.filter(isUint) : [],
    },
  }),
};

/** An Outline View edit: `{ edit: { Text: … } }`, or the edit itself. */
function outlineEditOf(id: string, a: Args): DeckOutlineEdit {
  const e = isObj(a.edit) ? a.edit : a;
  const [kind] = Object.keys(e);
  const v = kind ? e[kind] : undefined;
  if (!isObj(v)) throw new DeckArgsError(id, '"edit" must be an outline edit');
  const slide = want(id, v, 'slide', isUint, 'a slide index');
  const line = want(id, v, 'line', isUint, 'a line');
  switch (kind) {
    case 'Text':
      return { Text: { slide, line, text: want(id, v, 'text', isStr, 'text') } };
    case 'Split':
      return { Split: { slide, line, offset: want(id, v, 'offset', isUint, 'an offset') } };
    case 'Indent': {
      const by = want(id, v, 'by', isNum, '1 or -1');
      return { Indent: { slide, line, by: by < 0 ? -1 : 1 } };
    }
    case 'Join':
      return { Join: { slide, line } };
    default:
      throw new DeckArgsError(id, `"${kind}" is not an outline edit`);
  }
}

/** Milliseconds from `{ ms }` or a ribbon spin box's `{ value }` in seconds. */
function msOf(id: string, a: Args): number {
  if (isNum(a.ms)) return Math.max(0, Math.round(a.ms));
  const v = want(id, a, 'value', isNum, 'seconds');
  return Math.max(0, Math.round(v * 1000));
}

/** Set Up Slide Show from the dialog. */
function settingsOf(id: string, a: Args): DeckShowSettings {
  const v = a.settings;
  if (!isObj(v)) throw new DeckArgsError(id, '"settings" must be the slide show settings');
  const kind = choice(id, v, 'kind', ['Speaker', 'Window', 'Kiosk'] as const);
  const slide = (k: string) => {
    const n = v[k];
    if (n === undefined || n === null) return null;
    if (!isUint(n) || n < 1) throw new DeckArgsError(id, `"${k}" must be a slide number`);
    return n;
  };
  return {
    kind,
    loop_show: flag(id, v, 'loop_show'),
    no_narration: flag(id, v, 'no_narration'),
    no_animation: flag(id, v, 'no_animation'),
    from: slide('from'),
    to: slide('to'),
    manual: flag(id, v, 'manual'),
    presenter_view: flag(id, v, 'presenter_view'),
  };
}

/** Suite commands Deck's chrome sends (the desktop's second window). */
const DECK_SUITE_BUILDERS: Readonly<Record<string, (a: Args) => Command>> = {
  'suite.view.open_window': (a) => ({
    Suite: {
      OpenWindow: {
        window: choice('suite.view.open_window', a, 'window', ['Audience', 'Presenter'] as const),
      },
    },
  }),
};

/** The builders of every Deck core command (spread into `CORE_BUILDERS`). */
export const DECK_BUILDERS: Readonly<Record<string, (a: Args) => Command>> = Object.freeze({
  ...Object.fromEntries(
    Object.entries(DECK_BUILDER_TABLE).map(
      ([id, build]) => [id, (a: Args): Command => ({ Deck: build(a, id) })] as const,
    ),
  ),
  ...DECK_SUITE_BUILDERS,
});

// ── Ribbon state ─────────────────────────────────────────────────────────────────────────────────

/** The core's Deck state the editor put in the registry context (`deckState`). */
const ds = (ctx: CommandContext): DeckState | undefined => ctx.deckState as DeckState | undefined;
/** The first selected effect of the Animations tab. */
const selectedAnimation = (ctx: CommandContext) => {
  const m = ds(ctx)?.motion;
  const id = m?.selected[0];
  return id === undefined ? undefined : m?.animations.find((a) => a.id === id);
};
const df = (ctx: CommandContext) => ds(ctx)?.format ?? undefined;
const colours = new Map<string, { type: 'rgb'; hex: string } | { type: 'auto' } | { type: 'none' }>();
function colour(hex: string | null | undefined, none: 'auto' | 'none') {
  const key = hex ? `rgb:${hex}` : none;
  const known = colours.get(key);
  if (known) return known;
  const v: { type: 'rgb'; hex: string } | { type: 'auto' } | { type: 'none' } = hex
    ? { type: 'rgb', hex: `#${hex}` }
    : { type: none };
  colours.set(key, v);
  return v;
}

/**
 * Pressed states and values of the ribbon in Deck, read from the selection's formatting and the
 * view the core reports (`Query::Deck(State)`).
 */
export const DECK_CORE_STATE: {
  checked: Readonly<Record<string, (ctx: CommandContext) => boolean | undefined>>;
  values: Readonly<Record<string, (ctx: CommandContext) => unknown>>;
} = {
  checked: {
    'suite.format.bold': (c) => df(c)?.bold,
    'suite.format.italic': (c) => df(c)?.italic,
    'suite.format.underline': (c) => df(c)?.underline,
    'suite.format.strikethrough': (c) => df(c)?.strike,
    'suite.format.subscript': (c) => df(c)?.subscript,
    'suite.format.superscript': (c) => df(c)?.superscript,
    'suite.format.text_shadow': (c) => df(c)?.shadow,
    'deck.font.text_shadow': (c) => df(c)?.shadow,
    'suite.format.bullets': (c) => df(c)?.bullets,
    'suite.format.numbering': (c) => df(c)?.numbering,
    'suite.format.align_left': (c) => (df(c) ? df(c)?.align === 'left' : undefined),
    'suite.format.align_center': (c) => (df(c) ? df(c)?.align === 'center' : undefined),
    'suite.format.align_right': (c) => (df(c) ? df(c)?.align === 'right' : undefined),
    'suite.format.justify': (c) => (df(c) ? df(c)?.align === 'justify' : undefined),
    'deck.view.guides': (c) => ds(c)?.guides,
    'deck.view.gridlines': (c) => ds(c)?.gridlines,
    'suite.view.gridlines': (c) => ds(c)?.gridlines,
    'deck.view.notes': (c) => ds(c)?.notes_visible,
    'deck.show.hide_slide': (c) => {
      const s = ds(c);
      return s ? s.slides[s.current]?.hidden : undefined;
    },
    'deck.design.hide_background_graphics': (c) => ds(c)?.hide_graphics,
    'deck.master.preserve': (c) => {
      const v = ds(c)?.master_view;
      if (!v) return undefined;
      // The master of the item on the stage.
      const at = v.items.findIndex((i) => i.id === v.current);
      return v.items.slice(0, at + 1).findLast((i) => i.master)?.preserved;
    },
    'deck.master.title': (c) => ds(c)?.master_view?.title,
    'deck.master.footers': (c) => ds(c)?.master_view?.footers,
    'deck.transitions.on_click': (c) => ds(c)?.motion.transition?.on_click,
  },
  values: {
    'suite.format.font_family': (c) => df(c)?.font_family,
    'suite.format.font_size': (c) => {
      const s = df(c)?.font_size;
      return s === undefined ? undefined : String(Math.round(s * 2) / 2);
    },
    'suite.format.font_color': (c) => (df(c) ? colour(df(c)?.color, 'auto') : undefined),
    'suite.shape.fill': (c) => (df(c) ? colour(df(c)?.fill, 'none') : undefined),
    'suite.shape.outline': (c) => (df(c) ? colour(df(c)?.outline, 'none') : undefined),
    'deck.drawing.shape_fill': (c) => (df(c) ? colour(df(c)?.fill, 'none') : undefined),
    'deck.drawing.shape_outline': (c) => (df(c) ? colour(df(c)?.outline, 'none') : undefined),
    'suite.shape.height': (c) => ds(c)?.selection_box?.height,
    'suite.shape.width': (c) => ds(c)?.selection_box?.width,
    'deck.design.themes': (c) => {
      const t = ds(c)?.theme;
      return t ? t.toLowerCase() : undefined;
    },
    'deck.transitions.gallery': (c) => ds(c)?.motion.transition?.effect,
    'deck.transitions.duration': (c) => {
      const t = ds(c)?.motion.transition;
      return t ? t.duration_ms / 1000 : undefined;
    },
    'deck.transitions.after': (c) => {
      const t = ds(c)?.motion.transition;
      return t?.after_ms === null || t?.after_ms === undefined ? undefined : t.after_ms / 1000;
    },
    'deck.animations.gallery': (c) =>
      selectedAnimation(c)?.effect ?? (ds(c)?.selection.length ? 'none' : undefined),
    'deck.animations.start': (c) => selectedAnimation(c)?.start,
    'deck.animations.duration': (c) => {
      const a = selectedAnimation(c);
      return a ? a.duration_ms / 1000 : undefined;
    },
    'deck.animations.delay': (c) => {
      const a = selectedAnimation(c);
      return a ? a.delay_ms / 1000 : undefined;
    },
  },
};
