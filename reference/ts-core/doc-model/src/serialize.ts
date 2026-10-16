/**
 * Plain-text and JSON (de)serialisation for tests, clipboard fallbacks and version diffs.
 *
 * Plain text: one line per paragraph; tabs as `\t`, line breaks as `\n` (Word's "Text Only" export
 * writes manual line breaks as new lines too), table cells separated by tabs, rows by new lines; page
 * breaks as form feeds (`\f`). Header, footer and note stories are omitted unless requested.
 *
 * JSON: a stable document (object keys sorted) holding the PM tree and all side parts, so two versions
 * of a document diff line by line.
 */

import { officeTheme, type Theme } from '@kiln-ref/ooxml';
import type { Node as PMNode, Schema } from '@tiptap/pm/model';
import { Numbering, type NumberingJSON } from './numbering/numbering.ts';
import { docSchema } from './schema.ts';
import { defaultSectionProps } from './sections.ts';
import { StyleSheet, type StyleSheetJSON } from './styles/stylesheet.ts';
import type { DocComment, WriteDocument } from './template.ts';
import { DEFAULT_SETTINGS, type DocSettings, type SectionProps } from './types.ts';

export interface PlainTextOptions {
  /** Include header/footer stories (each prefixed by nothing, in document order). */
  includeHeadersFooters?: boolean;
  /** Include footnote/endnote bodies at the end. */
  includeNotes?: boolean;
  /** Paragraph separator (default `\n`). */
  paragraphSeparator?: string;
  /** Exclude text in `deletion` marks (show the document as if all changes were accepted). */
  acceptChanges?: boolean;
  /** Called for each list paragraph to prefix its label (e.g. from `computeListLabels`). */
  listLabel?: (paragraph: PMNode, pos: number) => string | undefined;
}

function inlineText(node: PMNode, acceptChanges: boolean): string {
  let out = '';
  node.forEach((child) => {
    if (child.isText) {
      if (acceptChanges && child.marks.some((m) => m.type.name === 'deletion')) return;
      out += child.text ?? '';
    } else if (child.type.spec.leafText) {
      out += child.type.spec.leafText(child);
    } else if (child.type.name === 'image' || child.type.name === 'shape' || child.type.name === 'equation') {
      out += '￼';
    }
  });
  return out;
}

/** Plain text of a document or fragment. */
export function toPlainText(node: PMNode, options: PlainTextOptions = {}): string {
  const sep = options.paragraphSeparator ?? '\n';
  const lines: string[] = [];
  const accept = options.acceptChanges ?? false;
  const blocks = (n: PMNode, base: number) => {
    n.forEach((child, offset) => {
      const pos = base + offset;
      const name = child.type.name;
      if ((name === 'header' || name === 'footer') && !options.includeHeadersFooters) return;
      if (name === 'footnote' && !options.includeNotes) return;
      if (name === 'paragraph') {
        const label = options.listLabel?.(child, pos);
        lines.push((label ? `${label}\t` : '') + inlineText(child, accept));
      } else if (name === 'table') {
        child.forEach((row) => {
          const cells: string[] = [];
          row.forEach((cell) => {
            cells.push(
              cell.textBetween(0, cell.content.size, ' ', (leaf) =>
                leaf.type.spec.leafText ? leaf.type.spec.leafText(leaf) : '',
              ),
            );
          });
          lines.push(cells.join('\t'));
        });
      } else if (name === 'opaqueBlock') {
        if (child.attrs.label) lines.push(child.attrs.label as string);
      } else if (!child.isLeaf && !child.isInline) {
        blocks(child, pos + 1);
      }
    });
  };
  if (node.isTextblock) return inlineText(node, accept);
  blocks(node, 0);
  return lines.join(sep);
}

/**
 * Builds a document from plain text: each line becomes a paragraph (in `styleId`, default Normal),
 * tab characters become tab nodes, form feeds become page breaks.
 */
