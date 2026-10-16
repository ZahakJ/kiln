/**
 * Property writers: `w:rPr`, `w:pPr`, `w:tblPr`, `w:trPr`, `w:tcPr`, `w:sectPr` and their building
 * blocks, each emitting children in the order of the ECMA-376 schema sequence (Word rejects files whose
 * property children are out of order). Raw XML kept by the reader for unmodelled children (`xml`) is
 * slotted into its schema position by element name.
 */
import type {
  Border,
  CellBorders,
  CellMargins,
  Color,
  Indent,
  ParagraphBorders,
  RevisionInfo,
  Shading,
  Spacing,
  TableBorders,
  TableCellProps,
  TableLook,
  TableProps,
  TableRowProps,
  TableWidth,
  TabStop,
} from '@kiln-ref/doc-model';
import type {
  DocxParagraphProps,
  DocxRunProps,
  DocxSectionProps,
  NoteProps,
  PageBorders,
} from '../common/types.ts';
import { type Attrs, attrs, fragmentName, hexByte, orderBySequence, tag, toggle, val } from './xml.ts';

// ------------------------------------------------------------------ building blocks

/** `w:val`/`w:themeColor`/`w:themeTint`/`w:themeShade` attributes of a colour. */
export function colorAttrs(
  c: Color | undefined,
  valName = 'w:val',
  prefix = 'w:theme',
): [string, string | undefined][] {
  if (!c) return [];
  return [
    [valName, c.val || 'auto'],
    [`${prefix}Color`, c.themeColor],
    [`${prefix}Tint`, c.themeTint === undefined ? undefined : hexByte(c.themeTint)],
    [`${prefix}Shade`, c.themeShade === undefined ? undefined : hexByte(c.themeShade)],
  ];
}

export function borderXml(name: string, b: Border | undefined): string {
  if (!b) return '';
  return tag(name, [
    ['w:val', b.style],
    ...colorAttrs(b.color, 'w:color'),
    ['w:sz', b.size],
    ['w:space', b.space],
    ['w:shadow', b.shadow],
    ['w:frame', b.frame],
  ]);
}

function edges(name: string, b: object | undefined, order: readonly string[]): string {
  if (!b) return '';
  const rec = b as Record<string, Border | undefined>;
  const inner = order.map((k) => borderXml(`w:${k}`, rec[k])).join('');
  return inner ? tag(name, [], inner) : '';
}

export const paragraphBordersXml = (b: ParagraphBorders | undefined) =>
  edges('w:pBdr', b, ['top', 'left', 'bottom', 'right', 'between', 'bar']);
export const tableBordersXml = (b: TableBorders | undefined) =>
  edges('w:tblBorders', b, ['top', 'left', 'bottom', 'right', 'insideH', 'insideV']);
export const cellBordersXml = (b: CellBorders | undefined) =>
  edges('w:tcBorders', b, ['top', 'left', 'bottom', 'right', 'insideH', 'insideV', 'tl2br', 'tr2bl']);

/**
 * `w:shd`. The reader takes a missing pattern-colour theme from `w:themeFill`, so a pattern colour that
 * only mirrors the fill's theme is written without its own theme attributes.
 */
export function shadingXml(s: Shading | undefined): string {
  if (!s) return '';
  const c = s.color;
  const f = s.fill;
  const mirrors =
    c !== undefined &&
    f !== undefined &&
    c.val === 'auto' &&
    c.themeColor !== undefined &&
    c.themeColor === f.themeColor &&
    c.themeTint === f.themeTint &&
    c.themeShade === f.themeShade;
  const list: Attrs = [
    ['w:val', s.val || 'clear'],
    ...(c ? (mirrors ? [['w:color', 'auto'] as const] : colorAttrs(c, 'w:color')) : []),
    ...(f
      ? ([
          ['w:fill', f.val || 'auto'],
          ['w:themeFill', f.themeColor],
          ['w:themeFillTint', f.themeTint === undefined ? undefined : hexByte(f.themeTint)],
          ['w:themeFillShade', f.themeShade === undefined ? undefined : hexByte(f.themeShade)],
        ] as const)
      : []),
  ];
  return tag('w:shd', list);
}

