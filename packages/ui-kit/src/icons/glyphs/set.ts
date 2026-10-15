/**
 * The Officina Glyphs set: one export per glyph. Import from `./index.ts`, which adds the registry.
 * Drawing conventions: content inside 2–18, 1.5 px strokes, round caps; parts marked A() take the accent
 * in the colourful icon style, M() parts are de-emphasised details.
 */
import {
  arrow,
  chevron,
  circle,
  cross,
  dot,
  type GlyphPart,
  glyph,
  grid,
  hlines,
  line,
  page,
  plus,
  rect,
} from './glyph.ts';

const A = (d: string | GlyphPart): GlyphPart =>
  typeof d === 'string' ? { d, accent: true } : { ...d, accent: true };
const M = (d: string) => ({ d, muted: true });
const F = (d: string) => ({ d, fill: true });
const FA = (d: string) => ({ d, fill: true, accent: true });

// Shared shapes ------------------------------------------------------------------------------------

/** Clipboard outline, open at the bottom-right for an overlay. */
const CLIPBOARD_OPEN =
  'M7.5 4H5.25a1.5 1.5 0 0 0-1.5 1.5v10.25a1.5 1.5 0 0 0 1.5 1.5H8.5M12.5 4h1.25a1.5 1.5 0 0 1 1.5 1.5V8';
const CLIP = rect(7.5, 2.75, 5, 2.5, 0.75);
/** Small page at the bottom-right of a clipboard. */
const PASTE_PAGE = page(10.5, 9.5, 6.75, 8, 2);
/** Spreadsheet-ish 3×3 grid occupying the icon. */
const SHEET = grid(2.75, 3.75, 14.5, 12.5, 3, 3, 1.5);
/** Slide frame. */
const SLIDE = rect(2.75, 4.25, 14.5, 10.5, 1.5);

// Clipboard ----------------------------------------------------------------------------------------

export const Paste = glyph('Paste', 'Paste', [CLIPBOARD_OPEN, CLIP, A(PASTE_PAGE)]);
export const PasteKeepSource = glyph('PasteKeepSource', 'Paste: Keep Source Formatting', [
  CLIPBOARD_OPEN,
  CLIP,
  A(PASTE_PAGE),
  A(line(11.5, 16, 14.75, 12.75)),
  A(line(13.25, 12.75, 14.75, 12.75, 14.75, 14.25)),
]);
export const PasteMergeFormatting = glyph('PasteMergeFormatting', 'Paste: Merge Formatting', [
  CLIPBOARD_OPEN,
  CLIP,
  A(PASTE_PAGE),
  A(line(12, 15.5, 13.5, 13.5, 15.5, 15.5)),
]);
export const PasteTextOnly = glyph('PasteTextOnly', 'Paste: Keep Text Only', [
  CLIPBOARD_OPEN,
  CLIP,
  A('M11 10.5h6M14 10.5v7'),
]);
export const PasteValues = glyph('PasteValues', 'Paste Values', [
  CLIPBOARD_OPEN,
  CLIP,
  A(rect(10.25, 10, 7, 7.25, 1)),
  A('M12.5 12.5h2.5M12.5 14.75h2.5'),
]);
export const PasteFormulas = glyph('PasteFormulas', 'Paste Formulas', [
  CLIPBOARD_OPEN,
  CLIP,
  A(rect(10.25, 10, 7, 7.25, 1)),
  A('M15 11.75h-1a1 1 0 0 0-1 1v3a1 1 0 0 1-1 1h-.5M12 13.5h2.5'),
]);
export const PasteFormatting = glyph('PasteFormatting', 'Paste Formatting', [
  CLIPBOARD_OPEN,
  CLIP,
  A(rect(10.25, 10, 7, 3.5, 1)),
  A('M13.75 13.5v1.5M13 15h1.5v2.25H13z'),
]);
export const PasteTranspose = glyph('PasteTranspose', 'Paste Transpose', [
  CLIPBOARD_OPEN,
  CLIP,
  A('M10.75 10.5h6.25M10.75 10.5v6.75'),
  A(arrow(12.5, 12.25, 16, 15.75, 2)),
]);
export const PasteSpecial = glyph('PasteSpecial', 'Paste Special', [
  CLIPBOARD_OPEN,
  CLIP,
  A(circle(14, 13.5, 3.25)),
  A('M14 12v1.75l1.25.75'),
]);
export const FormatPainter = glyph('FormatPainter', 'Format Painter', [
  rect(3.25, 2.75, 11, 4.5, 1.25),
  'M14.25 5h1.5a1 1 0 0 1 1 1v2.25a1 1 0 0 1-1 1H9.75a1 1 0 0 0-1 1v1.25',
  A(rect(7.5, 11.5, 2.5, 5.75, 0.75)),
]);

// Font ---------------------------------------------------------------------------------------------

const LETTER_A = 'M3 15.5L7 4.5h.5l4 11M4.5 11.5h5.5';
export const GrowFont = glyph('GrowFont', 'Increase Font Size', [LETTER_A, A(chevron(15.5, 6, 'up', 2.25))]);
export const ShrinkFont = glyph('ShrinkFont', 'Decrease Font Size', [
  'M3.5 15.5L6.75 6.5h.5l3.25 9M4.75 12.5h4.5',
  A(chevron(15.5, 6.5, 'down', 2.25)),
]);
export const ChangeCase = glyph('ChangeCase', 'Change Case', [
  'M2.5 15.5L6 5.5h.5l3.5 10M3.75 12h5',
  A('M16.75 10v5.5M16.75 12.75a2.5 2.5 0 1 0-5 0 2.5 2.5 0 0 0 5 0z'),
]);
export const ClearFormatting = glyph('ClearFormatting', 'Clear All Formatting', [
  'M4 15.5L7.5 4.5h.5l3.5 11M5.25 12h5',
  A('M12.25 11.5l4.5 4.5M16.75 11.5l-4.5 4.5'),
]);
export const TextHighlight = glyph('TextHighlight', 'Text Highlight Colour', [
  'M11.75 3.25l4 4-6.5 6.5H5.25v-4z',
  'M9.25 5.75l4 4',
  FA(rect(2.75, 15.5, 14.5, 2.25, 0.5)),
]);
export const FontColor = glyph('FontColor', 'Font Colour', [
  'M5 13.25L9.5 2.75h1l4.5 10.5M6.75 9.25h6.5',
  FA(rect(2.75, 15.5, 14.5, 2.25, 0.5)),
]);
export const TextEffects = glyph('TextEffects', 'Text Effects and Typography', [
  'M3 16L7.25 4.5h.5L12 16M4.5 12.25h6',
  A('M15.5 3v4M13.5 5h4'),
  A('M15.5 10.5v2.5M14.25 11.75h2.5'),
]);
export const CharacterSpacing = glyph('CharacterSpacing', 'Character Spacing', [
  'M5.5 12L9.25 3h1.5L14.5 12M7 8.75h6',
  A('M3 15.75h14M5 13.75l-2 2 2 2M15 13.75l2 2-2 2'),
]);
export const DoubleUnderline = glyph('DoubleUnderline', 'Double Underline', [
  'M6 3v5.5a4 4 0 0 0 8 0V3',
  A('M4.5 15h11M4.5 17.5h11'),
]);
export const TextShadow = glyph('TextShadow', 'Text Shadow', [
  M('M6 4.5h8.5M10.25 4.5V16'),
  'M4.5 3h8.5M8.75 3v11.5',
]);

// Paragraph ----------------------------------------------------------------------------------------

export const BulletList = glyph('BulletList', 'Bullets', [
  dot(4.25, 5, 1.25),
  dot(4.25, 10, 1.25),
  dot(4.25, 15, 1.25),
  hlines(8, 5, 5, [9, 9, 9]),
]);
export const NumberedList = glyph('NumberedList', 'Numbering', [
  A('M3 3.75h1.25v4M3 7.75h2.5'),
  A('M3 12.25a1.25 1.25 0 0 1 2.5 0c0 1.25-2.5 1.75-2.5 3.5h2.5'),
  hlines(8.5, 5.5, 8.5, [8.75, 8.75]),
]);
export const MultilevelList = glyph('MultilevelList', 'Multilevel List', [
  A('M3 3.5h1v3'),
  'M7 5h10',
  A('M6.5 9.25h1v3'),
  'M10.5 10.75H17',
  A('M10 15.5h1'),
  'M13.5 15.5H17',
]);
export const DecreaseIndent = glyph('DecreaseIndent', 'Decrease Indent', [
  'M3 3.75h14M9 8.25h8M9 12.25h8M3 16.25h14',
  A('M6 7.75L3.5 10.25 6 12.75'),
]);
export const IncreaseIndent = glyph('IncreaseIndent', 'Increase Indent', [
  'M3 3.75h14M9 8.25h8M9 12.25h8M3 16.25h14',
  A('M3.5 7.75L6 10.25 3.5 12.75'),
]);
export const SortText = glyph('SortText', 'Sort', [
  'M3.5 8.5L5.75 3h.5L8.5 8.5M4.25 6.75h3.5',
  'M3.75 11.5h4.5l-4.5 5.5h4.5',
  A(arrow(14, 3.5, 14, 16.5, 2.75)),
]);
export const ShowFormattingMarks = glyph('ShowFormattingMarks', 'Show/Hide ¶', [
  'M10.25 16.5V3.25H15.5M13.25 3.25V16.5',
  A('M10.25 3.25H8.5a3.5 3.5 0 0 0 0 7h1.75'),
]);
export const LineSpacing = glyph('LineSpacing', 'Line and Paragraph Spacing', [
  'M9.5 4.25h7.75M9.5 8.25h7.75M9.5 12.25h7.75M9.5 16.25h7.75',
  A(arrow(4.75, 9, 4.75, 3.25, 2)),
  A(arrow(4.75, 11, 4.75, 16.75, 2)),
]);
export const Shading = glyph('Shading', 'Shading', [
  'M8.25 3.5l6.5 6.5-5 5a1.5 1.5 0 0 1-2.1 0L3.5 10.85a1.5 1.5 0 0 1 0-2.1z',
  'M4 9.75h10.5',
  FA('M16.25 12.5c.9 1.3 1.5 2.25 1.5 3a1.5 1.5 0 0 1-3 0c0-.75.6-1.7 1.5-3z'),
]);

