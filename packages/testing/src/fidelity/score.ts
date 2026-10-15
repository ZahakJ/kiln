/**
 * Round-trip fidelity scoring (PLAN §5): for an original file and our rewritten version,
 *
 * - `structural` — fraction of model leaves (path → value) that are equal after read → write → read,
 *   ignoring identifiers;
 * - `text` — normalised text equality (1) or, when different, a word-level similarity below 1;
 * - `visual` — 1 − differing-pixel ratio between LibreOffice renderings of both files at 60 dpi,
 *   page by page (pixelmatch);
 * - `oracle_opens` — LibreOffice converts the rewritten file without error.
 *
 * WP-39 enforces the §3.5 thresholds on these numbers; this module provides the measurements.
 */
import { OpcPackage, parseXml, type XmlElement } from '@officina/ooxml';
import pixelmatch from 'pixelmatch';
import {
  convertWithSoffice,
  pdftoppmAvailable,
  renderPdfPages,
  sofficeAvailable,
} from '../oracles/soffice.ts';
import { decodePng, type RgbaImage } from '../png.ts';

export interface FidelityScore {
  file: string;
  structural: number;
  text: number;
  /** `null` when visual scoring was not requested or LibreOffice is unavailable. */
  visual: number | null;
  /** `null` when LibreOffice is unavailable. */
  oracle_opens: boolean | null;
}

// ------------------------------------------------------------------ structural

export interface StructuralOptions {
  /** Keys whose values are ignored (identifiers, revision ids). */
  ignoreKey?: (key: string) => boolean;
}

/** Keys that hold identifiers rather than content. */
export const DEFAULT_IGNORED_KEY = (key: string): boolean =>
  /^(id|.*:id|.*Id|.*:rsid\w*|rsid\w*|w14:paraId|w14:textId|.*:uid|uid|docPr|creationId)$/.test(key);

function flatten(
  value: unknown,
  path: string,
  out: Map<string, string>,
  ignore: (k: string) => boolean,
): void {
  if (value === null || value === undefined) {
    out.set(path, 'null');
    return;
  }
  if (Array.isArray(value)) {
    if (value.length === 0) out.set(path, '[]');
    for (const [i, v] of value.entries()) flatten(v, `${path}[${i}]`, out, ignore);
    return;
  }
  if (value instanceof Uint8Array) {
    out.set(path, `bytes:${value.length}:${hashBytes(value)}`);
    return;
  }
  if (typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>).filter(([k]) => !ignore(k));
    if (entries.length === 0) out.set(path, '{}');
    for (const [k, v] of entries) flatten(v, `${path}.${k}`, out, ignore);
    return;
  }
  out.set(path, typeof value === 'string' ? `s:${value}` : `${typeof value}:${String(value)}`);
}

function hashBytes(b: Uint8Array): string {
  let h = 2166136261;
  for (const x of b) h = Math.imul(h ^ x, 16777619);
  return (h >>> 0).toString(16);
}

/**
 * Fraction of leaves equal in both models, over the union of leaf paths (1 = identical, 0 = disjoint).
 */
export function structuralScore(a: unknown, b: unknown, options: StructuralOptions = {}): number {
  const ignore = options.ignoreKey ?? DEFAULT_IGNORED_KEY;
  const fa = new Map<string, string>();
  const fb = new Map<string, string>();
  flatten(a, '$', fa, ignore);
  flatten(b, '$', fb, ignore);
  const keys = new Set([...fa.keys(), ...fb.keys()]);
  if (keys.size === 0) return 1;
  let same = 0;
  for (const k of keys) if (fa.get(k) === fb.get(k)) same++;
  return same / keys.size;
}

// ------------------------------------------------------------------------ text

/** Unicode NFC, unified line breaks and quotes, collapsed whitespace, trimmed. */
export function normalizeText(s: string): string {
  return s
    .normalize('NFC')
    .replace(/[‘’]/g, "'")
    .replace(/[“”]/g, '"')
    .replace(/[ \s]+/g, ' ')
    .trim();
}

