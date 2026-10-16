/**
 * Secondary stories: headers and footers, footnotes and endnotes, comments (with Word 2013+ threads
 * from `commentsExtended.xml`) and comment authors (`people.xml`).
 */
import { type DocComment, toPlainText } from '@kiln-ref/doc-model';
import { child, children, RelType, type XmlElement } from '@kiln-ref/ooxml';
import type { Node as PMNode } from '@tiptap/pm/model';
import type { DocxNote, DocxPerson } from '../common/types.ts';
import { parseOnOff } from '../common/units.ts';
import { attr, stepped } from '../common/xml.ts';
import type { DocxReadContext } from './context.ts';
import { BlockSink, StoryReader } from './story.ts';

function ensureBlocks(ctx: DocxReadContext, blocks: PMNode[]): PMNode[] {
  if (blocks.length === 0) blocks.push(ctx.schema.nodes.paragraph?.create() as PMNode);
  return blocks;
}

/** Blocks of a header or footer part (cached: several sections may reference one part). */
export function readHeaderFooterPart(
  ctx: DocxReadContext,
  part: string,
  cache: Map<string, PMNode[]>,
): PMNode[] | undefined {
  const cached = cache.get(part);
  if (cached) return cached;
  const loaded = ctx.loadPart(part);
  if (!loaded) return undefined;
  ctx.consume(part);
  const rootStep = loaded.root.name;
  const story = new StoryReader(ctx, { partName: part, rootAttrs: loaded.root.attrs, rootStep });
  const sink = new BlockSink(ctx.schema);
  story.readBlocks(loaded.root, sink, []);
  const blocks = sink.blocks();
  story.finishStory(blocks);
  const out = ensureBlocks(ctx, blocks);
  cache.set(part, out);
  return out;
}

const SEPARATOR_TYPES = new Set(['separator', 'continuationSeparator', 'continuationNotice']);

/** Footnotes or endnotes part → note nodes in file order (separator notes are skipped). */
export function readNotes(ctx: DocxReadContext, mainPart: string, kind: 'footnote' | 'endnote'): DocxNote[] {
  const part = ctx.pkg.relatedPart(mainPart, kind === 'footnote' ? RelType.footnotes : RelType.endnotes);
  if (!part) return [];
  const loaded = ctx.loadPart(part);
  if (!loaded) return [];
  ctx.consume(part);
  const out: DocxNote[] = [];
  const noteName = kind === 'footnote' ? 'w:footnote' : 'w:endnote';
  for (const [el, step] of stepped(loaded.root)) {
    if (el.name !== noteName) continue;
    const type = attr(el, 'w:type');
    if (type && SEPARATOR_TYPES.has(type)) continue;
    const id = attr(el, 'w:id') ?? '';
    const story = new StoryReader(ctx, {
      partName: part,
      rootAttrs: loaded.root.attrs,
      rootStep: loaded.root.name,
      note: { id, kind },
    });
    story.path.push(step);
    const sink = new BlockSink(ctx.schema);
    story.readBlocks(el, sink, []);
    const blocks = sink.blocks();
    story.finishStory(blocks);
    const node = ctx.schema.node('footnote', { id, kind }, ensureBlocks(ctx, blocks));
    out.push({ id, kind, node });
  }
  return out;
}

/** The `w14:paraId` of a comment's last paragraph (the key `commentsExtended.xml` uses). */
function lastParaId(comment: XmlElement): string | undefined {
  let id: string | undefined;
  const walk = (n: XmlElement) => {
    for (const c of children(n)) {
      if (c.name === 'w:p') {
        const p = attr(c, 'w14:paraId');
        if (p) id = p;
      } else walk(c);
    }
  };
  walk(comment);
  return id;
}

/** `comments.xml` (+ `commentsExtended.xml` threads and resolved state). */
export function readComments(ctx: DocxReadContext, mainPart: string): DocComment[] {
  const part = ctx.pkg.relatedPart(mainPart, RelType.comments);
  if (!part) return [];
  const loaded = ctx.loadPart(part);
  if (!loaded) return [];
  ctx.consume(part);

  // Threads: paraId → { parent paraId, done }.
  const ext = new Map<string, { parent?: string; done: boolean }>();
  const extPart = ctx.pkg.relatedPart(mainPart, RelType.commentsExtended);
  const extLoaded = extPart ? ctx.loadPart(extPart) : undefined;
  if (extPart && extLoaded) {
    ctx.consume(extPart);
    for (const ce of children(extLoaded.root, 'w15:commentEx')) {
      const paraId = attr(ce, 'w15:paraId');
      if (!paraId) continue;
      const parent = attr(ce, 'w15:paraIdParent');
      ext.set(paraId, { ...(parent ? { parent } : {}), done: parseOnOff(attr(ce, 'w15:done'), false) });
    }
  }

  const comments: (DocComment & { paraId?: string })[] = [];
  for (const [el, step] of stepped(loaded.root)) {
    if (el.name !== 'w:comment') continue;
    const id = attr(el, 'w:id') ?? '';
    const story = new StoryReader(ctx, {
      partName: part,
      rootAttrs: loaded.root.attrs,
      rootStep: loaded.root.name,
    });
    story.path.push(step);
    const sink = new BlockSink(ctx.schema);
    story.readBlocks(el, sink, []);
    const blocks = sink.blocks();
    story.finishStory(blocks);
    const body = ensureBlocks(ctx, blocks);
    const comment: DocComment & { paraId?: string } = {
      id,
      author: attr(el, 'w:author') ?? '',
      date: attr(el, 'w:date') ?? '',
      text: body.map((b) => toPlainText(b)).join('\n'),
      content: body.map((b) => b.toJSON()),
    };
    const initials = attr(el, 'w:initials');
    if (initials) comment.initials = initials;
    const paraId = lastParaId(el);
    if (paraId) comment.paraId = paraId;
    comments.push(comment);
  }

  const byParaId = new Map<string, string>();
  for (const c of comments) if (c.paraId) byParaId.set(c.paraId, c.id);
  return comments.map(({ paraId, ...c }) => {
    const e = paraId ? ext.get(paraId) : undefined;
    if (e?.parent) {
      const parentId = byParaId.get(e.parent);
      if (parentId !== undefined && parentId !== c.id) c.parentId = parentId;
    }
    if (e?.done) c.resolved = true;
    return c;
  });
}

/** `people.xml`: comment authors with their identity provider. */
export function readPeople(ctx: DocxReadContext, mainPart: string): DocxPerson[] {
  const part = ctx.pkg.relatedPart(mainPart, RelType.people);
  if (!part) return [];
  const loaded = ctx.loadPart(part);
  if (!loaded) return [];
  ctx.consume(part);
  return children(loaded.root, 'w15:person').map((p) => {
    const info = child(p, 'w15:presenceInfo');
    const person: DocxPerson = { author: attr(p, 'w15:author') ?? '' };
    const providerId = attr(info, 'w15:providerId');
    const userId = attr(info, 'w15:userId');
    if (providerId) person.providerId = providerId;
    if (userId) person.userId = userId;
    return person;
  });
}
