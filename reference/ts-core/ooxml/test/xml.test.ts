import { describe, expect, it } from 'vitest';
import { buildXml, stripInvalidXmlChars, undeclaredPrefixes } from '../src/xml/build.ts';
import { canonicalPrefix, NS, splitQName } from '../src/xml/namespaces.ts';
import { cloneNode, el } from '../src/xml/node.ts';
import { decodeXmlBytes, parseXml, XmlParseError } from '../src/xml/parse.ts';
import {
  attr,
  attrBool,
  attrInt,
  attrNum,
  child,
  childPath,
  children,
  descendants,
  removeChildren,
  selectAlternateContent,
  setAttr,
  text,
  toggle,
  upsertChild,
} from '../src/xml/query.ts';

const W = `xmlns:w="${NS.w}"`;

describe('parseXml / buildXml', () => {
  it('round-trips an element tree, keeping attribute order and the declaration', () => {
    const xml = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\r\n<w:document ${W} xmlns:mc="${NS.mc}" xmlns:w14="${NS.w14}" mc:Ignorable="w14"><w:body><w:p w14:paraId="1A2B3C4D" w:rsidR="00AB"><w:r><w:t xml:space="preserve"> a &amp; b &lt;c&gt; "q" </w:t></w:r></w:p></w:body></w:document>`;
    const doc = parseXml(xml);
    expect(doc.declaration).toEqual({ version: '1.0', encoding: 'UTF-8', standalone: 'yes' });
    expect(buildXml(doc)).toBe(xml);
  });

  it('decodes character references and escapes on output', () => {
    const doc = parseXml(`<a t="x&#10;y&quot;">&#x41;&#66;&amp;</a>`);
    expect(doc.root.attrs.t).toBe('x\ny"');
    expect(text(doc.root)).toBe('AB&');
    expect(buildXml(doc.root, { declaration: false })).toBe('<a t="x&#10;y&quot;">AB&amp;</a>');
  });

  it('keeps significant whitespace and drops indentation between elements', () => {
    const doc = parseXml(
      `<w:p ${W}>\n  <w:r>\n    <w:t xml:space="preserve">  two  </w:t>\n  </w:r>\n  <w:r><w:t>  </w:t></w:r>\n</w:p>`,
    );
    expect(doc.root.children).toHaveLength(2);
    expect(text(doc.root)).toBe('  two    ');
  });

  it('keeps whitespace inside xml:space="preserve" scopes', () => {
    const doc = parseXml(`<x xml:space="preserve"> <y/> </x>`);
    expect(doc.root.children).toHaveLength(3);
  });

  it('round-trips comments, CDATA and processing instructions', () => {
    const xml = `<?mso-application progid="Word.Document"?><a><!-- note --><b><![CDATA[x < y]]></b></a>`;
    const doc = parseXml(xml);
    expect(doc.prolog[0]).toMatchObject({ type: 'pi', target: 'mso-application' });
    expect(buildXml(doc, { declaration: false })).toBe(xml);
  });

  it('rejects DTDs and malformed input', () => {
    expect(() => parseXml('<!DOCTYPE x [<!ENTITY a "b">]><x>&a;</x>')).toThrow(XmlParseError);
    expect(() => parseXml('')).toThrow(XmlParseError);
    expect(() => parseXml('<a/><b/>')).toThrow(XmlParseError);
  });

  it('normalises non-canonical prefixes to the Office prefix table', () => {
    const doc = parseXml(
      `<ns0:document xmlns:ns0="${NS.w}" xmlns:mc="${NS.mc}" xmlns:x1="${NS.w14}" mc:Ignorable="x1"><ns0:body><ns0:p x1:paraId="1"/></ns0:body></ns0:document>`,
    );
    expect(doc.root.name).toBe('w:document');
    expect(doc.root.attrs['xmlns:w']).toBe(NS.w);
    expect(doc.root.attrs['mc:Ignorable']).toBe('w14');
    expect(childPath(doc.root, 'w:body', 'w:p')?.attrs['w14:paraId']).toBe('1');
    // The result is still valid, namespace-correct XML.
    expect(undeclaredPrefixes(doc.root)).toEqual([]);
  });

  it('moves a default WordprocessingML namespace onto the w prefix', () => {
    const doc = parseXml(`<document xmlns="${NS.w}"><body><p/></body></document>`);
    expect(doc.root.name).toBe('w:document');
    expect(child(doc.root, 'w:body')).toBeDefined();
    expect(doc.root.attrs['xmlns:w']).toBe(NS.w);
  });

  it('keeps SpreadsheetML in the default namespace and maps Strict URIs to transitional prefixes', () => {
    const sheet = parseXml(`<x:worksheet xmlns:x="${NS.x}"><x:sheetData/></x:worksheet>`);
    expect(sheet.root.name).toBe('worksheet');
    expect(sheet.root.attrs.xmlns).toBe(NS.x);
    const strict = parseXml(`<w:document xmlns:w="http://purl.oclc.org/ooxml/wordprocessingml/main"/>`);
    expect(strict.root.name).toBe('w:document');
    expect(canonicalPrefix('http://purl.oclc.org/ooxml/drawingml/main')).toBe('a');
  });

  it('can leave prefixes untouched', () => {
    const doc = parseXml(`<ns0:a xmlns:ns0="${NS.w}"/>`, { normalizePrefixes: false });
    expect(doc.root.name).toBe('ns0:a');
  });

  it('declares used namespaces on the root, only when used, in table order', () => {
    const tree = el('w:document', {}, el('w:body', {}, el('w:p', { 'w14:paraId': '1' }, el('m:oMath'))));
    expect(undeclaredPrefixes(tree)).toEqual(['w', 'w14', 'm']);
    const xml = buildXml(tree, { declaration: false });
    expect(xml).toBe(
      `<w:document xmlns:w="${NS.w}" xmlns:w14="${NS.w14}" xmlns:m="${NS.m}"><w:body><w:p w14:paraId="1"><m:oMath/></w:p></w:body></w:document>`,
    );
    expect(xml).not.toContain('xmlns:a=');
  });

  it('treats prefixes named by mc:Ignorable as used', () => {
    const tree = el('w:document', { 'mc:Ignorable': 'w14 wp14' });
    const xml = buildXml(tree, { declaration: false });
    expect(xml).toContain(`xmlns:mc="${NS.mc}"`);
    expect(xml).toContain(`xmlns:w14="${NS.w14}"`);
    expect(xml).toContain(`xmlns:wp14="${NS.wp14}"`);
    expect(xml.indexOf('xmlns:')).toBeLessThan(xml.indexOf('mc:Ignorable'));
  });

  it('refuses unknown undeclared prefixes', () => {
    expect(() => buildXml(el('zz:x'))).toThrow(/zz/);
  });

  it('strips characters XML 1.0 forbids', () => {
    expect(stripInvalidXmlChars('a\u0001b\u0008c\tok￾\uD800x😀')).toBe('abc\tokx😀');
    expect(buildXml(el('t', { v: 'x\u0002' }, 'y\u0000'), { declaration: false })).toBe('<t v="x">y</t>');
  });

  it('decodes UTF-16 and BOM-prefixed parts', () => {
    const s = '<a>é</a>';
    const le = new Uint8Array([0xff, 0xfe, ...[...s].flatMap((c) => [c.charCodeAt(0), 0])]);
    expect(decodeXmlBytes(le)).toBe(s);
    const be = new Uint8Array([0xfe, 0xff, ...[...s].flatMap((c) => [0, c.charCodeAt(0)])]);
    expect(decodeXmlBytes(be)).toBe(s);
    const u8 = new Uint8Array([0xef, 0xbb, 0xbf, ...new TextEncoder().encode(s)]);
    expect(decodeXmlBytes(u8)).toBe(s);
    expect(text(parseXml(le).root)).toBe('é');
  });

  it('is safe against prototype pollution', () => {
    // fast-xml-parser refuses names that could pollute prototypes; no OOXML vocabulary uses them.
    expect(() => parseXml('<a __proto__="x"/>')).toThrow(XmlParseError);
    expect(() => parseXml('<a constructor="y"/>')).toThrow(XmlParseError);
    // Attribute records have no prototype, so inherited names never read as attributes.
    const doc = parseXml('<a val="z"/>');
    expect(attr(doc.root, 'toString')).toBeUndefined();
    expect(attr(doc.root, 'hasOwnProperty')).toBeUndefined();
    expect(({} as Record<string, unknown>).y).toBeUndefined();
  });
});

