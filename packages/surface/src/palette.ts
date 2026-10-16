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
};

/** The palette of a theme name. */
export function paletteFor(theme: 'light' | 'dark'): Palette {
  return theme === 'dark' ? DARK : LIGHT;
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
