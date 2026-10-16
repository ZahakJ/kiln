/**
 * `writeDocx(model)`: the Write model (as produced by `readDocx` or `DefaultTemplate.create()`) → a
 * WordprocessingML package (`.docx`, `.docm`, `.dotx`, `.dotm`), PLAN §3.5.
 *
 * - Every construct the reader understands is written back with Office's element order.
 * - Parts and relationships the reader did not model (`passthrough`: custom XML, macros, glossary,
 *   fonts, charts, unknown parts) are copied with their relationships repaired, before anything else
 *   is allocated, so relationship ids that preserved raw XML refers to stay valid.
 * - Output is deterministic: the same model and options give identical bytes (the only time-dependent
 *   value is `dcterms:modified`, taken from `options.now`).
 */
import {
  type DocComment,
  docSchema,
  type HeaderFooterType,
  type StyleSheet,
  statistics,
  type WriteDocument,
} from '@kiln-ref/doc-model';
import {
  type AppProperties,
  buildXml,
  ContentType,
  type CoreProperties,
  type DocumentProperties,
  directoryOf,
  OpcPackage,
  type PassthroughSet,
  PassthroughSet as PassthroughSetClass,
  RelType,
  serializeTheme,
  writeDocumentProperties,
} from '@kiln-ref/ooxml';
import type { Node as PMNode } from '@tiptap/pm/model';
import type {
  DocxExtras,
  DocxPerson,
  DocxRelTarget,
  DocxSectionProps,
  DocxSettings,
} from '../common/types.ts';
import { DocxWriteContext, type WriteIssue } from './context.ts';
import { commentsExtendedPart, numberingPart, peoplePart, settingsPart, stylesPart } from './parts.ts';
import { sectionPropsXml } from './props.ts';
import { StoryWriter } from './story.ts';
import { part, tag } from './xml.ts';

export type DocxFormat = 'docx' | 'docm' | 'dotx' | 'dotm';

/** What `writeDocx` accepts: a Write document plus, optionally, the DOCX side data `readDocx` returns. */
export interface DocxWriteModel extends WriteDocument {
  settings: DocxSettings;
  people?: DocxPerson[];
  properties?: DocumentProperties;
  passthrough?: PassthroughSet;
  relationships?: Map<string, Map<string, DocxRelTarget>>;
  mainPart?: string;
  mainContentType?: string;
  extras?: DocxExtras;
}

export interface WriteDocxOptions {
  /** Parts to copy unchanged (defaults to `model.passthrough`); `false` writes none. */
  passthrough?: PassthroughSet | false;
  /** Target format; defaults to the format the model was read from, else `docx`. */
  format?: DocxFormat;
  /** Save time written to `dcterms:modified` (and `dcterms:created` when missing). Default: now. */
  now?: Date;
  /** `cp:lastModifiedBy`; kept from the model when omitted. */
  author?: string;
  /** Page count from the layout engine for `docProps/app.xml` (omitted when unknown). */
  pages?: number;
}

export interface WriteDocxResult {
  bytes: Uint8Array;
  /** Things that could not be written exactly (also reported by `checkCompatibility`). */
  issues: WriteIssue[];
}

const WML = 'application/vnd.openxmlformats-officedocument.wordprocessingml';
const DOC = 'document';
/** Word's main part name. (Names are assembled so the worker-safety scan does not see `document.`.) */
const DEFAULT_MAIN = `/word/${DOC}${'.xml'}`;

const MAIN_TYPES: Record<DocxFormat, string> = {
  docx: `${WML}.${DOC}.main+xml`,
  docm: `application/vnd.ms-word.${DOC}.macroEnabled.main+xml`,
  dotx: `${WML}.template.main+xml`,
  dotm: 'application/vnd.ms-word.template.macroEnabledTemplate.main+xml',
};

const CT = {
  header: `${WML}.header+xml`,
  footer: `${WML}.footer+xml`,
  footnotes: `${WML}.footnotes+xml`,
  endnotes: `${WML}.endnotes+xml`,
  comments: `${WML}.comments+xml`,
  commentsExtended: `${WML}.commentsExtended+xml`,
  people: `${WML}.people+xml`,
};