describe('query helpers', () => {
  const doc = parseXml(
    `<w:p ${W}><w:pPr><w:jc w:val="center"/><w:keepNext/><w:widowControl w:val="0"/></w:pPr><w:r><w:t>A</w:t></w:r><w:r><w:t>B</w:t></w:r></w:p>`,
  );
  const p = doc.root;

  it('navigates children', () => {
    expect(child(p, 'w:pPr')?.name).toBe('w:pPr');
    expect(child(p, 'w:nope')).toBeUndefined();
    expect(child(undefined, 'w:pPr')).toBeUndefined();
    expect(children(p, 'w:r')).toHaveLength(2);
    expect(children(p)).toHaveLength(3);
    expect(children(p, ['w:r', 'w:pPr'])).toHaveLength(3);
    expect(descendants(p, 'w:t').map((t) => text(t))).toEqual(['A', 'B']);
    expect(attr(childPath(p, 'w:pPr', 'w:jc'), 'w:val')).toBe('center');
  });

  it('reads typed attributes and toggles', () => {
    const e = el('x', { i: '42', n: '-1.5', b1: 'on', b0: 'false', bad: 'x1' });
    expect(attrInt(e, 'i')).toBe(42);
    expect(attrInt(e, 'n')).toBeUndefined();
    expect(attrNum(e, 'n')).toBe(-1.5);
    expect(attrBool(e, 'b1')).toBe(true);
    expect(attrBool(e, 'b0')).toBe(false);
    expect(attrBool(e, 'missing', true)).toBe(true);
    expect(attrNum(e, 'bad')).toBeUndefined();
    const ppr = child(p, 'w:pPr');
    expect(toggle(ppr, 'w:keepNext')).toBe(true);
    expect(toggle(ppr, 'w:widowControl')).toBe(false);
    expect(toggle(ppr, 'w:keepLines')).toBeUndefined();
  });

  it('edits trees', () => {
    const copy = cloneNode(p);
    setAttr(copy, 'w:rsidR', '00FF');
    setAttr(copy, 'w:rsidR', undefined);
    expect(copy.attrs['w:rsidR']).toBeUndefined();
    expect(removeChildren(copy, 'w:r')).toBe(2);
    upsertChild(copy, el('w:r', {}, el('w:t', {}, 'C')));
    upsertChild(copy, el('w:pPr'), ['w:r']);
    expect(children(copy).map((c) => c.name)).toEqual(['w:pPr', 'w:r']);
    expect(text(copy)).toBe('C');
    // The original is untouched.
    expect(children(p, 'w:r')).toHaveLength(2);
  });

  it('selects mc:AlternateContent branches', () => {
    const ac = parseXml(
      `<mc:AlternateContent xmlns:mc="${NS.mc}" xmlns:wps="${NS.wps}"><mc:Choice Requires="wps"><wps:wsp/></mc:Choice><mc:Fallback><w:pict ${W}/></mc:Fallback></mc:AlternateContent>`,
    ).root;
    expect(selectAlternateContent(ac, new Set(['wps']))[0]?.name).toBe('wps:wsp');
    expect(selectAlternateContent(ac, new Set())[0]?.name).toBe('w:pict');
  });

  it('splits qualified names', () => {
    expect(splitQName('w:p')).toEqual(['w', 'p']);
    expect(splitQName('sheetData')).toEqual(['', 'sheetData']);
  });
});
