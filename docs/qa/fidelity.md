# Round-trip fidelity

Written by `cargo xtask fidelity` (`kiln-cli fidelity`, scorer in `crates/kiln-testing/src/fidelity/`). Each file is read, written back without edits and read again; the metrics and thresholds come per file from `parity/corpus.json` (the release targets for files it does not list). Do not edit by hand: rerun the command.

- Formats: docx, xlsx, pptx, odf
- Oracle: LibreOffice (`soffice`) through `tools/oracle-soffice.sh`; pages rendered by `pdftoppm` at 60 dpi
- Corpus: 336 files, 150 real-world and 186 generated or made by hand
- Compared: Kiln's reader on the file before and after (`structural`, facts in Kiln's own model; `text`; `formulas`), the stored values in the two files' XML (`values`, not recalculated; for ODF, through Kiln's reader), LibreOffice's rendering at 60 dpi (`visual`; the original and its round trip are rendered from one path, the same in every run and checkout, because a document can show its own path in a field) and the kinds of element in the raw XML (`raw_elements`). Nothing renders in Office.
- Stamps: docx=fc7ef2c489a7d15af3a6873d498285ed, xlsx=fc7ef2c489a7d15af3a6873d498285ed, pptx=fc7ef2c489a7d15af3a6873d498285ed, odf=fc7ef2c489a7d15af3a6873d498285ed
- Result: **pass**

## Summary

Lowest value of any file of the format (— = not measured).

| Format | Adapter | Files | Passing | structural | text | visual | values | formulas | oracle_opens | byte_identical_untouched | raw_elements |
|---|---|---|---|---|---|---|---|---|---|---|---|
| docx | kiln-io-docx | 90 | 90 | 1.000 | 1.000 | 0.916 | — | — | yes | yes | 0.978 |
| xlsx | kiln-io-xlsx | 102 | 102 | 1.000 | 1.000 | 0.900 | 1.000 | 1.000 | yes | yes | 0.857 |
| pptx | kiln-io-pptx | 81 | 81 | 1.000 | 1.000 | 0.999 | — | — | yes | yes | 0.964 |
| odf | kiln-io-odf | 63 | 63 | 0.979 | 1.000 | 0.812 | 1.000 | 1.000 | yes | — | — |

## Files

