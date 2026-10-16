# Fixtures

The test corpus of Kiln's file formats (PLAN.md §5). Rust tests reach it through `kiln-testing::fixtures`
(`discover`, `find("tables.docx")`), TypeScript tests through `@kiln/testing` (`fixturePath`, `listFixtures`),
and `parity/corpus.json` lists every feature file with the fidelity checks it must pass.

| Directory | What | Committed | Made by |
|---|---|---|---|
| `docx/features/`, `xlsx/features/`, `pptx/features/` | one file per feature area (styles, numbering, tables, sections, images, fields, comments, tracked changes, RTL; number formats, formulas, conditional formats, charts, tables; layouts, shapes, pictures, charts, transitions, animations, notes) | yes | `python3 -I tools/make-fixtures.py` |
| `docx/manual/`, `pptx/manual/` | hand-made edge cases (minimal packages, foreign prefixes) | yes | by hand |
| `docx/text/` | single-paragraph documents with their LibreOffice line breaks, for the text engine | yes | `docx/text/make_fixtures.py` |
| `ooxml/` | broken and hostile packages for the OPC/XML layer (missing content types, odd relationship targets, encodings, deep nesting, billion laughs, encrypted) | yes | `python3 -I fixtures/ooxml/make-fixtures.py` |
| `golden/` | golden images and text (`golden/scene/*.png` from `kiln-raster`, `golden/text/`) | yes | the tests, with `KILN_UPDATE_GOLDEN=1` |
| `xlsx/large/` | a 100,000-row workbook | no | `python3 -I tools/make-fixtures.py --large --only xlsx` |
| `large/` | the performance-budget inputs: `doc-300p.docx` (300 pages of headings, text, tables and pictures), `sheet-1m.xlsx` (1,000,000 cells, 200,000 formulas), `deck-200.pptx` (200 slides) | no | `python3 -I tools/make-fixtures.py --synthetic-large` |
| `external/<format>/` | 30 real-world files from the Apache POI test corpus (Apache-2.0), pinned to one commit | no | `bash tools/fetch-corpus.sh` |

## Regenerating

```sh
python3 -I tools/make-fixtures.py                       # features corpus (all formats)
python3 -I tools/make-fixtures.py --only xlsx           # one format
python3 -I tools/make-fixtures.py --synthetic-large     # fixtures/large/ (≈ 15 s)
bash tools/fetch-corpus.sh                              # fixtures/external/
```

The generator needs `python-docx`, `openpyxl` and `python-pptx`; when they are missing it creates the
virtualenv `~/.venvs/kiln` and re-runs itself there. Everything it writes is deterministic (fixed seeds,
dates and zip timestamps): regenerating gives byte-identical files, so a committed fixture only changes when
the generator does, and the large files can be compared by checksum across machines.

Adding a feature fixture: add a generator function, run the script, add the file to `parity/corpus.json`
with its checks (a test fails otherwise), and commit both.

## Oracles

`tools/oracle-soffice.sh <file> <pdf|csv|…> <outdir>` converts with LibreOffice headless using a profile that
recalculates formulas on load; `pdftoppm -r 60 -png` turns PDFs into page images. `kiln-testing::oracle`
wraps both (with time-outs) for the fidelity scorer, which skips oracle metrics where LibreOffice is not
installed (CI).
