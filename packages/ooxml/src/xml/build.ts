import { namespaceForPrefix, PREFIX_LIST_ATTRIBUTES, prefixRank, splitQName } from './namespaces.ts';
import type { XmlDeclaration, XmlDocument, XmlElement, XmlNode } from './node.ts';

export interface BuildXmlOptions {
  /**
   * The XML declaration. `true` (default) writes Office's
   * `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>` followed by CRLF, or the document's own
   * declaration when building an `XmlDocument`; `false` omits it.
   */
  declaration?: boolean | XmlDeclaration;
  /**
   * Declare namespaces that the tree uses but never declares, using the stable prefix table, on the root
   * element. Prefixes named by `mc:Ignorable` and friends count as used. Default `true`.
   */
  declareNamespaces?: boolean;
}

export class XmlBuildError extends Error {
  override name = 'XmlBuildError';
}

const OFFICE_DECLARATION: XmlDeclaration = { version: '1.0', encoding: 'UTF-8', standalone: 'yes' };

/**
 * Removes characters XML 1.0 forbids (most C0 controls, U+FFFE/U+FFFF, unpaired surrogates) rather than
 * producing a part that Office refuses to open.
 */
export function stripInvalidXmlChars(s: string): string {
  let out = '';
  let start = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    let ok: boolean;
    if (c >= 0x20 && c < 0xd800) ok = true;
    else if (c === 0x09 || c === 0x0a || c === 0x0d) ok = true;
    else if (c >= 0xd800 && c <= 0xdbff) {
      const n = s.charCodeAt(i + 1);
      ok = n >= 0xdc00 && n <= 0xdfff;
      if (ok) i++;
    } else if (c >= 0xdc00 && c <= 0xdfff) ok = false;
    else ok = c >= 0xe000 && c <= 0xfffd;
    if (!ok) {
      out += s.slice(start, i);
      start = i + 1;
    }
  }
  return start === 0 ? s : out + s.slice(start);
}

export function escapeText(s: string): string {
  return stripInvalidXmlChars(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

export function escapeAttr(s: string): string {
  return stripInvalidXmlChars(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/"/g, '&quot;')
    .replace(/\n/g, '&#10;')
    .replace(/\r/g, '&#13;')
    .replace(/\t/g, '&#9;');
}

function declarationString(d: XmlDeclaration): string {
  let s = `<?xml version="${d.version}"`;
  if (d.encoding) s += ` encoding="${d.encoding}"`;
  if (d.standalone) s += ` standalone="${d.standalone}"`;
  return `${s}?>\r\n`;
}

/**
 * Finds prefixes used in `root`'s subtree that are not declared where they are used. Walks with scope so
 * a declaration in one branch does not satisfy a use in a sibling.
 */
export function undeclaredPrefixes(root: XmlElement): string[] {
  const missing = new Set<string>();
  const walk = (node: XmlElement, declared: Set<string>) => {
    let scope = declared;
    for (const k of Object.keys(node.attrs)) {
      if (k === 'xmlns' || k.startsWith('xmlns:')) {
        if (scope === declared) scope = new Set(declared);
        scope.add(k === 'xmlns' ? '' : k.slice(6));
      }
    }
    const use = (p: string) => {
      if (p && p !== 'xml' && p !== 'xmlns' && !scope.has(p)) missing.add(p);
    };
    use(splitQName(node.name)[0]);
    for (const [k, v] of Object.entries(node.attrs)) {
      if (k === 'xmlns' || k.startsWith('xmlns:')) continue;
      use(splitQName(k)[0]);
      if (PREFIX_LIST_ATTRIBUTES.has(k)) for (const p of v.split(/\s+/)) if (p) use(p);
    }
    for (const c of node.children) if (c.type === 'element') walk(c, scope);
  };
  walk(root, new Set(['xml']));
  return [...missing].sort((a, b) => prefixRank(a) - prefixRank(b) || a.localeCompare(b));
}

function withDeclarations(root: XmlElement): XmlElement {
  const missing = undeclaredPrefixes(root);
  if (missing.length === 0) return root;
  const decls: [string, string][] = [];
  for (const p of missing) {
    const uri = namespaceForPrefix(p);
    if (!uri) throw new XmlBuildError(`namespace prefix "${p}" is used but neither declared nor known`);
    decls.push([`xmlns:${p}`, uri]);
  }
  // Declarations go after the element's existing xmlns attributes and before ordinary attributes.
  const attrs = Object.create(null) as Record<string, string>;
  const entries = Object.entries(root.attrs);
  const firstPlain = entries.findIndex(([k]) => k !== 'xmlns' && !k.startsWith('xmlns:'));
  const split = firstPlain < 0 ? entries.length : firstPlain;
  for (const [k, v] of entries.slice(0, split)) attrs[k] = v;
  for (const [k, v] of decls) attrs[k] = v;
  for (const [k, v] of entries.slice(split)) attrs[k] = v;
  return { ...root, attrs };
}

function writeNode(node: XmlNode, out: string[]): void {
  switch (node.type) {
    case 'text':
      if (node.cdata) out.push(`<![CDATA[${node.text.replace(/]]>/g, ']]]]><![CDATA[>')}]]>`);
      else out.push(escapeText(node.text));
      return;
    case 'comment':
      out.push(`<!--${node.text.replace(/--/g, '- -')}-->`);
      return;
    case 'pi': {
      let s = `<?${node.target}`;
      for (const [k, v] of Object.entries(node.attrs)) s += ` ${k}="${escapeAttr(v)}"`;
      out.push(`${s}?>`);
      return;
    }
    case 'element': {
      let s = `<${node.name}`;
      for (const [k, v] of Object.entries(node.attrs)) s += ` ${k}="${escapeAttr(v)}"`;
      if (node.children.length === 0) {
        out.push(`${s}/>`);
        return;
      }
      out.push(`${s}>`);
      for (const c of node.children) writeNode(c, out);
      out.push(`</${node.name}>`);
    }
  }
}

/**
 * Serialises a tree without indentation, as Office does. Attribute order is the tree's order, so output
 * is deterministic.
 */
export function buildXml(input: XmlDocument | XmlElement, options: BuildXmlOptions = {}): string {
  const isDoc = (input as XmlDocument).root !== undefined && (input as XmlElement).type !== 'element';
  const doc = isDoc ? (input as XmlDocument) : undefined;
  let root = doc ? doc.root : (input as XmlElement);
  if (options.declareNamespaces ?? true) root = withDeclarations(root);
  const out: string[] = [];
  const decl = options.declaration ?? true;
  if (decl !== false) {
    out.push(declarationString(decl === true ? (doc?.declaration ?? OFFICE_DECLARATION) : decl));
  }
  if (doc) for (const n of doc.prolog) writeNode(n, out);
  writeNode(root, out);
  return out.join('');
}

const encoder = new TextEncoder();

/** `buildXml` encoded as UTF-8 bytes, ready for `OpcPackage.setPart`. */
export function buildXmlBytes(input: XmlDocument | XmlElement, options?: BuildXmlOptions): Uint8Array {
  return encoder.encode(buildXml(input, options));
}