const BORDER_FRAME = { x: 3, y: 3, w: 14, h: 14 };
const DOTTED_FRAME = M(
  [
    'M3 3h.01M6.5 3h.01M10 3h.01M13.5 3h.01M17 3h.01',
    'M3 6.5h.01M3 10h.01M3 13.5h.01M3 17h.01',
    'M17 6.5h.01M17 10h.01M17 13.5h.01M17 17h.01',
    'M6.5 17h.01M10 17h.01M13.5 17h.01',
    'M10 6.5h.01M10 13.5h.01M6.5 10h.01M13.5 10h.01',
  ].join(''),
);
const { x: bx, y: by, w: bw, h: bh } = BORDER_FRAME;
export const BordersAll = glyph('BordersAll', 'All Borders', [
  A(rect(bx, by, bw, bh)),
  A(`M10 ${by}v${bh}M${bx} 10h${bw}`),
]);
export const BordersOutside = glyph('BordersOutside', 'Outside Borders', [
  DOTTED_FRAME,
  A(rect(bx, by, bw, bh)),
]);
export const BordersInside = glyph('BordersInside', 'Inside Borders', [
  DOTTED_FRAME,
  A(`M10 ${by}v${bh}M${bx} 10h${bw}`),
]);
export const BordersNone = glyph('BordersNone', 'No Border', [DOTTED_FRAME]);
export const BorderThickBox = glyph('BorderThickBox', 'Thick Box Border', [
  DOTTED_FRAME,
  { d: rect(bx, by, bw, bh), accent: true },
  A(rect(bx + 1, by + 1, bw - 2, bh - 2)),
]);
export const BorderTop = glyph('BorderTop', 'Top Border', [DOTTED_FRAME, A(`M${bx} ${by}h${bw}`)]);
export const BorderBottom = glyph('BorderBottom', 'Bottom Border', [DOTTED_FRAME, A(`M${bx} 17h${bw}`)]);
export const BorderLeft = glyph('BorderLeft', 'Left Border', [DOTTED_FRAME, A(`M${bx} ${by}v${bh}`)]);
export const BorderRight = glyph('BorderRight', 'Right Border', [DOTTED_FRAME, A(`M17 ${by}v${bh}`)]);
export const BorderMore = glyph('BorderMore', 'More Borders', [
  DOTTED_FRAME,
  A(rect(bx, by, bw, bh)),
  F(circle(10, 10, 1)),
  F(circle(6.5, 10, 1)),
  F(circle(13.5, 10, 1)),
]);

// Styles, editing, Officina extras ----------------------------------------------------------------------

export const StylesPane = glyph('StylesPane', 'Styles Pane', [
  rect(2.75, 3.25, 14.5, 13.5, 1.5),
  'M11.5 3.25v13.5',
  A('M5 7.5L6.5 6h.25L8.25 9.5M5.5 8.5h2.25'),
  hlines(5, 12, 2.5, [4, 4]),
  M(hlines(13.5, 6.5, 3, [1.75, 1.75, 1.75])),
]);
export const Replace = glyph('Replace', 'Replace', [
  'M3 8.25L5.75 2.75h.5L9 8.25M3.75 6.75h4.5',
  'M11 11.25h3.5a2 2 0 0 1 0 4H11zM11 15.25h4a2 2 0 0 1 0 4',
  A('M4.5 12.25v1.5a2.5 2.5 0 0 0 2.5 2.5h1.75M7.25 14.5l1.5 1.75-1.5 1.75'),
]);
export const SelectObjects = glyph('SelectObjects', 'Select Objects', [
  M(rect(2.75, 2.75, 10, 10, 1)),
  A('M8.75 8.75l2.75 8.5 1.5-3.5 3.5-1.5z'),
]);
export const GoTo = glyph('GoTo', 'Go To', [page(), A(arrow(2.5, 11, 9.5, 11, 2.5))]);
export const FocusMode = glyph('FocusMode', 'Focus Mode', [
  'M3 7V4.25A1.25 1.25 0 0 1 4.25 3H7M13 3h2.75A1.25 1.25 0 0 1 17 4.25V7M17 13v2.75A1.25 1.25 0 0 1 15.75 17H13M7 17H4.25A1.25 1.25 0 0 1 3 15.75V13',
  A(circle(10, 10, 2.5)),
]);
export const CommandPalette = glyph('CommandPalette', 'Command Palette', [
  rect(2.75, 3.75, 14.5, 12.5, 2),
  A('M6 8.5l2 1.75L6 12'),
  'M10.5 12h3.5',
]);

// Insert -------------------------------------------------------------------------------------------

export const CoverPage = glyph('CoverPage', 'Cover Page', [
  page(),
  A(rect(4.75, 9.5, 10.5, 3.5)),
  'M7 6.5h4',
  M('M7 15h3'),
]);
export const BlankPage = glyph('BlankPage', 'Blank Page', [
  page(4, 2.75, 10, 14.5),
  A(plus(15.25, 14.75, 2.25)),
]);
export const PageBreak = glyph('PageBreak', 'Page Break', [
  'M4.75 7.5V3.75a1 1 0 0 1 1-1h8.5a1 1 0 0 1 1 1V7.5',
  'M4.75 12.5v3.75a1 1 0 0 0 1 1h8.5a1 1 0 0 0 1-1V12.5',
  A('M2.5 10h2M7.25 10h2M10.75 10h2M15.5 10h2'),
]);
export const InsertTable = glyph('InsertTable', 'Table', [
  grid(2.75, 3.25, 14.5, 13.5, 3, 3, 1.5),
  A('M2.75 7.75h14.5M2.75 4.75a1.5 1.5 0 0 1 1.5-1.5h11.5a1.5 1.5 0 0 1 1.5 1.5v3h-14.5z'),
]);
export const ConvertTextToTable = glyph('ConvertTextToTable', 'Convert Text to Table', [
  hlines(2.75, 4, 3, [6, 6, 4]),
  A(grid(10, 9.5, 7.25, 7.5, 2, 2, 1)),
  'M3.5 13.5c0 2 1.25 3 3.5 3M5.75 15.25L7.25 16.5 5.75 17.75',
]);
export const Shapes = glyph('Shapes', 'Shapes', [circle(7, 7, 4.25), A(rect(9.25, 9.25, 8, 8, 1))]);
export const Screenshot = glyph('Screenshot', 'Screenshot', [
  'M2.75 6.5V4.25a1.5 1.5 0 0 1 1.5-1.5H6.5M13.5 2.75h2.25a1.5 1.5 0 0 1 1.5 1.5V6.5M17.25 13.5v2.25a1.5 1.5 0 0 1-1.5 1.5H13.5M6.5 17.25H4.25a1.5 1.5 0 0 1-1.5-1.5V13.5',
  A(rect(6, 7, 8, 6, 1)),
  A(circle(10, 10, 1.25)),
]);
export const Bookmark = glyph('Bookmark', 'Bookmark', [
  page(),
  A('M8 2.75v6.5l1.75-1.25 1.75 1.25v-6.5'),
  M(hlines(7.25, 12.5, 2.25, [5.5, 3.5])),
]);
export const CrossReference = glyph('CrossReference', 'Cross-reference', [
  page(2.75, 2.75, 7.5, 10, 2),
  page(9.75, 7.25, 7.5, 10, 2),
  A('M6.5 8.5c0 3.25 1.5 4.25 5 4.25M10.25 11.25l1.5 1.5-1.5 1.5'),
]);
export const NewComment = glyph('NewComment', 'New Comment', [
  'M3 5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H9l-4 3v-3a2 2 0 0 1-2-2z',
  A(plus(10, 8.5, 2.5)),
]);
export const Header = glyph('Header', 'Header', [
  page(),
  A(rect(4.75, 2.75, 7, 3.5)),
  M(hlines(7.25, 10, 2.5, [5.5, 5.5, 3.5])),
]);
export const Footer = glyph('Footer', 'Footer', [
  page(),
  A('M4.75 13.75h10.5'),
  M(hlines(7.25, 6.5, 2.5, [5.5, 3.5])),
  A('M7.25 15.5h5.5'),
]);
export const PageNumber = glyph('PageNumber', 'Page Number', [
  page(),
  A('M8.75 10.25l1.75-1.25v6.25M8.75 15.25h3.5'),
]);
export const TextBox = glyph('TextBox', 'Text Box', [
  rect(2.75, 3.75, 14.5, 12.5, 1.5),
  A('M6.5 7.5h7M10 7.5v6'),
]);
export const QuickParts = glyph('QuickParts', 'Quick Parts', [
  page(),
  A(rect(7, 8.25, 6, 5.25, 0.75)),
  A('M8.75 10.75h2.5'),
]);
export const WordArt = glyph('WordArt', 'WordArt', [
  'M3 16L7.5 4h1L13 16M4.75 11.75h6.5',
  A('M14.5 4.5l.9 1.85 2.1.3-1.5 1.45.35 2.05-1.85-1-1.85 1 .35-2.05-1.5-1.45 2.1-.3z'),
]);
export const DropCap = glyph('DropCap', 'Drop Cap', [
  A('M3 11V3.5h2.75a3.75 3.75 0 0 1 0 7.5z'),
  hlines(11.5, 4, 3.5, [5.75, 5.75]),
  hlines(3, 14, 3, [14.25, 10]),
]);
export const DateTime = glyph('DateTime', 'Date & Time', [
  rect(2.75, 4, 12, 11.5, 1.5),
  'M2.75 7.75h12M6 2.75v2.5M11.5 2.75v2.5',
  A(circle(14.5, 14.25, 3.25)),
  A('M14.5 12.75v1.5l1 .75'),
]);
export const Equation = glyph('Equation', 'Equation', [
  'M3.5 6.5c.5-1.25 1.5-1.75 3-1.75h10',
  'M7.5 4.75V15.5',
  A('M12.75 4.75v8.5a2 2 0 0 0 3.5 1.25'),
]);
export const SymbolOmega = glyph('SymbolOmega', 'Symbol', ['M3.5 16.5h4v-2.25a5.75 5.75 0 1 1 5 0v2.25h4']);
export const TableOfContents = glyph('TableOfContents', 'Table of Contents', [
  page(),
  A('M7.25 7h5.5'),
  hlines(7.25, 10, 2.5, [3, 3, 3]),
  M('M11.75 10h.01M11.75 12.5h.01M11.75 15h.01'),
]);
export const Callout = glyph('Callout', 'Callout Block', [
  rect(2.75, 3.75, 14.5, 12.5, 1.5),
  A('M5.25 6.5v7'),
  hlines(8, 7.5, 2.5, [6.5, 6.5, 4]),
]);
export const CodeBlock = glyph('CodeBlock', 'Code Block', [
  rect(2.75, 3.75, 14.5, 12.5, 1.5),
  A('M7.5 7.5L5.25 10l2.25 2.5M12.5 7.5l2.25 2.5-2.25 2.5'),
  M('M11 7l-2 6'),
]);

