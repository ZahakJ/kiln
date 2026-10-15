import { describe, expect, it } from 'vitest';
import { APPS, colors, contrastRatio, cssVar, PACKAGE_NAME, relativeLuminance, THEMES } from './index.ts';

const hex = (v: string) => /^#[0-9A-F]{6}$/i.test(v);

describe('@officina/tokens values', () => {
  it('exports its package name', () => {
    expect(PACKAGE_NAME).toBe('@officina/tokens');
  });

  it('matches the PLAN.md §1.2 colour table exactly', () => {
    expect(colors.light['bg-canvas']).toBe('#EEF1F6');
    expect(colors.dark['bg-canvas']).toBe('#15171C');
    expect(colors.light['bg-surface']).toBe('#FFFFFF');
    expect(colors.dark['bg-surface']).toBe('#1E2127');
    expect(colors.light['bg-surface-2']).toBe('#F6F8FB');
    expect(colors.dark['bg-surface-2']).toBe('#262A31');
    expect(colors.light.fg).toBe('#1B1F27');
    expect(colors.dark.fg).toBe('#E8EAEE');
    expect(colors.light['fg-muted']).toBe('#5A6170');
    expect(colors.dark['fg-muted']).toBe('#9AA1AE');
    expect(colors.light.border).toBe('#D8DDE6');
    expect(colors.dark.border).toBe('#333843');
    expect([colors.light['accent-write'], colors.dark['accent-write']]).toEqual(['#2B5FD9', '#6B8FF0']);
    expect([colors.light['accent-grid'], colors.dark['accent-grid']]).toEqual(['#1F8A5B', '#4FC08D']);
    expect([colors.light['accent-deck'], colors.dark['accent-deck']]).toEqual(['#D9552B', '#F2825E']);
    expect([colors.light['accent-notes'], colors.dark['accent-notes']]).toEqual(['#7A3FD1', '#A97CF0']);
    expect([colors.light['accent-hub'], colors.dark['accent-hub']]).toEqual(['#2B5FD9', '#6B8FF0']);
    expect([colors.light.danger, colors.light.warning, colors.light.success]).toEqual([
      '#C8352B',
      '#B7791F',
      '#1F8A5B',
    ]);
    expect([colors.dark.danger, colors.dark.warning, colors.dark.success]).toEqual([
      '#F07167',
      '#E0A52F',
      '#4FC08D',
    ]);
    expect([colors.light['selection-alpha'], colors.dark['selection-alpha']]).toEqual(['18%', '28%']);
  });

  it('defines the same token names in every theme', () => {
    const names = Object.keys(colors.light).sort();
    for (const t of THEMES) expect(Object.keys(colors[t]).sort()).toEqual(names);
  });

  it('computes WCAG contrast correctly', () => {
    expect(relativeLuminance('#FFFFFF')).toBeCloseTo(1, 5);
    expect(contrastRatio('#000000', '#FFFFFF')).toBeCloseTo(21, 5);
    expect(contrastRatio('#777777', '#777777')).toBeCloseTo(1, 5);
    expect(() => relativeLuminance('red')).toThrow();
  });

  // PLAN.md §3.10: text contrast ≥ 4.5:1 in all themes.
  for (const theme of THEMES) {
    it(`keeps text contrast ≥ 4.5:1 in the ${theme} theme`, () => {
      const c = colors[theme];
      const surfaces = [c['bg-canvas'], c['bg-surface'], c['bg-surface-2'], c['bg-surface-3']].filter(hex);
      for (const bg of surfaces) {
        expect(contrastRatio(c.fg, bg)).toBeGreaterThanOrEqual(4.5);
        expect(contrastRatio(c['fg-muted'], bg)).toBeGreaterThanOrEqual(4.5);
        for (const app of APPS) {
          expect(contrastRatio(c[`accent-strong-${app}`], bg), `${app} on ${bg}`).toBeGreaterThanOrEqual(4.5);
        }
      }
      for (const app of APPS) {
        expect(contrastRatio(c['on-accent'], c[`accent-strong-${app}`]), app).toBeGreaterThanOrEqual(4.5);
      }
      expect(contrastRatio(c['fg-tooltip'], c['bg-tooltip'])).toBeGreaterThanOrEqual(4.5);
    });
  }

  it('builds typed var() references', () => {
    expect(cssVar('accent')).toBe('var(--of-accent)');
    expect(cssVar('radius-dialog')).toBe('var(--of-radius-dialog)');
  });
});
