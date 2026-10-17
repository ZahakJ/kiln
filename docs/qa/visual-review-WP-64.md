# Visual review: WP-64 (Kiln Write editor)

Reviewed on 2026-10-17 against the brand kit's mockups `docs/design/mockups/write-light.png` and
`write-dark.png` (sources in `brand/mockups/src/write-light.html`). Screenshots come from
`e2e/specs/write.spec.ts` ("opens a fixture through the file picker, shows panes and dialogs, light and
dark"), which opens `fixtures/docx/features/styles.docx` through the web file picker and shows the
Navigation and Styles panes: `e2e/screenshots/write/editor-light-desktop.png` and
`editor-dark-desktop.png` (1440 × 900, device scale 1). During the review the panes, the mini toolbar,
the context menu, the Font, Paragraph and Page Setup dialogs, Find and Replace and Insert ▸ Table's grid
picker were also rendered and looked at in both themes (the same states the e2e tests drive).

axe-core (WCAG 2.2 A/AA and best practices) reports 0 serious or critical violations on the editor with
the Navigation pane, with the Styles pane, with the Paragraph dialog, light and dark.

| Mockup element | Verdict | Kiln Write | Deviations | Fixed during the review |
|---|---|---|---|---|
| Page on the canvas with the ember glow (light: white page, warm shadow; dark: char page) | Matches | The surface paints Word's page (size, margins, shadow) centred in the canvas, dark page in the dark theme | — | — |
| Ruler strip above the page (indent triangles in cobalt, minor ticks, inch numbers) | Matches | Horizontal ruler from the core's `Rulers` answer: margins sunken, text area white, first-line/hanging/left/right markers in the accent, tab stops; a vertical ruler at the left edge, as in Word | The mockup has no vertical ruler; Word does (View ▸ Ruler shows both) | Ruler colours from tokens (`--kiln-sunken`, `--kiln-line-strong`); it hides in Web Layout and Read Mode |
| Navigation pane card (search box, Headings/Pages/Results segmented control, heading tree with the current heading highlighted by a cobalt bar, word-goal card) | Matches | Same card, search, segmented tabs, heading tree with disclosure chevrons and the current heading tinted with the bar; the bottom card shows words, pages and minutes to read | Word goal (target and meter) is WP-66's | Option checkboxes kept on one line; disclosure buttons 24 × 24 px (WCAG target size); conventional ▸/▾ chevrons |
| Styles gallery in the ribbon (Normal, Title, Heading 1, Heading 2, Quote…) | Matches | The gallery shows the document's styles with their previews; the current style is outlined | — | — |
| Styles pane (not in the mockup) | Brand-consistent | Card like the Navigation pane; previews in each style's font and weight; style colours mixed with the text colour so they stay readable in both themes | — | Contrast (axe) in dark mode for dark document colours (Heading 5–9, Quote) |
| Status bar (Page 1 of 4, words, language, view buttons, zoom slider with percentage) | Matches | Page x of y (opens Go To), words (opens Word Count), language, Read/Print/Web buttons, zoom slider and presets | Spell-check state, Autosave and Focus belong to WP-66/WP-83; the character count and Track Changes are in the Customize menu (hidden by default, as in Word) | Character count hidden by default from the first render |
| Mini toolbar over a selection | Brand-consistent | The ribbon's own Font controls (font, size, grow/shrink, B, I, U, highlight, colour, bullets, numbering) in the kit's floating toolbar | — | — |
| Context menu | Brand-consistent | Cut, Copy, Paste Options (Keep Source, Merge, Keep Text Only), Font…, Paragraph…, list and table items in context, Link… / Remove Link | New Comment shows in development builds as planned (WP-66); release builds hide it | — |
| Dialogs (Font, Paragraph, Page Setup, Find and Replace, Insert Table, Link, Symbol, Date & Time, Word Count, Paste Special, Bookmark, Style) | Brand-consistent | The kit's dialog (Fraunces title, tabs, uppercase section labels, primary OK); lengths in inches for US English and centimetres elsewhere, as Word | — | Font style "Regular" selected (an empty value showed nothing); selects named by their labels (axe button-name) |
| Insert ▸ Table grid picker | Matches Word | 10 × 8 grid, "4×3 Table" label, Insert Table… and Convert Text to Table below | — | Keyboard: roving focus with the arrow keys, Enter inserts |
| Comments column, Editing mode chip, Dictate, Rewrite, Help tab, avatar | Not in WP-64 | — | Comments (WP-66), dictation and Rewrite (P1/P2 assistant hooks), sign-in (later); §1.4 has no Help tab | — |

**Dark page colours (fixed during the review).** In the dark theme the page is dark, and text in an
explicit document colour was painted as stored: Word's dark blue headings (`2F5496`) and dark greys
were unreadable on the char page, where the mockup shows a light cobalt. The canvas painter now adapts
such colours as Word's dark mode does (`packages/surface/src/contrast.ts`): text, underlines and strike
lines whose colour reads below 4.5:1 on the dark page keep their hue and take the inverse lightness,
raised until they read; `editor-dark-desktop.png` shows the result. Automatic-colour text already
followed the theme. Remaining: paragraph borders and cell shading in explicit colours are painted as
stored (the heading rule under "Text styles matrix" stays dark blue), and `kiln-raster`'s dark previews
do not adapt (print and PDF must not: they are on white paper) — noted in `docs/qa/handoffs.md`.
