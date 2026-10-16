# Round-trip fidelity

Written by `cargo xtask fidelity` (`kiln-cli fidelity`, scorer in `crates/kiln-testing/src/fidelity/`). Each file is read, written back without edits and read again; the metrics and thresholds are those of PLAN.md §3.10 and §5, per file from `parity/corpus.json` (the §3.10 release targets for files it does not list). Do not edit by hand: rerun the command.

- Formats: docx
- Oracle: LibreOffice (`/usr/bin/soffice`) through `tools/oracle-soffice.sh`; pages rendered by `pdftoppm` at 60 dpi
- Result: **pass**

## Summary

Lowest value of any file of the format (— = not measured).

| Format | Adapter | Files | Passing | structural | text | visual | values | formulas | oracle_opens | byte_identical_untouched |
|---|---|---|---|---|---|---|---|---|---|---|
| docx | kiln-io-docx | 31 | 31 | 1.000 | 1.000 | 0.916 | — | — | yes | yes |

## Files

| File | Result | structural | text | visual | values | formulas | oracle_opens | byte_identical_untouched | Notes |
|---|---|---|---|---|---|---|---|---|---|
| `docx/features/breaks.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/comments.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/fields.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/footnotes.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/headers-footers.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/hyperlinks.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/images-anchored.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/images-inline.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/long-text.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/numbering.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/page-setup.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/paragraphs.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/properties.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/rtl.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/sections.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/styles.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/tables.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/text-boxes.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/features/tracked-changes.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `docx/manual/foreign-prefixes.docx` | pass | 1.000 | 1.000 | 0.916 | — | — | yes | yes |  |
| `docx/manual/minimal.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `external/docx/poi-ComplexNumberedLists.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `external/docx/poi-FieldCodes.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `external/docx/poi-Headers.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `external/docx/poi-Numbering.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `external/docx/poi-SampleDoc.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `external/docx/poi-Styles.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `external/docx/poi-deep-table-cell.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `external/docx/poi-endnotes.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `external/docx/poi-footnotes.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |
| `external/docx/poi-headerPic.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes |  |

## OPC passthrough baseline

Every package opened and saved by `kiln-ooxml` without edits (what `kiln-cli repack` does): every part, `.rels` parts included, must come out byte-identical (PLAN.md §3.1 principle 4), and `[Content_Types].xml` must declare the same content types (it is parsed on open, so it is compared by its declarations and not counted among the parts below).

| Fixtures | Packages | Parts | Identical parts | Result |
|---|---|---|---|---|
| `docx/features` | 19 | 325 | 325 | pass |
| `docx/manual` | 2 | 10 | 10 | pass |
| `external/docx` | 10 | 123 | 123 | pass |
