/**
 * Writer tests for drawings, equations, comments, notes, bookmarks, hyperlinks, sections with headers
 * and footers, and the styles, numbering and settings parts.
 */
import { OpcPackage, RelType } from '@kiln-ref/ooxml';
import { describe, expect, it } from 'vitest';
import { readDocx } from '../../src/index.ts';
import {
  buildDocx,
  count,
  expectOrder,
  expectSameModel,
  marksOn,
  nodes,
  P,
  PNG,
  part,
  R,
  roundTrip,
  roundTripBytes,
  write,
} from './helpers.ts';

const MAIN = '/word/document.xml';
const RELS = '/word/_rels/document.xml.rels';

const imagePart = (name = '/word/media/image1.png', relId = 'rIdImg') => ({
  name,
  contentType: 'image/png',
  relType: RelType.image,
  relId,
  content: PNG,
});
const picGraphic = (
  rel = 'rIdImg',
  xfrm = '<a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="457200"/></a:xfrm>',
  srcRect = '',
) =>
  `<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="p"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="${rel}"/>${srcRect}<a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr>${xfrm}<a:prstGeom prst="rect"/></pic:spPr></pic:pic></a:graphicData>`;
const inline = (graphicData: string, docPr = '<wp:docPr id="1" name="Picture 1" descr="A chart"/>') =>
  `<w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="457200"/>${docPr}<a:graphic>${graphicData}</a:graphic></wp:inline></w:drawing></w:r>`;
const anchor = (attrs: string, inner: string, graphic = picGraphic()) =>
  `<w:r><w:drawing><wp:anchor ${attrs}>${inner}<wp:extent cx="914400" cy="457200"/><wp:docPr id="9" name="a"/><a:graphic>${graphic}</a:graphic></wp:anchor></w:drawing></w:r>`;
const POS =
  '<wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="margin"><wp:align>center</wp:align></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>12700</wp:posOffset></wp:positionV>';
const WSP =
  '<a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="457200"/></a:xfrm><a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill><a:effectLst><a:outerShdw blurRad="40000" dist="20000" dir="5400000"><a:srgbClr val="000000"><a:alpha val="38000"/></a:srgbClr></a:outerShdw></a:effectLst></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>Inside</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData>';
const shapeRun = (choice: string) =>
  `<w:r><mc:AlternateContent><mc:Choice Requires="wps">${choice}</mc:Choice><mc:Fallback><w:pict><v:oval style="width:72pt;height:36pt"/></w:pict></mc:Fallback></mc:AlternateContent></w:r>`;
const shapeDrawing = `<w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="251659264" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1">${POS}<wp:extent cx="914400" cy="457200"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="5" name="Oval 5"/><wp:cNvGraphicFramePr/><a:graphic>${WSP}</a:graphic></wp:anchor></w:drawing>`;

