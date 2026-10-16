/**
 * Writing of one *story* (the body, a header or footer, a note, a comment): blocks (paragraphs, tables,
 * generated tables of contents, preserved blocks, content controls) and inline content (runs, hyperlinks,
 * tracked changes, complex fields with cached results, symbols, notes, bookmarks, comment ranges,
 * drawings, equations).
 *
 * Mapping (the inverse of the reader):
 * - marks → `w:rPr` (`runPropsFromMarks`), `w:hyperlink`, `w:ins`/`w:del`, `w:rPrChange`, inline
 *   `w:sdt`, comment range markers plus a `w:commentReference` run after each range;
 * - `field` atoms → complex fields (`begin`, `instrText`, `separate`, cached result, `end`); nested fields
 *   written into the instruction as `{ … }` become nested complex fields again;
 * - `toc` blocks → a TOC field whose result is the block's paragraphs;
 * - `rowspan` cells → `w:vMerge` restart/continue cells; placeholder cells (`GRID_PLACEHOLDER`) are
 *   dropped (their `w:gridBefore`/`w:gridAfter` are in the row's raw properties);
 * - block, row and cell content controls (`sdt` attributes) → `w:sdt` around the grouped content.
 */
import {
  CELL_PROP_KEYS,
  type CellBorders,
  PARAGRAPH_PROP_KEYS,
  type ParagraphRevision,
  type RevisionInfo,
  ROW_PROP_KEYS,
  type RowRevision,
  runPropsFromMarks,
  type SdtInfo,
  TABLE_PROP_KEYS,
  type TableCellProps,
} from '@kiln-ref/doc-model';
import { RelType } from '@kiln-ref/ooxml';
import type { Mark, Node as PMNode } from '@tiptap/pm/model';
import type { DocxParagraphProps, DocxRunProps } from '../common/types.ts';
import { GRID_PLACEHOLDER } from '../read/tables.ts';
import type { DocxWriteContext } from './context.ts';
import { containerXml, docPrXml, patchShapeXml, pictureGraphicXml, shapeGraphicXml } from './drawing.ts';
import {
  cellPropsInner,
  paragraphPropsXml,
  revisionAttrs,
  rowPropsInner,
  runPropsXml,
  tablePropsInner,
} from './props.ts';
import { attrs, escapeText, needsPreserve, tag } from './xml.ts';

export type StoryKind = 'body' | 'header' | 'footer' | 'footnote' | 'endnote' | 'comment';

export interface StoryOptions {
  part: string;
  kind: StoryKind;
  /** Inside a note: the note's own id (its `footnoteRef` becomes `w:footnoteRef`). */
  noteId?: string;
  /** Note ids that exist, per kind (references to missing notes are not written). */
  notes?: { footnote: ReadonlySet<string>; endnote: ReadonlySet<string> };
  /** Comment ids that exist (ranges of missing comments are not written). */
  comments?: ReadonlySet<string>;
  /** Style ids present in the style sheet (for optional reference styles). */
  styleIds?: ReadonlySet<string>;
}

export interface BlockExtras {
  /** `w:sectPr` for the last paragraph of these blocks (section break). */
  sectPr?: string;
  /** Run content prepended to the first paragraph (comment `w:annotationRef`). */
  firstPrefix?: string;
  /** `w14:paraId` forced onto the last paragraph (comment threading). */
  lastParaId?: string;
}

interface Wrapper {
  key: string;
  open: () => string;
  close: string;
}

const isPlaceholderCell = (cell: PMNode) =>
  Array.isArray(cell.attrs.xml) && (cell.attrs.xml as string[]).includes(GRID_PLACEHOLDER);

/** Content controls of a block for grouping (a `toc` takes its first paragraph's). */
function blockSdt(node: PMNode | null | undefined): readonly SdtInfo[] {
  if (!node) return [];
  if (node.type.name === 'paragraph' || node.type.name === 'table')
    return (node.attrs.sdt as SdtInfo[] | null) ?? [];
  if (node.type.name === 'toc') return blockSdt(node.firstChild);
  return [];
}

function sdtOpen(info: SdtInfo): string {
  return `<w:sdt>${info.pr || '<w:sdtPr/>'}${info.endPr ?? ''}<w:sdtContent>`;
}
const SDT_CLOSE = '</w:sdtContent></w:sdt>';

/**
 * Wraps runs of consecutive items that share a content control of one of `levels` in `w:sdt`, outermost
 * first, recursively. `open` holds the keys of controls already written around this content.
 */
