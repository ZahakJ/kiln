/**
 * Element-level writer tests: hand-written packages for every construct the reader understands are read,
 * written and read again; the two models must agree (ids included), and the written XML must follow
 * Office's element order.
 */
import { RelType } from '@kiln-ref/ooxml';
import { TableMap } from '@tiptap/pm/tables';
import { describe, expect, it } from 'vitest';
import { GRID_PLACEHOLDER, readDocx } from '../../src/index.ts';
import {
  count,
  expectOrder,
  expectSameModel,
  marksOn,
  nodes,
  P,
  PNG,
  R,
  roundTrip,
  write,
} from './helpers.ts';

const MAIN = '/word/document.xml';

const imagePart = (name = '/word/media/image1.png', relId = 'rIdImg') => ({
  name,
  contentType: 'image/png',
  relType: RelType.image,
  relId,
  content: PNG,
});

const picGraphic = (rel = 'rIdImg') =>
  `<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="p"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="${rel}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="457200"/></a:xfrm><a:prstGeom prst="rect"/></pic:spPr></pic:pic></a:graphicData>`;
const inline = (graphicData: string, docPr = '<wp:docPr id="1" name="Picture 1" descr="A chart"/>') =>
  `<w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="457200"/>${docPr}<a:graphic>${graphicData}</a:graphic></wp:inline></w:drawing></w:r>`;

describe('runs', () => {
  it('writes every modelled run property back', () => {
    const rPr =
      '<w:rStyle w:val="Strong"/><w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:eastAsiaTheme="minorEastAsia" w:hint="eastAsia"/><w:b/><w:bCs w:val="0"/><w:i/><w:caps/><w:strike w:val="false"/><w:outline/><w:emboss/><w:vanish/>' +
      '<w:color w:val="1F3864" w:themeColor="accent1" w:themeShade="80"/><w:spacing w:val="20"/><w:w w:val="90"/><w:kern w:val="28"/><w:position w:val="-4"/><w:sz w:val="28"/><w:szCs w:val="30"/>' +
      '<w:highlight w:val="yellow"/><w:u w:val="double" w:color="FF0000"/><w:shd w:val="clear" w:color="auto" w:fill="E2EFDA" w:themeFill="accent6" w:themeFillTint="33"/><w:vertAlign w:val="superscript"/><w:rtl/><w:lang w:val="en-GB" w:eastAsia="ja-JP" w:bidi="ar-SA"/>';
    const rt = roundTrip(P(R('styled', rPr)));
    expectSameModel(rt);
    expect(marksOn(rt.b.doc, 'styled').fontSize).toEqual({ size: 28, sizeCs: 30 });
  });

  it('keeps unmodelled run properties as raw XML in schema order', () => {
    const rt = roundTrip(
      P(
        R(
          'raw',
          '<w:noProof/><w:sz w:val="20"/><w:em w:val="dot"/><w14:ligatures w14:val="standard"/><w:b/>',
        ),
      ),
    );
    expectSameModel(rt);
    const rPr = /<w:r><w:rPr>(.*?)<\/w:rPr><w:t>raw/.exec(rt.xml(MAIN))?.[1] ?? '';
    expectOrder(rPr, ['<w:b/>', '<w:noProof/>', '<w:sz ', '<w:em ', '<w14:ligatures']);
    expect(rt.xml(MAIN)).toMatch(/mc:Ignorable="[^"]*w14/);
  });

  it('writes breaks, carriage returns and special hyphens (as their characters)', () => {
    const rt = roundTrip(
      P(
        '<w:r><w:t>a</w:t><w:br/><w:t>b</w:t><w:br w:type="page"/><w:br w:type="column"/><w:br w:clear="all"/><w:cr/><w:t>c</w:t><w:noBreakHyphen/><w:t>d</w:t><w:softHyphen/><w:t>e</w:t></w:r>',
      ),
    );
    expectSameModel(rt);
    const xml = rt.xml(MAIN);
    expect(xml).toContain('<w:br w:type="page"/>');
    expect(xml).toContain('<w:br w:type="column"/>');
    expect(xml).toContain('<w:br w:clear="all"/>');
    expect(xml).toContain('<w:t>c\u2011d\u00ADe</w:t>');
  });

  it('restores symbol characters as w:sym with their font and code', () => {
    const rt = roundTrip(
      P(
        '<w:r><w:sym w:font="Wingdings" w:char="F04A"/><w:sym w:font="Wingdings" w:char="F04A"/></w:r><w:r><w:sym w:font="Symbol" w:char="F061"/></w:r><w:r><w:sym w:font="Mystery" w:char="F0FF"/></w:r>',
      ),
    );
    expectSameModel(rt);
    const xml = rt.xml(MAIN);
    expect(count(xml, '<w:sym w:font="Wingdings" w:char="F04A"/>')).toBe(2);
    expect(xml).toContain('<w:sym w:font="Symbol" w:char="F061"/>');
    expect(xml).toContain('<w:sym w:font="Mystery" w:char="F0FF"/>');
  });

  it('preserves significant spaces and merges runs with identical formatting', () => {
    const rt = roundTrip(P(R('  lead') + R('mid  dle') + R('trail ') + R('bold', '<w:b/>')));
    expectSameModel(rt);
    const xml = rt.xml(MAIN);
    expect(xml).toContain('<w:t xml:space="preserve">  leadmid  dletrail </w:t>');
    expect(xml).toContain('<w:rPr><w:b/></w:rPr><w:t>bold</w:t>');
  });
});

