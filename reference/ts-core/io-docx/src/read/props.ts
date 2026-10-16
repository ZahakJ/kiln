/**
 * Property parsers: `w:rPr`, `w:pPr`, `w:tblPr`, `w:trPr`, `w:tcPr` and their building blocks (colours,
 * borders, shading, tabs). Every child the model has no field for is kept as raw XML (`xml`) so nothing
 * is lost on save.
 */
import type {
  Alignment,
  Border,
  CellBorders,
  CellMargins,
  Color,
  HighlightColor,
  Indent,
  LineRule,
  ParagraphBorders,
  RevisionInfo,
  RunFonts,
  Shading,
  Spacing,
  TableBorders,
  TableCellProps,
  TableLook,
  TableProps,
  TableRowProps,
  TabStop,
  ThemeColorName,
  ThemeFontRef,
  UnderlineStyle,
} from '@kiln-ref/doc-model';
import { child, children, type XmlElement } from '@kiln-ref/ooxml';
import type { DocxParagraphProps, DocxRunProps } from '../common/types.ts';
import { parseDecimal, parseHalfPoints, parseOnOff, parseTableWidth, parseTwips } from '../common/units.ts';
import { attr } from '../common/xml.ts';

/** Serialises an unmodelled element for the `xml` extras. Supplied by the reader (namespace scope). */
export type RawXml = (node: XmlElement) => string;

const val = (node: XmlElement | undefined): string | undefined => attr(node, 'w:val');

/** WordprocessingML toggle element: present = on unless `w:val` is false. */
function onOff(node: XmlElement | undefined): boolean | undefined {
  return node ? parseOnOff(val(node)) : undefined;
}

function hexByte(v: string | undefined): number | undefined {
  if (v === undefined || !/^[0-9a-fA-F]{1,2}$/.test(v)) return undefined;
  return Number.parseInt(v, 16);
}

const THEME_COLORS = new Set<string>([
  'dark1',
  'light1',
  'dark2',
  'light2',
  'accent1',
  'accent2',
  'accent3',
  'accent4',
  'accent5',
  'accent6',
  'hyperlink',
  'followedHyperlink',
  'none',
  'background1',
  'text1',
  'background2',
  'text2',
]);

function normaliseHex(v: string | undefined): string {
  if (!v) return 'auto';
  const s = v.trim();
  if (/^[0-9a-fA-F]{6}$/.test(s)) return s.toUpperCase();
  if (s.toLowerCase() === 'auto') return 'auto';
  if (/^[0-9a-fA-F]{8}$/.test(s)) return s.slice(2).toUpperCase();
  return 'auto';
}

/** `w:color` style attributes (`w:val`, `w:themeColor`, `w:themeTint`, `w:themeShade`). */
export function parseColor(
  node: XmlElement | undefined,
  valName = 'w:val',
  themePrefix = 'w:theme',
): Color | undefined {
  if (!node) return undefined;
  const v = attr(node, valName);
  const theme = attr(node, `${themePrefix}Color`) ?? attr(node, `${themePrefix}Fill`);
  if (v === undefined && theme === undefined) return undefined;
  const color: Color = { val: normaliseHex(v) };
  if (theme && THEME_COLORS.has(theme)) color.themeColor = theme as ThemeColorName;
  const tint = hexByte(attr(node, `${themePrefix}Tint`) ?? attr(node, `${themePrefix}FillTint`));
  const shade = hexByte(attr(node, `${themePrefix}Shade`) ?? attr(node, `${themePrefix}FillShade`));
  if (tint !== undefined) color.themeTint = tint;
  if (shade !== undefined) color.themeShade = shade;
  return color;
}

/** One border edge (`w:top`, `w:insideH`, …). */
export function parseBorder(node: XmlElement | undefined): Border | undefined {
  if (!node) return undefined;
  const style = val(node);
  if (!style) return undefined;
  const b: Border = { style };
  const size = parseDecimal(attr(node, 'w:sz'));
  if (size !== undefined) b.size = size;
  const space = parseDecimal(attr(node, 'w:space'));
  if (space !== undefined) b.space = space;
  const color = parseColor(node, 'w:color', 'w:theme');
  if (color) b.color = color;
  if (attr(node, 'w:shadow') !== undefined) b.shadow = parseOnOff(attr(node, 'w:shadow'));
  if (attr(node, 'w:frame') !== undefined) b.frame = parseOnOff(attr(node, 'w:frame'));
  return b;
}

