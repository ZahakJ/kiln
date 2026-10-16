/**
 * Renders `tokens.css`, `theme.css` and `fonts.css` from the brand kit. Pure functions of the parsed
 * kit, so tests can assert that the committed files are exactly what `brand/tokens.json` produces.
 */
import { KIT_PRODUCTS, type KitProductId, type KitTokens } from './kit.ts';
import {
  APPS,
  BRAND_FONTS,
  kit as committedKit,
  DOCUMENT_FONTS,
  derived,
  FONT_STYLES,
  nativeThemeTokens,
  type ResolvedTheme,
  scaleTokens,
  semanticColors,
  themeBase,
} from './tokens.ts';

const HEADER = (what: string) =>
  `/*\n * ${what}\n * GENERATED from brand/tokens.json by \`pnpm --filter @kiln/tokens generate\`.\n * Do not edit by hand.\n */\n`;

/** Matches Biome's CSS formatting (double quotes, lowercase hex) so generated files pass `biome check`. */
const normalize = (css: string) =>
  css
    .replace(/'/g, '"')
    .replace(/#[0-9a-f]{3,8}\b/gi, (hex) => hex.toLowerCase())
    .replace(/rgba?\(([^)]*)\)/g, (_, args: string) => {
      const parts = args.split(',').map((a) => a.trim().replace(/^\./, '0.'));
      return `rgba(${parts.join(', ')})`;
    });

const decls = (entries: Record<string, string>, indent = '  ') =>
  Object.entries(entries)
    .map(([k, v]) => `${indent}--kiln-${k}: ${v};`)
    .join('\n');

/**
 * Every per-theme `--kiln-*` variable: the kit's names and the semantic names. Where both vocabularies
 * use the same name (`fg`, `danger`, `shadow-1`, …) the values must agree.
 */
export function themeTokens(k: KitTokens, theme: ResolvedTheme): Record<string, string> {
  const native = nativeThemeTokens(themeBase(k, theme));
  const semantic = semanticColors(k, theme) as unknown as Record<string, string>;
  for (const [name, v] of Object.entries(semantic)) {
    if (name in native && native[name] !== v) {
      throw new Error(
        `tokens: --kiln-${name} differs between kit (${native[name]}) and UI (${v}) in ${theme}`,
      );
    }
  }
  return { ...native, ...semantic };
}

const themeBlock = (k: KitTokens, theme: ResolvedTheme, indent = '  ') => {
  const scheme = theme === 'light' ? 'light' : 'dark';
  // High contrast replaces the app-coloured focus ring with the flame; other themes clear the override
  // so a light/dark scope nested in a high-contrast one gets the app colour back.
  const override = theme === 'hc' ? themeBase(k, 'hc').focus : 'initial';
  const extra = `\n${indent}--kiln-focus-override: ${override};`;
  return `${indent}color-scheme: ${scheme};\n${decls(themeTokens(k, theme), indent)}${extra}`;
};

/** The kit's per-app variables (`--app*`) for one product. */
function appVars(product: KitProductId): string {
  const app = product === 'home' ? 'var(--kiln-ember)' : `var(--kiln-${product}-fill)`;
  return [
    `  --app: ${app};`,
    `  --app-solid: var(--kiln-${product}-solid);`,
    `  --app-on-solid: var(--kiln-${product}-on-solid);`,
    `  --app-text: var(--kiln-${product}-text);`,
    `  --app-tint: var(--kiln-${product}-tint);`,
  ].join('\n');
}

/** The full `tokens.css`: every `--kiln-*` custom property for every theme and app. */
export function renderTokensCss(k: KitTokens = committedKit): string {
  const parts: string[] = [HEADER('Kiln design tokens, from the brand kit (PLAN.md §1.2).')];

  parts.push(
    `/* Theme-independent: glazes, fire, neutrals, type, radii, the 4 px grid, controls, motion, stacking. */\n:root {\n${decls(scaleTokens(k))}\n}\n`,
  );
  parts.push(`/* Light theme (default). */\n:root,\n[data-theme="light"] {\n${themeBlock(k, 'light')}\n}\n`);
  parts.push(
    `/* Dark theme when following the OS (no explicit data-theme on <html>). */\n@media (prefers-color-scheme: dark) {\n  :root:not([data-theme]) {\n${themeBlock(k, 'dark', '    ')}\n  }\n}\n`,
  );
  parts.push(
    `/* Dark theme, chosen explicitly. Tuned, not inverted. */\n[data-theme="dark"] {\n${themeBlock(k, 'dark')}\n}\n`,
  );
  parts.push(
    `/* High-contrast theme (assembled from kit colours; see docs/design/kit-gaps.md). */\n[data-theme="hc"] {\n${themeBlock(k, 'hc')}\n}\n`,
  );

  parts.push(
    `/* Derived from the app and theme; re-declared on every scope so nested data-app/data-theme work. */\n:root,\n[data-app],\n[data-theme] {\n${decls(derived)}\n}\n`,
  );

  // App activation. A nested [data-theme] re-resolves the theme-dependent --app-text/--app-tint.
  const appRules = KIT_PRODUCTS.map((p) => {
    const ids = APPS.filter((a) => (a === 'hub' ? 'home' : a) === p);
    const selectors = ids.flatMap((a) => [`[data-app="${a}"]`, `[data-app="${a}"] [data-theme]`]);
    return `${selectors.join(',\n')} {\n${appVars(p)}\n}`;
  }).join('\n');
  parts.push(
    `/* The active app: <html data-app="write|grid|deck|notes|home"> ("hub" = home). Home is the default. */\n:root,\n[data-theme] {\n${appVars('home')}\n}\n${appRules}\n`,
  );

  parts.push(
    `/* Motion: respect the OS setting. */\n@media (prefers-reduced-motion: reduce) {\n  :root {\n    --kiln-motion-fast: 0ms;\n    --kiln-motion-base: 0ms;\n    --kiln-motion-slow: 0ms;\n  }\n}\n`,
  );

  return normalize(`${parts.join('\n').trimEnd()}\n`);
}

/** Tailwind 4 `@theme` block mapping utilities (`bg-panel`, `text-muted`, `rounded-control`…) to tokens. */
export function renderThemeCss(k: KitTokens = committedKit): string {
  const v = (name: string) => `var(--kiln-${name})`;
  const color: Record<string, string> = {
    // The kit's surfaces and text.
    bg: v('bg'),
    panel: v('panel'),
    raised: v('raised'),
    sunken: v('sunken'),
    page: v('page'),
    canvas: v('canvas'),
    fg: v('fg'),
    muted: v('muted'),
    faint: v('faint'),
    line: v('line'),
    'line-strong': v('line-strong'),
    ember: v('ember'),
    'ember-text': v('ember-text'),
    'on-ember': v('on-ember'),
    selection: v('selection'),
    // The active app.
    app: 'var(--app)',
    'app-solid': 'var(--app-solid)',
    'app-on-solid': 'var(--app-on-solid)',
    'app-text': 'var(--app-text)',
    'app-tint': 'var(--app-tint)',
    // Semantic aliases.
    surface: v('bg-surface'),
    'surface-2': v('bg-surface-2'),
    'surface-3': v('bg-surface-3'),
    hover: v('bg-hover'),
    pressed: v('bg-pressed'),
    checked: v('bg-checked'),
    input: v('bg-input'),
    disabled: v('fg-disabled'),
    accent: v('accent'),
    'accent-strong': v('accent-strong'),
    'accent-subtle': v('accent-subtle'),
    'on-accent': v('on-accent'),
    danger: v('danger'),
    warning: v('warning'),
    success: v('success'),
    info: v('info'),
  };
  for (const p of KIT_PRODUCTS) color[p] = v(`${p}-fill`);
  color.hub = v('home-fill');
  for (const name of Object.keys(k.color.fire)) color[`fire-${name}`] = v(`fire-${name}`);
  for (const name of Object.keys(k.color.neutral)) color[name] = v(name);

  const lines: string[] = [];
  lines.push('  --spacing: 4px;');
  lines.push(`  --font-sans: ${v('font-ui')};`, `  --font-display: ${v('font-display')};`);
  for (const [name, val] of Object.entries(color)) lines.push(`  --color-${name}: ${val};`);
  for (const r of ['xs', 'sm', 'md', 'lg', 'xl', 'control', 'menu', 'dialog'])
    lines.push(`  --radius-${r}: ${v(`radius-${r}`)};`);
  lines.push(`  --radius-pill: ${v('radius-pill')};`);
  for (const s of ['1', '2', '3', 'page']) lines.push(`  --shadow-kiln-${s}: ${v(`shadow-${s}`)};`);
  for (const t of Object.keys(k.font.size)) lines.push(`  --text-${t}: ${v(`text-${t}`)};`);
  lines.push(`  --ease-kiln: ${v('ease')};`);
  return normalize(
    `${HEADER('Tailwind 4 theme for Kiln: `@import "@kiln/tokens/theme.css";` after `@import "tailwindcss";`.')}@theme inline {\n${lines.join('\n')}\n}\n`,
  );
}

/**
 * `fonts.css`: the brand's UI fonts (Figtree, Fraunces; variable, from the kit, served from
 * `assets/fonts/brand/`) and the bundled document fonts that are metric-compatible with Office's.
 */
export function renderFontsCss(): string {
  const brand = BRAND_FONTS.map(
    (f) =>
      `/* ${f.family} (${f.role}), variable ${f.weights[0]}–${f.weights[1]}, SIL OFL 1.1 — from the brand kit. */\n@font-face {\n  font-family: '${f.family}';\n  font-style: normal;\n  font-weight: ${f.weights[0]} ${f.weights[1]};\n  font-display: swap;\n  src: url('../../assets/fonts/brand/${f.dir}/${f.file}') format('truetype');\n}`,
  );
  const docs = DOCUMENT_FONTS.flatMap((font) =>
    FONT_STYLES.map(
      (s) =>
        `/* ${font.family} ${s.suffix} — metric-compatible with ${font.replaces}. */\n@font-face {\n  font-family: '${font.family}';\n  font-style: ${s.style};\n  font-weight: ${s.weight};\n  font-display: swap;\n  src: url('./fonts/${font.file}-${s.suffix}.ttf') format('truetype');\n}`,
    ),
  );
  return normalize(
    `${HEADER('Kiln fonts: the brand UI faces and the bundled document fonts (all SIL OFL 1.1).')}${[...brand, ...docs].join('\n\n')}\n`,
  );
}
