import { type UnzipFileInfo, unzipSync, type Zippable, zipSync } from 'fflate';
import { buildXmlBytes } from '../xml/build.ts';
import type { XmlDocument, XmlElement } from '../xml/node.ts';
import { decodeXmlBytes, type ParseXmlOptions, parseXml } from '../xml/parse.ts';
import { ContentType, ContentTypes } from './content-types.ts';
import {
  assertPartName,
  extensionOf,
  partKey,
  partNameToZipName,
  relativeTarget,
  relsPartName,
  sourcePartOfRels,
  zipNameToPartName,
} from './part-name.ts';
import { RelType } from './rel-types.ts';
import { type Relationship, Relationships, type TargetMode } from './relationships.ts';

export const CONTENT_TYPES_PART = '/[Content_Types].xml';

/** Limits applied before anything is decompressed, so a hostile zip cannot exhaust memory. */
export interface OpcLimits {
  /** Maximum number of zip entries. Default 20 000. */
  maxEntries: number;
  /** Maximum declared uncompressed size of one entry, in bytes. Default 512 MiB. */
  maxEntrySize: number;
  /** Maximum declared uncompressed size of all entries together, in bytes. Default 1 GiB. */
  maxTotalSize: number;
  /**
   * Maximum compression ratio of an entry larger than `ratioThreshold` bytes. Real Office XML stays far
   * below 1000:1; zip bombs exceed it by orders of magnitude. Default 1000.
   */
  maxCompressionRatio: number;
  /** Entries smaller than this are exempt from the ratio check. Default 1 MiB. */
  ratioThreshold: number;
}

export const DEFAULT_OPC_LIMITS: Readonly<OpcLimits> = {
  maxEntries: 20_000,
  maxEntrySize: 512 * 1024 * 1024,
  maxTotalSize: 1024 * 1024 * 1024,
  maxCompressionRatio: 1000,
  ratioThreshold: 1024 * 1024,
};

export class OpcError extends Error {
  override name = 'OpcError';
}

/** The file is an OLE compound document: an encrypted (password-protected) or legacy binary Office file. */
export class EncryptedOrLegacyFileError extends OpcError {
  override name = 'EncryptedOrLegacyFileError';
}

export class ZipBombError extends OpcError {
  override name = 'ZipBombError';
}

export interface SaveOptions {
  /** Timestamp written to every zip entry. Default 1980-01-01 00:00 (as Office does), for reproducible output. */
  mtime?: Date;
  /** Deflate level 0–9 for compressible parts. Default 6. */
  level?: 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9;
}

interface PartEntry {
  name: string;
  bytes: Uint8Array;
}

// Already-compressed media gain nothing from deflate.
const STORED_EXTENSIONS = new Set([
  'png',
  'jpg',
  'jpeg',
  'gif',
  'mp4',
  'mp3',
  'm4a',
  'zip',
  'docx',
  'xlsx',
  'pptx',
  'wdp',
]);
const textEncoder = new TextEncoder();

/**
 * An Open Packaging Conventions container (`.docx`, `.xlsx`, `.pptx`). Parts are held as bytes; a part
 * nobody touches is written back byte-for-byte. Relationships and content types are parsed lazily and
 * re-serialised only when modified.
 */
export class OpcPackage {
  readonly #parts = new Map<string, PartEntry>();
  readonly #rels = new Map<string, Relationships>();
  #contentTypes: ContentTypes;
  #contentTypesBytes: Uint8Array | undefined;

  private constructor(contentTypes: ContentTypes, contentTypesBytes?: Uint8Array) {
    this.#contentTypes = contentTypes;
    this.#contentTypesBytes = contentTypesBytes;
  }

  /** An empty package with Office's `rels` and `xml` defaults. */
  static create(): OpcPackage {
    const ct = new ContentTypes();
    ct.defaults.set('rels', ContentType.relationships);
    ct.defaults.set('xml', ContentType.xml);
    ct.dirty = true;
    return new OpcPackage(ct);
  }

