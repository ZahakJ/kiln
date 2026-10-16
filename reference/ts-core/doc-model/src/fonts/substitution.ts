/**
 * Font substitution (PLAN §1.2). Office documents name fonts that are not freely redistributable; we
 * bundle SIL-OFL fonts that are **metric-compatible** (identical advance widths, so line breaks and
 * pagination match Word) where such fonts exist, and choose the closest-looking fallback otherwise.
 *
 * `metricCompatible: false` entries are flagged by the Compatibility Checker, because layout can drift
 * (most notably Aptos, Office's default since 2023, which has no metric-compatible open equivalent).
 */

export type GenericFamily = 'sans-serif' | 'serif' | 'monospace' | 'cursive' | 'fantasy' | 'system-ui';

export interface FontSubstitution {
  /** The Office font name (as written in documents). */
  office: string;
  /** The bundled or widely-available font used in its place. */
  substitute: string;
  /** True when `substitute` shares the original's glyph metrics. */
  metricCompatible: boolean;
  /** True when `substitute` ships with Kiln (`packages/tokens/fonts`). */
  bundled: boolean;
  generic: GenericFamily;
  note?: string;
}

/** Fonts Kiln bundles (see `@kiln/tokens` DOCUMENT_FONTS) plus the UI font used for Aptos. */
export const BUNDLED_FONTS = [
  'Carlito',
  'Caladea',
  'Liberation Sans',
  'Liberation Serif',
  'Liberation Mono',
  'Inter Variable',
] as const;

const sub = (
  office: string,
  substitute: string,
  metricCompatible: boolean,
  generic: GenericFamily,
  note?: string,
): FontSubstitution => ({
  office,
  substitute,
  metricCompatible,
  bundled: (BUNDLED_FONTS as readonly string[]).includes(substitute),
  generic,
  ...(note ? { note } : {}),
});

/** The substitution table. Keys are matched case-insensitively. */
export const FONT_SUBSTITUTIONS: readonly FontSubstitution[] = [
  // Metric-compatible (layout-identical) replacements.
  sub('Calibri', 'Carlito', true, 'sans-serif'),
  sub('Cambria', 'Caladea', true, 'serif'),
  sub('Arial', 'Liberation Sans', true, 'sans-serif'),
  sub('Helvetica', 'Liberation Sans', true, 'sans-serif'),
  sub('Helvetica Neue', 'Liberation Sans', false, 'sans-serif'),
  sub('Times New Roman', 'Liberation Serif', true, 'serif'),
  sub('Times', 'Liberation Serif', true, 'serif'),
  sub('Courier New', 'Liberation Mono', true, 'monospace'),
  sub('Courier', 'Liberation Mono', true, 'monospace'),
  // Visual fallbacks (metrics differ).
  sub('Calibri Light', 'Carlito', false, 'sans-serif', 'No light weight bundled; Carlito Regular is used.'),
  sub('Cambria Math', 'Caladea', false, 'serif'),
  sub(
    'Aptos',
    'Inter Variable',
    false,
    'sans-serif',
    'Office 2023+ default; no metric-compatible open font exists.',
  ),
  sub('Aptos Display', 'Inter Variable', false, 'sans-serif', 'Office 2023+ heading font.'),
  sub('Aptos Narrow', 'Inter Variable', false, 'sans-serif'),
  sub('Segoe UI', 'Inter Variable', false, 'sans-serif'),
  sub('Segoe UI Light', 'Inter Variable', false, 'sans-serif'),
  sub('Segoe UI Semibold', 'Inter Variable', false, 'sans-serif'),
  sub('Tahoma', 'Liberation Sans', false, 'sans-serif'),
  sub('Verdana', 'Liberation Sans', false, 'sans-serif'),
  sub('Trebuchet MS', 'Liberation Sans', false, 'sans-serif'),
  sub('Arial Narrow', 'Liberation Sans', false, 'sans-serif'),
  sub('Arial Black', 'Liberation Sans', false, 'sans-serif'),
  sub('Century Gothic', 'Liberation Sans', false, 'sans-serif'),
  sub('Franklin Gothic Book', 'Liberation Sans', false, 'sans-serif'),
  sub('Gill Sans MT', 'Liberation Sans', false, 'sans-serif'),
  sub('Corbel', 'Carlito', false, 'sans-serif'),
  sub('Candara', 'Carlito', false, 'sans-serif'),
  sub('Lucida Sans Unicode', 'Liberation Sans', false, 'sans-serif'),
  sub('MS Sans Serif', 'Liberation Sans', false, 'sans-serif'),
  sub('Georgia', 'Liberation Serif', false, 'serif'),
  sub('Garamond', 'Liberation Serif', false, 'serif'),
  sub('Book Antiqua', 'Liberation Serif', false, 'serif'),
  sub('Palatino Linotype', 'Liberation Serif', false, 'serif'),
  sub('Constantia', 'Caladea', false, 'serif'),
  sub('Century Schoolbook', 'Liberation Serif', false, 'serif'),
  sub('Bookman Old Style', 'Liberation Serif', false, 'serif'),
  sub('MS Serif', 'Liberation Serif', false, 'serif'),
  sub('Consolas', 'Liberation Mono', false, 'monospace'),
  sub('Lucida Console', 'Liberation Mono', false, 'monospace'),
  sub('Cascadia Code', 'Liberation Mono', false, 'monospace'),
  sub('Cascadia Mono', 'Liberation Mono', false, 'monospace'),
  sub('Comic Sans MS', 'Comic Neue', false, 'cursive'),
  sub('Segoe Print', 'Comic Neue', false, 'cursive'),
  sub('Brush Script MT', 'cursive', false, 'cursive'),
  sub('Impact', 'Liberation Sans', false, 'fantasy'),
];

