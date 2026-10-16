import { officeTheme } from '@kiln-ref/ooxml';
import { describe, expect, it } from 'vitest';
import {
  fontStack,
  fontsWithLayoutRisk,
  genericFamilyFor,
  isSymbolFont,
  substituteFont,
} from '../src/fonts/substitution.ts';
import {
  breakBehaviour,
  columnWidths,
  contentWidth,
  defaultPaperForLocale,
  defaultSectionProps,
  headerFooterKindForPage,
  paperNameOf,
  resolveHeaderFooterRef,
  sectionStartPageNumbers,
  withOrientation,
} from '../src/sections.ts';
import { fromPlainText, toPlainText } from '../src/serialize.ts';
import { countWords, countWordsInText, statistics } from '../src/stats.ts';
import {
  applyTintShade,
  resolveDocColor,
  resolveRunFonts,
  resolveShadingFill,
  themeSlotFor,
} from '../src/theme.ts';
import type { SectionProps } from '../src/types.ts';
import {
  cmToTwips,
  cssLineHeight,
  formatLength,
  halfPointsToPx,
  lineMultipleToUnits,
  twipsToCm,
  twipsToPt,
  twipsToPx,
} from '../src/units.ts';
import { cell, doc, p, row, schema, table, text } from './helpers.ts';

describe('font substitution (§1.2)', () => {
  it.each([
    ['Calibri', 'Carlito', true],
    ['Cambria', 'Caladea', true],
    ['Arial', 'Liberation Sans', true],
    ['Times New Roman', 'Liberation Serif', true],
    ['Courier New', 'Liberation Mono', true],
    ['Aptos', 'Inter Variable', false],
    ['calibri light', 'Carlito', false],
  ])('%s → %s (metric-compatible: %s)', (office, sub, metric) => {
    const s = substituteFont(office);
    expect(s?.substitute).toBe(sub);
    expect(s?.metricCompatible).toBe(metric);
  });

  it('fontStack prefers the bundled metric-compatible font, then the original, then a generic', () => {
    expect(fontStack('Calibri')).toBe('Carlito, Calibri, sans-serif');
    expect(fontStack('Times New Roman')).toBe('"Liberation Serif", "Times New Roman", serif');
    expect(fontStack('Calibri', { preferInstalled: true })).toBe('Calibri, Carlito, sans-serif');
    expect(fontStack('Fira Code')).toBe('"Fira Code", monospace');
    expect(fontStack('Noto Serif JP')).toBe('"Noto Serif JP", serif');
    expect(fontStack('')).toBe('sans-serif');
  });

  it('classifies unknown fonts and symbol fonts', () => {
    expect(genericFamilyFor('Some Gothic')).toBe('sans-serif');
    expect(genericFamilyFor('Baskerville Old Face')).toBe('serif');
    expect(isSymbolFont('Wingdings')).toBe(true);
    expect(isSymbolFont('Arial')).toBe(false);
  });

  it('lists fonts with layout risk for the Compatibility Checker', () => {
    expect(
      fontsWithLayoutRisk(['Calibri', 'Aptos', 'Symbol', 'Segoe UI', 'Carlito', 'Arial', 'Aptos']),
    ).toEqual(['Aptos', 'Segoe UI']);
  });
});

describe('theme resolution', () => {
  const theme = officeTheme();

  it('maps w:themeColor through the colour scheme mapping', () => {
    expect(themeSlotFor('text1')).toBe('dk1');
    expect(themeSlotFor('background2')).toBe('lt2');
    expect(themeSlotFor('accent3')).toBe('accent3');
    expect(themeSlotFor('hyperlink')).toBe('hlink');
    expect(themeSlotFor('text1', { t1: 'light1' })).toBe('lt1');
  });

  it('applies Word tint/shade in HSL', () => {
    expect(applyTintShade('4472C4', undefined, 0xbf)).toBe('2F5496');
    expect(applyTintShade('4472C4', 0x33)).toBe('D9E2F3');
    expect(applyTintShade('000000', 0xa5)).toBe('5A5A5A');
    expect(applyTintShade('ABCDEF')).toBe('ABCDEF');
  });

  it('theme colours win over cached values and follow theme changes', () => {
    expect(resolveDocColor({ val: '000000', themeColor: 'accent2' }, { theme })).toBe('ED7D31');
    expect(resolveDocColor({ val: '1F3763', themeColor: 'accent1', themeShade: 0x7f }, { theme })).toBe(
      '1F3763',
    );
    const changed = officeTheme();
    changed.colorScheme.colors.accent1 = { kind: 'srgb', hex: 'FF0000', transforms: [] };
    expect(
      resolveDocColor({ val: '2F5496', themeColor: 'accent1', themeShade: 0xbf }, { theme: changed }),
    ).toBe('BF0000');
    expect(resolveDocColor({ val: 'auto' })).toBe('auto');
    expect(resolveDocColor(undefined)).toBe('auto');
    expect(resolveShadingFill({ val: 'clear', fill: { val: 'auto' } })).toBeUndefined();
    expect(
      resolveShadingFill(
        { val: 'clear', fill: { val: 'auto', themeColor: 'accent6', themeTint: 0x33 } },
        { theme },
      ),
    ).toBe('E2EFD9');
  });

  it('resolves theme fonts per slot and falls back for empty ea/cs', () => {
    const f = resolveRunFonts(
      {
        asciiTheme: 'majorHAnsi',
        hAnsiTheme: 'majorHAnsi',
        eastAsiaTheme: 'majorEastAsia',
        csTheme: 'majorBidi',
      },
      theme,
    );
    expect(f).toEqual({
      ascii: 'Calibri Light',
      hAnsi: 'Calibri Light',
      eastAsia: 'Calibri Light',
      cs: 'Calibri Light',
    });
    expect(resolveRunFonts({ ascii: 'Arial' }, theme)).toEqual({
      ascii: 'Arial',
      hAnsi: 'Arial',
      eastAsia: 'Arial',
      cs: 'Arial',
    });
    expect(resolveRunFonts(undefined, theme).ascii).toBe('');
  });
});