  /** Opens a package from zip bytes, enforcing `limits` before decompressing anything. */
  static open(bytes: Uint8Array, limits: Partial<OpcLimits> = {}): OpcPackage {
    const lim = { ...DEFAULT_OPC_LIMITS, ...limits };
    if (
      bytes.length >= 4 &&
      bytes[0] === 0xd0 &&
      bytes[1] === 0xcf &&
      bytes[2] === 0x11 &&
      bytes[3] === 0xe0
    ) {
      throw new EncryptedOrLegacyFileError(
        'this file is password-protected or uses a legacy binary Office format (OLE compound file)',
      );
    }
    if (bytes.length < 4 || bytes[0] !== 0x50 || bytes[1] !== 0x4b) {
      throw new OpcError('not a zip package');
    }
    let count = 0;
    let total = 0;
    const filter = (f: UnzipFileInfo): boolean => {
      if (f.name.endsWith('/')) return false;
      count++;
      if (count > lim.maxEntries) throw new ZipBombError(`more than ${lim.maxEntries} zip entries`);
      if (f.originalSize > lim.maxEntrySize) {
        throw new ZipBombError(
          `entry ${f.name} declares ${f.originalSize} bytes (limit ${lim.maxEntrySize})`,
        );
      }
      total += f.originalSize;
      if (total > lim.maxTotalSize)
        throw new ZipBombError(`package expands beyond ${lim.maxTotalSize} bytes`);
      if (
        f.originalSize > lim.ratioThreshold &&
        f.originalSize > lim.maxCompressionRatio * Math.max(f.size, 1)
      ) {
        throw new ZipBombError(`entry ${f.name} has a suspicious compression ratio`);
      }
      return true;
    };
    let files: Record<string, Uint8Array>;
    try {
      files = unzipSync(bytes, { filter });
    } catch (e) {
      if (e instanceof OpcError) throw e;
      throw new OpcError(`corrupt zip: ${(e as Error).message}`);
    }
    let ctBytes: Uint8Array | undefined;
    const entries: PartEntry[] = [];
    const seen = new Set<string>();
    for (const [zipName, data] of Object.entries(files)) {
      const name = zipNameToPartName(zipName);
      const key = partKey(name);
      if (seen.has(key)) throw new OpcError(`duplicate part name ${name}`);
      seen.add(key);
      if (key === partKey(CONTENT_TYPES_PART)) ctBytes = data;
      else entries.push({ name, bytes: data });
    }
    if (!ctBytes) throw new OpcError('missing [Content_Types].xml');
    let ct: ContentTypes;
    try {
      ct = ContentTypes.parse(ctBytes);
    } catch (e) {
      throw new OpcError(`unreadable [Content_Types].xml: ${(e as Error).message}`);
    }
    const pkg = new OpcPackage(ct, ctBytes);
    for (const e of entries) pkg.#parts.set(partKey(e.name), e);
    return pkg;
  }

  // ---------------------------------------------------------------- parts