function parseEdges<T extends object>(node: XmlElement | undefined, names: readonly string[]): T | undefined {
  if (!node) return undefined;
  const out: Record<string, Border> = {};
  for (const c of children(node)) {
    const local = c.name.slice(c.name.indexOf(':') + 1);
    // `start`/`end` are the bidi-aware names of left/right (Word 2010+, Strict).
    const key = local === 'start' ? 'left' : local === 'end' ? 'right' : local;
    if (!names.includes(key)) continue;
    const b = parseBorder(c);
    if (b) out[key] = b;
  }
  return Object.keys(out).length ? (out as T) : undefined;
}

export const parseParagraphBorders = (n: XmlElement | undefined) =>
  parseEdges<ParagraphBorders>(n, ['top', 'left', 'bottom', 'right', 'between', 'bar']);
export const parseTableBorders = (n: XmlElement | undefined) =>
  parseEdges<TableBorders>(n, ['top', 'left', 'bottom', 'right', 'insideH', 'insideV']);
export const parseCellBorders = (n: XmlElement | undefined) =>
  parseEdges<CellBorders>(n, ['top', 'left', 'bottom', 'right', 'insideH', 'insideV', 'tl2br', 'tr2bl']);

/** `w:shd`. */
export function parseShading(node: XmlElement | undefined): Shading | undefined {
  if (!node) return undefined;
  const s: Shading = { val: val(node) ?? 'clear' };
  const color = parseColor(node, 'w:color', 'w:theme');
  if (color) s.color = color;
  const fillVal = attr(node, 'w:fill');
  const fillTheme = attr(node, 'w:themeFill');
  if (fillVal !== undefined || fillTheme !== undefined) {
    const fill: Color = { val: normaliseHex(fillVal) };
    if (fillTheme && THEME_COLORS.has(fillTheme)) fill.themeColor = fillTheme as ThemeColorName;
    const tint = hexByte(attr(node, 'w:themeFillTint'));
    const shade = hexByte(attr(node, 'w:themeFillShade'));
    if (tint !== undefined) fill.themeTint = tint;
    if (shade !== undefined) fill.themeShade = shade;
    s.fill = fill;
  }
  // A theme pattern colour is given by w:themeColor on w:shd; parseColor above read it as `color`.
  return s;
}

/** `w:pPrChange`/`w:rPrChange`/`w:ins`/`w:del` metadata. */
export function parseRevision(node: XmlElement): RevisionInfo {
  return {
    id: attr(node, 'w:id') ?? '',
    author: attr(node, 'w:author') ?? '',
    date: attr(node, 'w:date') ?? '',
  };
}

// ------------------------------------------------------------------ run properties

const THEME_FONTS = new Set<string>([
  'majorAscii',
  'majorHAnsi',
  'majorEastAsia',
  'majorBidi',
  'minorAscii',
  'minorHAnsi',
  'minorEastAsia',
  'minorBidi',
]);

function parseFonts(node: XmlElement): RunFonts | undefined {
  const f: RunFonts = {};
  for (const slot of ['ascii', 'hAnsi', 'eastAsia', 'cs'] as const) {
    const name = attr(node, `w:${slot}`);
    if (name) f[slot] = name;
    const theme = attr(node, `w:${slot}Theme`);
    if (theme && THEME_FONTS.has(theme)) f[`${slot}Theme`] = theme as ThemeFontRef;
  }
  return Object.keys(f).length ? f : undefined;
}

const HIGHLIGHTS = new Set<string>([
  'black',
  'blue',
  'cyan',
  'green',
  'magenta',
  'red',
  'yellow',
  'white',
  'darkBlue',
  'darkCyan',
  'darkGreen',
  'darkMagenta',
  'darkRed',
  'darkYellow',
  'darkGray',
  'lightGray',
  'none',
]);

const TOGGLES: [string, keyof DocxRunProps][] = [
  ['w:b', 'bold'],
  ['w:bCs', 'boldCs'],
  ['w:i', 'italic'],
  ['w:iCs', 'italicCs'],
  ['w:caps', 'caps'],
  ['w:smallCaps', 'smallCaps'],
  ['w:strike', 'strike'],
  ['w:dstrike', 'dstrike'],
  ['w:vanish', 'hidden'],
  ['w:emboss', 'emboss'],
  ['w:imprint', 'imprint'],
  ['w:outline', 'outline'],
  ['w:shadow', 'shadow'],
  ['w:rtl', 'rtl'],
];