export function revisionAttrs(r: RevisionInfo): Attrs {
  return [
    ['w:id', r.id],
    ['w:author', r.author],
    ['w:date', r.date || undefined],
  ];
}

function widthXml(name: string, w: TableWidth | undefined): string {
  if (!w) return '';
  return tag(name, [
    ['w:w', w.value],
    ['w:type', w.type],
  ]);
}

function marginsXml(name: string, m: CellMargins | undefined): string {
  if (!m) return '';
  const inner = (['top', 'left', 'bottom', 'right'] as const)
    .map((k) =>
      m[k] === undefined
        ? ''
        : tag(`w:${k}`, [
            ['w:w', m[k]],
            ['w:type', 'dxa'],
          ]),
    )
    .join('');
  return inner ? tag(name, [], inner) : '';
}

/** Splits raw fragments into `[name, xml]` entries for sequence ordering. */
function rawEntries(xml: readonly string[] | undefined): [string, string][] {
  return (xml ?? []).map((x) => [fragmentName(x), x]);
}

const hasRaw = (xml: readonly string[] | undefined, name: string) =>
  (xml ?? []).some((x) => fragmentName(x) === name);

// ------------------------------------------------------------------ run properties

/** EG_RPrBase (ECMA-376 Part 1 §17.3.2.28) followed by `w:rPrChange`. */
export const RPR_SEQUENCE = [
  'w:ins',
  'w:del',
  'w:moveFrom',
  'w:moveTo',
  'w:rStyle',
  'w:rFonts',
  'w:b',
  'w:bCs',
  'w:i',
  'w:iCs',
  'w:caps',
  'w:smallCaps',
  'w:strike',
  'w:dstrike',
  'w:outline',
  'w:shadow',
  'w:emboss',
  'w:imprint',
  'w:noProof',
  'w:snapToGrid',
  'w:vanish',
  'w:webHidden',
  'w:color',
  'w:spacing',
  'w:w',
  'w:kern',
  'w:position',
  'w:sz',
  'w:szCs',
  'w:highlight',
  'w:u',
  'w:effect',
  'w:bdr',
  'w:shd',
  'w:fitText',
  'w:vertAlign',
  'w:rtl',
  'w:cs',
  'w:em',
  'w:lang',
  'w:eastAsianLayout',
  'w:specVanish',
  'w:oMath',
  // Word 2010+ text effects (`w14:*`) follow the base properties.
  '#ext',
  'w:rPrChange',
];

const RUN_TOGGLES: [string, keyof DocxRunProps][] = [
  ['w:b', 'bold'],
  ['w:bCs', 'boldCs'],
  ['w:i', 'italic'],
  ['w:iCs', 'italicCs'],
  ['w:caps', 'caps'],
  ['w:smallCaps', 'smallCaps'],
  ['w:strike', 'strike'],
  ['w:dstrike', 'dstrike'],
  ['w:outline', 'outline'],
  ['w:shadow', 'shadow'],
  ['w:emboss', 'emboss'],
  ['w:imprint', 'imprint'],
  ['w:vanish', 'hidden'],
  ['w:rtl', 'rtl'],
];

export interface RunPropsExtras {
  /** `w:rPrChange` (tracked formatting change) with the properties before it. */
  change?: RevisionInfo & { before: DocxRunProps };
  /** Paragraph-mark revision (`w:pPr/w:rPr/w:ins|w:del`). */
  markRevision?: RevisionInfo & { kind: 'insertion' | 'deletion' };
}

