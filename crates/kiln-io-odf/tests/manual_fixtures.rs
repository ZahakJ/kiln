// SPDX-License-Identifier: AGPL-3.0-or-later
//! The hand-made fixtures of `fixtures/odf/manual` (written by its `make.py`): what LibreOffice's
//! own files do not exercise.

use std::path::PathBuf;

use kiln_doc::query::PlainTextOptions;
use kiln_sheet::CellValue;

fn fixture(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/odf/manual")
        .join(name);
    std::fs::read(p).expect("fixture")
}

fn text(doc: &kiln_doc::Document) -> String {
    doc.plain_text(PlainTextOptions {
        include_headers_footers: true,
        include_notes: true,
        accept_changes: false,
        list_labels: false,
    })
}

#[test]
fn a_package_without_styles_or_manifest_reads() {
    let r = kiln_io_odf::read_odt(&fixture("minimal.odt")).expect("reads");
    let t = text(&r.document);
    assert!(t.contains("bold red text,   three spaces,\ta tab"), "{t:?}");
    assert!(
        t.contains("Nested item") && t.contains("The note."),
        "{t:?}"
    );
    let back =
        kiln_io_odf::write_odt(&r.document, &kiln_io_odf::WriteOptions::default()).expect("writes");
    let again = kiln_io_odf::read_odt(&back).expect("reads back");
    assert_eq!(text(&again.document), t);
}

#[test]
fn foreign_prefixes_and_unknown_elements_read() {
    let r = kiln_io_odf::read_odt(&fixture("foreign-prefixes.odt")).expect("reads");
    let t = text(&r.document);
    assert!(
        t.contains("Foreign prefixes") && t.contains("Plain and italic text kept."),
        "{t:?}"
    );
    assert!(!t.contains("hidden"), "{t:?}");
}

#[test]
fn repeated_rows_matrix_and_legacy_formulas_read() {
    let r = kiln_io_odf::read_ods(&fixture("structures.ods")).expect("reads");
    let wb = &r.workbook;
    let s = wb.first_sheet().expect("sheet");
    // Rows 2–4 are one repeated row.
    for row in 1..=3 {
        assert_eq!(wb.value(s, row, 1), CellValue::Number(2.0), "row {row}");
    }
    assert_eq!(wb.formula(s, 4, 2).as_deref(), Some("SUM(C2:C4)"));
    assert_eq!(wb.value(s, 4, 2), CellValue::Number(-4.5));
    assert_eq!(wb.formula(s, 4, 3).as_deref(), Some("B2*C2"));
    // The matrix formula keeps array semantics.
    assert_eq!(wb.value(s, 5, 2), CellValue::Number(-9.0));
    assert_eq!(wb.value(s, 5, 0), CellValue::Number(46_312.0));
    assert_eq!(
        wb.value(s, 5, 1),
        CellValue::Number(1.520_833_333_333_333_3)
    );
    assert_eq!(
        wb.value(s, 6, 1),
        CellValue::Text("Same, Same, Same".into())
    );
    assert_eq!(wb.value(s, 6, 2), CellValue::Text("two  spaces".into()));
    let sheet = wb.sheet(s).expect("sheet");
    assert_eq!(sheet.meta().merges.len(), 1);
    assert!(sheet.rows().get(5).is_some_and(|p| p.outline_level == 1));
    assert!(
        wb.names()
            .iter()
            .any(|n| n.name == "Prices" && n.formula == "Data!$C$2:$C$4")
    );
    assert_eq!(
        wb.styles()
            .get(wb.cell(s, 5, 1).expect("cell").style)
            .num_fmt
            .as_ref(),
        "[h]:mm"
    );
    let back = kiln_io_odf::write_ods(wb, &kiln_io_odf::WriteOptions::default()).expect("writes");
    let again = kiln_io_odf::read_ods(&back).expect("reads back");
    assert_eq!(
        kiln_io_xlsx::snapshot_cells(&again.workbook),
        kiln_io_xlsx::snapshot_cells(wb)
    );
}

#[test]
fn rotated_shapes_lines_groups_and_notes_read() {
    let r = kiln_io_odf::read_odp(&fixture("shapes.odp")).expect("reads");
    let p = &r.presentation;
    assert_eq!(p.slides.len(), 1);
    assert_eq!(p.slide_size.cx, 9_144_000);
    let s = &p.slides[0];
    let rotated = s
        .shapes
        .iter()
        .find(|x| x.name == "Rotated")
        .expect("shape");
    let x = rotated.xfrm.expect("xfrm");
    // 30° counter-clockwise in ODF is 330° clockwise.
    assert_eq!(x.rot, 330 * 60_000);
    assert!(
        s.shapes
            .iter()
            .any(|x| matches!(x.kind, kiln_deck::ShapeKind::Connector(_)))
    );
    assert!(
        s.shapes
            .iter()
            .any(|x| matches!(&x.kind, kiln_deck::ShapeKind::Group(g) if g.children.len() == 2))
    );
    assert!(s.notes_text().contains("Speaker notes"));
    assert_eq!(s.transition.as_ref().map(|t| t.effect.name()), Some("Push"));
    let back = kiln_io_odf::write_odp(p, &kiln_io_odf::WriteOptions::default()).expect("writes");
    let again = kiln_io_odf::read_odp(&back).expect("reads back");
    let x2 = again.presentation.slides[0]
        .shapes
        .iter()
        .find(|x| x.name == "Rotated")
        .and_then(|x| x.xfrm)
        .expect("xfrm");
    assert_eq!(x2.rot, x.rot);
    assert!(
        (x2.x - x.x).abs() <= 360 && (x2.y - x.y).abs() <= 360,
        "{x:?} {x2:?}"
    );
}
