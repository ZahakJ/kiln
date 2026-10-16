/**
 * Element-level reader tests on small hand-written packages: every WordprocessingML construct of
 * PLAN §3.5 that the generated corpus does not contain (content controls, equations, moves, nested
 * fields, VML, charts, threads, …) and the edge cases of the ones it does.
 */
import { ContentType, OpcPackage, RelType } from '@kiln-ref/ooxml';
import { loadFixture } from '@kiln/testing';
import type { Node as PMNode } from '@tiptap/pm/model';
import { TableMap } from '@tiptap/pm/tables';
import { describe, expect, it } from 'vitest';
import {
  DocxFormatError,
  GRID_PLACEHOLDER,
  parseFragment,
  readDocx,
  readDocxStream,
} from '../../src/index.ts';
import { buildDocx, part } from './helpers.ts';

const P = (inner: string, pPr = '') => `<w:p>${pPr ? `<w:pPr>${pPr}</w:pPr>` : ''}${inner}</w:p>`;
const R = (text: string, rPr = '') =>
  `<w:r>${rPr ? `<w:rPr>${rPr}</w:rPr>` : ''}<w:t xml:space="preserve">${text}</w:t></w:r>`;
const read = (
  body: string,
  extra: Parameters<typeof buildDocx>[0] extends infer O ? Partial<O> : never = {},
) => readDocx(buildDocx({ body, ...extra }));

function all(doc: PMNode, type: string): PMNode[] {
  const out: PMNode[] = [];
  doc.descendants((n) => {
    if (n.type.name === type) out.push(n);
  });
  return out;
}
const paragraphs = (doc: PMNode) => all(doc, 'paragraph');
function marksOn(doc: PMNode, text: string) {
  let found: PMNode | undefined;
  doc.descendants((n) => {
    if (!found && n.isText && n.text?.includes(text)) found = n;
  });
  if (!found) throw new Error(`no text ${text}`);
  return Object.fromEntries(found.marks.map((m) => [m.type.name, m.attrs]));
}
const codes = (r: { warnings: { code: string }[] }) => r.warnings.map((w) => w.code);

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0]);
const imagePart = (name = '/word/media/image1.png') => ({
  name,
  contentType: 'image/png',
  relType: RelType.image,
  relId: 'rIdImg',
  content: PNG,
});

describe('package and errors', () => {
  it('rejects packages that are not Word documents', () => {
    expect(() => readDocx(loadFixture('xlsx/features/formulas.xlsx'))).toThrow(DocxFormatError);
  });

  it('rejects a main part that is not well-formed XML with a typed error', () => {
    const pkg = OpcPackage.create();
    pkg.addPart('/word/document.xml', ContentType.docxDocument, 'this is not XML');
    pkg.addRelationship('/', RelType.officeDocument, '/word/document.xml');
    expect(() => readDocx(pkg.save())).toThrow(DocxFormatError);
  });

  it('accepts an ArrayBuffer and a stream', async () => {
    const bytes = buildDocx({ body: P(R('Hello')) });
    expect(readDocx(bytes.buffer.slice(0) as ArrayBuffer).doc.textContent).toBe('Hello');
    const stream = new ReadableStream<Uint8Array>({
      start(c) {
        c.enqueue(bytes.subarray(0, 100));
        c.enqueue(bytes.subarray(100));
        c.close();
      },
    });
    expect((await readDocxStream(stream)).doc.textContent).toBe('Hello');
  });

  it('uses defaults when styles, numbering, settings and theme are absent', () => {
    const r = read(P(R('Bare')));
    expect(r.styles.size).toBe(0);
    expect(r.settings.defaultTabStop).toBe(720);
    expect(r.theme.name).toBeTruthy();
    expect(r.sections[0]?.pageSize).toMatchObject({ width: 12240, height: 15840 });
    expect(r.mainContentType).toBe(ContentType.docxDocument);
  });

  it('reads ISO 29500 Strict documents', () => {
    const strict = 'http://purl.oclc.org/ooxml/wordprocessingml/main';
    const pkg = OpcPackage.create();
    pkg.addPart(
      '/word/document.xml',
      ContentType.docxDocument,
      `<w:document xmlns:w="${strict}"><w:body><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Strict</w:t></w:r></w:p></w:body></w:document>`,
    );
    pkg.addRelationship('/', RelType.officeDocument, '/word/document.xml');
    const r = readDocx(pkg.save());
    expect(r.doc.textContent).toBe('Strict');
    expect(paragraphs(r.doc)[0]?.attrs.alignment).toBe('center');
  });
});

describe('runs', () => {
  it('reads toggles including explicit off values', () => {
    const r = read(
      P(R('on', '<w:b/><w:i w:val="1"/>') + R('off', '<w:b w:val="0"/><w:caps w:val="false"/>')),
    );
    expect(marksOn(r.doc, 'on')).toMatchObject({ bold: { value: true }, italic: { value: true } });
    expect(marksOn(r.doc, 'off')).toMatchObject({ bold: { value: false }, caps: { value: false } });
  });

  it('reads sizes and spacing written as universal measures', () => {
    const r = read(P(R('x', '<w:sz w:val="12pt"/><w:spacing w:val="1pt"/><w:position w:val="2pt"/>')));
    const m = marksOn(r.doc, 'x');
    expect(m.fontSize.size).toBe(24);
    expect(m.charSpacing.value).toBe(20);
    expect(m.runProps.props.position).toBe(4);
  });

  it('reads theme colours with tint/shade, underline colour and theme fonts', () => {
    const r = read(
      P(
        R(
          'c',
          '<w:rFonts w:asciiTheme="majorHAnsi" w:hAnsiTheme="majorHAnsi" w:hint="eastAsia"/><w:color w:val="1F3864" w:themeColor="accent1" w:themeShade="80"/><w:u w:val="dash" w:color="FF0000"/>',
        ),
      ),
    );
    const m = marksOn(r.doc, 'c');
    expect(m.color.color).toEqual({ val: '1F3864', themeColor: 'accent1', themeShade: 128 });
    expect(m.underline).toEqual({ style: 'dash', color: { val: 'FF0000' } });
    expect(m.fontFamily.fonts).toEqual({ asciiTheme: 'majorHAnsi', hAnsiTheme: 'majorHAnsi' });
    expect(m.runProps.props.fontHint).toBe('eastAsia');
  });

  it('keeps unmodelled run properties as raw XML', () => {
    const r = read(P(R('np', '<w:noProof/><w:em w:val="dot"/>')));
    expect(marksOn(r.doc, 'np').runProps.props.xml).toEqual(['<w:noProof/>', '<w:em w:val="dot"/>']);
  });

  it('reads breaks, carriage returns, hyphens and clear attributes', () => {
    const r = read(
      P(
        '<w:r><w:t>a</w:t><w:br w:type="textWrapping" w:clear="all"/><w:t>b</w:t><w:cr/><w:t>c</w:t><w:br w:type="column"/><w:noBreakHyphen/><w:softHyphen/></w:r>',
      ),
    );
    const p = paragraphs(r.doc)[0] as PMNode;
    expect(p.content.content.map((n) => n.type.name)).toEqual([
      'text',
      'softBreak',
      'text',
      'softBreak',
      'text',
      'columnBreak',
      'text',
    ]);
    expect(p.child(1).attrs.clear).toBe('all');
    expect(p.child(6).text).toBe('‑­');
  });

  it('turns positional tabs into tabs with a warning', () => {
    const r = read(P('<w:r><w:ptab w:relativeTo="margin" w:alignment="right" w:leader="none"/></w:r>'));
    expect(all(r.doc, 'tab')).toHaveLength(1);
    expect(codes(r)).toContain('element.unknown');
  });

  it('keeps unmapped symbol characters in their font', () => {
    const r = read(P('<w:r><w:sym w:font="Marlett" w:char="F031"/></w:r>'));
    const m = marksOn(r.doc, '');
    expect(m.fontFamily.fonts.ascii).toBe('Marlett');
    expect(m.runProps.props.sym).toEqual({ font: 'Marlett', char: 'F031' });
  });

  it('reads ruby base text and warns that the guide was dropped', () => {
    const r = read(
      P(
        '<w:r><w:ruby><w:rubyPr/><w:rt><w:r><w:t>かん</w:t></w:r></w:rt><w:rubyBase><w:r><w:t>漢</w:t></w:r></w:rubyBase></w:ruby></w:r>',
      ),
    );
    expect(r.doc.textContent).toBe('漢');
    expect(codes(r)).toContain('element.unknown');
  });
});

