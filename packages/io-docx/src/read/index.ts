/**
 * `readDocx(bytes)`: a WordprocessingML package (`.docx`, `.docm`, `.dotx`, `.dotm`) → the Write model
 * plus everything a faithful writer needs (PLAN §3.5). Pure TypeScript without DOM or Node APIs, so it
 * runs unchanged in a Web Worker (`new Worker(…)` + `readDocx(await file.arrayBuffer())`).
 */
import { docSchema, type HeaderFooterType, Numbering, StyleSheet } from '@officina/doc-model';
import {
  child,
  type OpcLimits,
  OpcPackage,
  officeTheme,
  PassthroughSet,
  parseTheme,
  RelType,
  readDocumentProperties,
  type Theme,
} from '@officina/ooxml';
import type { Node as PMNode, Schema } from '@tiptap/pm/model';
import type { DocxSectionProps, ReadDocxResult } from '../common/types.ts';
import { attr, fragmentXml } from '../common/xml.ts';
import { DocxReadContext } from './context.ts';
import { parseNumbering } from './numbering.ts';
import { readComments, readHeaderFooterPart, readNotes, readPeople } from './parts.ts';
import { parseSectionProps } from './sections.ts';
import { parseSettings } from './settings.ts';
import { BlockSink, StoryReader } from './story.ts';
import { parseStyles } from './styles.ts';

export interface ReadDocxOptions {
  /** Schema to build the document with (defaults to doc-model's shared schema). */
  schema?: Schema;
  /** Zip limits (zip-bomb protection) passed to `OpcPackage.open`. */
  limits?: Partial<OpcLimits>;
}

/** The input is not a WordprocessingML package (or its main part is unusable). */
export class DocxFormatError extends Error {
  override name = 'DocxFormatError';
}

const WORD_MAIN =
  /wordprocessingml\.(document|template)\.main|ms-word\.(document|template)\.macroEnabled(Template)?\.main/;

/** Reads a `.docx`/`.docm`/`.dotx`/`.dotm` file. Throws `DocxFormatError` for other inputs. */
export function readDocx(bytes: Uint8Array | ArrayBuffer, options: ReadDocxOptions = {}): ReadDocxResult {
  const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  const pkg = OpcPackage.open(data, options.limits);
  const main = pkg.mainDocumentPart();
  if (!main) throw new DocxFormatError('The package has no main document part.');
  const mainContentType = pkg.getContentType(main) ?? '';
  if (!WORD_MAIN.test(mainContentType)) {
    throw new DocxFormatError(
      `The main part is not a Word document (${mainContentType || 'no content type'}).`,
    );
  }
  const schema = options.schema ?? docSchema;
  const ctx = new DocxReadContext(pkg, schema);
  const loaded = ctx.loadPart(main);
  if (loaded?.root.name !== 'w:document') {
    const why = ctx.warnings.list.find((w) => w.part === main)?.message;
    throw new DocxFormatError(why ?? 'The main document part is not a w:document.');
  }
  ctx.consume(main);
  const extras: ReadDocxResult['extras'] = { documentRootAttrs: { ...loaded.root.attrs } };

  // ---- theme, styles, numbering, settings
  let theme: Theme = officeTheme();
  const themePart = pkg.relatedPart(main, RelType.theme);
  const themeBytes = themePart ? pkg.getPart(themePart) : undefined;
  if (themePart && themeBytes) {
    try {
      theme = parseTheme(themeBytes);
      ctx.consume(themePart);
    } catch (e) {
      ctx.warn({
        code: 'part.invalidXml',
        severity: 'warning',
        message: `The theme could not be read (${(e as Error).message}); the Office theme is used instead.`,
        part: themePart,
        path: '/',
      });
    }
  }

  let styles = new StyleSheet();
  const stylesPart = pkg.relatedPart(main, RelType.styles);
  const stylesLoaded = stylesPart ? ctx.loadPart(stylesPart) : undefined;
  if (stylesPart && stylesLoaded) {
    const parsed = parseStyles(stylesLoaded.root, (n) => fragmentXml(n, stylesLoaded.root.attrs));
    styles = parsed.styles;
    if (parsed.latentStyles) extras.latentStyles = parsed.latentStyles;
    ctx.consume(stylesPart);
  }

  const numberingPart = pkg.relatedPart(main, RelType.numbering);
  const numberingLoaded = numberingPart ? ctx.loadPart(numberingPart) : undefined;
  const parsedNumbering = numberingLoaded
    ? parseNumbering(numberingLoaded.root, (n) => fragmentXml(n, numberingLoaded.root.attrs))
    : undefined;
  if (numberingPart && parsedNumbering) {
    ctx.consume(numberingPart);
    if (parsedNumbering.picBullets.length) extras.numPicBullets = parsedNumbering.picBullets;
  }
  const numbering = parsedNumbering?.numbering ?? new Numbering();
  numbering.listStyleResolver = (id) => styles.get(id)?.pPr?.numbering?.numId;

  const settingsPart = pkg.relatedPart(main, RelType.settings);
  const settingsLoaded = settingsPart ? ctx.loadPart(settingsPart) : undefined;
  const settings = parseSettings(settingsLoaded?.root);
  if (settingsPart && settingsLoaded) {
    ctx.consume(settingsPart);
    extras.settingsXml = fragmentXml(settingsLoaded.root, settingsLoaded.root.attrs);
  }

  const properties = readDocumentProperties(pkg);
  for (const t of [RelType.coreProperties, RelType.extendedProperties, RelType.customProperties]) {
    const p = pkg.relatedPart('/', t);
    if (p) ctx.consume(p);
  }

  // ---- body
  const root = loaded.root;
  const background = child(root, 'w:background');
  if (background) {
    extras.background = fragmentXml(background, root.attrs);
    const color = attr(background, 'w:color');
    if (color && /^[0-9a-fA-F]{6}$/.test(color)) settings.background = color.toUpperCase();
  }
  const body = child(root, 'w:body');
  const story = new StoryReader(ctx, { partName: main, rootAttrs: root.attrs, rootStep: 'w:document' });
  const sink = new BlockSink(schema);
  if (body) {
    story.path.push('w:body[1]');
    story.readBlocks(body, sink, []);
    story.path.pop();
  }
  sink.closeToc();
  const groups = sink.groups;
  const last = groups[groups.length - 1] as { blocks: PMNode[]; sectPr?: (typeof groups)[number]['sectPr'] };
  last.sectPr = child(body, 'w:sectPr');
  // A trailing empty group after a final paragraph-level section break has nothing to hold.
  if (groups.length > 1 && last.blocks.length === 0 && !last.sectPr) groups.pop();
  story.finishStory((groups[groups.length - 1] as { blocks: PMNode[] }).blocks);

  const hfCache = new Map<string, PMNode[]>();
  const rawDoc = (n: Parameters<typeof fragmentXml>[0]) => fragmentXml(n, root.attrs);
  const sections: DocxSectionProps[] = [];
  const sectionNodes: PMNode[] = [];
  for (const g of groups) {
    const parsed = parseSectionProps(g.sectPr, rawDoc);
    sections.push(parsed.props);
    const hf: PMNode[] = [];
    const addHf = (kind: 'header' | 'footer', refs: [HeaderFooterType, string][]) => {
      for (const [type, relId] of refs) {
        const rel = ctx.relationship(main, relId);
        if (!rel?.part) {
          ctx.warn({
            code: 'part.missing',
            severity: 'warning',
            message: `A ${kind} reference (${relId}) points to no part.`,
            part: main,
            path: '/w:document/w:body/w:sectPr',
          });
          continue;
        }
        const blocks = readHeaderFooterPart(ctx, rel.part, hfCache);
        if (blocks) hf.push(schema.node(kind, { type, partId: rel.part }, blocks));
      }
    };
    addHf('header', parsed.headers);
    addHf('footer', parsed.footers);
    const blocks = g.blocks.length ? g.blocks : [schema.nodes.paragraph?.create() as PMNode];
    sectionNodes.push(schema.node('section', { props: parsed.props }, [...hf, ...blocks]));
  }

  // ---- notes, comments, people
  const footnotes = readNotes(ctx, main, 'footnote');
  const endnotes = readNotes(ctx, main, 'endnote');
  const comments = readComments(ctx, main);
  const people = readPeople(ctx, main);
  checkReferences(ctx, main, footnotes, endnotes, comments);

  const doc = schema.node('doc', null, [
    ...sectionNodes,
    ...footnotes.map((n) => n.node),
    ...endnotes.map((n) => n.node),
  ]);

  const passthrough = PassthroughSet.collect(pkg, {
    consumedParts: ctx.consumedParts,
    consumedRelationships: ctx.consumedRels,
  });

  return {
    doc,
    styles,
    numbering,
    theme,
    settings,
    comments,
    media: ctx.media,
    sections,
    footnotes,
    endnotes,
    people,
    properties,
    passthrough,
    charts: ctx.charts,
    relationships: ctx.relationships,
    mainPart: main,
    mainContentType,
    extras,
    warnings: ctx.warnings.list,
  };
}

