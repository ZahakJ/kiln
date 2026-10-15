/**
 * "Horizon" design tokens (PLAN.md §1.2) — the single source of truth.
 *
 * `tokens.css` and `theme.css` (the Tailwind 4 `@theme` block) are generated from this file by
 * `pnpm --filter @officina/tokens generate`; a unit test fails when they drift.
 */

/** Themes that can be rendered. `hc` is the high-contrast theme. */
export const THEMES = ['light', 'dark', 'hc'] as const;
export type ResolvedTheme = (typeof THEMES)[number];

/** What the user picks; `system` follows `prefers-color-scheme`. */
export const THEME_PREFERENCES = ['system', 'light', 'dark', 'hc'] as const;
export type ThemePreference = (typeof THEME_PREFERENCES)[number];

/** Apps whose accent can be activated with `data-app` (PLAN.md §1.1). */
export const APPS = ['write', 'grid', 'deck', 'notes', 'hub'] as const;
export type AppId = (typeof APPS)[number];

/** Per-theme colour tokens. Keys map to `--of-<key>`. */
export interface ColorTokens {
  /** Area behind pages/sheets/slides. */
  'bg-canvas': string;
  /** Ribbon, panes, dialogs, menus. */
  'bg-surface': string;
  /** Ribbon tab strip, status bar. */
  'bg-surface-2': string;
  /** Sunken areas: wells, track of sliders, gallery background. */
  'bg-surface-3': string;
  /** Hover wash on any surface (translucent so it works on every surface). */
  'bg-hover': string;
  /** Pressed wash. */
  'bg-pressed': string;
  /** Checked toggle/selected item wash (neutral). */
  'bg-checked': string;
  /** Text inputs. */
  'bg-input': string;
  /** Tooltip background / text. */
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
  /** Keyboard focus ring colour (defaults to the active accent in light/dark). */
  'focus-ring': string;
  /** Text/icon colour on a filled accent background. */
  'on-accent': string;
  /** App accents (used for tab underlines, focus, icons). */
  'accent-write': string;
  'accent-grid': string;
  'accent-deck': string;
  'accent-notes': string;
  'accent-hub': string;
  /** Accent variants guaranteed ≥ 4.5:1 against surfaces and `on-accent` (text, filled buttons). */
  'accent-strong-write': string;
  'accent-strong-grid': string;
  'accent-strong-deck': string;
  'accent-strong-notes': string;
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
  /** Selection fill opacity of the accent (`--of-selection` = accent at this percentage). */
  'selection-alpha': string;
  /** Box shadows (theme-dependent strength). */
  'shadow-1': string;
  'shadow-2': string;
  'shadow-3': string;
}

export type ColorTokenName = keyof ColorTokens;

