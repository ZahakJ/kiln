/**
 * Package-level writer tests: parts, content types and relationships, document properties,
 * determinism, passthrough of unmodelled parts (custom XML, macros, glossary, unknown parts), formats,
 * and documents created from the default template.
 */
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DefaultTemplate, defaultStyles } from '@kiln-ref/doc-model';
import { ContentType, OpcPackage, RelType } from '@kiln-ref/ooxml';
import { loadFixture, pythonAvailable, readDocxWithMammoth, readWithPython } from '@kiln/testing';
import { describe, expect, it } from 'vitest';
import { readDocx, writeDocx, writeDocxWithReport } from '../../src/index.ts';
import { SAVE_TIME } from '../roundtrip/model.ts';
import { buildDocx, P, PNG, part, R, roundTripBytes, write } from './helpers.ts';

const MAIN = '/word/document.xml';

function parts(bytes: Uint8Array): Map<string, Uint8Array> {
  const pkg = OpcPackage.open(bytes);
  return new Map(pkg.allPartNames().map((n) => [n, pkg.getPart(n) as Uint8Array]));
}

describe('package structure', () => {
  it('registers every written part with its content type and relationship', () => {
    const bytes = write(readDocx(loadFixture('docx/features/footnotes.docx')));
    const pkg = OpcPackage.open(bytes);
    expect(pkg.mainDocumentPart()).toBe(MAIN);
    expect(pkg.getContentType(MAIN)).toBe(ContentType.docxDocument);
    for (const [type, name, ct] of [
      [RelType.styles, '/word/styles.xml', ContentType.docxStyles],
      [RelType.settings, '/word/settings.xml', ContentType.docxSettings],
      [RelType.numbering, '/word/numbering.xml', ContentType.docxNumbering],
      [RelType.theme, '/word/theme/theme1.xml', ContentType.theme],
      [
        RelType.footnotes,
        '/word/footnotes.xml',
        'application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml',
      ],
      [
        RelType.endnotes,
        '/word/endnotes.xml',
        'application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml',
      ],
      [
        RelType.fontTable,
        '/word/fontTable.xml',
        'application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml',
      ],
    ] as const) {
      expect(pkg.relatedPart(MAIN, type), type).toBe(name);
      expect(pkg.getContentType(name), name).toBe(ct);
    }
    expect(pkg.relatedPart('/', RelType.coreProperties)).toBe('/docProps/core.xml');
    expect(pkg.relatedPart('/', RelType.extendedProperties)).toBe('/docProps/app.xml');
    const names = pkg.allPartNames();
    expect(names[0]).toBe('/_rels/.rels');
  });

  it('writes Office-style XML: declaration, no indentation, only the namespaces in use', () => {
    const bytes = write(readDocx(buildDocx({ body: P(R('plain')) })));
    const xml = OpcPackage.open(bytes).getPartText(MAIN) ?? '';
    const decl = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\r\n';
    expect(xml.startsWith(`${decl}<w:document `)).toBe(true);
    expect(xml.slice(decl.length)).not.toMatch(/>\s+</);
    expect(xml).not.toContain('xmlns:wps');
    expect(xml).not.toContain('mc:Ignorable');
  });

  it('updates document properties: modified date, revision, application and statistics', () => {
    const a = readDocx(loadFixture('docx/features/properties.docx'));
    const b = readDocx(write(a, { author: 'Kiln Tester', pages: 1 }));
    expect(b.properties.core).toMatchObject({
      ...a.properties.core,
      modified: '2026-10-15T12:00:00Z',
      lastModifiedBy: 'Kiln Tester',
      revision: String(Number(a.properties.core.revision ?? '1') + 1),
    });
    expect(b.properties.core.created).toBe(a.properties.core.created);
    expect(b.properties.app.application).toBe('Kiln');
    expect(b.properties.app.appVersion).toBeUndefined();
    expect(b.properties.app.pages).toBe(1);
    expect(b.properties.app.words).toBeGreaterThan(0);
    expect(b.properties.custom).toEqual(a.properties.custom);
  });

  it('keeps the original part names of headers, footers and media', () => {
    const rt = roundTripBytes(loadFixture('docx/features/headers-footers.docx'));
    const before = OpcPackage.open(rt.original)
      .partNames()
      .filter((n) => /header|footer|media/.test(n))
      .sort();
    const after = OpcPackage.open(rt.bytes)
      .partNames()
      .filter((n) => /header|footer|media/.test(n))
      .sort();
    expect(after).toEqual(before);
  });
});

