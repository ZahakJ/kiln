/**
 * Effective formatting resolution — Word's property inheritance (ECMA-376 §17.7.2), in this order:
 *
 *   1. document defaults (`w:docDefaults`)
 *   2. table style (whole table, then conditional regions: bands, first/last column, header row, corners)
 *   3. paragraph style chain (`basedOn` root → leaf; `Normal` when none is set)
 *   4. numbering level `w:pPr` (paragraphs) / `w:rPr` (list labels only)
 *   5. character style chain (runs)
 *   6. direct formatting (paragraph attributes / run marks; the paragraph mark's `w:rPr` for labels)
 *
 * Toggle properties (bold, italic, caps, …, §17.7.3) toggle when a *character style* turns them on
 * over a paragraph that already has them (Emphasis inside the italic Quote style renders upright, as in
 * Word). Within one style's `basedOn` chain the nearest value wins, and document defaults, table
 * style, paragraph style, numbering and direct formatting set values absolutely.
 */

import type { Theme } from '@officina/ooxml';
import type { Mark, Node as PMNode } from '@tiptap/pm/model';
import { TableMap } from '@tiptap/pm/tables';
import type { Numbering } from '../numbering/numbering.ts';
import { PARAGRAPH_PROP_KEYS } from '../schema.ts';
import { type ResolvedFonts, resolveDocColor, resolveRunFonts } from '../theme.ts';
import {
  type Alignment,
  DEFAULT_TABLE_LOOK,
  type DocSettings,
  type LineRule,
  type NumberingRef,
  type ParagraphProps,
  type RunProps,
  type TableCellProps,
  type TableConditionType,
  type TableLook,
  type TableProps,
  type TabStop,
  TOGGLE_RUN_PROPS,
} from '../types.ts';
import { mergeCellProps, mergeParagraphProps, mergeRunProps, mergeTableProps } from './merge.ts';
import type { Style, StyleSheet } from './stylesheet.ts';

/** Everything resolution needs from the document besides the node tree. */
export interface DocContext {
  styles: StyleSheet;
  numbering?: Numbering;
  theme?: Theme;
  settings?: Partial<DocSettings>;
}

/** Where a paragraph sits inside a table, for table-style conditional formatting. */
export interface TableCellContext {
  tableStyleId?: string;
  /** Applicable conditional regions in application order (see `tableConditions`). */
  conditions: TableConditionType[];
}

// ------------------------------------------------------------------ node ↔ props

/** Direct paragraph properties stored on a paragraph node. */
export function paragraphPropsFromNode(node: PMNode): ParagraphProps {
  const out: ParagraphProps = {};
  for (const key of PARAGRAPH_PROP_KEYS) {
    const v = node.attrs[key];
    if (v !== null && v !== undefined) (out as Record<string, unknown>)[key] = v;
  }
  return out;
}

/** Direct run properties expressed by a run's marks. */
export function runPropsFromMarks(marks: readonly Mark[]): RunProps {
  const rp: RunProps = {};
  for (const m of marks) {
    const a = m.attrs;
    switch (m.type.name) {
      case 'runStyle':
        if (a.styleId) rp.styleId = a.styleId;
        break;
      case 'bold':
        rp.bold = a.value !== false;
        break;
      case 'italic':
        rp.italic = a.value !== false;
        break;
      case 'strike':
        rp.strike = a.value !== false;
        break;
      case 'dstrike':
        rp.dstrike = a.value !== false;
        break;
      case 'caps':
        rp.caps = a.value !== false;
        break;
      case 'smallCaps':
        rp.smallCaps = a.value !== false;
        break;
      case 'hidden':
        rp.hidden = a.value !== false;
        break;
      case 'underline':
        rp.underline = a.color ? { style: a.style, color: a.color } : { style: a.style };
        break;
      case 'vertAlign':
        rp.vertAlign = a.value;
        break;
      case 'color':
        rp.color = a.color;
        break;
      case 'highlight':
        rp.highlight = a.color;
        break;
      case 'runShading':
        if (a.shading) rp.shading = a.shading;
        break;
      case 'fontFamily':
        rp.fonts = { ...a.fonts };
        break;
      case 'fontSize':
        rp.size = a.size;
        if (a.sizeCs !== null) rp.sizeCs = a.sizeCs;
        break;
      case 'charSpacing':
        rp.spacing = a.value;
        break;
      case 'runProps':
        Object.assign(rp, a.props);
        break;
      default:
        break;
    }
  }
  return rp;
}

// ------------------------------------------------------------------ tables