describe('paragraphs', () => {
  it('writes every modelled paragraph property back', () => {
    const pPr =
      '<w:pStyle w:val="Heading1"/><w:keepNext w:val="0"/><w:keepLines/><w:pageBreakBefore/><w:widowControl w:val="0"/><w:numPr><w:ilvl w:val="2"/><w:numId w:val="7"/></w:numPr><w:suppressLineNumbers/>' +
      '<w:pBdr><w:top w:val="single" w:sz="4" w:space="1" w:color="auto"/><w:between w:val="dotted" w:sz="2" w:space="0" w:color="4472C4" w:themeColor="accent1"/></w:pBdr><w:shd w:val="pct10" w:color="FF0000" w:fill="FFFF00"/>' +
      '<w:tabs><w:tab w:val="right" w:leader="dot" w:pos="9350"/><w:tab w:val="decimal" w:pos="4000"/></w:tabs><w:suppressAutoHyphens/><w:bidi/><w:spacing w:before="120" w:after="0" w:line="360" w:lineRule="auto" w:beforeAutospacing="1"/>' +
      '<w:ind w:left="720" w:right="360" w:hanging="360"/><w:contextualSpacing/><w:jc w:val="both"/><w:textAlignment w:val="center"/><w:outlineLvl w:val="0"/>';
    const rt = roundTrip(P(R('p'), pPr));
    expectSameModel(rt);
  });

  it('keeps unmodelled paragraph properties in schema order without duplicating numbering', () => {
    const rt = roundTrip(
      P(
        R('x'),
        '<w:pStyle w:val="Normal"/><w:framePr w:w="2000" w:h="1000" w:hSpace="180" w:wrap="around" w:vAnchor="text" w:hAnchor="page" w:x="1000" w:y="1"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="3"/><w:ins w:id="5" w:author="A" w:date="2026-01-01T00:00:00Z"/></w:numPr><w:snapToGrid w:val="0"/><w:jc w:val="left"/><w:textDirection w:val="tbRl"/><w:cnfStyle w:val="100000000000"/>',
      ),
    );
    expectSameModel(rt);
    const pPr = /<w:pPr>(.*?)<\/w:pPr>/.exec(rt.xml(MAIN))?.[1] ?? '';
    expect(count(pPr, '<w:numPr>')).toBe(1);
    expectOrder(pPr, [
      '<w:pStyle',
      '<w:framePr',
      '<w:numPr>',
      '<w:snapToGrid',
      '<w:jc',
      '<w:textDirection',
      '<w:cnfStyle',
    ]);
  });

  it('writes drop caps as frames', () => {
    const rt = roundTrip(
      P(
        R('T'),
        '<w:framePr w:dropCap="drop" w:lines="3" w:wrap="around" w:vAnchor="text" w:hAnchor="text"/>',
      ),
    );
    expectSameModel(rt);
    expect(nodes(rt.b.doc, 'paragraph')[0]?.attrs.dropCap).toEqual({ type: 'drop', lines: 3 });
  });

  it('writes paragraph-mark revisions, paragraph and mark formatting changes', () => {
    const rt = roundTrip(
      P(
        R('changed'),
        '<w:jc w:val="center"/><w:rPr><w:ins w:id="11" w:author="Ann" w:date="2026-02-01T10:00:00Z"/><w:b/><w:rPrChange w:id="12" w:author="Ann" w:date="2026-02-01T10:00:00Z"><w:rPr><w:i/></w:rPr></w:rPrChange></w:rPr><w:pPrChange w:id="13" w:author="Bob" w:date="2026-02-02T10:00:00Z"><w:pPr><w:jc w:val="right"/><w:spacing w:after="0"/></w:pPr></w:pPrChange>',
      ),
    );
    expectSameModel(rt);
    const pPr = /<w:pPr>(.*?)<\/w:pPr><w:r>/.exec(rt.xml(MAIN))?.[1] ?? '';
    expectOrder(pPr, [
      '<w:jc w:val="center"/>',
      '<w:rPr><w:ins w:id="11"',
      '<w:b/>',
      '<w:rPrChange',
      '</w:rPr>',
      '<w:pPrChange',
    ]);
  });

  it('writes w14:paraId with the w14 namespace declared and ignorable', () => {
    const rt = roundTrip('<w:p w14:paraId="1A2B3C4D" w14:textId="77777777"><w:r><w:t>x</w:t></w:r></w:p>');
    expectSameModel(rt);
    const xml = rt.xml(MAIN);
    expect(xml).toContain('<w:p w14:paraId="1A2B3C4D">');
    expect(xml).toContain('xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"');
    expect(xml).toMatch(/mc:Ignorable="w14"/);
  });
});

