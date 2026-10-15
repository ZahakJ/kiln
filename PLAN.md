# Officina — Implementation Plan

**Document status:** v1.0, 2026-10-15. This is the single source of truth for the autonomous build. Agents executing a work package (WP) read this file, the repo, and nothing else. Every WP's acceptance criteria are shell-verifiable.

**Repository root:** `/home/muth/OfficeSuite` (this file lives at its root and is committed).

**Contents:** §0 Executive summary · §1 Naming, brand, design language, "feel at home" UX (ribbons, backstage, shortcuts, mobile) · §2 Feature matrix · §2A Competitive landscape and quality bars · §3 Architecture · §4 Toolchain, repository and commit cadence · §5 Testing & QA · §6 Milestones and work packages · §7 Parity matrix and continuous loop · §8 Effort estimate · §9 Risks and honesty · §10 Definition of Done · §11 Machine-readable work packages (JSON)

---

## 0. Executive summary

Officina (Latin for "workshop", the root of the word *office*) is an open-source (AGPL-3.0-or-later) cross-platform office suite that opens, edits and saves real Microsoft Office files (`.docx`, `.xlsx`, `.pptx`, plus CSV, PDF export and ODF import/export) and feels immediately familiar to Office users (ribbon, backstage, status bar, Office shortcuts) while looking cleaner and adding features Office lacks (command palette, built-in version history, live collaboration, Markdown shortcuts, formula autocompletion with inline docs, presenter view everywhere, focus mode, AI-ready hooks).

| App | Office equivalent | Scope tier |
|---|---|---|
| **Scriba** (Write) | Word | Tier 1 (full) |
| **Tabula** (Grid) | Excel | Tier 1 (full) |
| **Scena** (Deck) | PowerPoint | Tier 1 (full) |
| **Notula** (Notes) | OneNote (notebooks, sections, pages, tags) | Tier 2 (lite, reuses Write engine) |
| **Atrium** (Hub) | Office start screen / Home | Tier 1 (launcher, recents, templates) |
| Calendar / Mail / Tasks | Outlook | **Out of scope** (explicitly deferred; see §9) |

**Stack (decided, versions verified against the npm registry on 2026-10-15):**

- One TypeScript + React 19 web core in a **pnpm + Turborepo monorepo**.
- Desktop: **Electron 44** (`electron-vite`, `electron-builder` → AppImage/deb locally; NSIS/dmg via GitHub Actions).
- Android: **Capacitor 8** (Java 21 + Android cmdline-tools in `~/Android`; debug APK built locally with Gradle wrapper).
- Web: installable **PWA** (same build as the shells).
- Document editor: **TipTap 3 / ProseMirror** with a custom pagination layer.
- Spreadsheet: **custom canvas grid** + **our own MIT formula engine** (HyperFormula rejected: GPL-3.0-only).
- Presentations: custom DOM/SVG slide stage with TipTap text bodies.
- File formats: **our own OOXML layer** (OPC container over `fflate` + `fast-xml-parser`) with *opaque part passthrough* for round-trip fidelity; `mammoth`, `exceljs`, `docx`, `pptxgenjs`, `docx-preview` and **LibreOffice 24.2 headless (already installed at `/usr/bin/soffice`)** are used only as *test oracles*, never shipped.
- Collaboration/undo/offline: **Yjs** (+ `y-prosemirror`, `y-indexeddb`, `@hocuspocus/server`).
- Charts: **ECharts 6** (Apache-2.0).
- Tests: Vitest 5, Playwright 1.64 (Chromium headless + screenshots that agents review visually), axe-core.

**Why not Tauri 2?** No Rust toolchain is installed, agents cannot debug Rust compile issues cheaply at 2-way concurrency, Tauri's Android path still needs the NDK + Rust targets, and Tauri's WebView differs per OS (WebKitGTK on Linux), which hurts rendering fidelity for a WYSIWYG document editor. Electron ships Chromium everywhere, so pagination/print output is identical across desktop OSes. Cost: ~100 MB larger installers and ~150 MB more RAM. Accepted.

**Why not Flutter / Kotlin native for Android?** One codebase is the only way to ship three editors on four platforms with two concurrent agents. Capacitor gives a native shell (file intents, share sheet, SAF file access, status bar, keyboard insets) around the same web core; the mobile UX is adapted in CSS/React (§1.6), not re-implemented.

---

## 1. Product, brand, design language and "feel at home" UX spec

### 1.1 Naming

**Suite: officina** (Latin *officīna*, "workshop" — the word that became *office*; a place where things are made rather than administered). Repo `ZahakJ/officina`; written **Officina** in prose and in the UI; one word, lowercase, in the house style of `concord`, `astrolabe`, `abax`, `qarid`, `mimema`. The products are the rooms and tools of that workshop, all plain Latin nouns that an English speaker can say and spell, each hinting at what it does:

| Product | Meaning | Replaces | Native file type | Accent (light / dark) | Icon motif |
|---|---|---|---|---|---|
| **Scriba** | *scriba*, "scribe, the one who writes" | Word | `.scriba` (also `.docx .dotx .docm .odt .md .txt .html`) | blue `#2B5FD9` / `#6B8FF0` | nib stroke forming an S |
| **Tabula** | *tabula*, "table, tablet, ledger" (as in *tabula rasa*) | Excel | `.tabula` (also `.xlsx .xltx .xlsm .csv .tsv .ods .json`) | green `#1F8A5B` / `#4FC08D` | 2×2 grid with one filled cell |
| **Scena** | *scaena/scena*, "the stage" | PowerPoint | `.scena` (also `.pptx .potx .pptm .odp`) | orange `#D9552B` / `#F2825E` | 16:9 frame with a spotlight dot |
| **Notula** | *notula*, "a little note" | OneNote | notebook folder of `.scriba` pages + `notebook.json` | violet `#7A3FD1` / `#A97CF0` | dog-eared page with a tag |
| **Atrium** | *atrium*, "the entrance hall" | Office start screen / Home | — | brand accent | arch (doorway) |

Office users map the set instantly by colour (Word blue, Excel green, PowerPoint orange, OneNote purple are kept) and by the name's hint (scribe → writing, table → spreadsheet, stage → presentations, note → notes, hall → the place you enter). The suite reads as one family: same Latin register, same two- or three-syllable rhythm, same icon style (one bold stroke inside a rounded square tinted with the product accent).

**Identifiers.** npm scope `@officina/*`; Electron appId and Android applicationId `io.github.zahakj.officina`; executable `officina`; product name `Officina`. **Internal code identifiers keep generic short names** so the code reads plainly and never has to be renamed if marketing changes: routes `/write /grid /deck /notes /` and packages `editor-write`, `sheet-model`, `editor-deck`, `notes`, etc. Throughout this plan, the short names **Write, Grid, Deck, Notes and Hub** refer to **Scriba, Tabula, Scena, Notula and Atrium**; every user-facing string (window titles, ribbon, Backstage, Hub cards, file dialogs, manual, README, landing page, store listings) uses the product names.

**Availability check (2026-10-15):** `gh repo view ZahakJ/<name>` fails with "Could not resolve to a Repository" (= free) for `officina`, `scriba`, `tabula`, `scena`, `notula` and `atrium`. Only `officina` is created; the product names are not separate repos.

**Runner-up name sets considered and rejected:**

1. **diwan / qalam / jadwal / lawha / daftar / majlis** (Arabic: chancellery / pen / table / board / notebook / hall). Beautiful and in the house style, but *jadwal* and *lawha* mean nothing to most switchers and *lawha* is hard for English speakers to say; the mapping to Excel/PowerPoint would have to be learned. Rejected for opacity, kept as the Arabic locale's optional "classical names" Easter egg? No — one name set only (consistency beats cleverness).
2. **ergon / grapho / pinax / theatron / mnema / stoa** (Greek: work / I-write / tablet / theatre / memory / colonnade). *grapho* reads as "graphics", *pinax* is obscure, *theatron* is long, and the set sounds academic rather than friendly. Rejected.
3. **meridian + Write / Grid / Deck / Notes / Hub** (brand word plus generic English nouns, the way Office uses Word). Easy to map, but the generic nouns are unownable and collide with LibreOffice *Writer*, GridCraft, Pitch "Deck", Apple *Notes*; the set feels like a feature list, not a family. Rejected (the generic nouns survive as internal identifiers, where that is a virtue).

### 1.1a Brand, licence, repository

- **Tagline:** "Documents, spreadsheets and slides that feel like home." Banner spec line: `DOCX · XLSX · PPTX • AGPL-3.0-or-later • Linux · macOS · Windows · Android`.
- **Mark:** a bold lowercase "o" whose stroke opens at the bottom into an arch — the workshop doorway — inside a rounded square; each app's icon replaces the inner glyph with its motif from the table above and tints the square with its accent. Source of truth: `docs/media/icon.svg` (+ `icon-scriba.svg`, `icon-tabula.svg`, `icon-scena.svg`, `icon-notula.svg`); every raster (Electron icons, Android mipmaps, PWA icons, favicon, banner, social preview) is generated from them by `tools/render-media.mjs` (Playwright screenshot of SVG), never hand-edited.
- **Brand accent** (banner underline, badges, landing page `--accent`): `#3F6FE0` on dark, `#2B58C7` on light. Badge colour hex: `3f6fe0`.
- **Licence: GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later).** Rationale: (1) the suite ships a collaboration server (`apps/collab-server`) and a PWA, i.e. it can be offered as a hosted service; AGPL keeps hosted forks open, the same reason `concord` is AGPL while the single-user tools (`astrolabe`, `abax`) are MIT; ONLYOFFICE, Euro-Office and Collabora chose the same licence for the same reason (§2A). (2) Every runtime dependency is permissive (MIT / Apache-2.0 / BSD / ISC / MPL-2.0 file-level / OFL-1.1 fonts), all combinable into an AGPL work; the audit lives in `THIRD-PARTY-NOTICES.md` and is enforced by `tools/check-licenses.mjs` in CI (denied: `GPL-2.0-only`, `SSPL`, `BUSL`, `CC-BY-NC-*`, unknown). (3) GPL-3.0-only code (e.g. HyperFormula) *is* legally combinable with an AGPL-3.0 work (GPLv3 §13); we still build our own formula engine (§3.4) for Excel-semantics control and so that `packages/formula` could later be relicensed permissively, but HyperFormula and Univer's Apache-2.0 `@univerjs/engine-formula` are admissible fallbacks if WP-08/WP-10 slip (§9). LibreOffice (MPL-2.0) is a dev-time test oracle only and is never bundled. Contributions are accepted under the same licence (no CLA; DCO sign-off requested in CONTRIBUTING.md).
- **Repository:** public at `https://github.com/ZahakJ/officina`, default branch `main`, homepage `https://zahakj.github.io/officina/` (GitHub Pages from `/docs` on `main`), topics `office-suite, word-processor, spreadsheet, presentation, docx, xlsx, pptx, electron, capacitor, react, typescript, yjs, offline-first, android`. Description: "Open-source office suite for Linux, macOS, Windows and Android: documents, spreadsheets and slides that open and save Office files and feel like home." See §4.6 for repo hygiene and §4.7 for the commit cadence.

### 1.2 Design language: "Horizon"

Design tokens live in `packages/tokens` as CSS custom properties (`--of-*`), exported also as a TS object and a Tailwind 4 `@theme` block. Light and dark themes plus a high-contrast theme; theme follows the OS by default and is user-switchable (View ▸ Theme and Backstage ▸ Account).

**Colour tokens (light / dark):**

| Token | Light | Dark | Use |
|---|---|---|---|
| `--of-bg-canvas` | `#EEF1F6` | `#15171C` | Area behind pages/sheets |
| `--of-bg-surface` | `#FFFFFF` | `#1E2127` | Ribbon, panes, dialogs |
| `--of-bg-surface-2` | `#F6F8FB` | `#262A31` | Ribbon tab strip, status bar |
| `--of-fg` | `#1B1F27` | `#E8EAEE` | Primary text |
| `--of-fg-muted` | `#5A6170` | `#9AA1AE` | Secondary text |
| `--of-border` | `#D8DDE6` | `#333843` | Hairlines |
| `--of-accent-write` | `#2B5FD9` | `#6B8FF0` | Write |
| `--of-accent-grid` | `#1F8A5B` | `#4FC08D` | Grid |
| `--of-accent-deck` | `#D9552B` | `#F2825E` | Deck |
| `--of-accent-notes` | `#7A3FD1` | `#A97CF0` | Notes |
| `--of-accent-hub` | `#2B5FD9` | `#6B8FF0` | Hub |
| `--of-selection` | `accent @ 18%` | `accent @ 28%` | Selection fills |
| `--of-danger / --of-warning / --of-success` | `#C8352B / #B7791F / #1F8A5B` | `#F07167 / #E0A52F / #4FC08D` | States |

The active app's accent is exposed as `--of-accent`. Ribbon tabs underline in the accent; the title bar is neutral (not a solid coloured bar like Office 2013) to look lighter.

**Typography:** UI font **Inter Variable** (`@fontsource-variable/inter` 5.3.0, OFL-1.1); monospace **JetBrains Mono** (`@fontsource/jetbrains-mono`, OFL-1.1). UI type scale: 11 / 12 / 13 (base) / 15 / 18 / 24 px. Document fonts bundled for fidelity (all SIL OFL, metric-compatible with Office defaults): **Liberation Sans/Serif/Mono** (= Arial / Times New Roman / Courier New), **Carlito** (= Calibri), **Caladea** (= Cambria). The font-substitution table in `packages/doc-model/src/fonts/substitution.ts` maps Office names → bundled fonts so line breaks and pagination match Word closely. **Aptos** (Office's current default) has no metric-compatible open font; we substitute Inter and record this in the Compatibility Checker.

**Spacing & shape:** 4 px base grid; radii 4 (controls) / 6 (menus, cards) / 10 (dialogs); shadows `--of-shadow-1..3` (soft, low spread); 1 px borders only, no bevels. Motion: 120 ms ease-out for menus, 200 ms for panes, respects `prefers-reduced-motion`.

**Iconography:** **Lucide** (ISC, `lucide-react` 1.57) for general UI; **custom "Officina Glyphs" SVG set** (`packages/ui-kit/src/icons/glyphs/`) for the ~120 Office-specific commands Lucide lacks (Format Painter, Increase Indent, Merge & Center, Wrap Text, Freeze Panes, Slide Layout, Track Changes, etc.). Icons are 20 px in the ribbon at the default density and 16 px in menus; stroke 1.5 px; monochrome using `currentColor`, with an optional "colourful" icon style toggle (Office 2019 style) for people who miss it.

**Density:** two ribbon layouts, exactly like Office: **Classic** (3-row tall ribbon, default on desktop ≥ 1100 px wide) and **Simplified** (single-row, default below 1100 px and in Notes). Ribbon can be collapsed (Ctrl+F1) to tabs only.

### 1.3 Familiar chrome: window anatomy (desktop/web)

```
┌───────────────────────────────────────────────────────────────────────────────┐
│ ● App mark │ Document title ▾ (rename) │ AutoSave ⟳ │  🔍 Search / Tell me (Alt+Q) │ 👤 │ Share │ ⌃ │  (Title bar)
├───────────────────────────────────────────────────────────────────────────────┤
│ File  Home  Insert  Layout  References  Review  View   …  ⚙ Ribbon display ▾ │  (Tab strip)
├───────────────────────────────────────────────────────────────────────────────┤
│ [Clipboard]  [Font]  [Paragraph]  [Styles ▸ gallery]  [Editing]              │  (Ribbon groups, with group labels + launchers ↘)
├──────────┬────────────────────────────────────────────────────────┬───────────┤
│ Nav pane │           Document canvas / grid / slide stage         │ Task pane │
│ (opt.)   │                                                        │ (Styles,  │
│          │                                                        │ Comments, │
│          │                                                        │ Format…)  │
├──────────┴────────────────────────────────────────────────────────┴───────────┤
│ Page 3 of 12 │ 1,284 words │ English (US) │ Track Changes: Off │ [Layout ▦ ▤ ▥] [− ──●── +] 100% │ (Status bar)
└───────────────────────────────────────────────────────────────────────────────┘
```

- Quick Access Toolbar is *not* replicated as a separate row (Office itself hid it in 2022); instead Undo/Redo/Save live in the title bar left of the title, Office-style, and users may pin any command to the title bar (right-click ▸ Pin to title bar).
- Right-click shows Office-style **mini toolbar** (font, size, bold, italic, highlight, colour) above the context menu in Write/Deck; Grid shows the number-format mini toolbar.
- **Dialog launchers** (↘ in group corners) open the big dialogs people know: Font, Paragraph, Format Cells (with its six tabs: Number, Alignment, Font, Border, Fill, Protection), Page Setup.
- **Key Tips:** pressing Alt shows letter badges on tabs/commands (Alt, H, B = bold), matching Office letters where they exist (`H`ome, `N` Insert, `P` Layout, `S` References, `R` Review, `W` View, `M` Formulas, `A` Data, `G` Design, `K` Transitions, `A`nimations…).

### 1.4 Ribbon tabs and groups per app (exact)

Each entry is `Tab ▸ Group: commands`. Items marked ★ are Officina extras (not in Office); items marked (P1)/(P2) ship in later tiers but their ribbon slots exist from the start (greyed with a "coming soon" tooltip is **not** acceptable in release builds — tier gating hides them instead).

**Write**

- **File** → Backstage (§1.5)
- **Home** ▸ Clipboard: Paste ▾ (Keep Source / Merge / Text only / Paste Special…), Cut, Copy, **Format Painter** ▸ Font: family, size, Grow/Shrink (Ctrl+]/[), Change Case, Clear Formatting, Bold, Italic, Underline ▾, Strikethrough, Subscript, Superscript, Text Effects (P2), Highlight ▾, Font Colour ▾ ▸ Paragraph: Bullets ▾, Numbering ▾, Multilevel ▾, Decrease/Increase Indent, Sort, Show ¶, Align L/C/R/Justify, Line & Paragraph Spacing ▾, Shading ▾, Borders ▾ ▸ Styles: style gallery (Normal, No Spacing, Heading 1–3, Title, Subtitle, Subtle Emphasis, Emphasis, Intense Emphasis, Strong, Quote, Intense Quote, Subtle/Intense Reference, Book Title, List Paragraph) + Styles pane launcher ▸ Editing: Find ▾ (Find, Advanced, Go To), Replace, Select ▾ ▸ ★Focus: Focus Mode, ★Command Palette
- **Insert** ▸ Pages: Cover Page ▾, Blank Page, Page Break ▸ Tables: Table ▾ (grid picker, Insert Table…, Convert Text to Table, ★Paste Markdown table) ▸ Illustrations: Pictures ▾ (This device, ★Unsplash-style stock hook), Shapes ▾, Icons, Chart (via Grid chart engine), Screenshot (Electron only) ▸ Links: Link, Bookmark, Cross-reference ▸ Comments: Comment ▸ Header & Footer: Header ▾, Footer ▾, Page Number ▾ ▸ Text: Text Box ▾, Quick Parts/Fields ▾, WordArt (P2), Drop Cap, Date & Time, Object (P2) ▸ Symbols: Equation (P1, MathLive), Symbol ▾ ▸ ★Smart: Table of Contents (quick), ★Callout block, ★Code block with syntax highlighting
- **Layout** ▸ Page Setup: Margins ▾, Orientation ▾, Size ▾, Columns ▾, Breaks ▾, Line Numbers ▾ (P2), Hyphenation ▾ ▸ Paragraph: Indent Left/Right, Spacing Before/After ▸ Arrange: Position ▾, Wrap Text ▾, Bring Forward, Send Backward, Selection Pane, Align ▾, Group, Rotate
- **References** ▸ Table of Contents: TOC ▾, Add Text, Update Table ▸ Footnotes: Insert Footnote, Insert Endnote, Next Footnote, Show Notes ▸ Citations & Bibliography (P2): Insert Citation, Manage Sources, Style, Bibliography ▸ Captions: Insert Caption, Table of Figures (P1), Cross-reference ▸ Index (P2)
- **Review** ▸ Proofing: Spelling & Grammar (Hunspell via `nspell`), Thesaurus (P2), Word Count ▸ Language: Translate (hook), Language ▸ Comments: New, Delete ▾, Previous, Next, Show Comments ▸ Tracking: Track Changes ▾, display mode (Simple/All/None/Original), Show Markup ▾, Reviewing Pane ▸ Changes: Accept ▾, Reject ▾, Previous, Next ▸ Compare (P2) ▸ Protect: Restrict Editing (P2) ▸ ★History: Version History
- **View** ▸ Views: Read Mode, Print Layout, Web Layout, Outline (P1), Draft ▸ Immersive: ★Focus, Immersive Reader (P2) ▸ Page Movement: Vertical, Side to Side ▸ Show: Ruler, Gridlines, Navigation Pane ▸ Zoom: Zoom, 100%, One Page, Multiple Pages, Page Width ▸ Window: New Window, Split, Side by Side (Electron) ▸ Theme: Light/Dark/System/High contrast ▸ Macros (greyed with explanation — not supported, §9)
- **Contextual:** Table Design (styles gallery, header row/banded rows toggles, shading, borders), Table Layout (insert/delete rows/cols, merge/split, cell size, distribute, alignment, repeat header rows, convert to text, formula), Picture Format (corrections, crop, wrap, position, alt text), Shape Format, Header & Footer tools (different first page, odd/even, link to previous, position).

**Grid**

- **Home** ▸ Clipboard (same + Paste Special: Values/Formulas/Formats/Transpose) ▸ Font: family, size, B I U, borders ▾ (All, Outside, Thick box, Bottom, Top, Left, Right, No border, More borders…), Fill colour ▾, Font colour ▾ ▸ Alignment: Top/Middle/Bottom, Orientation ▾, Wrap Text, Left/Centre/Right, Indent −/+, **Merge & Center** ▾ (Merge Across, Merge Cells, Unmerge) ▸ Number: format combo (General, Number, Currency, Accounting, Short Date, Long Date, Time, Percentage, Fraction, Scientific, Text, More…), Accounting $ ▾, %, Comma style, Increase/Decrease Decimal ▸ Styles: Conditional Formatting ▾ (Highlight Cells, Top/Bottom, Data Bars, Colour Scales, Icon Sets, New Rule, Clear, Manage), Format as Table ▾, Cell Styles ▾ ▸ Cells: Insert ▾, Delete ▾, Format ▾ (row height, autofit, column width, hide/unhide, rename sheet, tab colour, protect) ▸ Editing: AutoSum ▾ (Sum, Average, Count Numbers, Max, Min), Fill ▾ (Down/Right/Up/Left/Series/Flash Fill(P1)), Clear ▾, Sort & Filter ▾, Find & Select ▾ ▸ ★Palette
- **Insert** ▸ Tables: PivotTable (P1), Table (Ctrl+T) ▸ Illustrations: Pictures, Shapes (P1), Icons ▸ Charts: Column/Bar, Line/Area, Pie/Doughnut, Scatter/Bubble, Hierarchy (treemap, sunburst), Statistic (histogram, box&whisker P1), Waterfall/Funnel (P1), Combo, Recommended Charts★(from selection shape) ▸ Sparklines: Line, Column, Win/Loss (P1) ▸ Filters: Slicer (P2), Timeline (P2) ▸ Links: Link ▸ Comments: Note, Threaded Comment ▸ Text: Text Box, Header & Footer, WordArt(P2) ▸ Symbols: Equation(P2), Symbol
- **Page Layout** ▸ Themes (P1) ▸ Page Setup: Margins, Orientation, Size, Print Area, Breaks, Background, Print Titles ▸ Scale to Fit: Width, Height, Scale ▸ Sheet Options: Gridlines view/print, Headings view/print ▸ Arrange (shapes)
- **Formulas** ▸ Function Library: Insert Function (fx dialog with search + inline docs), AutoSum, Recently Used, Financial, Logical, Text, Date & Time, Lookup & Reference, Math & Trig, More ▾ (Statistical, Engineering, Information, Web(P2), Cube✗) ▸ Defined Names: Name Manager, Define Name, Use in Formula, Create from Selection ▸ Formula Auditing: Trace Precedents, Trace Dependents, Remove Arrows, Show Formulas (Ctrl+`), Error Checking, Evaluate Formula, Watch Window (P1) ▸ Calculation: Calculation Options (Auto/Manual), Calculate Now (F9), Calculate Sheet (Shift+F9) ▸ ★Formula Lab: Formula explainer pane (AST tree + step values)
- **Data** ▸ Get & Transform: From Text/CSV, From JSON★, From Web (P2), Refresh ▸ Sort & Filter: Sort A→Z, Z→A, Sort…, Filter, Clear, Reapply, Advanced (P2) ▸ Data Tools: Text to Columns, Flash Fill (P1), Remove Duplicates, Data Validation ▾, Consolidate (P2), Relationships✗ ▸ Forecast: What-If ▾ (Scenario Manager P2, Goal Seek P1, Data Table P2), Forecast Sheet (P2) ▸ Outline: Group, Ungroup, Subtotal (P1), Show/Hide Detail
- **Review** ▸ Proofing: Spelling ▸ Comments: New, Delete, Previous, Next, Show ▸ Notes ▸ Protect: Protect Sheet (P1), Protect Workbook (P1), Allow Edit Ranges (P2) ▸ ★Version History
- **View** ▸ Workbook Views: Normal, Page Break Preview, Page Layout, Custom Views (P2) ▸ Show: Ruler (page layout), Gridlines, Formula Bar, Headings ▸ Zoom: Zoom, 100%, Zoom to Selection ▸ Window: New Window, Arrange, **Freeze Panes ▾ (Freeze Panes / Top Row / First Column)**, Split, Hide/Unhide, Side by Side ▸ Theme ▸ Macros (greyed, §9)
- **Contextual:** Table Design (table name, resize, remove duplicates, convert to range, style options, styles gallery), Chart Design (add element, quick layout, change colours, chart styles, switch row/column, select data, change type, move chart), Chart Format, PivotTable Analyze/Design (P1), Picture Format.
- **Below ribbon:** Name Box + Formula Bar (expandable, with fx button, cancel/enter). **Bottom:** sheet tabs (+ add, right-click: insert/delete/rename/move or copy/tab colour/hide/protect), tab scroll, status bar with Ready/Enter/Edit mode, **Average/Count/Sum of selection** (configurable), Page Layout buttons, zoom slider.

**Deck**

- **Home** ▸ Clipboard ▸ Slides: New Slide ▾ (layouts gallery), Layout ▾, Reset, Section ▾ ▸ Font (as Write + Character Spacing ▾, Shadow) ▸ Paragraph (as Write + Columns, Text Direction ▾, Align Text ▾ (top/middle/bottom), Convert to SmartArt✗→★"Convert to list diagram") ▸ Drawing: Shapes gallery, Arrange ▾, Quick Styles, Shape Fill, Outline, Effects ▸ Editing ▸ ★Designer: Design Ideas pane
- **Insert** ▸ Slides: New Slide, Reuse Slides (P1) ▸ Tables ▸ Images: Pictures, Screenshot, Photo Album(P2) ▸ Illustrations: Shapes, Icons, 3D✗, SmartArt✗ (import renders as grouped shapes), Chart ▸ Links: Link, Action (P2), Zoom(P2) ▸ Comments ▸ Text: Text Box, Header & Footer, WordArt(P2), Date & Time, Slide Number, Object(P2) ▸ Symbols: Equation(P1), Symbol ▸ Media: Video, Audio (P1), Screen Recording✗ ▸ ★Code block, ★Live Grid range (linked table from a Grid file, P2)
- **Design** ▸ Themes gallery (12 bundled themes, OOXML theme-compatible) ▸ Variants (colour/font/effects/background) ▸ Customise: Slide Size ▾ (16:9, 4:3, custom), Format Background ▸ ★Designer: Design Ideas
- **Transitions** ▸ Preview ▸ Transition gallery (None, Morph(P2), Fade, Push, Wipe, Split, Reveal, Cut, Random Bars(P1), Shape, Uncover, Cover, Flash) ▸ Timing: Sound(P2), Duration, Apply To All, Advance: On Click / After
- **Animations** ▸ Preview ▸ gallery (Entrance: Appear, Fade, Fly In, Float In, Split, Wipe, Zoom, Grow & Turn(P1); Emphasis: Pulse, Colour Pulse, Teeter, Spin, Grow/Shrink; Exit: Disappear, Fade, Fly Out, Zoom) ▸ Effect Options ▸ Advanced: Add Animation, **Animation Pane**, Trigger(P2), Painter ▸ Timing: Start (On Click/With Previous/After Previous), Duration, Delay, Reorder
- **Slide Show** ▸ Start: From Beginning (F5), From Current (Shift+F5), ★Present in Browser Tab, Custom Show (P2) ▸ Set Up: Set Up Slide Show, Hide Slide, Rehearse Timings (P1), Record (P2) ▸ Monitors: **Presenter View** toggle, Use Presenter View ▸ ★Audience: Laser pointer, Pen, Highlighter, Blackout (B/W keys)
- **Review** ▸ Proofing ▸ Comments ▸ Compare(P2) ▸ ★Version History
- **View** ▸ Presentation Views: Normal, Outline View, Slide Sorter, Notes Page, Reading View ▸ Master Views: Slide Master, Handout Master (P2), Notes Master (P2) ▸ Show: Ruler, Gridlines, Guides, Notes ▸ Zoom, Fit to Window ▸ Colour/Grayscale (P2) ▸ Window ▸ Theme ▸ Macros (greyed)
- **Contextual:** Shape Format (insert shapes, shape styles, WordArt styles, arrange, size), Picture Format, Table Design/Layout, Chart Design/Format, Video Format.
- **Chrome:** left **thumbnail rail** (drag-reorder, sections, right-click menu), **notes pane** below the stage (toggle), status bar: "Slide 3 of 24", theme name, Notes, Comments, view buttons, zoom slider, **Fit slide to window** button.

**Notes (Tier 2)**: Home (Clipboard, Basic Text, Styles, Tags: To Do (Ctrl+1), Important (Ctrl+2), Question (Ctrl+3), custom), Insert (Table, File attachment, Picture, Link, Date/Time, Code block, Equation), Draw (ink P2), View (Page colour, Rule lines, Zoom, Navigation panes). Left: notebooks ▸ sections (tabs) ▸ pages list; page canvas is a Write document with free title + timestamp.

### 1.5 Backstage (File menu)

Full-screen overlay (Esc / Back arrow closes), left nav exactly in Office order: **Home** (recents + pinned + templates strip), **New** (template gallery with search; "Blank"; per-app templates from `packages/templates`), **Open** (Recent, Pinned, This Device, ★Folders (desktop), Browse…), **Info** (Protect Document(P1: password P2, mark final), Inspect/Compatibility Checker★ shows features that won't survive .docx/.xlsx/.pptx, Properties panel: title, author, words, pages, created/modified, **Version History** button), **Save** (Ctrl+S), **Save As** / **Save a Copy** (format picker: native, Office, ODF, PDF, Markdown/CSV/HTML where applicable), **Print** (Ctrl+P: preview on the right, printer/pages/copies/orientation/scaling/margins on the left, "Print to PDF"), **Share** (copy link to collab session, export email attachment via OS share on Android), **Export** (PDF/XPS→PDF only, change file type, ★Markdown, ★HTML, ★images for slides), **Close**, then **Account** (theme, language, sign-in hooks) and **Options** (Office-style dialog: General, Display, Proofing, Save (AutoSave interval, recovery), Language, Advanced, Customise Ribbon(P2), Keyboard★ (view/remap every shortcut), Privacy).

### 1.6 Start screen (Hub)

Left rail: Home, New, Open; app filter chips (All, Write, Grid, Deck, Notes). Hero row: "Blank document / Blank workbook / Blank presentation / New notebook" cards in accent colours plus 8 featured templates. Below: **Recent** (list with name, location, last opened, pin), **Pinned**, **Shared with me** (collab). Search box filters recents and templates. Opening a file from the OS skips Hub and opens the right app directly.

### 1.7 Keyboard shortcut table (must all work; tested by Playwright)

Office defaults are kept unchanged. Mac uses ⌘ for Ctrl except where noted.

| Shortcut | Write | Grid | Deck |
|---|---|---|---|
| Ctrl+N / O / S / P / W | New / Open / Save / Print / Close | same | same |
| F12 / Ctrl+Shift+S | Save As | Save As | Save As |
| Ctrl+Z / Ctrl+Y (also Ctrl+Shift+Z) | Undo / Redo | same | same |
| Ctrl+X / C / V, Ctrl+Alt+V | Cut/Copy/Paste, Paste Special | same | same |
| Ctrl+Shift+C / Ctrl+Shift+V | Copy / Paste formatting (Format Painter) | — | Copy/Paste formatting |
| Ctrl+A | Select all | Select region / sheet | Select all objects / text |
| Ctrl+F / Ctrl+H / Ctrl+G | Find / Replace / Go To | Find / Replace / Go To (F5) | Find / Replace / — |
| Ctrl+B / I / U | Bold / Italic / Underline | same | same |
| Ctrl+Shift+D (Write: double underline) | ✓ | — | — |
| Ctrl+= / Ctrl+Shift+= | Subscript / Superscript | — | Subscript / Superscript |
| Ctrl+Shift+> / < , Ctrl+] / [ | Grow/shrink font | — | Grow/shrink font |
| Ctrl+Shift+A | All caps | — | — |
| Ctrl+Shift+L | Bullets | Toggle Filter | Bullets |
| Ctrl+E / L / R / J | Centre / Left / Right / Justify | Flash Fill(P1) / — / — / — | Centre / Left / Right / Justify |
| Ctrl+1 / 2 / 5 | Line spacing 1 / 2 / 1.5 | Format Cells (Ctrl+1) | — |
| Ctrl+M / Ctrl+Shift+M | Indent / Outdent | — | New slide (Ctrl+M) |
| Ctrl+Enter | Page break | Fill selection with entry | New slide (when in rail) |
| Ctrl+Shift+Enter | Column break | Array formula (legacy CSE) | — |
| Ctrl+K | Insert link | Insert link | Insert link |
| Ctrl+Shift+F / Ctrl+D | Font dialog | Fill down (Ctrl+D) | Duplicate (Ctrl+D) |
| Ctrl+Shift+N | Normal style | — | — |
| Ctrl+Alt+1/2/3 | Heading 1/2/3 | — | — |
| Ctrl+Space | Clear character formatting | Select column | — |
| Ctrl+Q | Clear paragraph formatting | — | — |
| Shift+F3 | Change case | — | Change case |
| F7 / Shift+F7 | Spelling / Thesaurus | Spelling | Spelling |
| Ctrl+Shift+E | Track changes | — | — |
| Ctrl+Alt+M | New comment | Shift+F2 (note) / Ctrl+Alt+M comment | Ctrl+Alt+M |
| Ctrl+Shift+8 | Show ¶ | Outline symbols | — |
| Alt+Shift+Arrows | Move paragraph / outline promote | Group/Ungroup rows | Promote/demote |
| Ctrl+F1 | Collapse/expand ribbon | same | same |
| Alt / F10 | Key Tips | same | same |
| Alt+Q, Ctrl+Shift+P ★ | Search / Command palette | same | same |
| Ctrl+Shift+V ★(Grid: Paste values) | — | Paste values only | — |
| F2 | — | **Edit cell in place** | Edit text of selected object |
| F4 | Repeat last action | **Toggle $ absolute/relative refs**; repeat action | Repeat |
| F9 / Shift+F9 | Update fields | Recalculate all / sheet | — |
| F5 / Shift+F5 | Go To | Go To | **Slide show from start / current** |
| Ctrl+` | — | Show formulas | — |
| Ctrl+; / Ctrl+Shift+; | Insert date / time | Insert date / time | — |
| Ctrl+T / Ctrl+L | — | Format as table | — |
| Ctrl+Shift+1 / 4 / 5 / % | — | Number / Currency / Percent formats | — |
| Ctrl+Shift+& / _ | — | Outline border / remove borders | — |
| Ctrl+9 / Ctrl+0 / Ctrl+Shift+9/0 | — | Hide rows / cols; unhide | — |
| Ctrl+Page Up / Page Down | — | Previous / next sheet | Previous / next slide |
| Ctrl+Arrows, Ctrl+Shift+Arrows, Ctrl+Home/End | Word jumps | Edge jumps / extend / A1 / last cell | — |
| Shift+Space / Ctrl+Space | — | Select row / column | — |
| Alt+= | — | AutoSum | — |
| Alt+Enter | — | New line in cell | — |
| Alt+Down | — | Open filter / autocomplete list | — |
| Ctrl+Shift+L | Bullets | Toggle AutoFilter | Bullets |
| Ctrl+- / Ctrl+Shift+= | — | Delete / Insert cells | — |
| Shift+F11 | — | New sheet | — |
| Ctrl+Shift+G | — | — | Group; Ctrl+Shift+H ungroup |
| Shift+Arrows (object) / Ctrl+Arrows | — | — | Resize / nudge selected shape |
| B / W / N / P / Home / End / number+Enter / Esc / Ctrl+L / Ctrl+P / Ctrl+I | — | — | In slide show: black / white / next / previous / first / last / go to / end / laser / pen / highlighter |
| Esc | Cancel / close pane | Cancel edit | Deselect / exit show |

A single source of truth, `packages/commands/src/keymaps/*.ts`, drives the editors, the Electron native menu accelerators, the Key Tips, the Options ▸ Keyboard page, and the "Shortcut cheat sheet" (Ctrl+/ ★). Playwright test `e2e/shortcuts.spec.ts` iterates the table.

### 1.8 Mobile adaptation (Android, Capacitor) — "native-feeling"

Design reference: Office Mobile and Google Docs Android, but with our chrome.

- **Breakpoints:** `< 640 px` phone layout; `640–1023 px` tablet (simplified ribbon, panes as modals); `≥ 1024` desktop layout even on tablets in landscape with keyboard.
- **Top app bar (56 dp):** back (to Hub), document title (tap to rename), Undo, Redo, Search, overflow (Share, Save as, Print/PDF, Version history, Find & Replace, Settings). Title bar colour follows the system status bar (Capacitor StatusBar plugin), dark mode follows system.
- **Bottom toolbar (48 dp) + "ribbon sheet":** a scrollable row of the most-used commands for the current context (text: B I U, colour, bullets, align, indent; cell: number format, B, fill, borders, merge, wrap; slide: new slide, layout, insert); the first button "**Home ▾**" opens a **bottom sheet** listing the full ribbon tabs as a horizontal chip row (Home / Insert / Layout / Review / View …) with groups rendered as vertically stacked tiles, exactly mirroring desktop group names so users can find things. The sheet uses the same command registry; no mobile-specific command definitions.
- **Keyboard handling:** Capacitor Keyboard plugin reports height; the bottom toolbar docks above the keyboard (`env(keyboard-inset-height)` fallback). Grid pins the Formula Bar above the keyboard with ✓/✕ buttons and a "123 / fx / A→Z" quick keys row.
- **Touch:** selection handles (teardrop) for text, cell range handles at the two corners, a floating context toolbar on selection (Copy/Cut/Paste/Comment/Format), long-press for context menu, pinch-to-zoom on canvas (Write page, Grid, Deck stage), double-tap to zoom to width. Minimum hit target 44 × 44 dp. Shape transform handles enlarge to 28 dp and show a rotation handle.
- **Write:** continuous (reflow) view by default, "Print layout" toggle; Find bar overlays the top bar; comments as a bottom sheet.
- **Grid:** sheet tabs become a bottom chip strip above the toolbar; column/row headers tap to select, drag header edge to resize; filter buttons open bottom sheets; frozen panes work.
- **Deck:** vertical slide list in portrait with a FAB "+" for new slide; tapping a slide opens full-width edit; **Present** goes fullscreen landscape with swipe navigation, tap zones, and a presenter notes toggle; the device can act as a remote for a desktop presentation over the collab server (P2).
- **Files:** open from Android file managers/share sheet via intent filters for the Office MIME types; Save uses SAF (document tree) through `@capacitor/filesystem` + `@capawesome/capacitor-file-picker`; recents stored in `@capacitor/preferences`.
- **Navigation:** Android back button closes sheets/dialogs before leaving the document; hardware keyboards get the full desktop shortcut table.

### 1.9 "Coming from Office" affordances

- First launch "Tour" (3 cards, skippable): ribbon is where you expect, Alt+Q/Ctrl+Shift+P finds any command, version history is built in.
- Every ribbon command tooltip shows the Office name when ours differs (we rarely differ).
- Options ▸ General ▸ "Classic Office look": colourful icons + solid accent title bar for people who want it.
- The command palette accepts Office terminology synonyms (`"Merge & Center"`, `"Freeze Panes"`, `"Track Changes"`, `"Format Painter"`), fuzzy matching, and shows shortcuts.

---

## 2. Feature matrix

Legend: **P0** must ship for v1.0 (Definition of Done requires it); **P1** ship in v1.x within this build if time allows (WPs include them where stated); **P2** follow-up. ★ = better-than-Office.

### 2.1 Suite-wide

| Feature | Tier |
|---|---|
| Ribbon (Classic + Simplified), Backstage, status bar, zoom slider, Key Tips, mini toolbar, context menus, dialog launchers | P0 |
| Open/save `.docx .xlsx .pptx .csv`; export PDF; native bundle format; Markdown/HTML export (Write), CSV/TSV (Grid), PNG per slide (Deck) | P0 |
| ODF `.odt .ods .odp` import/export | P1 (WP-41) |
| AutoSave + crash recovery (IndexedDB/OPFS journal; desktop recovery files) | P0 |
| ★ Command palette (Ctrl+Shift+P / Alt+Q) with fuzzy search, recent commands, Office synonyms | P0 |
| ★ Built-in version history (automatic snapshots on save + named versions, diff view for Write, restore) | P0 (Write, Deck), P1 (Grid cell-diff) |
| ★ Live collaboration (Yjs, cursors, presence, comments) via self-hostable server; offline-first merge | P1 (Write P1, Deck P1, Grid P2) |
| Light/Dark/High-contrast themes; OS theme follow | P0 |
| i18n (en, de, ar) with RTL layout and RTL text (bidi) in documents | P0 (en), P1 (de, ar) |
| Accessibility: full keyboard operation, ARIA on ribbon (toolbar/menubar pattern), screen-reader announcements for status changes, focus rings, axe-clean | P0 |
| ★ AI-ready hooks: `packages/commands` exposes an `AssistantProvider` interface (summarise, rewrite, formula from text, slide from outline) — ships with a "no provider configured" state and a sample local echo provider; no network calls by default | P1 |
| Templates (12 per app) | P0 (6 per app), P1 (12) |
| Printing with preview; print to PDF | P0 |
| Spell check (Hunspell dictionaries en-US bundled, de/ar P1) | P0 |
| Document properties, word/char/page counts | P0 |
| Recent files, pinned files, file association, open-with | P0 |
| Macros/VBA | **Never** executed; `vbaProject.bin` preserved opaquely in `.xlsm/.docm/.pptm` so files aren't damaged |

### 2.2 Write

| Area | P0 | P1 | P2 |
|---|---|---|---|
| Text | Paragraph & character styles (Word built-in names), fonts, size, colour, highlight, B/I/U/S/sub/sup, change case, clear formatting, alignment, indents, spacing, tabs, drop cap, text boxes | Text effects, columns balancing, hyphenation | WordArt |
| Lists | Bulleted/numbered/multilevel with Word numbering definitions (`w:numbering`), restart numbering, continue, legal numbering | List styles | — |
| Layout | Pages, sections, margins, orientation, size, page breaks, columns, headers/footers (first/odd/even), page numbers, footnotes/endnotes, line numbering off, **pagination that matches Word within ±1 line for standard fonts** | Watermarks, page borders | Line numbers |
| Tables | Insert/delete, merge/split, borders/shading, table styles (banded), autofit, repeat header, nested tables (read), text→table | Formula in table, sort | — |
| Objects | Inline & floating images with wrap (square/tight/top-bottom/behind/in-front), shapes (DrawingML presets), captions, charts (via charts pkg), links, bookmarks, cross-refs, symbols | Equations (OMML ↔ MathLive), icons, screenshots | SmartArt, embedded objects |
| References | TOC (styles-based, clickable, update), footnotes/endnotes, captions, cross-references | Table of figures, index | Citations/bibliography |
| Review | Comments (threaded, resolve), **Track changes** (insert/delete/format revisions, accept/reject, markup modes), word count, spelling | Compare documents | Restrict editing, password |
| View | Print layout, read mode, web layout, draft, ruler, navigation pane (headings/pages/results), zoom, split | Outline view | Immersive reader |
| ★ Extras | Focus mode, Markdown shortcuts (`# `, `- `, `1. `, `**bold**`, `` ` ``, `---`), command palette, version history, callout blocks, code blocks, smart paste (Markdown → styled), typography (smart quotes, dashes), word-count goals | Collaboration, AI hooks, "Document outline health" (heading-level checks) | Compare with diff view |

### 2.3 Grid

| Area | P0 | P1 | P2 |
|---|---|---|---|
| Grid | 1,048,576 × 16,384 addressable, virtualized canvas, freeze panes, split, hide/unhide, row/col resize/autofit, multiple sheets, tab colours, zoom, selection modes (range, row, col, multi, Ctrl-click), name box, formula bar | Custom views | Grouping/outline symbols (P1) |
| Editing | In-cell edit (F2), autocomplete of values & function names, formula autocompletion with argument tooltips **and inline docs★**, fill handle (series, copy, linear, dates), Ctrl+Enter fill, AutoSum, clipboard with formats/values/transpose, undo/redo, find/replace (values/formulas), sort (multi-key, custom), filter (values, text/number/date/colour criteria), remove duplicates, text to columns, data validation (list, number, date, text length, custom; dropdowns; input/error messages), comments & notes, hyperlinks | Flash Fill, Goal Seek, Subtotal, group/outline | Scenario manager, data table, consolidate, Power-Query-style editor |
| Formatting | Number formats (Excel format codes via `numfmt`), fonts, fills, borders (all styles), alignment, wrap, merge, indent, rotation, cell styles gallery, Format Cells dialog, conditional formatting (highlight rules, top/bottom, data bars, colour scales, icon sets, formula rules) | Themes | — |
| Formulas | ≥ 350 Excel functions (see §3.4), dynamic arrays & spill, structured references, 3-D refs, defined names (workbook/sheet scope), error values, iterative calc off, LAMBDA/LET, XLOOKUP/FILTER/SORT/UNIQUE/SEQUENCE, TEXTSPLIT, date system 1900, Excel precision rules | Locale separators, iterative calc, R1C1 | Cube/web functions, Python |
| Analysis | Charts (column/bar/line/area/pie/doughnut/scatter/bubble/combo + treemap/sunburst), Format as Table (structured refs, totals row, banded styles), sparklines | Pivot tables (sum/count/avg/min/max, rows/cols/values/filters, drill), histogram/box/waterfall charts | Slicers, timelines, forecast sheet |
| ★ Extras | Formula explainer pane (step evaluation), smart tables (auto-expand, column types), JSON import, command palette, instant stats in status bar, "Recommended charts" from selection shape, dark-mode-aware chart colours, version history | Collaboration, AI hook "formula from description" | Scripting (TypeScript macros via sandbox) |

### 2.4 Deck

| Area | P0 | P1 | P2 |
|---|---|---|---|
| Slides | Layouts/masters/themes (OOXML theme-compatible), new/duplicate/delete/reorder/hide, sections, slide size, background, notes pane, thumbnail rail, slide sorter, outline view | Reuse slides | Handout/notes masters |
| Objects | Text boxes/placeholders with autofit, shapes (≥ 60 preset geometries), lines/connectors, pictures (crop), tables, charts, grouping, z-order, align/distribute, smart guides & snapping, rotate/flip, fill/outline/effects (shadow), icons | Video/audio, equations | SmartArt, 3D, zoom links |
| Motion | Transitions (12), animations (entrance/emphasis/exit basics), animation pane, timing | Rehearse timings, morph-lite | Triggers, motion paths |
| Show | Slide show (F5/Shift+F5), **Presenter view** (second window on Electron; second tab/window on web), notes, timer, next-slide preview, laser/pen/highlighter, blackout, go-to | Rehearse timings | Recording |
| ★ Extras | Design Ideas (layout + theme variant suggestions for the current slide's content), code blocks, present in browser tab, PNG/PDF export, version history, command palette | Collaboration, mobile remote | Live Grid range embedding |

### 2.5 Notes (Tier 2, WP-33)

Notebooks → sections → pages tree; page = Write document with title/timestamp; tags (To Do, Important, Question, custom) with a Tags summary pane; quick search across notebooks; local storage in an app-managed folder; export page to .docx/.md/.pdf. OneNote import (`.one`) is **out of scope**.

---

## 2A. Competitive landscape (October 2026) and the quality bars we must beat

Researched 2026-10-15 with web search and the GitHub API (star counts are from the API on that day). Full write-up with sources is reproduced in the repo as `docs/COMPETITORS.md` (WP-03) and re-scanned every parity round (§7.3).

### 2A.1 The field

| Product | Stack / licence / size | Latest (as of Oct 2026) | What they do better than Officina v0.1 will | Obvious weaknesses (our opening) | Concrete bar we must beat |
|---|---|---|---|---|---|
| **Microsoft 365** (Word/Excel/PowerPoint desktop, web, mobile) | proprietary; subscription | Copilot **Agent Mode** became the default across Word/Excel/PowerPoint on 2026-04-22; Excel "Plan mode", Python in Excel; Startup Boost preloading (2025) | The reference for everything: fidelity by definition, 500+ Excel functions, Power Query/Pivot, Designer, Morph, co-authoring at scale, accessibility tooling, mobile apps | Subscription + account; telemetry; no Linux; AI pushed on by default; heavy (Startup Boost exists because cold start is slow; needs 8 GB RAM) | Open the same `.docx/.xlsx/.pptx` with the §3.5 fidelity scores; cold start to Hub < 2.5 s and open-document < 3 s on this 7 GB laptop; every §1.7 shortcut identical; no account, no telemetry, AI off unless configured |
| **Google Workspace** (Docs/Sheets/Slides) | proprietary web | Gemini in Docs/Sheets/Slides beta (2026-03-10: prompt-bar drafting, "Fill with Gemini", theme-matched slides) | Real-time collaboration and version history are effortless; zero install; comments/@mentions | Weak Office fidelity (import/export conversion), slow on large sheets (50k+ rows), 10M-cell cap, offline is partial, requires Google account | Collaboration with cursors + offline merge (WP-29) on a server you host; 1M-cell sheets scroll at 55+ fps; opens `.xlsx` natively without conversion |
| **LibreOffice 26.2** (Feb 2026) | C++; MPL-2.0/LGPL; `LibreOffice/core` ★4.5k mirror | Markdown import/export, faster Calc with hidden columns, BIFF12 clipboard paste from Excel, RTL alignment options, change-tracking refinements | Mature everything: 30 years of format handling, ODF reference implementation, Base, macros (BASIC/Python), hyphenation/typography, print engine | Switchers' top complaints: `.docx` style/font shifts and text-wrap pain, "looks different in Word", dated UI (ribbon is an experimental "tabbed" mode), no real-time collaboration without Collabora, large-sheet performance | Beat LibreOffice's own docx round-trip on our corpus (WP-39 measures LibreOffice's `docx→odt→docx` visual score next to ours; ours must be ≥ theirs on ≥ 80 % of files); native ribbon; built-in collaboration |
| **Collabora Online 26.04** (Jul 2026) | C++/JS on LibreOffice core; MPL; commercial support | Optional AI assistant (off by default, BYO model), colour-coded document comparison, Markdown, Calc per-user views + calculated pivot fields, collaborative presenting, **BITV 2.0 accessibility certification** | Server-grade collaboration, accessibility certification, proven at Nextcloud scale | Server-only (needs a deployment to edit at all), tile-rendering latency, LibreOffice UI underneath, no offline desktop story | Accessibility: axe 0 serious/critical on every screen **and** a WCAG 2.2 AA self-audit (docs/qa/a11y.md) with 100 % keyboard operability of ribbon commands; AI off by default with BYO provider exactly like theirs |
| **ONLYOFFICE Docs 9.4** (May 2026) | JS editors + C++ converters; AGPL-3.0; `ONLYOFFICE/DocumentServer` ★7.0k | Dark mode for spreadsheets, 25 new themes + 20 transitions, separate Chart Design tab, in-editor form recipients; 9.3: hyperlinks on images/shapes, multi-page view | Best-regarded OOXML fidelity among open suites (OOXML is its native model), tabbed ribbon-like UI, forms, DocSpace rooms, desktop + mobile editors | Licence-key friction, the Community edition is the server; desktop app is a thin client; sheet performance on big files; slow-ish startup; AI through their cloud | Fidelity ≥ ONLYOFFICE on the corpus is the long-term bar (parity loop measures ONLYOFFICE's conversion via its Docker image when available; v0.1 bar is the §3.5 thresholds); 25+ themes and 20+ transitions in Scena by round 2 |
| **Euro-Office** (ONLYOFFICE fork; IONOS, Nextcloud, XWiki, OpenProject…; first stable 2026-06-09) | HTML/JS/C++; AGPL-3.0-only | Co-editing, comments, version history, track changes, comparison, chat, forms; Windows/macOS/Linux/iOS/Android clients | Public-sector backing in the EU; embedded in Nextcloud/XWiki/OpenProject | Server-centric like ONLYOFFICE; a fork still converging; licence dispute history | Same bars as ONLYOFFICE; plus: Officina works fully offline as a single install |
| **WPS Office** (2026) | proprietary freemium | AI writing/proofing/contract review/resume assistant, PDF compare, presentation "beautification", 14 new languages | Very Office-like UI, strong PDF tools, mobile apps | Ads/upsell, telemetry concerns, closed source, ~400 MB RAM reported | Office-like UI with none of the upsell; PDF export quality; < 600 MB for a 300-page document |
| **Apple iWork** (Sep 2026) | proprietary | Siri questions about documents, request-access sharing, Pages "Screen View", new Numbers functions | Beautiful templates, Keynote animation quality (Magic Move), free on Apple devices | Apple-only, weak Office fidelity, no Windows/Linux/Android | Template quality: 36 templates reviewed to "looks finished" (WP-32); Morph-lite transition in the loop (round 2) |
| **Univer** (`dream-num/univer` ★22.6k, TypeScript, Apache-2.0; "the Office harness for AI agents") | plugin SDK: sheets (mature), docs, slides (early), canvas, formula engine, Facade API, Node runtime | Active (pushed daily); strong canvas sheet rendering, own formula engine, AI-agent focus (`dsh-univer-office`), Obsidian plugin | Excellent spreadsheet engine and renderer, agent/MCP story | It is an SDK, not an app: no ribbon/Backstage, docs and slides immature, OOXML import/export and collaboration are in paid "Pro/Server" tiers, no desktop/mobile apps | Beat on *product*: ribbon UX, file fidelity in the open core, offline desktop + Android. We benchmark our grid against Univer's demo (scroll fps, 1M cells) in WP-40 and must match it ±10 %. Its `@univerjs/engine-formula` (Apache-2.0) is our documented formula-engine fallback |
| **GenOffice** (`genspark-ai/genoffice` ★9.8k, TypeScript, Apache-2.0, public since 2026-07-31) | Electron-style desktop: Docs/Sheets/Slides/PDF/Markdown/HTML + AI panel, CLI + agent skill + MCP | Desktop builds for macOS/Windows/Linux; "byte-preserving" saves (only edited parts rewritten); AI edits land as tracked changes with rollback; full-text local file search; BYO key for 15+ providers | Clear AI-first UX, byte-preserving round-trip, agent skill/CLI, 20 README languages | AI routed through Genspark by default (account), no Android, no collaboration, UI is not Office-familiar (no ribbon), young codebase | **Byte-preservation bar:** every part we did not edit is byte-identical after save (test in WP-13/WP-18/WP-26); AI edits as tracked changes when a provider is configured (loop round 1); `officina` CLI + MCP server (loop round 1) |
| **ArtCraft "Craft" apps** — WordCraft ★13.1k, GridCraft ★5.1k, DeckCraft ★3.4k (`storytold/*`, Rust, MIT/Apache-2.0, created 2026-10-07, AI-built) | pure Rust, native + WASM web, agent-drivable over MCP/CLI/JSON control channel | Press claims: 88 % of Word ribbon, 513/523 Excel functions, 17 chart types, pivots saved to XLSX, Spanish formula names, Page Break Preview, copy-on-write undo; dark mode; "if you know Excel you know GridCraft" | Alpha ("50–60 % practically usable" by their own rating), no Android, Rust GUI toolkit a11y is weak, fidelity unproven, viral-but-young | Function count ≥ 513 (loop target; v0.1 ships ≥ 350), ≥ 17 chart types (v0.1 ships 13 → loop), localised formula names (loop), **every command drivable over a JSON control channel/MCP** (loop round 1 — our command registry already makes this cheap), and a *measured* fidelity corpus they do not have |

Also noted: CryptPad (own formats, E2EE), Workbench (GitLab, unclear status), Luckysheet (superseded by Univer), Collabora/ONLYOFFICE mobile apps.

### 2A.2 What this means for Officina — differentiators

1. **Fidelity you can measure.** No competitor publishes a reproducible round-trip score. `pnpm fidelity` and `docs/qa/fidelity.md` (per file, per metric, with LibreOffice's own score beside ours) become a public artefact and a landing-page section.
2. **Office-familiar *and* modern.** Only ONLYOFFICE/WPS offer a ribbon among the alternatives; neither has a command palette with Office synonyms, Key Tips, or a Format Cells dialog that matches Excel's six tabs. We ship all of §1.
3. **One install, everything offline, plus collaboration you host.** The web-first newcomers have no Android; the server suites cannot edit offline. Officina does both from one codebase.
4. **Agent-native without AI lock-in.** Every command is already a registry entry; exposing it over a JSON control channel + MCP server + CLI (loop round 1) matches GridCraft/GenOffice/Univer. AI providers are BYO and off by default (Collabora's stance), and AI edits arrive as tracked changes (GenOffice's bar).
5. **Byte-preserving saves** (GenOffice's strongest claim) are a design property of our OPC passthrough; we assert it in tests.
6. **Privacy and licence posture of the user's other projects:** no telemetry, no account, AGPL, fonts and icons compiled in, nothing fetched from third parties.

### 2A.3 Testable quality bars (enforced by tests named in parity rows, §7)

| Bar | v0.1 (this plan) | Loop target ("beats competitors") | Verified by |
|---|---|---|---|
| DOCX round-trip (corpus) | structural ≥ 0.95, text 1.00, visual ≥ 0.90, untouched parts byte-identical | structural ≥ 0.98, visual ≥ 0.95, ≥ LibreOffice's own score on ≥ 80 % of files, ≥ ONLYOFFICE on ≥ 50 % | `pnpm fidelity --format docx` |
| XLSX round-trip | structural ≥ 0.97, values 1.00, formulas 1.00, oracle opens 100 % | structural ≥ 0.99; pivots/sparklines/slicers preserved as models | `pnpm fidelity --format xlsx` |
| PPTX round-trip | structural ≥ 0.95, text 1.00, visual ≥ 0.85 | visual ≥ 0.92; animations preserved 100 % | `pnpm fidelity --format pptx` |
| Formula functions | ≥ 350, conformance 1,500 cases 0 failures | ≥ 520 (all Excel worksheet functions except CUBE/RTD/web), 3,000 cases | `listFunctions().length`, conformance suite |
| Chart types | 13 | ≥ 18 (add box & whisker, map-free choropleth skipped, stock, surface-as-heatmap, Pareto, funnel 3-D-as-2-D) | charts stories screenshot count |
| Performance | §3.9 table | open time ≤ 0.5× `soffice --convert-to` time for the same file; grid scroll ≥ Univer demo −10 % | `pnpm bench`, perf.spec.ts |
| Startup | Electron cold start < 2.5 s to Hub | < 1.5 s | perf.spec.ts (Electron) |
| UX polish | no overlapping/clipped UI at 1280×800, 1440×900, 412×915 (automated bounding-box overlap scan of ribbon/status bar/dialogs + visual review) | same at 1024×768 and 1920×1080 with 125 %/150 % zoom; 4 px grid lint | `e2e/specs/layout-scan.spec.ts` (WP-42) |
| Accessibility | axe 0 serious/critical on all screens; 100 % ribbon commands keyboard-reachable | WCAG 2.2 AA self-audit complete; screen-reader walkthrough script for each app (NVDA/Orca steps documented) | a11y.spec.ts, docs/qa/a11y.md |
| Shortcuts | 100 % of §1.7 table works | + Excel's full "Alt" sequences for the Home/Insert/Data tabs | shortcuts.spec.ts |
| Collaboration | 2 users, cursors, offline merge, version history | 10 concurrent editors in a test, comments with @mentions and notifications | collab.spec.ts |
| Mobile | APK with full editing; touch e2e on Pixel 7 emulation | plus iOS build in CI (macOS runner) | mobile.spec.ts, verify-apk.sh |
| Agent/automation | command registry introspectable | JSON control channel + MCP server + `officina` CLI (convert/edit/batch) | control.spec.ts (loop) |


---

---

## 3. Architecture

### 3.1 Monorepo layout (exact)

```
OfficeSuite/
├─ PLAN.md
├─ package.json                      # root scripts, packageManager pnpm@12.11.2
├─ pnpm-workspace.yaml               # packages/*, apps/*, + dependency catalog (single version per lib)
├─ turbo.json                        # build/test/lint/e2e pipelines
├─ tsconfig.base.json                # strict, ESNext, bundler resolution, paths
├─ biome.json                        # Biome 2.x: lint + format (replaces eslint+prettier)
├─ .nvmrc  (24.22.0)   .npmrc (node-linker=hoisted for electron-builder compatibility, shamefully-hoist=false)
├─ .github/workflows/ci.yml  desktop-release.yml  android.yml
├─ tools/
│  ├─ setup-node.sh  setup-android.sh  verify-env.sh  oracle-soffice.sh  make-fixtures.mjs
│  └─ release/  (electron-builder shared config, icons, notarize placeholder)
├─ docs/  adr/ (architecture decision records), ux/ (ribbon spec JSON exports), screenshots/ (reviewed e2e output)
├─ e2e/                              # Playwright project (apps/web dev server + electron smoke)
│  ├─ playwright.config.ts  fixtures/  specs/*.spec.ts  screenshots/ (gitignored except baseline/)
├─ apps/
│  ├─ web/            @officina/web            Vite 8 + React 19 SPA/PWA: routes /hub /write /grid /deck /notes /present
│  ├─ desktop/        @officina/desktop        Electron 44 via electron-vite: src/main, src/preload, builder config
│  ├─ android/        @officina/android        Capacitor 8 project (android/ Gradle project is generated, committed)
│  └─ collab-server/  @officina/collab-server  Hocuspocus (Node) with SQLite persistence; Dockerfile
└─ packages/
   ├─ tokens/         @officina/tokens         CSS vars, themes, Tailwind @theme, fonts (UI + document fonts)
   ├─ ui-kit/         @officina/ui-kit         Ribbon, Backstage, StatusBar, CommandPalette, panes, dialogs, mini toolbar, TransformBox, pickers, mobile sheet
   ├─ commands/       @officina/commands       Command registry, keymaps, Key Tips, AssistantProvider hooks
   ├─ doc-model/      @officina/doc-model      Write schema (ProseMirror), styles, numbering, sections, fonts table
   ├─ sheet-model/    @officina/sheet-model    Workbook model, sparse cell store, styles, undo log, numfmt bridge
   ├─ deck-model/     @officina/deck-model     Presentation model: masters, layouts, slides, shapes, themes
   ├─ formula/        @officina/formula        Tokenizer, parser, AST, evaluator, dependency graph, functions, docs
   ├─ ooxml/          @officina/ooxml          OPC container, XML (de)serialisation, relationships, DrawingML, theme, passthrough
   ├─ io-docx/        @officina/io-docx        docx ⇄ doc-model
   ├─ io-xlsx/        @officina/io-xlsx        xlsx ⇄ sheet-model (+ csv/tsv/json)
   ├─ io-pptx/        @officina/io-pptx        pptx ⇄ deck-model
   ├─ io-odf/         @officina/io-odf         odt/ods/odp ⇄ models (P1)
   ├─ charts/         @officina/charts         Chart model, ECharts renderer, chart ribbon groups, c:chart XML mapping
   ├─ editor-write/   @officina/editor-write   TipTap editor, pagination, header/footer, track changes, comments, panes
   ├─ editor-grid/    @officina/editor-grid    Canvas grid, selection, editing, formula bar, features (sort/filter/CF/validation…)
   ├─ editor-deck/    @officina/editor-deck    Stage, rail, shapes, transforms, slide show, presenter view, animations
   ├─ collab/         @officina/collab         Yjs providers (websocket, indexeddb), awareness, version history snapshots
   ├─ storage/        @officina/storage        File abstraction (Web FS Access / Electron / Capacitor), recents, autosave journal, recovery
   ├─ print-pdf/      @officina/print-pdf      Print layouts, print preview, PDF export pipeline (Chromium print + pdf-lib)
   ├─ i18n/           @officina/i18n           i18next setup, locales (en, de, ar), number/date locale data
   ├─ templates/      @officina/templates      Template documents (native JSON) + thumbnails
   ├─ notes/          @officina/notes          Notes app model + UI (Tier 2)
   └─ testing/        @officina/testing        Fixtures (.docx/.xlsx/.pptx corpus), oracles (soffice, exceljs, mammoth), helpers, fidelity scorer
```

Every package: `src/index.ts`, `package.json` (`"type":"module"`, `exports`, `catalog:` deps), `tsconfig.json` extending base, `vitest.config.ts`, `README.md` (one paragraph: purpose + public API). Packages are consumed from source via TS `paths` and Vite aliases (no per-package build step needed for dev; `pnpm build` runs `tsc -b` for type checking and `vite build` only in apps).

### 3.2 Runtime architecture

```
                 ┌──────────────── apps/web (React 19, routes per app) ────────────────┐
                 │  Hub │ Write │ Grid │ Deck │ Notes │ Present (slide show window)      │
                 │  uses: ui-kit (Ribbon/Backstage/StatusBar) + commands + editors      │
                 └───────────────┬─────────────────────────────┬────────────────────────┘
         platform bridge (interface `PlatformHost` in packages/storage)                  
   ┌─────────────┴──────────┐   ┌────────────┴──────────┐   ┌───────────┴────────────┐
   │ Web/PWA host           │   │ Electron host         │   │ Capacitor host         │
   │ File System Access API │   │ preload IPC: fs,      │   │ Filesystem, FilePicker │
   │ + OPFS/IndexedDB       │   │ dialogs, menu, print  │   │ Share, StatusBar, Kbd  │
   └────────────────────────┘   └───────────────────────┘   └────────────────────────┘

   Document layer (per open document):
     Write/Deck/Notes:  Y.Doc  ⇄ ProseMirror (y-prosemirror) / deck-model binding ; Y.UndoManager ; y-indexeddb persistence
     Grid:              SheetModel (sparse columnar) + CommandLog (undo) + FormulaEngine worker (comlink) ; snapshots → IndexedDB
   File layer:          io-* ⇄ ooxml (OPC) ⇄ fflate ; opaque parts preserved on the document as `passthrough: Map<partName, Uint8Array>`
```

**State management:** `zustand` stores per app for UI state (selection, panes, zoom, ribbon tab, dialogs); document state lives in the document models (Yjs or SheetModel), never duplicated in React state. React subscribes via thin hooks (`useDocSelector`) with `useSyncExternalStore`. Command execution goes through `@officina/commands` (`execute(id, args)`), which is the only mutation path from UI → model; this makes keyboard, ribbon, palette, native menus, and tests share one surface.

**Workers:** formula recalculation (Grid) and file parsing/serialisation (all apps) run in Web Workers via `comlink`; models are transferred as structured-cloneable snapshots or `SharedArrayBuffer` (where available — not required).

**Document identity & versioning:** every document has a `docId` (UUID), `versions/` (Yjs snapshots or SheetModel snapshots, gzip via fflate) in IndexedDB keyed by docId; native bundle `.scriba/.tabula/.scena` = zip with `document.json` (model), `yjs.bin` (update), `versions/*.bin`, `media/*`, `passthrough/*`.

### 3.3 Document models

- **doc-model (Write):** ProseMirror schema mirroring WordprocessingML semantics: `doc > section+ > (paragraph | table | …)`, paragraph attrs: `styleId, alignment, indent{left,right,firstLine,hanging}, spacing{before,after,line,lineRule}, numbering{numId,level}, pageBreakBefore, keepNext, tabs[], borders, shading, bidi`; marks: `bold, italic, underline{style,color}, strike, dstrike, color, highlight, fontFamily, fontSize, caps, smallCaps, vertAlign, link, comment{id}, insertion{author,date}, deletion{author,date}, formatChange, spellError`; nodes: `image(inline|anchor, wrap, position)`, `shape`, `table/tableRow/tableCell(gridSpan, vMerge, borders, shading, width)`, `footnoteRef/footnote`, `field(type, instr, result)`, `tab`, `softBreak`, `pageBreak`, `columnBreak`, `toc`, `header/footer(type)`, `equation(omml)`. Styles are a separate `StyleSheet` (paragraph/character/table/numbering styles with `basedOn` inheritance) resolved by `resolveParagraphStyle()`; theme fonts/colours resolved via ooxml theme. Default stylesheet reproduces Word's `Normal.dotm` (Normal: Aptos→Inter 12pt→11pt? **No**: we keep the file's values; our *new* documents use Calibri-metric Carlito 11 pt, 1.08 line, 8 pt after, Heading 1 = 16 pt Calibri Light→Carlito, colour `2F5496`... i.e. the Word 2016–2021 defaults, because they are what every `.docx` in the wild expects).
- **sheet-model (Grid):** `Workbook{sheets[], names[], styles: StyleTable, theme, calcProps, passthrough}`; `Sheet{id, name, cells: SparseStore, rows: RowProps[], cols: ColProps[], merges[], cf[], validations[], tables[], charts[], drawings[], views{freeze, zoom, selection}, hidden, tabColor, autoFilter}`; `SparseStore` = `Map<rowIndex, RowChunk>` where `RowChunk` keeps parallel arrays (`cols: Int32Array, values: (number|string|boolean|ErrorValue|null)[], formulas: (string|null)[], styleIdx: Int32Array`) — compact, fast iteration, O(1) random access via a per-row `Map` index built lazily. Styles deduplicated in a `StyleTable` (like `cellXfs`). Edits go through `CommandLog` (inverse ops) for undo/redo. Volatile derived data (computed values, spill ranges) in `CalcState`.
- **deck-model (Deck):** `Presentation{slideSize, theme, masters[], layouts[], slides[], sections[], notesMaster, passthrough}`; `Slide{id, layoutId, shapes[], background, transition, timing/animations[], notes, hidden}`; `Shape = TextBox | Picture | Geometry | Group | Table | Chart | Connector | Media | Placeholder`, all with `transform{x,y,w,h,rot,flipH,flipV}` in EMU (we store EMU to avoid rounding drift; UI converts to px at 96 dpi: 9525 EMU/px), `fill/line/effects`, `textBody{bodyPr, paragraphs[]}` using a *subset* of the doc-model schema (`deckText` schema) so TipTap is reused inside shapes.

### 3.4 Formula engine (own, MIT)

- **Why own:** HyperFormula 3.5.0 is GPL-3.0-only (commercial licence required otherwise); `fast-formula-parser` (MIT) is unmaintained and lacks dynamic arrays. We write `@officina/formula` and reuse **`@formulajs/formulajs` 4.6.1 (MIT)** as the implementation base for ~350 scalar functions (wrapped/adapted to our value model), replacing those with Excel-incompatible behaviour as conformance tests reveal them.
- **Pipeline:** tokenizer (Excel grammar incl. `A1`, `$A$1`, `Sheet1!A1`, `'My Sheet'!A1:B2`, `Sheet1:Sheet3!A1` 3-D, `Table1[[#Headers],[Col]]` structured refs, `A:A`, `1:1`, `#REF!`, `@` implicit intersection, `%`, `&`, comparison, range/union/intersection operators, array constants `{1,2;3,4}`) → parser (Pratt) → AST → **dependency graph** (range nodes with interval-tree lookups so `SUM(A:A)` doesn't register a million edges) → **incremental recalculation** (dirty-marking + topological order, cycle detection → `#CIRCULAR` unless iterative calc enabled) → evaluator with lazy arguments (for IF/IFS/CHOOSE/LET/LAMBDA), **dynamic arrays with spill** (`#SPILL!`), implicit intersection for legacy formulas, volatile functions (NOW, TODAY, RAND, RANDBETWEEN, OFFSET, INDIRECT) recalculated every cycle.
- **Value model:** `number | string | boolean | ErrorValue(#DIV/0!, #N/A, #NAME?, #NULL!, #NUM!, #REF!, #VALUE!, #SPILL!, #CALC!) | Array2D | Lambda | Blank`. Dates = serial numbers (1900 system including the Lotus 1900-02-29 bug), Excel's 15-significant-digit rounding on display, and Excel's "almost equal" comparison (`=0.1+0.2=0.3` → TRUE) implemented as in Excel (round to 15 sig digits before compare).
- **Functions (target ≥ 350 by WP-09):** Math/Trig (SUM, SUMIF(S), SUMPRODUCT, ROUND*, MOD, INT, TRUNC, ABS, SQRT, POWER, EXP, LN, LOG*, PI, RAND*, CEILING*, FLOOR*, MROUND, SIGN, FACT, COMBIN, GCD, LCM, SUBTOTAL, AGGREGATE, SEQUENCE, trig set…), Statistical (AVERAGE*, COUNT*, MIN/MAX*, MEDIAN, MODE*, STDEV*, VAR*, LARGE, SMALL, RANK*, PERCENTILE*, QUARTILE*, CORREL, FORECAST*, NORM.*, T.*, FREQUENCY…), Logical (IF, IFS, AND, OR, NOT, XOR, IFERROR, IFNA, SWITCH, TRUE, FALSE, LET, LAMBDA, MAP, REDUCE, SCAN, MAKEARRAY, BYROW, BYCOL), Text (CONCAT, CONCATENATE, &, TEXT, TEXTJOIN, LEFT/RIGHT/MID, LEN, FIND/SEARCH, SUBSTITUTE, REPLACE, TRIM, CLEAN, UPPER/LOWER/PROPER, VALUE, NUMBERVALUE, TEXTSPLIT, TEXTBEFORE, TEXTAFTER, REPT, CHAR/CODE/UNICHAR/UNICODE, EXACT, DOLLAR, FIXED), Date/Time (DATE, DATEVALUE, DAY/MONTH/YEAR, HOUR/MINUTE/SECOND, NOW, TODAY, WEEKDAY, WEEKNUM, ISOWEEKNUM, EOMONTH, EDATE, NETWORKDAYS(.INTL), WORKDAY(.INTL), DATEDIF, DAYS, DAYS360, YEARFRAC, TIME), Lookup (VLOOKUP, HLOOKUP, XLOOKUP, XMATCH, LOOKUP, INDEX, MATCH, OFFSET, INDIRECT, CHOOSE, ROW(S), COLUMN(S), ADDRESS, TRANSPOSE, FILTER, SORT, SORTBY, UNIQUE, TAKE, DROP, CHOOSEROWS/COLS, HSTACK/VSTACK, TOCOL/TOROW, WRAPROWS/COLS, EXPAND, HYPERLINK), Information (ISBLANK, ISNUMBER, ISTEXT, ISERROR, ISERR, ISNA, ISLOGICAL, ISFORMULA, ISEVEN/ODD, ISREF, N, NA, TYPE, CELL (subset), ERROR.TYPE, SHEET(S), INFO (subset)), Financial (PMT, IPMT, PPMT, PV, FV, NPV, XNPV, IRR, XIRR, RATE, NPER, SLN, DB, DDB, SYD, EFFECT, NOMINAL, CUMIPMT, CUMPRINC), Engineering (CONVERT, DEC2BIN… BIN2DEC…, BITAND…, COMPLEX basics, DELTA, GESTEP), Database (DSUM, DAVERAGE, DCOUNT(A), DGET, DMAX, DMIN, DPRODUCT, DSTDEV(P), DVAR(P)), Web (ENCODEURL only).
- **Function docs** (`packages/formula/src/functions/docs/*.json`: signature, argument descriptions, examples) power autocompletion tooltips, the fx dialog and the Formula Lab pane — this is a ★ feature.
- **Conformance:** `packages/formula/test/conformance/*.csv` (`formula,expected,type,notes`) with ≥ 1,500 cases: hand-curated from Excel documentation examples + LibreOffice-oracle generated (see §5.2). Differences from Excel are logged in `docs/formula-deviations.md`.

### 3.5 File format strategy & fidelity

**Principle: "never destroy what you don't understand".** All three readers build our model *and* keep every OPC part they did not fully consume (or consumed partially) as opaque bytes; writers re-emit known parts from the model and copy opaque parts (custom XML, `vbaProject.bin`, embedded OLE, thumbnails, pivot caches, printer settings, unknown extLst) with relationships fixed up. The Compatibility Checker lists dropped features before saving to Office formats.

- **ooxml package:** `OpcPackage.open(bytes)` → parts map, `[Content_Types].xml`, `_rels` traversal, typed helpers for `a:` DrawingML (shape geometry presets → SVG paths, fills, lines, effects, text body properties), `a:theme` (colour scheme, font scheme, format scheme), shared strings, units (EMU/twips/half-points/eighths), `st_` enum tables, XML writer with deterministic attribute order and namespace prefixes matching Office (so `git diff`/oracles are stable). XML parsing uses `fast-xml-parser` with `preserveOrder: true` (order matters in OOXML).
- **docx:** `word/document.xml` body: paragraphs, runs (incl. `w:rPr` inheritance from styles/defaults/theme), tables (gridSpan/vMerge/tblLook/tblStyle), sections (`w:sectPr` incl. headers/footers refs, columns, page size/margins, titlePg, evenAndOddHeaders), numbering (`w:numbering.xml` abstractNum/num/lvlOverride), styles, settings (defaultTabStop, evenAndOddHeaders, trackRevisions), footnotes/endnotes, comments (+ `commentsExtended` threads), fields (simple & complex `fldChar` → `field` node with cached result; TOC, PAGE, NUMPAGES, DATE, REF, HYPERLINK supported live, others preserved), drawings (`wp:inline`, `wp:anchor` with wrap, `pic:pic`, `wps:wsp` text boxes, `w:pict` VML fallback read-only), hyperlinks, bookmarks, revisions (`w:ins`, `w:del`, `w:rPrChange`, `w:pPrChange`), `w:sym`, `m:oMath` preserved as node (rendered via MathLive P1), document properties (`docProps/core.xml`, `app.xml`).
- **xlsx:** `workbook.xml` (sheets, definedNames, calcPr, workbookPr date1904), `styles.xml` (numFmts, fonts, fills, borders, cellXfs, cellStyles, dxfs), `sharedStrings.xml` (incl. rich text runs), `worksheets/sheetN.xml` (dimension, sheetViews incl. pane/freeze/selection, sheetFormatPr, cols, sheetData with `t=s/n/b/e/str/inlineStr`, formulas incl. shared (`t="shared"`) and array (`t="array"`) and dynamic-array metadata (`cm` attribute + `metadata.xml`), mergeCells, conditionalFormatting (+ `x14` extensions for data bars/icon sets), dataValidations (+ x14), hyperlinks, printOptions/pageMargins/pageSetup/headerFooter, rowBreaks/colBreaks, autoFilter, tableParts), `tables/tableN.xml`, `drawings/drawingN.xml` (two-cell anchors, pictures, charts, shapes), `charts/chartN.xml` (see charts), `comments` (legacy notes) + `threadedComments` + `persons`, `pivotTables/*` + `pivotCache/*` **preserved opaque** (P1 reads definitions to show the pivot as a static table), `vbaProject.bin` opaque, `externalLinks` opaque, `customXml` opaque. Dates read with the workbook date system; number formats stored as code strings and formatted via `numfmt` (MIT).
- **pptx:** `presentation.xml` (sldSz, sldIdLst, sldMasterIdLst, notesMasterIdLst, sections ext), `slideMasters/`, `slideLayouts/`, `slides/` (spTree: `p:sp` (geometry, text body with `a:p/a:r/a:fld`, lstStyle, placeholders `p:ph` type/idx inheritance master→layout→slide), `p:pic` (incl. crop `a:srcRect`), `p:graphicFrame` (tables `a:tbl`, charts, SmartArt → imported as a group of shapes via `dgm` `drawing` part if present, else placeholder), `p:grpSp`, `p:cxnSp` connectors, `p:bg`, `p:transition` (mapping table to our 12 + fallback Fade), `p:timing` (parse build list; map supported effects; **unsupported effects preserved** in passthrough and dropped only if the slide's shape set changed), `notesSlides/`, `comments`, `docProps`.
- **Charts (shared):** `c:chartSpace` subset: barChart/bar3D(as 2D), lineChart, areaChart, pieChart/doughnut, scatterChart, bubbleChart, radarChart, combos (multiple plot types), axes (cat/val/date, titles, number formats, min/max), legend, data labels, series formatting (fill/line/marker), titles, `c:ext` treemap/sunburst (cx:chart) read where possible. Series data stored as *references + cache*; charts in Grid are live; charts in Write/Deck use the embedded workbook part (`embeddings/*.xlsx`) read through io-xlsx for editing data.
- **CSV/TSV/JSON:** RFC 4180 with delimiter/quote/encoding sniffing (UTF-8 BOM, UTF-16, Windows-1252 fallback) via `papaparse`; JSON import flattens arrays of objects into a table.
- **ODF (P1, WP-41):** `content.xml`, `styles.xml`, `meta.xml`, automatic styles; mapping tables from our models (which are OOXML-shaped). Round-trip through LibreOffice as oracle.
- **PDF export:** a dedicated print layout for each app rendered by Chromium (Electron `webContents.printToPDF`, web `window.print()`, Android `WebView.createPrintDocumentAdapter` via a tiny Capacitor plugin in `apps/android` using `PrintManager`), post-processed with `pdf-lib` (metadata, outline/bookmarks from headings, page labels). Rationale: WYSIWYG pagination and print share one renderer, so what you see is what you export.
- **Fidelity testing:** §5. Target scores before release: docx ≥ 0.95 structural, text 1.00, visual ≥ 0.90 on the corpus; xlsx values 1.00, styles ≥ 0.97, formulas 1.00 (string equality after normalisation); pptx structural ≥ 0.95, visual ≥ 0.85.

### 3.6 Rendering & editors

- **Write:** TipTap 3 (`@tiptap/core`, `@tiptap/react`, `@tiptap/pm`, selected `@tiptap/extension-*`) + custom extensions for every doc-model node/mark. **Pagination layer** (`editor-write/src/pagination/`): the editor renders into a single tall contenteditable; a `PaginationPlugin` measures block boxes with `ResizeObserver` + `getClientRects` per block (and per line for splitting paragraphs across pages via `Range.getClientRects()`), computes page break positions respecting `keepNext`, `keepLines`, `pageBreakBefore`, widow/orphan (2 lines), table row `cantSplit`, and renders **page chrome as decorations/overlays** (page gaps, margins, headers/footers with live fields, footnote areas at page bottom — footnotes are rendered in a per-page footnote region and the body is shortened by their height using an iterative fit). Floating images/shapes are rendered in an overlay layer positioned relative to page/margin/paragraph anchors; text wrap is implemented with CSS `shape-outside` on float spacers injected as decorations (square/tight approximations) — this is the known hard part; acceptance is "±1 line vs Word for the fixture corpus", not pixel identity. Print layout for PDF reuses the same computed page model with `@page` CSS. Rulers (horizontal with indent/tab markers, vertical) drawn in canvas.
- **Grid:** `editor-grid` draws the visible viewport onto a `<canvas>` (device-pixel-ratio aware): grid lines, cell backgrounds, borders (with Excel's border precedence), text with number formatting, overflow into empty neighbours, wrapped text, rotation, merged cells, conditional formats (data bars/icon sets/colour scales drawn in canvas), selection/ranges/fill handle overlays on a second canvas; frozen panes = 4 viewports sharing one renderer. DOM overlays only for: in-cell editor (contenteditable with formula syntax highlighting + range colouring like Excel), autocomplete popover, comments, dropdown buttons (filter/validation), charts/images (absolutely positioned DOM via ECharts). Text measurement cached per (font, text) with a canvas `measureText` LRU. Row/column virtualisation with O(log n) offset lookup via prefix sums in a Fenwick tree for row heights. Target: 60 fps scroll on a 1M-populated-cell synthetic sheet (benchmark in WP-40).
- **Deck:** `editor-deck` renders each slide as an absolutely positioned 960×540 px stage (16:9 default; px = EMU/9525) scaled with `transform: scale()` to fit; shapes are DOM elements: geometry via inline SVG paths generated from DrawingML preset geometry definitions (`ooxml/src/drawingml/presets.ts` — the 187 preset definitions transcribed from `presetShapeDefinitions.xml`, with guides/adjust handles), text bodies via a lightweight TipTap instance mounted on focus (one live editor at a time; static HTML otherwise for performance), pictures as `<img>` with CSS crop, tables as DOM tables, charts via ECharts. Selection/transform via `ui-kit/TransformBox` (8 resize handles, rotation handle, snapping to guides/other shapes' edges/centres, smart spacing guides, Shift/Alt modifiers). Thumbnails rendered from the same component at small scale into offscreen canvases via `html-to-image`-free approach: render the DOM stage at `scale(0.2)` with `pointer-events:none` (cheap and always correct). Slide show: a dedicated `/present` route renders slides fullscreen with transitions implemented in CSS/WAAPI; presenter view talks to the show window via `BroadcastChannel`.

### 3.7 Collaboration, undo, offline (collab package)

- Write/Deck/Notes documents are `Y.Doc`s: ProseMirror ↔ `y-prosemirror` (`ySyncPlugin`, `yCursorPlugin`, `yUndoPlugin`); deck-model keeps slides/shapes in `Y.Array`/`Y.Map`, text bodies as `Y.XmlFragment`s. Undo via `Y.UndoManager` scoped to the local user (so collaborative undo only reverts your edits — better than Office).
- Persistence: `y-indexeddb` (every document, keyed by docId) → crash recovery for free, offline edits merge on reconnect.
- Transport: `@hocuspocus/provider` → `apps/collab-server` (Hocuspocus 4.7 + `@hocuspocus/extension-sqlite`); auth is a shared-secret token in v1 (documented as not production-grade; pluggable `onAuthenticate`). Sharing = "Copy link" with `?doc=<id>&server=<url>&token=`; presence avatars in the title bar; comments are Yjs maps.
- Grid: v1 collaboration is **P2**; the SheetModel's CommandLog is designed so operations are serialisable for later OT/CRDT bridging (each command carries `{id, docVersion, inverse}`).
- Version history: on every explicit save and every 10 minutes of edits, store a snapshot (`Y.snapshot` + state vector; for Grid a gzip'd model JSON) in IndexedDB `versions` store (max 200 per doc, thinning older ones); "Version History" pane lists them with author/time, supports naming, preview, restore (as new state, preserving history), and a text diff for Write (`diff-match-patch`-style word diff implemented in-house, ~150 lines).

### 3.8 Storage, autosave, recovery (storage package)

`PlatformHost` interface: `openFile(filters) → FileHandle`, `saveFile(handle, bytes)`, `saveFileAs(suggestedName, filters)`, `readRecent()`, `watchFile`, `showInFolder`, `print(options)`, `share(file)`, `getAppDataDir`. Implementations: `WebHost` (File System Access API with IndexedDB-stored handles; download fallback for Firefox), `ElectronHost` (preload-exposed `window.officina.*` IPC → `fs`, `dialog`, `shell`), `CapacitorHost` (`@capacitor/filesystem` + `@capawesome/capacitor-file-picker` + `@capacitor/share`). **Autosave** = continuous journal to IndexedDB (Yjs updates / Grid command log) + **"AutoSave to file"** toggle in the title bar (as Office) that writes the real file after 2 s idle when the document has a handle. **Recovery:** on startup, documents in IndexedDB marked `dirty && !closedCleanly` appear in a "Document Recovery" pane (left side, as Word) with Open/Save/Delete. Desktop additionally mirrors journals to `<appData>/recovery/<docId>.journal`.

### 3.9 Performance targets (verified by WP-40 benchmarks, `pnpm bench`)

| Scenario | Target |
|---|---|
| Open 10 MB `.xlsx` with 500k populated cells | < 4 s to first paint, < 8 s full calc (worker) |
| Scroll a sheet with 1M populated cells (synthetic) | ≥ 55 fps on this machine in headless Chromium timing (< 18 ms/frame avg) |
| Single-cell edit recalculation in a 100k-formula workbook | < 50 ms p95 |
| Open 300-page `.docx` (mixed text/tables/images) | < 3 s to first page, pagination complete < 6 s (progressive) |
| Typing latency in 300-page document | < 50 ms keystroke→paint p95 |
| Open 200-slide `.pptx` | < 3 s; thumbnails lazy |
| Memory, 300-page doc | < 600 MB renderer process |
| Cold start Electron | < 2.5 s to Hub |
| APK size | < 40 MB (web assets ~ 12 MB gzip'd fonts included) |

Techniques: workers for parse/calc, `requestIdleCallback` pagination in chunks, canvas grid, lazy TipTap mount in Deck, font subsetting not required (OFL fonts ~ 1 MB each; load UI font eagerly and document fonts lazily via `FontFace`).

### 3.10 Accessibility, i18n, RTL

- Ribbon uses the WAI-ARIA Tabs + Toolbar patterns (roving tabindex, arrow-key navigation, `aria-keyshortcuts`), dialogs trap focus (Radix primitives), live region announces mode changes ("Track Changes on", "Ready / Edit"), canvas grid exposes an ARIA grid shadow tree for the active cell + row/column headers (`aria-rowindex/colindex`, `aria-selected`) and announces cell content on move; Deck exposes shapes as a list with alt text; all icons have labels; colour contrast ≥ 4.5:1 for text in all themes; `prefers-reduced-motion` respected; axe-core runs in e2e for every main screen (0 serious/critical violations).
- i18n via `i18next` + `react-i18next`, ICU message format (`i18next-icu`), locale files `packages/i18n/locales/{en,de,ar}/*.json`; number/date formatting via `Intl`; function names stay English in v1 (Excel-localised function names P2); RTL: `dir="rtl"` on root flips ribbon layout; documents support paragraph `bidi` and `rtl` run properties; Grid supports right-to-left sheets (`sheetViews/@rightToLeft`).

### 3.11 Security

Documents are untrusted input: XML parsing with entity expansion disabled, zip bomb limits (max 2 GB uncompressed, 64k parts), image decoding in `<img>` only, no `eval` in formulas (own evaluator), hyperlinks open via platform `openExternal` with scheme allow-list, Electron: `contextIsolation`, `sandbox: true`, no `nodeIntegration`, CSP, IPC surface validated with `zod`.


## 4. Toolchain setup (exact commands, user-space only, no root)

All of this is scripted in `tools/setup-node.sh`, `tools/setup-android.sh` and checked by `tools/verify-env.sh` (WP-01 writes them from the commands below). Machine facts (verified 2026-10-15): Ubuntu 24.04.5, 4 CPUs, 7 GB RAM, 63 GB free, Node 18.19.1 (too old: Vite 8/Vitest 5/Capacitor 8 need ≥ 22), npm 9.2, OpenJDK 21.0.12 at `/usr/lib/jvm/java-21-openjdk-amd64`, Python 3.12.3 + pip 24, git, `gh` authenticated as `ZahakJ` (ssh), LibreOffice 24.2.7 at `/usr/bin/soffice`, `pdftoppm`, ImageMagick `convert`, `ffmpeg`, `unzip`/`zip`, `/dev/kvm` present (group `kvm`), `DISPLAY=:0` (a real desktop session exists; do not rely on it in automation), fonts Liberation/Carlito/Caladea installed system-wide. **Not** present: Rust, pnpm, Android SDK, adb, Flutter, wine, xvfb, Docker.

### 4.1 Node 24 LTS + pnpm (user-space)

```sh
mkdir -p ~/.local/opt ~/.local/bin && cd ~/.local/opt
curl -fsSLO https://nodejs.org/dist/v24.22.0/node-v24.22.0-linux-x64.tar.xz        # HTTP 200 verified
curl -fsSL https://nodejs.org/dist/v24.22.0/SHASUMS256.txt | grep linux-x64.tar.xz | sha256sum -c -
tar -xJf node-v24.22.0-linux-x64.tar.xz && ln -sfn ~/.local/opt/node-v24.22.0-linux-x64 ~/.local/opt/node
grep -q 'local/opt/node/bin' ~/.bashrc || printf '\nexport PATH="$HOME/.local/opt/node/bin:$PATH"\n' >> ~/.bashrc
grep -q 'local/opt/node/bin' ~/.profile || printf '\nexport PATH="$HOME/.local/opt/node/bin:$PATH"\n' >> ~/.profile
export PATH="$HOME/.local/opt/node/bin:$PATH"
node -v            # v24.22.0
npm install -g pnpm@12.11.2         # installs into ~/.local/opt/node/lib (writable, no root)
pnpm -v            # 12.11.2
```

Agents must `export PATH="$HOME/.local/opt/node/bin:$PATH"` at the start of every shell (shell state does not persist between tool calls). `tools/verify-env.sh` fails fast if `node -v` is not 24.x.

Root `package.json` has `"packageManager": "pnpm@12.11.2"` and `"engines": {"node": ">=24 <25"}`; `.npmrc`: `node-linker=hoisted` (electron-builder and Capacitor's Gradle copy step need a flat `node_modules`), `auto-install-peers=true`, `strict-peer-dependencies=false`, `engine-strict=true`.

### 4.2 Pinned library catalogue (versions and licences verified against registry.npmjs.org on 2026-10-15)

Declared once in `pnpm-workspace.yaml` under `catalog:` and referenced from packages as `"react": "catalog:"`. Agents never bump versions in individual packages.

| Package | Version | Licence | Role |
|---|---|---|---|
| typescript | 5.9.3 (pinned; 7.x is the new Go-based compiler and is not yet supported by all tooling) | Apache-2.0 | language |
| react, react-dom | 19.3.1 | MIT | UI |
| vite | 8.3.4 | MIT | bundler (Rolldown) |
| @vitejs/plugin-react | 6.1.2 | MIT | |
| vitest | 5.0.5 | MIT | unit tests |
| @playwright/test, playwright | 1.64.1 | Apache-2.0 | e2e, screenshots, media rendering |
| turbo | 2.11.8 | MIT | task runner |
| @biomejs/biome | 2.5.15 | MIT OR Apache-2.0 | lint + format |
| tsx | 4.23.15 | MIT | scripts |
| electron | 44.7.2 (Chromium 152, Node 24.21) | MIT | desktop |
| electron-vite | 6.0.0 | MIT | |
| electron-builder | 26.15.3 | MIT | packaging |
| electron-updater | 6.8.9 | MIT | updates from GitHub Releases |
| @capacitor/core, cli, android | 8.5.3 (cli needs Node ≥ 22) | MIT | Android |
| @capacitor/filesystem 8.1.5, app 8.1.2, share 8.0.3, keyboard 8.0.6, status-bar 8.0.4, preferences 8.0.1, haptics 8.0.2 | | MIT | |
| @capawesome/capacitor-file-picker | 8.1.0 | MIT | SAF picker |
| @tiptap/core, react, pm, starter-kit, extension-{table, image, text-align, color, highlight, font-family, text-style, underline, task-list, character-count, mention, placeholder, link, subscript, superscript, typography, list, table-of-contents, collaboration} | 3.32.0 | MIT | Write/Deck text |
| prosemirror-model 1.25.12, prosemirror-view 1.42.6, prosemirror-tables 1.8.5 | | MIT | via @tiptap/pm |
| yjs 13.6.33, y-prosemirror 1.3.7, y-indexeddb 9.0.12, y-websocket 3.1.0 | | MIT | collab/undo/offline |
| @hocuspocus/server, @hocuspocus/provider | 4.7.1 | MIT | collab server |
| zustand | 5.0.15 | MIT | UI state |
| immer | 11.1.21 | MIT | model updates (Grid) |
| comlink | 4.4.2 | Apache-2.0 | workers |
| fflate | 0.8.3 | MIT | zip (OPC) |
| fast-xml-parser | 5.11.2 | MIT | XML |
| @formulajs/formulajs | 4.6.1 | MIT | formula function base |
| numfmt | 3.2.6 | MIT | Excel number format codes |
| decimal.js | 10.6.0 | MIT | financial precision paths |
| papaparse | 5.7.0 | MIT | CSV |
| echarts | 6.1.0 | Apache-2.0 | charts |
| pdf-lib 1.17.1, @pdf-lib/fontkit 1.1.1 | | MIT | PDF post-processing |
| pdfjs-dist | 6.4.299 | Apache-2.0 | PDF preview/tests |
| i18next 26.4.6, react-i18next 17.0.18 | | MIT | i18n |
| zod | 4.6.7 | MIT | validation (IPC, files) |
| dexie | 4.4.6 | Apache-2.0 | IndexedDB (recents, versions, journals) |
| @radix-ui/react-dropdown-menu 2.2.1, react-popover 1.3.1 (+ dialog, tooltip, tabs, toolbar, select at their current versions) | | MIT | accessible primitives |
| lucide-react | 1.57.0 | ISC | icons |
| cmdk | 1.1.1 | MIT | command palette |
| react-colorful | 5.8.1 | MIT | colour picker |
| tailwindcss 4.3.3 + @tailwindcss/vite 4.3.3 | | MIT | utility CSS over tokens |
| @fontsource-variable/inter 5.3.0, @fontsource/jetbrains-mono 5.3.0 | | OFL-1.1 | UI fonts |
| mathlive | 0.111.1 | MIT | equations (P1) |
| nspell 2.1.5, dictionary-en 4.0.0 | | MIT; (MIT AND BSD) | spell check |
| dompurify | 3.4.17 | MPL-2.0 OR Apache-2.0 | HTML paste sanitising |
| vite-plugin-pwa 2.0.0, workbox-window 7.4.1 | | MIT | PWA |
| axe-core, @axe-core/playwright | 4.14.0 | MPL-2.0 | a11y tests (dev) |
| pixelmatch | 8.0.0 | ISC | visual diff (dev) |
| **Test oracles (dev only, never shipped):** exceljs 4.4.0 (MIT), mammoth 1.13.0 (BSD-2-Clause), docx 9.11.0 (MIT), pptxgenjs 4.0.1 (MIT), docx-preview 0.4.1 (Apache-2.0) | | | |
| **Rejected:** hyperformula 3.5.0 (GPL-3.0-only — admissible under AGPL but we build our own, §1.1/§3.4); xlsx (SheetJS CE on npm is stale 0.18.5 with known CVEs; the maintained CE builds are off-registry) → ExcelJS as oracle only; jszip (dual MIT/GPL; fflate is smaller/faster); fast-formula-parser (unmaintained); konva/fabric (DOM/SVG stage chosen); lexical (TipTap/ProseMirror has Yjs + tables maturity) | | | |

`tools/check-licenses.mjs` runs `pnpm licenses list --json --prod` and fails CI on any licence outside the allow-list `MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, 0BSD, MPL-2.0, OFL-1.1, CC0-1.0, Unlicense, Python-2.0, BlueOak-1.0.0, (MIT AND BSD), (MIT OR Apache-2.0), (MPL-2.0 OR Apache-2.0), (MIT OR GPL-3.0-or-later)`.

### 4.3 Playwright (Chromium headless)

```sh
cd /home/muth/OfficeSuite && pnpm exec playwright install chromium     # ~170 MB into ~/.cache/ms-playwright; no root
pnpm exec playwright install --dry-run chromium | head -5
```

Ubuntu 24.04 desktop already has Chromium's shared-library dependencies; if `playwright install-deps` would be needed it requires root — WP-01 verifies by running the smoke e2e; on failure it records the missing libraries in `docs/qa/env.md` and tries `pkexec apt-get install -y <libs>` **once**, as the only permitted root action.

Electron e2e: Playwright's `_electron.launch()` needs a display. `e2e/electron.spec.ts` runs only when `DISPLAY` is set (`test.skip(!process.env.DISPLAY)`); packaging verification never needs a display.

### 4.4 Android SDK, Gradle, JDK (user-space in `~/Android`)

```sh
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64
mkdir -p ~/Android/cmdline-tools && cd ~/Android
curl -fsSLO https://dl.google.com/android/repository/commandlinetools-linux-16111833_latest.zip   # HTTP 200 verified
mkdir -p /tmp/ct && unzip -q -o commandlinetools-linux-16111833_latest.zip -d /tmp/ct && rm -rf ~/Android/cmdline-tools/latest && mv /tmp/ct/cmdline-tools ~/Android/cmdline-tools/latest
export ANDROID_HOME="$HOME/Android" ANDROID_SDK_ROOT="$HOME/Android"
export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"
yes | sdkmanager --licenses >/dev/null
sdkmanager "platform-tools" "platforms;android-36" "build-tools;36.0.0"      # Capacitor 8: compileSdk/targetSdk 36, minSdk 24
sdkmanager --list_installed
```

Persist `JAVA_HOME`, `ANDROID_HOME`, `ANDROID_SDK_ROOT`, PATH additions in `~/.bashrc` and `~/.profile` (idempotent `grep -q` guards, as in 4.1). Gradle: Capacitor's generated project ships the **Gradle wrapper 8.14.3** (AGP 8.13.0) — `./gradlew` downloads Gradle into `~/.gradle` on first run (~130 MB); no separate Gradle install. Memory: `apps/android/android/gradle.properties` sets `org.gradle.jvmargs=-Xmx2048m -XX:MaxMetaspaceSize=512m`, `org.gradle.daemon=true`, `org.gradle.parallel=false`, `org.gradle.workers.max=2` (7 GB RAM, 4 CPUs). Build with `JAVA_HOME` exported. No emulator: the APK is verified statically (`aapt2 dump badging`, `apksigner verify`, `unzip -l`), and KVM-based emulator runs are an optional manual step documented in `docs/manual/building.md`.

Release signing keystore (WP-35): `keytool -genkeypair -v -keystore ~/.officina-keys/release.jks -alias officina -keyalg RSA -keysize 4096 -validity 10000 -storepass "$PW" -keypass "$PW" -dname "CN=Officina, O=ZahakJ"` with a random `PW` saved in `~/.officina-keys/release.env` (chmod 600, **outside the repo**), then `gh secret set ANDROID_KEYSTORE_B64 --body "$(base64 -w0 ~/.officina-keys/release.jks)"`, `gh secret set ANDROID_KEYSTORE_PASSWORD --body "$PW"` so CI and local builds sign identically (upgrades install over each other).

### 4.5 Test oracles

- **LibreOffice headless** (`tools/oracle-soffice.sh <in> <fmt> <outdir>`): `soffice -env:UserInstallation=file:///tmp/officina-lo-profile --headless --convert-to <fmt> --outdir <outdir> <in>`; the profile dir is pre-seeded with `user/registrymodifications.xcu` that sets `/org.openoffice.Office.Calc/Formula/Load/OOXMLRecalcMode = 0` and `ODFRecalcMode = 0` (always recalculate on load) so converting an `.xlsx` to `.csv` yields LibreOffice-computed values (the formula conformance oracle), and `--convert-to pdf` of `.docx/.pptx` + `pdftoppm -r 60 -png` gives reference raster pages for visual scoring.
- **Python venv** (fixture generation and independent readers): `python3 -m venv ~/.venvs/officina && ~/.venvs/officina/bin/pip install openpyxl==3.1.* python-docx==1.1.* python-pptx==1.0.*` (all MIT; venv avoids PEP 668 restrictions). Used by `tools/make-fixtures.py` to generate the synthetic corpus deterministically.
- **npm oracles** listed in 4.2.

### 4.6 GitHub repository, house style and hygiene (WP-02)

Everything below is created by WP-02 and kept current by later WPs. Style references: the user's `concord`, `astrolabe` and `abax` repositories (README, landing page, banner).

- **Create and configure:** `git config user.name ZahakJ`; `git config user.email "$(gh api user -q .id)+ZahakJ@users.noreply.github.com"` (the GitHub noreply address; never commit a personal e-mail); `gh repo create ZahakJ/officina --public --source=/home/muth/OfficeSuite --remote=origin --push --description "Open-source office suite for Linux, macOS, Windows and Android: documents, spreadsheets and slides that open and save Office files and feel like home." --homepage https://zahakj.github.io/officina/`; `gh repo edit ZahakJ/officina --add-topic office-suite --add-topic word-processor --add-topic spreadsheet --add-topic presentation --add-topic docx --add-topic xlsx --add-topic pptx --add-topic electron --add-topic capacitor --add-topic react --add-topic typescript --add-topic yjs --add-topic offline-first --add-topic android --enable-issues --enable-wiki=false --delete-branch-on-merge`; Pages: `gh api -X POST repos/ZahakJ/officina/pages -f "source[branch]=main" -f "source[path]=/docs"` (then `gh api repos/ZahakJ/officina/pages -q .html_url` = `https://zahakj.github.io/officina/`). Social preview image is uploaded manually by the user (API has no endpoint); the file is `docs/media/social-preview.png` (1280×640).
- **README.md** (house style, see `concord`): centred `<picture>` banner (dark `docs/media/banner.png`, light `docs/media/banner-light.png`, width 820); centred badge row: CI (`actions/workflows/ci.yml/badge.svg`), latest release (`img.shields.io/github/v/release/ZahakJ/officina?color=3f6fe0&label=release`), licence (`img.shields.io/badge/licence-AGPL--3.0--or--later-3f6fe0`); two or three plain factual paragraphs (what it is, what it opens, where it runs) — no hype, no emoji, British spelling ("licence", "colour") as in the references; one-line install/run block (`curl -fsSL https://github.com/ZahakJ/officina/releases/latest/download/officina-linux-x86_64.AppImage -o officina && chmod +x officina && ./officina` plus "Windows, macOS and Android builds are on the releases page, with `SHA256SUMS.txt`"); demo GIF (`docs/media/demo.gif`); a 2×2 grid of screenshots at width 400 (Write light, Grid dark, Deck light, Android phone); **Features** with bold-lead bullets grouped by section (Write / Grid / Deck / Everywhere); **Compared with Microsoft Office** bullets in the `astrolabe` manner (one bold lead each: *Opens and saves your files*, *No subscription, no account*, *Version history built in*, *Collaboration you host*, *Find any command*, *Open source*); **Building** (Node 24, pnpm, the four commands); **Documentation** table linking the manual (`https://zahakj.github.io/officina/manual/`), DESIGN.md, FORMATS.md (fidelity notes), SHORTCUTS.md, SECURITY.md, CONTRIBUTING.md, RELEASING.md, THIRD-PARTY-NOTICES.md; **Licence** section: "GNU Affero General Public License, version 3 or later. See [LICENSE](LICENSE)."
- **Files:** `LICENSE` (verbatim AGPL-3.0 text from gnu.org), `CONTRIBUTING.md` (development loop, `pnpm verify`, tests, Biome style, conventional commits, DCO sign-off), `SECURITY.md` (report via GitHub private vulnerability reporting; supported versions; known gaps: untrusted-document parsing surface, no sandboxing of the collab server), `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1), `THIRD-PARTY-NOTICES.md` (generated by `tools/check-licenses.mjs --notices`: every bundled dependency, fonts, icon sets, with licences), `.github/ISSUE_TEMPLATE/bug_report.yml` (fields: app, platform, version, file that reproduces it — with a note to strip private content, steps, expected, actual), `feature_request.yml`, `config.yml` (blank issues off; links to Discussions/manual), `.github/PULL_REQUEST_TEMPLATE.md`, `.github/CODEOWNERS` (`* @ZahakJ`), `.github/dependabot.yml` (npm weekly grouped minor/patch, github-actions weekly, gradle weekly in `/apps/android/android`), `docs/RELEASING.md` (runbook), `CHANGELOG.md` (Keep a Changelog; `Unreleased` section updated by every WP).
- **Workflows:** `ci.yml` — on push to `main` and PRs; `concurrency` cancel-in-progress; ubuntu-latest; `pnpm/action-setup@v4` (version from `packageManager`), `actions/setup-node@v4` (24, pnpm cache); steps `pnpm install --frozen-lockfile`, `pnpm lint`, `pnpm typecheck`, `pnpm test -- --coverage`, `pnpm licenses:check`, `pnpm exec playwright install chromium`, `pnpm e2e`, upload `e2e/screenshots` and `playwright-report` as artifacts (always); 30-minute timeout. `release.yml` — on `push: tags: ['v*']` and `workflow_dispatch`; jobs: `linux` (ubuntu-latest: `pnpm release:desktop --linux` → AppImage + deb), `windows` (windows-latest: NSIS x64 + portable), `macos` (macos-latest: dmg arm64 + x64, unsigned, `CSC_IDENTITY_AUTO_DISCOVERY=false`), `android` (ubuntu-latest, `actions/setup-java@v4` temurin 21, `android-actions/setup-android@v3`, decode `ANDROID_KEYSTORE_B64` if present, `pnpm release:android` → signed release APK; if the secret is absent the job signs with a throwaway key and names the file `-unsigned-key.apk`), each uploads artifacts; final `publish` job (`needs` all four) downloads artifacts, writes `SHA256SUMS.txt` over every file, and uploads everything to the GitHub Release for the tag with `softprops/action-gh-release@v2` (creates the release if it doesn't exist, appends otherwise; `generate_release_notes: true`). Root `package.json` scripts `release:desktop` and `release:android` are defined in WP-01 as `turbo run release:desktop` / `release:android` so that workflows never need editing when the shells arrive (WP-35/WP-36 implement the tasks).
- **Media:** `docs/media/icon.svg` (mark), `docs/media/banner.svg` and `banner-light.svg` (2000×600: dark rounded card `#111317` with 1 px `#262b32` border and 28 px radius; left: 300 px rounded-square app icon with a soft accent glow (`filter: blur(60px)` ellipse at 35 % opacity); right: bold Inter 800 wordmark "officina" at 170 px, a 180×6 px accent gradient underline (`#3F6FE0 → #7C5CFF`), two-line muted tagline in Inter 500 at 44 px, then the accent-coloured spec line in Inter 600 at 30 px with `·` and `•` separators; light variant swaps card `#fbfbfc`/border `#e2e5ea`/text `#121418`); `tools/render-media.mjs` (Playwright: open each SVG in Chromium, `page.screenshot` at device scale 1 → `banner.png`, `banner-light.png`, `social-preview.png` 1280×640 (banner centred on card), app icons 16–1024 px → `apps/desktop/build/icon.png` (512), `apps/web/public/icons/*.png`, Android via `@capacitor/assets`); fonts for rendering come from `@fontsource-variable/inter` so output is reproducible on CI.
- **Landing site** (`docs/index.html`, WP-02 placeholder → WP-38 full): same CSS token scheme as `concord` (`--bg --bg2 --panel --panel2 --line --fg --muted --accent --accent2 --accent-fg`, light override under `prefers-color-scheme: light`), self-hosted `docs/assets/inter.woff2` (copied from `@fontsource-variable/inter`, OFL), `docs/.nojekyll`, OpenGraph/Twitter meta with `og:image` = `https://zahakj.github.io/officina/media/social-preview.png`, data-URI SVG favicon, sticky nav (Features, Apps, Compared, Docs, GitHub), hero (banner + screenshots with a phone overlay), per-app sections (Write, Grid, Deck, Notes) each with two screenshots, "Feels like home" section showing the ribbon crop + shortcut list, comparison table vs Microsoft 365 (price, formats, offline, collaboration, version history, macros, platforms, licence), download buttons for Windows installer / macOS dmg / Linux AppImage / Android APK pointing at `releases/download/v<ver>/<file>` with a `data-version` span updated by WP-43, footer links. `docs/manual/` is plain Markdown rendered by `docs/manual/index.html` (a 60-line client-side Markdown viewer using `marked` vendored as `docs/assets/marked.min.js`, MIT) — no site generator, so Pages serves it as-is.

### 4.7 Commit and push cadence (every WP)

1. Before starting: `cd /home/muth/OfficeSuite && git pull --rebase origin main && pnpm install --frozen-lockfile` (if the lockfile changed upstream).
2. Work only inside the WP's **owned paths** (§6). Add dependencies only via `pnpm add --filter <pkg> <dep>@catalog:` (catalog entries added in `pnpm-workspace.yaml` are the one exception to the ownership rule — append-only, never reorder).
3. Finish with `pnpm verify` (= `turbo run lint typecheck test`) and the WP's own acceptance commands, all green.
4. Update `CHANGELOG.md` → `Unreleased` (one line per user-visible change) and the owned package READMEs.
5. Commit with a Conventional Commit (`feat(grid): sort and filter (WP-19)`, `fix`, `chore`, `docs`, `test`, `build`, `ci`), body listing the acceptance criteria met, and end the message with the line required by the orchestrator: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` (if the executing agent's harness mandates a different Co-Authored-By line, use the harness's line instead).
6. `git pull --rebase origin main` (on a `pnpm-lock.yaml` conflict: `git checkout --theirs pnpm-lock.yaml && pnpm install --lockfile-only && git add pnpm-lock.yaml && git rebase --continue`), then `git push origin main`.
7. Wait for CI: `gh run list --branch main --commit "$(git rev-parse HEAD)" --json databaseId -q '.[0].databaseId' | xargs gh run watch --exit-status`. If red, fix forward in the same WP (another commit) until green. A WP is not done while CI is red.
8. WP-01 is the only WP that ends with a local commit without a push (the remote does not exist until WP-02).

---

## 5. Testing & QA strategy

| Layer | Tool | Where | Gate |
|---|---|---|---|
| Static | Biome lint/format, `tsc -b` strict | `pnpm lint`, `pnpm typecheck` | CI |
| Unit | Vitest 5 (jsdom for React, node for models) | `packages/*/test/**/*.test.ts`, `pnpm test` | CI; coverage ≥ 70 % lines per package by WP-42 |
| Formula conformance | Vitest table tests over `packages/formula/test/conformance/*.csv` | ≥ 1,500 cases; generated expectations from LibreOffice oracle + curated Excel examples | 100 % pass (known deviations listed in `docs/formula-deviations.md`, each with a reason) |
| Round-trip fidelity | Vitest + oracles; `packages/testing/src/fidelity/*` | corpus in `packages/testing/fixtures/{docx,xlsx,pptx}` (synthetic generated files + 15 hand-made feature files per format + 10 real-world public-domain documents downloaded by `tools/fetch-corpus.sh` into the gitignored `fixtures/external/`) | scores per §3.5; `pnpm fidelity` prints a table and writes `docs/qa/fidelity.md` |
| E2E | Playwright 1.64 (Chromium headless; Pixel 7 and iPad emulation for mobile specs) | `e2e/specs/*.spec.ts`; `pnpm e2e` | CI; screenshots to `e2e/screenshots/` uploaded as artifacts |
| Visual review | Agents open PNGs with their image-reading tool and record findings in `docs/qa/visual-review-<WP>.md` (what was checked, defects found/fixed) | every integration gate WP | required for gates |
| Visual regression | `pixelmatch` against `e2e/screenshots/baseline/` (threshold 0.1, ≤ 0.5 % differing pixels) | gates | from WP-42 on |
| Accessibility | `@axe-core/playwright` on Hub, each app's main view, Backstage, dialogs | `e2e/specs/a11y.spec.ts` | 0 serious/critical |
| Performance | `pnpm bench` (Vitest bench for engines; Playwright timing for scroll/typing) | `bench/` | §3.9 targets; results in `docs/qa/perf.md` |
| Packaging | `pnpm release:desktop --linux` then `ls dist/*.AppImage dist/*.deb`; `dpkg-deb -I`; AppImage `--appimage-extract` and check `usr/lib/*/resources/app.asar`; Playwright Electron smoke when `DISPLAY` | WP-34, WP-42, WP-43 | artifacts exist, launch smoke passes |
| APK | `./gradlew assembleDebug` / `assembleRelease`; `aapt2 dump badging` (package `io.github.zahakj.officina`, intent filters for docx/xlsx/pptx MIME types), `apksigner verify --print-certs`, `unzip -l` contains `assets/public/index.html` | WP-35, WP-43 | checks pass |
| Licence | `tools/check-licenses.mjs` | CI | allow-list only |

**Fixture corpus details (WP-05 creates the generator; WP-39 expands):** generated per format with the Python oracles and the npm writers so expectations are known: text styles matrix, numbering (5 list types, restarts), tables (merges, nested, styles), sections (3 types, headers/footers variants), images (inline/anchored, each wrap mode), footnotes/endnotes, fields (TOC, PAGE), comments, tracked changes, RTL paragraphs; sheets: every number format category, 40 style combinations, merges, freeze panes, 300 formulas (one per function family), shared/array/dynamic formulas, conditional formats (every rule type), validations, tables, charts (each type), comments/notes, defined names, hidden rows/cols, 1904 date system, 100k-row generated sheet; slides: every placeholder type, 60 preset shapes, groups, pictures with crop, tables, charts, transitions, animation builds, notes, sections, 16:9 and 4:3.

**Fidelity score** (per file, `packages/testing/src/fidelity/score.ts`): `structural` (fraction of model nodes/attributes equal after read→write→read, ignoring ids), `text` (normalised text equality), `visual` (1 − differing-pixel-ratio between `soffice`-rendered PDFs of original vs rewritten file at 60 dpi, page by page), `oracle_opens` (LibreOffice converts the rewritten file without error). Printed as a table; thresholds in §3.5 are enforced by WP-39's test.

**Screenshot conventions:** `e2e/screenshots/<spec>/<name>-<theme>-<viewport>.png` (`light|dark`, `desktop|phone`), full-window at 1440×900 for desktop, 412×915 for phone (Pixel 7), device scale 2 for docs-media captures (WP-43) and 1 for CI.

---

## 6. Execution plan: milestones and work packages

**Scheduling rules for the orchestrator**

- WP-01 runs alone first. WP-42 (integration gate) and WP-43 (release) run alone. Every other WP may run concurrently with any other WP whose `depends_on` are satisfied — ownership sets (`owns`) are pairwise disjoint for every pair that can be concurrent; where a later WP touches files created by an earlier WP, it lists the earlier WP in `depends_on`.
- An agent may read anything but writes only inside `owns` (plus the append-only `pnpm-workspace.yaml` catalog, `pnpm-lock.yaml`, `CHANGELOG.md` Unreleased section, and its packages' `README.md`). If a change outside `owns` is unavoidable, the agent records a note in `docs/qa/handoffs.md` for the owning WP instead of editing.
- Every WP ends with the cadence of §4.7. "Gate" WPs additionally produce `docs/qa/visual-review-WP-xx.md` and must leave `pnpm e2e` green.
- Agents must not leave `TODO` stubs in exported APIs that acceptance criteria exercise; partial features are hidden behind tier flags (`packages/commands/src/tiers.ts`), not shipped broken.

| Milestone | Goal | WPs | Rounds at 2-way concurrency (approx.) |
|---|---|---|---|
| **M0 Foundation** | toolchain, repo + CI, parity tracker, design system, ribbon/commands, OOXML core, formula engine, web shell | WP-01 … WP-09 | 5 |
| **M1 Write (Scriba)** | document model, docx read/write, editor, pagination, review tools, print | WP-10 … WP-15 | 4 |
| **M2 Grid (Tabula)** | sheet model, canvas grid, xlsx, features, charts, analysis | WP-16 … WP-22 | 4 |
| **M3 Deck (Scena)** | deck model, pptx read/write, stage editor, show/presenter, motion | WP-23 … WP-28 | 4 |
| **M4 Cross-cutting** | collaboration & history, storage/recovery, a11y/i18n/RTL, templates/Hub, Notula | WP-29 … WP-33 | 3 |
| **M5 Shells & site** | Electron, Android, mobile UX, PWA, docs site | WP-34 … WP-38 | 3 |
| **M6 Quality & release** | fidelity, performance, ODF, integration gate, v0.1.0 | WP-39 … WP-43 | 4 |

Total: **43 work packages, 7 milestones, ~27 rounds** (effort estimate in §8). Detailed WP specifications follow; the JSON block at the end of this document is the machine-readable equivalent and is generated from the same source.


### WP-01 — Toolchain and monorepo scaffold

- **Milestone:** M0  ·  **Depends on:** none
- **Owns:** `package.json`, `pnpm-workspace.yaml`, `turbo.json`, `tsconfig.base.json`, `biome.json`, `.nvmrc`, `.npmrc`, `.gitignore`, `.editorconfig`, `tools/setup-node.sh`, `tools/setup-android.sh`, `tools/verify-env.sh`, `tools/check-licenses.mjs`, `tools/oracle-soffice.sh`, `apps/web/**`, `apps/desktop/package.json`, `apps/android/package.json`, `apps/collab-server/package.json`, `packages/*/package.json`, `packages/*/tsconfig.json`, `packages/*/vitest.config.ts`, `packages/*/src/index.ts`, `packages/*/README.md`, `e2e/playwright.config.ts`, `e2e/package.json`, `e2e/specs/smoke.spec.ts`, `docs/qa/env.md`, `docs/adr/0001-stack.md`
- **Task:** Run alone. Install the toolchain exactly as §4.1 and §4.3 (Node 24.22.0 into ~/.local/opt, pnpm 12.11.2, Playwright Chromium) and write the three setup scripts plus tools/verify-env.sh (checks node 24.x, pnpm, java 21, soffice, playwright chromium present; exits non-zero with a clear message otherwise). Run `tools/setup-android.sh` too (§4.4; ~1.5 GB download) so WP-35 finds the SDK ready; it must be idempotent. Create the monorepo described in §3.1: root package.json (packageManager pnpm@12.11.2, engines node >=24 <25, scripts: dev, build, lint, typecheck, test, e2e, bench, fidelity, verify (= turbo run lint typecheck test), licenses:check, licenses:notices, release:desktop (= turbo run release:desktop --), release:android (= turbo run release:android --), media:render, parity (= tsx parity/generate.ts), parity:verify (= tsx parity/verify.ts), competitors (prints docs/COMPETITORS.md date)), pnpm-workspace.yaml with `packages: [apps/*, packages/*, e2e]` and a `catalog:` containing every library in §4.2 at the pinned versions, turbo.json pipelines (build depends on ^build; test/lint/typecheck cached; e2e depends on build of apps/web; release tasks uncached), tsconfig.base.json (strict, ES2023 target, bundler resolution, jsx react-jsx, paths `@officina/*` -> packages/*/src), biome.json (recommended rules, 2-space, single quotes, 110 cols, organise imports), .npmrc (node-linker=hoisted, auto-install-peers=true, engine-strict=true), .nvmrc (24.22.0), .gitignore (node_modules, dist, out, .turbo, e2e/screenshots/** except baseline/, fixtures/external, android build dirs, *.local, .env). Create every package under packages/ listed in §3.1 (tokens, ui-kit, commands, doc-model, sheet-model, deck-model, formula, ooxml, io-docx, io-xlsx, io-pptx, io-odf, charts, editor-write, editor-grid, editor-deck, collab, storage, print-pdf, i18n, templates, notes, testing) each with package.json (name @officina/<pkg>, type module, exports ./src/index.ts, deps from catalog as listed for that package in §4.2 where obvious, scripts lint/typecheck/test), tsconfig.json extending base, vitest.config.ts (environment node, or jsdom for ui-kit/editor-*), src/index.ts exporting a `PACKAGE_NAME` const, one trivial test, and a README.md with the package purpose from §3.1. Create apps/web as a Vite 8 + React 19 + @vitejs/plugin-react 6 + @tailwindcss/vite app with index.html, src/main.tsx rendering a placeholder Hub page that shows the text "Officina" and the suite version, a vite.config.ts with the `@officina/*` aliases, and package.json scripts dev/build/preview/test. Create apps/desktop, apps/android, apps/collab-server with package.json only (name, private, placeholder `release:desktop`/`release:android` scripts that print "not implemented" and exit 1 — WP-35/WP-36 replace them). Create e2e/ as a Playwright project (playwright.config.ts starting `pnpm --filter @officina/web dev` on port 5173, Chromium only, screenshot on failure, outputDir e2e/screenshots) with smoke.spec.ts that loads / and screenshots `smoke/hub-light-desktop.png` asserting the text "Officina". tools/check-licenses.mjs implements the allow-list of §4.2 over `pnpm licenses list --json --prod` and a `--notices` mode that writes THIRD-PARTY-NOTICES.md content to stdout. tools/oracle-soffice.sh per §4.5 (creates the profile with the recalc settings on first run). Write docs/adr/0001-stack.md summarising the stack decisions of §0/§3 with the licence rationale, and docs/qa/env.md with the verified environment facts. `git init` with branch main and make the initial commit (no remote yet). 
Ends with a local commit only (WP-02 creates the remote).
- **Acceptance criteria:**
  - export PATH="$HOME/.local/opt/node/bin:$PATH"; node -v prints v24.22.0 and pnpm -v prints 12.11.2
  - bash tools/verify-env.sh exits 0
  - ls ~/Android/cmdline-tools/latest/bin/sdkmanager ~/Android/platforms/android-36 ~/Android/build-tools/36.0.0 succeeds
  - pnpm install --frozen-lockfile=false succeeds and pnpm-lock.yaml exists
  - pnpm verify exits 0 (lint, typecheck, test across all packages; each package has >= 1 passing test)
  - pnpm --filter @officina/web build produces apps/web/dist/index.html
  - pnpm e2e exits 0 and e2e/screenshots/smoke/hub-light-desktop.png exists
  - node tools/check-licenses.mjs exits 0
  - git log --oneline | wc -l >= 1 on branch main

### WP-02 — GitHub repository, hygiene files, CI/release workflows, brand media, Pages placeholder

- **Milestone:** M0  ·  **Depends on:** WP-01
- **Owns:** `.github/**`, `README.md`, `LICENSE`, `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, `THIRD-PARTY-NOTICES.md`, `CHANGELOG.md`, `docs/index.html`, `docs/.nojekyll`, `docs/assets/**`, `docs/media/icon.svg`, `docs/media/banner.svg`, `docs/media/banner-light.svg`, `docs/media/*.png`, `docs/manual/README.md`, `docs/manual/index.html`, `docs/RELEASING.md`, `docs/DESIGN.md`, `docs/FORMATS.md`, `docs/SHORTCUTS.md`, `tools/render-media.mjs`, `apps/desktop/build/icon.png`, `apps/web/public/icons/**`
- **Task:** Create the public repository and the house-style hygiene exactly as §4.6: git identity (noreply address), `gh repo create ZahakJ/officina --public --source=. --remote=origin --push` with description and homepage, topics, issues on, wiki off, delete-branch-on-merge; enable GitHub Pages from /docs on main via `gh api -X POST repos/ZahakJ/officina/pages`. Write LICENSE (AGPL-3.0 verbatim), README.md in the concord/astrolabe style (banner picture, badges with colour 3f6fe0, factual paragraphs, install one-liner that will be valid once v0.1.0 exists, placeholder screenshot grid referencing the paths WP-43 will fill, Features by section, Compared with Microsoft Office bullets, Building, Documentation table, Licence) — mark nothing as "coming soon"; describe the suite as it will be at v0.1.0 and keep the text truthful by WP-43's final edit. CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md (Contributor Covenant 2.1), THIRD-PARTY-NOTICES.md (from `node tools/check-licenses.mjs --notices` plus fonts/icons sections), CHANGELOG.md (Keep a Changelog, Unreleased), docs/DESIGN.md (condensed §3), docs/FORMATS.md (condensed §3.5 fidelity notes), docs/SHORTCUTS.md (the §1.7 table), docs/RELEASING.md (tag -> release.yml -> assets -> Pages; local Linux/APK build and upload commands). Issue templates (bug_report.yml, feature_request.yml, config.yml), PULL_REQUEST_TEMPLATE.md, CODEOWNERS, dependabot.yml. Workflows ci.yml and release.yml exactly as §4.6 (release.yml calls the root `release:desktop`/`release:android` scripts; android job tolerates a missing keystore secret). Brand media: docs/media/icon.svg (the qalam "d" mark, §1.1), banner.svg and banner-light.svg at 2000x600 following the §4.6 spec (reference: the concord banner is 2000x600; match its composition), and tools/render-media.mjs (Playwright Chromium screenshots of the SVGs, Inter from @fontsource-variable/inter inlined as a data URI in the SVG <style> so rendering is deterministic) that writes docs/media/banner.png, banner-light.png, social-preview.png (1280x640), icon PNGs (16, 32, 48, 64, 128, 192, 256, 512, 1024) into apps/web/public/icons/ and apps/desktop/build/icon.png (512). Landing placeholder docs/index.html with the full CSS token scheme and structure of §4.6 (nav, hero with banner, sections with headings and placeholder copy, comparison table, download buttons disabled with "first release coming" text, footer), docs/assets/inter.woff2 copied from node_modules/@fontsource-variable/inter/files/inter-latin-wght-normal.woff2, docs/manual/index.html (client-side Markdown viewer loading docs/manual/*.md via `marked`, vendored to docs/assets/marked.min.js with its MIT notice) and docs/manual/README.md (table of contents of the manual pages later WPs write). Push and watch CI until green.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - gh repo view ZahakJ/officina --json isPrivate,homepageUrl,repositoryTopics -q '.isPrivate' prints false and homepageUrl is https://zahakj.github.io/officina/
  - gh api repos/ZahakJ/officina/pages -q .html_url prints https://zahakj.github.io/officina/
  - git remote get-url origin contains ZahakJ/officina and git status shows the branch is up to date with origin/main
  - gh run list --workflow ci.yml --limit 1 --json conclusion -q '.[0].conclusion' prints success
  - test -f LICENSE && head -1 LICENSE matches 'GNU AFFERO GENERAL PUBLIC LICENSE'
  - for f in README.md CONTRIBUTING.md SECURITY.md CODE_OF_CONDUCT.md THIRD-PARTY-NOTICES.md CHANGELOG.md .github/workflows/ci.yml .github/workflows/release.yml .github/ISSUE_TEMPLATE/bug_report.yml .github/ISSUE_TEMPLATE/feature_request.yml .github/ISSUE_TEMPLATE/config.yml .github/PULL_REQUEST_TEMPLATE.md .github/CODEOWNERS .github/dependabot.yml docs/index.html docs/.nojekyll docs/RELEASING.md; do test -f $f; done exits 0
  - pnpm media:render exits 0 and `identify docs/media/banner.png` reports 2000x600, docs/media/banner-light.png 2000x600, docs/media/social-preview.png 1280x640, apps/desktop/build/icon.png 512x512
  - curl -s -o /dev/null -w '%{http_code}' https://zahakj.github.io/officina/ prints 200 (allow up to 10 minutes after the push for Pages to build; retry)
  - grep -c 'img.shields.io/badge/licence-AGPL--3.0--or--later-3f6fe0' README.md >= 1

### WP-03 — Parity tracker seed (parity.json, PARITY.md generator, verify runner) and competitor dossier

- **Milestone:** M0  ·  **Depends on:** WP-02
- **Owns:** `parity/**`, `PARITY.md`, `docs/parity-badge.json`, `docs/COMPETITORS.md`, `docs/qa/would-miss-evidence.md`
- **Task:** Create the parity tracker of §7.2 and seed it from this plan. parity/schema.ts (zod schema for rows, functions sidecar, corpus manifest; exported types), parity/parity.json with >= 530 rows covering: every ribbon command of §1.4 for Write (>= 160 rows), Grid (>= 180 rows; function *families* as rows with parity/functions.json listing every Excel worksheet function name (>= 520, from the official Excel function list by category: Compatibility, Cube (out), Database, Date and time, Engineering, Financial, Information, Logical, Lookup and reference, Math and trigonometry, Statistical, Text, Web, plus the dynamic-array/LAMBDA family) each marked planned|done|out with the WP that delivers it), Deck (>= 120 rows incl. every transition/animation/chart type/layout of §1.4), Notes (>= 30), Suite (>= 40) and explicit `out` rows for the niche scope decisions of §7.1 (VBA, Access, OLE, IRM, 3-D models, legacy binary formats, Power Pivot/DAX, macro recorder, WordArt effects, ink). Every row gets: stable dotted id, app, area, feature, the Office name/location, would-miss tier (assign from the evidence table you write in docs/qa/would-miss-evidence.md: cite §2A's switcher-complaint findings and add >= 10 further sources found with web search — LibreOffice/ONLYOFFICE/Google Sheets switching threads, "what I miss from Excel/Word" discussions, Office feature usage write-ups; each tier assignment must point at a reason), priority (P0/P1/P2/out consistent with §2 and the tier rules of §7.1), status `missing` (nothing is done yet — do not pre-mark), the WP from §6 that delivers it (null for loop candidates), and verify ids using the naming scheme of §7.2 that match the e2e/unit/conformance test names the §6 WPs are instructed to create (coordinate names with the WP texts: e.g. `e2e:write-features:format-painter`). parity/corpus.json listing every fixture file under packages/testing/fixtures with expected checks per §7.2 (the generator of WP-06 creates the files; list what it will create — names are fixed in §5). parity/generate.ts renders PARITY.md (per-app tables grouped by area, status glyphs, overall/per-app/per-tier scores with the §7.2 weights, the better-than-Office list, status flips since last tag via git) and docs/parity-badge.json (shields endpoint: label "parity", message "<n> %", color by band). parity/verify.ts: resolves verify ids to runners (`unit:` → vitest run with -t/path filter in the package, `e2e:` → playwright --grep on the spec, `conformance:` → checks the CSV row result file produced by the formula test run, `fidelity:` → reads docs/qa/fidelity.json produced by pnpm fidelity, `manual:` → checks the anchor exists and the row is niche) and fails on any done/better row whose verification is missing or failing; also fails if PARITY.md is stale (regenerate and diff) or ids are duplicated or a row's wp does not exist in PLAN.md's JSON block. Add a CI step in ci.yml? No — ci.yml is owned by WP-02: add the step by writing it into docs/qa/handoffs.md as a request AND, because WP-02 is complete, ask the orchestrator; instead make `pnpm verify` (root script, owned by WP-01 and already defined as turbo run lint typecheck test) unaffected and have parity validation run inside packages/testing's test suite (a test imports parity/schema.ts and validates parity.json + staleness), which CI already runs. docs/COMPETITORS.md: the §2A table rewritten as a dated document with the source URLs from the 2026-10-15 research (re-run the searches to capture URLs: GitHub API for star counts, release notes for ONLYOFFICE 9.4, LibreOffice 26.2, Collabora 26.04, Microsoft Agent Mode GA 2026-04-22, Google Gemini Workspace 2026-03-10, GenOffice, ArtCraft WordCraft/GridCraft/DeckCraft, Univer, Euro-Office), with a "bars" section mirroring §2A.3 and a re-scan checklist for the parity loop. README gets the parity badge only in WP-44 (do not edit README here).
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm parity exits 0 and regenerates PARITY.md; `node -e` count of rows in parity/parity.json >= 530 with >= 120 critical, >= 200 commonly-missed; parity/functions.json lists >= 520 function names
  - pnpm parity:verify exits 0 (all rows are missing, so nothing to verify) and exits 1 when a test row is flipped to done without a verify id (demonstrated in a unit test in packages/testing or parity/)
  - pnpm --filter @officina/testing test exits 0 including the parity schema/staleness test
  - docs/COMPETITORS.md exists with >= 10 competitor entries each with a dated source URL; docs/qa/would-miss-evidence.md cites >= 15 sources
  - Every `wp` referenced in parity.json is an id present in this PLAN.md JSON block (checked by parity/verify.ts)

### WP-04 — Design tokens and UI primitives

- **Milestone:** M0  ·  **Depends on:** WP-01
- **Owns:** `packages/tokens/**`, `packages/ui-kit/src/primitives/**`, `packages/ui-kit/src/icons/**`, `packages/ui-kit/src/theme/**`, `packages/ui-kit/src/index.ts`, `packages/ui-kit/test/**`, `packages/ui-kit/stories/**`
- **Task:** Implement §1.2. packages/tokens: `tokens.css` defining every --of-* custom property for light, dark and high-contrast (`[data-theme=light|dark|hc]` on <html>, default follows prefers-color-scheme), app accent switching via `[data-app=write|grid|deck|notes|hub]` setting --of-accent, spacing/radius/shadow/motion tokens, `@theme` block for Tailwind 4, a `tokens.ts` export with the same values typed, font-face registration for Inter Variable + JetBrains Mono (from fontsource, bundled) and the document fonts (Liberation Sans/Serif/Mono, Carlito, Caladea: download the OFL TTF/WOFF2 files from their upstream repositories with a `tools`-free script `packages/tokens/scripts/fetch-fonts.mjs` into packages/tokens/fonts/ and commit them; ~5 MB total; add their licences to THIRD-PARTY-NOTICES via the notices generator's extra-assets list in packages/tokens/fonts/NOTICES.json), plus `applyTheme()` / `useTheme()` helpers. packages/ui-kit/src/primitives: Button (variants: default, primary, ghost, toolbar, split), IconButton, ToggleButton, Menu/MenuItem/Submenu (Radix dropdown), Popover, Tooltip (with shortcut rendering), Dialog (modal, focus trap, Esc), Tabs, Select, Combobox (searchable, used for font family/size), NumberInput (with units pt/px/cm/in), ColorPicker (theme colours grid 10x6 like Office + standard colours + recent + custom via react-colorful), Slider (zoom), Toolbar (roving tabindex), SplitButton, Gallery (Office-style scrolling gallery with expand), Separator, Kbd, Toast, Spinner, VisuallyHidden, ScrollArea. All keyboard accessible with ARIA per §3.10. Icons: `Icon` component wrapping lucide-react plus the custom Officina Glyphs set (≥ 120 SVGs listed in §1.2, 20x20 viewBox, currentColor; create them as simple, clean strokes consistent with Lucide) exported by name from `packages/ui-kit/src/icons/glyphs/index.ts` with a generated `glyphs.test.ts` asserting every name referenced by §1.4 ribbon specs exists (write the list in `packages/ui-kit/src/icons/required-glyphs.ts`). Theme: `ThemeProvider` reading/persisting the user's choice to localStorage. Add a `stories/` folder with a single `Gallery.tsx` React page that renders every primitive in all three themes (used for screenshots, not Storybook) and export it; add a Vitest+jsdom test per primitive for render + keyboard behaviour.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/tokens --filter @officina/ui-kit test exits 0 with >= 25 tests passing
  - test -f packages/tokens/fonts/Carlito-Regular.ttf (or .woff2) and Liberation*/Caladea* files exist with NOTICES.json
  - grep -c -- '--of-accent-write' packages/tokens/tokens.css >= 2 (light and dark definitions)
  - node -e "import('./packages/ui-kit/src/icons/glyphs/index.ts')" via `pnpm --filter @officina/ui-kit exec tsx -e` lists >= 120 glyph exports
  - pnpm typecheck exits 0

### WP-05 — OOXML core (OPC container, XML, DrawingML, themes, passthrough) and fixture corpus generator

- **Milestone:** M0  ·  **Depends on:** WP-01
- **Owns:** `packages/ooxml/**`, `packages/testing/**`, `tools/make-fixtures.py`, `tools/fetch-corpus.sh`
- **Task:** Implement §3.5's shared layer in packages/ooxml: `OpcPackage` (open from Uint8Array via fflate with zip-bomb limits; parts map; [Content_Types].xml defaults/overrides; relationships per part with typed lookup by relationship type; `getPart`, `setPart`, `removePart`, `addPart(name, contentType, bytes)`, `nextPartName('/word/media/image{n}.png')`, `save()` producing a zip with Office-compatible ordering ([Content_Types].xml first, then _rels/.rels)), XML layer over fast-xml-parser with preserveOrder (`parseXml`, `buildXml` with a stable namespace prefix table for w, a, p, r, c, x14, mc, wp, pic, wps, m, xdr, dgm, vt, cp, dc, dcterms; `xmlns` declarations emitted only when used; `mc:Ignorable` preserved), a tiny typed query helper (`child(node,'w:pPr')`, `children`, `attr`, `text`) and node builders, units (EMU, twips, half-points, eighths-of-a-point, percentages, `ST_` enums as string unions), DrawingML: `a:theme` parser/serialiser (clrScheme incl. system colours, fontScheme with major/minor latin/ea/cs, fmtScheme), colour resolution (`a:srgbClr`, `a:schemeClr` with lumMod/lumOff/tint/shade/alpha transforms, `a:sysClr`, `a:prstClr`), fills (solid, gradient linear/path, pattern, picture/blip with stretch/tile, none), lines (width, dash, caps, joins, head/tail ends), effects (outer shadow, glow, reflection → stored; shadow rendered), `a:bodyPr` and `a:lstStyle`/`a:pPr`/`a:rPr` text properties, and **preset geometries**: transcribe all 187 presets from ECMA-376 presetShapeDefinitions (guides, adjust values, paths with formula evaluation) into `src/drawingml/presets/*.ts` with `presetToSvgPath(preset, w, h, adjustments)`; test ≥ 40 presets against hand-computed path strings and all 187 for "produces a non-empty closed path without NaN". Passthrough: `PassthroughSet` that records parts and relationships a reader did not consume, re-emitted by writers. Document properties: core/app/custom properties read/write. packages/testing: fixtures directory layout (fixtures/docx, xlsx, pptx, each with `features/` generated and `manual/` hand-authored), `tools/make-fixtures.py` (Python venv per §4.5) generating the synthetic corpus described in §5 deterministically (fixed seeds, fixed dates) and `tools/fetch-corpus.sh` downloading 10 public-domain real-world files per format (e.g. from the ECMA-376 samples, Apache POI test data, LibreOffice unit-test corpora — list exact URLs; skip files that fail to download) into fixtures/external (gitignored). Oracles in packages/testing/src/oracles: `soffice.ts` (wraps tools/oracle-soffice.sh; convert to pdf/csv/docx/xlsx/pptx; `renderPdfPages(pdf) -> PNG buffers` via pdftoppm), `exceljsReader.ts`, `mammothReader.ts`, `pythonReader.ts` (runs small Python scripts with openpyxl/python-docx/python-pptx to dump JSON). Fidelity scorer skeleton `src/fidelity/score.ts` per §5 with `structural`, `text`, `visual` (pixelmatch), `oracle_opens`. Helper `loadFixture(name)`.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/ooxml test exits 0 with >= 60 tests (OPC round-trip of a generated docx/xlsx/pptx yields byte-identical part contents for untouched parts; theme colour resolution cases; 187 preset geometry tests)
  - python3 -I tools/make-fixtures.py --out packages/testing/fixtures exits 0 and creates >= 15 .docx, >= 15 .xlsx, >= 15 .pptx files under features/
  - bash tools/oracle-soffice.sh packages/testing/fixtures/docx/features/styles.docx pdf /tmp/officina-oracle exits 0 and the PDF exists
  - pnpm --filter @officina/testing test exits 0 (oracle wrappers smoke-tested against generated fixtures; scorer unit tests)
  - LibreOffice opens every generated fixture: a test converts each features/* file to pdf without error

### WP-06 — Command registry, keymaps, Ribbon, Backstage, status bar, command palette, panes

- **Milestone:** M0  ·  **Depends on:** WP-04
- **Owns:** `packages/commands/**`, `packages/ui-kit/src/ribbon/**`, `packages/ui-kit/src/backstage/**`, `packages/ui-kit/src/statusbar/**`, `packages/ui-kit/src/palette/**`, `packages/ui-kit/src/panes/**`, `packages/ui-kit/src/minitoolbar/**`, `packages/ui-kit/src/contextmenu/**`, `packages/ui-kit/src/keytips/**`, `packages/ui-kit/src/chrome/**`, `packages/ui-kit/src/index.ts`
- **Task:** packages/commands: `CommandRegistry` (`register({id, title, officeName?, synonyms[], icon, shortcut?, when?, run(ctx,args), isEnabled(ctx), isChecked?(ctx)})`, `execute(id,args)`, `getState(id)`, change subscriptions), `Keymap` (platform-aware parsing of "Mod+Shift+L", Mac ⌘ mapping, chord-free; conflict detection; user overrides persisted), the full §1.7 shortcut table as `keymaps/{write,grid,deck,common}.ts` mapping to command ids (commands that editors implement later are declared here as ids with titles so the palette/keytips/menus are complete from day one; unimplemented ids resolve to a no-op flagged `pending` and are hidden in release builds by `tiers.ts`), `KeyTips` sequences (Alt → letters per §1.3), `AssistantProvider` interface (§2.1) with a `NullProvider` and `EchoProvider`, tier flags (`tiers.ts`: P0/P1/P2 with env/build-time gating), telemetry-free `recentCommands` store, and `createOfficeSynonyms()` seed. packages/ui-kit chrome: `AppFrame` (title bar with mark, title rename, AutoSave toggle, undo/redo, Search/Tell-me box = palette trigger, presence slot, share button, window controls slot), `Ribbon` (data-driven from a `RibbonSpec` {tabs:[{id,label,keyTip,groups:[{id,label,launcher?,items:[...]}]}]}, item kinds: button, toggle, split, dropdown, gallery, combobox, color, numberinput, spacer, custom; Classic 3-row and Simplified 1-row layouts chosen by width; collapse (Ctrl+F1); contextual tab sets appearing with a coloured header; overflow handling; keyboard Tabs+Toolbar ARIA; tooltip shows title, shortcut, Office name when different), `Backstage` (full-screen overlay; the §1.5 navigation; content panes are slots so apps plug in Info/Print/Export), `StatusBar` (left items/right items, zoom slider with ± and percentage popover with presets, view switcher), `CommandPalette` (cmdk; fuzzy over title+officeName+synonyms; sections Recent/All; shows shortcuts; Ctrl+Shift+P and Alt+Q), `TaskPane` host (right/left docking, resizable, multiple panes tabbed), `MiniToolbar` (appears above selection on mouse-up), `ContextMenu`, `KeyTips` overlay, `DialogLauncher`, `Options` dialog shell with the §1.5 sections (General, Display, Save, Language, Keyboard with live remapping). Provide `RibbonSpec` builders and a `validateRibbonSpec()` test helper that checks every item's command id exists in the registry and every icon exists. Add a demo page `packages/ui-kit/stories/Chrome.tsx` wiring a sample spec. Tests: registry/keymap/keytips unit tests; jsdom tests for ribbon keyboard navigation, palette filtering, backstage open/close.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/commands --filter @officina/ui-kit test exits 0 with >= 60 tests
  - A test asserts that every shortcut row of docs/SHORTCUTS.md (parsed from the markdown table in the repo) maps to a declared command id in packages/commands/src/keymaps
  - pnpm typecheck exits 0
  - Visual check: a jsdom-free Playwright script `pnpm --filter @officina/ui-kit exec playwright test` is NOT required here; instead `pnpm -w e2e` still passes

### WP-07 — Formula engine core: tokenizer, parser, references, dependency graph, incremental evaluator

- **Milestone:** M0  ·  **Depends on:** WP-01
- **Owns:** `packages/formula/src/engine/**`, `packages/formula/src/parser/**`, `packages/formula/src/values/**`, `packages/formula/src/index.ts`, `packages/formula/test/engine/**`, `packages/formula/test/parser/**`, `packages/formula/bench/**`
- **Task:** Implement §3.4 except the bulk function library. Tokenizer and Pratt parser for the full Excel grammar (A1/R1C1 parse-only for R1C1 in this WP, absolute/mixed refs, sheet-qualified and quoted sheet names, 3-D ranges, whole rows/columns, structured references with [#All]/[#Headers]/[#Data]/[#Totals]/[@Col]/[@] and column ranges, defined names, array constants, operators with Excel precedence (including unary minus before %, and `^` left-assoc as Excel), string/number/boolean/error literals, whitespace-as-intersection, union with comma inside parentheses, implicit intersection `@`, function calls with empty args). AST with source spans (for syntax highlighting + the Formula Lab). Reference canonicalisation and shifting (`moveFormula`, insert/delete rows/cols adjusting refs, copy with relative adjustment, `#REF!` on deletion), A1<->R1C1 conversion, absolute toggling (F4 cycle). Value model with ErrorValue, Array2D, Blank, Lambda; coercion rules (text->number per Excel incl. percentages, dates in text in en-US, booleans), comparison rules (text case-insensitive, type ordering number<text<boolean), Excel-style 15-significant-digit normalisation, date serial helpers (1900 with the Lotus bug, 1904 option). Evaluator with lazy arguments (annotate function descriptors with `lazy: [indices]`), dynamic arrays (spill area computation, `#SPILL!` on collisions, `#` spill-range operator), implicit intersection for legacy contexts, array lifting for scalar functions over arrays, volatile marking, LET/LAMBDA/MAP/REDUCE/SCAN/BYROW/BYCOL/MAKEARRAY semantics (these core functions are implemented here; the rest come in WP-09). `DependencyGraph`: cell and range nodes, interval tree per sheet so a range precedent is one edge, dirty propagation, Kahn topological recalculation of dirty subgraph, cycle detection with `#CIRCULAR` (represented as Excel's 0 + circular-reference flag), iterative calculation option (max iterations, max change). `Engine` API: `setCell(sheet,row,col, {formula?|value?})`, `insertRows/Cols`, `deleteRows/Cols`, `renameSheet`, `defineName`, `recalc()` (incremental) and `recalcAll()`, `getValue`, `getSpillRange`, `explain(formula) -> step tree` (for Formula Lab), host interface for reading cell values/types so the engine does not own storage. A built-in minimal function set of ~40 (SUM, AVERAGE, COUNT, COUNTA, MIN, MAX, IF, IFS, AND, OR, NOT, IFERROR, ROUND, ABS, INT, MOD, LEN, LEFT, RIGHT, MID, CONCAT, TEXTJOIN, VLOOKUP, XLOOKUP, INDEX, MATCH, ROW, COLUMN, OFFSET, INDIRECT, TODAY, NOW, DATE, SEQUENCE, FILTER, SORT, UNIQUE, TRANSPOSE, LET, LAMBDA) implemented natively with a `FunctionDescriptor` {name, minArgs, maxArgs, lazy, volatile, impl, docs} registry that WP-09 fills. Property-style tests for parse->print round-trips, reference shifting, and recalculation order; a micro-benchmark (vitest bench) for 100k dependent cells.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/formula test exits 0 with >= 200 tests
  - A test file test/engine/spill.test.ts covers dynamic-array spill, #SPILL!, and the # operator
  - A test asserts parse(print(ast)) round-trips for >= 300 formulas (including structured refs, 3-D refs, array constants, quoted sheet names)
  - pnpm --filter @officina/formula bench reports recalc of a 100k-cell dependent chain under 1.5 s
  - pnpm typecheck exits 0

### WP-08 — Web app shell: routing, Hub, document sessions, storage abstraction (web host), i18n bootstrap

- **Milestone:** M0  ·  **Depends on:** WP-02, WP-03, WP-06
- **Owns:** `apps/web/src/**`, `apps/web/index.html`, `apps/web/vite.config.ts`, `apps/web/public/manifest.webmanifest`, `packages/storage/src/**`, `packages/storage/test/**`, `packages/i18n/**`, `e2e/specs/hub.spec.ts`, `e2e/specs/shell.spec.ts`, `e2e/helpers/**`
- **Task:** Build apps/web as the real shell: client router with routes /, /write/:docId, /grid/:docId, /deck/:docId, /notes/:docId, /present/:docId (no server), `AppFrame` + `Ribbon` + `StatusBar` + `Backstage` from ui-kit mounted per app with a placeholder editor area (`<div data-testid="editor-root">`), app accent set via data-app, theme provider, command registry wiring (palette, key tips, shortcuts active), `DocumentSession` abstraction ({docId, app, title, dirty, handle?, model slot}) with a session store (zustand) and multi-document tabs/windows strategy (web: one document per tab; `window.name`-based docId handoff), Hub (§1.6) with recents/pinned from storage, template cards (placeholders until WP-32), "Open" via storage host, drag-and-drop of files onto Hub, file-type routing by extension/magic bytes. packages/storage: `PlatformHost` interface (§3.8), `WebHost` (File System Access API with permission re-prompt, IndexedDB-persisted handles via dexie; download fallback; `.scriba/.tabula/.scena` bundle read/write helpers), `RecentsStore`, `AutosaveJournal` interface (append(update), snapshot(), restore()) with an IndexedDB implementation, `RecoveryRegistry` (dirty/closedCleanly flags, heartbeat), `detectHost()` returning Web/Electron/Capacitor by probing `window.officina` / `Capacitor.isNativePlatform()` (adapters themselves arrive in WP-31/WP-35/34 — define the interfaces and a `NotImplementedHost` guard). packages/i18n: i18next + react-i18next + ICU, `locales/en/{common,ribbon,backstage,errors,formula}.json`, `t()` wrappers, `useLocale`, number/date formatting helpers via Intl, RTL detection (`dir` attribute), and a `missing-keys` test that scans ribbon specs and ui-kit strings for untranslated literals (allow-list file). Backstage Info/Print/Export panes get real property display (title, counts come from the editor via an `AppCapabilities` interface the editor WPs implement). Options dialog persisted (localStorage) incl. AutoSave interval, theme, language, Classic Office look. e2e: hub.spec.ts (Hub renders, theme toggle screenshot light+dark, recents list after opening a dropped fixture), shell.spec.ts (each app route shows ribbon with the §1.4 tab names in order, Ctrl+Shift+P opens palette and filters "Freeze Panes", Alt shows key tips, Ctrl+F1 collapses ribbon, Backstage opens via File and closes with Esc; screenshots per app light/dark at 1440x900 saved as shell/<app>-<theme>-desktop.png).
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm e2e exits 0 and the screenshots e2e/screenshots/shell/{write,grid,deck}-{light,dark}-desktop.png and hub/hub-{light,dark}-desktop.png exist
  - e2e asserts ribbon tab labels for Write are exactly: Home, Insert, Layout, References, Review, View; Grid: Home, Insert, Page Layout, Formulas, Data, Review, View; Deck: Home, Insert, Design, Transitions, Animations, Slide Show, Review, View (File is the backstage button)
  - pnpm --filter @officina/storage --filter @officina/i18n test exits 0 with >= 30 tests (fake IndexedDB via fake-indexeddb catalog entry)
  - Visual review recorded in docs/qa/visual-review-WP-08.md after opening the PNGs

### WP-09 — Formula function library (>= 350 functions), docs, conformance suite

- **Milestone:** M0  ·  **Depends on:** WP-07
- **Owns:** `packages/formula/src/functions/**`, `packages/formula/test/conformance/**`, `packages/formula/test/functions/**`, `docs/formula-deviations.md`
- **Task:** Fill the function registry to >= 350 Excel functions across the families of §3.4, using @formulajs/formulajs as the implementation base where its semantics match Excel (wrap with our value model: errors, blanks, arrays, lazy args) and native implementations where it does not (notably: TEXT/VALUE via numfmt, date functions with the 1900 system, SUMIF(S)/COUNTIF(S)/AVERAGEIF(S)/MAXIFS/MINIFS criteria parsing incl. wildcards and comparison strings, lookup functions with match modes and binary search, SUBTOTAL/AGGREGATE with hidden rows via host, financial functions on decimal.js where precision matters, the dynamic-array family, TEXTSPLIT/TEXTBEFORE/TEXTAFTER, LET/LAMBDA helpers already in WP-07). Each function gets a docs entry in `src/functions/docs/<FAMILY>.json` ({name, category, summary, syntax, args:[{name, description, optional, repeating}], examples:[{formula, result}], since}) used by autocomplete, the fx dialog and the Formula Lab; a test asserts every registered function has docs and every docs example evaluates to its stated result. Conformance: write `test/conformance/*.csv` with >= 1,500 rows (formula, expected, type, notes) in these sources: (a) curated from Microsoft's documented examples per function (type them in by family), (b) generated by `tools/gen-conformance.mjs` (owned here, under packages/formula/tools) which writes an .xlsx with formulas via exceljs, runs the LibreOffice oracle with recalc-on-load (tools/oracle-soffice.sh ... csv), and records values — flag rows where LibreOffice is known to differ from Excel in notes and exclude them from (b) by a deny-list. The conformance test runner compares with type-aware tolerance (numbers to 1e-9 relative, dates as serials, errors by name, text exact). Document genuine deviations in docs/formula-deviations.md with reasons. Implement `CELL` (subset: address, col, row, contents, type, filename blank), `INFO` subset, `ENCODEURL`, `HYPERLINK` returning a link value type.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/formula exec tsx -e "import('./src/index.ts').then(m=>console.log(m.listFunctions().length))" prints >= 350
  - pnpm --filter @officina/formula test exits 0; conformance runner reports >= 1500 cases with 0 failures (skips are listed with reasons in docs/formula-deviations.md)
  - A test asserts all function docs examples evaluate to their stated results
  - Spot checks pass as tests: =TEXT(1234.5,"#,##0.00") -> "1,234.50"; =DATEDIF(DATE(2020,1,31),DATE(2020,3,1),"md") -> 1; =XLOOKUP(5,{1,3,5},{"a","b","c"}) -> "c"; =SUMIFS(A:A,B:B,">=2024-01-01") style criteria; =0.1+0.2=0.3 -> TRUE; =IRR({-100,30,40,50}) within 1e-7 of 0.0894

### WP-10 — Write document model (ProseMirror schema, styles, numbering, sections, fonts)

- **Milestone:** M1  ·  **Depends on:** WP-01
- **Owns:** `packages/doc-model/**`
- **Task:** Implement §3.3 doc-model: the ProseMirror schema (`createDocSchema()`) with every node/mark listed, attribute types in `src/types.ts` mirroring WordprocessingML semantics (units in twips/half-points as in the file, with helpers to px/pt), `StyleSheet` (paragraph/character/table/numbering/list styles, basedOn/next/link, docDefaults, latent styles ignored), `resolveParagraphProps(doc, paragraphNode)` and `resolveRunProps(...)` implementing Word's inheritance order (docDefaults → table style → paragraph style chain → numbering level pPr/rPr → direct formatting → run style), `Numbering` (abstractNum/num/overrides, level text templates like "%1.%2", formats decimal/lowerLetter/upperRoman/bullet/… with `formatNumber(level, counters)`, restart logic, legal numbering, `computeListLabels(doc)` producing labels per paragraph in document order), sections (page size/margins/orientation/columns/header-footer references/titlePg/evenOdd, section break types), fields (`FieldInstruction` parser for PAGE, NUMPAGES, DATE, TIME, TOC with switches, REF, HYPERLINK, SEQ, PAGEREF; unknown preserved), theme font/colour resolution via @officina/ooxml theme types, the **font substitution table** (§1.2; Office names → bundled fonts, with metric-compatible flag) and `fontStack(name)` for CSS, `DefaultTemplate` producing a new document with Word-2016+ defaults (Normal: Calibri→Carlito 11 pt, line 1.08 (259 lineRule auto), after 8 pt; Heading 1–3 as Word; Title, Subtitle, Quote, List Paragraph, Table Grid, TOC 1–3, Footnote Text, Header, Footer, Hyperlink, Strong, Emphasis, Caption), a plain-text and JSON (de)serialiser for tests/version diffs, document statistics (`countWords/characters/paragraphs/pages?(from layout callback)`), and a `DocCommands` set of pure transforms (apply style, toggle mark, set alignment, indent/outdent list level, insert table/row/col, merge/split cells, insert section break, set numbering) used by the editor. Tests: schema validity, inheritance resolution matrix (≥ 40 cases), numbering label computation (≥ 30 cases incl. restart/legal/multilevel), field parsing, default template snapshot.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/doc-model test exits 0 with >= 120 tests
  - A test demonstrates resolveRunProps on a run in a styled paragraph in a styled table matches the documented Word precedence (table style < paragraph style < numbering < direct)
  - pnpm typecheck exits 0

### WP-11 — DOCX reader

- **Milestone:** M1  ·  **Depends on:** WP-05, WP-10
- **Owns:** `packages/io-docx/src/read/**`, `packages/io-docx/src/common/**`, `packages/io-docx/src/index.ts`, `packages/io-docx/test/read/**`
- **Task:** Implement `readDocx(bytes) -> {doc: ProseMirrorNode, styles, numbering, sections, media, comments, footnotes, endnotes, settings, properties, passthrough, warnings[]}` per §3.5: every element listed there (body paragraphs/runs/breaks/tabs/symbols, rPr/pPr including `w:rPrChange/w:pPrChange`, tables with gridSpan/vMerge/tblLook/tblStyle/cell margins/widths in all unit types, section properties incl. headers/footers (map `w:headerReference` types to section header/footer nodes; `w:titlePg`, `evenAndOddHeaders`), numbering.xml, styles.xml, settings.xml (defaultTabStop, trackRevisions, decimalSymbol), footnotes/endnotes with references, comments + commentsExtended threads + people, simple and complex fields with cached results (nested fields), hyperlinks (relationship targets, anchors), bookmarks (start/end), inline and anchored drawings (wp:inline/wp:anchor with position/wrap/extent/docPr; pic:pic images; wps:wsp text boxes and shapes using ooxml DrawingML; `a:graphicData` charts referenced for WP-20 by relationship id; VML `w:pict` images read-only fallback), `w:ins/w:del/w:moveFrom/w:moveTo` revisions → insertion/deletion marks with author/date, `m:oMath` → equation node holding OMML XML, `w:sdt` content controls → unwrap content and keep properties in a `sdt` attribute (preserved on write by WP-13), RTL/bidi attributes, document properties. Media extracted to blobs with content types. Warnings for every unsupported construct with part/path info (used by the Compatibility Checker). Read must be streaming-friendly: run in a worker-compatible way (no DOM). Tests: each generated fixture in packages/testing/fixtures/docx/features/* yields a document whose text equals mammoth's extracted text (normalised), expected structure assertions per feature fixture (e.g. numbering labels "1.", "a.", "i."; table 3x3 with merged cells; section count), no unhandled exceptions on every fixture in fixtures/external (when present), warnings list is empty for the synthetic corpus.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/io-docx test exits 0 with >= 80 tests
  - A test compares normalised text of every fixtures/docx/features/*.docx to mammoth's output and passes
  - A test reads a track-changes fixture and asserts insertion/deletion marks with authors
  - pnpm typecheck exits 0

### WP-12 — Write editor core: TipTap integration, pagination, rulers, Home/Insert/Layout ribbon wiring

- **Milestone:** M1  ·  **Depends on:** WP-06, WP-08, WP-10
- **Owns:** `packages/editor-write/src/**`, `packages/editor-write/test/**`, `apps/web/src/apps/write/**`, `e2e/specs/write-core.spec.ts`
- **Task:** Build the Write editor per §3.6: TipTap 3 editor with custom extensions for every doc-model node/mark (no StarterKit nodes that conflict with ours — use ours exclusively, with input rules), ProseMirror plugins: selection/cursor, history (later replaced by Yjs undo in WP-29 — keep a `HistoryStrategy` seam), paste handling (HTML sanitised with dompurify → doc-model via an HTML importer with style mapping; plain text; Markdown detection → styled; images as media), drag-and-drop images, tab key behaviour (list indent / tab char), smart typography (optional). Pagination layer (§3.6): PaginationPlugin measuring blocks via ResizeObserver + Range.getClientRects, page model {pages:[{top, height, breaks}]}, page chrome overlay (page edges, gap, margins guides, page numbers), per-page headers/footers rendered from section header/footer nodes with live PAGE/NUMPAGES fields, footnote areas, keepNext/keepLines/pageBreakBefore/widow-orphan rules, table rows cantSplit, column sections (CSS multicol per section block), progressive layout with requestIdleCallback for long documents, and `getPageCount()` / `pageOfPosition(pos)` used by the status bar and navigation. Views: Print Layout (paginated), Web Layout (continuous), Read Mode (paginated columns, no editing), Draft. Rulers (horizontal with margin/indent/tab stop drag; vertical with margins). Zoom (ctrl+wheel, slider, presets, page width/whole page). Status bar items: page x of y, word count (selection-aware), language, zoom. Ribbon wiring for Home/Insert/Layout/View per §1.4 (commands implemented here: all font/paragraph/style/clipboard/paste-special operations, insert table (grid picker), picture, link, page break, header/footer editing mode (double-click header area), page setup dialog (margins/orientation/size/columns), bullets/numbering galleries with Word's default definitions, line spacing, styles gallery with live preview and Styles task pane basics). Mini toolbar on selection; context menu. apps/web/src/apps/write: the Write page composing AppFrame/Ribbon/editor/panes/status bar, `WriteCapabilities` for Backstage Info, open/save through storage using WP-11 reader (writer arrives in WP-13 — until then Save writes the native .scriba bundle) with a worker for parsing. Tests: jsdom tests for extensions and commands; pagination tested with a fake measurer; e2e write-core.spec.ts: open fixtures/docx/features/{styles,numbering,tables,sections}.docx via the e2e file chooser, screenshot each (write-core/<name>-light-desktop.png), type text with Ctrl+B and verify bold mark, apply Heading 1 via ribbon and via Ctrl+Alt+1, insert a 3x3 table, check status bar page count > 1 for sections fixture, zoom slider changes scale.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/editor-write test exits 0 with >= 80 tests
  - pnpm e2e --grep write-core exits 0 and screenshots write-core/{styles,numbering,tables,sections}-light-desktop.png exist
  - e2e asserts the page count shown in the status bar for fixtures/docx/features/sections.docx equals the page count LibreOffice reports for the same file (via pdfinfo of the oracle PDF, exposed through a tiny e2e helper) within ±1
  - docs/qa/visual-review-WP-12.md records the review of the screenshots

### WP-13 — DOCX writer and round-trip fidelity tests

- **Milestone:** M1  ·  **Depends on:** WP-11
- **Owns:** `packages/io-docx/src/write/**`, `packages/io-docx/test/write/**`, `packages/io-docx/test/roundtrip/**`
- **Task:** Implement `writeDocx(model, {passthrough}) -> Uint8Array` emitting every construct the reader understands (§3.5) with Office-compatible XML (ordering of child elements per the schema sequences — e.g. w:pPr children order, w:rPr order; `w:tblGrid`; `w:sectPr` placement in the last paragraph of each section except the final one in body), styles.xml/numbering.xml/settings.xml/footnotes/endnotes/comments(+Extended, +people)/headers/footers parts, media parts with relationships, content types, core/app properties (modified date updated, application "Officina"), fields as complex fields with cached results, revisions, bookmarks, hyperlinks, drawings (inline/anchor, pictures, text boxes, shapes via ooxml DrawingML writer), equations (OMML passthrough), sdt properties re-wrapping, and the passthrough parts (customXml, vbaProject.bin for .docm, glossary, unknown parts) with relationships repaired. Deterministic output (same model → identical bytes). Compatibility Checker data: `checkCompatibility(model) -> issues[]` listing features that will be lost (e.g. callout blocks → bordered paragraphs, code blocks → 'Code' style + shading). Round-trip tests: for every fixture read→write→read, structural score (packages/testing scorer) ≥ 0.95 and text 1.00; LibreOffice opens every written file (oracle_opens = 1) and the visual score between oracle renders of original and rewritten ≥ 0.90; writer output also opens with mammoth and python-docx without exceptions; a write-from-DefaultTemplate test produces a .docx whose styles.xml contains the Word default style ids. Write results to docs/qa/fidelity.md (docx section).
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/io-docx test exits 0 with >= 60 additional tests
  - pnpm fidelity --format docx prints a table where every features/* fixture has structural >= 0.95, text == 1.00, oracle_opens == 1, visual >= 0.90
  - A determinism test asserts identical bytes for two consecutive writes of the same model
  - docs/qa/fidelity.md contains the docx table

### WP-14 — Write advanced features: tables UI, floating objects, find/replace, styles & navigation panes, TOC, footnotes, comments, track changes, Format Painter, Markdown shortcuts, focus mode, spell check

- **Milestone:** M1  ·  **Depends on:** WP-12
- **Owns:** `packages/editor-write/src/features/**`, `packages/editor-write/test/features/**`, `apps/web/src/apps/write/**`, `e2e/specs/write-features.spec.ts`
- **Task:** Add to editor-write (new `features/` modules, each registering its commands and ribbon contributions through a `WriteFeature` interface the core exposes): Tables (Table Design/Layout contextual tabs: styles gallery with banded/header options from doc-model table styles, borders/shading, insert/delete rows/cols, merge/split, cell size, distribute, alignment, repeat header rows, convert to text, autofit; cell selection with mouse/keyboard; column resize handles on the ruler and the table), Floating objects (Picture/Shape Format: wrap modes square/tight/top-bottom/behind/in-front implemented with float spacers and shape-outside; position dialog; anchor dragging; crop; alt text; bring forward/backward; Selection Pane), Find & Replace (navigation pane results, match case/whole word/regex/wildcards, replace all, Go To page/section/line), Styles pane (list, modify style dialog, new style, clear all, show preview), Navigation pane (headings tree with drag reorder, pages thumbnails (rendered lazily), results), References: TOC insertion/update from heading styles with page numbers from pagination, clickable; footnotes/endnotes insert/navigate/show; captions with SEQ; cross-references dialog; bookmarks. Review: comments task pane (threaded, resolve, reply, @mention stub) anchored to ranges; Track Changes (insert/delete/format revisions recorded as marks with author/time, display modes Simple/All/None/Original, Reviewing pane, accept/reject one/all, next/previous, balloons in margin in All Markup), Word Count dialog, Spelling via nspell + dictionary-en (squiggles as decorations, right-click suggestions, add to dictionary, ignore). Home: Format Painter (single and double-click persistent; Ctrl+Shift+C/V), Change Case, Clear Formatting, Sort paragraphs, Show ¶, Paste Special dialog. Markdown shortcuts (input rules: #..###### headings, -, *, 1., [ ] task, >, ```, ---, **bold**, *italic*, `code`, links), smart paste of Markdown text, Focus mode (hide chrome, centre page, typewriter scrolling, word-goal from Options), Callout and Code blocks (code block with syntax highlighting via a small tokenizer for 8 languages; both have Compatibility Checker mappings). Wire everything into the Write ribbon spec (apps/web/src/apps/write/ribbon.ts) per §1.4 including contextual tabs. e2e write-features.spec.ts: screenshots of track-changes view, comments pane, TOC inserted, floating image with square wrap, find/replace dialog, focus mode (light+dark); functional assertions: replace-all count, accept-all removes marks, TOC entries equal heading count, Format Painter copies bold+colour.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/editor-write test exits 0 with >= 120 additional tests
  - pnpm e2e --grep write-features exits 0 with screenshots write-features/{track-changes,comments,toc,floating-image,find-replace,focus}-{light,dark}-desktop.png
  - e2e: Ctrl+Shift+E toggles track changes and the status bar shows 'Track Changes: On'
  - docs/qa/visual-review-WP-14.md written after viewing all screenshots

### WP-15 — Write integration gate: open/edit/save Office files end to end, print & PDF export, Backstage panes  (GATE)

- **Milestone:** M1  ·  **Depends on:** WP-13, WP-14
- **Owns:** `apps/web/src/apps/write/**`, `packages/print-pdf/src/core/**`, `packages/print-pdf/src/write/**`, `packages/print-pdf/src/index.ts`, `packages/print-pdf/package.json`, `packages/print-pdf/test/core/**`, `packages/print-pdf/test/write/**`, `e2e/specs/write-e2e.spec.ts`, `e2e/specs/print.spec.ts`, `docs/qa/visual-review-WP-15.md`, `docs/manual/write.md`
- **Task:** Integration gate for Write. Wire the DOCX writer into Save/Save As (format picker: .docx, .scriba, .pdf, .md, .html, .txt; Compatibility Checker dialog before saving to .docx when issues exist, with "Don't show again" per document), AutoSave-to-file, dirty tracking, title rename → file rename, recents, "Open Recent" and drag-drop at the Write route. packages/print-pdf (create the package core in src/core: page model, print route host, preview component, pdf-lib post-processing; Write-specific layout in src/write; Grid/Deck add src/grid and src/deck later): print layout route (`/print/:docId` renders the paginated document using the same page model with @page CSS, headers/footers/footnotes per page, no chrome), Print preview inside Backstage ▸ Print (iframe of the print route with page thumbnails, settings: printer (browser), pages range, copies, orientation, paper size, margins, scale), `exportPdf()` pipeline: `PlatformHost.print({toPdf:true})` for Electron (printToPDF) and `window.print()` for web with instructions; post-processing with pdf-lib (title/author metadata, outline from headings, page labels) when the host returns bytes; Markdown and HTML exporters (doc-model → Markdown with tables, images, footnotes; HTML with inline styles). Backstage Info shows properties and version list placeholder (WP-29 fills). Test the whole loop in e2e write-e2e.spec.ts: open fixtures/docx/features/styles.docx → edit (type, bold, heading, insert table) → Save As docx via the e2e download hook → the saved file is re-read by io-docx in the test and contains the edits → LibreOffice converts the saved file to PDF without error; print.spec.ts: the print route renders N pages where N equals the status bar page count, screenshot print/write-print-preview-light-desktop.png; `exportPdf` in Chromium headless via Playwright's page.pdf() of the print route produces a PDF with N pages (pdfjs-dist page count). Write the Write chapter of the manual (docs/manual/write.md): opening/saving, ribbon tour, track changes, TOC, keyboard shortcuts link.
- **Acceptance criteria:**
  - pnpm e2e --grep 'write-e2e|print' exits 0
  - e2e: saved .docx re-read by io-docx contains the edited text and a Heading 1 paragraph; soffice converts it to PDF (exit 0)
  - e2e: PDF produced from the print route has the same page count as the status bar shows for fixtures/docx/features/sections.docx
  - pnpm verify exits 0 and CI is green
  - docs/qa/visual-review-WP-15.md lists every Write screenshot reviewed and defects fixed; docs/manual/write.md exists with >= 600 words

### WP-16 — Sheet model: workbook/sheets/sparse cell store, styles, merges, names, command log, formula binding

- **Milestone:** M2  ·  **Depends on:** WP-07, WP-09
- **Owns:** `packages/sheet-model/**`
- **Task:** Implement §3.3 sheet-model: Workbook/Sheet/SparseStore/RowChunk exactly as specified with O(1) random access, iteration in row-major order, `getUsedRange`, `forEachInRange`, bulk `setRange(values2D)`, row/col property arrays with default sizes (15 pt rows, 8.43 char columns → px via font metrics), hidden/outline levels, merges (with validation and split), `StyleTable` (numFmt code, font, fill, border per side with styles, alignment, protection; dedupe by hashing; `cellXfs`-like indices), named ranges (workbook/sheet scope, hidden names), tables (ListObjects: name, range, header/totals rows, column names, style, autofilter), conditional formatting rule model (all §2.3 types with priority/stopIfTrue and dxf styles), data validation model, hyperlinks, comments/notes, sheet views (freeze, split, zoom, selection, gridlines, headings, rightToLeft), print settings, autofilter state, charts/drawings anchors (objects referenced by id; chart model from WP-20 later), workbook properties (date1904, calcMode). `CommandLog` with inverse operations for every mutation (setCell(s), setStyle, insert/delete rows/cols (shifting merges, names, CF, validations, tables, formulas via engine), move/copy range, sort, rename sheet, add/remove/reorder sheets, freeze…) with undo/redo stacks, grouping of commands into transactions, and serialisation of commands. Formula binding: implement the @officina/formula host interface over the store; `WorkbookCalc` runs the engine (in-process here; a worker wrapper comes in WP-17) with incremental recalc, spill ranges, calc mode auto/manual, `recalcSheet`, structured reference resolution through the tables model, defined names, `explain(cell)`. Number formatting bridge: `formatCell(value, numFmtCode, locale)` via numfmt including Excel's General format rules (11 significant digits display, scientific switch), date/time formats, percentages, fractions, accounting with alignment padding markers, colours in format codes (`[Red]`), conditional sections. Snapshot/restore (structured clone-safe JSON) for versions/journals, with gzip via fflate. Tests: store semantics, insert/delete shifting matrices (≥ 50 cases), undo/redo invariants (property tests), number formatting (≥ 150 cases against numfmt's own expectations plus Excel-documented examples), structured references, calc integration (dependent cells recalc, spill), snapshot round-trip. Benchmark: fill 1M cells in < 2 s and iterate them in < 300 ms.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/sheet-model test exits 0 with >= 250 tests
  - pnpm --filter @officina/sheet-model bench shows 1M-cell fill < 2 s and full iteration < 300 ms
  - A test demonstrates inserting a row above a formula's reference shifts the formula text and the dependency graph correctly (value unchanged after recalc)
  - pnpm typecheck exits 0

### WP-17 — Grid editor core: canvas renderer, selection, scrolling, freeze panes, in-cell editing, formula bar, autocomplete, fill handle, clipboard, Home ribbon

- **Milestone:** M2  ·  **Depends on:** WP-06, WP-08, WP-16
- **Owns:** `packages/editor-grid/src/**`, `packages/editor-grid/test/**`, `packages/editor-grid/bench/**`, `apps/web/src/apps/grid/**`, `e2e/specs/grid-core.spec.ts`
- **Task:** Build editor-grid per §3.6: canvas renderer (DPR-aware; layers: cells, grid lines, borders with Excel precedence, text with numfmt output, overflow into empty neighbours, wrap/rotation/indent/vertical align, merged cells, conditional format visuals, selection/fill-handle overlay canvas), row/column headers with resize/autofit on double-click, hidden rows/cols markers, freeze panes (4 viewports), smooth scrolling with virtualisation (Fenwick-tree offsets), zoom (50–400 %), text measurement cache, sheet tab strip (+, rename, reorder by drag, colour, hide, context menu), Name Box (type A1/range/name to navigate; define name on Enter), Formula Bar (expand/collapse, fx button opening the Insert Function dialog built from function docs, with range highlighting while editing and syntax colouring), in-cell editor (F2/typing/double-click; Enter/Tab/Shift variants; Alt+Enter newline; Esc; arrow keys select ranges into the formula while editing like Excel's point mode; autocomplete of function names with signature tooltip and inline docs (★), autocomplete of column values, data validation dropdown host), selection model (ranges, multi with Ctrl, whole rows/cols, Ctrl+A regions, Ctrl+Arrow edge jumps, Shift extensions, keyboard per §1.7), fill handle (drag copy/series detection numbers/dates/weekdays/months/custom lists, Ctrl modifier, double-click autofill to adjacent extent, Fill menu Down/Right/Series dialog), clipboard (internal rich copy with styles/formulas, HTML table + TSV for external paste/copy, Paste Special dialog: all/values/formulas/formats/transpose/skip blanks/operations), cut/paste moving references, insert/delete cells/rows/cols with shift options, undo/redo via CommandLog, status bar: mode (Ready/Enter/Edit/Point), Average/Count/Sum of selection, zoom, view buttons. Worker: run `WorkbookCalc` in a Web Worker via comlink with a main-thread mirror of computed values (transfer dirty-cell batches); fall back to in-process when workers unavailable (tests). Home ribbon per §1.4 fully wired (font, borders menu, fill/font colour, alignment, wrap, merge variants, number format combo + buttons, cell styles gallery, insert/delete menus, format menu, AutoSum, Fill, Clear, Find & Select basics; Conditional Formatting and Format as Table menus route to WP-19's dialogs so leave the command ids registered but marked pending until WP-19). Format Cells dialog (Ctrl+1) with all six tabs. apps/web/src/apps/grid: page composition, open .xlsx through io-xlsx when WP-18 lands (until then .csv via papaparse and .tabula bundles — implement the CSV import/export here: delimiter/encoding sniffing, Text Import wizard-lite), `GridCapabilities`. Tests: renderer unit tests with a canvas mock (which cells drawn, border precedence), selection/navigation tests, fill-series detection tests, clipboard TSV/HTML parsing tests, editor state machine tests. e2e grid-core.spec.ts: open fixtures/xlsx via csv path and generated 50k-row CSV, type values and formulas (=SUM(A1:A3)) and verify computed display, F2 edit, fill handle drag via mouse, freeze top row then scroll (screenshot), zoom, Ctrl+1 dialog screenshot, status bar sum; screenshots grid-core/*-{light,dark}-desktop.png. Benchmark: Playwright scroll test measuring frame time on a synthetic 1M-populated-cell sheet (generate in-page).
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/editor-grid test exits 0 with >= 150 tests
  - pnpm e2e --grep grid-core exits 0 with screenshots grid-core/{basic,freeze,format-cells,formula-autocomplete}-{light,dark}-desktop.png
  - e2e: after typing 1,2,3 in A1:A3 and =SUM(A1:A3) in A4, cell A4 displays 6 and the status bar shows Sum: 6 when A1:A4 is selected
  - bench: average frame time while scrolling the 1M-cell synthetic sheet < 18 ms (recorded in docs/qa/perf.md)
  - docs/qa/visual-review-WP-17.md written

### WP-18 — XLSX reader and writer (+ CSV/TSV/JSON) with round-trip fidelity tests

- **Milestone:** M2  ·  **Depends on:** WP-05, WP-16
- **Owns:** `packages/io-xlsx/**`
- **Task:** Implement `readXlsx(bytes) -> Workbook` and `writeXlsx(workbook) -> Uint8Array` per §3.5 (everything listed: workbook/sheets/definedNames/calcPr/date1904, styles.xml fully (numFmts incl. built-in ids 0–49 table, fonts, fills incl. gradients, borders, cellXfs with alignment/protection, cellStyles/cellStyleXfs, dxfs), sharedStrings with rich text runs and phonetic ignored, inline strings, sheetData with all cell types and shared/array/dynamic-array formulas (`cm` + metadata.xml for spill), cached values kept so files open instantly before recalculation, cols/rows props, mergeCells, conditionalFormatting incl. x14 extLst (data bars, icon sets with custom sets, colour scales), dataValidations incl. x14 list references to other sheets, hyperlinks (internal/external), autoFilter with filter state, tableParts + tables, sheetViews (pane/freeze/split/selection/zoom/showGridLines/rightToLeft), sheetPr (tabColor, outlinePr), pageSetup/pageMargins/printOptions/headerFooter/rowBreaks/colBreaks, legacy comments + VML drawing for notes (write VML for notes so Excel shows them), threadedComments + persons, drawings with two-cell/one-cell/absolute anchors for pictures and shapes (DrawingML via ooxml), chart parts passed to the charts package via a `ChartPartHook` interface (WP-20 implements; until then chart parts are preserved as passthrough and re-anchored), pivot tables/caches, slicers, externalLinks, customXml, vbaProject.bin, printerSettings all passthrough with relationship repair; sheet order and hidden/veryHidden; workbook protection/sheet protection flags preserved). Writer emits deterministic, Excel-valid XML (element ordering per CT_Worksheet sequence; `r` attributes on rows/cells; styles deduped; shared strings table; `fullCalcOnLoad=1` set when any formula lacks a cached value). CSV/TSV read/write with papaparse (sniffing per §3.5; dates/numbers typed on import with locale option; export respects number formats option), JSON import (array of objects → table). Tests: every fixtures/xlsx/features/* file read → write → read with structural ≥ 0.97, values 1.00 (compare with exceljs reading both the original and our output), formulas string-equal after normalisation, styles compared through exceljs (font/fill/border/numFmt per cell), LibreOffice opens all outputs and `--convert-to csv` of the rewritten file equals that of the original for the first sheet, openpyxl reads outputs; 100k-row fixture read+write under 3 s each. Write docs/qa/fidelity.md xlsx section via `pnpm fidelity --format xlsx`.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/io-xlsx test exits 0 with >= 120 tests
  - pnpm fidelity --format xlsx prints structural >= 0.97, values == 1.00, oracle_opens == 1 for every features/* fixture
  - A test shows a dynamic-array formula (=SORT(A1:A5)) survives round-trip with its spill metadata and Excel-compatible cm attribute
  - A test reads a .xlsm fixture, writes it, and vbaProject.bin bytes are identical in the output

### WP-19 — Grid features: sort & filter, conditional formatting UI, data validation, Format as Table, find/replace, comments/notes, hyperlinks, Page Layout, Formulas and Data ribbons, formula auditing, Formula Lab

- **Milestone:** M2  ·  **Depends on:** WP-17, WP-18
- **Owns:** `packages/editor-grid/src/features/**`, `packages/editor-grid/test/features/**`, `apps/web/src/apps/grid/**`, `e2e/specs/grid-features.spec.ts`
- **Task:** Add feature modules (via a `GridFeature` interface the core exposes): AutoFilter (header dropdown buttons drawn on canvas; filter pane with search, value checklist, text/number/date/colour filters, sort A→Z/Z→A/by colour; filter state model persisted via io-xlsx), Sort dialog (multi-level, header detection, case, orientation, custom lists), Conditional Formatting dialogs (every rule type of §2.3, Manage Rules with priority/stop-if-true, live preview, rule application in renderer already supported by WP-17 visuals), Data Validation dialog (settings/input message/error alert; dropdown in-cell; circle invalid data), Format as Table (style gallery with 60 Excel-like table styles generated from theme colours; header/total/banded toggles; resize; convert to range; structured references in autocomplete), Find & Replace dialog (within sheet/workbook, by rows/cols, in formulas/values/notes, match case/entire cell; Go To Special: blanks/formulas/constants/visible), Notes and threaded Comments (pane, indicators, hover popups), Hyperlinks (insert dialog, Ctrl+click, cell styling), Clear menu, Remove Duplicates, Text to Columns wizard (delimited/fixed width with preview), Name Manager dialog + Define Name + Create from Selection + Use in Formula menu, Formula Auditing (trace precedents/dependents arrows drawn on overlay canvas, remove arrows, Show Formulas (Ctrl+`), Error Checking pane listing error cells with explanations, Evaluate Formula dialog stepping through the engine's explain tree, Watch Window), ★Formula Lab task pane (AST tree with live values per node, argument docs), Calculation options (auto/manual, F9/Shift+F9), Page Layout ribbon (margins/orientation/size/print area/print titles/breaks/scale-to-fit/gridlines+headings print toggles, Page Break Preview and Page Layout views rendering page boundaries), Insert ribbon items that don't need charts (table, picture floating over cells, shapes basic (rectangle/ellipse/line/textbox) with DrawingML anchors, text box, header & footer dialog, symbol), Review ribbon (spelling pass over text cells, comments navigation, protect sheet with password-less lock + locked-cell enforcement in editor, protect workbook structure), View ribbon (gridlines/headings/formula bar toggles, new window (web: new tab), split, hide/unhide, custom zoom). Group/outline rows and cols with outline symbols (Data ▸ Outline). Wire the complete Grid ribbon spec per §1.4. e2e grid-features.spec.ts: filter a column and assert hidden rows; conditional formatting data bars screenshot; validation dropdown screenshot and error on invalid; Format as Table + structured ref formula; trace precedents screenshot; Formula Lab screenshot; Page Break Preview screenshot; light+dark.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/editor-grid test exits 0 with >= 120 additional tests
  - pnpm e2e --grep grid-features exits 0 with screenshots grid-features/{filter,cf-databars,validation,table,trace,formula-lab,page-break-preview}-{light,dark}-desktop.png
  - e2e: applying a filter 'contains "a"' on a 100-row column hides the expected rows and the status bar reads 'n of 100 records found'
  - docs/qa/visual-review-WP-19.md written

### WP-20 — Charts package: chart model, ECharts renderer, chart editing UI, OOXML chart read/write

- **Milestone:** M2  ·  **Depends on:** WP-05, WP-16, WP-18
- **Owns:** `packages/charts/**`, `packages/io-xlsx/src/charts/**`, `packages/io-xlsx/test/charts/**`
- **Task:** packages/charts: `ChartModel` (types of §2.3/§3.5: column/bar clustered/stacked/100%, line (markers, smooth), area, pie/doughnut (explosion, labels), scatter (lines/markers), bubble, radar, combo with secondary axis, treemap, sunburst, histogram, waterfall, funnel; series {name ref/literal, categories ref, values ref, cache, formatting}, axes (category/value/date; title, min/max/major unit, number format, reversed, log, gridlines, label rotation), legend, titles, data labels (position, value/percent/name), plot area/chart styles (fill/line), colours from theme palette (`accent1..6` with variations) with dark-mode-aware rendering), `ChartRenderer` (React component mapping ChartModel → ECharts 6 option; resize observer; export to PNG/SVG), `ChartData` resolution via a host interface (`getRange(ref) -> values`) so Grid charts are live and Write/Deck charts use embedded workbook data (through io-xlsx on the embedded part), Chart Design/Format ribbon group specs and dialogs (Change Chart Type gallery, Select Data dialog with series editor and switch row/column, Add Chart Element menu (axis titles, data labels, gridlines, legend position, trendline linear/exp/moving-average computed in-house), Quick Layouts, Chart Styles (12 presets), colour palettes), ★Recommended Charts (heuristics from selection shape: 1 numeric column → column; dates + numbers → line; 2 numeric → scatter; part-to-whole small set → pie), chart floating-object host for Grid (two-cell anchor move/resize using editor-grid's drawing layer interface — expose a `ChartObject` component; WP-22 wires it), `chart-ooxml`: read/write `c:chartSpace` subsets (incl. `c:userShapes` passthrough, `c:extLst` passthrough, `cx:chart` chartEx read for treemap/sunburst/waterfall/funnel/histogram and write as chartEx), number formats, cached data (`c:numCache/c:strCache`) written so Excel renders without recalculation, `c:style`/`c:clrMapOvr` preserved, chart relationships (embedded workbook part for Write/Deck, external data refs for Grid). Hook into io-xlsx via the `ChartPartHook` (packages/io-xlsx/src/charts) so sheets round-trip charts as models: read drawing anchors → ChartModel; write back. Tests: model ↔ ECharts option snapshot tests per type; OOXML read/write round-trip of fixtures/xlsx/features/charts-*.xlsx (every chart type) with structural ≥ 0.95 and LibreOffice opening the rewritten file; renderer tests in jsdom with ECharts' SVG renderer producing non-empty SVG; a Playwright screenshot of each chart type from a `packages/charts/stories/Charts.tsx` page via the e2e project (charts/<type>-light-desktop.png) reviewed visually.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/charts --filter @officina/io-xlsx test exits 0 with >= 90 tests for charts
  - pnpm fidelity --format xlsx --only charts prints structural >= 0.95 and oracle_opens == 1 for every charts-* fixture
  - e2e --grep charts produces screenshots for >= 12 chart types and docs/qa/visual-review-WP-20.md reviews them
  - A test asserts a chart written to xlsx includes c:numCache values for every series

### WP-21 — Grid analysis: pivot tables, sparklines, Goal Seek, Subtotal, Flash Fill, protection, Themes

- **Milestone:** M2  ·  **Depends on:** WP-19
- **Owns:** `packages/editor-grid/src/analysis/**`, `packages/editor-grid/test/analysis/**`, `packages/sheet-model/src/pivot/**`, `packages/sheet-model/test/pivot/**`, `apps/web/src/apps/grid/**`, `e2e/specs/grid-analysis.spec.ts`
- **Task:** Pivot tables (P1 but included): pivot model in sheet-model (source range/table, fields, row/column/filter/value areas, aggregations sum/count/average/min/max/product/countNums/stdDev/var, subtotals/grand totals, number format per value field, sorting, manual filter per field, compact/outline/tabular layouts, expand/collapse), pivot computation engine (group-by with stable ordering, dates grouped by year/quarter/month option), rendering as normal cells in a reserved range with pivot styling (PivotStyleLight16 default) and refresh on source change (manual Refresh + auto when source edited, debounced), PivotTable Fields task pane (drag between areas, value field settings dialog), PivotTable Analyze/Design contextual ribbon tabs, io-xlsx bridge: write `pivotTableDefinition` + `pivotCacheDefinition/Records` sufficient for Excel to open and refresh (keep original parts via passthrough when a pivot was read unchanged; regenerate when edited) — implement in packages/sheet-model/src/pivot/ooxml.ts and call it from io-xlsx through an exported hook (`PivotPartHook`, registered by the Grid app; io-xlsx already defines the hook interface in WP-18 — if not, add it to the interface file listed in io-xlsx's owned paths via a handoff note). Sparklines (line/column/win-loss; model + x14 sparkline groups read/write via io-xlsx hook; renderer in canvas; Sparkline Design ribbon with markers/colours). What-If ▸ Goal Seek dialog (secant method over the engine with iteration cap), Data ▸ Subtotal dialog (inserts SUBTOTAL formulas and outline groups), Flash Fill (pattern inference for prefix/suffix/split/case/concat of adjacent columns; Ctrl+E), Protection (Protect Sheet dialog with permission checkboxes and password hashed per Excel's legacy algorithm + SHA-512 modern `sheetProtection` attributes; Allow Edit Ranges basic), Page Layout ▸ Themes (apply Office-compatible theme colour/font sets to the workbook styles). e2e grid-analysis.spec.ts: create a pivot from a 200-row generated table (sum by category), screenshot; sparklines screenshot; Goal Seek sets a cell to hit a target; Subtotal creates outline groups; Flash Fill fills a split column.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/sheet-model --filter @officina/editor-grid test exits 0 with >= 80 additional tests
  - pnpm e2e --grep grid-analysis exits 0 with screenshots grid-analysis/{pivot,sparklines,goal-seek,subtotal}-light-desktop.png
  - A test writes a workbook with a pivot and LibreOffice opens it (oracle_opens == 1) and openpyxl lists one pivot table on the sheet
  - docs/qa/visual-review-WP-21.md written

### WP-22 — Grid integration gate: Office file loop, chart objects in sheets, print/PDF for sheets, Backstage, manual chapter  (GATE)

- **Milestone:** M2  ·  **Depends on:** WP-15, WP-17, WP-18, WP-19, WP-20, WP-21
- **Owns:** `apps/web/src/apps/grid/**`, `packages/print-pdf/src/grid/**`, `e2e/specs/grid-e2e.spec.ts`, `docs/qa/visual-review-WP-22.md`, `docs/manual/grid.md`
- **Task:** Integration gate for Grid: wire io-xlsx open/save (xlsx, xlsm read, xltx, csv, tsv, tabula, pdf export) with the worker parser and Compatibility Checker; wire the charts package into the sheet drawing layer (Insert ▸ Charts gallery and Recommended Charts, chart objects movable/resizable with two-cell anchors, Chart Design/Format contextual tabs, double-click to edit data via Select Data), pictures and shapes in the drawing layer (from WP-19) share the selection/transform code (use ui-kit TransformBox); Page Layout/print: packages/print-pdf/src/grid: print layout route rendering sheets to pages honouring print area, print titles, scaling/fit-to, margins, headers/footers with &[Page] codes, gridlines/headings options, page order; Backstage ▸ Print preview; PDF export of workbook/active sheet/selection; Backstage Info (properties, calc mode, sheet count), Export (CSV per sheet, PDF, JSON), Share. End-to-end e2e grid-e2e.spec.ts: open fixtures/xlsx/features/{styles,formulas,charts-column,cf,validation,tables}.xlsx → screenshots light/dark → edit (values, a formula, apply number format, insert a chart from a range) → Save As .xlsx via the e2e hook → re-read with io-xlsx AND exceljs in the test asserting the edits, formulas, chart count → LibreOffice converts to PDF without error; print route page count equals the Page Break Preview count; PDF produced via page.pdf() has that many pages; CSV export of a sheet equals papaparse-generated expectation. Manual chapter docs/manual/grid.md (formulas, autocomplete, tables, charts, pivots, printing, shortcuts). Fix any cross-feature defects found; record the visual review.
- **Acceptance criteria:**
  - pnpm e2e --grep grid-e2e exits 0 with screenshots grid-e2e/*-{light,dark}-desktop.png
  - e2e: the saved .xlsx re-read by exceljs contains the edited value, the formula text, the number format and one chart; soffice converts it to PDF (exit 0)
  - e2e: PDF export page count equals Page Break Preview page count for fixtures/xlsx/features/print.xlsx
  - pnpm verify exits 0 and CI is green; docs/qa/visual-review-WP-22.md and docs/manual/grid.md (>= 600 words) exist

### WP-23 — Deck model: presentation, masters/layouts/slides, shapes, placeholders, themes, transitions, animations, notes

- **Milestone:** M3  ·  **Depends on:** WP-05
- **Owns:** `packages/deck-model/**`
- **Task:** Implement §3.3 deck-model: Presentation/Slide/Shape types (EMU transforms, all shape kinds, placeholders with type/idx inheritance resolution master→layout→slide for position, size, text styles and fills via `resolveShapeProps(slide, shape)`), text bodies using a `deckText` ProseMirror schema (paragraphs with level/alignment/indent/spacing/bullets (char/autonumber/picture/none, bullet font/colour/size), runs with font/size/bold/italic/underline/strike/colour/highlight/baseline/spacing/caps/link, line breaks, fields (slidenum, datetime)), `bodyPr` (insets, anchor, wrap, autofit: normAutofit fontScale/lnSpcReduction, spAutoFit; vertical text), list styles (lvl1–9 pPr) from master text styles (title/body/other), theme handling (reuse ooxml theme; 12 bundled themes authored as JSON+assets in `src/themes/` with layouts: Title Slide, Title and Content, Section Header, Two Content, Comparison, Title Only, Blank, Content with Caption, Picture with Caption — generated from the default theme definition so new presentations are Office-compatible), slide size, backgrounds (fill types), transitions (model for the §1.4 list + duration + advance on click/after), animations (effect types entrance/emphasis/exit per §1.4, targets, start conditions, duration, delay, order, effect options direction; `buildTimeline(slide)` producing click-steps), notes, sections, comments, hidden slides, document properties. Operations as pure functions (`addSlide(layoutId)`, `duplicateSlide`, `moveSlide`, `applyLayout`, `resetSlide`, `addShape`, `transform`, `group/ungroup` (with coordinate re-basing), `reorderZ`, `align/distribute`, `setPlaceholderText`), plus the Yjs binding seam (`toY(doc)`, `fromY`) used in WP-29 (here implement JSON serialisation only with stable ids). `designIdeas(slide)` heuristic engine (★): given the slide's content types/counts, rank candidate layouts/variants (title-only → big centred title; title + 3–6 bullets → two-column or icon list; one image → picture-with-caption full bleed; numbers → stat callouts) returning concrete slide mutations. Tests: inheritance resolution (≥ 40 cases), autofit computation, animation timeline building, grouping coordinate maths, theme generation produces valid layouts, design ideas ranking.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/deck-model test exits 0 with >= 120 tests
  - A test asserts a title placeholder on a slide inherits position from the layout and font from the master when unspecified
  - pnpm typecheck exits 0

### WP-24 — PPTX reader

- **Milestone:** M3  ·  **Depends on:** WP-23
- **Owns:** `packages/io-pptx/src/read/**`, `packages/io-pptx/src/common/**`, `packages/io-pptx/src/index.ts`, `packages/io-pptx/test/read/**`
- **Task:** Implement `readPptx(bytes) -> Presentation` per §3.5: presentation.xml (sizes, id lists, sections from `p14:sectionLst`, notesMaster), masters/layouts/slides with full spTree parsing (p:sp with geometry presets/custom geometry (`a:custGeom` paths), fills/lines/effects via ooxml, text bodies into deckText docs with all run/paragraph properties and fields, placeholders, p:pic with crops and media relationships, p:graphicFrame tables (a:tbl with merges, cell fills/borders, table styles by id), charts (relationship passed to charts package via `ChartPartHook` as in xlsx), SmartArt (`dgm` relationships: if a `drawing` fallback part exists, import its shapes as a group; else import a placeholder rectangle with the diagram text and mark a warning), OLE/embedded objects as picture fallback + passthrough, p:grpSp recursive, p:cxnSp connectors with start/end links, media (video/audio) relationships, backgrounds, transitions (map every `p:transition` child to our list with fallback Fade + original preserved), timing trees (`p:timing` parsed into the animation model for supported effect presets `presetClass/presetID` mapping table; unsupported ones recorded in passthrough with a warning), notes slides (text), comments (`p:cmLst` and modern `p188:` comments) + authors, hidden slides, docProps, theme per master, color map overrides, handout/notes masters passthrough, vbaProject passthrough. Warnings for every unsupported item. Tests: every fixtures/pptx/features/* reads without error; expected structure assertions (slide count, placeholder texts, shape geometries, group nesting, table dimensions, transition types, animation counts); text equality with python-pptx extraction for all fixtures; fixtures/external if present read without exception.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/io-pptx test exits 0 with >= 70 tests
  - A test compares extracted text of every features/*.pptx with python-pptx output and passes
  - A test asserts animations of fixtures/pptx/features/animations.pptx yield the expected click-step timeline
  - pnpm typecheck exits 0

### WP-25 — Deck editor core: slide stage, thumbnail rail, shapes and transforms, text editing, alignment guides, Home/Insert/Design ribbons

- **Milestone:** M3  ·  **Depends on:** WP-06, WP-08, WP-23
- **Owns:** `packages/editor-deck/src/**`, `packages/editor-deck/test/**`, `packages/ui-kit/src/transformbox/**`, `apps/web/src/apps/deck/**`, `e2e/specs/deck-core.spec.ts`
- **Task:** Build editor-deck per §3.6: `SlideStage` (scaled 960×540 stage; renders shapes from deck-model with SVG preset geometry paths from ooxml presets, fills/gradients/pictures/lines/shadows, text bodies as static HTML from deckText with autofit computations measured via an offscreen measurer, tables, pictures with crop, groups, connectors), one live TipTap editor mounted on the focused text body (deckText schema; bullets via list styles; paragraph/run commands), `ui-kit/TransformBox` (new primitive: 8 handles, rotation handle, aspect lock with Shift, centre-resize with Alt, multi-selection bounding box, keyboard nudge/resize, touch-friendly sizes via a prop) used for every shape, smart guides and snapping (slide edges/centre, other shapes' edges/centres, equal-spacing), marquee selection, z-order, grouping, align/distribute menu, duplicate (Ctrl+D), copy/paste shapes and slides (internal clipboard JSON + image/text fallbacks), undo/redo, rulers/gridlines/guides toggles, zoom/fit, thumbnail rail (virtualised scaled stages, drag reorder, multi-select, section headers collapsible, context menu: new/duplicate/delete/hide/layout/section), notes pane (plain deckText editor), slide sorter view, outline view (titles/bullets text tree editable), Home/Insert/Design ribbons per §1.4 wired (new slide with layout gallery, layout change, reset, section, font/paragraph tools incl. align text and columns, Shapes gallery (all presets categorised like Office), Arrange, Quick Styles, Shape Fill/Outline/Effects, Shape Format contextual tab with size/position, Picture Format with crop/corrections-lite/alt text, Insert: pictures, text box, table (grid picker), icons (lucide set as SVG shapes), link, slide number/date/footer dialog, symbol, Design: themes gallery (12 bundled), variants, slide size, format background pane; Design Ideas pane listing deck-model `designIdeas` with thumbnails and one-click apply ★). Status bar: slide x of y, theme name, notes/comments toggles, view buttons, zoom, fit. apps/web/src/apps/deck: page composition, open via io-pptx (worker), save .scena until WP-26; `DeckCapabilities`. Tests: stage rendering (jsdom with SVG assertions), transform maths, snapping, autofit, rail reorder, outline edits; e2e deck-core.spec.ts: open fixtures/pptx/features/{placeholders,shapes,tables,groups}.pptx with screenshots, add a slide with a layout, insert a rectangle and resize via handle drag, type a title, apply a theme, Design Ideas apply; screenshots light/dark.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/editor-deck --filter @officina/ui-kit test exits 0 with >= 120 tests for deck/transformbox
  - pnpm e2e --grep deck-core exits 0 with screenshots deck-core/{placeholders,shapes,tables,groups,theme,design-ideas}-{light,dark}-desktop.png
  - e2e: dragging a shape's corner handle changes its width by the dragged distance (±2 px at 100 % zoom)
  - docs/qa/visual-review-WP-25.md written

### WP-26 — PPTX writer and round-trip fidelity tests

- **Milestone:** M3  ·  **Depends on:** WP-24
- **Owns:** `packages/io-pptx/src/write/**`, `packages/io-pptx/test/write/**`, `packages/io-pptx/test/roundtrip/**`
- **Task:** Implement `writePptx(presentation, {passthrough}) -> Uint8Array`: presentation.xml with id lists and section list, masters/layouts/slides with correct relationship graphs (slide→layout→master→theme, notesSlide↔slide, media, charts via charts package writer, embedded workbook parts for charts), spTree serialisation for every shape kind (preset and custom geometry, fills/lines/effects, text bodies with paragraph/run properties, fields, bullets, autofit values), placeholders with `p:ph` type/idx, tables, pictures with crops, groups (with chOff/chExt), connectors, backgrounds, transitions (our model → `p:transition` with `p14:` extensions where needed inside `mc:AlternateContent` like PowerPoint), timing trees for our animation model (generate PowerPoint-compatible `p:timing` with `p:par/p:seq/p:cTn` structure, presetClass/presetID/presetSubtype, `p:bldLst`), notes slides, comments (modern format) + authors, hidden slides, docProps, the 12 bundled themes serialised as theme parts, passthrough re-emission (SmartArt data/layout/colors/quickStyle parts and their relationships when the slide's shapes were not edited; vbaProject; handout masters; unknown parts). Deterministic output. Compatibility Checker issues (code blocks → text box with monospace, callouts → shapes). Round-trip tests over fixtures/pptx/features/* (structural ≥ 0.95, text 1.00, LibreOffice opens → PDF, visual ≥ 0.85 vs original render), python-pptx opens outputs and counts slides/shapes equal, new-presentation-from-theme test writes a .pptx that LibreOffice opens and whose slides have the expected layouts; `pnpm fidelity --format pptx` section in docs/qa/fidelity.md.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/io-pptx test exits 0 with >= 60 additional tests
  - pnpm fidelity --format pptx prints structural >= 0.95, text == 1.00, oracle_opens == 1, visual >= 0.85 for every features/* fixture
  - A test asserts a presentation with 3 animated shapes writes a p:timing tree that python-pptx can read back (slide.element contains p:timing with 3 p:cTn effect nodes)
  - docs/qa/fidelity.md contains the pptx table

### WP-27 — Deck features: slide show, presenter view, transitions, animations, Animation Pane, Slide Show & Transitions/Animations ribbons, media, equations

- **Milestone:** M3  ·  **Depends on:** WP-25, WP-20
- **Owns:** `packages/editor-deck/src/features/**`, `packages/editor-deck/test/features/**`, `apps/web/src/apps/present/**`, `apps/web/src/apps/deck/**`, `e2e/specs/deck-show.spec.ts`
- **Task:** Slide show engine (`/present/:docId` route, fullscreen): renders slides via SlideStage at viewport scale, click/keys/swipe navigation per §1.7 show keys, transitions implemented with the Web Animations API per §1.4 list (Fade, Push, Wipe, Split, Reveal, Cut, Shape, Uncover, Cover, Flash, Random Bars; Morph-lite P2 excluded), animations engine playing deck-model timelines (entrance/emphasis/exit effects per §1.4 as WAAPI keyframes with effect options; click steps; with/after previous; delays; auto-advance timers), laser pointer/pen/highlighter overlays (canvas; ink discarded on exit unless "keep annotations" → stored as freeform shapes), black/white screens, go-to-slide via number+Enter, end-of-show screen, hidden slides skipped. Presenter view: a second window (web: `window.open` + BroadcastChannel; Electron: host opens a second BrowserWindow on the secondary display via `PlatformHost.openPresenterWindow`, implemented in WP-34 — define the interface here with a web fallback) showing current/next slide, notes with font size controls, timer/clock, slide grid, pointer controls, with the audience window following. Transitions ribbon (gallery with live preview on the stage, effect options, duration, apply to all, advance options), Animations ribbon (gallery, effect options, Add Animation, Animation Pane with reorder/timing/triggers-lite, Animation Painter, preview button plays on stage), Slide Show ribbon (from beginning/current, present in browser tab, set up show dialog (loop, show without narration, pen colour), hide slide, rehearse timings (records per-slide durations into transitions), presenter view toggle), Insert ▸ Media (video/audio from file: HTML5 playback on stage and in show; written as pptx media parts), Insert ▸ Equation (MathLive editor producing MathML/LaTeX rendered to an SVG picture + OMML preserved when imported), Insert ▸ Chart (charts package with embedded workbook data editing via a mini grid dialog), Insert ▸ Code block (★), Reuse Slides pane (open another .pptx and drag slides in), Outline import (paste a Markdown/indented outline → slides ★), PNG/SVG export of current slide. e2e deck-show.spec.ts: open fixtures/pptx/features/{transitions,animations}.pptx → start show (F5) → screenshot first slide → advance (Space) through click steps asserting shape visibility changes → presenter view opened and screenshot (two pages via context) → Esc ends; Transitions gallery screenshot; Animation Pane screenshot.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/editor-deck test exits 0 with >= 90 additional tests
  - pnpm e2e --grep deck-show exits 0 with screenshots deck-show/{show-first,show-after-click,presenter,transitions-gallery,animation-pane}-light-desktop.png
  - e2e: in fixtures/pptx/features/animations.pptx slide 1, the second shape is hidden before the first click and visible after it
  - docs/qa/visual-review-WP-27.md written

### WP-28 — Deck integration gate: Office file loop, print/PDF handouts, Backstage, manual chapter  (GATE)

- **Milestone:** M3  ·  **Depends on:** WP-15, WP-26, WP-27
- **Owns:** `apps/web/src/apps/deck/**`, `packages/print-pdf/src/deck/**`, `e2e/specs/deck-e2e.spec.ts`, `docs/qa/visual-review-WP-28.md`, `docs/manual/deck.md`
- **Task:** Integration gate for Deck: wire io-pptx open/save (pptx, potx, pptm read, scena, pdf, png/svg per slide, outline .md export) with worker parsing and Compatibility Checker; Backstage Info/Export/Print; packages/print-pdf/src/deck: print layouts (full-page slides, handouts 2/3/4/6/9 per page with optional lines, notes pages, outline), preview in Backstage, PDF export via the print pipeline, PNG export of all slides as a zip (fflate). e2e deck-e2e.spec.ts: open fixtures/pptx/features/{placeholders,charts,shapes}.pptx → screenshots light/dark → edit (title text, add slide, insert chart, add animation) → Save As .pptx via the hook → re-read with io-pptx and python-pptx asserting slide count/title/animation → LibreOffice converts to PDF without error; print route (handouts 6 per page) page count equals ceil(slides/6); page.pdf() of the print route yields that page count; PNG export zip contains one PNG per slide. Manual chapter docs/manual/deck.md. Fix cross-feature defects; visual review.
- **Acceptance criteria:**
  - pnpm e2e --grep deck-e2e exits 0 with screenshots deck-e2e/*-{light,dark}-desktop.png
  - e2e: saved .pptx re-read by python-pptx reports the added slide and edited title; soffice converts it to PDF (exit 0)
  - e2e: handout PDF page count == ceil(slideCount/6)
  - pnpm verify exits 0 and CI is green; docs/qa/visual-review-WP-28.md and docs/manual/deck.md (>= 500 words) exist

### WP-29 — Collaboration and version history: Yjs documents, collab server, presence, comments sync, version history UI

- **Milestone:** M4  ·  **Depends on:** WP-15, WP-28
- **Owns:** `packages/collab/**`, `apps/collab-server/**`, `packages/editor-write/src/collab/**`, `packages/editor-deck/src/collab/**`, `packages/ui-kit/src/versions/**`, `packages/ui-kit/src/presence/**`, `apps/web/src/collab/**`, `e2e/specs/collab.spec.ts`, `docs/manual/collaboration.md`
- **Task:** Implement §3.7. packages/collab: `CollabDoc` wrapper over Y.Doc (docId, awareness, undo manager scoped per user, `y-indexeddb` persistence, `@hocuspocus/provider` websocket provider with reconnect/offline queue, connection status), `VersionStore` (dexie: snapshots on explicit save and every 10 min of edits, thinning to max 200, named versions, `diffVersions(a,b)` for Write producing word-level insert/delete runs), bundle (de)serialisation of Yjs updates into `.scriba/.scena` native files and a `yjs.bin` sidecar inside DOCX-saved sessions (stored in IndexedDB keyed by file hash so reopening a .docx restores history). Write: replace ProseMirror history with y-prosemirror (sync, cursors with user colours/names, yUndoPlugin) via the HistoryStrategy seam; comments stored in a Y.Map and anchored by Yjs relative positions; track-changes marks carry author ids from awareness. Deck: deck-model `toY/fromY` binding (slides Y.Array of Y.Maps; text bodies Y.XmlFragment bound to the mounted TipTap editor), awareness showing collaborators' selected slide/shape. Sharing: title-bar Share button → dialog with "Start live session" (creates docId session on the configured server from Options ▸ Collaboration: server URL + token), copy link; opening a `/write/:docId?collab=<server>` link joins. Presence avatars in the title bar (ui-kit/presence). Version History pane (ui-kit/versions; Review ▸ Version History and Backstage ▸ Info): list grouped by day, name a version, preview read-only, restore (as a new version), diff view for Write (side panel with insert/delete highlighting), export a version to file. apps/collab-server: Hocuspocus 4.7 server (`@hocuspocus/server` + `@hocuspocus/extension-sqlite` via better-sqlite3 or sql.js fallback) with `onAuthenticate` shared-token check, per-document size limits, a `/health` endpoint, Dockerfile, README (deployment, token, reverse proxy), `pnpm --filter @officina/collab-server dev`. Grid: not collaborative in v1; make sure the Share dialog says so and offers "send a copy". e2e collab.spec.ts: start the collab server in the test (child process on a free port), open the same Write document in two browser contexts, type in one, assert text appears in the other within 2 s, screenshot showing two cursors; version history: save twice, open pane, assert two versions, restore the first and assert text; offline test: disconnect context A (route abort websocket), type, reconnect, assert convergence.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/collab --filter @officina/collab-server test exits 0 with >= 50 tests
  - pnpm e2e --grep collab exits 0 with screenshots collab/{two-cursors,version-history,diff}-light-desktop.png
  - e2e: offline edits in one context converge after reconnect (both contexts show identical text)
  - docs/manual/collaboration.md explains self-hosting the server; docs/qa/visual-review-WP-29.md written

### WP-30 — Storage hosts, AutoSave to file, document recovery, recents, file associations

- **Milestone:** M4  ·  **Depends on:** WP-15, WP-22, WP-28
- **Owns:** `packages/storage/src/hosts/**`, `packages/storage/src/autosave/**`, `packages/storage/src/recovery/**`, `packages/storage/test/**`, `packages/ui-kit/src/recovery/**`, `apps/web/src/recovery/**`, `apps/web/src/hosts/**`, `e2e/specs/recovery.spec.ts`
- **Task:** Complete §3.8 for all three hosts at the interface level (Electron and Capacitor adapters are implemented here against the IPC/plugin contracts that WP-35/WP-36 provide on their side: define `window.officina` preload API types in packages/storage/src/hosts/electron/api.d.ts and the Capacitor plugin usage in hosts/capacitor; both must degrade gracefully when the bridge is absent so web tests keep passing). AutoSave: title-bar toggle (persisted per document), debounce 2 s after last edit, atomic writes (write temp + rename on Electron; createWritable on web), conflict detection via file mtime/hash (prompt: overwrite/save copy), background save in a worker for Office formats, save progress in the status bar, error toasts with retry. Journals: Grid CommandLog journal and Yjs updates appended to IndexedDB (dexie) with periodic compaction into snapshots; `closedCleanly` flag on beforeunload/visibilitychange; heartbeat timestamps. Recovery pane (ui-kit/recovery; shown at startup in Hub and in the app when relevant): lists recoverable documents with last-saved vs recovered times, Open, Save As, Delete, "Show original"; recovered documents open with a yellow bar "Recovered document — Save to keep". Recents: unified store with per-host handle persistence (web FS handles, Electron paths, Capacitor content URIs), pin/unpin, remove, open-location, "Recent" list in Backstage ▸ Open with search; file watching on Electron (external modification → reload prompt). File association plumbing: `openFromOS(payloadBytesOrPath)` entry used by Electron's `open-file`/argv and Capacitor's intent handling, routing to the right app by extension/magic. Tests: fake hosts; autosave timing with fake timers; journal compaction; recovery registry edge cases (crash during save). e2e recovery.spec.ts: edit without saving, reload the page without clean close (page.evaluate to skip the beforeunload handler), assert the Recovery pane lists the document and opening it restores the text; screenshot recovery pane light.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/storage test exits 0 with >= 60 additional tests
  - pnpm e2e --grep recovery exits 0 with screenshot recovery/recovery-pane-light-desktop.png
  - e2e: after simulated crash the recovered Write document contains the unsaved paragraph
  - pnpm typecheck exits 0 (Electron/Capacitor adapters compile against the declared bridge types)

### WP-31 — Accessibility, i18n (de, ar), RTL, high-contrast theme, Key Tips completeness

- **Milestone:** M4  ·  **Depends on:** WP-15, WP-22, WP-28
- **Owns:** `packages/i18n/locales/**`, `packages/ui-kit/src/a11y/**`, `packages/editor-write/src/a11y/**`, `packages/editor-grid/src/a11y/**`, `packages/editor-deck/src/a11y/**`, `e2e/specs/a11y.spec.ts`, `e2e/specs/rtl.spec.ts`, `docs/qa/a11y.md`
- **Task:** Deliver §3.10. Audit and fix ARIA across ui-kit/editors (this WP may edit ARIA/label/focus-related code anywhere inside packages/ui-kit, packages/editor-write, packages/editor-grid, packages/editor-deck, packages/i18n; keep changes confined to accessibility/i18n concerns and record other defects as handoffs): ribbon tabs/toolbar roving focus, menus, dialogs, galleries, palette; Grid ARIA grid shadow tree for active cell/row/col headers with announcements; Write editor region labelling, headings navigation via screen reader (ProseMirror DOM is semantic already; add aria-describedby for comments/revisions), Deck shapes list for keyboard selection (Tab cycles shapes; Enter edits text; arrow nudges), live region announcer service (`announce(message, politeness)`) used by status changes (mode, track changes, save, recalc); focus visible rings in all themes; high-contrast theme verified (contrast checks via a unit test over token pairs ≥ 4.5:1 for text, ≥ 3:1 for UI); `prefers-reduced-motion`. Key Tips: every ribbon command reachable via Alt sequences with Office letters where applicable (write the mapping table in packages/commands keytips files? No — those are owned by WP-06: put the completeness test here and a handoff for missing letters; the ribbon spec files in apps/web/src/apps/*/ribbon.ts carry `keyTip` fields and belong to the app integration WPs, so edit only the `keyTip` strings there, noting it in the commit). i18n: German and Arabic locale files for every namespace (machine-quality translations acceptable, reviewed for UI length), language switch in Options with restart-free reload, Arabic RTL: html dir flips ribbon/tab order/status bar, Write bidi paragraphs (`dir` attribute per paragraph, RTL default for Arabic locale new documents), Grid rightToLeft sheets (columns from right, headers mirrored), Deck RTL text. e2e a11y.spec.ts: axe on Hub, Write, Grid, Deck, Backstage, Options, Format Cells dialog, Palette — 0 serious/critical; keyboard-only flow: from Hub to Write, Alt,H,B bold via Key Tips, open palette with keyboard and run "Freeze Panes" in Grid. rtl.spec.ts: switch to Arabic, screenshots of the three apps (rtl/<app>-ar-light-desktop.png) and an Arabic paragraph typed in Write renders right-aligned. docs/qa/a11y.md documents the patterns and known gaps.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm e2e --grep 'a11y|rtl' exits 0; axe reports 0 serious/critical violations on all listed screens
  - screenshots rtl/{write,grid,deck}-ar-light-desktop.png exist and are reviewed in docs/qa/visual-review-WP-31.md
  - pnpm --filter @officina/i18n test exits 0 including a test that de and ar have every key en has
  - A unit test asserts token contrast ratios for all three themes

### WP-32 — Templates, Hub polish, onboarding tour, Office-synonym command palette, Options completeness

- **Milestone:** M4  ·  **Depends on:** WP-15, WP-22, WP-28
- **Owns:** `packages/templates/**`, `apps/web/src/apps/hub/**`, `apps/web/src/onboarding/**`, `packages/commands/src/synonyms.ts`, `packages/commands/test/synonyms.test.ts`, `e2e/specs/hub-templates.spec.ts`, `docs/manual/getting-started.md`
- **Task:** packages/templates: 12 templates per app authored as native model JSON + thumbnail PNGs rendered by a script (`packages/templates/scripts/render-thumbnails.mjs` using the e2e Playwright setup to open each template and screenshot the first page/sheet/slide at 400×300): Write (Blank, Letter, Resume, Report with TOC, Meeting Minutes, Invoice, Memo, Newsletter, Academic Paper (APA-like), Project Proposal, Cover Letter, Brochure-lite), Grid (Blank, Budget, Invoice, Loan Calculator, Project Tracker with CF, Timesheet, Expense Report, Inventory, Gantt-lite with CF bars, Gradebook, Sales Dashboard with charts, To-do), Deck (Blank 16:9, Blank 4:3, Pitch, Quarterly Review, Lesson, Status Report, Portfolio, Minimal Typographic, Photo Story, Workshop, Agenda, Thank You), each exercising real features (styles, formulas, charts, layouts) so they double as demo documents. Hub: template gallery with categories/search/preview dialog, "Blank" hero cards, recents with pin/remove/open-location, drag-drop, keyboard navigation, empty states, "What's new" card linking to CHANGELOG; first-run onboarding tour (3 cards per §1.9, dismissible, re-openable from Help ▸ Tour, stored in Options); "Coming from Office" cheat-sheet dialog (Ctrl+/ shortcut list grouped by app, printable). Command palette synonyms: curated `synonyms.ts` with ≥ 150 Office terms/phrases mapped to command ids (e.g. "Merge & Center", "Freeze Panes", "Track Changes", "Format Painter", "Slide Master", "Paste Special", "Text to Columns", "Page Setup") plus common misspellings; test ensures every synonym target exists. Options dialog completeness per §1.5 (General, Display, Proofing, Save, Language, Advanced, Keyboard, Privacy pages with real settings bound to stores). e2e hub-templates.spec.ts: Hub screenshots light/dark; open each template (36) and assert no console errors and screenshot the first view (templates/<app>-<name>-light-desktop.png); palette search "merge and center" in Grid resolves to the Merge & Center command. docs/manual/getting-started.md.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm e2e --grep hub-templates exits 0 and 36 template screenshots exist under e2e/screenshots/templates/
  - pnpm --filter @officina/commands test exits 0 including synonyms.test.ts with >= 150 synonyms
  - docs/qa/visual-review-WP-32.md reviews all template screenshots (each template must look finished: no overlapping text, no placeholder lorem beyond intended samples)
  - docs/manual/getting-started.md exists (>= 500 words)

### WP-33 — Notula (OneNote-lite)

- **Milestone:** M4  ·  **Depends on:** WP-14, WP-30
- **Owns:** `packages/notes/**`, `apps/web/src/apps/notes/**`, `e2e/specs/notes.spec.ts`, `docs/manual/notes.md`
- **Task:** Tier 2 app per §2.5 built on editor-write: notebook model (notebook → sections → pages; a notebook is a folder on disk via the storage host (desktop/Android) or an IndexedDB-backed virtual folder on web, with each page a `.scriba` plus `notebook.json` index), left navigation (notebook switcher, section tabs with colours, page list with drag reorder and sub-pages), page canvas = Write editor in web-layout mode with a title field and created/modified timestamps, tags (To Do with checkbox, Important, Question, Remember, custom with icons; Ctrl+1/2/3 shortcuts; Tags summary pane across the notebook grouped by tag with jump links), quick search across all pages (text index built incrementally, stored in IndexedDB), page templates (To-do list, Meeting notes, Lecture notes, Project overview), insert: table, picture, file attachment (stored next to the page), link to page (`[[` autocomplete ★), code block, date/time, equation; export page to .docx/.pdf/.md and notebook to a zip of Markdown; ribbon per §1.4 Notes; command palette and shortcuts; mobile layout handled in WP-36. e2e notes.spec.ts: create notebook/section/pages, tag a line To Do, tags summary shows it, search finds a page, screenshot light/dark, export a page to .docx and re-read with io-docx.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/notes test exits 0 with >= 40 tests
  - pnpm e2e --grep notes exits 0 with screenshots notes/{overview,tags}-{light,dark}-desktop.png
  - e2e: exported page .docx re-read by io-docx contains the page title as a Title-styled paragraph
  - docs/manual/notes.md exists

### WP-34 — Electron desktop shell and Linux packaging (AppImage, deb) + Windows/macOS builder config

- **Milestone:** M5  ·  **Depends on:** WP-30
- **Owns:** `apps/desktop/**`, `packages/storage/src/hosts/electron/**`, `tools/release/**`, `e2e/specs/electron.spec.ts`, `docs/manual/building.md`, `docs/manual/install-desktop.md`
- **Task:** apps/desktop with electron-vite 6: `src/main` (single instance lock; BrowserWindow per document with `contextIsolation`, `sandbox: true`, CSP, no nodeIntegration; loads apps/web build (file://) or dev server; native application menu built from the command registry/keymaps (File/Edit/View/Insert/Format/Tools/Window/Help with accelerators from §1.7; menu items dispatch to the renderer via IPC), `open-file`/argv handling → `openFromOS`, recent documents (`app.addRecentDocument`), file dialogs with filters, atomic fs writes, file watching (chokidar), `printToPDF` and native print, `shell.openExternal` with allow-list, presenter second window on the secondary display, auto-updater (electron-updater against GitHub Releases, check-on-start opt-in in Options), crash recovery mirror dir in `app.getPath('userData')/recovery`, protocol/file associations), `src/preload` exposing `window.officina` matching packages/storage's declared types with zod-validated IPC, `ElectronHost` implementation in packages/storage/src/hosts/electron. electron-builder config in apps/desktop/electron-builder.yml (appId io.github.zahakj.officina, productName Officina, artifact names `officina-<version>-linux-x86_64.AppImage`, `officina_<version>_amd64.deb`, `Officina-Setup-<version>.exe`, `Officina-<version>-portable.exe`, `Officina-<version>-arm64.dmg`, `Officina-<version>-x64.dmg`; linux: AppImage + deb with desktop entry, MIME types for docx/xlsx/pptx/odt/ods/odp/csv/scriba/tabula/scena, icon from build/icon.png; win: nsis + portable, `signAndEditExecutable: false` so Linux cross-builds don't need wine; mac: dmg + zip, `hardenedRuntime: false`, no notarisation; `publish: github`), `release:desktop` task script (`electron-builder --<platform> --publish never`) and `release:desktop` root script passthrough. Try a Windows cross-build locally (`--win`) — if it succeeds, record it; if it fails for lack of wine, record the exact error in docs/manual/building.md and leave Windows to CI (not a failure of this WP). docs/manual/install-desktop.md (AppImage/deb/exe/dmg, unsigned warnings as in abax's README, Gatekeeper `xattr -cr`), docs/manual/building.md (desktop + Android + web). e2e electron.spec.ts with Playwright `_electron`: skipped unless DISPLAY is set; when run: launches the built app, opens a fixture via IPC, screenshots electron/write-light-desktop.png.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm release:desktop -- --linux exits 0 and produces apps/desktop/dist/officina-*-linux-x86_64.AppImage and apps/desktop/dist/officina_*_amd64.deb
  - dpkg-deb -I apps/desktop/dist/officina_*_amd64.deb shows Package: officina and the MIME types in the desktop file (dpkg-deb -c lists usr/share/applications/officina.desktop)
  - ./apps/desktop/dist/officina-*.AppImage --appimage-extract into a temp dir contains usr/resources/app.asar (or resources/app.asar) and `npx asar list` shows index.html
  - if DISPLAY is set: pnpm e2e --grep electron exits 0 and the screenshot exists; otherwise the spec reports skipped and docs/manual/building.md states how to run it
  - pnpm verify exits 0; CI green

### WP-35 — Android shell (Capacitor): project, plugins, intents, signing, debug and release APK

- **Milestone:** M5  ·  **Depends on:** WP-30
- **Owns:** `apps/android/**`, `packages/storage/src/hosts/capacitor/**`, `tools/setup-android.sh`, `docs/manual/install-android.md`
- **Task:** apps/android: `capacitor.config.ts` (appId io.github.zahakj.officina, appName Officina, webDir ../web/dist, android scheme https, `server.androidScheme`), `npx cap add android` committing the generated android/ project (Gradle wrapper 8.14.3, AGP 8.13, compileSdk/targetSdk 36, minSdk 24), gradle.properties memory limits per §4.4, plugins installed and configured: @capacitor/filesystem, @capawesome/capacitor-file-picker, @capacitor/share, @capacitor/app (back button, intents), @capacitor/keyboard (resize mode `native`, accessory bar off), @capacitor/status-bar (theme-coloured, overlays off), @capacitor/preferences, @capacitor/haptics; AndroidManifest intent filters: ACTION_VIEW/ACTION_SEND for the Office/ODF/CSV/native MIME types and file extensions, `android:launchMode="singleTask"`, FileProvider for sharing exports; a small custom Capacitor plugin `OfficinaPrint` (Java) exposing `print()` using `PrintManager` + `WebView.createPrintDocumentAdapter` and `printToPdf` via `PdfDocument` fallback, consumed by packages/print-pdf through `PlatformHost.print`; `CapacitorHost` in packages/storage/src/hosts/capacitor (open via file picker with content URIs, read/write with Filesystem, SAF-persisted permissions, recents by URI with display names, share sheet, app data dir for notebooks/recovery); icons/splash via `@capacitor/assets` from docs/media/icon.svg renders (committed outputs); `pnpm --filter @officina/android build:web` (builds apps/web with `VITE_PLATFORM=capacitor`), `cap sync`, `release:android` task script that runs `./gradlew assembleRelease` and signs with `~/.officina-keys/release.jks` (generate per §4.4 if missing, store secrets with `gh secret set`), outputs `apps/android/dist/officina-<version>-android.apk`; also `assembleDebug`. Update tools/setup-android.sh if anything was missing. Static verification script `apps/android/scripts/verify-apk.sh` (aapt2 badging → package/version/minSdk/intent filters, apksigner verify, unzip -l for assets/public/index.html). docs/manual/install-android.md (sideloading, file access, known limits).
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm release:android exits 0 and apps/android/dist/officina-*-android.apk exists and is < 40 MB
  - bash apps/android/scripts/verify-apk.sh apps/android/dist/officina-*-android.apk exits 0 (package io.github.zahakj.officina, minSdk 24, targetSdk 36, intent filters include application/vnd.openxmlformats-officedocument.wordprocessingml.document, spreadsheetml.sheet and presentationml.presentation, apksigner verified, assets/public/index.html present)
  - gh secret list shows ANDROID_KEYSTORE_B64 and ANDROID_KEYSTORE_PASSWORD
  - cd apps/android/android && ./gradlew assembleDebug exits 0
  - pnpm verify exits 0; CI green

### WP-36 — Mobile UX adaptation (phone and tablet layouts, bottom ribbon sheet, touch handles) with Playwright mobile emulation

- **Milestone:** M5  ·  **Depends on:** WP-31, WP-32, WP-33
- **Owns:** `packages/ui-kit/src/mobile/**`, `apps/web/src/mobile/**`, `packages/editor-write/src/touch/**`, `packages/editor-grid/src/touch/**`, `packages/editor-deck/src/touch/**`, `e2e/specs/mobile.spec.ts`, `docs/manual/mobile.md`
- **Task:** Implement §1.8 for all apps using the same command registry and ribbon specs: responsive `AppFrame` variants (top app bar with back/title/undo/redo/search/overflow), `BottomToolbar` (context-aware quick commands derived from the ribbon spec's Home group priorities via a `mobilePriority` field added to RibbonSpec items in ui-kit types — set defaults so untouched specs still work) and `RibbonSheet` (bottom sheet with tab chips and stacked groups), keyboard-inset handling (`env(keyboard-inset-height)` + Capacitor Keyboard events via a host hook), safe-area insets, touch selection handles for Write (custom handles over ProseMirror selection; long-press selection; floating context toolbar), Grid touch (range handles at corners, tap/drag headers, pinch zoom via pointer events with inertia, formula bar docked above the keyboard with ✓/✕ and the quick keys row, bottom sheet filters/validation lists, sheet chip strip), Deck touch (vertical slide list portrait, full-width edit, enlarged TransformBox handles, pinch zoom, Present fullscreen landscape with swipe + tap zones + notes toggle), Notes mobile (drawer navigation), Hub mobile (cards list), Android back-button routing (`@capacitor/app` listener closes sheets/dialogs first), dialogs become full-screen sheets under 640 px, min hit targets 44 px (lint rule test over ui-kit primitives' CSS), hover-only affordances get tap equivalents. e2e mobile.spec.ts with Playwright devices 'Pixel 7' (412×915, touch) and 'iPad (gen 7)' landscape: each app opened on a fixture; screenshots mobile/<app>-{light,dark}-phone.png and mobile/<app>-light-tablet.png; interactions: open ribbon sheet, tap Bold, verify; Grid: tap cell, type via keyboard, ✓ commits; Deck: present and swipe to next slide; Hub recents.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm e2e --grep mobile exits 0 with screenshots mobile/{hub,write,grid,deck,notes}-{light,dark}-phone.png and mobile/{write,grid,deck}-light-tablet.png
  - e2e: on Pixel 7 the ribbon is not rendered as the desktop tab strip; the bottom toolbar and the ribbon sheet are present and functional (Bold applied)
  - docs/qa/visual-review-WP-36.md reviews every mobile screenshot for overlap/clipping; docs/manual/mobile.md exists
  - pnpm verify exits 0; CI green

### WP-37 — PWA: service worker, offline, install, file handlers, share target

- **Milestone:** M5  ·  **Depends on:** WP-30
- **Owns:** `apps/web/vite.config.ts`, `apps/web/public/**`, `apps/web/src/pwa/**`, `e2e/specs/pwa.spec.ts`, `docs/manual/install-web.md`
- **Task:** vite-plugin-pwa 2.0 with Workbox: precache the app shell and fonts (document fonts lazily cached on first use), runtime caching none for third parties (there are none), `manifest.webmanifest` (name Officina, icons from public/icons, theme/background colours per theme, display standalone, `file_handlers` for docx/xlsx/pptx/odt/ods/odp/csv/scriba/tabula/scena launching the right route via `launchQueue`, `share_target` (POST multipart → `/share-target` route that reads the file and opens it), shortcuts (New document/workbook/presentation)), update flow (new SW → toast "Update available — Reload"), offline page behaviour (Hub works offline; recents open from IndexedDB handles), install prompt button in Hub (beforeinstallprompt) and Backstage ▸ Account, `navigator.storage.persist()` request, OPFS used for large journals when available. Deployment: `apps/web` build is also published to GitHub Pages under `docs/app/` by the release workflow? No — keep Pages for the site/manual only; document self-hosting (`pnpm --filter @officina/web build` → static folder; any static server; HTTPS required for FS Access). e2e pwa.spec.ts: production preview server; SW registers; manifest has file_handlers; go offline (context.setOffline) and reload Hub → renders; update toast appears when the SW is swapped (simulate by rebuilding with a changed constant in test). docs/manual/install-web.md.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/web build produces dist/sw.js and dist/manifest.webmanifest containing file_handlers for 10 extensions
  - pnpm e2e --grep pwa exits 0 (offline Hub render passes)
  - Lighthouse-equivalent checks in test: manifest has >= 2 icons (192, 512), display standalone, start_url
  - pnpm verify exits 0; CI green

### WP-38 — Landing site and manual (GitHub Pages)

- **Milestone:** M5  ·  **Depends on:** WP-15, WP-22, WP-28
- **Owns:** `docs/index.html`, `docs/assets/**`, `docs/manual/index.html`, `docs/manual/README.md`, `docs/manual/index.md`, `docs/manual/faq.md`, `docs/manual/troubleshooting.md`, `docs/manual/formats.md`, `docs/manual/shortcuts.md`, `docs/manual/dev/**`, `docs/media/ribbon-*.png`, `docs/comparison.md`, `docs/FAQ.md`
- **Task:** Replace the placeholder landing page with the full site per §4.6, in the concord style (same token names, nav, hero with the banner, screenshot gallery placeholders referencing the exact paths WP-43 will produce (docs/media/screenshots/<app>-{light,dark}.png and phone-*.png), per-app sections (Write/Grid/Deck/Notes) with two figures each, a "Feels like home" section (ribbon crop images docs/media/ribbon-write.png etc. captured now with Playwright from the current build — WP-43 refreshes them — plus a shortcut table excerpt), a comparison table vs Microsoft 365 (rows: price, works offline, opens/saves DOCX/XLSX/PPTX, collaboration, version history, macros/VBA, platforms, licence, telemetry) with honest cells (e.g. macros: "preserved, not run"), download buttons for Windows/macOS/Linux/Android built from a `data-version` constant (v0.1.0 to be set by WP-43) linking to GitHub release assets with the exact artifact names of WP-35/WP-36, an install one-liner box with copy button, footer with links). Manual (docs/manual/*.md, rendered by the existing viewer): this WP writes index.md, faq.md, troubleshooting.md, formats.md (fidelity notes and the Compatibility Checker), shortcuts.md (link to SHORTCUTS.md), dev/architecture.md (condensed DESIGN.md), dev/testing.md and the README.md table of contents; it only LINKS to the chapters other WPs own (write.md, grid.md, deck.md from the gates; getting-started.md WP-32; notes.md WP-33; collaboration.md WP-29; building.md and install-desktop.md WP-34; install-android.md WP-35; mobile.md WP-36; install-web.md WP-37) and must not create or edit them even if they do not exist yet (WP-42 verifies all links). docs/comparison.md and docs/FAQ.md. Verify the deployed site after push: fetch https://zahakj.github.io/officina/ and the manual index. Keep every statement truthful for the current build; features not yet merged are not mentioned.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - curl -s https://zahakj.github.io/officina/ | grep -c 'Feels like home' >= 1 (after Pages rebuild; retry up to 10 minutes)
  - curl -s -o /dev/null -w '%{http_code}' https://zahakj.github.io/officina/manual/ prints 200
  - ls docs/manual/*.md | wc -l >= 14
  - A Playwright script in e2e (pwa or shell project) screenshots the landing page at 1280 width light and dark into e2e/screenshots/site/ and docs/qa/visual-review-WP-38.md reviews them

### WP-39 — Fidelity corpus expansion and differential testing against LibreOffice; reader/writer hardening

- **Milestone:** M6  ·  **Depends on:** WP-15, WP-22, WP-28
- **Owns:** `packages/testing/**`, `packages/io-docx/src/**`, `packages/io-xlsx/src/**`, `packages/io-pptx/src/**`, `packages/io-docx/test/**`, `packages/io-xlsx/test/**`, `packages/io-pptx/test/**`, `docs/qa/fidelity.md`, `docs/FORMATS.md`
- **Task:** Expand the corpus: 15 hand-authored `manual/` feature files per format created with LibreOffice headless macros or the Python oracles covering tricky constructs (nested tables, floating tables, multi-section headers, numbered headings with TOC, footnotes spanning pages, RTL mixed runs, equations; xlsx: shared formulas across thousands of cells, array formulas legacy, CF with formulas referencing other cells, 1904 dates, merged header blocks, very long text, rich-text cells, external link cached values, hidden sheets; pptx: SmartArt fallback, custom geometry, connectors with arrows, grouped placeholders, picture crops with transparency, tables with merged cells and styles, charts of every type, slide-level theme overrides), the `fixtures/external` fetch script run and results included in the score run (gitignored inputs; scores committed). Build the differential harness: for each fixture, (1) our read→write→LibreOffice PDF vs original→LibreOffice PDF visual score per page, (2) LibreOffice converting the original to our format family round trip (e.g. docx→odt→docx) as a baseline so we can show "LibreOffice's own round-trip score" next to ours, (3) exceljs/openpyxl/python-docx/python-pptx differential reads. Produce `docs/qa/fidelity.md` with tables per format and a ranked list of the worst files with root causes. Fix reader/writer defects until thresholds in §3.5 hold across the expanded corpus (prioritise by frequency of the construct in real documents); raise warnings coverage; make readers resilient to malformed files (fuzz 200 mutated zips/XMLs per format — must never throw outside a `FormatError` with a message). Update docs/FORMATS.md with supported/preserved/unsupported matrices per format.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm fidelity prints tables for docx/xlsx/pptx where every fixture (features/ and manual/) meets the §3.5 thresholds, and docs/qa/fidelity.md is regenerated
  - A fuzz test (600 mutated inputs) passes: readers throw only FormatError
  - docs/FORMATS.md contains a supported/preserved/unsupported matrix for each format
  - pnpm verify exits 0; CI green

### WP-40 — Performance pass: benchmarks, workers, progressive loading, memory

- **Milestone:** M6  ·  **Depends on:** WP-15, WP-22, WP-28
- **Owns:** `bench/**`, `packages/formula/src/engine/perf/**`, `packages/sheet-model/src/perf/**`, `packages/editor-grid/src/perf/**`, `packages/editor-write/src/pagination/perf/**`, `packages/editor-deck/src/perf/**`, `docs/qa/perf.md`, `e2e/specs/perf.spec.ts`
- **Task:** Create `bench/` (Vitest bench + Playwright timing specs) implementing every row of §3.9 as a reproducible measurement with fixture generators (500k-cell xlsx, 100k-formula workbook, 300-page docx, 200-slide pptx produced by tools/make-fixtures.py --large into fixtures/large, gitignored). Measure, then optimise within the perf-named directories (which may contain replacement implementations the owning packages import through a single seam file each: e.g. `packages/editor-grid/src/perf/index.ts` exports tuned renderer helpers; owning-package files outside perf/ may be edited only to switch to the seam — note it in the commit): formula engine (typed-array dependency storage, batch dirty marking, worker transfer batching), sheet-model (chunk pooling), grid renderer (dirty-rect redraw, glyph atlas for digits, offscreen canvas for headers), Write pagination (incremental re-layout of only affected pages, measurement batching, idle-time pagination of far pages, virtualised page DOM for Print Layout beyond ±5 pages), Deck (thumbnail generation throttling, static HTML caching for text bodies), parsing (streaming sheetData parse in the worker, transferable buffers), startup (code splitting per app route, font loading strategy, preloading of the Hub). Add memory measurements via `performance.measureUserAgentSpecificMemory` where available / Chromium `--enable-precise-memory-info`. Record before/after in docs/qa/perf.md; perf.spec.ts asserts the §3.9 targets with 20 % tolerance for CI noise.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm bench exits 0 and docs/qa/perf.md lists every §3.9 scenario with measured values meeting the targets (±20 %)
  - pnpm e2e --grep perf exits 0
  - Opening fixtures/large/300-pages.docx in e2e shows the first page in < 3 s (measured from navigation to first page paint)
  - pnpm verify exits 0; CI green

### WP-41 — ODF import/export (odt, ods, odp)

- **Milestone:** M6  ·  **Depends on:** WP-13, WP-15, WP-18, WP-22, WP-26, WP-28
- **Owns:** `packages/io-odf/**`, `apps/web/src/formats/odf.ts`, `e2e/specs/odf.spec.ts`
- **Task:** packages/io-odf: ODF 1.3 package handling (manifest.xml, mimetype first/uncompressed, content.xml, styles.xml, meta.xml, settings.xml passthrough), automatic vs common styles, `text:` elements → doc-model (paragraphs, spans, lists with `text:list-style`, tables, sections, headers/footers via master pages, footnotes, images (`draw:frame`), fields, tracked changes (`text:tracked-changes`), bookmarks, links), doc-model → ODT with the same coverage; `table:` → sheet-model (cells with office:value-type, formulas in `of:` syntax converted to Excel syntax and back (function names/separators), styles via `style:` number formats mapped to Excel codes, merges, column/row styles, conditional formats basic, validations, named expressions, charts via a ODF chart subset ↔ charts package) and back; `draw:page` → deck-model (frames, custom shapes with `draw:enhanced-geometry` mapped to presets where names match, text, pictures, tables, notes, transitions subset) and back. Register the formats in the apps' open/save pickers via apps/web/src/formats/odf.ts (a small registration module imported by each app's integration; the apps' own files are not edited — expose `registerOdf()` and call it from apps/web/src/main.tsx through a handoff note if an import line is needed). Tests: LibreOffice oracle round-trips — convert each docx/xlsx/pptx features fixture to ODF with soffice, read with io-odf, compare text/values with our OOXML reads; write ODF from our models and have LibreOffice convert back to OOXML, then read with our OOXML readers and compare (text 1.00, structural ≥ 0.85). e2e odf.spec.ts: open an .odt/.ods/.odp, screenshots, Save As .odt and re-read.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm --filter @officina/io-odf test exits 0 with >= 90 tests
  - pnpm fidelity --format odf prints text == 1.00 and structural >= 0.85 for the converted corpus
  - pnpm e2e --grep odf exits 0 with screenshots odf/{odt,ods,odp}-light-desktop.png
  - pnpm verify exits 0; CI green

### WP-42 — Integration gate: full suite verification, visual regression baselines, defect sweep, DoD checklist  (GATE)

- **Milestone:** M6  ·  **Depends on:** WP-29, WP-30, WP-31, WP-32, WP-33, WP-34, WP-35, WP-36, WP-37, WP-38, WP-39, WP-40, WP-41
- **Owns:** `e2e/**`, `docs/qa/**`, `**/* (defect fixes anywhere, as the only running WP)`
- **Task:** Runs alone. Execute the complete verification: `pnpm verify`, `pnpm e2e` (all specs, both themes), `pnpm fidelity`, `pnpm bench`, `pnpm release:desktop -- --linux`, `pnpm release:android`, APK verification script, PWA build, collab server start/health, licence check, `pnpm audit --prod` (document unfixable advisories). Open and visually review every screenshot produced (hundreds): record findings in docs/qa/visual-review-WP-42.md and fix all defects of severity "visible to a user in normal use" anywhere in the repo (you are the only running WP). Create visual regression baselines `e2e/screenshots/baseline/` for the gate screenshots (one per app/view/theme, ~40 files) and enable the pixelmatch comparison in CI (threshold per §5). Run the §10 Definition of Done checklist item by item and write docs/qa/dod.md with evidence (command + result) for each item; anything failing is fixed or, if impossible, listed in docs/qa/known-issues.md with a GitHub issue created via `gh issue create` (labels: bug/enhancement, milestone v0.2.0). Ensure the unlisted-feature rule holds: no "coming soon" stubs visible in release builds (grep for the `pending` tier marker in built output). Update parity/parity.json: flip every row whose delivering WP is complete and whose verify ids pass to done (or better where the notes justify it), run `pnpm parity:verify` and `pnpm parity`, and commit PARITY.md with the v0.1.0 score; rows whose WP shipped but whose verification fails stay partial/missing and get a known-issue entry. Update CHANGELOG.md Unreleased into a complete v0.1.0 section draft including the parity score line.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - pnpm verify && pnpm e2e && pnpm fidelity && pnpm bench all exit 0 in one session (log attached as docs/qa/gate-WP-42.log)
  - e2e/screenshots/baseline/ contains >= 40 PNGs and CI runs the visual comparison successfully
  - docs/qa/dod.md lists every §10 item with PASS and evidence; docs/qa/known-issues.md lists any deferred items with issue URLs
  - pnpm parity:verify exits 0 and PARITY.md shows >= 85 % of critical rows done or better
  - gh run list --workflow ci.yml --limit 1 --json conclusion -q '.[0].conclusion' prints success

### WP-43 — Release v0.1.0: real screenshots and demo GIF, README and landing finalised, tagged release with Linux and Android assets  (GATE)

- **Milestone:** M6  ·  **Depends on:** WP-42
- **Owns:** `docs/media/screenshots/**`, `docs/media/demo.gif`, `docs/media/ribbon-*.png`, `docs/media/banner*.png`, `README.md`, `docs/index.html`, `CHANGELOG.md`, `e2e/specs/media-capture.spec.ts`, `tools/make-demo-gif.sh`, `package.json (version field only)`, `apps/*/package.json (version field only)`, `packages/*/package.json (version field only)`
- **Task:** Runs alone, last. Media: e2e/specs/media-capture.spec.ts at device scale 2 captures with real template documents (from WP-32) — Write (light/dark), Grid with a chart (light/dark), Deck with Design Ideas pane (light/dark), Notes (light), Hub (light/dark), phone captures (Pixel 7) of Write/Grid/Deck, ribbon crops for each app — into docs/media/screenshots/ with the exact names referenced by README/landing; tools/make-demo-gif.sh records a 20–30 s Playwright video (open Hub → open template → edit with ribbon → palette → save) and converts it to docs/media/demo.gif with ffmpeg (palette-optimised, 960 px wide, 12 fps, < 8 MB). Finalise README.md (house style per §4.6; replace placeholders; add the parity badge from docs/parity-badge.json via shields endpoint URL; the install one-liner must point at the real asset names) and docs/index.html (`data-version` = v0.1.0, download links valid, screenshots in place), re-render banners if the icon changed (`pnpm media:render`). Versions: set 0.1.0 in every package.json (`pnpm -r exec npm version 0.1.0 --no-git-tag-version` or a script), CHANGELOG.md v0.1.0 section dated with the parity score and fidelity table. Build local assets: `pnpm release:desktop -- --linux` (AppImage, deb), `pnpm release:android` (signed APK); compute SHA256SUMS-local.txt. Commit, push, wait for CI green, then tag: `git tag -a v0.1.0 -m "Officina 0.1.0"` and `git push origin v0.1.0`; create the release first (so CI appends): `gh release create v0.1.0 --title "Officina 0.1.0" --notes-file <generated from CHANGELOG> apps/desktop/dist/officina-0.1.0-linux-x86_64.AppImage apps/desktop/dist/officina_0.1.0_amd64.deb apps/android/dist/officina-0.1.0-android.apk`; watch release.yml (`gh run watch`), confirm macOS/Windows assets and the final SHA256SUMS.txt were attached by the workflow (if a CI platform job fails for environmental reasons, fix the workflow and re-run with `gh run rerun`; the release must end with all six platform assets or a documented exception in the release notes). Verify Pages serves the finalised landing page and that every download button URL returns HTTP 302/200. Pin nothing. Post-release: bump versions to 0.2.0-dev in a final commit.
Finish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).
- **Acceptance criteria:**
  - gh release view v0.1.0 --json assets -q '.assets[].name' lists officina-0.1.0-linux-x86_64.AppImage, officina_0.1.0_amd64.deb, officina-0.1.0-android.apk, Officina-Setup-0.1.0.exe, Officina-0.1.0-arm64.dmg (or documented exception), SHA256SUMS.txt
  - for each asset URL from the release: curl -sIL returns 200
  - ls docs/media/screenshots/*.png | wc -l >= 14 and identify docs/media/demo.gif reports width 960 and size < 8 MB
  - curl -s https://zahakj.github.io/officina/ | grep -c 'v0.1.0' >= 1 and README.md no longer contains the word 'placeholder'
  - gh run list --workflow release.yml --limit 1 --json conclusion -q '.[0].conclusion' prints success



---

## 7. Parity matrix and the continuous parity loop (post-v0.1)

The initial milestones (§6) take Officina to v0.1.0. After that the build keeps running autonomously in **parity rounds** until the stopping goal in §7.4 is met. Both phases are driven by one machine-readable tracker.

### 7.1 Scope rule: "what a switcher would miss", not a 1:1 clone

The goal is most of the useful features people coming from Microsoft Office will or might miss, plus our own better features — not every corner of Office. Every parity row therefore carries a **would-miss tier** assigned from evidence (switcher complaints about LibreOffice/Google Docs/ONLYOFFICE gathered in §2A and re-scanned each round, Office usage patterns, "what I miss from Excel/Word" threads, support-forum frequency):

| Tier | Definition | Evidence used (examples from the 2026-10-15 scan) | Loop obligation |
|---|---|---|---|
| **critical** | Daily-use behaviour; its absence makes a switcher go back within a week | `.docx` style/font/text-wrap fidelity ("looks different in Word" is the #1 LibreOffice complaint); Excel keyboard habits (F2, F4, Ctrl+Arrows, Alt+=); Freeze Panes, Format Painter, Paste Special, Merge & Center, number formats, AutoFilter/sort, VLOOKUP/XLOOKUP/SUMIFS; track changes & comments; bullets/numbering/styles/TOC; slide layouts/masters, presenter view; large-file performance (Sheets' 50k-row slowness); print/PDF | must be **done** or **better** |
| **commonly-missed** | Used weekly by many, or by whole professions; switchers mention it when asked what they miss | Pivot tables (and conditional formatting on them), Power-Query-style import/transform, Goal Seek/Solver-lite, sparklines, data validation lists, charts beyond the basics, mail merge (small business), equations, citations, SmartArt *display*, Morph transition, rehearse timings, slide sections, localised function names/decimal commas (GridCraft ships Spanish), document comparison (Collabora), version history & real-time co-editing (Google), templates/Designer, read mode, dictation hooks | must be **done** or **better** |
| **niche** | Legacy or specialist; cheap ones may be done opportunistically | VBA/macros (preserved, never run), Access-style databases, OLE embedding, WordArt text effects, macro recorder, Power Pivot/DAX, XLM, legacy `.doc/.xls/.ppt` binary formats (import-only via LibreOffice conversion is acceptable), Information Rights Management, SharePoint/OneDrive-specific features, 3-D models, ink/pen input beyond annotations | **out of scope unless cheap**; never blocks a release |

Priority (`P0/P1/P2/out`) is the *delivery* ordering; the tier is the *importance to a switcher*. All critical rows are P0; commonly-missed rows are P0 or P1; niche rows are P2 or out.

### 7.2 The tracker: `parity/parity.json` → `PARITY.md`

Committed in the repo and seeded by WP-03 (§6), updated by every WP that changes a row's status, regenerated and validated in CI.

```jsonc
// parity/parity.json (excerpt)
{
  "version": 1,
  "updated": "2026-10-15",
  "weights": { "tier": { "critical": 3, "commonly-missed": 2, "niche": 0.5 },
               "status": { "missing": 0, "partial": 0.5, "done": 1, "better": 1 } },
  "rows": [
    { "id": "write.home.clipboard.format-painter", "app": "write", "area": "Home ▸ Clipboard",
      "feature": "Format Painter (single and double-click persistent)", "office": "Word ▸ Home ▸ Clipboard ▸ Format Painter",
      "tier": "critical", "priority": "P0", "status": "missing", "wp": "WP-14",
      "verify": ["e2e:write-features:format-painter", "unit:editor-write/features/format-painter"], "notes": "" },
    { "id": "grid.formulas.functions.lookup.XLOOKUP", "app": "grid", "area": "Formulas ▸ Lookup & Reference",
      "feature": "XLOOKUP with match/search modes", "office": "Excel 2021+ XLOOKUP",
      "tier": "critical", "priority": "P0", "status": "missing", "wp": "WP-09",
      "verify": ["conformance:lookup.csv#XLOOKUP"], "notes": "" },
    { "id": "grid.data.get-transform.power-query", "app": "grid", "area": "Data ▸ Get & Transform",
      "feature": "Query editor: import CSV/JSON/sheets with steps (filter, split, merge, unpivot) and refresh", "office": "Excel ▸ Data ▸ Get Data (Power Query)",
      "tier": "commonly-missed", "priority": "P1", "status": "missing", "wp": null,
      "verify": ["e2e:grid-query:unpivot"], "notes": "loop candidate; M-language not in scope" },
    { "id": "suite.macros.vba", "app": "suite", "area": "Automation",
      "feature": "Run VBA macros", "office": "Developer ▸ Macros",
      "tier": "niche", "priority": "out", "status": "missing", "wp": null,
      "verify": [], "notes": "vbaProject.bin preserved opaquely (row suite.macros.preserve is P0)" }
  ]
}
```

**Row schema** (validated with zod in `parity/schema.ts`): `id` (dotted, unique, stable), `app` (`write|grid|deck|notes|suite`), `area` (ribbon path or subsystem), `feature`, `office` (the Office name/location so switchers can search for it), `tier`, `priority`, `status` (`missing|partial|done|better`), `wp` (the WP that delivers it, or null), `verify` (list of verification ids: `e2e:<spec>:<test-title-slug>`, `unit:<package>/<path>`, `conformance:<csv>#<FUNCTION>`, `fidelity:<format>:<metric>>=<value>`, `manual:<docs/qa/...md#anchor>` — manual only for niche rows), `notes`. A `status` of `done`/`better` **requires at least one non-manual verify id that exists and passes**: `pnpm parity:verify` resolves ids to Vitest `-t` filters, Playwright `--grep` filters, conformance rows and fidelity thresholds, runs them, and fails CI if any `done` row's verification is missing or failing. `better` additionally requires a `notes` sentence stating what is better than Office.

**Required coverage of the seed** (WP-03 acceptance, ≥ 530 rows): every ribbon command of §1.4 for Write (≥ 160 rows), Grid (≥ 180 rows: ribbon commands + one row per function *family* with a `functions.json` sidecar listing every Excel worksheet function (≥ 520 names, each `planned|done|out`) that the family row aggregates), Deck (≥ 120 rows incl. every transition and animation of §1.4, every chart type, every layout type), Notes (≥ 30), Suite (≥ 40: file formats, print, autosave/recovery, collaboration, version history, templates, palette, themes, a11y, i18n/RTL, mobile, PWA, automation/MCP, AI hooks, security), plus explicitly-out rows (VBA, Access, OLE, IRM, 3-D models…) so the scope decision is visible. Rows that exist to be *better than Office* (palette, version history, Formula Lab, Design Ideas, Markdown, focus mode, code blocks, byte-preserving saves, MCP) are tagged `tier: "commonly-missed"` with `office: "—"` and can only be `better`.

**Score.** `score = Σ tierWeight·statusWeight / Σ tierWeight` over rows with `priority != out`; reported overall and per app, per tier (`critical`, `commonly-missed`, `niche`), with counts. `pnpm parity` regenerates `PARITY.md` (tables per app grouped by area with status glyphs, the scores, the "better than Office" list, and a changelog of status flips since the last tag) and `docs/parity-badge.json` (shields.io endpoint schema; README badge "parity: 71 %"). CI fails if `PARITY.md` is stale or the JSON invalid.

**Fidelity corpus as part of parity.** `parity/corpus.json` lists every fixture of `packages/testing/fixtures/**` with its expected checks (`{ "file": "docx/features/tables.docx", "checks": { "structural": 0.95, "text": 1.0, "visual": 0.9, "oracle_opens": true, "byte_identical_untouched": true } }`); `pnpm fidelity` reads it, and parity rows of kind `fidelity:*` reference it. New real-world files that break are added to the corpus *before* being fixed (regression-first).

### 7.3 The parity round (repeat until §7.4 holds)

Each round is a self-contained orchestration unit; its WPs use the same JSON shape as §11 and are stored in `rounds/round-NN.json` (never appended to this document's block, which is the v0.1 plan only).

1. **Gap analysis (one agent, alone; produces `rounds/round-NN.md` + `.json`).** Inputs: `parity/parity.json` (all `missing|partial` rows ranked by tier then priority), `docs/qa/fidelity.md` failures and the worst-10 files, `docs/qa/perf.md` misses, open GitHub issues labelled `bug`/`fidelity`/`parity` (user-reported files are gold: add them to the corpus), and a fresh **competitor re-scan** (WebSearch for each product in §2A.1 plus "new open-source office suite" queries; update `docs/COMPETITORS.md` with stars/versions/new claims and adjust the bars in §2A.3 if a competitor moved them). Output: a ranked gap list with evidence and the round's theme (e.g. "Round 1: automation + mail merge + function completeness").
2. **Select 8–15 WPs** sized like §6's (one agent session each, disjoint `owns`, explicit `depends_on`, acceptance criteria that name the parity rows they flip and the verify ids that will prove it). Always include: one fidelity WP (corpus growth + fixes), one competitor-bar WP (perf/a11y/polish), one "better than Office" WP, and the round's gate WP (runs alone: full e2e, fidelity, parity verify, visual review, baseline refresh). Rows a WP flips must be listed in its `task`.
3. **Implement / review / fix** with the §4.7 cadence; each WP updates its rows in `parity/parity.json` (status + verify ids) and `CHANGELOG.md`. The gate WP runs `pnpm parity:verify` and rejects any `done` row without a passing test.
4. **Release:** gate green → bump minor version (`v0.N.0`), regenerate `PARITY.md` and badge, update README/landing numbers (functions count, parity %, fidelity table), `git tag` + `gh release create` (release.yml attaches all platform assets as in WP-43), release notes = CHANGELOG section + "Parity: 63 % → 71 % (critical 88 %, commonly-missed 60 %)" + fidelity table.
5. **Repeat.** Round N+1's gap analysis starts from the new tracker.

**Seeded backlog for round 1** (from the §2A gaps; the round's gap analysis confirms or reorders): JSON control channel + MCP server + `officina` CLI (convert/edit/batch, agent skill file); AI provider integration (BYO key, off by default) with edits as tracked changes; function library to ≥ 520 with localised function names + decimal-comma locales; mail merge (data source = Tabula/CSV, fields, preview, output to one or many documents); Power-Query-lite query editor; equations P1 completion (OMML ↔ MathLive both ways in Scriba/Scena/Tabula); SmartArt import as editable groups; document compare (Scriba) with colour-coded authors; Morph-lite transition and motion paths; chart types to ≥ 18; Excel Solver-lite (linear/GRG via a small in-house optimiser); Grid collaboration (CommandLog ↔ Yjs bridge); iOS build in CI (Capacitor iOS on the macOS runner; unsigned IPA artefact); 10 more locales.

### 7.4 Stopping goal (the loop ends when all hold at a tagged release)

1. Every `critical` and every `commonly-missed` row is `done` or `better` (niche rows may remain `missing`/`partial`; `out` rows are excluded).
2. Fidelity bars of §2A.3 "loop target" column hold on the full corpus, including ≥ 25 real-world files per format contributed through issues.
3. Every competitor quality bar in §2A.3 "loop target" column is met by its named test.
4. Two consecutive rounds found no `critical` regressions and no new critical gaps in the competitor re-scan.

When the goal holds, the release is tagged `v1.0.0`; the loop then continues at a slower cadence (monthly rounds of maintenance, dependency updates via Dependabot, competitor re-scan, issue-driven fidelity fixes).


---

## 8. Effort estimate and schedule (≈ 2 concurrent agents)

One WP ≈ one strong agent session of focused work (4–8 h including tests, CI wait and fixes). Estimates below assume 2 agents, dependency stalls included (M0 is only partly parallel; gates run alone).

| Milestone | WPs | Agent-hours (est.) | Wall-clock at 2 agents | Notes |
|---|---|---|---|---|
| M0 Foundation | 9 (WP-01 … WP-09) | 60 | ~36 h | WP-01 alone (toolchain downloads ~2 GB); parity seed and OOXML core are long |
| M1 Write | 6 (WP-10 … WP-15) | 48 | ~28 h | pagination and docx writer are the heavy ones |
| M2 Grid | 7 (WP-16 … WP-22) | 56 | ~32 h | canvas grid, xlsx, charts in parallel pairs |
| M3 Deck | 6 (WP-23 … WP-28) | 44 | ~26 h | |
| M4 Cross-cutting | 5 (WP-29 … WP-33) | 36 | ~20 h | |
| M5 Shells & site | 5 (WP-34 … WP-38) | 34 | ~20 h | Android SDK/Gradle first build ~20 min; Electron builds ~10 min each |
| M6 Quality & release | 5 (WP-39 … WP-43) | 42 | ~30 h | two of them run alone |
| **Total v0.1.0** | **43** | **≈ 320 agent-hours** | **≈ 190 h wall-clock (~8 days continuous)** | add ~15 % for CI re-runs and lockfile rebases |
| Parity round (each) | 8–15 | 60–110 | 40–70 h | §7.3; expect 6–10 rounds to reach §7.4 |

Machine budget: ~60 GB free disk is enough (node_modules ≈ 2.5 GB, Playwright ≈ 0.5 GB, Android SDK + Gradle caches ≈ 4 GB, Electron caches ≈ 1.5 GB, fixtures ≈ 1 GB, builds ≈ 2 GB). RAM (7 GB) is the tighter constraint: never run an Electron packaging, a Gradle build and Playwright e2e at the same time — the orchestrator should treat WP-34/WP-35/WP-43 as "heavy" and not pair them with each other.

---

## 9. Risks and an honest statement of what this autonomous build can and cannot achieve

| # | Risk / limit | Likelihood · impact | Mitigation in this plan | Needs follow-up by a human? |
|---|---|---|---|---|
| 1 | **Perfect DOCX fidelity is unattainable**; complex layouts (floating tables, multi-column + footnotes, nested fields, legacy VML, exotic numbering) will differ from Word | certain · medium | Measured corpus with thresholds (§3.5, §7.2), passthrough so nothing is lost on save, Compatibility Checker, regression-first corpus growth from user files, font metric-compatibility table; the parity loop keeps raising the bar | Yes: real-world documents from users; donate failing files via the bug template |
| 2 | **Pagination ±1 line vs Word** depends on font metrics and justification/hyphenation rules we approximate in CSS | likely · medium | Metric-compatible fonts bundled; e2e compares page counts with LibreOffice; known-difference list in docs/FORMATS.md | Later: own line-breaking engine if the loop finds it necessary |
| 3 | **Own formula engine** (350 → 520 functions) may lag Excel semantics in edge cases | likely · medium | Conformance suite with LibreOffice-oracle generation + curated Excel examples; deviations documented; AGPL-admissible fallbacks (HyperFormula GPLv3, Univer engine Apache-2.0) if WP-07/WP-09 slip | No, unless the fallback is taken (then a licence note) |
| 4 | **Macros/VBA are never executed** | certain · low for most, high for niche users | `vbaProject.bin` preserved so files are not damaged; ribbon shows a clear explanation; parity rows marked `out` | Out of scope by decision |
| 5 | **Code signing / notarisation** cannot be done here (no certificates); Windows SmartScreen and macOS Gatekeeper warnings | certain · medium | Unsigned builds + documented workarounds (as `abax` does); `release.yml` has `CSC_*`/notarize hooks ready for secrets | Yes: buy certificates, add secrets, flip the hooks |
| 6 | **macOS and Windows binaries are built only in CI** and never launched by an agent | certain · medium | Electron + Chromium behave identically; Playwright Electron smoke runs on Linux; CI matrix builds on real runners; first macOS/Windows user reports go to the bug template | Yes: a human smoke test on each OS before announcing |
| 7 | **No Android emulator run** (static APK verification only; KVM exists but RAM is tight) | likely · medium | Capacitor is a WebView; mobile UX is e2e-tested in Chromium device emulation; APK verified with aapt2/apksigner; an optional emulator step is documented | Yes: install the APK on a real phone once per release |
| 8 | **iOS** is not in v0.1 (needs macOS + Xcode) | certain · low | Capacitor supports iOS; the loop's round 1 adds an unsigned IPA build on the macOS runner | Yes: Apple developer account for TestFlight/App Store |
| 9 | **Performance targets on a 7 GB machine** (1M cells, 300 pages) | possible · medium | Workers, canvas grid, virtualised pagination, benchmarks with tolerance; perf WP dedicated | No |
| 10 | **Agent concurrency = 2 and shared working tree**: lockfile races, accidental cross-WP edits | likely · low | Disjoint `owns`, catalog-pinned deps installed in WP-01, rebase rules (§4.7), handoff notes file | No |
| 11 | **Collab server auth is a shared token** in v1 | certain · medium for public deployments | Documented as not production-grade; pluggable `onAuthenticate`; loop adds OIDC | Yes for public hosting |
| 12 | **Spell-check languages, hyphenation, thesaurus** limited to en-US (+de/ar UI) | certain · low | Hunspell dictionaries are drop-in; loop adds locales | No |
| 13 | **Accessibility of a canvas grid** is inherently harder than DOM | likely · medium | ARIA shadow grid + announcements; axe + keyboard e2e; WCAG self-audit in the loop | Yes: testing with real screen-reader users |
| 14 | **Competitors move fast** (ArtCraft shipped three apps in a week; GenOffice/Univer are AI-agent-native) | certain · medium | Competitor re-scan each round; automation (MCP/CLI) seeded for round 1; our measurable fidelity is the moat | No |
| 15 | **Trademark/IP**: "Office", "Word", "Excel", "PowerPoint", "OneNote" are Microsoft marks | certain · low | Used only nominatively ("opens Word documents"); product names are Latin common nouns; no Microsoft icons/fonts/artwork; ribbon layout is functional, not copied artwork | Optional legal review before 1.0 |
| 16 | **Scope creep** from parity rounds | likely · medium | Would-miss tiers; niche rows cannot block releases; 8–15 WPs per round; stopping goal defined | No |

**What is realistic from this autonomous build:** a genuinely usable, good-looking suite on Linux (tested here), with CI-built Windows/macOS installers and a sideloadable Android APK; Office files round-trip with measured scores; the familiar ribbon/shortcut surface; collaboration and version history; a public repo, site and releases in the user's house style. **What needs people:** signing, store listings, real-device testing on macOS/Windows/Android/iOS, screen-reader user testing, and a stream of real-world documents to push fidelity past what synthetic corpora can reveal.

---

## 10. Definition of Done (v0.1.0)

All items are verified in WP-42 (`docs/qa/dod.md`) and must hold at the `v0.1.0` tag:

1. **Repo & release:** `ZahakJ/officina` public; CI green on `main`; `v0.1.0` release with AppImage, deb, APK (built locally), NSIS + portable exe and dmg (CI), `SHA256SUMS.txt`; README and landing page in house style with real screenshots and demo GIF; Pages live; licence files and notices complete; `pnpm licenses:check` clean.
2. **Open/save:** every fixture in `packages/testing/fixtures/{docx,xlsx,pptx}/features` opens and round-trips within §3.5 thresholds; untouched parts byte-identical; LibreOffice opens 100 % of our outputs; CSV/TSV/JSON import/export; ODF import/export passes WP-41 thresholds; PDF export of each app produces page counts equal to the on-screen layout.
3. **Scriba:** all P0 rows of §2.2 done; pagination page count within ±1 of LibreOffice on the corpus; track changes, comments, TOC, footnotes, tables, floating images, find/replace, styles, navigation pane, spell check, Markdown shortcuts, focus mode working in e2e.
4. **Tabula:** all P0 rows of §2.3 done; ≥ 350 functions with 0 conformance failures; 1M-cell scroll ≥ 55 fps and other §3.9 targets within tolerance; charts (13 types) round-trip; pivot tables, sparklines, Goal Seek, Subtotal, Flash Fill present (P1 delivered by WP-21).
5. **Scena:** all P0 rows of §2.4 done; 12 transitions and the listed animations play and round-trip; presenter view works in web and Electron; Design Ideas offers suggestions for every bundled layout.
6. **Notula:** notebooks/sections/pages/tags/search/export working per WP-33.
7. **Suite:** ribbon/Backstage/status bar/palette/Key Tips/mini toolbar on all apps; 100 % of the §1.7 shortcut table passes; three themes; en/de/ar with RTL; axe 0 serious/critical; autosave + recovery proven by e2e; collaboration (2 users, offline merge) and version history proven by e2e; 36 templates reviewed; onboarding tour; Options dialog complete.
8. **Shells:** Electron app launches on Linux (smoke when DISPLAY available), opens files from the OS, native menus with accelerators, print to PDF; Android APK passes static verification, intent filters registered, mobile layouts pass mobile e2e; PWA installs and works offline.
9. **Quality:** `pnpm verify`, `pnpm e2e`, `pnpm fidelity`, `pnpm bench`, `pnpm parity:verify` all green in one session; visual regression baselines committed; every gate's visual review recorded; `PARITY.md` regenerated with the v0.1.0 score (expected ≈ 60–70 % overall, ≥ 85 % of critical rows done) and no `done` row without a passing verification.
10. **Docs:** manual with ≥ 14 pages; DESIGN/FORMATS/SHORTCUTS/RELEASING/CONTRIBUTING/SECURITY current; CHANGELOG v0.1.0 section; known issues filed as GitHub issues; `docs/COMPETITORS.md` dated.
11. **Honesty:** nothing in README, landing page or UI claims a feature that is not verified by a test; unsupported features are visible in the Compatibility Checker and in `PARITY.md`.

---

## 11. Machine-readable work packages (v0.1.0 milestones only)

The array below is generated from the same source as §6; ids, dependencies and ownership are identical. Parity rounds (§7.3) produce their own `rounds/round-NN.json` files with the same shape and are not appended here.

```json
[
 {
  "id": "WP-01",
  "title": "Toolchain and monorepo scaffold",
  "milestone": "M0",
  "depends_on": [],
  "owns": [
   "package.json",
   "pnpm-workspace.yaml",
   "turbo.json",
   "tsconfig.base.json",
   "biome.json",
   ".nvmrc",
   ".npmrc",
   ".gitignore",
   ".editorconfig",
   "tools/setup-node.sh",
   "tools/setup-android.sh",
   "tools/verify-env.sh",
   "tools/check-licenses.mjs",
   "tools/oracle-soffice.sh",
   "apps/web/**",
   "apps/desktop/package.json",
   "apps/android/package.json",
   "apps/collab-server/package.json",
   "packages/*/package.json",
   "packages/*/tsconfig.json",
   "packages/*/vitest.config.ts",
   "packages/*/src/index.ts",
   "packages/*/README.md",
   "e2e/playwright.config.ts",
   "e2e/package.json",
   "e2e/specs/smoke.spec.ts",
   "docs/qa/env.md",
   "docs/adr/0001-stack.md"
  ],
  "task": "Run alone. Install the toolchain exactly as §4.1 and §4.3 (Node 24.22.0 into ~/.local/opt, pnpm 12.11.2, Playwright Chromium) and write the three setup scripts plus tools/verify-env.sh (checks node 24.x, pnpm, java 21, soffice, playwright chromium present; exits non-zero with a clear message otherwise). Run `tools/setup-android.sh` too (§4.4; ~1.5 GB download) so WP-35 finds the SDK ready; it must be idempotent. Create the monorepo described in §3.1: root package.json (packageManager pnpm@12.11.2, engines node >=24 <25, scripts: dev, build, lint, typecheck, test, e2e, bench, fidelity, verify (= turbo run lint typecheck test), licenses:check, licenses:notices, release:desktop (= turbo run release:desktop --), release:android (= turbo run release:android --), media:render, parity (= tsx parity/generate.ts), parity:verify (= tsx parity/verify.ts), competitors (prints docs/COMPETITORS.md date)), pnpm-workspace.yaml with `packages: [apps/*, packages/*, e2e]` and a `catalog:` containing every library in §4.2 at the pinned versions, turbo.json pipelines (build depends on ^build; test/lint/typecheck cached; e2e depends on build of apps/web; release tasks uncached), tsconfig.base.json (strict, ES2023 target, bundler resolution, jsx react-jsx, paths `@officina/*` -> packages/*/src), biome.json (recommended rules, 2-space, single quotes, 110 cols, organise imports), .npmrc (node-linker=hoisted, auto-install-peers=true, engine-strict=true), .nvmrc (24.22.0), .gitignore (node_modules, dist, out, .turbo, e2e/screenshots/** except baseline/, fixtures/external, android build dirs, *.local, .env). Create every package under packages/ listed in §3.1 (tokens, ui-kit, commands, doc-model, sheet-model, deck-model, formula, ooxml, io-docx, io-xlsx, io-pptx, io-odf, charts, editor-write, editor-grid, editor-deck, collab, storage, print-pdf, i18n, templates, notes, testing) each with package.json (name @officina/<pkg>, type module, exports ./src/index.ts, deps from catalog as listed for that package in §4.2 where obvious, scripts lint/typecheck/test), tsconfig.json extending base, vitest.config.ts (environment node, or jsdom for ui-kit/editor-*), src/index.ts exporting a `PACKAGE_NAME` const, one trivial test, and a README.md with the package purpose from §3.1. Create apps/web as a Vite 8 + React 19 + @vitejs/plugin-react 6 + @tailwindcss/vite app with index.html, src/main.tsx rendering a placeholder Hub page that shows the text \"Officina\" and the suite version, a vite.config.ts with the `@officina/*` aliases, and package.json scripts dev/build/preview/test. Create apps/desktop, apps/android, apps/collab-server with package.json only (name, private, placeholder `release:desktop`/`release:android` scripts that print \"not implemented\" and exit 1 — WP-35/WP-36 replace them). Create e2e/ as a Playwright project (playwright.config.ts starting `pnpm --filter @officina/web dev` on port 5173, Chromium only, screenshot on failure, outputDir e2e/screenshots) with smoke.spec.ts that loads / and screenshots `smoke/hub-light-desktop.png` asserting the text \"Officina\". tools/check-licenses.mjs implements the allow-list of §4.2 over `pnpm licenses list --json --prod` and a `--notices` mode that writes THIRD-PARTY-NOTICES.md content to stdout. tools/oracle-soffice.sh per §4.5 (creates the profile with the recalc settings on first run). Write docs/adr/0001-stack.md summarising the stack decisions of §0/§3 with the licence rationale, and docs/qa/env.md with the verified environment facts. `git init` with branch main and make the initial commit (no remote yet).\n\n\nEnds with a local commit only (WP-02 creates the remote).",
  "acceptance": [
   "export PATH=\"$HOME/.local/opt/node/bin:$PATH\"; node -v prints v24.22.0 and pnpm -v prints 12.11.2",
   "bash tools/verify-env.sh exits 0",
   "ls ~/Android/cmdline-tools/latest/bin/sdkmanager ~/Android/platforms/android-36 ~/Android/build-tools/36.0.0 succeeds",
   "pnpm install --frozen-lockfile=false succeeds and pnpm-lock.yaml exists",
   "pnpm verify exits 0 (lint, typecheck, test across all packages; each package has >= 1 passing test)",
   "pnpm --filter @officina/web build produces apps/web/dist/index.html",
   "pnpm e2e exits 0 and e2e/screenshots/smoke/hub-light-desktop.png exists",
   "node tools/check-licenses.mjs exits 0",
   "git log --oneline | wc -l >= 1 on branch main"
  ],
  "gate": false
 },
 {
  "id": "WP-02",
  "title": "GitHub repository, hygiene files, CI/release workflows, brand media, Pages placeholder",
  "milestone": "M0",
  "depends_on": [
   "WP-01"
  ],
  "owns": [
   ".github/**",
   "README.md",
   "LICENSE",
   "CONTRIBUTING.md",
   "SECURITY.md",
   "CODE_OF_CONDUCT.md",
   "THIRD-PARTY-NOTICES.md",
   "CHANGELOG.md",
   "docs/index.html",
   "docs/.nojekyll",
   "docs/assets/**",
   "docs/media/icon.svg",
   "docs/media/banner.svg",
   "docs/media/banner-light.svg",
   "docs/media/*.png",
   "docs/manual/README.md",
   "docs/manual/index.html",
   "docs/RELEASING.md",
   "docs/DESIGN.md",
   "docs/FORMATS.md",
   "docs/SHORTCUTS.md",
   "tools/render-media.mjs",
   "apps/desktop/build/icon.png",
   "apps/web/public/icons/**"
  ],
  "task": "Create the public repository and the house-style hygiene exactly as §4.6: git identity (noreply address), `gh repo create ZahakJ/officina --public --source=. --remote=origin --push` with description and homepage, topics, issues on, wiki off, delete-branch-on-merge; enable GitHub Pages from /docs on main via `gh api -X POST repos/ZahakJ/officina/pages`. Write LICENSE (AGPL-3.0 verbatim), README.md in the concord/astrolabe style (banner picture, badges with colour 3f6fe0, factual paragraphs, install one-liner that will be valid once v0.1.0 exists, placeholder screenshot grid referencing the paths WP-43 will fill, Features by section, Compared with Microsoft Office bullets, Building, Documentation table, Licence) — mark nothing as \"coming soon\"; describe the suite as it will be at v0.1.0 and keep the text truthful by WP-43's final edit. CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md (Contributor Covenant 2.1), THIRD-PARTY-NOTICES.md (from `node tools/check-licenses.mjs --notices` plus fonts/icons sections), CHANGELOG.md (Keep a Changelog, Unreleased), docs/DESIGN.md (condensed §3), docs/FORMATS.md (condensed §3.5 fidelity notes), docs/SHORTCUTS.md (the §1.7 table), docs/RELEASING.md (tag -> release.yml -> assets -> Pages; local Linux/APK build and upload commands). Issue templates (bug_report.yml, feature_request.yml, config.yml), PULL_REQUEST_TEMPLATE.md, CODEOWNERS, dependabot.yml. Workflows ci.yml and release.yml exactly as §4.6 (release.yml calls the root `release:desktop`/`release:android` scripts; android job tolerates a missing keystore secret). Brand media: docs/media/icon.svg (the qalam \"d\" mark, §1.1), banner.svg and banner-light.svg at 2000x600 following the §4.6 spec (reference: the concord banner is 2000x600; match its composition), and tools/render-media.mjs (Playwright Chromium screenshots of the SVGs, Inter from @fontsource-variable/inter inlined as a data URI in the SVG <style> so rendering is deterministic) that writes docs/media/banner.png, banner-light.png, social-preview.png (1280x640), icon PNGs (16, 32, 48, 64, 128, 192, 256, 512, 1024) into apps/web/public/icons/ and apps/desktop/build/icon.png (512). Landing placeholder docs/index.html with the full CSS token scheme and structure of §4.6 (nav, hero with banner, sections with headings and placeholder copy, comparison table, download buttons disabled with \"first release coming\" text, footer), docs/assets/inter.woff2 copied from node_modules/@fontsource-variable/inter/files/inter-latin-wght-normal.woff2, docs/manual/index.html (client-side Markdown viewer loading docs/manual/*.md via `marked`, vendored to docs/assets/marked.min.js with its MIT notice) and docs/manual/README.md (table of contents of the manual pages later WPs write). Push and watch CI until green.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "gh repo view ZahakJ/officina --json isPrivate,homepageUrl,repositoryTopics -q '.isPrivate' prints false and homepageUrl is https://zahakj.github.io/officina/",
   "gh api repos/ZahakJ/officina/pages -q .html_url prints https://zahakj.github.io/officina/",
   "git remote get-url origin contains ZahakJ/officina and git status shows the branch is up to date with origin/main",
   "gh run list --workflow ci.yml --limit 1 --json conclusion -q '.[0].conclusion' prints success",
   "test -f LICENSE && head -1 LICENSE matches 'GNU AFFERO GENERAL PUBLIC LICENSE'",
   "for f in README.md CONTRIBUTING.md SECURITY.md CODE_OF_CONDUCT.md THIRD-PARTY-NOTICES.md CHANGELOG.md .github/workflows/ci.yml .github/workflows/release.yml .github/ISSUE_TEMPLATE/bug_report.yml .github/ISSUE_TEMPLATE/feature_request.yml .github/ISSUE_TEMPLATE/config.yml .github/PULL_REQUEST_TEMPLATE.md .github/CODEOWNERS .github/dependabot.yml docs/index.html docs/.nojekyll docs/RELEASING.md; do test -f $f; done exits 0",
   "pnpm media:render exits 0 and `identify docs/media/banner.png` reports 2000x600, docs/media/banner-light.png 2000x600, docs/media/social-preview.png 1280x640, apps/desktop/build/icon.png 512x512",
   "curl -s -o /dev/null -w '%{http_code}' https://zahakj.github.io/officina/ prints 200 (allow up to 10 minutes after the push for Pages to build; retry)",
   "grep -c 'img.shields.io/badge/licence-AGPL--3.0--or--later-3f6fe0' README.md >= 1"
  ],
  "gate": false
 },
 {
  "id": "WP-03",
  "title": "Parity tracker seed (parity.json, PARITY.md generator, verify runner) and competitor dossier",
  "milestone": "M0",
  "depends_on": [
   "WP-02"
  ],
  "owns": [
   "parity/**",
   "PARITY.md",
   "docs/parity-badge.json",
   "docs/COMPETITORS.md",
   "docs/qa/would-miss-evidence.md"
  ],
  "task": "Create the parity tracker of §7.2 and seed it from this plan. parity/schema.ts (zod schema for rows, functions sidecar, corpus manifest; exported types), parity/parity.json with >= 530 rows covering: every ribbon command of §1.4 for Write (>= 160 rows), Grid (>= 180 rows; function *families* as rows with parity/functions.json listing every Excel worksheet function name (>= 520, from the official Excel function list by category: Compatibility, Cube (out), Database, Date and time, Engineering, Financial, Information, Logical, Lookup and reference, Math and trigonometry, Statistical, Text, Web, plus the dynamic-array/LAMBDA family) each marked planned|done|out with the WP that delivers it), Deck (>= 120 rows incl. every transition/animation/chart type/layout of §1.4), Notes (>= 30), Suite (>= 40) and explicit `out` rows for the niche scope decisions of §7.1 (VBA, Access, OLE, IRM, 3-D models, legacy binary formats, Power Pivot/DAX, macro recorder, WordArt effects, ink). Every row gets: stable dotted id, app, area, feature, the Office name/location, would-miss tier (assign from the evidence table you write in docs/qa/would-miss-evidence.md: cite §2A's switcher-complaint findings and add >= 10 further sources found with web search — LibreOffice/ONLYOFFICE/Google Sheets switching threads, \"what I miss from Excel/Word\" discussions, Office feature usage write-ups; each tier assignment must point at a reason), priority (P0/P1/P2/out consistent with §2 and the tier rules of §7.1), status `missing` (nothing is done yet — do not pre-mark), the WP from §6 that delivers it (null for loop candidates), and verify ids using the naming scheme of §7.2 that match the e2e/unit/conformance test names the §6 WPs are instructed to create (coordinate names with the WP texts: e.g. `e2e:write-features:format-painter`). parity/corpus.json listing every fixture file under packages/testing/fixtures with expected checks per §7.2 (the generator of WP-06 creates the files; list what it will create — names are fixed in §5). parity/generate.ts renders PARITY.md (per-app tables grouped by area, status glyphs, overall/per-app/per-tier scores with the §7.2 weights, the better-than-Office list, status flips since last tag via git) and docs/parity-badge.json (shields endpoint: label \"parity\", message \"<n> %\", color by band). parity/verify.ts: resolves verify ids to runners (`unit:` → vitest run with -t/path filter in the package, `e2e:` → playwright --grep on the spec, `conformance:` → checks the CSV row result file produced by the formula test run, `fidelity:` → reads docs/qa/fidelity.json produced by pnpm fidelity, `manual:` → checks the anchor exists and the row is niche) and fails on any done/better row whose verification is missing or failing; also fails if PARITY.md is stale (regenerate and diff) or ids are duplicated or a row's wp does not exist in PLAN.md's JSON block. Add a CI step in ci.yml? No — ci.yml is owned by WP-02: add the step by writing it into docs/qa/handoffs.md as a request AND, because WP-02 is complete, ask the orchestrator; instead make `pnpm verify` (root script, owned by WP-01 and already defined as turbo run lint typecheck test) unaffected and have parity validation run inside packages/testing's test suite (a test imports parity/schema.ts and validates parity.json + staleness), which CI already runs. docs/COMPETITORS.md: the §2A table rewritten as a dated document with the source URLs from the 2026-10-15 research (re-run the searches to capture URLs: GitHub API for star counts, release notes for ONLYOFFICE 9.4, LibreOffice 26.2, Collabora 26.04, Microsoft Agent Mode GA 2026-04-22, Google Gemini Workspace 2026-03-10, GenOffice, ArtCraft WordCraft/GridCraft/DeckCraft, Univer, Euro-Office), with a \"bars\" section mirroring §2A.3 and a re-scan checklist for the parity loop. README gets the parity badge only in WP-44 (do not edit README here).\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm parity exits 0 and regenerates PARITY.md; `node -e` count of rows in parity/parity.json >= 530 with >= 120 critical, >= 200 commonly-missed; parity/functions.json lists >= 520 function names",
   "pnpm parity:verify exits 0 (all rows are missing, so nothing to verify) and exits 1 when a test row is flipped to done without a verify id (demonstrated in a unit test in packages/testing or parity/)",
   "pnpm --filter @officina/testing test exits 0 including the parity schema/staleness test",
   "docs/COMPETITORS.md exists with >= 10 competitor entries each with a dated source URL; docs/qa/would-miss-evidence.md cites >= 15 sources",
   "Every `wp` referenced in parity.json is an id present in this PLAN.md JSON block (checked by parity/verify.ts)"
  ],
  "gate": false
 },
 {
  "id": "WP-04",
  "title": "Design tokens and UI primitives",
  "milestone": "M0",
  "depends_on": [
   "WP-01"
  ],
  "owns": [
   "packages/tokens/**",
   "packages/ui-kit/src/primitives/**",
   "packages/ui-kit/src/icons/**",
   "packages/ui-kit/src/theme/**",
   "packages/ui-kit/src/index.ts",
   "packages/ui-kit/test/**",
   "packages/ui-kit/stories/**"
  ],
  "task": "Implement §1.2. packages/tokens: `tokens.css` defining every --of-* custom property for light, dark and high-contrast (`[data-theme=light|dark|hc]` on <html>, default follows prefers-color-scheme), app accent switching via `[data-app=write|grid|deck|notes|hub]` setting --of-accent, spacing/radius/shadow/motion tokens, `@theme` block for Tailwind 4, a `tokens.ts` export with the same values typed, font-face registration for Inter Variable + JetBrains Mono (from fontsource, bundled) and the document fonts (Liberation Sans/Serif/Mono, Carlito, Caladea: download the OFL TTF/WOFF2 files from their upstream repositories with a `tools`-free script `packages/tokens/scripts/fetch-fonts.mjs` into packages/tokens/fonts/ and commit them; ~5 MB total; add their licences to THIRD-PARTY-NOTICES via the notices generator's extra-assets list in packages/tokens/fonts/NOTICES.json), plus `applyTheme()` / `useTheme()` helpers. packages/ui-kit/src/primitives: Button (variants: default, primary, ghost, toolbar, split), IconButton, ToggleButton, Menu/MenuItem/Submenu (Radix dropdown), Popover, Tooltip (with shortcut rendering), Dialog (modal, focus trap, Esc), Tabs, Select, Combobox (searchable, used for font family/size), NumberInput (with units pt/px/cm/in), ColorPicker (theme colours grid 10x6 like Office + standard colours + recent + custom via react-colorful), Slider (zoom), Toolbar (roving tabindex), SplitButton, Gallery (Office-style scrolling gallery with expand), Separator, Kbd, Toast, Spinner, VisuallyHidden, ScrollArea. All keyboard accessible with ARIA per §3.10. Icons: `Icon` component wrapping lucide-react plus the custom Officina Glyphs set (≥ 120 SVGs listed in §1.2, 20x20 viewBox, currentColor; create them as simple, clean strokes consistent with Lucide) exported by name from `packages/ui-kit/src/icons/glyphs/index.ts` with a generated `glyphs.test.ts` asserting every name referenced by §1.4 ribbon specs exists (write the list in `packages/ui-kit/src/icons/required-glyphs.ts`). Theme: `ThemeProvider` reading/persisting the user's choice to localStorage. Add a `stories/` folder with a single `Gallery.tsx` React page that renders every primitive in all three themes (used for screenshots, not Storybook) and export it; add a Vitest+jsdom test per primitive for render + keyboard behaviour.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/tokens --filter @officina/ui-kit test exits 0 with >= 25 tests passing",
   "test -f packages/tokens/fonts/Carlito-Regular.ttf (or .woff2) and Liberation*/Caladea* files exist with NOTICES.json",
   "grep -c -- '--of-accent-write' packages/tokens/tokens.css >= 2 (light and dark definitions)",
   "node -e \"import('./packages/ui-kit/src/icons/glyphs/index.ts')\" via `pnpm --filter @officina/ui-kit exec tsx -e` lists >= 120 glyph exports",
   "pnpm typecheck exits 0"
  ],
  "gate": false
 },
 {
  "id": "WP-05",
  "title": "OOXML core (OPC container, XML, DrawingML, themes, passthrough) and fixture corpus generator",
  "milestone": "M0",
  "depends_on": [
   "WP-01"
  ],
  "owns": [
   "packages/ooxml/**",
   "packages/testing/**",
   "tools/make-fixtures.py",
   "tools/fetch-corpus.sh"
  ],
  "task": "Implement §3.5's shared layer in packages/ooxml: `OpcPackage` (open from Uint8Array via fflate with zip-bomb limits; parts map; [Content_Types].xml defaults/overrides; relationships per part with typed lookup by relationship type; `getPart`, `setPart`, `removePart`, `addPart(name, contentType, bytes)`, `nextPartName('/word/media/image{n}.png')`, `save()` producing a zip with Office-compatible ordering ([Content_Types].xml first, then _rels/.rels)), XML layer over fast-xml-parser with preserveOrder (`parseXml`, `buildXml` with a stable namespace prefix table for w, a, p, r, c, x14, mc, wp, pic, wps, m, xdr, dgm, vt, cp, dc, dcterms; `xmlns` declarations emitted only when used; `mc:Ignorable` preserved), a tiny typed query helper (`child(node,'w:pPr')`, `children`, `attr`, `text`) and node builders, units (EMU, twips, half-points, eighths-of-a-point, percentages, `ST_` enums as string unions), DrawingML: `a:theme` parser/serialiser (clrScheme incl. system colours, fontScheme with major/minor latin/ea/cs, fmtScheme), colour resolution (`a:srgbClr`, `a:schemeClr` with lumMod/lumOff/tint/shade/alpha transforms, `a:sysClr`, `a:prstClr`), fills (solid, gradient linear/path, pattern, picture/blip with stretch/tile, none), lines (width, dash, caps, joins, head/tail ends), effects (outer shadow, glow, reflection → stored; shadow rendered), `a:bodyPr` and `a:lstStyle`/`a:pPr`/`a:rPr` text properties, and **preset geometries**: transcribe all 187 presets from ECMA-376 presetShapeDefinitions (guides, adjust values, paths with formula evaluation) into `src/drawingml/presets/*.ts` with `presetToSvgPath(preset, w, h, adjustments)`; test ≥ 40 presets against hand-computed path strings and all 187 for \"produces a non-empty closed path without NaN\". Passthrough: `PassthroughSet` that records parts and relationships a reader did not consume, re-emitted by writers. Document properties: core/app/custom properties read/write. packages/testing: fixtures directory layout (fixtures/docx, xlsx, pptx, each with `features/` generated and `manual/` hand-authored), `tools/make-fixtures.py` (Python venv per §4.5) generating the synthetic corpus described in §5 deterministically (fixed seeds, fixed dates) and `tools/fetch-corpus.sh` downloading 10 public-domain real-world files per format (e.g. from the ECMA-376 samples, Apache POI test data, LibreOffice unit-test corpora — list exact URLs; skip files that fail to download) into fixtures/external (gitignored). Oracles in packages/testing/src/oracles: `soffice.ts` (wraps tools/oracle-soffice.sh; convert to pdf/csv/docx/xlsx/pptx; `renderPdfPages(pdf) -> PNG buffers` via pdftoppm), `exceljsReader.ts`, `mammothReader.ts`, `pythonReader.ts` (runs small Python scripts with openpyxl/python-docx/python-pptx to dump JSON). Fidelity scorer skeleton `src/fidelity/score.ts` per §5 with `structural`, `text`, `visual` (pixelmatch), `oracle_opens`. Helper `loadFixture(name)`.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/ooxml test exits 0 with >= 60 tests (OPC round-trip of a generated docx/xlsx/pptx yields byte-identical part contents for untouched parts; theme colour resolution cases; 187 preset geometry tests)",
   "python3 -I tools/make-fixtures.py --out packages/testing/fixtures exits 0 and creates >= 15 .docx, >= 15 .xlsx, >= 15 .pptx files under features/",
   "bash tools/oracle-soffice.sh packages/testing/fixtures/docx/features/styles.docx pdf /tmp/officina-oracle exits 0 and the PDF exists",
   "pnpm --filter @officina/testing test exits 0 (oracle wrappers smoke-tested against generated fixtures; scorer unit tests)",
   "LibreOffice opens every generated fixture: a test converts each features/* file to pdf without error"
  ],
  "gate": false
 },
 {
  "id": "WP-06",
  "title": "Command registry, keymaps, Ribbon, Backstage, status bar, command palette, panes",
  "milestone": "M0",
  "depends_on": [
   "WP-04"
  ],
  "owns": [
   "packages/commands/**",
   "packages/ui-kit/src/ribbon/**",
   "packages/ui-kit/src/backstage/**",
   "packages/ui-kit/src/statusbar/**",
   "packages/ui-kit/src/palette/**",
   "packages/ui-kit/src/panes/**",
   "packages/ui-kit/src/minitoolbar/**",
   "packages/ui-kit/src/contextmenu/**",
   "packages/ui-kit/src/keytips/**",
   "packages/ui-kit/src/chrome/**",
   "packages/ui-kit/src/index.ts"
  ],
  "task": "packages/commands: `CommandRegistry` (`register({id, title, officeName?, synonyms[], icon, shortcut?, when?, run(ctx,args), isEnabled(ctx), isChecked?(ctx)})`, `execute(id,args)`, `getState(id)`, change subscriptions), `Keymap` (platform-aware parsing of \"Mod+Shift+L\", Mac ⌘ mapping, chord-free; conflict detection; user overrides persisted), the full §1.7 shortcut table as `keymaps/{write,grid,deck,common}.ts` mapping to command ids (commands that editors implement later are declared here as ids with titles so the palette/keytips/menus are complete from day one; unimplemented ids resolve to a no-op flagged `pending` and are hidden in release builds by `tiers.ts`), `KeyTips` sequences (Alt → letters per §1.3), `AssistantProvider` interface (§2.1) with a `NullProvider` and `EchoProvider`, tier flags (`tiers.ts`: P0/P1/P2 with env/build-time gating), telemetry-free `recentCommands` store, and `createOfficeSynonyms()` seed. packages/ui-kit chrome: `AppFrame` (title bar with mark, title rename, AutoSave toggle, undo/redo, Search/Tell-me box = palette trigger, presence slot, share button, window controls slot), `Ribbon` (data-driven from a `RibbonSpec` {tabs:[{id,label,keyTip,groups:[{id,label,launcher?,items:[...]}]}]}, item kinds: button, toggle, split, dropdown, gallery, combobox, color, numberinput, spacer, custom; Classic 3-row and Simplified 1-row layouts chosen by width; collapse (Ctrl+F1); contextual tab sets appearing with a coloured header; overflow handling; keyboard Tabs+Toolbar ARIA; tooltip shows title, shortcut, Office name when different), `Backstage` (full-screen overlay; the §1.5 navigation; content panes are slots so apps plug in Info/Print/Export), `StatusBar` (left items/right items, zoom slider with ± and percentage popover with presets, view switcher), `CommandPalette` (cmdk; fuzzy over title+officeName+synonyms; sections Recent/All; shows shortcuts; Ctrl+Shift+P and Alt+Q), `TaskPane` host (right/left docking, resizable, multiple panes tabbed), `MiniToolbar` (appears above selection on mouse-up), `ContextMenu`, `KeyTips` overlay, `DialogLauncher`, `Options` dialog shell with the §1.5 sections (General, Display, Save, Language, Keyboard with live remapping). Provide `RibbonSpec` builders and a `validateRibbonSpec()` test helper that checks every item's command id exists in the registry and every icon exists. Add a demo page `packages/ui-kit/stories/Chrome.tsx` wiring a sample spec. Tests: registry/keymap/keytips unit tests; jsdom tests for ribbon keyboard navigation, palette filtering, backstage open/close.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/commands --filter @officina/ui-kit test exits 0 with >= 60 tests",
   "A test asserts that every shortcut row of docs/SHORTCUTS.md (parsed from the markdown table in the repo) maps to a declared command id in packages/commands/src/keymaps",
   "pnpm typecheck exits 0",
   "Visual check: a jsdom-free Playwright script `pnpm --filter @officina/ui-kit exec playwright test` is NOT required here; instead `pnpm -w e2e` still passes"
  ],
  "gate": false
 },
 {
  "id": "WP-07",
  "title": "Formula engine core: tokenizer, parser, references, dependency graph, incremental evaluator",
  "milestone": "M0",
  "depends_on": [
   "WP-01"
  ],
  "owns": [
   "packages/formula/src/engine/**",
   "packages/formula/src/parser/**",
   "packages/formula/src/values/**",
   "packages/formula/src/index.ts",
   "packages/formula/test/engine/**",
   "packages/formula/test/parser/**",
   "packages/formula/bench/**"
  ],
  "task": "Implement §3.4 except the bulk function library. Tokenizer and Pratt parser for the full Excel grammar (A1/R1C1 parse-only for R1C1 in this WP, absolute/mixed refs, sheet-qualified and quoted sheet names, 3-D ranges, whole rows/columns, structured references with [#All]/[#Headers]/[#Data]/[#Totals]/[@Col]/[@] and column ranges, defined names, array constants, operators with Excel precedence (including unary minus before %, and `^` left-assoc as Excel), string/number/boolean/error literals, whitespace-as-intersection, union with comma inside parentheses, implicit intersection `@`, function calls with empty args). AST with source spans (for syntax highlighting + the Formula Lab). Reference canonicalisation and shifting (`moveFormula`, insert/delete rows/cols adjusting refs, copy with relative adjustment, `#REF!` on deletion), A1<->R1C1 conversion, absolute toggling (F4 cycle). Value model with ErrorValue, Array2D, Blank, Lambda; coercion rules (text->number per Excel incl. percentages, dates in text in en-US, booleans), comparison rules (text case-insensitive, type ordering number<text<boolean), Excel-style 15-significant-digit normalisation, date serial helpers (1900 with the Lotus bug, 1904 option). Evaluator with lazy arguments (annotate function descriptors with `lazy: [indices]`), dynamic arrays (spill area computation, `#SPILL!` on collisions, `#` spill-range operator), implicit intersection for legacy contexts, array lifting for scalar functions over arrays, volatile marking, LET/LAMBDA/MAP/REDUCE/SCAN/BYROW/BYCOL/MAKEARRAY semantics (these core functions are implemented here; the rest come in WP-09). `DependencyGraph`: cell and range nodes, interval tree per sheet so a range precedent is one edge, dirty propagation, Kahn topological recalculation of dirty subgraph, cycle detection with `#CIRCULAR` (represented as Excel's 0 + circular-reference flag), iterative calculation option (max iterations, max change). `Engine` API: `setCell(sheet,row,col, {formula?|value?})`, `insertRows/Cols`, `deleteRows/Cols`, `renameSheet`, `defineName`, `recalc()` (incremental) and `recalcAll()`, `getValue`, `getSpillRange`, `explain(formula) -> step tree` (for Formula Lab), host interface for reading cell values/types so the engine does not own storage. A built-in minimal function set of ~40 (SUM, AVERAGE, COUNT, COUNTA, MIN, MAX, IF, IFS, AND, OR, NOT, IFERROR, ROUND, ABS, INT, MOD, LEN, LEFT, RIGHT, MID, CONCAT, TEXTJOIN, VLOOKUP, XLOOKUP, INDEX, MATCH, ROW, COLUMN, OFFSET, INDIRECT, TODAY, NOW, DATE, SEQUENCE, FILTER, SORT, UNIQUE, TRANSPOSE, LET, LAMBDA) implemented natively with a `FunctionDescriptor` {name, minArgs, maxArgs, lazy, volatile, impl, docs} registry that WP-09 fills. Property-style tests for parse->print round-trips, reference shifting, and recalculation order; a micro-benchmark (vitest bench) for 100k dependent cells.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/formula test exits 0 with >= 200 tests",
   "A test file test/engine/spill.test.ts covers dynamic-array spill, #SPILL!, and the # operator",
   "A test asserts parse(print(ast)) round-trips for >= 300 formulas (including structured refs, 3-D refs, array constants, quoted sheet names)",
   "pnpm --filter @officina/formula bench reports recalc of a 100k-cell dependent chain under 1.5 s",
   "pnpm typecheck exits 0"
  ],
  "gate": false
 },
 {
  "id": "WP-08",
  "title": "Web app shell: routing, Hub, document sessions, storage abstraction (web host), i18n bootstrap",
  "milestone": "M0",
  "depends_on": [
   "WP-02",
   "WP-03",
   "WP-06"
  ],
  "owns": [
   "apps/web/src/**",
   "apps/web/index.html",
   "apps/web/vite.config.ts",
   "apps/web/public/manifest.webmanifest",
   "packages/storage/src/**",
   "packages/storage/test/**",
   "packages/i18n/**",
   "e2e/specs/hub.spec.ts",
   "e2e/specs/shell.spec.ts",
   "e2e/helpers/**"
  ],
  "task": "Build apps/web as the real shell: client router with routes /, /write/:docId, /grid/:docId, /deck/:docId, /notes/:docId, /present/:docId (no server), `AppFrame` + `Ribbon` + `StatusBar` + `Backstage` from ui-kit mounted per app with a placeholder editor area (`<div data-testid=\"editor-root\">`), app accent set via data-app, theme provider, command registry wiring (palette, key tips, shortcuts active), `DocumentSession` abstraction ({docId, app, title, dirty, handle?, model slot}) with a session store (zustand) and multi-document tabs/windows strategy (web: one document per tab; `window.name`-based docId handoff), Hub (§1.6) with recents/pinned from storage, template cards (placeholders until WP-32), \"Open\" via storage host, drag-and-drop of files onto Hub, file-type routing by extension/magic bytes. packages/storage: `PlatformHost` interface (§3.8), `WebHost` (File System Access API with permission re-prompt, IndexedDB-persisted handles via dexie; download fallback; `.scriba/.tabula/.scena` bundle read/write helpers), `RecentsStore`, `AutosaveJournal` interface (append(update), snapshot(), restore()) with an IndexedDB implementation, `RecoveryRegistry` (dirty/closedCleanly flags, heartbeat), `detectHost()` returning Web/Electron/Capacitor by probing `window.officina` / `Capacitor.isNativePlatform()` (adapters themselves arrive in WP-31/WP-35/34 — define the interfaces and a `NotImplementedHost` guard). packages/i18n: i18next + react-i18next + ICU, `locales/en/{common,ribbon,backstage,errors,formula}.json`, `t()` wrappers, `useLocale`, number/date formatting helpers via Intl, RTL detection (`dir` attribute), and a `missing-keys` test that scans ribbon specs and ui-kit strings for untranslated literals (allow-list file). Backstage Info/Print/Export panes get real property display (title, counts come from the editor via an `AppCapabilities` interface the editor WPs implement). Options dialog persisted (localStorage) incl. AutoSave interval, theme, language, Classic Office look. e2e: hub.spec.ts (Hub renders, theme toggle screenshot light+dark, recents list after opening a dropped fixture), shell.spec.ts (each app route shows ribbon with the §1.4 tab names in order, Ctrl+Shift+P opens palette and filters \"Freeze Panes\", Alt shows key tips, Ctrl+F1 collapses ribbon, Backstage opens via File and closes with Esc; screenshots per app light/dark at 1440x900 saved as shell/<app>-<theme>-desktop.png).\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm e2e exits 0 and the screenshots e2e/screenshots/shell/{write,grid,deck}-{light,dark}-desktop.png and hub/hub-{light,dark}-desktop.png exist",
   "e2e asserts ribbon tab labels for Write are exactly: Home, Insert, Layout, References, Review, View; Grid: Home, Insert, Page Layout, Formulas, Data, Review, View; Deck: Home, Insert, Design, Transitions, Animations, Slide Show, Review, View (File is the backstage button)",
   "pnpm --filter @officina/storage --filter @officina/i18n test exits 0 with >= 30 tests (fake IndexedDB via fake-indexeddb catalog entry)",
   "Visual review recorded in docs/qa/visual-review-WP-08.md after opening the PNGs"
  ],
  "gate": false
 },
 {
  "id": "WP-09",
  "title": "Formula function library (>= 350 functions), docs, conformance suite",
  "milestone": "M0",
  "depends_on": [
   "WP-07"
  ],
  "owns": [
   "packages/formula/src/functions/**",
   "packages/formula/test/conformance/**",
   "packages/formula/test/functions/**",
   "docs/formula-deviations.md"
  ],
  "task": "Fill the function registry to >= 350 Excel functions across the families of §3.4, using @formulajs/formulajs as the implementation base where its semantics match Excel (wrap with our value model: errors, blanks, arrays, lazy args) and native implementations where it does not (notably: TEXT/VALUE via numfmt, date functions with the 1900 system, SUMIF(S)/COUNTIF(S)/AVERAGEIF(S)/MAXIFS/MINIFS criteria parsing incl. wildcards and comparison strings, lookup functions with match modes and binary search, SUBTOTAL/AGGREGATE with hidden rows via host, financial functions on decimal.js where precision matters, the dynamic-array family, TEXTSPLIT/TEXTBEFORE/TEXTAFTER, LET/LAMBDA helpers already in WP-07). Each function gets a docs entry in `src/functions/docs/<FAMILY>.json` ({name, category, summary, syntax, args:[{name, description, optional, repeating}], examples:[{formula, result}], since}) used by autocomplete, the fx dialog and the Formula Lab; a test asserts every registered function has docs and every docs example evaluates to its stated result. Conformance: write `test/conformance/*.csv` with >= 1,500 rows (formula, expected, type, notes) in these sources: (a) curated from Microsoft's documented examples per function (type them in by family), (b) generated by `tools/gen-conformance.mjs` (owned here, under packages/formula/tools) which writes an .xlsx with formulas via exceljs, runs the LibreOffice oracle with recalc-on-load (tools/oracle-soffice.sh ... csv), and records values — flag rows where LibreOffice is known to differ from Excel in notes and exclude them from (b) by a deny-list. The conformance test runner compares with type-aware tolerance (numbers to 1e-9 relative, dates as serials, errors by name, text exact). Document genuine deviations in docs/formula-deviations.md with reasons. Implement `CELL` (subset: address, col, row, contents, type, filename blank), `INFO` subset, `ENCODEURL`, `HYPERLINK` returning a link value type.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/formula exec tsx -e \"import('./src/index.ts').then(m=>console.log(m.listFunctions().length))\" prints >= 350",
   "pnpm --filter @officina/formula test exits 0; conformance runner reports >= 1500 cases with 0 failures (skips are listed with reasons in docs/formula-deviations.md)",
   "A test asserts all function docs examples evaluate to their stated results",
   "Spot checks pass as tests: =TEXT(1234.5,\"#,##0.00\") -> \"1,234.50\"; =DATEDIF(DATE(2020,1,31),DATE(2020,3,1),\"md\") -> 1; =XLOOKUP(5,{1,3,5},{\"a\",\"b\",\"c\"}) -> \"c\"; =SUMIFS(A:A,B:B,\">=2024-01-01\") style criteria; =0.1+0.2=0.3 -> TRUE; =IRR({-100,30,40,50}) within 1e-7 of 0.0894"
  ],
  "gate": false
 },
 {
  "id": "WP-10",
  "title": "Write document model (ProseMirror schema, styles, numbering, sections, fonts)",
  "milestone": "M1",
  "depends_on": [
   "WP-01"
  ],
  "owns": [
   "packages/doc-model/**"
  ],
  "task": "Implement §3.3 doc-model: the ProseMirror schema (`createDocSchema()`) with every node/mark listed, attribute types in `src/types.ts` mirroring WordprocessingML semantics (units in twips/half-points as in the file, with helpers to px/pt), `StyleSheet` (paragraph/character/table/numbering/list styles, basedOn/next/link, docDefaults, latent styles ignored), `resolveParagraphProps(doc, paragraphNode)` and `resolveRunProps(...)` implementing Word's inheritance order (docDefaults → table style → paragraph style chain → numbering level pPr/rPr → direct formatting → run style), `Numbering` (abstractNum/num/overrides, level text templates like \"%1.%2\", formats decimal/lowerLetter/upperRoman/bullet/… with `formatNumber(level, counters)`, restart logic, legal numbering, `computeListLabels(doc)` producing labels per paragraph in document order), sections (page size/margins/orientation/columns/header-footer references/titlePg/evenOdd, section break types), fields (`FieldInstruction` parser for PAGE, NUMPAGES, DATE, TIME, TOC with switches, REF, HYPERLINK, SEQ, PAGEREF; unknown preserved), theme font/colour resolution via @officina/ooxml theme types, the **font substitution table** (§1.2; Office names → bundled fonts, with metric-compatible flag) and `fontStack(name)` for CSS, `DefaultTemplate` producing a new document with Word-2016+ defaults (Normal: Calibri→Carlito 11 pt, line 1.08 (259 lineRule auto), after 8 pt; Heading 1–3 as Word; Title, Subtitle, Quote, List Paragraph, Table Grid, TOC 1–3, Footnote Text, Header, Footer, Hyperlink, Strong, Emphasis, Caption), a plain-text and JSON (de)serialiser for tests/version diffs, document statistics (`countWords/characters/paragraphs/pages?(from layout callback)`), and a `DocCommands` set of pure transforms (apply style, toggle mark, set alignment, indent/outdent list level, insert table/row/col, merge/split cells, insert section break, set numbering) used by the editor. Tests: schema validity, inheritance resolution matrix (≥ 40 cases), numbering label computation (≥ 30 cases incl. restart/legal/multilevel), field parsing, default template snapshot.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/doc-model test exits 0 with >= 120 tests",
   "A test demonstrates resolveRunProps on a run in a styled paragraph in a styled table matches the documented Word precedence (table style < paragraph style < numbering < direct)",
   "pnpm typecheck exits 0"
  ],
  "gate": false
 },
 {
  "id": "WP-11",
  "title": "DOCX reader",
  "milestone": "M1",
  "depends_on": [
   "WP-05",
   "WP-10"
  ],
  "owns": [
   "packages/io-docx/src/read/**",
   "packages/io-docx/src/common/**",
   "packages/io-docx/src/index.ts",
   "packages/io-docx/test/read/**"
  ],
  "task": "Implement `readDocx(bytes) -> {doc: ProseMirrorNode, styles, numbering, sections, media, comments, footnotes, endnotes, settings, properties, passthrough, warnings[]}` per §3.5: every element listed there (body paragraphs/runs/breaks/tabs/symbols, rPr/pPr including `w:rPrChange/w:pPrChange`, tables with gridSpan/vMerge/tblLook/tblStyle/cell margins/widths in all unit types, section properties incl. headers/footers (map `w:headerReference` types to section header/footer nodes; `w:titlePg`, `evenAndOddHeaders`), numbering.xml, styles.xml, settings.xml (defaultTabStop, trackRevisions, decimalSymbol), footnotes/endnotes with references, comments + commentsExtended threads + people, simple and complex fields with cached results (nested fields), hyperlinks (relationship targets, anchors), bookmarks (start/end), inline and anchored drawings (wp:inline/wp:anchor with position/wrap/extent/docPr; pic:pic images; wps:wsp text boxes and shapes using ooxml DrawingML; `a:graphicData` charts referenced for WP-20 by relationship id; VML `w:pict` images read-only fallback), `w:ins/w:del/w:moveFrom/w:moveTo` revisions → insertion/deletion marks with author/date, `m:oMath` → equation node holding OMML XML, `w:sdt` content controls → unwrap content and keep properties in a `sdt` attribute (preserved on write by WP-13), RTL/bidi attributes, document properties. Media extracted to blobs with content types. Warnings for every unsupported construct with part/path info (used by the Compatibility Checker). Read must be streaming-friendly: run in a worker-compatible way (no DOM). Tests: each generated fixture in packages/testing/fixtures/docx/features/* yields a document whose text equals mammoth's extracted text (normalised), expected structure assertions per feature fixture (e.g. numbering labels \"1.\", \"a.\", \"i.\"; table 3x3 with merged cells; section count), no unhandled exceptions on every fixture in fixtures/external (when present), warnings list is empty for the synthetic corpus.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/io-docx test exits 0 with >= 80 tests",
   "A test compares normalised text of every fixtures/docx/features/*.docx to mammoth's output and passes",
   "A test reads a track-changes fixture and asserts insertion/deletion marks with authors",
   "pnpm typecheck exits 0"
  ],
  "gate": false
 },
 {
  "id": "WP-12",
  "title": "Write editor core: TipTap integration, pagination, rulers, Home/Insert/Layout ribbon wiring",
  "milestone": "M1",
  "depends_on": [
   "WP-06",
   "WP-08",
   "WP-10"
  ],
  "owns": [
   "packages/editor-write/src/**",
   "packages/editor-write/test/**",
   "apps/web/src/apps/write/**",
   "e2e/specs/write-core.spec.ts"
  ],
  "task": "Build the Write editor per §3.6: TipTap 3 editor with custom extensions for every doc-model node/mark (no StarterKit nodes that conflict with ours — use ours exclusively, with input rules), ProseMirror plugins: selection/cursor, history (later replaced by Yjs undo in WP-29 — keep a `HistoryStrategy` seam), paste handling (HTML sanitised with dompurify → doc-model via an HTML importer with style mapping; plain text; Markdown detection → styled; images as media), drag-and-drop images, tab key behaviour (list indent / tab char), smart typography (optional). Pagination layer (§3.6): PaginationPlugin measuring blocks via ResizeObserver + Range.getClientRects, page model {pages:[{top, height, breaks}]}, page chrome overlay (page edges, gap, margins guides, page numbers), per-page headers/footers rendered from section header/footer nodes with live PAGE/NUMPAGES fields, footnote areas, keepNext/keepLines/pageBreakBefore/widow-orphan rules, table rows cantSplit, column sections (CSS multicol per section block), progressive layout with requestIdleCallback for long documents, and `getPageCount()` / `pageOfPosition(pos)` used by the status bar and navigation. Views: Print Layout (paginated), Web Layout (continuous), Read Mode (paginated columns, no editing), Draft. Rulers (horizontal with margin/indent/tab stop drag; vertical with margins). Zoom (ctrl+wheel, slider, presets, page width/whole page). Status bar items: page x of y, word count (selection-aware), language, zoom. Ribbon wiring for Home/Insert/Layout/View per §1.4 (commands implemented here: all font/paragraph/style/clipboard/paste-special operations, insert table (grid picker), picture, link, page break, header/footer editing mode (double-click header area), page setup dialog (margins/orientation/size/columns), bullets/numbering galleries with Word's default definitions, line spacing, styles gallery with live preview and Styles task pane basics). Mini toolbar on selection; context menu. apps/web/src/apps/write: the Write page composing AppFrame/Ribbon/editor/panes/status bar, `WriteCapabilities` for Backstage Info, open/save through storage using WP-11 reader (writer arrives in WP-13 — until then Save writes the native .scriba bundle) with a worker for parsing. Tests: jsdom tests for extensions and commands; pagination tested with a fake measurer; e2e write-core.spec.ts: open fixtures/docx/features/{styles,numbering,tables,sections}.docx via the e2e file chooser, screenshot each (write-core/<name>-light-desktop.png), type text with Ctrl+B and verify bold mark, apply Heading 1 via ribbon and via Ctrl+Alt+1, insert a 3x3 table, check status bar page count > 1 for sections fixture, zoom slider changes scale.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/editor-write test exits 0 with >= 80 tests",
   "pnpm e2e --grep write-core exits 0 and screenshots write-core/{styles,numbering,tables,sections}-light-desktop.png exist",
   "e2e asserts the page count shown in the status bar for fixtures/docx/features/sections.docx equals the page count LibreOffice reports for the same file (via pdfinfo of the oracle PDF, exposed through a tiny e2e helper) within ±1",
   "docs/qa/visual-review-WP-12.md records the review of the screenshots"
  ],
  "gate": false
 },
 {
  "id": "WP-13",
  "title": "DOCX writer and round-trip fidelity tests",
  "milestone": "M1",
  "depends_on": [
   "WP-11"
  ],
  "owns": [
   "packages/io-docx/src/write/**",
   "packages/io-docx/test/write/**",
   "packages/io-docx/test/roundtrip/**"
  ],
  "task": "Implement `writeDocx(model, {passthrough}) -> Uint8Array` emitting every construct the reader understands (§3.5) with Office-compatible XML (ordering of child elements per the schema sequences — e.g. w:pPr children order, w:rPr order; `w:tblGrid`; `w:sectPr` placement in the last paragraph of each section except the final one in body), styles.xml/numbering.xml/settings.xml/footnotes/endnotes/comments(+Extended, +people)/headers/footers parts, media parts with relationships, content types, core/app properties (modified date updated, application \"Officina\"), fields as complex fields with cached results, revisions, bookmarks, hyperlinks, drawings (inline/anchor, pictures, text boxes, shapes via ooxml DrawingML writer), equations (OMML passthrough), sdt properties re-wrapping, and the passthrough parts (customXml, vbaProject.bin for .docm, glossary, unknown parts) with relationships repaired. Deterministic output (same model → identical bytes). Compatibility Checker data: `checkCompatibility(model) -> issues[]` listing features that will be lost (e.g. callout blocks → bordered paragraphs, code blocks → 'Code' style + shading). Round-trip tests: for every fixture read→write→read, structural score (packages/testing scorer) ≥ 0.95 and text 1.00; LibreOffice opens every written file (oracle_opens = 1) and the visual score between oracle renders of original and rewritten ≥ 0.90; writer output also opens with mammoth and python-docx without exceptions; a write-from-DefaultTemplate test produces a .docx whose styles.xml contains the Word default style ids. Write results to docs/qa/fidelity.md (docx section).\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/io-docx test exits 0 with >= 60 additional tests",
   "pnpm fidelity --format docx prints a table where every features/* fixture has structural >= 0.95, text == 1.00, oracle_opens == 1, visual >= 0.90",
   "A determinism test asserts identical bytes for two consecutive writes of the same model",
   "docs/qa/fidelity.md contains the docx table"
  ],
  "gate": false
 },
 {
  "id": "WP-14",
  "title": "Write advanced features: tables UI, floating objects, find/replace, styles & navigation panes, TOC, footnotes, comments, track changes, Format Painter, Markdown shortcuts, focus mode, spell check",
  "milestone": "M1",
  "depends_on": [
   "WP-12"
  ],
  "owns": [
   "packages/editor-write/src/features/**",
   "packages/editor-write/test/features/**",
   "apps/web/src/apps/write/**",
   "e2e/specs/write-features.spec.ts"
  ],
  "task": "Add to editor-write (new `features/` modules, each registering its commands and ribbon contributions through a `WriteFeature` interface the core exposes): Tables (Table Design/Layout contextual tabs: styles gallery with banded/header options from doc-model table styles, borders/shading, insert/delete rows/cols, merge/split, cell size, distribute, alignment, repeat header rows, convert to text, autofit; cell selection with mouse/keyboard; column resize handles on the ruler and the table), Floating objects (Picture/Shape Format: wrap modes square/tight/top-bottom/behind/in-front implemented with float spacers and shape-outside; position dialog; anchor dragging; crop; alt text; bring forward/backward; Selection Pane), Find & Replace (navigation pane results, match case/whole word/regex/wildcards, replace all, Go To page/section/line), Styles pane (list, modify style dialog, new style, clear all, show preview), Navigation pane (headings tree with drag reorder, pages thumbnails (rendered lazily), results), References: TOC insertion/update from heading styles with page numbers from pagination, clickable; footnotes/endnotes insert/navigate/show; captions with SEQ; cross-references dialog; bookmarks. Review: comments task pane (threaded, resolve, reply, @mention stub) anchored to ranges; Track Changes (insert/delete/format revisions recorded as marks with author/time, display modes Simple/All/None/Original, Reviewing pane, accept/reject one/all, next/previous, balloons in margin in All Markup), Word Count dialog, Spelling via nspell + dictionary-en (squiggles as decorations, right-click suggestions, add to dictionary, ignore). Home: Format Painter (single and double-click persistent; Ctrl+Shift+C/V), Change Case, Clear Formatting, Sort paragraphs, Show ¶, Paste Special dialog. Markdown shortcuts (input rules: #..###### headings, -, *, 1., [ ] task, >, ```, ---, **bold**, *italic*, `code`, links), smart paste of Markdown text, Focus mode (hide chrome, centre page, typewriter scrolling, word-goal from Options), Callout and Code blocks (code block with syntax highlighting via a small tokenizer for 8 languages; both have Compatibility Checker mappings). Wire everything into the Write ribbon spec (apps/web/src/apps/write/ribbon.ts) per §1.4 including contextual tabs. e2e write-features.spec.ts: screenshots of track-changes view, comments pane, TOC inserted, floating image with square wrap, find/replace dialog, focus mode (light+dark); functional assertions: replace-all count, accept-all removes marks, TOC entries equal heading count, Format Painter copies bold+colour.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/editor-write test exits 0 with >= 120 additional tests",
   "pnpm e2e --grep write-features exits 0 with screenshots write-features/{track-changes,comments,toc,floating-image,find-replace,focus}-{light,dark}-desktop.png",
   "e2e: Ctrl+Shift+E toggles track changes and the status bar shows 'Track Changes: On'",
   "docs/qa/visual-review-WP-14.md written after viewing all screenshots"
  ],
  "gate": false
 },
 {
  "id": "WP-15",
  "title": "Write integration gate: open/edit/save Office files end to end, print & PDF export, Backstage panes",
  "milestone": "M1",
  "depends_on": [
   "WP-13",
   "WP-14"
  ],
  "owns": [
   "apps/web/src/apps/write/**",
   "packages/print-pdf/src/core/**",
   "packages/print-pdf/src/write/**",
   "packages/print-pdf/src/index.ts",
   "packages/print-pdf/package.json",
   "packages/print-pdf/test/core/**",
   "packages/print-pdf/test/write/**",
   "e2e/specs/write-e2e.spec.ts",
   "e2e/specs/print.spec.ts",
   "docs/qa/visual-review-WP-15.md",
   "docs/manual/write.md"
  ],
  "task": "Integration gate for Write. Wire the DOCX writer into Save/Save As (format picker: .docx, .scriba, .pdf, .md, .html, .txt; Compatibility Checker dialog before saving to .docx when issues exist, with \"Don't show again\" per document), AutoSave-to-file, dirty tracking, title rename → file rename, recents, \"Open Recent\" and drag-drop at the Write route. packages/print-pdf (create the package core in src/core: page model, print route host, preview component, pdf-lib post-processing; Write-specific layout in src/write; Grid/Deck add src/grid and src/deck later): print layout route (`/print/:docId` renders the paginated document using the same page model with @page CSS, headers/footers/footnotes per page, no chrome), Print preview inside Backstage ▸ Print (iframe of the print route with page thumbnails, settings: printer (browser), pages range, copies, orientation, paper size, margins, scale), `exportPdf()` pipeline: `PlatformHost.print({toPdf:true})` for Electron (printToPDF) and `window.print()` for web with instructions; post-processing with pdf-lib (title/author metadata, outline from headings, page labels) when the host returns bytes; Markdown and HTML exporters (doc-model → Markdown with tables, images, footnotes; HTML with inline styles). Backstage Info shows properties and version list placeholder (WP-29 fills). Test the whole loop in e2e write-e2e.spec.ts: open fixtures/docx/features/styles.docx → edit (type, bold, heading, insert table) → Save As docx via the e2e download hook → the saved file is re-read by io-docx in the test and contains the edits → LibreOffice converts the saved file to PDF without error; print.spec.ts: the print route renders N pages where N equals the status bar page count, screenshot print/write-print-preview-light-desktop.png; `exportPdf` in Chromium headless via Playwright's page.pdf() of the print route produces a PDF with N pages (pdfjs-dist page count). Write the Write chapter of the manual (docs/manual/write.md): opening/saving, ribbon tour, track changes, TOC, keyboard shortcuts link.",
  "acceptance": [
   "pnpm e2e --grep 'write-e2e|print' exits 0",
   "e2e: saved .docx re-read by io-docx contains the edited text and a Heading 1 paragraph; soffice converts it to PDF (exit 0)",
   "e2e: PDF produced from the print route has the same page count as the status bar shows for fixtures/docx/features/sections.docx",
   "pnpm verify exits 0 and CI is green",
   "docs/qa/visual-review-WP-15.md lists every Write screenshot reviewed and defects fixed; docs/manual/write.md exists with >= 600 words"
  ],
  "gate": true
 },
 {
  "id": "WP-16",
  "title": "Sheet model: workbook/sheets/sparse cell store, styles, merges, names, command log, formula binding",
  "milestone": "M2",
  "depends_on": [
   "WP-07",
   "WP-09"
  ],
  "owns": [
   "packages/sheet-model/**"
  ],
  "task": "Implement §3.3 sheet-model: Workbook/Sheet/SparseStore/RowChunk exactly as specified with O(1) random access, iteration in row-major order, `getUsedRange`, `forEachInRange`, bulk `setRange(values2D)`, row/col property arrays with default sizes (15 pt rows, 8.43 char columns → px via font metrics), hidden/outline levels, merges (with validation and split), `StyleTable` (numFmt code, font, fill, border per side with styles, alignment, protection; dedupe by hashing; `cellXfs`-like indices), named ranges (workbook/sheet scope, hidden names), tables (ListObjects: name, range, header/totals rows, column names, style, autofilter), conditional formatting rule model (all §2.3 types with priority/stopIfTrue and dxf styles), data validation model, hyperlinks, comments/notes, sheet views (freeze, split, zoom, selection, gridlines, headings, rightToLeft), print settings, autofilter state, charts/drawings anchors (objects referenced by id; chart model from WP-20 later), workbook properties (date1904, calcMode). `CommandLog` with inverse operations for every mutation (setCell(s), setStyle, insert/delete rows/cols (shifting merges, names, CF, validations, tables, formulas via engine), move/copy range, sort, rename sheet, add/remove/reorder sheets, freeze…) with undo/redo stacks, grouping of commands into transactions, and serialisation of commands. Formula binding: implement the @officina/formula host interface over the store; `WorkbookCalc` runs the engine (in-process here; a worker wrapper comes in WP-17) with incremental recalc, spill ranges, calc mode auto/manual, `recalcSheet`, structured reference resolution through the tables model, defined names, `explain(cell)`. Number formatting bridge: `formatCell(value, numFmtCode, locale)` via numfmt including Excel's General format rules (11 significant digits display, scientific switch), date/time formats, percentages, fractions, accounting with alignment padding markers, colours in format codes (`[Red]`), conditional sections. Snapshot/restore (structured clone-safe JSON) for versions/journals, with gzip via fflate. Tests: store semantics, insert/delete shifting matrices (≥ 50 cases), undo/redo invariants (property tests), number formatting (≥ 150 cases against numfmt's own expectations plus Excel-documented examples), structured references, calc integration (dependent cells recalc, spill), snapshot round-trip. Benchmark: fill 1M cells in < 2 s and iterate them in < 300 ms.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/sheet-model test exits 0 with >= 250 tests",
   "pnpm --filter @officina/sheet-model bench shows 1M-cell fill < 2 s and full iteration < 300 ms",
   "A test demonstrates inserting a row above a formula's reference shifts the formula text and the dependency graph correctly (value unchanged after recalc)",
   "pnpm typecheck exits 0"
  ],
  "gate": false
 },
 {
  "id": "WP-17",
  "title": "Grid editor core: canvas renderer, selection, scrolling, freeze panes, in-cell editing, formula bar, autocomplete, fill handle, clipboard, Home ribbon",
  "milestone": "M2",
  "depends_on": [
   "WP-06",
   "WP-08",
   "WP-16"
  ],
  "owns": [
   "packages/editor-grid/src/**",
   "packages/editor-grid/test/**",
   "packages/editor-grid/bench/**",
   "apps/web/src/apps/grid/**",
   "e2e/specs/grid-core.spec.ts"
  ],
  "task": "Build editor-grid per §3.6: canvas renderer (DPR-aware; layers: cells, grid lines, borders with Excel precedence, text with numfmt output, overflow into empty neighbours, wrap/rotation/indent/vertical align, merged cells, conditional format visuals, selection/fill-handle overlay canvas), row/column headers with resize/autofit on double-click, hidden rows/cols markers, freeze panes (4 viewports), smooth scrolling with virtualisation (Fenwick-tree offsets), zoom (50–400 %), text measurement cache, sheet tab strip (+, rename, reorder by drag, colour, hide, context menu), Name Box (type A1/range/name to navigate; define name on Enter), Formula Bar (expand/collapse, fx button opening the Insert Function dialog built from function docs, with range highlighting while editing and syntax colouring), in-cell editor (F2/typing/double-click; Enter/Tab/Shift variants; Alt+Enter newline; Esc; arrow keys select ranges into the formula while editing like Excel's point mode; autocomplete of function names with signature tooltip and inline docs (★), autocomplete of column values, data validation dropdown host), selection model (ranges, multi with Ctrl, whole rows/cols, Ctrl+A regions, Ctrl+Arrow edge jumps, Shift extensions, keyboard per §1.7), fill handle (drag copy/series detection numbers/dates/weekdays/months/custom lists, Ctrl modifier, double-click autofill to adjacent extent, Fill menu Down/Right/Series dialog), clipboard (internal rich copy with styles/formulas, HTML table + TSV for external paste/copy, Paste Special dialog: all/values/formulas/formats/transpose/skip blanks/operations), cut/paste moving references, insert/delete cells/rows/cols with shift options, undo/redo via CommandLog, status bar: mode (Ready/Enter/Edit/Point), Average/Count/Sum of selection, zoom, view buttons. Worker: run `WorkbookCalc` in a Web Worker via comlink with a main-thread mirror of computed values (transfer dirty-cell batches); fall back to in-process when workers unavailable (tests). Home ribbon per §1.4 fully wired (font, borders menu, fill/font colour, alignment, wrap, merge variants, number format combo + buttons, cell styles gallery, insert/delete menus, format menu, AutoSum, Fill, Clear, Find & Select basics; Conditional Formatting and Format as Table menus route to WP-19's dialogs so leave the command ids registered but marked pending until WP-19). Format Cells dialog (Ctrl+1) with all six tabs. apps/web/src/apps/grid: page composition, open .xlsx through io-xlsx when WP-18 lands (until then .csv via papaparse and .tabula bundles — implement the CSV import/export here: delimiter/encoding sniffing, Text Import wizard-lite), `GridCapabilities`. Tests: renderer unit tests with a canvas mock (which cells drawn, border precedence), selection/navigation tests, fill-series detection tests, clipboard TSV/HTML parsing tests, editor state machine tests. e2e grid-core.spec.ts: open fixtures/xlsx via csv path and generated 50k-row CSV, type values and formulas (=SUM(A1:A3)) and verify computed display, F2 edit, fill handle drag via mouse, freeze top row then scroll (screenshot), zoom, Ctrl+1 dialog screenshot, status bar sum; screenshots grid-core/*-{light,dark}-desktop.png. Benchmark: Playwright scroll test measuring frame time on a synthetic 1M-populated-cell sheet (generate in-page).\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/editor-grid test exits 0 with >= 150 tests",
   "pnpm e2e --grep grid-core exits 0 with screenshots grid-core/{basic,freeze,format-cells,formula-autocomplete}-{light,dark}-desktop.png",
   "e2e: after typing 1,2,3 in A1:A3 and =SUM(A1:A3) in A4, cell A4 displays 6 and the status bar shows Sum: 6 when A1:A4 is selected",
   "bench: average frame time while scrolling the 1M-cell synthetic sheet < 18 ms (recorded in docs/qa/perf.md)",
   "docs/qa/visual-review-WP-17.md written"
  ],
  "gate": false
 },
 {
  "id": "WP-18",
  "title": "XLSX reader and writer (+ CSV/TSV/JSON) with round-trip fidelity tests",
  "milestone": "M2",
  "depends_on": [
   "WP-05",
   "WP-16"
  ],
  "owns": [
   "packages/io-xlsx/**"
  ],
  "task": "Implement `readXlsx(bytes) -> Workbook` and `writeXlsx(workbook) -> Uint8Array` per §3.5 (everything listed: workbook/sheets/definedNames/calcPr/date1904, styles.xml fully (numFmts incl. built-in ids 0–49 table, fonts, fills incl. gradients, borders, cellXfs with alignment/protection, cellStyles/cellStyleXfs, dxfs), sharedStrings with rich text runs and phonetic ignored, inline strings, sheetData with all cell types and shared/array/dynamic-array formulas (`cm` + metadata.xml for spill), cached values kept so files open instantly before recalculation, cols/rows props, mergeCells, conditionalFormatting incl. x14 extLst (data bars, icon sets with custom sets, colour scales), dataValidations incl. x14 list references to other sheets, hyperlinks (internal/external), autoFilter with filter state, tableParts + tables, sheetViews (pane/freeze/split/selection/zoom/showGridLines/rightToLeft), sheetPr (tabColor, outlinePr), pageSetup/pageMargins/printOptions/headerFooter/rowBreaks/colBreaks, legacy comments + VML drawing for notes (write VML for notes so Excel shows them), threadedComments + persons, drawings with two-cell/one-cell/absolute anchors for pictures and shapes (DrawingML via ooxml), chart parts passed to the charts package via a `ChartPartHook` interface (WP-20 implements; until then chart parts are preserved as passthrough and re-anchored), pivot tables/caches, slicers, externalLinks, customXml, vbaProject.bin, printerSettings all passthrough with relationship repair; sheet order and hidden/veryHidden; workbook protection/sheet protection flags preserved). Writer emits deterministic, Excel-valid XML (element ordering per CT_Worksheet sequence; `r` attributes on rows/cells; styles deduped; shared strings table; `fullCalcOnLoad=1` set when any formula lacks a cached value). CSV/TSV read/write with papaparse (sniffing per §3.5; dates/numbers typed on import with locale option; export respects number formats option), JSON import (array of objects → table). Tests: every fixtures/xlsx/features/* file read → write → read with structural ≥ 0.97, values 1.00 (compare with exceljs reading both the original and our output), formulas string-equal after normalisation, styles compared through exceljs (font/fill/border/numFmt per cell), LibreOffice opens all outputs and `--convert-to csv` of the rewritten file equals that of the original for the first sheet, openpyxl reads outputs; 100k-row fixture read+write under 3 s each. Write docs/qa/fidelity.md xlsx section via `pnpm fidelity --format xlsx`.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/io-xlsx test exits 0 with >= 120 tests",
   "pnpm fidelity --format xlsx prints structural >= 0.97, values == 1.00, oracle_opens == 1 for every features/* fixture",
   "A test shows a dynamic-array formula (=SORT(A1:A5)) survives round-trip with its spill metadata and Excel-compatible cm attribute",
   "A test reads a .xlsm fixture, writes it, and vbaProject.bin bytes are identical in the output"
  ],
  "gate": false
 },
 {
  "id": "WP-19",
  "title": "Grid features: sort & filter, conditional formatting UI, data validation, Format as Table, find/replace, comments/notes, hyperlinks, Page Layout, Formulas and Data ribbons, formula auditing, Formula Lab",
  "milestone": "M2",
  "depends_on": [
   "WP-17",
   "WP-18"
  ],
  "owns": [
   "packages/editor-grid/src/features/**",
   "packages/editor-grid/test/features/**",
   "apps/web/src/apps/grid/**",
   "e2e/specs/grid-features.spec.ts"
  ],
  "task": "Add feature modules (via a `GridFeature` interface the core exposes): AutoFilter (header dropdown buttons drawn on canvas; filter pane with search, value checklist, text/number/date/colour filters, sort A→Z/Z→A/by colour; filter state model persisted via io-xlsx), Sort dialog (multi-level, header detection, case, orientation, custom lists), Conditional Formatting dialogs (every rule type of §2.3, Manage Rules with priority/stop-if-true, live preview, rule application in renderer already supported by WP-17 visuals), Data Validation dialog (settings/input message/error alert; dropdown in-cell; circle invalid data), Format as Table (style gallery with 60 Excel-like table styles generated from theme colours; header/total/banded toggles; resize; convert to range; structured references in autocomplete), Find & Replace dialog (within sheet/workbook, by rows/cols, in formulas/values/notes, match case/entire cell; Go To Special: blanks/formulas/constants/visible), Notes and threaded Comments (pane, indicators, hover popups), Hyperlinks (insert dialog, Ctrl+click, cell styling), Clear menu, Remove Duplicates, Text to Columns wizard (delimited/fixed width with preview), Name Manager dialog + Define Name + Create from Selection + Use in Formula menu, Formula Auditing (trace precedents/dependents arrows drawn on overlay canvas, remove arrows, Show Formulas (Ctrl+`), Error Checking pane listing error cells with explanations, Evaluate Formula dialog stepping through the engine's explain tree, Watch Window), ★Formula Lab task pane (AST tree with live values per node, argument docs), Calculation options (auto/manual, F9/Shift+F9), Page Layout ribbon (margins/orientation/size/print area/print titles/breaks/scale-to-fit/gridlines+headings print toggles, Page Break Preview and Page Layout views rendering page boundaries), Insert ribbon items that don't need charts (table, picture floating over cells, shapes basic (rectangle/ellipse/line/textbox) with DrawingML anchors, text box, header & footer dialog, symbol), Review ribbon (spelling pass over text cells, comments navigation, protect sheet with password-less lock + locked-cell enforcement in editor, protect workbook structure), View ribbon (gridlines/headings/formula bar toggles, new window (web: new tab), split, hide/unhide, custom zoom). Group/outline rows and cols with outline symbols (Data ▸ Outline). Wire the complete Grid ribbon spec per §1.4. e2e grid-features.spec.ts: filter a column and assert hidden rows; conditional formatting data bars screenshot; validation dropdown screenshot and error on invalid; Format as Table + structured ref formula; trace precedents screenshot; Formula Lab screenshot; Page Break Preview screenshot; light+dark.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/editor-grid test exits 0 with >= 120 additional tests",
   "pnpm e2e --grep grid-features exits 0 with screenshots grid-features/{filter,cf-databars,validation,table,trace,formula-lab,page-break-preview}-{light,dark}-desktop.png",
   "e2e: applying a filter 'contains \"a\"' on a 100-row column hides the expected rows and the status bar reads 'n of 100 records found'",
   "docs/qa/visual-review-WP-19.md written"
  ],
  "gate": false
 },
 {
  "id": "WP-20",
  "title": "Charts package: chart model, ECharts renderer, chart editing UI, OOXML chart read/write",
  "milestone": "M2",
  "depends_on": [
   "WP-05",
   "WP-16",
   "WP-18"
  ],
  "owns": [
   "packages/charts/**",
   "packages/io-xlsx/src/charts/**",
   "packages/io-xlsx/test/charts/**"
  ],
  "task": "packages/charts: `ChartModel` (types of §2.3/§3.5: column/bar clustered/stacked/100%, line (markers, smooth), area, pie/doughnut (explosion, labels), scatter (lines/markers), bubble, radar, combo with secondary axis, treemap, sunburst, histogram, waterfall, funnel; series {name ref/literal, categories ref, values ref, cache, formatting}, axes (category/value/date; title, min/max/major unit, number format, reversed, log, gridlines, label rotation), legend, titles, data labels (position, value/percent/name), plot area/chart styles (fill/line), colours from theme palette (`accent1..6` with variations) with dark-mode-aware rendering), `ChartRenderer` (React component mapping ChartModel → ECharts 6 option; resize observer; export to PNG/SVG), `ChartData` resolution via a host interface (`getRange(ref) -> values`) so Grid charts are live and Write/Deck charts use embedded workbook data (through io-xlsx on the embedded part), Chart Design/Format ribbon group specs and dialogs (Change Chart Type gallery, Select Data dialog with series editor and switch row/column, Add Chart Element menu (axis titles, data labels, gridlines, legend position, trendline linear/exp/moving-average computed in-house), Quick Layouts, Chart Styles (12 presets), colour palettes), ★Recommended Charts (heuristics from selection shape: 1 numeric column → column; dates + numbers → line; 2 numeric → scatter; part-to-whole small set → pie), chart floating-object host for Grid (two-cell anchor move/resize using editor-grid's drawing layer interface — expose a `ChartObject` component; WP-22 wires it), `chart-ooxml`: read/write `c:chartSpace` subsets (incl. `c:userShapes` passthrough, `c:extLst` passthrough, `cx:chart` chartEx read for treemap/sunburst/waterfall/funnel/histogram and write as chartEx), number formats, cached data (`c:numCache/c:strCache`) written so Excel renders without recalculation, `c:style`/`c:clrMapOvr` preserved, chart relationships (embedded workbook part for Write/Deck, external data refs for Grid). Hook into io-xlsx via the `ChartPartHook` (packages/io-xlsx/src/charts) so sheets round-trip charts as models: read drawing anchors → ChartModel; write back. Tests: model ↔ ECharts option snapshot tests per type; OOXML read/write round-trip of fixtures/xlsx/features/charts-*.xlsx (every chart type) with structural ≥ 0.95 and LibreOffice opening the rewritten file; renderer tests in jsdom with ECharts' SVG renderer producing non-empty SVG; a Playwright screenshot of each chart type from a `packages/charts/stories/Charts.tsx` page via the e2e project (charts/<type>-light-desktop.png) reviewed visually.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/charts --filter @officina/io-xlsx test exits 0 with >= 90 tests for charts",
   "pnpm fidelity --format xlsx --only charts prints structural >= 0.95 and oracle_opens == 1 for every charts-* fixture",
   "e2e --grep charts produces screenshots for >= 12 chart types and docs/qa/visual-review-WP-20.md reviews them",
   "A test asserts a chart written to xlsx includes c:numCache values for every series"
  ],
  "gate": false
 },
 {
  "id": "WP-21",
  "title": "Grid analysis: pivot tables, sparklines, Goal Seek, Subtotal, Flash Fill, protection, Themes",
  "milestone": "M2",
  "depends_on": [
   "WP-19"
  ],
  "owns": [
   "packages/editor-grid/src/analysis/**",
   "packages/editor-grid/test/analysis/**",
   "packages/sheet-model/src/pivot/**",
   "packages/sheet-model/test/pivot/**",
   "apps/web/src/apps/grid/**",
   "e2e/specs/grid-analysis.spec.ts"
  ],
  "task": "Pivot tables (P1 but included): pivot model in sheet-model (source range/table, fields, row/column/filter/value areas, aggregations sum/count/average/min/max/product/countNums/stdDev/var, subtotals/grand totals, number format per value field, sorting, manual filter per field, compact/outline/tabular layouts, expand/collapse), pivot computation engine (group-by with stable ordering, dates grouped by year/quarter/month option), rendering as normal cells in a reserved range with pivot styling (PivotStyleLight16 default) and refresh on source change (manual Refresh + auto when source edited, debounced), PivotTable Fields task pane (drag between areas, value field settings dialog), PivotTable Analyze/Design contextual ribbon tabs, io-xlsx bridge: write `pivotTableDefinition` + `pivotCacheDefinition/Records` sufficient for Excel to open and refresh (keep original parts via passthrough when a pivot was read unchanged; regenerate when edited) — implement in packages/sheet-model/src/pivot/ooxml.ts and call it from io-xlsx through an exported hook (`PivotPartHook`, registered by the Grid app; io-xlsx already defines the hook interface in WP-18 — if not, add it to the interface file listed in io-xlsx's owned paths via a handoff note). Sparklines (line/column/win-loss; model + x14 sparkline groups read/write via io-xlsx hook; renderer in canvas; Sparkline Design ribbon with markers/colours). What-If ▸ Goal Seek dialog (secant method over the engine with iteration cap), Data ▸ Subtotal dialog (inserts SUBTOTAL formulas and outline groups), Flash Fill (pattern inference for prefix/suffix/split/case/concat of adjacent columns; Ctrl+E), Protection (Protect Sheet dialog with permission checkboxes and password hashed per Excel's legacy algorithm + SHA-512 modern `sheetProtection` attributes; Allow Edit Ranges basic), Page Layout ▸ Themes (apply Office-compatible theme colour/font sets to the workbook styles). e2e grid-analysis.spec.ts: create a pivot from a 200-row generated table (sum by category), screenshot; sparklines screenshot; Goal Seek sets a cell to hit a target; Subtotal creates outline groups; Flash Fill fills a split column.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/sheet-model --filter @officina/editor-grid test exits 0 with >= 80 additional tests",
   "pnpm e2e --grep grid-analysis exits 0 with screenshots grid-analysis/{pivot,sparklines,goal-seek,subtotal}-light-desktop.png",
   "A test writes a workbook with a pivot and LibreOffice opens it (oracle_opens == 1) and openpyxl lists one pivot table on the sheet",
   "docs/qa/visual-review-WP-21.md written"
  ],
  "gate": false
 },
 {
  "id": "WP-22",
  "title": "Grid integration gate: Office file loop, chart objects in sheets, print/PDF for sheets, Backstage, manual chapter",
  "milestone": "M2",
  "depends_on": [
   "WP-15",
   "WP-17",
   "WP-18",
   "WP-19",
   "WP-20",
   "WP-21"
  ],
  "owns": [
   "apps/web/src/apps/grid/**",
   "packages/print-pdf/src/grid/**",
   "e2e/specs/grid-e2e.spec.ts",
   "docs/qa/visual-review-WP-22.md",
   "docs/manual/grid.md"
  ],
  "task": "Integration gate for Grid: wire io-xlsx open/save (xlsx, xlsm read, xltx, csv, tsv, tabula, pdf export) with the worker parser and Compatibility Checker; wire the charts package into the sheet drawing layer (Insert ▸ Charts gallery and Recommended Charts, chart objects movable/resizable with two-cell anchors, Chart Design/Format contextual tabs, double-click to edit data via Select Data), pictures and shapes in the drawing layer (from WP-19) share the selection/transform code (use ui-kit TransformBox); Page Layout/print: packages/print-pdf/src/grid: print layout route rendering sheets to pages honouring print area, print titles, scaling/fit-to, margins, headers/footers with &[Page] codes, gridlines/headings options, page order; Backstage ▸ Print preview; PDF export of workbook/active sheet/selection; Backstage Info (properties, calc mode, sheet count), Export (CSV per sheet, PDF, JSON), Share. End-to-end e2e grid-e2e.spec.ts: open fixtures/xlsx/features/{styles,formulas,charts-column,cf,validation,tables}.xlsx → screenshots light/dark → edit (values, a formula, apply number format, insert a chart from a range) → Save As .xlsx via the e2e hook → re-read with io-xlsx AND exceljs in the test asserting the edits, formulas, chart count → LibreOffice converts to PDF without error; print route page count equals the Page Break Preview count; PDF produced via page.pdf() has that many pages; CSV export of a sheet equals papaparse-generated expectation. Manual chapter docs/manual/grid.md (formulas, autocomplete, tables, charts, pivots, printing, shortcuts). Fix any cross-feature defects found; record the visual review.",
  "acceptance": [
   "pnpm e2e --grep grid-e2e exits 0 with screenshots grid-e2e/*-{light,dark}-desktop.png",
   "e2e: the saved .xlsx re-read by exceljs contains the edited value, the formula text, the number format and one chart; soffice converts it to PDF (exit 0)",
   "e2e: PDF export page count equals Page Break Preview page count for fixtures/xlsx/features/print.xlsx",
   "pnpm verify exits 0 and CI is green; docs/qa/visual-review-WP-22.md and docs/manual/grid.md (>= 600 words) exist"
  ],
  "gate": true
 },
 {
  "id": "WP-23",
  "title": "Deck model: presentation, masters/layouts/slides, shapes, placeholders, themes, transitions, animations, notes",
  "milestone": "M3",
  "depends_on": [
   "WP-05"
  ],
  "owns": [
   "packages/deck-model/**"
  ],
  "task": "Implement §3.3 deck-model: Presentation/Slide/Shape types (EMU transforms, all shape kinds, placeholders with type/idx inheritance resolution master→layout→slide for position, size, text styles and fills via `resolveShapeProps(slide, shape)`), text bodies using a `deckText` ProseMirror schema (paragraphs with level/alignment/indent/spacing/bullets (char/autonumber/picture/none, bullet font/colour/size), runs with font/size/bold/italic/underline/strike/colour/highlight/baseline/spacing/caps/link, line breaks, fields (slidenum, datetime)), `bodyPr` (insets, anchor, wrap, autofit: normAutofit fontScale/lnSpcReduction, spAutoFit; vertical text), list styles (lvl1–9 pPr) from master text styles (title/body/other), theme handling (reuse ooxml theme; 12 bundled themes authored as JSON+assets in `src/themes/` with layouts: Title Slide, Title and Content, Section Header, Two Content, Comparison, Title Only, Blank, Content with Caption, Picture with Caption — generated from the default theme definition so new presentations are Office-compatible), slide size, backgrounds (fill types), transitions (model for the §1.4 list + duration + advance on click/after), animations (effect types entrance/emphasis/exit per §1.4, targets, start conditions, duration, delay, order, effect options direction; `buildTimeline(slide)` producing click-steps), notes, sections, comments, hidden slides, document properties. Operations as pure functions (`addSlide(layoutId)`, `duplicateSlide`, `moveSlide`, `applyLayout`, `resetSlide`, `addShape`, `transform`, `group/ungroup` (with coordinate re-basing), `reorderZ`, `align/distribute`, `setPlaceholderText`), plus the Yjs binding seam (`toY(doc)`, `fromY`) used in WP-29 (here implement JSON serialisation only with stable ids). `designIdeas(slide)` heuristic engine (★): given the slide's content types/counts, rank candidate layouts/variants (title-only → big centred title; title + 3–6 bullets → two-column or icon list; one image → picture-with-caption full bleed; numbers → stat callouts) returning concrete slide mutations. Tests: inheritance resolution (≥ 40 cases), autofit computation, animation timeline building, grouping coordinate maths, theme generation produces valid layouts, design ideas ranking.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/deck-model test exits 0 with >= 120 tests",
   "A test asserts a title placeholder on a slide inherits position from the layout and font from the master when unspecified",
   "pnpm typecheck exits 0"
  ],
  "gate": false
 },
 {
  "id": "WP-24",
  "title": "PPTX reader",
  "milestone": "M3",
  "depends_on": [
   "WP-23"
  ],
  "owns": [
   "packages/io-pptx/src/read/**",
   "packages/io-pptx/src/common/**",
   "packages/io-pptx/src/index.ts",
   "packages/io-pptx/test/read/**"
  ],
  "task": "Implement `readPptx(bytes) -> Presentation` per §3.5: presentation.xml (sizes, id lists, sections from `p14:sectionLst`, notesMaster), masters/layouts/slides with full spTree parsing (p:sp with geometry presets/custom geometry (`a:custGeom` paths), fills/lines/effects via ooxml, text bodies into deckText docs with all run/paragraph properties and fields, placeholders, p:pic with crops and media relationships, p:graphicFrame tables (a:tbl with merges, cell fills/borders, table styles by id), charts (relationship passed to charts package via `ChartPartHook` as in xlsx), SmartArt (`dgm` relationships: if a `drawing` fallback part exists, import its shapes as a group; else import a placeholder rectangle with the diagram text and mark a warning), OLE/embedded objects as picture fallback + passthrough, p:grpSp recursive, p:cxnSp connectors with start/end links, media (video/audio) relationships, backgrounds, transitions (map every `p:transition` child to our list with fallback Fade + original preserved), timing trees (`p:timing` parsed into the animation model for supported effect presets `presetClass/presetID` mapping table; unsupported ones recorded in passthrough with a warning), notes slides (text), comments (`p:cmLst` and modern `p188:` comments) + authors, hidden slides, docProps, theme per master, color map overrides, handout/notes masters passthrough, vbaProject passthrough. Warnings for every unsupported item. Tests: every fixtures/pptx/features/* reads without error; expected structure assertions (slide count, placeholder texts, shape geometries, group nesting, table dimensions, transition types, animation counts); text equality with python-pptx extraction for all fixtures; fixtures/external if present read without exception.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/io-pptx test exits 0 with >= 70 tests",
   "A test compares extracted text of every features/*.pptx with python-pptx output and passes",
   "A test asserts animations of fixtures/pptx/features/animations.pptx yield the expected click-step timeline",
   "pnpm typecheck exits 0"
  ],
  "gate": false
 },
 {
  "id": "WP-25",
  "title": "Deck editor core: slide stage, thumbnail rail, shapes and transforms, text editing, alignment guides, Home/Insert/Design ribbons",
  "milestone": "M3",
  "depends_on": [
   "WP-06",
   "WP-08",
   "WP-23"
  ],
  "owns": [
   "packages/editor-deck/src/**",
   "packages/editor-deck/test/**",
   "packages/ui-kit/src/transformbox/**",
   "apps/web/src/apps/deck/**",
   "e2e/specs/deck-core.spec.ts"
  ],
  "task": "Build editor-deck per §3.6: `SlideStage` (scaled 960×540 stage; renders shapes from deck-model with SVG preset geometry paths from ooxml presets, fills/gradients/pictures/lines/shadows, text bodies as static HTML from deckText with autofit computations measured via an offscreen measurer, tables, pictures with crop, groups, connectors), one live TipTap editor mounted on the focused text body (deckText schema; bullets via list styles; paragraph/run commands), `ui-kit/TransformBox` (new primitive: 8 handles, rotation handle, aspect lock with Shift, centre-resize with Alt, multi-selection bounding box, keyboard nudge/resize, touch-friendly sizes via a prop) used for every shape, smart guides and snapping (slide edges/centre, other shapes' edges/centres, equal-spacing), marquee selection, z-order, grouping, align/distribute menu, duplicate (Ctrl+D), copy/paste shapes and slides (internal clipboard JSON + image/text fallbacks), undo/redo, rulers/gridlines/guides toggles, zoom/fit, thumbnail rail (virtualised scaled stages, drag reorder, multi-select, section headers collapsible, context menu: new/duplicate/delete/hide/layout/section), notes pane (plain deckText editor), slide sorter view, outline view (titles/bullets text tree editable), Home/Insert/Design ribbons per §1.4 wired (new slide with layout gallery, layout change, reset, section, font/paragraph tools incl. align text and columns, Shapes gallery (all presets categorised like Office), Arrange, Quick Styles, Shape Fill/Outline/Effects, Shape Format contextual tab with size/position, Picture Format with crop/corrections-lite/alt text, Insert: pictures, text box, table (grid picker), icons (lucide set as SVG shapes), link, slide number/date/footer dialog, symbol, Design: themes gallery (12 bundled), variants, slide size, format background pane; Design Ideas pane listing deck-model `designIdeas` with thumbnails and one-click apply ★). Status bar: slide x of y, theme name, notes/comments toggles, view buttons, zoom, fit. apps/web/src/apps/deck: page composition, open via io-pptx (worker), save .scena until WP-26; `DeckCapabilities`. Tests: stage rendering (jsdom with SVG assertions), transform maths, snapping, autofit, rail reorder, outline edits; e2e deck-core.spec.ts: open fixtures/pptx/features/{placeholders,shapes,tables,groups}.pptx with screenshots, add a slide with a layout, insert a rectangle and resize via handle drag, type a title, apply a theme, Design Ideas apply; screenshots light/dark.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/editor-deck --filter @officina/ui-kit test exits 0 with >= 120 tests for deck/transformbox",
   "pnpm e2e --grep deck-core exits 0 with screenshots deck-core/{placeholders,shapes,tables,groups,theme,design-ideas}-{light,dark}-desktop.png",
   "e2e: dragging a shape's corner handle changes its width by the dragged distance (±2 px at 100 % zoom)",
   "docs/qa/visual-review-WP-25.md written"
  ],
  "gate": false
 },
 {
  "id": "WP-26",
  "title": "PPTX writer and round-trip fidelity tests",
  "milestone": "M3",
  "depends_on": [
   "WP-24"
  ],
  "owns": [
   "packages/io-pptx/src/write/**",
   "packages/io-pptx/test/write/**",
   "packages/io-pptx/test/roundtrip/**"
  ],
  "task": "Implement `writePptx(presentation, {passthrough}) -> Uint8Array`: presentation.xml with id lists and section list, masters/layouts/slides with correct relationship graphs (slide→layout→master→theme, notesSlide↔slide, media, charts via charts package writer, embedded workbook parts for charts), spTree serialisation for every shape kind (preset and custom geometry, fills/lines/effects, text bodies with paragraph/run properties, fields, bullets, autofit values), placeholders with `p:ph` type/idx, tables, pictures with crops, groups (with chOff/chExt), connectors, backgrounds, transitions (our model → `p:transition` with `p14:` extensions where needed inside `mc:AlternateContent` like PowerPoint), timing trees for our animation model (generate PowerPoint-compatible `p:timing` with `p:par/p:seq/p:cTn` structure, presetClass/presetID/presetSubtype, `p:bldLst`), notes slides, comments (modern format) + authors, hidden slides, docProps, the 12 bundled themes serialised as theme parts, passthrough re-emission (SmartArt data/layout/colors/quickStyle parts and their relationships when the slide's shapes were not edited; vbaProject; handout masters; unknown parts). Deterministic output. Compatibility Checker issues (code blocks → text box with monospace, callouts → shapes). Round-trip tests over fixtures/pptx/features/* (structural ≥ 0.95, text 1.00, LibreOffice opens → PDF, visual ≥ 0.85 vs original render), python-pptx opens outputs and counts slides/shapes equal, new-presentation-from-theme test writes a .pptx that LibreOffice opens and whose slides have the expected layouts; `pnpm fidelity --format pptx` section in docs/qa/fidelity.md.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/io-pptx test exits 0 with >= 60 additional tests",
   "pnpm fidelity --format pptx prints structural >= 0.95, text == 1.00, oracle_opens == 1, visual >= 0.85 for every features/* fixture",
   "A test asserts a presentation with 3 animated shapes writes a p:timing tree that python-pptx can read back (slide.element contains p:timing with 3 p:cTn effect nodes)",
   "docs/qa/fidelity.md contains the pptx table"
  ],
  "gate": false
 },
 {
  "id": "WP-27",
  "title": "Deck features: slide show, presenter view, transitions, animations, Animation Pane, Slide Show & Transitions/Animations ribbons, media, equations",
  "milestone": "M3",
  "depends_on": [
   "WP-25",
   "WP-20"
  ],
  "owns": [
   "packages/editor-deck/src/features/**",
   "packages/editor-deck/test/features/**",
   "apps/web/src/apps/present/**",
   "apps/web/src/apps/deck/**",
   "e2e/specs/deck-show.spec.ts"
  ],
  "task": "Slide show engine (`/present/:docId` route, fullscreen): renders slides via SlideStage at viewport scale, click/keys/swipe navigation per §1.7 show keys, transitions implemented with the Web Animations API per §1.4 list (Fade, Push, Wipe, Split, Reveal, Cut, Shape, Uncover, Cover, Flash, Random Bars; Morph-lite P2 excluded), animations engine playing deck-model timelines (entrance/emphasis/exit effects per §1.4 as WAAPI keyframes with effect options; click steps; with/after previous; delays; auto-advance timers), laser pointer/pen/highlighter overlays (canvas; ink discarded on exit unless \"keep annotations\" → stored as freeform shapes), black/white screens, go-to-slide via number+Enter, end-of-show screen, hidden slides skipped. Presenter view: a second window (web: `window.open` + BroadcastChannel; Electron: host opens a second BrowserWindow on the secondary display via `PlatformHost.openPresenterWindow`, implemented in WP-34 — define the interface here with a web fallback) showing current/next slide, notes with font size controls, timer/clock, slide grid, pointer controls, with the audience window following. Transitions ribbon (gallery with live preview on the stage, effect options, duration, apply to all, advance options), Animations ribbon (gallery, effect options, Add Animation, Animation Pane with reorder/timing/triggers-lite, Animation Painter, preview button plays on stage), Slide Show ribbon (from beginning/current, present in browser tab, set up show dialog (loop, show without narration, pen colour), hide slide, rehearse timings (records per-slide durations into transitions), presenter view toggle), Insert ▸ Media (video/audio from file: HTML5 playback on stage and in show; written as pptx media parts), Insert ▸ Equation (MathLive editor producing MathML/LaTeX rendered to an SVG picture + OMML preserved when imported), Insert ▸ Chart (charts package with embedded workbook data editing via a mini grid dialog), Insert ▸ Code block (★), Reuse Slides pane (open another .pptx and drag slides in), Outline import (paste a Markdown/indented outline → slides ★), PNG/SVG export of current slide. e2e deck-show.spec.ts: open fixtures/pptx/features/{transitions,animations}.pptx → start show (F5) → screenshot first slide → advance (Space) through click steps asserting shape visibility changes → presenter view opened and screenshot (two pages via context) → Esc ends; Transitions gallery screenshot; Animation Pane screenshot.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/editor-deck test exits 0 with >= 90 additional tests",
   "pnpm e2e --grep deck-show exits 0 with screenshots deck-show/{show-first,show-after-click,presenter,transitions-gallery,animation-pane}-light-desktop.png",
   "e2e: in fixtures/pptx/features/animations.pptx slide 1, the second shape is hidden before the first click and visible after it",
   "docs/qa/visual-review-WP-27.md written"
  ],
  "gate": false
 },
 {
  "id": "WP-28",
  "title": "Deck integration gate: Office file loop, print/PDF handouts, Backstage, manual chapter",
  "milestone": "M3",
  "depends_on": [
   "WP-15",
   "WP-26",
   "WP-27"
  ],
  "owns": [
   "apps/web/src/apps/deck/**",
   "packages/print-pdf/src/deck/**",
   "e2e/specs/deck-e2e.spec.ts",
   "docs/qa/visual-review-WP-28.md",
   "docs/manual/deck.md"
  ],
  "task": "Integration gate for Deck: wire io-pptx open/save (pptx, potx, pptm read, scena, pdf, png/svg per slide, outline .md export) with worker parsing and Compatibility Checker; Backstage Info/Export/Print; packages/print-pdf/src/deck: print layouts (full-page slides, handouts 2/3/4/6/9 per page with optional lines, notes pages, outline), preview in Backstage, PDF export via the print pipeline, PNG export of all slides as a zip (fflate). e2e deck-e2e.spec.ts: open fixtures/pptx/features/{placeholders,charts,shapes}.pptx → screenshots light/dark → edit (title text, add slide, insert chart, add animation) → Save As .pptx via the hook → re-read with io-pptx and python-pptx asserting slide count/title/animation → LibreOffice converts to PDF without error; print route (handouts 6 per page) page count equals ceil(slides/6); page.pdf() of the print route yields that page count; PNG export zip contains one PNG per slide. Manual chapter docs/manual/deck.md. Fix cross-feature defects; visual review.",
  "acceptance": [
   "pnpm e2e --grep deck-e2e exits 0 with screenshots deck-e2e/*-{light,dark}-desktop.png",
   "e2e: saved .pptx re-read by python-pptx reports the added slide and edited title; soffice converts it to PDF (exit 0)",
   "e2e: handout PDF page count == ceil(slideCount/6)",
   "pnpm verify exits 0 and CI is green; docs/qa/visual-review-WP-28.md and docs/manual/deck.md (>= 500 words) exist"
  ],
  "gate": true
 },
 {
  "id": "WP-29",
  "title": "Collaboration and version history: Yjs documents, collab server, presence, comments sync, version history UI",
  "milestone": "M4",
  "depends_on": [
   "WP-15",
   "WP-28"
  ],
  "owns": [
   "packages/collab/**",
   "apps/collab-server/**",
   "packages/editor-write/src/collab/**",
   "packages/editor-deck/src/collab/**",
   "packages/ui-kit/src/versions/**",
   "packages/ui-kit/src/presence/**",
   "apps/web/src/collab/**",
   "e2e/specs/collab.spec.ts",
   "docs/manual/collaboration.md"
  ],
  "task": "Implement §3.7. packages/collab: `CollabDoc` wrapper over Y.Doc (docId, awareness, undo manager scoped per user, `y-indexeddb` persistence, `@hocuspocus/provider` websocket provider with reconnect/offline queue, connection status), `VersionStore` (dexie: snapshots on explicit save and every 10 min of edits, thinning to max 200, named versions, `diffVersions(a,b)` for Write producing word-level insert/delete runs), bundle (de)serialisation of Yjs updates into `.scriba/.scena` native files and a `yjs.bin` sidecar inside DOCX-saved sessions (stored in IndexedDB keyed by file hash so reopening a .docx restores history). Write: replace ProseMirror history with y-prosemirror (sync, cursors with user colours/names, yUndoPlugin) via the HistoryStrategy seam; comments stored in a Y.Map and anchored by Yjs relative positions; track-changes marks carry author ids from awareness. Deck: deck-model `toY/fromY` binding (slides Y.Array of Y.Maps; text bodies Y.XmlFragment bound to the mounted TipTap editor), awareness showing collaborators' selected slide/shape. Sharing: title-bar Share button → dialog with \"Start live session\" (creates docId session on the configured server from Options ▸ Collaboration: server URL + token), copy link; opening a `/write/:docId?collab=<server>` link joins. Presence avatars in the title bar (ui-kit/presence). Version History pane (ui-kit/versions; Review ▸ Version History and Backstage ▸ Info): list grouped by day, name a version, preview read-only, restore (as a new version), diff view for Write (side panel with insert/delete highlighting), export a version to file. apps/collab-server: Hocuspocus 4.7 server (`@hocuspocus/server` + `@hocuspocus/extension-sqlite` via better-sqlite3 or sql.js fallback) with `onAuthenticate` shared-token check, per-document size limits, a `/health` endpoint, Dockerfile, README (deployment, token, reverse proxy), `pnpm --filter @officina/collab-server dev`. Grid: not collaborative in v1; make sure the Share dialog says so and offers \"send a copy\". e2e collab.spec.ts: start the collab server in the test (child process on a free port), open the same Write document in two browser contexts, type in one, assert text appears in the other within 2 s, screenshot showing two cursors; version history: save twice, open pane, assert two versions, restore the first and assert text; offline test: disconnect context A (route abort websocket), type, reconnect, assert convergence.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/collab --filter @officina/collab-server test exits 0 with >= 50 tests",
   "pnpm e2e --grep collab exits 0 with screenshots collab/{two-cursors,version-history,diff}-light-desktop.png",
   "e2e: offline edits in one context converge after reconnect (both contexts show identical text)",
   "docs/manual/collaboration.md explains self-hosting the server; docs/qa/visual-review-WP-29.md written"
  ],
  "gate": false
 },
 {
  "id": "WP-30",
  "title": "Storage hosts, AutoSave to file, document recovery, recents, file associations",
  "milestone": "M4",
  "depends_on": [
   "WP-15",
   "WP-22",
   "WP-28"
  ],
  "owns": [
   "packages/storage/src/hosts/**",
   "packages/storage/src/autosave/**",
   "packages/storage/src/recovery/**",
   "packages/storage/test/**",
   "packages/ui-kit/src/recovery/**",
   "apps/web/src/recovery/**",
   "apps/web/src/hosts/**",
   "e2e/specs/recovery.spec.ts"
  ],
  "task": "Complete §3.8 for all three hosts at the interface level (Electron and Capacitor adapters are implemented here against the IPC/plugin contracts that WP-35/WP-36 provide on their side: define `window.officina` preload API types in packages/storage/src/hosts/electron/api.d.ts and the Capacitor plugin usage in hosts/capacitor; both must degrade gracefully when the bridge is absent so web tests keep passing). AutoSave: title-bar toggle (persisted per document), debounce 2 s after last edit, atomic writes (write temp + rename on Electron; createWritable on web), conflict detection via file mtime/hash (prompt: overwrite/save copy), background save in a worker for Office formats, save progress in the status bar, error toasts with retry. Journals: Grid CommandLog journal and Yjs updates appended to IndexedDB (dexie) with periodic compaction into snapshots; `closedCleanly` flag on beforeunload/visibilitychange; heartbeat timestamps. Recovery pane (ui-kit/recovery; shown at startup in Hub and in the app when relevant): lists recoverable documents with last-saved vs recovered times, Open, Save As, Delete, \"Show original\"; recovered documents open with a yellow bar \"Recovered document — Save to keep\". Recents: unified store with per-host handle persistence (web FS handles, Electron paths, Capacitor content URIs), pin/unpin, remove, open-location, \"Recent\" list in Backstage ▸ Open with search; file watching on Electron (external modification → reload prompt). File association plumbing: `openFromOS(payloadBytesOrPath)` entry used by Electron's `open-file`/argv and Capacitor's intent handling, routing to the right app by extension/magic. Tests: fake hosts; autosave timing with fake timers; journal compaction; recovery registry edge cases (crash during save). e2e recovery.spec.ts: edit without saving, reload the page without clean close (page.evaluate to skip the beforeunload handler), assert the Recovery pane lists the document and opening it restores the text; screenshot recovery pane light.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/storage test exits 0 with >= 60 additional tests",
   "pnpm e2e --grep recovery exits 0 with screenshot recovery/recovery-pane-light-desktop.png",
   "e2e: after simulated crash the recovered Write document contains the unsaved paragraph",
   "pnpm typecheck exits 0 (Electron/Capacitor adapters compile against the declared bridge types)"
  ],
  "gate": false
 },
 {
  "id": "WP-31",
  "title": "Accessibility, i18n (de, ar), RTL, high-contrast theme, Key Tips completeness",
  "milestone": "M4",
  "depends_on": [
   "WP-15",
   "WP-22",
   "WP-28"
  ],
  "owns": [
   "packages/i18n/locales/**",
   "packages/ui-kit/src/a11y/**",
   "packages/editor-write/src/a11y/**",
   "packages/editor-grid/src/a11y/**",
   "packages/editor-deck/src/a11y/**",
   "e2e/specs/a11y.spec.ts",
   "e2e/specs/rtl.spec.ts",
   "docs/qa/a11y.md"
  ],
  "task": "Deliver §3.10. Audit and fix ARIA across ui-kit/editors (this WP may edit ARIA/label/focus-related code anywhere inside packages/ui-kit, packages/editor-write, packages/editor-grid, packages/editor-deck, packages/i18n; keep changes confined to accessibility/i18n concerns and record other defects as handoffs): ribbon tabs/toolbar roving focus, menus, dialogs, galleries, palette; Grid ARIA grid shadow tree for active cell/row/col headers with announcements; Write editor region labelling, headings navigation via screen reader (ProseMirror DOM is semantic already; add aria-describedby for comments/revisions), Deck shapes list for keyboard selection (Tab cycles shapes; Enter edits text; arrow nudges), live region announcer service (`announce(message, politeness)`) used by status changes (mode, track changes, save, recalc); focus visible rings in all themes; high-contrast theme verified (contrast checks via a unit test over token pairs ≥ 4.5:1 for text, ≥ 3:1 for UI); `prefers-reduced-motion`. Key Tips: every ribbon command reachable via Alt sequences with Office letters where applicable (write the mapping table in packages/commands keytips files? No — those are owned by WP-06: put the completeness test here and a handoff for missing letters; the ribbon spec files in apps/web/src/apps/*/ribbon.ts carry `keyTip` fields and belong to the app integration WPs, so edit only the `keyTip` strings there, noting it in the commit). i18n: German and Arabic locale files for every namespace (machine-quality translations acceptable, reviewed for UI length), language switch in Options with restart-free reload, Arabic RTL: html dir flips ribbon/tab order/status bar, Write bidi paragraphs (`dir` attribute per paragraph, RTL default for Arabic locale new documents), Grid rightToLeft sheets (columns from right, headers mirrored), Deck RTL text. e2e a11y.spec.ts: axe on Hub, Write, Grid, Deck, Backstage, Options, Format Cells dialog, Palette — 0 serious/critical; keyboard-only flow: from Hub to Write, Alt,H,B bold via Key Tips, open palette with keyboard and run \"Freeze Panes\" in Grid. rtl.spec.ts: switch to Arabic, screenshots of the three apps (rtl/<app>-ar-light-desktop.png) and an Arabic paragraph typed in Write renders right-aligned. docs/qa/a11y.md documents the patterns and known gaps.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm e2e --grep 'a11y|rtl' exits 0; axe reports 0 serious/critical violations on all listed screens",
   "screenshots rtl/{write,grid,deck}-ar-light-desktop.png exist and are reviewed in docs/qa/visual-review-WP-31.md",
   "pnpm --filter @officina/i18n test exits 0 including a test that de and ar have every key en has",
   "A unit test asserts token contrast ratios for all three themes"
  ],
  "gate": false
 },
 {
  "id": "WP-32",
  "title": "Templates, Hub polish, onboarding tour, Office-synonym command palette, Options completeness",
  "milestone": "M4",
  "depends_on": [
   "WP-15",
   "WP-22",
   "WP-28"
  ],
  "owns": [
   "packages/templates/**",
   "apps/web/src/apps/hub/**",
   "apps/web/src/onboarding/**",
   "packages/commands/src/synonyms.ts",
   "packages/commands/test/synonyms.test.ts",
   "e2e/specs/hub-templates.spec.ts",
   "docs/manual/getting-started.md"
  ],
  "task": "packages/templates: 12 templates per app authored as native model JSON + thumbnail PNGs rendered by a script (`packages/templates/scripts/render-thumbnails.mjs` using the e2e Playwright setup to open each template and screenshot the first page/sheet/slide at 400×300): Write (Blank, Letter, Resume, Report with TOC, Meeting Minutes, Invoice, Memo, Newsletter, Academic Paper (APA-like), Project Proposal, Cover Letter, Brochure-lite), Grid (Blank, Budget, Invoice, Loan Calculator, Project Tracker with CF, Timesheet, Expense Report, Inventory, Gantt-lite with CF bars, Gradebook, Sales Dashboard with charts, To-do), Deck (Blank 16:9, Blank 4:3, Pitch, Quarterly Review, Lesson, Status Report, Portfolio, Minimal Typographic, Photo Story, Workshop, Agenda, Thank You), each exercising real features (styles, formulas, charts, layouts) so they double as demo documents. Hub: template gallery with categories/search/preview dialog, \"Blank\" hero cards, recents with pin/remove/open-location, drag-drop, keyboard navigation, empty states, \"What's new\" card linking to CHANGELOG; first-run onboarding tour (3 cards per §1.9, dismissible, re-openable from Help ▸ Tour, stored in Options); \"Coming from Office\" cheat-sheet dialog (Ctrl+/ shortcut list grouped by app, printable). Command palette synonyms: curated `synonyms.ts` with ≥ 150 Office terms/phrases mapped to command ids (e.g. \"Merge & Center\", \"Freeze Panes\", \"Track Changes\", \"Format Painter\", \"Slide Master\", \"Paste Special\", \"Text to Columns\", \"Page Setup\") plus common misspellings; test ensures every synonym target exists. Options dialog completeness per §1.5 (General, Display, Proofing, Save, Language, Advanced, Keyboard, Privacy pages with real settings bound to stores). e2e hub-templates.spec.ts: Hub screenshots light/dark; open each template (36) and assert no console errors and screenshot the first view (templates/<app>-<name>-light-desktop.png); palette search \"merge and center\" in Grid resolves to the Merge & Center command. docs/manual/getting-started.md.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm e2e --grep hub-templates exits 0 and 36 template screenshots exist under e2e/screenshots/templates/",
   "pnpm --filter @officina/commands test exits 0 including synonyms.test.ts with >= 150 synonyms",
   "docs/qa/visual-review-WP-32.md reviews all template screenshots (each template must look finished: no overlapping text, no placeholder lorem beyond intended samples)",
   "docs/manual/getting-started.md exists (>= 500 words)"
  ],
  "gate": false
 },
 {
  "id": "WP-33",
  "title": "Notula (OneNote-lite)",
  "milestone": "M4",
  "depends_on": [
   "WP-14",
   "WP-30"
  ],
  "owns": [
   "packages/notes/**",
   "apps/web/src/apps/notes/**",
   "e2e/specs/notes.spec.ts",
   "docs/manual/notes.md"
  ],
  "task": "Tier 2 app per §2.5 built on editor-write: notebook model (notebook → sections → pages; a notebook is a folder on disk via the storage host (desktop/Android) or an IndexedDB-backed virtual folder on web, with each page a `.scriba` plus `notebook.json` index), left navigation (notebook switcher, section tabs with colours, page list with drag reorder and sub-pages), page canvas = Write editor in web-layout mode with a title field and created/modified timestamps, tags (To Do with checkbox, Important, Question, Remember, custom with icons; Ctrl+1/2/3 shortcuts; Tags summary pane across the notebook grouped by tag with jump links), quick search across all pages (text index built incrementally, stored in IndexedDB), page templates (To-do list, Meeting notes, Lecture notes, Project overview), insert: table, picture, file attachment (stored next to the page), link to page (`[[` autocomplete ★), code block, date/time, equation; export page to .docx/.pdf/.md and notebook to a zip of Markdown; ribbon per §1.4 Notes; command palette and shortcuts; mobile layout handled in WP-36. e2e notes.spec.ts: create notebook/section/pages, tag a line To Do, tags summary shows it, search finds a page, screenshot light/dark, export a page to .docx and re-read with io-docx.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/notes test exits 0 with >= 40 tests",
   "pnpm e2e --grep notes exits 0 with screenshots notes/{overview,tags}-{light,dark}-desktop.png",
   "e2e: exported page .docx re-read by io-docx contains the page title as a Title-styled paragraph",
   "docs/manual/notes.md exists"
  ],
  "gate": false
 },
 {
  "id": "WP-34",
  "title": "Electron desktop shell and Linux packaging (AppImage, deb) + Windows/macOS builder config",
  "milestone": "M5",
  "depends_on": [
   "WP-30"
  ],
  "owns": [
   "apps/desktop/**",
   "packages/storage/src/hosts/electron/**",
   "tools/release/**",
   "e2e/specs/electron.spec.ts",
   "docs/manual/building.md",
   "docs/manual/install-desktop.md"
  ],
  "task": "apps/desktop with electron-vite 6: `src/main` (single instance lock; BrowserWindow per document with `contextIsolation`, `sandbox: true`, CSP, no nodeIntegration; loads apps/web build (file://) or dev server; native application menu built from the command registry/keymaps (File/Edit/View/Insert/Format/Tools/Window/Help with accelerators from §1.7; menu items dispatch to the renderer via IPC), `open-file`/argv handling → `openFromOS`, recent documents (`app.addRecentDocument`), file dialogs with filters, atomic fs writes, file watching (chokidar), `printToPDF` and native print, `shell.openExternal` with allow-list, presenter second window on the secondary display, auto-updater (electron-updater against GitHub Releases, check-on-start opt-in in Options), crash recovery mirror dir in `app.getPath('userData')/recovery`, protocol/file associations), `src/preload` exposing `window.officina` matching packages/storage's declared types with zod-validated IPC, `ElectronHost` implementation in packages/storage/src/hosts/electron. electron-builder config in apps/desktop/electron-builder.yml (appId io.github.zahakj.officina, productName Officina, artifact names `officina-<version>-linux-x86_64.AppImage`, `officina_<version>_amd64.deb`, `Officina-Setup-<version>.exe`, `Officina-<version>-portable.exe`, `Officina-<version>-arm64.dmg`, `Officina-<version>-x64.dmg`; linux: AppImage + deb with desktop entry, MIME types for docx/xlsx/pptx/odt/ods/odp/csv/scriba/tabula/scena, icon from build/icon.png; win: nsis + portable, `signAndEditExecutable: false` so Linux cross-builds don't need wine; mac: dmg + zip, `hardenedRuntime: false`, no notarisation; `publish: github`), `release:desktop` task script (`electron-builder --<platform> --publish never`) and `release:desktop` root script passthrough. Try a Windows cross-build locally (`--win`) — if it succeeds, record it; if it fails for lack of wine, record the exact error in docs/manual/building.md and leave Windows to CI (not a failure of this WP). docs/manual/install-desktop.md (AppImage/deb/exe/dmg, unsigned warnings as in abax's README, Gatekeeper `xattr -cr`), docs/manual/building.md (desktop + Android + web). e2e electron.spec.ts with Playwright `_electron`: skipped unless DISPLAY is set; when run: launches the built app, opens a fixture via IPC, screenshots electron/write-light-desktop.png.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm release:desktop -- --linux exits 0 and produces apps/desktop/dist/officina-*-linux-x86_64.AppImage and apps/desktop/dist/officina_*_amd64.deb",
   "dpkg-deb -I apps/desktop/dist/officina_*_amd64.deb shows Package: officina and the MIME types in the desktop file (dpkg-deb -c lists usr/share/applications/officina.desktop)",
   "./apps/desktop/dist/officina-*.AppImage --appimage-extract into a temp dir contains usr/resources/app.asar (or resources/app.asar) and `npx asar list` shows index.html",
   "if DISPLAY is set: pnpm e2e --grep electron exits 0 and the screenshot exists; otherwise the spec reports skipped and docs/manual/building.md states how to run it",
   "pnpm verify exits 0; CI green"
  ],
  "gate": false
 },
 {
  "id": "WP-35",
  "title": "Android shell (Capacitor): project, plugins, intents, signing, debug and release APK",
  "milestone": "M5",
  "depends_on": [
   "WP-30"
  ],
  "owns": [
   "apps/android/**",
   "packages/storage/src/hosts/capacitor/**",
   "tools/setup-android.sh",
   "docs/manual/install-android.md"
  ],
  "task": "apps/android: `capacitor.config.ts` (appId io.github.zahakj.officina, appName Officina, webDir ../web/dist, android scheme https, `server.androidScheme`), `npx cap add android` committing the generated android/ project (Gradle wrapper 8.14.3, AGP 8.13, compileSdk/targetSdk 36, minSdk 24), gradle.properties memory limits per §4.4, plugins installed and configured: @capacitor/filesystem, @capawesome/capacitor-file-picker, @capacitor/share, @capacitor/app (back button, intents), @capacitor/keyboard (resize mode `native`, accessory bar off), @capacitor/status-bar (theme-coloured, overlays off), @capacitor/preferences, @capacitor/haptics; AndroidManifest intent filters: ACTION_VIEW/ACTION_SEND for the Office/ODF/CSV/native MIME types and file extensions, `android:launchMode=\"singleTask\"`, FileProvider for sharing exports; a small custom Capacitor plugin `OfficinaPrint` (Java) exposing `print()` using `PrintManager` + `WebView.createPrintDocumentAdapter` and `printToPdf` via `PdfDocument` fallback, consumed by packages/print-pdf through `PlatformHost.print`; `CapacitorHost` in packages/storage/src/hosts/capacitor (open via file picker with content URIs, read/write with Filesystem, SAF-persisted permissions, recents by URI with display names, share sheet, app data dir for notebooks/recovery); icons/splash via `@capacitor/assets` from docs/media/icon.svg renders (committed outputs); `pnpm --filter @officina/android build:web` (builds apps/web with `VITE_PLATFORM=capacitor`), `cap sync`, `release:android` task script that runs `./gradlew assembleRelease` and signs with `~/.officina-keys/release.jks` (generate per §4.4 if missing, store secrets with `gh secret set`), outputs `apps/android/dist/officina-<version>-android.apk`; also `assembleDebug`. Update tools/setup-android.sh if anything was missing. Static verification script `apps/android/scripts/verify-apk.sh` (aapt2 badging → package/version/minSdk/intent filters, apksigner verify, unzip -l for assets/public/index.html). docs/manual/install-android.md (sideloading, file access, known limits).\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm release:android exits 0 and apps/android/dist/officina-*-android.apk exists and is < 40 MB",
   "bash apps/android/scripts/verify-apk.sh apps/android/dist/officina-*-android.apk exits 0 (package io.github.zahakj.officina, minSdk 24, targetSdk 36, intent filters include application/vnd.openxmlformats-officedocument.wordprocessingml.document, spreadsheetml.sheet and presentationml.presentation, apksigner verified, assets/public/index.html present)",
   "gh secret list shows ANDROID_KEYSTORE_B64 and ANDROID_KEYSTORE_PASSWORD",
   "cd apps/android/android && ./gradlew assembleDebug exits 0",
   "pnpm verify exits 0; CI green"
  ],
  "gate": false
 },
 {
  "id": "WP-36",
  "title": "Mobile UX adaptation (phone and tablet layouts, bottom ribbon sheet, touch handles) with Playwright mobile emulation",
  "milestone": "M5",
  "depends_on": [
   "WP-31",
   "WP-32",
   "WP-33"
  ],
  "owns": [
   "packages/ui-kit/src/mobile/**",
   "apps/web/src/mobile/**",
   "packages/editor-write/src/touch/**",
   "packages/editor-grid/src/touch/**",
   "packages/editor-deck/src/touch/**",
   "e2e/specs/mobile.spec.ts",
   "docs/manual/mobile.md"
  ],
  "task": "Implement §1.8 for all apps using the same command registry and ribbon specs: responsive `AppFrame` variants (top app bar with back/title/undo/redo/search/overflow), `BottomToolbar` (context-aware quick commands derived from the ribbon spec's Home group priorities via a `mobilePriority` field added to RibbonSpec items in ui-kit types — set defaults so untouched specs still work) and `RibbonSheet` (bottom sheet with tab chips and stacked groups), keyboard-inset handling (`env(keyboard-inset-height)` + Capacitor Keyboard events via a host hook), safe-area insets, touch selection handles for Write (custom handles over ProseMirror selection; long-press selection; floating context toolbar), Grid touch (range handles at corners, tap/drag headers, pinch zoom via pointer events with inertia, formula bar docked above the keyboard with ✓/✕ and the quick keys row, bottom sheet filters/validation lists, sheet chip strip), Deck touch (vertical slide list portrait, full-width edit, enlarged TransformBox handles, pinch zoom, Present fullscreen landscape with swipe + tap zones + notes toggle), Notes mobile (drawer navigation), Hub mobile (cards list), Android back-button routing (`@capacitor/app` listener closes sheets/dialogs first), dialogs become full-screen sheets under 640 px, min hit targets 44 px (lint rule test over ui-kit primitives' CSS), hover-only affordances get tap equivalents. e2e mobile.spec.ts with Playwright devices 'Pixel 7' (412×915, touch) and 'iPad (gen 7)' landscape: each app opened on a fixture; screenshots mobile/<app>-{light,dark}-phone.png and mobile/<app>-light-tablet.png; interactions: open ribbon sheet, tap Bold, verify; Grid: tap cell, type via keyboard, ✓ commits; Deck: present and swipe to next slide; Hub recents.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm e2e --grep mobile exits 0 with screenshots mobile/{hub,write,grid,deck,notes}-{light,dark}-phone.png and mobile/{write,grid,deck}-light-tablet.png",
   "e2e: on Pixel 7 the ribbon is not rendered as the desktop tab strip; the bottom toolbar and the ribbon sheet are present and functional (Bold applied)",
   "docs/qa/visual-review-WP-36.md reviews every mobile screenshot for overlap/clipping; docs/manual/mobile.md exists",
   "pnpm verify exits 0; CI green"
  ],
  "gate": false
 },
 {
  "id": "WP-37",
  "title": "PWA: service worker, offline, install, file handlers, share target",
  "milestone": "M5",
  "depends_on": [
   "WP-30"
  ],
  "owns": [
   "apps/web/vite.config.ts",
   "apps/web/public/**",
   "apps/web/src/pwa/**",
   "e2e/specs/pwa.spec.ts",
   "docs/manual/install-web.md"
  ],
  "task": "vite-plugin-pwa 2.0 with Workbox: precache the app shell and fonts (document fonts lazily cached on first use), runtime caching none for third parties (there are none), `manifest.webmanifest` (name Officina, icons from public/icons, theme/background colours per theme, display standalone, `file_handlers` for docx/xlsx/pptx/odt/ods/odp/csv/scriba/tabula/scena launching the right route via `launchQueue`, `share_target` (POST multipart → `/share-target` route that reads the file and opens it), shortcuts (New document/workbook/presentation)), update flow (new SW → toast \"Update available — Reload\"), offline page behaviour (Hub works offline; recents open from IndexedDB handles), install prompt button in Hub (beforeinstallprompt) and Backstage ▸ Account, `navigator.storage.persist()` request, OPFS used for large journals when available. Deployment: `apps/web` build is also published to GitHub Pages under `docs/app/` by the release workflow? No — keep Pages for the site/manual only; document self-hosting (`pnpm --filter @officina/web build` → static folder; any static server; HTTPS required for FS Access). e2e pwa.spec.ts: production preview server; SW registers; manifest has file_handlers; go offline (context.setOffline) and reload Hub → renders; update toast appears when the SW is swapped (simulate by rebuilding with a changed constant in test). docs/manual/install-web.md.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/web build produces dist/sw.js and dist/manifest.webmanifest containing file_handlers for 10 extensions",
   "pnpm e2e --grep pwa exits 0 (offline Hub render passes)",
   "Lighthouse-equivalent checks in test: manifest has >= 2 icons (192, 512), display standalone, start_url",
   "pnpm verify exits 0; CI green"
  ],
  "gate": false
 },
 {
  "id": "WP-38",
  "title": "Landing site and manual (GitHub Pages)",
  "milestone": "M5",
  "depends_on": [
   "WP-15",
   "WP-22",
   "WP-28"
  ],
  "owns": [
   "docs/index.html",
   "docs/assets/**",
   "docs/manual/index.html",
   "docs/manual/README.md",
   "docs/manual/index.md",
   "docs/manual/faq.md",
   "docs/manual/troubleshooting.md",
   "docs/manual/formats.md",
   "docs/manual/shortcuts.md",
   "docs/manual/dev/**",
   "docs/media/ribbon-*.png",
   "docs/comparison.md",
   "docs/FAQ.md"
  ],
  "task": "Replace the placeholder landing page with the full site per §4.6, in the concord style (same token names, nav, hero with the banner, screenshot gallery placeholders referencing the exact paths WP-43 will produce (docs/media/screenshots/<app>-{light,dark}.png and phone-*.png), per-app sections (Write/Grid/Deck/Notes) with two figures each, a \"Feels like home\" section (ribbon crop images docs/media/ribbon-write.png etc. captured now with Playwright from the current build — WP-43 refreshes them — plus a shortcut table excerpt), a comparison table vs Microsoft 365 (rows: price, works offline, opens/saves DOCX/XLSX/PPTX, collaboration, version history, macros/VBA, platforms, licence, telemetry) with honest cells (e.g. macros: \"preserved, not run\"), download buttons for Windows/macOS/Linux/Android built from a `data-version` constant (v0.1.0 to be set by WP-43) linking to GitHub release assets with the exact artifact names of WP-35/WP-36, an install one-liner box with copy button, footer with links). Manual (docs/manual/*.md, rendered by the existing viewer): this WP writes index.md, faq.md, troubleshooting.md, formats.md (fidelity notes and the Compatibility Checker), shortcuts.md (link to SHORTCUTS.md), dev/architecture.md (condensed DESIGN.md), dev/testing.md and the README.md table of contents; it only LINKS to the chapters other WPs own (write.md, grid.md, deck.md from the gates; getting-started.md WP-32; notes.md WP-33; collaboration.md WP-29; building.md and install-desktop.md WP-34; install-android.md WP-35; mobile.md WP-36; install-web.md WP-37) and must not create or edit them even if they do not exist yet (WP-42 verifies all links). docs/comparison.md and docs/FAQ.md. Verify the deployed site after push: fetch https://zahakj.github.io/officina/ and the manual index. Keep every statement truthful for the current build; features not yet merged are not mentioned.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "curl -s https://zahakj.github.io/officina/ | grep -c 'Feels like home' >= 1 (after Pages rebuild; retry up to 10 minutes)",
   "curl -s -o /dev/null -w '%{http_code}' https://zahakj.github.io/officina/manual/ prints 200",
   "ls docs/manual/*.md | wc -l >= 14",
   "A Playwright script in e2e (pwa or shell project) screenshots the landing page at 1280 width light and dark into e2e/screenshots/site/ and docs/qa/visual-review-WP-38.md reviews them"
  ],
  "gate": false
 },
 {
  "id": "WP-39",
  "title": "Fidelity corpus expansion and differential testing against LibreOffice; reader/writer hardening",
  "milestone": "M6",
  "depends_on": [
   "WP-15",
   "WP-22",
   "WP-28"
  ],
  "owns": [
   "packages/testing/**",
   "packages/io-docx/src/**",
   "packages/io-xlsx/src/**",
   "packages/io-pptx/src/**",
   "packages/io-docx/test/**",
   "packages/io-xlsx/test/**",
   "packages/io-pptx/test/**",
   "docs/qa/fidelity.md",
   "docs/FORMATS.md"
  ],
  "task": "Expand the corpus: 15 hand-authored `manual/` feature files per format created with LibreOffice headless macros or the Python oracles covering tricky constructs (nested tables, floating tables, multi-section headers, numbered headings with TOC, footnotes spanning pages, RTL mixed runs, equations; xlsx: shared formulas across thousands of cells, array formulas legacy, CF with formulas referencing other cells, 1904 dates, merged header blocks, very long text, rich-text cells, external link cached values, hidden sheets; pptx: SmartArt fallback, custom geometry, connectors with arrows, grouped placeholders, picture crops with transparency, tables with merged cells and styles, charts of every type, slide-level theme overrides), the `fixtures/external` fetch script run and results included in the score run (gitignored inputs; scores committed). Build the differential harness: for each fixture, (1) our read→write→LibreOffice PDF vs original→LibreOffice PDF visual score per page, (2) LibreOffice converting the original to our format family round trip (e.g. docx→odt→docx) as a baseline so we can show \"LibreOffice's own round-trip score\" next to ours, (3) exceljs/openpyxl/python-docx/python-pptx differential reads. Produce `docs/qa/fidelity.md` with tables per format and a ranked list of the worst files with root causes. Fix reader/writer defects until thresholds in §3.5 hold across the expanded corpus (prioritise by frequency of the construct in real documents); raise warnings coverage; make readers resilient to malformed files (fuzz 200 mutated zips/XMLs per format — must never throw outside a `FormatError` with a message). Update docs/FORMATS.md with supported/preserved/unsupported matrices per format.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm fidelity prints tables for docx/xlsx/pptx where every fixture (features/ and manual/) meets the §3.5 thresholds, and docs/qa/fidelity.md is regenerated",
   "A fuzz test (600 mutated inputs) passes: readers throw only FormatError",
   "docs/FORMATS.md contains a supported/preserved/unsupported matrix for each format",
   "pnpm verify exits 0; CI green"
  ],
  "gate": false
 },
 {
  "id": "WP-40",
  "title": "Performance pass: benchmarks, workers, progressive loading, memory",
  "milestone": "M6",
  "depends_on": [
   "WP-15",
   "WP-22",
   "WP-28"
  ],
  "owns": [
   "bench/**",
   "packages/formula/src/engine/perf/**",
   "packages/sheet-model/src/perf/**",
   "packages/editor-grid/src/perf/**",
   "packages/editor-write/src/pagination/perf/**",
   "packages/editor-deck/src/perf/**",
   "docs/qa/perf.md",
   "e2e/specs/perf.spec.ts"
  ],
  "task": "Create `bench/` (Vitest bench + Playwright timing specs) implementing every row of §3.9 as a reproducible measurement with fixture generators (500k-cell xlsx, 100k-formula workbook, 300-page docx, 200-slide pptx produced by tools/make-fixtures.py --large into fixtures/large, gitignored). Measure, then optimise within the perf-named directories (which may contain replacement implementations the owning packages import through a single seam file each: e.g. `packages/editor-grid/src/perf/index.ts` exports tuned renderer helpers; owning-package files outside perf/ may be edited only to switch to the seam — note it in the commit): formula engine (typed-array dependency storage, batch dirty marking, worker transfer batching), sheet-model (chunk pooling), grid renderer (dirty-rect redraw, glyph atlas for digits, offscreen canvas for headers), Write pagination (incremental re-layout of only affected pages, measurement batching, idle-time pagination of far pages, virtualised page DOM for Print Layout beyond ±5 pages), Deck (thumbnail generation throttling, static HTML caching for text bodies), parsing (streaming sheetData parse in the worker, transferable buffers), startup (code splitting per app route, font loading strategy, preloading of the Hub). Add memory measurements via `performance.measureUserAgentSpecificMemory` where available / Chromium `--enable-precise-memory-info`. Record before/after in docs/qa/perf.md; perf.spec.ts asserts the §3.9 targets with 20 % tolerance for CI noise.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm bench exits 0 and docs/qa/perf.md lists every §3.9 scenario with measured values meeting the targets (±20 %)",
   "pnpm e2e --grep perf exits 0",
   "Opening fixtures/large/300-pages.docx in e2e shows the first page in < 3 s (measured from navigation to first page paint)",
   "pnpm verify exits 0; CI green"
  ],
  "gate": false
 },
 {
  "id": "WP-41",
  "title": "ODF import/export (odt, ods, odp)",
  "milestone": "M6",
  "depends_on": [
   "WP-13",
   "WP-15",
   "WP-18",
   "WP-22",
   "WP-26",
   "WP-28"
  ],
  "owns": [
   "packages/io-odf/**",
   "apps/web/src/formats/odf.ts",
   "e2e/specs/odf.spec.ts"
  ],
  "task": "packages/io-odf: ODF 1.3 package handling (manifest.xml, mimetype first/uncompressed, content.xml, styles.xml, meta.xml, settings.xml passthrough), automatic vs common styles, `text:` elements → doc-model (paragraphs, spans, lists with `text:list-style`, tables, sections, headers/footers via master pages, footnotes, images (`draw:frame`), fields, tracked changes (`text:tracked-changes`), bookmarks, links), doc-model → ODT with the same coverage; `table:` → sheet-model (cells with office:value-type, formulas in `of:` syntax converted to Excel syntax and back (function names/separators), styles via `style:` number formats mapped to Excel codes, merges, column/row styles, conditional formats basic, validations, named expressions, charts via a ODF chart subset ↔ charts package) and back; `draw:page` → deck-model (frames, custom shapes with `draw:enhanced-geometry` mapped to presets where names match, text, pictures, tables, notes, transitions subset) and back. Register the formats in the apps' open/save pickers via apps/web/src/formats/odf.ts (a small registration module imported by each app's integration; the apps' own files are not edited — expose `registerOdf()` and call it from apps/web/src/main.tsx through a handoff note if an import line is needed). Tests: LibreOffice oracle round-trips — convert each docx/xlsx/pptx features fixture to ODF with soffice, read with io-odf, compare text/values with our OOXML reads; write ODF from our models and have LibreOffice convert back to OOXML, then read with our OOXML readers and compare (text 1.00, structural ≥ 0.85). e2e odf.spec.ts: open an .odt/.ods/.odp, screenshots, Save As .odt and re-read.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm --filter @officina/io-odf test exits 0 with >= 90 tests",
   "pnpm fidelity --format odf prints text == 1.00 and structural >= 0.85 for the converted corpus",
   "pnpm e2e --grep odf exits 0 with screenshots odf/{odt,ods,odp}-light-desktop.png",
   "pnpm verify exits 0; CI green"
  ],
  "gate": false
 },
 {
  "id": "WP-42",
  "title": "Integration gate: full suite verification, visual regression baselines, defect sweep, DoD checklist",
  "milestone": "M6",
  "depends_on": [
   "WP-29",
   "WP-30",
   "WP-31",
   "WP-32",
   "WP-33",
   "WP-34",
   "WP-35",
   "WP-36",
   "WP-37",
   "WP-38",
   "WP-39",
   "WP-40",
   "WP-41"
  ],
  "owns": [
   "e2e/**",
   "docs/qa/**",
   "**/* (defect fixes anywhere, as the only running WP)"
  ],
  "task": "Runs alone. Execute the complete verification: `pnpm verify`, `pnpm e2e` (all specs, both themes), `pnpm fidelity`, `pnpm bench`, `pnpm release:desktop -- --linux`, `pnpm release:android`, APK verification script, PWA build, collab server start/health, licence check, `pnpm audit --prod` (document unfixable advisories). Open and visually review every screenshot produced (hundreds): record findings in docs/qa/visual-review-WP-42.md and fix all defects of severity \"visible to a user in normal use\" anywhere in the repo (you are the only running WP). Create visual regression baselines `e2e/screenshots/baseline/` for the gate screenshots (one per app/view/theme, ~40 files) and enable the pixelmatch comparison in CI (threshold per §5). Run the §10 Definition of Done checklist item by item and write docs/qa/dod.md with evidence (command + result) for each item; anything failing is fixed or, if impossible, listed in docs/qa/known-issues.md with a GitHub issue created via `gh issue create` (labels: bug/enhancement, milestone v0.2.0). Ensure the unlisted-feature rule holds: no \"coming soon\" stubs visible in release builds (grep for the `pending` tier marker in built output). Update parity/parity.json: flip every row whose delivering WP is complete and whose verify ids pass to done (or better where the notes justify it), run `pnpm parity:verify` and `pnpm parity`, and commit PARITY.md with the v0.1.0 score; rows whose WP shipped but whose verification fails stay partial/missing and get a known-issue entry. Update CHANGELOG.md Unreleased into a complete v0.1.0 section draft including the parity score line.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "pnpm verify && pnpm e2e && pnpm fidelity && pnpm bench all exit 0 in one session (log attached as docs/qa/gate-WP-42.log)",
   "e2e/screenshots/baseline/ contains >= 40 PNGs and CI runs the visual comparison successfully",
   "docs/qa/dod.md lists every §10 item with PASS and evidence; docs/qa/known-issues.md lists any deferred items with issue URLs",
   "pnpm parity:verify exits 0 and PARITY.md shows >= 85 % of critical rows done or better",
   "gh run list --workflow ci.yml --limit 1 --json conclusion -q '.[0].conclusion' prints success"
  ],
  "gate": true
 },
 {
  "id": "WP-43",
  "title": "Release v0.1.0: real screenshots and demo GIF, README and landing finalised, tagged release with Linux and Android assets",
  "milestone": "M6",
  "depends_on": [
   "WP-42"
  ],
  "owns": [
   "docs/media/screenshots/**",
   "docs/media/demo.gif",
   "docs/media/ribbon-*.png",
   "docs/media/banner*.png",
   "README.md",
   "docs/index.html",
   "CHANGELOG.md",
   "e2e/specs/media-capture.spec.ts",
   "tools/make-demo-gif.sh",
   "package.json (version field only)",
   "apps/*/package.json (version field only)",
   "packages/*/package.json (version field only)"
  ],
  "task": "Runs alone, last. Media: e2e/specs/media-capture.spec.ts at device scale 2 captures with real template documents (from WP-32) — Write (light/dark), Grid with a chart (light/dark), Deck with Design Ideas pane (light/dark), Notes (light), Hub (light/dark), phone captures (Pixel 7) of Write/Grid/Deck, ribbon crops for each app — into docs/media/screenshots/ with the exact names referenced by README/landing; tools/make-demo-gif.sh records a 20–30 s Playwright video (open Hub → open template → edit with ribbon → palette → save) and converts it to docs/media/demo.gif with ffmpeg (palette-optimised, 960 px wide, 12 fps, < 8 MB). Finalise README.md (house style per §4.6; replace placeholders; add the parity badge from docs/parity-badge.json via shields endpoint URL; the install one-liner must point at the real asset names) and docs/index.html (`data-version` = v0.1.0, download links valid, screenshots in place), re-render banners if the icon changed (`pnpm media:render`). Versions: set 0.1.0 in every package.json (`pnpm -r exec npm version 0.1.0 --no-git-tag-version` or a script), CHANGELOG.md v0.1.0 section dated with the parity score and fidelity table. Build local assets: `pnpm release:desktop -- --linux` (AppImage, deb), `pnpm release:android` (signed APK); compute SHA256SUMS-local.txt. Commit, push, wait for CI green, then tag: `git tag -a v0.1.0 -m \"Officina 0.1.0\"` and `git push origin v0.1.0`; create the release first (so CI appends): `gh release create v0.1.0 --title \"Officina 0.1.0\" --notes-file <generated from CHANGELOG> apps/desktop/dist/officina-0.1.0-linux-x86_64.AppImage apps/desktop/dist/officina_0.1.0_amd64.deb apps/android/dist/officina-0.1.0-android.apk`; watch release.yml (`gh run watch`), confirm macOS/Windows assets and the final SHA256SUMS.txt were attached by the workflow (if a CI platform job fails for environmental reasons, fix the workflow and re-run with `gh run rerun`; the release must end with all six platform assets or a documented exception in the release notes). Verify Pages serves the finalised landing page and that every download button URL returns HTTP 302/200. Pin nothing. Post-release: bump versions to 0.2.0-dev in a final commit.\nFinish with the §4.7 cadence: pnpm verify green, CHANGELOG Unreleased updated, conventional commit, rebase, push to main, CI green (gh run watch --exit-status).",
  "acceptance": [
   "gh release view v0.1.0 --json assets -q '.assets[].name' lists officina-0.1.0-linux-x86_64.AppImage, officina_0.1.0_amd64.deb, officina-0.1.0-android.apk, Officina-Setup-0.1.0.exe, Officina-0.1.0-arm64.dmg (or documented exception), SHA256SUMS.txt",
   "for each asset URL from the release: curl -sIL returns 200",
   "ls docs/media/screenshots/*.png | wc -l >= 14 and identify docs/media/demo.gif reports width 960 and size < 8 MB",
   "curl -s https://zahakj.github.io/officina/ | grep -c 'v0.1.0' >= 1 and README.md no longer contains the word 'placeholder'",
   "gh run list --workflow release.yml --limit 1 --json conclusion -q '.[0].conclusion' prints success"
  ],
  "gate": true
 }
]
```
