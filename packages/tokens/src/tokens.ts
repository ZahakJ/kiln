/**
 * Kiln design tokens (PLAN.md §1.2), derived from the brand kit.
 *
 * The brand kit's `tokens.json` (imported verbatim into `brand/` by `tools/import-brand.sh`) is the
 * single source of truth. `pnpm --filter @kiln/tokens generate` parses it into `src/generated/kit.ts`
 * and renders `tokens.css`, `theme.css` and `fonts.css`; a unit test fails when any of them drifts.
 *
 * Two vocabularies are exposed, both as `--kiln-*` custom properties:
 *  - the kit's own names (`--kiln-bg`, `--kiln-panel`, `--kiln-line`, `--kiln-write-solid`, …) and its
 *    per-app variables (`--app`, `--app-solid`, `--app-on-solid`, `--app-text`, `--app-tint`), exactly as
 *    `brand/tokens.css` and the mockups use them;
 *  - semantic UI names (`--kiln-bg-surface`, `--kiln-border`, `--kiln-accent-strong`, …) that the chrome
 *    components are written against, each mapped onto a kit value.
 * No colour here is invented: every value is a kit colour, a mix of kit colours, or (for the
 * high-contrast theme, which the kit does not define; see docs/design/kit-gaps.md) a kit colour chosen
 * for maximum contrast.
 */
import { kit as generatedKit } from './generated/kit.ts';
import { KIT_PRODUCTS, type KitProductId, type KitThemeColors, type KitTokens } from './kit.ts';

/** Themes that can be rendered. `hc` is the high-contrast theme. */
export const THEMES = ['light', 'dark', 'hc'] as const;
export type ResolvedTheme = (typeof THEMES)[number];

/** What the user picks; `system` follows `prefers-color-scheme`. */
export const THEME_PREFERENCES = ['system', 'light', 'dark', 'hc'] as const;
export type ThemePreference = (typeof THEME_PREFERENCES)[number];

/**
 * Apps whose colour can be activated with `data-app` (PLAN.md §1.1). `home` is Kiln Home; `hub` is the
 * id the command registry uses for it and is accepted as an alias.
 */
export const APPS = ['write', 'grid', 'deck', 'notes', 'home', 'hub'] as const;
export type AppId = (typeof APPS)[number];

/** The kit product (glaze) behind an app id. */
export function productOf(app: AppId): KitProductId {
  return app === 'hub' ? 'home' : app;
}

/** The brand kit, typed (generated from `brand/tokens.json`). */
export const kit: KitTokens = generatedKit;

/** Per-theme semantic colour tokens. Keys map to `--kiln-<key>`. */
export interface ColorTokens {
  /** Area behind pages/sheets/slides. */
  'bg-canvas': string;
  /** Ribbon, panes, dialogs, menus. */
  'bg-surface': string;
  /** Window background: tab strip, title bar, status bar. */
  'bg-surface-2': string;
  /** Sunken areas: wells, segmented controls, search fields. */
  'bg-surface-3': string;
  /** Hover / pressed / checked washes (translucent, so they work on every surface). */
  'bg-hover': string;
  'bg-pressed': string;
  'bg-checked': string;
  /** Text inputs and raised controls. */
  'bg-input': string;
  /** Tooltip and Key Tip badges. */
  'bg-tooltip': string;
  'fg-tooltip': string;
  /** Modal backdrop. */
  overlay: string;
  /** Primary / secondary / disabled text. */
  fg: string;
  'fg-muted': string;
  'fg-disabled': string;
  /** Hairlines and stronger control outlines. */
  border: string;
  'border-strong': string;
  /** Text on a filled `accent-strong` background. */
  'on-accent': string;
  /** App colours for shapes (indicator bars, slider rails, selected outlines). */
  'accent-write': string;
  'accent-grid': string;
  'accent-deck': string;
  'accent-notes': string;
  'accent-home': string;
  'accent-hub': string;
  /** App colours tuned for text: ≥ 4.5:1 on every surface. */
  'accent-strong-write': string;
  'accent-strong-grid': string;
  'accent-strong-deck': string;
  'accent-strong-notes': string;
  'accent-strong-home': string;
  'accent-strong-hub': string;
  /** Status colours and their subtle backgrounds. */
  danger: string;
  warning: string;
  success: string;
  info: string;
  'danger-subtle': string;
  'warning-subtle': string;
  'success-subtle': string;
  'info-subtle': string;
  /** Scrollbar thumb. */
  scrollbar: string;
  'scrollbar-hover': string;
  /** Box shadows: controls, popovers/menus, dialogs. */
  'shadow-1': string;
  'shadow-2': string;
  'shadow-3': string;
}

