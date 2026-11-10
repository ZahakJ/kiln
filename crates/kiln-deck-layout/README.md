# kiln-deck-layout

Slide rendering for Kiln Deck: a slide of the `kiln-deck` model becomes `kiln-scene` draw ops — the
stage the editor paints, thumbnails, golden renders and the CLI's PNGs all come from the same code.

- `render`: `Renderer::draw` resolves inheritance (slide → layout → master), the background, every shape kind
  (preset and custom geometry with flips and rotation, fills, outlines with dashes and arrowheads, theme style
  references, shadows and glow, pictures with cropping, tables, charts through `kiln-charts`, groups, connectors)
  and text bodies through `kiln-text` (bullets and numbering from the list styles, columns, vertical and stacked
  text, fields, links, autofit at the stored scale). In the editing view empty placeholders show their prompts.
- `text`: body layout with caret, hit testing and selection rectangles for the editor, and `fit_autofit`,
  PowerPoint's shrink-on-overflow search, run when typing stops.
- `scene`: the stage's tiles (1024 device px, the slide on a pasteboard), thumbnails of slides, layouts and
  themes as self-contained scenes, one-slide scenes for export.
- `select`: the eight handles and the rotation handle, grips under the pointer, resize (Shift keeps the
  proportions, Ctrl from the centre, flips past an edge) and rotation (Shift: 15° steps).
- `guides`: smart guides and snapping to the slide's centre and edges, to other shapes and to equal spacing.
- `a11y`: the accessibility tree of a slide; `fonts`: the bundled document fonts plus the brand faces.

Slide text is painted with alpha `0xFE`: its colours were chosen for the slide, so the web painter does not
adapt them for a dark page. Goldens: `cargo nextest run -p kiln-deck-layout` checks `fixtures/golden/deck`;
`KILN_UPDATE_GOLDEN=1` rewrites them.
