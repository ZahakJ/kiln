/**
 * Tables (`w:tbl`, ECMA-376 Part 1 §17.4). Word's cell model is converted to prosemirror-tables'
 * rectangular model:
 * - `w:gridSpan` → `colspan`;
 * - a `w:vMerge="restart"` cell and the `continue` cells below it → one cell with `rowspan`;
 * - legacy `w:hMerge` runs → one cell with `colspan`;
 * - `w:gridBefore`/`w:gridAfter` (and ragged rows) → empty placeholder cells, identified by
 *   `GRID_PLACEHOLDER` in their `xml` attribute so the writer can drop them again.
 */
import type { RowRevision, SdtInfo } from '@officina/doc-model';
import { child, children, selectAlternateContent, type XmlElement } from '@officina/ooxml';
import type { Node as PMNode } from '@tiptap/pm/model';
import { parseTwips } from '../common/units.ts';
import { attr, stepped } from '../common/xml.ts';
import { parseCellProps, parseRowProps, parseTableProps } from './props.ts';
import { parseSdtInfo } from './sdt.ts';
import { BlockSink, type StoryReader, SUPPORTED_MC } from './story.ts';

/** Marker in a placeholder cell's `xml` attribute (cells that stand for `w:gridBefore`/`w:gridAfter`). */
export const GRID_PLACEHOLDER = '<!--officina:grid-placeholder-->';

interface CellRec {
  col: number;
  span: number;
  rowspan: number;
  blocks: PMNode[];
  attrs: Record<string, unknown>;
  skip: boolean;
}

interface RowRec {
  attrs: Record<string, unknown>;
  cells: CellRec[];
  width: number;
  gridBefore: number;
}

function twipsToPx(twips: number): number {
  return Math.max(1, Math.round(twips / 15));
}

function isEmptyParagraph(n: PMNode): boolean {
  return n.type.name === 'paragraph' && n.content.size === 0;
}

