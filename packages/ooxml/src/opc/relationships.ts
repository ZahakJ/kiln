import { buildXmlBytes } from '../xml/build.ts';
import { NS } from '../xml/namespaces.ts';
import { el } from '../xml/node.ts';
import { parseXml } from '../xml/parse.ts';
import { attr, children } from '../xml/query.ts';
import { resolveTarget } from './part-name.ts';
import { relTypeMatches } from './rel-types.ts';

export type TargetMode = 'Internal' | 'External';

export interface Relationship {
  id: string;
  type: string;
  /** The target exactly as written in the .rels part. */
  target: string;
  targetMode: TargetMode;
}

/** The relationships of one source part (or of the package, source `/`). */
export class Relationships {
  readonly #list: Relationship[] = [];
  dirty = false;

  constructor(
    /** Source part name; `/` for package relationships. */
    readonly source: string,
  ) {}

  static parse(source: string, bytes: Uint8Array): Relationships {
    const rels = new Relationships(source);
    const root = parseXml(bytes).root;
    for (const r of children(root, 'Relationship')) {
      const id = attr(r, 'Id');
      const type = attr(r, 'Type');
      const target = attr(r, 'Target');
      if (id === undefined || type === undefined || target === undefined) continue;
      rels.#list.push({
        id,
        type,
        target,
        targetMode: attr(r, 'TargetMode') === 'External' ? 'External' : 'Internal',
      });
    }
    return rels;
  }

  get size(): number {
    return this.#list.length;
  }

  all(): readonly Relationship[] {
    return this.#list;
  }

  get(id: string): Relationship | undefined {
    return this.#list.find((r) => r.id === id);
  }

  /** Relationships of a type (Strict type URIs match their transitional equivalents). */
  byType(type: string): Relationship[] {
    return this.#list.filter((r) => relTypeMatches(r.type, type));
  }

  firstByType(type: string): Relationship | undefined {
    return this.#list.find((r) => relTypeMatches(r.type, type));
  }

  /** Absolute part name of an internal relationship's target; `undefined` for external targets. */
  resolve(rel: Relationship): string | undefined {
    return rel.targetMode === 'External' ? undefined : resolveTarget(this.source, rel.target);
  }

  /** The next unused `rIdN`. */
  nextId(prefix = 'rId'): string {
    const used = new Set(this.#list.map((r) => r.id));
    let n = 1;
    while (used.has(`${prefix}${n}`)) n++;
    return `${prefix}${n}`;
  }

  /** Adds a relationship and returns it. `id` defaults to the next free `rIdN`; a taken id throws. */
  add(type: string, target: string, options: { id?: string; targetMode?: TargetMode } = {}): Relationship {
    const id = options.id ?? this.nextId();
    if (this.get(id)) throw new Error(`relationship id ${id} already exists on ${this.source}`);
    const rel: Relationship = { id, type, target, targetMode: options.targetMode ?? 'Internal' };
    this.#list.push(rel);
    this.dirty = true;
    return rel;
  }

  remove(id: string): boolean {
    const i = this.#list.findIndex((r) => r.id === id);
    if (i < 0) return false;
    this.#list.splice(i, 1);
    this.dirty = true;
    return true;
  }

  /** Removes every internal relationship whose target resolves to `partName` (case-insensitive). */
  removeTargeting(partName: string): Relationship[] {
    const key = partName.toLowerCase();
    const removed = this.#list.filter((r) => this.resolve(r)?.toLowerCase() === key);
    for (const r of removed) this.remove(r.id);
    return removed;
  }

  toBytes(): Uint8Array {
    return buildXmlBytes(
      el(
        'Relationships',
        { xmlns: NS.rel },
        this.#list.map((r) =>
          el('Relationship', {
            Id: r.id,
            Type: r.type,
            Target: r.target,
            TargetMode: r.targetMode === 'External' ? 'External' : undefined,
          }),
        ),
      ),
    );
  }
}
