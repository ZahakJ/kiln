/**
 * Attribute types of the Write document model. They mirror WordprocessingML (ECMA-376 Part 1 §17)
 * semantics and keep the file's units, so a DOCX round trip never re-quantises a value:
 *
 * - lengths (indents, spacing, page size, margins, tab positions, table widths) are **twips**
 *   (twentieths of a point, `w:ind/@w:left`, `w:pgSz/@w:w`, …);
 * - font sizes are **half-points** (`w:sz`);
 * - border widths are **eighths of a point** (`w:bdr/@w:sz`), border spacing is points;
 * - line spacing with `lineRule="auto"` is **240ths of a line** (`240` = single, `259` = 1.08);
 * - drawing extents are **EMU** (914400 per inch).
 *
 * Every property is optional: `undefined` means "not set at this level, inherit". The resolved
 * (effective) values come from `resolveParagraphProps` / `resolveRunProps` in `styles/resolve.ts`.
 */

// ------------------------------------------------------------------ simple types

/** `ST_Jc` for paragraphs. `both` is Word's "Justify"; `start`/`end` are normalised to left/right. */
export type Alignment =
  | 'left'
  | 'center'
  | 'right'
  | 'both'
  | 'distribute'
  | 'mediumKashida'
  | 'highKashida'
  | 'lowKashida'
  | 'thaiDistribute';

/** `ST_LineSpacingRule`: `auto` = multiple of a line in 240ths, `exact`/`atLeast` = twips. */
export type LineRule = 'auto' | 'exact' | 'atLeast';

/** `ST_Underline`. */
export type UnderlineStyle =
  | 'single'
  | 'words'
  | 'double'
  | 'thick'
  | 'dotted'
  | 'dottedHeavy'
  | 'dash'
  | 'dashedHeavy'
  | 'dashLong'
  | 'dashLongHeavy'
  | 'dotDash'
  | 'dashDotHeavy'
  | 'dotDotDash'
  | 'dashDotDotHeavy'
  | 'wave'
  | 'wavyHeavy'
  | 'wavyDouble'
  | 'none';

/** `ST_VerticalAlignRun`. */
export type VertAlign = 'baseline' | 'superscript' | 'subscript';

/** `ST_HighlightColor` (the 16 Word highlighter colours plus `none`). */
export type HighlightColor =
  | 'black'
  | 'blue'
  | 'cyan'
  | 'green'
  | 'magenta'
  | 'red'
  | 'yellow'
  | 'white'
  | 'darkBlue'
  | 'darkCyan'
  | 'darkGreen'
  | 'darkMagenta'
  | 'darkRed'
  | 'darkYellow'
  | 'darkGray'
  | 'lightGray'
  | 'none';

/** Hex values Word renders for each highlight colour. */
export const HIGHLIGHT_HEX: Readonly<Record<Exclude<HighlightColor, 'none'>, string>> = {
  black: '000000',
  blue: '0000FF',
  cyan: '00FFFF',
  green: '00FF00',
  magenta: 'FF00FF',
  red: 'FF0000',
  yellow: 'FFFF00',
  white: 'FFFFFF',
  darkBlue: '000080',
  darkCyan: '008080',
  darkGreen: '008000',
  darkMagenta: '800080',
  darkRed: '800000',
  darkYellow: '808000',
  darkGray: '808080',
  lightGray: 'C0C0C0',
};

/** `ST_ThemeColor` as used by `w:color/@w:themeColor` and `w:shd/@w:themeFill`. */
export type ThemeColorName =
  | 'dark1'
  | 'light1'
  | 'dark2'
  | 'light2'
  | 'accent1'
  | 'accent2'
  | 'accent3'
  | 'accent4'
  | 'accent5'
  | 'accent6'
  | 'hyperlink'
  | 'followedHyperlink'
  | 'none'
  | 'background1'
  | 'text1'
  | 'background2'
  | 'text2';

/**
 * A WordprocessingML colour: `w:val` (`RRGGBB` or `auto`) plus optional theme reference. When a theme
 * colour is present it wins over `val` (which is then only the cached rendering).
 */
export interface Color {
  /** `RRGGBB` (upper-case hex, no `#`) or `auto`. */
  val: string;
  themeColor?: ThemeColorName;
  /** `w:themeTint`, 0–255 as in the file (hex byte parsed to a number). */
  themeTint?: number;
  /** `w:themeShade`, 0–255. */
  themeShade?: number;
}

