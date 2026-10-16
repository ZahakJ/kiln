/**
 * Sections (`w:sectPr`): page size and orientation, margins, columns, header/footer references and
 * break types, plus Word's header/footer selection and inheritance rules.
 */
import type { Node as PMNode } from '@tiptap/pm/model';
import type { DocSettings, HeaderFooterType, PageMargins, SectionBreakType, SectionProps } from './types.ts';

/** Paper sizes in twips (portrait). */
export const PAPER_SIZES = {
  letter: { width: 12240, height: 15840, code: 1, label: 'Letter' },
  legal: { width: 12240, height: 20160, code: 5, label: 'Legal' },
  executive: { width: 10440, height: 15120, code: 7, label: 'Executive' },
  tabloid: { width: 15840, height: 24480, code: 3, label: 'Tabloid' },
  a3: { width: 16838, height: 23811, code: 8, label: 'A3' },
  a4: { width: 11906, height: 16838, code: 9, label: 'A4' },
  a5: { width: 8391, height: 11906, code: 11, label: 'A5' },
  b5: { width: 10319, height: 14572, code: 13, label: 'B5 (JIS)' },
} as const;
export type PaperName = keyof typeof PAPER_SIZES;

/** Regions where Word defaults to US Letter (everywhere else uses A4). */
const LETTER_REGIONS = new Set([
  'US',
  'CA',
  'MX',
  'PH',
  'CL',
  'CO',
  'VE',
  'PR',
  'GT',
  'CR',
  'PA',
  'SV',
  'DO',
  'NI',
]);

/** Default paper for a BCP-47 locale (`en-US` → letter, `de-DE` → a4). */
export function defaultPaperForLocale(locale: string): PaperName {
  const region = /[-_]([A-Za-z]{2})\b/.exec(locale)?.[1]?.toUpperCase();
  if (region) return LETTER_REGIONS.has(region) ? 'letter' : 'a4';
  return locale.toLowerCase() === 'en' ? 'letter' : 'a4';
}

/** Word's "Normal" margins: 1" (2.54 cm) all round, header/footer 0.5". */
export const NORMAL_MARGINS: Readonly<PageMargins> = {
  top: 1440,
  right: 1440,
  bottom: 1440,
  left: 1440,
  header: 720,
  footer: 720,
  gutter: 0,
};

/** Word's Layout ▸ Margins presets. */
export const MARGIN_PRESETS: Readonly<Record<string, PageMargins>> = {
  normal: NORMAL_MARGINS,
  narrow: { ...NORMAL_MARGINS, top: 720, right: 720, bottom: 720, left: 720 },
  moderate: { ...NORMAL_MARGINS, top: 1440, right: 1080, bottom: 1440, left: 1080 },
  wide: { ...NORMAL_MARGINS, top: 1440, right: 2880, bottom: 1440, left: 2880 },
};

/** Section properties of a new Word document (Letter or A4, Normal margins, one column). */
export function defaultSectionProps(paper: PaperName = 'letter'): SectionProps {
  const size = PAPER_SIZES[paper];
  return {
    type: 'nextPage',
    pageSize: { width: size.width, height: size.height, orientation: 'portrait' },
    margins: { ...NORMAL_MARGINS },
    columns: { count: 1, space: 720, equalWidth: true, separator: false },
    titlePage: false,
    docGrid: { linePitch: 360 },
  };
}

/** Changes orientation, swapping width/height as Word does. */
export function withOrientation(props: SectionProps, orientation: 'portrait' | 'landscape'): SectionProps {
  if (props.pageSize.orientation === orientation) return props;
  const { width, height } = props.pageSize;
  const landscape = orientation === 'landscape';
  return {
    ...props,
    pageSize: {
      ...props.pageSize,
      orientation,
      width: landscape ? Math.max(width, height) : Math.min(width, height),
      height: landscape ? Math.min(width, height) : Math.max(width, height),
    },
  };
}