describe('paragraph properties', () => {
  it('normalises start/end indents and prefers hanging over firstLine', () => {
    const r = read(P(R('i'), '<w:ind w:start="720" w:end="360" w:firstLine="200" w:hanging="400"/>'));
    expect(paragraphs(r.doc)[0]?.attrs.indent).toEqual({ left: 720, right: 360, hanging: 400 });
  });

  it('reads numbering, outline level, paragraph mark props and paraId', () => {
    const r = read(
      '<w:p w14:paraId="1A2B3C4D"><w:pPr><w:numPr><w:ilvl w:val="2"/><w:numId w:val="5"/></w:numPr><w:outlineLvl w:val="1"/><w:rPr><w:b/></w:rPr></w:pPr><w:r><w:t>n</w:t></w:r></w:p>',
    );
    const a = paragraphs(r.doc)[0]?.attrs;
    expect(a?.numbering).toEqual({ numId: 5, level: 2 });
    expect(a?.outlineLevel).toBe(1);
    expect(a?.markProps).toEqual({ bold: true });
    expect(a?.paraId).toBe('1A2B3C4D');
  });

  it('keeps unmodelled paragraph properties and frames as raw XML', () => {
    const r = read(
      P(
        R('f'),
        '<w:framePr w:w="2000" w:h="400" w:hSpace="180" w:wrap="around" w:vAnchor="text" w:hAnchor="margin" w:x="200"/><w:snapToGrid w:val="0"/>',
      ),
    );
    const xml = paragraphs(r.doc)[0]?.attrs.xml as string[];
    expect(xml[0]).toMatch(/^<w:framePr /);
    expect(xml[1]).toBe('<w:snapToGrid w:val="0"/>');
    expect(paragraphs(r.doc)[0]?.attrs.dropCap).toBeNull();
  });

  it('reads drop caps', () => {
    const r = read(
      P(
        R('D'),
        '<w:framePr w:dropCap="drop" w:lines="3" w:wrap="around" w:vAnchor="text" w:hAnchor="text"/>',
      ),
    );
    expect(paragraphs(r.doc)[0]?.attrs.dropCap).toEqual({ type: 'drop', lines: 3 });
  });

  it('reads paragraph-mark revisions and formatting changes', () => {
    const r = read(
      P(
        R('x'),
        '<w:rPr><w:ins w:id="9" w:author="Ann" w:date="2026-02-01T00:00:00Z"/><w:i/><w:rPrChange w:id="10" w:author="Ann"><w:rPr><w:b/></w:rPr></w:rPrChange></w:rPr>',
      ),
    );
    const rev = paragraphs(r.doc)[0]?.attrs.revision;
    expect(rev.mark).toEqual({ id: '9', author: 'Ann', date: '2026-02-01T00:00:00Z', kind: 'insertion' });
    expect(rev.markPrChange).toMatchObject({ id: '10', before: { bold: true } });
    expect(paragraphs(r.doc)[0]?.attrs.markProps).toEqual({ italic: true });
  });
});

describe('revisions', () => {
  it('reads moves as deletion and insertion with an info warning', () => {
    const r = read(
      P(
        '<w:moveFromRangeStart w:id="1" w:name="move1"/><w:moveFrom w:id="2" w:author="Bo" w:date="2026-03-01T00:00:00Z"><w:r><w:t>moved</w:t></w:r></w:moveFrom><w:moveFromRangeEnd w:id="1"/>',
      ) + P('<w:moveTo w:id="3" w:author="Bo"><w:r><w:t>arrived</w:t></w:r></w:moveTo>'),
    );
    expect(marksOn(r.doc, 'moved').deletion).toEqual({ id: '2', author: 'Bo', date: '2026-03-01T00:00:00Z' });
    expect(marksOn(r.doc, 'arrived').insertion).toEqual({ id: '3', author: 'Bo', date: '' });
    expect(r.warnings.find((w) => w.code === 'revision.move')?.severity).toBe('info');
  });

  it('reads deleted field codes and deleted text inside w:del', () => {
    const r = read(
      P(
        '<w:del w:id="5" w:author="Cy"><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:delInstrText> PAGE </w:delInstrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:delText>7</w:delText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:del>',
      ),
    );
    const f = all(r.doc, 'field')[0] as PMNode;
    expect(f.attrs).toMatchObject({ type: 'PAGE', result: '7' });
    expect(f.marks.map((m) => m.type.name)).toContain('deletion');
  });

  it('reads tracked row insertions and keeps cell revisions with a warning', () => {
    const r = read(
      '<w:tbl><w:tblGrid><w:gridCol w:w="1000"/></w:tblGrid><w:tr><w:trPr><w:ins w:id="1" w:author="Di"/></w:trPr><w:tc><w:tcPr><w:cellIns w:id="2" w:author="Di"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl>',
    );
    const row = all(r.doc, 'tableRow')[0] as PMNode;
    expect(row.attrs.revision).toMatchObject({ kind: 'insertion', author: 'Di' });
    expect(codes(r)).toContain('revision.cell');
  });
});