const BY_NAME = new Map(FONT_SUBSTITUTIONS.map((s) => [s.office.toLowerCase(), s]));

/** Symbol fonts render through Unicode mapping (see `bulletDisplayText`), not substitution. */
const SYMBOL_FONTS = new Set(['symbol', 'wingdings', 'wingdings 2', 'wingdings 3', 'webdings', 'marlett']);

/** The substitution for an Office font name, if one is defined. */
export function substituteFont(name: string): FontSubstitution | undefined {
  return BY_NAME.get(name.trim().toLowerCase());
}

/** True when the font is a symbol/dingbat font. */
export function isSymbolFont(name: string): boolean {
  return SYMBOL_FONTS.has(name.trim().toLowerCase());
}

const SERIF_HINT =
  /(serif|roman|times|garamond|georgia|book|antiqua|palatino|mincho|song|ming|batang|cambria|baskerville|bodoni|caslon)/i;
const MONO_HINT = /(mono|courier|consol|code|typewriter|fixed)/i;
const SANS_HINT = /(sans|gothic|arial|helvet|grotesk)/i;

/** Guesses the CSS generic family for an unknown font name. */
export function genericFamilyFor(name: string): GenericFamily {
  const known = substituteFont(name);
  if (known) return known.generic;
  if (MONO_HINT.test(name)) return 'monospace';
  if (SANS_HINT.test(name)) return 'sans-serif';
  if (SERIF_HINT.test(name)) return 'serif';
  return 'sans-serif';
}

const quote = (family: string) =>
  /^[a-z-]+$/i.test(family) && !family.includes(' ') ? family : `"${family.replace(/"/g, '\\"')}"`;
const GENERICS = new Set<string>(['sans-serif', 'serif', 'monospace', 'cursive', 'fantasy', 'system-ui']);

export interface FontStackOptions {
  /**
   * Put the original Office font first so an installed copy (e.g. Calibri on Windows) is used. Off by
   * default: the bundled metric-compatible font is preferred so layout is identical on every OS.
   */
  preferInstalled?: boolean;
}

/**
 * A CSS `font-family` value for an Office font name: bundled substitute, original name, generic.
 * `fontStack('Calibri')` → `Carlito, Calibri, sans-serif`.
 */
export function fontStack(name: string, options: FontStackOptions = {}): string {
  const trimmed = name.trim();
  if (!trimmed) return 'sans-serif';
  const s = substituteFont(trimmed);
  const generic = genericFamilyFor(trimmed);
  const families: string[] = [];
  const add = (f: string) => {
    if (!families.some((x) => x.toLowerCase() === f.toLowerCase())) families.push(f);
  };
  if (s && !GENERICS.has(s.substitute)) {
    if (options.preferInstalled) {
      add(trimmed);
      add(s.substitute);
    } else {
      add(s.substitute);
      add(trimmed);
    }
  } else {
    add(trimmed);
  }
  return [...families.map(quote), generic].join(', ');
}

/** Font names in a document that have no metric-compatible bundled substitute (Compatibility Checker). */
export function fontsWithLayoutRisk(names: Iterable<string>): string[] {
  const out = new Set<string>();
  for (const n of names) {
    if (!n || isSymbolFont(n)) continue;
    const s = substituteFont(n);
    if (s?.metricCompatible) continue;
    if ((BUNDLED_FONTS as readonly string[]).some((b) => b.toLowerCase() === n.toLowerCase())) continue;
    out.add(n);
  }
  return [...out].sort();
}
