// SPDX-License-Identifier: AGPL-3.0-or-later
//! The LibreOffice oracle against real fixtures. Skipped (with a message) where LibreOffice or
//! poppler is not installed, as on CI runners.

use kiln_testing::fixtures;
use kiln_testing::image;
use kiln_testing::oracle::Oracle;

fn scratch(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("kiln-oracle-{name}-{}", std::process::id()))
}

#[test]
fn renders_a_document_to_page_images() {
    let Ok(oracle) = Oracle::locate() else {
        eprintln!("skipped: LibreOffice is not installed");
        return;
    };
    if !oracle.can_render_pages() {
        eprintln!("skipped: pdftoppm is not installed");
        return;
    }
    let dir = scratch("render");
    let f = fixtures::find("breaks.docx").unwrap();
    let pdf = oracle.convert(&f.path, "pdf", &dir).unwrap();
    assert_eq!(pdf.extension().unwrap(), "pdf");
    let pages = oracle.render_pages(&pdf, 30, &dir, "page").unwrap();
    assert!(pages.len() >= 2, "breaks.docx has page breaks: {pages:?}");
    let first = image::load_png(&pages[0]).unwrap();
    // US Letter or A4 at 30 dpi.
    assert!((240..=260).contains(&first.width), "{}", first.width);
    // The page has ink on it.
    let dark = first
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] < 128)
        .count();
    assert!(dark > 50, "{dark} dark pixels");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn recalculates_formulas_when_converting_to_csv() {
    let Ok(oracle) = Oracle::locate() else {
        eprintln!("skipped: LibreOffice is not installed");
        return;
    };
    let dir = scratch("csv");
    let f = fixtures::find("formulas.xlsx").unwrap();
    let csv = oracle.convert(&f.path, "csv", &dir).unwrap();
    let text = std::fs::read_to_string(&csv).unwrap();
    assert!(text.lines().count() > 10, "{text}");
    let missing = dir.join("missing.xlsx");
    assert!(oracle.convert(&missing, "csv", &dir).is_err());
    assert!(!oracle.opens(&missing, &dir));
    let _ = std::fs::remove_dir_all(&dir);
}
