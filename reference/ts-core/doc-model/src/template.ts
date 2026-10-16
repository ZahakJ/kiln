/**
 * The default template for new documents: Word 2016–2021 `Normal.dotm` defaults (PLAN §3.3), because
 * that is what most `.docx` files in the wild were authored against. Body text is Calibri (rendered
 * with the metric-compatible Carlito) 11 pt, line spacing 1.08 (`259` auto), 8 pt after; headings use
 * Calibri Light in the Office theme's Accent 1 shades.
 */

import { officeTheme, type Theme } from '@kiln-ref/ooxml';
import type { Node as PMNode, Schema } from '@tiptap/pm/model';
import { Numbering } from './numbering/numbering.ts';
import { docSchema } from './schema.ts';
import { defaultPaperForLocale, defaultSectionProps, type PaperName } from './sections.ts';
import { type Style, StyleSheet } from './styles/stylesheet.ts';
import {
  DEFAULT_SETTINGS,
  type DocSettings,
  type RunFonts,
  type SectionProps,
  type TableBorders,
} from './types.ts';

const MAJOR: RunFonts = {
  asciiTheme: 'majorHAnsi',
  eastAsiaTheme: 'majorEastAsia',
  hAnsiTheme: 'majorHAnsi',
  csTheme: 'majorBidi',
};
const MINOR: RunFonts = {
  asciiTheme: 'minorHAnsi',
  eastAsiaTheme: 'minorHAnsi',
  hAnsiTheme: 'minorHAnsi',
  csTheme: 'minorBidi',
};

const accent1Shade = (val: string, shade: number) => ({
  val,
  themeColor: 'accent1' as const,
  themeShade: shade,
});

const single = { style: 'single', size: 4, space: 0, color: { val: 'auto' } } as const;
const GRID_BORDERS: TableBorders = {
  top: single,
  left: single,
  bottom: single,
  right: single,
  insideH: single,
  insideV: single,
};

