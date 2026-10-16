import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import {
  APPS,
  colors,
  contrastRatio,
  cssVar,
  kit,
  PACKAGE_NAME,
  productOf,
  relativeLuminance,
  THEMES,
  themeBase,
} from './index.ts';
import { KIT_PRODUCTS, parseKit } from './kit.ts';

const hex = (v: string) => /^#[0-9A-F]{6}$/i.test(v);
const brandJson = () =>
  JSON.parse(readFileSync(fileURLToPath(new URL('../../../brand/tokens.json', import.meta.url)), 'utf8'));

describe('@kiln/tokens values', () => {
  it('exports its package name', () => {
    expect(PACKAGE_NAME).toBe('@kiln/tokens');
  });

  it('the typed kit object is exactly brand/tokens.json (run `pnpm generate` after a kit import)', () => {
    expect(kit).toEqual(brandJson());
  });

  it('maps the semantic surfaces and text onto the kit (BRAND.md §3)', () => {
    expect(colors.light['bg-surface-2']).toBe('#F6EFE6');
    expect(colors.light['bg-surface']).toBe('#FFFCF7');
    expect(colors.light.fg).toBe('#2B1C14');
    expect(colors.dark['bg-surface-2']).toBe('#17110E');
    expect(colors.dark['bg-surface']).toBe('#201813');
    expect(colors.dark.fg).toBe('#F5EBDD');
    expect(colors.light['accent-write']).toBe('#3159C6');
    expect(colors.light['accent-strong-grid']).toBe('#17714E');
    expect(colors.dark['accent-strong-deck']).toBe('#F7976B');
    expect(colors.light['accent-strong-notes']).toBe('#87590A');
    // Home uses ember, never a product colour.
    expect(colors.light['accent-home']).toBe(kit.color.light.ember);
    expect(colors.dark['accent-strong-hub']).toBe(kit.color.dark.emberText);
  });

  it('uses only kit colours: every hex value in every theme exists in the kit', () => {
    const palette = new Set(
      JSON.stringify(kit)
        .match(/#[0-9A-F]{6}/gi)
        ?.map((h) => h.toUpperCase()),
    );
    for (const t of THEMES) {
      for (const [name, value] of Object.entries(colors[t])) {
        for (const h of value.match(/#[0-9A-F]{6}/gi) ?? []) {
          expect(palette.has(h.toUpperCase()), `${t}.${name} uses ${h}`).toBe(true);
        }
      }
    }
  });

  it('defines the same token names in every theme', () => {
    const names = Object.keys(colors.light).sort();
    for (const t of THEMES) expect(Object.keys(colors[t]).sort()).toEqual(names);
  });

  it('accepts hub as an alias of home', () => {
    expect(productOf('hub')).toBe('home');
    expect(APPS).toContain('home');
    for (const t of THEMES) expect(colors[t]['accent-hub']).toBe(colors[t]['accent-home']);
  });

  it('computes WCAG contrast correctly', () => {
    expect(relativeLuminance('#FFFFFF')).toBeCloseTo(1, 5);
    expect(contrastRatio('#000000', '#FFFFFF')).toBeCloseTo(21, 5);
    expect(contrastRatio('#777777', '#777777')).toBeCloseTo(1, 5);
    expect(() => relativeLuminance('red')).toThrow();
  });

  // PLAN.md §1.2 / BRAND.md §3: every text pair passes WCAG 2.1 AA in every theme.
  for (const theme of THEMES) {
    it(`keeps text contrast ≥ 4.5:1 in the ${theme} theme`, () => {
      const c = colors[theme];
      const b = themeBase(kit, theme);
      // App-coloured text sits on the window, panels and inputs; the canvas behind pages only carries
      // neutral text (deck/home text on the light canvas is 4.497:1, see docs/design/kit-gaps.md).
      const textSurfaces = [c['bg-surface'], c['bg-surface-2'], c['bg-input'], b.page];
      for (const bg of [...textSurfaces, c['bg-canvas'], c['bg-surface-3']]) {
        expect(hex(bg)).toBe(true);
        expect(contrastRatio(c.fg, bg)).toBeGreaterThanOrEqual(4.5);
        expect(contrastRatio(c['fg-muted'], bg), `muted on ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
      for (const bg of textSurfaces) {
        for (const app of APPS) {
          expect(contrastRatio(c[`accent-strong-${app}`], bg), `${app} on ${bg}`).toBeGreaterThanOrEqual(4.5);
        }
      }
      for (const app of APPS) {
        expect(contrastRatio(c['on-accent'], c[`accent-strong-${app}`]), app).toBeGreaterThanOrEqual(4.5);
        // Text on the app tint (selected toggles, active view buttons).
        const p = productOf(app);
        expect(
          contrastRatio(c[`accent-strong-${app}`], b.productTint[p]),
          `${app} on tint`,
        ).toBeGreaterThanOrEqual(4.5);
      }
      expect(contrastRatio(c['fg-tooltip'], c['bg-tooltip'])).toBeGreaterThanOrEqual(4.5);
      for (const s of ['danger', 'success', 'warning'] as const) {
        expect(contrastRatio(c[s], c['bg-surface']), s).toBeGreaterThanOrEqual(4.5);
      }
      // The focus ring (app text colour, or the flame in high contrast) is visible: ≥ 3:1 (WCAG 1.4.11).
      expect(contrastRatio(b.focus, c['bg-surface'])).toBeGreaterThanOrEqual(3);
    });
  }

  it('solid app buttons pass AA with their on-solid text (both themes use the same solids)', () => {
    for (const p of KIT_PRODUCTS) {
      const g = kit.color.product[p];
      expect(contrastRatio(g.solid, g.onSolid), p).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('builds typed var() references', () => {
    expect(cssVar('accent')).toBe('var(--kiln-accent)');
    expect(cssVar('radius-dialog')).toBe('var(--kiln-radius-dialog)');
    expect(cssVar('panel')).toBe('var(--kiln-panel)');
  });
});

describe('parseKit', () => {
  it('accepts the committed kit', () => {
    expect(() => parseKit(brandJson())).not.toThrow();
  });

  it('rejects a wrong schema, a malformed colour and CSS injection, naming the JSON path', () => {
    const bad = (mutate: (k: ReturnType<typeof brandJson>) => void) => {
      const k = brandJson();
      mutate(k);
      return () => parseKit(k);
    };
    expect(bad((k) => (k.$schema = 'other'))).toThrow(/\$schema/);
    expect(bad((k) => (k.color.light.panel = 'white'))).toThrow(/color\.light\.panel/);
    expect(bad((k) => delete k.color.product.deck)).toThrow(/color\.product\.deck/);
    expect(bad((k) => (k.shadow.dark['1'] = '0 0 1px red; } body { display: none'))).toThrow(
      /shadow\.dark\.1/,
    );
    expect(bad((k) => (k.font.stack.ui = 'Figtree; color: red'))).toThrow(/font\.stack\.ui/);
    expect(bad((k) => (k.motion.fast = 'fast'))).toThrow(/motion\.fast/);
    expect(() => parseKit(null)).toThrow(/\$/);
  });
});
