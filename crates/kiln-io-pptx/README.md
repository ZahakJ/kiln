# kiln-io-pptx

Reads `.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx` and `.ppsm` files into the Kiln Deck model
(`kiln_deck::Presentation`), keeping every part it does not understand. The reader (WP-77) is in
place; the writer (WP-79) follows in `src/write/`.

```rust
let read = kiln_io_pptx::read_pptx(&bytes)?;
let deck = &read.presentation;            // validated: Presentation::validate passes
println!("{}", kiln_io_pptx::Summary::of(&read).to_text());
for w in &read.warnings { eprintln!("{w}"); } // what the Compatibility Checker lists
```

`kiln-cli inspect --pptx <file>` prints the same summary (masters and layouts, one line per slide
with its layout, shape count and title, counts, diagnostics); `--json` gives it to scripts.

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

## Robustness

Presentations are untrusted input. Malformed slides, layouts, themes and comment parts become
diagnostics and the rest opens; a slide on a missing layout gets the first layout; positions
outside the DrawingML range are clamped; ragged tables are padded; sections that do not take the
slides in order are dropped. Nothing recurses with the depth of the file (deep groups, timing
trees and alternate content are walked iteratively or bounded), which the tests check on a 1 MiB
stack, and mutated parts are fuzzed with proptest.

## Tests

- `tests/read_oracle.rs`: every deck of `fixtures/pptx` (and the external corpus) against the
  python-pptx dump in `fixtures/pptx/read/*.pptx.json` (slide size, layouts, slides, each shape's
  kind, name and text, table cells, notes). Regenerate the dumps with
  `python3 -I tools/make-fixtures.py --dump-pptx`.
- `tests/read_fixtures.rs`: what the model holds for each feature deck.
- `tests/read_elements.rs`: one construct per slide, built into a real package.
- `tests/read_hostile.rs`: deep nesting on a small stack, zip limits, mutated parts.