| File | Result | structural | text | visual | values | formulas | oracle_opens | byte_identical_untouched | raw_elements | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| `docx/features/arabic-document.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/breaks.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/cjk-document.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/columns-footnotes.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/comments.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/content-controls.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/drop-caps-tabs.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/embedded-chart.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/equations.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/fields.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/floating-tables.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/footer-control.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/footnotes.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/form-fields.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/headers-footers.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/hyperlinks.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/images-anchored.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/images-inline.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/large-table.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/legal-numbering.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/long-text.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/mixed-sections.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.997 |  |
| `docx/features/nested-fields.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/nested-tables.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/numbering.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/page-borders.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/page-setup.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/paragraphs.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/properties.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/rtl.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/section-headers.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/sections.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.998 |  |
| `docx/features/styles.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/tables.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/text-boxes.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/tracked-changes.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/tracked-formatting.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/features/vml-fallbacks.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `docx/manual/foreign-prefixes.docx` | pass | 1.000 | 1.000 | 0.916 | — | — | yes | yes | 1.000 |  |
| `docx/manual/minimal.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-2129393649.docx` | pass | 1.000 | 1.000 | 0.987 | — | — | yes | yes | 0.999 |  |
| `external/docx/lo-TOC_field_b.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.997 |  |
| `external/docx/lo-chart-size.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-clearing-break.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-dashed_line_custdash_1000th_of_percent.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-fdo76586.docx` | pass | 1.000 | 1.000 | 0.989 | — | — | yes | yes | 0.993 |  |
| `external/docx/lo-fdo80997.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-floating-table-position.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-grabbag.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-n652364.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-n816593.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.978 |  |
| `external/docx/lo-numbering1.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-para-auto-spacing.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-table-rtl.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-tdf126590.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-tdf143476_lockedCanvas_position.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-tdf159158_zOrder_1and0equalB.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.994 |  |
| `external/docx/lo-tdf170003_bottomSpacing.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-tdf48658_transparentOLEheader.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/lo-test_msword_hang.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.986 |  |
| `external/docx/poi-55966.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-58067.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-Bug54771b.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-Bug55142.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-Bug60337.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-ComplexNumberedLists.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-EnforcedWith.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-FieldCodes.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-FldSimple.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-Headers.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-Numbering.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-OPCCompliance_CoreProperties_SUCCESS.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-OPCCompliance_CoreProperties_UnauthorizedXMLLangAttributeFAIL.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-OPCCompliance_DerivedPartNameFAIL.docx` | pass | 1.000 | 1.000 | — | — | — | yes | yes | 1.000 | visual: the original does not convert: LibreOffice could not convert poi-OPCCompliance_DerivedPartNameFAIL.docx to pdf: Error: source file could not be loaded |
| `external/docx/poi-Office2010-SP1-XAdES-X-L.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-SampleDoc.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-Styles.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-ThreeColHeadFoot.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-bug56075-changeTracking_on.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-bug65738.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.994 |  |
| `external/docx/poi-deep-table-cell.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-documentProtection_no_protection.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-endnotes.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-footnotes.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-headerPic.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-issue_51265_2.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-sample.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-stress005.docx` | pass | 1.000 | 1.000 | 0.990 | — | — | yes | yes | 1.000 |  |
| `external/docx/poi-stress018.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.996 |  |
| `external/docx/poi-testComment.docx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.994 |  |
| `xlsx/features/autofilter.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.990 |  |
| `xlsx/features/cf-all-types.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.980 |  |
| `xlsx/features/charts.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.987 |  |
| `xlsx/features/cjk-sheet.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.978 |  |
| `xlsx/features/comments.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.979 |  |
| `xlsx/features/conditional-formatting.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.990 |  |
| `xlsx/features/data-validation.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.978 |  |
| `xlsx/features/date1904-formats.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.989 |  |
| `xlsx/features/date1904.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.989 |  |
| `xlsx/features/defined-names.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.989 |  |
| `xlsx/features/formula-matrix.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.978 |  |
| `xlsx/features/formulas.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.989 |  |
| `xlsx/features/freeze-panes.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.978 |  |
| `xlsx/features/hidden-rows-cols.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.978 |  |
| `xlsx/features/hyperlinks.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.978 |  |
| `xlsx/features/images.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.981 |  |
| `xlsx/features/large-table.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.980 |  |
| `xlsx/features/locale-formats.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.989 |  |
| `xlsx/features/merges.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.978 |  |
| `xlsx/features/multi-sheet.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.978 |  |
| `xlsx/features/number-formats.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.989 |  |
| `xlsx/features/outlines.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.978 |  |
| `xlsx/features/pivot-multi-fields.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.983 |  |
| `xlsx/features/pivot-tables.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.984 |  |
| `xlsx/features/print-setup.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.990 |  |
| `xlsx/features/protection.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.978 |  |
| `xlsx/features/rich-text.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.979 |  |
| `xlsx/features/rtl-sheet.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.978 |  |
| `xlsx/features/shared-array-formulas.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.980 |  |
| `xlsx/features/slicers.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.981 |  |
| `xlsx/features/sparklines.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.981 |  |
| `xlsx/features/structured-refs.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.979 |  |
| `xlsx/features/styles.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.980 |  |
| `xlsx/features/tables.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.979 |  |
| `xlsx/features/validation-types.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.978 |  |
| `xlsx/manual/beta-strings.xlsx` | pass | 1.000 | 1.000 | 0.999 | 1.000 | 1.000 | yes | yes | 0.857 |  |
| `xlsx/manual/minimal.xlsx` | pass | 1.000 | 1.000 | 0.999 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `xlsx/manual/pivot-libreoffice.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `xlsx/charts/area.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.986 |  |
| `xlsx/charts/axes.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.994 |  |
| `xlsx/charts/bar.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.986 |  |
| `xlsx/charts/bubble.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.985 |  |
| `xlsx/charts/chartsheet.xlsx` | pass | 1.000 | 1.000 | 0.957 | 1.000 | 1.000 | yes | yes | 0.986 |  |
| `xlsx/charts/column.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.986 |  |
| `xlsx/charts/combo.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.986 |  |
| `xlsx/charts/doughnut.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.985 |  |
| `xlsx/charts/line.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.986 |  |
| `xlsx/charts/pie.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.985 |  |
| `xlsx/charts/radar.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.986 |  |
| `xlsx/charts/scatter.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.986 |  |
| `xlsx/charts/sunburst.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.986 |  |
| `xlsx/charts/treemap.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.986 |  |
| `external/xlsx/lo-PivotTable_CachedDefinitionAndDataInSync.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-autofilter-colors-fg.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-cond_format_theme_color3.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-conditional_fmt_origin.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-hiddenButton.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-hyperlinks.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-named-ranges-hidden.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-pivot_table_first_header_row.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-pivottable_date_field_filter.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-tdf110440.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-tdf115159.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-tdf136721_letter_sized_paper.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-tdf148820.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-tdf161365.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-tdf169379.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-tdf58243.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-tdf79972.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/lo-tdf83779.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.988 |  |
| `external/xlsx/lo-tdf91332.xlsx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/xlsx/lo-twoCellAnchor_editAs_oneCell.xlsx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/xlsx/poi-47813.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.989 |  |
| `external/xlsx/poi-49872.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.988 |  |
| `external/xlsx/poi-50867_with_table.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/poi-51222.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/poi-54436.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.991 |  |
| `external/xlsx/poi-55814.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/poi-55924.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/poi-56730.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/poi-57482-OnlyNumeric.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/poi-58315.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.996 |  |
| `external/xlsx/poi-59106.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.989 |  |
| `external/xlsx/poi-61652.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.991 |  |
| `external/xlsx/poi-64450.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.993 |  |
| `external/xlsx/poi-65016.xlsx` | pass | 1.000 | 1.000 | 0.900 | 1.000 | 1.000 | yes | yes | 0.989 |  |
| `external/xlsx/poi-ExcelTables.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.990 |  |
| `external/xlsx/poi-Formatting.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/poi-SampleSS.strict.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.989 |  |
| `external/xlsx/poi-SampleSS.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.989 |  |
| `external/xlsx/poi-Themes.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/poi-Themes2.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/poi-WidthsAndHeights.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/poi-WithChart.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/poi-WithConditionalFormatting.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.989 |  |
| `external/xlsx/poi-WithDrawing.xlsx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/xlsx/poi-bug62181.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/poi-comments.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `external/xlsx/poi-sample-beta.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.979 |  |
| `external/xlsx/poi-shared_formulas.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 0.989 |  |
| `external/xlsx/poi-stress002.xlsx` | pass | 1.000 | 1.000 | 0.999 | 1.000 | 1.000 | yes | yes | 0.992 |  |
| `external/xlsx/poi-styles.xlsx` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | yes | 1.000 |  |
| `pptx/features/all-layouts.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/animation-presets.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/animations.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/arabic-deck.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/aspect-16x9.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/aspect-4x3.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/backgrounds.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/chart-kinds.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.995 |  |
| `pptx/features/charts.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.996 |  |
| `pptx/features/cjk-deck.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/columns-vertical.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/connectors.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/deck-4x3.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.995 |  |
| `pptx/features/gradients-effects.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/groups.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/hyperlinks.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/large-table.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/many-slides.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/notes.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/pictures.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/placeholders.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.993 |  |
| `pptx/features/preset-shapes.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/sections.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/shape-styles.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/tables.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/text-autofit.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/text-effects.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/text-formatting.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/theme-colors.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/transition-presets.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `pptx/features/transitions.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/lo-bnc870233_2.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/lo-bnc910045.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/lo-fill-color-list.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/lo-paraMarginAndIndentation.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/lo-smartart-background-drawingml-fallback.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/lo-smartart-background.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/lo-smartart-cnt.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/lo-smartart-multidirectional.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/lo-tdf100926_ODP.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.988 |  |
| `external/pptx/lo-tdf103792.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/lo-tdf112089.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/lo-tdf113818-swivel.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/lo-tdf125360_1.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.998 |  |
| `external/pptx/lo-tdf132302_SmartArt_rightArrow.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/lo-tdf141058-1.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.990 |  |
| `external/pptx/lo-tdf142590.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.994 |  |
| `external/pptx/lo-tdf142915.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.994 |  |
| `external/pptx/lo-tdf143624.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.994 |  |
| `external/pptx/lo-tdf152335.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.999 |  |
| `external/pptx/lo-tdf90626.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/poi-51187.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.994 |  |
| `external/pptx/poi-60042.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/poi-SampleShow.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/poi-SmartArt.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.996 |  |
| `external/pptx/poi-alterman_security.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.989 |  |
| `external/pptx/poi-aptia.pptx` | pass | 1.000 | 1.000 | 0.999 | — | — | yes | yes | 0.987 |  |
| `external/pptx/poi-backgrounds.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.993 |  |
| `external/pptx/poi-bar-chart.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/poi-bug60993.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/poi-bug64693.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.984 |  |
| `external/pptx/poi-bug65523.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.964 |  |
| `external/pptx/poi-bug68703.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.982 |  |
| `external/pptx/poi-chart-picture-bg.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.997 |  |
| `external/pptx/poi-crop-to-0.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/poi-hello-world-signed.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/poi-hello-world-unsigned.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/poi-highlight-test-case.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.983 |  |
| `external/pptx/poi-layouts.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.993 |  |
| `external/pptx/poi-ms-office-2010-signed.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/poi-pptx2svg.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/poi-radar-chart.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/poi-sample.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/poi-scatter-chart.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/poi-shapes.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.995 |  |
| `external/pptx/poi-smartart-simple.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 1.000 |  |
| `external/pptx/poi-stress021.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.989 |  |
| `external/pptx/poi-table_test.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.988 |  |
| `external/pptx/poi-themes.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.991 |  |
| `external/pptx/poi-tika-2605.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.992 |  |
| `external/pptx/poi-with_japanese.pptx` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | yes | 0.999 |  |
| `odf/features/animations.odp` | pass | 1.000 | 1.000 | 0.992 | — | — | yes | — | — |  |
| `odf/features/aspect-16x9.odp` | pass | 1.000 | 1.000 | 0.994 | — | — | yes | — | — |  |
| `odf/features/aspect-4x3.odp` | pass | 1.000 | 1.000 | 0.992 | — | — | yes | — | — |  |
| `odf/features/autofilter.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/backgrounds.odp` | pass | 1.000 | 1.000 | 0.852 | — | — | yes | — | — |  |
| `odf/features/breaks.odt` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | — | — |  |
| `odf/features/charts.odp` | pass | 1.000 | 1.000 | 0.913 | — | — | yes | — | — |  |
| `odf/features/charts.ods` | pass | 1.000 | 1.000 | 0.971 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/comments.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/comments.odt` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | — | — |  |
| `odf/features/conditional-formatting.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/connectors.odp` | pass | 1.000 | 1.000 | 0.980 | — | — | yes | — | — |  |
| `odf/features/data-validation.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/date1904.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/defined-names.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/fields.odt` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | — | — |  |
| `odf/features/footnotes.odt` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | — | — |  |
| `odf/features/formulas.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/freeze-panes.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/groups.odp` | pass | 1.000 | 1.000 | 0.996 | — | — | yes | — | — |  |
| `odf/features/headers-footers.odt` | pass | 1.000 | 1.000 | 0.999 | — | — | yes | — | — |  |
| `odf/features/hidden-rows-cols.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/hyperlinks.odp` | pass | 1.000 | 1.000 | 0.991 | — | — | yes | — | — |  |
| `odf/features/hyperlinks.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/hyperlinks.odt` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | — | — |  |
| `odf/features/images-anchored.odt` | pass | 1.000 | 1.000 | 0.997 | — | — | yes | — | — |  |
| `odf/features/images-inline.odt` | pass | 1.000 | 1.000 | 0.973 | — | — | yes | — | — |  |
| `odf/features/long-text.odt` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | — | — |  |
| `odf/features/many-slides.odp` | pass | 1.000 | 1.000 | 0.975 | — | — | yes | — | — |  |
| `odf/features/merges.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/multi-sheet.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/notes.odp` | pass | 1.000 | 1.000 | 0.977 | — | — | yes | — | — |  |
| `odf/features/number-formats.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/numbering.odt` | pass | 0.979 | 1.000 | 1.000 | — | — | yes | — | — |  |
| `odf/features/page-setup.odt` | pass | 1.000 | 1.000 | 0.994 | — | — | yes | — | — |  |
| `odf/features/paragraphs.odt` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | — | — |  |
| `odf/features/pictures.odp` | pass | 1.000 | 1.000 | 0.984 | — | — | yes | — | — |  |
| `odf/features/placeholders.odp` | pass | 1.000 | 1.000 | 0.977 | — | — | yes | — | — |  |
| `odf/features/preset-shapes.odp` | pass | 1.000 | 1.000 | 0.983 | — | — | yes | — | — |  |
| `odf/features/print-setup.ods` | pass | 1.000 | 1.000 | 0.812 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/properties.odt` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | — | — |  |
| `odf/features/protection.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/rich-text.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/rtl.odt` | pass | 1.000 | 1.000 | 0.989 | — | — | yes | — | — |  |
| `odf/features/sections.odp` | pass | 1.000 | 1.000 | 0.995 | — | — | yes | — | — |  |
| `odf/features/sections.odt` | pass | 1.000 | 1.000 | 0.993 | — | — | yes | — | — |  |
| `odf/features/shape-styles.odp` | pass | 1.000 | 1.000 | 0.918 | — | — | yes | — | — |  |
| `odf/features/shared-array-formulas.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/styles.ods` | pass | 1.000 | 1.000 | 0.998 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/styles.odt` | pass | 1.000 | 1.000 | 0.995 | — | — | yes | — | — |  |
| `odf/features/tables.odp` | pass | 1.000 | 1.000 | 0.988 | — | — | yes | — | — |  |
| `odf/features/tables.ods` | pass | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | yes | — | — |  |
| `odf/features/tables.odt` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | — | — |  |
| `odf/features/text-boxes.odt` | pass | 1.000 | 1.000 | 0.982 | — | — | yes | — | — |  |
| `odf/features/text-formatting.odp` | pass | 1.000 | 1.000 | 0.959 | — | — | yes | — | — |  |
| `odf/features/theme-colors.odp` | pass | 1.000 | 1.000 | 0.995 | — | — | yes | — | — |  |
| `odf/features/tracked-changes.odt` | pass | 1.000 | 1.000 | 1.000 | — | — | yes | — | — |  |
| `odf/features/transitions.odp` | pass | 1.000 | 1.000 | 0.993 | — | — | yes | — | — |  |
| `odf/manual/chart.odt` | pass | 1.000 | 1.000 | 0.946 | — | — | yes | — | — |  |
| `odf/manual/foreign-prefixes.odt` | pass | 1.000 | 1.000 | 0.996 | — | — | yes | — | — |  |
| `odf/manual/minimal.odt` | pass | 1.000 | 1.000 | — | — | — | yes | — | — | visual: the original does not convert: LibreOffice could not convert minimal.odt to pdf: Error: source file could not be loaded |
| `odf/manual/shapes.odp` | pass | 1.000 | 1.000 | 0.962 | — | — | yes | — | — |  |
| `odf/manual/structures.ods` | pass | 1.000 | 1.000 | 0.984 | 1.000 | 1.000 | yes | — | — |  |

<!-- differential:start -->
## Side by side

- Stamp: `fc7ef2c489a7d15af3a6873d498285ed`

Written by `cargo xtask fidelity --differential`. Each tool takes the same input files and every output goes through the same comparison: Kiln's reader gives `structural`, `text` and `formulas`, the XML of each file gives `values` (the values it stores, not a recalculation) and `raw_elements`, and LibreOffice renders original and output at 60 dpi for `visual`, both from one path that is the same in every run and checkout (a document can show its own path in a field). Nothing renders in Office. `structural` counts facts in Kiln's own model, so a tool that restructures a file without changing how it looks (new style ids, runs split differently) loses points there; `text` depends on what that model reads; `values`, `visual` and `raw_elements` do not depend on it. LibreOffice's round trips run with its default settings.

The rule, fixed before the runs: Kiln counts as at least as good as a tool on a file only if it is not below the tool on any of those metrics. A file a tool fails on is a loss for that tool, Kiln included. Every file of the corpus counts. In the means, a failed file scores 0.

LibreOffice runs two ways: through OpenDocument (`docx → odt → docx`), the round trip the release bar names, and directly (`docx → docx`), what a user gets from open and save.

ONLYOFFICE and the Craft apps were not run.

### Corpus

| Kind | Files | Real-world | Generated |
|---|---|---|---|
| docx files | 90 | 50 | 40 |
| xlsx files | 102 | 50 | 52 |
| pptx files | 81 | 50 | 31 |

### Kiln against each tool

| Kind | Tool | Kiln at least as good | Share | structural | text | visual | raw_elements | Failed files |
|---|---|---|---|---|---|---|---|---|
| docx files | Kiln | — | — | 1.000 | 1.000 | 0.999 | 0.999 | 0 |
| docx files | LibreOffice via ODF | 84 of 90 | 93.3 % | 0.417 | 0.952 | 0.978 | 0.462 | 1 |
| docx files | LibreOffice direct | 83 of 90 | 92.2 % | 0.539 | 0.924 | 0.981 | 0.592 | 1 |
| xlsx files | Kiln | — | — | 1.000 | 1.000 | 0.999 | 0.989 | 0 |
| xlsx files | LibreOffice via ODF | 98 of 102 | 96.1 % | 0.250 | 0.957 | 0.976 | 0.768 | 0 |
| xlsx files | LibreOffice direct | 97 of 102 | 95.1 % | 0.300 | 0.957 | 0.999 | 0.831 | 0 |
| pptx files | Kiln | — | — | 1.000 | 1.000 | 1.000 | 0.997 | 0 |
| pptx files | LibreOffice via ODF | 81 of 81 | 100.0 % | 0.098 | 0.950 | 0.993 | 0.577 | 0 |
| pptx files | LibreOffice direct | 81 of 81 | 100.0 % | 0.197 | 0.999 | 0.994 | 0.645 | 0 |

### Page counts

Kiln's own PDF against LibreOffice's PDF of the same original, every sheet of a workbook included, with Kiln's bundled metric-compatible fonts available to both.

| Kind | Compared | Equal | Within one page | Largest gap |
|---|---|---|---|---|
| docx files | 89 | 83 | 86 | 102 |
| xlsx files | 101 | 91 | 95 | 584 |
| pptx files | 81 | 81 | 81 | 0 |

### Files a tool clearly beats Kiln on

A lead above 0.02 on any metric: 6 entries.

| File | Tool | Lead |
|---|---|---|
| `docx/manual/foreign-prefixes.docx` | LibreOffice via ODF | visual +0.084 |
| `docx/manual/foreign-prefixes.docx` | LibreOffice direct | visual +0.084 |
| `xlsx/charts/chartsheet.xlsx` | LibreOffice via ODF | visual +0.040 |
| `xlsx/charts/chartsheet.xlsx` | LibreOffice direct | visual +0.043 |
| `external/xlsx/poi-65016.xlsx` | LibreOffice via ODF | visual +0.046 |
| `external/xlsx/poi-65016.xlsx` | LibreOffice direct | visual +0.099 |

### Every file

Scores in the order structural, text, visual, then values and formulas for workbooks, then raw_elements.

| File | Kiln | LibreOffice via ODF | LibreOffice direct | Pages Kiln | Pages LibreOffice |
|---|---|---|---|---|---|
| `docx/features/arabic-document.docx` | 1.000 1.000 1.000 1.000 | 0.406 1.000 1.000 0.436 | 0.585 1.000 1.000 0.560 | 1 | 1 |
| `docx/features/breaks.docx` | 1.000 1.000 1.000 1.000 | 0.415 1.000 1.000 0.407 | 0.699 1.000 1.000 0.537 | 2 | 2 |
| `docx/features/cjk-document.docx` | 1.000 1.000 1.000 1.000 | 0.407 1.000 1.000 0.437 | 0.683 1.000 1.000 0.554 | 1 | 2 |
| `docx/features/columns-footnotes.docx` | 1.000 1.000 1.000 1.000 | 0.452 1.000 0.994 0.427 | 0.672 1.000 0.994 0.553 | 3 | 3 |
| `docx/features/comments.docx` | 1.000 1.000 1.000 1.000 | 0.436 1.000 1.000 0.418 | 0.720 1.000 1.000 0.545 | 1 | 1 |
| `docx/features/content-controls.docx` | 1.000 1.000 1.000 1.000 | 0.402 1.000 1.000 0.435 | 0.702 1.000 1.000 0.559 | 1 | 1 |
| `docx/features/drop-caps-tabs.docx` | 1.000 1.000 1.000 1.000 | 0.385 0.974 0.979 0.395 | 0.700 0.974 0.979 0.526 | 1 | 1 |
| `docx/features/embedded-chart.docx` | 1.000 1.000 1.000 1.000 | 0.385 1.000 1.000 0.523 | 0.702 1.000 1.000 0.629 | 1 | 1 |
| `docx/features/equations.docx` | 1.000 1.000 1.000 1.000 | 0.398 1.000 1.000 0.445 | 0.708 1.000 1.000 0.566 | 1 | 1 |
| `docx/features/fields.docx` | 1.000 1.000 1.000 1.000 | 0.420 0.990 1.000 0.411 | 0.694 0.990 1.000 0.539 | 1 | 1 |
| `docx/features/floating-tables.docx` | 1.000 1.000 1.000 1.000 | 0.444 1.000 1.000 0.430 | 0.667 1.000 1.000 0.555 | 1 | 1 |
| `docx/features/footer-control.docx` | 1.000 1.000 1.000 1.000 | 0.414 0.999 1.000 0.390 | 0.650 0.999 1.000 0.549 | 4 | 4 |
| `docx/features/footnotes.docx` | 1.000 1.000 1.000 1.000 | 0.449 1.000 0.981 0.435 | 0.697 1.000 0.981 0.559 | 1 | 1 |
| `docx/features/form-fields.docx` | 1.000 1.000 1.000 1.000 | 0.405 1.000 1.000 0.426 | 0.700 1.000 1.000 0.554 | 1 | 1 |
| `docx/features/headers-footers.docx` | 1.000 1.000 1.000 1.000 | 0.434 0.999 1.000 0.423 | 0.652 0.994 1.000 0.550 | 5 | 5 |
| `docx/features/hyperlinks.docx` | 1.000 1.000 1.000 1.000 | 0.424 1.000 1.000 0.410 | 0.669 1.000 1.000 0.540 | 2 | 2 |
| `docx/features/images-anchored.docx` | 1.000 1.000 1.000 1.000 | 0.454 1.000 0.997 0.497 | 0.722 1.000 0.997 0.608 | 2 | 2 |
| `docx/features/images-inline.docx` | 1.000 1.000 1.000 1.000 | 0.386 1.000 0.974 0.464 | 0.690 1.000 0.974 0.582 | 1 | 1 |
| `docx/features/large-table.docx` | 1.000 1.000 1.000 1.000 | 0.255 1.000 1.000 0.450 | 0.267 1.000 1.000 0.567 | 14 | 14 |
| `docx/features/legal-numbering.docx` | 1.000 1.000 1.000 1.000 | 0.373 1.000 1.000 0.412 | 0.623 1.000 1.000 0.544 | 1 | 1 |
| `docx/features/long-text.docx` | 1.000 1.000 1.000 1.000 | 0.504 1.000 1.000 0.399 | 0.677 1.000 1.000 0.531 | 10 | 10 |
| `docx/features/mixed-sections.docx` | 1.000 1.000 1.000 0.997 | 0.426 1.000 1.000 0.403 | 0.717 1.000 1.000 0.534 | 5 | 5 |
| `docx/features/nested-fields.docx` | 1.000 1.000 1.000 1.000 | 0.406 0.947 1.000 0.410 | 0.698 0.947 1.000 0.538 | 1 | 1 |
| `docx/features/nested-tables.docx` | 1.000 1.000 1.000 1.000 | 0.420 1.000 1.000 0.430 | 0.684 1.000 0.999 0.555 | 1 | 1 |
| `docx/features/numbering.docx` | 1.000 1.000 1.000 1.000 | 0.469 1.000 1.000 0.409 | 0.620 1.000 1.000 0.545 | 2 | 2 |
| `docx/features/page-borders.docx` | 1.000 1.000 1.000 1.000 | 0.396 1.000 1.000 0.403 | 0.699 1.000 1.000 0.534 | 1 | 1 |
| `docx/features/page-setup.docx` | 1.000 1.000 1.000 1.000 | 0.404 1.000 1.000 0.410 | 0.698 1.000 1.000 0.539 | 1 | 1 |
| `docx/features/paragraphs.docx` | 1.000 1.000 1.000 1.000 | 0.436 1.000 0.998 0.405 | 0.689 1.000 1.000 0.535 | 3 | 3 |
| `docx/features/properties.docx` | 1.000 1.000 1.000 1.000 | 0.397 1.000 1.000 0.396 | 0.710 1.000 1.000 0.527 | 1 | 1 |
| `docx/features/rtl.docx` | 1.000 1.000 1.000 1.000 | 0.404 1.000 1.000 0.409 | 0.693 1.000 1.000 0.538 | 1 | 1 |
| `docx/features/section-headers.docx` | 1.000 1.000 1.000 1.000 | 0.457 0.999 1.000 0.423 | 0.668 0.999 1.000 0.550 | 6 | 6 |
| `docx/features/sections.docx` | 1.000 1.000 1.000 0.998 | 0.460 0.989 0.993 0.426 | 0.648 0.989 0.993 0.552 | 5 | 4 |
| `docx/features/styles.docx` | 1.000 1.000 1.000 1.000 | 0.597 1.000 1.000 0.415 | 0.771 1.000 1.000 0.548 | 2 | 2 |
| `docx/features/tables.docx` | 1.000 1.000 1.000 1.000 | 0.363 1.000 1.000 0.434 | 0.523 1.000 1.000 0.567 | 1 | 1 |
| `docx/features/text-boxes.docx` | 1.000 1.000 1.000 1.000 | 0.409 1.000 0.999 0.492 | 0.707 1.000 0.999 0.608 | 1 | 1 |
| `docx/features/tracked-changes.docx` | 1.000 1.000 1.000 1.000 | 0.412 1.000 0.998 0.416 | 0.714 1.000 1.000 0.548 | 1 | 1 |
| `docx/features/tracked-formatting.docx` | 1.000 1.000 1.000 1.000 | 0.420 0.750 1.000 0.432 | 0.710 0.750 1.000 0.560 | 1 | 1 |
| `docx/features/vml-fallbacks.docx` | 1.000 1.000 1.000 1.000 | 0.392 1.000 0.958 0.478 | 0.701 1.000 0.958 0.595 | 1 | 1 |
| `docx/manual/foreign-prefixes.docx` | 1.000 1.000 0.916 1.000 | 0.182 0.333 1.000 0.367 | 0.182 0.333 1.000 0.633 | 1 | 1 |
| `docx/manual/minimal.docx` | 1.000 1.000 1.000 1.000 | 0.273 1.000 1.000 1.000 | 0.273 1.000 1.000 1.000 | 1 | 1 |
| `external/docx/lo-2129393649.docx` | 1.000 1.000 0.987 0.999 | 0.571 0.965 0.989 0.521 | 0.596 0.969 0.996 0.619 | 3 | 3 |
| `external/docx/lo-TOC_field_b.docx` | 1.000 1.000 1.000 0.997 | 0.652 1.000 1.000 0.457 | 0.667 1.000 1.000 0.590 | 1 | 1 |
| `external/docx/lo-chart-size.docx` | 1.000 1.000 1.000 1.000 | 0.438 1.000 1.000 0.554 | 0.471 1.000 1.000 0.620 | 1 | 1 |
| `external/docx/lo-clearing-break.docx` | 1.000 1.000 1.000 1.000 | 0.375 1.000 1.000 0.548 | 0.500 1.000 1.000 0.656 | 1 | 1 |
| `external/docx/lo-dashed_line_custdash_1000th_of_percent.docx` | 1.000 1.000 1.000 1.000 | 0.524 1.000 1.000 0.520 | 0.654 1.000 1.000 0.651 | 1 | 1 |
| `external/docx/lo-fdo76586.docx` | 1.000 1.000 0.989 0.993 | 0.603 1.000 1.000 0.490 | 0.621 1.000 1.000 0.593 | 1 | 1 |
| `external/docx/lo-fdo80997.docx` | 1.000 1.000 1.000 1.000 | 0.436 1.000 0.999 0.442 | 0.495 0.538 1.000 0.580 | 1 | 1 |
| `external/docx/lo-floating-table-position.docx` | 1.000 1.000 1.000 1.000 | 0.324 1.000 1.000 0.500 | 0.400 1.000 1.000 0.614 | 1 | 1 |
| `external/docx/lo-grabbag.docx` | 1.000 1.000 1.000 1.000 | 0.467 1.000 1.000 0.460 | 0.500 1.000 1.000 0.635 | 1 | 1 |
| `external/docx/lo-n652364.docx` | 1.000 1.000 1.000 1.000 | 0.478 1.000 0.500 0.466 | 0.500 1.000 0.500 0.626 | 1 | 2 |
| `external/docx/lo-n816593.docx` | 1.000 1.000 1.000 0.978 | 0.298 1.000 1.000 0.430 | 0.310 1.000 1.000 0.525 | 1 | 1 |
| `external/docx/lo-numbering1.docx` | 1.000 1.000 1.000 1.000 | 0.099 1.000 1.000 0.446 | 0.109 1.000 1.000 0.541 | 1 | 1 |
| `external/docx/lo-para-auto-spacing.docx` | 1.000 1.000 1.000 1.000 | 0.429 1.000 1.000 0.367 | 0.476 1.000 1.000 0.461 | 1 | 1 |
| `external/docx/lo-table-rtl.docx` | 1.000 1.000 1.000 1.000 | 0.556 1.000 1.000 0.490 | 0.607 1.000 1.000 0.610 | 1 | 1 |
| `external/docx/lo-tdf126590.docx` | 1.000 1.000 1.000 1.000 | 0.125 1.000 1.000 0.492 | 0.176 1.000 1.000 0.641 | 1 | 1 |
| `external/docx/lo-tdf143476_lockedCanvas_position.docx` | 1.000 1.000 1.000 1.000 | 0.146 1.000 0.986 0.438 | 0.167 1.000 0.893 0.527 | 1 | 1 |
| `external/docx/lo-tdf159158_zOrder_1and0equalB.docx` | 1.000 1.000 1.000 0.994 | 0.912 1.000 1.000 0.560 | 0.914 1.000 1.000 0.670 | 1 | 1 |
| `external/docx/lo-tdf170003_bottomSpacing.docx` | 1.000 1.000 1.000 1.000 | 0.378 0.882 1.000 0.490 | 0.457 0.882 1.000 0.614 | 2 | 2 |
| `external/docx/lo-tdf48658_transparentOLEheader.docx` | 1.000 1.000 1.000 1.000 | 0.380 1.000 0.992 0.560 | 0.389 0.500 0.995 0.660 | 1 | 1 |
| `external/docx/lo-test_msword_hang.docx` | 1.000 1.000 1.000 0.986 | 0.833 1.000 1.000 0.769 | 0.842 1.000 1.000 0.889 | 1 | 1 |
| `external/docx/poi-55966.docx` | 1.000 1.000 1.000 1.000 | 0.600 1.000 1.000 0.405 | 0.615 1.000 1.000 0.718 | 1 | 1 |
| `external/docx/poi-58067.docx` | 1.000 1.000 1.000 1.000 | 0.568 1.000 0.993 0.442 | 0.595 1.000 0.993 0.540 | 1 | 1 |
| `external/docx/poi-Bug54771b.docx` | 1.000 1.000 1.000 1.000 | 0.586 1.000 1.000 0.395 | 0.621 1.000 1.000 0.698 | 1 | 1 |
| `external/docx/poi-Bug55142.docx` | 1.000 1.000 1.000 1.000 | 0.366 1.000 0.995 0.435 | 0.505 1.000 1.000 0.742 | 1 | 1 |
| `external/docx/poi-Bug60337.docx` | 1.000 1.000 1.000 1.000 | 0.274 1.000 1.000 0.531 | 0.311 1.000 1.000 0.636 | 1 | 1 |
| `external/docx/poi-ComplexNumberedLists.docx` | 1.000 1.000 1.000 1.000 | 0.682 1.000 1.000 0.482 | 0.691 1.000 1.000 0.582 | 1 | 1 |
| `external/docx/poi-EnforcedWith.docx` | 1.000 1.000 1.000 1.000 | 0.500 1.000 1.000 0.488 | 0.533 1.000 1.000 0.637 | 1 | 1 |
| `external/docx/poi-FieldCodes.docx` | 1.000 1.000 1.000 1.000 | 0.543 1.000 1.000 0.487 | 0.553 1.000 1.000 0.607 | 1 | 1 |
| `external/docx/poi-FldSimple.docx` | 1.000 1.000 1.000 1.000 | 0.385 0.000 1.000 0.430 | 0.429 0.000 1.000 0.540 | 1 | 1 |
| `external/docx/poi-Headers.docx` | 1.000 1.000 1.000 1.000 | 0.200 1.000 1.000 0.462 | 0.152 0.500 1.000 0.616 | 3 | 3 |
| `external/docx/poi-Numbering.docx` | 1.000 1.000 1.000 1.000 | 0.525 1.000 1.000 0.516 | 0.529 1.000 1.000 0.651 | 1 | 1 |
| `external/docx/poi-OPCCompliance_CoreProperties_SUCCESS.docx` | 1.000 1.000 1.000 1.000 | 0.417 1.000 1.000 0.460 | 0.462 1.000 1.000 0.623 | 1 | 1 |
| `external/docx/poi-OPCCompliance_CoreProperties_UnauthorizedXMLLangAttributeFAIL.docx` | 1.000 1.000 1.000 1.000 | 0.417 1.000 1.000 0.460 | 0.462 1.000 1.000 0.623 | 1 | 1 |
| `external/docx/poi-OPCCompliance_DerivedPartNameFAIL.docx` | 1.000 1.000 1.000 | failed | failed | 1 | — |
| `external/docx/poi-Office2010-SP1-XAdES-X-L.docx` | 1.000 1.000 1.000 1.000 | 0.286 1.000 1.000 0.273 | 0.333 1.000 1.000 0.340 | 1 | 1 |
| `external/docx/poi-SampleDoc.docx` | 1.000 1.000 1.000 1.000 | 0.458 1.000 1.000 0.448 | 0.480 1.000 1.000 0.641 | 2 | 2 |
| `external/docx/poi-Styles.docx` | 1.000 1.000 1.000 1.000 | 0.200 1.000 1.000 0.518 | 0.233 1.000 1.000 0.666 | 1 | 1 |
| `external/docx/poi-ThreeColHeadFoot.docx` | 1.000 1.000 1.000 1.000 | 0.564 1.000 1.000 0.488 | 0.478 0.822 1.000 0.631 | 2 | 2 |
| `external/docx/poi-bug56075-changeTracking_on.docx` | 1.000 1.000 1.000 1.000 | 0.308 1.000 1.000 0.470 | 0.357 1.000 1.000 0.582 | 1 | 1 |
| `external/docx/poi-bug65738.docx` | 1.000 1.000 1.000 0.994 | 0.787 1.000 0.999 0.564 | 0.738 0.891 1.000 0.658 | 2 | 2 |
| `external/docx/poi-deep-table-cell.docx` | 1.000 1.000 1.000 1.000 | 0.000 0.006 1.000 0.917 | 0.000 0.006 1.000 0.917 | 3 | 105 |
| `external/docx/poi-documentProtection_no_protection.docx` | 1.000 1.000 1.000 1.000 | 0.462 1.000 1.000 0.460 | 0.500 1.000 1.000 0.627 | 1 | 1 |
| `external/docx/poi-endnotes.docx` | 1.000 1.000 1.000 1.000 | 0.386 1.000 1.000 0.444 | 0.400 1.000 1.000 0.597 | 1 | 1 |
| `external/docx/poi-footnotes.docx` | 1.000 1.000 1.000 1.000 | 0.333 1.000 1.000 0.474 | 0.360 1.000 1.000 0.653 | 1 | 1 |
| `external/docx/poi-headerPic.docx` | 1.000 1.000 1.000 1.000 | 0.083 1.000 1.000 0.533 | 0.071 0.500 1.000 0.643 | 1 | 1 |
| `external/docx/poi-issue_51265_2.docx` | 1.000 1.000 1.000 1.000 | 0.091 1.000 1.000 0.533 | 0.077 0.667 1.000 0.651 | 1 | 1 |
| `external/docx/poi-sample.docx` | 1.000 1.000 1.000 1.000 | 0.577 1.000 1.000 0.422 | 0.677 1.000 1.000 0.584 | 1 | 1 |
| `external/docx/poi-stress005.docx` | 1.000 1.000 0.990 1.000 | 0.566 0.998 0.769 0.507 | 0.583 0.995 1.000 0.679 | 4 | 7 |
| `external/docx/poi-stress018.docx` | 1.000 1.000 1.000 0.996 | 0.539 1.000 0.890 0.515 | 0.551 1.000 1.000 0.633 | 42 | 37 |
| `external/docx/poi-testComment.docx` | 1.000 1.000 1.000 0.994 | 0.393 0.875 1.000 0.415 | 0.414 0.875 1.000 0.535 | 1 | 1 |
| `xlsx/features/autofilter.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.990 | 0.143 1.000 0.965 1.000 1.000 0.740 | 0.163 1.000 1.000 1.000 1.000 0.824 | 1 | 1 |
| `xlsx/features/cf-all-types.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.980 | 0.091 1.000 0.998 1.000 1.000 0.787 | 0.102 1.000 1.000 1.000 1.000 0.848 | 2 | 2 |
| `xlsx/features/charts.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.987 | 0.032 1.000 0.973 1.000 1.000 0.851 | 0.035 1.000 0.998 1.000 1.000 0.890 | 6 | 6 |
| `xlsx/features/cjk-sheet.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.978 | 0.310 1.000 0.995 1.000 1.000 0.737 | 0.345 1.000 1.000 1.000 1.000 0.803 | 1 | 1 |
| `xlsx/features/comments.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.979 | 0.462 1.000 0.997 1.000 1.000 0.751 | 0.500 1.000 1.000 1.000 1.000 0.813 | 1 | 1 |
| `xlsx/features/conditional-formatting.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.990 | 0.082 1.000 1.000 1.000 1.000 0.790 | 0.143 1.000 0.998 1.000 1.000 0.850 | 2 | 2 |
| `xlsx/features/data-validation.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.978 | 0.357 1.000 0.992 1.000 1.000 0.743 | 0.393 1.000 1.000 1.000 1.000 0.807 | 1 | 1 |
| `xlsx/features/date1904-formats.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.989 | 0.312 0.000 0.988 1.000 1.000 0.737 | 0.344 0.000 1.000 1.000 1.000 0.802 | 1 | 1 |
| `xlsx/features/date1904.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.989 | 0.190 1.000 0.992 1.000 1.000 0.737 | 0.214 1.000 1.000 1.000 1.000 0.803 | 1 | 1 |
| `xlsx/features/defined-names.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.989 | 0.444 1.000 0.997 1.000 1.000 0.748 | 0.481 1.000 1.000 1.000 1.000 0.814 | 1 | 1 |
| `xlsx/features/formula-matrix.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.978 | 0.293 1.000 0.993 1.000 0.986 0.734 | 0.317 1.000 1.000 1.000 0.986 0.801 | 3 | 3 |
| `xlsx/features/formulas.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.989 | 0.034 1.000 0.945 1.000 0.947 0.743 | 0.036 1.000 1.000 1.000 0.947 0.807 | 9 | 16 |
| `xlsx/features/freeze-panes.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.978 | 0.071 1.000 0.945 1.000 1.000 0.720 | 0.087 1.000 1.000 1.000 1.000 0.801 | 19 | 19 |
| `xlsx/features/hidden-rows-cols.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.978 | 0.339 1.000 0.950 1.000 1.000 0.737 | 0.355 1.000 1.000 1.000 1.000 0.803 | 1 | 1 |
| `xlsx/features/hyperlinks.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.978 | 0.364 1.000 0.999 1.000 1.000 0.740 | 0.448 1.000 1.000 1.000 1.000 0.805 | 2 | 2 |
| `xlsx/features/images.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.981 | 0.286 1.000 0.981 1.000 1.000 0.758 | 0.600 1.000 1.000 1.000 1.000 0.814 | 1 | 1 |
| `xlsx/features/large-table.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.980 | 0.003 1.000 0.851 1.000 1.000 0.735 | 0.004 1.000 1.000 1.000 1.000 0.807 | 47 | 47 |
| `xlsx/features/locale-formats.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.989 | 0.333 1.000 0.981 1.000 1.000 0.734 | 0.367 1.000 1.000 1.000 1.000 0.800 | 1 | 1 |
| `xlsx/features/merges.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.978 | 0.176 1.000 0.972 1.000 1.000 0.740 | 0.189 1.000 1.000 1.000 1.000 0.805 | 1 | 1 |
| `xlsx/features/multi-sheet.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.978 | 0.409 1.000 1.000 1.000 0.750 0.769 | 0.439 1.000 1.000 1.000 0.750 0.835 | 6 | 6 |
| `xlsx/features/number-formats.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.989 | 0.121 1.000 0.947 1.000 1.000 0.740 | 0.132 1.000 1.000 1.000 1.000 0.805 | 2 | 2 |
| `xlsx/features/outlines.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.978 | 0.263 1.000 0.982 1.000 1.000 0.737 | 0.289 1.000 1.000 1.000 1.000 0.814 | 1 | 1 |
| `xlsx/features/pivot-multi-fields.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.983 | 0.159 0.893 0.960 0.902 1.000 0.794 | 0.168 0.893 1.000 0.902 1.000 0.845 | 3 | 3 |
| `xlsx/features/pivot-tables.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.984 | 0.177 0.922 0.970 0.971 1.000 0.771 | 0.185 0.922 0.998 0.971 1.000 0.820 | 3 | 3 |
| `xlsx/features/print-setup.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.990 | 0.060 1.000 0.831 1.000 1.000 0.733 | 0.067 1.000 1.000 1.000 1.000 0.793 | 4 | 4 |
| `xlsx/features/protection.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.978 | 0.188 1.000 0.993 1.000 1.000 0.737 | 0.250 1.000 1.000 1.000 1.000 0.803 | 1 | 1 |
| `xlsx/features/rich-text.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.979 | 0.409 1.000 0.997 1.000 1.000 0.777 | 0.455 1.000 1.000 1.000 1.000 0.841 | 1 | 1 |
| `xlsx/features/rtl-sheet.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.978 | 0.310 1.000 0.994 1.000 1.000 0.740 | 0.357 1.000 1.000 1.000 1.000 0.805 | 1 | 1 |
| `xlsx/features/shared-array-formulas.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.980 | 0.364 1.000 0.994 1.000 1.000 0.674 | 0.409 1.000 1.000 1.000 1.000 0.735 | 1 | 1 |
| `xlsx/features/slicers.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.981 | 0.267 1.000 0.993 1.000 1.000 0.690 | 0.300 1.000 1.000 1.000 1.000 0.748 | 1 | 1 |
| `xlsx/features/sparklines.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.981 | 0.455 1.000 0.990 1.000 1.000 0.768 | 0.500 1.000 1.000 1.000 1.000 0.826 | 1 | 1 |
| `xlsx/features/structured-refs.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.979 | 0.348 1.000 0.996 1.000 0.000 0.743 | 0.391 1.000 1.000 1.000 1.000 0.805 | 1 | 1 |
| `xlsx/features/styles.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.980 | 0.505 1.000 0.985 1.000 1.000 0.769 | 0.520 1.000 1.000 1.000 1.000 0.829 | 3 | 3 |
| `xlsx/features/tables.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.979 | 0.197 1.000 0.980 1.000 0.000 0.743 | 0.213 1.000 1.000 1.000 1.000 0.805 | 2 | 2 |
| `xlsx/features/validation-types.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.978 | 0.394 1.000 0.999 1.000 1.000 0.743 | 0.519 1.000 1.000 1.000 1.000 0.807 | 1 | 1 |
| `xlsx/manual/beta-strings.xlsx` | 1.000 1.000 0.999 1.000 1.000 0.857 | 0.667 0.000 1.000 0.000 1.000 0.714 | 0.667 0.000 1.000 0.000 1.000 0.714 | 1 | 1 |
| `xlsx/manual/minimal.xlsx` | 1.000 1.000 0.999 1.000 1.000 1.000 | 0.615 1.000 1.000 1.000 1.000 1.000 | 0.615 1.000 1.000 1.000 1.000 1.000 | 1 | 1 |
| `xlsx/manual/pivot-libreoffice.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.460 1.000 1.000 1.000 1.000 1.000 | 0.410 1.000 1.000 1.000 1.000 1.000 | 2 | 2 |
| `xlsx/charts/area.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.986 | 0.065 1.000 0.957 1.000 1.000 0.831 | 0.076 1.000 1.000 1.000 1.000 0.875 | 4 | 4 |
| `xlsx/charts/axes.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.994 | 0.051 0.795 0.960 1.000 1.000 0.799 | 0.058 0.795 0.988 1.000 1.000 0.837 | 2 | 2 |
| `xlsx/charts/bar.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.986 | 0.030 1.000 0.962 1.000 1.000 0.838 | 0.037 1.000 1.000 1.000 1.000 0.880 | 4 | 4 |
| `xlsx/charts/bubble.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.985 | 0.087 1.000 0.984 1.000 1.000 0.838 | 0.096 1.000 1.000 1.000 1.000 0.880 | 2 | 2 |
| `xlsx/charts/chartsheet.xlsx` | 1.000 1.000 0.957 1.000 1.000 0.986 | 0.099 1.000 0.997 1.000 1.000 0.819 | 0.108 1.000 1.000 1.000 1.000 0.862 | 3 | 2 |
| `xlsx/charts/column.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.986 | 0.030 1.000 0.972 1.000 1.000 0.838 | 0.037 1.000 1.000 1.000 1.000 0.880 | 4 | 4 |
| `xlsx/charts/combo.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.986 | 0.054 1.000 0.989 1.000 1.000 0.834 | 0.060 1.000 1.000 1.000 1.000 0.876 | 2 | 2 |
| `xlsx/charts/doughnut.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.985 | 0.134 1.000 0.992 1.000 1.000 0.826 | 0.152 1.000 1.000 1.000 1.000 0.872 | 2 | 2 |
| `xlsx/charts/line.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.986 | 0.078 1.000 0.977 1.000 1.000 0.840 | 0.089 1.000 1.000 1.000 1.000 0.881 | 4 | 4 |
| `xlsx/charts/pie.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.985 | 0.179 1.000 0.991 1.000 1.000 0.826 | 0.197 1.000 1.000 1.000 1.000 0.872 | 2 | 2 |
| `xlsx/charts/radar.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.986 | 0.071 1.000 0.983 1.000 1.000 0.830 | 0.083 1.000 0.994 1.000 1.000 0.872 | 4 | 4 |
| `xlsx/charts/scatter.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.986 | 0.040 1.000 0.969 1.000 1.000 0.837 | 0.045 1.000 0.998 1.000 1.000 0.885 | 2 | 2 |
| `xlsx/charts/sunburst.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.986 | 0.157 1.000 0.490 1.000 1.000 0.489 | 0.294 1.000 0.992 1.000 1.000 0.861 | 2 | 2 |
| `xlsx/charts/treemap.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.986 | 0.157 1.000 0.490 1.000 1.000 0.482 | 0.275 1.000 0.992 1.000 1.000 0.849 | 2 | 2 |
| `external/xlsx/lo-PivotTable_CachedDefinitionAndDataInSync.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.155 0.955 0.997 0.968 1.000 0.807 | 0.183 0.955 1.000 0.968 1.000 0.843 | 3 | 3 |
| `external/xlsx/lo-autofilter-colors-fg.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.069 1.000 0.998 1.000 1.000 0.760 | 0.111 1.000 1.000 1.000 1.000 0.851 | 1 | 1 |
| `external/xlsx/lo-cond_format_theme_color3.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.240 1.000 0.998 1.000 1.000 0.802 | 0.348 1.000 1.000 1.000 1.000 0.845 | 1 | 1 |
| `external/xlsx/lo-conditional_fmt_origin.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.300 1.000 0.999 1.000 1.000 0.793 | 0.500 1.000 1.000 1.000 1.000 0.838 | 1 | 1 |
| `external/xlsx/lo-hiddenButton.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.231 1.000 0.999 1.000 1.000 0.794 | 0.308 1.000 1.000 1.000 1.000 0.848 | 1 | 1 |
| `external/xlsx/lo-hyperlinks.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.400 1.000 0.998 0.812 1.000 0.762 | 0.517 1.000 1.000 0.812 1.000 0.830 | 1 | 6 |
| `external/xlsx/lo-named-ranges-hidden.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.412 1.000 0.994 1.000 1.000 0.756 | 0.529 1.000 1.000 1.000 1.000 0.802 | 1 | 1 |
| `external/xlsx/lo-pivot_table_first_header_row.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.013 0.842 1.000 0.690 1.000 0.906 | 0.033 0.845 1.000 0.690 1.000 0.922 | 6 | 5 |
| `external/xlsx/lo-pivottable_date_field_filter.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.321 0.974 0.999 0.667 1.000 0.712 | 0.357 0.974 0.999 0.667 1.000 0.747 | 2 | 3 |
| `external/xlsx/lo-tdf110440.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.452 1.000 0.976 1.000 1.000 0.776 | 0.552 1.000 0.976 1.000 1.000 0.827 | 1 | 1 |
| `external/xlsx/lo-tdf115159.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.483 1.000 1.000 1.000 1.000 0.629 | 0.630 1.000 1.000 1.000 1.000 0.858 | 1 | 1 |
| `external/xlsx/lo-tdf136721_letter_sized_paper.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.333 1.000 0.965 1.000 1.000 0.762 | 0.643 1.000 1.000 1.000 1.000 0.806 | 1 | 1 |
| `external/xlsx/lo-tdf148820.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.039 1.000 1.000 1.000 1.000 0.745 | 0.058 1.000 1.000 1.000 1.000 0.798 | 1 | 1 |
| `external/xlsx/lo-tdf161365.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.389 1.000 0.995 1.000 1.000 0.768 | 0.529 1.000 1.000 1.000 1.000 0.810 | 1 | 1 |
| `external/xlsx/lo-tdf169379.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.257 1.000 1.000 1.000 1.000 0.867 | 0.257 1.000 1.000 1.000 1.000 0.880 | 1 | 1 |
| `external/xlsx/lo-tdf58243.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.001 1.000 1.000 1.000 1.000 0.783 | 0.001 1.000 1.000 1.000 1.000 0.869 | 2 | 2 |
| `external/xlsx/lo-tdf79972.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.533 1.000 1.000 1.000 1.000 0.739 | 0.600 1.000 1.000 1.000 1.000 0.797 | 1 | 1 |
| `external/xlsx/lo-tdf83779.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.988 | 0.438 1.000 0.999 1.000 0.000 0.727 | 0.500 1.000 1.000 1.000 0.000 0.785 | 1 | 1 |
| `external/xlsx/lo-tdf91332.xlsx` | 1.000 1.000 1.000 1.000 | 0.176 1.000 1.000 0.752 | 0.124 1.000 1.000 0.782 | 1 | 1 |
| `external/xlsx/lo-twoCellAnchor_editAs_oneCell.xlsx` | 1.000 1.000 1.000 1.000 | 0.500 1.000 1.000 0.788 | 0.692 1.000 1.000 0.836 | 1 | 1 |
| `external/xlsx/poi-47813.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.989 | 0.035 1.000 1.000 0.987 1.000 0.803 | 0.035 1.000 1.000 0.987 1.000 0.835 | 18 | 17 |
| `external/xlsx/poi-49872.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.988 | 0.483 1.000 0.999 1.000 1.000 0.751 | 0.593 1.000 1.000 1.000 1.000 0.822 | 1 | 1 |
| `external/xlsx/poi-50867_with_table.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.400 1.000 0.999 1.000 1.000 0.781 | 0.485 1.000 1.000 1.000 1.000 0.847 | 1 | 1 |
| `external/xlsx/poi-51222.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.359 1.000 0.997 1.000 1.000 0.759 | 0.432 1.000 1.000 1.000 1.000 0.828 | 1 | 1 |
| `external/xlsx/poi-54436.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.991 | 0.312 0.930 0.982 0.929 1.000 0.801 | 0.378 0.930 1.000 0.929 1.000 0.853 | 1 | 1 |
| `external/xlsx/poi-55814.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.429 1.000 0.999 1.000 1.000 0.764 | 0.571 1.000 1.000 1.000 1.000 0.834 | 1 | 1 |
| `external/xlsx/poi-55924.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.519 1.000 1.000 1.000 1.000 0.667 | 0.640 1.000 1.000 1.000 1.000 0.807 | 1 | 1 |
| `external/xlsx/poi-56730.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.533 1.000 1.000 1.000 1.000 0.671 | 0.571 1.000 1.000 1.000 1.000 0.809 | 1 | 1 |
| `external/xlsx/poi-57482-OnlyNumeric.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.438 1.000 1.000 1.000 1.000 0.736 | 0.533 1.000 1.000 1.000 1.000 0.811 | 1 | 1 |
| `external/xlsx/poi-58315.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.996 | 0.556 1.000 1.000 1.000 1.000 0.774 | 0.500 1.000 1.000 1.000 1.000 0.842 | 1 | 1 |
| `external/xlsx/poi-59106.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.989 | 0.269 1.000 0.992 1.000 1.000 0.756 | 0.320 1.000 1.000 1.000 1.000 0.823 | 1 | 1 |
| `external/xlsx/poi-61652.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.991 | 0.114 1.000 0.993 1.000 1.000 0.750 | 0.133 1.000 1.000 1.000 1.000 0.804 | 18 | 4 |
| `external/xlsx/poi-64450.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.993 | 0.084 0.356 0.997 0.400 1.000 0.884 | 0.084 0.356 1.000 0.400 1.000 0.920 | 4 | 4 |
| `external/xlsx/poi-65016.xlsx` | 1.000 1.000 0.900 1.000 1.000 0.989 | 0.002 1.000 0.946 1.000 1.000 0.943 | 0.002 1.000 0.999 1.000 1.000 0.943 | 953 | 369 |
| `external/xlsx/poi-ExcelTables.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.990 | 0.000 1.000 1.000 1.000 1.000 0.771 | 0.001 1.000 1.000 1.000 1.000 0.832 | 2 | 2 |
| `external/xlsx/poi-Formatting.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.264 1.000 0.984 1.000 1.000 0.774 | 0.314 1.000 1.000 1.000 1.000 0.842 | 1 | 1 |
| `external/xlsx/poi-SampleSS.strict.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.989 | 0.316 1.000 0.994 1.000 1.000 0.695 | 0.441 1.000 1.000 1.000 1.000 0.759 | 2 | — |
| `external/xlsx/poi-SampleSS.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.989 | 0.316 1.000 0.994 1.000 1.000 0.762 | 0.471 1.000 1.000 1.000 1.000 0.831 | 2 | 2 |
| `external/xlsx/poi-Themes.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.189 1.000 0.995 1.000 1.000 0.752 | 0.216 1.000 1.000 1.000 1.000 0.821 | 1 | 1 |
| `external/xlsx/poi-Themes2.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.091 1.000 0.991 1.000 1.000 0.763 | 0.182 1.000 1.000 1.000 1.000 0.825 | 1 | 1 |
| `external/xlsx/poi-WidthsAndHeights.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.474 1.000 0.999 1.000 1.000 0.789 | 0.526 1.000 1.000 1.000 1.000 0.836 | 1 | 1 |
| `external/xlsx/poi-WithChart.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.301 1.000 0.985 1.000 1.000 0.839 | 0.341 1.000 1.000 1.000 1.000 0.883 | 2 | 2 |
| `external/xlsx/poi-WithConditionalFormatting.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.989 | 0.278 1.000 1.000 1.000 1.000 0.778 | 0.327 1.000 1.000 1.000 1.000 0.842 | 1 | 1 |
| `external/xlsx/poi-WithDrawing.xlsx` | 1.000 1.000 1.000 1.000 | 0.292 1.000 0.964 0.794 | 0.533 1.000 1.000 0.844 | 2 | 2 |
| `external/xlsx/poi-bug62181.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.000 1.000 0.983 1.000 1.000 0.947 | 0.000 1.000 0.983 1.000 1.000 0.947 | 974 | 1024 |
| `external/xlsx/poi-comments.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.424 1.000 1.000 1.000 1.000 0.776 | 0.533 1.000 1.000 1.000 1.000 0.842 | 1 | 1 |
| `external/xlsx/poi-sample-beta.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.979 | 0.240 0.000 0.999 0.000 0.000 0.603 | 0.240 0.000 0.999 0.000 0.000 0.603 | 1 | 1 |
| `external/xlsx/poi-shared_formulas.xlsx` | 1.000 1.000 1.000 1.000 1.000 0.989 | 0.067 1.000 1.000 1.000 1.000 0.756 | 0.059 1.000 1.000 1.000 1.000 0.831 | 1 | 1 |
| `external/xlsx/poi-stress002.xlsx` | 1.000 1.000 0.999 1.000 1.000 0.992 | 0.013 0.991 0.991 0.975 0.409 0.624 | 0.013 0.991 0.991 0.998 0.409 0.757 | 67 | 53 |
| `external/xlsx/poi-styles.xlsx` | 1.000 1.000 1.000 1.000 1.000 1.000 | 0.031 1.000 1.000 1.000 1.000 0.775 | 0.033 1.000 1.000 1.000 1.000 0.842 | 1 | 1 |
| `pptx/features/all-layouts.pptx` | 1.000 1.000 1.000 1.000 | 0.068 0.960 0.990 0.578 | 0.152 0.960 0.992 0.616 | 11 | 11 |
| `pptx/features/animation-presets.pptx` | 1.000 1.000 1.000 1.000 | 0.174 1.000 1.000 0.680 | 0.264 1.000 1.000 0.691 | 17 | 17 |
| `pptx/features/animations.pptx` | 1.000 1.000 1.000 1.000 | 0.079 1.000 1.000 0.668 | 0.180 1.000 1.000 0.678 | 2 | 2 |
| `pptx/features/arabic-deck.pptx` | 1.000 1.000 1.000 1.000 | 0.048 1.000 0.999 0.557 | 0.138 1.000 0.999 0.596 | 3 | 3 |
| `pptx/features/aspect-16x9.pptx` | 1.000 1.000 1.000 1.000 | 0.065 1.000 1.000 0.633 | 0.156 1.000 1.000 0.636 | 2 | 2 |
| `pptx/features/aspect-4x3.pptx` | 1.000 1.000 1.000 1.000 | 0.065 1.000 1.000 0.633 | 0.156 1.000 1.000 0.636 | 2 | 2 |
| `pptx/features/backgrounds.pptx` | 1.000 1.000 1.000 1.000 | 0.047 1.000 0.926 0.561 | 0.143 1.000 0.926 0.594 | 4 | 4 |
| `pptx/features/chart-kinds.pptx` | 1.000 1.000 1.000 0.995 | 0.136 1.000 0.997 0.723 | 0.207 1.000 0.997 0.750 | 6 | 6 |
| `pptx/features/charts.pptx` | 1.000 1.000 1.000 0.996 | 0.203 1.000 0.999 0.723 | 0.262 1.000 0.999 0.749 | 9 | 9 |
| `pptx/features/cjk-deck.pptx` | 1.000 1.000 1.000 1.000 | 0.048 1.000 1.000 0.557 | 0.138 1.000 1.000 0.596 | 3 | 3 |
| `pptx/features/columns-vertical.pptx` | 1.000 1.000 1.000 1.000 | 0.044 1.000 0.997 0.607 | 0.144 1.000 0.997 0.634 | 1 | 1 |
| `pptx/features/connectors.pptx` | 1.000 1.000 1.000 1.000 | 0.063 1.000 0.991 0.626 | 0.166 1.000 0.990 0.635 | 1 | 1 |
| `pptx/features/deck-4x3.pptx` | 1.000 1.000 1.000 0.995 | 0.069 1.000 1.000 0.714 | 0.163 1.000 1.000 0.724 | 3 | 3 |
| `pptx/features/gradients-effects.pptx` | 1.000 1.000 1.000 1.000 | 0.054 1.000 0.949 0.613 | 0.157 1.000 0.949 0.630 | 1 | 1 |
| `pptx/features/groups.pptx` | 1.000 1.000 1.000 1.000 | 0.076 1.000 1.000 0.632 | 0.180 1.000 1.000 0.635 | 1 | 1 |
| `pptx/features/hyperlinks.pptx` | 1.000 1.000 1.000 1.000 | 0.052 1.000 1.000 0.623 | 0.149 1.000 1.000 0.645 | 2 | 2 |
| `pptx/features/large-table.pptx` | 1.000 1.000 1.000 1.000 | 0.038 1.000 1.000 0.587 | 0.139 1.000 1.000 0.617 | 1 | 1 |
| `pptx/features/many-slides.pptx` | 1.000 1.000 1.000 1.000 | 0.135 1.000 1.000 0.555 | 0.194 1.000 1.000 0.596 | 28 | 28 |
| `pptx/features/notes.pptx` | 1.000 1.000 1.000 1.000 | 0.053 1.000 1.000 0.562 | 0.144 1.000 1.000 0.579 | 3 | 3 |
| `pptx/features/pictures.pptx` | 1.000 1.000 1.000 1.000 | 0.047 1.000 1.000 0.577 | 0.147 1.000 1.000 0.607 | 1 | 1 |
| `pptx/features/placeholders.pptx` | 1.000 1.000 1.000 0.993 | 0.069 1.000 0.998 0.583 | 0.153 1.000 1.000 0.620 | 11 | 11 |
| `pptx/features/preset-shapes.pptx` | 1.000 1.000 1.000 1.000 | 0.343 1.000 1.000 0.594 | 0.417 1.000 1.000 0.648 | 5 | 5 |
| `pptx/features/sections.pptx` | 1.000 1.000 1.000 1.000 | 0.052 1.000 1.000 0.546 | 0.147 1.000 1.000 0.586 | 6 | 6 |
| `pptx/features/shape-styles.pptx` | 1.000 1.000 1.000 1.000 | 0.085 1.000 0.999 0.638 | 0.192 1.000 0.999 0.649 | 1 | 1 |
| `pptx/features/tables.pptx` | 1.000 1.000 1.000 1.000 | 0.051 1.000 1.000 0.590 | 0.150 1.000 1.000 0.627 | 2 | 2 |
| `pptx/features/text-autofit.pptx` | 1.000 1.000 1.000 1.000 | 0.044 1.000 0.991 0.611 | 0.144 1.000 0.954 0.644 | 1 | 1 |
| `pptx/features/text-effects.pptx` | 1.000 1.000 1.000 1.000 | 0.035 1.000 1.000 0.586 | 0.135 1.000 1.000 0.617 | 1 | 1 |
| `pptx/features/text-formatting.pptx` | 1.000 1.000 1.000 1.000 | 0.049 1.000 0.994 0.619 | 0.145 1.000 0.995 0.652 | 2 | 2 |
| `pptx/features/theme-colors.pptx` | 1.000 1.000 1.000 1.000 | 0.278 1.000 1.000 0.607 | 0.401 1.000 1.000 0.641 | 1 | 1 |
| `pptx/features/transition-presets.pptx` | 1.000 1.000 1.000 1.000 | 0.095 1.000 1.000 0.586 | 0.189 1.000 1.000 0.623 | 15 | 15 |
| `pptx/features/transitions.pptx` | 1.000 1.000 1.000 1.000 | 0.083 1.000 1.000 0.587 | 0.178 1.000 1.000 0.624 | 12 | 12 |
| `external/pptx/lo-bnc870233_2.pptx` | 1.000 1.000 1.000 1.000 | 0.036 1.000 1.000 0.395 | 0.125 1.000 1.000 0.777 | 1 | 1 |
| `external/pptx/lo-bnc910045.pptx` | 1.000 1.000 1.000 1.000 | 0.044 1.000 0.999 0.506 | 0.131 1.000 0.999 0.539 | 1 | 1 |
| `external/pptx/lo-fill-color-list.pptx` | 1.000 1.000 1.000 1.000 | 0.036 0.000 1.000 0.406 | 0.129 1.000 0.989 0.785 | 1 | 1 |
| `external/pptx/lo-paraMarginAndIndentation.pptx` | 1.000 1.000 1.000 1.000 | 0.045 1.000 1.000 0.589 | 0.132 1.000 1.000 0.635 | 1 | 1 |
| `external/pptx/lo-smartart-background-drawingml-fallback.pptx` | 1.000 1.000 1.000 1.000 | 0.037 1.000 0.997 0.391 | 0.126 1.000 1.000 0.765 | 1 | 1 |
| `external/pptx/lo-smartart-background.pptx` | 1.000 1.000 1.000 1.000 | 0.037 0.000 0.984 0.412 | 0.129 1.000 0.989 0.773 | 1 | 1 |
| `external/pptx/lo-smartart-cnt.pptx` | 1.000 1.000 1.000 1.000 | 0.035 0.000 0.998 0.415 | 0.129 1.000 0.987 0.779 | 1 | 1 |
| `external/pptx/lo-smartart-multidirectional.pptx` | 1.000 1.000 1.000 1.000 | 0.023 0.000 0.993 0.411 | 0.129 1.000 0.994 0.779 | 1 | 1 |
| `external/pptx/lo-tdf100926_ODP.pptx` | 1.000 1.000 1.000 0.988 | 0.046 1.000 1.000 0.549 | 0.133 1.000 1.000 0.571 | 1 | 1 |
| `external/pptx/lo-tdf103792.pptx` | 1.000 1.000 1.000 1.000 | 0.288 1.000 1.000 0.622 | 0.303 1.000 1.000 0.591 | 1 | 1 |
| `external/pptx/lo-tdf112089.pptx` | 1.000 1.000 1.000 1.000 | 0.103 1.000 1.000 0.773 | 0.179 1.000 1.000 0.785 | 1 | 1 |
| `external/pptx/lo-tdf113818-swivel.pptx` | 1.000 1.000 1.000 1.000 | 0.052 1.000 0.998 0.648 | 0.139 1.000 1.000 0.691 | 1 | 1 |
| `external/pptx/lo-tdf125360_1.pptx` | 1.000 1.000 1.000 0.998 | 0.342 1.000 0.998 0.595 | 0.364 1.000 1.000 0.570 | 1 | 1 |
| `external/pptx/lo-tdf132302_SmartArt_rightArrow.pptx` | 1.000 1.000 1.000 1.000 | 0.085 1.000 0.871 0.346 | 0.316 1.000 1.000 0.719 | 1 | 1 |
| `external/pptx/lo-tdf141058-1.pptx` | 1.000 1.000 1.000 0.990 | 0.088 1.000 0.997 0.444 | 0.209 1.000 0.999 0.649 | 1 | 1 |
| `external/pptx/lo-tdf142590.pptx` | 1.000 1.000 1.000 0.994 | 0.055 1.000 1.000 0.632 | 0.136 1.000 1.000 0.660 | 3 | 3 |
| `external/pptx/lo-tdf142915.pptx` | 1.000 1.000 1.000 0.994 | 0.044 1.000 1.000 0.594 | 0.136 1.000 1.000 0.648 | 2 | 2 |
| `external/pptx/lo-tdf143624.pptx` | 1.000 1.000 1.000 0.994 | 0.044 1.000 1.000 0.618 | 0.136 1.000 1.000 0.642 | 2 | 2 |
| `external/pptx/lo-tdf152335.pptx` | 1.000 1.000 1.000 0.999 | 0.310 1.000 1.000 0.364 | 0.600 1.000 1.000 0.353 | 1 | 1 |
| `external/pptx/lo-tdf90626.pptx` | 1.000 1.000 1.000 1.000 | 0.066 1.000 0.995 0.795 | 0.321 1.000 0.995 0.850 | 1 | 1 |
| `external/pptx/poi-51187.pptx` | 1.000 1.000 1.000 0.994 | 0.128 1.000 1.000 0.259 | 0.251 1.000 1.000 0.373 | 1 | 1 |
| `external/pptx/poi-60042.pptx` | 1.000 1.000 1.000 1.000 | 0.048 1.000 1.000 0.594 | 0.130 1.000 1.000 0.638 | 2 | 2 |
| `external/pptx/poi-SampleShow.pptx` | 1.000 1.000 1.000 1.000 | 0.051 1.000 0.997 0.594 | 0.134 1.000 0.999 0.615 | 2 | 2 |
| `external/pptx/poi-SmartArt.pptx` | 1.000 1.000 1.000 0.996 | 0.038 1.000 0.999 0.411 | 0.129 1.000 1.000 0.786 | 1 | 1 |
| `external/pptx/poi-alterman_security.pptx` | 1.000 1.000 1.000 0.989 | 0.088 1.000 0.999 0.578 | 0.117 1.000 0.999 0.506 | 23 | 23 |
| `external/pptx/poi-aptia.pptx` | 1.000 1.000 0.999 0.987 | 0.126 1.000 0.990 0.492 | 0.197 1.000 0.989 0.623 | 9 | 9 |
| `external/pptx/poi-backgrounds.pptx` | 1.000 1.000 1.000 0.993 | 0.114 1.000 0.993 0.649 | 0.215 1.000 0.999 0.665 | 4 | 4 |
| `external/pptx/poi-bar-chart.pptx` | 1.000 1.000 1.000 1.000 | 0.048 1.000 1.000 0.696 | 0.140 1.000 1.000 0.721 | 1 | 1 |
| `external/pptx/poi-bug60993.pptx` | 1.000 1.000 1.000 1.000 | 0.074 1.000 1.000 0.717 | 0.314 1.000 1.000 0.782 | 1 | 1 |
| `external/pptx/poi-bug64693.pptx` | 1.000 1.000 1.000 0.984 | 0.044 1.000 1.000 0.425 | 0.089 1.000 1.000 0.380 | 1 | 1 |
| `external/pptx/poi-bug65523.pptx` | 1.000 1.000 1.000 0.964 | 0.035 1.000 1.000 0.583 | 0.135 1.000 1.000 0.642 | 1 | 1 |
| `external/pptx/poi-bug68703.pptx` | 1.000 1.000 1.000 0.982 | 0.305 1.000 1.000 0.424 | 0.351 1.000 1.000 0.501 | 1 | 1 |
| `external/pptx/poi-chart-picture-bg.pptx` | 1.000 1.000 1.000 0.997 | 0.099 1.000 1.000 0.749 | 0.176 1.000 1.000 0.761 | 1 | 1 |
| `external/pptx/poi-crop-to-0.pptx` | 1.000 1.000 1.000 1.000 | 0.864 1.000 1.000 0.921 | 0.898 1.000 1.000 0.979 | 3 | 3 |
| `external/pptx/poi-hello-world-signed.pptx` | 1.000 1.000 1.000 1.000 | 0.043 1.000 1.000 0.446 | 0.128 1.000 1.000 0.482 | 1 | 1 |
| `external/pptx/poi-hello-world-unsigned.pptx` | 1.000 1.000 1.000 1.000 | 0.043 1.000 1.000 0.597 | 0.128 1.000 1.000 0.645 | 1 | 1 |
| `external/pptx/poi-highlight-test-case.pptx` | 1.000 1.000 1.000 0.983 | 0.057 1.000 0.999 0.570 | 0.143 1.000 0.999 0.593 | 1 | 1 |
| `external/pptx/poi-layouts.pptx` | 1.000 1.000 1.000 0.993 | 0.079 1.000 1.000 0.622 | 0.161 1.000 1.000 0.638 | 10 | 10 |
| `external/pptx/poi-ms-office-2010-signed.pptx` | 1.000 1.000 1.000 1.000 | 0.043 1.000 1.000 0.413 | 0.128 1.000 1.000 0.447 | 1 | 1 |
| `external/pptx/poi-pptx2svg.pptx` | 1.000 1.000 1.000 1.000 | 0.063 1.000 0.996 0.613 | 0.159 1.000 0.998 0.652 | 1 | 1 |
| `external/pptx/poi-radar-chart.pptx` | 1.000 1.000 1.000 1.000 | 0.051 1.000 0.985 0.695 | 0.143 1.000 0.985 0.720 | 1 | 1 |
| `external/pptx/poi-sample.pptx` | 1.000 1.000 1.000 1.000 | 0.069 1.000 0.998 0.608 | 0.376 1.000 0.998 0.615 | 2 | 2 |
| `external/pptx/poi-scatter-chart.pptx` | 1.000 1.000 1.000 1.000 | 0.044 1.000 1.000 0.682 | 0.134 1.000 1.000 0.708 | 1 | 1 |
| `external/pptx/poi-shapes.pptx` | 1.000 1.000 1.000 0.995 | 0.130 1.000 0.997 0.611 | 0.232 1.000 1.000 0.636 | 6 | 6 |
| `external/pptx/poi-smartart-simple.pptx` | 1.000 1.000 1.000 1.000 | 0.044 1.000 0.933 0.416 | 0.134 1.000 0.998 0.790 | 1 | 1 |
| `external/pptx/poi-stress021.pptx` | 1.000 1.000 1.000 0.989 | 0.389 0.989 0.961 0.526 | 0.530 0.989 0.988 0.515 | 41 | 41 |
| `external/pptx/poi-table_test.pptx` | 1.000 1.000 1.000 0.988 | 0.032 1.000 1.000 0.534 | 0.133 1.000 1.000 0.564 | 1 | 1 |
| `external/pptx/poi-themes.pptx` | 1.000 1.000 1.000 0.991 | 0.017 1.000 0.998 0.650 | 0.130 1.000 0.858 0.665 | 10 | 10 |
| `external/pptx/poi-tika-2605.pptx` | 1.000 1.000 1.000 0.992 | 0.112 1.000 0.991 0.579 | 0.203 1.000 0.991 0.625 | 14 | 14 |
| `external/pptx/poi-with_japanese.pptx` | 1.000 1.000 1.000 0.999 | 0.089 1.000 0.972 0.607 | 0.173 1.000 0.998 0.640 | 1 | 1 |

<!-- differential:end -->

## OPC passthrough baseline

Every package opened and saved by `kiln-ooxml` without edits (what `kiln-cli repack` does): every part, `.rels` parts included, must come out byte-identical, and `[Content_Types].xml` must declare the same content types (it is parsed on open, so it is compared by its declarations and not counted among the parts below).

| Fixtures | Packages | Parts | Identical parts | Result |
|---|---|---|---|---|
| `docx/features` | 38 | 641 | 641 | pass |
| `docx/manual` | 2 | 10 | 10 | pass |
| `external/docx` | 50 | 689 | 689 | pass |
| `xlsx/features` | 35 | 349 | 349 | pass |
| `xlsx/manual` | 3 | 25 | 25 | pass |
| `external/xlsx` | 50 | 778 | 778 | pass |
| `xlsx/charts` | 14 | 181 | 181 | pass |
| `pptx/features` | 31 | 1467 | 1467 | pass |
| `external/pptx` | 50 | 2406 | 2406 | pass |
