# @kiln-ref/doc-model

The Kiln Write document model: a ProseMirror schema that mirrors WordprocessingML, the style sheet
with Word-accurate formatting resolution, list numbering, sections, fields, theme fonts and colours,
the Office-to-bundled font substitution table, the default "Blank document" template, plain-text/JSON
(de)serialisation, document statistics and the editing commands used by `@kiln/editor-write`.

All lengths keep the file's units (twips, half-points, eighths of a point, EMU); `units.ts` converts to
pt/px/cm for display.

## Public API

| Module | Exports |
|---|---|
| `schema.ts` | `createDocSchema()`, `docSchema` — `doc > section+ footnote*`, `section > (header\|footer)* block+`, paragraphs, prosemirror-tables-compatible tables, `toc`, and the inline nodes `tab`, `softBreak`, `pageBreak`, `columnBreak`, `image`, `shape`, `field`, `footnoteRef`, `equation`, `bookmarkStart/End`, `opaqueInline`; marks for every run property plus `link`, `comment`, `insertion`, `deletion`, `formatChange`, `spellError`. |
| `types.ts` | `ParagraphProps`, `RunProps`, `TableProps`, `TableCellProps`, `SectionProps`, `Color`, `Border`, `Shading`, `TabStop`, `DocSettings`, … |
| `styles/` | `StyleSheet` (paragraph/character/table/numbering styles, `basedOn`/`next`/`link`, docDefaults), `resolveParagraphProps`, `resolveRunProps`, `resolveTableProps`, `resolveCellProps`, `resolveAt`, `tableConditions`, merge helpers. |
| `numbering/` | `Numbering` (abstract definitions, instances, overrides, `restart`, `createBulletList/NumberedList/MultilevelList`), `ListCounter`, `formatLevelText`, `formatNumber`, `bulletDisplayText`, `computeListLabels(doc, ctx)`. |
| `sections.ts` | paper sizes and locale defaults, margins presets, orientation, content/column widths, header/footer kind and "link to previous" resolution, page-number restarts. |
| `fields.ts` | `parseFieldInstruction` (typed PAGE, NUMPAGES, DATE/TIME, TOC, REF, PAGEREF, HYPERLINK, SEQ; unknown preserved), `buildField`, `evaluateField`, `formatFieldDate`, `applyNumberSwitches`. |
| `theme.ts` | `resolveDocColor` (theme colours with Word tint/shade), `resolveRunFonts` (theme fonts), `resolveShadingFill`. |
| `fonts/substitution.ts` | `FONT_SUBSTITUTIONS`, `substituteFont`, `fontStack(name)` for CSS, `fontsWithLayoutRisk`. |
| `template.ts` | `DefaultTemplate.create({ locale, paper })` → `WriteDocument`, `createDefaultStyleSheet()`. |
| `serialize.ts` | `toPlainText`, `fromPlainText`, `serializeDocument`/`parseDocument` (stable, diff-friendly JSON). |
| `stats.ts` | `statistics`, `countWords`, `countCharacters`, `countParagraphs`, `countPages(layout)`. |
| `commands.ts` | `createDocCommands(ctx)` → ProseMirror commands: apply style, toggle bold/italic/… against effective formatting, underline, super/subscript, fonts and sizes, alignment, spacing, indent/outdent (list levels), bullets/numbering/restart, clear formatting, change case, insert table, rows/columns, merge/split cells, page and section breaks, page setup. |

## Inheritance order

`docDefaults → table style (whole table, bands, first/last column, header row, corners) → paragraph
style chain → numbering level → character style chain → direct formatting`. Toggle properties
(bold, italic, caps, …) applied by a character style over a paragraph that already has them switch
off, as in Word. Numbering `w:rPr` formats the list label only, never the paragraph's text.

Scripts: `pnpm lint`, `pnpm typecheck`, `pnpm test`.

Licence: AGPL-3.0-or-later.
