// SPDX-License-Identifier: AGPL-3.0-or-later
//! OpenDocument files in the session: Write opens and saves `.odt`, Grid `.ods`.
#![cfg(feature = "odf")]

use kiln_protocol::{FileFormat, OpenHint};
use kiln_session::AppFactory;
use kiln_session::apps::grid::GridApp;
use kiln_session::apps::write::WriteApp;

fn fixture(rel: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel);
    std::fs::read(path).expect("fixture")
}

fn hint(name: &str) -> OpenHint {
    OpenHint {
        name: Some(name.into()),
        ..OpenHint::default()
    }
}

#[test]
fn write_opens_and_saves_odt() {
    let app = WriteApp;
    assert!(app.opens(FileFormat::Odt));
    let doc = app
        .open(
            &fixture("odf/features/tables.odt"),
            FileFormat::Odt,
            &hint("tables.odt"),
        )
        .expect("opens");
    assert!(doc.save_formats().contains(&FileFormat::Odt));
    let odt = doc.save(FileFormat::Odt).expect("odt");
    let back = kiln_io_odf::read_odt(&odt).expect("reads back");
    assert!(!back.document.body_paragraphs().is_empty());
    let docx = doc.save(FileFormat::Docx).expect("docx");
    assert!(kiln_io_docx::read_docx(&docx).is_ok());
    // A Word document saves as OpenDocument too.
    let word = app
        .open(
            &fixture("docx/features/tables.docx"),
            FileFormat::Docx,
            &hint("tables.docx"),
        )
        .expect("opens");
    assert!(kiln_io_odf::read_odt(&word.save(FileFormat::Odt).expect("odt")).is_ok());
}

#[test]
fn grid_opens_and_saves_ods() {
    let app = GridApp;
    assert!(app.opens(FileFormat::Ods));
    let doc = app
        .open(
            &fixture("odf/features/formulas.ods"),
            FileFormat::Ods,
            &hint("formulas.ods"),
        )
        .expect("opens");
    let ods = doc.save(FileFormat::Ods).expect("ods");
    let back = kiln_io_odf::read_ods(&ods).expect("reads back");
    let xlsx = doc.save(FileFormat::Xlsx).expect("xlsx");
    let x = kiln_io_xlsx::read_xlsx(&xlsx).expect("xlsx reads");
    assert_eq!(
        kiln_io_xlsx::snapshot_cells(&back.workbook),
        kiln_io_xlsx::snapshot_cells(&x.workbook)
    );
}
