import { XMLParser } from 'fast-xml-parser';
import { canonicalPrefix, PREFIX_LIST_ATTRIBUTES, splitQName } from './namespaces.ts';
import { attrsFrom, type XmlDeclaration, type XmlDocument, type XmlElement, type XmlNode } from './node.ts';

export interface ParseXmlOptions {
  /**
   * Rename namespace prefixes to the canonical Office prefixes from the namespace table (a document that
   * binds `ns0` to the WordprocessingML namespace comes back with `w:` names). Default `true`.
   */
  normalizePrefixes?: boolean;
  /**
   * Drop whitespace-only text between elements (indentation). Text inside leaf elements and inside
   * `xml:space="preserve"` scopes is always kept. Default `true`.
   */
  dropInterElementWhitespace?: boolean;
}

export class XmlParseError extends Error {
  override name = 'XmlParseError';
}

const parser = new XMLParser({
  preserveOrder: true,
  ignoreAttributes: false,
  attributeNamePrefix: '',
  trimValues: false,
  parseTagValue: false,
  parseAttributeValue: false,
  processEntities: true,
  htmlEntities: true,
  ignoreDeclaration: false,
  ignorePiTags: false,
  commentPropName: '#comment',
  cdataPropName: '#cdata',
  allowBooleanAttributes: false,
  // Word nests tables inside tables; real documents exceed the parser's default depth of 100.
  maxNestedTags: 2000,
});

const utf8 = new TextDecoder('utf-8', { fatal: false });

/** Decodes XML bytes honouring a UTF-8 / UTF-16 byte order mark. */
export function decodeXmlBytes(bytes: Uint8Array): string {
  if (bytes.length >= 2 && bytes[0] === 0xff && bytes[1] === 0xfe) {
    return new TextDecoder('utf-16le').decode(bytes.subarray(2));
  }
  if (bytes.length >= 2 && bytes[0] === 0xfe && bytes[1] === 0xff) {
    return new TextDecoder('utf-16be').decode(bytes.subarray(2));
  }
  // Without a BOM, a UTF-16 document starts with "<\0" or "\0<".
  if (bytes.length >= 2 && bytes[0] === 0x3c && bytes[1] === 0x00) {
    return new TextDecoder('utf-16le').decode(bytes);
  }
  if (bytes.length >= 2 && bytes[0] === 0x00 && bytes[1] === 0x3c) {
    return new TextDecoder('utf-16be').decode(bytes);
  }
  const s = utf8.decode(bytes);
  return s.charCodeAt(0) === 0xfeff ? s.slice(1) : s;
}

type FxpEntry = Record<string, unknown> & { ':@'?: Record<string, string> };

interface Scope {
  /** prefix in the source document → namespace URI */
  src: Map<string, string>;
  /** prefix in the source document → prefix in our tree */
  rename: Map<string, string>;
  /** prefix in our tree → namespace URI (to avoid binding one prefix to two URIs) */
  out: Map<string, string>;
}

function childScope(parent: Scope): Scope {
  return { src: new Map(parent.src), rename: new Map(parent.rename), out: new Map(parent.out) };
}

function entryName(entry: FxpEntry): string {
  for (const k of Object.keys(entry)) if (k !== ':@') return k;
  throw new XmlParseError('empty XML entry');
}

/**
 * Parses an XML part into an ordered tree. Rejects DTDs (OOXML forbids them, and refusing them closes the
 * entity-expansion attack surface).
 */
