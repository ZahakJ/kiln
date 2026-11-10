import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { inflateRawSync } from 'node:zlib';
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

/**
 * Names and bytes of the entries of a zip archive, read from its central directory. Test-only: it
 * checks the committed corpus, it is not a document reader (those live in Rust).
 */
function zipEntries(bytes: Uint8Array): Map<string, () => Uint8Array> {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let eocd = bytes.length - 22;
  while (eocd >= 0 && view.getUint32(eocd, true) !== 0x06054b50) eocd--;
  if (eocd < 0) throw new Error('not a zip archive');
  const count = view.getUint16(eocd + 10, true);
  let pos = view.getUint32(eocd + 16, true);
  const entries = new Map<string, () => Uint8Array>();
  for (let i = 0; i < count; i++) {
    if (view.getUint32(pos, true) !== 0x02014b50) throw new Error('bad central directory');
    const method = view.getUint16(pos + 10, true);
    const compressedSize = view.getUint32(pos + 20, true);
    const nameLen = view.getUint16(pos + 28, true);
    const extraLen = view.getUint16(pos + 30, true);
    const commentLen = view.getUint16(pos + 32, true);
    const local = view.getUint32(pos + 42, true);
    const name = new TextDecoder().decode(bytes.subarray(pos + 46, pos + 46 + nameLen));
    entries.set(name, () => {
      const start = local + 30 + view.getUint16(local + 26, true) + view.getUint16(local + 28, true);
      const data = bytes.subarray(start, start + compressedSize);
      return method === 0 ? data : new Uint8Array(inflateRawSync(data));
    });
    pos += 46 + nameLen + extraLen + commentLen;
  }
  return entries;
}

const MAIN_PART: Record<string, string> = {
  docx: 'word/document.xml',
  xlsx: 'xl/workbook.xml',
  pptx: 'ppt/presentation.xml',
};

describe('fixture corpus', () => {
  it('has the documented layout', () => {
    for (const f of FIXTURE_FORMATS) {
      expect(existsSync(join(FIXTURES_DIR, f, 'features'))).toBe(true);
      expect(existsSync(join(FIXTURES_DIR, f, 'manual'))).toBe(true);
    }
  });

  it.each(FIXTURE_FORMATS)('has at least 15 generated %s files, each an OPC package', (format) => {
    const files = listFixtures(format);
    expect(files.length).toBeGreaterThanOrEqual(15);
    for (const f of files) {
      const entries = zipEntries(loadFixture(fixtureLabel(f)));
      expect(entries.has('[Content_Types].xml'), f).toBe(true);
      expect(entries.has('_rels/.rels'), f).toBe(true);
      expect(entries.has(MAIN_PART[format] as string), f).toBe(true);
      expect(entries.has('docProps/core.xml'), f).toBe(true);
    }
  });

  it('loads fixtures by relative path or bare name', () => {
    const a = loadFixture('docx/features/styles.docx');
    const b = loadFixture('styles.docx');
    expect(Buffer.from(a).equals(Buffer.from(b))).toBe(true);
    expect(loadFixture('styles.xlsx').length).toBeGreaterThan(0);
    expect(fixturePath('charts.pptx')).toMatch(/fixtures\/pptx\/features\/charts\.pptx$/);
    expect(() => loadFixture('missing.docx')).toThrow(FixtureNotFoundError);
    expect(() => loadFixture('docx/features/missing.docx')).toThrow(FixtureNotFoundError);
    expect(listFixtures('docx', 'external')).toEqual(expect.any(Array));
  });

  it('pins fixture dates for reproducibility', () => {
    const core = zipEntries(loadFixture('docx/features/properties.docx')).get('docProps/core.xml');
    expect(core).toBeDefined();
    expect(new TextDecoder().decode(core?.())).toContain('2026-01-15T09:30:00Z');
  });
});