/** `ST_Border` values; the common ones are listed, others pass through as strings. */
export type BorderStyle =
  | 'nil'
  | 'none'
  | 'single'
  | 'thick'
  | 'double'
  | 'dotted'
  | 'dashed'
  | 'dotDash'
  | 'dotDotDash'
  | 'triple'
  | 'thinThickSmallGap'
  | 'thickThinSmallGap'
  | 'thinThickMediumGap'
  | 'thickThinMediumGap'
  | 'thinThickLargeGap'
  | 'thickThinLargeGap'
  | 'wave'
  | 'doubleWave'
  | 'dashSmallGap'
  | 'dashDotStroked'
  | 'threeDEmboss'
  | 'threeDEngrave'
  | 'outset'
  | 'inset'
  | (string & {});

/** One border edge (`w:top`, `w:left`, …). */
export interface Border {
  style: BorderStyle;
  /** Width in eighths of a point (`w:sz`). */
  size?: number;
  /** Distance from text in points (`w:space`). */
  space?: number;
  color?: Color;
  shadow?: boolean;
  frame?: boolean;
}

export interface ParagraphBorders {
  top?: Border;
  left?: Border;
  bottom?: Border;
  right?: Border;
  /** Border between consecutive paragraphs with identical border settings. */
  between?: Border;
  bar?: Border;
}

export interface TableBorders {
  top?: Border;
  left?: Border;
  bottom?: Border;
  right?: Border;
  insideH?: Border;
  insideV?: Border;
}

export interface CellBorders extends TableBorders {
  tl2br?: Border;
  tr2bl?: Border;
}

/** `w:shd`. */
export interface Shading {
  /** `ST_Shd` pattern (`clear`, `solid`, `pct10`, `horzStripe`, …). */
  val: string;
  /** Pattern colour. */
  color?: Color;
  /** Background fill (`RRGGBB`/`auto`) with optional theme reference. */
  fill?: Color;
}

/** `ST_TabJc`. */
export type TabAlignment = 'left' | 'center' | 'right' | 'decimal' | 'bar' | 'clear' | 'num';
/** `ST_TabTlc`. */
export type TabLeader = 'none' | 'dot' | 'hyphen' | 'underscore' | 'heavy' | 'middleDot';

export interface TabStop {
  /** Position in twips from the leading indent edge of the text area. */
  pos: number;
  align: TabAlignment;
  leader?: TabLeader;
}

export interface Indent {
  /** Leading indent (twips). `w:start` is normalised to `left`. */
  left?: number;
  /** Trailing indent (twips). `w:end` is normalised to `right`. */
  right?: number;
  /** First-line indent (twips). Mutually exclusive with `hanging`. */
  firstLine?: number;
  /** Hanging indent (twips). Mutually exclusive with `firstLine`. */
  hanging?: number;
}

export interface Spacing {
  /** Space before in twips. */
  before?: number;
  /** Space after in twips. */
  after?: number;
  /** Line spacing: 240ths of a line when `lineRule` is `auto`, twips otherwise. */
  line?: number;
  lineRule?: LineRule;
  beforeAutospacing?: boolean;
  afterAutospacing?: boolean;
}

/** `w:numPr`. `numId` 0 explicitly removes numbering inherited from a style. */
export interface NumberingRef {
  numId: number;
  /** List level 0–8 (`w:ilvl`). */
  level: number;
}

/** Paragraph properties (`w:pPr`). */
export interface ParagraphProps {
  styleId?: string;
  alignment?: Alignment;
  indent?: Indent;
  spacing?: Spacing;
  numbering?: NumberingRef;
  keepNext?: boolean;
  keepLines?: boolean;
  pageBreakBefore?: boolean;
  widowControl?: boolean;
  contextualSpacing?: boolean;
  suppressAutoHyphens?: boolean;
  suppressLineNumbers?: boolean;
  /** Outline level 0–8 (9 = body text). Headings 1–9 are 0–8. */
  outlineLevel?: number;
  tabs?: TabStop[];
  borders?: ParagraphBorders;
  shading?: Shading;
  bidi?: boolean;
  /** `w:textAlignment` (vertical alignment of characters on the line). */
  textAlignment?: 'top' | 'center' | 'baseline' | 'bottom' | 'auto';
  /** Drop cap frame (`w:framePr/@w:dropCap`) with the number of lines. */
  dropCap?: { type: 'drop' | 'margin'; lines: number };
}

