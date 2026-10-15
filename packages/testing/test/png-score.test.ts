import { buildXml, child, OpcPackage, parseXml } from '@officina/ooxml';
import { describe, expect, it } from 'vitest';
import {
  compareImages,
  decodePng,
  encodePng,
  formatScoreTable,
  loadFixture,
  normalizeText,
  packageModel,
  packageText,
  scoreRoundTrip,
  solidImage,
  structuralScore,
  textScore,
  visualScore,
} from '../src/index.ts';

describe('PNG codec', () => {
  it('round-trips RGBA images', () => {
    const img = solidImage(7, 5, [10, 20, 30, 255]);
    img.data.set([255, 0, 0, 128], (2 * 7 + 3) * 4);
    const back = decodePng(encodePng(img));
    expect(back.width).toBe(7);
    expect(back.height).toBe(5);
    expect(Buffer.from(back.data).equals(Buffer.from(img.data))).toBe(true);
  });

  it('rejects other data', () => {
    expect(() => decodePng(new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]))).toThrow();
  });
});

describe('structural score', () => {
  it('is 1 for identical models and ignores identifiers', () => {
    const a = { body: [{ p: { text: 'a', id: '1' } }, { p: { text: 'b', 'w:rsidR': '00AB' } }] };
    const b = { body: [{ p: { text: 'a', id: '9' } }, { p: { text: 'b', 'w:rsidR': '00CD' } }] };
    expect(structuralScore(a, b)).toBe(1);
    expect(structuralScore({}, {})).toBe(1);
  });

  it('counts differing, missing and extra leaves over the union', () => {
    const a = { x: 1, y: 2, z: [1, 2] };
    expect(structuralScore(a, { x: 1, y: 3, z: [1, 2] })).toBeCloseTo(3 / 4);
    expect(structuralScore(a, { x: 1, y: 2, z: [1] })).toBeCloseTo(3 / 4);
    expect(structuralScore(a, { x: 1, y: 2, z: [1, 2], w: 0 })).toBeCloseTo(4 / 5);
    expect(structuralScore({ a: 1 }, { b: 1 })).toBe(0);
    expect(structuralScore({ v: '1' }, { v: 1 })).toBe(0);
    expect(structuralScore({ bin: new Uint8Array([1, 2]) }, { bin: new Uint8Array([1, 3]) })).toBe(0);
  });
});

describe('text score', () => {
  it('normalises whitespace, quotes and Unicode', () => {
    expect(normalizeText('  “Hello” \n world  ')).toBe('"Hello" world');
    expect(textScore('Café au lait', 'Café  au\tlait')).toBe(1);
  });

  it('scores differences below 1 by word edit distance', () => {
    expect(textScore('one two three four', 'one two three five')).toBeCloseTo(0.75);
    expect(textScore('a b c d', '')).toBeCloseTo(0);
    expect(textScore('same', 'same')).toBe(1);
    expect(textScore('a b', 'a b c')).toBeLessThan(1);
  });
});

describe('visual score', () => {
  it('compares rasters pixel by pixel', () => {
    const white = solidImage(10, 10, [255, 255, 255, 255]);
    const half = solidImage(10, 10, [255, 255, 255, 255]);
    for (let i = 0; i < 50; i++) half.data.set([0, 0, 0, 255], i * 4);
    expect(compareImages(white, white).score).toBe(1);
    expect(compareImages(white, half).score).toBeCloseTo(0.5);
    // A smaller page is padded with white.
    expect(compareImages(white, solidImage(10, 5, [255, 255, 255, 255])).score).toBe(1);
  });

  it('averages pages and counts missing pages as 0', () => {
    const page = encodePng(solidImage(4, 4, [255, 255, 255, 255]));
    const black = encodePng(solidImage(4, 4, [0, 0, 0, 255]));
    expect(visualScore([page, page], [page, page])).toBe(1);
    expect(visualScore([page, page], [page])).toBe(0.5);
    expect(visualScore([page], [black])).toBe(0);
    expect(visualScore([], [])).toBe(1);
  });
});

describe('default package model and text', () => {
  it('extracts visible text from each format', () => {
    expect(packageText(loadFixture('docx/features/styles.docx'))).toContain('Text styles matrix');
    expect(packageText(loadFixture('pptx/features/notes.pptx'))).toContain('Speaker notes for slide 1');
    expect(packageText(loadFixture('xlsx/features/rich-text.xlsx'))).toContain('Shared string repeated');
  });

  it('scores an unchanged round trip as 1 and a text edit below 1', async () => {
    const original = loadFixture('docx/features/paragraphs.docx');
    const same = OpcPackage.open(original).save();
    expect(structuralScore(packageModel(original), packageModel(same))).toBe(1);

    const pkg = OpcPackage.open(original);
    const doc = parseXml(pkg.getPart('/word/document.xml') as Uint8Array);
    const body = child(doc.root, 'w:body');
    body?.children.splice(0, 1);
    pkg.setPart('/word/document.xml', buildXml(doc));
    const edited = pkg.save();
    const s = await scoreRoundTrip({ file: 'paragraphs.docx', ext: 'docx', original, rewritten: same });
    expect(s.structural).toBe(1);
    expect(s.text).toBe(1);
    const e = await scoreRoundTrip({ file: 'paragraphs.docx', ext: 'docx', original, rewritten: edited });
    expect(e.structural).toBeLessThan(1);
    expect(e.text).toBeLessThan(1);
    expect(e.text).toBeGreaterThan(0.9);
    const table = formatScoreTable([s, e]);
    expect(table.split('\n')).toHaveLength(4);
    expect(table).toContain('| paragraphs.docx | 1.000 | 1.000 |');
  });
});
