/**
 * Screenshot conventions (PLAN.md §5): `e2e/screenshots/<spec>/<name>-<theme>-<viewport>.png`, with
 * the themes `light | dark | hc` and the viewports `desktop` (1440 × 900), `phone` (412 × 915, Pixel 7)
 * and `tablet` (1024 × 1366). Device scale 1 in CI, 2 for documentation media.
 */
import { join } from 'node:path';
import { REPO_ROOT } from '../paths.ts';

export type Theme = 'light' | 'dark' | 'hc';
export type ViewportName = 'desktop' | 'phone' | 'tablet';

export const THEMES: readonly Theme[] = ['light', 'dark', 'hc'];

/** CSS pixel sizes of the named viewports. */
export const VIEWPORTS: Readonly<Record<ViewportName, { width: number; height: number }>> = {
  desktop: { width: 1440, height: 900 },
  phone: { width: 412, height: 915 },
  tablet: { width: 1024, height: 1366 },
};

/** Root of the screenshot tree. */
export const SCREENSHOTS_DIR = join(REPO_ROOT, 'e2e', 'screenshots');

const SLUG = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

/** Lower-case words joined by single hyphens (`Ribbon: Home tab` → `ribbon-home-tab`). */
export function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

/** The file name of a screenshot: `<name>-<theme>-<viewport>.png`. */
export function screenshotName(name: string, theme: Theme, viewport: ViewportName): string {
  const slug = slugify(name);
  if (!SLUG.test(slug)) throw new Error(`screenshot name '${name}' has no letters or digits`);
  return `${slug}-${theme}-${viewport}.png`;
}

/** The path of a screenshot of `spec` (a spec file name with or without `.spec.ts`). */
export function screenshotPath(
  spec: string,
  name: string,
  theme: Theme,
  viewport: ViewportName,
  root: string = SCREENSHOTS_DIR,
): string {
  const specSlug = slugify(spec.replace(/\.spec\.ts$/, '').replace(/^.*\//, ''));
  if (!SLUG.test(specSlug)) throw new Error(`spec name '${spec}' has no letters or digits`);
  return join(root, specSlug, screenshotName(name, theme, viewport));
}

/** Parses a screenshot file name back into its parts (or `null` when it does not follow the convention). */
export function parseScreenshotName(
  file: string,
): { name: string; theme: Theme; viewport: ViewportName } | null {
  const m = /^([a-z0-9]+(?:-[a-z0-9]+)*)-(light|dark|hc)-(desktop|phone|tablet)\.png$/.exec(file);
  if (!m) return null;
  return { name: m[1] as string, theme: m[2] as Theme, viewport: m[3] as ViewportName };
}
