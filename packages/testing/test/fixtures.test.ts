import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { OpcPackage, RelType } from '@officina/ooxml';
import { describe, expect, it } from 'vitest';
import {
  FIXTURE_FORMATS,
  FIXTURES_DIR,
  FixtureNotFoundError,
  fixtureLabel,
  fixturePath,
  listFixtures,
  loadFixture,
} from '../src/index.ts';

describe('fixture corpus', () => {
  it('has the documented layout', () => {
    for (const f of FIXTURE_FORMATS) {
      expect(existsSync(join(FIXTURES_DIR, f, 'features'))).toBe(true);
      expect(existsSync(join(FIXTURES_DIR, f, 'manual'))).toBe(true);
    }
  });

  it.each(FIXTURE_FORMATS)('has at least 15 generated %s files, each a valid package', (format) => {
    const files = listFixtures(format);
    expect(files.length).toBeGreaterThanOrEqual(15);
    for (const f of files) {
      const pkg = OpcPackage.open(loadFixture(fixtureLabel(f)));
      expect(pkg.mainDocumentPart(), f).toBeDefined();
      expect(pkg.relatedPart('/', RelType.coreProperties), f).toBe('/docProps/core.xml');
    }
  });

  it('loads fixtures by relative path or bare name', () => {
    const a = loadFixture('docx/features/styles.docx');
    const b = loadFixture('styles.docx');
    expect(Buffer.from(a).equals(Buffer.from(b))).toBe(true);
    expect(loadFixture('styles.xlsx').length).toBeGreaterThan(0);
    expect(fixturePath('charts.pptx')).toMatch(/pptx\/features\/charts\.pptx$/);
    expect(() => loadFixture('missing.docx')).toThrow(FixtureNotFoundError);
    expect(() => loadFixture('docx/features/missing.docx')).toThrow(FixtureNotFoundError);
    expect(listFixtures('docx', 'external')).toEqual(expect.any(Array));
  });

  it('pins fixture dates for reproducibility', () => {
    const pkg = OpcPackage.open(loadFixture('docx/features/properties.docx'));
    const core = new TextDecoder().decode(pkg.getPart('/docProps/core.xml'));
    expect(core).toContain('2026-01-15T09:30:00Z');
  });
});