export function groupSdt<T>(
  items: readonly T[],
  infosOf: (item: T) => readonly SdtInfo[],
  levels: readonly SdtInfo['level'][],
  open: ReadonlySet<string>,
  emit: (item: T, open: ReadonlySet<string>, index: number) => string,
  offset = 0,
): string {
  const pending = (item: T) => infosOf(item).find((i) => levels.includes(i.level) && !open.has(i.key));
  let out = '';
  let i = 0;
  while (i < items.length) {
    const first = pending(items[i] as T);
    if (!first) {
      out += emit(items[i] as T, open, offset + i);
      i++;
      continue;
    }
    let j = i + 1;
    while (j < items.length && pending(items[j] as T)?.key === first.key) j++;
    const inner = groupSdt(
      items.slice(i, j),
      infosOf,
      levels,
      new Set([...open, first.key]),
      emit,
      offset + i,
    );
    out += sdtOpen(first) + inner + SDT_CLOSE;
    i = j;
  }
  return out;
}

/** Splits an instruction into text and nested-field pieces (`{ … }` as the reader writes them). */
export function splitInstruction(instr: string): (string | { nested: string })[] {
  const out: (string | { nested: string })[] = [];
  let depth = 0;
  let start = 0;
  let buf = '';
  for (let i = 0; i < instr.length; i++) {
    const c = instr[i];
    if (c === '{' && instr[i + 1] === ' ') {
      if (depth === 0) {
        buf += instr.slice(start, i);
        start = i;
      }
      depth++;
    } else if (c === '}' && depth > 0 && instr[i - 1] === ' ') {
      depth--;
      if (depth === 0) {
        if (buf) out.push(buf);
        buf = '';
        out.push({ nested: instr.slice(start + 2, i - 1).trim() });
        start = i + 1;
      }
    }
  }
  if (depth > 0) {
    // Unbalanced: literal braces.
    buf += instr.slice(start);
  } else buf += instr.slice(start);
  if (buf) out.push(buf);
  return out;
}

const RUN_SIMPLE = new Set(['text', 'tab', 'softBreak', 'pageBreak', 'columnBreak']);

export class StoryWriter {
  readonly part: string;
  /** Inline index (document order) → comment ids starting / ending there. */
  #commentStarts = new Map<number, string[]>();
  #commentEnds = new Map<number, string[]>();
  #inlineIndex = 0;
  #calloutDepth = 0;

  constructor(
    readonly ctx: DocxWriteContext,
    readonly options: StoryOptions,
  ) {
    this.part = options.part;
  }

  // ---------------------------------------------------------------- comments

  /** Records where each comment range starts and ends in `blocks` (call before writing them). */
  prepareComments(blocks: readonly PMNode[]): void {
    const first = new Map<string, number>();
    const last = new Map<string, number>();
    let index = 0;
    const visit = (n: PMNode) => {
      if (n.isInline) {
        for (const m of n.marks) {
          if (m.type.name !== 'comment') continue;
          const id = String(m.attrs.id);
          if (this.options.comments && !this.options.comments.has(id)) continue;
          if (!first.has(id)) first.set(id, index);
          last.set(id, index);
        }
        index++;
        return;
      }
      n.forEach(visit);
    };
    for (const b of blocks) visit(b);
    const push = (map: Map<number, string[]>, i: number, id: string) =>
      map.set(i, [...(map.get(i) ?? []), id]);
    for (const [id, i] of first) push(this.#commentStarts, i, id);
    for (const [id, i] of last) push(this.#commentEnds, i, id);
    this.#inlineIndex = 0;
  }

  // ---------------------------------------------------------------- blocks

  /** Writes blocks, grouping block-level content controls. */
  blocks(nodes: readonly PMNode[], open: ReadonlySet<string> = new Set(), extras: BlockExtras = {}): string {
    const last = nodes.length - 1;
    let out = groupSdt(nodes, blockSdt, ['block'], open, (node, o, i) =>
      this.#block(node, o, {
        ...(i === 0 && extras.firstPrefix ? { firstPrefix: extras.firstPrefix } : {}),
        ...(i === last && extras.sectPr ? { sectPr: extras.sectPr } : {}),
        ...(i === last && extras.lastParaId ? { lastParaId: extras.lastParaId } : {}),
      }),
    );
    if (extras.sectPr && !this.#endsWithParagraph(nodes[last])) {
      out += `<w:p><w:pPr>${extras.sectPr}</w:pPr></w:p>`;
    }
    return out;
  }

