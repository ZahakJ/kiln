import { strToU8, unzipSync, zipSync } from 'fflate';
import { describe, expect, it } from 'vitest';
import { ContentType, ContentTypes } from '../src/opc/content-types.ts';
import { EncryptedOrLegacyFileError, OpcError, OpcPackage, ZipBombError } from '../src/opc/package.ts';
import {
  assertPartName,
  extensionOf,
  relativeTarget,
  relsPartName,
  resolveTarget,
  sourcePartOfRels,
} from '../src/opc/part-name.ts';
import { RelType, relTypeMatches } from '../src/opc/rel-types.ts';
import { el } from '../src/xml/node.ts';
import { child, text } from '../src/xml/query.ts';
import { featureFixtures, fixture } from './fixtures.ts';

const ALL = [...featureFixtures('docx'), ...featureFixtures('xlsx'), ...featureFixtures('pptx')];

const same = (a: Uint8Array | undefined, b: Uint8Array | undefined) =>
  !!a &&
  !!b &&
  Buffer.from(a.buffer, a.byteOffset, a.byteLength).equals(Buffer.from(b.buffer, b.byteOffset, b.byteLength));

function zipOrder(bytes: Uint8Array): string[] {
  return Object.keys(unzipSync(bytes));
}

describe('OPC round trip of the generated corpus', () => {
  it('has generated fixtures for every format', () => {
    expect(featureFixtures('docx').length).toBeGreaterThanOrEqual(15);
    expect(featureFixtures('xlsx').length).toBeGreaterThanOrEqual(15);
    expect(featureFixtures('pptx').length).toBeGreaterThanOrEqual(15);
  });

  it.each(ALL)('%s: untouched parts are byte-identical after open → save → open', (name) => {
    const original = fixture(name);
    const src = unzipSync(original);
    const pkg = OpcPackage.open(original);
    const saved = pkg.save();
    const out = unzipSync(saved);
    expect(Object.keys(out).sort()).toEqual(Object.keys(src).sort());
    for (const [entry, bytes] of Object.entries(src)) expect(same(out[entry], bytes), entry).toBe(true);
    const order = zipOrder(saved);
    expect(order[0]).toBe('[Content_Types].xml');
    expect(order[1]).toBe('_rels/.rels');
    // Re-opening gives the same parts.
    expect(OpcPackage.open(saved).allPartNames()).toEqual(pkg.allPartNames());
  });

  it('keeps other parts byte-identical when one part changes', () => {
    const original = fixture('docx/features/styles.docx');
    const src = unzipSync(original);
    const pkg = OpcPackage.open(original);
    const main = pkg.mainDocumentPart();
    expect(main).toBe('/word/document.xml');
    const doc = pkg.getPartXml(main as string);
    expect(doc).toBeDefined();
    const body = child(doc?.root, 'w:body');
    body?.children.unshift(el('w:p', {}, el('w:r', {}, el('w:t', {}, 'Inserted by the test'))));
    pkg.setPart(main as string, doc as NonNullable<typeof doc>);
    const out = unzipSync(pkg.save());
    for (const [entry, bytes] of Object.entries(src)) {
      if (entry === 'word/document.xml') continue;
      expect(same(out[entry], bytes), entry).toBe(true);
    }
    const reread = OpcPackage.open(pkg.save()).getPartXml('/word/document.xml');
    expect(text(child(reread?.root, 'w:body'))).toContain('Inserted by the test');
  });
});

