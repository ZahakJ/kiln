import { unzipSync } from 'fflate';
import { describe, expect, it } from 'vitest';
import { ContentType } from '../src/opc/content-types.ts';
import { OpcPackage } from '../src/opc/package.ts';
import { RelType } from '../src/opc/rel-types.ts';
import { PassthroughSet } from '../src/passthrough.ts';
import { readDocumentProperties, writeDocumentProperties } from '../src/props.ts';
import {
  angleToDegrees,
  degreesToAngle,
  eighthsToPt,
  emuToPt,
  emuToPx,
  halfPointsToPt,
  parseFiftiethsPercent,
  parseLengthPt,
  parsePercentage,
  parseUniversalMeasure,
  ptToEmu,
  ptToTwips,
  pxToEmu,
  twipsToEmu,
  twipsToPt,
} from '../src/units.ts';
import { el } from '../src/xml/node.ts';
import { fixture } from './fixtures.ts';

describe('PassthroughSet', () => {
  /** A "reader" that understands only the main document, styles and core properties. */
  const CONSUMED = ['/word/document.xml', '/word/styles.xml', '/docProps/core.xml'];

  it('records unconsumed parts and the relationships that reach them', () => {
    const src = OpcPackage.open(fixture('docx/features/footnotes.docx'));
    const set = PassthroughSet.collect(src, { consumedParts: CONSUMED });
    const names = set.parts.map((p) => p.name);
    expect(names).toContain('/word/footnotes.xml');
    expect(names).toContain('/word/endnotes.xml');
    expect(names).toContain('/word/theme/theme1.xml');
    expect(names).not.toContain('/word/document.xml');
    expect(set.has('/word/settings.xml')).toBe(true);
    const fromDoc = set.relationships.filter((r) => r.source === '/word/document.xml').map((r) => r.rel.type);
    expect(fromDoc).toContain(RelType.footnotes);
    expect(fromDoc).not.toContain(RelType.styles); // styles were consumed; the writer recreates that link
    expect(set.byteLength).toBeGreaterThan(0);
  });

  it('re-emits parts byte-for-byte into a freshly written package with relationships fixed up', () => {
    const original = fixture('docx/features/footnotes.docx');
    const src = OpcPackage.open(original);
    const set = PassthroughSet.collect(src, { consumedParts: CONSUMED });

    // A "writer" that regenerates only the consumed parts.
    const out = OpcPackage.create();
    out.addPart(
      '/word/document.xml',
      ContentType.docxDocument,
      src.getPart('/word/document.xml') as Uint8Array,
    );
    out.addPart('/word/styles.xml', ContentType.docxStyles, src.getPart('/word/styles.xml') as Uint8Array);
    out.addPart(
      '/docProps/core.xml',
      ContentType.coreProperties,
      src.getPart('/docProps/core.xml') as Uint8Array,
    );
    out.addRelationship('/', RelType.officeDocument, '/word/document.xml');
    out.addRelationship('/', RelType.coreProperties, '/docProps/core.xml');
    out.addRelationship('/word/document.xml', RelType.styles, '/word/styles.xml');

    const result = set.emit(out);
    expect(result.dropped).toEqual([]);
    const reopened = OpcPackage.open(out.save());
    for (const p of set.parts) {
      expect(Buffer.from(reopened.getPart(p.name) as Uint8Array).equals(Buffer.from(p.bytes)), p.name).toBe(
        true,
      );
      expect(reopened.getContentType(p.name)).toBe(src.getContentType(p.name));
    }
    expect(reopened.relatedPart('/word/document.xml', RelType.footnotes)).toBe('/word/footnotes.xml');
    expect(reopened.relatedPart('/word/document.xml', RelType.theme)).toBe('/word/theme/theme1.xml');
    expect(reopened.relatedPart('/', RelType.extendedProperties)).toBe('/docProps/app.xml');
    // Ids that the document body refers to (r:id) survive when they are free.
    const srcIds = src.relationships('/word/document.xml').byType(RelType.footnotes)[0]?.id;
    expect(reopened.relationships('/word/document.xml').byType(RelType.footnotes)[0]?.id).toBe(srcIds);
    // Every original part is present again.
    expect(Object.keys(unzipSync(out.save())).sort()).toEqual(Object.keys(unzipSync(original)).sort());
  });

  it('renames colliding parts and reallocates taken relationship ids', () => {
    const src = OpcPackage.open(fixture('pptx/features/pictures.pptx'));
    const consumed = src.partNames().filter((n) => !n.startsWith('/ppt/media/'));
    const set = PassthroughSet.collect(src, { consumedParts: consumed });
    expect(set.parts.every((p) => p.name.startsWith('/ppt/media/'))).toBe(true);

    const out = OpcPackage.create();
    out.addPart('/ppt/slides/slide1.xml', ContentType.pptxSlide, el('p:sld'));
    // The writer already used the first media name and the first relationship id.
    const firstMedia = set.parts[0]?.name as string;
    out.addPart(firstMedia, 'image/png', new Uint8Array([9]));
    out.addRelationship('/ppt/slides/slide1.xml', RelType.image, firstMedia, { id: 'rId2' });

    const result = set.emit(out);
    expect(result.renamedParts.get(firstMedia)).toMatch(/_1\.\w+$/);
    const ids = result.relationshipIds.get('/ppt/slides/slide1.xml');
    expect(ids).toBeDefined();
    const rels = out.relationships('/ppt/slides/slide1.xml');
    for (const [oldId, newId] of ids ?? []) {
      const target = rels.resolve(rels.get(newId) as NonNullable<ReturnType<typeof rels.get>>) as string;
      const origTarget = src.resolveRelationship('/ppt/slides/slide1.xml', oldId) as string;
      expect(
        Buffer.from(out.getPart(target) as Uint8Array).equals(
          Buffer.from(src.getPart(origTarget) as Uint8Array),
        ),
      ).toBe(true);
    }
  });

  it('maps renamed sources and reports relationships whose source disappeared', () => {
    const src = OpcPackage.open(fixture('xlsx/features/comments.xlsx'));
    const consumed = ['/xl/workbook.xml', '/xl/worksheets/sheet1.xml', '/xl/styles.xml'];
    const set = PassthroughSet.collect(src, { consumedParts: consumed });
    const out = OpcPackage.create();
    out.addPart('/xl/workbook.xml', ContentType.xlsxWorkbook, el('workbook'));
    out.addPart('/xl/worksheets/sheet9.xml', ContentType.xlsxWorksheet, el('worksheet'));
    const r1 = set.emit(out, {
      mapPart: (n) =>
        n === '/xl/worksheets/sheet1.xml'
          ? '/xl/worksheets/sheet9.xml'
          : n === '/xl/styles.xml'
            ? undefined
            : n,
    });
    const commentsRel = out.relationships('/xl/worksheets/sheet9.xml').byType(RelType.comments)[0];
    expect(commentsRel).toBeDefined();
    expect(out.relatedPart('/xl/worksheets/sheet9.xml', RelType.comments)).toMatch(/^\/xl\/comments/);
    expect(r1.dropped).toEqual([]);

    const out2 = OpcPackage.create();
    const r2 = set.emit(out2, { mapPart: () => undefined });
    expect(r2.dropped.length).toBeGreaterThan(0);
  });

  it('honours relationships the reader consumed', () => {
    const src = OpcPackage.open(fixture('docx/features/hyperlinks.docx'));
    const hyperlinkIds = src
      .relationships('/word/document.xml')
      .byType(RelType.hyperlink)
      .map((r) => r.id);
    const all = PassthroughSet.collect(src, { consumedParts: CONSUMED });
    expect(all.relationships.filter((r) => r.rel.type === RelType.hyperlink)).toHaveLength(3);
    const some = PassthroughSet.collect(src, {
      consumedParts: CONSUMED,
      consumedRelationships: new Map([['/word/document.xml', hyperlinkIds]]),
    });
    expect(some.relationships.filter((r) => r.rel.type === RelType.hyperlink)).toHaveLength(0);
  });
});