export const colors: Record<ResolvedTheme, ColorTokens> = {
  light: {
    'bg-canvas': '#EEF1F6',
    'bg-surface': '#FFFFFF',
    'bg-surface-2': '#F6F8FB',
    'bg-surface-3': '#ECEFF4',
    'bg-hover': 'rgb(27 31 39 / 0.06)',
    'bg-pressed': 'rgb(27 31 39 / 0.11)',
    'bg-checked': 'rgb(27 31 39 / 0.09)',
    'bg-input': '#FFFFFF',
    'bg-tooltip': '#1B1F27',
    'fg-tooltip': '#F6F8FB',
    overlay: 'rgb(15 18 25 / 0.36)',
    fg: '#1B1F27',
    'fg-muted': '#5A6170',
    'fg-disabled': '#A1A7B3',
    border: '#D8DDE6',
    'border-strong': '#B4BBC8',
    'focus-ring': 'var(--of-accent)',
    'on-accent': '#FFFFFF',
    'accent-write': '#2B5FD9',
    'accent-grid': '#1F8A5B',
    'accent-deck': '#D9552B',
    'accent-notes': '#7A3FD1',
    'accent-hub': '#2B5FD9',
    'accent-strong-write': '#2B5FD9',
    'accent-strong-grid': '#1A7A50',
    'accent-strong-deck': '#B8461F',
    'accent-strong-notes': '#7A3FD1',
    'accent-strong-hub': '#2B5FD9',
    danger: '#C8352B',
    warning: '#B7791F',
    success: '#1F8A5B',
    info: '#2B5FD9',
    'danger-subtle': '#FCECEA',
    'warning-subtle': '#FBF3E3',
    'success-subtle': '#E7F5EE',
    'info-subtle': '#EAF0FC',
    scrollbar: 'rgb(27 31 39 / 0.22)',
    'scrollbar-hover': 'rgb(27 31 39 / 0.38)',
    'selection-alpha': '18%',
    'shadow-1': '0 1px 2px rgb(16 24 40 / 0.06), 0 1px 3px rgb(16 24 40 / 0.08)',
    'shadow-2': '0 4px 12px rgb(16 24 40 / 0.1), 0 2px 4px rgb(16 24 40 / 0.05)',
    'shadow-3': '0 16px 40px rgb(16 24 40 / 0.16), 0 4px 10px rgb(16 24 40 / 0.06)',
  },
  dark: {
    'bg-canvas': '#15171C',
    'bg-surface': '#1E2127',
    'bg-surface-2': '#262A31',
    'bg-surface-3': '#2C3039',
    'bg-hover': 'rgb(232 234 238 / 0.07)',
    'bg-pressed': 'rgb(232 234 238 / 0.12)',
    'bg-checked': 'rgb(232 234 238 / 0.1)',
    'bg-input': '#17191E',
    'bg-tooltip': '#E8EAEE',
    'fg-tooltip': '#15171C',
    overlay: 'rgb(0 0 0 / 0.55)',
    fg: '#E8EAEE',
    'fg-muted': '#9AA1AE',
    'fg-disabled': '#5D6470',
    border: '#333843',
    'border-strong': '#4A515E',
    'focus-ring': 'var(--of-accent)',
    'on-accent': '#0E1014',
    'accent-write': '#6B8FF0',
    'accent-grid': '#4FC08D',
    'accent-deck': '#F2825E',
    'accent-notes': '#A97CF0',
    'accent-hub': '#6B8FF0',
    'accent-strong-write': '#7C9CF2',
    'accent-strong-grid': '#4FC08D',
    'accent-strong-deck': '#F2825E',
    'accent-strong-notes': '#B38AF2',
    'accent-strong-hub': '#7C9CF2',
    danger: '#F07167',
    warning: '#E0A52F',
    success: '#4FC08D',
    info: '#6B8FF0',
    'danger-subtle': '#3A2225',
    'warning-subtle': '#372D1A',
    'success-subtle': '#1B3329',
    'info-subtle': '#1F2A44',
    scrollbar: 'rgb(232 234 238 / 0.2)',
    'scrollbar-hover': 'rgb(232 234 238 / 0.36)',
    'selection-alpha': '28%',
    'shadow-1': '0 1px 2px rgb(0 0 0 / 0.3), 0 1px 3px rgb(0 0 0 / 0.2)',
    'shadow-2': '0 6px 16px rgb(0 0 0 / 0.38), 0 2px 4px rgb(0 0 0 / 0.2)',
    'shadow-3': '0 18px 48px rgb(0 0 0 / 0.5), 0 4px 12px rgb(0 0 0 / 0.3)',
  },
  hc: {
    'bg-canvas': '#000000',
    'bg-surface': '#000000',
    'bg-surface-2': '#0B0B0B',
    'bg-surface-3': '#161616',
    'bg-hover': 'rgb(255 255 255 / 0.18)',
    'bg-pressed': 'rgb(255 255 255 / 0.3)',
    'bg-checked': 'rgb(255 255 255 / 0.24)',
    'bg-input': '#000000',
    'bg-tooltip': '#000000',
    'fg-tooltip': '#FFFFFF',
    overlay: 'rgb(0 0 0 / 0.75)',
    fg: '#FFFFFF',
    'fg-muted': '#E2E2E2',
    'fg-disabled': '#9C9C9C',
    border: '#FFFFFF',
    'border-strong': '#FFFFFF',
    'focus-ring': '#FFD400',
    'on-accent': '#000000',
    'accent-write': '#8AB4FF',
    'accent-grid': '#5EE6A0',
    'accent-deck': '#FF9E7A',
    'accent-notes': '#CDA6FF',
    'accent-hub': '#8AB4FF',
    'accent-strong-write': '#8AB4FF',
    'accent-strong-grid': '#5EE6A0',
    'accent-strong-deck': '#FF9E7A',
    'accent-strong-notes': '#CDA6FF',
    'accent-strong-hub': '#8AB4FF',
    danger: '#FF7A70',
    warning: '#FFD400',
    success: '#5EE6A0',
    info: '#8AB4FF',
    'danger-subtle': '#000000',
    'warning-subtle': '#000000',
    'success-subtle': '#000000',
    'info-subtle': '#000000',
    scrollbar: '#FFFFFF',
    'scrollbar-hover': '#FFD400',
    'selection-alpha': '40%',
    'shadow-1': 'none',
    'shadow-2': '0 0 0 1px #FFFFFF',
    'shadow-3': '0 0 0 2px #FFFFFF',
  },
};

