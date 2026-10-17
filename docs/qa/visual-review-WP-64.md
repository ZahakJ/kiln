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
the Navigation pane, with the Styles pane, with the Paragraph dialog, light and dark; since fix round 1
also on the Font, Page Setup and Find and Replace dialogs, the context menu with the mini toolbar and
the Insert Table picker, in both themes ("theme switches repaint the rulers; …"), apart from two ui-kit
findings handed off to WP-58 (below).

| Mockup element | Verdict | Kiln Write | Deviations | Fixed during the review |
|---|---|---|---|---|
| Page on the canvas with the ember glow (light: white page, warm shadow; dark: char page) | Matches | The surface paints Word's page (size, margins, shadow) centred in the canvas, dark page in the dark theme | — | — |
| Ruler strip above the page (indent triangles in cobalt, minor ticks, inch numbers) | Matches (since fix round 1; the first review wrongly passed the dark ruler) | Horizontal ruler from the core's `Rulers` answer: margins sunken, text area white, first-line/hanging/left/right markers in the accent, tab stops; a vertical ruler at the left edge, as in Word | The mockup has no vertical ruler; Word does (View ▸ Ruler shows both) | Ruler colours from tokens (`--kiln-sunken`, `--kiln-line-strong`); it hides in Web Layout and Read Mode. Fix round 1: the rulers repaint when the theme changes (they stayed white on the dark UI after a light → dark switch) and hide below 600 px wide, as in Word mobile |
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

## Fix round 1 (independent review findings)

Re-rendered and looked at after the fixes: `editor-light-desktop.png`, `editor-dark-desktop.png`
(regenerated by the e2e test), and the Font and Paragraph dialogs in dark, Page Setup in light, the
Styles pane in dark and the editor at 412 × 915.

| Finding | Before | After |
|---|---|---|
| Font dialog Preview in the dark theme | Near-black text (`#1c1917`, the Automatic colour) on the dark page colour: the box looked empty | Automatic colour is the page's text colour in the current theme (`--kiln-fg`): light text on the dark page, as the document is painted; the e2e test asserts ≥ 4.5:1 in both themes. The preview shows the size in points as on the page at 100 % (it showed 11 pt as 11 px) |
| Rulers after a theme switch | Colours read only when drawing, no theme listener: after light → dark both rulers stayed bright strips; the committed dark screenshot showed it while this review said "Matches" | `useThemeKey` (OS colour scheme, contrast and forced-colours queries, the theme root's `data-theme`/`data-app`/`class`/`style`) repaints both rulers; the e2e test checks the ruler's luminance after each switch and the dark screenshot now has the dark ruler of `write-dark.png` |
| Section label against the tab strip (Paragraph: GENERAL, Page Setup: MARGINS) | No space between the tabs and the first label | A tab's content starts 14 px below the strip in every Write dialog |
| Styles pane names at ~10–11 px (Heading 5–9, No Spacing, Quote, emphasis styles) | Hard to read, especially in dark | Previews are at least 13 px |
| Rulers at phone width | Both rulers visible at 37 % zoom, taking width from the page | Hidden below 600 px wide (View ▸ Ruler still toggles them on wider screens) |

Not changed in this round: the preview shows the font name rather than the selected text (Word shows
the selection; the selection's text is not in the protocol's `FormatState`, and adding it is a
protocol change for a later Write round). ui-kit findings from the wider axe coverage, handed off to
WP-58 in `docs/qa/handoffs.md`: the menus' shortcut column and group labels (`--kiln-faint`) read
below 4.5:1, and the mini toolbar's small split buttons are under 24 px (target size); the context
menu scan excludes exactly those.