export interface ParsedRunProps {
  props: DocxRunProps;
  /** `w:rPrChange`: tracked formatting change with the properties before it. */
  change?: RevisionInfo & { before: DocxRunProps };
  /** Paragraph-mark revision (`w:ins`/`w:del`/`w:moveFrom`/`w:moveTo` inside a paragraph's `w:rPr`). */
  markRevision?: RevisionInfo & { kind: 'insertion' | 'deletion'; move?: boolean };
}

/** Parses `w:rPr` (or a style's/numbering level's `w:rPr`). */
export function parseRunProps(node: XmlElement | undefined, raw: RawXml): ParsedRunProps {
  const props: DocxRunProps = {};
  const out: ParsedRunProps = { props };
  if (!node) return out;
  const xml: string[] = [];
  for (const c of children(node)) {
    const toggle = TOGGLES.find(([n]) => n === c.name);
    if (toggle) {
      (props as Record<string, unknown>)[toggle[1]] = onOff(c);
      continue;
    }
    switch (c.name) {
      case 'w:rStyle': {
        const v = val(c);
        if (v) props.styleId = v;
        break;
      }
      case 'w:rFonts': {
        const f = parseFonts(c);
        if (f) props.fonts = f;
        // w:hint picks the font slot for characters shared by several scripts.
        const hint = attr(c, 'w:hint');
        if (hint) props.fontHint = hint;
        break;
      }
      case 'w:color': {
        const col = parseColor(c);
        if (col) props.color = col;
        break;
      }
      case 'w:spacing': {
        const n = parseTwips(val(c));
        if (n !== undefined) props.spacing = n;
        break;
      }
      case 'w:w': {
        const s = val(c);
        const n = s === undefined ? undefined : Number.parseFloat(s);
        if (n !== undefined && Number.isFinite(n)) props.scale = n;
        break;
      }
      case 'w:kern': {
        const n = parseHalfPoints(val(c));
        if (n !== undefined) props.kern = n;
        break;
      }
      case 'w:position': {
        const n = parseHalfPoints(val(c));
        if (n !== undefined) props.position = n;
        break;
      }
      case 'w:sz': {
        const n = parseHalfPoints(val(c));
        if (n !== undefined) props.size = n;
        break;
      }
      case 'w:szCs': {
        const n = parseHalfPoints(val(c));
        if (n !== undefined) props.sizeCs = n;
        break;
      }
      case 'w:highlight': {
        const v = val(c);
        if (v && HIGHLIGHTS.has(v)) props.highlight = v as HighlightColor;
        else xml.push(raw(c));
        break;
      }
      case 'w:u': {
        const style = (val(c) ?? 'single') as UnderlineStyle;
        const color = parseColor(c, 'w:color', 'w:theme');
        props.underline = color ? { style, color } : { style };
        break;
      }
      case 'w:shd': {
        const s = parseShading(c);
        if (s) props.shading = s;
        break;
      }
      case 'w:vertAlign': {
        const v = val(c);
        if (v === 'superscript' || v === 'subscript' || v === 'baseline') props.vertAlign = v;
        break;
      }
      case 'w:lang': {
        const lang: NonNullable<DocxRunProps['lang']> = {};
        const v = val(c);
        const ea = attr(c, 'w:eastAsia');
        const bidi = attr(c, 'w:bidi');
        if (v) lang.val = v;
        if (ea) lang.eastAsia = ea;
        if (bidi) lang.bidi = bidi;
        if (Object.keys(lang).length) props.lang = lang;
        break;
      }
      case 'w:rPrChange': {
        const before = parseRunProps(child(c, 'w:rPr'), raw).props;
        out.change = { ...parseRevision(c), before };
        break;
      }
      case 'w:ins':
      case 'w:moveTo':
        out.markRevision = {
          ...parseRevision(c),
          kind: 'insertion',
          ...(c.name === 'w:moveTo' ? { move: true } : {}),
        };
        break;
      case 'w:del':
      case 'w:moveFrom':
        out.markRevision = {
          ...parseRevision(c),
          kind: 'deletion',
          ...(c.name === 'w:moveFrom' ? { move: true } : {}),
        };
        break;
      default:
        // w:noProof, w:snapToGrid, w:webHidden, w:effect, w:bdr, w:fitText, w:cs, w:em,
        // w:eastAsianLayout, w:specVanish, w:oMath, w14:* text effects, …
        xml.push(raw(c));
    }
  }
  if (xml.length) props.xml = xml;
  return out;
}

