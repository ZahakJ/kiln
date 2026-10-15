/**
 * Text fidelity against an independent reader: for every generated feature fixture, the body text we
 * read equals mammoth's raw text, paragraph by paragraph.
 *
 * mammoth's text model differs from Word's in documented ways; the comparison follows it rather than
 * hiding differences:
 * - line breaks inside a paragraph produce no text, tabs are kept, deleted text is omitted;
 * - headers, footers, footnotes/endnotes and text boxes are not part of the body text;
 * - mammoth ignores `w:fldSimple` (it warns "unrecognised element"), so the oracle is given the file
 *   with simple fields unwrapped into their result runs — the content Word displays.
 */
import { readFileSync } from 'node:fs';
import { OpcPackage } from '@officina/ooxml';
import { fixtureLabel, listFixtures } from '@officina/testing';
import { describe, expect, it } from 'vitest';
import { readDocx } from '../../src/index.ts';
import { mammothModelParagraphs, mammothParagraphs, mammothText } from './helpers.ts';

/** The oracle input: simple fields unwrapped so mammoth sees their displayed result. */
function unwrapSimpleFields(bytes: Uint8Array): Uint8Array {
  const pkg = OpcPackage.open(bytes);
  const xml = pkg.getPartText('/word/document.xml') ?? '';
  if (!xml.includes('fldSimple')) return bytes;
  const unwrapped = xml
    .replace(/<w:fldSimple\b[^>]*\/>/g, '')
    .replace(/<w:fldSimple\b[^>]*>|<\/w:fldSimple>/g, '');
  pkg.setPart('/word/document.xml', unwrapped);
  return pkg.save();
}

const normalise = (s: string) => s.replace(/[  ]+/g, ' ').replace(/ +$/g, '');

const fixtures = listFixtures('docx', 'features');

describe('body text equals mammoth (normalised)', () => {
  it('has the generated feature corpus', () => {
    expect(fixtures.length).toBeGreaterThanOrEqual(15);
  });

  for (const path of fixtures) {
    it(fixtureLabel(path), async () => {
      const bytes = new Uint8Array(readFileSync(path));
      const ours = mammothModelParagraphs(readDocx(bytes).doc).map(normalise);
      const theirs = mammothParagraphs(await mammothText(unwrapSimpleFields(bytes))).map(normalise);
      expect(theirs.join('').trim().length).toBeGreaterThan(0);
      expect(ours).toEqual(theirs);
    });
  }
});
