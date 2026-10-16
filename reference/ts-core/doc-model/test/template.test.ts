import { describe, expect, it } from 'vitest';
import { documentToJSON, parseDocument, serializeDocument, stableValue } from '../src/serialize.ts';
import { resolveParagraphProps, resolveRunProps } from '../src/styles/resolve.ts';
import { StyleSheet, StyleSheetError, styleDisplayName, styleIdFromName } from '../src/styles/stylesheet.ts';
import { createDefaultStyleSheet, DefaultTemplate } from '../src/template.ts';
import { lineUnitsToMultiple } from '../src/units.ts';

describe('DefaultTemplate (Word 2016+ Normal.dotm)', () => {
  const d = DefaultTemplate.create();
  const ctx = { styles: d.styles, numbering: d.numbering, theme: d.theme };

  it('creates one Letter section with one empty Normal paragraph', () => {
    expect(d.doc.childCount).toBe(1);
    const section = d.doc.child(0);
    expect(section.attrs.props.pageSize).toEqual({ width: 12240, height: 15840, orientation: 'portrait' });
    expect(section.attrs.props.margins).toMatchObject({ top: 1440, left: 1440, header: 720, footer: 720 });
    expect(section.childCount).toBe(1);
    expect(section.child(0).type.name).toBe('paragraph');
    expect(() => d.doc.check()).not.toThrow();
  });

  it('uses A4 outside letter regions', () => {
    expect(DefaultTemplate.create({ locale: 'de-DE' }).doc.child(0).attrs.props.pageSize.width).toBe(11906);
    expect(DefaultTemplate.create({ locale: 'en-GB' }).doc.child(0).attrs.props.pageSize.height).toBe(16838);
    expect(DefaultTemplate.create({ locale: 'en-CA' }).doc.child(0).attrs.props.pageSize.width).toBe(12240);
    expect(DefaultTemplate.create({ locale: 'de-DE' }).styles.docDefaults.rPr.lang?.val).toBe('de-DE');
  });

  it('Normal: Calibri 11 pt, 1.08 lines (259 auto), 8 pt after', () => {
    const para = resolveParagraphProps(ctx, {});
    expect(para.spacing).toMatchObject({ after: 160, line: 259, lineRule: 'auto', before: 0 });
    expect(lineUnitsToMultiple(para.spacing.line)).toBeCloseTo(1.08, 2);
    const run = resolveRunProps(ctx, { paragraph: {} });
    expect(run.size).toBe(22);
    expect(run.resolvedFonts.ascii).toBe('Calibri');
    expect(run.colorHex).toBe('auto');
  });

  it.each([
    ['Heading1', 32, '2F5496', 240, 0],
    ['Heading2', 26, '2F5496', 40, 1],
    ['Heading3', 24, '1F3763', 40, 2],
  ])(
    '%s: Calibri Light %i half-points, colour %s from the theme, before %i, outline %i',
    (id, size, color, before, outline) => {
      const para = resolveParagraphProps(ctx, { styleId: id });
      const run = resolveRunProps(ctx, { paragraph: { styleId: id } });
      expect(run.size).toBe(size);
      expect(run.colorHex).toBe(color);
      expect(run.resolvedFonts.ascii).toBe('Calibri Light');
      expect(para.spacing.before).toBe(before);
      expect(para.spacing.after).toBe(0);
      expect(para.outlineLevel).toBe(outline);
      expect(para.keepNext && para.keepLines).toBe(true);
      expect(d.styles.nextStyleId(id)).toBe('Normal');
    },
  );

  it('Title, Subtitle, Quote, List Paragraph, Caption', () => {
    expect(resolveRunProps(ctx, { paragraph: { styleId: 'Title' } })).toMatchObject({
      size: 56,
      kern: 28,
      spacing: -10,
    });
    expect(resolveRunProps(ctx, { paragraph: { styleId: 'Subtitle' } }).colorHex).toBe('5A5A5A');
    const quote = resolveParagraphProps(ctx, { styleId: 'Quote' });
    expect([quote.alignment, quote.indent.left, quote.indent.right]).toEqual(['center', 864, 864]);
    expect(resolveRunProps(ctx, { paragraph: { styleId: 'Quote' } }).italic).toBe(true);
    expect(resolveParagraphProps(ctx, { styleId: 'ListParagraph' })).toMatchObject({
      contextualSpacing: true,
      indent: { left: 720 },
    });
    expect(resolveRunProps(ctx, { paragraph: { styleId: 'Caption' } })).toMatchObject({
      size: 18,
      italic: true,
      colorHex: '44546A',
    });
  });

  it('Table Grid, TOC 1–3, Footnote Text, Header, Footer', () => {
    expect(d.styles.get('TableGrid')?.tblPr?.borders?.insideV?.style).toBe('single');
    expect([1, 2, 3].map((n) => resolveParagraphProps(ctx, { styleId: `TOC${n}` }).indent.left)).toEqual([
      0, 220, 440,
    ]);
    expect(resolveRunProps(ctx, { paragraph: { styleId: 'FootnoteText' } }).size).toBe(20);
    expect(resolveParagraphProps(ctx, { styleId: 'Header' }).tabs.map((t) => t.pos)).toEqual([4680, 9360]);
    expect(resolveParagraphProps(ctx, { styleId: 'Footer' }).spacing.after).toBe(0);
  });

  it('Hyperlink, Strong, Emphasis character styles', () => {
    const run = (styleId: string) => resolveRunProps(ctx, { paragraph: {}, props: { styleId } });
    expect(run('Hyperlink')).toMatchObject({ colorHex: '0563C1', underline: { style: 'single' } });
    expect(run('Strong').bold).toBe(true);
    expect(run('Emphasis').italic).toBe(true);
    expect(
      resolveRunProps(ctx, { paragraph: { styleId: 'Quote' }, props: { styleId: 'Emphasis' } }).italic,
    ).toBe(false);
  });

  it('quick styles gallery is ordered like Word', () => {
    const names = d.styles.quickStyles().map((s) => styleDisplayName(s.name));
    expect(names.slice(0, 6)).toEqual([
      'Normal',
      'No Spacing',
      'Heading 1',
      'Heading 2',
      'Heading 3',
      'Heading 4',
    ]);
    expect(names).toContain('Title');
    expect(names).toContain('Strong');
  });

  it('default style sheet snapshot', () => {
    expect(stableValue(createDefaultStyleSheet().toJSON())).toMatchSnapshot();
  });

  it('new document JSON snapshot', () => {
    const json = documentToJSON(DefaultTemplate.create());
    expect(
      stableValue({ doc: json.doc, settings: json.settings, numbering: json.numbering }),
    ).toMatchSnapshot();
  });
});

