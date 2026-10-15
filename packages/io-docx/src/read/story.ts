/**
 * Reading of one *story* (the main body, a header or footer, a footnote, a comment, …): block content
 * (paragraphs, tables, content controls), inline content (runs, hyperlinks, revisions, fields, symbols,
 * bookmarks, comment ranges, drawings, equations) and the state that spans paragraphs inside a story
 * (open complex fields, open comment ranges, block-level bookmarks).
 *
 * Mapping notes:
 * - Complex fields (`w:fldChar`) and simple fields (`w:fldSimple`) inside a paragraph become a `field`
 *   atom holding the instruction and the cached result text. Nested fields inside an instruction are
 *   written into `instr` the way Word displays field codes (`{ MERGEFIELD Name }`).
 * - Generated-table fields (TOC, INDEX, BIBLIOGRAPHY, TOA) span paragraphs; they become a `toc` block
 *   holding the result paragraphs.
 * - `HYPERLINK` fields become `link` marks on their result, exactly like `w:hyperlink`.
 * - `w:ins`/`w:moveTo` → `insertion` marks, `w:del`/`w:moveFrom` → `deletion` marks; `w:rPrChange` →
 *   `formatChange`; `w:pPrChange` and paragraph-mark revisions → the paragraph's `revision` attribute.
 * - Content controls are unwrapped; their properties are kept in the `sdt` attribute (blocks) or the
 *   `sdt` mark (inline).
 */
import {
  type HyperlinkField,
  marksFromRunProps,
  type ParagraphRevision,
  parseFieldInstruction,
  type RowRevision,
  type SdtInfo,
} from '@officina/doc-model';
import { child, selectAlternateContent, type XmlElement, text as xmlText } from '@officina/ooxml';
import { Fragment, type Mark, type Node as PMNode, type Schema } from '@tiptap/pm/model';
import { symbolText, symbolToUnicode } from '../common/symbols.ts';
import type { DocxRunProps } from '../common/types.ts';
import { parseOnOff } from '../common/units.ts';
import type { DocxWarningCode, DocxWarningSeverity } from '../common/warnings.ts';
import { attr, fragmentXml, stepped, XmlPath } from '../common/xml.ts';
import type { DocxReadContext } from './context.ts';
import { readDrawing, readObject, readPict } from './drawing.ts';
import { parseParagraphProps, parseRevision, parseRunProps, type RawXml } from './props.ts';
import { parseSdtInfo } from './sdt.ts';
import { readTable } from './tables.ts';

/** Markup-compatibility namespaces whose `mc:Choice` branches the reader understands. */
export const SUPPORTED_MC = new Set([
  'w14',
  'w15',
  'w16',
  'w16cex',
  'w16cid',
  'w16se',
  'w16sdtdh',
  'wp14',
  'wps',
  'wpg',
  'wpc',
  'wpi',
  'a14',
  'a16',
  'cx',
  'cx1',
  'cx2',
  'cx3',
  'cx4',
  'cx5',
  'cx6',
  'cx7',
  'cx8',
  'aink',
  'am3d',
  'v',
  'o',
  'w10',
  'm',
]);

/** Fields whose result is a run of whole paragraphs. */
const BLOCK_FIELDS = new Set(['TOC', 'INDEX', 'BIBLIOGRAPHY', 'TOA']);

/** Inline node types that a field result may contain and still be summarised as text. */
const TEXTUAL = new Set(['text', 'tab', 'softBreak', 'bookmarkStart', 'bookmarkEnd']);

interface FieldFrame {
  instr: string;
  phase: 'instr' | 'result';
  /**
   * collect: result buffered and summarised into a `field` atom; block: result paragraphs form a `toc`
   * block; link: result flows into the paragraph with a link mark; flat: the field broke across
   * paragraphs and its result flows into the paragraph unwrapped.
   */
  mode: 'collect' | 'block' | 'link' | 'flat';
  result: PMNode[];
  marks: readonly Mark[];
  dirty: boolean;
  locked: boolean;
  formField: boolean;
  nestedInInstr: boolean;
  link?: Mark;
  path: string;
}

/** Collects the blocks of a container, grouping generated-table paragraphs into `toc` nodes. */
export class BlockSink {
  readonly groups: { blocks: PMNode[]; sectPr?: XmlElement }[] = [{ blocks: [] }];
  #toc: { instr: string; dirty: boolean; paragraphs: PMNode[] } | null = null;

