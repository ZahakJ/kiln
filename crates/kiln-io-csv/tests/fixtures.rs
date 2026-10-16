// SPDX-License-Identifier: AGPL-3.0-or-later
//! The files of `fixtures/csv/`: what Kiln makes of each, and that saving gives text that reads
//! back to the same values.

use kiln_formula::ErrorValue;
use kiln_io_csv::{
    Encoding, ExportOptions, ImportOptions, read_csv, read_json, sheet_to_text, write_json,
};
use kiln_sheet::CellValue;

fn fixture(name: &str) -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/csv")
        .join(name);
    std::fs::read(p).expect("fixture")
}

#[test]
fn excel_csv_utf8_with_semicolons() {
    let imp = read_csv(
        &fixture("semicolon-utf8-bom.csv"),
        &ImportOptions::default(),
    )
    .expect("csv");
    assert_eq!(
        (imp.encoding, imp.dialect.delimiter),
        (Encoding::Utf8Bom, ';')
    );
    assert_eq!((imp.report.rows, imp.report.cols), (4, 4));
    let wb = &imp.workbook;
    let s = wb.first_sheet().expect("sheet");
    assert_eq!(wb.value(s, 2, 0), CellValue::text("Café au lait"));
    assert_eq!(wb.value(s, 2, 1), CellValue::Number(3.25));
    assert_eq!(wb.value(s, 1, 2), CellValue::Number(0.12));
    assert_eq!(wb.display_text(s, 1, 2), "12%");
    assert_eq!(wb.value(s, 1, 3), CellValue::Number(46_037.0));
    assert_eq!(wb.value(s, 3, 0), CellValue::text("Cake; large"));
    // Saved back with the same delimiter, the values read again the same.
    let text = sheet_to_text(
        wb,
        s,
        &ExportOptions {
            dialect: imp.dialect,
            ..ExportOptions::default()
        },
    );
    let again = read_csv(text.as_bytes(), &ImportOptions::default()).expect("again");
    for r in 0..4 {
        for c in 0..4 {
            assert_eq!(again.workbook.value(s, r, c), wb.value(s, r, c), "{r},{c}");
        }
    }
}

#[test]
fn excel_unicode_text_and_windows_1252() {
    let imp = read_csv(&fixture("unicode-text.txt"), &ImportOptions::default()).expect("utf-16");
    assert_eq!(
        (imp.encoding, imp.dialect.delimiter),
        (Encoding::Utf16Le, '\t')
    );
    let s = imp.workbook.first_sheet().expect("sheet");
    assert_eq!(imp.workbook.value(s, 2, 1), CellValue::text("北京"));
    assert_eq!(imp.workbook.value(s, 1, 2), CellValue::Number(91.0));
    let imp = read_csv(&fixture("windows-1252.csv"), &ImportOptions::default()).expect("1252");
    assert_eq!(imp.encoding, Encoding::Windows1252);
    assert_eq!(imp.workbook.value(s, 1, 0), CellValue::text("Crème brûlée"));
    assert_eq!(imp.workbook.value(s, 1, 1), CellValue::text("€4.50"));
}

#[test]
fn rfc4180_edge_cases() {
    let imp = read_csv(&fixture("rfc4180.csv"), &ImportOptions::default()).expect("csv");
    let wb = &imp.workbook;
    let s = wb.first_sheet().expect("sheet");
    assert_eq!(wb.value(s, 1, 1), CellValue::text("He said \"hi\""));
    assert_eq!(wb.value(s, 1, 2), CellValue::text("line one\nline two"));
    assert_eq!(wb.value(s, 2, 1), CellValue::Blank);
    assert_eq!(wb.value(s, 2, 2), CellValue::text("comma, inside"));
    assert_eq!(wb.value(s, 3, 1), CellValue::text("007"));
    let out = sheet_to_text(wb, s, &ExportOptions::default());
    assert!(
        out.contains("\"He said \"\"hi\"\"\",\"line one\nline two\""),
        "{out}"
    );
    assert!(
        read_csv(b"#DIV/0!,x", &ImportOptions::default())
            .expect("errors")
            .workbook
            .value(s, 0, 0)
            == CellValue::Error(ErrorValue::Div0)
    );
}

#[test]
fn json_records_become_a_table_and_write_back() {
    let (wb, report) = read_json(&fixture("people.json"), &ImportOptions::default()).expect("json");
    assert_eq!((report.rows, report.cols), (4, 5));
    let s = wb.first_sheet().expect("sheet");
    let keys: Vec<CellValue> = (0..5).map(|c| wb.value(s, 0, c)).collect();
    assert_eq!(
        keys,
        ["name", "age", "member", "joined", "tags"].map(CellValue::text)
    );
    assert_eq!(wb.value(s, 2, 4), CellValue::text("[\"kid\",\"chess\"]"));
    assert_eq!(wb.value(s, 3, 1), CellValue::Blank);
    assert_eq!(wb.sheet(s).expect("sheet").tables().len(), 1);
    let back: serde_json::Value = serde_json::from_slice(&write_json(&wb, s)).expect("valid");
    assert_eq!(back[2]["member"], serde_json::Value::Bool(false));
    assert_eq!(back.as_array().map(Vec::len), Some(3));
}