  #endsWithParagraph(node: PMNode | undefined): boolean {
    return !!node && (node.type.name === 'paragraph' || node.type.name === 'toc');
  }

  #block(node: PMNode, open: ReadonlySet<string>, extras: BlockExtras): string {
    switch (node.type.name) {
      case 'paragraph':
        return this.paragraph(node, extras);
      case 'table':
        return this.table(node, open);
      case 'toc':
        return this.#toc(node, extras);
      case 'opaqueBlock':
        return (node.attrs.xml as string) || '';
      case 'codeBlock':
        return this.#codeBlock(node, extras);
      case 'callout': {
        this.#calloutDepth++;
        try {
          return this.blocks(this.#children(node), open, extras);
        } finally {
          this.#calloutDepth--;
        }
      }
      default: {
        // Block types from schema extensions: keep their content.
        if (node.isTextblock) return this.paragraph(node, extras);
        if (node.childCount > 0) return this.blocks(this.#children(node), open, extras);
        return '';
      }
    }
  }

  #children(node: PMNode): PMNode[] {
    const out: PMNode[] = [];
    node.forEach((c) => {
      out.push(c);
    });
    return out;
  }

  #codeBlock(node: PMNode, extras: BlockExtras): string {
    const lines = node.textContent.split('\n');
    const style = this.options.styleIds?.has('Code') ? 'Code' : undefined;
    return lines
      .map((line, i) => {
        const pPr = paragraphPropsXml(
          {
            ...(style ? { styleId: style } : {}),
            spacing: { before: 0, after: 0, line: 240, lineRule: 'auto' },
            shading: { val: 'clear', color: { val: 'auto' }, fill: { val: 'F2F2F2' } },
          },
          i === lines.length - 1 && extras.sectPr ? { sectPr: extras.sectPr } : {},
        );
        const rPr = style
          ? ''
          : '<w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas" w:cs="Consolas"/></w:rPr>';
        const text = line ? `<w:r>${rPr}${this.#textElement(line, false)}</w:r>` : '';
        return `<w:p>${pPr}${text}</w:p>`;
      })
      .join('');
  }

  #toc(node: PMNode, extras: BlockExtras): string {
    const paras = this.#children(node);
    const begin =
      `<w:r><w:fldChar w:fldCharType="begin"${node.attrs.dirty ? ' w:dirty="true"' : ''}/></w:r>` +
      `<w:r><w:instrText xml:space="preserve"> ${escapeText(String(node.attrs.instr ?? ''))} </w:instrText></w:r>` +
      '<w:r><w:fldChar w:fldCharType="separate"/></w:r>';
    const end = '<w:r><w:fldChar w:fldCharType="end"/></w:r>';
    return paras
      .map((p, i) =>
        this.paragraph(p, {
          ...(i === 0 ? { firstPrefix: (extras.firstPrefix ?? '') + begin } : {}),
          ...(i === paras.length - 1 ? { suffix: end } : {}),
          ...(i === paras.length - 1 && extras.sectPr ? { sectPr: extras.sectPr } : {}),
          ...(i === paras.length - 1 && extras.lastParaId ? { lastParaId: extras.lastParaId } : {}),
        }),
      )
      .join('');
  }

  // ---------------------------------------------------------------- paragraphs

  paragraph(node: PMNode, extras: BlockExtras & { suffix?: string } = {}): string {
    const a = node.attrs;
    const props: DocxParagraphProps = {};
    for (const k of PARAGRAPH_PROP_KEYS) {
      const v = a[k];
      if (v !== null && v !== undefined) (props as Record<string, unknown>)[k] = v;
    }
    if (Array.isArray(a.xml) && a.xml.length) props.xml = a.xml as string[];
    if (this.#calloutDepth > 0) {
      props.borders ??= {
        top: { style: 'single', size: 4, space: 4, color: { val: 'auto' } },
        left: { style: 'single', size: 4, space: 4, color: { val: 'auto' } },
        bottom: { style: 'single', size: 4, space: 4, color: { val: 'auto' } },
        right: { style: 'single', size: 4, space: 4, color: { val: 'auto' } },
      };
      props.shading ??= { val: 'clear', color: { val: 'auto' }, fill: { val: 'F2F2F2' } };
    }
    const rev = (a.revision as ParagraphRevision | null) ?? {};
    const pPr = paragraphPropsXml(props, {
      markProps: (a.markProps as DocxRunProps | null) ?? null,
      ...(rev.mark ? { markRevision: this.#revision(rev.mark) } : {}),
      ...(rev.markPrChange ? { markPrChange: this.#revision(rev.markPrChange) } : {}),
      ...(rev.pPrChange ? { pPrChange: this.#revision(rev.pPrChange) } : {}),
      ...(extras.sectPr ? { sectPr: extras.sectPr } : {}),
    });
    const paraId = extras.lastParaId ?? (a.paraId as string | null) ?? undefined;
    const content = (extras.firstPrefix ?? '') + this.inline(node) + (extras.suffix ?? '');
    const open = `<w:p${attrs([['w14:paraId', paraId]])}`;
    return pPr || content ? `${open}>${pPr}${content}</w:p>` : `${open}/>`;
  }

  #revision<T extends RevisionInfo>(r: T): T {
    return r.id ? r : { ...r, id: this.ctx.nextRevisionId() };
  }

  // ---------------------------------------------------------------- inline

  /** Wrapper elements implied by a node's marks, outermost first. */
  #wrappers(marks: readonly Mark[]): Wrapper[] {
    const out: Wrapper[] = [];
    for (const m of marks) {
      if (m.type.name === 'sdt' && m.attrs.info) {
        const info = m.attrs.info as SdtInfo;
        out.push({ key: `sdt:${info.key}`, open: () => sdtOpen(info), close: SDT_CLOSE });
      }
    }
    const link = marks.find((m) => m.type.name === 'link');
    if (link) {
      out.push({
        key: `link:${JSON.stringify(link.attrs)}`,
        open: () => this.#hyperlinkOpen(link),
        close: '</w:hyperlink>',
      });
    }
    for (const m of marks) {
      if (m.type.name === 'insertion' || m.type.name === 'deletion') {
        const name = m.type.name === 'insertion' ? 'w:ins' : 'w:del';
        const r = this.#revision(m.attrs as RevisionInfo);
        out.push({
          key: `${name}:${JSON.stringify(m.attrs)}`,
          open: () => `<${name}${attrs(revisionAttrs(r))}>`,
          close: `</${name}>`,
        });
      }
    }
    return out;
  }

  #hyperlinkOpen(link: Mark): string {
    const href = link.attrs.href as string | null;
    let relId: string | undefined;
    if (href) {
      const internal = href.startsWith('/') && this.ctx.pkg.has(href);
      relId = this.ctx.rels(this.part).get(RelType.hyperlink, href, !internal);
    }
    return `<w:hyperlink${attrs([
      ['r:id', relId],
      ['w:anchor', (link.attrs.anchor as string | null) ?? undefined],
      ['w:tooltip', (link.attrs.tooltip as string | null) ?? undefined],
      ['w:tgtFrame', (link.attrs.target as string | null) ?? undefined],
      ['w:history', '1'],
    ])}>`;
  }

  /** The paragraph's inline content. */
  inline(paragraph: PMNode): string {
    const items: { node: PMNode; chain: Wrapper[]; index: number }[] = [];
    paragraph.forEach((n) => {
      items.push({ node: n, chain: this.#wrappers(n.marks), index: this.#inlineIndex++ });
    });
    // Zero-width markers take the wrappers their neighbours share, so they do not split a hyperlink.
    for (let i = 0; i < items.length; i++) {
      const it = items[i] as (typeof items)[number];
      if (it.chain.length || (it.node.type.name !== 'bookmarkStart' && it.node.type.name !== 'bookmarkEnd'))
        continue;
      const prev = items[i - 1]?.chain ?? [];
      let k = i + 1;
      while (k < items.length && (items[k] as (typeof items)[number]).chain.length === 0) k++;
      const next = items[k]?.chain ?? [];
      const common: Wrapper[] = [];
      for (let d = 0; d < Math.min(prev.length, next.length); d++) {
        if ((prev[d] as Wrapper).key !== (next[d] as Wrapper).key) break;
        common.push(prev[d] as Wrapper);
      }
      it.chain = common;
    }
    return this.#group(items, 0);
  }

  #group(items: { node: PMNode; chain: Wrapper[]; index: number }[], depth: number): string {
    let out = '';
    let run: { rPr: string; body: string } | null = null;
    const flush = () => {
      if (run) out += `<w:r>${run.rPr}${run.body}</w:r>`;
      run = null;
    };
    let i = 0;
    while (i < items.length) {
      const it = items[i] as (typeof items)[number];
      const w = it.chain[depth];
      if (w) {
        flush();
        let j = i + 1;
        while (j < items.length && (items[j] as (typeof items)[number]).chain[depth]?.key === w.key) j++;
        out += w.open() + this.#group(items.slice(i, j), depth + 1) + w.close;
        i = j;
        continue;
      }
      const starts = this.#commentStarts.get(it.index);
      if (starts) {
        flush();
        for (const id of starts) out += `<w:commentRangeStart w:id="${escapeText(id)}"/>`;
      }
      const deleted = it.node.marks.some((m) => m.type.name === 'deletion');
      if (RUN_SIMPLE.has(it.node.type.name)) {
        const rPr = this.#runProps(it.node.marks);
        const body = this.#simpleContent(it.node, deleted);
        if (run && (run as { rPr: string }).rPr === rPr) (run as { body: string }).body += body;
        else {
          flush();
          run = { rPr, body };
        }
      } else {
        flush();
        out += this.#leaf(it.node, deleted);
      }
      const ends = this.#commentEnds.get(it.index);
      if (ends) {
        flush();
        for (const id of ends) out += this.#commentEnd(id);
      }
      i++;
    }
    flush();
    return out;
  }

  #commentEnd(id: string): string {
    const style = this.options.styleIds?.has('CommentReference')
      ? '<w:rPr><w:rStyle w:val="CommentReference"/></w:rPr>'
      : '';
    const eid = escapeText(id);
    return `<w:commentRangeEnd w:id="${eid}"/><w:r>${style}<w:commentReference w:id="${eid}"/></w:r>`;
  }

  #runProps(marks: readonly Mark[]): string {
    const props = runPropsFromMarks(marks) as DocxRunProps;
    const fc = marks.find((m) => m.type.name === 'formatChange');
    return runPropsXml(
      props,
      fc ? { change: this.#revision({ ...(fc.attrs as RevisionInfo & { before: DocxRunProps }) }) } : {},
    );
  }

  /**
   * `w:t`/`w:delText`. Optional (U+00AD) and non-breaking (U+2011) hyphens stay literal characters,
   * which Word, LibreOffice and other readers all treat like `w:softHyphen`/`w:noBreakHyphen`.
   */
  #textElement(text: string, deleted: boolean): string {
    if (!text) return '';
    const name = deleted ? 'w:delText' : 'w:t';
    return `<${name}${needsPreserve(text) ? ' xml:space="preserve"' : ''}>${escapeText(text)}</${name}>`;
  }

  #simpleContent(node: PMNode, deleted: boolean): string {
    switch (node.type.name) {
      case 'text': {
        const props = runPropsFromMarks(node.marks) as DocxRunProps;
        if (props.sym) {
          const sym = tag('w:sym', [
            ['w:font', props.sym.font],
            ['w:char', props.sym.char],
          ]);
          return [...(node.text ?? '')].map(() => sym).join('');
        }
        return this.#textElement(node.text ?? '', deleted);
      }
      case 'tab':
        return '<w:tab/>';
      case 'softBreak':
        return node.attrs.clear ? tag('w:br', [['w:clear', node.attrs.clear as string]]) : '<w:br/>';
      case 'pageBreak':
        return '<w:br w:type="page"/>';
      case 'columnBreak':
        return '<w:br w:type="column"/>';
      default:
        return '';
    }
  }

  #leaf(node: PMNode, deleted: boolean): string {
    const a = node.attrs;
    switch (node.type.name) {
      case 'bookmarkStart':
        return tag('w:bookmarkStart', [
          ['w:id', a.id as string],
          ['w:name', a.name as string],
        ]);
      case 'bookmarkEnd':
        return tag('w:bookmarkEnd', [['w:id', a.id as string]]);
      case 'equation': {
        const omml = (a.omml as string) || '';
        if (!omml)
          this.ctx.issue(
            'equation.unsaved',
            'An equation without Office Math markup was not saved.',
            this.part,
          );
        return omml;
      }
      case 'field':
        return this.#field(node, deleted);
      case 'footnoteRef':
        return this.#noteRef(node);
      case 'image':
        return this.#image(node);
      case 'shape':
        return this.#shape(node);
      case 'opaqueInline': {
        const xml = (a.xml as string) || '';
        return xml ? `<w:r>${this.#runProps(node.marks)}${xml}</w:r>` : '';
      }
      default: {
        // Inline leaves from schema extensions: keep their text.
        const text = node.type.spec.leafText ? node.type.spec.leafText(node) : node.textContent;
        return text ? `<w:r>${this.#runProps(node.marks)}${this.#textElement(text, deleted)}</w:r>` : '';
      }
    }
  }

  #field(node: PMNode, deleted: boolean): string {
    const rPr = this.#runProps(node.marks);
    const a = node.attrs;
    const instrName = deleted ? 'w:delInstrText' : 'w:instrText';
    const instr = (text: string) =>
      `<w:r>${rPr}<${instrName} xml:space="preserve">${escapeText(text)}</${instrName}></w:r>`;
    const char = (type: string, extra = '') =>
      `<w:r>${rPr}<w:fldChar w:fldCharType="${type}"${extra}/></w:r>`;
    const instruction = (text: string): string =>
      splitInstruction(text)
        .map((piece) =>
          typeof piece === 'string'
            ? instr(piece)
            : char('begin') + instruction(` ${piece.nested} `) + char('end'),
        )
        .join('');
    let beginExtra = '';
    if (a.dirty) beginExtra += ' w:dirty="true"';
    if (a.locked) beginExtra += ' w:fldLock="true"';
    const result = String(a.result ?? '');
    let resultXml = '';
    if (result) {
      let buf = '';
      const flush = () => {
        if (buf) resultXml += this.#textElement(buf, deleted);
        buf = '';
      };
      for (const ch of result) {
        if (ch === '\t') {
          flush();
          resultXml += '<w:tab/>';
        } else if (ch === '\n') {
          flush();
          resultXml += '<w:br/>';
        } else buf += ch;
      }
      flush();
      resultXml = `<w:r>${rPr}${resultXml}</w:r>`;
    }
    return (
      char('begin', beginExtra) +
      instruction(` ${String(a.instr ?? '')} `) +
      char('separate') +
      resultXml +
      char('end')
    );
  }

  #noteRef(node: PMNode): string {
    const kind = node.attrs.kind === 'endnote' ? 'endnote' : 'footnote';
    const id = String(node.attrs.id ?? '');
    const rPr = this.#runProps(node.marks);
    if (
      (this.options.kind === 'footnote' || this.options.kind === 'endnote') &&
      this.options.kind === kind &&
      this.options.noteId === id
    ) {
      return `<w:r>${rPr}<w:${kind}Ref/></w:r>`;
    }
    if (this.options.notes && !this.options.notes[kind].has(id)) {
      this.ctx.issue('note.missing', `A ${kind} reference without its ${kind} was removed.`, this.part);
      return '';
    }
    const custom = node.attrs.customMark as string | null;
    const ref = tag(`w:${kind}Reference`, [
      ['w:customMarkFollows', custom ? '1' : undefined],
      ['w:id', id],
    ]);
    return `<w:r>${rPr}${ref}${custom ? this.#textElement(custom, false) : ''}</w:r>`;
  }

  #image(node: PMNode): string {
    const a = node.attrs;
    const part = this.ctx.mediaPart((a.mediaId as string | null) ?? null, (a.src as string | null) ?? null);
    if (!part) {
      this.ctx.issue(
        'image.missing',
        'A picture whose image data is not available was not saved.',
        this.part,
      );
      return '';
    }
    const relId = this.ctx.imageRel(this.part, part);
    const id = this.ctx.nextDocPrId();
    const name = `Picture ${id}`;
    const graphic = pictureGraphicXml({
      relId,
      docPrId: id,
      name,
      width: (a.width as number) || 0,
      height: (a.height as number) || 0,
      crop: (a.crop as { l: number; t: number; r: number; b: number } | null) ?? null,
      rotation: (a.rotation as number) || 0,
      flipH: !!a.flipH,
      flipV: !!a.flipV,
    });
    const docPr = docPrXml({
      id,
      name,
      alt: a.alt as string,
      title: a.title as string,
      decorative: !!a.decorative,
    });
    const drawing = containerXml(
      {
        placement: a.placement === 'anchor' ? 'anchor' : 'inline',
        wrap: a.wrap,
        position: a.position,
        width: (a.width as number) || 0,
        height: (a.height as number) || 0,
        docPr,
        graphic,
      },
      251658240 + id,
    );
    return `<w:r>${this.#runProps(node.marks)}<w:drawing>${drawing}</w:drawing></w:r>`;
  }

  #shape(node: PMNode): string {
    const a = node.attrs;
    const attrsIn = {
      preset: (a.preset as string) || 'rect',
      width: (a.width as number) || 0,
      height: (a.height as number) || 0,
      placement: a.placement === 'inline' ? ('inline' as const) : ('anchor' as const),
      wrap: a.wrap,
      position: a.position,
      fill: a.fill ?? null,
      line: a.line ?? null,
      rotation: (a.rotation as number) || 0,
    };
    let content: string;
    if (a.xml) content = patchShapeXml(a.xml as string, attrsIn, 251658240);
    else {
      const id = this.ctx.nextDocPrId();
      content = `<w:drawing>${containerXml(
        {
          ...attrsIn,
          docPr: docPrXml({ id, name: `Shape ${id}` }),
          graphic: shapeGraphicXml({ docPrId: id, ...attrsIn }),
        },
        251658240 + id,
      )}</w:drawing>`;
    }
    return `<w:r>${this.#runProps(node.marks)}${content}</w:r>`;
  }

  // ---------------------------------------------------------------- tables

  table(node: PMNode, open: ReadonlySet<string>): string {
    const a = node.attrs;
    const props: Record<string, unknown> = {};
    for (const k of TABLE_PROP_KEYS) if (a[k] !== null && a[k] !== undefined) props[k] = a[k];
    const layout = layoutTable(node);
    const grid = tableGrid(node, layout);
    const tblPr = tablePropsInner(props, (a.xml as string[] | null) ?? undefined);
    const rows: PMNode[] = this.#children(node);
    const rowXml = (row: PMNode, rowOpen: ReadonlySet<string>, r: number) =>
      this.#row(row, layout[r] ?? [], rowOpen);
    const body = groupSdt(rows, (row) => blockSdt(firstRealCell(row)?.firstChild), ['row'], open, rowXml);
    const gridXml = `<w:tblGrid>${grid.map((w) => `<w:gridCol w:w="${Math.max(0, Math.round(w))}"/>`).join('')}</w:tblGrid>`;
    return `<w:tbl>${tblPr ? `<w:tblPr>${tblPr}</w:tblPr>` : '<w:tblPr/>'}${gridXml}${body}</w:tbl>`;
  }

  #row(row: PMNode, slots: Slot[], open: ReadonlySet<string>): string {
    const a = row.attrs;
    const props: Record<string, unknown> = {};
    for (const k of ROW_PROP_KEYS) if (a[k] !== null && a[k] !== undefined) props[k] = a[k];
    const all = (a.xml as string[] | null) ?? [];
    const tblPrEx = all.filter((x) => x.startsWith('<w:tblPrEx'));
    let rest = all.filter((x) => !x.startsWith('<w:tblPrEx'));
    const rev = (a.revision as RowRevision | null) ?? undefined;
    if (slots.length === 0) {
      // A row of placeholders only: Word needs at least one cell, so the grid-before/after columns
      // become empty borderless cells.
      rest = rest.filter((x) => !x.startsWith('<w:gridBefore') && !x.startsWith('<w:gridAfter'));
      let col = 0;
      row.forEach((cell) => {
        const span = Math.max(1, (cell.attrs.colspan as number) || 1);
        slots.push({ kind: 'cell', cell, col, span });
        col += span;
      });
    }
    const trPr = rowPropsInner(props, rest, rev ? this.#revision(rev) : undefined);
    const cells = groupSdt(
      slots,
      (s) => (s.kind === 'cell' ? blockSdt(s.cell.firstChild) : []),
      ['cell'],
      open,
      (s, cellOpen) => this.#cell(s, cellOpen),
    );
    return `<w:tr>${tblPrEx.join('')}${trPr ? `<w:trPr>${trPr}</w:trPr>` : ''}${cells}</w:tr>`;
  }

  #cell(slot: Slot, open: ReadonlySet<string>): string {
    const cell = slot.cell;
    const a = cell.attrs;
    const props: TableCellProps = {};
    for (const k of CELL_PROP_KEYS)
      if (a[k] !== null && a[k] !== undefined) (props as Record<string, unknown>)[k] = a[k];
    const xml = ((a.xml as string[] | null) ?? []).filter((x) => x !== GRID_PLACEHOLDER);
    if (slot.kind === 'continue') {
      const keep: TableCellProps = {};
      if (props.width) keep.width = props.width;
      if (props.borders) keep.borders = props.borders as CellBorders;
      if (props.shading) keep.shading = props.shading;
      if (props.vAlign) keep.vAlign = props.vAlign;
      const tcPr = cellPropsInner(keep, { gridSpan: slot.span, vMerge: 'continue' });
      return `<w:tc><w:tcPr>${tcPr}</w:tcPr><w:p/></w:tc>`;
    }
    const rowspan = (a.rowspan as number) || 1;
    const tcPr = cellPropsInner(
      props,
      { gridSpan: slot.span, ...(rowspan > 1 ? { vMerge: 'restart' as const } : {}) },
      xml,
    );
    const blocks = this.#children(cell);
    let content = this.blocks(blocks, open);
    const lastBlock = blocks[blocks.length - 1];
    if (!lastBlock || (lastBlock.type.name !== 'paragraph' && lastBlock.type.name !== 'toc'))
      content += '<w:p/>';
    return `<w:tc>${tcPr ? `<w:tcPr>${tcPr}</w:tcPr>` : ''}${content}</w:tc>`;
  }
}

