import { unzipSync } from 'fflate';
import { describe, expect, it } from 'vitest';
import { parseGeometry, resolveGeometry } from '../src/drawingml/geometry.ts';
import { isPresetShapeType, presetGeometry } from '../src/drawingml/presets/index.ts';
import { parseTheme, serializeTheme } from '../src/drawingml/theme.ts';
import { OpcPackage } from '../src/opc/package.ts';
import { RelType } from '../src/opc/rel-types.ts';
import { PassthroughSet } from '../src/passthrough.ts';
import { readDocumentProperties } from '../src/props.ts';
import { buildXml } from '../src/xml/build.ts';
import { XmlParseError } from '../src/xml/parse.ts';
import { attr, descendants } from '../src/xml/query.ts';
import { externalFixtures, fixture } from './fixtures.ts';

/**
 * Real-world files from `tools/fetch-corpus.sh`. Skipped when the corpus has not been downloaded
 * (fresh clones, CI); run `bash tools/fetch-corpus.sh` to enable.
 */
const FILES = externalFixtures();
/** POI's deliberately hostile document: 15 000 nested elements. It must be refused cleanly, not crash. */
const HOSTILE = new Set(['external/docx/poi-deep-table-cell.docx']);

describe.skipIf(FILES.length === 0)('real-world corpus', () => {
  it.each(FILES)('%s round-trips byte-identically and parses fully', (name) => {
    const bytes = fixture(name);
    const pkg = OpcPackage.open(bytes);
    const src = unzipSync(bytes);
    const out = unzipSync(pkg.save());
    for (const [entry, data] of Object.entries(src)) {
      if (entry.endsWith('/')) continue;
      expect(Buffer.from(out[entry] as Uint8Array).equals(Buffer.from(data)), entry).toBe(true);
    }
    const main = pkg.mainDocumentPart();
    expect(main).toBeDefined();
    if (HOSTILE.has(name)) {
      expect(() => pkg.getPartXml(main as string)).toThrow(XmlParseError);
      expect(() => pkg.getPartXml(main as string)).toThrow(/nested/);
      return;
    }
    // Every XML part parses and re-serialises to XML that parses to the same tree.
    for (const part of pkg.partNames()) {
      const ct = pkg.getContentType(part) ?? '';
      if (!ct.endsWith('xml')) continue;
      const doc = pkg.getPartXml(part);
      expect(doc, part).toBeDefined();
      const again = OpcPackage.create();
      again.addPart('/x.xml', 'application/xml', buildXml(doc as NonNullable<typeof doc>));
      expect(again.getPartXml('/x.xml')?.root).toEqual(doc?.root);
    }
    // Themes go through the model losslessly.
    for (const part of pkg.partNames().filter((p) => /\/theme\d*\.xml$/.test(p))) {
      const t = parseTheme(pkg.getPart(part) as Uint8Array);
      expect(parseTheme(buildXml(serializeTheme(t)))).toEqual(t);
    }
    // Every preset and custom geometry evaluates.
    for (const part of pkg.partNames().filter((p) => (pkg.getContentType(p) ?? '').endsWith('xml'))) {
      const root = pkg.getPartXml(part)?.root;
      if (!root) continue;
      for (const g of descendants(root, 'a:prstGeom')) {
        const prst = attr(g, 'prst') as string;
        expect(isPresetShapeType(prst), `${part}: ${prst}`).toBe(true);
        const adj = Object.fromEntries(
          descendants(g, 'a:gd').map((gd) => [
            attr(gd, 'name'),
            Number((attr(gd, 'fmla') ?? 'val 0').split(' ')[1]),
          ]),
        );
        const geo = presetGeometry(prst, 914400, 457200, adj);
        expect(
          geo.paths.every((p) => !/NaN/.test(p.d)),
          prst,
        ).toBe(true);
      }
      for (const g of descendants(root, 'a:custGeom')) {
        const geo = resolveGeometry(parseGeometry(g), 914400, 457200);
        expect(geo.paths.every((p) => !/NaN/.test(p.d))).toBe(true);
      }
    }
    readDocumentProperties(pkg);
    // Passthrough with only the main part consumed re-creates every other part byte-for-byte.
    const set = PassthroughSet.collect(pkg, { consumedParts: [main as string] });
    const target = OpcPackage.create();
    target.addPart(
      main as string,
      pkg.getContentType(main as string) as string,
      pkg.getPart(main as string) as Uint8Array,
    );
    target.addRelationship('/', RelType.officeDocument, main as string);
    set.emit(target);
    const rebuilt = OpcPackage.open(target.save());
    for (const p of pkg.partNames()) {
      expect(
        Buffer.from(rebuilt.getPart(p) as Uint8Array).equals(Buffer.from(pkg.getPart(p) as Uint8Array)),
        p,
      ).toBe(true);
    }
  });
});