  /** Every part name except `[Content_Types].xml`, in package order, including `.rels` parts. */
  allPartNames(): string[] {
    return [...this.#parts.values()].map((p) => p.name);
  }

  /** Part names excluding relationship parts. */
  partNames(): string[] {
    return this.allPartNames().filter((n) => !isRelsPart(n));
  }

  has(name: string): boolean {
    return this.#parts.has(partKey(name));
  }

  /** The bytes of a part, exactly as stored. Relationship parts reflect pending relationship edits. */
  getPart(name: string): Uint8Array | undefined {
    if (isRelsPart(name)) this.#flushRels(name);
    return this.#parts.get(partKey(name))?.bytes;
  }

  getPartText(name: string): string | undefined {
    const b = this.getPart(name);
    return b ? decodeXmlBytes(b) : undefined;
  }

  /** Parses an XML part; `undefined` if the part does not exist. */
  getPartXml(name: string, options?: ParseXmlOptions): XmlDocument | undefined {
    const b = this.getPart(name);
    return b ? parseXml(b, options) : undefined;
  }

  /**
   * Replaces a part's content, or creates the part when `contentType` is given. Accepts bytes, a string
   * (UTF-8) or an XML tree (serialised with `buildXml`).
   */
  setPart(name: string, content: Uint8Array | string | XmlDocument | XmlElement, contentType?: string): void {
    const bytes = toBytes(content);
    const key = partKey(name);
    const existing = this.#parts.get(key);
    if (existing) {
      existing.bytes = bytes;
      if (contentType !== undefined) this.#contentTypes.set(existing.name, contentType);
      if (isRelsPart(existing.name)) this.#rels.delete(partKey(sourcePartOfRels(existing.name) ?? ''));
      return;
    }
    if (contentType === undefined) {
      throw new OpcError(`part ${name} does not exist; pass a content type to create it`);
    }
    this.addPart(name, contentType, bytes);
  }

  /** Adds a new part. Throws if the part exists. */
  addPart(name: string, contentType: string, content: Uint8Array | string | XmlDocument | XmlElement): void {
    assertPartName(name);
    if (partKey(name) === partKey(CONTENT_TYPES_PART))
      throw new OpcError('[Content_Types].xml is managed by the package');
    if (this.has(name)) throw new OpcError(`part ${name} already exists`);
    this.#parts.set(partKey(name), { name, bytes: toBytes(content) });
    this.#contentTypes.set(name, contentType);
  }

  /**
   * Removes a part, its relationships part and its content-type override. With `removeRelationships`
   * (default) every relationship in the package that targets it is removed too.
   */
  removePart(name: string, options: { removeRelationships?: boolean } = {}): boolean {
    const key = partKey(name);
    const entry = this.#parts.get(key);
    if (!entry) return false;
    this.#parts.delete(key);
    this.#contentTypes.remove(entry.name);
    const relsName = relsPartName(entry.name);
    this.#parts.delete(partKey(relsName));
    this.#rels.delete(key);
    if (options.removeRelationships ?? true) {
      for (const source of this.#relationshipSources())
        this.relationships(source).removeTargeting(entry.name);
    }
    return true;
  }

  /**
   * The first free part name for a pattern containing `{n}` (numbered from 1):
   * `nextPartName('/word/media/image{n}.png')` → `/word/media/image3.png`.
   */
  nextPartName(pattern: string): string {
    if (!pattern.includes('{n}')) throw new OpcError('pattern must contain {n}');
    for (let n = 1; ; n++) {
      const candidate = pattern.replace('{n}', String(n));
      if (!this.has(candidate)) return candidate;
    }
  }

  getContentType(name: string): string | undefined {
    if (partKey(name) === partKey(CONTENT_TYPES_PART)) return undefined;
    return this.#contentTypes.get(name);
  }

  setContentType(name: string, contentType: string): void {
    if (!this.has(name)) throw new OpcError(`part ${name} does not exist`);
    this.#contentTypes.set(this.#parts.get(partKey(name))?.name ?? name, contentType);
  }

  /** The content-types table (read-only use; mutate through the package). */
  get contentTypes(): ContentTypes {
    return this.#contentTypes;
  }

  /** Parts whose content type equals `contentType`. */
  partsByContentType(contentType: string): string[] {
    return this.partNames().filter((n) => this.getContentType(n) === contentType);
  }

  // -------------------------------------------------------- relationships

  /** Relationships of a source part (`/` = package relationships). Created empty if absent. */
  relationships(source = '/'): Relationships {
    const key = partKey(source);
    let rels = this.#rels.get(key);
    if (!rels) {
      const relsBytes = this.#parts.get(partKey(relsPartName(source)))?.bytes;
      try {
        rels = relsBytes ? Relationships.parse(source, relsBytes) : new Relationships(source);
      } catch (e) {
        throw new OpcError(`unreadable relationships for ${source}: ${(e as Error).message}`);
      }
      this.#rels.set(key, rels);
    }
    return rels;
  }

  /** Absolute names of the internal parts that `source` relates to with `type`, in relationship order. */
  relatedParts(source: string, type: string): string[] {
    const rels = this.relationships(source);
    return rels
      .byType(type)
      .map((r) => rels.resolve(r))
      .filter((p): p is string => p !== undefined);
  }

  relatedPart(source: string, type: string): string | undefined {
    return this.relatedParts(source, type)[0];
  }

  /** Resolves a relationship id on `source` to an absolute part name (internal targets only). */
  resolveRelationship(source: string, id: string): string | undefined {
    const rels = this.relationships(source);
    const r = rels.get(id);
    return r ? rels.resolve(r) : undefined;
  }

  /**
   * Adds a relationship from `source`. For internal targets pass the absolute part name; the stored target
   * is made relative as Office writes it. External targets (URLs) are stored verbatim.
   */
  addRelationship(
    source: string,
    type: string,
    target: string,
    options: { id?: string; targetMode?: TargetMode } = {},
  ): Relationship {
    const external = options.targetMode === 'External';
    const stored = external ? target : relativeTarget(source, target);
    return this.relationships(source).add(type, stored, options);
  }

  /** The main document part (`/word/document.xml`, `/xl/workbook.xml`, `/ppt/presentation.xml`). */
  mainDocumentPart(): string | undefined {
    return this.relatedPart('/', RelType.officeDocument);
  }

  #relationshipSources(): string[] {
    const sources = new Set<string>();
    for (const p of this.#parts.values()) {
      const s = sourcePartOfRels(p.name);
      if (s !== undefined) sources.add(s);
    }
    for (const r of this.#rels.values()) sources.add(r.source);
    return [...sources];
  }

  #flushRels(relsName: string): void {
    const source = sourcePartOfRels(relsName);
    if (source === undefined) return;
    const rels = this.#rels.get(partKey(source));
    if (!rels?.dirty) return;
    const key = partKey(relsName);
    if (rels.size === 0) {
      this.#parts.delete(key);
    } else {
      const entry = this.#parts.get(key);
      const bytes = rels.toBytes();
      if (entry) entry.bytes = bytes;
      else this.#parts.set(key, { name: relsName, bytes });
    }
    rels.dirty = false;
  }

  // ----------------------------------------------------------------- save

  /**
   * Serialises the package. `[Content_Types].xml` comes first and `_rels/.rels` second, as Office writes
   * them; other parts keep their original order with new parts at the end.
   */
  save(options: SaveOptions = {}): Uint8Array {
    for (const r of this.#rels.values()) if (r.dirty) this.#flushRels(relsPartName(r.source));
    if (this.has('/_rels/.rels') && !this.#contentTypes.defaults.has('rels')) {
      this.#contentTypes.defaults.set('rels', ContentType.relationships);
      this.#contentTypes.dirty = true;
    }
    const mtime = options.mtime ?? new Date(1980, 0, 1, 0, 0, 0);
    const level = options.level ?? 6;
    const files: Zippable = {};
    const ctBytes =
      this.#contentTypes.dirty || !this.#contentTypesBytes
        ? this.#contentTypes.toBytes()
        : this.#contentTypesBytes;
    this.#contentTypesBytes = ctBytes;
    this.#contentTypes.dirty = false;
    files[partNameToZipName(CONTENT_TYPES_PART)] = [ctBytes, { level, mtime }];
    const pkgRels = this.#parts.get(partKey('/_rels/.rels'));
    if (pkgRels) files[partNameToZipName(pkgRels.name)] = [pkgRels.bytes, { level, mtime }];
    for (const p of this.#parts.values()) {
      if (p === pkgRels) continue;
      const lvl = STORED_EXTENSIONS.has(extensionOf(p.name)) ? 0 : level;
      files[partNameToZipName(p.name)] = [p.bytes, { level: lvl, mtime }];
    }
    return zipSync(files);
  }
}

function isRelsPart(name: string): boolean {
  return sourcePartOfRels(name) !== undefined;
}

function toBytes(content: Uint8Array | string | XmlDocument | XmlElement): Uint8Array {
  if (content instanceof Uint8Array) return content;
  if (typeof content === 'string') return textEncoder.encode(content);
  return buildXmlBytes(content);
}