describe('fields', () => {
  const complex = (instr: string, result: string) =>
    `<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> ${instr} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>${result}<w:r><w:fldChar w:fldCharType="end"/></w:r>`;

  it('writes nested fields inside an instruction as field codes', () => {
    const r = read(
      P(
        '<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> IF </w:instrText></w:r>' +
          complex('MERGEFIELD Gender', R('F')) +
          '<w:r><w:instrText xml:space="preserve"> = "F" "Ms" "Mr" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>' +
          R('Ms') +
          '<w:r><w:fldChar w:fldCharType="end"/></w:r>',
      ),
    );
    const fields = all(r.doc, 'field');
    expect(fields).toHaveLength(1);
    expect(fields[0]?.attrs).toMatchObject({
      type: 'IF',
      instr: 'IF { MERGEFIELD Gender } = "F" "Ms" "Mr"',
      result: 'Ms',
    });
  });

  it('turns HYPERLINK fields into links and keeps nested PAGEREF fields in a TOC', () => {
    const entry = (n: number) =>
      P(
        complex(
          `HYPERLINK \\l "_Toc${n}"`,
          `${R(`Entry ${n}`)}<w:r><w:tab/></w:r>${complex(`PAGEREF _Toc${n} \\h`, R(String(n + 1)))}`,
        ),
      );
    const r = read(
      `<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> TOC \\o "1-3" \\h </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r></w:p>${entry(1)}${entry(2)}${P('<w:r><w:fldChar w:fldCharType="end"/></w:r>')}${P(R('After'))}`,
    );
    const toc = all(r.doc, 'toc')[0] as PMNode;
    expect(toc.childCount).toBe(4);
    expect(toc.attrs.instr).toBe('TOC \\o "1-3" \\h');
    expect(marksOn(toc, 'Entry 2').link).toMatchObject({ anchor: '_Toc2', href: null });
    expect(all(toc, 'field').map((f) => [f.attrs.instr, f.attrs.result])).toEqual([
      ['PAGEREF _Toc1 \\h', '2'],
      ['PAGEREF _Toc2 \\h', '3'],
    ]);
    expect(paragraphs(r.doc).at(-1)?.textContent).toBe('After');
    expect(r.warnings).toEqual([]);
  });

  it('reads an external HYPERLINK field with a tooltip', () => {
    const r = read(P(complex('HYPERLINK "https://example.com/a" \\o "Tip"', R('site'))));
    expect(marksOn(r.doc, 'site').link).toMatchObject({ href: 'https://example.com/a', tooltip: 'Tip' });
    expect(all(r.doc, 'field')).toHaveLength(0);
  });

  it('reads fields without a separator, dirty and locked flags', () => {
    const r = read(
      P(
        '<w:r><w:fldChar w:fldCharType="begin" w:dirty="true" w:fldLock="1"/></w:r><w:r><w:instrText> XE "Index entry" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>',
      ),
    );
    expect(all(r.doc, 'field')[0]?.attrs).toMatchObject({
      type: 'XE',
      result: '',
      dirty: true,
      locked: true,
    });
  });

  it('warns about unterminated fields and fields spanning paragraphs', () => {
    const open = read(
      P('<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r>'),
    );
    expect(codes(open)).toContain('field.unterminated');
    const multi = read(
      `<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> QUOTE "x" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>${R('one')}</w:p>${P(`${R('two')}<w:r><w:fldChar w:fldCharType="end"/></w:r>`)}`,
    );
    expect(multi.doc.textContent).toBe('onetwo');
    expect(multi.warnings.find((w) => w.code === 'field.multiParagraph')?.path).toBe(
      '/w:document/w:body[1]/w:p[1]/w:r[1]/w:fldChar[1]',
    );
  });

  it('keeps pictures produced by a field and drops only the field code', () => {
    const drawing = `<w:r><w:drawing><wp:inline><wp:extent cx="100" cy="100"/><wp:docPr id="1" name="p"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill><a:blip r:embed="rIdImg"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>`;
    const r = read(P(complex('INCLUDEPICTURE "x.png" \\* MERGEFORMATINET', drawing)), {
      parts: [imagePart()],
    });
    expect(all(r.doc, 'image')).toHaveLength(1);
    expect(codes(r)).toContain('field.unwrapped');
  });

  it('converts legacy form fields with a warning', () => {
    const r = read(
      P(
        '<w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:name w:val="Text1"/><w:enabled/></w:ffData></w:fldChar></w:r><w:r><w:instrText> FORMTEXT </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>' +
          R('value') +
          '<w:r><w:fldChar w:fldCharType="end"/></w:r>',
      ),
    );
    expect(all(r.doc, 'field')[0]?.attrs.result).toBe('value');
    expect(codes(r)).toContain('field.formField');
  });

  it('reads w:pgNum as a PAGE field', () => {
    const r = read(P('<w:r><w:pgNum/></w:r>'));
    expect(all(r.doc, 'field')[0]?.attrs.type).toBe('PAGE');
  });
});

describe('content controls', () => {
  it('unwraps block content controls and keeps their properties on every block', () => {
    const r = read(
      `<w:sdt><w:sdtPr><w:alias w:val="Intro"/><w:tag w:val="intro"/><w:id w:val="42"/><w:docPartObj><w:docPartGallery w:val="Table of Contents"/><w:docPartUnique/></w:docPartObj></w:sdtPr><w:sdtEndPr/><w:sdtContent>${P(R('A'))}${P(R('B'))}</w:sdtContent></w:sdt>${P(R('C'))}`,
    );
    const [a, b, c] = paragraphs(r.doc);
    expect(a?.attrs.sdt).toHaveLength(1);
    expect(a?.attrs.sdt[0]).toMatchObject({
      level: 'block',
      kind: 'docPartObj',
      id: '42',
      tag: 'intro',
      alias: 'Intro',
      docPartGallery: 'Table of Contents',
      endPr: '<w:sdtEndPr/>',
    });
    expect(a?.attrs.sdt[0].pr).toMatch(/^<w:sdtPr>/);
    expect(b?.attrs.sdt[0].key).toBe(a?.attrs.sdt[0].key);
    expect(c?.attrs.sdt).toBeNull();
  });

  it('reads inline content controls as sdt marks (checkbox, drop-down, date)', () => {
    const r = read(
      P(
        `<w:sdt><w:sdtPr><w14:checkbox><w14:checked w14:val="1"/></w14:checkbox></w:sdtPr><w:sdtContent>${R('☒')}</w:sdtContent></w:sdt>` +
          `<w:sdt><w:sdtPr><w:dropDownList><w:listItem w:displayText="Yes" w:value="Y"/><w:listItem w:value="N"/></w:dropDownList></w:sdtPr><w:sdtContent>${R('Yes')}</w:sdtContent></w:sdt>` +
          `<w:sdt><w:sdtPr><w:showingPlcHdr/><w:date><w:dateFormat w:val="d/M/yyyy"/></w:date></w:sdtPr><w:sdtContent>${R('Pick')}</w:sdtContent></w:sdt>`,
      ),
    );
    expect(marksOn(r.doc, '☒').sdt.info).toMatchObject({ level: 'inline', kind: 'checkbox', checked: true });
    expect(marksOn(r.doc, 'Yes').sdt.info.listItems).toEqual([
      { displayText: 'Yes', value: 'Y' },
      { displayText: 'N', value: 'N' },
    ]);
    expect(marksOn(r.doc, 'Pick').sdt.info).toMatchObject({
      kind: 'date',
      dateFormat: 'd/M/yyyy',
      showingPlaceholder: true,
    });
  });

  it('unwraps row-level content controls onto the paragraphs of their rows', () => {
    const r = read(
      `<w:tbl><w:tblGrid><w:gridCol w:w="1000"/></w:tblGrid><w:sdt><w:sdtPr><w15:repeatingSectionItem/></w:sdtPr><w:sdtContent><w:tr><w:tc><w:p>${R('row')}</w:p></w:tc></w:tr></w:sdtContent></w:sdt></w:tbl>`,
    );
    const p = paragraphs(r.doc)[0] as PMNode;
    expect(p.attrs.sdt[0]).toMatchObject({ level: 'row', kind: 'repeatingSectionItem' });
  });
});

