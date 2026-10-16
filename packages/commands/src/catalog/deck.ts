/** Kiln Deck and Kiln Notes commands: PLAN.md §1.4 "Deck"/"Notes" and the Deck column of §1.7. */
import { c, DECK, NOTES, section } from './define.ts';

const D = { apps: DECK };

export const deckCommands = [
  ...section('Home ▸ Slides', D, [
    c('deck.newSlide', 'New Slide', 'NewSlide', { keyTip: 'I' }),
    c('deck.layout', 'Layout', 'SlideLayout', { keyTip: 'L' }),
    c('deck.resetSlide', 'Reset', 'ResetSlide'),
    c('deck.section', 'Section', 'Section'),
    c('deck.duplicate', 'Duplicate', 'CopyPlus', { synonyms: ['duplicate slide'] }),
    c('deck.previousSlide', 'Previous Slide', 'ChevronUp'),
    c('deck.nextSlide', 'Next Slide', 'ChevronDown'),
    c('deck.editText', 'Edit Text', 'PencilLine'),
    c('deck.promote', 'Promote', 'ArrowLeftToLine'),
    c('deck.demote', 'Demote', 'ArrowRightToLine'),
  ]),
  ...section('Home ▸ Paragraph', D, [
    c('deck.textDirection', 'Text Direction', 'TextDirection'),
    c('deck.alignText', 'Align Text', 'AlignText'),
    c('deck.convertToDiagram', 'Convert to List Diagram', 'ConvertToDiagram', {
      officeName: 'Convert to SmartArt',
    }),
  ]),
  ...section('Home ▸ Drawing', D, [
    c('deck.arrange', 'Arrange', 'Arrange'),
    c('deck.quickStyles', 'Quick Styles', 'QuickStyles'),
    c('deck.shapeFill', 'Shape Fill', 'ShapeFill'),
    c('deck.shapeOutline', 'Shape Outline', 'ShapeOutline'),
    c('deck.shapeEffects', 'Shape Effects', 'ShapeEffects'),
    c('deck.group', 'Group', 'GroupObjects'),
    c('deck.ungroup', 'Ungroup', 'Ungroup'),
    c('deck.nudge', 'Nudge', 'Move', { palette: false }),
    c('deck.resize', 'Resize', 'Scaling', { palette: false }),
    c('deck.designIdeas', 'Design Ideas', 'DesignIdeas', { officeName: 'Designer' }),
  ]),
  ...section('Insert', D, [
    c('deck.reuseSlides', 'Reuse Slides', 'ReuseSlides', { tier: 'P1' }),
    c('deck.video', 'Video', 'InsertVideo'),
    c('deck.audio', 'Audio', 'InsertAudio', { tier: 'P1' }),
    c('deck.slideNumber', 'Slide Number', 'SlideNumber'),
    c('deck.liveGridRange', 'Live Grid Range', 'LiveGridRange', { tier: 'P2' }),
  ]),
  ...section('Design', D, [
    c('deck.themes', 'Themes', 'Palette'),
    c('deck.variants', 'Variants', 'SwatchBook'),
    c('deck.slideSize', 'Slide Size', 'SlideSize', { synonyms: ['16:9', '4:3', 'widescreen'] }),
    c('deck.formatBackground', 'Format Background', 'FormatBackground'),
  ]),
  ...section('Transitions', D, [
    c('deck.previewTransition', 'Preview', 'TransitionPreview'),
    c('deck.transitions', 'Transition to This Slide', 'Blend'),
    c('deck.transitionDuration', 'Duration', 'Timer'),
    c('deck.applyToAll', 'Apply To All', 'ApplyToAll'),
  ]),
  ...section('Animations', D, [
    c('deck.previewAnimation', 'Preview Animations', 'Play'),
    c('deck.animations', 'Animation Styles', 'Sparkle'),
    c('deck.addAnimation', 'Add Animation', 'AddAnimation'),
    c('deck.animationPane', 'Animation Pane', 'AnimationPane'),
    c('deck.animationPainter', 'Animation Painter', 'AnimationPainter'),
  ]),
  ...section('Slide Show', D, [
    c('deck.show.fromBeginning', 'From Beginning', 'FromBeginning', {
      synonyms: ['present', 'start slide show', 'play'],
    }),
    c('deck.show.fromCurrent', 'From Current Slide', 'FromCurrent'),
    c('deck.show.inTab', 'Present in Browser Tab', 'PresentInTab'),
    c('deck.show.setUp', 'Set Up Slide Show', 'SetUpShow'),
    c('deck.hideSlide', 'Hide Slide', 'HideSlide'),
    c('deck.rehearseTimings', 'Rehearse Timings', 'RehearseTimings', { tier: 'P1' }),
    c('deck.presenterView', 'Use Presenter View', 'PresenterView'),
  ]),
  ...section('Slide Show ▸ During the show', { apps: DECK }, [
    c('deck.show.next', 'Next', 'ChevronRight', { palette: false }),
    c('deck.show.previous', 'Previous', 'ChevronLeft', { palette: false }),
    c('deck.show.first', 'First Slide', 'ChevronFirst', { palette: false }),
    c('deck.show.last', 'Last Slide', 'ChevronLast', { palette: false }),
    c('deck.show.digit', 'Type Slide Number', 'Hash', { palette: false }),
    c('deck.show.goToTyped', 'Go to Slide', 'CornerDownLeft', { palette: false }),
    c('deck.show.black', 'Black Screen', 'Blackout', { palette: false }),
    c('deck.show.white', 'White Screen', 'Square', { palette: false }),
    c('deck.show.laser', 'Laser Pointer', 'LaserPointer', { palette: false }),
    c('deck.show.pen', 'Pen', 'PenTool', { palette: false }),
    c('deck.show.highlighter', 'Highlighter', 'HighlighterTool', { palette: false }),
    c('deck.show.end', 'End Show', 'X', { palette: false }),
  ]),
  ...section('View', D, [
    c('deck.normalView', 'Normal', 'NormalView'),
    c('deck.outlineView', 'Outline View', 'OutlineView'),
    c('deck.slideSorter', 'Slide Sorter', 'SlideSorter'),
    c('deck.notesPage', 'Notes Page', 'NotesPage'),
    c('deck.readingView', 'Reading View', 'ReadingView'),
    c('deck.slideMaster', 'Slide Master', 'SlideMaster'),
    c('deck.handoutMaster', 'Handout Master', 'HandoutMaster', { tier: 'P2' }),
    c('deck.notesMaster', 'Notes Master', 'NotesMaster', { tier: 'P2' }),
    c('deck.guides', 'Guides', 'Crosshair'),
    c('deck.notes', 'Notes', 'NotebookText'),
    c('deck.fitToWindow', 'Fit to Window', 'FitToWindow'),
  ]),
];

export const notesCommands = section('Notes', { apps: NOTES }, [
  c('notes.tagToDo', 'To Do Tag', 'TagToDo'),
  c('notes.tagImportant', 'Important Tag', 'TagImportant'),
  c('notes.tagQuestion', 'Question Tag', 'TagQuestion'),
  c('notes.attachFile', 'File Attachment', 'AttachFile'),
  c('notes.pageColor', 'Page Color', 'PageColor'),
  c('notes.ruleLines', 'Rule Lines', 'RuleLines'),
  c('notes.newPage', 'New Page', 'FilePlus'),
  c('notes.newSection', 'New Section', 'FolderPlus'),
]);
