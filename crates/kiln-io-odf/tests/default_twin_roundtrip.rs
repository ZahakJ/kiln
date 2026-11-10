// SPDX-License-Identifier: AGPL-3.0-or-later
//! A cell that shows the default under a formatted row or column holds the default's twin (the
//! same format under another index). Saved as .xlsx and as .ods and opened again, it still shows
//! the default, also once a value is typed there; the formatted cells keep their format; and the
//! styles a person can see (named cell styles) are the same as before.

use kiln_sheet::commands::CellInput;
use kiln_sheet::styles::{Fill, FontPatch, Patch, StylePatch};
use kiln_sheet::{CellValue, Color, Command, MAX_COLS, MAX_ROWS, Range, Workbook};

fn style(wb: &mut Workbook, r: Range, patch: StylePatch) {
    let sheet = wb.first_sheet().expect("a sheet");
    wb.apply(&Command::SetStyle {
        sheet,
        ranges: vec![r],
        patch: Box::new(patch),
    })
    .expect("format");
}

fn rows(r0: u32, r1: u32) -> Range {
    Range::new(r0, 0, r1, MAX_COLS - 1)
}

fn b_to_xfd() -> Range {
    Range::new(0, 1, MAX_ROWS - 1, MAX_COLS - 1)
}

/// Rows 2:1048576 bold, then B:XFD as 0.00.
fn bold_rows_then_columns() -> Workbook {
    let mut wb = Workbook::new();
    style(
        &mut wb,
        rows(1, MAX_ROWS - 1),
        StylePatch {
            font: Some(FontPatch {
                bold: Some(true),
                ..FontPatch::default()
            }),
            ..StylePatch::default()
        },
    );
    style(
        &mut wb,
        b_to_xfd(),
        StylePatch {
            num_fmt: Some("0.00".into()),
            ..StylePatch::default()
        },
    );
    wb
}

/// The whole sheet as 0.0, Clear Formats (the default) on rows 1:20, then B:XFD with a fill.
fn default_rows_then_columns() -> Workbook {
    let mut wb = Workbook::new();
    style(
        &mut wb,
        Range::new(0, 0, MAX_ROWS - 1, MAX_COLS - 1),
        StylePatch {
            num_fmt: Some("0.0".into()),
            ..StylePatch::default()
        },
    );
    let sheet = wb.first_sheet().expect("a sheet");
    wb.apply(&Command::SetStyleIndex {
        sheet,
        ranges: vec![rows(0, 19)],
        style: 0,
    })
    .expect("default");
    style(
        &mut wb,
        b_to_xfd(),
        StylePatch {
            fill: Patch::Set(Fill::solid(Color::hex("FFEE00").expect("colour"))),
            ..StylePatch::default()
        },
    );
    wb
}

fn is_default(wb: &Workbook, row: u32, col: u32) -> bool {
    let sheet = wb.first_sheet().expect("a sheet");
    let s = wb.sheet(sheet).expect("sheet");
    wb.styles().get(s.style_at(row, col)) == wb.styles().get(0)
}

fn check(label: &str, mut wb: Workbook, names_before: usize) {
    assert!(is_default(&wb, 0, 0), "{label}: A1 shows the default");
    assert!(!is_default(&wb, 0, 1), "{label}: B1 has the format");
    assert_eq!(
        wb.styles().named_styles().len(),
        names_before,
        "{label}: no style a person can see was added"
    );
    let sheet = wb.first_sheet().expect("a sheet");
    wb.apply(&Command::SetCells {
        sheet,
        cells: vec![CellInput::value(0, 0, CellValue::Number(5.0))],
    })
    .expect("typed");
    assert!(
        is_default(&wb, 0, 0),
        "{label}: a value typed in A1 shows the default"
    );
}

#[test]
fn the_default_under_formatted_rows_survives_xlsx_and_ods() {
    for (case, build) in [
        (
            "bold rows, then B:XFD",
            bold_rows_then_columns as fn() -> Workbook,
        ),
        ("default rows, then B:XFD", default_rows_then_columns),
    ] {
        let wb = build();
        let names = wb.styles().named_styles().len();
        check(&format!("{case}, in memory"), build(), names);
        let xlsx = kiln_io_xlsx::write_xlsx(&wb, None, &kiln_io_xlsx::WriteOptions::default())
            .expect("write xlsx");
        let back = kiln_io_xlsx::read_xlsx(&xlsx.bytes).expect("read xlsx");
        check(&format!("{case}, .xlsx"), back.workbook, names);
        let ods = kiln_io_odf::sheet::write_ods(&wb, &kiln_io_odf::WriteOptions::default())
            .expect("write ods");
        // No cell style of its own for the twin: an automatic table-cell style with nothing in
        // it (only its parent, Default) would be one.
        let pkg = kiln_io_odf::package::Package::open(&ods).expect("package");
        let content = String::from_utf8_lossy(pkg.file("content.xml").expect("content.xml"));
        for (i, _) in content.match_indices("<style:style ") {
            let tag = &content[i..i + content[i..].find('>').expect("tag end") + 1];
            assert!(
                !(tag.contains("style:family=\"table-cell\"") && tag.ends_with("/>")),
                "{case}: an empty automatic cell style: {tag}"
            );
        }
        let back = kiln_io_odf::sheet::read_ods(&ods).expect("read ods");
        check(&format!("{case}, .ods"), back.workbook, names);
    }
}

/// The style table never hands out the twin for the default itself.
#[test]
fn interning_the_default_never_gives_the_twin() {
    let mut wb = bold_rows_then_columns();
    let default = wb.styles().get(0);
    let mut styles = wb.styles().clone();
    let twin = styles.default_twin();
    assert_ne!(twin, 0);
    assert_eq!(styles.intern(&default), 0);
    assert_eq!(styles.patch(twin, &StylePatch::default()), twin);
    let _ = &mut wb;
}