export type ColorTokenName = keyof ColorTokens;

/** The kit's per-theme surface colours, plus the theme-dependent product text/tint and shadows. */
export interface ThemeBase extends KitThemeColors {
  productText: Record<KitProductId, string>;
  productTint: Record<KitProductId, string>;
  shadow: { '1': string; '2': string; page: string };
}

const mix = (color: string, pct: number, other = 'transparent') =>
  `color-mix(in srgb, ${color} ${pct}%, ${other})`;

const perProduct = (k: KitTokens, pick: (p: KitTokens['color']['product'][KitProductId]) => string) =>
  Object.fromEntries(KIT_PRODUCTS.map((p) => [p, pick(k.color.product[p])])) as Record<KitProductId, string>;

/**
 * The kit colours a theme renders with. Light and dark are the kit's own; high contrast is assembled
 * from kit colours (char ground, bone text, the fire flame for focus) because the kit has no HC set.
 */
export function themeBase(k: KitTokens, theme: ResolvedTheme): ThemeBase {
  const { light, dark, neutral, fire } = k.color;
  if (theme === 'light') {
    return {
      ...light,
      productText: perProduct(k, (p) => p.textLight),
      productTint: perProduct(k, (p) => p.tintLight),
      shadow: { ...k.shadow.light },
    };
  }
  const darkText = perProduct(k, (p) => p.textDark);
  const darkTint = perProduct(k, (p) => p.tintDark);
  if (theme === 'dark') {
    return { ...dark, productText: darkText, productTint: darkTint, shadow: { ...k.shadow.dark } };
  }
  return {
    bg: neutral.char,
    panel: neutral.char,
    raised: neutral.soot,
    sunken: neutral.char,
    page: neutral.soot,
    canvas: neutral.char,
    fg: neutral.bone,
    muted: dark.fg,
    faint: dark.muted,
    line: dark.muted,
    lineStrong: neutral.bone,
    ember: fire.flame,
    emberText: fire.flame,
    onEmber: neutral.char,
    selection: dark.selection,
    focus: fire.flame,
    danger: dark.danger,
    success: dark.success,
    warning: dark.warning,
    productText: { ...darkText, home: fire.flame },
    productTint: darkTint,
    shadow: { '1': 'none', '2': `0 0 0 1px ${neutral.bone}`, page: `0 0 0 1px ${neutral.bone}` },
  };
}

/** The kit's own variable names for one theme (`--kiln-bg`, `--kiln-write-text`, …). */
export function nativeThemeTokens(base: ThemeBase): Record<string, string> {
  const out: Record<string, string> = {
    bg: base.bg,
    panel: base.panel,
    raised: base.raised,
    sunken: base.sunken,
    page: base.page,
    canvas: base.canvas,
    fg: base.fg,
    muted: base.muted,
    faint: base.faint,
    line: base.line,
    'line-strong': base.lineStrong,
    ember: base.ember,
    'ember-text': base.emberText,
    'on-ember': base.onEmber,
    selection: base.selection,
    focus: base.focus,
    danger: base.danger,
    success: base.success,
    warning: base.warning,
  };
  for (const p of KIT_PRODUCTS) {
    out[`${p}-text`] = base.productText[p];
    out[`${p}-tint`] = base.productTint[p];
  }
  out['shadow-1'] = base.shadow['1'];
  out['shadow-2'] = base.shadow['2'];
  out['shadow-page'] = base.shadow.page;
  return out;
}