// ------------------------------------------------------------------ paragraph properties

const ALIGNMENTS: Record<string, Alignment> = {
  left: 'left',
  start: 'left',
  center: 'center',
  right: 'right',
  end: 'right',
  both: 'both',
  justify: 'both',
  distribute: 'distribute',
  mediumKashida: 'mediumKashida',
  highKashida: 'highKashida',
  lowKashida: 'lowKashida',
  thaiDistribute: 'thaiDistribute',
};

function parseIndent(node: XmlElement): Indent | undefined {
  const ind: Indent = {};
  const left = parseTwips(attr(node, 'w:left') ?? attr(node, 'w:start'));
  const right = parseTwips(attr(node, 'w:right') ?? attr(node, 'w:end'));
  const first = parseTwips(attr(node, 'w:firstLine'));
  const hanging = parseTwips(attr(node, 'w:hanging'));
  if (left !== undefined) ind.left = left;
  if (right !== undefined) ind.right = right;
  // Hanging wins over firstLine when both are present (ECMA-376 §17.3.1.12).
  if (hanging !== undefined) ind.hanging = hanging;
  else if (first !== undefined) ind.firstLine = first;
  return Object.keys(ind).length ? ind : undefined;
}

function parseSpacing(node: XmlElement): Spacing | undefined {
  const sp: Spacing = {};
  const before = parseTwips(attr(node, 'w:before'));
  const after = parseTwips(attr(node, 'w:after'));
  const line = parseTwips(attr(node, 'w:line'));
  const rule = attr(node, 'w:lineRule');
  if (before !== undefined) sp.before = before;
  if (after !== undefined) sp.after = after;
  if (line !== undefined) sp.line = line;
  if (rule === 'auto' || rule === 'exact' || rule === 'atLeast') sp.lineRule = rule as LineRule;
  else if (line !== undefined) sp.lineRule = 'auto';
  if (attr(node, 'w:beforeAutospacing') !== undefined)
    sp.beforeAutospacing = parseOnOff(attr(node, 'w:beforeAutospacing'));
  if (attr(node, 'w:afterAutospacing') !== undefined)
    sp.afterAutospacing = parseOnOff(attr(node, 'w:afterAutospacing'));
  return Object.keys(sp).length ? sp : undefined;
}

function parseTabs(node: XmlElement): TabStop[] {
  const tabs: TabStop[] = [];
  for (const t of children(node, 'w:tab')) {
    const pos = parseTwips(attr(t, 'w:pos'));
    let align = val(t) ?? 'left';
    if (align === 'start') align = 'left';
    if (align === 'end') align = 'right';
    if (pos === undefined) continue;
    const stop: TabStop = { pos, align: align as TabStop['align'] };
    const leader = attr(t, 'w:leader');
    if (leader) stop.leader = leader as NonNullable<TabStop['leader']>;
    tabs.push(stop);
  }
  return tabs;
}

export interface ParsedParagraphProps {
  props: DocxParagraphProps;
  /** Paragraph-mark run properties (`w:pPr/w:rPr`). */
  mark?: ParsedRunProps;
  /** `w:sectPr` ending a section at this paragraph. */
  sectPr?: XmlElement;
  /** `w:pPrChange`. */
  change?: RevisionInfo & { before: DocxParagraphProps };
}