describe('revisions', () => {
  it('writes insertions, deletions and formatting changes with authors and dates', () => {
    const rt = roundTrip(
      P(
        `${R('keep ')}<w:ins w:id="1" w:author="Ann" w:date="2026-03-01T09:00:00Z">${R('added ')}</w:ins><w:del w:id="2" w:author="Bob" w:date="2026-03-02T09:00:00Z"><w:r><w:delText xml:space="preserve">gone </w:delText></w:r></w:del>${R('fmt', '<w:b/><w:rPrChange w:id="3" w:author="Cy" w:date="2026-03-03T09:00:00Z"><w:rPr><w:i/></w:rPr></w:rPrChange>')}`,
      ),
    );
    expectSameModel(rt);
    expect(marksOn(rt.b.doc, 'added').insertion).toEqual({
      id: '1',
      author: 'Ann',
      date: '2026-03-01T09:00:00Z',
    });
    expect(marksOn(rt.b.doc, 'gone').deletion).toMatchObject({ author: 'Bob' });
    expect(rt.xml(MAIN)).toContain('<w:delText xml:space="preserve">gone </w:delText>');
  });

  it('writes tracked moves as an insertion and a deletion', () => {
    const rt = roundTrip(
      P(
        `<w:moveFrom w:id="4" w:author="A" w:date="2026-01-01T00:00:00Z"><w:r><w:delText>moved</w:delText></w:r></w:moveFrom><w:moveTo w:id="5" w:author="A" w:date="2026-01-01T00:00:00Z">${R('moved')}</w:moveTo>`,
      ),
    );
    expectSameModel(rt, { ignoreWarnings: true });
    expect(rt.a.warnings.map((w) => w.code)).toContain('revision.move');
    expect(rt.b.warnings).toEqual([]);
  });

  it('writes deleted field codes as w:delInstrText', () => {
    const rt = roundTrip(
      P(
        '<w:del w:id="6" w:author="A" w:date="2026-01-01T00:00:00Z"><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:delInstrText> PAGE </w:delInstrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:delText>3</w:delText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:del>',
      ),
    );
    expectSameModel(rt);
    expect(rt.xml(MAIN)).toContain('<w:delInstrText xml:space="preserve"> PAGE </w:delInstrText>');
  });

  it('writes tracked row insertions and keeps cell revisions', () => {
    const rt = roundTrip(
      '<w:tbl><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:trPr><w:ins w:id="7" w:author="A" w:date="2026-01-01T00:00:00Z"/></w:trPr><w:tc><w:tcPr><w:cellIns w:id="8" w:author="A" w:date="2026-01-01T00:00:00Z"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl><w:p/>',
    );
    expectSameModel(rt);
    expect(nodes(rt.b.doc, 'tableRow')[0]?.attrs.revision).toMatchObject({ kind: 'insertion', id: '7' });
  });
});

