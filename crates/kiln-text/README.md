# kiln-text

The text pipeline of Kiln (PLAN.md §3.6): fonts, the Office font substitution table, shaping, bidi, line
breaking with Word's rules, hyphenation, carets and hit testing, and the glyph atlas. Every surface — Write
pages, Grid cells, Deck text bodies, PDF export — lays text out through this crate, with the same metrics the
rasteriser uses, so line breaks are identical on every platform, on screen and in print.

```rust
use kiln_text::{Alignment, FontStore, ParagraphSpec, SpanStyle, TextEngine};

let mut engine = TextEngine::new(FontStore::bundled());
let mut spec = ParagraphSpec::plain("Kiln lays out text like Word.", SpanStyle::new("Calibri", 14.67), 120.0);
spec.props.alignment = Alignment::Justify;
let layout = engine.layout(&spec);
for line in &layout.lines {
    // line.runs: glyph runs (font id, size, colour, positioned glyphs) in visual order
    // line.decorations: underlines, strikethroughs, heavy leader rules
}
let caret = layout.caret_rect(5);
assert_eq!(layout.hit_test(caret.x, caret.center_y()), 5);
```

## Fonts — `FontStore`

- **Bundled OFL fonts** (`bundled-fonts`, default): Liberation Sans/Serif/Mono 2.1.5, Carlito and Caladea,
  compiled in from `assets/fonts/` (URLs and SHA-256 checksums in `assets/fonts/SOURCES.md`, verify with
  `cd assets/fonts && sha256sum -c SOURCES.md`). `FontStore::bundled()` uses only these, so results are the
  same everywhere (tests, PDF export, the web build).
- **System fonts** (`system-fonts`, default, native targets): discovered with `fontique` (fontconfig is loaded
  with `dlopen` on Linux). `FontStoreOptions { bundled, system, directories }` controls what is loaded;
  `add_font_data` registers fonts from memory (user fonts on the web), `load_directory` a folder.
- **Substitution** (`src/substitution.rs`): `resolve("Calibri")` → Carlito (`MetricCompatible`), Cambria →
  Caladea, Arial/Helvetica → Liberation Sans, Times New Roman → Liberation Serif, Courier New → Liberation
  Mono, Calibri Light → Carlito (`Approximate`), Aptos and Segoe UI → `ui_sans()` (`UiSans`, settable with
  `set_ui_sans`, default Liberation Sans), Symbol/Wingdings/Webdings → `SymbolFallback`, East Asian Office fonts
  → the CJK fallback chain, anything else → a bundled family chosen from the name (`Missing`). A family that is
  installed is always used as is (`Exact`). Every lossy substitution of a paragraph is listed in
  `ParagraphLayout::substitutions` for the Compatibility Checker.
- **Script fallback chains**: Latin/Greek/Cyrillic/Hebrew → Liberation Sans; Arabic → Noto Naskh/Sans Arabic,
  Arial, Tahoma…; Han/Kana/Hangul → Noto Sans CJK, YaHei, Yu Gothic, Malgun…; Thai, Devanagari, emoji. Only
  families that exist are used; `set_script_fallbacks(*b"Arab", &[..])` overrides a chain.
- **Metrics** (`metrics(font, size)`, cached per (font, size)): Word's rule — single line height =
  `usWinAscent + usWinDescent`, or `sTypoAscender − sTypoDescender + sTypoLineGap` when the font sets
  `USE_TYPO_METRICS`; underline and strikeout position/thickness, x-height and cap height.
- `FontId` identifies an interned font instance (data, face index, variation coordinates, synthetic bold or
  oblique); `font(id)` gives the bytes to the rasteriser and the PDF writer.

## Layout — `TextEngine::layout(&ParagraphSpec) -> ParagraphLayout`