/** The children of `w:rPr` (without the wrapper). */
export function runPropsInner(p: DocxRunProps, extras: RunPropsExtras = {}): string {
  const e: [string, string][] = [];
  if (extras.markRevision) {
    const n = extras.markRevision.kind === 'insertion' ? 'w:ins' : 'w:del';
    e.push([n, tag(n, revisionAttrs(extras.markRevision))]);
  }
  if (p.styleId) e.push(['w:rStyle', val('w:rStyle', p.styleId)]);
  if (p.fonts || p.fontHint) {
    const f = p.fonts ?? {};
    e.push([
      'w:rFonts',
      tag('w:rFonts', [
        ['w:hint', p.fontHint],
        ['w:ascii', f.ascii],
        ['w:hAnsi', f.hAnsi],
        ['w:eastAsia', f.eastAsia],
        ['w:cs', f.cs],
        ['w:asciiTheme', f.asciiTheme],
        ['w:hAnsiTheme', f.hAnsiTheme],
        ['w:eastAsiaTheme', f.eastAsiaTheme],
        ['w:cstheme', f.csTheme],
      ]),
    ]);
  }
  for (const [name, key] of RUN_TOGGLES) {
    const v = p[key] as boolean | undefined;
    if (v !== undefined) e.push([name, toggle(name, v)]);
  }
  if (p.color) e.push(['w:color', tag('w:color', colorAttrs(p.color))]);
  if (p.spacing !== undefined) e.push(['w:spacing', val('w:spacing', p.spacing)]);
  if (p.scale !== undefined) e.push(['w:w', val('w:w', p.scale)]);
  if (p.kern !== undefined) e.push(['w:kern', val('w:kern', p.kern)]);
  if (p.position !== undefined) e.push(['w:position', val('w:position', p.position)]);
  if (p.size !== undefined) e.push(['w:sz', val('w:sz', p.size)]);
  if (p.sizeCs !== undefined) e.push(['w:szCs', val('w:szCs', p.sizeCs)]);
  if (p.highlight) e.push(['w:highlight', val('w:highlight', p.highlight)]);
  if (p.underline) {
    e.push(['w:u', tag('w:u', [['w:val', p.underline.style], ...colorAttrs(p.underline.color, 'w:color')])]);
  }
  if (p.shading) e.push(['w:shd', shadingXml(p.shading)]);
  if (p.vertAlign) e.push(['w:vertAlign', val('w:vertAlign', p.vertAlign)]);
  if (p.lang) {
    e.push([
      'w:lang',
      tag('w:lang', [
        ['w:val', p.lang.val],
        ['w:eastAsia', p.lang.eastAsia],
        ['w:bidi', p.lang.bidi],
      ]),
    ]);
  }
  for (const [name, xml] of rawEntries(p.xml)) e.push([name.startsWith('w:') ? name : '#ext', xml]);
  if (extras.change) {
    const before = runPropsInner(extras.change.before);
    e.push([
      'w:rPrChange',
      tag('w:rPrChange', revisionAttrs(extras.change), tag('w:rPr', [], before) || '<w:rPr/>'),
    ]);
  }
  return orderBySequence(e, RPR_SEQUENCE, '#ext');
}

/** `w:rPr` with its content, or `''` when empty. */
export function runPropsXml(p: DocxRunProps, extras: RunPropsExtras = {}): string {
  const inner = runPropsInner(p, extras);
  return inner ? `<w:rPr>${inner}</w:rPr>` : '';
}

// ------------------------------------------------------------------ paragraph properties

/** CT_PPr (ECMA-376 Part 1 §17.3.1.26). */
export const PPR_SEQUENCE = [
  'w:pStyle',
  'w:keepNext',
  'w:keepLines',
  'w:pageBreakBefore',
  'w:framePr',
  'w:widowControl',
  'w:numPr',
  'w:suppressLineNumbers',
  'w:pBdr',
  'w:shd',
  'w:tabs',
  'w:suppressAutoHyphens',
  'w:kinsoku',
  'w:wordWrap',
  'w:overflowPunct',
  'w:topLinePunct',
  'w:autoSpaceDE',
  'w:autoSpaceDN',
  'w:bidi',
  'w:adjustRightInd',
  'w:snapToGrid',
  'w:spacing',
  'w:ind',
  'w:contextualSpacing',
  'w:mirrorIndents',
  'w:suppressOverlap',
  'w:jc',
  'w:textDirection',
  'w:textAlignment',
  'w:textboxTightWrap',
  'w:outlineLvl',
  'w:divId',
  'w:cnfStyle',
  '#unknown',
  'w:rPr',
  'w:sectPr',
  'w:pPrChange',
];