/** Word's built-in style definitions used by new documents. */
export function defaultStyles(): Style[] {
  const heading = (n: number, extra: Partial<Style>): Style => ({
    id: `Heading${n}`,
    type: 'paragraph',
    name: `heading ${n}`,
    basedOn: 'Normal',
    next: 'Normal',
    link: `Heading${n}Char`,
    uiPriority: 9,
    quickStyle: true,
    ...(n > 1 ? { unhideWhenUsed: true } : {}),
    ...extra,
  });
  const linkedChar = (id: string, name: string, para: string, rPr: Style['rPr']): Style => ({
    id,
    type: 'character',
    name,
    basedOn: 'DefaultParagraphFont',
    link: para,
    uiPriority: 9,
    ...(rPr ? { rPr } : {}),
  });
  const toc = (n: number): Style => ({
    id: `TOC${n}`,
    type: 'paragraph',
    name: `toc ${n}`,
    basedOn: 'Normal',
    next: 'Normal',
    uiPriority: 39,
    unhideWhenUsed: true,
    pPr: { spacing: { after: 100 }, ...(n > 1 ? { indent: { left: (n - 1) * 220 } } : {}) },
  });

  const h1rPr = { fonts: MAJOR, color: accent1Shade('2F5496', 0xbf), size: 32, sizeCs: 32 };
  const h2rPr = { fonts: MAJOR, color: accent1Shade('2F5496', 0xbf), size: 26, sizeCs: 26 };
  const h3rPr = { fonts: MAJOR, color: accent1Shade('1F3763', 0x7f), size: 24, sizeCs: 24 };
  const h4rPr = { fonts: MAJOR, italic: true, italicCs: true, color: accent1Shade('2F5496', 0xbf) };
  const h5rPr = { fonts: MAJOR, color: accent1Shade('2F5496', 0xbf) };
  const h6rPr = { fonts: MAJOR, color: accent1Shade('1F3763', 0x7f) };
  const subHeadPPr = (level: number) => ({
    keepNext: true,
    keepLines: true,
    spacing: { before: 40, after: 0 },
    outlineLevel: level,
  });
  const titleRPr = { fonts: MAJOR, spacing: -10, kern: 28, size: 56, sizeCs: 56 };
  const subtitleRPr = {
    fonts: { eastAsiaTheme: 'minorEastAsia' as const },
    color: { val: '5A5A5A', themeColor: 'text1' as const, themeTint: 0xa5 },
    spacing: 15,
  };
  const quoteRPr = {
    italic: true,
    italicCs: true,
    color: { val: '404040', themeColor: 'text1' as const, themeTint: 0xbf },
  };
  const tabsHF = [
    { pos: 4680, align: 'center' as const },
    { pos: 9360, align: 'right' as const },
  ];

  return [
    { id: 'Normal', type: 'paragraph', name: 'Normal', isDefault: true, quickStyle: true, uiPriority: 0 },
    heading(1, {
      pPr: { keepNext: true, keepLines: true, spacing: { before: 240, after: 0 }, outlineLevel: 0 },
      rPr: h1rPr,
    }),
    heading(2, { pPr: subHeadPPr(1), rPr: h2rPr }),
    heading(3, { pPr: subHeadPPr(2), rPr: h3rPr }),
    heading(4, { pPr: subHeadPPr(3), rPr: h4rPr }),
    heading(5, { pPr: subHeadPPr(4), rPr: h5rPr }),
    heading(6, { pPr: subHeadPPr(5), rPr: h6rPr }),
    {
      id: 'DefaultParagraphFont',
      type: 'character',
      name: 'Default Paragraph Font',
      isDefault: true,
      uiPriority: 1,
      semiHidden: true,
      unhideWhenUsed: true,
    },
    {
      id: 'TableNormal',
      type: 'table',
      name: 'Normal Table',
      isDefault: true,
      uiPriority: 99,
      semiHidden: true,
      unhideWhenUsed: true,
      tblPr: { indent: 0, cellMargins: { top: 0, left: 108, bottom: 0, right: 108 } },
    },
    {
      id: 'NoList',
      type: 'numbering',
      name: 'No List',
      isDefault: true,
      uiPriority: 99,
      semiHidden: true,
      unhideWhenUsed: true,
    },
    linkedChar('Heading1Char', 'Heading 1 Char', 'Heading1', h1rPr),
    linkedChar('Heading2Char', 'Heading 2 Char', 'Heading2', h2rPr),
    linkedChar('Heading3Char', 'Heading 3 Char', 'Heading3', h3rPr),
    linkedChar('Heading4Char', 'Heading 4 Char', 'Heading4', h4rPr),
    linkedChar('Heading5Char', 'Heading 5 Char', 'Heading5', h5rPr),
    linkedChar('Heading6Char', 'Heading 6 Char', 'Heading6', h6rPr),
    {
      id: 'Title',
      type: 'paragraph',
      name: 'Title',
      basedOn: 'Normal',
      next: 'Normal',
      link: 'TitleChar',
      uiPriority: 10,
      quickStyle: true,
      pPr: { spacing: { after: 0, line: 240, lineRule: 'auto' }, contextualSpacing: true },
      rPr: titleRPr,
    },
    { ...linkedChar('TitleChar', 'Title Char', 'Title', titleRPr), uiPriority: 10 },
    {
      id: 'Subtitle',
      type: 'paragraph',
      name: 'Subtitle',
      basedOn: 'Normal',
      next: 'Normal',
      link: 'SubtitleChar',
      uiPriority: 11,
      quickStyle: true,
      pPr: { spacing: { after: 160 } },
      rPr: subtitleRPr,
    },
    { ...linkedChar('SubtitleChar', 'Subtitle Char', 'Subtitle', subtitleRPr), uiPriority: 11 },
    {
      id: 'SubtleEmphasis',
      type: 'character',
      name: 'Subtle Emphasis',
      basedOn: 'DefaultParagraphFont',
      uiPriority: 19,
      quickStyle: true,
      rPr: { italic: true, italicCs: true, color: { val: '404040', themeColor: 'text1', themeTint: 0xbf } },
    },
    {
      id: 'Emphasis',
      type: 'character',
      name: 'Emphasis',
      basedOn: 'DefaultParagraphFont',
      uiPriority: 20,
      quickStyle: true,
      rPr: { italic: true, italicCs: true },
    },
    {
      id: 'IntenseEmphasis',
      type: 'character',
      name: 'Intense Emphasis',
      basedOn: 'DefaultParagraphFont',
      uiPriority: 21,
      quickStyle: true,
      rPr: { italic: true, italicCs: true, color: { val: '4472C4', themeColor: 'accent1' } },
    },
    {
      id: 'Strong',
      type: 'character',
      name: 'Strong',
      basedOn: 'DefaultParagraphFont',
      uiPriority: 22,
      quickStyle: true,
      rPr: { bold: true, boldCs: true },
    },
    {
      id: 'Quote',
      type: 'paragraph',
      name: 'Quote',
      basedOn: 'Normal',
      next: 'Normal',
      link: 'QuoteChar',
      uiPriority: 29,
      quickStyle: true,
      pPr: { spacing: { before: 200, after: 160 }, indent: { left: 864, right: 864 }, alignment: 'center' },
      rPr: quoteRPr,
    },
    { ...linkedChar('QuoteChar', 'Quote Char', 'Quote', quoteRPr), uiPriority: 29 },
    {
      id: 'IntenseQuote',
      type: 'paragraph',
      name: 'Intense Quote',
      basedOn: 'Normal',
      next: 'Normal',
      uiPriority: 30,
      quickStyle: true,
      pPr: {
        borders: {
          top: { style: 'single', size: 4, space: 10, color: { val: '4472C4', themeColor: 'accent1' } },
          bottom: { style: 'single', size: 4, space: 10, color: { val: '4472C4', themeColor: 'accent1' } },
        },
        spacing: { before: 360, after: 360 },
        indent: { left: 864, right: 864 },
        alignment: 'center',
      },
      rPr: { italic: true, italicCs: true, color: { val: '4472C4', themeColor: 'accent1' } },
    },
    {
      id: 'SubtleReference',
      type: 'character',
      name: 'Subtle Reference',
      basedOn: 'DefaultParagraphFont',
      uiPriority: 31,
      quickStyle: true,
      rPr: { smallCaps: true, color: { val: '5A5A5A', themeColor: 'text1', themeTint: 0xa5 } },
    },
    {
      id: 'IntenseReference',
      type: 'character',
      name: 'Intense Reference',
      basedOn: 'DefaultParagraphFont',
      uiPriority: 32,
      quickStyle: true,
      rPr: {
        bold: true,
        boldCs: true,
        smallCaps: true,
        color: { val: '4472C4', themeColor: 'accent1' },
        spacing: 5,
      },
    },
    {
      id: 'BookTitle',
      type: 'character',
      name: 'Book Title',
      basedOn: 'DefaultParagraphFont',
      uiPriority: 33,
      quickStyle: true,
      rPr: { bold: true, boldCs: true, italic: true, italicCs: true, spacing: 5 },
    },
    {
      id: 'ListParagraph',
      type: 'paragraph',
      name: 'List Paragraph',
      basedOn: 'Normal',
      uiPriority: 34,
      quickStyle: true,
      pPr: { indent: { left: 720 }, contextualSpacing: true },
    },
    {
      id: 'Caption',
      type: 'paragraph',
      name: 'caption',
      basedOn: 'Normal',
      next: 'Normal',
      uiPriority: 35,
      unhideWhenUsed: true,
      quickStyle: true,
      pPr: { spacing: { after: 200, line: 240, lineRule: 'auto' } },
      rPr: {
        italic: true,
        italicCs: true,
        color: { val: '44546A', themeColor: 'text2' },
        size: 18,
        sizeCs: 18,
      },
    },
    {
      id: 'TableGrid',
      type: 'table',
      name: 'Table Grid',
      basedOn: 'TableNormal',
      uiPriority: 39,
      pPr: { spacing: { after: 0, line: 240, lineRule: 'auto' } },
      tblPr: { borders: GRID_BORDERS },
    },
    {
      id: 'TOCHeading',
      type: 'paragraph',
      name: 'TOC Heading',
      basedOn: 'Heading1',
      next: 'Normal',
      uiPriority: 39,
      unhideWhenUsed: true,
      quickStyle: true,
      pPr: { outlineLevel: 9 },
    },
    toc(1),
    toc(2),
    toc(3),
    {
      id: 'FootnoteText',
      type: 'paragraph',
      name: 'footnote text',
      basedOn: 'Normal',
      link: 'FootnoteTextChar',
      uiPriority: 99,
      semiHidden: true,
      unhideWhenUsed: true,
      pPr: { spacing: { after: 0, line: 240, lineRule: 'auto' } },
      rPr: { size: 20, sizeCs: 20 },
    },
    {
      ...linkedChar('FootnoteTextChar', 'Footnote Text Char', 'FootnoteText', { size: 20, sizeCs: 20 }),
      uiPriority: 99,
      semiHidden: true,
    },
    {
      id: 'FootnoteReference',
      type: 'character',
      name: 'footnote reference',
      basedOn: 'DefaultParagraphFont',
      uiPriority: 99,
      semiHidden: true,
      unhideWhenUsed: true,
      rPr: { vertAlign: 'superscript' },
    },
    {
      id: 'EndnoteText',
      type: 'paragraph',
      name: 'endnote text',
      basedOn: 'Normal',
      uiPriority: 99,
      semiHidden: true,
      unhideWhenUsed: true,
      pPr: { spacing: { after: 0, line: 240, lineRule: 'auto' } },
      rPr: { size: 20, sizeCs: 20 },
    },
    {
      id: 'EndnoteReference',
      type: 'character',
      name: 'endnote reference',
      basedOn: 'DefaultParagraphFont',
      uiPriority: 99,
      semiHidden: true,
      unhideWhenUsed: true,
      rPr: { vertAlign: 'superscript' },
    },
    {
      id: 'Header',
      type: 'paragraph',
      name: 'header',
      basedOn: 'Normal',
      link: 'HeaderChar',
      uiPriority: 99,
      unhideWhenUsed: true,
      pPr: { tabs: tabsHF, spacing: { after: 0, line: 240, lineRule: 'auto' } },
    },
    { ...linkedChar('HeaderChar', 'Header Char', 'Header', undefined), uiPriority: 99 },
    {
      id: 'Footer',
      type: 'paragraph',
      name: 'footer',
      basedOn: 'Normal',
      link: 'FooterChar',
      uiPriority: 99,
      unhideWhenUsed: true,
      pPr: { tabs: tabsHF, spacing: { after: 0, line: 240, lineRule: 'auto' } },
    },
    { ...linkedChar('FooterChar', 'Footer Char', 'Footer', undefined), uiPriority: 99 },
    {
      id: 'Hyperlink',
      type: 'character',
      name: 'Hyperlink',
      basedOn: 'DefaultParagraphFont',
      uiPriority: 99,
      unhideWhenUsed: true,
      rPr: { color: { val: '0563C1', themeColor: 'hyperlink' }, underline: { style: 'single' } },
    },
    {
      id: 'FollowedHyperlink',
      type: 'character',
      name: 'FollowedHyperlink',
      basedOn: 'DefaultParagraphFont',
      uiPriority: 99,
      semiHidden: true,
      unhideWhenUsed: true,
      rPr: { color: { val: '954F72', themeColor: 'followedHyperlink' }, underline: { style: 'single' } },
    },
    {
      id: 'NoSpacing',
      type: 'paragraph',
      name: 'No Spacing',
      uiPriority: 1,
      quickStyle: true,
      pPr: { spacing: { after: 0, line: 240, lineRule: 'auto' } },
    },
  ];
}

