/**
 * Print settings as the core takes them (`PrintSettings`), the choices the Print and Export panes
 * offer, and small helpers. The core validates everything (page boxes, ranges); these are only
 * the defaults and labels.
 */
import type { PrintMargins, PrintPaper, PrintSettings } from '@kiln/protocol';

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
  { value: 'Letter', label: 'Letter', size: '8.5″ × 11″' },
  { value: 'Legal', label: 'Legal', size: '8.5″ × 14″' },
  { value: 'A4', label: 'A4', size: '21 × 29.7 cm' },
  { value: 'A3', label: 'A3', size: '29.7 × 42 cm' },
  { value: 'A5', label: 'A5', size: '14.8 × 21 cm' },
];

export const MARGINS: readonly { value: PrintMargins; label: string; detail: string }[] = [
  { value: 'Normal', label: 'Normal', detail: '1″ all round' },
  { value: 'Narrow', label: 'Narrow', detail: '0.5″ all round' },
  { value: 'Moderate', label: 'Moderate', detail: '1″ top and bottom, 0.75″ sides' },
  { value: 'Wide', label: 'Wide', detail: '1″ top and bottom, 2″ sides' },
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
  const n = `${(sheet + 1).toLocaleString()} of ${sheets.toLocaleString()}`;
  if (perSheet <= 1 || pages.length === 0) return `Page ${n}`;
  const first = (pages[0] ?? 0) + 1;
  const last = (pages[pages.length - 1] ?? 0) + 1;
  return `Sheet ${n} (pages ${first}–${last})`;
}
