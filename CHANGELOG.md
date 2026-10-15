# Changelog

All notable changes to Officina are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Monorepo scaffold: pnpm workspace with Turborepo, TypeScript, Biome, Vitest and Playwright; the four apps (web, desktop, Android, collaboration server) and the library packages; a licence gate for every shipped dependency.
- Repository hygiene: AGPL-3.0-or-later licence, README, contributing guide, security policy, Code of Conduct, issue and pull request templates, CODEOWNERS and Dependabot.
- Continuous integration (lint, typecheck, unit tests with coverage, licence check, end-to-end tests) and a release workflow that builds Linux, Windows, macOS and Android packages from a tag and attaches them with `SHA256SUMS.txt`.
- The Officina mark, the README banner in dark and light, the social preview image and the application icons, all rendered from SVG by `pnpm media:render`.
- Placeholder landing page and Markdown manual viewer on GitHub Pages.
- Reference documents: design overview, file-format fidelity notes, keyboard shortcuts and the release runbook.
- "Horizon" design tokens: light, dark and high-contrast themes (following the OS by default), per-app accent colours, spacing, radii, shadows, motion and type scale as CSS variables, a typed TypeScript object and a Tailwind 4 theme; the theme choice is remembered.
- Bundled fonts: Inter and JetBrains Mono for the interface, and the metric-compatible document fonts Liberation Sans/Serif/Mono, Carlito and Caladea so Office documents lay out like they do in Word.
- UI building blocks shared by every app: buttons (including Classic-ribbon large and split buttons), toggles, menus with submenus and shortcuts, tooltips with Office-style descriptions, dialogs, tabs, drop-down lists, an editable font/size box, unit-aware number fields, an Office-style colour picker with theme tints and recent colours, a zoom slider, a styles gallery, toasts, keyboard keycaps and scroll areas, all keyboard accessible.
- Officina Glyphs: 275 icons for Office commands (Format Painter, Merge & Center, Freeze Panes, Track Changes, slide layouts…), with an optional colourful style.
- The Office window: title bar with AutoSave, Save/Undo/Redo, rename-in-place and a Search box; the ribbon in Classic (three rows) or Simplified (one row) layout, chosen by window width, collapsible with Ctrl+F1, with contextual tabs, group dialog launchers and ScreenTips that show the shortcut and the Office name; the File menu (Backstage) in Office's order; a status bar with view buttons, a zoom slider and zoom presets, customisable from its right-click menu; task panes that dock left or right, resize and stack as tabs; the mini toolbar on selection; right-click menus.
- Every Office shortcut from the keyboard reference is mapped for Scriba, Tabula and Scena (including slide-show keys), with ⌘ on macOS; Key Tips (press Alt or F10, then the letters) reach every ribbon command.
- Command palette (Ctrl+Shift+P or Alt+Q): finds any command by its name, its Office name or everyday words ("merge and center", "freeze panes", "strikeout"), lists recent commands and shows shortcuts.
- File ▸ Options with General, Display, Proofing, Save, Language, Keyboard and Privacy pages; Keyboard lists every command and lets you reassign shortcuts the way Office's Customize Keyboard does. A Ctrl+/ cheat sheet lists the current app's shortcuts.
- Hooks for an optional writing assistant (summarise, rewrite, formula from a description, slides from an outline); none is configured by default and nothing is sent anywhere.
- Office Open XML core (`@officina/ooxml`): reads and writes the zip container of .docx, .xlsx and .pptx files, leaving every part it does not change byte-for-byte identical; refuses zip bombs and recognises password-protected files.
- Theme colours, fills, outlines, shadows, glow and reflection, and text formatting from DrawingML, resolved the way Office resolves them (theme tints and shades included).
- All 187 Office preset shapes (rectangles, arrows, callouts, flowchart symbols, stars, banners, action buttons…) drawn from their ECMA-376 definitions, with adjustment handles, text areas and connection points.
- Parts of a file Officina does not understand yet (macros, custom XML, embedded objects) are kept and written back on save instead of being dropped; document properties (title, author, custom fields) are read and written.
- Test corpus: 58 generated Word, Excel and PowerPoint feature files plus hand-made edge cases, a script that downloads 30 real-world files, and checks against LibreOffice, ExcelJS, mammoth and the Python Office libraries, with a round-trip fidelity scorer.
- Scriba document model (`@officina/doc-model`): documents keep Word's own structure and units (sections, paragraph and character formatting, tables with merged cells, headers and footers, footnotes, fields, bookmarks, equations, images and shapes, comments and tracked changes), so nothing is rounded or lost when a .docx is opened and saved.
- Styles behave like Word's: Normal, Heading 1–6, Title, Subtitle, Quote, List Paragraph, Caption, TOC 1–3, Header/Footer, Hyperlink, Strong, Emphasis and Table Grid with the Word 2016+ defaults (Calibri 11 pt, 1.08 lines, 8 pt after), and formatting is inherited in Word's order — table style, paragraph style, list level, character style, direct formatting — including Word's rule that Emphasis inside an italic paragraph shows upright.
- Lists number exactly as Word numbers them: multilevel and legal numbering, restart and continue, start values, bullets from Symbol and Wingdings fonts, Roman, letter, ordinal, word and East Asian formats.
- Field codes (PAGE, NUMPAGES, DATE/TIME with date pictures, TOC, REF, PAGEREF, HYPERLINK, SEQ) are understood, and unknown fields are kept unchanged.
- Office fonts are replaced by bundled look-alikes with identical spacing where one exists (Calibri → Carlito, Cambria → Caladea, Arial/Times New Roman/Courier New → Liberation), and fonts that may shift the layout (such as Aptos) are reported.
- Word count statistics as in Word (words, characters with and without spaces, paragraphs; East Asian characters counted one per word), and editing commands for styles, bold/italic toggling that respects styles, font size steps, case changes, lists and indents, tables (insert, rows, columns, merge, split), page and section breaks and page setup.
- Formula engine (`@officina/formula`): Excel formula syntax including structured table references, 3-D and whole-column references, array constants, `@` and the `#` spill operator; formulas keep their own spacing when rows, columns or sheets change.
- Fast, incremental recalculation: only what changed is recalculated (a 100,000-cell chain in about 0.2 s), with circular reference detection and optional iterative calculation.
- Dynamic arrays that spill into neighbouring cells, with `#SPILL!` when something is in the way, plus LET, LAMBDA (including named and recursive lambdas), MAP, REDUCE, SCAN, BYROW, BYCOL and MAKEARRAY.
- A core set of 47 native functions (SUM, IF, XLOOKUP, FILTER, SORT, UNIQUE, SEQUENCE, TEXTJOIN, INDIRECT…) with Excel-compatible text-to-number, date and comparison rules, and a step-by-step evaluation tree for the upcoming Formula Lab.

[Unreleased]: https://github.com/ZahakJ/officina/commits/main