/**
 * The conditional regions of a table style that apply to a cell, in Word's application order:
 * whole table, vertical bands, horizontal bands, first/last column, first/last row, corner cells.
 */
export function tableConditions(opts: {
  row: number;
  col: number;
  rowCount: number;
  colCount: number;
  look?: Partial<TableLook>;
  rowBandSize?: number;
  colBandSize?: number;
  /** Rows marked as repeating header rows also count as first rows. */
  headerRows?: number;
}): TableConditionType[] {
  const look = { ...DEFAULT_TABLE_LOOK, ...opts.look };
  const out: TableConditionType[] = ['wholeTable'];
  const headerRows = look.firstRow ? Math.max(1, opts.headerRows ?? 1) : 0;
  const isFirstRow = look.firstRow && opts.row < headerRows;
  const isLastRow = look.lastRow && opts.row === opts.rowCount - 1;
  const isFirstCol = look.firstColumn && opts.col === 0;
  const isLastCol = look.lastColumn && opts.col === opts.colCount - 1;
  if (!look.noVBand && !isFirstCol && !isLastCol) {
    const c = opts.col - (look.firstColumn ? 1 : 0);
    out.push(Math.floor(c / Math.max(1, opts.colBandSize ?? 1)) % 2 === 0 ? 'band1Vert' : 'band2Vert');
  }
  if (!look.noHBand && !isFirstRow && !isLastRow) {
    const r = opts.row - headerRows;
    out.push(Math.floor(r / Math.max(1, opts.rowBandSize ?? 1)) % 2 === 0 ? 'band1Horz' : 'band2Horz');
  }
  if (isFirstCol) out.push('firstCol');
  if (isLastCol) out.push('lastCol');
  if (isFirstRow) out.push('firstRow');
  if (isLastRow) out.push('lastRow');
  if (isFirstRow && isFirstCol) out.push('nwCell');
  if (isFirstRow && isLastCol) out.push('neCell');
  if (isLastRow && isFirstCol) out.push('swCell');
  if (isLastRow && isLastCol) out.push('seCell');
  return out;
}

/** Table-style context for the cell at (`row`, `col`) of a table node (col = grid column). */
export function tableCellContext(table: PMNode, row: number, col: number): TableCellContext {
  const map = TableMap.get(table);
  let headerRows = 0;
  for (let r = 0; r < table.childCount && table.child(r).attrs.header; r++) headerRows++;
  const ctx: TableCellContext = {
    conditions: tableConditions({
      row,
      col,
      rowCount: map.height,
      colCount: map.width,
      look: table.attrs.look ?? undefined,
      rowBandSize: table.attrs.rowBandSize ?? 1,
      colBandSize: table.attrs.colBandSize ?? 1,
      headerRows: headerRows || 1,
    }),
  };
  if (table.attrs.styleId) ctx.tableStyleId = table.attrs.styleId;
  return ctx;
}

/** Merges one property group across a table style chain and its applicable conditions. */
function tableStyleLayer<K extends 'pPr' | 'rPr' | 'tblPr' | 'tcPr'>(
  styles: StyleSheet,
  table: TableCellContext | undefined,
  key: K,
  merge: (a: NonNullable<Style[K]>, b: NonNullable<Style[K]> | undefined) => NonNullable<Style[K]>,
): NonNullable<Style[K]> | undefined {
  if (!table) return undefined;
  const chain = styles.chain(table.tableStyleId ?? styles.defaultStyle('table')?.id);
  if (chain.length === 0) return undefined;
  let acc = {} as NonNullable<Style[K]>;
  let any = false;
  for (const cond of table.conditions) {
    for (const s of chain) {
      const part = cond === 'wholeTable' ? s[key] : undefined;
      if (part) {
        acc = merge(acc, part as NonNullable<Style[K]>);
        any = true;
      }
      const c = s.conditions?.[cond]?.[key as keyof NonNullable<Style['conditions']>[TableConditionType]];
      if (c) {
        acc = merge(acc, c as NonNullable<Style[K]>);
        any = true;
      }
    }
  }
  return any ? acc : undefined;
}

// ------------------------------------------------------------------ paragraphs

export interface ResolvedParagraphProps extends ParagraphProps {
  styleId: string;
  alignment: Alignment;
  indent: { left: number; right: number; firstLine: number; hanging: number };
  spacing: {
    before: number;
    after: number;
    line: number;
    lineRule: LineRule;
    beforeAutospacing: boolean;
    afterAutospacing: boolean;
  };
  keepNext: boolean;
  keepLines: boolean;
  pageBreakBefore: boolean;
  widowControl: boolean;
  contextualSpacing: boolean;
  outlineLevel: number;
  tabs: TabStop[];
  bidi: boolean;
  /** Effective list membership (after style numbering and `numId` 0 cancellation). */
  numbering?: NumberingRef;
}

