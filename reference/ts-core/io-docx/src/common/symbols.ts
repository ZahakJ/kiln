/**
 * `w:sym` characters live in symbol fonts (Symbol, Wingdings, Webdings, …) that are not available on
 * every platform. The reader maps them to their Unicode equivalents for display and keeps the original
 * font and code so the writer restores the `w:sym` element.
 */
import { hex as dingbatHex } from 'dingbat-to-unicode';

/**
 * Unicode text for a symbol-font character, or `undefined` when the font/character has no known
 * equivalent. `charHex` is `w:sym/@w:char` (`F04A`, `004A`, `4A`): Word stores symbol-font code points in
 * the private use area `F0xx`, which map to the font's 8-bit code `xx`.
 */
export function symbolToUnicode(font: string, charHex: string): string | undefined {
  const code = charHex.trim();
  if (!/^[0-9a-fA-F]{1,6}$/.test(code)) return undefined;
  const direct = dingbatHex(font, code);
  if (direct) return direct.string;
  if (/^f0[0-9a-f]{2}$/i.test(code)) return dingbatHex(font, code.slice(2))?.string;
  return undefined;
}

/** The text a `w:sym` run displays: the Unicode equivalent, else the raw code point. */
export function symbolText(font: string, charHex: string): string {
  const mapped = symbolToUnicode(font, charHex);
  if (mapped) return mapped;
  const cp = Number.parseInt(charHex, 16);
  return Number.isFinite(cp) && cp > 0 && cp <= 0x10ffff ? String.fromCodePoint(cp) : '';
}
