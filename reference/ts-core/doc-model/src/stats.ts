/**
 * Document statistics as shown by Word's Review ▸ Word Count dialog and status bar. Words are runs of
 * non-whitespace characters (Word's rule), except that each CJK ideograph, kana or hangul syllable
 * counts as one word, as Word does for East Asian text. Deleted tracked-change text is excluded.
 */
import type { Node as PMNode } from '@tiptap/pm/model';
import { toPlainText } from './serialize.ts';

const EAST_ASIAN = /[぀-ヿ㐀-䶿一-鿿가-힯豈-﫿\u{20000}-\u{2FA1F}]/u;
const EAST_ASIAN_G = /[぀-ヿ㐀-䶿一-鿿가-힯豈-﫿\u{20000}-\u{2FA1F}]/gu;

/** Counts words in a string. */
export function countWordsInText(text: string): number {
  let count = 0;
  for (const token of text.split(/[\s\u00A0\u2000-\u200B\u3000]+/)) {
    if (!token) continue;
    if (!EAST_ASIAN.test(token)) {
      // Pure punctuation tokens ("—", "-", "•") are not words in Word.
      if (/[\p{L}\p{N}]/u.test(token)) count++;
      continue;
    }
    const ideographs = token.match(EAST_ASIAN_G)?.length ?? 0;
    const rest = token
      .replace(EAST_ASIAN_G, ' ')
      .split(/\s+/)
      .filter((t) => /[\p{L}\p{N}]/u.test(t)).length;
    count += ideographs + rest;
  }
  return count;
}

export interface DocumentStatistics {
  words: number;
  /** Characters excluding spaces (Word: "Characters (no spaces)"). */
  characters: number;
  /** Characters including spaces. */
  charactersWithSpaces: number;
  /** Non-empty paragraphs (Word counts paragraphs that contain text). */
  paragraphs: number;
  /** Lines and pages need layout; they are filled from the layout callback when supplied. */
  lines?: number;
  pages?: number;
}

export interface StatisticsOptions {
  /** Include footnotes/endnotes (Word's "Include textboxes, footnotes and endnotes" checkbox). */
  includeNotes?: boolean;
  /** Layout callback providing page and line counts. */
  layout?: () => { pages: number; lines?: number } | undefined;
}

/** Counts characters the way Word does: tabs/line breaks are not characters; spaces are separate. */
function countChars(text: string): { withSpaces: number; noSpaces: number } {
  let withSpaces = 0;
  let noSpaces = 0;
  for (const ch of text) {
    if (ch === '\n' || ch === '\t' || ch === '\f' || ch === '\v' || ch === '￼') continue;
    withSpaces++;
    if (!/\s/.test(ch)) noSpaces++;
  }
  return { withSpaces, noSpaces };
}

export const countWords = (doc: PMNode, opts: StatisticsOptions = {}): number => statistics(doc, opts).words;
export const countCharacters = (doc: PMNode, opts: StatisticsOptions & { spaces?: boolean } = {}): number => {
  const s = statistics(doc, opts);
  return opts.spaces ? s.charactersWithSpaces : s.characters;
};
export const countParagraphs = (doc: PMNode, opts: StatisticsOptions = {}): number =>
  statistics(doc, opts).paragraphs;

/** Page count from a layout callback (pagination lives in the editor). */
export function countPages(layout: () => { pages: number } | undefined): number | undefined {
  return layout()?.pages;
}

/** Full statistics for a document or a fragment (e.g. the selection's slice content). */
export function statistics(node: PMNode, opts: StatisticsOptions = {}): DocumentStatistics {
  const text = toPlainText(node, { acceptChanges: true, includeNotes: opts.includeNotes ?? false });
  let paragraphs = 0;
  node.descendants((child) => {
    if (child.type.name === 'header' || child.type.name === 'footer') return false;
    if (child.type.name === 'footnote' && !opts.includeNotes) return false;
    if (child.type.name === 'paragraph') {
      if (/\S/.test(child.textContent)) paragraphs++;
      return false;
    }
    return true;
  });
  if (node.type.name === 'paragraph' && /\S/.test(node.textContent)) paragraphs = 1;
  const chars = countChars(text);
  const out: DocumentStatistics = {
    words: countWordsInText(text),
    characters: chars.noSpaces,
    charactersWithSpaces: chars.withSpaces,
    paragraphs,
  };
  const layout = opts.layout?.();
  if (layout) {
    out.pages = layout.pages;
    if (layout.lines !== undefined) out.lines = layout.lines;
  }
  return out;
}