/** Parses `w:pPr`. */
export function parseParagraphProps(node: XmlElement | undefined, raw: RawXml): ParsedParagraphProps {
  const props: DocxParagraphProps = {};
  const out: ParsedParagraphProps = { props };
  if (!node) return out;
  const xml: string[] = [];
  for (const c of children(node)) {
    switch (c.name) {
      case 'w:pStyle': {
        const v = val(c);
        if (v) props.styleId = v;
        break;
      }
      case 'w:jc': {
        const a = ALIGNMENTS[val(c) ?? ''];
        if (a) props.alignment = a;
        else xml.push(raw(c));
        break;
      }
      case 'w:ind': {
        const ind = parseIndent(c);
        if (ind) props.indent = ind;
        break;
      }
      case 'w:spacing': {
        const sp = parseSpacing(c);
        if (sp) props.spacing = sp;
        break;
      }
      case 'w:numPr': {
        const numId = parseDecimal(val(child(c, 'w:numId')));
        const level = parseDecimal(val(child(c, 'w:ilvl')));
        if (numId !== undefined || level !== undefined) {
          props.numbering = { numId: numId ?? 0, level: Math.min(8, Math.max(0, level ?? 0)) };
        }
        if (child(c, 'w:ins')) xml.push(raw(c));
        break;
      }
      case 'w:keepNext':
        props.keepNext = onOff(c);
        break;
      case 'w:keepLines':
        props.keepLines = onOff(c);
        break;
      case 'w:pageBreakBefore':
        props.pageBreakBefore = onOff(c);
        break;
      case 'w:widowControl':
        props.widowControl = onOff(c);
        break;
      case 'w:contextualSpacing':
        props.contextualSpacing = onOff(c);
        break;
      case 'w:suppressAutoHyphens':
        props.suppressAutoHyphens = onOff(c);
        break;
      case 'w:suppressLineNumbers':
        props.suppressLineNumbers = onOff(c);
        break;
      case 'w:bidi':
        props.bidi = onOff(c);
        break;
      case 'w:outlineLvl': {
        const n = parseDecimal(val(c));
        if (n !== undefined) props.outlineLevel = n;
        break;
      }
      case 'w:tabs':
        props.tabs = parseTabs(c);
        break;
      case 'w:pBdr': {
        const b = parseParagraphBorders(c);
        if (b) props.borders = b;
        break;
      }
      case 'w:shd': {
        const s = parseShading(c);
        if (s) props.shading = s;
        break;
      }
      case 'w:textAlignment': {
        const v = val(c);
        if (v === 'top' || v === 'center' || v === 'baseline' || v === 'bottom' || v === 'auto')
          props.textAlignment = v;
        break;
      }
      case 'w:framePr': {
        const drop = attr(c, 'w:dropCap');
        const lines = parseDecimal(attr(c, 'w:lines'));
        if ((drop === 'drop' || drop === 'margin') && Object.keys(c.attrs).length <= 6) {
          props.dropCap = { type: drop, lines: lines ?? 3 };
        }
        // The full frame (position, wrap, size) is kept verbatim.
        xml.push(raw(c));
        break;
      }
      case 'w:rPr':
        out.mark = parseRunProps(c, raw);
        break;
      case 'w:sectPr':
        out.sectPr = c;
        break;
      case 'w:pPrChange': {
        const before = parseParagraphProps(child(c, 'w:pPr'), raw).props;
        out.change = { ...parseRevision(c), before };
        break;
      }
      default:
        // w:snapToGrid, w:adjustRightInd, w:autoSpaceDE/DN, w:kinsoku, w:wordWrap, w:overflowPunct,
        // w:topLinePunct, w:mirrorIndents, w:cnfStyle, w:divId, w:textDirection, w:textboxTightWrap, …
        xml.push(raw(c));
    }
  }
  if (xml.length) props.xml = xml;
  return out;
}

// ------------------------------------------------------------------ tables

function parseCellMargins(node: XmlElement | undefined): CellMargins | undefined {
  if (!node) return undefined;
  const m: CellMargins = {};
  for (const c of children(node)) {
    const local = c.name.slice(c.name.indexOf(':') + 1);
    const key = local === 'start' ? 'left' : local === 'end' ? 'right' : local;
    if (key !== 'top' && key !== 'left' && key !== 'bottom' && key !== 'right') continue;
    const w = parseTableWidth(attr(c, 'w:w'), attr(c, 'w:type'));
    // Cell margins are twips; a percentage margin is not meaningful in Word and is ignored.
    if (w && w.type !== 'pct') m[key] = w.value;
  }
  return Object.keys(m).length ? m : undefined;
}