describe('tables', () => {
  const tbl = (grid: number[], rows: string, tblPr = '') =>
    `<w:tbl><w:tblPr>${tblPr}</w:tblPr><w:tblGrid>${grid.map((g) => `<w:gridCol w:w="${g}"/>`).join('')}</w:tblGrid>${rows}</w:tbl>`;
  const tc = (text: string, tcPr = '') =>
    `<w:tc>${tcPr ? `<w:tcPr>${tcPr}</w:tcPr>` : ''}<w:p>${R(text)}</w:p></w:tc>`;

  it('reads widths in every unit notation', () => {
    const r = read(
      tbl(
        [1440, 1440, 1440, 1440],
        `<w:tr>${tc('a', '<w:tcW w:w="1440" w:type="dxa"/>')}${tc('b', '<w:tcW w:w="2500" w:type="pct"/>')}${tc('c', '<w:tcW w:w="50%" w:type="pct"/>')}${tc('d', '<w:tcW w:w="1in" w:type="dxa"/>')}</w:tr>`,
        '<w:tblW w:w="0" w:type="auto"/><w:tblInd w:w="0.5in" w:type="dxa"/><w:tblCellMar><w:start w:w="108" w:type="dxa"/><w:end w:w="108" w:type="dxa"/></w:tblCellMar>',
      ),
    );
    const cells = all(r.doc, 'tableCell').map((c) => c.attrs.width);
    expect(cells).toEqual([
      { type: 'dxa', value: 1440 },
      { type: 'pct', value: 2500 },
      { type: 'pct', value: 2500 },
      { type: 'dxa', value: 1440 },
    ]);
    const t = all(r.doc, 'table')[0]?.attrs;
    expect(t?.width).toEqual({ type: 'auto', value: 0 });
    expect(t?.indent).toBe(720);
    expect(t?.cellMargins).toEqual({ left: 108, right: 108 });
  });

  it('reads 2007-style hex tblLook and nil widths', () => {
    const r = read(
      tbl([100], `<w:tr>${tc('x', '<w:tcW w:w="0" w:type="nil"/>')}</w:tr>`, '<w:tblLook w:val="04A0"/>'),
    );
    expect(all(r.doc, 'table')[0]?.attrs.look).toEqual({
      firstRow: true,
      lastRow: false,
      firstColumn: true,
      lastColumn: false,
      noHBand: false,
      noVBand: true,
    });
    expect(all(r.doc, 'tableCell')[0]?.attrs.width).toEqual({ type: 'nil', value: 0 });
  });

  it('builds a 3x3 table with a horizontally and vertically merged block', () => {
    const r = read(
      tbl(
        [1000, 1000, 1000],
        `<w:tr>${tc('A', '<w:gridSpan w:val="2"/><w:vMerge w:val="restart"/>')}${tc('B')}</w:tr>` +
          `<w:tr>${tc('', '<w:gridSpan w:val="2"/><w:vMerge/>')}${tc('C')}</w:tr>` +
          `<w:tr>${tc('D')}${tc('E')}${tc('F')}</w:tr>`,
      ),
    );
    const t = all(r.doc, 'table')[0] as PMNode;
    const map = TableMap.get(t);
    expect([map.width, map.height]).toEqual([3, 3]);
    expect(map.problems).toBeNull();
    expect(t.child(0).child(0).attrs).toMatchObject({ colspan: 2, rowspan: 2 });
    expect(t.child(1).childCount).toBe(1);
  });

  it('treats a vMerge continue without an origin as a normal cell', () => {
    const r = read(tbl([1000], `<w:tr>${tc('orphan', '<w:vMerge/>')}</w:tr>`));
    expect(all(r.doc, 'tableCell')[0]?.attrs.rowspan).toBe(1);
    expect(r.doc.textContent).toBe('orphan');
  });

  it('merges legacy hMerge cells', () => {
    const r = read(
      tbl([1000, 1000], `<w:tr>${tc('wide', '<w:hMerge w:val="restart"/>')}${tc('', '<w:hMerge/>')}</w:tr>`),
    );
    const cells = all(r.doc, 'tableCell');
    expect(cells).toHaveLength(1);
    expect(cells[0]?.attrs.colspan).toBe(2);
  });

  it('pads gridBefore/gridAfter and ragged rows with placeholder cells', () => {
    const r = read(
      tbl(
        [1000, 1000, 1000],
        `<w:tr><w:trPr><w:gridBefore w:val="1"/></w:trPr>${tc('b')}${tc('c')}</w:tr><w:tr>${tc('only')}</w:tr>`,
      ),
    );
    const t = all(r.doc, 'table')[0] as PMNode;
    expect(TableMap.get(t).problems).toBeNull();
    const first = t.child(0).child(0);
    expect(first.attrs.xml).toEqual([GRID_PLACEHOLDER]);
    expect(t.child(1).child(1).attrs).toMatchObject({ colspan: 2, xml: [GRID_PLACEHOLDER] });
    expect(t.child(1).attrs.xml).toContain('<w:gridAfter w:val="2"/>');
    expect(r.warnings.find((w) => w.code === 'table.irregular')?.severity).toBe('info');
  });

  it('reads row and cell properties', () => {
    const r = read(
      tbl(
        [1000],
        `<w:tr><w:trPr><w:trHeight w:val="400" w:hRule="exact"/><w:tblHeader/><w:cantSplit/></w:trPr>${tc('h', '<w:vAlign w:val="center"/><w:textDirection w:val="btLr"/><w:noWrap/><w:tcBorders><w:top w:val="single" w:sz="4" w:color="auto"/></w:tcBorders>')}</w:tr>`,
      ),
    );
    expect(all(r.doc, 'tableRow')[0]?.attrs).toMatchObject({
      height: { value: 400, rule: 'exact' },
      header: true,
      cantSplit: true,
    });
    expect(all(r.doc, 'tableCell')[0]?.attrs).toMatchObject({
      vAlign: 'center',
      textDirection: 'btLr',
      noWrap: true,
      borders: { top: { style: 'single', size: 4, color: { val: 'auto' } } },
    });
  });

  it('gives empty cells an empty paragraph', () => {
    const r = read(tbl([1000], '<w:tr><w:tc><w:tcPr/></w:tc></w:tr>'));
    expect(all(r.doc, 'tableCell')[0]?.childCount).toBe(1);
    expect(() => r.doc.check()).not.toThrow();
  });
});

