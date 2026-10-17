# kiln-io-pptx

Reads `.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx` and `.ppsm` files into the Kiln Deck model
(`kiln_deck::Presentation`), keeping every part it does not understand (reader, WP-77, `src/read/`),
and writes the model back to any of those formats (writer, WP-79, `src/write/`).

```rust
let read = kiln_io_pptx::read_pptx(&bytes)?;
let deck = &read.presentation;            // validated: Presentation::validate passes
println!("{}", kiln_io_pptx::Summary::of(&read).to_text());
for w in &read.warnings { eprintln!("{w}"); } // what the Compatibility Checker lists
```

`kiln-cli inspect --pptx <file>` prints the same summary (masters and layouts, one line per slide
with its layout, shape count and title, counts, diagnostics); `--json` gives it to scripts.

```rust
// Save: the read's side data and the original package make the save faithful.
let pkg = kiln_ooxml::opc::OpcPackage::open(&bytes)?;
let read = kiln_io_pptx::read_package(&pkg)?;
let source = kiln_io_pptx::WriteSource::from_read(&read).with_package(&pkg);
let options = kiln_io_pptx::WriteOptions {
    now: Some("2026-10-16T09:30:00Z".into()),  // stamps dcterms:modified, the revision, "Kiln"
    preserve: Some(&touched),                  // TouchedParts recorded from the session's edits
    ..Default::default()
};
let written = kiln_io_pptx::write_pptx(&deck, Some(&source), &options)?;
let report = kiln_io_pptx::compat_report(&deck, kiln_io_pptx::CompatOptions { .. }); // the Compatibility Checker
```

`kiln-cli convert in.pptx out.pptx` (or `.pptm`, `.potx`, `.potm`, `.ppsx`, `.ppsm`) saves a
presentation the way Kiln does and prints what was written, what was carried over and the
Compatibility Checker's findings.

## What is read

| Part | Into the model |
|---|---|
| `presentation.xml` | slide and notes size, `firstSlideNum`, the master, notes master and slide lists, `p:defaultTextStyle`, sections (`p14:sectionLst`) |
| `slideMasters/` | theme (normalised), colour map, background, shapes, title/body/other text styles, `p:hf`, the layouts it lists (and layouts it relates to but does not list) |
| `slideLayouts/` | type, name, shapes, background, colour map override, `showMasterSp`, `preserve` |
| `slides/` | layout, name, shapes, background, colour map override, `show`, `showMasterSp`, transition, animations, notes, comments |
| shapes | `p:sp` (preset geometry with adjust values, custom geometry, fill, line, effects, `p:style`, placeholders by type and index, text box flag, alt text, title, hidden, click hyperlinks); `p:pic` (image, `a:srcRect` crop, opacity; video and audio with `p14:media`, poster frame, trim); `p:graphicFrame` with `a:tbl` (grid, rows, spans, merges, cell fills, borders, margins, anchor, direction, table style and options), charts (`c:chart` and `cx:chart`, checked with kiln-charts), SmartArt, OLE objects; `p:grpSp` with its child space; `p:cxnSp` with the shapes its ends are glued to; `mc:AlternateContent` (the branch Kiln understands, else the fallback) |
| text | `a:bodyPr`, `a:lstStyle`, paragraphs (`a:pPr` with levels, bullets including picture bullets, spacing), runs (`a:rPr`, theme colours with their exact RGB cached), line breaks, fields (`a:fld`), hyperlinks on runs (URLs, slide jumps, actions), equations (`a14:m` as OMML) |
| `notesSlides/`, `notesMasters/` | the notes text (the body placeholder), the other shapes of each notes page, the notes master's shapes, background, colour map and notes style |
| comments | legacy `ppt/comments/commentN.xml` with `commentAuthors.xml`, and modern threaded comments (`modernComment_*.xml`, `authors.xml`) with replies and resolved state |
| `docProps/` | core, app and custom properties (`PptxRead::properties`) |

