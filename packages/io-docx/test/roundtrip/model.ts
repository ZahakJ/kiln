/**
 * The DOCX structural model for the fidelity scorer (PLAN §5): everything `readDocx` produces, as plain
 * JSON, minus what a save changes on purpose:
 *
 * - `dcterms:modified`, `cp:revision` (bumped) and the extended properties (`Application` becomes
 *   "Officina", statistics are recomputed);
 * - the namespace declarations of the document root (the writer declares what the content uses);
 * - relationship ids of the id → target tables (relationships are compared through the content).
 *
 * Identifier keys (`id`, `*Id`, `paraId`, …) are ignored by the scorer itself.
 */
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { OpcPackage, parseXml, type XmlElement } from '@officina/ooxml';
import {
  convertBatchWithSoffice,
  type FidelityScore,
  fixtureLabel,
  listFixtures,
  pdftoppmAvailable,
  renderPdfPages,
  scoreRoundTrip,
  sofficeAvailable,
  structuralScore,
  textScore,
  visualScore,
} from '@officina/testing';
import { DocxFormatError, type ReadDocxResult, readDocx, writeDocx } from '../../src/index.ts';

/** A fixed save time, so rewritten files (and their scores) are reproducible. */
export const SAVE_TIME = new Date('2026-10-15T12:00:00Z');

export function modelOf(r: ReadDocxResult): Record<string, unknown> {
  const { modified: _m, revision: _r, ...core } = r.properties.core;
  const { documentRootAttrs: _d, ...extras } = r.extras;
  return {
    doc: r.doc.toJSON(),
    styles: r.styles.toJSON(),
    numbering: r.numbering.toJSON(),
    theme: r.theme,
    settings: r.settings,
    sections: r.sections,
    comments: r.comments,
    people: r.people,
    core,
    custom: r.properties.custom,
    media: [...r.media.entries()]
      .map(([k, v]) => ({ part: k, contentType: v.contentType, data: v.data }))
      .sort((a, b) => a.part.localeCompare(b.part)),
    passthrough: r.passthrough.parts
      .map((p) => ({ part: p.name, contentType: p.contentType, data: p.bytes }))
      .sort((a, b) => a.part.localeCompare(b.part)),
    charts: r.charts.map(({ relId: _id, ...c }) => c),
    mainPart: r.mainPart,
    mainContentType: r.mainContentType,
    extras,
    warnings: r.warnings.map((w) => `${w.code}|${w.severity}|${w.part}`),
  };
}

const TEXT = new Set(['w:t', 'w:delText', 'a:t', 'm:t']);
const IGNORED_PARTS = /^\/(docProps\/|\[trash\]\/)|theme|styles|settings|fontTable|webSettings|numbering/i;

/**
 * Visible text of a WordprocessingML package, independent of our reader: the `packageText` model of
 * `@officina/testing` (text runs and tabs of every story part, one line per paragraph) with Word's
 * character elements — `w:softHyphen` and `w:noBreakHyphen` — counted as the characters they stand for,
 * so a literal U+00AD and `<w:softHyphen/>` compare equal.
 */
export function docxText(bytes: Uint8Array): string {
  const pkg = OpcPackage.open(bytes);
  const out: string[] = [];
  const walk = (e: XmlElement) => {
    if (TEXT.has(e.name)) {
      for (const c of e.children) if (c.type === 'text') out.push(c.text);
      return;
    }
    if (e.name === 'w:tab') out.push('\t');
    else if (e.name === 'w:softHyphen') out.push('\u00AD');
    else if (e.name === 'w:noBreakHyphen') out.push('\u2011');
    for (const c of e.children) if (c.type === 'element') walk(c);
    if (e.name === 'w:p' || e.name === 'a:p') out.push('\n');
  };
  for (const name of [...pkg.partNames()].sort()) {
    const ct = pkg.getContentType(name) ?? '';
    if (!ct.endsWith('+xml') || IGNORED_PARTS.test(name)) continue;
    walk(parseXml(pkg.getPart(name) as Uint8Array).root);
  }
  return out.join('');
}

/** `modelOf(readDocx(bytes))` — the scorer's `model` callback for .docx files. */
export const docxModel = (bytes: Uint8Array) => modelOf(readDocx(bytes));

/** read → write with the fixed save time. */
export function rewrite(bytes: Uint8Array): Uint8Array {
  return writeDocx(readDocx(bytes), { now: SAVE_TIME });
}