// Layout (page setup, paragraph, arrange) -------------------------------------------------------------

export const Margins = glyph('Margins', 'Margins', [
  page(4.25, 2.75, 11.5, 14.5, 0.01),
  A('M4.25 6h11.5M4.25 14h11.5M7 2.75v14.5M13 2.75v14.5'),
]);
export const Orientation = glyph('Orientation', 'Orientation', [
  rect(2.75, 6.75, 11, 8, 1),
  A('M8 3.25h5.25a3.5 3.5 0 0 1 3.5 3.5V10M15.25 8.5l1.5 1.5 1.5-1.5'),
]);
export const PageSize = glyph('PageSize', 'Size', [
  page(2.75, 2.75, 10, 14.5, 2.5),
  A('M15.75 3v14M14.5 4.25L15.75 3 17 4.25M14.5 15.75L15.75 17 17 15.75'),
]);
export const Columns = glyph('Columns', 'Columns', [
  hlines(2.75, 4, 3, [6, 6, 6, 6, 4]),
  A(hlines(11.25, 4, 3, [6, 6, 6, 6, 4])),
]);
export const Breaks = glyph('Breaks', 'Breaks', [
  'M4.75 2.75v4.5h10.5v-4.5M4.75 17.25v-4.5h10.5v4.5',
  A('M3 10h2.5M8.75 10h2.5M14.5 10h2.5'),
]);
export const LineNumbers = glyph('LineNumbers', 'Line Numbers', [
  A('M3 3.75h1.25V7.5'),
  A('M3 11.25a1.25 1.25 0 0 1 2.5.25c0 1-2.5 1.5-2.5 3h2.5'),
  hlines(8.5, 5.5, 4, [8.5, 8.5, 6]),
  'M8.5 17.5h8.5',
]);
export const Hyphenation = glyph('Hyphenation', 'Hyphenation', [
  hlines(2.75, 5, 5, [14.5, 9, 14.5]),
  A('M13.5 10h3.5'),
]);
export const IndentLeft = glyph('IndentLeft', 'Indent Left', [
  hlines(8.5, 4, 4, [8.5, 8.5, 8.5, 8.5]),
  A(arrow(3, 10, 6.5, 10, 2)),
]);
export const IndentRight = glyph('IndentRight', 'Indent Right', [
  hlines(3, 4, 4, [8.5, 8.5, 8.5, 8.5]),
  A(arrow(17, 10, 13.5, 10, 2)),
]);
export const SpacingBefore = glyph('SpacingBefore', 'Spacing Before', [
  hlines(3, 10, 3, [14, 14, 9]),
  A(arrow(10, 2.5, 10, 7, 2)),
]);
export const SpacingAfter = glyph('SpacingAfter', 'Spacing After', [
  hlines(3, 3.5, 3, [14, 14, 9]),
  A(arrow(10, 17.5, 10, 13, 2)),
]);
export const Position = glyph('Position', 'Position', [
  page(4.75, 2.75, 10.5, 14.5, 0.01),
  A(rect(9.75, 5, 3.5, 3.5, 0.5)),
  M(hlines(6.75, 11, 2.5, [6.5, 6.5])),
]);
export const WrapText = glyph('WrapText', 'Wrap Text', [
  'M3 4h14M3 8h14M3 12h10.5a2.25 2.25 0 0 1 0 4.5H11',
  A('M12.5 15l-1.5 1.5 1.5 1.5'),
  'M3 16.5h4.5',
]);
export const WrapSquare = glyph('WrapSquare', 'Wrap Text: Square', [
  A(rect(6.75, 6.25, 6.5, 7.5, 0.75)),
  'M3 3.5h14M3 6.5h1.75M3 10h1.75M3 13.5h1.75M15.25 6.5H17M15.25 10H17M15.25 13.5H17M3 16.5h14',
]);
export const BringForward = glyph('BringForward', 'Bring Forward', [
  M(rect(2.75, 2.75, 8.5, 8.5, 1)),
  A(rect(8.75, 8.75, 8.5, 8.5, 1)),
  F(rect(9.5, 9.5, 7, 7, 0.5)),
]);
export const SendBackward = glyph('SendBackward', 'Send Backward', [
  F(rect(3.5, 3.5, 7, 7, 0.5)),
  A(rect(2.75, 2.75, 8.5, 8.5, 1)),
  M(rect(8.75, 8.75, 8.5, 8.5, 1)),
]);
export const SelectionPane = glyph('SelectionPane', 'Selection Pane', [
  rect(2.75, 2.75, 14.5, 14.5, 1.5),
  'M11 2.75v14.5',
  A(rect(4.75, 5.5, 4, 4, 0.5)),
  M(hlines(13, 6.25, 3.25, [2.25, 2.25, 2.25])),
]);
export const AlignObjects = glyph('AlignObjects', 'Align', [
  'M3 2.75v14.5',
  A(rect(5.5, 4, 11, 4, 1)),
  A(rect(5.5, 11.75, 7, 4, 1)),
]);
export const GroupObjects = glyph('GroupObjects', 'Group', [
  M('M2.75 5V2.75H5M15 2.75h2.25V5M17.25 15v2.25H15M5 17.25H2.75V15'),
  A(rect(5, 5, 6, 6, 0.75)),
  rect(9, 9, 6, 6, 0.75),
]);
export const RotateObject = glyph('RotateObject', 'Rotate', [
  A(rect(6.5, 8.25, 7, 7, 1)),
  'M3.25 9.5A6.75 6.75 0 0 1 14.75 4.5M14.75 1.75v2.75H12',
]);

// References ---------------------------------------------------------------------------------------

export const UpdateTable = glyph('UpdateTable', 'Update Table', [
  page(),
  hlines(7.25, 6.5, 2.5, [5.5, 3.5]),
  A('M13.5 14a3.25 3.25 0 1 1-1-2.35M12.5 10.25v1.5h1.5'),
]);
export const AddText = glyph('AddText', 'Add Text', [
  hlines(3, 4.5, 3.5, [14, 9, 7]),
  A(plus(14.5, 14.25, 2.75)),
]);
export const InsertFootnote = glyph('InsertFootnote', 'Insert Footnote', [
  page(),
  M(hlines(7.25, 6.5, 2.5, [5.5])),
  A('M7.25 13.25h5.5M7.25 15.25h3.5'),
  A('M11.25 7.75v-2'),
]);
export const InsertEndnote = glyph('InsertEndnote', 'Insert Endnote', [
  page(),
  M(hlines(7.25, 6.5, 2.5, [5.5, 5.5])),
  A('M4.75 12.25h10.5'),
  A('M7.25 14.75h3.5'),
]);
export const NextFootnote = glyph('NextFootnote', 'Next Footnote', [
  page(2.75, 2.75, 9.5, 14.5),
  A(arrow(9.5, 12, 17, 12, 2.25)),
  M(hlines(5, 7, 2.5, [4.5])),
]);
export const InsertCitation = glyph('InsertCitation', 'Insert Citation', [
  'M3.5 5a1.5 1.5 0 0 1 1.5-1.5h4.25V16H5a1.5 1.5 0 0 1-1.5-1.5zM9.25 3.5H15a1.5 1.5 0 0 1 1.5 1.5v4',
  A('M12.25 12.25h1.5v3h-2v-1M15 12.25h1.5v3h-2v-1'),
]);
export const ManageSources = glyph('ManageSources', 'Manage Sources', [
  'M3 4.75a1.5 1.5 0 0 1 1.5-1.5h3v13.5h-3A1.5 1.5 0 0 1 3 15.25z',
  'M7.5 3.25h3v13.5h-3',
  A('M12.25 4.25l2.9-.75 2.6 12.5-2.9.75z'),
]);
export const Bibliography = glyph('Bibliography', 'Bibliography', [
  'M4 3.25h9.5a2 2 0 0 1 2 2v11.5H6a2 2 0 0 1-2-2zM4 14.75a2 2 0 0 1 2-2h9.5',
  A(hlines(7, 6.25, 2.75, [5.5, 3.5])),
]);
export const InsertCaption = glyph('InsertCaption', 'Insert Caption', [
  rect(2.75, 2.75, 14.5, 9.5, 1.5),
  'M5.75 9.75l2.75-3 2.25 2.25 1.5-1.5 2.25 2.25',
  A('M4.5 15h11M4.5 17.5h7'),
]);
export const TableOfFigures = glyph('TableOfFigures', 'Insert Table of Figures', [
  rect(2.75, 2.75, 6.5, 6.5, 1),
  'M4 8l1.75-2 1.25 1.25L8 6.25',
  A(hlines(11.25, 4.5, 3, [6, 4])),
  M(hlines(2.75, 12.5, 2.5, [14.5, 14.5, 10])),
]);
export const MarkEntry = glyph('MarkEntry', 'Mark Entry (Index)', [
  page(),
  A('M7 8.25h6M7 11h6'),
  M('M7 13.75h3.5'),
  A('M12.5 12.75h3.25l1.25 1.75-1.25 1.75H12.5z'),
]);

// Review ---------------------------------------------------------------------------------------------

