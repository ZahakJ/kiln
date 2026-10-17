/** Kiln Deck commands and ribbon: PLAN.md §1.4 "Deck" and the Deck column of §1.7. */
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
import { c, DECK, section } from './define.ts';
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
  ]),
  ...section('Home ▸ Paragraph', D, [
    c('deck.paragraph.text_direction', 'Text Direction', 'TextDirection'),
    c('deck.paragraph.align_text', 'Align Text', 'AlignText'),
    c('deck.paragraph.convert_to_diagram', 'Convert to List Diagram', 'ConvertToDiagram', {
      officeName: 'Convert to SmartArt',
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
    c('deck.insert.video', 'Video', 'InsertVideo'),
    c('deck.insert.audio', 'Audio', 'InsertAudio', { tier: 'P1' }),
    c('deck.insert.slide_number', 'Slide Number', 'SlideNumber'),
    c('deck.insert.live_grid_range', 'Live Grid Range', 'LiveGridRange', { tier: 'P2' }),
  ]),
  ...section('Design', D, [
    c('deck.design.themes', 'Themes', 'Palette'),
    c('deck.design.variants', 'Variants', 'SwatchBook'),
    c('deck.design.slide_size', 'Slide Size', 'SlideSize', { synonyms: ['16:9', '4:3', 'widescreen'] }),
    c('deck.design.format_background', 'Format Background', 'FormatBackground'),
  ]),
  ...section('Transitions', D, [
    c('deck.transitions.preview', 'Preview', 'TransitionPreview'),
    c('deck.transitions.gallery', 'Transition to This Slide', 'Blend'),
    c('deck.transitions.duration', 'Duration', 'Timer'),
    c('deck.transitions.apply_to_all', 'Apply To All', 'ApplyToAll'),
  ]),
  ...section('Animations', D, [
    c('deck.animations.preview', 'Preview Animations', 'Play'),
    c('deck.animations.gallery', 'Animation Styles', 'Sparkle'),
    c('deck.animations.add_animation', 'Add Animation', 'AddAnimation'),
    c('deck.animations.animation_pane', 'Animation Pane', 'AnimationPane'),
    c('deck.animations.animation_painter', 'Animation Painter', 'AnimationPainter'),
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
  ]),
  ...section('View', D, [
    c('deck.view.outline_view', 'Outline View', 'OutlineView'),
    c('deck.view.slide_master', 'Slide Master', 'SlideMaster'),
    c('deck.view.handout_master', 'Handout Master', 'HandoutMaster', { tier: 'P2' }),
    c('deck.view.notes_master', 'Notes Master', 'NotesMaster', { tier: 'P2' }),
    c('deck.view.guides', 'Guides', 'Crosshair'),
    c('deck.view.notes', 'Notes', 'NotebookText'),
    c('deck.view.fit_to_window', 'Fit to Window', 'FitToWindow'),
    c('deck.view.color_grayscale', 'Color / Grayscale', 'Contrast', { tier: 'P2' }),
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

const TRANSITIONS: readonly RibbonGalleryEntry[] = [
  ['None', 'None'],
  ['Fade', 'Fade'],
  ['Push', 'Push'],
  ['Wipe', 'Wipe'],
  ['Split', 'Split'],
  ['Reveal', 'Reveal'],
  ['Cut', 'Cut'],
  ['Shape', 'Shape'],
  ['Uncover', 'Uncover'],
  ['Cover', 'Cover'],
  ['Flash', 'Flash'],
].map(([id, label]) => ({
  id: id ?? '',
  label: label ?? '',
  sample: label,
  look: { font: 'ui' as const, size: 12 },
}));

const ANIMATIONS: readonly RibbonGalleryEntry[] = [
  ['Appear', 'Appear', 'Entrance'],
  ['Fade', 'Fade', 'Entrance'],
  ['FlyIn', 'Fly In', 'Entrance'],
  ['FloatIn', 'Float In', 'Entrance'],
  ['Wipe', 'Wipe', 'Entrance'],
  ['Zoom', 'Zoom', 'Entrance'],
  ['Pulse', 'Pulse', 'Emphasis'],
  ['Spin', 'Spin', 'Emphasis'],
  ['Teeter', 'Teeter', 'Emphasis'],
  ['Disappear', 'Disappear', 'Exit'],
  ['FlyOut', 'Fly Out', 'Exit'],
].map(([id, label]) => ({
  id: id ?? '',
  label: label ?? '',
  sample: label,
  look: { font: 'ui' as const, size: 12 },
}));

const THEMES: readonly RibbonGalleryEntry[] = [
  ['kiln', 'Kiln', '#FFF7EE', '#1B1411'],
  ['porcelain', 'Porcelain', '#FBFAF7', '#22211F'],
  ['cobalt', 'Cobalt', '#E9EFFD', '#14213F'],
  ['jade', 'Jade', '#E6F4EC', '#10281C'],
  ['tenmoku', 'Tenmoku', '#2A1B13', '#120B08'],
  ['celadon', 'Celadon', '#E8F1EC', '#18241F'],
].map(([id, label, light, dark]) => ({
  id: id ?? '',
  label: label ?? '',
  look: { swatch: [light ?? '', dark ?? ''] },
}));

/** Kiln Deck's ribbon. */
export const deckRibbon: RibbonData = {
  app: 'deck',
  tabs: [
    rtab(
      'home',
      'Home',
      [
        clipboardGroup(),
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
              {
                size: 'large',
                keyTip: 'I',
              },
            ),
            rbutton('deck.slides.layout', { size: 'medium', keyTip: 'L' }),
            rbutton('deck.slides.reset_slide', { size: 'medium', keyTip: 'A' }),
            rbutton('deck.slides.section', { size: 'medium', keyTip: 'T1' }),
          ],
          { icon: 'NewSlide', priority: 3 },
        ),
        fontGroup({ spacing: true }),
        paragraphGroup({
          second: [
            rbutton('deck.paragraph.text_direction', { hideInSimplified: true }),
            rbutton('deck.paragraph.align_text', { hideInSimplified: true }),
            rbutton('deck.paragraph.convert_to_diagram', { hideInSimplified: true }),
          ],
        }),
        rgroup(
          'drawing',
          'Drawing',
          [
            rbutton('suite.insert.shapes', { size: 'large', keyTip: 'SH' }),
            rbutton('deck.drawing.arrange', { size: 'medium', keyTip: 'G' }),
            rbutton('deck.drawing.quick_styles', { size: 'medium', keyTip: 'Q' }),
            rbutton('deck.drawing.shape_fill', { size: 'small', keyTip: 'SF' }),
            rbutton('deck.drawing.shape_outline', { size: 'small', keyTip: 'SO' }),
            rbutton('deck.drawing.shape_effects', { size: 'small', keyTip: 'SE' }),
          ],
          { icon: 'Shapes', priority: 1 },
        ),
        rgroup(
          'editing',
          'Editing',
          [
            rbutton('suite.edit.find', { size: 'medium', keyTip: 'FD' }),
            rbutton('suite.edit.replace', { size: 'medium', keyTip: 'R' }),
            rdropdown(
              'suite.edit.select',
              [mitem('suite.edit.select_all'), mitem('suite.arrange.selection_pane')],
              { size: 'medium', keyTip: 'SL' },
            ),
          ],
          { icon: 'Search', priority: 0 },
        ),
        rgroup(
          'kiln',
          'Kiln',
          [rbutton('deck.drawing.design_ideas', { size: 'large', label: 'Ideas', keyTip: 'DI' })],
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
          rbutton('deck.slides.new_slide', { size: 'large', keyTip: 'I' }),
          rbutton('deck.insert.reuse_slides', { size: 'medium', keyTip: 'RS' }),
        ]),
        rgroup('tables', 'Tables', [rbutton('suite.insert.table', { size: 'large', keyTip: 'T' })]),
        rgroup('images', 'Images', [
          rbutton('suite.insert.pictures', { size: 'large', keyTip: 'P' }),
          rbutton('suite.insert.screenshot', { size: 'medium', keyTip: 'SC' }),
        ]),
        rgroup('illustrations', 'Illustrations', [
          rbutton('suite.insert.shapes', { size: 'medium', keyTip: 'SH' }),
          rbutton('suite.insert.icons', { size: 'medium', keyTip: 'Y1' }),
          rbutton('suite.chart.insert', { size: 'medium', keyTip: 'C' }),
        ]),
        rgroup('links', 'Links', [rbutton('suite.insert.link', { size: 'large', keyTip: 'I2' })]),
        rgroup('comments', 'Comments', [
          rbutton('suite.review.new_comment', { size: 'large', label: 'Comment', keyTip: 'L' }),
        ]),
        rgroup('text', 'Text', [
          rbutton('suite.insert.text_box', { size: 'large', keyTip: 'X' }),
          rbutton('suite.insert.header_footer', { size: 'medium', keyTip: 'H' }),
          rbutton('suite.insert.date_time', { size: 'small', keyTip: 'D' }),
          rbutton('deck.insert.slide_number', { size: 'small', keyTip: 'SN' }),
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
              itemWidth: 64,
              itemHeight: 48,
              keyTip: 'H',
            }),
          ],
          {
            priority: 2,
          },
        ),
        rgroup('variants', 'Variants', [rbutton('deck.design.variants', { size: 'large', keyTip: 'V' })]),
        rgroup('customize', 'Customize', [
          rdropdown(
            'deck.design.slide_size',
            [
              mitem('deck.design.slide_size', { label: 'Widescreen (16:9)', args: { ratio: '16:9' } }),
              mitem('deck.design.slide_size', { label: 'Standard (4:3)', args: { ratio: '4:3' } }),
              msep(),
              mitem('deck.design.slide_size', { label: 'Custom Slide Size…', args: { custom: true } }),
            ],
            { size: 'large', keyTip: 'S' },
          ),
          rbutton('deck.design.format_background', { size: 'large', keyTip: 'B' }),
        ]),
        rgroup('designer', 'Designer', [
          rbutton('deck.drawing.design_ideas', { size: 'large', keyTip: 'DI' }),
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
              columns: 6,
              itemWidth: 56,
              itemHeight: 48,
              keyTip: 'T',
            }),
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
            keyTip: 'E',
          }),
          rbutton('deck.transitions.apply_to_all', { size: 'medium', keyTip: 'L' }),
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
              columns: 6,
              itemWidth: 56,
              itemHeight: 48,
              keyTip: 'S',
            }),
          ],
          {
            priority: 2,
          },
        ),
        rgroup('advanced', 'Advanced Animation', [
          rbutton('deck.animations.add_animation', { size: 'large', keyTip: 'AA' }),
          rtoggle('deck.animations.animation_pane', { size: 'medium', keyTip: 'C' }),
          rbutton('deck.animations.animation_painter', { size: 'medium', keyTip: 'PA' }),
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
          rbutton('suite.review.delete_comment', { size: 'medium', keyTip: 'D' }),
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
          rbutton('deck.view.outline_view', { size: 'large', keyTip: 'PO' }),
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
          rtoggle('suite.view.ruler', { size: 'small', keyTip: 'R' }),
          rtoggle('suite.view.gridlines', { size: 'small', keyTip: 'G' }),
          rtoggle('deck.view.guides', { size: 'small', keyTip: 'U' }),
          rtoggle('deck.view.notes', { size: 'small', keyTip: 'NO' }),
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
        rgroup('window', 'Window', [
          rbutton('suite.view.new_window', { size: 'large', keyTip: 'NW' }),
          rbutton('suite.view.side_by_side', { size: 'small', keyTip: 'B' }),
        ]),
        themeGroup(),
        macrosGroup(),
      ],
      { keyTip: 'W' },
    ),
    shapeFormatTab(),
    pictureFormatTab(),
    ...tableTabs('deck'),
  ],
};