  constructor(readonly schema: Schema) {}

  get #current(): PMNode[] {
    return (this.groups[this.groups.length - 1] as { blocks: PMNode[] }).blocks;
  }

  addParagraph(p: PMNode, flags: { tocStart?: { instr: string; dirty: boolean }; tocEnd?: boolean }): void {
    if (flags.tocStart) {
      this.closeToc();
      this.#toc = { ...flags.tocStart, paragraphs: [] };
    }
    if (this.#toc) this.#toc.paragraphs.push(p);
    else this.#current.push(p);
    if (flags.tocEnd) this.closeToc();
  }

  addBlock(node: PMNode): void {
    this.closeToc();
    this.#current.push(node);
  }

  /** Ends the current section (body only): the blocks so far belong to `sectPr`. */
  sectionBreak(sectPr: XmlElement): void {
    this.closeToc();
    const cur = this.groups[this.groups.length - 1] as { blocks: PMNode[]; sectPr?: XmlElement };
    cur.sectPr = sectPr;
    this.groups.push({ blocks: [] });
  }

  closeToc(): void {
    if (!this.#toc) return;
    const { instr, dirty, paragraphs } = this.#toc;
    this.#toc = null;
    if (paragraphs.length === 0) return;
    this.#current.push(this.schema.node('toc', { instr, dirty }, paragraphs));
  }

  /** All blocks (single-group sinks). */
  blocks(): PMNode[] {
    this.closeToc();
    return this.groups.flatMap((g) => g.blocks);
  }
}

export interface StoryOptions {
  partName: string;
  /** Attributes of the part's root element (namespace declarations for raw XML). */
  rootAttrs: Readonly<Record<string, string>>;
  /** Root step of warning paths (`w:document`, `w:hdr`, …). */
  rootStep: string;
  /** Set when reading a footnote/endnote body (for `w:footnoteRef`). */
  note?: { id: string; kind: 'footnote' | 'endnote' };
}

export class StoryReader {
  readonly path: XmlPath;
  readonly schema: Schema;
  readonly partName: string;
  readonly raw: RawXml;
  readonly note: StoryOptions['note'];
  /** Open comment ranges, in start order. */
  readonly activeComments = new Set<string>();
  readonly fields: FieldFrame[] = [];
  /** Block-level bookmarks waiting for the next paragraph. */
  pendingInline: PMNode[] = [];
  /** Inline output of the paragraph being read. */
  #out: PMNode[] = [];
  #tocStart: { instr: string; dirty: boolean } | undefined;
  #tocEnd = false;
  #customMarkPending: PMNode | undefined;

  constructor(
    readonly ctx: DocxReadContext,
    readonly options: StoryOptions,
  ) {
    this.schema = ctx.schema;
    this.partName = options.partName;
    this.note = options.note;
    this.path = new XmlPath(options.rootStep);
    const scope = options.rootAttrs;
    this.raw = (n) => fragmentXml(n, scope);
  }

  warn(code: DocxWarningCode, severity: DocxWarningSeverity, message: string): void {
    this.ctx.warn({ code, severity, message, part: this.partName, path: this.path.toString() });
  }

  // ---------------------------------------------------------------- blocks

  /** Reads the block children of `container` (body, cell, header, note, comment, sdtContent). */
  readBlocks(container: XmlElement, sink: BlockSink, sdt: readonly SdtInfo[]): void {
    for (const [el, step] of stepped(container)) {
      this.path.push(step);
      try {
        this.#readBlock(el, sink, sdt);
      } finally {
        this.path.pop();
      }
    }
  }

  #readBlock(el: XmlElement, sink: BlockSink, sdt: readonly SdtInfo[]): void {
    switch (el.name) {
      case 'w:p':
        this.readParagraph(el, sink, sdt);
        return;
      case 'w:tbl':
        sink.addBlock(readTable(this, el, sdt));
        return;
      case 'w:sdt': {
        const info = parseSdtInfo(el, 'block', this.ctx.nextSdtKey(), this.raw);
        const content = child(el, 'w:sdtContent');
        if (content) {
          this.path.push('w:sdtContent[1]');
          this.readBlocks(content, sink, [...sdt, info]);
          this.path.pop();
        }
        return;
      }
      case 'w:customXml':
      case 'w:smartTag':
        this.readBlocks(el, sink, sdt);
        return;
      case 'w:customXmlPr':
      case 'w:smartTagPr':
      case 'w:sectPr':
      case 'w:tcPr':
      case 'w:proofErr':
      case 'w:moveFromRangeStart':
      case 'w:moveFromRangeEnd':
      case 'w:moveToRangeStart':
      case 'w:moveToRangeEnd':
      case 'w:customXmlInsRangeStart':
      case 'w:customXmlInsRangeEnd':
      case 'w:customXmlDelRangeStart':
      case 'w:customXmlDelRangeEnd':
        return;
      case 'w:bookmarkStart':
      case 'w:bookmarkEnd': {
        const node = this.#bookmark(el);
        if (node) this.pendingInline.push(node);
        return;
      }
      case 'w:commentRangeStart':
      case 'w:commentRangeEnd':
        this.#commentRange(el);
        return;
      case 'w:ins':
      case 'w:del':
      case 'w:moveFrom':
      case 'w:moveTo':
        // Block-level revision containers only wrap further block content.
        this.readBlocks(el, sink, sdt);
        return;
      case 'mc:AlternateContent':
        for (const c of selectAlternateContent(el, SUPPORTED_MC)) this.#readBlock(c, sink, sdt);
        return;
      case 'w:permStart':
      case 'w:permEnd':
        this.warn('permission', 'warning', 'Editing permission ranges are not supported and were removed.');
        return;
      case 'w:altChunk':
        this.warn(
          'altChunk',
          'info',
          'Imported content (altChunk) is kept in the file but not shown; Word merges it on open.',
        );
        sink.addBlock(this.schema.node('opaqueBlock', { xml: this.raw(el), label: 'Imported content' }));
        return;
      default:
        this.warn('element.unknown', 'warning', `Unsupported element ${el.name} was skipped.`);
    }
  }

  // ---------------------------------------------------------------- paragraphs

  readParagraph(p: XmlElement, sink: BlockSink, sdt: readonly SdtInfo[]): void {
    const parsed = parseParagraphProps(child(p, 'w:pPr'), this.raw);
    this.#out = this.pendingInline;
    this.pendingInline = [];
    this.#tocStart = undefined;
    this.#tocEnd = false;

    for (const [el, step] of stepped(p)) {
      if (el.name === 'w:pPr') continue;
      this.path.push(step);
      try {
        this.readInline(el, []);
      } finally {
        this.path.pop();
      }
    }
    this.#breakOpenFields();

    const { xml, ...props } = parsed.props;
    const attrs: Record<string, unknown> = { ...props };
    if (parsed.mark) {
      const { xml: markXml, ...markProps } = parsed.mark.props;
      const mp = markXml ? { ...markProps, xml: markXml } : markProps;
      if (Object.keys(mp).length) attrs.markProps = mp;
    }
    const paraId = attr(p, 'w14:paraId');
    if (paraId) attrs.paraId = paraId;
    if (sdt.length) attrs.sdt = [...sdt];
    const revision: ParagraphRevision = {};
    if (parsed.change) revision.pPrChange = parsed.change;
    const mr = parsed.mark?.markRevision;
    if (mr) {
      const { move, ...rest } = mr;
      revision.mark = rest;
      if (move) this.#warnMove();
    }
    if (parsed.mark?.change) revision.markPrChange = parsed.mark.change;
    if (Object.keys(revision).length) attrs.revision = revision;
    if (xml?.length) attrs.xml = xml;

    const node = this.schema.nodes.paragraph?.create(attrs, this.#out) as PMNode;
    this.#out = [];
    sink.addParagraph(node, {
      ...(this.#tocStart ? { tocStart: this.#tocStart } : {}),
      ...(this.#tocEnd ? { tocEnd: true } : {}),
    });
    if (parsed.sectPr) sink.sectionBreak(parsed.sectPr);
  }

  /** Paragraph ended inside a field that collects its result: release it into the text. */
  #breakOpenFields(): void {
    let warned = false;
    for (let i = 0; i < this.fields.length; i++) {
      const f = this.fields[i] as FieldFrame;
      if (f.phase === 'result' && f.mode === 'collect') {
        f.mode = 'flat';
        const buffered = f.result;
        f.result = [];
        for (const n of buffered) this.#emitFrom(i - 1, n);
        if (!warned) {
          warned = true;
          this.ctx.warn({
            code: 'field.multiParagraph',
            severity: 'warning',
            message: `The field "${f.instr.trim()}" spans several paragraphs; its result was kept as plain text.`,
            part: this.partName,
            path: f.path,
          });
        }
      }
    }
  }

  /** Called when the story ends: closes fields that never ended. */
  finishStory(lastBlocks?: PMNode[]): void {
    if (this.fields.length) {
      const f = this.fields[0] as FieldFrame;
      this.ctx.warn({
        code: 'field.unterminated',
        severity: 'warning',
        message: `The field "${f.instr.trim()}" has no end; its result was kept as plain text.`,
        part: this.partName,
        path: f.path,
      });
      this.fields.length = 0;
    }
    this.activeComments.clear();
    if (this.pendingInline.length && lastBlocks) {
      // Bookmarks after the last paragraph: attach to the end of the last paragraph.
      for (let i = lastBlocks.length - 1; i >= 0; i--) {
        const b = lastBlocks[i] as PMNode;
        if (b.type.name === 'paragraph') {
          lastBlocks[i] = b.copy(b.content.append(Fragment.fromArray(this.pendingInline)));
          this.pendingInline = [];
          break;
        }
      }
    }
    this.pendingInline = [];
  }

  // ---------------------------------------------------------------- inline

  /** Context marks that apply to everything emitted now (comments, hyperlink fields). */
  #ambientMarks(): Mark[] {
    const out: Mark[] = [];
    const commentType = this.schema.marks.comment;
    if (commentType) for (const id of this.activeComments) out.push(commentType.create({ id }));
    for (const f of this.fields) if (f.link && f.phase === 'result') out.push(f.link);
    return out;
  }

  #withMarks(base: readonly Mark[], extra: readonly Mark[]): readonly Mark[] {
    let set = base;
    for (const m of extra) set = m.addToSet(set);
    return set;
  }

  #emit(node: PMNode): void {
    this.#emitFrom(this.fields.length - 1, node);
  }

  /** Routes output to the innermost collecting field at or below `index`, else the paragraph. */
  #emitFrom(index: number, node: PMNode): void {
    for (let i = index; i >= 0; i--) {
      const f = this.fields[i] as FieldFrame;
      if (f.phase === 'instr') return; // display text inside a field code: not part of the document
      if (f.mode === 'collect') {
        f.result.push(node);
        return;
      }
    }
    this.#out.push(node);
  }

  #noteComments(node: PMNode): void {
    for (const m of node.marks)
      if (m.type.name === 'comment') this.ctx.anchoredComments.add(m.attrs.id as string);
  }

  #emitText(text: string, marks: readonly Mark[]): void {
    if (!text) return;
    const node = this.schema.text(text, marks);
    this.#noteComments(node);
    this.#emit(node);
  }

  #emitNode(name: string, attrs: Record<string, unknown> | null, marks: readonly Mark[]): void {
    const type = this.schema.nodes[name];
    if (!type) return;
    const node = type.create(attrs, null, marks);
    this.#noteComments(node);
    this.#emit(node);
  }

  /** Emits an already-built inline node (drawings, equations) with the ambient marks. */
  emitInline(node: PMNode): void {
    this.#noteComments(node);
    this.#emit(node);
  }

  readInline(el: XmlElement, ctxMarks: readonly Mark[]): void {
    const m = this.schema.marks;
    switch (el.name) {
      case 'w:r':
        this.readRun(el, ctxMarks);
        return;
      case 'w:hyperlink': {
        const link = this.#hyperlinkMark(el);
        this.#readInlineChildren(el, link ? [...ctxMarks, link] : ctxMarks);
        return;
      }
      case 'w:ins':
      case 'w:moveTo':
      case 'w:del':
      case 'w:moveFrom': {
        const ins = el.name === 'w:ins' || el.name === 'w:moveTo';
        if (el.name === 'w:moveTo' || el.name === 'w:moveFrom') this.#warnMove();
        const type = ins ? m.insertion : m.deletion;
        const mark = type?.create(parseRevision(el));
        this.#readInlineChildren(el, mark ? [...ctxMarks, mark] : ctxMarks);
        return;
      }
      case 'w:fldSimple':
        this.#simpleField(el, ctxMarks);
        return;
      case 'w:sdt': {
        const info = parseSdtInfo(el, 'inline', this.ctx.nextSdtKey(), this.raw);
        const mark = m.sdt?.create({ info });
        const content = child(el, 'w:sdtContent');
        if (content) {
          this.path.push('w:sdtContent[1]');
          this.#readInlineChildren(content, mark ? [...ctxMarks, mark] : ctxMarks);
          this.path.pop();
        }
        return;
      }
      case 'w:smartTag':
      case 'w:customXml':
      case 'w:dir':
      case 'w:bdo':
        this.#readInlineChildren(el, ctxMarks);
        return;
      case 'w:bookmarkStart':
      case 'w:bookmarkEnd': {
        const node = this.#bookmark(el);
        // Bookmarks mark positions in the document, never inside a field's cached result.
        if (node) this.#out.push(node);
        return;
      }
      case 'w:commentRangeStart':
      case 'w:commentRangeEnd':
        this.#commentRange(el);
        return;
      case 'm:oMath':
      case 'm:oMathPara': {
        const node = this.schema.nodes.equation?.create(
          { omml: this.raw(el), display: el.name === 'm:oMathPara' },
          null,
          this.#withMarks(ctxMarks, this.#ambientMarks()),
        );
        if (node) this.emitInline(node);
        return;
      }
      case 'mc:AlternateContent':
        for (const c of selectAlternateContent(el, SUPPORTED_MC)) this.readInline(c, ctxMarks);
        return;
      case 'w:permStart':
      case 'w:permEnd':
        this.warn('permission', 'warning', 'Editing permission ranges are not supported and were removed.');
        return;
      case 'w:subDoc':
        this.warn('subDocument', 'warning', 'Links to subdocuments (master documents) are not supported.');
        return;
      case 'w:customXmlPr':
      case 'w:smartTagPr':
      case 'w:proofErr':
      case 'w:moveFromRangeStart':
      case 'w:moveFromRangeEnd':
      case 'w:moveToRangeStart':
      case 'w:moveToRangeEnd':
      case 'w:customXmlInsRangeStart':
      case 'w:customXmlInsRangeEnd':
      case 'w:customXmlDelRangeStart':
      case 'w:customXmlDelRangeEnd':
      case 'w:customXmlMoveFromRangeStart':
      case 'w:customXmlMoveFromRangeEnd':
      case 'w:customXmlMoveToRangeStart':
      case 'w:customXmlMoveToRangeEnd':
        return;
      default:
        this.warn('element.unknown', 'warning', `Unsupported element ${el.name} was skipped.`);
    }
  }

  #readInlineChildren(el: XmlElement, ctxMarks: readonly Mark[]): void {
    for (const [c, step] of stepped(el)) {
      if (c.name === 'w:sdtPr' || c.name === 'w:sdtEndPr') continue;
      this.path.push(step);
      try {
        this.readInline(c, ctxMarks);
      } finally {
        this.path.pop();
      }
    }
  }

  #warnMove(): void {
    this.warn(
      'revision.move',
      'info',
      'A tracked move is shown as a deletion and an insertion; Word will no longer link the two.',
    );
  }

  #hyperlinkMark(el: XmlElement): Mark | undefined {
    const type = this.schema.marks.link;
    if (!type) return undefined;
    const attrs: Record<string, string | null> = { href: null, anchor: null, tooltip: null, target: null };
    const id = attr(el, 'r:id');
    if (id) {
      const rel = this.ctx.relationship(this.partName, id);
      if (rel) {
        attrs.href = rel.targetMode === 'External' ? rel.target : (rel.part ?? rel.target);
        this.ctx.consumeRel(this.partName, id);
      }
    }
    const anchor = attr(el, 'w:anchor');
    if (anchor) attrs.anchor = anchor;
    const tooltip = attr(el, 'w:tooltip');
    if (tooltip) attrs.tooltip = tooltip;
    const target = attr(el, 'w:tgtFrame');
    if (target) attrs.target = target;
    if (!attrs.href && !attrs.anchor) {
      const loc = attr(el, 'w:docLocation');
      if (loc) attrs.anchor = loc;
    }
    return type.create(attrs);
  }

  #bookmark(el: XmlElement): PMNode | undefined {
    const id = attr(el, 'w:id') ?? '';
    if (el.name === 'w:bookmarkStart') {
      return this.schema.nodes.bookmarkStart?.create({ id, name: attr(el, 'w:name') ?? '' });
    }
    return this.schema.nodes.bookmarkEnd?.create({ id });
  }

  /** A bookmark or comment-range marker outside any paragraph (between rows or cells). */
  readInlineMarker(el: XmlElement): void {
    if (el.name === 'w:bookmarkStart' || el.name === 'w:bookmarkEnd') {
      const node = this.#bookmark(el);
      if (node) this.pendingInline.push(node);
    } else this.#commentRange(el);
  }

  #commentRange(el: XmlElement): void {
    const id = attr(el, 'w:id');
    if (id === undefined) return;
    if (el.name === 'w:commentRangeStart') this.activeComments.add(id);
    else this.activeComments.delete(id);
  }

  // ---------------------------------------------------------------- runs

  #runMarks(props: DocxRunProps, change: ReturnType<typeof parseRunProps>['change'], ctx: readonly Mark[]) {
    let marks: readonly Mark[] = marksFromRunProps(props, this.schema);
    if (change) {
      const fc = this.schema.marks.formatChange?.create({
        id: change.id,
        author: change.author,
        date: change.date,
        before: change.before,
      });
      if (fc) marks = fc.addToSet(marks);
    }
    return this.#withMarks(this.#withMarks(marks, ctx), this.#ambientMarks());
  }

  readRun(r: XmlElement, ctxMarks: readonly Mark[]): void {
    const parsed = parseRunProps(child(r, 'w:rPr'), this.raw);
    const marks = (): readonly Mark[] => this.#runMarks(parsed.props, parsed.change, ctxMarks);
    for (const [c, step] of stepped(r)) {
      this.path.push(step);
      try {
        this.#readRunChild(c, parsed.props, parsed.change, ctxMarks, marks);
      } finally {
        this.path.pop();
      }
    }
    this.#customMarkPending = undefined;
  }

  #readRunChild(
    c: XmlElement,
    props: DocxRunProps,
    change: ReturnType<typeof parseRunProps>['change'],
    ctxMarks: readonly Mark[],
    marks: () => readonly Mark[],
  ): void {
    switch (c.name) {
      case 'w:rPr':
        return;
      case 'w:t':
      case 'w:delText': {
        const t = xmlText(c);
        if (this.#customMarkPending) {
          // w:customMarkFollows: this text is the note's custom reference mark.
          const ref = this.#customMarkPending;
          this.#customMarkPending = undefined;
          this.#emit(ref.type.create({ ...ref.attrs, customMark: t }, null, ref.marks));
          return;
        }
        this.#emitText(t, marks());
        return;
      }
      case 'w:tab':
        this.#emitNode('tab', null, marks());
        return;
      case 'w:ptab':
        this.warn(
          'element.unknown',
          'warning',
          'An alignment tab (positional tab) was converted to an ordinary tab.',
        );
        this.#emitNode('tab', null, marks());
        return;
      case 'w:br': {
        const type = attr(c, 'w:type');
        if (type === 'page') this.#emitNode('pageBreak', null, marks());
        else if (type === 'column') this.#emitNode('columnBreak', null, marks());
        else {
          const clear = attr(c, 'w:clear');
          this.#emitNode('softBreak', clear && clear !== 'none' ? { clear } : null, marks());
        }
        return;
      }
      case 'w:cr':
        this.#emitNode('softBreak', null, marks());
        return;
      case 'w:noBreakHyphen':
        this.#emitText('‑', marks());
        return;
      case 'w:softHyphen':
        this.#emitText('­', marks());
        return;
      case 'w:sym': {
        const font = attr(c, 'w:font') ?? '';
        const ch = attr(c, 'w:char') ?? '';
        const symProps: DocxRunProps = { ...props, sym: { font, char: ch } };
        if (!symbolToUnicode(font, ch) && font) symProps.fonts = { ...props.fonts, ascii: font, hAnsi: font };
        this.#emitText(symbolText(font, ch), this.#runMarks(symProps, change, ctxMarks));
        return;
      }
      case 'w:fldChar':
        this.#fieldChar(c, marks());
        return;
      case 'w:instrText':
      case 'w:delInstrText': {
        const top = this.fields[this.fields.length - 1];
        if (top && top.phase === 'instr') top.instr += xmlText(c);
        return;
      }
      case 'w:footnoteReference':
      case 'w:endnoteReference': {
        const kind = c.name === 'w:footnoteReference' ? 'footnote' : 'endnote';
        const id = attr(c, 'w:id') ?? '';
        this.ctx.referencedNotes[kind].add(id);
        const node = this.schema.nodes.footnoteRef?.create({ id, kind }, null, marks());
        if (!node) return;
        if (parseOnOff(attr(c, 'w:customMarkFollows'), false)) {
          this.#customMarkPending = node;
          return;
        }
        this.#noteComments(node);
        this.#emit(node);
        return;
      }
      case 'w:footnoteRef':
      case 'w:endnoteRef':
        if (this.note) this.#emitNode('footnoteRef', { id: this.note.id, kind: this.note.kind }, marks());
        return;
      case 'w:separator':
      case 'w:continuationSeparator':
      case 'w:annotationRef':
      case 'w:lastRenderedPageBreak':
        return;
      case 'w:commentReference': {
        const id = attr(c, 'w:id');
        if (id !== undefined) this.ctx.referencedComments.add(id);
        return;
      }
      case 'w:pgNum':
        this.#emitNode('field', { type: 'PAGE', instr: 'PAGE', result: '' }, marks());
        return;
      case 'w:drawing': {
        const node = readDrawing(this, c, marks());
        if (node) this.emitInline(node);
        return;
      }
      case 'w:pict': {
        const node = readPict(this, c, marks());
        if (node) this.emitInline(node);
        return;
      }
      case 'w:object': {
        const node = readObject(this, c, marks());
        if (node) this.emitInline(node);
        return;
      }
      case 'w:contentPart':
        this.warn('drawing.ink', 'info', 'Ink is kept in the file but cannot be edited.');
        this.#emitNode('opaqueInline', { xml: this.raw(c), label: 'Ink' }, marks());
        return;
      case 'mc:AlternateContent': {
        const chosen = selectAlternateContent(c, SUPPORTED_MC);
        for (const k of chosen) {
          const before = this.#out.length;
          const beforeField = this.fields[this.fields.length - 1]?.result.length ?? 0;
          this.#readRunChild(k, props, change, ctxMarks, marks);
          // A placeholder for something we cannot edit keeps the whole AlternateContent (with its
          // fallback) so Word gets back exactly what it wrote.
          this.#rewrapOpaque(c, before, beforeField);
        }
        return;
      }
      case 'w:ruby': {
        this.warn(
          'element.unknown',
          'warning',
          'Phonetic guide (ruby) text was dropped; the base text is kept.',
        );
        const base = child(c, 'w:rubyBase');
        if (base) this.#readInlineChildren(base, ctxMarks);
        return;
      }
      default:
        this.warn('element.unknown', 'warning', `Unsupported run content ${c.name} was skipped.`);
    }
  }

  #rewrapOpaque(ac: XmlElement, before: number, beforeField: number): void {
    const top = this.fields[this.fields.length - 1];
    const list = top && top.phase === 'result' && top.mode === 'collect' ? top.result : this.#out;
    const start = list === this.#out ? before : beforeField;
    for (let i = start; i < list.length; i++) {
      const n = list[i] as PMNode;
      if (n.type.name === 'opaqueInline' || n.type.name === 'shape') {
        list[i] = n.type.create({ ...n.attrs, xml: this.raw(ac) }, null, n.marks);
      }
    }
  }

  // ---------------------------------------------------------------- fields

  #fieldChar(c: XmlElement, marks: readonly Mark[]): void {
    const type = attr(c, 'w:fldCharType');
    if (type === 'begin') {
      const parent = this.fields[this.fields.length - 1];
      this.fields.push({
        instr: '',
        phase: 'instr',
        mode: 'collect',
        result: [],
        marks,
        dirty: parseOnOff(attr(c, 'w:dirty'), false),
        locked: parseOnOff(attr(c, 'w:fldLock'), false),
        formField: child(c, 'w:ffData') !== undefined,
        nestedInInstr: parent?.phase === 'instr',
        path: this.path.toString(),
      });
      return;
    }
    const top = this.fields[this.fields.length - 1];
    if (!top) return;
    if (type === 'separate') {
      if (top.phase === 'result') return;
      top.phase = 'result';
      this.#chooseMode(top);
      return;
    }
    if (type === 'end') {
      this.fields.pop();
      this.#endField(top);
    }
  }

  #chooseMode(f: FieldFrame): void {
    const kw = fieldKeyword(f.instr);
    if (f.nestedInInstr) return; // result is part of the parent's code
    if (kw === 'HYPERLINK') {
      const parsed = parseFieldInstruction(f.instr.trim()) as HyperlinkField;
      const link = this.schema.marks.link?.create({
        href: parsed.url ?? null,
        anchor: parsed.anchor ?? null,
        tooltip: parsed.tooltip ?? null,
        target: parsed.target ?? null,
      });
      if (link && (parsed.url || parsed.anchor)) {
        f.mode = 'link';
        f.link = link;
        return;
      }
    }
    if (BLOCK_FIELDS.has(kw) && this.fields.length === 1) {
      f.mode = 'block';
      this.#tocStart = { instr: f.instr.trim(), dirty: f.dirty };
    }
  }

  #endField(f: FieldFrame): void {
    const instr = f.instr.trim();
    const parent = this.fields[this.fields.length - 1];
    if (f.nestedInInstr && parent && parent.phase === 'instr') {
      parent.instr += `{ ${instr} }`;
      return;
    }
    if (f.phase === 'instr') {
      // A field without a separator has no cached result (e.g. a hidden TC/XE entry).
      f.phase = 'result';
    }
    switch (f.mode) {
      case 'block':
        this.#tocEnd = true;
        return;
      case 'link':
      case 'flat':
        return;
      default:
        break;
    }
    const kw = fieldKeyword(instr);
    if (kw === 'HYPERLINK' || BLOCK_FIELDS.has(kw)) {
      // HYPERLINK without a usable target, or a generated table nested in another field: keep the
      // result as it is shown.
      for (const n of f.result) this.#emit(n);
      return;
    }
    const nonText = f.result.find((n) => !TEXTUAL.has(n.type.name) && n.type.name !== 'field');
    if (nonText) {
      for (const n of f.result) this.#emit(n);
      this.ctx.warn({
        code: 'field.unwrapped',
        severity: 'warning',
        message: `The result of the field "${instr}" contains ${nonText.type.name === 'image' ? 'a picture' : 'an object'}; it was kept and the field code removed.`,
        part: this.partName,
        path: f.path,
      });
      return;
    }
    if (f.formField) {
      this.ctx.warn({
        code: 'field.formField',
        severity: 'warning',
        message: `Legacy form field ${kw} was converted to a plain field; its form settings were dropped.`,
        part: this.partName,
        path: f.path,
      });
    }
    const result = f.result.map(leafText).join('');
    const firstText = f.result.find((n) => n.isText);
    const marks = firstText ? firstText.marks : f.marks;
    this.#emitNode('field', { type: kw, instr, result, dirty: f.dirty, locked: f.locked }, marks);
  }

  #simpleField(el: XmlElement, ctxMarks: readonly Mark[]): void {
    const instr = attr(el, 'w:instr') ?? '';
    const frame: FieldFrame = {
      instr,
      phase: 'result',
      mode: 'collect',
      result: [],
      marks: this.#withMarks(ctxMarks, this.#ambientMarks()),
      dirty: parseOnOff(attr(el, 'w:dirty'), false),
      locked: parseOnOff(attr(el, 'w:fldLock'), false),
      formField: false,
      nestedInInstr: false,
      path: this.path.toString(),
    };
    this.fields.push(frame);
    if (fieldKeyword(instr) === 'HYPERLINK') this.#chooseMode(frame);
    this.#readInlineChildren(el, ctxMarks);
    // A simple field never spans paragraphs; pop it whatever happened inside.
    const idx = this.fields.lastIndexOf(frame);
    if (idx >= 0) this.fields.splice(idx);
    this.#endField(frame);
  }
}

export function fieldKeyword(instr: string): string {
  const t = instr.trim();
  if (t.startsWith('=')) return '=';
  const m = /^[^\s\\"{]+/.exec(t);
  return m ? m[0].toUpperCase() : '';
}

function leafText(n: PMNode): string {
  if (n.isText) return n.text ?? '';
  const lt = n.type.spec.leafText;
  return lt ? lt(n) : '';
}

export type { RowRevision };
