/**
 * Theme palettes the painter resolves Scene colour roles against. The values are the brand kit's
 * and identical to `kiln_scene::Palette` (checked against `fixtures/golden/scene/palette.json`).
 */

import type { ColorRole, Paint, Palette } from '@kiln/protocol';

/** The light theme. */
export const LIGHT: Palette = {
  name: 'light',
  canvas: 0xf6efe6ff,
  page: 0xffffffff,
  ink: 0x2b1c14ff,
  selection: 0xffd9b8ff,
  caret: 0x3159c6ff,
  composition: 0x2b1c14ff,
  squiggle: 0xd93d12ff,
  guide: 0xcdb8a0ff,
  spark: 0xff6a26ff,
  selection_blend: 'Darken',
  page_shadow: [
    { dx: 0, dy: 1, blur: 2, spread: 0, rgba: 0x3c1e0a14 },
    { dx: 0, dy: 12, blur: 40, spread: 0, rgba: 0x3c1e0a1a },
  ],
  panel: 0xfffcf7ff,
  muted: 0x6b5547ff,
  line: 0xe4d6c4ff,
  gridline: 0xece2d6ff,
  app_fill: 0x3159c6ff,
  app_tint: 0xe7ecfaff,
  app_text: 0x2d4fb5ff,
};

/** The dark theme (tuned, not inverted; the page glows faintly ember). */
export const DARK: Palette = {
  name: 'dark',
  canvas: 0x17110eff,
  page: 0x221a15ff,
  ink: 0xf5ebddff,
  selection: 0x5a2a14ff,
  caret: 0x97aef4ff,
  composition: 0xf5ebddff,
  squiggle: 0xff8a7aff,
  guide: 0x4b3a2fff,
  spark: 0xff6a26ff,
  selection_blend: 'Lighten',
  page_shadow: [
    { dx: 0, dy: 30, blur: 80, spread: -20, rgba: 0xff6a2638 },
    { dx: 0, dy: 2, blur: 10, spread: 0, rgba: 0x00000080 },
  ],
  panel: 0x201813ff,
  muted: 0xb8a392ff,
  line: 0x372a22ff,
  gridline: 0x31251eff,
  app_fill: 0x3159c6ff,
  app_tint: 0x1c2440ff,
  app_text: 0x97aef4ff,
};

/** The apps whose colours a palette can carry (the brand kit's `--app-*`). */
export type PaletteApp = 'write' | 'grid' | 'deck' | 'notes' | 'home';

/** `[fill, tint light, tint dark, text light, text dark]` of every app (`kiln_scene::Palette::for_app`). */
const APP_COLORS: Readonly<Record<PaletteApp, readonly [number, number, number, number, number]>> = {
  write: [0x3159c6ff, 0xe7ecfaff, 0x1c2440ff, 0x2d4fb5ff, 0x97aef4ff],
  grid: [0x1d8a60ff, 0xe2f2eaff, 0x142c22ff, 0x17714eff, 0x6fd0a6ff],
  deck: [0xe05a27ff, 0xfbe8deff, 0x36190eff, 0xb2431aff, 0xf7976bff],
  notes: [0xe3a126ff, 0xfaf0d9ff, 0x30230cff, 0x87590aff, 0xf2c35cff],
  home: [0xc2481cff, 0xfbe8deff, 0x2a1a12ff, 0xb2431aff, 0xf7976bff],
};

const APP_PALETTES = new Map<string, Palette>();

/** A theme's palette with an app's accent, tint, text and caret colours (Write's by default). */
function withApp(base: Palette, app: PaletteApp): Palette {
  if (app === 'write') return base;
  const key = `${base.name}:${app}`;
  const cached = APP_PALETTES.get(key);
  if (cached) return cached;
  const [fill, tintL, tintD, textL, textD] = APP_COLORS[app];
  const dark = base.name === 'dark';
  const p: Palette = {
    ...base,
    app_fill: fill,
    app_tint: dark ? tintD : tintL,
    app_text: dark ? textD : textL,
    caret: dark ? textD : fill,
  };
  APP_PALETTES.set(key, p);
  return p;
}

/** The palette of a theme name, with an app's colours (Kiln Write's by default). */
export function paletteFor(theme: 'light' | 'dark', app: PaletteApp = 'write'): Palette {
  return withApp(theme === 'dark' ? DARK : LIGHT, app);
}

/** The colour of a role, `0xRRGGBBAA`. */
export function roleColor(palette: Palette, role: ColorRole): number {
  switch (role) {
    case 'Canvas':
      return palette.canvas;
    case 'Page':
      return palette.page;
    case 'Ink':
      return palette.ink;
    case 'Selection':
      return palette.selection;
    case 'Caret':
      return palette.caret;
    case 'Composition':
      return palette.composition;
    case 'Squiggle':
      return palette.squiggle;
    case 'Guide':
      return palette.guide;
    case 'Panel':
      return palette.panel;
    case 'Muted':
      return palette.muted;
    case 'Line':
      return palette.line;
    case 'Gridline':
      return palette.gridline;
    case 'AppFill':
      return palette.app_fill;
    case 'AppTint':
      return palette.app_tint;
    case 'AppText':
      return palette.app_text;
  }
}

/** The colour of a paint, `0xRRGGBBAA`. */
export function resolvePaint(palette: Palette, paint: Paint): number {
  return 'Rgba' in paint ? paint.Rgba >>> 0 : roleColor(palette, paint.Role);
}

/** `[r, g, b, a]` of `0xRRGGBBAA`. */
export function channels(rgba: number): [number, number, number, number] {
  const v = rgba >>> 0;
  return [(v >>> 24) & 0xff, (v >>> 16) & 0xff, (v >>> 8) & 0xff, v & 0xff];
}

/** A CSS colour for `0xRRGGBBAA`. */
export function cssColor(rgba: number): string {
  const [r, g, b, a] = channels(rgba);
  return a === 255 ? `rgb(${r} ${g} ${b})` : `rgb(${r} ${g} ${b} / ${a / 255})`;
}