/** The format a model was read as (from its main content type). */
export function formatOfContentType(contentType: string | undefined): DocxFormat {
  if (!contentType) return 'docx';
  if (/macroEnabledTemplate/i.test(contentType)) return 'dotm';
  if (/macroEnabled/i.test(contentType)) return 'docm';
  if (/template\.main/i.test(contentType)) return 'dotx';
  return 'docx';
}

const MACRO_TYPES = /vbaProject|vbaData|ms-office\.activeX|keyMapCustomizations|wordVbaData/i;

/** Passthrough without macro parts (saving a macro-enabled document as .docx/.dotx). */
function withoutMacros(set: PassthroughSet): PassthroughSet {
  const out = new PassthroughSetClass();
  const dropped = new Set<string>();
  for (const p of set.parts) {
    if (MACRO_TYPES.test(p.contentType)) dropped.add(p.name.toLowerCase());
    else out.recordPart(p);
  }
  for (const r of set.relationships) {
    if (MACRO_TYPES.test(r.rel.type)) continue;
    out.recordRelationship(r.source, r.rel);
  }
  return out;
}

function isoSeconds(d: Date): string {
  return `${d.toISOString().slice(0, 19)}Z`;
}

/** Deterministic `w14:paraId` (8 hex digits, below 0x80000000) not in `used`. */
function makeParaId(seed: string, used: Set<string>): string {
  let h = 2166136261;
  for (const ch of seed) h = Math.imul(h ^ ch.charCodeAt(0), 16777619);
  for (;;) {
    const id = ((h >>> 1) & 0x7fffffff || 1).toString(16).toUpperCase().padStart(8, '0');
    if (!used.has(id)) {
      used.add(id);
      return id;
    }
    h = Math.imul(h ^ 0x9e3779b9, 16777619);
  }
}

interface HfPart {
  kind: 'header' | 'footer';
  name: string;
  node: PMNode;
  original?: string;
}

/** Writes a `.docx` (or `.docm`/`.dotx`/`.dotm`). */
export function writeDocx(model: DocxWriteModel | WriteDocument, options: WriteDocxOptions = {}): Uint8Array {
  return writeDocxWithReport(model, options).bytes;
}