describe('sections', () => {
  it('default paper by locale', () => {
    expect(defaultPaperForLocale('en-US')).toBe('letter');
    expect(defaultPaperForLocale('es-MX')).toBe('letter');
    expect(defaultPaperForLocale('fr-FR')).toBe('a4');
    expect(defaultPaperForLocale('ar')).toBe('a4');
  });

  it('orientation swaps the page size; paper names match either way', () => {
    const land = withOrientation(defaultSectionProps('a4'), 'landscape');
    expect(land.pageSize).toMatchObject({ width: 16838, height: 11906, orientation: 'landscape' });
    expect(paperNameOf(land)).toBe('a4');
    expect(withOrientation(land, 'portrait').pageSize.width).toBe(11906);
  });

  it('content and column widths', () => {
    const s = defaultSectionProps('letter');
    expect(contentWidth(s)).toBe(9360);
    expect(
      columnWidths({ ...s, columns: { count: 2, space: 720, equalWidth: true, separator: false } }),
    ).toEqual([4320, 4320]);
    expect(
      columnWidths({
        ...s,
        columns: {
          count: 2,
          space: 0,
          equalWidth: false,
          separator: true,
          cols: [
            { width: 3000, space: 360 },
            { width: 6000, space: 0 },
          ],
        },
      }),
    ).toEqual([3000, 6000]);
  });

  it('break behaviour per section type', () => {
    expect(breakBehaviour('continuous')).toEqual({ newPage: false });
    expect(breakBehaviour('oddPage')).toEqual({ newPage: true, parity: 'odd' });
    expect(breakBehaviour('nextPage')).toEqual({ newPage: true });
  });

  it('header kind per page (titlePg, evenAndOddHeaders)', () => {
    const s: SectionProps = { ...defaultSectionProps(), titlePage: true };
    expect(
      headerFooterKindForPage(s, { evenAndOddHeaders: true }, { isFirstOfSection: true, pageNumber: 1 }),
    ).toBe('first');
    expect(
      headerFooterKindForPage(s, { evenAndOddHeaders: true }, { isFirstOfSection: false, pageNumber: 2 }),
    ).toBe('even');
    expect(
      headerFooterKindForPage(s, { evenAndOddHeaders: false }, { isFirstOfSection: false, pageNumber: 2 }),
    ).toBe('default');
  });

  it('header references link to previous sections', () => {
    const a: SectionProps = { ...defaultSectionProps(), headerRefs: { default: 'rId1', first: 'rId2' } };
    const b: SectionProps = { ...defaultSectionProps() };
    const c: SectionProps = { ...defaultSectionProps(), headerRefs: { default: 'rId9' } };
    const all = [a, b, c];
    expect(resolveHeaderFooterRef(all, 1, 'header', 'default')).toBe('rId1');
    expect(resolveHeaderFooterRef(all, 2, 'header', 'default')).toBe('rId9');
    expect(resolveHeaderFooterRef(all, 2, 'header', 'first')).toBe('rId2');
    expect(resolveHeaderFooterRef(all, 2, 'footer', 'default')).toBeUndefined();
  });

  it('page numbering restarts', () => {
    const restart: SectionProps = {
      ...defaultSectionProps(),
      pageNumbering: { start: 1, format: 'decimal' },
    };
    expect(
      sectionStartPageNumbers([defaultSectionProps(), defaultSectionProps(), restart], [3, 2, 4]),
    ).toEqual([1, 4, 1]);
  });
});