describe('determinism', () => {
  it('writes identical bytes for two consecutive writes of the same model', () => {
    for (const name of [
      'docx/features/tables.docx',
      'docx/features/images-anchored.docx',
      'docx/features/comments.docx',
    ]) {
      const model = readDocx(loadFixture(name));
      const first = writeDocx(model, { now: SAVE_TIME });
      const second = writeDocx(model, { now: SAVE_TIME });
      expect(Buffer.from(second).equals(Buffer.from(first)), name).toBe(true);
    }
  });

  it('is deterministic for new documents too, and only the save time differs between saves', () => {
    const doc = DefaultTemplate.create({ locale: 'en-GB' });
    expect(
      Buffer.from(writeDocx(doc, { now: SAVE_TIME })).equals(Buffer.from(writeDocx(doc, { now: SAVE_TIME }))),
    ).toBe(true);
    const a = parts(writeDocx(doc, { now: new Date('2026-01-01T00:00:00Z') }));
    const b = parts(writeDocx(doc, { now: new Date('2026-06-01T00:00:00Z') }));
    const differing = [...a.keys()].filter(
      (k) => !Buffer.from(a.get(k) as Uint8Array).equals(Buffer.from(b.get(k) as Uint8Array)),
    );
    expect(differing).toEqual(['/docProps/core.xml']);
  });
});