/** Width available for text (twips) — page width minus left/right margins and gutter. */
export function contentWidth(props: SectionProps): number {
  return props.pageSize.width - props.margins.left - props.margins.right - props.margins.gutter;
}

/** Height available for body text (twips). */
export function contentHeight(props: SectionProps): number {
  return props.pageSize.height - Math.abs(props.margins.top) - Math.abs(props.margins.bottom);
}

/** Column widths in twips for a section (equal-width or explicit). */
export function columnWidths(props: SectionProps): number[] {
  const { columns } = props;
  if (!columns.equalWidth && columns.cols?.length) return columns.cols.map((c) => c.width);
  const n = Math.max(1, columns.count);
  const total = contentWidth(props);
  return Array.from({ length: n }, () => Math.floor((total - columns.space * (n - 1)) / n));
}

/** Matches a page size to a named paper (either orientation, ±10 twips). */
export function paperNameOf(props: SectionProps): PaperName | undefined {
  const w = Math.min(props.pageSize.width, props.pageSize.height);
  const h = Math.max(props.pageSize.width, props.pageSize.height);
  for (const [name, s] of Object.entries(PAPER_SIZES)) {
    if (Math.abs(s.width - w) <= 10 && Math.abs(s.height - h) <= 10) return name as PaperName;
  }
  return undefined;
}

/** Section nodes of a document with their positions. */
export function sectionsOf(doc: PMNode): { node: PMNode; pos: number; props: SectionProps }[] {
  const out: { node: PMNode; pos: number; props: SectionProps }[] = [];
  doc.forEach((node, pos) => {
    if (node.type.name === 'section')
      out.push({ node, pos, props: (node.attrs.props as SectionProps | null) ?? defaultSectionProps() });
  });
  return out;
}

/** Whether a section break of `type` forces a new page, and whether a blank page may be inserted. */
export function breakBehaviour(type: SectionBreakType): { newPage: boolean; parity?: 'even' | 'odd' } {
  switch (type) {
    case 'continuous':
    case 'nextColumn':
      return { newPage: false };
    case 'evenPage':
      return { newPage: true, parity: 'even' };
    case 'oddPage':
      return { newPage: true, parity: 'odd' };
    default:
      return { newPage: true };
  }
}

/**
 * Which header/footer kind a page uses: `first` on the first page of a section with a different first
 * page, `even` on even pages when odd/even headers are on, `default` otherwise.
 */
export function headerFooterKindForPage(
  props: SectionProps,
  settings: Pick<DocSettings, 'evenAndOddHeaders'>,
  page: { isFirstOfSection: boolean; pageNumber: number },
): HeaderFooterType {
  if (props.titlePage && page.isFirstOfSection) return 'first';
  if (settings.evenAndOddHeaders && page.pageNumber % 2 === 0) return 'even';
  return 'default';
}

/**
 * Resolves the header (or footer) reference for a section, applying Word's "Link to Previous"
 * inheritance: a section without a reference of that kind uses the previous section's. A first/even
 * kind without any reference in the chain is blank (empty string) rather than falling back to default.
 */
export function resolveHeaderFooterRef(
  sections: readonly SectionProps[],
  index: number,
  part: 'header' | 'footer',
  kind: HeaderFooterType,
): string | undefined {
  for (let i = Math.min(index, sections.length - 1); i >= 0; i--) {
    const refs = part === 'header' ? sections[i]?.headerRefs : sections[i]?.footerRefs;
    const ref = refs?.[kind];
    if (ref !== undefined) return ref;
  }
  return undefined;
}

/** First page number of each section given its page count (restarts from `pageNumbering.start`). */
export function sectionStartPageNumbers(
  sections: readonly SectionProps[],
  pageCounts: readonly number[],
): number[] {
  const out: number[] = [];
  let next = 1;
  sections.forEach((s, i) => {
    const start = s.pageNumbering?.start ?? next;
    out.push(start);
    next = start + (pageCounts[i] ?? 0);
  });
  return out;
}
