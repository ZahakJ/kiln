/**
 * Writes the hand-authored fixtures in `fixtures/<format>/manual/`. Each package below is spelled out
 * part by part, to pin down edge cases that generators never produce: the smallest valid packages,
 * non-canonical namespace prefixes, a default WordprocessingML namespace, custom XML and binary parts.
 *
 *   pnpm --filter @kiln/testing exec tsx scripts/make-manual-fixtures.ts
 */
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { strToU8, type Zippable, zipSync } from 'fflate';
import { FIXTURES_DIR } from '../src/paths.ts';

const MTIME = new Date(1980, 0, 1);
const DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\r\n';
const W = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main';
const R = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships';
const PKG_RELS = 'http://schemas.openxmlformats.org/package/2006/relationships';
const CT = 'http://schemas.openxmlformats.org/package/2006/content-types';

function write(rel: string, parts: [string, string | Uint8Array][]): void {
  const files: Zippable = {};
  for (const [name, body] of parts)
    files[name] = [typeof body === 'string' ? strToU8(body) : body, { level: 6, mtime: MTIME }];
  writeFileSync(join(FIXTURES_DIR, rel), zipSync(files));
  console.log(rel);
}

// The smallest Word document: content types, package relationships, one paragraph.
write('docx/manual/minimal.docx', [
  [
    '[Content_Types].xml',
    `${DECL}<Types xmlns="${CT}"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>`,
  ],
  [
    '_rels/.rels',
    `${DECL}<Relationships xmlns="${PKG_RELS}"><Relationship Id="rId1" Type="${R}/officeDocument" Target="word/document.xml"/></Relationships>`,
  ],
  [
    'word/document.xml',
    `${DECL}<w:document xmlns:w="${W}"><w:body><w:p><w:r><w:t>Minimal document.</w:t></w:r></w:p></w:body></w:document>`,
  ],
]);

// Foreign prefixes: the main part uses a default namespace, the comments part binds WordprocessingML to
// "ns0"; a custom XML data store and an opaque binary part must survive a round trip untouched.
write('docx/manual/foreign-prefixes.docx', [
  [
    '[Content_Types].xml',
    `${DECL}<Types xmlns="${CT}"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="bin" ContentType="application/vnd.kiln.test-binary"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/comments.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/><Override PartName="/customXml/itemProps1.xml" ContentType="application/vnd.openxmlformats-officedocument.customXmlProperties+xml"/></Types>`,
  ],
  [
    '_rels/.rels',
    `${DECL}<Relationships xmlns="${PKG_RELS}"><Relationship Id="rId1" Type="${R}/officeDocument" Target="word/document.xml"/></Relationships>`,
  ],
  [
    'word/_rels/document.xml.rels',
    `${DECL}<Relationships xmlns="${PKG_RELS}"><Relationship Id="rId1" Type="${R}/comments" Target="comments.xml"/><Relationship Id="rId2" Type="${R}/customXml" Target="../customXml/item1.xml"/><Relationship Id="rId3" Type="http://schemas.kiln.example/relationships/blob" Target="blobs/data.bin"/></Relationships>`,
  ],
  [
    'word/document.xml',
    `${DECL}<document xmlns="${W}" xmlns:rel="${R}"><body><p><r><t xml:space="preserve">Default namespace, </t></r><commentRangeStart id="0"/><r><t>commented</t></r><commentRangeEnd id="0"/><r><commentReference id="0"/></r></p><sectPr><pgSz w="11906" h="16838"/><pgMar top="1440" right="1440" bottom="1440" left="1440" header="708" footer="708" gutter="0"/></sectPr></body></document>`,
  ],
  [
    'word/comments.xml',
    `${DECL}<ns0:comments xmlns:ns0="${W}"><ns0:comment ns0:id="0" ns0:author="Hand" ns0:date="2026-01-15T09:30:00Z"><ns0:p><ns0:r><ns0:t>A comment in a foreign prefix.</ns0:t></ns0:r></ns0:p></ns0:comment></ns0:comments>`,
  ],
  [
    'customXml/item1.xml',
    `${DECL}<kiln:data xmlns:kiln="urn:kiln:test"><kiln:value>42</kiln:value></kiln:data>`,
  ],
  [
    'customXml/_rels/item1.xml.rels',
    `${DECL}<Relationships xmlns="${PKG_RELS}"><Relationship Id="rId1" Type="${R}/customXmlProps" Target="itemProps1.xml"/></Relationships>`,
  ],
  [
    'customXml/itemProps1.xml',
    `${DECL}<ds:datastoreItem ds:itemID="{6B0D1A2E-0000-4000-8000-000000000001}" xmlns:ds="http://schemas.openxmlformats.org/officeDocument/2006/customXml"><ds:schemaRefs/></ds:datastoreItem>`,
  ],
  ['word/blobs/data.bin', new Uint8Array([0, 1, 2, 3, 254, 255, 0x50, 0x4b])],
]);

// The smallest workbook: no styles, no shared strings (inline strings), one formula with a cached value.
write('xlsx/manual/minimal.xlsx', [
  [
    '[Content_Types].xml',
    `${DECL}<Types xmlns="${CT}"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>`,
  ],
  [
    '_rels/.rels',
    `${DECL}<Relationships xmlns="${PKG_RELS}"><Relationship Id="rId1" Type="${R}/officeDocument" Target="xl/workbook.xml"/></Relationships>`,
  ],
  [
    'xl/workbook.xml',
    `${DECL}<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="${R}"><sheets><sheet name="Only" sheetId="1" r:id="rId1"/></sheets></workbook>`,
  ],
  [
    'xl/_rels/workbook.xml.rels',
    `${DECL}<Relationships xmlns="${PKG_RELS}"><Relationship Id="rId1" Type="${R}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>`,
  ],
  [
    'xl/worksheets/sheet1.xml',
    `${DECL}<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Total</t></is></c><c r="B1"><v>40</v></c><c r="C1"><v>2</v></c><c r="D1"><f>B1+C1</f><v>42</v></c></row></sheetData></worksheet>`,
  ],
]);
