# kiln-ooxml

The Office Open XML foundation of Kiln (PLAN.md §3.10). The `.docx`, `.xlsx` and `.pptx` readers and
writers (`kiln-io-docx`, `kiln-io-xlsx`, `kiln-io-pptx`) and the chart code are built on it: the OPC zip
container, an order-preserving XML layer, simple types and units, DrawingML (colours, fills, lines, effects,
text properties, themes and the 187 preset geometries), the passthrough of parts nobody models, and document
properties. Principle: **never destroy what you don't understand** — a part the reader does not consume is
written back byte for byte.

```rust
use kiln_ooxml::opc::{OpcPackage, rel_type};
use kiln_ooxml::drawingml::presets::preset_geometry;

let pkg = OpcPackage::open(&bytes)?;                       // limits checked before inflating
let main = pkg.main_document_part()?.expect("a main part"); // "/word/document.xml"
let doc = pkg.part_xml(&main)?.expect("exists");            // prefixes normalised to w:, a:, r:, …
let styles = pkg.related_part(&main, rel_type::STYLES)?;    // Strict types match too
let saved = pkg.save()?;                                    // untouched parts byte-identical

let star = preset_geometry("star5", 914_400.0, 914_400.0, &[("adj", 19_098.0)])?;
let path = star.paths[0].to_bez_path(0.25);                 // kurbo::BezPath, arcs as cubics
```

## Module layout (format crates follow the same shape)

| Module | What it holds |
|---|---|
| `opc` | `OpcPackage` (open/save, parts, content types, relationships), `OpcLimits`, part-name helpers (`resolve_target`, `relative_target`, `rels_part_name`, `normalize_part_name`), `rel_type::*` and `content_type::*` constants |
| `xml` | `XmlElement`/`XmlNode`/`XmlDocument`, `parse_xml*`, `write_document`/`write_element`, the namespace table (`xml::namespaces`, `ns::*` URIs) |
| `st` | ECMA-376 simple types as enums (`as_str`, `parse`, `ALL`), `parse_on_off` (ST_OnOff) |
| `units` | EMU/twips/half-points/eighths/centipoints/angles/percentages, universal measures, Office rounding |
| `drawingml::color` | `DrawingColor` (srgb, scheme, sys, preset, hsl, scRGB + ordered transforms), `ColorMap` (`p:clrMap`/`w:clrSchemeMapping`), `ColorContext`, `resolve_color`, `apply_excel_tint` |
| `drawingml::fill` / `line` / `effects` | `Fill` (none/solid/gradient/pattern/picture/group), `Line` (+ `to_stroke` → `kurbo::Stroke`), `EffectList` |
| `drawingml::text` | `BodyProperties`, `ParagraphProperties`, `RunProperties`, `ListStyle`, inheritance merges |
| `drawingml::theme` | `Theme` (colour/font/format schemes), `office_theme()`, `resolve_theme_font`, `theme_color_context` |
| `drawingml::geometry` | `GeometryDef` (`a:custGeom`, round-trips), `CompiledGeometry` (guides resolved to slots), `ResolvedGeometry` (segments → `kurbo` paths or SVG, text rectangle, connection sites, handles) |
| `drawingml::presets` | the 187 ST_ShapeType presets (`data.rs`, generated), categories, `preset_geometry`, `preset_definition` |
| `passthrough` | `PassthroughSet::collect(pkg, consumed)` and `emit(target, map_part)` |
| `props` | core/app/custom document properties: `read_document_properties`, `write_document_properties` |

Conventions every format crate keeps:

- **Parse/serialise pairs.** Each model type has `parse_x(&XmlElement) -> X` and `serialize_x(&X) -> XmlElement`
  with `parse(serialize(x)) == x` (tested). Attributes are written in Office's order, children in schema order.
- **Unknown children survive.** Elements the model does not cover go to an `extras`/`other` field and are
  written back in schema position; whole parts go through `passthrough`.
- **Readers never panic on input.** Errors are `Result`s (`OpcError`, `XmlParseError`, `GeometryError`,
  `ThemeError`); values outside a simple type's enumeration read as absent.
- **Qualified names, not namespaces.** Trees use Office's prefixes (`w:p`, `a:srgbClr`, unprefixed
  SpreadsheetML), whatever the source file bound; `xml::namespaces` maps URIs, including ISO 29500 Strict.

## OPC — `OpcPackage`