describe('drawings', () => {
  it('writes inline pictures with alt text, title, crop, rotation, flips and decorative flag', () => {
    const rt = roundTrip(
      P(
        inline(
          picGraphic(
            'rIdImg',
            '<a:xfrm rot="5400000" flipH="1"><a:off x="0" y="0"/><a:ext cx="914400" cy="457200"/></a:xfrm>',
            '<a:srcRect l="10000" t="5000" r="0" b="2500"/>',
          ),
          '<wp:docPr id="3" name="Logo" descr="The logo" title="Logo"><a:extLst><a:ext uri="{C183D7F6-B498-43B3-948B-1728B52AA6E4}"><adec:decorative xmlns:adec="http://schemas.microsoft.com/office/drawing/2017/decorative" val="1"/></a:ext></a:extLst></wp:docPr>',
        ),
      ),
      { parts: [imagePart()] },
    );
    expectSameModel(rt);
    expect(nodes(rt.b.doc, 'image')[0]?.attrs).toMatchObject({
      alt: 'The logo',
      title: 'Logo',
      decorative: true,
      rotation: 90,
      flipH: true,
      crop: { l: 0.1, t: 0.05, r: 0, b: 0.025 },
    });
    const xml = rt.xml(MAIN);
    expectOrder(xml, [
      '<wp:inline',
      '<wp:extent',
      '<wp:effectExtent',
      '<wp:docPr',
      '<wp:cNvGraphicFramePr',
      '<a:graphic',
    ]);
    expect(rt.xml(RELS)).toContain('Target="media/image1.png"');
  });

  it('writes floating pictures with position, alignment, distances and every wrap mode', () => {
    const wraps = [
      '<wp:wrapSquare wrapText="bothSides"/>',
      '<wp:wrapTight wrapText="bothSides"><wp:wrapPolygon edited="0"><wp:start x="0" y="0"/><wp:lineTo x="0" y="21600"/></wp:wrapPolygon></wp:wrapTight>',
      '<wp:wrapThrough wrapText="bothSides"><wp:wrapPolygon edited="0"><wp:start x="0" y="0"/><wp:lineTo x="0" y="21600"/></wp:wrapPolygon></wp:wrapThrough>',
      '<wp:wrapTopAndBottom/>',
      '<wp:wrapNone/>',
    ];
    const runs = wraps.map((w, i) =>
      anchor(
        `distT="1" distB="2" distL="3" distR="4" simplePos="0" relativeHeight="${10 + i}" behindDoc="${i === 4 ? 1 : 0}" locked="0" layoutInCell="1" allowOverlap="0"`,
        `${POS}${w}`,
      ),
    );
    const rt = roundTrip(P(runs.join('')), { parts: [imagePart()] });
    expectSameModel(rt);
    expect(nodes(rt.b.doc, 'image').map((n) => n.attrs.wrap)).toEqual([
      'square',
      'tight',
      'through',
      'topAndBottom',
      'behind',
    ]);
    expect(nodes(rt.b.doc, 'image')[0]?.attrs.position).toMatchObject({
      relativeFromH: 'margin',
      h: { align: 'center' },
      v: { offset: 12700 },
      distT: 1,
      distR: 4,
      allowOverlap: false,
      zIndex: 10,
    });
    const xml = rt.xml(MAIN);
    expectOrder(xml, [
      '<wp:anchor',
      '<wp:simplePos',
      '<wp:positionH',
      '<wp:positionV',
      '<wp:extent',
      '<wp:effectExtent',
      '<wp:wrapSquare',
      '<wp:docPr',
      '<wp:cNvGraphicFramePr',
      '<a:graphic',
    ]);
    expect(count(xml, '<wp:wrapPolygon')).toBe(2);
  });

  it('gives every drawing a unique id above the ids kept in raw XML', () => {
    const rt = roundTrip(P(inline(picGraphic()) + inline(picGraphic()) + shapeRun(shapeDrawing)), {
      parts: [imagePart()],
    });
    const ids = [...rt.xml(MAIN).matchAll(/<wp:docPr id="(\d+)"/g)].map((m) => Number(m[1]));
    expect(new Set(ids).size).toBe(ids.length);
    expect(Math.min(...ids.filter((i) => i !== 5))).toBeGreaterThan(5);
  });

  it('writes pictures pasted as data URLs as new media parts', () => {
    const base = readDocx(buildDocx({ body: P(R('x')) }));
    const schema = base.doc.type.schema;
    const url = `data:image/png;base64,${Buffer.from(PNG).toString('base64')}`;
    const image = schema.nodes.image?.create({ src: url, width: 952500, height: 952500, alt: 'Pasted' });
    const doc = schema.node('doc', null, [
      schema.node('section', { props: base.sections[0] }, [
        schema.node('paragraph', null, image ? [image] : []),
      ]),
    ]);
    const b = readDocx(write({ ...base, doc }));
    const img = nodes(b.doc, 'image')[0];
    expect(img?.attrs.mediaId).toBe('/word/media/image1.png');
    expect(b.media.get('/word/media/image1.png')?.data).toEqual(PNG);
    expect(img?.attrs.alt).toBe('Pasted');
  });

  it('keeps shapes verbatim, AlternateContent included, when they are unchanged', () => {
    const rt = roundTrip(P(shapeRun(shapeDrawing)));
    expectSameModel(rt);
    const xml = rt.xml(MAIN);
    expect(xml).toContain('<mc:Fallback><w:pict><v:oval');
    expect(xml).toContain('<a:outerShdw');
    expect(xml).toContain('<w:t>Inside</w:t>');
  });

  it('patches the kept XML of shapes changed in the editor', () => {
    const a = readDocx(buildDocx({ body: P(shapeRun(shapeDrawing)) }));
    const json = a.doc.toJSON() as unknown as Record<string, unknown>;
    const walk = (n: Record<string, unknown>) => {
      if (n.type === 'shape') {
        const attrs = n.attrs as Record<string, unknown>;
        attrs.preset = 'roundRect';
        attrs.fill = { type: 'solid', color: { kind: 'srgb', hex: '00B050', transforms: [] } };
        attrs.width = 1828800;
        attrs.wrap = 'topAndBottom';
      }
      for (const c of (n.content as Record<string, unknown>[] | undefined) ?? []) walk(c);
    };
    walk(json);
    const bytes = write({ ...a, doc: a.doc.type.schema.nodeFromJSON(json) });
    const b = readDocx(bytes);
    const shape = nodes(b.doc, 'shape')[0]?.attrs;
    expect(shape).toMatchObject({ preset: 'roundRect', width: 1828800, wrap: 'topAndBottom' });
    expect(shape?.fill).toMatchObject({ type: 'solid' });
    const xml = OpcPackage.open(bytes).getPartText(MAIN) ?? '';
    expect(xml).not.toContain('mc:Fallback');
    expect(xml).toContain('<a:outerShdw');
    expect(xml).toContain('<w:t>Inside</w:t>');
    expect(xml).toContain('<wp:docPr id="5" name="Oval 5"/>');
  });

  it('writes shapes created in the editor as DrawingML shapes', () => {
    const base = readDocx(buildDocx({ body: P(R('x')) }));
    const schema = base.doc.type.schema;
    const shape = schema.nodes.shape?.create({
      preset: 'rightArrow',
      width: 914400,
      height: 457200,
      placement: 'inline',
      wrap: 'inline',
      fill: { type: 'solid', color: { kind: 'srgb', hex: '4472C4', transforms: [] } },
      line: { width: 12700, fill: { type: 'solid', color: { kind: 'srgb', hex: '000000', transforms: [] } } },
    });
    const doc = schema.node('doc', null, [
      schema.node('section', { props: base.sections[0] }, [
        schema.node('paragraph', null, shape ? [shape] : []),
      ]),
    ]);
    const b = readDocx(write({ ...base, doc }));
    expect(nodes(b.doc, 'shape')[0]?.attrs).toMatchObject({
      preset: 'rightArrow',
      width: 914400,
      placement: 'inline',
      fill: { type: 'solid' },
      line: { width: 12700 },
    });
    expect(b.warnings).toEqual([]);
  });

  it('keeps charts with their parts and relationship ids', () => {
    const chart = {
      name: '/word/charts/chart1.xml',
      contentType: 'application/vnd.openxmlformats-officedocument.drawingml.chart+xml',
      relType: RelType.chart,
      relId: 'rIdChart',
      content: '<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"/>',
    };
    const rt = roundTrip(
      P(
        inline(
          '<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart r:id="rIdChart"/></a:graphicData>',
        ),
      ),
      { parts: [chart] },
    );
    expectSameModel(rt);
    expect(rt.b.charts[0]).toMatchObject({ relId: 'rIdChart', chartPart: '/word/charts/chart1.xml' });
    expect(rt.xml(RELS)).toContain('Id="rIdChart"');
  });

  it('writes VML pictures as DrawingML and keeps other VML, OLE objects, SmartArt and groups', () => {
    const rt = roundTrip(
      P(
        '<w:r><w:pict><v:shape style="width:72pt;height:0.5in"><v:imagedata r:id="rIdImg" o:title="Logo"/></v:shape></w:pict></w:r>' +
          '<w:r><w:pict><v:rect><v:textbox><w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent></v:textbox></v:rect></w:pict></w:r>' +
          '<w:r><w:object><v:shape style="width:10pt;height:10pt"><v:imagedata r:id="rIdImg"/></v:shape><o:OLEObject Type="Embed" ProgID="Excel.Sheet.12" ShapeID="_x0000_i1025" DrawAspect="Content" ObjectID="_1" r:id="rIdOle"/></w:object></w:r>' +
          inline('<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"/>') +
          inline('<a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"/>'),
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
    expect(nodes(rt.b.doc, 'image')[0]?.attrs).toMatchObject({
      title: 'Logo',
      width: 914400,
      mediaId: '/word/media/image1.png',
    });
    expect(nodes(rt.b.doc, 'opaqueInline').map((n) => n.attrs.label)).toEqual([
      'Boxed',
      'Embedded Excel.Sheet.12',
      'SmartArt',
      'Group',
    ]);
    // The OLE object's preview image shares the picture's relationship id.
    const rels = rt.xml(RELS);
    expect(rels).toMatch(/Id="rIdImg"[^>]*Target="media\/image1.png"/);
    expect(rels).toMatch(/Id="rIdOle"[^>]*Target="embeddings\/Microsoft_Excel_Worksheet.xlsx"/);
    expect(rt.b.passthrough.has('/word/embeddings/Microsoft_Excel_Worksheet.xlsx')).toBe(true);
  });

  it('reports pictures whose image data is missing instead of writing broken drawings', () => {
    const base = readDocx(buildDocx({ body: P(R('x')) }));
    const schema = base.doc.type.schema;
    const image = schema.nodes.image?.create({ mediaId: '/word/media/gone.png', width: 10, height: 10 });
    const doc = schema.node('doc', null, [
      schema.node('section', { props: base.sections[0] }, [
        schema.node('paragraph', null, image ? [image] : []),
      ]),
    ]);
    const bytes = write({ ...base, doc });
    expect(nodes(readDocx(bytes).doc, 'image')).toHaveLength(0);
    expect(OpcPackage.open(bytes).getPartText(MAIN)).not.toContain('<w:drawing>');
  });
});

describe('equations', () => {
  it('writes inline and display equations as their Office Math markup', () => {
    const omml =
      '<m:oMath><m:r><m:t>x</m:t></m:r><m:sSup><m:e><m:r><m:t>y</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:oMath>';
    const rt = roundTrip(P(`${R('Inline ')}${omml}`) + P(`<m:oMathPara>${omml}</m:oMathPara>`));
    expectSameModel(rt);
    expect(nodes(rt.b.doc, 'equation').map((n) => n.attrs.display)).toEqual([false, true]);
    const xml = rt.xml(MAIN);
    expect(xml).toContain('<m:oMathPara><m:oMath>');
    expect(xml).toContain('xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"');
  });
});

describe('comments', () => {
  const commentsPart = (inner: string) => ({
    name: '/word/comments.xml',
    contentType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml',
    relType: RelType.comments,
    content: part('w:comments', inner),
  });
  const extPart = (inner: string) => ({
    name: '/word/commentsExtended.xml',
    contentType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml',
    relType: RelType.commentsExtended,
    content: part('w15:commentsEx', inner),
  });

  it('writes threads, resolved state, people and comment ranges', () => {
    const rt = roundTrip(
      P(
        `<w:commentRangeStart w:id="1"/><w:commentRangeStart w:id="2"/>${R('discussed')}<w:commentRangeEnd w:id="1"/><w:r><w:commentReference w:id="1"/></w:r><w:commentRangeEnd w:id="2"/><w:r><w:commentReference w:id="2"/></w:r>`,
      ),
      {
        parts: [
          commentsPart(
            '<w:comment w:id="1" w:author="Ann" w:date="2026-01-01T00:00:00Z" w:initials="A"><w:p w14:paraId="00000001"><w:r><w:t>Why?</w:t></w:r></w:p></w:comment><w:comment w:id="2" w:author="Bo"><w:p w14:paraId="00000002"><w:r><w:rPr><w:b/></w:rPr><w:t>Because.</w:t></w:r></w:p></w:comment>',
          ),
          extPart(
            '<w15:commentEx w15:paraId="00000001" w15:done="1"/><w15:commentEx w15:paraId="00000002" w15:paraIdParent="00000001" w15:done="0"/>',
          ),
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
    expectSameModel(rt);
    expect(rt.b.comments.map((c) => [c.id, c.parentId, c.resolved])).toEqual([
      ['1', undefined, true],
      ['2', '1', undefined],
    ]);
    expect(rt.b.people).toEqual([{ author: 'Ann', providerId: 'AD', userId: 'S-1' }]);
    const comments = rt.xml('/word/comments.xml');
    expect(comments).toContain(
      '<w:comment w:id="1" w:author="Ann" w:date="2026-01-01T00:00:00Z" w:initials="A">',
    );
    expect(comments).toContain('<w:p w14:paraId="00000001"><w:r><w:annotationRef/></w:r>');
    expect(rt.xml('/word/commentsExtended.xml')).toContain(
      'w15:paraId="00000002" w15:paraIdParent="00000001" w15:done="0"',
    );
    const xml = rt.xml(MAIN);
    expectOrder(xml, [
      '<w:commentRangeStart w:id="1"/>',
      '>discussed<',
      '<w:commentRangeEnd w:id="1"/>',
      '<w:commentReference w:id="1"/>',
    ]);
  });

  it('writes comment ranges that span paragraphs', () => {
    const rt = roundTrip(
      `<w:commentRangeStart w:id="7"/>${P(R('first'))}${P(`${R('second')}<w:commentRangeEnd w:id="7"/>`)}${P(R('third'))}`,
      {
        parts: [
          commentsPart('<w:comment w:id="7" w:author="A"><w:p><w:r><w:t>Note</w:t></w:r></w:p></w:comment>'),
        ],
      },
    );
    expectSameModel(rt, { ignoreWarnings: true });
    expect(marksOn(rt.b.doc, 'first').comment).toEqual({ id: '7' });
    expect(marksOn(rt.b.doc, 'second').comment).toEqual({ id: '7' });
    expect(marksOn(rt.b.doc, 'third').comment).toBeUndefined();
    expect(rt.b.comments[0]?.text).toBe('Note');
  });

  it('writes comments added in the editor from their plain text', () => {
    const base = readDocx(buildDocx({ body: P(R('x')) }));
    const schema = base.doc.type.schema;
    const mark = schema.marks.comment?.create({ id: '0' });
    const doc = schema.node('doc', null, [
      schema.node('section', { props: base.sections[0] }, [
        schema.node('paragraph', null, [schema.text('hello', mark ? [mark] : [])]),
      ]),
    ]);
    const b = readDocx(
      write({
        ...base,
        doc,
        comments: [
          { id: '0', author: 'Me', date: '2026-10-15T09:00:00Z', text: 'Line one\nLine two' },
          { id: '1', author: 'You', date: '', text: 'Reply', parentId: '0', resolved: true },
        ],
      }),
    );
    expect(b.comments.map((c) => [c.author, c.text, c.parentId ?? null, c.resolved ?? false])).toEqual([
      ['Me', 'Line one\nLine two', null, false],
      ['You', 'Reply', '0', true],
    ]);
    expect(marksOn(b.doc, 'hello').comment).toEqual({ id: '0' });
  });
});

describe('notes, bookmarks and hyperlinks', () => {
  const notesPart = (kind: 'footnote' | 'endnote', inner: string) => ({
    name: `/word/${kind}s.xml`,
    contentType: `application/vnd.openxmlformats-officedocument.wordprocessingml.${kind}s+xml`,
    relType: kind === 'footnote' ? RelType.footnotes : RelType.endnotes,
    content: part(`w:${kind}s`, inner),
  });

  it('writes footnotes and endnotes with separators, references and custom marks', () => {
    const rt = roundTrip(
      P(
        `${R('Text')}<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="1"/></w:r><w:r><w:footnoteReference w:customMarkFollows="1" w:id="2"/><w:t>*</w:t></w:r><w:r><w:endnoteReference w:id="1"/></w:r>`,
      ),
      {
        parts: [
          notesPart(
            'footnote',
            '<w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote><w:footnote w:id="1"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve"> First note.</w:t></w:r></w:p></w:footnote><w:footnote w:id="2"><w:p><w:r><w:t>Starred.</w:t></w:r></w:p></w:footnote>',
          ),
          notesPart(
            'endnote',
            '<w:endnote w:id="1"><w:p><w:r><w:endnoteRef/></w:r><w:r><w:t>End.</w:t></w:r></w:p></w:endnote>',
          ),
        ],
      },
    );
    expectSameModel(rt);
    const footnotes = rt.xml('/word/footnotes.xml');
    expectOrder(footnotes, [
      'w:type="separator" w:id="-1"',
      '<w:separator/>',
      'w:type="continuationSeparator" w:id="0"',
      '<w:footnote w:id="1"><w:p><w:r><w:footnoteRef/></w:r>',
    ]);
    expect(rt.xml(MAIN)).toContain('<w:footnoteReference w:customMarkFollows="1" w:id="2"/><w:t>*</w:t>');
    expect(rt.xml('/word/endnotes.xml')).toContain('<w:endnoteRef/>');
    expect(rt.b.footnotes.map((n) => n.node.textContent)).toEqual([' First note.', 'Starred.']);
  });

  it('keeps bookmarks inside a hyperlink from splitting it', () => {
    const rt = roundTrip(
      P(
        `<w:hyperlink w:anchor="target">${R('one ')}<w:bookmarkStart w:id="3" w:name="inside"/>${R('two')}<w:bookmarkEnd w:id="3"/></w:hyperlink><w:bookmarkStart w:id="4" w:name="target"/>${R('dest')}<w:bookmarkEnd w:id="4"/>`,
      ),
    );
    expectSameModel(rt);
    const xml = rt.xml(MAIN);
    expect(count(xml, '<w:hyperlink ')).toBe(1);
    expectOrder(xml, [
      '<w:hyperlink w:anchor="target"',
      '<w:bookmarkStart w:id="3" w:name="inside"/>',
      '</w:hyperlink>',
      '<w:bookmarkStart w:id="4" w:name="target"/>',
    ]);
  });

  it('writes hyperlinks with relationships, tooltips and frames, sharing one relationship per URL', () => {
    const rt = roundTrip(
      P(
        `<w:hyperlink r:id="rIdL" w:tooltip="Go" w:tgtFrame="_blank" w:history="1">${R('ext')}</w:hyperlink>${R(' and ')}<w:hyperlink r:id="rIdL">${R('again', '<w:b/>')}</w:hyperlink>`,
      ),
      { rels: [['rIdL', RelType.hyperlink, 'https://example.org/a?b=1&c=2', true]] },
    );
    expectSameModel(rt);
    expect(marksOn(rt.b.doc, 'ext').link).toEqual({
      href: 'https://example.org/a?b=1&c=2',
      anchor: null,
      tooltip: 'Go',
      target: '_blank',
    });
    const rels = rt.xml(RELS);
    expect(count(rels, 'https://example.org/a?b=1&amp;c=2')).toBe(1);
    expect(rels).toContain('TargetMode="External"');
  });

  it('keeps altChunk imports with their parts', () => {
    const rt = roundTrip(`${P(R('before'))}<w:altChunk r:id="rIdA"/>${P(R('after'))}`, {
      parts: [
        {
          name: '/word/afchunk.html',
          contentType: 'text/html',
          relType: 'http://schemas.openxmlformats.org/officeDocument/2006/relationships/aFChunk',
          relId: 'rIdA',
          content: '<p>hi</p>',
        },
      ],
    });
    expectSameModel(rt);
    expect(rt.xml(MAIN)).toContain('<w:altChunk r:id="rIdA"/>');
  });
});

describe('sections, headers and footers', () => {
  const hdr = (name: string, id: string, inner: string, root = 'w:hdr') => ({
    name,
    contentType: `application/vnd.openxmlformats-officedocument.wordprocessingml.${root === 'w:hdr' ? 'header' : 'footer'}+xml`,
    relType: root === 'w:hdr' ? RelType.header : RelType.footer,
    relId: id,
    content: part(root, inner),
  });

  it('writes sections with their breaks, headers and footers, sharing parts between sections', () => {
    const rt = roundTrip(
      `${P(R('one'), '<w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:type w:val="continuous"/><w:pgSz w:w="12240" w:h="15840"/><w:cols w:num="3" w:space="720"/></w:sectPr>')}${P(R('two'))}<w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:footerReference w:type="first" r:id="rIdF"/><w:footerReference w:type="even" r:id="rIdE"/><w:titlePg/><w:pgSz w:w="15840" w:h="12240"/><w:vAlign w:val="center"/><w:docGrid w:linePitch="360"/></w:sectPr>`,
      {
        parts: [
          hdr(
            '/word/header1.xml',
            'rIdH',
            `<w:p><w:r><w:t>Head</w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>`,
          ),
          hdr('/word/footer1.xml', 'rIdF', '<w:p><w:r><w:t>First foot</w:t></w:r></w:p>', 'w:ftr'),
          hdr('/word/footer2.xml', 'rIdE', '<w:p><w:r><w:t>Even foot</w:t></w:r></w:p>', 'w:ftr'),
        ],
      },
    );
    expectSameModel(rt);
    expect(rt.b.sections.map((s) => s.type)).toEqual(['continuous', 'nextPage']);
    expect(count(rt.xml(RELS), 'header1.xml')).toBe(1);
    const xml = rt.xml(MAIN);
    expectOrder(xml, [
      '<w:pPr><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:type w:val="continuous"/>',
      '</w:sectPr></w:pPr>',
      '>one<',
      '>two<',
      '<w:sectPr><w:headerReference',
      '<w:footerReference w:type="first" r:id="rIdF"/>',
      '<w:footerReference w:type="even" r:id="rIdE"/>',
      '<w:pgSz',
      '<w:vAlign',
      '<w:titlePg/>',
      '<w:docGrid',
      '</w:sectPr></w:body>',
    ]);
    expect(rt.xml('/word/header1.xml')).toContain('<w:instrText xml:space="preserve"> PAGE </w:instrText>');
  });

  it('writes pictures in headers with relationships from the header part', () => {
    const header = {
      name: '/word/header1.xml',
      contentType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml',
      relType: RelType.header,
      relId: 'rIdH',
      content: part('w:hdr', `<w:p>${inline(picGraphic('rIdHI'))}</w:p>`),
    };
    const pkg = OpcPackage.open(
      buildDocx({
        body: `${P(R('x'))}<w:sectPr><w:headerReference w:type="default" r:id="rIdH"/></w:sectPr>`,
        parts: [header],
      }),
    );
    pkg.addPart('/word/media/logo.png', 'image/png', PNG);
    pkg.addRelationship('/word/header1.xml', RelType.image, '/word/media/logo.png', { id: 'rIdHI' });
    const rt = roundTripBytes(pkg.save());
    expectSameModel(rt);
    expect(rt.xml('/word/_rels/header1.xml.rels')).toMatch(/Id="rIdHI"[^>]*Target="media\/logo.png"/);
    expect(nodes(rt.b.doc, 'image')[0]?.attrs.mediaId).toBe('/word/media/logo.png');
  });

  it('writes every modelled section property', () => {
    const rt = roundTrip(
      `${P(R('x'))}<w:sectPr><w:footnotePr><w:numFmt w:val="lowerRoman"/><w:numRestart w:val="eachPage"/></w:footnotePr><w:endnotePr><w:pos w:val="sectEnd"/></w:endnotePr><w:type w:val="oddPage"/><w:pgSz w:w="11906" w:h="16838" w:code="9"/><w:pgMar w:top="1418" w:right="1134" w:bottom="1418" w:left="1134" w:header="709" w:footer="709" w:gutter="567"/><w:paperSrc w:first="7"/><w:pgBorders w:offsetFrom="page"><w:top w:val="single" w:sz="4" w:space="24" w:color="auto"/></w:pgBorders><w:lnNumType w:countBy="5" w:distance="360" w:restart="continuous"/><w:pgNumType w:fmt="lowerRoman" w:start="3"/><w:cols w:num="2" w:sep="1" w:space="708" w:equalWidth="0"><w:col w:w="3000" w:space="400"/><w:col w:w="5000"/></w:cols><w:formProt w:val="0"/><w:vAlign w:val="bottom"/><w:noEndnote/><w:textDirection w:val="lrTb"/><w:bidi/><w:rtlGutter/><w:docGrid w:type="lines" w:linePitch="312" w:charSpace="4"/></w:sectPr>`,
    );
    expectSameModel(rt);
    const sectPr = /<w:sectPr>(.*?)<\/w:sectPr>/.exec(rt.xml(MAIN))?.[1] ?? '';
    expectOrder(sectPr, [
      '<w:footnotePr',
      '<w:endnotePr',
      '<w:type',
      '<w:pgSz',
      '<w:pgMar',
      '<w:paperSrc',
      '<w:pgBorders',
      '<w:lnNumType',
      '<w:pgNumType',
      '<w:cols',
      '<w:formProt',
      '<w:vAlign',
      '<w:noEndnote',
      '<w:textDirection',
      '<w:bidi',
      '<w:rtlGutter',
      '<w:docGrid',
    ]);
  });
});

describe('styles, numbering and settings parts', () => {
  it('writes table styles with conditional formatting, aliases and latent styles', () => {
    const rt = roundTrip(P(R('x')), {
      styles:
        '<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi"/><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>' +
        '<w:latentStyles w:defLockedState="0" w:count="376"><w:lsdException w:name="Normal" w:qFormat="1"/></w:latentStyles>' +
        '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>' +
        '<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:link w:val="Heading1Char"/><w:uiPriority w:val="9"/><w:qFormat/><w:pPr><w:keepNext/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style>' +
        '<w:style w:type="table" w:styleId="Grid"><w:name w:val="Grid"/><w:uiPriority w:val="39"/><w:semiHidden/><w:unhideWhenUsed/><w:pPr><w:spacing w:after="0"/></w:pPr><w:tblPr><w:tblBorders><w:insideH w:val="single" w:sz="4"/></w:tblBorders><w:tblStyleRowBandSize w:val="1"/></w:tblPr><w:tcPr><w:vAlign w:val="center"/></w:tcPr><w:tblStylePr w:type="firstRow"><w:rPr><w:b/></w:rPr><w:tblPr/><w:tcPr><w:shd w:val="clear" w:fill="4472C4"/></w:tcPr></w:tblStylePr><w:tblStylePr w:type="band1Horz"><w:tcPr><w:shd w:val="clear" w:fill="D9E2F3"/></w:tcPr></w:tblStylePr></w:style>' +
        '<w:style w:type="character" w:customStyle="1" w:styleId="Em"><w:name w:val="Em"/><w:aliases w:val="E1,E2"/><w:hidden/><w:locked/><w:rPr><w:i/></w:rPr></w:style>',
    });
    expectSameModel(rt);
    const styles = rt.xml('/word/styles.xml');
    expectOrder(styles, [
      '<w:docDefaults><w:rPrDefault>',
      '<w:pPrDefault>',
      '<w:latentStyles',
      '<w:style w:type="paragraph" w:default="1" w:styleId="Normal">',
      'w:styleId="Heading1"',
      '<w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:link w:val="Heading1Char"/><w:uiPriority w:val="9"/><w:qFormat/><w:pPr>',
    ]);
    expectOrder(styles, [
      'w:styleId="Grid"',
      '<w:pPr>',
      '<w:tblPr>',
      '<w:tcPr>',
      '<w:tblStylePr w:type="band1Horz">',
      '<w:tblStylePr w:type="firstRow">',
    ]);
  });

  it('writes numbering definitions, overrides and picture bullets', () => {
    const rt = roundTrip(P(R('x'), '<w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr>'), {
      numbering:
        '<w:numPicBullet w:numPicBulletId="0"><w:pict><v:shape style="width:9pt;height:9pt"/></w:pict></w:numPicBullet>' +
        '<w:abstractNum w:abstractNumId="1"><w:nsid w:val="ABCD1234"/><w:multiLevelType w:val="multilevel"/><w:name w:val="Legal"/><w:styleLink w:val="MyList"/>' +
        '<w:lvl w:ilvl="0"><w:start w:val="3"/><w:numFmt w:val="decimal"/><w:pStyle w:val="Heading1"/><w:isLgl/><w:suff w:val="space"/><w:lvlText w:val="%1."/><w:lvlJc w:val="right"/><w:pPr><w:ind w:left="360" w:hanging="360"/></w:pPr><w:rPr><w:b/></w:rPr></w:lvl>' +
        '<w:lvl w:ilvl="1"><w:numFmt w:val="custom" w:format="001, 002, 003, ..."/><w:lvlRestart w:val="0"/><w:lvlText w:null="1"/><w:lvlPicBulletId w:val="0"/></w:lvl></w:abstractNum>' +
        '<w:num w:numId="1"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="5"/></w:lvlOverride><w:lvlOverride w:ilvl="1"><w:lvl w:ilvl="1"><w:start w:val="10"/><w:numFmt w:val="upperLetter"/><w:lvlText w:val="%2)"/></w:lvl></w:lvlOverride></w:num>',
    });
    expectSameModel(rt);
    const numbering = rt.xml('/word/numbering.xml');
    expectOrder(numbering, [
      '<w:numPicBullet',
      '<w:abstractNum w:abstractNumId="1"><w:nsid',
      '<w:multiLevelType',
      '<w:name',
      '<w:styleLink',
      '<w:lvl w:ilvl="0"><w:start w:val="3"/><w:numFmt w:val="decimal"/><w:pStyle w:val="Heading1"/><w:isLgl/><w:suff w:val="space"/><w:lvlText w:val="%1."/><w:lvlJc w:val="right"/><w:pPr>',
      '<w:num w:numId="1"><w:abstractNumId w:val="1"/><w:lvlOverride w:ilvl="0"><w:startOverride w:val="5"/>',
    ]);
  });

  it('patches modelled settings and keeps everything else in schema order', () => {
    const settings =
      '<w:zoom w:percent="120"/><w:proofState w:spelling="clean"/><w:defaultTabStop w:val="709"/><w:characterSpacingControl w:val="doNotCompress"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="14"/></w:compat><w:rsids><w:rsidRoot w:val="00AB12CD"/></w:rsids><w:decimalSymbol w:val=","/><w:listSeparator w:val=";"/>';
    const a = readDocx(buildDocx({ body: P(R('x')), settings }));
    const bytes = write({
      ...a,
      settings: {
        ...a.settings,
        trackRevisions: true,
        defaultTabStop: 720,
        evenAndOddHeaders: true,
        compatibilityMode: 15,
        protection: { edit: 'comments', enforced: true },
      },
    });
    const b = readDocx(bytes);
    expect(b.settings).toMatchObject({
      trackRevisions: true,
      defaultTabStop: 720,
      evenAndOddHeaders: true,
      compatibilityMode: 15,
      protection: { edit: 'comments', enforced: true },
      decimalSymbol: ',',
      listSeparator: ';',
    });
    const xml = OpcPackage.open(bytes).getPartText('/word/settings.xml') ?? '';
    expectOrder(xml, [
      '<w:zoom w:percent="120"/>',
      '<w:proofState',
      '<w:trackRevisions/>',
      '<w:documentProtection w:edit="comments" w:enforcement="1"/>',
      '<w:defaultTabStop w:val="720"/>',
      '<w:evenAndOddHeaders/>',
      '<w:characterSpacingControl',
      '<w:compat>',
      'w:val="15"',
      '<w:rsids>',
      '<w:decimalSymbol',
      '<w:listSeparator',
    ]);
  });

  it('keeps Word 2007 compatibility mode without adding a compatibility setting', () => {
    const rt = roundTrip(P(R('x')), { settings: '<w:compat><w:useFELayout/></w:compat>' });
    expectSameModel(rt);
    expect(rt.b.settings.compatibilityMode).toBe(12);
    expect(rt.xml('/word/settings.xml')).not.toContain('compatSetting');
  });

  it('writes the page background colour and drops it when removed', () => {
    const pkg = OpcPackage.open(buildDocx({ body: P(R('x')) }));
    pkg.setPart(
      MAIN,
      (pkg.getPartText(MAIN) ?? '').replace('<w:body>', '<w:background w:color="FFF2CC"/><w:body>'),
    );
    const rt = roundTripBytes(pkg.save());
    expectSameModel(rt);
    expect(rt.xml(MAIN)).toContain('<w:background w:color="FFF2CC"/><w:body>');
    const cleared = readDocx(write({ ...rt.a, settings: { ...rt.a.settings, background: undefined } }));
    expect(cleared.settings.background).toBeUndefined();
    const recoloured = readDocx(write({ ...rt.a, settings: { ...rt.a.settings, background: 'DEEAF6' } }));
    expect(recoloured.settings.background).toBe('DEEAF6');
  });
});
