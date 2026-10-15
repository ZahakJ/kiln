/**
 * Colour helpers for the Office-style colour picker: theme colour tint/shade rows, standard colours,
 * and the persisted "Recent Colors" list.
 */

export type ThemeColorSlot =
  | 'bg1'
  | 'tx1'
  | 'bg2'
  | 'tx2'
  | 'accent1'
  | 'accent2'
  | 'accent3'
  | 'accent4'
  | 'accent5'
  | 'accent6';

export interface ThemeColorDef {
  slot: ThemeColorSlot;
  /** e.g. "Blue, Accent 1". */
  name: string;
  hex: string;
}

/** The default Office theme ("Office", 2013–2022), used until a document supplies its own. */
export const OFFICE_THEME_COLORS: readonly ThemeColorDef[] = [
  { slot: 'bg1', name: 'White, Background 1', hex: '#FFFFFF' },
  { slot: 'tx1', name: 'Black, Text 1', hex: '#000000' },
  { slot: 'bg2', name: 'Light Gray, Background 2', hex: '#E7E6E6' },
  { slot: 'tx2', name: 'Blue-Gray, Text 2', hex: '#44546A' },
  { slot: 'accent1', name: 'Blue, Accent 1', hex: '#4472C4' },
  { slot: 'accent2', name: 'Orange, Accent 2', hex: '#ED7D31' },
  { slot: 'accent3', name: 'Gray, Accent 3', hex: '#A5A5A5' },
  { slot: 'accent4', name: 'Gold, Accent 4', hex: '#FFC000' },
  { slot: 'accent5', name: 'Blue, Accent 5', hex: '#5B9BD5' },
  { slot: 'accent6', name: 'Green, Accent 6', hex: '#70AD47' },
];

export const STANDARD_COLORS: ReadonlyArray<{ name: string; hex: string }> = [
  { name: 'Dark Red', hex: '#C00000' },
  { name: 'Red', hex: '#FF0000' },
  { name: 'Orange', hex: '#FFC000' },
  { name: 'Yellow', hex: '#FFFF00' },
  { name: 'Light Green', hex: '#92D050' },
  { name: 'Green', hex: '#00B050' },
  { name: 'Light Blue', hex: '#00B0F0' },
  { name: 'Blue', hex: '#0070C0' },
  { name: 'Dark Blue', hex: '#002060' },
  { name: 'Purple', hex: '#7030A0' },
];

export type ColorChoice =
  | { type: 'auto' }
  | { type: 'none' }
  | { type: 'rgb'; hex: string }
  /** `tint` in −1…1: negative = darker by |tint|, positive = lighter by tint (OOXML lumMod/lumOff). */
  | { type: 'theme'; slot: ThemeColorSlot; tint: number; hex: string };

export function normalizeHex(input: string): string | null {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(input.trim());
  if (!m?.[1]) return null;
  const h = m[1].length === 3 ? [...m[1]].map((c) => c + c).join('') : m[1];
  return `#${h.toUpperCase()}`;
}

function hexToHsl(hex: string): [number, number, number] {
  const v = Number.parseInt(hex.slice(1), 16);
  const r = ((v >> 16) & 255) / 255;
  const g = ((v >> 8) & 255) / 255;
  const b = (v & 255) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h: number;
  if (max === r) h = (g - b) / d + (g < b ? 6 : 0);
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return [h / 6, s, l];
}

function hslToHex(h: number, s: number, l: number): string {
  const f = (n: number) => {
    const k = (n + h * 12) % 12;
    const a = s * Math.min(l, 1 - l);
    const c = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(c * 255)
      .toString(16)
      .padStart(2, '0');
  };
  return `#${f(0)}${f(8)}${f(4)}`.toUpperCase();
}

/** Applies an Office tint (−1…1) in HSL luminance space, like DrawingML lumMod/lumOff. */
export function applyTint(hex: string, tint: number): string {
  const [h, s, l] = hexToHsl(hex);
  const nl = tint < 0 ? l * (1 + tint) : l + (1 - l) * tint;
  return hslToHex(h, s, Math.min(1, Math.max(0, nl)));
}

/**
 * The five tint/shade variants Office shows under a theme colour, top to bottom, chosen by the base
 * colour's luminance (white gets darker steps, black lighter, mid tones 80/60/40 % lighter, 25/50 % darker).
 */
export function themeVariants(hex: string): number[] {
  const l = hexToHsl(hex)[2];
  if (l >= 0.999) return [-0.05, -0.15, -0.25, -0.35, -0.5];
  if (l <= 0.001) return [0.5, 0.35, 0.25, 0.15, 0.05];
  if (l < 0.2) return [0.9, 0.75, 0.5, 0.25, 0.1];
  if (l > 0.8) return [-0.1, -0.25, -0.5, -0.75, -0.9];
  return [0.8, 0.6, 0.4, -0.25, -0.5];
}

export function tintLabel(tint: number): string {
  const pct = Math.round(Math.abs(tint) * 100);
  return tint < 0 ? `Darker ${pct}%` : `Lighter ${pct}%`;
}

// Recent colours ------------------------------------------------------------------------------------

export const RECENT_COLORS_KEY = 'officina.recentColors';
const MAX_RECENT = 10;
const listeners = new Set<() => void>();
let recentCache: string[] | null = null;

function readRecent(): string[] {
  try {
    const raw = globalThis.localStorage?.getItem(RECENT_COLORS_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed)
      ? parsed.map((x) => normalizeHex(String(x))).filter((x): x is string => !!x)
      : [];
  } catch {
    return [];
  }
}

export function getRecentColors(): string[] {
  recentCache ??= readRecent();
  return recentCache;
}

/** Adds a custom colour to the front of the shared, persisted recent list (max 10, no duplicates). */
export function addRecentColor(hex: string): void {
  const h = normalizeHex(hex);
  if (!h) return;
  recentCache = [h, ...getRecentColors().filter((c) => c !== h)].slice(0, MAX_RECENT);
  try {
    globalThis.localStorage?.setItem(RECENT_COLORS_KEY, JSON.stringify(recentCache));
  } catch {
    // Storage unavailable: keep the in-memory list.
  }
  for (const l of listeners) l();
}

export function subscribeRecentColors(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Test helper: forget the in-memory cache. */
export function resetRecentColorsCache(): void {
  recentCache = null;
}
