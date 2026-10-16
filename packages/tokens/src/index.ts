/**
 * Public entry point of @kiln/tokens: typed "Horizon" design tokens (PLAN.md §1.2) and the
 * framework-free theme runtime. CSS lives next to this package's package.json:
 * `@kiln/tokens/index.css` (fonts + tokens), `tokens.css`, `fonts.css`, `theme.css` (Tailwind 4).
 */
export const PACKAGE_NAME = '@kiln/tokens' as const;

export { renderFontsCss, renderThemeCss, renderTokensCss } from './css.ts';
export * from './theme.ts';
export * from './tokens.ts';
