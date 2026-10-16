/**
 * String helpers shared by the reader and the writer: XML entity decoding/escaping tuned for the hot
 * paths (cell values, shared strings), SpreadsheetML's `_xHHHH_` escapes and attribute parsing.
 */

const ENTITY_RE = /&(?:#(\d+)|#x([0-9a-fA-F]+)|(lt|gt|amp|quot|apos));/g;
const NAMED: Record<string, string> = { lt: '<', gt: '>', amp: '&', quot: '"', apos: "'" };

/** Decodes the five XML entities and numeric character references. */
export function decodeEntities(s: string): string {
  if (s.indexOf('&') < 0) return s;
  return s.replace(ENTITY_RE, (_m, dec: string | undefined, hex: string | undefined, name: string | undefined) => {
    if (name) return NAMED[name] as string;
    const cp = dec !== undefined ? Number.parseInt(dec, 10) : Number.parseInt(hex as string, 16);
    try {
      return String.fromCodePoint(cp);
    } catch {
      return '';
    }
  });
}

const ESC_TEXT_RE = /[&<>]/;
const ESC_ATTR_RE = /[&<>"\n\r\t]/;
// XML 1.0 forbids most C0 controls, U+FFFE/U+FFFF and unpaired surrogates.
const INVALID_RE = /[\u0000-\u0008\u000b\u000c\u000e-\u001f￾￿]|[\ud800-\udbff](?![\udc00-\udfff])|(?<![\ud800-\udbff])[\udc00-\udfff]/g;

function stripInvalid(s: string): string {
  INVALID_RE.lastIndex = 0;
  return INVALID_RE.test(s) ? s.replace(INVALID_RE, '') : s;
}

/** Escapes element text. */
export function escText(s: string): string {
  const t = stripInvalid(s);
  if (!ESC_TEXT_RE.test(t)) return t;
  return t.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/** Escapes an attribute value (double-quoted). */
export function escAttr(s: string): string {
  const t = stripInvalid(s);
  if (!ESC_ATTR_RE.test(t)) return t;
  return t
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/\n/g, '&#10;')
    .replace(/\r/g, '&#13;')
    .replace(/\t/g, '&#9;');
}

const XESC_RE = /_x([0-9A-Fa-f]{4})_/g;

/**
 * Decodes SpreadsheetML's `_xHHHH_` escapes (ECMA-376 Part 1 §22.4.2.4), used for characters XML
 * cannot carry (`_x000D_` for a carriage return, `_x0001_`…). `_x005F_` escapes a literal underscore.
 */
export function decodeXEscapes(s: string): string {
  if (s.indexOf('_x') < 0) return s;
  return s.replace(XESC_RE, (_m, hex: string) => String.fromCharCode(Number.parseInt(hex, 16)));
}

// biome-ignore lint/suspicious/noControlCharactersInRegex: these are exactly the characters to escape
const NEEDS_XESC = /[\u0000-\u0008\u000b\u000c\u000e-\u001f\r￾￿]|_x[0-9A-Fa-f]{4}_/;

/** Encodes text for a SpreadsheetML string: control characters and literal `_xHHHH_` sequences. */
export function encodeXEscapes(s: string): string {
  if (!NEEDS_XESC.test(s)) return s;
  return s
    .replace(/_(x[0-9A-Fa-f]{4}_)/g, '_x005F_$1')
    .replace(
      // biome-ignore lint/suspicious/noControlCharactersInRegex: see above
      /[\u0000-\u0008\u000b\u000c\u000e-\u001f\r￾￿]/g,
      (c) => `_x${c.charCodeAt(0).toString(16).toUpperCase().padStart(4, '0')}_`,
    );
}

/** xsd:boolean → boolean (`1`/`true`); `undefined` when absent. */
export function parseBool(v: string | undefined): boolean | undefined {
  if (v === undefined) return undefined;
  return v === '1' || v === 'true';
}

/** Number attribute; `undefined` when absent or not numeric. */
export function parseNum(v: string | undefined): number | undefined {
  if (v === undefined || v === '') return undefined;
  const n = Number(v);
  return Number.isFinite(n) ? n : undefined;
}

/** Integer attribute; `undefined` when absent or not an integer. */
export function parseInt10(v: string | undefined): number | undefined {
  if (v === undefined || v === '') return undefined;
  const n = Number.parseInt(v, 10);
  return Number.isFinite(n) ? n : undefined;
}

/** Shortest round-tripping text for a number, as Excel writes it (`1E+21` style for big exponents). */
export function formatNumber(n: number): string {
  if (Number.isInteger(n) && Math.abs(n) < 1e21) return String(n);
  const s = String(n);
  // JS writes 1e+21 / 1e-7; Excel writes 1E+21 / 1E-7. Both parse, but match Excel for diffs.
  return s.includes('e') ? s.replace('e', 'E') : s;
}

/** Cell key for per-cell maps. */
export function cellKey(row: number, col: number): string {
  return `${row},${col}`;
}

export function parseCellKey(key: string): { row: number; col: number } {
  const i = key.indexOf(',');
  return { row: Number(key.slice(0, i)), col: Number(key.slice(i + 1)) };
}