/** 1 when the normalised texts are equal; otherwise 1 − word edit distance / longer length. */
export function textScore(a: string, b: string): number {
  const na = normalizeText(a);
  const nb = normalizeText(b);
  if (na === nb) return 1;
  const wa = na ? na.split(' ') : [];
  const wb = nb ? nb.split(' ') : [];
  const n = Math.max(wa.length, wb.length);
  if (n === 0) return 1;
  if (wa.length * wb.length > 25_000_000) {
    // Too long for an exact edit distance: compare word multisets instead.
    const count = new Map<string, number>();
    for (const w of wa) count.set(w, (count.get(w) ?? 0) + 1);
    let common = 0;
    for (const w of wb) {
      const c = count.get(w) ?? 0;
      if (c > 0) {
        common++;
        count.set(w, c - 1);
      }
    }
    return Math.min(common / n, 0.999999);
  }
  let prev = new Uint32Array(wb.length + 1).map((_, j) => j);
  for (let i = 1; i <= wa.length; i++) {
    const cur = new Uint32Array(wb.length + 1);
    cur[0] = i;
    for (let j = 1; j <= wb.length; j++) {
      const cost = wa[i - 1] === wb[j - 1] ? 0 : 1;
      cur[j] = Math.min((prev[j] as number) + 1, (cur[j - 1] as number) + 1, (prev[j - 1] as number) + cost);
    }
    prev = cur;
  }
  return Math.min(1 - (prev[wb.length] as number) / n, 0.999999);
}

// ---------------------------------------------------------------------- visual

function padTo(img: RgbaImage, width: number, height: number): Uint8Array {
  if (img.width === width && img.height === height) return img.data;
  const out = new Uint8Array(width * height * 4).fill(255);
  for (let y = 0; y < img.height; y++) {
    out.set(img.data.subarray(y * img.width * 4, (y + 1) * img.width * 4), y * width * 4);
  }
  return out;
}

/** 1 − differing-pixel ratio of two rasters (pages of different sizes are padded with white). */
export function compareImages(
  a: RgbaImage,
  b: RgbaImage,
  threshold = 0.1,
): { score: number; differing: number } {
  const w = Math.max(a.width, b.width);
  const h = Math.max(a.height, b.height);
  if (w === 0 || h === 0) return { score: 1, differing: 0 };
  const differing = pixelmatch(padTo(a, w, h), padTo(b, w, h), undefined, w, h, { threshold });
  return { score: 1 - differing / (w * h), differing };
}

/**
 * Average page score over the longer document; a page missing from either side scores 0. Accepts PNG
 * buffers (as returned by `renderPdfPages`).
 */
export function visualScore(
  pagesA: readonly Uint8Array[],
  pagesB: readonly Uint8Array[],
  threshold = 0.1,
): number {
  const n = Math.max(pagesA.length, pagesB.length);
  if (n === 0) return 1;
  let total = 0;
  for (let i = 0; i < n; i++) {
    const pa = pagesA[i];
    const pb = pagesB[i];
    if (!pa || !pb) continue;
    total += compareImages(decodePng(pa), decodePng(pb), threshold).score;
  }
  return total / n;
}

// ---------------------------------------------------------------- oracle_opens

/**
 * True when the file is a well-formed package with a main document *and* LibreOffice converts it to
 * PDF; `null` when LibreOffice is unavailable. (LibreOffice alone is too lenient: it imports any byte
 * soup as plain text, so the package check comes first.)
 */
export async function oracleOpens(bytes: Uint8Array, ext: string): Promise<boolean | null> {
  if (!sofficeAvailable()) return null;
  try {
    if (!OpcPackage.open(bytes).mainDocumentPart()) return false;
  } catch {
    return false;
  }
  try {
    await convertWithSoffice({ bytes, name: `rewritten.${ext}` }, 'pdf');
    return true;
  } catch {
    return false;
  }
}

// ------------------------------------------------------------- default models

/** Parts whose content is presentation-irrelevant bookkeeping and excluded from the default model. */
const IGNORED_PARTS = /^\/(docProps\/(core|app)\.xml|\[trash\]\/.*)$/i;

/**
 * The default structural model of an OOXML package: every XML part (except document properties) as a
 * parsed tree, every binary part as a byte hash. Readers' own models (WP-11/18/24) replace this once
 * they exist.
 */
export function packageModel(bytes: Uint8Array): Record<string, unknown> {
  const pkg = OpcPackage.open(bytes);
  const model: Record<string, unknown> = {};
  for (const name of pkg.partNames().sort()) {
    if (IGNORED_PARTS.test(name)) continue;
    const ct = pkg.getContentType(name) ?? '';
    const data = pkg.getPart(name) as Uint8Array;
    model[name] = ct.endsWith('xml') ? toPlain(parseXml(data).root) : data;
  }
  return model;
}

