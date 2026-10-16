/**
 * Package-wide state of one `writeDocx` call: the target package, relationship allocation (reusing the
 * original relationship ids where possible so preserved raw XML keeps pointing at the right targets),
 * media parts, drawing ids and the issues found while writing.
 */
import { type OpcPackage, type Relationships, RelType } from '@kiln-ref/ooxml';
import type { Schema } from '@tiptap/pm/model';
import type { DocxRelTarget } from '../common/types.ts';

export interface WriteIssue {
  code: string;
  message: string;
  /** Part the issue concerns. */
  part: string;
}

/** Allocates relationship ids for one source part. */
export class RelAllocator {
  readonly #byKey = new Map<string, string>();
  /** Original ids of this source (reserved for their own targets). */
  readonly #reserved: Set<string>;

  constructor(
    readonly rels: Relationships,
    readonly source: string,
    /** The original relationships of the part this one was read from (id → target). */
    readonly original: ReadonlyMap<string, DocxRelTarget> | undefined,
    readonly pkg: OpcPackage,
  ) {
    this.#reserved = new Set(original?.keys() ?? []);
    for (const r of rels.all()) {
      const target = r.targetMode === 'External' ? r.target : (rels.resolve(r) ?? r.target);
      this.#byKey.set(key(r.type, target, r.targetMode === 'External'), r.id);
    }
  }

  /** The id of a relationship to `target` (a part name, or a URL when `external`), adding it if needed. */
  get(type: string, target: string, external = false): string {
    const k = key(type, target, external);
    const known = this.#byKey.get(k);
    if (known) return known;
    let id: string | undefined;
    for (const [oid, t] of this.original ?? []) {
      if (t.type === type && t.target === target && t.external === external && !this.rels.get(oid)) {
        id = oid;
        break;
      }
    }
    if (!id) id = this.#freshId();
    if (external) this.rels.add(type, target, { id, targetMode: 'External' });
    else this.pkg.addRelationship(this.source, type, target, { id });
    this.#byKey.set(k, id);
    return id;
  }

  /** Adds a relationship with a fixed id (raw XML refers to it); false when the id is taken. */
  addWithId(id: string, type: string, target: string, external: boolean): boolean {
    if (this.rels.get(id)) return false;
    if (external) this.rels.add(type, target, { id, targetMode: 'External' });
    else this.pkg.addRelationship(this.source, type, target, { id });
    this.#byKey.set(key(type, target, external), id);
    return true;
  }

  #freshId(): string {
    for (let n = 1; ; n++) {
      const id = `rId${n}`;
      if (!this.rels.get(id) && !this.#reserved.has(id)) return id;
    }
  }
}

function key(type: string, target: string, external: boolean): string {
  return `${type}|${external ? 'E' : 'I'}|${external ? target : target.toLowerCase()}`;
}

const EXT_TYPES: Record<string, string> = {
  'image/png': 'png',
  'image/jpeg': 'jpeg',
  'image/gif': 'gif',
  'image/bmp': 'bmp',
  'image/tiff': 'tiff',
  'image/svg+xml': 'svg',
  'image/x-emf': 'emf',
  'image/x-wmf': 'wmf',
  'image/webp': 'webp',
};

export class DocxWriteContext {
  readonly issues: WriteIssue[] = [];
  readonly #allocators = new Map<string, RelAllocator>();
  /** media key (model media id or data-URL hash) → written part name */
  readonly #media = new Map<string, string>();
  #docPrId: number;
  #revisionId: number;
  /** New part name → name of the part it was read from (for original relationship lookups). */
  readonly originalName = new Map<string, string>();

  constructor(
    readonly pkg: OpcPackage,
    readonly schema: Schema,
    readonly modelMedia: ReadonlyMap<string, { contentType: string; data: Uint8Array }>,
    readonly originalRels: ReadonlyMap<string, ReadonlyMap<string, DocxRelTarget>>,
    options: { firstDocPrId: number; firstRevisionId: number },
  ) {
    this.#docPrId = options.firstDocPrId;
    this.#revisionId = options.firstRevisionId;
  }

  issue(code: string, message: string, part: string): void {
    if (this.issues.some((i) => i.code === code && i.part === part && i.message === message)) return;
    this.issues.push({ code, message, part });
  }

  rels(source: string): RelAllocator {
    let a = this.#allocators.get(source);
    if (!a) {
      const orig = this.originalRels.get(this.originalName.get(source) ?? source);
      a = new RelAllocator(this.pkg.relationships(source), source, orig, this.pkg);
      this.#allocators.set(source, a);
    }
    return a;
  }

  nextDocPrId(): number {
    this.#docPrId += 1;
    return this.#docPrId;
  }

  /** A revision id for a tracked change that has none. */
  nextRevisionId(): string {
    this.#revisionId += 1;
    return String(this.#revisionId);
  }

  /**
   * The part name holding an image: the model's media part (written under its original name when
   * free), or a new part for a `data:` URL. `undefined` when the bytes are unavailable.
   */
  mediaPart(mediaId: string | null, src: string | null): string | undefined {
    if (mediaId) {
      const done = this.#media.get(mediaId);
      if (done) return done;
      const m = this.modelMedia.get(mediaId);
      if (m) return this.#addMedia(mediaId, mediaId, m.contentType, m.data);
    }
    if (src?.startsWith('data:')) {
      const done = this.#media.get(src);
      if (done) return done;
      const parsed = parseDataUrl(src);
      if (parsed) return this.#addMedia(src, undefined, parsed.contentType, parsed.data);
    }
    return undefined;
  }

  /** Writes a model media part referenced only from preserved raw XML. */
  ensureMedia(partName: string): boolean {
    if (this.#media.has(partName)) return true;
    const m = this.modelMedia.get(partName);
    if (!m) return false;
    return this.#addMedia(partName, partName, m.contentType, m.data) !== undefined;
  }

  #addMedia(key: string, preferred: string | undefined, contentType: string, data: Uint8Array): string {
    const ext = EXT_TYPES[contentType] ?? 'bin';
    let name = preferred && /^\/[^?#]+$/.test(preferred) && !this.pkg.has(preferred) ? preferred : undefined;
    if (!name) name = this.pkg.nextPartName(`/word/media/image{n}.${ext}`);
    this.pkg.addPart(name, contentType, data);
    this.#media.set(key, name);
    return name;
  }

  /** Relationship id from `source` to an image part. */
  imageRel(source: string, part: string): string {
    return this.rels(source).get(RelType.image, part);
  }
}

/** Decodes a base64 `data:` URL. */
export function parseDataUrl(url: string): { contentType: string; data: Uint8Array } | undefined {
  const m = /^data:([\w.+-]+\/[\w.+-]+)?(;[^,]*)?,(.*)$/s.exec(url);
  if (!m) return undefined;
  const contentType = m[1] ?? 'application/octet-stream';
  const isBase64 = (m[2] ?? '').includes(';base64');
  const payload = m[3] ?? '';
  try {
    if (isBase64) {
      const bin = atob(payload.replace(/\s+/g, ''));
      const data = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) data[i] = bin.charCodeAt(i);
      return { contentType, data };
    }
    return { contentType, data: new TextEncoder().encode(decodeURIComponent(payload)) };
  } catch {
    return undefined;
  }
}