describe('units', () => {
  it('converts twips, half-points and line spacing', () => {
    expect(twipsToPt(240)).toBe(12);
    expect(twipsToPx(1440)).toBe(96);
    expect(cmToTwips(2.54)).toBe(1440);
    expect(twipsToCm(1440)).toBeCloseTo(2.54);
    expect(halfPointsToPx(24)).toBe(16);
    expect(lineMultipleToUnits(1.08)).toBe(259);
    expect(cssLineHeight({ line: 360, lineRule: 'auto' })).toBe('1.5');
    expect(cssLineHeight({ line: 280, lineRule: 'exact' })).toBe('14pt');
    expect(cssLineHeight(undefined)).toBe('normal');
    expect(formatLength(1440, 'cm')).toBe('2.54 cm');
    expect(formatLength(720, 'in')).toBe('0.5"');
    expect(formatLength(240, 'pt')).toBe('12 pt');
  });
});

describe('plain text', () => {
  it('serialises paragraphs, tabs, breaks and tables', () => {
    const d = doc(
      p('Title'),
      p('a', schema.node('tab'), 'b', schema.node('softBreak'), 'c'),
      table({}, row(cell(p('1')), cell(p('2'))), row(cell(p('3')), cell(p('4')))),
      p(
        text('kept'),
        text(' gone', { styleId: undefined }),
        schema.text(' deleted', [schema.mark('deletion', { id: '1', author: 'x' })]),
      ),
    );
    expect(toPlainText(d)).toBe('Title\na\tb\nc\n1\t2\n3\t4\nkept gone deleted');
    expect(toPlainText(d, { acceptChanges: true }).endsWith('kept gone')).toBe(true);
    expect(
      toPlainText(d, { listLabel: (n) => (n.textContent === 'Title' ? '1.' : undefined) }).startsWith(
        '1.\tTitle',
      ),
    ).toBe(true);
  });

  it('round-trips through fromPlainText', () => {
    const txt = 'Hello\tworld\n\nSecond paragraph\fafter break';
    const d = fromPlainText(txt);
    expect(() => d.check()).not.toThrow();
    expect(toPlainText(d)).toBe(txt);
    expect(fromPlainText('x', { styleId: 'Title' }).firstChild?.firstChild?.attrs.styleId).toBe('Title');
  });

  it('omits headers and notes unless asked', () => {
    const s = schema.node('section', { props: defaultSectionProps() }, [
      schema.node('header', null, [p('Head')]),
      p('Body'),
    ]);
    const d = schema.node('doc', null, [s, schema.node('footnote', { id: '1' }, [p('Note')])]);
    expect(toPlainText(d)).toBe('Body');
    expect(toPlainText(d, { includeHeadersFooters: true, includeNotes: true })).toBe('Head\nBody\nNote');
  });
});

describe('statistics', () => {
  it.each([
    ['The quick brown fox', 4],
    ['  spaced   out  ', 2],
    ["don't stop-believing", 2],
    ['Hello — world', 2],
    ['日本語のテキスト', 8],
    ['Kiln 日本 rocks', 4],
    ['', 0],
    ['1 2 3', 3],
  ])('countWordsInText(%j) = %i', (s, n) => expect(countWordsInText(s)).toBe(n));

  it('counts a document like Word', () => {
    const d = doc(
      p('One two three.'),
      p(''),
      p('Four', schema.node('tab'), 'five'),
      table({}, row(cell(p('six')), cell(p('seven eight')))),
    );
    const s = statistics(d, { layout: () => ({ pages: 2, lines: 5 }) });
    expect(s.words).toBe(8);
    expect(s.paragraphs).toBe(4);
    expect(s.characters).toBe('Onetwothree.Fourfivesixseveneight'.length);
    expect(s.charactersWithSpaces).toBe('One two three.Fourfivesixseven eight'.length); // tabs between cells are not characters
    expect([s.pages, s.lines]).toEqual([2, 5]);
    expect(countWords(d)).toBe(8);
  });

  it('excludes deleted revisions and notes by default', () => {
    const del = schema.mark('deletion', { id: '1', author: 'x' });
    const d = schema.node('doc', null, [
      schema.node('section', { props: defaultSectionProps() }, [
        p('keep', schema.text(' removed words', [del])),
      ]),
      schema.node('footnote', { id: '1' }, [p('note words here')]),
    ]);
    expect(statistics(d).words).toBe(1);
    expect(statistics(d, { includeNotes: true }).words).toBe(4);
  });
});
