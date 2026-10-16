/**
 * Theme font and colour resolution for WordprocessingML properties, using the DrawingML theme types of
 * `@kiln-ref/ooxml`. Word stores theme references (`w:asciiTheme="minorHAnsi"`,
 * `w:themeColor="accent1" w:themeShade="BF"`) alongside cached values; the theme reference wins.
 */
import {
  hexToRgba,
  hslToRgb,
  officeTheme,
  resolveColorHex,
  resolveThemeFont,
  rgbToHsl,
  type Theme,
} from '@kiln-ref/ooxml';
import type { Color, RunFonts, Shading, ThemeColorName } from './types.ts';

/** Word's default `w:clrSchemeMapping` (settings.xml). */
export const DEFAULT_CLR_SCHEME_MAPPING: Readonly<Record<string, string>> = {
  bg1: 'light1',
  t1: 'dark1',
  bg2: 'light2',
  t2: 'dark2',
};

const THEME_SLOT: Record<string, string> = {
  dark1: 'dk1',
  light1: 'lt1',
  dark2: 'dk2',
  light2: 'lt2',
  accent1: 'accent1',
  accent2: 'accent2',
  accent3: 'accent3',
  accent4: 'accent4',
  accent5: 'accent5',
  accent6: 'accent6',
  hyperlink: 'hlink',
  followedHyperlink: 'folHlink',
};

/** Maps a `w:themeColor` value to the DrawingML theme slot (`text1` → `dk1` via the mapping). */
export function themeSlotFor(
  name: ThemeColorName,
  mapping: Partial<Record<string, string>> = DEFAULT_CLR_SCHEME_MAPPING,
): string | undefined {
  const aliased: Record<string, string> = {
    text1: 't1',
    background1: 'bg1',
    text2: 't2',
    background2: 'bg2',
  };
  const key = aliased[name];
  const target = key ? (mapping[key] ?? DEFAULT_CLR_SCHEME_MAPPING[key]) : name;
  return target ? THEME_SLOT[target] : undefined;
}

/**
 * Applies Word's `w:themeTint`/`w:themeShade` (0–255) to a hex colour in HSL space, as Word does:
 * tint: L' = L·t + (1 − t); shade: L' = L·s, with channels truncated (Accent 1 `4472C4` + shade `BF` =
 * `2F5496`, + tint `33` = `D9E2F3`).
 */
export function applyTintShade(hex: string, tint?: number, shade?: number): string {
  if (tint === undefined && shade === undefined) return hex.toUpperCase();
  const { r, g, b } = hexToRgba(hex);
  const hsl = rgbToHsl(r, g, b);
  let l = hsl.l;
  if (tint !== undefined) l = l * (tint / 255) + (1 - tint / 255);
  if (shade !== undefined) l = l * (shade / 255);
  const [R, G, B] = hslToRgb(hsl.h, hsl.s, Math.max(0, Math.min(1, l)));
  return [R, G, B]
    .map((c) =>
      Math.min(255, Math.floor(c + 1e-6))
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')
    .toUpperCase();
}

export interface ThemeResolveOptions {
  theme?: Theme;
  /** `w:clrSchemeMapping` from settings. */
  colorMapping?: Partial<Record<string, string>>;
}

/**
 * Resolves a WordprocessingML colour to `RRGGBB` or `auto`. Theme references win over `val`; without a
 * theme the Office 2013–2022 theme is used.
 */
export function resolveDocColor(color: Color | undefined, opts: ThemeResolveOptions = {}): string {
  if (!color) return 'auto';
  if (color.themeColor && color.themeColor !== 'none') {
    const slot = themeSlotFor(color.themeColor, opts.colorMapping);
    const theme = opts.theme ?? defaultTheme();
    if (slot && slot in theme.colorScheme.colors) {
      const base = resolveColorHex(theme.colorScheme.colors[slot as keyof typeof theme.colorScheme.colors]);
      const computed = applyTintShade(base, color.themeTint, color.themeShade);
      // Word's own tint/shade arithmetic rounds slightly differently (Heading 3's cached 1F3763 vs the
      // exact 203864). When the cached value is within rounding distance, it is Word's rendering of
      // this very theme colour, so use it verbatim; a changed theme moves far beyond the tolerance.
      return nearHex(color.val, computed, 2) ? color.val.toUpperCase() : computed;
    }
  }
  return color.val === 'auto' ? 'auto' : color.val.toUpperCase();
}

function nearHex(a: string, b: string, tolerance: number): boolean {
  if (!/^[0-9a-f]{6}$/i.test(a) || !/^[0-9a-f]{6}$/i.test(b)) return false;
  for (let i = 0; i < 6; i += 2) {
    if (Math.abs(Number.parseInt(a.slice(i, i + 2), 16) - Number.parseInt(b.slice(i, i + 2), 16)) > tolerance)
      return false;
  }
  return true;
}

/** The fill colour of a shading (`w:shd/@w:fill` with theme fill), or undefined for none/auto. */
export function resolveShadingFill(
  shading: Shading | undefined,
  opts: ThemeResolveOptions = {},
): string | undefined {
  if (!shading?.fill) return undefined;
  const hex = resolveDocColor(shading.fill, opts);
  return hex === 'auto' ? undefined : hex;
}

let cachedTheme: Theme | undefined;
/** The default Office theme (cached). */
export function defaultTheme(): Theme {
  cachedTheme ??= officeTheme();
  return cachedTheme;
}

export interface ResolvedFonts {
  ascii: string;
  hAnsi: string;
  eastAsia: string;
  cs: string;
}

/** Resolves run fonts to concrete names: theme references win over explicit names. */
export function resolveRunFonts(fonts: RunFonts | undefined, theme: Theme = defaultTheme()): ResolvedFonts {
  const pick = (slot: 'ascii' | 'hAnsi' | 'eastAsia' | 'cs'): string => {
    const ref = fonts?.[`${slot}Theme`];
    if (ref) {
      const name = resolveThemeFont(ref, theme);
      if (name && name !== ref) return name;
      // Theme collections often leave ea/cs empty: fall back to the latin font of the same collection.
      if (!name || name === ref)
        return resolveThemeFont(ref.startsWith('major') ? 'majorHAnsi' : 'minorHAnsi', theme);
    }
    return fonts?.[slot] ?? '';
  };
  const ascii = pick('ascii');
  const hAnsi = pick('hAnsi') || ascii;
  return { ascii: ascii || hAnsi, hAnsi, eastAsia: pick('eastAsia') || ascii, cs: pick('cs') || ascii };
}