Conventions: pictures, picture fills and bullets, media and charts refer to `Presentation::parts`
by **part name** (`/ppt/media/image1.png`), never by relationship id; a chart's relationships part
and its targets (embedded workbook, style, colours) are stored next to it. Shape and run links are
URLs, `#slide:<id>` for a jump to a slide, or the `ppaction://` verb. Fields keep the DrawingML
field type (`slidenum`, `datetime1`, …) as their instruction and the shown text as their result.

## Mapped, and kept for saving

| Construct | Shown as | Kept in |
|---|---|---|
| SmartArt | a group of the shapes PowerPoint drew into `diagrams/drawingN.xml` (an empty frame when there is none) | the diagram parts stay in the passthrough; the graphic frame in `PptxExtras::shape_sources` |
| OLE object | its preview picture | the embedding in the passthrough; the frame (with its `mc:AlternateContent`) in `shape_sources` |
| Transition without an exact twin (dissolve, zoom, blinds, PowerPoint 2010+ exotics) | the nearest of Kiln's twelve, Fade otherwise | `Transition::original`, and the whole `p:transition` in `PptxExtras::transitions` |
| Animation effect Kiln does not play (motion paths, media verbs, other presets) | — | `AnimationEffect::Other { xml }` with the effect's `p:par`; the whole `p:timing` in `PptxExtras::timing` |
| Ink, unknown graphics, groups nested deeper than 32 levels | — | `PptxExtras::opaque_shapes` with their position |
| 3-D scene and bevel, extension lists, custom data, `p:custShowLst`, `p:photoAlbum`, `p:kinsoku`, … | — | `PptxExtras::shape_xml`, `owner_xml`, `presentation_xml` |
| everything else (presProps, viewProps, tableStyles, the notes master's theme, thumbnails, VBA, custom XML, printer settings, embeddings) | — | the passthrough set, byte for byte |

Every approximation is reported as a `PptxWarning` (stable codes such as `shape.smartArt`,
`transition.mapped`, `animation.unsupported`, `image.linked`).

## What is written

| Part | From |
|---|---|
| `presentation.xml` | slide and notes size, `firstSlideNum`, the master, notes master and slide lists (with the ids read), the default text style, sections (`p14:sectionLst` with their GUIDs); the attributes and children the reader kept, each in its schema place |
| `slideMasters/`, `theme/` | theme, colour map, background, shapes, title/body/other text styles, `p:hf`, the layout list; a theme the model did not change is the original's bytes |
| `slideLayouts/` | type, name, `showMasterSp`, `preserve`, colour map override, background, shapes |
| `slides/` | name, hidden, `showMasterSp`, background, colour map override, shapes, transition, animations, the link to its comments |
| shapes | `p:sp`, `p:pic` (crop, opacity; video and audio with `a:videoFile`/`a:audioFile` and the `p14:media` extension with trim), `p:graphicFrame` (tables; charts with `c:chart`/`cx:chart`), `p:grpSp`, `p:cxnSp` glued to the shapes' ids; placeholders, alt text, titles, hidden, click links; 3-D and extension XML the reader kept |
| text | `a:bodyPr`, `a:lstStyle`, `a:pPr`, runs with `a:rPr` (theme colours with the transform that reproduces the rendering read — Office's `tint` and `shade` come back exactly), click links (URLs, slide jumps, actions), `a:br`, `a:fld`, equations (`a14:m`), Office's `_xHHHH_` escapes |
| `notesSlides/`, `notesMasters/` | the notes text as the body placeholder among the page's other shapes; a notes master (and its theme) is created when a presentation gets its first notes |
| comments | legacy `comment*.xml` with `commentAuthors.xml`; threaded `modernComment_*.xml` with `authors.xml` for files read with them and for comments with replies or a resolved state |
| `docProps/` | the properties read, with the slide, notes and hidden-slide counts; `WriteOptions::now` stamps the save |
| media, charts | from `Presentation::parts` under their part names (a chart with its relationships, embedded workbook, style and colours) |
| everything else | the passthrough set, byte for byte, with its relationships (macros only into `.pptm`/`.potm`/`.ppsm`) |

Faithfulness rules:

- **Kept XML goes back where it came from.** SmartArt and OLE frames are written as read with the
  model's position and name; a shape read from `mc:AlternateContent` goes back into the branch it
  came from with the other branches untouched; ink and unknown graphics keep their place in the
  shape list; relationships that kept XML refers to are re-created with their original ids.
- **Motion is regenerated only when it changed.** The original `p:transition` (sounds and
  PowerPoint 2010+ options included) is written back while the model still says the same; the
  original `p:timing` is written back byte for byte while the slide's timeline and the shapes it
  animates are unchanged (effects Kiln does not play included). Otherwise transitions are written
  as PowerPoint does (newer ones and exact durations inside `mc:AlternateContent` with a Fade
  fallback) and the timing tree is generated from the timeline, with kept effects merged in.
- **Ids stay.** Part names, relationship ids, slide, master and layout list ids and `p:cNvPr` ids
  are the ones read; new parts and shapes take the next free ones.
- **Untouched parts stay.** With `WriteOptions::preserve` (a `TouchedParts` recorded from the
  edits' transactions), every slide, layout, master, notes page and `presentation.xml` no edit
  touched is copied byte for byte; relationships parts whose relationships did not change keep
  their bytes.
- **Deterministic.** The same model, source and options give the same bytes.

`compat_report` lists what saving loses or changes (macros into a format without them, clips and
pictures whose file is missing, Morph and other newer transitions for older PowerPoint versions,
threaded comments, SmartArt kept as read) and then the reader's diagnostics.

## Robustness

Presentations are untrusted input. Malformed slides, layouts, themes and comment parts become
diagnostics and the rest opens; a slide on a missing layout gets the first layout; positions
outside the DrawingML range are clamped; ragged tables are padded; sections that do not take the
slides in order are dropped. Nothing recurses with the depth of the file (deep groups, timing
trees and alternate content are walked iteratively or bounded), which the tests check on a 1 MiB
stack, and mutated parts are fuzzed with proptest.

Writing never panics either: text and names XML cannot hold are cleaned, missing media and charts
are reported (`PptxWritten::issues`) and left out, kept XML that is not well-formed is dropped
rather than corrupting a part, links to slides that are gone are dropped, and deep groups are
written on a small stack.

## Tests

- `tests/read_oracle.rs`: every deck of `fixtures/pptx` (and the external corpus) against the
  python-pptx dump in `fixtures/pptx/read/*.pptx.json` (slide size, layouts, slides, each shape's
  kind, name and text, table cells, notes). Regenerate the dumps with
  `python3 -I tools/make-fixtures.py --dump-pptx`.
- `tests/read_fixtures.rs`: what the model holds for each feature deck.
- `tests/read_elements.rs`: one construct per slide, built into a real package.
- `tests/read_hostile.rs`: deep nesting on a small stack, zip limits, mutated parts.
- `tests/roundtrip_fixtures.rs`: every corpus deck through read → write → read (facts, text and the
  python-pptx view unchanged, untouched parts byte-identical, deterministic, idempotent);
  `unchanged_slide_keeps_original_p_timing_bytes`; edited animations and exotic transitions.
- `tests/roundtrip_elements.rs`: clips, OLE objects, ink, alternate content, fields, equations and
  comments through a round trip, opened by LibreOffice.
- `tests/roundtrip_edits.rs`: Deck commands saved with `TouchedParts`; the parts they rewrite are
  compared with `fixtures/pptx/roundtrip/<case>/` (`KILN_BLESS=1` regenerates them), every other part
  is the original's.
- `tests/write_elements.rs`: a presentation built in Kiln with every construct, read back field by
  field; package layout, properties, templates, threaded comments.
- `tests/write_hostile.rs`: bad text and names, missing parts, broken kept XML, deep groups, notes
  without a notes master, empty decks, macros, random deletions (proptest).
- `cargo xtask fidelity --format pptx`: the corpus scored (structural, text, LibreOffice's
  rendering, LibreOffice opens, untouched parts) into `docs/qa/fidelity.md`.