/** Word 2016+ `w:docDefaults`: minor theme font, 11 pt, en-US; 8 pt after, 1.08 line spacing. */
export function defaultDocDefaults(): StyleSheet['docDefaults'] {
  return {
    rPr: { fonts: MINOR, size: 22, sizeCs: 22, lang: { val: 'en-US', eastAsia: 'en-US', bidi: 'ar-SA' } },
    pPr: { spacing: { after: 160, line: 259, lineRule: 'auto' } },
  };
}

export function createDefaultStyleSheet(): StyleSheet {
  return new StyleSheet(defaultDocDefaults(), defaultStyles());
}

/** A comment thread entry (comment bodies live outside the PM tree; ranges use the `comment` mark). */
export interface DocComment {
  id: string;
  author: string;
  initials?: string;
  date: string;
  /** Plain-text body (rich bodies are stored as PM JSON in `content` when present). */
  text: string;
  content?: unknown;
  parentId?: string;
  resolved?: boolean;
}

/** An in-memory Write document: the PM tree plus the parts it refers to. */
export interface WriteDocument {
  doc: PMNode;
  styles: StyleSheet;
  numbering: Numbering;
  theme: Theme;
  settings: DocSettings;
  comments: DocComment[];
  /** Media by `mediaId` (DOCX part name), as bytes. */
  media: Map<string, { contentType: string; data: Uint8Array }>;
}