export function fromPlainText(
  text: string,
  options: { schema?: Schema; styleId?: string; section?: SectionProps } = {},
): PMNode {
  const schema = options.schema ?? docSchema;
  const lines = text.replace(/\r\n?/g, '\n').split('\n');
  const paragraphs = lines.map((line) => {
    const content: PMNode[] = [];
    let buf = '';
    const flush = () => {
      if (buf) content.push(schema.text(buf));
      buf = '';
    };
    for (const ch of line) {
      if (ch === '\t') {
        flush();
        content.push(schema.node('tab'));
      } else if (ch === '\f') {
        flush();
        content.push(schema.node('pageBreak'));
      } else if (ch === '\v') {
        flush();
        content.push(schema.node('softBreak'));
      } else if (ch >= ' ' || ch === ' ') {
        buf += ch;
      }
    }
    flush();
    return schema.node('paragraph', options.styleId ? { styleId: options.styleId } : null, content);
  });
  return schema.node('doc', null, [
    schema.node('section', { props: options.section ?? defaultSectionProps() }, paragraphs),
  ]);
}

// ------------------------------------------------------------------ JSON

export interface WriteDocumentJSON {
  format: 'kiln-write';
  version: 1;
  doc: unknown;
  styles: StyleSheetJSON;
  numbering: NumberingJSON;
  settings: DocSettings;
  comments: DocComment[];
  /** Theme is stored as JSON of the ooxml Theme (XML extras are already JSON-able nodes). */
  theme: Theme;
  /** Media are referenced by id only; bytes live in the package, not the JSON. */
  media: { id: string; contentType: string; size: number }[];
}

/** Recursively sorts object keys so JSON output is stable across runs. */
export function stableValue(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(stableValue);
  if (value && typeof value === 'object' && !(value instanceof Uint8Array)) {
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(value).sort()) {
      const v = (value as Record<string, unknown>)[key];
      if (v !== undefined) out[key] = stableValue(v);
    }
    return out;
  }
  return value;
}

/** Converts a document to its JSON form (a plain object). */
export function documentToJSON(doc: WriteDocument): WriteDocumentJSON {
  return {
    format: 'kiln-write',
    version: 1,
    doc: doc.doc.toJSON(),
    styles: doc.styles.toJSON(),
    numbering: doc.numbering.toJSON(),
    settings: doc.settings,
    comments: doc.comments,
    theme: doc.theme,
    media: [...doc.media.entries()]
      .map(([id, m]) => ({ id, contentType: m.contentType, size: m.data.byteLength }))
      .sort((a, b) => a.id.localeCompare(b.id)),
  };
}

/** Serialises to stable, pretty-printed JSON text (2-space indent, sorted keys). */
export function serializeDocument(doc: WriteDocument): string {
  return `${JSON.stringify(stableValue(documentToJSON(doc)), null, 2)}\n`;
}

export class DocumentFormatError extends Error {
  override name = 'DocumentFormatError';
}

/**
 * Parses JSON produced by `serializeDocument`/`documentToJSON`. Media bytes are not part of the JSON;
 * pass them in `media` when available.
 */
export function parseDocument(
  input: string | WriteDocumentJSON,
  options: { schema?: Schema; media?: WriteDocument['media'] } = {},
): WriteDocument {
  const json = (typeof input === 'string' ? JSON.parse(input) : input) as Partial<WriteDocumentJSON>;
  if (json.format !== 'kiln-write') throw new DocumentFormatError('not an Kiln Write document');
  if (json.version !== 1) throw new DocumentFormatError(`unsupported version ${String(json.version)}`);
  const schema = options.schema ?? docSchema;
  const doc = schema.nodeFromJSON(json.doc);
  doc.check();
  const styles = StyleSheet.fromJSON(json.styles ?? { docDefaults: { pPr: {}, rPr: {} }, styles: [] });
  const numbering = Numbering.fromJSON(json.numbering ?? { abstracts: [], instances: [] });
  numbering.listStyleResolver = (id) => styles.get(id)?.pPr?.numbering?.numId;
  return {
    doc,
    styles,
    numbering,
    theme: json.theme ?? officeTheme(),
    settings: { ...DEFAULT_SETTINGS, ...json.settings },
    comments: json.comments ?? [],
    media: options.media ?? new Map(),
  };
}