function indentXml(ind: Indent | undefined): string {
  if (!ind) return '';
  return tag('w:ind', [
    ['w:left', ind.left],
    ['w:right', ind.right],
    ['w:hanging', ind.hanging],
    ['w:firstLine', ind.hanging === undefined ? ind.firstLine : undefined],
  ]);
}

function spacingXml(sp: Spacing | undefined): string {
  if (!sp) return '';
  return tag('w:spacing', [
    ['w:before', sp.before],
    ['w:beforeAutospacing', sp.beforeAutospacing],
    ['w:after', sp.after],
    ['w:afterAutospacing', sp.afterAutospacing],
    ['w:line', sp.line],
    ['w:lineRule', sp.line !== undefined || sp.lineRule !== 'auto' ? sp.lineRule : undefined],
  ]);
}

function tabsXml(tabs: readonly TabStop[] | undefined): string {
  if (!tabs?.length) return '';
  return tag(
    'w:tabs',
    [],
    tabs
      .map((t) =>
        tag('w:tab', [
          ['w:val', t.align],
          ['w:leader', t.leader],
          ['w:pos', t.pos],
        ]),
      )
      .join(''),
  );
}

export interface ParagraphPropsExtras {
  /** Paragraph-mark run properties (`w:pPr/w:rPr`). */
  markProps?: DocxRunProps | null;
  markRevision?: RevisionInfo & { kind: 'insertion' | 'deletion' };
  markPrChange?: RevisionInfo & { before: DocxRunProps };
  /** A complete `w:sectPr` ending a section at this paragraph. */
  sectPr?: string;
  pPrChange?: RevisionInfo & { before: DocxParagraphProps };
}

/** The children of `w:pPr` (without the wrapper). */
export function paragraphPropsInner(p: DocxParagraphProps, extras: ParagraphPropsExtras = {}): string {
  const e: [string, string][] = [];
  const raw = p.xml;
  if (p.styleId) e.push(['w:pStyle', val('w:pStyle', p.styleId)]);
  if (p.keepNext !== undefined) e.push(['w:keepNext', toggle('w:keepNext', p.keepNext)]);
  if (p.keepLines !== undefined) e.push(['w:keepLines', toggle('w:keepLines', p.keepLines)]);
  if (p.pageBreakBefore !== undefined)
    e.push(['w:pageBreakBefore', toggle('w:pageBreakBefore', p.pageBreakBefore)]);
  if (p.dropCap && !hasRaw(raw, 'w:framePr')) {
    e.push([
      'w:framePr',
      tag('w:framePr', [
        ['w:dropCap', p.dropCap.type],
        ['w:lines', p.dropCap.lines],
        ['w:wrap', 'around'],
        ['w:vAnchor', 'text'],
        ['w:hAnchor', 'text'],
      ]),
    ]);
  }
  if (p.widowControl !== undefined) e.push(['w:widowControl', toggle('w:widowControl', p.widowControl)]);
  if (p.numbering && !hasRaw(raw, 'w:numPr')) {
    e.push([
      'w:numPr',
      `<w:numPr>${val('w:ilvl', p.numbering.level)}${val('w:numId', p.numbering.numId)}</w:numPr>`,
    ]);
  }
  if (p.suppressLineNumbers !== undefined)
    e.push(['w:suppressLineNumbers', toggle('w:suppressLineNumbers', p.suppressLineNumbers)]);
  if (p.borders) e.push(['w:pBdr', paragraphBordersXml(p.borders)]);
  if (p.shading) e.push(['w:shd', shadingXml(p.shading)]);
  if (p.tabs?.length) e.push(['w:tabs', tabsXml(p.tabs)]);
  if (p.suppressAutoHyphens !== undefined)
    e.push(['w:suppressAutoHyphens', toggle('w:suppressAutoHyphens', p.suppressAutoHyphens)]);
  if (p.bidi !== undefined) e.push(['w:bidi', toggle('w:bidi', p.bidi)]);
  if (p.spacing) e.push(['w:spacing', spacingXml(p.spacing)]);
  if (p.indent) e.push(['w:ind', indentXml(p.indent)]);
  if (p.contextualSpacing !== undefined)
    e.push(['w:contextualSpacing', toggle('w:contextualSpacing', p.contextualSpacing)]);
  if (p.alignment) e.push(['w:jc', val('w:jc', p.alignment)]);
  if (p.textAlignment) e.push(['w:textAlignment', val('w:textAlignment', p.textAlignment)]);
  if (p.outlineLevel !== undefined) e.push(['w:outlineLvl', val('w:outlineLvl', p.outlineLevel)]);
  for (const [name, xml] of rawEntries(raw)) e.push([name, xml]);
  if (extras.markProps || extras.markRevision || extras.markPrChange) {
    const inner = runPropsInner(extras.markProps ?? {}, {
      ...(extras.markRevision ? { markRevision: extras.markRevision } : {}),
      ...(extras.markPrChange ? { change: extras.markPrChange } : {}),
    });
    if (inner) e.push(['w:rPr', `<w:rPr>${inner}</w:rPr>`]);
  }
  if (extras.sectPr) e.push(['w:sectPr', extras.sectPr]);
  if (extras.pPrChange) {
    const before = paragraphPropsInner(extras.pPrChange.before);
    e.push([
      'w:pPrChange',
      tag('w:pPrChange', revisionAttrs(extras.pPrChange), before ? `<w:pPr>${before}</w:pPr>` : '<w:pPr/>'),
    ]);
  }
  return orderBySequence(e, PPR_SEQUENCE, '#unknown');
}