Input (`src/spec.rs`, lengths in CSS px): styled `TextRun`s (`SpanStyle`: family, size, weight, italic, colour,
underline and strikethrough kinds, letter spacing, baseline shift, language, Word's run-level `rtl`,
kerning and ligature switches) and `ParagraphProps` (alignment start/centre/end/left/right/justify/distribute,
start and end indents, first-line or hanging indent, tab stops with start/centre/end/decimal alignment and
dot/hyphen/underscore/middle-dot/heavy leaders, default tab interval, line spacing multiple/exactly/at least,
space before/after, direction LTR/RTL/auto, automatic hyphenation, paragraph-mark style). In the text, `\t` is
a tab, `\n` a manual line break and U+00AD a soft hyphen.

Pipeline:

1. **Bidi** (UAX #9 via `unicode-bidi`, skipped for left-to-right text without right-to-left characters).
2. **Shaping**: the paragraph is cut into segments that cannot interact typographically (a word with its
   trailing spaces, one CJK character, a tab, a single-direction stretch of one style); each is shaped once by
   `parley` 0.12 (font selection with fallback, `harfrust` OpenType shaping) and kept in a word cache, so
   re-laying out a document after an edit or a width change shapes almost nothing. Misses are shaped together
   in one parley layout per direction.
3. **Line breaking** with Word's rules: UAX #14 opportunities (`unicode-linebreak`), spaces that hang past the
   end indent, tab stops measured from the start margin with an implicit stop at a hanging indent, tabs that
   stop at the end indent, centre/end/decimal tabs that look ahead, Knuth–Liang hyphenation (en-US and German
   patterns, chosen by the run's language) and soft hyphens, emergency breaks inside words wider than the line.
4. **Alignment**: justification stretches only the spaces after the last tab (CJK lines stretch between
   characters; Arabic is never stretched between letters); manual line breaks are justified, the last line is
   not; distribute stretches every line between characters.
5. **Visual order** (UAX #9 L1/L2) and positioning; underline/strikethrough segments; leader glyphs on a grid.
6. **Line boxes** from the largest Word-metric ascent/descent on the line (with baseline shifts), line spacing
   (extra space of "multiple" above the text, "exactly"/"at least" anchored at the bottom), space before/after.

Output (`src/output.rs`): `ParagraphLayout { lines, height, rtl, substitutions }`; each `Line` has its text range,
top, height, baseline, ascent/descent, content x and width, why it ended (`Wrap`, `Hyphen`, `Emergency`,
`LineBreak`, `Paragraph`), glyph runs (`GlyphRun { font, size, span, color, level, text_range, glyphs }`, each
glyph with id, x, y, advance and the byte offset of its cluster) and cluster boxes in visual order.

Carets (`src/caret.rs`): `caret_positions()` (grapheme cluster boundaries), `caret_rect(i)` /
`caret_rect_with(i, Affinity)` (leading edge of the character after `i`, bidi-aware), `hit_test(x, y)` (inverse
of `caret_rect`, preferring the position whose caret is drawn where the click landed at bidi boundaries),
`selection_rects(range)`, `next_position`/`prev_position`, `word_at`, `next_word_start`/`prev_word_start` and
`sentence_at` (UAX #29 via `unicode-segmentation`).

## Limits on untrusted input

Paragraph geometry comes from documents, so layout clamps it before use: widths, indents, first-line/hanging
indents, tab stops, default tab interval and space before/after to ±100,000 px (`MAX_EXTENT`, far beyond
Word's 22-inch page), font sizes to 0.5–4,000 px, letter spacing to −size/4 … 2,112 px (1,584 pt, Word's
maximum expansion; condensing is bounded tighter than Word so text cannot collapse onto one point), and
non-finite values to defaults. A tab leader emits at most 16,384 glyphs, placed from an integer index. Words
longer than 96 clusters (URLs, base64, identifiers) are never hyphenated, which keeps hyphenation cost per line
constant. `tests/robustness.rs` holds the regression tests.

## Known fidelity gaps

- CJK text is shaped one character per segment (so the word cache works and every ideograph is a break
  opportunity); kerning, `palt`/`vpal` and contextual forms *between* CJK characters are therefore not applied.
  Word's defaults do not use them; documents that do will set slightly looser.
- `glyph_outline` and outline runs (≥ 96 device px) do not embolden synthetic-bold faces: the painter (WP-54)
  must stroke the path by the synthetic-bold amount.
- The LibreOffice oracle fixtures are Latin left-to-right; bidi, CJK and hyphenation breaks are covered by the
  insta snapshots only.

## Glyph atlas — `GlyphAtlas`

`prepare_run(&fonts, &run, origin_x, origin_y, scale)` rasterises a run's glyphs with `swash` at
(font, device px size, quarter-pixel x offset) into 1024 × 1024 pages — A8 coverage for text, premultiplied
RGBA8 for colour glyphs — with shelf packing, and returns placed bitmaps (`page`, rectangle, device x/y) for
the Scene. Each glyph is rasterised once; `take_dirty()` lists the pages the painter must re-upload. Runs at
96 device px or larger are returned as `kurbo` outline paths instead (`glyph_outline` for single glyphs).
The atlas does not evict yet: entries and pages grow with every (glyph, size, offset) seen, so long sessions
across many zoom levels need an LRU/page-recycling policy agreed with the painter (follow-up).

## Features

| Feature | Default | Purpose |
|---|---|---|
| `bundled-fonts` | yes | compile the OFL document fonts in |
| `system-fonts` | yes | discover platform fonts (native only) |
| `hyphenation` | yes | embed the en-US and German pattern dictionaries (`data/hyphenation/`) |

The web build uses `--no-default-features --features bundled-fonts` (plus `hyphenation` if wanted):
`cargo build --target wasm32-unknown-unknown -p kiln-text --no-default-features --features bundled-fonts`.
`bundled-fonts` embeds about 9.7 MB of TTFs, which does not fit the web core's 4.5 MB gzip budget (B12): the
shipped web build must load fonts at run time with `FontStore::add_font_data` instead (WP-52/WP-94).

## Tests and tools

- `cargo nextest run -p kiln-text`: unit tests; 41 `insta` snapshots of line breaks, glyph counts and geometry
  (`fixtures/golden/text/`: English left/centre/right/justify/distribute, indents, tabs and leaders, manual
  breaks, soft hyphens, emergency breaks, English and German hyphenation, Arabic and Hebrew RTL, mixed
  directions with numbers, Chinese and Japanese, styles, decorations, spacing, substitutions); property tests
  (`hit_test(caret_rect(i)) == i` for Latin, Hebrew and mixed bidi text; layout never panics and covers the
  text); atlas, outline, font, substitution and metrics tests.
- **LibreOffice oracle**: `fixtures/docx/text/` holds ten single-paragraph `.docx` files written by
  `make_fixtures.py` (python-docx) with a JSON description each; `oracle.json` records LibreOffice's rendering
  (`tools/oracle-soffice.sh` → PDF → `pdftotext -layout`). The test requires the same line count and the same
  text on every line; `KILN_ORACLE_LIVE=1` re-renders with LibreOffice during the test.
- Tests use bundled fonts plus `tests/fonts/` (Noto Naskh Arabic and a Noto Sans CJK SC subset, OFL) with system
  fonts off, so snapshots are identical on every machine.
- `cargo run --release -p kiln-text --example layout_bench`: 1,000 mixed paragraphs at 600 px; prints `cold_ms`
  (first pass, empty caches), `total_ms` (warm engine, best of five; the vocabulary is small, so most words hit
  the shaping cache) and `vocab_ms` (1,000 further paragraphs of ~40,000 distinct generated words, mostly cache
  misses); fails when `total_ms` reaches 60 ms.
- `cargo run -p kiln-text --example render_png -- out.png [scale] [dark]`: renders sample paragraphs through the
  atlas to a PNG for visual review. `--example dump -- "text" [width] [family]` prints line breaks.