describe('drawings', () => {
  const inline = (graphic: string, docPr = '<wp:docPr id="1" name="Picture 1" descr="A chart" title="T"/>') =>
    `<w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="457200"/>${docPr}<a:graphic>${graphic}</a:graphic></wp:inline></w:drawing></w:r>`;
  const picGraphic = (blip = '<a:blip r:embed="rIdImg"/>', extra = '') =>
    `<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill>${blip}<a:srcRect l="10000" r="5000"/></pic:blipFill><pic:spPr><a:xfrm rot="5400000" flipH="1"/>${extra}</pic:spPr></pic:pic></a:graphicData>`;

  it('reads pictures with alt text, crop and rotation', () => {
    const r = read(P(inline(picGraphic())), { parts: [imagePart()] });
    const img = all(r.doc, 'image')[0]?.attrs;
    expect(img).toMatchObject({
      mediaId: '/word/media/image1.png',
      alt: 'A chart',
      title: 'T',
      width: 914400,
      height: 457200,
      rotation: 90,
      flipH: true,
      crop: { l: 0.1, t: 0, r: 0.05, b: 0 },
    });
    expect(r.media.get('/word/media/image1.png')?.contentType).toBe('image/png');
  });

  it('reads decorative pictures', () => {
    const docPr =
      '<wp:docPr id="2" name="d"><a:extLst><a:ext uri="{C183D7F6-B498-43B3-948B-1728B52AA6E4}"><adec:decorative xmlns:adec="http://schemas.microsoft.com/office/drawing/2017/decorative" val="1"/></a:ext></a:extLst></wp:docPr>';
    const r = read(P(inline(picGraphic(), docPr)), { parts: [imagePart()] });
    expect(all(r.doc, 'image')[0]?.attrs.decorative).toBe(true);
  });

  it('warns about linked and missing pictures', () => {
    const linked = read(P(inline(picGraphic('<a:blip r:link="rIdLink"/>'))), {
      rels: [['rIdLink', RelType.image, 'https://example.com/x.png', true]],
    });
    expect(codes(linked)).toContain('image.linked');
    const missing = read(P(inline(picGraphic('<a:blip r:embed="rIdNope"/>'))));
    expect(codes(missing)).toContain('image.missing');
    expect(all(missing.doc, 'image')[0]?.attrs.mediaId).toBeNull();
  });

  it('reads charts as references for the charts package and keeps the chart part', () => {
    const chart = {
      name: '/word/charts/chart1.xml',
      contentType: 'application/vnd.openxmlformats-officedocument.drawingml.chart+xml',
      relType: RelType.chart,
      relId: 'rIdChart',
      content: '<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"/>',
    };
    const r = read(
      P(
        inline(
          '<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart r:id="rIdChart"/></a:graphicData>',
        ),
      ),
      { parts: [chart] },
    );
    expect(r.charts).toEqual([
      {
        sourcePart: '/word/document.xml',
        relId: 'rIdChart',
        chartPart: '/word/charts/chart1.xml',
        width: 914400,
        height: 457200,
        placement: 'inline',
        name: 'Picture 1',
        description: 'A chart',
      },
    ]);
    expect(all(r.doc, 'opaqueInline')[0]?.attrs.label).toBe('Chart');
    expect(r.passthrough.has('/word/charts/chart1.xml')).toBe(true);
    expect(r.relationships.get('/word/document.xml')?.get('rIdChart')?.target).toBe(
      '/word/charts/chart1.xml',
    );
    expect(r.warnings).toEqual([]);
  });

  it('keeps SmartArt and groups as placeholders with info warnings', () => {
    const r = read(
      P(
        inline('<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"/>') +
          inline('<a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"/>'),
      ),
    );
    expect(all(r.doc, 'opaqueInline').map((n) => n.attrs.label)).toEqual(['SmartArt', 'Group']);
    expect(r.warnings.map((w) => [w.code, w.severity])).toEqual([
      ['drawing.smartArt', 'info'],
      ['drawing.group', 'info'],
    ]);
  });

  it('prefers the wps choice of mc:AlternateContent and keeps the whole block', () => {
    const wsp =
      '<a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:spPr><a:prstGeom prst="ellipse"/><a:noFill/></wps:spPr><wps:bodyPr/></wps:wsp></a:graphicData>';
    const r = read(
      P(
        `<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing><wp:inline><wp:extent cx="10" cy="20"/><wp:docPr id="3" name="s"/><a:graphic>${wsp}</a:graphic></wp:inline></w:drawing></mc:Choice><mc:Fallback><w:pict><v:oval/></w:pict></mc:Fallback></mc:AlternateContent></w:r>`,
      ),
    );
    const shape = all(r.doc, 'shape')[0]?.attrs;
    expect(shape?.preset).toBe('ellipse');
    expect(shape?.fill).toEqual({ type: 'none' });
    expect(shape?.xml).toMatch(/^<mc:AlternateContent/);
    expect(shape?.xml).toContain('<mc:Fallback>');
    expect(r.warnings).toEqual([]);
  });

  it('reads anchors with alignment, simplePos and one-sided wrapping', () => {
    const anchor = (attrs: string, inner: string) =>
      `<w:r><w:drawing><wp:anchor ${attrs}>${inner}<wp:extent cx="1" cy="1"/><wp:docPr id="9" name="a"/><a:graphic>${picGraphic()}</a:graphic></wp:anchor></w:drawing></w:r>`;
    const r = read(
      P(
        anchor(
          'behindDoc="0" relativeHeight="5" simplePos="0"',
          '<wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="margin"><wp:align>center</wp:align></wp:positionH><wp:positionV relativeFrom="page"><wp:align>top</wp:align></wp:positionV><wp:wrapSquare wrapText="left"/>',
        ) +
          anchor(
            'behindDoc="1" simplePos="1"',
            '<wp:simplePos x="12700" y="25400"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:wrapNone/>',
          ),
      ),
      { parts: [imagePart()] },
    );
    const [a, b] = all(r.doc, 'image');
    expect(a?.attrs.position).toMatchObject({
      relativeFromH: 'margin',
      h: { align: 'center' },
      relativeFromV: 'page',
      v: { align: 'top' },
      zIndex: 5,
    });
    expect(a?.attrs.wrap).toBe('square');
    expect(b?.attrs.wrap).toBe('behind');
    expect(b?.attrs.position).toMatchObject({
      relativeFromH: 'page',
      h: { offset: 12700 },
      v: { offset: 25400 },
    });
    expect(codes(r)).toContain('drawing.unsupported');
  });

  it('reads VML pictures as images and keeps other VML as placeholders', () => {
    const r = read(
      P(
        '<w:r><w:pict><v:shape style="width:72pt;height:0.5in"><v:imagedata r:id="rIdImg" o:title="Logo"/></v:shape></w:pict></w:r>' +
          '<w:r><w:pict><v:rect><v:textbox><w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent></v:textbox></v:rect></w:pict></w:r>',
      ),
      { parts: [imagePart()] },
    );
    expect(all(r.doc, 'image')[0]?.attrs).toMatchObject({
      width: 914400,
      height: 457200,
      title: 'Logo',
      mediaId: '/word/media/image1.png',
    });
    expect(all(r.doc, 'opaqueInline')[0]?.attrs.label).toBe('Boxed');
    expect(r.warnings.map((w) => w.code)).toEqual(['drawing.vml']);
  });

  it('keeps embedded OLE objects as placeholders', () => {
    const r = read(
      P(
        '<w:r><w:object><v:shape style="width:10pt;height:10pt"><v:imagedata r:id="rIdImg"/></v:shape><o:OLEObject Type="Embed" ProgID="Excel.Sheet.12" ShapeID="_x0000_i1025" DrawAspect="Content" ObjectID="_1" r:id="rIdOle"/></w:object></w:r>',
      ),
      {
        parts: [
          imagePart(),
          {
            name: '/word/embeddings/Microsoft_Excel_Worksheet.xlsx',
            contentType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
            relType: RelType.package,
            relId: 'rIdOle',
            content: new Uint8Array([1, 2, 3]),
          },
        ],
      },
    );
    expect(all(r.doc, 'opaqueInline')[0]?.attrs.label).toBe('Embedded Excel.Sheet.12');
    expect(r.warnings[0]).toMatchObject({ code: 'object.embedded', severity: 'info' });
    expect(r.passthrough.has('/word/embeddings/Microsoft_Excel_Worksheet.xlsx')).toBe(true);
    expect(r.relationships.get('/word/document.xml')?.get('rIdOle')?.type).toBe(RelType.package);
  });
});