export function paragraphPropsXml(p: DocxParagraphProps, extras: ParagraphPropsExtras = {}): string {
  const inner = paragraphPropsInner(p, extras);
  return inner ? `<w:pPr>${inner}</w:pPr>` : '';
}

// ------------------------------------------------------------------ tables

/** CT_TblPr (ECMA-376 Part 1 §17.4.60). */
export const TBLPR_SEQUENCE = [
  'w:tblStyle',
  'w:tblpPr',
  'w:tblOverlap',
  'w:bidiVisual',
  'w:tblStyleRowBandSize',
  'w:tblStyleColBandSize',
  'w:tblW',
  'w:jc',
  'w:tblCellSpacing',
  'w:tblInd',
  'w:tblBorders',
  'w:shd',
  'w:tblLayout',
  'w:tblCellMar',
  'w:tblLook',
  'w:tblCaption',
  'w:tblDescription',
  '#unknown',
  'w:tblPrChange',
];

export function tableLookXml(look: TableLook): string {
  let mask = 0;
  if (look.firstRow) mask |= 0x20;
  if (look.lastRow) mask |= 0x40;
  if (look.firstColumn) mask |= 0x80;
  if (look.lastColumn) mask |= 0x100;
  if (look.noHBand) mask |= 0x200;
  if (look.noVBand) mask |= 0x400;
  return tag('w:tblLook', [
    ['w:val', mask.toString(16).toUpperCase().padStart(4, '0')],
    ['w:firstRow', look.firstRow],
    ['w:lastRow', look.lastRow],
    ['w:firstColumn', look.firstColumn],
    ['w:lastColumn', look.lastColumn],
    ['w:noHBand', look.noHBand],
    ['w:noVBand', look.noVBand],
  ]);
}