function styleChainParagraph(ctx: DocContext, styleId: string | undefined): Style[] {
  const id = styleId && ctx.styles.has(styleId) ? styleId : ctx.styles.defaultStyle('paragraph')?.id;
  return ctx.styles.chain(id);
}

/**
 * The list a paragraph belongs to: direct `numPr` (with `numId` 0 cancelling a style's list, and an
 * `ilvl`-only override keeping the style's list), else the paragraph style's numbering, with the level
 * from the style, or from the list level linked to the style (`w:pStyle`).
 */
export function effectiveNumbering(ctx: DocContext, direct: ParagraphProps): NumberingRef | undefined {
  const chain = styleChainParagraph(ctx, direct.styleId);
  const leaf = chain[chain.length - 1]?.id;
  const fromStyle = ctx.styles.styleNumbering(leaf);
  const d = direct.numbering as Partial<NumberingRef> | undefined;
  if (d && d.numId === 0) return undefined;
  if (d && d.numId !== undefined) return { numId: d.numId, level: d.level ?? fromStyle?.level ?? 0 };
  if (!fromStyle) return undefined;
  let level = d?.level ?? fromStyle.level;
  const styleHasLevel = chain.some((s) => s.pPr?.numbering?.level !== undefined);
  if (d?.level === undefined && !styleHasLevel && ctx.numbering && leaf) {
    level = ctx.numbering.levelForStyle(fromStyle.numId, leaf) ?? level;
  }
  return { numId: fromStyle.numId, level };
}

/** Resolves the effective paragraph properties of a paragraph node (or of direct props). */
export function resolveParagraphProps(
  ctx: DocContext,
  paragraph: PMNode | ParagraphProps,
  table?: TableCellContext,
): ResolvedParagraphProps {
  const direct = isNode(paragraph) ? paragraphPropsFromNode(paragraph) : paragraph;
  let p: ParagraphProps = mergeParagraphProps({}, ctx.styles.docDefaults.pPr);
  p = mergeParagraphProps(p, tableStyleLayer(ctx.styles, table, 'pPr', mergeParagraphProps));
  const chain = styleChainParagraph(ctx, direct.styleId);
  for (const s of chain) p = mergeParagraphProps(p, s.pPr);
  const num = effectiveNumbering(ctx, direct);
  if (num && ctx.numbering) p = mergeParagraphProps(p, ctx.numbering.level(num.numId, num.level)?.pPr);
  p = mergeParagraphProps(p, { ...direct, numbering: undefined });
  const ind = p.indent ?? {};
  const sp = p.spacing ?? {};
  const resolved: ResolvedParagraphProps = {
    ...p,
    styleId: chain[chain.length - 1]?.id ?? direct.styleId ?? 'Normal',
    alignment: p.alignment ?? 'left',
    indent: {
      left: ind.left ?? 0,
      right: ind.right ?? 0,
      firstLine: ind.firstLine ?? 0,
      hanging: ind.hanging ?? 0,
    },
    spacing: {
      before: sp.before ?? 0,
      after: sp.after ?? 0,
      line: sp.line ?? 240,
      lineRule: sp.lineRule ?? 'auto',
      beforeAutospacing: sp.beforeAutospacing ?? false,
      afterAutospacing: sp.afterAutospacing ?? false,
    },
    keepNext: p.keepNext ?? false,
    keepLines: p.keepLines ?? false,
    pageBreakBefore: p.pageBreakBefore ?? false,
    widowControl: p.widowControl ?? true,
    contextualSpacing: p.contextualSpacing ?? false,
    outlineLevel: p.outlineLevel ?? 9,
    tabs: p.tabs ?? [],
    bidi: p.bidi ?? false,
  };
  if (num) resolved.numbering = num;
  else delete resolved.numbering;
  return resolved;
}

// ------------------------------------------------------------------ runs

export interface ResolvedRunProps extends RunProps {
  bold: boolean;
  italic: boolean;
  strike: boolean;
  dstrike: boolean;
  caps: boolean;
  smallCaps: boolean;
  hidden: boolean;
  /** Font size in half-points (Word's default without any setting is 20 = 10 pt). */
  size: number;
  vertAlign: 'baseline' | 'superscript' | 'subscript';
  /** Concrete font names after theme resolution. */
  resolvedFonts: ResolvedFonts;
  /** `RRGGBB` or `auto`, after theme colour resolution. */
  colorHex: string;
}

