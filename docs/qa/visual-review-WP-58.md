# Visual review: WP-58 (app frame, ribbon, Backstage, status bar, palette, Kiln Home)

Reviewed on 2026-10-16 against the brand kit's mockups in [`docs/design/mockups/`](../design/mockups/)
and their sources in `brand/mockups/src/` (`kiln-ui.css`, `home-light.html`, `write-light.html`…).
Screenshots come from the Playwright specs (`pnpm e2e --grep 'chrome|shortcuts|home|backstage'`) and
land in `e2e/screenshots/chrome/` (CI uploads them as an artifact): `home-{light,dark}-{desktop,phone}.png`,
`<app>-frame-{light,dark}-desktop.png` for write, grid, deck and notes, plus `grid-simplified-light-desktop.png`,
`write-frame-light-{tablet,phone}.png`, `write-keytips-light-desktop.png`, `grid-palette-light-desktop.png`,
`write-options-light-desktop.png` and `backstage-{home,info}-light-desktop.png`, `backstage-new-dark-desktop.png`.

The frames show the chrome around an empty document surface: the editors (WP-64 Write, WP-71 Grid,
WP-78 Deck, WP-86 Notes) mount their canvases in the frame's surface slot, and the mockups' document
content (pages with text, cells, slides, panes) is theirs. Kiln Deck and Kiln Notes documents cannot
be created by the core yet (`Session::new_document` has no Deck or Notes app), so those frames say so
over the empty stage instead of pretending to be editors.

Development builds (the e2e target) show the P0 ribbon in full, including commands the core does not
implement yet, so the layout can be compared with the mockups; running one shows a "not in this build
yet" toast. Release builds hide those commands (PLAN.md §1.4: never greyed).

**What release builds ship (fix round 1).** The rows below compare the development ribbon with the
mockups; a production build (`vite build`, also the Pages site) is reviewed separately here, from
`e2e/screenshots/chrome/release-*.png`, which chrome.spec's "chrome (release build)" tests take from a
real `VITE_KILN_RELEASE=1` build. With 38 commands in the core's `catalog.json`, the release ribbon is
small and honest: a group whose commands are all hidden disappears, a tab with no group left
disappears, and a group thinned to one command shows it as a full-height button.

| Release screenshot | Verdict | What it shows | Deviations (until the editors' commands land) |
|---|---|---|---|
| `release-write-frame-{light,dark}-desktop.png` | Whole, sparse | Title bar as in development; tabs Home (Editing ▸ Select ▾ with Select All), Insert (Illustrations ▸ Chart), View (Views, Zoom, Theme, Macros); status bar with words, view buttons and the zoom slider in one row | No Clipboard/Font/Paragraph/Styles, Layout, References or Review until WP-64 implements them; no Comments or Share (their commands do not exist yet, and §1.4 forbids greyed slots) |
| `release-grid-frame-{light,dark}-desktop.png` | Whole, sparse | Insert (Charts ▸ Chart), View; status bar Ready, view buttons, zoom | Home tab hidden until WP-71's cell commands exist |
| `release-{deck,notes}-frame-light-desktop.png` | Whole | No document can be created yet: tabs only (no slab over the notice), File opens Backstage for New and Open | Editors arrive with WP-78/WP-86 |
| `release-home-light-{desktop,phone}.png` | Matches the development Home | Same layout; the base primitive styles are present | — |

Fixed in this round: the primitives' stylesheet was missing from production bundles (the bundler
skipped its re-export barrel, and the CSS with it), which unstyled every `.kiln-btn` and broke the zoom
control; empty group slabs and empty tabs in release builds; Deck and Notes frames no longer draw a full
ribbon over the "cannot be created" notice (development screenshots `deck-frame-*` and `notes-frame-*`
now show the tab strip only); on the phone Home the floating New button no longer covers the Recent
card (the page scrolls clear of it).

