/**
 * Print settings as the core takes them (`PrintSettings`), the choices the Print and Export panes
 * offer, and small helpers. The core validates everything (page boxes, ranges); these are only
 * the defaults and labels.
 */
import { msg, t } from '@kiln/i18n';
import type { PrintMargins, PrintPaper, PrintSettings, SlidePrint } from '@kiln/protocol';

/** Word's defaults: every page, one copy, as laid out, markup on, tagged with bookmarks. */
export const DEFAULT_PRINT_SETTINGS: PrintSettings = {
  pages: '',
  parity: 'All',
  copies: 1,
  paper: null,
  scale: 1,
  per_sheet: 1,
  orientation: null,
  margins: null,
  grayscale: false,
  markup: true,
  hidden_text: false,
  tagged: true,
  bookmarks: true,
};

export const PAPERS: readonly { value: PrintPaper; label: string; size: string }[] = [
  { value: 'Letter', label: msg('Letter'), size: msg('8.5″ × 11″') },
  { value: 'Legal', label: msg('Legal'), size: msg('8.5″ × 14″') },
  { value: 'A4', label: msg('A4'), size: msg('21 × 29.7 cm') },
  { value: 'A3', label: msg('A3'), size: msg('29.7 × 42 cm') },
  { value: 'A5', label: msg('A5'), size: msg('14.8 × 21 cm') },
];

export const MARGINS: readonly { value: PrintMargins; label: string; detail: string }[] = [
  { value: 'Normal', label: msg('Normal'), detail: msg('1″ all round') },
  { value: 'Narrow', label: msg('Narrow'), detail: msg('0.5″ all round') },
  { value: 'Moderate', label: msg('Moderate'), detail: msg('1″ top and bottom, 0.75″ sides') },
  { value: 'Wide', label: msg('Wide'), detail: msg('1″ top and bottom, 2″ sides') },
];

export const PER_SHEET = [1, 2, 4, 6, 9, 16] as const;

/** Which part of the selection prints. */
export type PageMode = 'all' | 'custom' | 'odd' | 'even';

export function pageMode(s: PrintSettings): PageMode {
  if (s.parity === 'Odd') return 'odd';
  if (s.parity === 'Even') return 'even';
  return s.pages.trim() ? 'custom' : 'all';
}

/** "Page 2 of 12" / "Sheet 2 of 6 (pages 3–4)". */
export function sheetLabel(
  sheet: number,
  sheets: number,
  pages: readonly number[],
  perSheet: number,
): string {
  if (perSheet <= 1 || pages.length === 0)
    return t('Page {page, number} of {pages, number}', { page: sheet + 1, pages: sheets });
  const first = (pages[0] ?? 0) + 1;
  const last = (pages[pages.length - 1] ?? 0) + 1;
  return t('Sheet {sheet, number} of {sheets, number} (pages {first, number}–{last, number})', {
    sheet: sheet + 1,
    sheets,
    first,
    last,
  });
}

/** Kiln Deck's print layouts (PowerPoint's Print Layout and Handouts menu). */
export const SLIDE_LAYOUTS: readonly {
  value: string;
  label: string;
  group: 'Print Layout' | 'Handouts';
  slides: Pick<SlidePrint, 'layout' | 'per_page' | 'vertical'>;
}[] = [
  {
    value: 'full',
    label: msg('Full Page Slides'),
    group: 'Print Layout',
    slides: { layout: 'FullPage', per_page: 1, vertical: false },
  },
  {
    value: 'notes',
    label: msg('Notes Pages'),
    group: 'Print Layout',
    slides: { layout: 'NotesPages', per_page: 1, vertical: false },
  },
  {
    value: 'outline',
    label: msg('Outline'),
    group: 'Print Layout',
    slides: { layout: 'Outline', per_page: 1, vertical: false },
  },
  {
    value: 'h1',
    label: msg('1 Slide'),
    group: 'Handouts',
    slides: { layout: 'Handouts', per_page: 1, vertical: false },
  },
  {
    value: 'h2',
    label: msg('2 Slides'),
    group: 'Handouts',
    slides: { layout: 'Handouts', per_page: 2, vertical: false },
  },
  {
    value: 'h3',
    label: msg('3 Slides'),
    group: 'Handouts',
    slides: { layout: 'Handouts', per_page: 3, vertical: false },
  },
  {
    value: 'h4',
    label: msg('4 Slides Horizontal'),
    group: 'Handouts',
    slides: { layout: 'Handouts', per_page: 4, vertical: false },
  },
  {
    value: 'h6',
    label: msg('6 Slides Horizontal'),
    group: 'Handouts',
    slides: { layout: 'Handouts', per_page: 6, vertical: false },
  },
  {
    value: 'h9',
    label: msg('9 Slides Horizontal'),
    group: 'Handouts',
    slides: { layout: 'Handouts', per_page: 9, vertical: false },
  },
  {
    value: 'h4v',
    label: msg('4 Slides Vertical'),
    group: 'Handouts',
    slides: { layout: 'Handouts', per_page: 4, vertical: true },
  },
  {
    value: 'h6v',
    label: msg('6 Slides Vertical'),
    group: 'Handouts',
    slides: { layout: 'Handouts', per_page: 6, vertical: true },
  },
  {
    value: 'h9v',
    label: msg('9 Slides Vertical'),
    group: 'Handouts',
    slides: { layout: 'Handouts', per_page: 9, vertical: true },
  },
];

/** The slide options in force (full page slides when the settings have none). */
export function slidesOf(s: PrintSettings): SlidePrint {
  return s.slides ?? { layout: 'FullPage', per_page: 6, vertical: false, frame: false, hidden: false };
}

/** The `SLIDE_LAYOUTS` entry of settings. */
export function slideLayoutOf(s: PrintSettings): string {
  const v = slidesOf(s);
  const found = SLIDE_LAYOUTS.find(
    (l) =>
      l.slides.layout === v.layout &&
      (v.layout !== 'Handouts' ||
        (l.slides.per_page === v.per_page && (l.slides.vertical === v.vertical || l.slides.per_page < 4))),
  );
  return found?.value ?? 'full';
}

/** Settings with another slide layout (frame and hidden slides kept; handouts are framed). */
export function withSlideLayout(s: PrintSettings, value: string): Partial<PrintSettings> {
  const l = SLIDE_LAYOUTS.find((x) => x.value === value) ?? SLIDE_LAYOUTS[0];
  const now = slidesOf(s);
  if (!l) return {};
  const frame = l.slides.layout === 'Handouts' && now.layout !== 'Handouts' ? true : now.frame;
  return { slides: { ...now, ...l.slides, frame } };
}