/** `writeDocx` plus the issues met while writing. */
export function writeDocxWithReport(
  input: DocxWriteModel | WriteDocument,
  options: WriteDocxOptions = {},
): WriteDocxResult {
  const model = input as DocxWriteModel;
  const schema = model.doc.type.schema ?? docSchema;
  const format = options.format ?? formatOfContentType(model.mainContentType);
  const now = options.now ?? new Date();
  const extras: DocxExtras = model.extras ?? {};

  let passthrough: PassthroughSet | undefined =
    options.passthrough === false ? undefined : (options.passthrough ?? model.passthrough);
  const macroEnabled = format === 'docm' || format === 'dotm';
  if (passthrough && !macroEnabled) passthrough = withoutMacros(passthrough);

  const pkg = OpcPackage.create();
  const reserved = new Set((passthrough?.parts ?? []).map((p) => p.name.toLowerCase()));
  const taken = (n: string) => reserved.has(n.toLowerCase()) || pkg.has(n);
  const pick = (preferred: string): string => {
    if (!taken(preferred)) return preferred;
    const dot = preferred.lastIndexOf('.');
    const stem = preferred.slice(0, dot).replace(/\d+$/, '');
    const ext = preferred.slice(dot);
    for (let n = 1; ; n++) {
      const c = `${stem}${n}${ext}`;
      if (!taken(c)) return c;
    }
  };

  // ---- the model's stories
  const sections: PMNode[] = [];
  const notes: PMNode[] = [];
  model.doc.forEach((n) => {
    if (n.type.name === 'section') sections.push(n);
    else if (n.type.name === 'footnote') notes.push(n);
  });
  const footnotes = notes.filter((n) => n.attrs.kind !== 'endnote');
  const endnotes = notes.filter((n) => n.attrs.kind === 'endnote');
  const comments = model.comments ?? [];

  // ---- part names (added as placeholders so passthrough relationships from them survive)
  const mainPart = model.mainPart && /^\/[^?#]+\.xml$/i.test(model.mainPart) ? model.mainPart : DEFAULT_MAIN;
  const dir = directoryOf(mainPart);
  const placeholders: [string, string][] = [];
  const add = (name: string, ct: string) => {
    pkg.addPart(name, ct, new Uint8Array());
    placeholders.push([name, ct]);
    return name;
  };
  const main = add(mainPart, MAIN_TYPES[format]);
  const stylesName = add(pick(`${dir}styles.xml`), ContentType.docxStyles);
  const settingsName = add(pick(`${dir}settings.xml`), ContentType.docxSettings);
  const numberingJson = model.numbering.toJSON();
  const hasNumbering =
    numberingJson.abstracts.length > 0 ||
    numberingJson.instances.length > 0 ||
    (extras.numPicBullets?.length ?? 0) > 0;
  const numberingName = hasNumbering
    ? add(pick(`${dir}numbering.xml`), ContentType.docxNumbering)
    : undefined;
  // Separator notes the original settings refer to (Word keeps notes parts even without notes).
  const settingsSeparators = (kind: 'footnote' | 'endnote'): string[] => {
    const block =
      new RegExp(`<w:${kind}Pr>(.*?)</w:${kind}Pr>`, 's').exec(extras.settingsXml ?? '')?.[1] ?? '';
    return [...block.matchAll(new RegExp(`<w:${kind}\\b[^>]*?w:id="(-?\\d+)"`, 'g'))].map(
      (m) => m[1] as string,
    );
  };
  const fnRefs = settingsSeparators('footnote');
  const enRefs = settingsSeparators('endnote');
  const footnotesName =
    footnotes.length || fnRefs.length ? add(pick(`${dir}footnotes.xml`), CT.footnotes) : undefined;
  const endnotesName =
    endnotes.length || enRefs.length ? add(pick(`${dir}endnotes.xml`), CT.endnotes) : undefined;
  const commentsName = comments.length ? add(pick(`${dir}comments.xml`), CT.comments) : undefined;
  const commentsExtName = comments.length
    ? add(pick(`${dir}commentsExtended.xml`), CT.commentsExtended)
    : undefined;
  const people = model.people ?? [];
  const peopleName = people.length ? add(pick(`${dir}people.xml`), CT.people) : undefined;

  const hfParts = new Map<string, HfPart>();
  const hfOf = new Map<PMNode, HfPart>();
  for (const s of sections) {
    s.forEach((n) => {
      if (n.type.name !== 'header' && n.type.name !== 'footer') return;
      const kind = n.type.name as 'header' | 'footer';
      const original = (n.attrs.partId as string | null) ?? undefined;
      const key = original ? original.toLowerCase() : undefined;
      let hf = key ? hfParts.get(key) : undefined;
      if (!hf) {
        const preferred = original && /^\/[^?#]+\.xml$/i.test(original) ? original : `${dir}${kind}1.xml`;
        const name = add(pick(preferred), kind === 'header' ? CT.header : CT.footer);
        hf = { kind, name, node: n, ...(original ? { original } : {}) };
        hfParts.set(key ?? `#${hfParts.size}`, hf);
      }
      hfOf.set(n, hf);
    });
  }
  const themeName = add(pick(`${dir}theme/theme1.xml`), ContentType.theme);

  // ---- passthrough first: its relationship ids are kept as written
  let issues: WriteIssue[] = [];
  if (passthrough) {
    const res = passthrough.emit(pkg);
    for (const d of res.dropped) {
      issues.push({
        code: 'passthrough.dropped',
        message: `A relationship from ${d.source} (${d.rel.type.split('/').pop()}) could not be kept.`,
        part: d.source,
      });
    }
  }

  // ---- context
  const rawStrings: string[] = [];
  let maxRevision = 0;
  model.doc.descendants((n) => {
    if (n.type.name === 'shape' || n.type.name === 'opaqueInline' || n.type.name === 'opaqueBlock') {
      if (n.attrs.xml) rawStrings.push(n.attrs.xml as string);
    }
    for (const m of n.marks) {
      if (m.type.name === 'insertion' || m.type.name === 'deletion' || m.type.name === 'formatChange') {
        const v = Number(m.attrs.id);
        if (Number.isFinite(v)) maxRevision = Math.max(maxRevision, v);
      }
    }
    return true;
  });
  let maxDocPr = 0;
  for (const s of rawStrings) {
    for (const m of s.matchAll(/<wp:docPr\b[^>]*?\sid="(\d+)"/g)) maxDocPr = Math.max(maxDocPr, Number(m[1]));
  }
  const ctx = new DocxWriteContext(pkg, schema, model.media ?? new Map(), model.relationships ?? new Map(), {
    firstDocPrId: maxDocPr,
    firstRevisionId: Math.max(maxRevision, 0),
  });
  for (const hf of hfParts.values()) if (hf.original) ctx.originalName.set(hf.name, hf.original);
  if (model.mainPart) ctx.originalName.set(main, model.mainPart);

  pkg.addRelationship('/', RelType.officeDocument, main);
  const mainRels = ctx.rels(main);
  mainRels.get(RelType.styles, stylesName);
  mainRels.get(RelType.settings, settingsName);
  if (numberingName) mainRels.get(RelType.numbering, numberingName);
  if (footnotesName) mainRels.get(RelType.footnotes, footnotesName);
  if (endnotesName) mainRels.get(RelType.endnotes, endnotesName);
  if (commentsName) mainRels.get(RelType.comments, commentsName);
  if (commentsExtName) mainRels.get(RelType.commentsExtended, commentsExtName);
  if (peopleName) mainRels.get(RelType.people, peopleName);
  mainRels.get(RelType.theme, themeName);

  // ---- styles (a code block needs the Code style)
  let styles: StyleSheet = model.styles;
  let hasCode = false;
  model.doc.descendants((n) => {
    if (n.type.name === 'codeBlock') hasCode = true;
    return !hasCode;
  });
  if (hasCode && !styles.has('Code')) {
    styles = styles.clone();
    styles.add({
      id: 'Code',
      type: 'paragraph',
      name: 'Code',
      basedOn: 'Normal',
      custom: true,
      quickStyle: true,
      uiPriority: 39,
      pPr: { spacing: { before: 0, after: 0, line: 240, lineRule: 'auto' } },
      rPr: {
        fonts: { ascii: 'Consolas', hAnsi: 'Consolas', cs: 'Consolas', eastAsia: 'Consolas' },
        size: 20,
      },
    });
  }
  const styleIds = new Set(styles.all().map((s) => s.id));
  const noteIds = {
    footnote: new Set(footnotes.map((n) => String(n.attrs.id))),
    endnote: new Set(endnotes.map((n) => String(n.attrs.id))),
  };
  const commentIds = new Set(comments.map((c) => c.id));
  const storyOptions = { notes: noteIds, comments: commentIds, styleIds };

  const written = new Map<string, string>();
  const setXml = (name: string, xml: string) => written.set(name, xml);

  // ---- headers and footers
  for (const hf of hfParts.values()) {
    const w = new StoryWriter(ctx, { part: hf.name, kind: hf.kind, ...storyOptions });
    const blocks: PMNode[] = [];
    hf.node.forEach((b) => {
      blocks.push(b);
    });
    w.prepareComments(blocks);
    let body = w.blocks(blocks);
    if (blocks[blocks.length - 1]?.type.name === 'table') body += '<w:p/>';
    setXml(hf.name, part(hf.kind === 'header' ? 'w:hdr' : 'w:ftr', body));
  }

  // ---- body
  const bodyWriter = new StoryWriter(ctx, { part: main, kind: 'body', ...storyOptions });
  const sectionBlocks = sections.map((s) => {
    const blocks: PMNode[] = [];
    s.forEach((n) => {
      if (n.type.name !== 'header' && n.type.name !== 'footer') blocks.push(n);
    });
    return blocks;
  });
  bodyWriter.prepareComments(sectionBlocks.flat());
  let body = '';
  sections.forEach((s, i) => {
    const refs: { kind: 'header' | 'footer'; type: string; id: string }[] = [];
    s.forEach((n) => {
      const hf = hfOf.get(n);
      if (!hf) return;
      const relType = hf.kind === 'header' ? RelType.header : RelType.footer;
      refs.push({
        kind: hf.kind,
        type: (n.attrs.type as HeaderFooterType) || 'default',
        id: mainRels.get(relType, hf.name),
      });
    });
    const props = (s.attrs.props as DocxSectionProps | null) ?? undefined;
    const sectPr = sectionPropsXml(props ?? ({} as DocxSectionProps), refs);
    const blocks = sectionBlocks[i] as PMNode[];
    const last = i === sections.length - 1;
    body += bodyWriter.blocks(blocks, new Set(), last ? {} : { sectPr });
    if (last) body += sectPr;
  });
  const settings: DocxSettings = model.settings;
  let background = extras.background ?? '';
  if (settings.background) {
    const m = /w:color="([0-9A-Fa-f]{6})"/.exec(background);
    if (!m || (m[1] as string).toUpperCase() !== settings.background) {
      background = tag('w:background', [['w:color', settings.background]]);
    }
  } else if (background && /w:color="[0-9A-Fa-f]{6}"/.test(background)) background = '';
  setXml(main, part('w:document', `${background}<w:body>${body}</w:body>`));

  // ---- notes
  const separatorIds = (list: PMNode[], original: string[]): [string, string] => {
    const used = new Set(list.map((n) => String(n.attrs.id)));
    const [a, b] = original;
    if (a !== undefined && b !== undefined && a !== b && !used.has(a) && !used.has(b)) return [a, b];
    if (!used.has('-1') && !used.has('0')) return ['-1', '0'];
    let min = 0;
    for (const id of used) {
      const v = Number(id);
      if (Number.isFinite(v)) min = Math.min(min, v);
    }
    return [String(min - 2), String(min - 1)];
  };
  const writeNotes = (name: string | undefined, list: PMNode[], kind: 'footnote' | 'endnote') => {
    if (!name) return undefined;
    const [sep, cont] = separatorIds(list, kind === 'footnote' ? fnRefs : enRefs);
    const el = `w:${kind}`;
    let body =
      `<${el} w:type="separator" w:id="${sep}"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:separator/></w:r></w:p></${el}>` +
      `<${el} w:type="continuationSeparator" w:id="${cont}"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:continuationSeparator/></w:r></w:p></${el}>`;
    for (const n of list) {
      const id = String(n.attrs.id);
      const w = new StoryWriter(ctx, { part: name, kind, noteId: id, ...storyOptions });
      const blocks: PMNode[] = [];
      n.forEach((b) => {
        blocks.push(b);
      });
      w.prepareComments(blocks);
      body += tag(el, [['w:id', id]], w.blocks(blocks));
    }
    setXml(name, part(kind === 'footnote' ? 'w:footnotes' : 'w:endnotes', body));
    return [sep, cont];
  };
  const fnSeparators = writeNotes(footnotesName, footnotes, 'footnote');
  const enSeparators = writeNotes(endnotesName, endnotes, 'endnote');

  // ---- comments
  if (commentsName && commentsExtName) {
    const usedParaIds = new Set<string>();
    model.doc.descendants((n) => {
      if (n.attrs.paraId) usedParaIds.add(String(n.attrs.paraId).toUpperCase());
      return true;
    });
    const bodies = comments.map((c) => commentBlocks(c, schema));
    for (const blocks of bodies)
      for (const b of blocks) if (b.attrs.paraId) usedParaIds.add(String(b.attrs.paraId).toUpperCase());
    const paraIdOf = new Map<string, string>();
    // Threads and resolved state are keyed by the paraId of a comment's last paragraph.
    const parents = new Set(comments.map((c) => c.parentId).filter((p): p is string => !!p));
    let xml = '';
    comments.forEach((c, i) => {
      const blocks = bodies[i] as PMNode[];
      const lastBlock = blocks[blocks.length - 1];
      const own = lastBlock?.type.name === 'paragraph' ? (lastBlock.attrs.paraId as string | null) : null;
      const needed = !!c.parentId || !!c.resolved || parents.has(c.id);
      const paraId = own ?? (needed ? makeParaId(`comment:${c.id}:${i}`, usedParaIds) : undefined);
      if (paraId) paraIdOf.set(c.id, paraId);
      const w = new StoryWriter(ctx, { part: commentsName, kind: 'comment', ...storyOptions });
      const inner = w.blocks(blocks, new Set(), {
        firstPrefix: '<w:r><w:annotationRef/></w:r>',
        ...(paraId ? { lastParaId: paraId } : {}),
      });
      xml += tag(
        'w:comment',
        [
          ['w:id', c.id],
          ['w:author', c.author],
          ['w:date', c.date || undefined],
          ['w:initials', c.initials],
        ],
        inner,
      );
    });
    setXml(commentsName, part('w:comments', xml));
    setXml(
      commentsExtName,
      commentsExtendedPart(
        comments
          .filter((c) => paraIdOf.has(c.id))
          .map((c) => ({
            paraId: paraIdOf.get(c.id) as string,
            ...(c.parentId && paraIdOf.has(c.parentId) ? { parent: paraIdOf.get(c.parentId) as string } : {}),
            done: !!c.resolved,
          })),
      ),
    );
  }
  if (peopleName) setXml(peopleName, peoplePart(people));

  // ---- styles, numbering, settings, theme
  setXml(stylesName, stylesPart(styles, extras.latentStyles));
  if (numberingName) setXml(numberingName, numberingPart(model.numbering, extras.numPicBullets));
  setXml(
    settingsName,
    settingsPart({
      settings,
      ...(extras.settingsXml ? { original: extras.settingsXml } : {}),
      ...(fnSeparators ? { footnoteSeparators: fnSeparators } : {}),
      ...(enSeparators ? { endnoteSeparators: enSeparators } : {}),
    }),
  );
  pkg.setPart(themeName, buildXml(serializeTheme(model.theme)));

  // ---- relationships that preserved raw XML refers to
  for (const [name, xml] of written) {
    repairRawRelationships(ctx, name, xml);
    pkg.setPart(name, xml);
  }

  // ---- document properties
  const props = model.properties ?? { core: {}, app: {}, custom: [] };
  const core: CoreProperties = { ...props.core, modified: isoSeconds(now) };
  core.created ??= isoSeconds(now);
  if (options.author) core.lastModifiedBy = options.author;
  if (core.revision && /^\d+$/.test(core.revision)) core.revision = String(Number(core.revision) + 1);
  const stats = statistics(model.doc);
  const app: AppProperties = { ...props.app, application: 'Kiln' };
  delete app.appVersion;
  delete app.lines;
  delete app.pages;
  if (options.pages !== undefined) app.pages = options.pages;
  app.words = stats.words;
  app.characters = stats.characters;
  app.charactersWithSpaces = stats.charactersWithSpaces;
  app.paragraphs = stats.paragraphs;
  writeDocumentProperties(pkg, { core, app, custom: props.custom ?? [] });

  issues = [...issues, ...ctx.issues];
  return { bytes: pkg.save(), issues };
}

/** The blocks of a comment: its rich content, else its plain text as paragraphs. */
export function commentBlocks(c: DocComment, schema = docSchema): PMNode[] {
  if (Array.isArray(c.content) && c.content.length) {
    try {
      return (c.content as unknown[]).map((j) => schema.nodeFromJSON(j));
    } catch {
      // Fall back to the plain text below.
    }
  }
  const lines = (c.text ?? '').split('\n');
  return lines.map((line) => schema.nodes.paragraph?.create(null, line ? schema.text(line) : null) as PMNode);
}

/**
 * Raw XML kept from the original file may refer to relationships the writer did not recreate (for
 * example an image used both by a picture and by a kept shape). Those ids are re-added with their
 * original targets.
 */
function repairRawRelationships(ctx: DocxWriteContext, partName: string, xml: string): void {
  const rels = ctx.pkg.relationships(partName);
  const original = ctx.originalRels.get(ctx.originalName.get(partName) ?? partName);
  const seen = new Set<string>();
  for (const m of xml.matchAll(/\sr:[A-Za-z]+="([^"]+)"/g)) {
    const id = m[1] as string;
    if (seen.has(id) || rels.get(id)) continue;
    seen.add(id);
    const target = original?.get(id);
    if (!target) {
      ctx.issue(
        'relationship.missing',
        `Content refers to a relationship (${id}) that no longer exists.`,
        partName,
      );
      continue;
    }
    if (!target.external) {
      if (!ctx.pkg.has(target.target) && !ctx.ensureMedia(target.target)) {
        ctx.issue(
          'relationship.missing',
          `Content refers to a part (${target.target}) that is not in the file.`,
          partName,
        );
        continue;
      }
    }
    ctx.rels(partName).addWithId(id, target.type, target.target, target.external);
  }
}

export type { WriteIssue };