/** `w:tblLook`: either the Word 2010 attributes or the Word 2007 hex bitmask. */
export function parseTableLook(node: XmlElement): TableLook {
  const hex = val(node);
  if (attr(node, 'w:firstRow') === undefined && hex && /^[0-9a-fA-F]+$/.test(hex)) {
    const n = Number.parseInt(hex, 16);
    return {
      firstRow: (n & 0x20) !== 0,
      lastRow: (n & 0x40) !== 0,
      firstColumn: (n & 0x80) !== 0,
      lastColumn: (n & 0x100) !== 0,
      noHBand: (n & 0x200) !== 0,
      noVBand: (n & 0x400) !== 0,
    };
  }
  const b = (name: string, fallback: boolean) => {
    const v = attr(node, `w:${name}`);
    return v === undefined ? fallback : parseOnOff(v);
  };
  return {
    firstRow: b('firstRow', false),
    lastRow: b('lastRow', false),
    firstColumn: b('firstColumn', false),
    lastColumn: b('lastColumn', false),
    noHBand: b('noHBand', false),
    noVBand: b('noVBand', false),
  };
}

export interface ParsedTableProps {
  props: TableProps;
  xml: string[];
}

export function parseTableProps(node: XmlElement | undefined, raw: RawXml): ParsedTableProps {
  const props: TableProps = {};
  const xml: string[] = [];
  if (!node) return { props, xml };
  for (const c of children(node)) {
    switch (c.name) {
      case 'w:tblStyle': {
        const v = val(c);
        if (v) props.styleId = v;
        break;
      }
      case 'w:tblW': {
        const w = parseTableWidth(attr(c, 'w:w'), attr(c, 'w:type'));
        if (w) props.width = w;
        break;
      }
      case 'w:jc': {
        const v = val(c);
        if (v === 'center') props.alignment = 'center';
        else if (v === 'right' || v === 'end') props.alignment = 'right';
        else if (v === 'left' || v === 'start') props.alignment = 'left';
        break;
      }
      case 'w:tblInd': {
        const w = parseTableWidth(attr(c, 'w:w'), attr(c, 'w:type'));
        if (w && w.type === 'dxa') props.indent = w.value;
        else xml.push(raw(c));
        break;
      }
      case 'w:tblBorders': {
        const b = parseTableBorders(c);
        if (b) props.borders = b;
        break;
      }
      case 'w:shd': {
        const s = parseShading(c);
        if (s) props.shading = s;
        break;
      }
      case 'w:tblCellMar': {
        const m = parseCellMargins(c);
        if (m) props.cellMargins = m;
        break;
      }
      case 'w:tblCellSpacing': {
        const w = parseTableWidth(attr(c, 'w:w'), attr(c, 'w:type'));
        if (w && w.type === 'dxa') props.cellSpacing = w.value;
        break;
      }
      case 'w:tblLayout':
        props.layout = attr(c, 'w:type') === 'fixed' ? 'fixed' : 'autofit';
        break;
      case 'w:tblLook':
        props.look = parseTableLook(c);
        break;
      case 'w:tblStyleRowBandSize': {
        const n = parseDecimal(val(c));
        if (n !== undefined) props.rowBandSize = n;
        break;
      }
      case 'w:tblStyleColBandSize': {
        const n = parseDecimal(val(c));
        if (n !== undefined) props.colBandSize = n;
        break;
      }
      case 'w:bidiVisual':
        props.bidiVisual = onOff(c);
        break;
      case 'w:tblCaption': {
        const v = val(c);
        if (v !== undefined) props.caption = v;
        break;
      }
      case 'w:tblDescription': {
        const v = val(c);
        if (v !== undefined) props.description = v;
        break;
      }
      default:
        // w:tblpPr (floating table position), w:tblOverlap, w:tblPrChange, …
        xml.push(raw(c));
    }
  }
  return { props, xml };
}

export interface ParsedRowProps {
  props: TableRowProps;
  gridBefore: number;
  gridAfter: number;
  revision?: RevisionInfo & { kind: 'insertion' | 'deletion' };
  xml: string[];
}

