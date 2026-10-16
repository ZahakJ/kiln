/**
 * Kiln Glyphs — Office-specific command icons that Lucide lacks (PLAN.md §1.2).
 * 20×20 viewBox, 1.5 px strokes, `currentColor`. Each glyph is a named React component export;
 * `glyphs` maps export names to components and `glyphList()` lists them (galleries, tests, docs).
 */
import type { GlyphComponent } from './glyph.ts';
import * as set from './set.ts';

export type { GlyphComponent, GlyphPart, GlyphProps } from './glyph.ts';
export { glyph, glyphToSvg } from './glyph.ts';
export * from './set.ts';

/** Export name of every glyph, e.g. `'FormatPainter'`. */
export type GlyphName = {
  [K in keyof typeof set]: (typeof set)[K] extends GlyphComponent ? K : never;
}[keyof typeof set];

/** Every glyph by export name. */
export const glyphs = Object.freeze(
  Object.fromEntries(Object.entries(set).filter(([, v]) => typeof v === 'function' && 'glyphId' in v)),
) as Readonly<Record<GlyphName, GlyphComponent>>;

/** Every glyph as `[name, component]`, sorted by name. */
export function glyphList(): Array<[GlyphName, GlyphComponent]> {
  return (Object.entries(glyphs) as Array<[GlyphName, GlyphComponent]>).sort(([a], [b]) =>
    a.localeCompare(b),
  );
}

export function isGlyphName(name: string): name is GlyphName {
  return Object.hasOwn(glyphs, name);
}
