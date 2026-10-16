# kiln-doc

The Kiln Write document model, shaped like WordprocessingML (ECMA-376 Part 1 §17), ported from the v1.0 TypeScript `doc-model` (WP-61). A `Document` holds sections of blocks (paragraphs, tables with `gridSpan`/`vMerge`, content controls, multi-paragraph field results, passthrough XML), header/footer stories referenced per section and kind, footnotes and endnotes, threaded comments, the style sheet, list numbering, settings, document properties, the theme and media. Paragraph text lives in a `ropey` rope with formatting runs (`RunProps` = `w:rPr`), overlapping marks (hyperlinks, comment ranges, tracked insertions, deletions, moves and format changes, inline content controls) and inline objects (breaks, drawings with inline/anchored placement and wrapping, simple and complex fields, note references, symbols, equations kept as OMML, bookmarks). Every block-level node has a stable `NodeId`. Formatting resolves the way Word does: document defaults, table style regions, the paragraph style chain, numbering, character styles with toggle properties, direct formatting, theme fonts and colours with Word's tint/shade arithmetic. Every change is a `Transaction` of `Op`s whose inverses restore the document exactly (property-tested over random op sequences); `UndoStack` groups typing per word. No layout here: `kiln-doc-layout` turns the model into pages.

## Public API

| Module | What it gives |
|---|---|
| `model` | `Document` (`new`, `with_options`, `alloc_id`, `paragraph`, `table`, `row`, `cell`, `container`, `parent_of`, `enclosing_cell`, `body_paragraphs`, `paragraphs_in`, `validate`), `Section`, `Block`, `Paragraph`, `Table`/`Row`/`Cell` (`cell_map`), `SdtBlock`, `FieldBlock`, `OpaqueBlock`, `HeaderFooter`, `Note`, `Comment`, `Drawing` (`Placement::Inline`/`Anchor`) |
| `inline` | `Inlines` (rope + runs + marks + objects), `Fragment`, `RunSpan`, `Mark`, `MarkSpan`, `InlineObject`, `Hyperlink`, `NoteRef`, `Break`, `Symbol`, `Equation` |
| `props`, `st` | `ParagraphProps`, `RunProps`, `TableProps`, `TableRowProps`, `TableCellProps`, borders, shading, tabs, colours (`DocColor`, `ColorValue`), `ToggleProp`; every `ST_*` enumeration with its schema spelling |
| `style`, `template` | `StyleSheet` (`chain`, `next_style_id`, `linked`, `quick_styles`, `get_by_name`, `unique_id`, `update`, `remove`), `Style`, `DocDefaults`; Word 2016–2021 defaults (`default_style_sheet`, `Document::new`) |
| `numbering` | `Numbering` (abstract definitions, instances, overrides, `restart`, `create_bullet_list`/`numbered_list`/`multilevel_list`), `ListCounter`, `format_level_text`, `formats::format_number`, `bullet_display_text` |
| `resolve`, `theme`, `merge` | `resolve_paragraph`, `resolve_run`, `resolve_table`, `resolve_cell`, `table_conditions`, `effective_numbering`, `Document::context`/`resolve_paragraph`/`resolve_run_at`; `DocTheme` (`office`, `from_ooxml`), `resolve_color`, `resolve_run_fonts`, `apply_tint_shade` |
| `fields` | `Field`, `parse_field_instruction` (PAGE, NUMPAGES, SECTIONPAGES, SECTION, DATE/TIME family, TOC, REF, PAGEREF, NOTEREF, HYPERLINK, SEQ; anything else kept verbatim), `build_field`, `evaluate_field`, `format_field_date`, `apply_number_switches` |
| `section`, `settings`, `revision`, `sdt` | `SectionProps`, `Paper`, margin presets, `HeaderFooterRefs`, `resolve_header_footer`, `section_start_page_numbers`; `Settings`, `DocProperties`, `Media`; `RevisionInfo`, `ParagraphRevision`; `SdtInfo` |
| `ops`, `undo`, `pos` | `Op` (paragraph content, marks, objects, split/merge, blocks, tables, sections, stories, comments, styles, numbering, settings, theme, media), `Transaction`, `Document::apply` (atomic, returns the inverse), `UndoStack`, `Pos`, `Range`, `Document::compare` |
| `edit` | Transaction builders: `insert_text`, `delete_range`, `split_paragraph`, `format_runs`, `toggle`, `format_paragraphs`, `insert_table`, `insert_row`/`delete_row`, `insert_column`/`delete_column`, `add_comment`/`delete_comment`, `insert_section_break`, `resolve_revision` |
| `query` | `outline`, `statistics` (Word's word and character counts), `comment_threads`, `revisions`, `list_labels`, `bookmarks`, `bookmark_text`, `plain_text` |
| serialisation | `Document::to_bytes`/`from_bytes` (postcard behind a `KDOC` header; validated on load), `Transaction::to_bytes`/`from_bytes` |

Node lookup walks the document (no index yet): an op costs about 0.3 ms in a 5,000-paragraph document in an unoptimised build, which is far inside the typing budget; an id index can be added behind the same API if layout profiling asks for it.

Licence: AGPL-3.0-or-later.