describe('fields', () => {
  it('writes complex fields with their cached result, dirty and locked flags', () => {
    const rt = roundTrip(
      P(
        '<w:r><w:fldChar w:fldCharType="begin" w:dirty="true"/></w:r><w:r><w:instrText> DATE \\@ "d MMMM yyyy" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>15 October</w:t><w:tab/><w:t>2026</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>' +
          '<w:r><w:fldChar w:fldCharType="begin" w:fldLock="1"/></w:r><w:r><w:instrText>AUTHOR</w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>',
      ),
    );
    expectSameModel(rt);
    const [date, author] = nodes(rt.b.doc, 'field');
    expect(date?.attrs).toMatchObject({ type: 'DATE', result: '15 October\t2026', dirty: true });
    expect(author?.attrs).toMatchObject({ type: 'AUTHOR', locked: true });
    expect(rt.xml(MAIN)).toContain('<w:fldChar w:fldCharType="begin" w:dirty="true"/>');
  });

  it('writes nested fields inside an instruction as nested complex fields', () => {
    const rt = roundTrip(
      P(
        '<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> IF </w:instrText></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> MERGEFIELD Gender </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>F</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:instrText xml:space="preserve"> = "F" "Ms" "Mr" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>Ms</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>',
      ),
    );
    expectSameModel(rt);
    expect(nodes(rt.b.doc, 'field')[0]?.attrs.instr).toBe('IF { MERGEFIELD Gender } = "F" "Ms" "Mr"');
    expect(count(rt.xml(MAIN), 'w:fldCharType="begin"')).toBe(2);
  });

  it('writes simple fields as complex fields', () => {
    const rt = roundTrip(P('<w:fldSimple w:instr=" NUMPAGES "><w:r><w:t>4</w:t></w:r></w:fldSimple>'));
    expectSameModel(rt);
    expect(rt.xml(MAIN)).not.toContain('fldSimple');
    expect(nodes(rt.b.doc, 'field')[0]?.attrs).toMatchObject({ type: 'NUMPAGES', result: '4' });
  });

  it('writes HYPERLINK fields as hyperlinks and TOC blocks as fields across paragraphs', () => {
    const rt = roundTrip(
      `<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> TOC \\o "1-3" \\h \\z \\u </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> HYPERLINK \\l "_Toc1" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>${R('Intro')}<w:r><w:tab/></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> PAGEREF _Toc1 \\h </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>${R('1')}<w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>` +
        `${P(R('Second\t2'))}<w:p><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>${P(R('after'))}`,
    );
    expectSameModel(rt);
    const toc = nodes(rt.b.doc, 'toc')[0];
    expect(toc?.childCount).toBe(3);
    expect(marksOn(rt.b.doc, 'Intro').link).toMatchObject({ anchor: '_Toc1' });
    expect(rt.xml(MAIN)).toContain('<w:hyperlink w:anchor="_Toc1" w:history="1">');
  });

  it('writes w:pgNum page numbers as PAGE fields', () => {
    const rt = roundTrip(P('<w:r><w:pgNum/></w:r>'));
    expectSameModel(rt);
    expect(rt.xml(MAIN)).toContain('<w:instrText xml:space="preserve"> PAGE </w:instrText>');
  });
});