export const Spelling = glyph('Spelling', 'Spelling & Grammar', [
  'M3 10.75L6 3.25h.5l3 7.5M4.25 8.25h4.25',
  'M11.5 3.5h2.75a1.75 1.75 0 0 1 0 3.5H11.5zM11.5 7h3.25a1.85 1.85 0 0 1 0 3.75H11.5z',
  A('M4 14.5l2.5 2.5 6-6'),
]);
export const Thesaurus = glyph('Thesaurus', 'Thesaurus', [
  'M10 5.25C8.5 3.75 6 3.25 3 3.5v12c3-.25 5.5.25 7 1.75 1.5-1.5 4-2 7-1.75v-12c-3-.25-5.5.25-7 1.75zM10 5.25v12',
  A('M5.5 7.5h2.25M12.25 7.5h2.25'),
]);
export const WordCount = glyph('WordCount', 'Word Count', [
  A('M3.5 4h3.5M5.25 4v6.25'),
  'M10 4h7M10 7.25h7M3 13h14M3 16.25h9',
  M('M8.5 7.25h.01'),
]);
export const Translate = glyph('Translate', 'Translate', [
  'M2.75 4.5h8M6.75 2.75V4.5M4.5 4.5c.75 2.75 2.75 5 5.5 6.25M9 4.5C8.25 7.5 6.25 9.75 3 11',
  A('M10.5 17.25L13.5 9h.5l3 8.25M11.5 14.75h5'),
]);
export const Language = glyph('Language', 'Language', [
  circle(10, 10, 7.25),
  A('M2.75 10h14.5M10 2.75c2 2 3 4.5 3 7.25s-1 5.25-3 7.25c-2-2-3-4.5-3-7.25s1-5.25 3-7.25z'),
]);
export const DeleteComment = glyph('DeleteComment', 'Delete Comment', [
  'M3 5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H9l-4 3v-3a2 2 0 0 1-2-2z',
  A(cross(10, 8.5, 2)),
]);
export const PreviousComment = glyph('PreviousComment', 'Previous Comment', [
  'M3 5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H9l-4 3v-3a2 2 0 0 1-2-2z',
  A(chevron(10, 8.5, 'left', 2.75)),
]);
export const NextComment = glyph('NextComment', 'Next Comment', [
  'M3 5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H9l-4 3v-3a2 2 0 0 1-2-2z',
  A(chevron(10, 8.5, 'right', 2.75)),
]);
export const ShowComments = glyph('ShowComments', 'Show Comments', [
  'M2.75 4.75a2 2 0 0 1 2-2h7a2 2 0 0 1 2 2v4.5a2 2 0 0 1-2 2H7.5L4.5 13.5v-2.25a1.75 1.75 0 0 1-1.75-1.75z',
  A('M15.25 7.25a2 2 0 0 1 2 2v3.5a2 2 0 0 1-2 2v2.5l-3-2.5H10a2 2 0 0 1-1.75-1'),
]);
export const TrackChanges = glyph('TrackChanges', 'Track Changes', [
  page(),
  hlines(7.25, 6.5, 2.5, [5.5]),
  A('M7.25 9h5.5'),
  A('M10.25 15.5l4.75-4.75 1.5 1.5-4.75 4.75H10.25z'),
]);
export const ShowMarkup = glyph('ShowMarkup', 'Show Markup', [
  page(),
  hlines(7.25, 7, 3, [5.5, 5.5]),
  A('M6.5 13h3.5'),
  A('M2.25 7v6'),
]);
export const ReviewingPane = glyph('ReviewingPane', 'Reviewing Pane', [
  rect(2.75, 2.75, 14.5, 14.5, 1.5),
  'M8 2.75v14.5',
  A(hlines(4.75, 6, 3, [1.5, 1.5, 1.5])),
  M(hlines(10.25, 6, 3, [4.75, 4.75, 3])),
]);
export const AcceptChange = glyph('AcceptChange', 'Accept', [
  page(4.75, 2.75, 10.5, 14.5),
  A('M7.25 11l2 2 4-4.5'),
]);
export const RejectChange = glyph('RejectChange', 'Reject', [page(), A(cross(10, 11, 2.25))]);
export const PreviousChange = glyph('PreviousChange', 'Previous Change', [
  page(6.75, 2.75, 10.5, 14.5),
  A(arrow(13.5, 10, 2.75, 10, 2.25)),
]);
export const NextChange = glyph('NextChange', 'Next Change', [
  page(2.75, 2.75, 10.5, 14.5),
  A(arrow(6.5, 10, 17.25, 10, 2.25)),
]);
export const CompareDocuments = glyph('CompareDocuments', 'Compare', [
  page(2.5, 2.75, 7, 11, 2),
  A(page(10.5, 6.25, 7, 11, 2)),
  M('M5 9.5h2M13 13h2'),
]);
export const RestrictEditing = glyph('RestrictEditing', 'Restrict Editing', [
  page(3.5, 2.75, 10, 14.5),
  A(rect(10.5, 12, 7, 5.5, 1)),
  A('M12 12v-1.5a2 2 0 0 1 4 0V12'),
]);
export const VersionHistory = glyph('VersionHistory', 'Version History', [
  'M3.25 10a6.75 6.75 0 1 0 2-4.8M3 2.75v2.75h2.75',
  A('M10 6.25V10l2.75 1.75'),
]);

// View ---------------------------------------------------------------------------------------------

export const ReadMode = glyph('ReadMode', 'Read Mode', [
  'M10 5.25C8.5 3.75 6 3.25 3 3.5v12c3-.25 5.5.25 7 1.75 1.5-1.5 4-2 7-1.75v-12c-3-.25-5.5.25-7 1.75zM10 5.25v12',
  A(hlines(5, 7.5, 2.75, [3, 3])),
  M(hlines(12, 7.5, 2.75, [3, 3])),
]);
export const PrintLayout = glyph('PrintLayout', 'Print Layout', [
  rect(2.75, 2.75, 14.5, 14.5, 1.5),
  A(rect(6, 5.25, 8, 12)),
  M(hlines(7.75, 8, 2.5, [4.5, 4.5, 3])),
]);
export const WebLayout = glyph('WebLayout', 'Web Layout', [
  rect(2.75, 3.25, 14.5, 13.5, 1.5),
  'M2.75 6.75h14.5',
  M('M5 5h.01M7 5h.01'),
  A(hlines(5, 9.5, 2.75, [10, 10, 6])),
]);
export const OutlineView = glyph('OutlineView', 'Outline', [
  A(dot(3.75, 4.5, 1)),
  'M6.25 4.5H17',
  dot(7.25, 8.75, 1),
  'M9.75 8.75H17',
  dot(7.25, 13, 1),
  'M9.75 13H17',
  A(dot(3.75, 17, 1)),
  'M6.25 17H14',
]);
export const DraftView = glyph('DraftView', 'Draft', [
  hlines(3, 4, 3, [14, 14, 9]),
  A('M3 13h14'),
  hlines(3, 16, 3, [10]),
]);
export const ImmersiveReader = glyph('ImmersiveReader', 'Immersive Reader', [
  'M10 5.25C8.5 3.75 6 3.25 3 3.5v12c3-.25 5.5.25 7 1.75 1.5-1.5 4-2 7-1.75v-12c-3-.25-5.5.25-7 1.75zM10 5.25v12',
  A('M12.75 7.25l.5 1 1 .5-1 .5-.5 1-.5-1-1-.5 1-.5z'),
]);
export const VerticalPages = glyph('VerticalPages', 'Vertical', [
  rect(5.25, 1.75, 9.5, 7, 1),
  rect(5.25, 11.25, 9.5, 7, 1),
  A(arrow(17.25, 6, 17.25, 14, 2)),
]);
export const SideToSide = glyph('SideToSide', 'Side to Side', [
  rect(2.25, 3.75, 7, 9.5, 1),
  rect(10.75, 3.75, 7, 9.5, 1),
  A(arrow(6, 16.5, 14, 16.5, 2)),
]);
export const Ruler = glyph('Ruler', 'Ruler', [
  rect(2.25, 6.25, 15.5, 7.5, 1),
  A('M5.25 6.25v2.5M8.25 6.25v4M11.25 6.25v2.5M14.25 6.25v4'),
]);
export const Gridlines = glyph('Gridlines', 'Gridlines', [
  M(grid(2.75, 2.75, 14.5, 14.5, 4, 4, 0.01)),
  A('M2.75 2.75h14.5v14.5H2.75z'),
]);
export const NavigationPane = glyph('NavigationPane', 'Navigation Pane', [
  rect(2.75, 2.75, 14.5, 14.5, 1.5),
  A('M7.75 2.75v14.5'),
  M(hlines(4.5, 6, 2.75, [1.5, 1.5, 1.5])),
  hlines(10, 6, 2.75, [5, 5, 3.5]),
]);
export const OnePage = glyph('OnePage', 'One Page', [
  rect(2.75, 2.75, 14.5, 14.5, 1.5),
  A(rect(6.5, 4.75, 7, 10.5, 0.5)),
]);
export const MultiplePages = glyph('MultiplePages', 'Multiple Pages', [
  rect(2.75, 2.75, 14.5, 14.5, 1.5),
  A(rect(4.75, 5.25, 4.5, 9.5, 0.5)),
  A(rect(10.75, 5.25, 4.5, 9.5, 0.5)),
]);
export const PageWidth = glyph('PageWidth', 'Page Width', [
  rect(2.75, 2.75, 14.5, 14.5, 1.5),
  A('M4.75 10h10.5M6.75 8l-2 2 2 2M13.25 8l2 2-2 2'),
]);
export const Zoom100 = glyph('Zoom100', '100%', [
  circle(8.5, 8.5, 5.75),
  'M12.75 12.75l4.5 4.5',
  A('M7.25 6.75l1.5-1.25v6'),
]);
export const ZoomToSelection = glyph('ZoomToSelection', 'Zoom to Selection', [
  circle(8.5, 8.5, 5.75),
  'M12.75 12.75l4.5 4.5',
  A(rect(6, 6, 5, 5, 0.5)),
]);
export const NewWindow = glyph('NewWindow', 'New Window', [
  rect(2.75, 4.75, 12, 12.5, 1.5),
  'M2.75 8.25h12',
  A('M7 2.75h8.75a1.5 1.5 0 0 1 1.5 1.5V13'),
]);
export const SplitWindow = glyph('SplitWindow', 'Split', [
  rect(2.75, 2.75, 14.5, 14.5, 1.5),
  A('M2.75 10h14.5'),
  M(hlines(5, 6, 0, [10])),
  M(hlines(5, 13.5, 0, [10])),
]);
export const SideBySide = glyph('SideBySide', 'View Side by Side', [
  rect(2.25, 3.75, 7.25, 12.5, 1),
  rect(10.5, 3.75, 7.25, 12.5, 1),
  A('M2.25 6.75h7.25M10.5 6.75h7.25'),
]);
export const Macros = glyph('Macros', 'Macros', [
  rect(2.75, 3.75, 14.5, 12.5, 1.5),
  A('M6 13V7l2.5 3.5L11 7v6'),
  'M13.5 13h1.5',
]);
export const ThemeToggle = glyph('ThemeToggle', 'Theme', [
  circle(10, 10, 7.25),
  FA('M10 2.75a7.25 7.25 0 0 1 0 14.5z'),
]);

