# kiln-io-pdf

PDF export, print and print preview for Kiln, written with [`krilla`](https://crates.io/crates/krilla) 0.8.2 from the
same layout and Scene ops that paint documents on screen — so the screen, the preview, the printout and the PDF agree.

```rust
use kiln_io_pdf::{PdfOptions, PageSelection, export_report, preview, write::{WritePrint, print_layout}};

let layout = print_layout(&doc, kiln_doc_layout::default_fonts(), /* markup */ true, None);
let printable = WritePrint::new(&doc, &layout, Some("Quarterly report".into()));
let options = PdfOptions { pages: PageSelection::parse("1-3, 5")?, ..PdfOptions::default() };
let out = export_report(&printable, &options)?;      // out.pdf, out.pdf_ua, out.ua_issues
let sheet = preview(&printable, &options, 0, 2.0)?;  // a SceneDelta for the Surface painter
```

## What a PDF contains

| Area | Behaviour |
|---|---|
| Text | Real text: glyph runs from the layout with their glyph ids and positions, fonts embedded **and subset** (CID fonts, `Identity-H`), Unicode mapping from the clusters (ToUnicode / ActualText), so text is selectable, searchable and extractable. Synthetic bold is fill + stroke, synthetic italic a skew. Variable fonts keep their instance. Colour fonts work. |
| Graphics | Rectangles, rounded rectangles, paths (fill rules, strokes, dashes, joins, caps), clips, transforms, linear and radial gradients, picture crops and opacity. Drop shadows are approximated with translucent layers. On-screen-only ops (caret, selection, guides, squiggles, page shadows) are not printed. Automatic colours print black. |
| Pictures | JPEG embedded as is (DCT passthrough); PNG decoded and re-compressed losslessly at full resolution; GIF and BMP from the decoded pixels. |
| Structure (tagged PDF) | Document title and language; `H1`–`H6` from outline levels (heading styles) with their text as title; `P`; `Table`/`TR`/`TH` (header rows)/`TD` with spans; `Figure` with alt text (decorative pictures are artifacts); `Link` with link annotations (alt text included) for hyperlinks and `HYPERLINK` fields, TOC entries included; `TOC`/`TOCI`; `Note`; text boxes as `Div`; headers/footers as pagination artifacts; shading, borders and rules as layout artifacts. Reading order follows the layout. |
| PDF/UA-1 | Tagged exports are made PDF/UA-1 whenever the content allows it (krilla's PDF/UA-1 validator) and fall back to plain tagged PDF otherwise; `export_report` says why (pictures without alt text, glyphs missing from every font). 15 of the 19 feature fixtures come out PDF/UA-1. |
| Navigation | Bookmarks (outline) from headings, nested by level (a document without headings gets one bookmark to its first page); internal links jump to their bookmark's page; page labels from the sections' page-number formats (decimal, roman, letters; other formats as the number Word shows). |
| Metadata | XMP and Info: title (document property, else the file name), authors, subject, keywords, language, creator `Kiln Write`, producer `Kiln`, creation date. |
| Options | Page ranges (Word's `1-3, 5, 8-`), odd/even pages, copies (collated; later copies are artifacts), scale to Letter/Legal/A3/A4/A5, zoom 10–400 %, 1/2/4/6/9/16 pages per sheet (two and six turn the sheet), grayscale, tagging and bookmarks on/off. Kiln Write adds print markup on/off, print hidden text, and orientation and margin overrides ([`write::print_document`]). |

## The contract for apps

`PrintablePages` (pages with sizes and labels, document info, structure tree, outline, images, and `paint` into a
`PageCanvas`) is all an app implements. Kiln Write's is `write::WritePrint`; Kiln Grid (page setup → sheet pages) and Kiln
Deck (slides, handouts with 1/2/3/6 per page, notes pages) implement it in their work packages. Painting uses the Scene's
`DrawOp`s plus `TextRun`s for text and `Mark`s that tie content to the structure; the PDF writer and the preview are the two
canvases, and `sheet::impose` places pages on paper for both.

## Measured

- `fixtures/large/doc-300p.docx`: 300 pages, print layout + tagged PDF ≈ 0.5 s on this machine (budget B11 < 2 s,
  `cargo xtask budgets --only B11`, bench `pdf-export-300p`), 1.6 MB.
- Feature fixtures (`tests/fixtures.rs`, poppler-utils as the oracle): page counts equal the layout's for all 19; every
  font embedded and subset; tagged; 99.96 % of the model's words come back out of `pdftotext` (the rest is text in
  scripts the bundled fonts lack, which prints as missing-glyph boxes); every page renders with `pdftoppm`.

## Tests

- Unit tests: page boxes, imposition (fit, grids, copies), structure and labels, page-setup overrides.
- `tests/fixtures.rs`: every `fixtures/docx/features/*.docx` through `pdfinfo`, `pdffonts`, `pdftotext` and `pdftoppm`
  (`KILN_KEEP_PDF=1` keeps the files; `KILN_SKIP_POPPLER=1` skips on machines without poppler-utils).
- `examples/export.rs`: times an export (`cargo run --release -p kiln-io-pdf --example export -- in.docx out.pdf`).

## Known limits

- Text in scripts no available font covers is drawn as missing-glyph boxes (as on screen); such documents cannot be
  PDF/UA-1 and poppler may drop a character of those runs when extracting.
- Charts, EMF/WMF pictures and SmartArt print as the quiet placeholder the screen shows until they render.
- Lists are tagged as paragraphs (no `L`/`LI` yet); footnotes are `Note`s at their reference.