describe('equations', () => {
  it('reads inline m:oMath and display m:oMathPara as equation nodes holding OMML', () => {
    const r = read(
      P(
        `${R('E = ')}<m:oMath><m:r><m:t>mc</m:t></m:r><m:sSup><m:e><m:r><m:t>2</m:t></m:r></m:e></m:sSup></m:oMath>`,
      ) + P('<m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara>'),
    );
    const [inlineEq, display] = all(r.doc, 'equation');
    expect(inlineEq?.attrs.display).toBe(false);
    expect(inlineEq?.attrs.omml).toBe(
      '<m:oMath><m:r><m:t>mc</m:t></m:r><m:sSup><m:e><m:r><m:t>2</m:t></m:r></m:e></m:sSup></m:oMath>',
    );
    expect(parseFragment(inlineEq?.attrs.omml).name).toBe('m:oMath');
    expect(display?.attrs.display).toBe(true);
    expect(display?.attrs.omml).toMatch(/^<m:oMathPara/);
  });
});

describe('comments', () => {
  const commentsPart = (inner: string) => ({
    name: '/word/comments.xml',
    contentType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml',
    relType: RelType.comments,
    content: part('w:comments', inner),
  });

  it('reads threads and resolved state from commentsExtended, and people', () => {
    const r = read(
      P(
        `<w:commentRangeStart w:id="1"/>${R('discussed')}<w:commentRangeEnd w:id="1"/><w:r><w:commentReference w:id="1"/></w:r><w:r><w:commentReference w:id="2"/></w:r>`,
      ),
      {
        parts: [
          commentsPart(
            '<w:comment w:id="1" w:author="Ann" w:date="2026-01-01T00:00:00Z"><w:p w14:paraId="00000001"><w:r><w:t>Why?</w:t></w:r></w:p></w:comment><w:comment w:id="2" w:author="Bo"><w:p w14:paraId="00000002"><w:r><w:t>Because.</w:t></w:r></w:p></w:comment>',
          ),
          {
            name: '/word/commentsExtended.xml',
            contentType:
              'application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml',
            relType: RelType.commentsExtended,
            content: part(
              'w15:commentsEx',
              '<w15:commentEx w15:paraId="00000001" w15:done="1"/><w15:commentEx w15:paraId="00000002" w15:paraIdParent="00000001" w15:done="0"/>',
            ),
          },
          {
            name: '/word/people.xml',
            contentType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.people+xml',
            relType: RelType.people,
            content: part(
              'w15:people',
              '<w15:person w15:author="Ann"><w15:presenceInfo w15:providerId="AD" w15:userId="S-1"/></w15:person>',
            ),
          },
        ],
      },
    );
    expect(r.comments.map((c) => [c.id, c.author, c.text, c.parentId, c.resolved])).toEqual([
      ['1', 'Ann', 'Why?', undefined, true],
      ['2', 'Bo', 'Because.', '1', undefined],
    ]);
    expect(r.people).toEqual([{ author: 'Ann', providerId: 'AD', userId: 'S-1' }]);
    // The reply covers no text of its own (Word anchors replies to the parent's range).
    expect(r.warnings.map((w) => [w.code, w.message])).toEqual([
      ['comment.unanchored', expect.stringContaining('Comment 2')],
    ]);
  });

  it('marks comment ranges that span paragraphs and block-level markers', () => {
    const r = read(
      `<w:commentRangeStart w:id="7"/>${P(R('first'))}${P(`${R('second')}<w:commentRangeEnd w:id="7"/>`)}${P(R('third'))}`,
      { parts: [commentsPart('<w:comment w:id="7" w:author="A"><w:p/></w:comment>')] },
    );
    expect(marksOn(r.doc, 'first').comment).toEqual({ id: '7' });
    expect(marksOn(r.doc, 'second').comment).toEqual({ id: '7' });
    expect(marksOn(r.doc, 'third').comment).toBeUndefined();
  });

  it('warns about references to comments that do not exist', () => {
    const r = read(P('<w:r><w:commentReference w:id="99"/></w:r>'));
    expect(codes(r)).toContain('comment.missing');
  });
});

