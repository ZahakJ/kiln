/**
 * Structure assertions for each generated feature fixture (packages/testing/fixtures/docx/features),
 * plus corpus-wide guarantees: valid documents, no warnings for the synthetic corpus, and no unhandled
 * exceptions on the downloaded real-world files.
 */
import { readFileSync } from 'node:fs';
import { computeListLabels, toPlainText } from '@officina/doc-model';
import { fixtureLabel, listFixtures, loadFixture } from '@officina/testing';
import type { Node as PMNode } from '@tiptap/pm/model';
import { TableMap } from '@tiptap/pm/tables';
import { describe, expect, it } from 'vitest';
import { DocxFormatError, parseFragment, type ReadDocxResult, readDocx } from '../../src/index.ts';

const cache = new Map<string, ReadDocxResult>();
function read(name: string): ReadDocxResult {
  let r = cache.get(name);
  if (!r) {
    r = readDocx(loadFixture(name));
    cache.set(name, r);
  }
  return r;
}

function all(doc: PMNode, type: string): PMNode[] {
  const out: PMNode[] = [];
  doc.descendants((n) => {
    if (n.type.name === type) out.push(n);
  });
  return out;
}

function paragraphWith(doc: PMNode, text: string): PMNode {
  const p = all(doc, 'paragraph').find((n) => n.textContent.includes(text));
  if (!p) throw new Error(`no paragraph containing ${text}`);
  return p;
}

function marksOn(doc: PMNode, text: string) {
  let found: PMNode | undefined;
  doc.descendants((n) => {
    if (!found && n.isText && n.text?.includes(text)) found = n;
  });
  if (!found) throw new Error(`no text ${text}`);
  return Object.fromEntries(found.marks.map((m) => [m.type.name, m.attrs]));
}