export function readTable(story: StoryReader, tbl: XmlElement, sdt: readonly SdtInfo[]): PMNode {
  const schema = story.schema;
  const tp = parseTableProps(child(tbl, 'w:tblPr'), story.raw);
  const grid = children(child(tbl, 'w:tblGrid'), 'w:gridCol').map((g) => parseTwips(attr(g, 'w:w')) ?? 0);

  // ---- collect rows (unwrapping row-level content controls and custom XML)
  const rowEls: [XmlElement, SdtInfo[], string][] = [];
  const collectRows = (parent: XmlElement, rowSdt: SdtInfo[]) => {
    for (const [el, step] of stepped(parent)) {
      story.path.push(step);
      try {
        switch (el.name) {
          case 'w:tr':
            rowEls.push([el, rowSdt, story.path.toString()]);
            break;
          case 'w:sdt': {
            const info = parseSdtInfo(el, 'row', story.ctx.nextSdtKey(), story.raw);
            const content = child(el, 'w:sdtContent');
            if (content) collectRows(content, [...rowSdt, info]);
            break;
          }
          case 'w:customXml':
            collectRows(el, rowSdt);
            break;
          case 'mc:AlternateContent':
            for (const c of selectAlternateContent(el, SUPPORTED_MC)) {
              if (c.name === 'w:tr') rowEls.push([c, rowSdt, story.path.toString()]);
            }
            break;
          case 'w:bookmarkStart':
          case 'w:bookmarkEnd':
          case 'w:commentRangeStart':
          case 'w:commentRangeEnd':
            // Row-level markers: hand them to the next paragraph via the story.
            story.readInlineMarker(el);
            break;
          default:
            break;
        }
      } finally {
        story.path.pop();
      }
    }
  };
  collectRows(tbl, []);

  // ---- read rows and cells
  const rows: RowRec[] = [];
  /** Open vertical merges: grid column → origin cell. */
  const open = new Map<number, CellRec>();
  let maxWidth = grid.length;

  for (const [tr, rowSdt, rowPath] of rowEls) {
    const rp = parseRowProps(child(tr, 'w:trPr'), story.raw);
    const tblPrEx = child(tr, 'w:tblPrEx');
    const rowXml = tblPrEx ? [story.raw(tblPrEx), ...rp.xml] : rp.xml;
    const attrs: Record<string, unknown> = { ...rp.props };
    if (rp.revision) attrs.revision = rp.revision satisfies RowRevision;
    if (rowXml.length) attrs.xml = rowXml;
    const row: RowRec = { attrs, cells: [], width: 0, gridBefore: rp.gridBefore };
    let col = rp.gridBefore;
    const touched = new Set<number>();

    const cellEls: [XmlElement, SdtInfo[], string][] = [];
    const collectCells = (parent: XmlElement, cellSdt: SdtInfo[], base: string) => {
      const counts = new Map<string, number>();
      for (const el of children(parent)) {
        const n = (counts.get(el.name) ?? 0) + 1;
        counts.set(el.name, n);
        const p = `${base}/${el.name}[${n}]`;
        if (el.name === 'w:tc') cellEls.push([el, cellSdt, p]);
        else if (el.name === 'w:sdt') {
          const info = parseSdtInfo(el, 'cell', story.ctx.nextSdtKey(), story.raw);
          const content = child(el, 'w:sdtContent');
          if (content) collectCells(content, [...cellSdt, info], `${p}/w:sdtContent[1]`);
        } else if (el.name === 'w:customXml') collectCells(el, cellSdt, p);
        else if (
          el.name === 'w:bookmarkStart' ||
          el.name === 'w:bookmarkEnd' ||
          el.name === 'w:commentRangeStart' ||
          el.name === 'w:commentRangeEnd'
        )
          story.readInlineMarker(el);
      }
    };
    collectCells(tr, [], rowPath);

    let previous: CellRec | undefined;
    for (const [tc, cellSdt, cellPath] of cellEls) {
      const cp = parseCellProps(child(tc, 'w:tcPr'), story.raw);
      const sink = new BlockSink(schema);
      story.path.pushAbsolute(cellPath);
      try {
        story.readBlocks(tc, sink, [...rowSdt, ...cellSdt]);
        if (cp.revision) {
          story.warn(
            'revision.cell',
            'info',
            'A tracked cell insertion, deletion or merge is kept but not shown.',
          );
        }
      } finally {
        story.path.popAbsolute();
      }
      const blocks = sink.blocks();
      if (blocks.length === 0) blocks.push(schema.nodes.paragraph?.create() as PMNode);
      const cellAttrs: Record<string, unknown> = { ...cp.props };
      if (cp.xml.length) cellAttrs.xml = cp.xml;
      const rec: CellRec = { col, span: cp.gridSpan, rowspan: 1, blocks, attrs: cellAttrs, skip: false };

      if (cp.hMerge === 'continue' && previous && !previous.skip) {
        previous.span += cp.gridSpan;
        previous.blocks.push(...blocks.filter((b) => !isEmptyParagraph(b)));
        col += cp.gridSpan;
        continue;
      }
      if (cp.vMerge === 'continue') {
        const origin = open.get(col);
        if (origin && origin.span === cp.gridSpan) {
          origin.rowspan += 1;
          const extra = blocks.filter((b) => !isEmptyParagraph(b));
          origin.blocks.push(...extra);
          rec.skip = true;
          for (let k = 0; k < cp.gridSpan; k++) touched.add(col + k);
        }
      }
      if (!rec.skip) {
        for (let k = 0; k < cp.gridSpan; k++) {
          open.delete(col + k);
          touched.add(col + k);
        }
        if (cp.vMerge === 'restart' || cp.vMerge === 'continue') open.set(col, rec);
      }
      row.cells.push(rec);
      previous = rec;
      col += cp.gridSpan;
    }
    // A column with no cell in this row ends any vertical merge through it.
    for (const c of [...open.keys()]) if (!touched.has(c)) open.delete(c);
    row.width = col + rp.gridAfter;
    maxWidth = Math.max(maxWidth, row.width);
    rows.push(row);
  }

  // ---- build nodes
  const cellType = schema.nodes.tableCell;
  const rowType = schema.nodes.tableRow;
  if (!cellType || !rowType) throw new Error('schema lacks table nodes');
  const placeholder = (col: number, span: number) =>
    cellType.create(
      {
        colspan: span,
        colwidth: colwidths(grid, col, span),
        borders: {
          top: { style: 'nil' },
          left: { style: 'nil' },
          bottom: { style: 'nil' },
          right: { style: 'nil' },
        },
        xml: [GRID_PLACEHOLDER],
      },
      schema.nodes.paragraph?.create(),
    );

  let irregular = false;
  const rowNodes: PMNode[] = [];
  for (const row of rows) {
    const cells: PMNode[] = [];
    if (row.gridBefore > 0) cells.push(placeholder(0, row.gridBefore));
    for (const c of row.cells) {
      if (c.skip) continue;
      cells.push(
        cellType.create(
          {
            ...c.attrs,
            colspan: c.span,
            rowspan: c.rowspan,
            colwidth: colwidths(grid, c.col, c.span),
          },
          c.blocks,
        ),
      );
    }
    if (row.width < maxWidth) {
      const missing = maxWidth - row.width;
      cells.push(placeholder(row.width, missing));
      const xml = [...((row.attrs.xml as string[] | undefined) ?? [])];
      if (!xml.some((x) => x.startsWith('<w:gridAfter'))) {
        // A ragged row: Word lays it out as if it had w:gridAfter, which is what the writer emits.
        xml.push(`<w:gridAfter w:val="${missing}"/>`);
        irregular = true;
      }
      row.attrs.xml = xml;
    }
    if (cells.length === 0) cells.push(placeholder(0, Math.max(1, maxWidth)));
    rowNodes.push(rowType.create(row.attrs, cells));
  }
  if (irregular) {
    story.warn(
      'table.irregular',
      'info',
      'Rows with fewer cells than the table grid were padded with empty cells.',
    );
  }
  if (rowNodes.length === 0) {
    rowNodes.push(rowType.create(null, cellType.create(null, schema.nodes.paragraph?.create())));
  }

  const tableAttrs: Record<string, unknown> = { ...tp.props, grid: grid.length ? grid : null };
  if (sdt.length) tableAttrs.sdt = [...sdt];
  if (tp.xml.length) tableAttrs.xml = tp.xml;
  return schema.nodes.table?.create(tableAttrs, rowNodes) as PMNode;
}

function colwidths(grid: readonly number[], col: number, span: number): number[] | null {
  const out: number[] = [];
  for (let k = 0; k < span; k++) {
    const w = grid[col + k];
    if (w === undefined || w <= 0) return null;
    out.push(twipsToPx(w));
  }
  return out;
}