/** Theme or explicit font names per script slot (`w:rFonts`). */
export interface RunFonts {
  ascii?: string;
  hAnsi?: string;
  eastAsia?: string;
  cs?: string;
  asciiTheme?: ThemeFontRef;
  hAnsiTheme?: ThemeFontRef;
  eastAsiaTheme?: ThemeFontRef;
  csTheme?: ThemeFontRef;
}

/** `ST_Theme` font references. */
export type ThemeFontRef =
  | 'majorAscii'
  | 'majorHAnsi'
  | 'majorEastAsia'
  | 'majorBidi'
  | 'minorAscii'
  | 'minorHAnsi'
  | 'minorEastAsia'
  | 'minorBidi';

export interface Underline {
  style: UnderlineStyle;
  color?: Color;
}

/** Run properties (`w:rPr`). Toggle properties are tri-state: `undefined` = inherit. */
export interface RunProps {
  /** Character style (`w:rStyle`). */
  styleId?: string;
  fonts?: RunFonts;
  bold?: boolean;
  boldCs?: boolean;
  italic?: boolean;
  italicCs?: boolean;
  underline?: Underline;
  strike?: boolean;
  dstrike?: boolean;
  caps?: boolean;
  smallCaps?: boolean;
  hidden?: boolean;
  emboss?: boolean;
  imprint?: boolean;
  outline?: boolean;
  shadow?: boolean;
  color?: Color;
  highlight?: HighlightColor;
  shading?: Shading;
  /** Font size in half-points (`w:sz`). */
  size?: number;
  /** Complex-script font size in half-points (`w:szCs`). */
  sizeCs?: number;
  vertAlign?: VertAlign;
  /** Character spacing adjustment in twips (`w:spacing`). */
  spacing?: number;
  /** Horizontal scale percentage (`w:w`). */
  scale?: number;
  /** Kerning threshold in half-points (`w:kern`). */
  kern?: number;
  /** Baseline offset in half-points (`w:position`). */
  position?: number;
  rtl?: boolean;
  lang?: { val?: string; eastAsia?: string; bidi?: string };
}

/** The toggle properties of ECMA-376 §17.7.3 (XOR between style types). */
export const TOGGLE_RUN_PROPS = [
  'bold',
  'boldCs',
  'italic',
  'italicCs',
  'caps',
  'smallCaps',
  'strike',
  'dstrike',
  'hidden',
  'emboss',
  'imprint',
  'outline',
  'shadow',
] as const satisfies readonly (keyof RunProps)[];
export type ToggleRunProp = (typeof TOGGLE_RUN_PROPS)[number];

// ------------------------------------------------------------------ tables

/** `w:tblW`, `w:tcW`, …: `dxa` = twips, `pct` = fiftieths of a percent (5000 = 100 %), `auto`, `nil`. */
export interface TableWidth {
  type: 'dxa' | 'pct' | 'auto' | 'nil';
  value: number;
}

export interface CellMargins {
  top?: number;
  left?: number;
  bottom?: number;
  right?: number;
}

/** `w:tblLook` — which conditional formats of the table style apply. */
export interface TableLook {
  firstRow: boolean;
  lastRow: boolean;
  firstColumn: boolean;
  lastColumn: boolean;
  noHBand: boolean;
  noVBand: boolean;
}

export const DEFAULT_TABLE_LOOK: Readonly<TableLook> = {
  firstRow: true,
  lastRow: false,
  firstColumn: true,
  lastColumn: false,
  noHBand: false,
  noVBand: true,
};

/** Table properties (`w:tblPr`). */
export interface TableProps {
  styleId?: string;
  width?: TableWidth;
  alignment?: 'left' | 'center' | 'right';
  /** `w:tblInd` (twips). */
  indent?: number;
  borders?: TableBorders;
  shading?: Shading;
  cellMargins?: CellMargins;
  /** `w:tblCellSpacing` (twips). */
  cellSpacing?: number;
  layout?: 'fixed' | 'autofit';
  look?: TableLook;
  /** Rows per horizontal band (`w:tblStyleRowBandSize`, default 1). */
  rowBandSize?: number;
  /** Columns per vertical band (`w:tblStyleColBandSize`, default 1). */
  colBandSize?: number;
  bidiVisual?: boolean;
  caption?: string;
  description?: string;
}