function checkReferences(
  ctx: DocxReadContext,
  main: string,
  footnotes: readonly { id: string }[],
  endnotes: readonly { id: string }[],
  comments: readonly { id: string }[],
): void {
  const notes = {
    footnote: new Set(footnotes.map((n) => n.id)),
    endnote: new Set(endnotes.map((n) => n.id)),
  };
  for (const kind of ['footnote', 'endnote'] as const) {
    for (const id of ctx.referencedNotes[kind]) {
      if (!notes[kind].has(id)) {
        ctx.warn({
          code: 'note.missing',
          severity: 'warning',
          message: `A ${kind} reference points to ${kind} ${id}, which the file does not contain.`,
          part: main,
          path: '/',
        });
      }
    }
  }
  const known = new Set(comments.map((c) => c.id));
  for (const id of ctx.referencedComments) {
    if (!known.has(id)) {
      ctx.warn({
        code: 'comment.missing',
        severity: 'warning',
        message: `A comment reference points to comment ${id}, which the file does not contain.`,
        part: main,
        path: '/',
      });
    }
  }
  for (const c of comments) {
    if (!ctx.anchoredComments.has(c.id)) {
      ctx.warn({
        code: 'comment.unanchored',
        severity: 'info',
        message: `Comment ${c.id} does not cover any text; it is listed in the Comments pane without a position.`,
        part: main,
        path: '/',
      });
    }
  }
}

/** Reads a `.docx` from a stream (e.g. `fetch(url).body` in a worker). The zip needs all bytes first. */
export async function readDocxStream(
  stream: ReadableStream<Uint8Array>,
  options: ReadDocxOptions = {},
): Promise<ReadDocxResult> {
  const reader = stream.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    size += value.byteLength;
  }
  const all = new Uint8Array(size);
  let offset = 0;
  for (const c of chunks) {
    all.set(c, offset);
    offset += c.byteLength;
  }
  return readDocx(all, options);
}