export interface RunResolveInput {
  /** The containing paragraph (node or direct props). */
  paragraph: PMNode | ParagraphProps;
  /**
   * The run's direct formatting: PM marks or run props. For list labels of a plain-props paragraph,
   * `props` is the paragraph mark's run properties.
   */
  marks?: readonly Mark[];
  props?: RunProps;
  table?: TableCellContext;
  /**
   * `text` (default) for runs of text; `listLabel` for the paragraph's list number/bullet, which takes
   * the numbering level `w:rPr` and the paragraph mark's direct formatting instead of a run's.
   */
  target?: 'text' | 'listLabel';
}

function applyToggleLayer(
  out: RunProps,
  toggles: Record<string, boolean>,
  layer: RunProps | undefined,
): RunProps {
  if (!layer) return out;
  const nonToggle: RunProps = { ...layer };
  for (const t of TOGGLE_RUN_PROPS) {
    const v = layer[t];
    delete nonToggle[t];
    if (v === true) toggles[t] = !toggles[t];
  }
  return mergeRunProps(out, nonToggle);
}

function applyAbsolute(
  out: RunProps,
  toggles: Record<string, boolean>,
  layer: RunProps | undefined,
): RunProps {
  if (!layer) return out;
  for (const t of TOGGLE_RUN_PROPS) {
    const v = layer[t];
    if (v !== undefined) toggles[t] = v;
  }
  return mergeRunProps(out, layer);
}

function chainRun(chain: Style[]): RunProps | undefined {
  let acc: RunProps | undefined;
  for (const s of chain) if (s.rPr) acc = mergeRunProps(acc ?? {}, s.rPr);
  return acc;
}

/** Resolves the effective run properties of a run (or of a paragraph's list label). */
export function resolveRunProps(ctx: DocContext, input: RunResolveInput): ResolvedRunProps {
  const paraDirect = isNode(input.paragraph) ? paragraphPropsFromNode(input.paragraph) : input.paragraph;
  const markProps = isNode(input.paragraph)
    ? ((input.paragraph.attrs.markProps as RunProps | null) ?? undefined)
    : input.target === 'listLabel'
      ? input.props
      : undefined;
  const direct: RunProps = input.props ?? (input.marks ? runPropsFromMarks(input.marks) : {});
  const target = input.target ?? 'text';

  const toggles: Record<string, boolean> = {};
  let rp: RunProps = {};
  rp = applyAbsolute(rp, toggles, ctx.styles.docDefaults.rPr);
  rp = applyAbsolute(rp, toggles, tableStyleLayer(ctx.styles, input.table, 'rPr', mergeRunProps));
  rp = applyAbsolute(rp, toggles, chainRun(styleChainParagraph(ctx, paraDirect.styleId)));
  if (target === 'listLabel') {
    const num = effectiveNumbering(ctx, paraDirect);
    if (num && ctx.numbering) rp = applyAbsolute(rp, toggles, ctx.numbering.level(num.numId, num.level)?.rPr);
    rp = applyAbsolute(rp, toggles, markProps);
  } else {
    const charStyle = direct.styleId ?? undefined;
    if (charStyle && ctx.styles.get(charStyle)?.type === 'character') {
      rp = applyToggleLayer(rp, toggles, chainRun(ctx.styles.chain(charStyle)));
    } else {
      rp = applyToggleLayer(
        rp,
        toggles,
        chainRun(ctx.styles.chain(ctx.styles.defaultStyle('character')?.id)),
      );
    }
    rp = applyAbsolute(rp, toggles, direct);
  }
  for (const t of TOGGLE_RUN_PROPS) {
    if (t in toggles) rp[t] = toggles[t] as boolean;
  }
  const theme = ctx.theme;
  const resolved: ResolvedRunProps = {
    ...rp,
    bold: rp.bold ?? false,
    italic: rp.italic ?? false,
    strike: rp.strike ?? false,
    dstrike: rp.dstrike ?? false,
    caps: rp.caps ?? false,
    smallCaps: rp.smallCaps ?? false,
    hidden: rp.hidden ?? false,
    size: rp.size ?? 20,
    vertAlign: rp.vertAlign ?? 'baseline',
    resolvedFonts: resolveRunFonts(rp.fonts, theme),
    colorHex: resolveDocColor(rp.color, {
      ...(theme ? { theme } : {}),
      ...(ctx.settings?.colorSchemeMapping ? { colorMapping: ctx.settings.colorSchemeMapping } : {}),
    }),
  };
  if (direct.styleId && target === 'text') resolved.styleId = direct.styleId;
  else delete resolved.styleId;
  return resolved;
}