// ------------------------------------------------------------------ table layout

export type Slot =
  | { kind: 'cell'; cell: PMNode; col: number; span: number }
  | { kind: 'continue'; cell: PMNode; col: number; span: number };

function firstRealCell(row: PMNode): PMNode | undefined {
  let found: PMNode | undefined;
  row.forEach((c) => {
    if (!found && !isPlaceholderCell(c)) found = c;
  });
  return found;
}

/**
 * Places every cell on the grid: real cells, `continue` slots for rows covered by a `rowspan` above, and
 * nothing for placeholder cells (they stand for `w:gridBefore`/`w:gridAfter`).
 */
export function layoutTable(table: PMNode): Slot[][] {
  const out: Slot[][] = [];
  const occupied = new Map<number, { cell: PMNode; span: number; left: number }>();
  table.forEach((row) => {
    const slots: Slot[] = [];
    let col = 0;
    const takeContinuations = (until: number) => {
      for (;;) {
        const o = [...occupied.entries()]
          .filter(([c]) => c >= col && c < until)
          .sort((x, y) => x[0] - y[0])[0];
        if (!o) return;
        const [c, info] = o;
        slots.push({ kind: 'continue', cell: info.cell, col: c, span: info.span });
        col = c + info.span;
      }
    };
    row.forEach((cell) => {
      // Rows covered from above come before this cell when they sit at the current column.
      for (;;) {
        const o = occupied.get(col);
        if (!o) break;
        slots.push({ kind: 'continue', cell: o.cell, col, span: o.span });
        col += o.span;
      }
      const span = Math.max(1, (cell.attrs.colspan as number) || 1);
      if (!isPlaceholderCell(cell)) slots.push({ kind: 'cell', cell, col, span });
      const rowspan = Math.max(1, (cell.attrs.rowspan as number) || 1);
      if (rowspan > 1) occupied.set(col, { cell, span, left: rowspan });
      col += span;
    });
    takeContinuations(Number.POSITIVE_INFINITY);
    // Advance merges: cells that started in this row keep `left` rows including this one.
    for (const [c, info] of [...occupied.entries()]) {
      info.left -= 1;
      if (info.left <= 0) occupied.delete(c);
    }
    out.push(slots);
  });
  return out;
}