function toPlain(e: XmlElement): unknown {
  const kids = e.children
    .map((c) => (c.type === 'element' ? toPlain(c) : c.type === 'text' ? c.text : undefined))
    .filter((c) => c !== undefined);
  return { [e.name]: { ...e.attrs, ...(kids.length ? { _: kids } : {}) } };
}

const TEXT_ELEMENTS = new Set(['w:t', 'w:delText', 'a:t', 't', 'm:t']);

/** Visible text of an OOXML package (WordprocessingML, DrawingML and SpreadsheetML text runs), in part order. */
export function packageText(bytes: Uint8Array): string {
  const pkg = OpcPackage.open(bytes);
  const out: string[] = [];
  const walk = (e: XmlElement) => {
    if (TEXT_ELEMENTS.has(e.name)) {
      for (const c of e.children) if (c.type === 'text') out.push(c.text);
      return;
    }
    if (e.name === 'w:p' || e.name === 'a:p' || e.name === 'si') {
      for (const c of e.children) if (c.type === 'element') walk(c);
      out.push('\n');
      return;
    }
    if (e.name === 'w:tab') out.push('\t');
    for (const c of e.children) if (c.type === 'element') walk(c);
  };
  for (const name of pkg.partNames().sort()) {
    const ct = pkg.getContentType(name) ?? '';
    if (
      !ct.endsWith('+xml') ||
      IGNORED_PARTS.test(name) ||
      /theme|styles|settings|fontTable|webSettings/i.test(name)
    )
      continue;
    walk(parseXml(pkg.getPart(name) as Uint8Array).root);
  }
  return out.join('');
}

// ------------------------------------------------------------------- the score

export interface ScoreRoundTripInput {
  /** Label for reports (usually the fixture name). */
  file: string;
  /** `docx`, `xlsx` or `pptx`. */
  ext: string;
  original: Uint8Array;
  rewritten: Uint8Array;
  /** Our reader's model; defaults to `packageModel`. */
  model?: (bytes: Uint8Array) => unknown;
  /** Text extraction; defaults to `packageText`. */
  text?: (bytes: Uint8Array) => string;
  /** Render both files with LibreOffice and compare pages (slow). Default `false`. */
  visual?: boolean;
  /** Raster resolution for the visual score. Default 60 dpi. */
  dpi?: number;
}

/** Scores one round trip. */
export async function scoreRoundTrip(input: ScoreRoundTripInput): Promise<FidelityScore> {
  const model = input.model ?? packageModel;
  const text = input.text ?? packageText;
  const score: FidelityScore = {
    file: input.file,
    structural: structuralScore(model(input.original), model(input.rewritten)),
    text: textScore(text(input.original), text(input.rewritten)),
    visual: null,
    oracle_opens: null,
  };
  if (!sofficeAvailable()) return score;
  if (input.visual && pdftoppmAvailable()) {
    try {
      const [a, b] = await Promise.all([
        convertWithSoffice({ bytes: input.original, name: `original.${input.ext}` }, 'pdf'),
        convertWithSoffice({ bytes: input.rewritten, name: `rewritten.${input.ext}` }, 'pdf'),
      ]);
      score.oracle_opens = true;
      const dpi = input.dpi ?? 60;
      score.visual = visualScore(
        await renderPdfPages(a.bytes, { dpi }),
        await renderPdfPages(b.bytes, { dpi }),
      );
    } catch {
      score.oracle_opens = await oracleOpens(input.rewritten, input.ext);
      score.visual = score.oracle_opens ? null : 0;
    }
  } else {
    score.oracle_opens = await oracleOpens(input.rewritten, input.ext);
  }
  return score;
}

const fmt = (n: number | null) => (n === null ? '—' : n.toFixed(3));

/** A Markdown table of scores, for `pnpm fidelity` and `docs/qa/fidelity.md`. */
export function formatScoreTable(scores: readonly FidelityScore[]): string {
  const rows = scores.map(
    (s) =>
      `| ${s.file} | ${fmt(s.structural)} | ${fmt(s.text)} | ${fmt(s.visual)} | ${s.oracle_opens === null ? '—' : s.oracle_opens ? 'yes' : 'no'} |`,
  );
  return [
    '| File | Structural | Text | Visual | LibreOffice opens |',
    '|---|---:|---:|---:|:---:|',
    ...rows,
  ].join('\n');
}
