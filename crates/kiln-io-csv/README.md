# kiln-io-csv

Delimited text and JSON for Kiln Grid (WP-70): CSV and TSV in and out with the encoding and
delimiter detected, Excel's cell-entry recognition on import and Excel's "Save As CSV" on
export, and JSON arrays of objects as tables. No I/O; runs natively and in the web build.

```rust
let imp = kiln_io_csv::read_csv(&bytes, &ImportOptions::default())?;   // one-sheet Workbook
println!("{} {:?}", imp.encoding.label(), imp.dialect.delimiter);       // "UTF-8 with BOM" ';'
let sheet = imp.workbook.first_sheet().unwrap();
let csv = kiln_io_csv::write_delimited(&imp.workbook, sheet, &ExportOptions::default());
let tsv = kiln_io_csv::write_delimited(&imp.workbook, sheet, &ExportOptions::tsv());
let (wb, _) = kiln_io_csv::read_json(br#"[{"a":1},{"a":2,"b":true}]"#, &ImportOptions::default())?;
```

## Import

- **Encoding** (`encoding::detect`): a UTF-8, UTF-16 LE or UTF-16 BE byte order mark; UTF-16
  without a mark (from where the zero bytes fall); valid UTF-8; otherwise Windows-1252, as Excel
  assumes for "CSV" on Western systems. `ImportOptions::encoding` overrides it.
- **Delimiter** (`dialect::sniff`): `,` `;` tab or `|`, whichever gives the most consistent field
  count over the first 50 lines (quotes respected). `ImportOptions::delimiter` overrides it
  (tab for `.tsv`).
- **Records** (`dialect::parse_records`): RFC 4180 — quoted fields, doubled quotes, line breaks
  inside quotes, CRLF, LF or CR line ends — and tolerant of what spreadsheets meet (a quote
  inside an unquoted field is a character).
- **Values**: each field goes through `kiln_sheet::data::parse_input`, Excel's cell entry:
  numbers with thousands separators, percentages, currency, dates, times, logicals and error
  codes become typed values with the number format the typing implies; a leading `'` keeps
  text. `=…` fields stay text unless `ImportOptions::formulas` is set (Excel evaluates them;
  Kiln does not by default, so a file cannot plant formulas). `as_text` keeps everything text.
- **JSON** (`json::read_json`): an array of objects becomes a header row of the keys, in the
  order they first appear, one row per object with typed values (numbers, logicals, text,
  `null` as an empty cell, nested values as JSON text) and a table over it; an array of arrays
  becomes plain rows.

## Export

`write_delimited` writes a sheet as Excel's "Save As CSV" does: from A1 to the last row and
column with content (spilled values included), each cell as displayed by its number format,
General numbers with up to 15 significant digits, fields quoted only when they hold the
delimiter, a quote, a line break or edge spaces, CRLF line ends, UTF-8 (`Encoding` also offers
UTF-8 with BOM, UTF-16 and Windows-1252). `ExportOptions::as_displayed = false` writes raw values.
`write_json` writes a sheet's first table (or its used range, first row as keys) as an array of
objects.

## Tests

`cargo nextest run -p kiln-io-csv`: unit tests of every module (a property test that any
fields written with any dialect read back unchanged; every encoding detected and round-tripped)
and `tests/fixtures.rs` over the files of `fixtures/csv/` (Excel's CSV UTF-8 with semicolons,
Unicode Text in UTF-16, Windows-1252, RFC 4180 edge cases, JSON records).
`kiln-cli convert` uses this crate for `.csv`, `.tsv` and `.json`; the comparison of its CSV with
LibreOffice's is in `crates/kiln-io-xlsx/tests/libreoffice_values.rs`.

Licence: AGPL-3.0-or-later.