describe('OpcPackage', () => {
  it('resolves typed relationships in each format', () => {
    const docx = OpcPackage.open(fixture('docx/features/comments.docx'));
    expect(docx.relatedPart('/word/document.xml', RelType.styles)).toBe('/word/styles.xml');
    expect(docx.relatedPart('/word/document.xml', RelType.comments)).toBe('/word/comments.xml');
    expect(docx.relatedPart('/', RelType.coreProperties)).toBe('/docProps/core.xml');
    expect(docx.getContentType('/word/document.xml')).toBe(ContentType.docxDocument);

    const xlsx = OpcPackage.open(fixture('xlsx/features/multi-sheet.xlsx'));
    const wb = xlsx.mainDocumentPart() as string;
    expect(wb).toBe('/xl/workbook.xml');
    expect(xlsx.relatedParts(wb, RelType.worksheet)).toHaveLength(6);
    expect(xlsx.relatedPart(wb, RelType.theme)).toBe('/xl/theme/theme1.xml');

    const pptx = OpcPackage.open(fixture('pptx/features/pictures.pptx'));
    const pres = pptx.mainDocumentPart() as string;
    const slides = pptx.relatedParts(pres, RelType.slide);
    expect(slides).toHaveLength(1);
    const images = pptx.relatedParts(slides[0] as string, RelType.image);
    expect(images.length).toBeGreaterThanOrEqual(3);
    for (const img of images) expect(pptx.getContentType(img)).toMatch(/^image\//);
    const layout = pptx.relatedPart(slides[0] as string, RelType.slideLayout) as string;
    expect(layout).toMatch(/^\/ppt\/slideLayouts\/slideLayout\d+\.xml$/);
    expect(pptx.relatedPart(layout, RelType.slideMaster)).toBe('/ppt/slideMasters/slideMaster1.xml');
  });

  it('reports external relationship targets without resolving them', () => {
    const pkg = OpcPackage.open(fixture('docx/features/hyperlinks.docx'));
    const rels = pkg.relationships('/word/document.xml');
    const links = rels.byType(RelType.hyperlink);
    expect(links.length).toBe(3);
    expect(links.every((r) => r.targetMode === 'External')).toBe(true);
    expect(rels.resolve(links[0] as (typeof links)[number])).toBeUndefined();
    expect(links.map((l) => l.target)).toContain('https://example.org/');
  });

  it('adds, names and removes parts and relationships', () => {
    const pkg = OpcPackage.open(fixture('docx/features/images-inline.docx'));
    const media = pkg.partNames().filter((n) => n.startsWith('/word/media/'));
    expect(media.length).toBeGreaterThanOrEqual(2);
    const next = pkg.nextPartName('/word/media/image{n}.png');
    expect(pkg.has(next)).toBe(false);
    pkg.addPart(next, 'image/png', new Uint8Array([1, 2, 3]));
    const rel = pkg.addRelationship('/word/document.xml', RelType.image, next);
    expect(rel.target).toBe(`media/${next.split('/').pop()}`);
    expect(pkg.resolveRelationship('/word/document.xml', rel.id)).toBe(next);
    expect(() => pkg.addPart(next, 'image/png', new Uint8Array())).toThrow(OpcError);

    const reopened = OpcPackage.open(pkg.save());
    expect(reopened.getPart(next)).toEqual(new Uint8Array([1, 2, 3]));
    expect(reopened.resolveRelationship('/word/document.xml', rel.id)).toBe(next);

    expect(reopened.removePart(next)).toBe(true);
    expect(reopened.relationships('/word/document.xml').get(rel.id)).toBeUndefined();
    const again = OpcPackage.open(reopened.save());
    expect(again.has(next)).toBe(false);
    expect(again.relationships('/word/document.xml').get(rel.id)).toBeUndefined();
  });

  it('removing a part removes its relationships part and override', () => {
    const pkg = OpcPackage.open(fixture('docx/features/comments.docx'));
    expect(pkg.getContentType('/word/comments.xml')).toBeDefined();
    pkg.removePart('/word/document.xml');
    expect(pkg.has('/word/_rels/document.xml.rels')).toBe(false);
    expect(pkg.mainDocumentPart()).toBeUndefined();
    expect(pkg.contentTypes.overrides.has('/word/document.xml')).toBe(false);
  });

  it('builds a package from scratch with Office ordering and content types', () => {
    const pkg = OpcPackage.create();
    pkg.addPart('/word/document.xml', ContentType.docxDocument, el('w:document', {}, el('w:body')));
    pkg.addRelationship('/', RelType.officeDocument, '/word/document.xml');
    pkg.addPart('/word/media/image1.png', 'image/png', new Uint8Array([137, 80, 78, 71]));
    pkg.addRelationship('/word/document.xml', RelType.image, '/word/media/image1.png');
    const bytes = pkg.save();
    expect(zipOrder(bytes).slice(0, 2)).toEqual(['[Content_Types].xml', '_rels/.rels']);
    const reopened = OpcPackage.open(bytes);
    expect(reopened.mainDocumentPart()).toBe('/word/document.xml');
    expect(reopened.contentTypes.defaults.get('png')).toBe('image/png');
    expect(reopened.contentTypes.overrides.has('/word/media/image1.png')).toBe(false);
    expect(reopened.getContentType('/word/document.xml')).toBe(ContentType.docxDocument);
    expect(reopened.relatedPart('/word/document.xml', RelType.image)).toBe('/word/media/image1.png');
    // Deterministic output.
    expect(pkg.save()).toEqual(bytes);
  });

  it('looks up part names case-insensitively', () => {
    const pkg = OpcPackage.open(fixture('docx/features/styles.docx'));
    expect(pkg.has('/WORD/Document.xml')).toBe(true);
    expect(pkg.getPart('/word/STYLES.xml')).toBeDefined();
  });

  it('refuses zip bombs before inflating them', () => {
    const zeros = new Uint8Array(4 * 1024 * 1024);
    const bomb = zipSync({ '[Content_Types].xml': strToU8('<Types/>'), 'big.bin': [zeros, { level: 9 }] });
    expect(() => OpcPackage.open(bomb, { maxCompressionRatio: 100 })).toThrow(ZipBombError);
    expect(() => OpcPackage.open(bomb, { maxEntrySize: 1024 * 1024 })).toThrow(ZipBombError);
    expect(() => OpcPackage.open(bomb, { maxTotalSize: 1024 })).toThrow(ZipBombError);
    expect(() => OpcPackage.open(bomb, { maxEntries: 1 })).toThrow(ZipBombError);
    // With a permissive ratio the same archive opens: the limits, not the archive, decide.
    expect(OpcPackage.open(bomb, { maxCompressionRatio: 1e9 }).has('/big.bin')).toBe(true);
  });

  it('recognises encrypted and non-zip input', () => {
    const cfb = new Uint8Array([0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]);
    expect(() => OpcPackage.open(cfb)).toThrow(EncryptedOrLegacyFileError);
    expect(() => OpcPackage.open(strToU8('hello'))).toThrow(OpcError);
    expect(() => OpcPackage.open(zipSync({ 'a.xml': strToU8('<a/>') }))).toThrow(/Content_Types/);
    expect(() => OpcPackage.open(new Uint8Array([0x50, 0x4b, 3, 4, 0, 0]))).toThrow(OpcError);
  });
});

describe('part names', () => {
  it('resolves and relativises targets', () => {
    expect(resolveTarget('/word/document.xml', 'media/image1.png')).toBe('/word/media/image1.png');
    expect(resolveTarget('/xl/worksheets/sheet1.xml', '../drawings/drawing1.xml')).toBe(
      '/xl/drawings/drawing1.xml',
    );
    expect(resolveTarget('/', 'word/document.xml')).toBe('/word/document.xml');
    expect(resolveTarget('/ppt/slides/slide1.xml', '/ppt/media/a%20b.png')).toBe('/ppt/media/a b.png');
    expect(relativeTarget('/xl/worksheets/sheet1.xml', '/xl/drawings/drawing1.xml')).toBe(
      '../drawings/drawing1.xml',
    );
    expect(relativeTarget('/word/document.xml', '/word/media/image1.png')).toBe('media/image1.png');
    expect(relativeTarget('/', '/docProps/core.xml')).toBe('docProps/core.xml');
  });

  it('maps relationship parts to sources', () => {
    expect(relsPartName('/word/document.xml')).toBe('/word/_rels/document.xml.rels');
    expect(relsPartName('/')).toBe('/_rels/.rels');
    expect(sourcePartOfRels('/_rels/.rels')).toBe('/');
    expect(sourcePartOfRels('/word/_rels/document.xml.rels')).toBe('/word/document.xml');
    expect(sourcePartOfRels('/word/document.xml')).toBeUndefined();
    expect(extensionOf('/word/media/IMAGE1.PNG')).toBe('png');
  });

  it('validates part names', () => {
    expect(() => assertPartName('word/document.xml')).toThrow();
    expect(() => assertPartName('/word//document.xml')).toThrow();
    expect(() => assertPartName('/word/')).toThrow();
    expect(() => assertPartName('/word/doc.')).toThrow();
    expect(() => assertPartName('/word/document.xml')).not.toThrow();
  });

  it('matches Strict relationship types', () => {
    expect(
      relTypeMatches(
        'http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument',
        RelType.officeDocument,
      ),
    ).toBe(true);
    expect(relTypeMatches(RelType.styles, RelType.theme)).toBe(false);
  });

  it('registers content types the way Office does', () => {
    const ct = new ContentTypes();
    ct.set('/word/media/image1.emf', 'image/x-emf');
    expect(ct.defaults.get('emf')).toBe('image/x-emf');
    ct.set('/word/document.xml', ContentType.docxDocument);
    expect(ct.overrides.get('/word/document.xml')?.[1]).toBe(ContentType.docxDocument);
    ct.pruneDefaults(['/word/document.xml']);
    expect(ct.defaults.has('emf')).toBe(false);
    expect(ContentTypes.parse(ct.toBytes()).get('/word/document.xml')).toBe(ContentType.docxDocument);
  });
});