// Grid: alignment, merge, number --------------------------------------------------------------------

export const MergeCenter = glyph('MergeCenter', 'Merge & Center', [
  rect(2.75, 4.75, 14.5, 10.5, 1),
  M('M7.5 4.75v2M12.5 4.75v2M7.5 13.25v2M12.5 13.25v2'),
  A('M4.5 10h11M6.5 8l-2 2 2 2M13.5 8l2 2-2 2'),
]);
export const MergeAcross = glyph('MergeAcross', 'Merge Across', [
  grid(2.75, 2.75, 14.5, 14.5, 1, 3, 1),
  A('M5 10h10M7 8.5L5 10l2 1.5M13 8.5l2 1.5-2 1.5'),
]);
export const MergeCells = glyph('MergeCells', 'Merge Cells', [
  rect(2.75, 2.75, 14.5, 14.5, 1),
  M('M10 2.75v3M10 14.25v3M2.75 10h3M14.25 10h3'),
  A(rect(7, 7, 6, 6, 0.75)),
]);
export const UnmergeCells = glyph('UnmergeCells', 'Unmerge Cells', [
  grid(2.75, 2.75, 14.5, 14.5, 2, 2, 1),
  A('M8 8L5.25 5.25M12 8l2.75-2.75M8 12l-2.75 2.75M12 12l2.75 2.75'),
]);
export const AlignTop = glyph('AlignTop', 'Top Align', [A('M3 3h14'), 'M7 6.25h6M7 9.5h6', M('M7 12.75h6')]);
export const AlignMiddle = glyph('AlignMiddle', 'Middle Align', [
  M('M3 3.5h14M3 16.5h14'),
  A('M6 8.25h8M6 11.75h8'),
]);
export const AlignBottom = glyph('AlignBottom', 'Bottom Align', [
  A('M3 17h14'),
  'M7 10.5h6M7 13.75h6',
  M('M7 7.25h6'),
]);
export const TextOrientation = glyph('TextOrientation', 'Orientation (text angle)', [
  'M3 14L7.5 3h.5l4.5 11M4.5 10.25h6.5',
  A(arrow(12.5, 17.25, 17.25, 12.5, 2)),
]);
export const AccountingFormat = glyph('AccountingFormat', 'Accounting Number Format', [
  rect(2.25, 5.25, 15.5, 9.5, 1.5),
  A(circle(10, 10, 2.25)),
  M('M5 10h.01M15 10h.01'),
]);
export const PercentStyle = glyph('PercentStyle', 'Percent Style', [
  'M15.5 4L4.5 16',
  A(circle(5.75, 5.75, 2)),
  A(circle(14.25, 14.25, 2)),
]);
export const CommaStyle = glyph('CommaStyle', 'Comma Style', [
  rect(2.25, 6, 3.25, 7, 1.625),
  A('M8 12.25v1.5a2.25 2.25 0 0 1-1.25 2'),
  rect(10, 6, 3.25, 7, 1.625),
  rect(14.5, 6, 3.25, 7, 1.625),
]);
export const IncreaseDecimal = glyph('IncreaseDecimal', 'Increase Decimal', [
  dot(3.5, 9.5, 1),
  'M6.75 5.5h3.5v5.5h-3.5zM12.25 5.5h3.5v5.5h-3.5z',
  A(arrow(15.75, 15.5, 7.25, 15.5, 2)),
]);
export const DecreaseDecimal = glyph('DecreaseDecimal', 'Decrease Decimal', [
  dot(3.5, 9.5, 1),
  'M6.75 5.5h3.5v5.5h-3.5z',
  A(arrow(7.25, 15.5, 15.75, 15.5, 2)),
]);
export const NumberFormat = glyph('NumberFormat', 'Number Format', [
  rect(2.75, 3.75, 14.5, 12.5, 1.5),
  A('M6 8.25l1.5-1v5.5M10.5 7.5a1.5 1.5 0 0 1 3 0c0 1.5-3 2.5-3 5h3'),
]);

// Grid: styles, cells, editing ------------------------------------------------------------------------

export const ConditionalFormatting = glyph('ConditionalFormatting', 'Conditional Formatting', [
  SHEET,
  FA(rect(8.75, 8.25, 4.5, 3.75)),
  A('M2.75 12h4.5'),
]);
export const FormatAsTable = glyph('FormatAsTable', 'Format as Table', [
  grid(2.75, 3.25, 14.5, 13.5, 3, 4, 1.5),
  FA('M4.25 3.25h11.5a1.5 1.5 0 0 1 1.5 1.5v1.875H2.75V4.75a1.5 1.5 0 0 1 1.5-1.5z'),
  { d: 'M2.75 10h14.5v3.375H2.75z', fill: true, muted: true },
]);
export const CellStyles = glyph('CellStyles', 'Cell Styles', [
  rect(2.75, 4.25, 14.5, 11.5, 1.5),
  A(rect(5.5, 7, 9, 6, 0.75)),
]);
export const InsertCells = glyph('InsertCells', 'Insert Cells', [
  grid(2.75, 2.75, 10.5, 10.5, 2, 2, 1),
  A(plus(15, 15, 2.5)),
]);
export const DeleteCells = glyph('DeleteCells', 'Delete Cells', [
  grid(2.75, 2.75, 10.5, 10.5, 2, 2, 1),
  A(cross(15, 15, 2)),
]);
export const FormatCells = glyph('FormatCells', 'Format Cells', [
  grid(2.75, 2.75, 10.5, 10.5, 2, 2, 1),
  A('M16.5 11.75l-4.75 4.75H10v-1.75l4.75-4.75z'),
]);
export const RowHeight = glyph('RowHeight', 'Row Height', [
  'M2.75 6h9.5M2.75 14h9.5',
  A('M15.5 3v14M14 4.5L15.5 3 17 4.5M14 15.5l1.5 1.5 1.5-1.5'),
]);
export const ColumnWidth = glyph('ColumnWidth', 'Column Width', [
  'M6 2.75v9.5M14 2.75v9.5',
  A('M3 15.5h14M4.5 14L3 15.5 4.5 17M15.5 14l1.5 1.5-1.5 1.5'),
]);
export const AutoSum = glyph('AutoSum', 'AutoSum', [A('M15 4.25V3H4.75l5.5 7-5.5 7H15v-1.25')]);
export const FillDown = glyph('FillDown', 'Fill', [
  grid(4.75, 2.75, 10.5, 14.5, 1, 3, 1),
  F(rect(5.5, 3.5, 9, 3.333, 0.25)),
  A(arrow(10, 9, 10, 15, 2)),
]);
export const ClearAll = glyph('ClearAll', 'Clear', [
  'M8 16.75h9',
  A('M12.25 3.25l4.5 4.5-7.5 7.5-3.5 1.5-2.75-2.75 1.5-3.5z'),
  'M7.75 7.75l4.5 4.5',
]);
export const SortFilter = glyph('SortFilter', 'Sort & Filter', [
  'M3 4.5L5.25 3 7.5 4.5M5.25 3v14M3 15.5l2.25 1.5 2.25-1.5',
  A('M9.75 4h7.5l-3 4.25v5L12.75 14.5v-6.25z'),
]);
export const FilterToggle = glyph('FilterToggle', 'Filter', [A('M3 3.5h14l-5.25 6.75v5.5l-3.5 1.75v-7.25z')]);
export const SortAscending = glyph('SortAscending', 'Sort A to Z', [
  'M3 8.5L5.25 3h.5L8 8.5M3.75 6.75h3.5',
  'M3.25 11.5h4.5l-4.5 5.5h4.5',
  A(arrow(14, 3.5, 14, 16.5, 2.5)),
]);
export const SortDescending = glyph('SortDescending', 'Sort Z to A', [
  'M3.25 3h4.5l-4.5 5.5h4.5',
  'M3 17l2.25-5.5h.5L8 17M3.75 15.25h3.5',
  A(arrow(14, 3.5, 14, 16.5, 2.5)),
]);
export const FindSelect = glyph('FindSelect', 'Find & Select', [
  circle(8.25, 8.25, 5.5),
  'M12.25 12.25l5 5',
  A('M6 8.25l1.5 1.5 3-3'),
]);
export const PivotTable = glyph('PivotTable', 'PivotTable', [
  rect(2.75, 2.75, 14.5, 14.5, 1.5),
  'M2.75 7h14.5M7 2.75v14.5',
  A('M10 11.5h4.5M13 10l1.5 1.5-1.5 1.5M11.5 10v4.5'),
]);
export const SparklineLine = glyph('SparklineLine', 'Line Sparkline', [
  rect(2.75, 4.75, 14.5, 10.5, 1.5),
  A('M4.75 12.25l2.75-3 2.5 2 2.75-4 2.5 2'),
]);
export const SparklineColumn = glyph('SparklineColumn', 'Column Sparkline', [
  rect(2.75, 4.75, 14.5, 10.5, 1.5),
  A('M6 13v-3M8.75 13V8M11.5 13v-2M14.25 13V7.25'),
]);
export const SparklineWinLoss = glyph('SparklineWinLoss', 'Win/Loss Sparkline', [
  rect(2.75, 4.75, 14.5, 10.5, 1.5),
  M('M4.75 10h10.5'),
  A('M6.25 10V7.75M9 10v2.25M11.75 10V7.75M14.25 10v2.25'),
]);
export const Slicer = glyph('Slicer', 'Slicer', [
  rect(2.75, 2.75, 14.5, 14.5, 1.5),
  'M2.75 6.5h14.5',
  A(rect(5, 8.75, 10, 2.5, 0.75)),
  M(rect(5, 13, 10, 2, 0.75)),
]);
export const Timeline = glyph('Timeline', 'Timeline', [
  rect(2.75, 4.25, 14.5, 11.5, 1.5),
  'M2.75 8h14.5',
  A('M5.75 11.75h5.25'),
  M('M5.75 11.75h.01M14.25 11.75h.01'),
]);
export const NoteSticky = glyph('NoteSticky', 'New Note', [
  'M3.75 4.75a1.5 1.5 0 0 1 1.5-1.5h9.5a1.5 1.5 0 0 1 1.5 1.5v7.5l-4.5 4.5h-6.5a1.5 1.5 0 0 1-1.5-1.5z',
  A('M16.25 12.25h-3a1.5 1.5 0 0 0-1.5 1.5v3'),
  M(hlines(6.5, 7, 2.75, [6.5, 4])),
]);
export const PrintArea = glyph('PrintArea', 'Print Area', [
  M(grid(2.75, 2.75, 14.5, 14.5, 3, 3, 1)),
  A('M2.75 2.75h9.667v9.667H2.75z'),
]);
export const PrintTitles = glyph('PrintTitles', 'Print Titles', [
  page(4.75, 2.75, 10.5, 14.5),
  FA('M4.75 2.75h7.5v3H4.75z'),
  M(hlines(7, 9, 2.5, [6, 6, 4])),
]);
export const ScaleToFit = glyph('ScaleToFit', 'Scale to Fit', [
  page(5.25, 4.25, 9.5, 11.5, 2),
  A('M2.75 6V2.75H6M14 2.75h3.25V6M17.25 14v3.25H14M6 17.25H2.75V14'),
]);
export const Headings = glyph('Headings', 'Headings', [
  rect(2.75, 2.75, 14.5, 14.5, 1),
  FA('M2.75 2.75h14.5V6.5H2.75zM2.75 6.5H6.5v10.75H2.75z'),
  M('M11.75 6.5v10.75M6.5 11.75h10.75'),
]);
export const FreezePanes = glyph('FreezePanes', 'Freeze Panes', [
  M(grid(2.75, 2.75, 14.5, 14.5, 3, 3, 1)),
  A('M2.75 7.583h14.5M7.583 2.75v14.5'),
]);
export const FreezeTopRow = glyph('FreezeTopRow', 'Freeze Top Row', [
  M(grid(2.75, 2.75, 14.5, 14.5, 3, 3, 1)),
  A('M2.75 7.583h14.5'),
]);
export const FreezeFirstColumn = glyph('FreezeFirstColumn', 'Freeze First Column', [
  M(grid(2.75, 2.75, 14.5, 14.5, 3, 3, 1)),
  A('M7.583 2.75v14.5'),
]);
export const PageBreakPreview = glyph('PageBreakPreview', 'Page Break Preview', [
  M(grid(2.75, 2.75, 14.5, 14.5, 4, 4, 1)),
  A('M10 2.75v14.5'),
  { d: 'M2.75 10h14.5', accent: true, muted: true },
]);
export const PageLayoutView = glyph('PageLayoutView', 'Page Layout', [
  page(3.75, 2.75, 12.5, 14.5, 0.01),
  A(rect(3.75, 2.75, 12.5, 3)),
  M(grid(5.75, 7.75, 8.5, 7.5, 2, 3, 0.01)),
]);