/** Table row properties (`w:trPr`). */
export interface TableRowProps {
  /** Row height in twips with rule. */
  height?: { value: number; rule: 'auto' | 'atLeast' | 'exact' };
  /** Repeat as header row on each page (`w:tblHeader`). */
  header?: boolean;
  cantSplit?: boolean;
  /** Row alignment override (`w:jc`). */
  alignment?: 'left' | 'center' | 'right';
}

/** Table cell properties (`w:tcPr`) besides the span attributes on the node. */
export interface TableCellProps {
  width?: TableWidth;
  borders?: CellBorders;
  shading?: Shading;
  margins?: CellMargins;
  vAlign?: 'top' | 'center' | 'bottom';
  noWrap?: boolean;
  textDirection?: 'lrTb' | 'tbRl' | 'btLr' | 'lrTbV' | 'tbRlV' | 'tbLrV';
  fitText?: boolean;
  hideMark?: boolean;
}

/** `ST_TblStyleOverrideType` — conditional formatting regions of a table style. */
export type TableConditionType =
  | 'wholeTable'
  | 'firstRow'
  | 'lastRow'
  | 'firstCol'
  | 'lastCol'
  | 'band1Vert'
  | 'band2Vert'
  | 'band1Horz'
  | 'band2Horz'
  | 'neCell'
  | 'nwCell'
  | 'seCell'
  | 'swCell';

// ------------------------------------------------------------------ sections

/** `ST_SectionMark`: how a section starts relative to the previous one. */
export type SectionBreakType = 'nextPage' | 'continuous' | 'evenPage' | 'oddPage' | 'nextColumn';

export type HeaderFooterType = 'default' | 'first' | 'even';

export interface PageSize {
  /** Width in twips. */
  width: number;
  /** Height in twips. */
  height: number;
  orientation: 'portrait' | 'landscape';
  /** Printer paper code (`w:code`), preserved. */
  code?: number;
}

export interface PageMargins {
  top: number;
  right: number;
  bottom: number;
  left: number;
  header: number;
  footer: number;
  gutter: number;
}

export interface Columns {
  count: number;
  /** Spacing between columns in twips (equal-width columns). */
  space: number;
  equalWidth: boolean;
  separator: boolean;
  /** Individual column widths/spacing when `equalWidth` is false. */
  cols?: { width: number; space: number }[];
}

export interface PageNumbering {
  /** `w:pgNumType/@w:start`; undefined continues from the previous section. */
  start?: number;
  /** Number format (`decimal`, `lowerRoman`, …). */
  format?: string;
}

/** Section properties (`w:sectPr`). */
export interface SectionProps {
  type: SectionBreakType;
  pageSize: PageSize;
  margins: PageMargins;
  columns: Columns;
  /** Different first page header/footer (`w:titlePg`). */
  titlePage: boolean;
  pageNumbering?: PageNumbering;
  /** Vertical alignment of text on the page (`w:vAlign`). */
  vAlign?: 'top' | 'center' | 'both' | 'bottom';
  bidi?: boolean;
  /** `w:docGrid`, preserved. */
  docGrid?: { type?: string; linePitch?: number; charSpace?: number };
  /** Header part references by kind (relationship ids in DOCX); absent = inherit from previous section. */
  headerRefs?: Partial<Record<HeaderFooterType, string>>;
  footerRefs?: Partial<Record<HeaderFooterType, string>>;
  lineNumbering?: {
    countBy: number;
    start?: number;
    distance?: number;
    restart: 'newPage' | 'newSection' | 'continuous';
  };
}

// ------------------------------------------------------------------ drawings

export type WrapType =
  | 'inline'
  | 'square'
  | 'tight'
  | 'through'
  | 'topAndBottom'
  | 'behind'
  | 'inFront'
  | 'none';