describe('notes, bookmarks and hyperlinks', () => {
  it('reads custom note marks and warns about missing notes', () => {
    const r = read(P('<w:r><w:footnoteReference w:customMarkFollows="1" w:id="4"/><w:t>*</w:t></w:r>'));
    expect(all(r.doc, 'footnoteRef')[0]?.attrs).toEqual({ id: '4', kind: 'footnote', customMark: '*' });
    expect(codes(r)).toContain('note.missing');
  });

  it('attaches block-level bookmarks to the next paragraph, trailing ones to the last', () => {
    const r = read(`<w:bookmarkStart w:id="1" w:name="b"/>${P(R('x'))}<w:bookmarkEnd w:id="1"/>`);
    const p = paragraphs(r.doc)[0] as PMNode;
    expect(p.content.content.map((n) => n.type.name)).toEqual(['bookmarkStart', 'text', 'bookmarkEnd']);
  });

  it('resolves hyperlink relationships, tooltips and target frames', () => {
    const r = read(
      P(
        `<w:hyperlink r:id="rIdL" w:tooltip="Go" w:tgtFrame="_blank" w:history="1">${R('ext')}</w:hyperlink>`,
      ),
      { rels: [['rIdL', RelType.hyperlink, 'https://example.org/p?q=1', true]] },
    );
    expect(marksOn(r.doc, 'ext').link).toEqual({
      href: 'https://example.org/p?q=1',
      anchor: null,
      tooltip: 'Go',
      target: '_blank',
    });
    expect(r.passthrough.relationships).toEqual([]);
  });

  it('unwraps smart tags and custom XML, and reports permissions and subdocuments', () => {
    const r = read(
      `${P(
        `<w:smartTag w:uri="urn:x" w:element="place"><w:smartTagPr/>${R('Paris')}</w:smartTag><w:customXml w:element="e">${R(' Rome')}</w:customXml><w:permStart w:id="1" w:edGrp="everyone"/>${R('!')}<w:permEnd w:id="1"/>`,
      )}<w:customXml w:element="blk"><w:p><w:r><w:t>Block</w:t></w:r></w:p></w:customXml>`,
    );
    expect(paragraphs(r.doc).map((p) => p.textContent)).toEqual(['Paris Rome!', 'Block']);
    expect(codes(r)).toContain('permission');
  });

  it('keeps altChunk imports as opaque blocks', () => {
    const r = read(`${P(R('a'))}<w:altChunk r:id="rIdAlt"/>`);
    expect(all(r.doc, 'opaqueBlock')[0]?.attrs.label).toBe('Imported content');
    expect(codes(r)).toContain('altChunk');
  });

  it('reports unknown elements with their location', () => {
    const r = read(P('<w:r><w:dayLong/></w:r>'));
    expect(r.warnings[0]).toMatchObject({
      code: 'element.unknown',
      part: '/word/document.xml',
      path: '/w:document/w:body[1]/w:p[1]/w:r[1]/w:dayLong[1]',
    });
  });
});

describe('sections, headers and footers', () => {
  const hdr = (name: string, id: string, text: string, root = 'w:hdr') => ({
    name,
    contentType: `application/vnd.openxmlformats-officedocument.wordprocessingml.${root === 'w:hdr' ? 'header' : 'footer'}+xml`,
    relType: root === 'w:hdr' ? RelType.header : RelType.footer,
    relId: id,
    content: part(root, `<w:p><w:r><w:t>${text}</w:t></w:r></w:p>`),
  });

  it('splits the body at paragraph section breaks and reuses shared header parts', () => {
    const r = read(
      `${P(R('one'), '<w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:type w:val="continuous"/><w:pgSz w:w="8.5in" w:h="11in"/><w:cols w:num="3" w:space="0.5in"/></w:sectPr>')}${P(R('two'))}<w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:footerReference w:type="first" r:id="rIdF"/><w:titlePg/><w:pgSz w:w="15840" w:h="12240"/><w:vAlign w:val="center"/><w:docGrid w:linePitch="360"/></w:sectPr>`,
      {
        parts: [hdr('/word/header1.xml', 'rIdH', 'Head'), hdr('/word/footer1.xml', 'rIdF', 'Foot', 'w:ftr')],
      },
    );
    expect(r.sections).toHaveLength(2);
    expect(r.sections[0]).toMatchObject({
      type: 'continuous',
      pageSize: { width: 12240, height: 15840, orientation: 'portrait' },
      columns: { count: 3, space: 720, equalWidth: true },
      headerRefs: { default: 'rIdH' },
    });
    expect(r.sections[1]).toMatchObject({
      titlePage: true,
      vAlign: 'center',
      pageSize: { orientation: 'landscape' },
      footerRefs: { first: 'rIdF' },
      docGrid: { linePitch: 360 },
    });
    const s1 = r.doc.child(0);
    const s2 = r.doc.child(1);
    expect(s1.child(0).type.name).toBe('header');
    expect(s1.child(0).attrs).toEqual({ type: 'default', partId: '/word/header1.xml' });
    expect(s2.child(1).type.name).toBe('footer');
    expect(s2.child(1).textContent).toBe('Foot');
    expect(s1.child(0).content.eq(s2.child(0).content)).toBe(true);
  });

  it('keeps unmodelled section properties and warns about missing header parts', () => {
    const r = read(
      `${P(R('x'))}<w:sectPr><w:headerReference w:type="even" r:id="rIdGone"/><w:paperSrc w:first="7"/><w:footnotePr><w:numFmt w:val="lowerRoman"/><w:numRestart w:val="eachPage"/></w:footnotePr><w:rtlGutter/></w:sectPr>`,
    );
    const s = r.sections[0];
    expect(s?.xml).toEqual(['<w:paperSrc w:first="7"/>']);
    expect(s?.footnotePr).toEqual({ numFmt: 'lowerRoman', numRestart: 'eachPage' });
    expect(s?.rtlGutter).toBe(true);
    expect(codes(r)).toContain('part.missing');
  });

  it('reads custom column widths', () => {
    const r = read(
      `${P(R('x'))}<w:sectPr><w:cols w:num="2" w:equalWidth="0"><w:col w:w="3000" w:space="400"/><w:col w:w="5000"/></w:cols></w:sectPr>`,
    );
    expect(r.sections[0]?.columns).toEqual({
      count: 2,
      space: 720,
      equalWidth: false,
      separator: false,
      cols: [
        { width: 3000, space: 400 },
        { width: 5000, space: 0 },
      ],
    });
  });
});

