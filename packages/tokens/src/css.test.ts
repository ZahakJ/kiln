import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { formatGenerated } from '../scripts/format.ts';
import { renderFontsCss, renderThemeCss, renderTokensCss, themeTokens } from './css.ts';
import { parseKit, renderKitModule } from './kit.ts';
import { BRAND_FONTS, colors, DOCUMENT_FONTS, FONT_STYLES, kit, scale, THEMES } from './tokens.ts';

const pkg = (p: string) => fileURLToPath(new URL(`../${p}`, import.meta.url));
const repo = (p: string) => fileURLToPath(new URL(`../../../${p}`, import.meta.url));
const read = (p: string) => readFileSync(pkg(p), 'utf8');
/** The kit as the generator sees it: parsed fresh from brand/tokens.json, not the committed module. */
const freshKit = () => parseKit(JSON.parse(readFileSync(repo('brand/tokens.json'), 'utf8')));

describe('generated files (drift check: regenerate from brand/tokens.json and diff)', () => {
  it('tokens.css, theme.css and fonts.css are what brand/tokens.json produces (run `pnpm generate`)', () => {
    const k = freshKit();
    expect(read('tokens.css')).toBe(formatGenerated(renderTokensCss(k), 'tokens.css'));
    expect(read('theme.css')).toBe(formatGenerated(renderThemeCss(k), 'theme.css'));
    expect(read('fonts.css')).toBe(formatGenerated(renderFontsCss(), 'fonts.css'));
  });

  it('src/generated/kit.ts holds the same data as brand/tokens.json', () => {
    const k = freshKit();
    expect(kit).toEqual(k);
    expect(read('src/generated/kit.ts')).toBe(formatGenerated(renderKitModule(k), 'src/generated/kit.ts'));
  });
});

describe('tokens.css', () => {
  const css = renderTokensCss();

  it('declares every semantic colour for light, OS-dark, explicit dark and high contrast', () => {
    for (const name of Object.keys(colors.light)) {
      const count = css.split(`--kiln-${name}:`).length - 1;
      expect(count, name).toBe(4);
    }
    expect(css).toContain('[data-theme="dark"]');
    expect(css).toContain('[data-theme="hc"]');
    expect(css).toMatch(/@media \(prefers-color-scheme: dark\)\s*{\s*:root:not\(\[data-theme\]\)/);
  });

  it("defines every variable the kit's own tokens.css defines, so the mockups' CSS works unchanged", () => {
    const kitCss = readFileSync(repo('brand/tokens.css'), 'utf8');
    const names = new Set([...kitCss.matchAll(/(--(?:kiln|app)[a-z0-9-]*)\s*:/g)].map((m) => m[1]));
    expect(names.size).toBeGreaterThan(80);
    for (const name of names) expect(css, name).toContain(`${name}:`);
  });

  it('activates the app colours with data-app (write=cobalt, grid=jade, deck=ember, notes=saffron)', () => {
    for (const p of ['write', 'grid', 'deck', 'notes'] as const) {
      expect(css).toContain(
        `[data-app="${p}"],\n[data-app="${p}"] [data-theme] {\n  --app: var(--kiln-${p}-fill);`,
      );
    }
    expect(css).toContain('[data-app="home"],\n[data-app="home"] [data-theme],\n[data-app="hub"]');
    expect(css).toMatch(/:root,\n\[data-theme\] {\n {2}--app: var\(--kiln-ember\);/);
    expect(css).toContain(`--kiln-write-fill: ${kit.color.product.write.fill.toLowerCase()};`);
    expect(css).toContain(`--kiln-grid-fill: ${kit.color.product.grid.fill.toLowerCase()};`);
    expect(css).toContain(`--kiln-deck-fill: ${kit.color.product.deck.fill.toLowerCase()};`);
    expect(css).toContain(`--kiln-notes-fill: ${kit.color.product.notes.fill.toLowerCase()};`);
    expect(css).toContain('--kiln-focus-ring: var(--kiln-focus-override, var(--app-text));');
  });

  it('keeps kit and semantic names consistent where they overlap', () => {
    for (const t of THEMES) {
      const all = themeTokens(kit, t);
      expect(all.fg).toBe(colors[t].fg);
      expect(all.panel).toBe(colors[t]['bg-surface']);
    }
  });

  it('declares every scale token and zeroes motion for reduced motion', () => {
    for (const [name, value] of Object.entries(scale))
      expect(css).toContain(
        `--kiln-${name}: ${value.replace(/'/g, '"').replace(/#[0-9a-f]{6}/gi, (h) => h.toLowerCase())};`,
      );
    expect(css).toMatch(/prefers-reduced-motion: reduce\)\s*{\s*:root\s*{\s*--kiln-motion-fast: 0ms/);
    expect(css).toContain('--kiln-font-ui: "Figtree"');
    expect(css).toContain('--kiln-font-display: "Fraunces"');
    expect(css).not.toMatch(/Inter|JetBrains/);
  });
});

describe('theme.css and fonts.css', () => {
  it('exposes a Tailwind 4 @theme block over the tokens', () => {
    const css = renderThemeCss();
    expect(css).toContain('@theme inline {');
    expect(css).toContain('--color-panel: var(--kiln-panel);');
    expect(css).toContain('--color-app-solid: var(--app-solid);');
    expect(css).toContain('--color-accent: var(--kiln-accent);');
    expect(css).toContain('--font-display: var(--kiln-font-display);');
    expect(css).toContain('--radius-dialog: var(--kiln-radius-dialog);');
  });

  it('registers the brand UI fonts from assets/fonts/brand/, and the files and licences exist', () => {
    const css = renderFontsCss();
    for (const f of BRAND_FONTS) {
      const url = `../../assets/fonts/brand/${f.dir}/${f.file}`;
      expect(css).toContain(`url("${url}")`);
      expect(existsSync(pkg(url)), url).toBe(true);
      expect(readFileSync(repo(`assets/fonts/brand/${f.dir}/OFL.txt`), 'utf8')).toMatch(
        /SIL OPEN FONT LICENSE/i,
      );
      expect(css).toContain(`font-weight: ${f.weights[0]} ${f.weights[1]};`);
    }
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
    expect(css).not.toContain('@import');
  });

  it('lists every bundled font with an OFL licence in fonts/NOTICES.json', () => {
    const notices = JSON.parse(read('fonts/NOTICES.json')) as {
      assets: Array<{ name: string; license: string; files?: string[]; licenseFile: string }>;
    };
    const listed = notices.assets.flatMap((a) => a.files ?? []);
    for (const font of DOCUMENT_FONTS) {
      for (const style of FONT_STYLES) expect(listed).toContain(`${font.file}-${style.suffix}.ttf`);
    }
    for (const f of BRAND_FONTS) expect(listed).toContain(`../../../assets/fonts/brand/${f.dir}/${f.file}`);
    for (const a of notices.assets) {
      expect(a.license).toBe('OFL-1.1');
      expect(read(`fonts/${a.licenseFile}`)).toMatch(/SIL Open Font License/i);
    }
  });
});
