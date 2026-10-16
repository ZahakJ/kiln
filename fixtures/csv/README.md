# CSV, TSV and JSON fixtures

Hand-written inputs for `kiln-io-csv` (WP-70), one per case Excel users meet; the tests in
`crates/kiln-io-csv/tests/fixtures.rs` read every file and check what Kiln makes of it.

| File | What |
|---|---|
| `semicolon-utf8-bom.csv` | Excel's "CSV UTF-8": a byte order mark, `;` as the delimiter, percentages, ISO dates, a quoted field with the delimiter |
| `unicode-text.txt` | Excel's "Unicode Text": UTF-16 LE with a byte order mark, tab-separated, non-Latin text |
| `windows-1252.csv` | Excel's "CSV" on Western Windows: Windows-1252 with accented letters and `€` |
| `rfc4180.csv` | RFC 4180 edge cases: doubled quotes, a line break inside a quoted field, empty fields, LF line ends, a `'`-prefixed field kept as text |
| `people.json` | A JSON array of objects with numbers, logicals, nulls, a nested array and a missing key |