/** Semantic UI tokens for one theme, mapped onto the kit. */
export function semanticColors(k: KitTokens, theme: ResolvedTheme): ColorTokens {
  const b = themeBase(k, theme);
  const hc = theme === 'hc';
  const product = k.color.product;
  const fill = (p: KitProductId) => (hc ? b.productText[p] : p === 'home' ? b.ember : product[p].fill);
  const text = (p: KitProductId) => (p === 'home' ? b.emberText : b.productText[p]);
  const info = b.productText.write;
  const subtle = (c: string) => (hc ? b.panel : mix(c, 14, b.panel));
  return {
    'bg-canvas': b.canvas,
    'bg-surface': b.panel,
    'bg-surface-2': b.bg,
    'bg-surface-3': b.sunken,
    'bg-hover': mix(b.fg, hc ? 18 : 6),
    'bg-pressed': mix(b.fg, hc ? 30 : 11),
    'bg-checked': mix(b.fg, hc ? 24 : 9),
    'bg-input': b.raised,
    'bg-tooltip': theme === 'light' ? k.color.neutral.tenmoku : hc ? k.color.neutral.char : b.raised,
    'fg-tooltip': theme === 'light' ? k.color.neutral.bone : b.fg,
    overlay: mix(
      theme === 'light' ? k.color.neutral.soot : k.color.neutral.char,
      hc ? 80 : theme === 'light' ? 40 : 60,
    ),
    fg: b.fg,
    'fg-muted': b.muted,
    'fg-disabled': b.faint,
    border: b.line,
    'border-strong': b.lineStrong,
    'on-accent': b.onEmber,
    'accent-write': fill('write'),
    'accent-grid': fill('grid'),
    'accent-deck': fill('deck'),
    'accent-notes': fill('notes'),
    'accent-home': fill('home'),
    'accent-hub': fill('home'),
    'accent-strong-write': text('write'),
    'accent-strong-grid': text('grid'),
    'accent-strong-deck': text('deck'),
    'accent-strong-notes': text('notes'),
    'accent-strong-home': text('home'),
    'accent-strong-hub': text('home'),
    danger: b.danger,
    warning: b.warning,
    success: b.success,
    info,
    'danger-subtle': subtle(b.danger),
    'warning-subtle': subtle(b.warning),
    'success-subtle': subtle(b.success),
    'info-subtle': subtle(info),
    scrollbar: hc ? b.fg : mix(b.fg, 22),
    'scrollbar-hover': hc ? b.focus : mix(b.fg, 38),
    'shadow-1': b.shadow['1'],
    'shadow-2': b.shadow['2'],
    // The kit has two elevation shadows plus the page shadow; dialogs use the deepest non-glowing one.
    'shadow-3': theme === 'light' ? b.shadow.page : hc ? `0 0 0 2px ${b.fg}` : b.shadow['2'],
  };
}

/** Semantic colours per theme, from the committed kit. */
export const colors: Record<ResolvedTheme, ColorTokens> = {
  light: semanticColors(kit, 'light'),
  dark: semanticColors(kit, 'dark'),
  hc: semanticColors(kit, 'hc'),
};

const px = (n: number) => `${n}px`;

