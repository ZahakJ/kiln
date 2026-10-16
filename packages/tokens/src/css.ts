/**
 * Renders `tokens.css` and `theme.css` from `tokens.ts`. Pure functions so tests can assert that the
 * committed CSS files are exactly what the TS tokens produce.
 */
import { APPS, type ColorTokens, colors, DOCUMENT_FONTS, derived, FONT_STYLES, scale } from './tokens.ts';

const HEADER = (what: string) =>
  `/*\n * ${what}\n * GENERATED from packages/tokens/src/tokens.ts by \`pnpm --filter @kiln/tokens generate\`.\n * Do not edit by hand.\n */\n`;

/** Matches Biome's CSS formatting (double quotes, lowercase hex) so generated files pass `biome check`. */
const normalize = (css: string) =>
  css.replace(/'/g, '"').replace(/#[0-9a-f]{3,8}\b/gi, (hex) => hex.toLowerCase());

const decls = (entries: Record<string, string>, indent = '  ') =>
  Object.entries(entries)
    .map(([k, v]) => `${indent}--kiln-${k}: ${v};`)
    .join('\n');

const themeBlock = (c: ColorTokens, scheme: 'light' | 'dark', indent = '  ') =>
  `${indent}color-scheme: ${scheme};\n${decls(c as unknown as Record<string, string>, indent)}`;

/** The full `tokens.css`: every `--kiln-*` custom property for every theme and app accent. */
export function renderTokensCss(): string {
  const parts: string[] = [HEADER('Kiln "Horizon" design tokens (PLAN.md §1.2).')];

  parts.push(
    `/* Theme-independent scale: spacing (4 px grid), radii, type, controls, motion, stacking. */\n:root {\n${decls(scale)}\n}\n`,
  );

  parts.push(
    `/* Light theme (default). */\n:root,\n[data-theme="light"] {\n${themeBlock(colors.light, 'light')}\n}\n`,
  );

  parts.push(
    `/* Dark theme when following the OS (no explicit data-theme on <html>). */\n@media (prefers-color-scheme: dark) {\n  :root:not([data-theme]) {\n${themeBlock(colors.dark, 'dark', '    ')}\n  }\n}\n`,
  );
  parts.push(
    `/* Dark theme, chosen explicitly. */\n[data-theme="dark"] {\n${themeBlock(colors.dark, 'dark')}\n}\n`,
  );
  parts.push(`/* High-contrast theme. */\n[data-theme="hc"] {\n${themeBlock(colors.hc, 'dark')}\n}\n`);

  const accentRules = APPS.map(
    (app) =>
      `[data-app="${app}"] {\n  --kiln-accent: var(--kiln-accent-${app});\n  --kiln-accent-strong: var(--kiln-accent-strong-${app});\n}`,
  ).join('\n');
  parts.push(
    `/* Active app accent: <html data-app="write|grid|deck|notes|hub">. Hub is the default. */\n:root {\n  --kiln-accent: var(--kiln-accent-hub);\n  --kiln-accent-strong: var(--kiln-accent-strong-hub);\n}\n${accentRules}\n`,
  );

  parts.push(
    `/* Derived from the accent and theme; re-declared on every scope so nested data-app/data-theme work. */\n:root,\n[data-app],\n[data-theme] {\n${decls(derived)}\n}\n`,
  );

  parts.push(
    `/* Motion: respect the OS setting. */\n@media (prefers-reduced-motion: reduce) {\n  :root {\n    --kiln-duration-fast: 0ms;\n    --kiln-duration-menu: 0ms;\n    --kiln-duration-pane: 0ms;\n  }\n}\n`,
  );

  return normalize(`${parts.join('\n').trimEnd()}\n`);
}

/** Tailwind 4 `@theme` block mapping utilities (`bg-surface`, `text-fg`, `rounded-control`…) to tokens. */
export function renderThemeCss(): string {
  const v = (name: string) => `var(--kiln-${name})`;
  const color: Record<string, string> = {
    canvas: v('bg-canvas'),
    surface: v('bg-surface'),
    'surface-2': v('bg-surface-2'),
    'surface-3': v('bg-surface-3'),
    hover: v('bg-hover'),
    pressed: v('bg-pressed'),
    checked: v('bg-checked'),
    input: v('bg-input'),
    fg: v('fg'),
    muted: v('fg-muted'),
    disabled: v('fg-disabled'),
    line: v('border'),
    'line-strong': v('border-strong'),
    accent: v('accent'),
    'accent-strong': v('accent-strong'),
    'accent-subtle': v('accent-subtle'),
    'on-accent': v('on-accent'),
    selection: v('selection'),
    write: v('accent-write'),
    grid: v('accent-grid'),
    deck: v('accent-deck'),
    notes: v('accent-notes'),
    hub: v('accent-hub'),
    danger: v('danger'),
    warning: v('warning'),
    success: v('success'),
    info: v('info'),
  };
  const lines: string[] = [];
  lines.push('  --spacing: 4px;');
  lines.push(`  --font-sans: ${v('font-ui')};`, `  --font-mono: ${v('font-mono')};`);
  for (const [k, val] of Object.entries(color)) lines.push(`  --color-${k}: ${val};`);
  for (const r of ['control', 'menu', 'dialog'] as const) lines.push(`  --radius-${r}: ${v(`radius-${r}`)};`);
  for (const s of [1, 2, 3]) lines.push(`  --shadow-of-${s}: ${v(`shadow-${s}`)};`);
  for (const t of ['xs', 'sm', 'base', 'md', 'lg', 'xl'] as const)
    lines.push(`  --text-${t}: ${v(`text-${t}`)};`);
  lines.push(`  --ease-out: ${v('ease-out')};`);
  return normalize(
    `${HEADER('Tailwind 4 theme for Kiln: `@import "@kiln/tokens/theme.css";` after `@import "tailwindcss";`.')}@theme inline {\n${lines.join('\n')}\n}\n`,
  );
}

/**
 * `fonts.css`: the document fonts committed in `fonts/` (metric-compatible with Office defaults).
 * The UI fonts come from the brand kit, which WP-51 imports; until then the UI stacks fall back to
 * system fonts.
 */
export function renderFontsCss(): string {
  const faces = DOCUMENT_FONTS.flatMap((font) =>
    FONT_STYLES.map(
      (s) =>
        `/* ${font.family} ${s.suffix} — metric-compatible with ${font.replaces}. */\n@font-face {\n  font-family: '${font.family}';\n  font-style: ${s.style};\n  font-weight: ${s.weight};\n  font-display: swap;\n  src: url('./fonts/${font.file}-${s.suffix}.ttf') format('truetype');\n}`,
    ),
  );
  return normalize(`${HEADER('Kiln fonts: bundled document fonts (SIL OFL 1.1). UI fonts arrive with the brand kit (WP-51).')}${faces.join('\n\n')}
`);
}