// Grid: formulas & data --------------------------------------------------------------------------------

export const InsertFunction = glyph('InsertFunction', 'Insert Function', [
  'M10.5 3.5h-1a2 2 0 0 0-2 2V15a2 2 0 0 1-2 2h-1M5 9h5',
  A('M11.5 9.5l5 5.5M16.5 9.5l-5 5.5'),
]);
export const NameManager = glyph('NameManager', 'Name Manager', [
  rect(2.75, 4.75, 14.5, 10.5, 1.5),
  A('M5.25 12.5V7.5l3.5 5V7.5'),
  'M11.25 10.5h3.5',
]);
export const DefineName = glyph('DefineName', 'Define Name', [
  rect(2.75, 4.75, 10.5, 8, 1.5),
  'M5.25 10.5V7l3 3.5V7',
  A(plus(15, 15, 2.5)),
]);
export const TracePrecedents = glyph('TracePrecedents', 'Trace Precedents', [
  rect(2.75, 2.75, 5.5, 4.5, 0.75),
  rect(11.75, 12.75, 5.5, 4.5, 0.75),
  A(dot(5.5, 5, 1)),
  A(arrow(5.5, 5, 13.5, 13, 2.5)),
]);
export const TraceDependents = glyph('TraceDependents', 'Trace Dependents', [
  rect(2.75, 2.75, 5.5, 4.5, 0.75),
  rect(11.75, 12.75, 5.5, 4.5, 0.75),
  A(dot(14.5, 15, 1)),
  A(arrow(14.5, 15, 6.5, 7, 2.5)),
]);
export const RemoveArrows = glyph('RemoveArrows', 'Remove Arrows', [
  M(arrow(3, 3, 11, 11, 2.5)),
  A(cross(14.5, 14.5, 2.5)),
]);
export const ShowFormulas = glyph('ShowFormulas', 'Show Formulas', [
  rect(2.75, 3.75, 14.5, 12.5, 1.5),
  A('M9 6.75h-.5a1.5 1.5 0 0 0-1.5 1.5v4a1.5 1.5 0 0 1-1.5 1.5H5M5.5 9.75H8.5'),
  'M11.5 9.5h3.25M11.5 11.75h3.25',
]);
export const ErrorChecking = glyph('ErrorChecking', 'Error Checking', [
  rect(2.75, 2.75, 10, 10, 1),
  A('M12.75 10.25l4.5 7.5h-9z'),
  A('M12.75 13.25v1.75M12.75 16.5h.01'),
]);
export const EvaluateFormula = glyph('EvaluateFormula', 'Evaluate Formula', [
  'M7.5 3.5h-.5a1.75 1.75 0 0 0-1.75 1.75v9.5A1.75 1.75 0 0 1 3.5 16.5H3M3.5 9h4.5',
  A('M10.5 8l3.5 3.5-3.5 3.5M14.5 8l3 3.5-3 3.5'),
]);
export const WatchWindow = glyph('WatchWindow', 'Watch Window', [
  rect(2.75, 3.75, 14.5, 12.5, 1.5),
  'M2.75 7h14.5',
  A('M5.5 11.75c1.25-2 2.75-3 4.5-3s3.25 1 4.5 3c-1.25 2-2.75 3-4.5 3s-3.25-1-4.5-3z'),
  A(dot(10, 11.75, 1)),
]);
export const CalculateNow = glyph('CalculateNow', 'Calculate Now', [
  rect(3.75, 2.75, 12.5, 14.5, 1.5),
  A(rect(6.25, 5.25, 7.5, 3, 0.5)),
  dot(7, 11.75, 0.85),
  dot(10, 11.75, 0.85),
  dot(13, 11.75, 0.85),
  dot(7, 14.5, 0.85),
  dot(10, 14.5, 0.85),
  dot(13, 14.5, 0.85),
]);
export const CalculateSheet = glyph('CalculateSheet', 'Calculate Sheet', [
  M(grid(2.75, 2.75, 10.5, 10.5, 2, 2, 1)),
  A(rect(10.75, 9.75, 6.5, 7.5, 1)),
  A('M12.5 12h3M12.5 14.75h3'),
]);
export const CalculationOptions = glyph('CalculationOptions', 'Calculation Options', [
  rect(2.75, 2.75, 10.5, 14.5, 1.5),
  'M5.25 5.5h5.5M5.25 9h.01M8 9h.01M5.25 12h.01M8 12h.01',
  A(circle(14.75, 14.75, 1.75)),
  A('M14.75 11.5v1.5M14.75 16.5V18M11.5 14.75H13M16.5 14.75H18'),
]);
export const FormulaLab = glyph('FormulaLab', 'Formula Lab', [
  'M7.5 2.75h5M8.5 2.75v5L3.75 15.5a1.25 1.25 0 0 0 1.1 1.75h10.3a1.25 1.25 0 0 0 1.1-1.75L11.5 7.75v-5',
  A('M5.5 12.5h9'),
  A(dot(9, 14.75, 0.85)),
  A(dot(11.75, 14.25, 0.6)),
]);
export const FromTextCsv = glyph('FromTextCsv', 'From Text/CSV', [
  page(),
  A('M8.5 9.25a1.75 1.75 0 1 0 0 3.5M11.5 9.25v3.5M11.5 12.75h1.25'),
  M('M7.25 15h5.5'),
]);
export const FromJson = glyph('FromJson', 'From JSON', [
  page(),
  A(
    'M8.25 8.75c-.75 0-1 .5-1 1v.75l-.75.75.75.75v.75c0 .5.25 1 1 1M11.75 8.75c.75 0 1 .5 1 1v.75l.75.75-.75.75v.75c0 .5-.25 1-1 1',
  ),
]);
export const RefreshAll = glyph('RefreshAll', 'Refresh All', [
  'M16.5 10a6.5 6.5 0 0 1-11.5 4.15M3.5 10A6.5 6.5 0 0 1 15 5.85',
  A('M15 2.75v3.1h-3.1M5 17.25v-3.1h3.1'),
]);
export const TextToColumns = glyph('TextToColumns', 'Text to Columns', [
  hlines(2.75, 4.5, 3, [14.5, 14.5]),
  A('M2.75 12.5h6M11.25 12.5h6M2.75 15.75h6M11.25 15.75h6'),
  M('M10 11v6.25'),
]);
export const FlashFill = glyph('FlashFill', 'Flash Fill', [
  grid(2.75, 2.75, 9, 14.5, 1, 4, 1),
  A('M15.5 3.5l-2.5 6h3.25L14 16.5'),
]);
export const RemoveDuplicates = glyph('RemoveDuplicates', 'Remove Duplicates', [
  rect(2.75, 2.75, 9.5, 5.5, 1),
  M(rect(2.75, 9.75, 9.5, 5.5, 1)),
  A(cross(15, 12.5, 2)),
]);
export const DataValidation = glyph('DataValidation', 'Data Validation', [
  grid(2.75, 2.75, 10.5, 10.5, 2, 2, 1),
  A('M10.5 14.25l2.25 2.25 4.75-5'),
]);
export const Consolidate = glyph('Consolidate', 'Consolidate', [
  rect(2.75, 2.75, 5, 5, 0.75),
  rect(2.75, 12.25, 5, 5, 0.75),
  A(rect(12.25, 7.5, 5, 5, 0.75)),
  A('M7.75 5.25h1.5a1.5 1.5 0 0 1 1.5 1.5V10M7.75 14.75h1.5a1.5 1.5 0 0 0 1.5-1.5V10h1.5'),
]);
export const WhatIfAnalysis = glyph('WhatIfAnalysis', 'What-If Analysis', [
  rect(2.75, 3.75, 14.5, 12.5, 1.5),
  A('M8 8.25a2 2 0 1 1 2.75 1.85c-.5.2-.75.6-.75 1.15v.5M10 13.75h.01'),
]);
export const GoalSeek = glyph('GoalSeek', 'Goal Seek', [
  circle(10, 10, 7.25),
  circle(10, 10, 4),
  A(dot(10, 10, 1.25)),
]);
export const ForecastSheet = glyph('ForecastSheet', 'Forecast Sheet', [
  'M2.75 2.75v14.5h14.5',
  'M5.5 13.5l3-3.75 2.75 2 2-2.75',
  A('M13.25 9l1.5-1.75 1.75-1.5'),
  M('M13.25 9l3.25.75M13.25 9l1.25-3.5'),
]);
export const GroupRows = glyph('GroupRows', 'Group', [
  'M8.5 4h8.5M8.5 8h8.5M8.5 12h8.5M8.5 16h8.5',
  A('M3.5 4v8h2M3.5 4h2'),
  A(plus(4.5, 16, 1.5)),
]);
export const UngroupRows = glyph('UngroupRows', 'Ungroup', [
  'M8.5 4h8.5M8.5 8h8.5M8.5 12h8.5M8.5 16h8.5',
  M('M3.5 4v8h2M3.5 4h2'),
  A('M3 16h3'),
]);
export const Subtotal = glyph('Subtotal', 'Subtotal', [
  hlines(3, 4, 3, [9, 9, 9]),
  A('M3 13h14M3 15.25h14'),
  A('M14.5 3.5v6.25'),
]);
export const ProtectSheet = glyph('ProtectSheet', 'Protect Sheet', [
  M(grid(2.75, 2.75, 10.5, 10.5, 2, 2, 1)),
  A(rect(10.5, 11.75, 7, 5.5, 1)),
  A('M12 11.75v-1.5a2 2 0 0 1 4 0v1.5'),
]);
export const ProtectWorkbook = glyph('ProtectWorkbook', 'Protect Workbook', [
  'M3 4.75a1.5 1.5 0 0 1 1.5-1.5H12a1.5 1.5 0 0 1 1.5 1.5V8M3 4.75v10a1.5 1.5 0 0 0 1.5 1.5h4.5',
  'M6 3.25v13',
  A(rect(10.5, 11.75, 7, 5.5, 1)),
  A('M12 11.75v-1.5a2 2 0 0 1 4 0v1.5'),
]);