/** Theme-independent tokens. Keys map to `--of-<key>`. */
export const scale = {
  // 4 px base grid.
  'space-0': '0px',
  'space-0-5': '2px',
  'space-1': '4px',
  'space-1-5': '6px',
  'space-2': '8px',
  'space-3': '12px',
  'space-4': '16px',
  'space-5': '20px',
  'space-6': '24px',
  'space-8': '32px',
  'space-10': '40px',
  'space-12': '48px',
  // Radii: controls / menus & cards / dialogs.
  'radius-control': '4px',
  'radius-menu': '6px',
  'radius-dialog': '10px',
  'radius-full': '9999px',
  // Type.
  'font-ui': "'Inter Variable', 'Inter', ui-sans-serif, system-ui, -apple-system, 'Segoe UI', sans-serif",
  'font-mono': "'JetBrains Mono', ui-monospace, 'Cascadia Mono', 'SFMono-Regular', Menlo, monospace",
  'font-doc-sans': "'Liberation Sans', Arial, sans-serif",
  'font-doc-serif': "'Liberation Serif', 'Times New Roman', serif",
  'font-doc-mono': "'Liberation Mono', 'Courier New', monospace",
  'font-doc-calibri': 'Carlito, Calibri, sans-serif',
  'font-doc-cambria': 'Caladea, Cambria, serif',
  'text-xs': '11px',
  'text-sm': '12px',
  'text-base': '13px',
  'text-md': '15px',
  'text-lg': '18px',
  'text-xl': '24px',
  'leading-tight': '1.25',
  'leading-normal': '1.45',
  'weight-regular': '400',
  'weight-medium': '500',
  'weight-semibold': '600',
  // Controls and icons.
  'control-h-sm': '24px',
  'control-h': '28px',
  'control-h-lg': '32px',
  'icon-sm': '16px',
  'icon-md': '20px',
  'icon-lg': '32px',
  'border-width': '1px',
  'focus-width': '2px',
  // Motion.
  'duration-fast': '80ms',
  'duration-menu': '120ms',
  'duration-pane': '200ms',
  'ease-out': 'cubic-bezier(0.16, 1, 0.3, 1)',
  'ease-in-out': 'cubic-bezier(0.65, 0, 0.35, 1)',
  // Stacking.
  'z-ribbon': '10',
  'z-pane': '20',
  'z-menu': '1000',
  'z-dialog': '1100',
  'z-popover': '1200',
  'z-tooltip': '1300',
  'z-toast': '1400',
} as const;

export type ScaleTokenName = keyof typeof scale;

/**
 * Tokens derived from the active app accent and theme. They are declared on every element carrying
 * `data-app` or `data-theme`, so nested scopes (e.g. app cards in the Hub) get the right colours.
 */
export const derived = {
  selection: 'color-mix(in srgb, var(--of-accent) var(--of-selection-alpha), transparent)',
  'accent-subtle': 'color-mix(in srgb, var(--of-accent) 12%, transparent)',
  'accent-hover': 'color-mix(in srgb, var(--of-accent-strong), var(--of-fg) 12%)',
  'accent-pressed': 'color-mix(in srgb, var(--of-accent-strong), var(--of-fg) 22%)',
} as const;

export type DerivedTokenName = keyof typeof derived;

export type TokenName = ColorTokenName | ScaleTokenName | DerivedTokenName | 'accent' | 'accent-strong';

/** `var(--of-<name>)` with type-checked names, for inline styles. */
export function cssVar(name: TokenName): string {
  return `var(--of-${name})`;
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