/** Theme-independent tokens (kit scales plus the semantic aliases). Keys map to `--kiln-<key>`. */
export function scaleTokens(k: KitTokens): Record<string, string> {
  const out: Record<string, string> = {};
  // Product glazes (fixed in every theme) and the fixed palette.
  for (const p of KIT_PRODUCTS) {
    const g = k.color.product[p];
    out[`${p}-fill`] = g.fill;
    out[`${p}-solid`] = g.solid;
    out[`${p}-on-solid`] = g.onSolid;
    out[`${p}-text-light`] = g.textLight;
    out[`${p}-text-dark`] = g.textDark;
    out[`${p}-tint-light`] = g.tintLight;
    out[`${p}-tint-dark`] = g.tintDark;
    out[`${p}-icon-top`] = g.icon[0] ?? g.fill;
    out[`${p}-icon-bottom`] = g.icon[g.icon.length - 1] ?? g.fill;
  }
  for (const [name, v] of Object.entries(k.color.fire)) out[`fire-${name}`] = v;
  for (const [name, v] of Object.entries(k.color.neutral)) out[name] = v;
  // Type.
  out['font-display'] = k.font.stack.display;
  out['font-ui'] = k.font.stack.ui;
  out['font-doc'] = 'var(--kiln-font-ui)';
  // The kit sets no monospace anywhere in the chrome (BRAND.md §4): code-ish fields use the UI face
  // with tabular figures.
  out['font-mono'] = 'var(--kiln-font-ui)';
  out['font-doc-sans'] = "'Liberation Sans', Arial, sans-serif";
  out['font-doc-serif'] = "'Liberation Serif', 'Times New Roman', serif";
  out['font-doc-mono'] = "'Liberation Mono', 'Courier New', monospace";
  out['font-doc-calibri'] = 'Carlito, Calibri, sans-serif';
  out['font-doc-cambria'] = 'Caladea, Cambria, serif';
  out['display-settings'] = Object.entries(k.font.display.settings)
    .filter(([axis]) => axis === 'SOFT' || axis === 'WONK')
    .map(([axis, v]) => `'${axis}' ${v}`)
    .join(', ');
  for (const [name, v] of Object.entries(k.font.size)) out[`text-${name}`] = px(v);
  out['leading-tight'] = '1.25';
  out['leading-normal'] = '1.4';
  out['weight-regular'] = '400';
  out['weight-medium'] = '500';
  out['weight-semibold'] = '600';
  out['weight-bold'] = '700';
  out['weight-heavy'] = '800';
  // Radii: the kit scale, and the semantic roles the chrome uses.
  for (const name of ['xs', 'sm', 'md', 'lg', 'xl', 'pill'] as const)
    out[`radius-${name}`] = px(k.radius[name]);
  out['radius-control'] = 'var(--kiln-radius-md)';
  out['radius-menu'] = 'var(--kiln-radius-lg)';
  out['radius-dialog'] = 'var(--kiln-radius-xl)';
  out['radius-full'] = 'var(--kiln-radius-pill)';
  // Space: the kit's 4 px grid plus the half steps the dense ribbon needs.
  out['space-0'] = '0px';
  out['space-0-5'] = '2px';
  for (const [name, v] of Object.entries(k.space)) out[`space-${name}`] = px(v);
  out['space-1-5'] = '6px';
  // Control metrics (from the mockups: 28 px icon buttons and fields, 30 px buttons, 32 px inputs).
  out['control-h-sm'] = '24px';
  out['control-h'] = '28px';
  out['button-h'] = '30px';
  out['control-h-lg'] = '32px';
  out['icon-sm'] = '16px';
  out['icon-md'] = '20px';
  out['icon-lg'] = '32px';
  out['border-width'] = '1px';
  out['focus-width'] = '2px';
  // Motion.
  out['motion-fast'] = k.motion.fast;
  out['motion-base'] = k.motion.base;
  out['motion-slow'] = k.motion.slow;
  out.ease = k.motion.ease;
  out['duration-fast'] = 'var(--kiln-motion-fast)';
  out['duration-menu'] = 'var(--kiln-motion-fast)';
  out['duration-pane'] = 'var(--kiln-motion-base)';
  out['duration-slow'] = 'var(--kiln-motion-slow)';
  out['ease-out'] = 'var(--kiln-ease)';
  out['ease-in-out'] = 'var(--kiln-ease)';
  // Stacking.
  out['z-ribbon'] = '10';
  out['z-pane'] = '20';
  out['z-menu'] = '1000';
  out['z-dialog'] = '1100';
  out['z-popover'] = '1200';
  out['z-tooltip'] = '1300';
  out['z-toast'] = '1400';
  return out;
}