/** Grid column widths (twips): the stored grid unless columns were resized in the editor. */
export function tableGrid(table: PMNode, layout: Slot[][]): number[] {
  let cols = 0;
  table.forEach((row, _o, r) => {
    let end = 0;
    for (const s of layout[r] ?? []) end = Math.max(end, s.col + s.span);
    let c = 0;
    row.forEach((cell) => {
      c += Math.max(1, (cell.attrs.colspan as number) || 1);
    });
    cols = Math.max(cols, end, c);
  });
  cols = Math.max(cols, 1);
  const stored = (table.attrs.grid as number[] | null) ?? null;
  const fromCells: (number | undefined)[] = new Array(cols).fill(undefined);
  const resizedCol: boolean[] = new Array(cols).fill(false);
  let resized = false;
  table.forEach((_row, _o, r) => {
    for (const s of layout[r] ?? []) {
      if (s.kind !== 'cell') continue;
      const cw = s.cell.attrs.colwidth as number[] | null;
      if (!cw) continue;
      for (let k = 0; k < s.span && k < cw.length; k++) {
        const px = cw[k] as number;
        const c = s.col + k;
        if (c >= cols) continue;
        fromCells[c] ??= px * 15;
        if (stored && stored[c] !== undefined && Math.max(1, Math.round((stored[c] as number) / 15)) !== px) {
          resized = true;
          resizedCol[c] = true;
        }
      }
    }
  });
  if (stored && stored.length === cols && !resized) return stored;
  const grid: number[] = [];
  for (let c = 0; c < cols; c++) {
    // Columns the editor resized take their new width; the others keep the exact stored twips.
    const known = resizedCol[c] ? fromCells[c] : (stored?.[c] ?? fromCells[c]);
    grid.push(known ?? 0);
  }
  const missing = grid.filter((w) => !w).length;
  if (missing) {
    const width = 9360;
    const used = grid.reduce((s, w) => s + w, 0);
    const each = Math.max(360, Math.round(Math.max(width - used, missing * 360) / missing));
    for (let c = 0; c < cols; c++) if (!grid[c]) grid[c] = each;
  }
  return grid;
}
