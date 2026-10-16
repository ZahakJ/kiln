# Visual review: WP-51 (brand kit import, ui-kit primitives)

Date: 2026-10-16. Reviewer: the WP-51 agent. Method: `pnpm --filter @kiln/ui-kit shots` renders every
primitive (including the ones that only exist while open) in the light and dark themes with the Write
app active, at 1440 px, with Figtree and Fraunces loaded from `assets/fonts/brand/`; the theme matrix
(`?view=matrix&app=notes|deck|home`) was rendered for the other apps and high contrast. Each capture was
compared side by side with the kit mockups and `brand/mockups/src/kiln-ui.css`.

Review sheets (committed): [primitives-light.png](visual-review-WP-51/primitives-light.png),
[primitives-dark.png](visual-review-WP-51/primitives-dark.png).

## Mockups checked

| Mockup | Elements compared | Result |
|---|---|---|
| `write-light.png` | Ribbon slab (panel, hairline, 14 px radius, group labels in faint 11 px); Paste well (40 px, 10 px radius, sunken); Cut/Copy/Format rows (24 px, 12 px text); Font dropdowns (28 px, 7 px radius, raised, faint chevron); Bold toggle on (app tint, cobalt text, 1 px app ring); Styles gallery cards (78 × 58, 9 px radius, selected = cobalt border + 2 px 22 % ring, "Aa Bb" in Figtree/Fraunces); Share button (solid cobalt, white, inner foot); Kbd ("Alt Q": 11 px 600, 2 px bottom border); segmented Headings/Pages/Results; zoom slider (app-ringed thumb on line-strong rail) | Match. Our gallery shows Fraunces headings in the "Kiln" theme as the mockup does. |
| `write-dark.png` | Same elements on the dark surfaces (panel `#201813`, raised `#2A201A`, line `#372A22`); cobalt tint `#1C2440` with `#97AEF4` text; solid buttons unchanged across themes | Match; dark is tuned, not inverted. |
| `grid-light.png`, `grid-dark.png` | Jade activation via `data-app="grid"` (solid `#17714E`, tint, text `#6FD0A6` in dark); dropdown and icon-button metrics in the Alignment/Number groups; status-bar zoom | Match (theme matrix for grid in the gallery). The sheet tabs (inverted arch) are chrome for WP-71. |
| `deck-light.png`, `deck-dark.png` | Ember activation: solid `#E05A27` with near-black `#1C0D06` text (the deliberate poster contrast), Bold toggle on in ember tint, focus ring in ember text | Match (matrix `app=deck`). |
| `notes-light.png`, `notes-dark.png` | Saffron activation: solid `#E8A82C` with near-black text, tint `#FAF0D9`, text `#87590A`; arch tab dot in saffron | Match (matrix `app=notes`). |
| `home-light.png`, `home-dark.png` | Ember New button (`variant="ember"` with the spark), Open file / All templates outline buttons (30 px, 9 px radius, line-strong), Recent/Pinned/Shared segmented control, eyebrow headers (Figtree 800, uppercase, +12 %), app tiles at 20 px (flat `-32` drawing) in the sidebar list | Match. The arch "doors" and the coming-from-Office banner are WP-58/WP-85 chrome. |
| `phone-light.png`, `phone-dark.png` | Tiles at 32 px, ember New, type scale at phone size | Primitives fit 390 px without horizontal overflow (gallery phone check); the phone layouts are WP-90. |
| `board.png` | Glaze values and AA text colours, Fraunces/Figtree usage, mark sizes (16/32/48/hero drawings) | Match: every colour in `@kiln/tokens` is a kit colour (unit-tested), tiles pick the drawing made for their size. |
| `brand/icons/size-check.png` | Tiles at 16, 20, 32, 48 and 72 px on bone and soot | Match: the 16 px pixel-fitted tiles stay crisp; the hero art is never scaled below 96 px. |

## Primitive by primitive

- **Button / IconButton / ToggleButton / SplitButton**: shapes, weights and the solid-button inner foot as
  in `kiln-ui.css` (`.btn`, `.btn.solid`, `.btn.ember`, `.ib`, `.ib.on`, `.big .ic`). Hover and pressed
  states (not drawn in the kit) mix the foreground into the surface; recorded in kit-gaps.md.
- **Menu, Popover, Select list, Combobox list**: panel, 12 px radius, `shadow.2`, 30 px rows with 7 px
  radius, eyebrow group labels; the current choice uses the app tint with the app bar, like the
  navigation pane's active heading in `write-light.png`. No menu mockup exists (kit gap 13).
- **Dialog**: panel, 16 px radius, Fraunces 20 px title, footer on the window colour with OK in the app
  solid. No dialog mockup exists (kit gap 12).
- **Tabs**: `arch` reproduces the ribbon's active arch with the app-colour dot; `segmented` reproduces
  `.seg`; `underline` remains for panes.
- **Toolbar, Select, Combobox, NumberInput**: 28 px controls, 7 px radius, raised fill, line border;
  focus shows the app colour border and a 2 px 22 % ring (the selected style card treatment).
- **Slider**: line-strong rail, app-coloured fill, raised thumb with a 2 px app ring (status-bar zoom).
- **Tooltip, Toast**: raised/panel surfaces with hairlines; toasts carry a short tone bar.
- **ScrollArea, Kbd, Gallery, ColorPicker**: scrollbars from the theme foreground; Kbd as the kit's
  "Alt Q"/"Ctrl K" keycaps; gallery cards as the Styles gallery; colour headings as eyebrows and the
  selected swatch ringed in the app text colour.
- **Brand glyph set**: the five app marks, the spark and the flame render from the kit's own paths; the
  flame keeps its fire colours (saving is the only state that gets a flame).

## Issues found and fixed during the review

1. Ribbon large-button icons were 32 px in a 40 px well (too heavy against `write-light.png`); now 24 px.
2. The menu capture cut off the parent menu; the sheet now frames the menu and its submenu together.
3. The gallery still offered the old UI fonts in its font list; it now lists Figtree and Fraunces as the
   theme fonts, as the mockups' font box does.

## Open items (owned by later WPs)

- The ribbon, title bar, File button, status bar and Home screens themselves are chrome (WP-58); they
  already pick up the new tokens through the semantic aliases but are not yet laid out like the mockups.
- `apps/web/index.html` still carries the old theme-color values and does not link
  `manifest.webmanifest` (handoff to WP-58/WP-91 in docs/qa/handoffs.md).
