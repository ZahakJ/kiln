# Brand kit gaps

Things the brand kit (`brand/`, imported in WP-51) does not define but later work packages need. For
each, what we do until the design team fills the gap. Each entry names who needs it. Please send
answers as an updated kit; `tools/import-brand.sh` takes it from there.

## Colour and themes

1. **High-contrast theme.** The kit defines light and dark only. Kiln ships a high-contrast theme
   (`data-theme="hc"`) for accessibility (PLAN.md §1.2, §5). Until the kit has one, it is assembled
   from kit colours only: char `#0F0805` ground, bone `#FFF6E8` text and strong lines, the dark-theme
   muted colour for hairlines, the dark-theme text colours for the apps, and the fire flame `#FFB547`
   for focus rings and Home (all text pairs ≥ 4.5:1, tested). *Needed by:* every UI WP; WP-58 Options.
2. **Info colour.** The kit has danger, success and warning but no info/neutral status colour. We use
   the Write text colour (cobalt) for `--kiln-info`. *Needed by:* toasts, message bars (WP-58).
3. **Status tints.** No subtle backgrounds for danger/success/warning; we mix 14 % of the status colour
   into the panel. *Needed by:* message bars, validation (WP-58, WP-72).
4. **Overlay/scrim.** No modal backdrop colour; we use soot at 40 % (light) and char at 60 % (dark).
5. **Third elevation.** The kit has `shadow.1`, `shadow.2` and the page shadow. Dialogs use the page
   shadow in light and `shadow.2` in dark (the dark page shadow is an ember glow, wrong for a dialog).
   A dialog/menu elevation from the kit would be better. *Needed by:* WP-58, WP-85.
6. **Tooltip colours.** No tooltip treatment. Primitives use a raised panel with a hairline; Key Tip
   badges use tenmoku/bone (light) and raised/fg (dark).
7. **Text on the canvas.** Deck and Home text (`#B2431A`) on the light canvas `#EDE4D8` is 4.497:1, a
   hair under AA. Only neutral text is placed on the canvas until the kit adjusts one of them.
8. **Document chart palette.** The mockups' charts use the glazes; the kit does not name a chart series
   order or an Office theme colour mapping (Accent 1–6). *Needed by:* WP-73 charts, WP-78 Deck themes.

## Type

9. **Monospace.** BRAND.md says no monospace in the brand or chrome. Code-like fields (formula bar,
   hex input) use Figtree with tabular figures (`--kiln-font-mono` aliases the UI face). If the formula
   bar needs a monospace for alignment, the kit should name one. *Needed by:* WP-71.
10. **Italic.** Figtree and Fraunces ship as upright variable fonts only; italics in the chrome are
    synthesised by the browser. The "Kiln" document theme (Figtree body, Fraunces headings) needs true
    italics. *Needed by:* WP-61/64 default document theme.
11. **Type scale roles.** The kit gives sizes (xs … hero) but not which role uses which (dialog title,
    pane title, group label). Primitives use: dialog title Fraunces 20 px 700, eyebrows 10.5–12 px
    Figtree 800 uppercase +12 %, body 13 px.

## Components and screens

12. **Dialogs and Backstage.** No mockup of a dialog (Font, Paragraph, Format Cells with its six tabs,
    Page Setup) or of the File menu (Backstage). We follow `kiln-ui.css` (fields, segmented control,
    buttons) with Fraunces titles. *Needed by:* WP-58, WP-85.
13. **Menus and context menus, the mini toolbar.** Not drawn. Menus use the panel, 12 px radius,
    `shadow.2`, 30 px rows and eyebrow group labels. *Needed by:* WP-58.
14. **Phone variants.** Only the phone Home is drawn; there is no phone Write/Grid/Deck/Notes editor,
    no phone dialog or bottom sheet, no phone ribbon. *Needed by:* WP-90.
15. **Tablet / Simplified ribbon.** The single-row Simplified ribbon (default below 1100 px and in
    Notes, PLAN.md §1.2) is not drawn. *Needed by:* WP-58, WP-86.
16. **Hover, pressed and disabled states.** The mockups show rest and selected states only. Primitives
    derive hover/pressed by mixing the foreground into the surface (4–12 %) and use the faint colour for
    disabled text.
17. **Motion.** `motion.fast/base/slow` and one easing are given; no enter/exit choreography for menus,
    panes and dialogs. We use fast (120 ms) for menus, base (180 ms) for panes and dialogs, no motion
    under `prefers-reduced-motion`.

## Icons

18. **Office command glyphs.** The kit's glyph set is the five app marks, the spark and the flame; the
    mockups use Lucide for every command. Office-specific commands Lucide lacks (Format Painter, Merge &
    Center, Freeze Panes, Track Changes, …) keep the in-house Kiln Glyphs (`packages/ui-kit/src/icons/glyphs`),
    drawn to Lucide's metrics. A kit-drawn set would make them unmistakably Kiln. *Needed by:* WP-58,
    WP-64, WP-71, WP-78.
19. **Document-type icons.** File associations use the app tiles (`write.png`, `grid.png`, `deck.png`);
    the kit has no document icons (a page with the tile), which Windows and macOS show in file managers.
    *Needed by:* WP-91 packaging.
20. **Android splash and notification icons.** Not provided (the monochrome adaptive layer is used for
    themed icons). *Needed by:* WP-90.
21. **iOS app icon background.** Not a target platform, but `pnpm tauri icon` generates iOS icons; they
    sit on bone `#FFF6E8`.