describe('StyleSheet', () => {
  it('display names and ids', () => {
    expect(styleDisplayName('heading 1')).toBe('Heading 1');
    expect(styleDisplayName('toc 2')).toBe('TOC 2');
    expect(styleDisplayName('footnote text')).toBe('Footnote Text');
    expect(styleDisplayName('My Style')).toBe('My Style');
    expect(styleIdFromName('heading 1')).toBe('Heading1');
    expect(styleIdFromName('List Paragraph')).toBe('ListParagraph');
  });

  it('finds styles by stored name, display name and alias', () => {
    const s = createDefaultStyleSheet();
    expect(s.getByName('Heading 1')?.id).toBe('Heading1');
    expect(s.getByName('heading 1')?.id).toBe('Heading1');
    s.update('Quote', { aliases: ['Citation'] });
    expect(s.getByName('citation')?.id).toBe('Quote');
  });

  it('rejects basedOn cycles on update and re-parents on remove', () => {
    const s = createDefaultStyleSheet();
    expect(() => s.update('Normal', { basedOn: 'Heading1' })).toThrow(StyleSheetError);
    s.add({ id: 'Child', type: 'paragraph', name: 'Child', basedOn: 'Heading1', next: 'Heading1' });
    s.remove('Heading1');
    expect(s.get('Child')?.basedOn).toBe('Normal');
    expect(s.get('Child')?.next).toBeUndefined();
  });

  it('only one default per type', () => {
    const s = createDefaultStyleSheet();
    s.add({ id: 'Body', type: 'paragraph', name: 'Body', isDefault: true });
    expect(s.defaultStyle('paragraph')?.id).toBe('Body');
    expect(s.get('Normal')?.isDefault).toBe(false);
  });

  it('chains stop at a parent of a different type', () => {
    const s = new StyleSheet(undefined, [
      { id: 'C', type: 'character', name: 'C' },
      { id: 'P', type: 'paragraph', name: 'P', basedOn: 'C' },
    ]);
    expect(s.chain('P').map((x) => x.id)).toEqual(['P']);
  });

  it('linked styles and unique ids', () => {
    const s = createDefaultStyleSheet();
    expect(s.linked('Heading1')?.id).toBe('Heading1Char');
    expect(s.linked('Heading1Char')?.id).toBe('Heading1');
    expect(s.uniqueId('Heading 1')).toBe('Heading11');
    expect(s.uniqueId('Fresh Style')).toBe('FreshStyle');
  });

  it('clone is deep', () => {
    const s = createDefaultStyleSheet();
    const c = s.clone();
    c.update('Normal', { rPr: { size: 30 } });
    expect(s.get('Normal')?.rPr).toBeUndefined();
  });
});

describe('JSON serialisation', () => {
  it('round-trips a document with stable output', () => {
    const d = DefaultTemplate.create();
    d.numbering.createBulletList();
    d.comments.push({ id: '1', author: 'A', date: '2026-10-15T00:00:00Z', text: 'Hi' });
    const text = serializeDocument(d);
    const back = parseDocument(text);
    expect(back.doc.eq(d.doc)).toBe(true);
    expect(serializeDocument(back)).toBe(text);
    expect(back.numbering.level(1, 0)?.format).toBe('bullet');
  });

  it('sorts keys for diff-friendly output', () => {
    expect(JSON.stringify(stableValue({ b: 1, a: { d: 2, c: [{ z: 1, y: 2 }] } }))).toBe(
      '{"a":{"c":[{"y":2,"z":1}],"d":2},"b":1}',
    );
  });

  it('rejects foreign JSON', () => {
    expect(() => parseDocument('{"format":"other"}')).toThrow(/not an Kiln/);
    expect(() => parseDocument('{"format":"kiln-write","version":9}')).toThrow(/unsupported version/);
  });
});
