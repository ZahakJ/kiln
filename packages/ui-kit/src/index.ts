/**
 * Public entry point of @officina/ui-kit: the Horizon UI primitives (PLAN.md §1.2, §3.10), icons
 * (Lucide + Officina Glyphs) and theme bindings. Import `@officina/tokens/index.css` once in the app
 * for fonts and tokens; primitive styles are bundled with the components.
 */
export const PACKAGE_NAME = '@officina/ui-kit' as const;

export * from './icons/index.ts';
export * from './primitives/index.ts';
export * from './theme/index.ts';