describe('document properties', () => {
  it('reads core, extended and custom properties', () => {
    const props = readDocumentProperties(OpcPackage.open(fixture('docx/features/properties.docx')));
    expect(props.core).toMatchObject({
      title: 'Document properties',
      subject: 'Fixture subject',
      creator: 'Kiln Fixtures',
      keywords: 'kiln; fixture; properties',
      description: 'Description of the document',
      category: 'Testing',
      contentStatus: 'Draft',
      language: 'en-GB',
      created: '2026-01-15T09:30:00Z',
      modified: '2026-01-15T09:30:00Z',
    });
    expect(props.custom).toEqual([
      { name: 'Project', value: { type: 'lpwstr', value: 'Kiln' } },
      { name: 'Build', value: { type: 'i4', value: 42 } },
      { name: 'Ratio', value: { type: 'r8', value: 0.75 } },
      { name: 'Reviewed', value: { type: 'bool', value: true } },
      { name: 'Due', value: { type: 'filetime', value: '2026-02-01T00:00:00Z' } },
    ]);
    const xlsx = readDocumentProperties(OpcPackage.open(fixture('xlsx/features/styles.xlsx')));
    expect(xlsx.app.application).toBe('Microsoft Excel Compatible / Openpyxl 3.1.5');
  });

  it('writes properties into a new package and reads them back', () => {
    const pkg = OpcPackage.create();
    writeDocumentProperties(pkg, {
      core: {
        title: 'Quarterly <report> & "notes"',
        creator: 'Ada',
        created: '2026-10-15T08:00:00Z',
        modified: '2026-10-15T09:00:00Z',
      },
      app: {
        application: 'Kiln',
        appVersion: '0.1',
        pages: 3,
        words: 120,
        scaleCrop: false,
        company: 'Workshop',
      },
      custom: [
        { name: 'Client', value: { type: 'lpwstr', value: 'ACME' } },
        { name: 'Approved', value: { type: 'bool', value: false } },
      ],
    });
    const reread = readDocumentProperties(OpcPackage.open(pkg.save()));
    expect(reread.core).toEqual({
      title: 'Quarterly <report> & "notes"',
      creator: 'Ada',
      created: '2026-10-15T08:00:00Z',
      modified: '2026-10-15T09:00:00Z',
    });
    expect(reread.app).toEqual({
      application: 'Kiln',
      appVersion: '0.1',
      pages: 3,
      words: 120,
      scaleCrop: false,
      company: 'Workshop',
    });
    expect(reread.custom.map((c) => c.name)).toEqual(['Client', 'Approved']);
    const xml = new TextDecoder().decode(pkg.getPart('/docProps/core.xml'));
    expect(xml).toContain(
      '<dcterms:created xsi:type="dcterms:W3CDTF">2026-10-15T08:00:00Z</dcterms:created>',
    );
  });

  it('updates existing properties in place and preserves unknown app.xml elements', () => {
    const pkg = OpcPackage.open(fixture('docx/features/properties.docx'));
    const before = readDocumentProperties(pkg);
    writeDocumentProperties(pkg, { core: { ...before.core, title: 'Renamed' }, app: before.app, custom: [] });
    const after = readDocumentProperties(OpcPackage.open(pkg.save()));
    expect(after.core.title).toBe('Renamed');
    expect(after.core.subject).toBe('Fixture subject');
    expect(after.app).toEqual(before.app);
    expect(after.custom).toEqual([]);
    expect(pkg.has('/docProps/custom.xml')).toBe(false);
    expect(pkg.relatedPart('/', RelType.customProperties)).toBeUndefined();
  });
});