export function parseXml(input: string | Uint8Array, options: ParseXmlOptions = {}): XmlDocument {
  const normalize = options.normalizePrefixes ?? true;
  const dropWs = options.dropInterElementWhitespace ?? true;
  const xml = typeof input === 'string' ? input : decodeXmlBytes(input);
  if (/<!DOCTYPE/i.test(xml)) throw new XmlParseError('DTDs are not allowed in Office Open XML parts');

  let raw: FxpEntry[];
  try {
    raw = parser.parse(xml) as FxpEntry[];
  } catch (e) {
    throw new XmlParseError(`malformed XML: ${(e as Error).message}`);
  }

  let declaration: XmlDeclaration | undefined;
  const prolog: XmlNode[] = [];
  let root: XmlElement | undefined;
  const rootScope: Scope = {
    src: new Map([['xml', 'http://www.w3.org/XML/1998/namespace']]),
    rename: new Map(),
    out: new Map([['xml', 'http://www.w3.org/XML/1998/namespace']]),
  };

  const convert = (entry: FxpEntry, scope: Scope, preserveSpace: boolean): XmlNode | null => {
    const name = entryName(entry);
    const value = entry[name];
    if (name === '#text') return { type: 'text', text: String(value) };
    if (name === '#comment') return { type: 'comment', text: textOf(value) };
    if (name === '#cdata') return { type: 'text', text: textOf(value), cdata: true };
    if (name.startsWith('?')) {
      return { type: 'pi', target: name.slice(1), attrs: attrsFrom(entry[':@']) };
    }
    const srcAttrs = entry[':@'] ?? {};
    const scopeHere = normalize ? childScope(scope) : scope;
    const attrs = attrsFrom();

    if (normalize) {
      // First pass: bind this element's declarations and decide renames.
      for (const [k, v] of Object.entries(srcAttrs)) {
        if (k === 'xmlns') scopeHere.src.set('', v);
        else if (k.startsWith('xmlns:')) scopeHere.src.set(k.slice(6), v);
      }
      for (const [k, v] of Object.entries(srcAttrs)) {
        if (k !== 'xmlns' && !k.startsWith('xmlns:')) continue;
        const srcPrefix = k === 'xmlns' ? '' : k.slice(6);
        const canon = canonicalPrefix(v);
        let target = srcPrefix;
        if (canon !== undefined && canon !== srcPrefix) {
          const bound = scopeHere.out.get(canon);
          if (bound === undefined || bound === v) target = canon;
        }
        scopeHere.rename.set(srcPrefix, target);
        scopeHere.out.set(target, v);
      }
      for (const [k, v] of Object.entries(srcAttrs)) {
        if (k === 'xmlns' || k.startsWith('xmlns:')) {
          const srcPrefix = k === 'xmlns' ? '' : k.slice(6);
          const target = scopeHere.rename.get(srcPrefix) ?? srcPrefix;
          const outKey = target === '' ? 'xmlns' : `xmlns:${target}`;
          // A default-namespace declaration that moved to a prefix no longer applies to unprefixed names.
          if (!(outKey in attrs)) attrs[outKey] = v;
          continue;
        }
        attrs[renameQName(k, scopeHere, true)] = PREFIX_LIST_ATTRIBUTES.has(k)
          ? v
              .split(/\s+/)
              .filter(Boolean)
              .map((p) => scopeHere.rename.get(p) ?? p)
              .join(' ')
          : v;
      }
    } else {
      for (const [k, v] of Object.entries(srcAttrs)) attrs[k] = v;
    }

    const elName = normalize ? renameQName(name, scopeHere, false) : name;
    const space = attrs['xml:space'];
    const preserve = space === 'preserve' ? true : space === 'default' ? false : preserveSpace;
    const kids: XmlNode[] = [];
    if (Array.isArray(value)) {
      for (const c of value as FxpEntry[]) {
        const n = convert(c, scopeHere, preserve);
        if (n) kids.push(n);
      }
    }
    const node: XmlElement = { type: 'element', name: elName, attrs, children: kids };
    if (dropWs && !preserve && kids.some((k) => k.type === 'element')) {
      node.children = kids.filter((k) => !(k.type === 'text' && !k.cdata && /^\s*$/.test(k.text)));
    }
    return node;
  };

  for (const entry of raw) {
    const name = entryName(entry);
    if (name === '?xml') {
      const a = entry[':@'] ?? {};
      declaration = { version: a.version ?? '1.0' };
      if (a.encoding !== undefined) declaration.encoding = a.encoding;
      if (a.standalone !== undefined) declaration.standalone = a.standalone;
      continue;
    }
    if (name === '#text') {
      if (/^\s*$/.test(String(entry[name]))) continue;
      throw new XmlParseError('text outside the root element');
    }
    const node = convert(entry, rootScope, false);
    if (!node) continue;
    if (node.type === 'element') {
      if (root) throw new XmlParseError('more than one root element');
      root = node;
    } else if (!root) {
      prolog.push(node);
    }
  }
  if (!root) throw new XmlParseError('no root element');
  const doc: XmlDocument = { prolog, root };
  if (declaration) doc.declaration = declaration;
  return doc;
}

function textOf(value: unknown): string {
  if (Array.isArray(value)) return value.map((v) => String((v as FxpEntry)['#text'] ?? '')).join('');
  return String(value ?? '');
}

function renameQName(qname: string, scope: Scope, isAttribute: boolean): string {
  const [prefix, local] = splitQName(qname);
  if (prefix === '' && isAttribute) return qname; // unprefixed attributes are in no namespace
  if (prefix === 'xml' || prefix === 'xmlns') return qname;
  const target = scope.rename.get(prefix);
  if (target === undefined || target === prefix) return qname;
  return target === '' ? local : `${target}:${local}`;
}

/** Parses and returns only the root element. */
export function parseXmlElement(input: string | Uint8Array, options?: ParseXmlOptions): XmlElement {
  return parseXml(input, options).root;
}