/** The children of `w:tblPr`. `xml` holds raw unmodelled children. */
export function tablePropsInner(p: TableProps, xml?: readonly string[]): string {
  const e: [string, string][] = [];
  if (p.styleId) e.push(['w:tblStyle', val('w:tblStyle', p.styleId)]);
  if (p.bidiVisual !== undefined) e.push(['w:bidiVisual', toggle('w:bidiVisual', p.bidiVisual)]);
  if (p.rowBandSize !== undefined)
    e.push(['w:tblStyleRowBandSize', val('w:tblStyleRowBandSize', p.rowBandSize)]);
  if (p.colBandSize !== undefined)
    e.push(['w:tblStyleColBandSize', val('w:tblStyleColBandSize', p.colBandSize)]);
  if (p.width) e.push(['w:tblW', widthXml('w:tblW', p.width)]);
  if (p.alignment) e.push(['w:jc', val('w:jc', p.alignment)]);
  if (p.cellSpacing !== undefined)
    e.push(['w:tblCellSpacing', widthXml('w:tblCellSpacing', { type: 'dxa', value: p.cellSpacing })]);
  if (p.indent !== undefined && !hasRaw(xml, 'w:tblInd'))
    e.push(['w:tblInd', widthXml('w:tblInd', { type: 'dxa', value: p.indent })]);
  if (p.borders) e.push(['w:tblBorders', tableBordersXml(p.borders)]);
  if (p.shading) e.push(['w:shd', shadingXml(p.shading)]);
  if (p.layout) e.push(['w:tblLayout', tag('w:tblLayout', [['w:type', p.layout]])]);
  if (p.cellMargins) e.push(['w:tblCellMar', marginsXml('w:tblCellMar', p.cellMargins)]);
  if (p.look) e.push(['w:tblLook', tableLookXml(p.look)]);
  if (p.caption !== undefined) e.push(['w:tblCaption', val('w:tblCaption', p.caption)]);
  if (p.description !== undefined) e.push(['w:tblDescription', val('w:tblDescription', p.description)]);
  for (const [name, x] of rawEntries(xml)) e.push([name, x]);
  return orderBySequence(e, TBLPR_SEQUENCE, '#unknown');
}

/** CT_TrPr: Word's order of the row property choice, then the revision elements. */
export const TRPR_SEQUENCE = [
  'w:cnfStyle',
  'w:divId',
  'w:gridBefore',
  'w:gridAfter',
  'w:wBefore',
  'w:wAfter',
  'w:cantSplit',
  'w:trHeight',
  'w:tblHeader',
  'w:tblCellSpacing',
  'w:jc',
  'w:hidden',
  '#unknown',
  'w:ins',
  'w:del',
  'w:trPrChange',
];

export function rowPropsInner(
  p: TableRowProps,
  xml?: readonly string[],
  revision?: RevisionInfo & { kind: 'insertion' | 'deletion' },
): string {
  const e: [string, string][] = [];
  if (p.cantSplit !== undefined) e.push(['w:cantSplit', toggle('w:cantSplit', p.cantSplit)]);
  if (p.height) {
    e.push([
      'w:trHeight',
      tag('w:trHeight', [
        ['w:val', p.height.value],
        ['w:hRule', p.height.rule === 'atLeast' ? undefined : p.height.rule],
      ]),
    ]);
  }
  if (p.header !== undefined) e.push(['w:tblHeader', toggle('w:tblHeader', p.header)]);
  if (p.alignment) e.push(['w:jc', val('w:jc', p.alignment)]);
  for (const [name, x] of rawEntries(xml)) e.push([name, x]);
  if (revision) {
    const n = revision.kind === 'insertion' ? 'w:ins' : 'w:del';
    e.push([n, tag(n, revisionAttrs(revision))]);
  }
  return orderBySequence(e, TRPR_SEQUENCE, '#unknown');
}

/** CT_TcPr (ECMA-376 Part 1 §17.4.70). */
export const TCPR_SEQUENCE = [
  'w:cnfStyle',
  'w:tcW',
  'w:gridSpan',
  'w:hMerge',
  'w:vMerge',
  'w:tcBorders',
  'w:shd',
  'w:noWrap',
  'w:tcMar',
  'w:textDirection',
  'w:tcFitText',
  'w:vAlign',
  'w:hideMark',
  'w:headers',
  '#unknown',
  'w:cellIns',
  'w:cellDel',
  'w:cellMerge',
  'w:tcPrChange',
];

