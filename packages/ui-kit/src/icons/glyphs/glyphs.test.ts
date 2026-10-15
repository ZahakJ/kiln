import { describe, expect, it } from 'vitest';
import { REQUIRED_GLYPH_NAMES, REQUIRED_GLYPHS } from '../required-glyphs.ts';
import * as exported from './index.ts';
import { glyphList, glyphs, glyphToSvg } from './index.ts';

describe('Officina Glyphs', () => {
  it('ships at least 120 glyphs (PLAN.md §1.2)', () => {
    expect(glyphList().length).toBeGreaterThanOrEqual(120);
  });

  it('lists at least 120 required glyph names from the §1.4 ribbon specs', () => {
    expect(REQUIRED_GLYPH_NAMES.length).toBeGreaterThanOrEqual(120);
    expect(Object.keys(REQUIRED_GLYPHS).length).toBeGreaterThan(10);
  });

  for (const name of REQUIRED_GLYPH_NAMES) {
    it(`exports ${name}`, () => {
      expect(Object.hasOwn(exported, name), `${name} is required by §1.4 but not exported`).toBe(true);
      expect(glyphs).toHaveProperty(name);
    });
  }

  it('every glyph is a well-formed 20×20 currentColor icon with a unique id', () => {
    const ids = new Set<string>();
    for (const [name, g] of glyphList()) {
      expect(g.glyphLabel.length, name).toBeGreaterThan(0);
      expect(ids.has(g.glyphId), `duplicate id ${g.glyphId}`).toBe(false);
      ids.add(g.glyphId);
      expect(g.parts.length, name).toBeGreaterThan(0);
      for (const p of g.parts) {
        expect(p.d, name).toMatch(/^M/);
        expect(p.d, name).not.toMatch(/NaN|undefined/);
        // Every absolute coordinate stays inside the viewBox.
        for (const m of p.d.matchAll(/[ML](-?[\d.]+) (-?[\d.]+)/g)) {
          const [x, y] = [Number(m[1]), Number(m[2])];
          expect(x >= 0 && x <= 20 && y >= 0 && y <= 20, `${name}: ${m[0]} outside viewBox`).toBe(true);
        }
      }
      const svg = glyphToSvg(g);
      expect(svg).toContain('viewBox="0 0 20 20"');
      expect(svg).toContain('stroke="currentColor"');
    }
  });
});