/** Floating (anchored) object position (`wp:positionH` / `wp:positionV`). */
export interface AnchorPosition {
  relativeFromH:
    | 'character'
    | 'column'
    | 'insideMargin'
    | 'leftMargin'
    | 'margin'
    | 'outsideMargin'
    | 'page'
    | 'rightMargin';
  relativeFromV:
    | 'insideMargin'
    | 'line'
    | 'margin'
    | 'outsideMargin'
    | 'page'
    | 'paragraph'
    | 'topMargin'
    | 'bottomMargin';
  /** Offset in EMU, or an alignment keyword. */
  h: { offset: number } | { align: 'left' | 'center' | 'right' | 'inside' | 'outside' };
  v: { offset: number } | { align: 'top' | 'center' | 'bottom' | 'inside' | 'outside' };
  /** Distance from text (EMU). */
  distT?: number;
  distB?: number;
  distL?: number;
  distR?: number;
  allowOverlap?: boolean;
  layoutInCell?: boolean;
  /** z-order (`relativeHeight`). */
  zIndex?: number;
}

// ------------------------------------------------------------------ revisions

/** Author/date metadata of a tracked change (`w:ins`, `w:del`, `w:rPrChange`). */
export interface RevisionInfo {
  id: string;
  author: string;
  /** ISO 8601, or empty when the file omits it. */
  date: string;
}

/** Tracked changes attached to a paragraph node (`revision` attribute). */
export interface ParagraphRevision {
  /** `w:pPrChange`: the paragraph properties before the tracked formatting change. */
  pPrChange?: RevisionInfo & { before: ParagraphProps };
  /** Tracked insertion/deletion of the paragraph mark (`w:pPr/w:rPr/w:ins|w:del`). */
  mark?: RevisionInfo & { kind: 'insertion' | 'deletion' };
  /** `w:pPr/w:rPr/w:rPrChange`: paragraph-mark run properties before a tracked formatting change. */
  markPrChange?: RevisionInfo & { before: RunProps };
}

/** Tracked insertion/deletion of a whole table row (`w:trPr/w:ins|w:del`). */
export interface RowRevision extends RevisionInfo {
  kind: 'insertion' | 'deletion';
}

/** `ST_SdtType`-like classification of a content control, derived from `w:sdtPr`. */
export type SdtKind =
  | 'richText'
  | 'text'
  | 'date'
  | 'dropDownList'
  | 'comboBox'
  | 'checkbox'
  | 'picture'
  | 'docPartObj'
  | 'docPartList'
  | 'group'
  | 'bibliography'
  | 'citation'
  | 'equation'
  | 'repeatingSection'
  | 'repeatingSectionItem'
  | 'other';

/**
 * A content control (`w:sdt`). The editor shows its title/tag and kind; `pr`/`endPr` keep the original
 * `w:sdtPr`/`w:sdtEndPr` XML so a writer re-wraps the content exactly. Blocks that belong to the same
 * block-level control share `key`.
 */
export interface SdtInfo {
  key: string;
  level: 'block' | 'inline' | 'row' | 'cell';
  kind: SdtKind;
  id?: string;
  tag?: string;
  alias?: string;
  /** Gallery of a building-block control (`Table of Contents`, `Cover Pages`, …). */
  docPartGallery?: string;
  /** Checkbox state (`w14:checkbox/w14:checked`). */
  checked?: boolean;
  /** Showing placeholder text (`w:showingPlcHdr`). */
  showingPlaceholder?: boolean;
  /** List items of drop-down/combo controls. */
  listItems?: { displayText: string; value: string }[];
  /** Date format of a date picker (`w:date/w:dateFormat`). */
  dateFormat?: string;
  pr: string;
  endPr?: string;
}

/** Document-level settings that affect layout (`settings.xml`). */
export interface DocSettings {
  /** `w:defaultTabStop` (twips). */
  defaultTabStop: number;
  /** Different odd and even page headers/footers (`w:evenAndOddHeaders`). */
  evenAndOddHeaders: boolean;
  /** `w:mirrorMargins`. */
  mirrorMargins: boolean;
  /** `w:trackRevisions`. */
  trackRevisions: boolean;
  /** `w:clrSchemeMapping` (bg1→light1 …); absent uses Word's default mapping. */
  colorSchemeMapping?: Partial<Record<string, string>>;
  /** Compatibility mode (`w:compatSetting compatibilityMode`): 15 = Word 2013+. */
  compatibilityMode: number;
  /** `w:updateFields` — ask the user to update fields on open. */
  updateFields?: boolean;
}

export const DEFAULT_SETTINGS: Readonly<DocSettings> = {
  defaultTabStop: 720,
  evenAndOddHeaders: false,
  mirrorMargins: false,
  trackRevisions: false,
  compatibilityMode: 15,
};