describe('corpus', () => {
  const features = listFixtures('docx', 'features');
  for (const path of features) {
    it(`${fixtureLabel(path)} reads into a valid document without warnings`, () => {
      const r = readDocx(new Uint8Array(readFileSync(path)));
      expect(() => r.doc.check()).not.toThrow();
      expect(r.warnings).toEqual([]);
      expect(r.sections.length).toBeGreaterThan(0);
    });
  }

  const external = listFixtures('docx', 'external');
  it.runIf(external.length > 0)('reads every external fixture without unhandled exceptions', () => {
    for (const path of external) {
      let result: ReadDocxResult | undefined;
      try {
        result = readDocx(new Uint8Array(readFileSync(path)));
      } catch (e) {
        // A file we cannot open must fail with the reader's typed error, never a crash.
        expect(e, fixtureLabel(path)).toBeInstanceOf(DocxFormatError);
        continue;
      }
      expect(() => result?.doc.check(), fixtureLabel(path)).not.toThrow();
      for (const w of result.warnings) {
        expect(w.part, fixtureLabel(path)).toMatch(/^\//);
        expect(w.path, fixtureLabel(path)).toMatch(/^\//);
      }
    }
  });

  it('reads the hand-made fixtures', () => {
    for (const path of listFixtures('docx', 'manual')) {
      const r = readDocx(new Uint8Array(readFileSync(path)));
      expect(() => r.doc.check()).not.toThrow();
    }
  });
});

describe('numbering.docx', () => {
  const r = () => read('numbering.docx');
  const labels = () => {
    const { doc, styles, numbering } = r();
    const map = computeListLabels(doc, { styles, numbering });
    return new Map([...map.values()].map((l) => [doc.nodeAt(l.pos)?.textContent ?? '', l.text]));
  };

  it('reads abstract definitions and instances', () => {
    expect(r().numbering.instances.size).toBeGreaterThanOrEqual(6);
    const restart = r().numbering.instances.get(60);
    expect(restart?.overrides).toEqual([{ level: 0, startOverride: 1 }]);
  });

  it('labels decimal lists 1. 2. 3. with nested levels restarting', () => {
    const l = labels();
    expect(l.get('decimal item 1')).toBe('1.');
    expect(l.get('decimal item 2')).toBe('2.');
    expect(l.get('decimal item 3')).toBe('3.');
    expect(l.get('decimal item 2.1')).toBe('1.');
  });

  it('labels letter and roman lists', () => {
    const l = labels();
    expect(l.get('lowerLetter item 1')).toBe('a)');
    expect(l.get('lowerLetter item 3')).toBe('c)');
    expect(l.get('upperRoman item 2')).toBe('II.');
    expect(l.get('upperRoman item 3')).toBe('III.');
  });

  it('labels legal numbering 1.1.1 and bullets', () => {
    const l = labels();
    expect(l.get('legal item 2.1')).toBe('2.1');
    expect(l.get('legal level 3')).toBe('3.1.1');
    expect(l.get('bullet item 1')).toBe('•');
    expect(l.get('bullet item 1.1')).toBe('o');
    expect(l.get('bullet level 3')).toBe('▪');
  });

  it('continues and restarts lists as Word does', () => {
    const l = labels();
    expect(l.get('Continues the decimal list')).toBe('4.');
    expect(l.get('Restarts at one')).toBe('1.');
    expect(l.get('Then two')).toBe('2.');
  });

  it('keeps level indents and fonts from numbering.xml', () => {
    const abs = r().numbering.abstracts.get(40);
    expect(abs?.multiLevelType).toBe('hybridMultilevel');
    expect(abs?.levels[0]?.pPr?.indent).toEqual({ left: 720, hanging: 360 });
    expect(abs?.levels[0]?.rPr?.fonts?.ascii).toBe('Symbol');
    expect(r().numbering.abstracts.get(44)?.levels[2]?.text).toBe('%1.%2.%3');
  });
});

describe('tables.docx', () => {
  const tables = () => all(read('tables.docx').doc, 'table');

  it('reads a 4x4 grid table with its style and grid', () => {
    const t = tables()[0] as PMNode;
    expect(t.attrs.styleId).toBe('TableGrid');
    expect(t.childCount).toBe(4);
    const map = TableMap.get(t);
    expect([map.width, map.height]).toEqual([4, 4]);
    expect(t.child(0).child(0).textContent).toBe('Header 1');
    expect(t.child(3).child(3).textContent).toBe('R4C4');
  });

  it('turns gridSpan and vMerge into colspan and rowspan', () => {
    const t = tables()[1] as PMNode;
    const map = TableMap.get(t);
    expect([map.width, map.height]).toEqual([4, 5]);
    expect(map.problems).toBeNull();
    const merged = t.child(1).child(0);
    expect(merged.attrs.colspan).toBe(3);
    expect(merged.textContent).toBe('Merged across three columns');
    const down = all(t, 'tableCell').find((c) => c.textContent === 'Merged down three rows');
    expect(down?.attrs.rowspan).toBe(3);
    expect(t.attrs.alignment).toBe('center');
    expect(t.attrs.look).toMatchObject({ firstRow: true, noVBand: true });
  });

  it('reads cell shading, widths in twips with px colwidths, and nested tables', () => {
    const t = tables()[2] as PMNode;
    const first = t.child(0).child(0);
    expect(first.attrs.shading.fill.val).toBe('FFF2CC');
    expect(first.attrs.width).toEqual({ type: 'dxa', value: 1701 });
    expect(first.attrs.colwidth).toEqual([Math.round(t.attrs.grid[0] / 15)]);
    const nested = all(t, 'table');
    expect(nested).toHaveLength(1);
    expect(TableMap.get(nested[0] as PMNode).width).toBe(2);
  });
});

describe('sections.docx', () => {
  const r = () => read('sections.docx');

  it('reads four sections with break types, sizes and orientation', () => {
    const s = r().sections;
    expect(s).toHaveLength(4);
    expect(s.map((x) => x.type)).toEqual(['nextPage', 'nextPage', 'continuous', 'oddPage']);
    expect(s[0]?.pageSize).toMatchObject({ width: 11906, height: 16838, orientation: 'portrait' });
    expect(s[1]?.pageSize.orientation).toBe('landscape');
    expect(s[1]?.margins.left).toBe(850);
    expect(s[2]?.columns).toMatchObject({ count: 2, space: 720, separator: true });
    expect(s[3]?.margins.gutter).toBe(567);
  });

  it('maps header references to header nodes by type with titlePg and even/odd settings', () => {
    const { doc, settings } = r();
    const first = doc.child(0);
    const headers = all(first, 'header').map((h) => [h.attrs.type, h.textContent]);
    expect(headers).toEqual(
      expect.arrayContaining([
        ['default', 'Default header (section 1)'],
        ['first', 'First-page header'],
        ['even', 'Even-page header'],
      ]),
    );
    expect(r().sections[0]?.titlePage).toBe(true);
    expect(settings.evenAndOddHeaders).toBe(true);
    expect(all(doc.child(1), 'header').map((h) => h.textContent)).toContain('Landscape section header');
  });
});

describe('headers-footers.docx', () => {
  it('reads header text with tabs and PAGE / NUMPAGES fields in the footer', () => {
    const { doc } = read('headers-footers.docx');
    const header = all(doc, 'header')[0] as PMNode;
    expect(toPlainText(header.child(0))).toBe('Officina fixtures\tHeaders and footers\tConfidential');
    const footer = all(doc, 'footer')[0] as PMNode;
    const fields = all(footer, 'field').map((f) => [f.attrs.type, f.attrs.instr, f.attrs.result]);
    expect(fields).toEqual([
      ['PAGE', 'PAGE', '1'],
      ['NUMPAGES', 'NUMPAGES', '3'],
    ]);
    expect(footer.child(0).attrs.alignment).toBe('center');
  });
});

describe('images', () => {
  it('extracts inline images with media, sizes and content types', () => {
    const r = read('images-inline.docx');
    const images = all(r.doc, 'image');
    expect(images).toHaveLength(4);
    expect(images[0]?.attrs).toMatchObject({ placement: 'inline', width: 2880000 });
    for (const img of images) {
      expect(r.media.has(img.attrs.mediaId as string)).toBe(true);
      expect(img.attrs.height).toBeGreaterThan(0);
    }
    const types = images.map((i) => r.media.get(i.attrs.mediaId as string)?.contentType);
    expect(types).toContain('image/png');
    expect(types).toContain('image/jpeg');
    const png = r.media.get(images[0]?.attrs.mediaId as string)?.data;
    expect([...(png?.slice(0, 4) ?? [])]).toEqual([0x89, 0x50, 0x4e, 0x47]);
    expect(r.passthrough.has(images[0]?.attrs.mediaId as string)).toBe(false);
  });

  it('reads anchored images with every wrap mode and position', () => {
    const images = all(read('images-anchored.docx').doc, 'image');
    expect(images.map((i) => i.attrs.wrap)).toEqual([
      'square',
      'tight',
      'through',
      'topAndBottom',
      'behind',
      'inFront',
    ]);
    const second = images[1]?.attrs;
    expect(second?.placement).toBe('anchor');
    expect(second?.position).toMatchObject({
      relativeFromH: 'column',
      relativeFromV: 'paragraph',
      h: { offset: 914400 },
      v: { offset: 0 },
      distL: 114300,
      allowOverlap: true,
      layoutInCell: true,
      zIndex: 251659264 + 101,
    });
    expect(second?.width).toBe(1828800);
  });
});

describe('text-boxes.docx', () => {
  it('reads wps shapes with preset geometry, DrawingML fill and line, keeping the XML', () => {
    const shapes = all(read('text-boxes.docx').doc, 'shape');
    expect(shapes.map((s) => s.attrs.preset)).toEqual(['rect', 'roundRect', 'ellipse']);
    const s = shapes[0]?.attrs;
    expect(s?.fill).toMatchObject({ type: 'solid' });
    expect(JSON.stringify(s?.fill)).toContain('DEEBF7');
    expect(s?.line?.width).toBe(12700);
    expect([s?.width, s?.height]).toEqual([2286000, 914400]);
    expect(s?.xml).toContain('Text box 1: rect');
    const parsed = parseFragment(s?.xml);
    expect(parsed.name).toBe('w:drawing');
  });
});

describe('footnotes.docx', () => {
  it('reads footnotes and endnotes with references in the text', () => {
    const r = read('footnotes.docx');
    expect(r.footnotes.map((n) => n.id)).toEqual(['1', '2', '3']);
    expect(r.endnotes.map((n) => n.id)).toEqual(['1', '2']);
    expect(r.footnotes[0]?.node.textContent).toMatch(/^ Footnote 1: /);
    const refs = all(r.doc.child(0), 'footnoteRef').map((n) => `${n.attrs.kind}:${n.attrs.id}`);
    expect(refs).toEqual(['footnote:1', 'footnote:2', 'footnote:3', 'endnote:1', 'endnote:2']);
    const notes = r.doc.content.content.filter((n) => n.type.name === 'footnote');
    expect(notes).toHaveLength(5);
    expect(all(notes[0] as PMNode, 'footnoteRef')[0]?.attrs).toMatchObject({ id: '1', kind: 'footnote' });
  });
});

describe('fields.docx', () => {
  const r = () => read('fields.docx');

  it('turns a TOC field spanning paragraphs into a toc block', () => {
    const toc = all(r().doc, 'toc');
    expect(toc).toHaveLength(1);
    expect(toc[0]?.attrs.instr).toBe('TOC \\o "1-3" \\h \\z \\u');
    expect(toc[0]?.childCount).toBe(3);
    expect(toc[0]?.child(0).textContent).toBe('Chapter One\t2');
  });

  it('reads complex fields with cached results and simple fields', () => {
    const fields = all(r().doc, 'field').map((f) => [f.attrs.type, f.attrs.instr, f.attrs.result]);
    expect(fields).toEqual([
      ['PAGE', 'PAGE', '1'],
      ['NUMPAGES', 'NUMPAGES', '1'],
      ['DATE', 'DATE \\@ "yyyy-MM-dd"', '2026-01-15'],
      ['REF', 'REF chapter_one \\h', 'Chapter One'],
      ['AUTHOR', 'AUTHOR', 'Officina Fixtures'],
    ]);
  });

  it('keeps bookmarks around the referenced heading', () => {
    const heading = all(r().doc, 'paragraph').find(
      (p) => p.attrs.styleId === 'Heading1' && p.textContent === 'Chapter One',
    ) as PMNode;
    const inline = heading.content.content.map((n) => n.type.name);
    expect(inline).toEqual(['bookmarkStart', 'text', 'bookmarkEnd']);
    expect(heading.child(0).attrs).toEqual({ id: '0', name: 'chapter_one' });
  });
});

describe('comments.docx', () => {
  it('reads comments with authors, dates and initials, and marks their ranges', () => {
    const r = read('comments.docx');
    expect(r.comments).toHaveLength(3);
    expect(r.comments[0]).toMatchObject({
      id: '0',
      author: 'Reviewer 1',
      initials: 'R1',
      date: '2026-01-11T10:00:00Z',
    });
    expect(r.comments[1]?.text).toMatch(/^Comment number 2: /);
    expect(marksOn(r.doc, 'commented text 2').comment).toEqual({ id: '1' });
    expect(Array.isArray(r.comments[0]?.content)).toBe(true);
  });
});

describe('tracked-changes.docx', () => {
  const r = () => read('tracked-changes.docx');

  it('reads w:ins and w:del as insertion and deletion marks with author and date', () => {
    expect(marksOn(r().doc, 'Inserted words.').insertion).toEqual({
      id: '1',
      author: 'Editor',
      date: '2026-01-15T09:30:00Z',
    });
    expect(marksOn(r().doc, 'Deleted words.').deletion).toEqual({
      id: '2',
      author: 'Editor',
      date: '2026-01-15T09:30:00Z',
    });
    expect(marksOn(r().doc, 'Unchanged end.').insertion).toBeUndefined();
  });

  it('reads w:rPrChange as a formatChange mark and w:pPrChange as a paragraph revision', () => {
    const fc = marksOn(r().doc, 'made bold').formatChange;
    expect(fc).toMatchObject({ id: '3', author: 'Editor', before: {} });
    expect(marksOn(r().doc, 'made bold').bold).toEqual({ value: true });
    const p = paragraphWith(r().doc, 'Paragraph property change');
    expect(p.attrs.alignment).toBe('center');
    expect(p.attrs.revision.pPrChange).toMatchObject({ id: '4', author: 'Editor', before: {} });
    expect(r().settings.trackRevisions).toBe(true);
  });
});

describe('rtl.docx', () => {
  it('reads bidi paragraphs and rtl runs', () => {
    const p = paragraphWith(read('rtl.docx').doc, 'هذه فقرة');
    expect(p.attrs.bidi).toBe(true);
    expect(p.attrs.alignment).toBe('right');
    const m = marksOn(read('rtl.docx').doc, 'זוהי');
    expect(m.runProps?.props.rtl).toBe(true);
    expect(m.fontFamily?.fonts.cs).toBe('Arial');
    expect(paragraphWith(read('rtl.docx').doc, 'left-to-right').attrs.bidi).toBeNull();
  });
});

describe('hyperlinks.docx', () => {
  it('reads external links from relationships and internal anchors', () => {
    const r = read('hyperlinks.docx');
    expect(marksOn(r.doc, 'https://example.org/').link).toMatchObject({ href: 'https://example.org/' });
    expect(marksOn(r.doc, 'someone@example.org').link.href).toBe('mailto:someone@example.org');
    expect(marksOn(r.doc, 'jump to target').link).toMatchObject({ href: null, anchor: 'target' });
    expect(all(r.doc, 'bookmarkStart').map((b) => b.attrs.name)).toContain('target');
    // Modelled hyperlink relationships are not passthrough.
    expect(r.passthrough.relationships.some((p) => p.rel.type.endsWith('/hyperlink'))).toBe(false);
  });
});

describe('page-setup.docx', () => {
  it('reads page size, margins, line numbering, page numbering and page borders', () => {
    const s = read('page-setup.docx').sections[0];
    expect(s?.pageSize).toMatchObject({ width: 12240, height: 20160 });
    expect(s?.margins).toMatchObject({ left: 1803, right: 1803, header: 709, footer: 709 });
    expect(s?.lineNumbering).toEqual({ countBy: 5, restart: 'newPage' });
    expect(s?.pageNumbering).toEqual({ start: 3, format: 'lowerRoman' });
    expect(s?.pageBorders).toMatchObject({
      offsetFrom: 'page',
      top: { style: 'single', size: 12, space: 24, color: { val: '4472C4' } },
    });
  });
});

describe('breaks.docx', () => {
  const r = () => read('breaks.docx');

  it('reads line breaks, page breaks and tabs', () => {
    expect(all(paragraphWith(r().doc, 'Line one'), 'softBreak')).toHaveLength(1);
    expect(all(r().doc, 'pageBreak')).toHaveLength(1);
    expect(all(paragraphWith(r().doc, 'Tabs:'), 'tab')).toHaveLength(3);
  });

  it('maps w:sym symbol-font characters to Unicode and keeps the original', () => {
    const p = paragraphWith(r().doc, 'Symbol characters');
    expect(p.textContent).toBe('Symbol characters: ☺α');
    expect(marksOn(r().doc, '☺').runProps?.props.sym).toEqual({ font: 'Wingdings', char: 'F04A' });
  });

  it('keeps special characters verbatim', () => {
    expect(paragraphWith(r().doc, 'Special characters').textContent).toContain('soft­hyphen');
  });
});

describe('styles.docx', () => {
  const r = () => read('styles.docx');

  it('reads the style sheet with basedOn chains and docDefaults', () => {
    const { styles } = r();
    expect(styles.get('Heading1')?.basedOn).toBe('Normal');
    expect(styles.chain('Heading2').map((s) => s.id)).toEqual(['Normal', 'Heading2']);
    expect(styles.defaultStyle('paragraph')?.id).toBe('Normal');
    expect(styles.docDefaults.rPr.fonts?.asciiTheme).toBe('minorHAnsi');
    expect(styles.get('Title')?.quickStyle).toBe(true);
  });

  it('reads character formatting into marks', () => {
    const d = r().doc;
    expect(marksOn(d, 'bold italic')).toMatchObject({ bold: { value: true }, italic: { value: true } });
    expect(marksOn(d, 'double strike').dstrike).toEqual({ value: true });
    expect(marksOn(d, 'small caps').smallCaps).toEqual({ value: true });
    expect(marksOn(d, 'superscript').vertAlign).toEqual({ value: 'superscript' });
    expect(marksOn(d, 'underline double').underline.style).toBe('double');
    expect(marksOn(d, 'underline wavy').underline.style).toBe('wave');
    expect(marksOn(d, '36pt').fontSize.size).toBe(72);
    expect(marksOn(d, 'in Georgia').fontFamily.fonts.ascii).toBe('Georgia');
    expect(marksOn(d, '#C00000').color.color).toEqual({ val: 'C00000' });
    expect(marksOn(d, 'theme accent_1').color.color).toMatchObject({ themeColor: 'accent1' });
    expect(marksOn(d, 'yellow').highlight.color).toBe('yellow');
    expect(marksOn(d, 'spacing 60').charSpacing.value).toBe(60);
    expect(marksOn(d, 'hidden').hidden).toEqual({ value: true });
  });
});

describe('paragraphs.docx', () => {
  const d = () => read('paragraphs.docx').doc;

  it('reads alignment, indents and spacing in twips', () => {
    expect(paragraphWith(d(), '[CENTER]').attrs.alignment).toBe('center');
    expect(paragraphWith(d(), '[JUSTIFY]').attrs.alignment).toBe('both');
    expect(paragraphWith(d(), '[left indent 2 cm]').attrs.indent).toEqual({ left: 1134 });
    expect(paragraphWith(d(), '[hanging 1.27 cm]').attrs.indent).toEqual({ left: 720, hanging: 720 });
    expect(paragraphWith(d(), '[space before 18').attrs.spacing).toMatchObject({ before: 360, after: 120 });
  });

  it('reads line spacing rules', () => {
    expect(paragraphWith(d(), 'line spacing double').attrs.spacing).toMatchObject({
      line: 480,
      lineRule: 'auto',
    });
    expect(paragraphWith(d(), 'line spacing exactly').attrs.spacing).toMatchObject({
      line: 400,
      lineRule: 'exact',
    });
    expect(paragraphWith(d(), 'line spacing at least').attrs.spacing).toMatchObject({
      line: 480,
      lineRule: 'atLeast',
    });
  });

  it('reads keep options, borders, shading, tab stops and page break before', () => {
    expect(paragraphWith(d(), 'Keep with next').attrs.keepNext).toBe(true);
    expect(paragraphWith(d(), 'Widow control off').attrs.widowControl).toBe(false);
    const boxed = paragraphWith(d(), 'Boxed paragraph').attrs;
    expect(boxed.borders.bottom).toMatchObject({ style: 'double', size: 6 });
    expect(boxed.shading.fill.val).toBe('DEEAF6');
    const tabs = paragraphWith(d(), 'Left').attrs.tabs;
    expect(tabs.map((t: { align: string }) => t.align)).toEqual(['center', 'right', 'decimal']);
    expect(tabs[0].leader).toBe('dot');
    expect(paragraphWith(d(), 'Page break before').attrs.pageBreakBefore).toBe(true);
  });
});

describe('properties.docx', () => {
  it('reads core and custom document properties', () => {
    const p = read('properties.docx').properties;
    expect(p.core).toMatchObject({
      title: 'Document properties',
      subject: 'Fixture subject',
      creator: 'Officina Fixtures',
      category: 'Testing',
    });
    expect(p.custom.map((c) => c.name)).toEqual(
      expect.arrayContaining(['Project', 'Build', 'Ratio', 'Reviewed']),
    );
    expect(p.custom.find((c) => c.name === 'Build')?.value).toEqual({ type: 'i4', value: 42 });
  });
});

describe('long-text.docx', () => {
  it('reads every heading of a long document', () => {
    const d = read('long-text.docx').doc;
    const headings = all(d, 'paragraph').filter((p) => p.attrs.styleId === 'Heading2');
    expect(headings).toHaveLength(15);
    expect(all(d, 'paragraph').filter((p) => p.attrs.styleId === 'Heading1')).toHaveLength(5);
  });
});

describe('passthrough', () => {
  it('keeps unmodelled parts (custom XML, font table, web settings, thumbnail)', () => {
    const r = read('styles.docx');
    const names = r.passthrough.parts.map((p) => p.name);
    expect(names).toEqual(
      expect.arrayContaining(['/customXml/item1.xml', '/word/fontTable.xml', '/word/webSettings.xml']),
    );
    expect(names).not.toContain('/word/document.xml');
    expect(names).not.toContain('/word/styles.xml');
    expect(names).not.toContain('/docProps/core.xml');
  });
});