describe('passthrough', () => {
  const withExtras = () => {
    const pkg = OpcPackage.open(buildDocx({ body: P(R('x')) }));
    pkg.addPart('/customXml/item1.xml', 'application/xml', '<root xmlns="urn:x"><v>1</v></root>');
    pkg.addPart(
      '/customXml/itemProps1.xml',
      'application/vnd.openxmlformats-officedocument.customXmlProperties+xml',
      '<ds:datastoreItem ds:itemID="{1}" xmlns:ds="http://schemas.openxmlformats.org/officeDocument/2006/customXml"/>',
    );
    pkg.addRelationship('/customXml/item1.xml', RelType.customXmlProps, '/customXml/itemProps1.xml');
    pkg.addRelationship(MAIN, RelType.customXml, '/customXml/item1.xml', { id: 'rIdCx' });
    pkg.addPart(
      '/word/glossary/document.xml',
      'application/vnd.openxmlformats-officedocument.wordprocessingml.document.glossary+xml',
      part('w:glossaryDocument', '<w:docParts/>'),
    );
    pkg.addRelationship(MAIN, RelType.glossaryDocument, '/word/glossary/document.xml');
    pkg.addPart('/word/unknown/thing.bin', 'application/x-thing', new Uint8Array([9, 8, 7]));
    pkg.addRelationship(MAIN, 'urn:example:thing', '/word/unknown/thing.bin', { id: 'rIdThing' });
    return pkg;
  };

  it('copies custom XML, glossary and unknown parts with their relationships', () => {
    const rt = roundTripBytes(withExtras().save());
    const pkg = OpcPackage.open(rt.bytes);
    expect(pkg.getPartText('/customXml/item1.xml')).toBe('<root xmlns="urn:x"><v>1</v></root>');
    expect(pkg.relatedPart('/customXml/item1.xml', RelType.customXmlProps)).toBe('/customXml/itemProps1.xml');
    expect(pkg.relationships(MAIN).get('rIdCx')?.type).toBe(RelType.customXml);
    expect(pkg.relatedPart(MAIN, RelType.glossaryDocument)).toBe('/word/glossary/document.xml');
    expect(pkg.getPart('/word/unknown/thing.bin')).toEqual(new Uint8Array([9, 8, 7]));
    expect(pkg.getContentType('/word/unknown/thing.bin')).toBe('application/x-thing');
    expect(pkg.relationships(MAIN).get('rIdThing')?.target).toBe('unknown/thing.bin');
    expect(rt.b.passthrough.parts.map((p) => p.name).sort()).toEqual(
      rt.a.passthrough.parts.map((p) => p.name).sort(),
    );
  });

  it('writes no passthrough parts when asked not to', () => {
    const bytes = write(readDocx(withExtras().save()), { passthrough: false });
    const pkg = OpcPackage.open(bytes);
    expect(pkg.has('/customXml/item1.xml')).toBe(false);
    expect(pkg.relatedPart(MAIN, RelType.glossaryDocument)).toBeUndefined();
  });

  const docm = () => {
    const pkg = OpcPackage.open(buildDocx({ body: P(R('macro doc')) }));
    pkg.setContentType(MAIN, 'application/vnd.ms-word.document.macroEnabled.main+xml');
    pkg.addPart(
      '/word/vbaProject.bin',
      'application/vnd.ms-office.vbaProject',
      new Uint8Array([0xd0, 0xcf, 0x11, 0xe0]),
    );
    pkg.addRelationship(MAIN, RelType.vbaProject, '/word/vbaProject.bin');
    pkg.addPart(
      '/word/vbaData.xml',
      'application/vnd.ms-word.vbaData+xml',
      '<wne:vbaSuppData xmlns:wne="http://schemas.microsoft.com/office/word/2006/wordml"/>',
    );
    pkg.addRelationship(
      '/word/vbaProject.bin',
      'http://schemas.microsoft.com/office/2006/relationships/wordVbaData',
      '/word/vbaData.xml',
    );
    return pkg.save();
  };

  it('keeps macros in .docm files', () => {
    const a = readDocx(docm());
    const pkg = OpcPackage.open(write(a));
    expect(pkg.getContentType(MAIN)).toBe('application/vnd.ms-word.document.macroEnabled.main+xml');
    expect(pkg.relatedPart(MAIN, RelType.vbaProject)).toBe('/word/vbaProject.bin');
    expect(pkg.getPart('/word/vbaProject.bin')).toEqual(new Uint8Array([0xd0, 0xcf, 0x11, 0xe0]));
    expect(pkg.has('/word/vbaData.xml')).toBe(true);
  });

  it('removes macros when a .docm is saved as .docx', () => {
    const pkg = OpcPackage.open(write(readDocx(docm()), { format: 'docx' }));
    expect(pkg.getContentType(MAIN)).toBe(ContentType.docxDocument);
    expect(pkg.has('/word/vbaProject.bin')).toBe(false);
    expect(pkg.has('/word/vbaData.xml')).toBe(false);
    expect(pkg.relatedPart(MAIN, RelType.vbaProject)).toBeUndefined();
  });

  it('keeps templates as templates', () => {
    const pkg = OpcPackage.open(buildDocx({ body: P(R('t')) }));
    pkg.setContentType(
      MAIN,
      'application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml',
    );
    const out = OpcPackage.open(write(readDocx(pkg.save())));
    expect(out.getContentType(MAIN)).toBe(
      'application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml',
    );
  });

  it('writes Strict documents back as Transitional', () => {
    const strict = 'http://purl.oclc.org/ooxml/wordprocessingml/main';
    const pkg = OpcPackage.create();
    pkg.addPart(
      MAIN,
      ContentType.docxDocument,
      `<w:document xmlns:w="${strict}"><w:body><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Strict</w:t></w:r></w:p></w:body></w:document>`,
    );
    pkg.addRelationship('/', RelType.officeDocument, MAIN);
    const bytes = write(readDocx(pkg.save()));
    const xml = OpcPackage.open(bytes).getPartText(MAIN) ?? '';
    expect(xml).toContain('xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"');
    expect(readDocx(bytes).doc.textContent).toBe('Strict');
  });

  it('keeps foreign namespace prefixes declared on the fragments that use them', () => {
    const rt = roundTripBytes(loadFixture('docx/manual/foreign-prefixes.docx'));
    expect(rt.b.doc.textContent).toBe(rt.a.doc.textContent);
    expect(rt.b.warnings.map((w) => w.code)).toEqual(rt.a.warnings.map((w) => w.code));
  });
});

