// SPDX-License-Identifier: AGPL-3.0-or-later
//! Grid workbooks in the session: opening `.xlsx` and `.csv`, status, selection, accessibility,
//! saving every format, the Compatibility Checker.

use kiln_protocol::{Command, DocKind, FileFormat, OpenHint, SuiteCommand};

use super::*;

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/xlsx/features")
        .join(name);
    std::fs::read(path).expect("fixture")
}

#[test]
fn opens_workbooks_and_saves_them_faithfully() {
    let app = GridApp;
    assert!(app.opens(FileFormat::Xlsx) && app.opens(FileFormat::Csv));
    assert!(!app.opens(FileFormat::Docx));
    let doc = app
        .open(
            &fixture("formulas.xlsx"),
            FileFormat::Xlsx,
            &OpenHint {
                name: Some("formulas.xlsx".into()),
                ..OpenHint::default()
            },
        )
        .expect("opens");
    assert_eq!(doc.kind(), DocKind::Grid);
    assert!(doc.paragraphs().is_none());
    let tree = doc.a11y("formulas");
    assert_eq!(tree.nodes[0].role, A11yRole::Document);
    let grids: Vec<_> = tree
        .nodes
        .iter()
        .filter(|n| n.role == A11yRole::Grid)
        .filter_map(|n| n.name.clone())
        .collect();
    assert_eq!(grids, ["Formulas", "Data"]);
    assert!(matches!(doc.selection().kind, SelKind::Cells { .. }));
    assert!(doc.status().cells.is_some());

    let saved = doc
        .save_at(FileFormat::Xlsx, 1_760_000_000_000)
        .expect("xlsx");
    let back = read_xlsx(&saved).expect("reads back");
    let s = back.workbook.first_sheet().expect("sheet");
    assert_eq!(back.workbook.value(s, 1, 2), CellValue::Number(210.0));
    let csv = doc.save(FileFormat::Csv).expect("csv");
    assert!(String::from_utf8_lossy(&csv).starts_with("Family,Formula,Result\r\n"));
    assert!(doc.save(FileFormat::Pdf).is_err());
    let report = doc.compat_report(FileFormat::Csv);
    assert!(
        report
            .iter()
            .any(|i| i.code == "csv.sheets" && i.count == 1)
    );
    assert!(report.iter().any(|i| i.code == "csv.formulas"));
    assert!(doc.compat_report(FileFormat::Xlsx).is_empty());
}

#[test]
fn csv_files_open_and_new_workbooks_save() {
    let app = GridApp;
    let mut doc = app
        .open(
            b"Item,Qty\r\nTea,3\r\n",
            FileFormat::Csv,
            &OpenHint {
                name: Some("orders.csv".into()),
                ..OpenHint::default()
            },
        )
        .expect("csv");
    assert_eq!(doc.a11y("orders").nodes[1].name.as_deref(), Some("orders"));
    assert!(doc.execute(&Command::Suite(SuiteCommand::Undo)).is_err());
    assert!(!doc.undo() && !doc.can_undo());
    let blank = app.create();
    let bytes = blank.save(FileFormat::Xlsx).expect("blank");
    assert!(read_xlsx(&bytes).is_ok());
    assert!(
        app.open(b"not a zip", FileFormat::Xlsx, &OpenHint::default())
            .is_err()
    );
}

#[test]
fn w3cdtf_dates() {
    assert_eq!(w3cdtf(0), "1970-01-01T00:00:00Z");
    assert_eq!(w3cdtf(1_760_000_000_000), "2025-10-09T08:53:20Z");
}