export interface RoundTripOptions {
  /** Render both files with LibreOffice and compare pages (slow). */
  visual?: boolean;
}

/** Scores one file's round trip. Text uses the package-level extractor (independent of our reader). */
export async function scoreDocx(
  path: string,
  label: string,
  options: RoundTripOptions = {},
): Promise<FidelityScore> {
  const original = new Uint8Array(readFileSync(path));
  const rewritten = rewrite(original);
  return scoreRoundTrip({
    file: label,
    ext: 'docx',
    original,
    rewritten,
    model: docxModel,
    text: docxText,
    visual: options.visual ?? false,
  });
}

export const featureFixtures = () => listFixtures('docx', 'features');

export interface CorpusEntry {
  path: string;
  label: string;
  original: Uint8Array;
  rewritten: Uint8Array;
}

/**
 * Reads and rewrites every file. Files the reader rejects as not being Word documents are skipped (and
 * listed in `skipped`); any other failure throws with the file's name.
 */
export function rewriteAll(paths: readonly string[], skipped: string[] = []): CorpusEntry[] {
  const out: CorpusEntry[] = [];
  for (const path of paths) {
    const original = new Uint8Array(readFileSync(path));
    try {
      readDocx(original);
    } catch (e) {
      if (e instanceof DocxFormatError) {
        skipped.push(`${fixtureLabel(path)} (${e.message})`);
        continue;
      }
      throw e;
    }
    try {
      out.push({ path, label: fixtureLabel(path), original, rewritten: rewrite(original) });
    } catch (e) {
      throw new Error(`${fixtureLabel(path)}: ${(e as Error).message}`);
    }
  }
  return out;
}

/**
 * Scores a corpus. With `visual`, LibreOffice converts all originals and all rewritten files to PDF in
 * two batches (one start-up each, much faster than per-file conversions) and pages are compared at
 * 60 dpi; `oracle_opens` is true when the rewritten file is a valid package with a main part and
 * LibreOffice produced its PDF.
 */
export async function scoreCorpus(
  entries: readonly CorpusEntry[],
  options: RoundTripOptions = {},
): Promise<FidelityScore[]> {
  const scores: FidelityScore[] = entries.map((e) => ({
    file: e.label,
    structural: structuralScore(docxModel(e.original), docxModel(e.rewritten)),
    text: textScore(docxText(e.original), docxText(e.rewritten)),
    visual: null,
    oracle_opens: null,
  }));
  if (!sofficeAvailable()) return scores;
  const work = mkdtempSync(join(tmpdir(), 'officina-docx-fidelity-'));
  try {
    const inA = join(work, 'original');
    const inB = join(work, 'rewritten');
    mkdirSync(inA);
    mkdirSync(inB);
    const name = (e: CorpusEntry, i: number) => `${String(i).padStart(3, '0')}-${basename(e.path)}`;
    const pathsA = entries.map((e, i) => {
      const p = join(inA, name(e, i));
      writeFileSync(p, e.original);
      return p;
    });
    const pathsB = entries.map((e, i) => {
      const p = join(inB, name(e, i));
      writeFileSync(p, e.rewritten);
      return p;
    });
    const outB = await convertBatchWithSoffice(pathsB, 'pdf', join(work, 'pdf-rewritten'));
    const outA = options.visual
      ? await convertBatchWithSoffice(pathsA, 'pdf', join(work, 'pdf-original'))
      : [];
    const canRender = options.visual && pdftoppmAvailable();
    for (const [i, e] of entries.entries()) {
      const s = scores[i] as FidelityScore;
      let valid = false;
      try {
        valid = OpcPackage.open(e.rewritten).mainDocumentPart() !== undefined;
      } catch {
        valid = false;
      }
      const pdfB = outB.find((r) => r.input === pathsB[i])?.output;
      s.oracle_opens = valid && pdfB !== undefined;
      if (!canRender) continue;
      const pdfA = outA.find((r) => r.input === pathsA[i])?.output;
      if (!pdfB) {
        s.visual = 0;
        continue;
      }
      if (!pdfA) continue;
      s.visual = visualScore(
        await renderPdfPages(readFileSync(pdfA), { dpi: 60 }),
        await renderPdfPages(readFileSync(pdfB), { dpi: 60 }),
      );
    }
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  return scores;
}
