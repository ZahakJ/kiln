/**
 * Writer test helpers: round trips of hand-written packages and of models, part access, and the
 * comparison of two read results through the fidelity model.
 */
import { OpcPackage, RelType } from '@officina/ooxml';
import type { Node as PMNode } from '@tiptap/pm/model';
import { expect } from 'vitest';
import {
  type DocxWriteModel,
  type ReadDocxResult,
  readDocx,
  type WriteDocxOptions,
  writeDocx,
} from '../../src/index.ts';
import { buildDocx } from '../read/helpers.ts';
import { modelOf, SAVE_TIME } from '../roundtrip/model.ts';

export { buildDocx, part } from '../read/helpers.ts';

export const P = (inner: string, pPr = '') => `<w:p>${pPr ? `<w:pPr>${pPr}</w:pPr>` : ''}${inner}</w:p>`;
export const R = (text: string, rPr = '') =>
  `<w:r>${rPr ? `<w:rPr>${rPr}</w:rPr>` : ''}<w:t xml:space="preserve">${text}</w:t></w:r>`;

export type BuildOptions = Parameters<typeof buildDocx>[0];

export function write(model: DocxWriteModel | ReadDocxResult, options: WriteDocxOptions = {}): Uint8Array {
  return writeDocx(model, { now: SAVE_TIME, ...options });
}

export interface RoundTrip {
  original: Uint8Array;
  a: ReadDocxResult;
  bytes: Uint8Array;
  b: ReadDocxResult;
  /** Text of a written part. */
  xml: (part: string) => string;
}

/** read → write → read of a package. */
export function roundTripBytes(original: Uint8Array, options: WriteDocxOptions = {}): RoundTrip {
  const a = readDocx(original);
  const bytes = write(a, options);
  const b = readDocx(bytes);
  const pkg = OpcPackage.open(bytes);
  return { original, a, bytes, b, xml: (name) => pkg.getPartText(name) ?? '' };
}

/** Builds a package from snippets (see `buildDocx`) and round-trips it. */
export function roundTrip(
  body: string,
  extra: Partial<BuildOptions> = {},
  options: WriteDocxOptions = {},
): RoundTrip {
  return roundTripBytes(buildDocx({ body, ...extra }), options);
}

/**
 * The two reads agree on everything the fidelity model compares (ids included). A package without
 * settings or theme parts gets Word's defaults written for them, so those are only compared when the
 * original had them.
 */
export function expectSameModel(rt: RoundTrip, options: { ignoreWarnings?: boolean } = {}): void {
  const a = modelOf(rt.a);
  const b = modelOf(rt.b);
  if (rt.a.extras.settingsXml === undefined) {
    delete (a.extras as Record<string, unknown>).settingsXml;
    delete (b.extras as Record<string, unknown>).settingsXml;
  }
  const original = OpcPackage.open(rt.original);
  if (!original.relatedPart(rt.a.mainPart, RelType.theme)) {
    delete a.theme;
    delete b.theme;
  }
  // A save stamps a missing creation date.
  if (rt.a.properties.core.created === undefined) delete (b.core as Record<string, unknown>).created;
  if (options.ignoreWarnings) {
    delete a.warnings;
    delete b.warnings;
  }
  expect(b).toEqual(a);
}

export function nodes(doc: PMNode, type: string): PMNode[] {
  const out: PMNode[] = [];
  doc.descendants((n) => {
    if (n.type.name === type) out.push(n);
  });
  return out;
}

export function marksOn(doc: PMNode, text: string): Record<string, Record<string, unknown>> {
  let found: PMNode | undefined;
  doc.descendants((n) => {
    if (!found && n.isText && n.text?.includes(text)) found = n;
  });
  if (!found) throw new Error(`no text ${text}`);
  return Object.fromEntries(found.marks.map((m) => [m.type.name, m.attrs]));
}

/** Index of each needle in `xml`, asserting that they appear in this order. */
export function expectOrder(xml: string, needles: readonly string[]): void {
  let at = -1;
  for (const n of needles) {
    const i = xml.indexOf(n, at + 1);
    expect(i, `${n} after position ${at} in ${xml.slice(0, 400)}`).toBeGreaterThan(at);
    at = i;
  }
}

export const count = (xml: string, needle: string) => xml.split(needle).length - 1;

export const PNG = new Uint8Array([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0,
  1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89, 0, 0, 0, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8,
  0xcf, 0xc0, 0xf0, 0x1f, 0, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0, 0, 0, 0, 0x49, 0x45, 0x4e,
  0x44, 0xae, 0x42, 0x60, 0x82,
]);