describe('new documents', () => {
  it('writes the default template with Word’s built-in style ids', () => {
    const doc = DefaultTemplate.create();
    const bytes = writeDocx(doc, { now: SAVE_TIME });
    const styles = OpcPackage.open(bytes).getPartText('/word/styles.xml') ?? '';
    for (const id of [
      'Normal',
      'Heading1',
      'Heading2',
      'Heading3',
      'Title',
      'Subtitle',
      'Quote',
      'ListParagraph',
      'Caption',
      'TOC1',
      'Header',
      'Footer',
      'Hyperlink',
      'Strong',
      'Emphasis',
      'TableGrid',
      'DefaultParagraphFont',
      'TableNormal',
      'NoList',
    ]) {
      if (!defaultStyles().some((s) => s.id === id)) continue;
      expect(styles, id).toContain(`w:styleId="${id}"`);
    }
    expect(styles).toContain('<w:style w:type="paragraph" w:default="1" w:styleId="Normal">');
    const b = readDocx(bytes);
    expect(b.styles.toJSON()).toEqual(doc.styles.toJSON());
    expect(b.sections[0]?.pageSize).toEqual(expect.objectContaining({ width: 12240, height: 15840 }));
    expect(b.warnings).toEqual([]);
  });

  it('writes content built in the editor: headings, lists, tables, links and pictures', async () => {
    const doc = DefaultTemplate.create({ locale: 'en-GB' });
    const schema = doc.doc.type.schema;
    const list = doc.numbering.addInstance(
      doc.numbering.addAbstract({
        multiLevelType: 'hybridMultilevel',
        levels: [
          {
            level: 0,
            start: 1,
            format: 'decimal',
            text: '%1.',
            pPr: { indent: { left: 720, hanging: 360 } },
          },
        ],
      }).id,
    );
    const link = schema.marks.link?.create({ href: 'https://kiln.example/' });
    const bold = schema.marks.bold?.create();
    const cell = (t: string) =>
      schema.node('tableCell', null, [schema.node('paragraph', null, [schema.text(t)])]);
    const url = `data:image/png;base64,${Buffer.from(PNG).toString('base64')}`;
    const content = [
      schema.node('paragraph', { styleId: 'Title' }, [schema.text('Report')]),
      schema.node('paragraph', { styleId: 'Heading1' }, [schema.text('Introduction')]),
      schema.node('paragraph', null, [
        schema.text('Visit '),
        schema.text('our site', link ? [link] : []),
        schema.text(' now', bold ? [bold] : []),
      ]),
      schema.node('paragraph', { styleId: 'ListParagraph', numbering: { numId: list.id, level: 0 } }, [
        schema.text('First'),
      ]),
      schema.node('paragraph', { styleId: 'ListParagraph', numbering: { numId: list.id, level: 0 } }, [
        schema.text('Second'),
      ]),
      schema.node('table', { styleId: 'TableGrid' }, [
        schema.node('tableRow', null, [cell('A'), cell('B')]),
        schema.node('tableRow', null, [cell('C'), cell('D')]),
      ]),
      schema.node('paragraph', null, [
        schema.nodes.image?.create({ src: url, width: 914400, height: 914400, alt: 'Dot' }) as never,
      ]),
    ];
    const model = {
      ...doc,
      doc: schema.node('doc', null, [
        schema.node('section', { props: doc.doc.firstChild?.attrs.props }, content),
      ]),
    };
    const bytes = writeDocx(model, { now: SAVE_TIME });
    const b = readDocx(bytes);
    expect(b.doc.textContent).toBe(model.doc.textContent);
    expect(b.numbering.instances.get(list.id)?.abstractId).toBe(list.abstractId);
    const mammoth = await readDocxWithMammoth(bytes);
    expect(mammoth.html).toContain('<h1>Introduction</h1>');
    expect(mammoth.html).toContain('<a href="https://kiln.example/">our site</a>');
    expect(mammoth.html).toContain('<table>');
    expect(mammoth.html).toContain('<img');
    expect(b.media.size).toBe(1);
  });

  it.runIf(pythonAvailable())('opens new documents with python-docx', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'kiln-docx-new-'));
    const path = join(dir, 'blank.docx');
    writeFileSync(path, writeDocx(DefaultTemplate.create(), { now: SAVE_TIME }));
    const dump = await readWithPython(path);
    expect(dump.format).toBe('docx');
  });

  it('reports what could not be written', () => {
    const doc = DefaultTemplate.create();
    const schema = doc.doc.type.schema;
    const content = [
      schema.node('paragraph', null, [
        schema.nodes.image?.create({ mediaId: '/word/media/missing.png' }) as never,
        schema.nodes.equation?.create({ omml: '', source: 'x^2' }) as never,
      ]),
    ];
    const { issues } = writeDocxWithReport({
      ...doc,
      doc: schema.node('doc', null, [
        schema.node('section', { props: doc.doc.firstChild?.attrs.props }, content),
      ]),
    });
    expect(issues.map((i) => i.code).sort()).toEqual(['equation.unsaved', 'image.missing']);
  });
});