export const scale = scaleTokens(kit);
export type ScaleTokenName = keyof typeof scale & string;

/**
 * Tokens derived from the active app and theme. They are declared on every element carrying
 * `data-app` or `data-theme`, so nested scopes (e.g. app cards on Home) get the right colours.
 */
export const derived = {
  accent: 'var(--app)',
  'accent-strong': 'var(--app-text)',
  'accent-subtle': 'var(--app-tint)',
  'accent-hover': 'color-mix(in srgb, var(--app-solid), var(--kiln-fg) 12%)',
  'accent-pressed': 'color-mix(in srgb, var(--app-solid), var(--kiln-fg) 22%)',
  // The focus ring follows the app colour (BRAND.md §5); high contrast overrides it with the flame.
  'focus-ring': 'var(--kiln-focus-override, var(--app-text))',
} as const;

export type DerivedTokenName = keyof typeof derived;

export type TokenName =
  | ColorTokenName
  | DerivedTokenName
  | 'selection'
  | 'bg'
  | 'panel'
  | 'raised'
  | 'sunken'
  | 'page'
  | 'canvas'
  | 'muted'
  | 'faint'
  | 'line'
  | 'line-strong'
  | 'ember'
  | 'ember-text'
  | 'on-ember'
  | 'shadow-page'
  | 'radius-control'
  | 'radius-menu'
  | 'radius-dialog'
  | 'font-ui'
  | 'font-display'
  | 'ease';

/** `var(--kiln-<name>)` with type-checked names, for inline styles. */
export function cssVar(name: TokenName): string {
  return `var(--kiln-${name})`;
}

/** Bundled document font families and their Office equivalents (metric-compatible, SIL OFL). */
export const DOCUMENT_FONTS = [
  { family: 'Liberation Sans', file: 'LiberationSans', replaces: 'Arial' },
  { family: 'Liberation Serif', file: 'LiberationSerif', replaces: 'Times New Roman' },
  { family: 'Liberation Mono', file: 'LiberationMono', replaces: 'Courier New' },
  { family: 'Carlito', file: 'Carlito', replaces: 'Calibri' },
  { family: 'Caladea', file: 'Caladea', replaces: 'Cambria' },
] as const;

export const FONT_STYLES = [
  { suffix: 'Regular', weight: 400, style: 'normal' },
  { suffix: 'Bold', weight: 700, style: 'normal' },
  { suffix: 'Italic', weight: 400, style: 'italic' },
  { suffix: 'BoldItalic', weight: 700, style: 'italic' },
] as const;

/**
 * The brand's UI fonts (variable TTFs from the kit, SIL OFL 1.1), served from `assets/fonts/brand/`.
 * `weights` is the variable weight range the file covers.
 */
export const BRAND_FONTS = [
  { family: 'Figtree', dir: 'Figtree', file: 'Figtree-VF.ttf', weights: [300, 900], role: 'interface' },
  { family: 'Fraunces', dir: 'Fraunces', file: 'Fraunces-VF.ttf', weights: [100, 900], role: 'display' },
] as const;

/** Parses `#RRGGBB` to linear-light relative luminance (WCAG 2.x). */
export function relativeLuminance(hex: string): number {
  const m = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!m?.[1]) throw new Error(`relativeLuminance: expected #RRGGBB, got ${hex}`);
  const v = Number.parseInt(m[1], 16);
  const channel = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel((v >> 16) & 255) + 0.7152 * channel((v >> 8) & 255) + 0.0722 * channel(v & 255);
}

/** WCAG contrast ratio between two `#RRGGBB` colours (1–21). */
export function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [relativeLuminance(a), relativeLuminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}