export interface NewDocumentOptions {
  /** Paper size; defaults from `locale` (Letter for US/CA/MX/…, A4 elsewhere). */
  paper?: PaperName;
  locale?: string;
  schema?: Schema;
}

/** Builds an empty body: one section with one empty Normal paragraph. */
export function emptyBody(schema: Schema, props: SectionProps): PMNode {
  const paragraph = schema.nodes.paragraph?.create() as PMNode;
  return schema.node('doc', null, [schema.node('section', { props }, [paragraph])]);
}

/** Factory for new documents from the built-in "Blank document" template. */
export const DefaultTemplate = {
  id: 'blank',
  name: 'Blank document',
  create(options: NewDocumentOptions = {}): WriteDocument {
    const schema = options.schema ?? docSchema;
    const paper = options.paper ?? defaultPaperForLocale(options.locale ?? 'en-US');
    const lang = options.locale ?? 'en-US';
    const styles = createDefaultStyleSheet();
    if (lang !== 'en-US') styles.docDefaults.rPr.lang = { ...styles.docDefaults.rPr.lang, val: lang };
    const numbering = new Numbering();
    numbering.listStyleResolver = (id) => styles.get(id)?.pPr?.numbering?.numId;
    return {
      doc: emptyBody(schema, defaultSectionProps(paper)),
      styles,
      numbering,
      theme: officeTheme(),
      settings: { ...DEFAULT_SETTINGS },
      comments: [],
      media: new Map(),
    };
  },
};