describe('styles, numbering and settings parts', () => {
  it('reads table styles with conditional formatting and latent styles', () => {
    const r = read(P(R('x')), {
      styles:
        '<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>' +
        '<w:latentStyles w:defLockedState="0" w:count="376"><w:lsdException w:name="Normal" w:qFormat="1"/></w:latentStyles>' +
        '<w:style w:type="table" w:styleId="Grid"><w:name w:val="Grid"/><w:tblPr><w:tblBorders><w:insideH w:val="single" w:sz="4"/></w:tblBorders><w:tblStyleRowBandSize w:val="1"/></w:tblPr><w:tblStylePr w:type="firstRow"><w:rPr><w:b/></w:rPr><w:tcPr><w:shd w:val="clear" w:fill="4472C4"/></w:tcPr></w:tblStylePr><w:tblStylePr w:type="bogus"/></w:style>' +
        '<w:style w:type="character" w:customStyle="1" w:styleId="Em"><w:name w:val="Em"/><w:aliases w:val="E1, E2"/><w:rPr><w:i/></w:rPr></w:style>',
    });
    const grid = r.styles.get('Grid');
    expect(grid?.tblPr?.borders?.insideH).toMatchObject({ style: 'single', size: 4 });
    expect(grid?.conditions?.firstRow?.rPr).toEqual({ bold: true });
    expect(grid?.conditions?.firstRow?.tcPr?.shading?.fill?.val).toBe('4472C4');
    expect(Object.keys(grid?.conditions ?? {})).toEqual(['firstRow']);
    expect(r.styles.get('Em')).toMatchObject({ custom: true, aliases: ['E1', 'E2'] });
    expect(r.styles.docDefaults).toEqual({
      rPr: { size: 22 },
      pPr: { spacing: { after: 160, line: 259, lineRule: 'auto' } },
    });
    expect(r.extras.latentStyles).toMatch(/^<w:latentStyles /);
  });

  it('reads level overrides with full definitions, legal numbering and suffixes', () => {
    const r = read(P(R('x')), {
      numbering:
        '<w:numPicBullet w:numPicBulletId="0"><w:pict/></w:numPicBullet>' +
        '<w:abstractNum w:abstractNumId="1"><w:nsid w:val="ABCD1234"/><w:multiLevelType w:val="multilevel"/><w:numStyleLink w:val="MyList"/>' +
        '<w:lvl w:ilvl="0"><w:start w:val="3"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:isLgl/><w:suff w:val="space"/><w:lvlJc w:val="end"/></w:lvl>' +
        '<w:lvl w:ilvl="1"><w:numFmt w:val="custom" w:format="001, 002, 003, ..."/><w:lvlText w:null="1"/><w:lvlRestart w:val="0"/></w:lvl></w:abstractNum>' +
        '<w:num w:numId="1"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="0"><w:lvl w:ilvl="0"><w:start w:val="10"/><w:numFmt w:val="upperLetter"/><w:lvlText w:val="%1)"/></w:lvl></w:lvlOverride></w:num>' +
        '<w:num w:numId="2"><w:abstractNumId w:val="99"/></w:num>',
    });
    const abs = r.numbering.abstracts.get(1);
    expect(abs).toMatchObject({ nsid: 'ABCD1234', multiLevelType: 'multilevel', numStyleLink: 'MyList' });
    expect(abs?.levels[0]).toMatchObject({ start: 3, isLegal: true, suffix: 'space', alignment: 'right' });
    expect(abs?.levels[1]).toMatchObject({
      start: 0,
      format: 'custom',
      customFormat: '001, 002, 003, ...',
      text: '',
      restartAfter: 0,
    });
    expect(r.numbering.instances.get(1)?.overrides[0]?.definition).toMatchObject({
      start: 10,
      format: 'upperLetter',
      text: '%1)',
    });
    expect(r.numbering.instances.has(2)).toBe(false);
    expect(r.extras.numPicBullets).toHaveLength(1);
  });

  it('reads settings', () => {
    const r = read(P(R('x')), {
      settings:
        '<w:trackRevisions/><w:doNotTrackMoves/><w:defaultTabStop w:val="709"/><w:evenAndOddHeaders/><w:mirrorMargins w:val="0"/><w:updateFields w:val="true"/><w:documentProtection w:edit="readOnly" w:enforcement="1"/><w:footnotePr><w:numFmt w:val="chicago"/></w:footnotePr><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat><w:themeFontLang w:val="de-DE" w:bidi="ar-SA"/><w:clrSchemeMapping w:bg1="light1" w:t1="dark1"/><w:decimalSymbol w:val=","/><w:listSeparator w:val=";"/>',
    });
    expect(r.settings).toMatchObject({
      trackRevisions: true,
      doNotTrackMoves: true,
      defaultTabStop: 709,
      evenAndOddHeaders: true,
      mirrorMargins: false,
      updateFields: true,
      protection: { edit: 'readOnly', enforced: true },
      footnotePr: { numFmt: 'chicago' },
      compatibilityMode: 14,
      themeFontLang: { val: 'de-DE', bidi: 'ar-SA' },
      colorSchemeMapping: { bg1: 'light1', t1: 'dark1' },
      decimalSymbol: ',',
      listSeparator: ';',
    });
    expect(r.extras.settingsXml).toMatch(/^<w:settings /);
  });

  it('treats a compat block without compatSetting as Word 2007', () => {
    expect(
      read(P(R('x')), { settings: '<w:compat><w:useFELayout/></w:compat>' }).settings.compatibilityMode,
    ).toBe(12);
  });

  it('reads the page background colour', () => {
    const bytes = buildDocx({ body: P(R('x')) });
    const pkg = OpcPackage.open(bytes);
    const xml = (pkg.getPartText('/word/document.xml') ?? '').replace(
      '<w:body>',
      '<w:background w:color="FFF2CC"/><w:body>',
    );
    pkg.setPart('/word/document.xml', xml);
    const r = readDocx(pkg.save());
    expect(r.settings.background).toBe('FFF2CC');
    expect(r.extras.background).toBe('<w:background w:color="FFF2CC"/>');
  });
});

describe('unprefixed attributes', () => {
  it('accepts WordprocessingML written with a default namespace and unprefixed attributes', () => {
    const r = readDocx(loadFixture('docx/manual/foreign-prefixes.docx'));
    expect(r.warnings).toEqual([]);
    expect(r.comments).toHaveLength(1);
    expect(marksOn(r.doc, 'commented').comment).toEqual({ id: '0' });
  });
});
