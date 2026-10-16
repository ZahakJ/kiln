// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-io-csv
//!
//! Delimited text and JSON for Kiln Grid:
//!
//! - **Import** ([`read_csv`]): the encoding is detected (UTF-8 with or without BOM, UTF-16
//!   with or without BOM, else Windows-1252), the delimiter sniffed (`,` `;` tab `|`) unless
//!   given, records parsed per RFC 4180 (quoted fields, doubled quotes, line breaks inside
//!   quotes, any line ending), and each field recognised as Excel's cell entry does (numbers,
//!   percentages, currency, dates, times, logicals, errors) with the implied number format.
//! - **Export** ([`write_delimited`]): CSV or TSV of a sheet as Excel saves it — A1 to the end
//!   of the used range, cells as displayed, fields quoted only when needed, CRLF.
//! - **JSON** ([`read_json`], [`write_json`]): an array of objects becomes a table (keys in the
//!   order they first appear); a table writes back as an array of objects.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dialect;
pub mod encoding;
pub mod export;
pub mod import;
pub mod json;

pub use dialect::{Dialect, parse_records, sniff};
pub use encoding::Encoding;
pub use export::{ExportOptions, LineEnding, number_text, sheet_to_text, write_delimited};
pub use import::{ImportReport, records_to_workbook};
pub use json::{read_json, write_json};

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Why a file could not be imported.
#[derive(Debug, thiserror::Error)]
pub enum CsvError {
    /// The model refused the data.
    #[error("the sheet could not be built: {0}")]
    Model(#[from] kiln_sheet::ModelError),
    /// Not a JSON array.
    #[error("not a JSON array of records: {0}")]
    Json(String),
}

/// How to import.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportOptions {
    /// The delimiter (`None` = sniffed; pass tab for `.tsv`).
    pub delimiter: Option<char>,
    /// The encoding (`None` = detected).
    pub encoding: Option<Encoding>,
    /// Read `=…` fields as formulas (Excel does; off by default, so a file cannot plant
    /// formulas).
    pub formulas: bool,
    /// Keep every field as text (no number or date recognition).
    pub as_text: bool,
    /// The 1904 date system for recognised dates.
    pub date1904: bool,
    /// The sheet name (default "Sheet1"; invalid names fall back to it).
    pub sheet_name: Option<String>,
}

/// An imported file.
#[derive(Debug)]
pub struct Imported {
    /// The workbook (one sheet).
    pub workbook: kiln_sheet::Workbook,
    /// The encoding read.
    pub encoding: Encoding,
    /// The dialect read.
    pub dialect: Dialect,
    /// Size and warnings.
    pub report: ImportReport,
}

/// Reads delimited text into a one-sheet workbook.
///
/// # Errors
///
/// [`CsvError::Model`] when the model refuses the data.
pub fn read_csv(bytes: &[u8], options: &ImportOptions) -> Result<Imported, CsvError> {
    let encoding = options.encoding.unwrap_or_else(|| encoding::detect(bytes));
    let text = encoding::decode(bytes, encoding);
    let mut dialect = sniff(&text);
    if let Some(d) = options.delimiter {
        dialect.delimiter = d;
    }
    let records = parse_records(&text, dialect);
    let (workbook, report) = records_to_workbook(&records, options)?;
    Ok(Imported {
        workbook,
        encoding,
        dialect,
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_sheet::CellValue;

    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }

    #[test]
    fn imports_typed_values_with_formats() {
        let csv = "Item;Price;Share;Date;Flag;Note\r\nTea;1,5;12%;2026-01-15;TRUE;'007\r\n\"Cake; big\";$3.50;;1/2/2026;#N/A;=1+1\r\n";
        let imp = read_csv(csv.as_bytes(), &ImportOptions::default()).expect("csv");
        assert_eq!(imp.dialect.delimiter, ';');
        assert_eq!(imp.encoding, Encoding::Utf8);
        let wb = &imp.workbook;
        let s = wb.first_sheet().expect("sheet");
        assert_eq!(wb.value(s, 1, 0), CellValue::text("Tea"));
        assert_eq!(wb.value(s, 1, 2), CellValue::Number(0.12));
        assert_eq!(wb.display_text(s, 1, 2), "12%");
        assert_eq!(wb.value(s, 1, 3), CellValue::Number(46_037.0));
        assert_eq!(wb.value(s, 1, 4), CellValue::Bool(true));
        assert_eq!(wb.value(s, 1, 5), CellValue::text("007"));
        assert_eq!(wb.value(s, 2, 0), CellValue::text("Cake; big"));
        assert_eq!(wb.value(s, 2, 1), CellValue::Number(3.5));
        assert_eq!(wb.value(s, 2, 2), CellValue::Blank);
        assert_eq!(
            wb.value(s, 2, 4),
            CellValue::Error(kiln_formula::ErrorValue::NA)
        );
        // Formulas stay text unless asked for.
        assert_eq!(wb.value(s, 2, 5), CellValue::text("=1+1"));
        let f = read_csv(
            csv.as_bytes(),
            &ImportOptions {
                formulas: true,
                ..ImportOptions::default()
            },
        )
        .expect("csv");
        let s2 = f.workbook.first_sheet().expect("sheet");
        assert_eq!(f.workbook.value(s2, 2, 5), CellValue::Number(2.0));
    }

    #[test]
    fn exports_like_excel_and_reads_back() {
        let csv = "a,b,c\r\n1,\"x,y\",2.5\r\n,\"q\"\"uote\",\r\n";
        let imp = read_csv(csv.as_bytes(), &ImportOptions::default()).expect("csv");
        let s = imp.workbook.first_sheet().expect("sheet");
        let out = sheet_to_text(&imp.workbook, s, &ExportOptions::default());
        assert_eq!(out, csv);
        let tsv = sheet_to_text(&imp.workbook, s, &ExportOptions::tsv());
        assert_eq!(tsv, "a\tb\tc\r\n1\tx,y\t2.5\r\n\t\"q\"\"uote\"\t\r\n");
        let bytes = write_delimited(
            &imp.workbook,
            s,
            &ExportOptions {
                encoding: Encoding::Utf16Le,
                ..ExportOptions::tsv()
            },
        );
        let back = read_csv(&bytes, &ImportOptions::default()).expect("utf16");
        assert_eq!(back.encoding, Encoding::Utf16Le);
        assert_eq!(back.dialect.delimiter, '\t');
        assert_eq!(
            sheet_to_text(&back.workbook, s, &ExportOptions::default()),
            csv
        );
    }

    #[test]
    fn windows_1252_files_are_read() {
        let bytes = encoding::encode("Name,Price\r\nCafé,€3\r\n", Encoding::Windows1252);
        let imp = read_csv(&bytes, &ImportOptions::default()).expect("csv");
        assert_eq!(imp.encoding, Encoding::Windows1252);
        let s = imp.workbook.first_sheet().expect("sheet");
        assert_eq!(imp.workbook.value(s, 1, 0), CellValue::text("Café"));
    }
}
