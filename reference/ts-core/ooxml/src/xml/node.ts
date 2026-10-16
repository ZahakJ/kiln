/**
 * The XML tree used throughout Kiln's file-format code. Order is significant in OOXML, so a node's
 * children are an ordered array and attributes keep their document order.
 */

export interface XmlElement {
  readonly type: 'element';
  /** Qualified name with the canonical prefix (`w:p`, `a:srgbClr`, `sheetData`). */
  name: string;
  /** Attributes in document order, including `xmlns:*` declarations parsed from the source. */
  attrs: Record<string, string>;
  children: XmlNode[];
}

export interface XmlText {
  readonly type: 'text';
  text: string;
  /** Emitted as a CDATA section when true. */
  cdata?: boolean;
}

export interface XmlComment {
  readonly type: 'comment';
  text: string;
}

export interface XmlProcessingInstruction {
  readonly type: 'pi';
  target: string;
  /** The instruction's pseudo-attributes (`progid="Word.Document"`). */
  attrs: Record<string, string>;
}

export type XmlNode = XmlElement | XmlText | XmlComment | XmlProcessingInstruction;

export interface XmlDeclaration {
  version: string;
  encoding?: string;
  standalone?: string;
}

export interface XmlDocument {
  declaration?: XmlDeclaration;
  /** Comments and processing instructions before the root element. */
  prolog: XmlNode[];
  root: XmlElement;
}

function makeAttrs(): Record<string, string> {
  return Object.create(null) as Record<string, string>;
}

/** Copies attributes into a prototype-less record, dropping `undefined`, so keys such as `__proto__` are inert. */
export function attrsFrom(
  src?: Record<string, string | number | boolean | undefined | null>,
): Record<string, string> {
  const out = makeAttrs();
  if (!src) return out;
  for (const [k, v] of Object.entries(src)) {
    if (v === undefined || v === null) continue;
    out[k] = typeof v === 'boolean' ? (v ? '1' : '0') : String(v);
  }
  return out;
}

export type ChildInput = XmlNode | string | null | undefined | false | readonly ChildInput[];

function pushChildren(out: XmlNode[], input: ChildInput): void {
  if (input === null || input === undefined || input === false) return;
  if (typeof input === 'string') {
    out.push({ type: 'text', text: input });
  } else if (Array.isArray(input)) {
    for (const c of input as readonly ChildInput[]) pushChildren(out, c);
  } else {
    out.push(input as XmlNode);
  }
}

/**
 * Element builder. Attribute values that are `undefined`/`null` are omitted; booleans become `1`/`0`;
 * children may be nodes, strings (text) or nested arrays, with `null`/`false` skipped.
 *
 * ```ts
 * el('a:solidFill', {}, el('a:srgbClr', { val: '4472C4' }))
 * ```
 */
export function el(
  name: string,
  attrs?: Record<string, string | number | boolean | undefined | null>,
  ...children: ChildInput[]
): XmlElement {
  const out: XmlNode[] = [];
  for (const c of children) pushChildren(out, c);
  return { type: 'element', name, attrs: attrsFrom(attrs), children: out };
}

export function textNode(text: string): XmlText {
  return { type: 'text', text };
}

export function isElement(node: XmlNode | undefined | null): node is XmlElement {
  return !!node && node.type === 'element';
}

/** Deep-clones a node (attributes and children are copied). */
export function cloneNode<T extends XmlNode>(node: T): T {
  switch (node.type) {
    case 'element':
      return {
        type: 'element',
        name: node.name,
        attrs: attrsFrom(node.attrs),
        children: node.children.map(cloneNode),
      } as T;
    case 'pi':
      return { type: 'pi', target: node.target, attrs: attrsFrom(node.attrs) } as T;
    default:
      return { ...node };
  }
}
