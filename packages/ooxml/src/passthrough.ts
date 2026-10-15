/**
 * "Never destroy what you don't understand" (PLAN §3.5). A reader records every part and relationship it
 * did not consume; the writer re-emits them into the new package with relationships fixed up.
 */
import type { OpcPackage } from './opc/package.ts';
import {
  basenameOf,
  directoryOf,
  extensionOf,
  partKey,
  relativeTarget,
  resolveTarget,
} from './opc/part-name.ts';
import type { Relationship } from './opc/relationships.ts';

export interface PassthroughPart {
  name: string;
  contentType: string;
  bytes: Uint8Array;
  /** The part's own outgoing relationships, as written. */
  relationships: Relationship[];
}

export interface PassthroughRelationship {
  /** Source part in the original package (`/` = package relationships). */
  source: string;
  rel: Relationship;
}

export interface CollectOptions {
  /** Parts the reader consumed (fully modelled; the writer regenerates them). */
  consumedParts: Iterable<string>;
  /**
   * Relationships the reader consumed although their target is not a consumed part (for example
   * hyperlinks it modelled), as `source → ids`.
   */
  consumedRelationships?: ReadonlyMap<string, Iterable<string>>;
}

export interface EmitOptions {
  /**
   * Maps an original part name to its name in the new package, for consumed parts the writer renamed
   * (`/xl/worksheets/sheet3.xml` → `/xl/worksheets/sheet1.xml`). Return `undefined` when the part no
   * longer exists: relationships from it are dropped and reported. Identity when omitted.
   */
  mapPart?: (originalName: string) => string | undefined;
}

export interface EmitResult {
  /** New part names of passthrough parts that had to be renamed to avoid collisions. */
  renamedParts: Map<string, string>;
  /** For each (new) source part, original relationship id → id used in the new package. */
  relationshipIds: Map<string, Map<string, string>>;
  /** Relationships that could not be re-emitted because their source no longer exists. */
  dropped: PassthroughRelationship[];
}

export class PassthroughSet {
  readonly #parts = new Map<string, PassthroughPart>();
  readonly #rels: PassthroughRelationship[] = [];

  /** Records everything in `pkg` that the reader did not consume. */
  static collect(pkg: OpcPackage, options: CollectOptions): PassthroughSet {
    const set = new PassthroughSet();
    const consumed = new Set<string>();
    for (const p of options.consumedParts) consumed.add(partKey(p));
    const consumedRels = new Map<string, Set<string>>();
    for (const [src, ids] of options.consumedRelationships ?? [])
      consumedRels.set(partKey(src), new Set(ids));

    for (const name of pkg.partNames()) {
      if (consumed.has(partKey(name))) continue;
      set.recordPart({
        name,
        contentType: pkg.getContentType(name) ?? 'application/octet-stream',
        bytes: pkg.getPart(name) as Uint8Array,
        relationships: pkg
          .relationships(name)
          .all()
          .map((r) => ({ ...r })),
      });
    }
    for (const source of ['/', ...pkg.partNames()]) {
      if (source !== '/' && !consumed.has(partKey(source))) continue;
      const rels = pkg.relationships(source);
      const skip = consumedRels.get(partKey(source));
      for (const r of rels.all()) {
        if (skip?.has(r.id)) continue;
        const target = rels.resolve(r);
        if (target !== undefined && consumed.has(partKey(target))) continue;
        if (target !== undefined && !pkg.has(target)) continue; // dangling: nothing to preserve
        set.recordRelationship(source, { ...r });
      }
    }
    return set;
  }

  recordPart(part: PassthroughPart): void {
    this.#parts.set(partKey(part.name), part);
  }

  recordRelationship(source: string, rel: Relationship): void {
    this.#rels.push({ source, rel });
  }

  get parts(): readonly PassthroughPart[] {
    return [...this.#parts.values()];
  }

  get relationships(): readonly PassthroughRelationship[] {
    return this.#rels;
  }

  has(partName: string): boolean {
    return this.#parts.has(partKey(partName));
  }

  get size(): number {
    return this.#parts.size;
  }

  /** Total bytes held, for diagnostics and the Compatibility Checker. */
  get byteLength(): number {
    let n = 0;
    for (const p of this.#parts.values()) n += p.bytes.length;
    return n;
  }

  /** Re-emits recorded parts and relationships into `target`. */
  emit(target: OpcPackage, options: EmitOptions = {}): EmitResult {
    const mapPart = options.mapPart ?? ((n: string) => n);
    const renamed = new Map<string, string>();
    const result: EmitResult = { renamedParts: renamed, relationshipIds: new Map(), dropped: [] };

    // 1. Choose names: keep the original unless the writer already produced a part with that name.
    const newName = new Map<string, string>();
    for (const p of this.#parts.values()) {
      let name = p.name;
      if (target.has(name)) {
        const ext = extensionOf(name);
        const stem = basenameOf(name).slice(0, ext ? -(ext.length + 1) : undefined);
        name = target.nextPartName(`${directoryOf(name)}${stem}_{n}${ext ? `.${ext}` : ''}`);
        renamed.set(p.name, name);
      }
      newName.set(partKey(p.name), name);
    }
    const mapTarget = (originalTarget: string): string | undefined =>
      newName.get(partKey(originalTarget)) ?? mapPart(originalTarget);

    // 2. Parts and their own relationships.
    for (const p of this.#parts.values()) {
      const name = newName.get(partKey(p.name)) as string;
      target.addPart(name, p.contentType, p.bytes);
      const rels = target.relationships(name);
      for (const r of p.relationships) {
        if (r.targetMode === 'External') {
          rels.add(r.type, r.target, { id: r.id, targetMode: 'External' });
          continue;
        }
        const orig = resolveTarget(p.name, r.target);
        const mapped = mapTarget(orig) ?? orig;
        const stored = mapped === orig && name === p.name ? r.target : relativeTarget(name, mapped);
        rels.add(r.type, stored, { id: r.id });
      }
    }

    // 3. Relationships from consumed parts (or the package) to passthrough parts and external targets.
    for (const pr of this.#rels) {
      const source = pr.source === '/' ? '/' : mapPart(pr.source);
      if (source === undefined || (source !== '/' && !target.has(source))) {
        result.dropped.push(pr);
        continue;
      }
      const rels = target.relationships(source);
      const id = rels.get(pr.rel.id) ? rels.nextId() : pr.rel.id;
      if (pr.rel.targetMode === 'External') {
        rels.add(pr.rel.type, pr.rel.target, { id, targetMode: 'External' });
      } else {
        const orig = resolveTarget(pr.source, pr.rel.target);
        const mapped = mapTarget(orig) ?? orig;
        rels.add(pr.rel.type, relativeTarget(source, mapped), { id });
      }
      let ids = result.relationshipIds.get(source);
      if (!ids) {
        ids = new Map();
        result.relationshipIds.set(source, ids);
      }
      ids.set(pr.rel.id, id);
    }
    return result;
  }
}