- `open(bytes)` refuses OLE compound files (`EncryptedOrLegacy`: password-protected or legacy binary Office),
  non-zips and archives over the `OpcLimits` (65 536 parts, 2 GiB per part and in total, a compression ratio
  above 1000:1 for parts over 1 MiB) **before inflating anything**. Only `[Content_Types].xml` is inflated at
  open; parts inflate on first access, are cached, and an entry that inflates beyond its declared size is
  refused. A damaged entry fails when read, not at open.
- Part names are case-insensitive; zip entry names are percent-decoded. Directory entries are not parts.
- `relationships(source)` parses `.rels` lazily (`/` = package relationships); `related_part(s)`,
  `resolve_relationship`, `main_document_part`. Strict relationship types match their transitional twins.
- Editing: `add_part`/`add_xml_part`, `set_part`/`set_part_xml`, `put_part`, `remove_part` (with the
  relationships that target it), `add_relationship` (targets stored relative, as Office writes them),
  `remove_relationship`, `next_part_name("/word/media/image{n}.png")`. Content types are registered the way
  Office does (a `<Default>` for media extensions, an `<Override>` for XML parts).
- `save()` writes `[Content_Types].xml` first and `_rels/.rels` second, then the parts in their original order
  with new parts last. **An untouched part's zip entry is copied verbatim** (its compressed bytes included),
  so a repack is byte-identical part by part and takes milliseconds; new and changed parts are deflated at
  level 6 (media stored) with the 1980-01-01 timestamp, so output is reproducible.

## XML

`quick-xml` 0.42 underneath, with no DTD support at all: `<!DOCTYPE` is an error, so entity expansion
("billion laughs") and external entities have nothing to work with; only the five predefined entities and
character references expand. Nesting is capped at 2 000 levels (`ParseOptions::max_depth`); the writer and
`descendants` are iterative. Bytes may be UTF-8 (with or without BOM), UTF-16 (BOM or sniffed) or any encoding
an XML declaration names (`windows-1252`, `Shift_JIS`, … via `encoding_rs`). Whitespace-only text between
elements is dropped unless `xml:space="preserve"` applies. The writer escapes `& < >` (and CR) in text and
`& < " \t \n \r` in attributes, strips characters XML 1.0 forbids, writes no indentation, and declares any
prefix the tree uses but never declares on the root, in the stable table order (`mc:Ignorable` lists count).

## Geometry and presets

`GeometryDef::compile` resolves every guide reference once (built-ins, adjust values and guides become array
slots); evaluating a shape at a size is then a single pass, and presets are compiled once per process on
first use. Formula results that are not finite (division by zero in degenerate shapes) evaluate to 0, so no
NaN reaches a path. `ResolvedPath::to_bez_path(tolerance)` converts arcs (DrawingML's visual-angle
convention) to cubic Béziers; `to_svg()` gives SVG path data (used by the reference tests). The preset table
`src/drawingml/presets/data.rs` is generated:

```sh
python3 -I tools/port-presets.py           # regenerate from reference/ts-core/ooxml/src/drawingml/presets/
python3 -I tools/port-presets.py --check   # CI: the committed file is current
```

## Tests and benchmark

- `cargo nextest run -p kiln-ooxml` — every TS test ported (`tests/{opc,xml,drawingml,presets,passthrough_props,external}.rs`),
  the repack test over all 58 feature fixtures (`fixtures/{docx,xlsx,pptx}/features`: every part byte-identical
  inflated and compressed, entry order stable, second save identical), the hand-made edge cases in
  `fixtures/ooxml/` (missing content types, absolute and odd relationship targets, UTF-16/windows-1252/BOM
  parts, case and order oddities, Strict, deep nesting, billion laughs, a damaged entry, an encrypted file;
  rebuilt with `python3 -I fixtures/ooxml/make-fixtures.py`), and property tests for part-name normalisation
  and XML escaping. `tests/external.rs` also runs over the real-world corpus of `tools/fetch-corpus.sh` when
  it is present.
- `cargo run --release -p kiln-ooxml --example repack_bench` — open + repack of `fixtures/xlsx/large/*.xlsx`
  (budget 300 ms; 2 ms for the 22 MB-uncompressed 100k-row workbook on the reference machine). The workbook
  is gitignored; `python3 -I tools/make-fixtures.py --out fixtures --large --only xlsx` generates it.
- `cargo build --target wasm32-unknown-unknown -p kiln-ooxml` — the crate is pure Rust (deflate through
  `zlib-rs`) and runs in the web build.

Licence: AGPL-3.0-or-later. The preset definitions are transcribed from ECMA-376 Part 1 Annex D
(`presetShapeDefinitions.xml`, Ecma International).
