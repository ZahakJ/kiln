/**
 * Package-wide state of one `readDocx` call: the OPC package, the schema, media and charts found so far,
 * the parts and relationships the reader consumed (everything else becomes passthrough), and warnings.
 */
import { type OpcPackage, parseXml, type Relationship, type XmlElement } from '@officina/ooxml';
import type { Schema } from '@tiptap/pm/model';
import type { DocxChartRef, DocxRelTarget } from '../common/types.ts';
import { type DocxWarning, WarningCollector } from '../common/warnings.ts';

export interface LoadedPart {
  name: string;
  root: XmlElement;
}

export class DocxReadContext {
  readonly warnings = new WarningCollector();
  readonly media = new Map<string, { contentType: string; data: Uint8Array }>();
  readonly charts: DocxChartRef[] = [];
  readonly consumedParts = new Set<string>();
  readonly consumedRels = new Map<string, Set<string>>();
  readonly relationships = new Map<string, Map<string, DocxRelTarget>>();
  /** Comment ids whose range marked at least one node. */
  readonly anchoredComments = new Set<string>();
  /** Comment ids referenced from a story (`w:commentReference`). */
  readonly referencedComments = new Set<string>();
  /** Note ids referenced from stories, per kind. */
  readonly referencedNotes = { footnote: new Set<string>(), endnote: new Set<string>() };
  #sdtCounter = 0;
  readonly #partCache = new Map<string, LoadedPart | null>();

  constructor(
    readonly pkg: OpcPackage,
    readonly schema: Schema,
  ) {}

  warn(w: DocxWarning): void {
    this.warnings.add(w);
  }

  nextSdtKey(): string {
    this.#sdtCounter += 1;
    return `sdt${this.#sdtCounter}`;
  }

  consume(part: string): void {
    this.consumedParts.add(part);
  }

  consumeRel(source: string, id: string): void {
    let s = this.consumedRels.get(source);
    if (!s) {
      s = new Set();
      this.consumedRels.set(source, s);
    }
    s.add(id);
  }

  /** Parses an XML part once; `null` (with a warning) when it is missing or malformed. */
  loadPart(name: string): LoadedPart | undefined {
    const cached = this.#partCache.get(name);
    if (cached !== undefined) return cached ?? undefined;
    const bytes = this.pkg.getPart(name);
    if (!bytes) {
      this.#partCache.set(name, null);
      this.warn({
        code: 'part.missing',
        severity: 'warning',
        message: `The part ${name} is referenced but missing from the package.`,
        part: name,
        path: '/',
      });
      return undefined;
    }
    try {
      const loaded = { name, root: parseXml(bytes).root };
      this.#partCache.set(name, loaded);
      return loaded;
    } catch (e) {
      this.#partCache.set(name, null);
      this.warn({
        code: 'part.invalidXml',
        severity: 'warning',
        message: `The part ${name} is not well-formed XML and was skipped (${(e as Error).message}).`,
        part: name,
        path: '/',
      });
      return undefined;
    }
  }

  /** Looks up a relationship of `source` and records it in the id → target table. */
  relationship(source: string, id: string): (Relationship & { part?: string }) | undefined {
    const rels = this.pkg.relationships(source);
    const r = rels.get(id);
    if (!r) return undefined;
    const part = rels.resolve(r);
    let table = this.relationships.get(source);
    if (!table) {
      table = new Map();
      this.relationships.set(source, table);
    }
    table.set(id, {
      type: r.type,
      target: part ?? r.target,
      external: r.targetMode === 'External',
    });
    return part ? { ...r, part } : { ...r };
  }

  /** Records every relationship of `source` in the id → target table (for raw XML snippets). */
  recordAllRelationships(source: string): void {
    for (const r of this.pkg.relationships(source).all()) this.relationship(source, r.id);
  }
}
