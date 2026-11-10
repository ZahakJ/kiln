# Right-to-left Word fixtures

Written by `make.py` in this folder (`python3 -I fixtures/docx/rtl/make.py`), part by part, as
Word writes the same content: `w:bidi` paragraphs, `w:rtl` runs with complex-script formatting,
`w:bidiVisual` tables, numbering in right-to-left paragraphs, a right-to-left section
(`w:sectPr/w:bidi`). Fonts: Times New Roman for Latin text and Noto Naskh Arabic for complex
scripts (`w:rFonts w:cs`), 12 pt Latin and 14 pt Arabic (`w:szCs`), `w:lang w:bidi="ar-SA"`.
A4, 1-inch margins. The script writes fixed zip timestamps, so the files only change when it does.

| File | What it holds |
| --- | --- |
| `arabic-digits.docx` | Arabic paragraphs with Arabic-Indic digits (١٢٣٬٤٥٦, ٢٠٢٦, ٣٫٧٥), European digits, Latin words, a vocalised sentence (harakat, shadda), the lam-alef ligature, brackets and guillemets, a phone number, a time range, a percentage and an e-mail address |
| `arabic-justified.docx` | Long Arabic paragraphs with `w:jc="both"` (Word stretches the spaces), a short justified one, a centred one and one aligned to the end of the line (`w:jc="right"`, the left side in a right-to-left paragraph) |
| `bidi-table.docx` | An invoice table with `w:bidiVisual`: the first column at the right, a shaded repeated header row, numbers aligned to the cells' ends, a total row spanning three columns |
| `mixed-runs.docx` | Left-to-right and right-to-left runs on one line in both paragraph directions: Arabic words in English sentences, English words, version numbers and lists in Arabic sentences, bold and italic runs, guillemets |
| `rtl-lists.docx` | Numbered (`decimal`, `arabicAlpha` at level 2), bulleted and `arabicAbjad` lists in right-to-left paragraphs |

## Oracle

Each file opens in LibreOffice 26.2 (`tools/oracle-soffice.sh <file> pdf <dir>`, then
`pdftoppm -r 96`), which is the reference the goldens in `fixtures/golden/rtl/` were checked
against: Arabic joining, the lam-alef ligature and the marks, the order of the runs on each line,
the line breaks of the justified paragraphs, the right-to-left table and the list labels at the
right margin (".1" as Word draws it) agree. Known differences from LibreOffice, where Kiln follows
Word: a list label in a right-to-left paragraph sits at the hanging indent (LibreOffice moves it
nearer the margin), and LibreOffice wraps "2026." of the third paragraph of
`arabic-digits.docx` one word earlier.
