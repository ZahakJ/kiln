/**
 * Public entry point of @kiln/tokens: typed design tokens generated from the brand kit (PLAN.md §1.2) and the
 * framework-free theme runtime. CSS lives next to this package's package.json:
 * `@kiln/tokens/index.css` (fonts + tokens), `tokens.css`, `fonts.css`, `theme.css` (Tailwind 4).
 */
export const PACKAGE_NAME = '@kiln/tokens' as const;

export { renderFontsCss, renderThemeCss, renderTokensCss, themeTokens } from './css.ts';
export * from './kit.ts';
export * from './theme.ts';
export * from './tokens.ts';