describe('content controls', () => {
  it('re-wraps block content controls around the blocks they held', () => {
    const rt = roundTrip(
      `<w:sdt><w:sdtPr><w:alias w:val="Intro"/><w:tag w:val="intro"/><w:id w:val="42"/></w:sdtPr><w:sdtEndPr/><w:sdtContent>${P(R('A'))}<w:sdt><w:sdtPr><w:id w:val="43"/><w:text/></w:sdtPr><w:sdtContent>${P(R('B'))}</w:sdtContent></w:sdt></w:sdtContent></w:sdt>${P(R('C'))}`,
    );
    expectSameModel(rt);
    const xml = rt.xml(MAIN);
    expect(count(xml, '<w:sdt>')).toBe(2);
    expectOrder(xml, [
      '<w:sdtPr><w:alias',
      '<w:sdtEndPr/>',
      '<w:sdtContent>',
      '>A<',
      '<w:sdt><w:sdtPr><w:id w:val="43"/>',
      '>B<',
      '</w:sdtContent></w:sdt></w:sdtContent></w:sdt>',
      '>C<',
    ]);
  });

  it('re-wraps inline content controls (checkbox, drop-down, date)', () => {
    const rt = roundTrip(
      P(
        `${R('Done: ')}<w:sdt><w:sdtPr><w14:checkbox><w14:checked w14:val="1"/></w14:checkbox></w:sdtPr><w:sdtContent>${R('☒')}</w:sdtContent></w:sdt>` +
          `<w:sdt><w:sdtPr><w:dropDownList><w:listItem w:displayText="Yes" w:value="Y"/></w:dropDownList></w:sdtPr><w:sdtContent>${R('Yes')}${R('!', '<w:b/>')}</w:sdtContent></w:sdt>`,
      ),
    );
    expectSameModel(rt);
    expect(count(rt.xml(MAIN), '<w:sdt>')).toBe(2);
  });

  it('re-wraps row-level and cell-level content controls', () => {
    const rt = roundTrip(
      `<w:tbl><w:tblGrid><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/></w:tblGrid><w:sdt><w:sdtPr><w15:repeatingSection/></w:sdtPr><w:sdtContent><w:sdt><w:sdtPr><w15:repeatingSectionItem/></w:sdtPr><w:sdtContent><w:tr><w:tc><w:p>${R('r1')}</w:p></w:tc><w:sdt><w:sdtPr><w:tag w:val="cell"/></w:sdtPr><w:sdtContent><w:tc><w:p>${R('c')}</w:p></w:tc></w:sdtContent></w:sdt></w:tr></w:sdtContent></w:sdt></w:sdtContent></w:sdt><w:tr><w:tc><w:p>${R('r2')}</w:p></w:tc><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/>`,
    );
    expectSameModel(rt);
    const xml = rt.xml(MAIN);
    expectOrder(xml, [
      '<w:tblGrid>',
      '<w15:repeatingSection/>',
      '<w15:repeatingSectionItem/>',
      '<w:tr>',
      '>r1<',
      '<w:tag w:val="cell"/>',
      '<w:tc>',
      '>c<',
      '</w:tr></w:sdtContent></w:sdt></w:sdtContent></w:sdt><w:tr>',
      '>r2<',
    ]);
  });

  it('keeps a table of contents inside its building-block control', () => {
    const rt = roundTrip(
      `<w:sdt><w:sdtPr><w:docPartObj><w:docPartGallery w:val="Table of Contents"/><w:docPartUnique/></w:docPartObj></w:sdtPr><w:sdtContent>${P(R('Contents'), '<w:pStyle w:val="TOCHeading"/>')}<w:p><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> TOC \\o "1-3" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>${R('Intro\t1')}</w:p><w:p><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p></w:sdtContent></w:sdt>${P(R('Body'))}`,
    );
    expectSameModel(rt);
    expect(count(rt.xml(MAIN), '<w:sdt>')).toBe(1);
  });
});