describe('units', () => {
  it('converts between EMU, points, pixels and twips', () => {
    expect(emuToPt(12700)).toBe(1);
    expect(ptToEmu(72)).toBe(914400);
    expect(emuToPx(9525)).toBe(1);
    expect(pxToEmu(96)).toBe(914400);
    expect(twipsToPt(240)).toBe(12);
    expect(ptToTwips(11)).toBe(220);
    expect(twipsToEmu(1440)).toBe(914400);
    expect(halfPointsToPt(22)).toBe(11);
    expect(eighthsToPt(4)).toBe(0.5);
    expect(angleToDegrees(5400000)).toBe(90);
    expect(degreesToAngle(45)).toBe(2700000);
  });

  it('parses percentages and universal measures', () => {
    expect(parsePercentage('50000')).toBe(0.5);
    expect(parsePercentage('50%')).toBe(0.5);
    expect(parsePercentage('x')).toBeUndefined();
    expect(parseFiftiethsPercent('5000')).toBe(1);
    expect(parseFiftiethsPercent('25%')).toBe(0.25);
    expect(parseUniversalMeasure('1in')).toBe(72);
    expect(parseUniversalMeasure('2.54cm')).toBeCloseTo(72, 9);
    expect(parseUniversalMeasure('12pt')).toBe(12);
    expect(parseUniversalMeasure('1pc')).toBe(12);
    expect(parseUniversalMeasure('100')).toBeUndefined();
    expect(parseLengthPt('1440', 20)).toBe(72);
    expect(parseLengthPt('0.5in', 20)).toBe(36);
    expect(parseLengthPt(undefined, 20)).toBeUndefined();
  });
});

