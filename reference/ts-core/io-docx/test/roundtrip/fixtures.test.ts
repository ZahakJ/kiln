/**
 * Round-trip fidelity of the corpus (PLAN §3.5, §5): every feature fixture is read, written and read
 * again. Structure ≥ 0.95 and text = 1.00 always; the rewritten file must open with mammoth and
 * python-docx (same text as the original) and with LibreOffice, whose renderings of the original and
 * the rewritten file must agree ≥ 0.90 (oracle checks are skipped where the tools are missing, e.g. CI).
 */
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { OpcPackage } from '@kiln-ref/ooxml';
import {
  listFixtures,
  pdftoppmAvailable,
  pythonAvailable,
  readDocxWithMammoth,
  readWithPython,
  sofficeAvailable,
  structuralScore,
  textScore,
} from '@kiln/testing';
import { describe, expect, it } from 'vitest';
import { THRESHOLDS } from './fidelity-thresholds.ts';
import { type CorpusEntry, docxModel, docxText, featureFixtures, rewriteAll, scoreCorpus } from './model.ts';

const features = rewriteAll(featureFixtures());

/**
 * mammoth and python-docx ignore the result of `w:fldSimple` (the writer emits complex fields, whose
 * result they do read), so the originals are compared with simple fields unwrapped into the result runs
 * Word displays.
 */
function unwrapSimpleFields(bytes: Uint8Array): Uint8Array {
  const pkg = OpcPackage.open(bytes);
  const main = pkg.mainDocumentPart() ?? '';
  const xml = pkg.getPartText(main) ?? '';
  if (!xml.includes('fldSimple')) return bytes;
  pkg.setPart(
    main,
    xml.replace(/<w:fldSimple\b[^>]*\/>/g, '').replace(/<w:fldSimple\b[^>]*>|<\/w:fldSimple>/g, ''),
  );
  return pkg.save();
}

describe('feature fixtures round-trip', () => {
  it('covers the whole generated corpus', () => {
    expect(features.length).toBeGreaterThanOrEqual(19);
  });

  for (const e of features) {
    it(`${e.label}: structure ≥ ${THRESHOLDS.structural} and identical text`, () => {
      const structural = structuralScore(docxModel(e.original), docxModel(e.rewritten));
      expect(structural).toBeGreaterThanOrEqual(THRESHOLDS.structural);
      expect(textScore(docxText(e.original), docxText(e.rewritten))).toBe(1);
    });
  }

  for (const e of features) {
    it(`${e.label}: mammoth reads the rewritten file with the same text`, async () => {
      const [a, b] = await Promise.all([
        readDocxWithMammoth(unwrapSimpleFields(e.original)),
        readDocxWithMammoth(e.rewritten),
      ]);
      expect(b.text).toBe(a.text);
      expect(b.messages.filter((m) => m.startsWith('error'))).toEqual([]);
    });
  }
});

describe.runIf(pythonAvailable())('python-docx', () => {
  const dir = mkdtempSync(join(tmpdir(), 'kiln-docx-python-'));
  const save = (e: CorpusEntry, kind: string) => {
    const path = join(dir, `${kind}-${basename(e.path)}`);
    writeFileSync(path, kind === 'original' ? unwrapSimpleFields(e.original) : e.rewritten);
    return path;
  };
  for (const e of features) {
    it(`${e.label}: opens the rewritten file with the same paragraphs, tables and sections`, async () => {
      const [a, b] = await Promise.all([
        readWithPython(save(e, 'original')),
        readWithPython(save(e, 'rewritten')),
      ]);
      if (a.format !== 'docx' || b.format !== 'docx') throw new Error('not a docx dump');
      expect(b.paragraphs.map((p) => [p.style, p.text])).toEqual(a.paragraphs.map((p) => [p.style, p.text]));
      expect(b.tables).toEqual(a.tables);
      expect(b.sections).toEqual(a.sections);
    }, 60_000);
  }
});

describe.runIf(sofficeAvailable() && pdftoppmAvailable())('LibreOffice oracle', () => {
  it(
    'opens every rewritten file and renders it like the original (visual ≥ 0.90)',
    async () => {
      const scores = await scoreCorpus(features, { visual: true });
      const failing = scores.filter((s) => s.oracle_opens !== true || (s.visual ?? 0) < THRESHOLDS.visual);
      expect(failing.map((s) => `${s.file}: opens=${s.oracle_opens} visual=${s.visual}`)).toEqual([]);
    },
    15 * 60_000,
  );
});

const external = listFixtures('docx', 'external');
describe.runIf(external.length > 0)('real-world corpus', () => {
  it('round-trips every readable external file with its structure and text', () => {
    const skipped: string[] = [];
    const entries = rewriteAll(external, skipped);
    expect(entries.length + skipped.length).toBe(external.length);
    for (const e of entries) {
      expect(structuralScore(docxModel(e.original), docxModel(e.rewritten)), e.label).toBeGreaterThanOrEqual(
        THRESHOLDS.structural,
      );
      expect(textScore(docxText(e.original), docxText(e.rewritten)), e.label).toBe(1);
    }
  }, 120_000);
});