describe('tables', () => {
  const grid = '<w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid>';
  const tc = (text: string, tcPr = '') =>
    `<w:tc>${tcPr ? `<w:tcPr>${tcPr}</w:tcPr>` : ''}<w:p>${R(text)}</w:p></w:tc>`;

  it('writes table properties, widths in every unit and the table look', () => {
    const rt = roundTrip(
      `<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblpPr w:leftFromText="180" w:vertAnchor="text" w:tblpY="1"/><w:bidiVisual/><w:tblStyleRowBandSize w:val="2"/><w:tblW w:w="50%" w:type="pct"/><w:jc w:val="center"/><w:tblCellSpacing w:w="20" w:type="dxa"/><w:tblInd w:w="0.5in" w:type="dxa"/><w:tblBorders><w:top w:val="single" w:sz="8" w:space="0" w:color="auto"/><w:insideV w:val="nil"/></w:tblBorders><w:shd w:val="clear" w:color="auto" w:fill="EEEEEE"/><w:tblLayout w:type="fixed"/><w:tblCellMar><w:left w:w="57" w:type="dxa"/><w:right w:w="0.1in" w:type="dxa"/></w:tblCellMar><w:tblLook w:val="04A0"/><w:tblCaption w:val="Cap"/><w:tblDescription w:val="Desc"/></w:tblPr>${grid}<w:tr>${tc('a', '<w:tcW w:w="0" w:type="nil"/>')}${tc('b', '<w:tcW w:w="1in" w:type="dxa"/>')}${tc('c', '<w:tcW w:w="0" w:type="auto"/>')}</w:tr></w:tbl><w:p/>`,
    );
    expectSameModel(rt);
    const tblPr = /<w:tblPr>(.*?)<\/w:tblPr>/.exec(rt.xml(MAIN))?.[1] ?? '';
    expectOrder(tblPr, [
      '<w:tblStyle',
      '<w:tblpPr',
      '<w:bidiVisual',
      '<w:tblStyleRowBandSize',
      '<w:tblW',
      '<w:jc',
      '<w:tblCellSpacing',
      '<w:tblInd',
      '<w:tblBorders',
      '<w:shd',
      '<w:tblLayout',
      '<w:tblCellMar',
      '<w:tblLook',
      '<w:tblCaption',
      '<w:tblDescription',
    ]);
    expect(tblPr).toContain('w:firstRow="1"');
  });

  it('writes merged cells as gridSpan and vMerge restart/continue', () => {
    const rt = roundTrip(
      `<w:tbl>${grid}<w:tr>${tc('A1', '<w:gridSpan w:val="2"/><w:vMerge w:val="restart"/>')}${tc('C1')}</w:tr><w:tr>${tc('', '<w:gridSpan w:val="2"/><w:vMerge/>')}${tc('C2')}</w:tr><w:tr>${tc('A3')}${tc('B3')}${tc('C3')}</w:tr></w:tbl><w:p/>`,
    );
    expectSameModel(rt);
    const table = nodes(rt.b.doc, 'table')[0];
    if (!table) throw new Error('no table');
    expect(TableMap.get(table).width).toBe(3);
    expect(nodes(rt.b.doc, 'tableCell')[0]?.attrs).toMatchObject({ colspan: 2, rowspan: 2 });
    const xml = rt.xml(MAIN);
    expect(xml).toContain('<w:gridSpan w:val="2"/><w:vMerge w:val="restart"/>');
    expect(xml).toContain('<w:gridSpan w:val="2"/><w:vMerge/>');
  });

  it('writes legacy hMerge cells as one spanning cell', () => {
    const rt = roundTrip(
      `<w:tbl>${grid}<w:tr>${tc('L', '<w:hMerge w:val="restart"/>')}${tc('', '<w:hMerge/>')}${tc('R')}</w:tr></w:tbl><w:p/>`,
    );
    expect(nodes(rt.b.doc, 'tableCell').map((c) => c.attrs.colspan)).toEqual([2, 1]);
    expect(rt.xml(MAIN)).not.toContain('hMerge');
    expect(rt.b.doc.textContent).toBe(rt.a.doc.textContent);
  });

  it('drops grid placeholders and writes gridBefore, gridAfter and ragged rows', () => {
    const rt = roundTrip(
      `<w:tbl>${grid}<w:tr><w:trPr><w:gridBefore w:val="1"/></w:trPr>${tc('b')}${tc('c')}</w:tr><w:tr><w:trPr><w:gridAfter w:val="1"/></w:trPr>${tc('a')}${tc('b')}</w:tr><w:tr>${tc('only')}</w:tr></w:tbl><w:p/>`,
    );
    expectSameModel(rt, { ignoreWarnings: true });
    const xml = rt.xml(MAIN);
    expect(xml).not.toContain(GRID_PLACEHOLDER);
    expect(count(xml, '<w:tc>')).toBe(5);
    expect(xml).toContain('<w:gridBefore w:val="1"/>');
    expect(count(xml, '<w:gridAfter w:val="1"/>')).toBe(1);
    expect(xml).toContain('<w:gridAfter w:val="2"/>');
    expect(rt.b.warnings).toEqual([]);
  });

  it('writes a row made only of grid-before columns as a real cell (Word needs one)', () => {
    const rt = roundTrip(
      `<w:tbl>${grid}<w:tr>${tc('a')}${tc('b')}${tc('c')}</w:tr><w:tr><w:trPr><w:gridBefore w:val="3"/></w:trPr></w:tr></w:tbl><w:p/>`,
    );
    const rows = /<w:tbl>.*<\/w:tbl>/.exec(rt.xml(MAIN))?.[0].split('<w:tr>').slice(1) ?? [];
    expect(rows).toHaveLength(2);
    expect(rows[1]).toContain('<w:tc>');
    expect(rows[1]).not.toContain('gridBefore');
    expect(rt.b.doc.textContent).toBe('abc');
  });

  it('writes row and cell properties in schema order', () => {
    const rt = roundTrip(
      `<w:tbl>${grid}<w:tr><w:tblPrEx><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tblBorders></w:tblPrEx><w:trPr><w:cnfStyle w:val="100000000000"/><w:cantSplit/><w:trHeight w:val="400" w:hRule="exact"/><w:tblHeader/><w:jc w:val="center"/></w:trPr>${tc('x', '<w:cnfStyle w:val="001000000000"/><w:tcW w:w="2000" w:type="dxa"/><w:tcBorders><w:top w:val="double" w:sz="4" w:space="0" w:color="FF0000"/><w:tl2br w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tcBorders><w:shd w:val="clear" w:color="auto" w:fill="FFFF00"/><w:noWrap/><w:tcMar><w:top w:w="100" w:type="dxa"/></w:tcMar><w:textDirection w:val="btLr"/><w:tcFitText/><w:vAlign w:val="bottom"/><w:hideMark/>')}${tc('y')}${tc('z')}</w:tr></w:tbl><w:p/>`,
    );
    expectSameModel(rt);
    const xml = rt.xml(MAIN);
    expectOrder(xml, [
      '<w:tr><w:tblPrEx>',
      '<w:trPr><w:cnfStyle',
      '<w:cantSplit/>',
      '<w:trHeight w:val="400" w:hRule="exact"/>',
      '<w:tblHeader/>',
      '<w:jc w:val="center"/></w:trPr>',
    ]);
    const tcPr = /<w:tc><w:tcPr>(.*?)<\/w:tcPr>/.exec(xml)?.[1] ?? '';
    expectOrder(tcPr, [
      '<w:cnfStyle',
      '<w:tcW',
      '<w:tcBorders',
      '<w:shd',
      '<w:noWrap',
      '<w:tcMar',
      '<w:textDirection',
      '<w:tcFitText',
      '<w:vAlign',
      '<w:hideMark',
    ]);
  });

  it('writes nested tables and ends every cell with a paragraph', () => {
    const inner = `<w:tbl><w:tblGrid><w:gridCol w:w="900"/></w:tblGrid><w:tr>${tc('inner')}</w:tr></w:tbl>`;
    const rt = roundTrip(
      `<w:tbl>${grid}<w:tr><w:tc>${inner}<w:p/></w:tc>${tc('b')}${tc('c')}</w:tr></w:tbl><w:p/>`,
    );
    expectSameModel(rt);
    expect(nodes(rt.b.doc, 'table')).toHaveLength(2);
    expect(rt.xml(MAIN)).toContain('</w:tbl><w:p/></w:tc>');
  });

  it('recomputes the grid when columns were resized in the editor', () => {
    const rt = roundTrip(`<w:tbl>${grid}<w:tr>${tc('a')}${tc('b')}${tc('c')}</w:tr></w:tbl><w:p/>`);
    // Resize the first column to 200 px (3000 twips) the way prosemirror-tables does: colwidth only.
    const json = rt.a.doc.toJSON() as { content: { content: { type: string }[] }[] };
    const tableJson = json.content[0]?.content.find((c) => c.type === 'table') as unknown as {
      content: { content: { attrs: { colwidth: number[] } }[] }[];
    };
    const firstCell = tableJson.content[0]?.content[0];
    if (!firstCell) throw new Error('no cell');
    firstCell.attrs.colwidth = [200];
    const b = readDocx(write({ ...rt.a, doc: rt.a.doc.type.schema.nodeFromJSON(json) }));
    expect(nodes(b.doc, 'table')[0]?.attrs.grid).toEqual([3000, 2000, 2000]);
    // Unchanged widths keep the stored grid.
    expect(nodes(rt.b.doc, 'table')[0]?.attrs.grid).toEqual([2000, 2000, 2000]);
  });

  it('adds a paragraph to carry a section break after a table', () => {
    const rt = roundTrip(
      `${P(R('one'), '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:type w:val="continuous"/></w:sectPr>')}<w:tbl>${grid}<w:tr>${tc('a')}${tc('b')}${tc('c')}</w:tr></w:tbl><w:p><w:pPr><w:sectPr><w:pgSz w:w="15840" w:h="12240" w:orient="landscape"/></w:sectPr></w:pPr></w:p><w:tbl>${grid}<w:tr>${tc('d')}${tc('e')}${tc('f')}</w:tr></w:tbl><w:p/><w:sectPr/>`,
    );
    expectSameModel(rt);
    expect(rt.b.sections.map((s) => s.pageSize.orientation)).toEqual(['portrait', 'landscape', 'portrait']);
  });
});