| Mockup | Verdict | Matches | Deviations | Fixed during the review |
|---|---|---|---|---|
| `home-light.png` | Matches | Left rail (kiln lockup, ember **New** with Ctrl N, Home/Recent/Shared with me/Pinned/Open file with counts and shortcuts, *Apps* filters, local-first card with the fire gradient meter, Settings), the glow behind the greeting, "What are we making today?" in Fraunces 46/800, the four arch doors (exact outline from `home-light.html`, glazed solids, rim light, fire glow, the 92 px tiles, format chips, + buttons), the "Coming from Microsoft Office?" banner with the tilted format cards, the Recent/Pinned/Shared segmented control, Open file button | Recent table is empty on the web build (the core's recents arrive with WP-83) and shows a designed empty state instead of the mockup's rows; "Fresh from the kiln" templates, "All templates", the tour and "Import settings" appear when WP-85 provides them; no avatar until sign-in exists; the Deck and Notes doors show **Soon** instead of + because the core cannot create those documents yet; Notes' chip reads `.md` (`.one` import is out of scope, PLAN.md §2.5) | Search field, kbd and labels moved from the kit's `faint` to `muted` text (faint fails WCAG AA on panel, 4.0:1); New's kbd on a darker inset for contrast; door names exposed to screen readers without a label mismatch |
| `home-dark.png` | Matches | Same layout on soot/char panels, ember New, the dark glow, the doors keep their glaze | As light | As light |
| `phone-light.png` | Matches | kiln lockup in the top bar, search icon, greeting and two-line title, 2 × 2 doors with noun and blurb, Recent list, the floating ember **New**, the bottom tab bar with the active Home pill | Tab bar's fourth item is Settings (Templates arrives with WP-85); no avatar (no sign-in); the status-bar area of the mockup is the device's | The glow pseudo-element no longer widens the page (no horizontal scroll at 390 px); New sits above the tab bar |
| `phone-dark.png` | Matches | As light | As light | — |
| `write-light.png` | Matches (chrome) | Title bar (Write tile + Fraunces name, Save/Undo/Redo, centred title with muted extension, flame chip, sunken Search with Alt Q kbd), tab strip (cobalt **File** with spark, the arch-shaped active tab with the ember dot, Comments, cobalt Share), the rounded ribbon slab (Clipboard with the Paste tile, Font with two rows, Paragraph, the Styles gallery with "Aa Bb" samples and names, Editing, Kiln), group labels, status bar with words, view buttons and zoom slider | No Help tab (not in §1.4); no Voice ▸ Dictate group (no such command in §1.4); the Kiln group shows Focus Mode (Rewrite is the P1 assistant, hidden in P0 builds); font and size boxes show "Font"/"Size" until the core reports the selection's values (WP-64); "Editing ▾" mode and avatar not built (WP-66, sign-in); navigation pane, ruler, comments and word goal belong to WP-64/WP-66 | Narrowed the font boxes, gallery tiles and group padding so every Home group fits at 1440 px as in the mockup; group labels on `muted` |
| `write-dark.png` | Matches (chrome) | As light; dark slab and panels, ember glow under the page | As light | — |
| `grid-light.png` | Matches (chrome) | Jade File/Share, Home groups Clipboard, Font (borders, fill, font colour), Alignment, Number (format box, accounting, %, comma, decimals), Styles (three text buttons as in the mockup), Cells (Insert/Delete/Format), Editing (AutoSum, Sort & filter, Find) and Kiln ▸ Explain (Formula Lab), status bar Ready + Average/Count/Sum from the core + view buttons + zoom | Tab label "Page Layout" (§1.4; the mockup says "Layout"); no Help tab; Fill and Clear sit as icons beside the Editing column; name box, formula bar, headers, sheet tabs and the chart card are WP-71/WP-73's; the number box shows "Number format" until the core reports the cell's format | Editing restructured into one labelled column so the group fits beside Kiln at 1440 px |
| `grid-dark.png` | Matches (chrome) | As light | As light | — |
| `deck-light.png` | Matches (chrome) | Ember File/Share, Clipboard, Slides (New slide tile, Layout, Reset, Section), Font, Paragraph, Drawing, Editing, Kiln ▸ Ideas (Design Ideas), status bar | The core cannot create presentations yet, so the frame shows the empty 16:9 stage with "Kiln Deck documents cannot be created in this build yet"; Record and Present split buttons, thumbnails, notes and the Design ideas pane are WP-78/WP-80's; the mockup's separate Arrange group is the §1.4 Drawing ▸ Arrange button | — |
| `deck-dark.png` | Matches (chrome) | As light | As light | — |
| `notes-light.png` | Matches (chrome) | Saffron File/Share, Home groups Clipboard, Basic Text, Styles (Heading 1/Heading 2/Normal), Tags (To do, Important, Question, All tags) | Notes documents cannot be created by the core yet (WP-86): empty page with the message; Draw (P2) and History tabs hidden; notebook/section/page panes, Follow and History buttons are WP-86's | — |
| `notes-dark.png` | Matches (chrome) | As light | As light | — |
| Backstage (no mockup; `kiln-ui.css` component styles, kit-gaps.md) | Matches the kit's language | Full-screen File menu with the app-solid rail in Office order (§1.5), Fraunces page titles, Info cards (compatibility check from the core's `CompatReport`, properties from `Summary`/`Status`), Save As/Export format list with ext tiles, New tiles, Account theme cards and core version | Print and Share pages are omitted until their core commands exist; Save As/Export run the core's `SaveAs` (formats the app cannot write are refused by the core with a message) | Rail moved from the old accent gradient to `--app-solid`; content on `--kiln-panel` |
| `board.png`, `construction.png`, `size-check.png` | Reference | Tiles at 16/20/24/32/48/92 px use the kit's drawings for their size (`AppTile`); glazes, spark, flame and type follow the board | — | `AppMark` now draws the kit's tile instead of the old placeholder square |
