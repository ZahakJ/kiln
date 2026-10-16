/**
 * Small XML helpers for the DOCX layer: serialisation of the raw XML fragments kept in the model,
 * attribute lookup tolerant of unprefixed WordprocessingML attributes, and location tracking for
 * warnings.
 */
import {
  buildXml,
  NS,
  namespaceForPrefix,
  attr as ooxmlAttr,
  parseXmlElement,
  undeclaredPrefixes,
  type XmlElement,
} from '@kiln-ref/ooxml';

/**
 * Serialises `node` as a fragment (no XML declaration) for the raw-XML attributes of the model.
 *
 * Contract: fragments use the stable Office prefixes (`w:`, `a:`, `wp:`, `r:`, `m:`, `v:`, …) without
 * declaring them — whoever embeds a fragment declares those namespaces on the part's root (as every
 * writer does) — which keeps the many small fragments compact. Prefixes outside the stable table are
 * declared on the fragment, taken from `scope` (normally the attributes of the part's root element).
 */
export function fragmentXml(node: XmlElement, scope?: Readonly<Record<string, string>>): string {
  const missing = undeclaredPrefixes(node).filter((p) => !namespaceForPrefix(p));
  let root = node;
  if (missing.length > 0) {
    const attrs = Object.create(null) as Record<string, string>;
    for (const p of missing) {
      const uri = scope?.[`xmlns:${p}`];
      if (uri) attrs[`xmlns:${p}`] = uri;
    }
    for (const [k, v] of Object.entries(node.attrs)) attrs[k] = v;
    root = { ...node, attrs };
  }
  return buildXml(root, { declaration: false, declareNamespaces: false });
}

/** Parses a fragment produced by `fragmentXml`, declaring the stable Office prefixes around it. */
export function parseFragment(xml: string): XmlElement {
  const decls = Object.entries(NS)
    .filter(([p]) => p !== 'x')
    .map(([p, uri]) => `xmlns:${p}="${uri}"`)
    .join(' ');
  const wrapper = parseXmlElement(`<kiln-fragment ${decls}>${xml}</kiln-fragment>`);
  const first = wrapper.children.find((c) => c.type === 'element');
  if (first?.type !== 'element') throw new Error('empty XML fragment');
  return first;
}

/** XPath-like location tracking: `/w:document/w:body/w:p[3]`. */
export class XmlPath {
  #steps: string[] = [];
  readonly #saved: string[][] = [];

  constructor(root?: string) {
    if (root) this.#steps.push(root);
  }

  push(step: string): void {
    this.#steps.push(step);
  }

  pop(): void {
    this.#steps.pop();
  }

  /** Temporarily replaces the whole path with `path` (`/a/b[2]/c`) until `popAbsolute`. */
  pushAbsolute(path: string): void {
    this.#saved.push(this.#steps);
    this.#steps = path.split('/').filter(Boolean);
  }

  popAbsolute(): void {
    this.#steps = this.#saved.pop() ?? [];
  }

  toString(): string {
    return `/${this.#steps.join('/')}`;
  }
}

/**
 * Iterates the child elements of `parent`, giving each its XPath step (`w:p[2]`, 1-based among siblings
 * of the same name).
 */
export function* stepped(parent: XmlElement): Generator<[XmlElement, string]> {
  const counts = new Map<string, number>();
  for (const c of parent.children) {
    if (c.type !== 'element') continue;
    const n = (counts.get(c.name) ?? 0) + 1;
    counts.set(c.name, n);
    yield [c, `${c.name}[${n}]`];
  }
}

/**
 * Attribute lookup for WordprocessingML. Some producers write the main namespace as the default
 * namespace and leave attributes unprefixed (`<commentRangeStart id="0"/>`); Word accepts that, so a
 * `w:` attribute falls back to its unprefixed name.
 */
export function attr(node: XmlElement | undefined, name: string): string | undefined {
  const v = ooxmlAttr(node, name);
  if (v !== undefined || !name.startsWith('w:')) return v;
  return ooxmlAttr(node, name.slice(2));
}
