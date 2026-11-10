// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pagination and printed pages.

use kiln_io_pdf::printable::{LinkTarget, Mark, PageCanvas, PrintablePages, StructId, TextRun};
use kiln_scene::{Bounds, DrawOp};
use kiln_sheet::commands::{CellInput, Content, MetaField};
use kiln_sheet::meta::{DefinedName, Orientation};
use kiln_sheet::{Command, Range, Workbook};

use super::pages::{PrintOptions, WorkbookPrint};
use super::*;

fn numbers(rows: u32, cols: u32) -> Workbook {
    let mut wb = Workbook::new();
    let sid = wb.first_sheet().expect("a sheet");
    let mut cells = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            cells.push(CellInput {
                row: r,
                col: c,
                content: Content::Value(kiln_sheet::CellValue::Number(f64::from(
                    (r + 1) * (c + 1),
                ))),
                style: None,
            });
        }
    }
    wb.apply(&Command::SetCells { sheet: sid, cells })
        .expect("cells");
    wb
}

fn set_print(wb: &mut Workbook, f: impl FnOnce(&mut PrintSettings)) {
    let sid = wb.first_sheet().expect("a sheet");
    let mut p = wb.sheet(sid).expect("sheet").meta().print.clone();
    f(&mut p);
    wb.apply(&Command::SetSheetMeta {
        sheet: sid,
        field: Box::new(MetaField::Print(p)),
    })
    .expect("print settings");
}

fn define(wb: &mut Workbook, name: &str, formula: &str) {
    let sid = wb.first_sheet().expect("a sheet");
    wb.load_name(DefinedName {
        name: name.into(),
        formula: formula.into(),
        scope: Some(sid),
        hidden: false,
        comment: None,
    })
    .expect("name");
}

#[test]
fn a_letter_portrait_page_holds_the_rows_that_fit() {
    let wb = numbers(100, 3);
    let p = paginate(&wb, 0).expect("pages");
    // Letter, 0.75 in top and bottom margins: 9.5 in = 912 px of 20 px rows.
    assert_eq!(p.width, 816.0);
    assert_eq!(p.height, 1056.0);
    assert_eq!(p.pages[0].rows, (0, 44));
    assert_eq!(p.pages.len(), 3);
    assert_eq!(p.row_breaks, vec![45, 90]);
    assert!(p.col_breaks.is_empty());
}

#[test]
fn fit_to_one_page_wide_scales_down_and_keeps_manual_row_breaks() {
    let mut wb = numbers(120, 40);
    set_print(&mut wb, |p| {
        p.fit_to_page = true;
        p.fit_to_width = 1;
        p.fit_to_height = 0;
        p.orientation = Orientation::Landscape;
        p.paper_size = 9;
        p.row_breaks = vec![60];
    });
    let p = paginate(&wb, 0).expect("pages");
    assert!(p.scale < 1.0, "scaled to {}", p.scale);
    assert!(p.pages.iter().all(|pg| pg.cols == (0, 39)), "one page wide");
    assert!(
        p.row_breaks.contains(&60),
        "the manual break stays: {:?}",
        p.row_breaks
    );
}

#[test]
fn fit_to_one_page_ignores_manual_breaks() {
    let mut wb = numbers(200, 10);
    set_print(&mut wb, |p| {
        p.fit_to_page = true;
        p.fit_to_width = 1;
        p.fit_to_height = 1;
        p.row_breaks = vec![50];
    });
    let p = paginate(&wb, 0).expect("pages");
    assert_eq!(p.pages.len(), 1);
    assert!(p.scale <= 0.5);
}

#[test]
fn print_area_and_titles_come_from_the_reserved_names() {
    let mut wb = numbers(200, 4);
    define(&mut wb, "_xlnm.Print_Area", "'Sheet1'!$A$1:$C$150");
    define(&mut wb, "_xlnm.Print_Titles", "Sheet1!$1:$2");
    let p = paginate(&wb, 0).expect("pages");
    assert_eq!(p.areas, vec![Range::new(0, 0, 149, 2)]);
    assert_eq!(p.setup.title_rows, Some((0, 1)));
    assert_eq!(
        p.pages[0].title_rows, None,
        "the first page starts with the titles"
    );
    assert_eq!(p.pages[1].title_rows, Some((0, 1)));
    // The titles take two rows of every later page.
    assert_eq!(p.pages[1].rows.1 - p.pages[1].rows.0 + 1, 43);
}

