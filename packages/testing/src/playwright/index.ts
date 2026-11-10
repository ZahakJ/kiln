/**
 * Playwright helpers for the end-to-end tests (`@kiln/testing/playwright`): opening and typing into
 * a scratch document through the input proxy, IME composition, screenshot naming, the axe wrapper
 * and the layout scan. A separate entry point so unit tests of `@kiln/testing` never load
 * Playwright.
 */
export * from './a11y.ts';
export * from './layout-scan.ts';
export * from './scratch.ts';
export * from './screenshots.ts';
