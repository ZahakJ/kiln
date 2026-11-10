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

#[test]
fn json_round_trip_keeps_integers_as_integers() {
    let src = fixture("people.json");
    let (wb, _) = read_json(&src, &ImportOptions::default()).expect("json");
    let s = wb.first_sheet().expect("sheet");
    let out = String::from_utf8(write_json(&wb, s)).expect("utf8");
    assert!(!out.contains(".0,") && !out.contains(".0}"), "{out}");
    let before: serde_json::Value = serde_json::from_slice(&src).expect("source");
    let after: serde_json::Value = serde_json::from_str(&out).expect("output");
    assert_eq!(after[0]["age"], before[0]["age"]);
    assert!(after[0]["age"].is_i64());
}

#[test]
fn json_mixed_arrays_keep_every_item() {
    let json = br#"[{"a":1,"b":"x"},{"z":null,"a":2},1,"s",[1]]"#;
    let (wb, report) = read_json(json, &ImportOptions::default()).expect("json");
    let s = wb.first_sheet().expect("sheet");
    let header: Vec<CellValue> = (0..4).map(|c| wb.value(s, 0, c)).collect();
    assert_eq!(header, ["a", "b", "z", "value"].map(CellValue::text));
    assert_eq!(wb.value(s, 1, 0), CellValue::Number(1.0));
    assert_eq!(wb.value(s, 2, 0), CellValue::Number(2.0));
    assert_eq!(wb.value(s, 3, 3), CellValue::Number(1.0));
    assert_eq!(wb.value(s, 4, 3), CellValue::text("s"));
    assert_eq!(wb.value(s, 5, 3), CellValue::text("[1]"));
    assert_eq!(wb.value(s, 3, 0), CellValue::Blank);
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(report.warnings[0].contains("\"value\""));
    // A key already called "value" is not overwritten.
    let (wb, _) = read_json(br#"[{"value":1},2]"#, &ImportOptions::default()).expect("json");
    let s = wb.first_sheet().expect("sheet");
    assert_eq!(wb.value(s, 0, 1), CellValue::text("value 2"));
    assert_eq!(wb.value(s, 2, 1), CellValue::Number(2.0));
}

#[test]
fn json_with_more_keys_than_columns_is_truncated_with_a_warning() {
    let keys = 20_000;
    let mut json = String::from("[{");
    for k in 0..keys {
        if k > 0 {
            json.push(',');
        }
        json.push_str(&format!("\"k{k}\":{k}"));
    }
    // A repeated key keeps its first position and last value.
    json.push_str(",\"k0\":-1}]");
    let started = std::time::Instant::now();
    let (wb, report) = read_json(json.as_bytes(), &ImportOptions::default()).expect("json");
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    let s = wb.first_sheet().expect("sheet");
    assert_eq!(wb.value(s, 0, 16_383), CellValue::text("k16383"));
    assert_eq!(wb.value(s, 1, 0), CellValue::Number(-1.0));
    assert_eq!(wb.value(s, 1, 16_383), CellValue::Number(16_383.0));
    assert!(
        report.warnings.iter().any(|w| w.contains("20000 keys")),
        "{:?}",
        report.warnings
    );
    assert_eq!(wb.sheet(s).expect("sheet").tables().len(), 1);
}

#[test]
fn csv_limits_are_reported_once_and_cells_are_cut_to_fit() {
    // Three rows wider than the sheet: one warning, not one per row.
    let wide = format!("{}\n", vec!["1"; 16_390].join(","));
    let text = format!("{wide}{wide}{wide}");
    let imp = read_csv(text.as_bytes(), &ImportOptions::default()).expect("csv");
    assert_eq!(imp.report.warnings.len(), 1, "{:?}", imp.report.warnings);
    assert!(imp.report.warnings[0].starts_with("3 row(s)"));
    // A 5,000,000-character field is cut to Excel's 32,767-character cell limit, counted in
    // UTF-16 units and never splitting a character.
    let long = format!(
        "a,{}\nb,{}\n",
        "x".repeat(5_000_000),
        "\u{1F600}".repeat(20_000)
    );
    let imp = read_csv(long.as_bytes(), &ImportOptions::default()).expect("csv");
    let s = imp.workbook.first_sheet().expect("sheet");
    let CellValue::Text(t) = imp.workbook.value(s, 0, 1) else {
        panic!("text expected");
    };
    assert_eq!(t.len(), 32_767);
    let CellValue::Text(t) = imp.workbook.value(s, 1, 1) else {
        panic!("text expected");
    };
    assert_eq!(t.encode_utf16().count(), 32_766);
    assert_eq!(imp.report.warnings.len(), 1, "{:?}", imp.report.warnings);
    assert!(imp.report.warnings[0].starts_with("2 cell(s)"));
}