#[test]
fn over_then_down_orders_pages_by_rows_first() {
    let mut wb = numbers(100, 30);
    set_print(&mut wb, |p| p.down_then_over = false);
    let p = paginate(&wb, 0).expect("pages");
    assert!(p.pages.len() >= 4);
    assert_eq!(p.pages[0].rows, p.pages[1].rows);
    assert_ne!(p.pages[0].cols, p.pages[1].cols);
}

/// A canvas that records text and counts ops.
#[derive(Default)]
struct Record {
    text: Vec<String>,
    ops: usize,
}

impl PageCanvas for Record {
    fn op(&mut self, _op: &DrawOp) {
        self.ops += 1;
    }
    fn text(&mut self, run: &TextRun<'_>) {
        let r = run.run.text_range.clone();
        self.text.push(run.text.get(r).unwrap_or("").to_owned());
    }
    fn begin(&mut self, _mark: Mark) {}
    fn end(&mut self) {}
    fn link(&mut self, _rect: Bounds, _target: &LinkTarget, _node: StructId) {}
}

#[test]
fn pages_print_titles_header_and_footer_as_text() {
    let mut wb = numbers(120, 3);
    define(&mut wb, "_xlnm.Print_Titles", "Sheet1!$1:$1");
    set_print(&mut wb, |p| {
        p.header_footer.odd_header = Some("&C&A".into());
        p.header_footer.odd_footer = Some("&RPage &P of &N".into());
    });
    let print = WorkbookPrint::new(
        &mut wb,
        kiln_text::FontStore::bundled(),
        PrintOptions {
            file: "book.xlsx".into(),
            ..PrintOptions::default()
        },
    );
    let pages = print.pages();
    assert_eq!(pages.len(), 3);
    let mut r = Record::default();
    print.paint(1, &mut r);
    assert!(r.text.iter().any(|t| t == "Sheet1"), "header: {:?}", r.text);
    assert!(
        r.text.iter().any(|t| t == "Page 2 of 3"),
        "footer: {:?}",
        r.text
    );
    // The title row (1 2 3) repeats above row 46 (46 92 138).
    let first = r.text.iter().position(|t| t == "1").expect("title row");
    let body = r.text.iter().position(|t| t == "46").expect("body");
    assert!(first < body);
    assert!(r.ops > 10);
}

#[test]
fn print_setup_paginates_as_libreoffice_with_its_title_row() {
    // fixtures/xlsx/features/print-setup.xlsx: landscape A4, one page wide, row 1 repeated, a
    // manual break after row 60. LibreOffice prints rows 1–31, 32–60, 61–90 and 91–120.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/xlsx/features/print-setup.xlsx"
    );
    let read = kiln_io_xlsx::read_xlsx(&std::fs::read(path).expect("fixture")).expect("read");
    let pages = paginate(&read.workbook, 0).expect("pages");
    let rows: Vec<(u32, u32)> = pages
        .pages
        .iter()
        .map(|p| (p.rows.0 + 1, p.rows.1 + 1))
        .collect();
    assert_eq!(rows, [(1, 31), (32, 60), (61, 90), (91, 120)]);
    assert!(pages.pages[0].title_rows.is_none());
    assert_eq!(pages.pages[2].title_rows, Some((0, 0)));
}

/// A canvas that keeps the fill colours of the paths it draws.
#[derive(Default)]
struct Fills(Vec<kiln_scene::Paint>);

impl PageCanvas for Fills {
    fn op(&mut self, op: &DrawOp) {
        if let DrawOp::Path { fill: Some(f), .. } = op {
            self.0.push(*f);
        }
    }
    fn text(&mut self, _run: &TextRun<'_>) {}
    fn begin(&mut self, _mark: Mark) {}
    fn end(&mut self) {}
    fn link(&mut self, _rect: Bounds, _target: &LinkTarget, _node: StructId) {}
}

#[test]
fn notes_and_comments_indicators_do_not_print() {
    let mut wb = numbers(10, 3);
    let id = wb.sheets()[0].id();
    wb.apply(&Command::SetSheetMeta {
        sheet: id,
        field: Box::new(MetaField::Notes(vec![kiln_sheet::meta::Note {
            at: kiln_sheet::CellPos::new(1, 1),
            author: "Ana".into(),
            text: "Check".into(),
            visible: false,
            size: None,
        }])),
    })
    .expect("note");
    let print = WorkbookPrint::new(
        &mut wb,
        kiln_text::FontStore::bundled(),
        PrintOptions::default(),
    );
    let mut f = Fills::default();
    print.paint(0, &mut f);
    let red = crate::look::rgb(0xD8_2E2E);
    assert!(!f.0.contains(&red), "{:?}", f.0);
}