export function cellPropsInner(
  p: TableCellProps,
  span: { gridSpan?: number; vMerge?: 'restart' | 'continue' } = {},
  xml?: readonly string[],
): string {
  const e: [string, string][] = [];
  if (p.width) e.push(['w:tcW', widthXml('w:tcW', p.width)]);
  if (span.gridSpan && span.gridSpan > 1) e.push(['w:gridSpan', val('w:gridSpan', span.gridSpan)]);
  if (span.vMerge === 'restart') e.push(['w:vMerge', val('w:vMerge', 'restart')]);
  else if (span.vMerge === 'continue') e.push(['w:vMerge', '<w:vMerge/>']);
  if (p.borders) e.push(['w:tcBorders', cellBordersXml(p.borders)]);
  if (p.shading) e.push(['w:shd', shadingXml(p.shading)]);
  if (p.noWrap !== undefined) e.push(['w:noWrap', toggle('w:noWrap', p.noWrap)]);
  if (p.margins) e.push(['w:tcMar', marginsXml('w:tcMar', p.margins)]);
  if (p.textDirection) e.push(['w:textDirection', val('w:textDirection', p.textDirection)]);
  if (p.fitText !== undefined) e.push(['w:tcFitText', toggle('w:tcFitText', p.fitText)]);
  if (p.vAlign) e.push(['w:vAlign', val('w:vAlign', p.vAlign)]);
  if (p.hideMark !== undefined) e.push(['w:hideMark', toggle('w:hideMark', p.hideMark)]);
  for (const [name, x] of rawEntries(xml)) e.push([name, x]);
  return orderBySequence(e, TCPR_SEQUENCE, '#unknown');
}

// ------------------------------------------------------------------ sections

/** CT_SectPr (ECMA-376 Part 1 §17.6.17), after the header/footer references. */
export const SECTPR_SEQUENCE = [
  'w:headerReference',
  'w:footerReference',
  'w:footnotePr',
  'w:endnotePr',
  'w:type',
  'w:pgSz',
  'w:pgMar',
  'w:paperSrc',
  'w:pgBorders',
  'w:lnNumType',
  'w:pgNumType',
  'w:cols',
  'w:formProt',
  'w:vAlign',
  'w:noEndnote',
  'w:titlePg',
  'w:textDirection',
  'w:bidi',
  'w:rtlGutter',
  'w:docGrid',
  'w:printerSettings',
  '#unknown',
  'w:sectPrChange',
];

/** `w:footnotePr`/`w:endnotePr`; `ids` lists the separator notes Word references from settings. */
export function notePropsXml(
  name: string,
  p: NoteProps | undefined,
  ids: { name: string; ids: readonly string[] } | undefined = undefined,
): string {
  if (!p && !ids?.ids.length) return '';
  const inner =
    val('w:pos', p?.position) +
    val('w:numFmt', p?.numFmt) +
    val('w:numStart', p?.numStart) +
    val('w:numRestart', p?.numRestart) +
    (ids?.ids ?? []).map((id) => tag(ids?.name ?? '', [['w:id', id]])).join('');
  return inner ? tag(name, [], inner) : '';
}

function pageBordersXml(pb: PageBorders | undefined): string {
  if (!pb) return '';
  return tag(
    'w:pgBorders',
    [
      ['w:zOrder', pb.zOrder],
      ['w:display', pb.display],
      ['w:offsetFrom', pb.offsetFrom],
    ],
    borderXml('w:top', pb.top) +
      borderXml('w:left', pb.left) +
      borderXml('w:bottom', pb.bottom) +
      borderXml('w:right', pb.right),
  );
}

/**
 * `w:sectPr`. `refs` are the header/footer references (already resolved to relationship ids), in the
 * order Word writes them.
 */
