import { type ChildInput, el, type XmlElement, type XmlNode } from './node.ts';

/** First child element named `name`, or `undefined`. */
export function child(node: XmlElement | undefined, name: string): XmlElement | undefined {
  if (!node) return undefined;
  for (const c of node.children) if (c.type === 'element' && c.name === name) return c;
  return undefined;
}

/** Child elements, optionally only those named `name` (or any of `names`). */
export function children(node: XmlElement | undefined, name?: string | readonly string[]): XmlElement[] {
  if (!node) return [];
  const out: XmlElement[] = [];
  for (const c of node.children) {
    if (c.type !== 'element') continue;
    if (name === undefined || (typeof name === 'string' ? c.name === name : name.includes(c.name)))
      out.push(c);
  }
  return out;
}

/** First child element (of any name). */
export function firstElement(node: XmlElement | undefined): XmlElement | undefined {
  if (!node) return undefined;
  for (const c of node.children) if (c.type === 'element') return c;
  return undefined;
}

/** Follows a chain of child names: `childPath(sp, 'p:spPr', 'a:xfrm', 'a:off')`. */
export function childPath(node: XmlElement | undefined, ...names: string[]): XmlElement | undefined {
  let cur = node;
  for (const n of names) cur = child(cur, n);
  return cur;
}

/** All descendant elements named `name`, in document order. */
export function descendants(node: XmlElement | undefined, name: string): XmlElement[] {
  const out: XmlElement[] = [];
  const walk = (n: XmlElement) => {
    for (const c of n.children) {
      if (c.type !== 'element') continue;
      if (c.name === name) out.push(c);
      walk(c);
    }
  };
  if (node) walk(node);
  return out;
}

export function attr(node: XmlElement | undefined, name: string): string | undefined {
  if (!node) return undefined;
  return Object.hasOwn(node.attrs, name) ? node.attrs[name] : undefined;
}

/** Integer attribute; `undefined` when absent or not an integer. */
export function attrInt(node: XmlElement | undefined, name: string): number | undefined {
  const v = attr(node, name);
  if (v === undefined || !/^\s*[+-]?\d+\s*$/.test(v)) return undefined;
  return Number.parseInt(v, 10);
}

/** Numeric attribute; `undefined` when absent or not a finite number. */
export function attrNum(node: XmlElement | undefined, name: string): number | undefined {
  const v = attr(node, name);
  if (v === undefined || v.trim() === '') return undefined;
  const n = Number(v);
  return Number.isFinite(n) ? n : undefined;
}

/**
 * Boolean attribute per ST_OnOff / xsd:boolean (`true|false|1|0|on|off`). Returns `fallback` when absent.
 */
export function attrBool(
  node: XmlElement | undefined,
  name: string,
  fallback?: boolean,
): boolean | undefined {
  const v = attr(node, name);
  if (v === undefined) return fallback;
  switch (v.trim().toLowerCase()) {
    case 'true':
    case '1':
    case 'on':
      return true;
    case 'false':
    case '0':
    case 'off':
      return false;
    default:
      return fallback;
  }
}

/** WordprocessingML toggle: `<w:b/>` = on, `<w:b w:val="0"/>` = off, absent = `undefined`. */
export function toggle(node: XmlElement | undefined, name: string, valAttr = 'w:val'): boolean | undefined {
  const c = child(node, name);
  if (!c) return undefined;
  return attrBool(c, valAttr, true);
}

/** Concatenated text of all descendant text nodes. */
export function text(node: XmlNode | undefined): string {
  if (!node) return '';
  if (node.type === 'text') return node.text;
  if (node.type !== 'element') return '';
  let s = '';
  for (const c of node.children) s += text(c);
  return s;
}

export function setAttr(node: XmlElement, name: string, value: string | number | boolean | undefined): void {
  if (value === undefined) {
    delete node.attrs[name];
    return;
  }
  node.attrs[name] = typeof value === 'boolean' ? (value ? '1' : '0') : String(value);
}

export function appendChild(node: XmlElement, ...kids: ChildInput[]): XmlElement {
  const tmp = el('tmp', {}, ...kids);
  node.children.push(...tmp.children);
  return node;
}

/** Removes child elements named `name`; returns how many were removed. */
export function removeChildren(node: XmlElement, name: string): number {
  const before = node.children.length;
  node.children = node.children.filter((c) => !(c.type === 'element' && c.name === name));
  return before - node.children.length;
}

/**
 * Replaces the first child named `replacement.name` in place, or inserts it before the first child whose
 * name appears in `before` (schema order), or appends it.
 */
export function upsertChild(node: XmlElement, replacement: XmlElement, before: readonly string[] = []): void {
  const i = node.children.findIndex((c) => c.type === 'element' && c.name === replacement.name);
  if (i >= 0) {
    node.children[i] = replacement;
    return;
  }
  const j = node.children.findIndex((c) => c.type === 'element' && before.includes(c.name));
  if (j >= 0) node.children.splice(j, 0, replacement);
  else node.children.push(replacement);
}

/**
 * Resolves `mc:AlternateContent` blocks: picks the first `mc:Choice` whose `Requires` prefixes are all
 * supported, else `mc:Fallback`, and returns the chosen branch's children (empty when nothing applies).
 */
export function selectAlternateContent(ac: XmlElement, supported: ReadonlySet<string>): XmlElement[] {
  for (const choice of children(ac, 'mc:Choice')) {
    const req = (attr(choice, 'Requires') ?? '').split(/\s+/).filter(Boolean);
    if (req.every((p) => supported.has(p))) return children(choice);
  }
  return children(child(ac, 'mc:Fallback'));
}