export function parseRowProps(node: XmlElement | undefined, raw: RawXml): ParsedRowProps {
  const props: TableRowProps = {};
  const out: ParsedRowProps = { props, gridBefore: 0, gridAfter: 0, xml: [] };
  if (!node) return out;
  for (const c of children(node)) {
    switch (c.name) {
      case 'w:trHeight': {
        const v = parseTwips(val(c));
        if (v !== undefined) {
          const rule = attr(c, 'w:hRule');
          props.height = { value: v, rule: rule === 'exact' || rule === 'auto' ? rule : 'atLeast' };
        }
        break;
      }
      case 'w:tblHeader':
        props.header = onOff(c);
        break;
      case 'w:cantSplit':
        props.cantSplit = onOff(c);
        break;
      case 'w:jc': {
        const v = val(c);
        if (v === 'center') props.alignment = 'center';
        else if (v === 'right' || v === 'end') props.alignment = 'right';
        else if (v === 'left' || v === 'start') props.alignment = 'left';
        break;
      }
      case 'w:gridBefore':
        out.gridBefore = Math.max(0, parseDecimal(val(c)) ?? 0);
        out.xml.push(raw(c));
        break;
      case 'w:gridAfter':
        out.gridAfter = Math.max(0, parseDecimal(val(c)) ?? 0);
        out.xml.push(raw(c));
        break;
      case 'w:ins':
        out.revision = { ...parseRevision(c), kind: 'insertion' };
        break;
      case 'w:del':
        out.revision = { ...parseRevision(c), kind: 'deletion' };
        break;
      default:
        // w:wBefore, w:wAfter, w:tblCellSpacing, w:hidden, w:cnfStyle, w:divId, w:trPrChange, …
        out.xml.push(raw(c));
    }
  }
  return out;
}

export interface ParsedCellProps {
  props: TableCellProps;
  gridSpan: number;
  /** `restart`, `continue` or undefined (no vertical merge). */
  vMerge?: 'restart' | 'continue';
  /** Legacy horizontal merge (`w:hMerge`). */
  hMerge?: 'restart' | 'continue';
  /** Cell-level revision marks (`w:cellIns`, `w:cellDel`, `w:cellMerge`). */
  revision?: XmlElement;
  xml: string[];
}

export function parseCellProps(node: XmlElement | undefined, raw: RawXml): ParsedCellProps {
  const props: TableCellProps = {};
  const out: ParsedCellProps = { props, gridSpan: 1, xml: [] };
  if (!node) return out;
  for (const c of children(node)) {
    switch (c.name) {
      case 'w:tcW': {
        const w = parseTableWidth(attr(c, 'w:w'), attr(c, 'w:type'));
        if (w) props.width = w;
        break;
      }
      case 'w:gridSpan':
        out.gridSpan = Math.max(1, parseDecimal(val(c)) ?? 1);
        break;
      case 'w:vMerge':
        out.vMerge = val(c) === 'restart' ? 'restart' : 'continue';
        break;
      case 'w:hMerge':
        out.hMerge = val(c) === 'restart' ? 'restart' : 'continue';
        break;
      case 'w:tcBorders': {
        const b = parseCellBorders(c);
        if (b) props.borders = b;
        break;
      }
      case 'w:shd': {
        const s = parseShading(c);
        if (s) props.shading = s;
        break;
      }
      case 'w:tcMar': {
        const m = parseCellMargins(c);
        if (m) props.margins = m;
        break;
      }
      case 'w:vAlign': {
        const v = val(c);
        if (v === 'top' || v === 'bottom') props.vAlign = v;
        else if (v === 'center' || v === 'both') props.vAlign = 'center';
        break;
      }
      case 'w:noWrap':
        props.noWrap = onOff(c);
        break;
      case 'w:textDirection': {
        const v = val(c);
        const map: Record<string, NonNullable<TableCellProps['textDirection']>> = {
          lrTb: 'lrTb',
          tbRl: 'tbRl',
          btLr: 'btLr',
          lrTbV: 'lrTbV',
          tbRlV: 'tbRlV',
          tbLrV: 'tbLrV',
          // Strict / 2010 names.
          lr: 'lrTb',
          tb: 'tbRl',
          rl: 'tbRl',
        };
        if (v && map[v]) props.textDirection = map[v];
        else out.xml.push(raw(c));
        break;
      }
      case 'w:tcFitText':
        props.fitText = onOff(c);
        break;
      case 'w:hideMark':
        props.hideMark = onOff(c);
        break;
      case 'w:cellIns':
      case 'w:cellDel':
      case 'w:cellMerge':
        out.revision = c;
        out.xml.push(raw(c));
        break;
      default:
        // w:cnfStyle, w:headers, w:tcPrChange, …
        out.xml.push(raw(c));
    }
  }
  return out;
}