export function sectionPropsXml(
  p: DocxSectionProps,
  refs: { kind: 'header' | 'footer'; type: string; id: string }[],
): string {
  const e: [string, string][] = [];
  for (const r of refs) {
    const n = r.kind === 'header' ? 'w:headerReference' : 'w:footerReference';
    e.push([
      n,
      tag(n, [
        ['w:type', r.type],
        ['r:id', r.id],
      ]),
    ]);
  }
  if (p.footnotePr) e.push(['w:footnotePr', notePropsXml('w:footnotePr', p.footnotePr)]);
  if (p.endnotePr) e.push(['w:endnotePr', notePropsXml('w:endnotePr', p.endnotePr)]);
  if (p.type && p.type !== 'nextPage') e.push(['w:type', val('w:type', p.type)]);
  const sz = p.pageSize;
  if (sz) {
    const inferredLandscape = sz.width > sz.height;
    const orient = sz.orientation === 'landscape' ? 'landscape' : inferredLandscape ? 'portrait' : undefined;
    e.push([
      'w:pgSz',
      tag('w:pgSz', [
        ['w:w', sz.width],
        ['w:h', sz.height],
        ['w:orient', orient],
        ['w:code', sz.code],
      ]),
    ]);
  }
  const m = p.margins;
  if (m) {
    e.push([
      'w:pgMar',
      tag('w:pgMar', [
        ['w:top', m.top],
        ['w:right', m.right],
        ['w:bottom', m.bottom],
        ['w:left', m.left],
        ['w:header', m.header],
        ['w:footer', m.footer],
        ['w:gutter', m.gutter],
      ]),
    ]);
  }
  if (p.pageBorders) e.push(['w:pgBorders', pageBordersXml(p.pageBorders)]);
  if (p.lineNumbering) {
    const ln = p.lineNumbering;
    e.push([
      'w:lnNumType',
      tag('w:lnNumType', [
        ['w:countBy', ln.countBy],
        ['w:start', ln.start],
        ['w:distance', ln.distance],
        ['w:restart', ln.restart === 'newPage' ? undefined : ln.restart],
      ]),
    ]);
  }
  if (p.pageNumbering && !hasRaw(p.xml, 'w:pgNumType')) {
    e.push([
      'w:pgNumType',
      tag('w:pgNumType', [
        ['w:fmt', p.pageNumbering.format],
        ['w:start', p.pageNumbering.start],
      ]),
    ]);
  }
  const c = p.columns;
  if (c) {
    const cols = c.cols ?? [];
    const inferredEqual = cols.length === 0;
    e.push([
      'w:cols',
      tag(
        'w:cols',
        [
          ['w:num', c.count !== 1 || cols.length ? c.count : undefined],
          ['w:sep', c.separator ? true : undefined],
          ['w:space', c.space],
          ['w:equalWidth', c.equalWidth !== inferredEqual ? c.equalWidth : undefined],
        ],
        cols
          .map((col) =>
            tag('w:col', [
              ['w:w', col.width],
              ['w:space', col.space],
            ]),
          )
          .join(''),
      ),
    ]);
  }
  if (p.formProt !== undefined) e.push(['w:formProt', toggle('w:formProt', p.formProt)]);
  if (p.vAlign) e.push(['w:vAlign', val('w:vAlign', p.vAlign)]);
  if (p.noEndnote !== undefined) e.push(['w:noEndnote', toggle('w:noEndnote', p.noEndnote)]);
  if (p.titlePage) e.push(['w:titlePg', '<w:titlePg/>']);
  if (p.textDirection) e.push(['w:textDirection', val('w:textDirection', p.textDirection)]);
  if (p.bidi !== undefined) e.push(['w:bidi', toggle('w:bidi', p.bidi)]);
  if (p.rtlGutter !== undefined) e.push(['w:rtlGutter', toggle('w:rtlGutter', p.rtlGutter)]);
  if (p.docGrid) {
    e.push([
      'w:docGrid',
      tag('w:docGrid', [
        ['w:type', p.docGrid.type],
        ['w:linePitch', p.docGrid.linePitch],
        ['w:charSpace', p.docGrid.charSpace],
      ]),
    ]);
  }
  for (const [name, x] of rawEntries(p.xml)) e.push([name, x]);
  if (p.sectPrChange) e.push(['w:sectPrChange', p.sectPrChange]);
  return `<w:sectPr>${orderBySequence(e, SECTPR_SEQUENCE, '#unknown')}</w:sectPr>`;
}

export { attrs };
