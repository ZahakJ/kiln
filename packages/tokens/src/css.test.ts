import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { renderFontsCss, renderThemeCss, renderTokensCss } from './css.ts';
import { APPS, colors, DOCUMENT_FONTS, FONT_STYLES, scale, THEMES } from './tokens.ts';

const pkg = (p: string) => fileURLToPath(new URL(`../${p}`, import.meta.url));
const read = (p: string) => readFileSync(pkg(p), 'utf8');

describe('generated CSS', () => {
  it('tokens.css, theme.css and fonts.css are up to date (run `pnpm generate`)', () => {
    expect(read('tokens.css')).toBe(renderTokensCss());
    expect(read('theme.css')).toBe(renderThemeCss());
    expect(read('fonts.css')).toBe(renderFontsCss());
  });

  it('declares every colour token for light, OS-dark, explicit dark and high contrast', () => {
    const css = renderTokensCss();
    for (const name of Object.keys(colors.light)) {
      const count = css.split(`--of-${name}:`).length - 1;
      expect(count, name).toBe(4);
    }
    expect(css).toContain('[data-theme="dark"]');
    expect(css).toContain('[data-theme="hc"]');
    expect(css).toMatch(/@media \(prefers-color-scheme: dark\)\s*{\s*:root:not\(\[data-theme\]\)/);
    for (const theme of THEMES) expect(css).toContain(colors[theme]['accent-write'].toLowerCase());
  });

  it('switches the accent per app with data-app', () => {
    const css = renderTokensCss();
    for (const app of APPS) {
      expect(css).toContain(`[data-app="${app}"] {\n  --of-accent: var(--of-accent-${app});`);
    }
    expect(css).toContain('--of-selection: color-mix(in srgb, var(--of-accent) var(--of-selection-alpha)');
  });

  it('declares every scale token once and zeroes motion for reduced motion', () => {
    const css = renderTokensCss();
    for (const name of Object.keys(scale))
      expect(css).toContain(`--of-${name}: ${scale[name as keyof typeof scale].replace(/'/g, '"')};`);
    expect(css).toMatch(/prefers-reduced-motion: reduce\)\s*{\s*:root\s*{\s*--of-duration-fast: 0ms/);
  });

  it('exposes a Tailwind 4 @theme block', () => {
    const css = renderThemeCss();
    expect(css).toContain('@theme inline {');
    expect(css).toContain('--color-accent: var(--of-accent);');
    expect(css).toContain('--radius-dialog: var(--of-radius-dialog);');
  });

  it('registers every bundled document font face, and the files exist', () => {
    const css = renderFontsCss();
    for (const font of DOCUMENT_FONTS) {
      for (const style of FONT_STYLES) {
        const file = `fonts/${font.file}-${style.suffix}.ttf`;
        expect(css).toContain(`url("./${file}")`);
        expect(existsSync(pkg(file)), file).toBe(true);
      }
    }
    expect(css).toContain('@import "@fontsource-variable/inter/wght.css";');
    expect(css).toContain('@import "@fontsource/jetbrains-mono/400.css";');
  });

  it('lists every bundled font with an OFL licence in fonts/NOTICES.json', () => {
    const notices = JSON.parse(read('fonts/NOTICES.json')) as {
      assets: Array<{ license: string; files: string[]; licenseFile: string }>;
    };
    const listed = notices.assets.flatMap((a) => a.files);
    for (const font of DOCUMENT_FONTS) {
      for (const style of FONT_STYLES) expect(listed).toContain(`${font.file}-${style.suffix}.ttf`);
    }
    for (const a of notices.assets) {
      expect(a.license).toBe('OFL-1.1');
      expect(read(`fonts/${a.licenseFile}`)).toMatch(/SIL Open Font License/i);
    }
  });
});
