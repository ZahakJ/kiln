/**
 * Test helpers: building small DOCX packages from XML snippets, and a plain-text view of a read
 * document that follows mammoth's text model (the independent oracle).
 */

import { ContentType, OpcPackage, RelType } from '@officina/ooxml';
import { readDocxWithMammoth } from '@officina/testing';
import type { Node as PMNode } from '@tiptap/pm/model';

export const W_NS = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main';

/** Namespace declarations used by the snippets in tests. */
export const NS_DECLS = [
  `xmlns:w="${W_NS}"`,
  'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"',
  'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"',
  'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"',
  'xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"',
  'xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"',
  'xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"',
  'xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"',
  'xmlns:v="urn:schemas-microsoft-com:vml"',
  'xmlns:o="urn:schemas-microsoft-com:office:office"',
  'xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"',
  'xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml"',
  'xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"',
].join(' ');

export interface ExtraPart {
  name: string;
  contentType: string;
  /** Relationship type from the main document (omit for parts reached otherwise). */
  relType?: string;
  /** Fixed relationship id from the main document. */
  relId?: string;
  content: string | Uint8Array;
}

export interface BuildOptions {
  /** `w:body` content (paragraphs, tables, …) without the body element. */
  body: string;
  styles?: string;
  numbering?: string;
  settings?: string;
  parts?: ExtraPart[];
  /** Extra relationships of the main part: [id, type, target, external?]. */
  rels?: [string, string, string, boolean?][];
}

const wrap = (root: string, inner: string) =>
  `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<${root} ${NS_DECLS}>${inner}</${root.split(' ')[0]}>`;

/** Builds a .docx from XML snippets. */
export function buildDocx(o: BuildOptions): Uint8Array {
  const pkg = OpcPackage.create();
  const main = '/word/document.xml';
  pkg.addPart(main, ContentType.docxDocument, wrap('w:document', `<w:body>${o.body}</w:body>`));
  pkg.addRelationship('/', RelType.officeDocument, main);
  const add = (p: ExtraPart) => {
    pkg.addPart(p.name, p.contentType, typeof p.content === 'string' ? p.content : p.content);
    if (p.relType) {
      pkg.addRelationship(main, p.relType, p.name, p.relId ? { id: p.relId } : {});
    }
  };
  if (o.styles !== undefined)
    add({
      name: '/word/styles.xml',
      contentType: ContentType.docxStyles,
      relType: RelType.styles,
      content: wrap('w:styles', o.styles),
    });
  if (o.numbering !== undefined)
    add({
      name: '/word/numbering.xml',
      contentType: ContentType.docxNumbering,
      relType: RelType.numbering,
      content: wrap('w:numbering', o.numbering),
    });
  if (o.settings !== undefined)
    add({
      name: '/word/settings.xml',
      contentType: ContentType.docxSettings,
      relType: RelType.settings,
      content: wrap('w:settings', o.settings),
    });
  for (const p of o.parts ?? []) add(p);
  for (const [id, type, target, external] of o.rels ?? []) {
    pkg.addRelationship(
      main,
      type,
      external ? target : `/word/${target}`,
      external ? { id, targetMode: 'External' } : { id },
    );
  }
  return pkg.save();
}

/** Wraps an XML string as a complete part with the test namespace declarations. */
export const part = wrap;

/**
 * Body text following mammoth's raw-text model: one entry per paragraph of the main story (table cells
 * paragraph by paragraph, generated tables included), deleted text and line breaks omitted, tabs kept,
 * headers, footers, notes and text boxes excluded.
 */
export function mammothModelParagraphs(doc: PMNode): string[] {
  const out: string[] = [];
  const inline = (p: PMNode) => {
    let s = '';
    p.forEach((n) => {
      if (n.isText) {
        if (!n.marks.some((m) => m.type.name === 'deletion')) s += n.text ?? '';
      } else if (n.type.name === 'tab') s += '\t';
      else if (n.type.name === 'field') s += n.attrs.result as string;
    });
    return s;
  };
  const walk = (n: PMNode) => {
    n.forEach((c) => {
      const t = c.type.name;
      if (t === 'header' || t === 'footer' || t === 'footnote') return;
      if (t === 'paragraph') out.push(inline(c));
      else if (!c.isLeaf && !c.isInline) walk(c);
    });
  };
  walk(doc);
  return out;
}

/** mammoth's raw text split into paragraphs (it ends every paragraph with a blank line). */
export function mammothParagraphs(text: string): string[] {
  const parts = text.split('\n\n');
  if (parts[parts.length - 1] === '') parts.pop();
  return parts;
}

export async function mammothText(bytes: Uint8Array): Promise<string> {
  return (await readDocxWithMammoth(bytes)).text;
}
