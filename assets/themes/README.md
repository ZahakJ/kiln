# Kiln Deck themes

The twelve themes of Design ▸ Themes, compiled into `kiln-deck` (`crates/kiln-deck/src/themes/bundled.rs`):
Kiln (the default), Tenmoku, Cobalt, Jade, Saffron, Celadon, Shino, Raku, Porcelain, Slate, Terracotta and Ash,
each with four variants.

A `.kdeck-theme` file is XML (`urn:kiln:deck-theme:1`) holding a complete DrawingML theme (`a:theme`: colour,
font and format schemes, written unchanged into a `.pptx`) plus the master and layout definitions: background,
layout grid, title/body/other text styles as DrawingML list styles over Kiln's defaults, placeholder overrides,
decoration shapes, per-layout treatments and variants. Positions are fractions of the slide, so a theme serves
16:9, 4:3 and custom sizes. The format is documented in `crates/kiln-deck/src/themes/mod.rs` and
`docs/FORMATS.md`.

The files are generated: the design of every theme is in `build.py`. Edit it and run

    python3 assets/themes/build.py

To review the designs, render them all: `cargo run -p kiln-deck --example theme_gallery -- themes.html <fonts-dir>`
(where `fonts-dir` holds the brand's Fraunces and Figtree and the `assets/fonts` families) and open the page.