// ------------------------------------------------------------------ tables (props)

/** Effective table properties: table style chain `tblPr` then direct. */
export function resolveTableProps(ctx: DocContext, table: PMNode): TableProps {
  const styleId = (table.attrs.styleId as string | null) ?? ctx.styles.defaultStyle('table')?.id;
  let tp: TableProps = {};
  for (const s of ctx.styles.chain(styleId)) tp = mergeTableProps(tp, s.tblPr);
  const direct: TableProps = {};
  for (const [k, v] of Object.entries(table.attrs))
    if (v !== null && k !== 'grid') (direct as Record<string, unknown>)[k] = v;
  return mergeTableProps(tp, direct);
}

/** Effective cell properties: table style (`tcPr` + conditional) then the cell's own. */
export function resolveCellProps(ctx: DocContext, table: PMNode, row: number, col: number): TableCellProps {
  const cellCtx = tableCellContext(table, row, col);
  let cp: TableCellProps = tableStyleLayer(ctx.styles, cellCtx, 'tcPr', mergeCellProps) ?? {};
  // Conditional shading/borders also come from the region's tblPr (e.g. firstRow tblPr shading).
  const map = TableMap.get(table);
  const index = map.map[row * map.width + col];
  if (index !== undefined) {
    const cell = table.nodeAt(index);
    if (cell) {
      const own: TableCellProps = {};
      for (const [k, v] of Object.entries(cell.attrs)) {
        if (v !== null && k !== 'colspan' && k !== 'rowspan' && k !== 'colwidth')
          (own as Record<string, unknown>)[k] = v;
      }
      cp = mergeCellProps(cp, own);
    }
  }
  return cp;
}

// ------------------------------------------------------------------ positions

/** Effective paragraph and run formatting at a document position (for the ribbon's state). */
export function resolveAt(
  ctx: DocContext,
  doc: PMNode,
  pos: number,
): { paragraph: ResolvedParagraphProps; run: ResolvedRunProps } | undefined {
  const $pos = doc.resolve(Math.max(0, Math.min(pos, doc.content.size)));
  let paragraph: PMNode | undefined;
  let table: TableCellContext | undefined;
  for (let d = $pos.depth; d >= 0; d--) {
    const n = $pos.node(d);
    if (!paragraph && n.type.name === 'paragraph') paragraph = n;
    if (!table && n.type.name === 'tableCell' && d >= 2) {
      const tableNode = $pos.node(d - 2);
      const rowIndex = $pos.index(d - 2);
      const map = TableMap.get(tableNode);
      const cellPos = $pos.before(d) - $pos.start(d - 2);
      const rect = map.findCell(cellPos);
      table = tableCellContext(tableNode, rowIndex, rect.left);
    }
  }
  if (!paragraph) return undefined;
  const marks = $pos.marks();
  return {
    paragraph: resolveParagraphProps(ctx, paragraph, table),
    run: resolveRunProps(ctx, { paragraph, marks, ...(table ? { table } : {}) }),
  };
}

/** Walks the body (not headers/footers/notes) calling `fn` for each paragraph with its table context. */
export function forEachBodyParagraph(
  doc: PMNode,
  fn: (paragraph: PMNode, pos: number, table: TableCellContext | undefined) => void,
): void {
  const walk = (node: PMNode, offset: number, table: TableCellContext | undefined) => {
    node.forEach((child, childOffset) => {
      const pos = offset + childOffset;
      const name = child.type.name;
      if (name === 'header' || name === 'footer' || name === 'footnote') return;
      if (name === 'paragraph') {
        fn(child, pos, table);
        return;
      }
      if (name === 'table') {
        const map = TableMap.get(child);
        child.forEach((row, rowOffset, rowIndex) => {
          row.forEach((cell, cellOffset) => {
            const rel = rowOffset + 1 + cellOffset;
            const rect = map.findCell(rel);
            walk(cell, pos + 1 + rel + 1, tableCellContext(child, rowIndex, rect.left));
          });
        });
        return;
      }
      if (!child.isLeaf) walk(child, pos + 1, table);
    });
  };
  walk(doc, 0, undefined);
}

function isNode(x: unknown): x is PMNode {
  return typeof x === 'object' && x !== null && 'type' in x && 'attrs' in x && 'nodeSize' in x;
}