// Grid: charts ---------------------------------------------------------------------------------------

export const ChartColumn = glyph('ChartColumn', 'Column or Bar Chart', [
  'M2.75 17.25h14.5',
  A('M5 17.25V10h2.5v7.25'),
  'M8.75 17.25V4h2.5v13.25',
  A('M12.5 17.25V7.5H15v9.75'),
]);
export const ChartLine = glyph('ChartLine', 'Line or Area Chart', [
  'M2.75 2.75v14.5h14.5',
  A('M5.25 13.5l3.25-4.25 2.75 2.5 5-6.5'),
]);
export const ChartPie = glyph('ChartPie', 'Pie or Doughnut Chart', [
  'M10 3a7 7 0 1 0 7 7h-7z',
  A('M12.5 2.75a5 5 0 0 1 4.75 4.75H12.5z'),
]);
export const ChartScatter = glyph('ChartScatter', 'Scatter (X, Y) or Bubble Chart', [
  'M2.75 2.75v14.5h14.5',
  A(dot(6.5, 12.5, 1.25)),
  A(dot(9.5, 9, 1.25)),
  A(dot(12.25, 11.25, 1.25)),
  A(dot(14.5, 5.5, 1.25)),
]);
export const ChartHierarchy = glyph('ChartHierarchy', 'Hierarchy Chart', [
  rect(2.75, 2.75, 14.5, 14.5, 1),
  A('M9.5 2.75v14.5M9.5 9.25h7.75M13.25 9.25v8'),
]);
export const ChartStatistic = glyph('ChartStatistic', 'Statistic Chart', [
  'M2.75 17.25h14.5',
  'M4.25 17.25V12h3v5.25M7.25 17.25V6h3v11.25',
  A('M10.25 17.25V9h3v8.25M13.25 17.25V14h3v3.25'),
]);
export const ChartWaterfall = glyph('ChartWaterfall', 'Waterfall or Funnel Chart', [
  'M2.75 17.25h14.5',
  'M4 16V9.5h2.5V16',
  A('M7.5 9.5V5.5H10v4'),
  A('M11 5.5V8h2.5V5.5'),
  'M14.5 8V16H17V8',
]);
export const ChartCombo = glyph('ChartCombo', 'Combo Chart', [
  'M2.75 17.25h14.5',
  M('M5 17.25v-5.5M8.5 17.25V9.5M12 17.25v-4M15.5 17.25V8'),
  A('M4 8.5l4-3.5 4 2.75 5-4.25'),
]);
export const RecommendedCharts = glyph('RecommendedCharts', 'Recommended Charts', [
  'M2.75 17.25h14.5',
  'M5 17.25V11M8.5 17.25V8',
  A('M14 2.75l.9 1.85 2.1.3-1.5 1.45.35 2.05-1.85-1-1.85 1 .35-2.05-1.5-1.45 2.1-.3z'),
  'M12 17.25v-5',
]);

// Deck ---------------------------------------------------------------------------------------------