describe('hand-authored edge cases', () => {
  it('normalises a default namespace and foreign prefixes in a real package', () => {
    const pkg = OpcPackage.open(fixture('docx/manual/foreign-prefixes.docx'));
    const doc = pkg.getPartXml('/word/document.xml')?.root;
    expect(doc?.name).toBe('w:document');
    const comments = pkg.getPartXml(pkg.relatedPart('/word/document.xml', RelType.comments) as string)?.root;
    expect(comments?.name).toBe('w:comments');
    expect(comments?.children[0]).toMatchObject({ name: 'w:comment', attrs: { 'w:author': 'Hand' } });
  });

  it('passes custom XML, unknown relationship types and binary parts through', () => {
    const src = OpcPackage.open(fixture('docx/manual/foreign-prefixes.docx'));
    const set = PassthroughSet.collect(src, { consumedParts: ['/word/document.xml', '/word/comments.xml'] });
    expect(set.parts.map((p) => p.name).sort()).toEqual([
      '/customXml/item1.xml',
      '/customXml/itemProps1.xml',
      '/word/blobs/data.bin',
    ]);
    const out = OpcPackage.create();
    out.addPart(
      '/word/document.xml',
      ContentType.docxDocument,
      src.getPart('/word/document.xml') as Uint8Array,
    );
    out.addRelationship('/', RelType.officeDocument, '/word/document.xml');
    set.emit(out);
    const re = OpcPackage.open(out.save());
    expect(re.relatedPart('/word/document.xml', RelType.customXml)).toBe('/customXml/item1.xml');
    expect(re.relatedPart('/customXml/item1.xml', RelType.customXmlProps)).toBe('/customXml/itemProps1.xml');
    expect(re.relatedPart('/word/document.xml', 'http://schemas.kiln.example/relationships/blob')).toBe(
      '/word/blobs/data.bin',
    );
    expect(re.getPart('/word/blobs/data.bin')).toEqual(new Uint8Array([0, 1, 2, 3, 254, 255, 0x50, 0x4b]));
    expect(re.getContentType('/word/blobs/data.bin')).toBe('application/vnd.kiln.test-binary');
  });

  it('opens the smallest valid packages', () => {
    for (const name of ['docx/manual/minimal.docx', 'xlsx/manual/minimal.xlsx']) {
      const pkg = OpcPackage.open(fixture(name));
      expect(pkg.getPartXml(pkg.mainDocumentPart() as string)).toBeDefined();
      expect(readDocumentProperties(pkg)).toEqual({ core: {}, app: {}, custom: [] });
    }
  });
});
