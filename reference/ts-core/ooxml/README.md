# @kiln-ref/ooxml

The Office Open XML core shared by the .docx, .xlsx and .pptx readers and writers: the OPC zip container,
XML (de)serialisation, relationships and content types, units and simple types, DrawingML (colours, fills,
lines, effects, text properties, themes, preset and custom geometry), opaque-part passthrough and document
properties. Principle (PLAN §3.5): never destroy what you don't understand.

**Public API** (all from `src/index.ts`):

- **OPC** — `OpcPackage.open(bytes, limits?)` (zip-bomb limits checked before inflating; password-protected
  and legacy binary files raise `EncryptedOrLegacyFileError`), `OpcPackage.create()`, `getPart`/`getPartXml`,
  `setPart`, `addPart(name, contentType, bytes)`, `removePart`, `nextPartName('/word/media/image{n}.png')`,
  `relationships(source)` (typed lookup `byType`, `firstByType`, `resolve`, `add`, `remove`),
  `relatedParts(source, RelType.x)`, `addRelationship`, `mainDocumentPart()`, `save()` (Office order:
  `[Content_Types].xml`, then `_rels/.rels`; untouched parts byte-identical; deterministic timestamps).
  Helpers: `RelType`, `ContentType`, `resolveTarget`, `relativeTarget`, `relsPartName`.
- **XML** — `parseXml` (fast-xml-parser, `preserveOrder`; DTDs refused; prefixes normalised to the
  Office table `w a p r c x14 mc wp pic wps m xdr dgm vt cp dc dcterms …`), `buildXml` (declares used
  namespaces only, keeps `mc:Ignorable` prefixes declared, strips characters XML forbids), the node
  builder `el(name, attrs, ...children)` and queries `child`, `children`, `childPath`, `descendants`,
  `attr`, `attrInt`, `attrBool`, `toggle`, `text`, `selectAlternateContent`.
- **Units and simple types** — EMU, twips, half-points, eighths, angles, percentages, universal measures;
  `ST_*` enumerations as `const` tuples with string-union types and guards.
- **DrawingML** — `parseColor`/`resolveColor` (srgb, scheme with colour map and `phClr`, sys, preset,
  HSL, scRGB; lumMod/lumOff/tint/shade/alpha/… transforms), `parseFill`/`serializeFill`/`fillToCss`,
  `parseLine`/`serializeLine`/`dashArrayPx`, `parseEffectList`/`outerShadowToCss`, `parseBodyProperties`,
  `parseParagraphProperties`, `parseRunProperties`, `parseListStyle` (+ serialisers and inheritance
  merges), `parseTheme`/`serializeTheme`/`officeTheme()`/`resolveThemeFont`.
- **Geometry** — `presetToSvgPath(preset, w, h, adjustments)`, `presetGeometry` (paths with fill/stroke
  flags, text rectangle, connection sites, handles), `PRESET_CATEGORIES`, `parseGeometry` /
  `serializeCustomGeometry` for `a:custGeom`.
- **Passthrough** — `PassthroughSet.collect(pkg, { consumedParts })` and `emit(target, { mapPart })`.
- **Properties** — `readDocumentProperties`, `writeDocumentProperties` (core, app, custom).

The 187 preset definitions in `src/drawingml/presets/` are transcribed from ECMA-376 Part 1 Annex D
(`presetShapeDefinitions.xml`, Ecma International, royalty-free standard) by `pnpm gen:presets`
(`scripts/gen-presets.ts`); do not edit them by hand.

Scripts: `pnpm lint`, `pnpm typecheck`, `pnpm test` (the OPC round-trip tests read the generated corpus
in `packages/testing/fixtures`; real-world files from `tools/fetch-corpus.sh` are tested when present).

Licence: AGPL-3.0-or-later.