export const NewSlide = glyph('NewSlide', 'New Slide', [
  rect(2.25, 3.25, 12, 9, 1.25),
  M(hlines(4.5, 6.25, 2.75, [7.5, 5])),
  A(plus(15.25, 15.25, 2.5)),
]);
export const SlideLayout = glyph('SlideLayout', 'Layout', [
  SLIDE,
  A('M5 6.75h10'),
  rect(5, 8.75, 4.5, 3.75, 0.5),
  rect(10.5, 8.75, 4.5, 3.75, 0.5),
]);
export const ResetSlide = glyph('ResetSlide', 'Reset', [
  'M17.25 9.5V5.75a1.5 1.5 0 0 0-1.5-1.5H4.25a1.5 1.5 0 0 0-1.5 1.5v7.5a1.5 1.5 0 0 0 1.5 1.5H8',
  A('M11.25 14.5a3 3 0 1 0 1-2.25M11 10.25v2.25h2.25'),
]);
export const Section = glyph('Section', 'Section', [
  A('M2.75 4.5h4.5l1.5 1.5h8.5'),
  rect(5.25, 8.75, 12, 3, 0.75),
  rect(5.25, 14, 12, 3, 0.75),
]);
export const TextDirection = glyph('TextDirection', 'Text Direction', [
  'M3 4.5h8M7 4.5v10',
  A(arrow(15, 3.5, 15, 16.5, 2.25)),
]);
export const AlignText = glyph('AlignText', 'Align Text', [
  rect(2.75, 2.75, 14.5, 14.5, 1.5),
  A('M6 8.75h8M6 11.25h8'),
]);
export const ConvertToDiagram = glyph('ConvertToDiagram', 'Convert to List Diagram', [
  rect(2.75, 2.75, 6, 4.5, 0.75),
  A(rect(11.25, 7.75, 6, 4.5, 0.75)),
  rect(2.75, 12.75, 6, 4.5, 0.75),
  'M8.75 5h1.25a1.25 1.25 0 0 1 1.25 1.25V10M8.75 15h1.25a1.25 1.25 0 0 0 1.25-1.25V10',
]);
export const Arrange = glyph('Arrange', 'Arrange', [
  M(rect(2.75, 2.75, 7.5, 7.5, 1)),
  A(rect(6.25, 6.25, 7.5, 7.5, 1)),
  rect(9.75, 9.75, 7.5, 7.5, 1),
]);
export const QuickStyles = glyph('QuickStyles', 'Quick Styles', [
  rect(2.75, 2.75, 6.25, 6.25, 1),
  FA(rect(11, 2.75, 6.25, 6.25, 1)),
  A(rect(2.75, 11, 6.25, 6.25, 1)),
  rect(11, 11, 6.25, 6.25, 1),
]);
export const ShapeFill = glyph('ShapeFill', 'Shape Fill', [
  'M8.25 2.75l6.5 6.5-5 5a1.5 1.5 0 0 1-2.1 0L3.5 10.1a1.5 1.5 0 0 1 0-2.1z',
  'M3.75 9h10.75',
  FA(rect(2.75, 15.75, 14.5, 2, 0.5)),
]);
export const ShapeOutline = glyph('ShapeOutline', 'Shape Outline', [
  'M13.25 2.75l3 3-8.25 8.25H5v-3z',
  'M11.25 4.75l3 3',
  FA(rect(2.75, 15.75, 14.5, 2, 0.5)),
]);
export const ShapeEffects = glyph('ShapeEffects', 'Shape Effects', [
  { d: rect(5, 5, 12.25, 12.25, 1.5), muted: true, accent: true, fill: true },
  rect(2.75, 2.75, 12, 12, 1.5),
]);
export const DesignIdeas = glyph('DesignIdeas', 'Design Ideas', [
  'M7.5 14.5h5M8.25 17h3.5',
  A('M10 2.75a5 5 0 0 0-3 9c.5.4.75.9.75 1.5v1.25h4.5v-1.25c0-.6.25-1.1.75-1.5a5 5 0 0 0-3-9z'),
]);
export const ReuseSlides = glyph('ReuseSlides', 'Reuse Slides', [
  rect(2.75, 2.75, 10, 7, 1),
  A(rect(7.25, 10.25, 10, 7, 1)),
  'M14.25 6.5h1.5a1.5 1.5 0 0 1 1.5 1.5v.75M5.75 13.5h-1.5a1.5 1.5 0 0 1-1.5-1.5v-.75',
]);
export const InsertVideo = glyph('InsertVideo', 'Video', [
  rect(2.75, 4.25, 14.5, 11.5, 1.5),
  A('M8.5 7.5v5l4-2.5z'),
]);
export const InsertAudio = glyph('InsertAudio', 'Audio', [
  'M3 8v4h3l4 3.5v-11L6 8z',
  A('M13 7.25a4 4 0 0 1 0 5.5M15.25 5a7 7 0 0 1 0 10'),
]);
export const SlideSize = glyph('SlideSize', 'Slide Size', [
  rect(4.75, 6.25, 12.5, 9, 1),
  A('M2.75 4.25V2.75h1.5M2.75 4.25L6 7.5'),
  M('M8 2.75h1.5M12 2.75h1.5'),
]);
export const FormatBackground = glyph('FormatBackground', 'Format Background', [
  SLIDE,
  FA('M2.75 11.75l4.25-3.5 3.5 2.75 2.5-2 4.25 3.25v2a1.5 1.5 0 0 1-1.5 1.5H4.25a1.5 1.5 0 0 1-1.5-1.5z'),
]);
export const TransitionPreview = glyph('TransitionPreview', 'Preview', [
  rect(2.75, 4.25, 8, 11.5, 1),
  A(rect(9.25, 4.25, 8, 11.5, 1)),
  A('M12 8l3 2-3 2z'),
]);
export const ApplyToAll = glyph('ApplyToAll', 'Apply To All', [
  rect(2.75, 2.75, 7, 5, 0.75),
  rect(10.25, 2.75, 7, 5, 0.75),
  rect(2.75, 9.5, 7, 5, 0.75),
  A(rect(10.25, 9.5, 7, 5, 0.75)),
  A('M4 17.25h12'),
]);
export const AnimationPane = glyph('AnimationPane', 'Animation Pane', [
  rect(2.75, 2.75, 14.5, 14.5, 1.5),
  'M11.75 2.75v14.5',
  A('M5 7.5l1.25-1.25L7.5 7.5 8.75 6.25'),
  M(hlines(14, 6, 3, [1.25, 1.25, 1.25])),
]);
export const AddAnimation = glyph('AddAnimation', 'Add Animation', [
  A('M9 3.25l1.4 3.1 3.35.4-2.5 2.3.7 3.3L9 10.65l-2.95 1.7.7-3.3-2.5-2.3 3.35-.4z'),
  plus(14.5, 14.5, 2.5),
]);
export const AnimationPainter = glyph('AnimationPainter', 'Animation Painter', [
  rect(3.25, 2.75, 11, 4.5, 1.25),
  'M14.25 5h1.5a1 1 0 0 1 1 1v2.25a1 1 0 0 1-1 1H9.75a1 1 0 0 0-1 1v1.25',
  A('M9 13.5l.6 1.4 1.5.15-1.1 1 .3 1.45L9 16.75l-1.3.75.3-1.45-1.1-1 1.5-.15z'),
]);
export const FromBeginning = glyph('FromBeginning', 'From Beginning', [
  SLIDE,
  A('M8.5 7.25v5.5l4.5-2.75z'),
  'M6 7.25v5.5',
]);
export const FromCurrent = glyph('FromCurrent', 'From Current Slide', [SLIDE, A('M8 7.25v5.5l4.5-2.75z')]);
export const PresentInTab = glyph('PresentInTab', 'Present in Browser Tab', [
  rect(2.75, 3.25, 14.5, 13.5, 1.5),
  'M2.75 6.75h14.5',
  M('M5 5h.01M7 5h.01'),
  A('M8.5 9.5v4.5l3.75-2.25z'),
]);
export const SetUpShow = glyph('SetUpShow', 'Set Up Slide Show', [
  rect(2.75, 3.25, 14.5, 9.5, 1.25),
  'M10 12.75v2.5M6.5 17.25h7',
  A(circle(10, 8, 1.5)),
  A('M10 5.25v.75M10 10v.75M12.75 8h-.75M7.25 8h.75'),
]);
export const HideSlide = glyph('HideSlide', 'Hide Slide', [
  rect(2.75, 4.25, 14.5, 10.5, 1.5),
  A('M3.5 17.25L16.5 2.75'),
]);
export const RehearseTimings = glyph('RehearseTimings', 'Rehearse Timings', [
  circle(10, 11, 6.25),
  'M8 2.75h4M10 2.75v2',
  A('M10 8.25V11l2 1.5'),
]);
export const PresenterView = glyph('PresenterView', 'Presenter View', [
  rect(2.75, 3.25, 9, 6.5, 1),
  A(rect(13, 3.25, 4.25, 3, 0.5)),
  M(rect(13, 7.25, 4.25, 2.5, 0.5)),
  hlines(2.75, 13, 2.75, [14.5, 10]),
]);
export const LaserPointer = glyph('LaserPointer', 'Laser Pointer', [
  'M3.25 16.75l7.5-7.5',
  FA(circle(13.25, 6.75, 2.25)),
  A('M13.25 2.25v1M17.75 6.75h-1M16.4 3.6l-.7.7'),
]);
export const PenTool = glyph('PenTool', 'Pen', [
  'M13.75 3l3.25 3.25-9.75 9.75-4 .75.75-4z',
  A('M11.75 5l3.25 3.25'),
]);
export const HighlighterTool = glyph('HighlighterTool', 'Highlighter', [
  'M12.25 2.75l5 5-6.5 6.5-5-5z',
  'M5.75 9.25L4 11v3.5h-1.25L2.75 17h4l1.25-1.25v-1.5h1.75l1-1.25',
  FA(rect(11, 15.75, 6.25, 1.75, 0.5)),
]);
export const Blackout = glyph('Blackout', 'Black Screen', [
  F(rect(2.75, 4.25, 14.5, 11.5, 1.5)),
  'M7 17.75h6',
]);
export const SlideSorter = glyph('SlideSorter', 'Slide Sorter', [
  rect(2.75, 3.5, 6.25, 5, 0.75),
  rect(11, 3.5, 6.25, 5, 0.75),
  A(rect(2.75, 11.5, 6.25, 5, 0.75)),
  rect(11, 11.5, 6.25, 5, 0.75),
]);
export const NotesPage = glyph('NotesPage', 'Notes Page', [
  page(4.25, 2.75, 11.5, 14.5, 0.01),
  A(rect(6.25, 4.75, 7.5, 5, 0.5)),
  M(hlines(6.25, 12.5, 2.5, [7.5, 5])),
]);
export const ReadingView = glyph('ReadingView', 'Reading View', [
  SLIDE,
  A('M6.5 9.5c1-1.5 2.25-2.25 3.5-2.25S12.5 8 13.5 9.5c-1 1.5-2.25 2.25-3.5 2.25S7.5 11 6.5 9.5z'),
  'M6.5 17.25h7',
]);
export const NormalView = glyph('NormalView', 'Normal', [
  rect(2.75, 2.75, 14.5, 14.5, 1.5),
  'M6.75 2.75v14.5',
  A(rect(8.75, 5, 6.5, 5, 0.5)),
  M('M8.75 13.75h6.5'),
]);
export const SlideMaster = glyph('SlideMaster', 'Slide Master', [
  rect(5.25, 2.75, 12, 8.5, 1),
  A('M7.5 5.25h7.5'),
  M(hlines(7.5, 7.5, 2, [5, 3.5])),
  'M2.75 6v8.75a1.5 1.5 0 0 0 1.5 1.5H13',
]);
export const HandoutMaster = glyph('HandoutMaster', 'Handout Master', [
  page(4.25, 2.75, 11.5, 14.5, 0.01),
  A(rect(6.25, 4.75, 3, 2.5, 0.25)),
  A(rect(10.75, 4.75, 3, 2.5, 0.25)),
  A(rect(6.25, 9, 3, 2.5, 0.25)),
  A(rect(10.75, 9, 3, 2.5, 0.25)),
  M('M6.25 14.25h7.5'),
]);
export const NotesMaster = glyph('NotesMaster', 'Notes Master', [
  page(4.25, 2.75, 11.5, 14.5, 0.01),
  A(rect(6.25, 4.75, 7.5, 5, 0.5)),
  'M6.25 12.5h7.5M6.25 15h4',
]);
export const FitToWindow = glyph('FitToWindow', 'Fit Slide to Current Window', [
  rect(5.75, 6.75, 8.5, 6.5, 0.75),
  A('M2.75 6V2.75H6M14 2.75h3.25V6M17.25 14v3.25H14M6 17.25H2.75V14'),
]);
export const SlideNumber = glyph('SlideNumber', 'Slide Number', [
  SLIDE,
  A('M12 12.25h2.25M13 12.25v-3.5l-1 .75'),
]);
export const HeaderFooterDeck = glyph('HeaderFooterDeck', 'Header & Footer', [
  SLIDE,
  A('M5 12.25h4.5'),
  M('M5 7h10'),
]);
export const LiveGridRange = glyph('LiveGridRange', 'Live Grid Range', [
  SLIDE,
  A(grid(5.25, 6.75, 9.5, 5.5, 3, 2, 0.5)),
]);

// Notes ----------------------------------------------------------------------------------------------

export const TagToDo = glyph('TagToDo', 'To Do Tag', [
  rect(3.25, 3.25, 13.5, 13.5, 2),
  A('M6.5 10.25l2.5 2.5 4.5-5'),
]);
export const TagImportant = glyph('TagImportant', 'Important Tag', [
  A('M10 2.75l2.2 4.6 5.05.65-3.7 3.5.95 5L10 14.05 5.5 16.5l.95-5-3.7-3.5 5.05-.65z'),
]);
export const TagQuestion = glyph('TagQuestion', 'Question Tag', [
  circle(10, 10, 7.25),
  A('M7.75 8a2.25 2.25 0 1 1 3.1 2.1c-.5.2-.85.65-.85 1.2v.45M10 14.25h.01'),
]);
export const RuleLines = glyph('RuleLines', 'Rule Lines', [
  page(4.25, 2.75, 11.5, 14.5, 0.01),
  A(hlines(4.25, 6.5, 2.75, [11.5, 11.5, 11.5, 11.5])),
  M('M7 2.75v14.5'),
]);
export const PageColor = glyph('PageColor', 'Page Colour', [page(), FA('M4.75 12.5h10.5v4.75H4.75z')]);
export const AttachFile = glyph('AttachFile', 'File Attachment', [
  'M15.5 9.5l-5.75 5.75a3.75 3.75 0 0 1-5.3-5.3l6.25-6.25a2.5 2.5 0 0 1 3.55 3.55l-6.25 6.25a1.25 1.25 0 0 1-1.75-1.75l5.5-5.5',
]);
