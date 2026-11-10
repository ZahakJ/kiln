// SPDX-License-Identifier: AGPL-3.0-or-later
//! What formatting whole rows, whole columns and the whole sheet costs. A row format wins over a
//! column format where the two meet, so a careless format writes a cell for every place a
//! formatted row meets a formatted column: rows × 16,384 cells for the whole sheet over formatted
//! rows. Every format command here runs over n and 8n formatted rows (and columns): the cells it
//! stores and the time it takes grow at most about 8×, and stay a small multiple of the formatted
//! rows and columns. Each cell shows what Excel shows, also a cell typed afterwards, and Undo
//! restores the sheet exactly.

mod support;

use std::time::{Duration, Instant};

use kiln_formula::SheetId;
use kiln_sheet::commands::{CellInput, ClearWhat, MergeMode};
use kiln_sheet::styles::{
    AlignmentPatch, BorderPatch, BorderSide, BorderStyle, Fill, FontPatch, HAlign, Patch,
    StylePatch,
};
use kiln_sheet::{CellValue, Color, Command, MAX_COLS, MAX_ROWS, Range, Workbook};
use support::state;

const LAST_ROW: u32 = MAX_ROWS - 1;
const LAST_COL: u32 = MAX_COLS - 1;

fn sheet(wb: &Workbook) -> SheetId {
    wb.first_sheet().expect("a sheet")
}

fn records(wb: &Workbook) -> usize {
    wb.sheet(sheet(wb)).expect("sheet").cells().len()
}

fn style(wb: &mut Workbook, r: Range, patch: StylePatch) -> Command {
    let s = sheet(wb);
    wb.apply(&Command::SetStyle {
        sheet: s,
        ranges: vec![r],
        patch: Box::new(patch),
    })
    .expect("format")
}

fn num(code: &str) -> StylePatch {
    StylePatch {
        num_fmt: Some(code.into()),
        ..StylePatch::default()
    }
}

fn bold() -> StylePatch {
    StylePatch {
        font: Some(FontPatch {
            bold: Some(true),
            ..FontPatch::default()
        }),
        ..StylePatch::default()
    }
}

fn rows(r0: u32, r1: u32) -> Range {
    Range::new(r0, 0, r1, LAST_COL)
}

fn cols(c0: u32, c1: u32) -> Range {
    Range::new(0, c0, LAST_ROW, c1)
}

fn all() -> Range {
    Range::new(0, 0, LAST_ROW, LAST_COL)
}

/// A workbook with `n` formatted rows (from row 10, two formats taking turns) and a few values.
fn styled_rows(n: u32) -> Workbook {
    let mut wb = Workbook::new();
    for i in 0..n {
        let code = if i % 2 == 0 { "0.00" } else { "0.000" };
        style(&mut wb, rows(10 + i, 10 + i), num(code));
    }
    values(&mut wb);
    wb
}

/// A workbook with `n` formatted columns (from column K, two formats taking turns).
fn styled_cols(n: u32) -> Workbook {
    let mut wb = Workbook::new();
    for i in 0..n {
        let code = if i % 2 == 0 { "0.00" } else { "0.000" };
        style(&mut wb, cols(10 + i, 10 + i), num(code));
    }
    values(&mut wb);
    wb
}

fn values(wb: &mut Workbook) {
    let s = sheet(wb);
    wb.apply(&Command::SetCells {
        sheet: s,
        cells: vec![
            CellInput::value(0, 0, CellValue::Number(1.0)),
            CellInput::value(11, 11, CellValue::Number(2.0)),
            CellInput::value(3, 12, CellValue::Number(3.0)),
        ],
    })
    .expect("values");
}

fn thin() -> Patch<BorderSide> {
    Patch::Set(BorderSide {
        style: BorderStyle::Thin,
        color: None,
    })
}

/// Every format command, over a range.
fn commands(s: SheetId, r: Range) -> Vec<(&'static str, Command)> {
    let set = |patch: StylePatch| Command::SetStyle {
        sheet: s,
        ranges: vec![r],
        patch: Box::new(patch),
    };
    vec![
        ("number format", set(num("0.0%"))),
        ("bold", set(bold())),
        (
            "fill",
            set(StylePatch {
                fill: Patch::Set(Fill::solid(Color::hex("FFEE00").expect("colour"))),
                ..StylePatch::default()
            }),
        ),
        (
            "borders",
            set(StylePatch {
                border: Some(BorderPatch {
                    top: thin(),
                    bottom: thin(),
                    left: thin(),
                    right: thin(),
                    ..BorderPatch::default()
                }),
                ..StylePatch::default()
            }),
        ),
        (
            "alignment",
            set(StylePatch {
                alignment: Some(AlignmentPatch {
                    horizontal: Some(HAlign::Right),
                    ..AlignmentPatch::default()
                }),
                ..StylePatch::default()
            }),
        ),
        (
            "style index",
            Command::SetStyleIndex {
                sheet: s,
                ranges: vec![r],
                style: 0,
            },
        ),
        (
            "merge & center",
            Command::Merge {
                sheet: s,
                range: r,
                mode: MergeMode::Center,
            },
        ),
        (
            "clear formats",
            Command::ClearRange {
                sheet: s,
                ranges: vec![r],
                what: ClearWhat::Formats,
            },
        ),
    ]
}

/// The ranges a person selects: a few whole rows, a few whole columns, most rows, most columns
/// and the whole sheet.
fn targets() -> Vec<(&'static str, Range)> {
    vec![
        ("rows 4:6", rows(3, 5)),
        ("columns C:D", cols(2, 3)),
        ("the whole sheet", all()),
        ("columns B:XFD", cols(1, LAST_COL)),
        ("rows 12:13", rows(11, 12)),
        ("columns L:M", cols(11, 12)),
        ("rows 2:1048576", rows(1, LAST_ROW)),
        ("A2:A1048576", Range::new(1, 0, LAST_ROW, 0)),
        ("C3:D1048576", Range::new(2, 2, LAST_ROW, 3)),
    ]
}

/// The cells a format may store over `lines` formatted rows or columns: a few for each.
fn bound(lines: u32) -> usize {
    500 + 8 * lines as usize
}

fn run(build: fn(u32) -> Workbook, what: &str) {
    let (n, big) = (16, 128);
    let names: Vec<(&str, Range)> = targets();
    for (target, r) in names {
        let count = commands(SheetId(0), r).len();
        for k in 0..count {
            let measure = |lines: u32| {
                let mut wb = build(lines);
                let s = sheet(&wb);
                let (name, cmd) = commands(s, r).swap_remove(k);
                let before = state(&wb);
                let t = Instant::now();
                let inverse = wb.apply(&cmd);
                let took = t.elapsed();
                let stored = records(&wb);
                // Bounded first, so the over-large case stops here quickly.
                assert!(
                    stored <= bound(lines),
                    "{name} over {target}, {lines} formatted {what}: {stored} cells stored (at most {})",
                    bound(lines)
                );
                if let Ok(inv) = inverse {
                    wb.apply(&inv).expect("undo");
                    assert_eq!(
                        state(&wb),
                        before,
                        "{name} over {target}: undo restores the sheet"
                    );
                }
                (name, stored, took)
            };
            let (name, small, t_small) = measure(n);
            let (_, large, t_large) = measure(big);
            assert!(
                large <= 8 * small + 500,
                "{name} over {target}: {small} cells over {n} formatted {what}, {large} over {big}"
            );
            assert!(
                t_large <= t_small * 8 + Duration::from_secs(2),
                "{name} over {target}: {t_small:?} over {n} formatted {what}, {t_large:?} over {big}"
            );
        }
    }
}

#[test]
fn formats_over_formatted_rows_grow_with_the_rows() {
    run(styled_rows, "rows");
}

#[test]
fn formats_over_formatted_columns_grow_with_the_columns() {
    run(styled_cols, "columns");
}

/// The number format and boldness a cell shows.
fn shown(wb: &Workbook, row: u32, col: u32) -> (String, bool) {
    let sh = wb.sheet(sheet(wb)).expect("sheet");
    let st = wb.styles().get(sh.style_at(row, col));
    (st.num_fmt.to_string(), st.font.bold)
}

fn typed(wb: &mut Workbook, row: u32, col: u32) {
    let s = sheet(wb);
    wb.apply(&Command::SetCells {
        sheet: s,
        cells: vec![CellInput::value(row, col, CellValue::Number(5.0))],
    })
    .expect("typed");
}

/// Builds a workbook with `setup`, applies `change`, checks it is bounded and that Undo restores
/// the sheet exactly; returns the changed workbook (built again) for the checks of what it shows.
fn checked(
    setup: &dyn Fn(&mut Workbook),
    change: &dyn Fn(&mut Workbook) -> Command,
    most: usize,
) -> Workbook {
    let mut wb = Workbook::new();
    setup(&mut wb);
    let before = state(&wb);
    let t = Instant::now();
    let inv = change(&mut wb);
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    assert!(
        records(&wb) <= most,
        "{} cells stored (at most {most})",
        records(&wb)
    );
    wb.apply(&inv).expect("undo");
    assert_eq!(state(&wb), before, "undo restores the sheet");
    let mut wb = Workbook::new();
    setup(&mut wb);
    change(&mut wb);
    wb
}

/// 2,000 bold rows, then the whole sheet: one number format.
#[test]
fn the_whole_sheet_over_2000_formatted_rows() {
    let mut wb = checked(
        &|wb| {
            style(wb, rows(100, 2099), bold());
        },
        &|wb| style(wb, all(), num("0.00")),
        0,
    );
    assert_eq!(
        shown(&wb, 500, 7000),
        ("0.00".into(), true),
        "a formatted row"
    );
    assert_eq!(
        shown(&wb, 5000, 7000),
        ("0.00".into(), false),
        "a row without a format"
    );
    typed(&mut wb, 600, 16000);
    typed(&mut wb, 9000, 3);
    assert_eq!(
        shown(&wb, 600, 16000),
        ("0.00".into(), true),
        "typed in a formatted row"
    );
    assert_eq!(
        shown(&wb, 9000, 3),
        ("0.00".into(), false),
        "typed in another row"
    );
}

/// The whole sheet, then 2,000 whole rows.
#[test]
fn two_thousand_rows_after_the_whole_sheet() {
    let mut wb = checked(
        &|wb| {
            style(wb, all(), num("0.00"));
        },
        &|wb| style(wb, rows(100, 2099), bold()),
        0,
    );
    assert_eq!(shown(&wb, 500, 7000), ("0.00".into(), true));
    assert_eq!(shown(&wb, 5000, 7000), ("0.00".into(), false));
    typed(&mut wb, 600, 16000);
    assert_eq!(shown(&wb, 600, 16000), ("0.00".into(), true));
}

/// 2,000 bold columns, then the whole sheet.
#[test]
fn the_whole_sheet_over_2000_formatted_columns() {
    let mut wb = checked(
        &|wb| {
            style(wb, cols(100, 2099), bold());
        },
        &|wb| style(wb, all(), num("0.00")),
        0,
    );
    assert_eq!(shown(&wb, 500_000, 1000), ("0.00".into(), true));
    assert_eq!(shown(&wb, 500_000, 5000), ("0.00".into(), false));
    typed(&mut wb, 7, 1000);
    assert_eq!(shown(&wb, 7, 1000), ("0.00".into(), true));
}

/// The whole sheet, then 2,000 whole columns (two formatted rows cross them).
#[test]
fn two_thousand_columns_after_the_whole_sheet() {
    let mut wb = checked(
        &|wb| {
            style(wb, all(), num("0.00"));
            style(wb, rows(5, 6), num("0.000"));
        },
        &|wb| style(wb, cols(100, 2099), bold()),
        // Two formatted rows meet 2,000 columns: those 4,000 places are cells of their own.
        4_000,
    );
    assert_eq!(
        shown(&wb, 5, 1000),
        ("0.000".into(), true),
        "a formatted row"
    );
    assert_eq!(shown(&wb, 900, 1000), ("0.00".into(), true));
    assert_eq!(shown(&wb, 900, 5000), ("0.00".into(), false));
    typed(&mut wb, 900, 1000);
    assert_eq!(shown(&wb, 900, 1000), ("0.00".into(), true));
}

/// Merge & Center of the whole sheet over 2,000 formatted rows.
#[test]
fn merge_and_center_of_the_whole_sheet_over_formatted_rows() {
    let mut wb = Workbook::new();
    style(&mut wb, rows(100, 2099), bold());
    values(&mut wb);
    let before = state(&wb);
    let s = sheet(&wb);
    let t = Instant::now();
    let inv = wb
        .apply(&Command::Merge {
            sheet: s,
            range: all(),
            mode: MergeMode::Center,
        })
        .expect("merge");
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    assert!(records(&wb) <= 10, "{} cells stored", records(&wb));
    let sh = wb.sheet(s).expect("sheet");
    for (row, col) in [(500, 7000), (5000, 7000)] {
        let st = wb.styles().get(sh.style_at(row, col));
        assert_eq!(st.alignment.horizontal, HAlign::Center, "{row},{col}");
    }
    assert!(wb.styles().get(sh.style_at(500, 7000)).font.bold);
    wb.apply(&inv).expect("undo");
    assert_eq!(state(&wb), before);
}

/// Columns whose formats differ: a row formatted afterwards keeps each column's format.
#[test]
fn a_row_over_columns_with_different_formats() {
    let mut wb = Workbook::new();
    style(&mut wb, cols(0, 0), num("0.00"));
    style(&mut wb, cols(1, 1), num("0.000"));
    let before = state(&wb);
    let inv = style(&mut wb, rows(4, 4), bold());
    assert_eq!(shown(&wb, 4, 0), ("0.00".into(), true));
    assert_eq!(shown(&wb, 4, 1), ("0.000".into(), true));
    assert_eq!(shown(&wb, 4, 9), ("General".into(), true));
    assert_eq!(shown(&wb, 5, 0), ("0.00".into(), false));
    assert!(records(&wb) <= 2, "{} cells stored", records(&wb));
    wb.apply(&inv).expect("undo");
    assert_eq!(state(&wb), before);
}

/// Most columns over formatted rows: the columns outside keep what they showed.
#[test]
fn most_columns_over_a_formatted_row() {
    let mut wb = Workbook::new();
    style(&mut wb, rows(4, 4), bold());
    let before = state(&wb);
    let inv = style(&mut wb, cols(1, LAST_COL), num("0.00"));
    assert_eq!(shown(&wb, 4, 0), ("General".into(), true));
    assert_eq!(shown(&wb, 4, 5), ("0.00".into(), true));
    assert_eq!(shown(&wb, 9, 0), ("General".into(), false));
    assert_eq!(shown(&wb, 9, 5), ("0.00".into(), false));
    assert!(records(&wb) <= 2, "{} cells stored", records(&wb));
    wb.apply(&inv).expect("undo");
    assert_eq!(state(&wb), before);
}

/// Most rows over formatted columns: the rows outside keep what they showed.
#[test]
fn most_rows_over_formatted_columns() {
    let mut wb = Workbook::new();
    style(&mut wb, cols(2, 2), bold());
    let before = state(&wb);
    let inv = style(&mut wb, rows(1, LAST_ROW), num("0.00"));
    assert_eq!(shown(&wb, 0, 2), ("General".into(), true));
    assert_eq!(shown(&wb, 0, 5), ("General".into(), false));
    assert_eq!(shown(&wb, 9, 2), ("0.00".into(), true));
    assert_eq!(shown(&wb, 9, 5), ("0.00".into(), false));
    assert!(records(&wb) <= 2, "{} cells stored", records(&wb));
    wb.apply(&inv).expect("undo");
    assert_eq!(state(&wb), before);
}

/// Most of a column (an outline's side over rows 2:1048576): the column changes, and the cell
/// above the range keeps what it showed.
#[test]
fn most_of_a_column() {
    let mut wb = checked(
        &|wb| {
            style(wb, rows(5, 5), bold());
        },
        &|wb| style(wb, Range::new(1, 0, LAST_ROW, 0), num("0.00")),
        2,
    );
    assert_eq!(
        shown(&wb, 0, 0),
        ("General".into(), false),
        "above the range"
    );
    assert_eq!(shown(&wb, 900_000, 0), ("0.00".into(), false));
    assert_eq!(shown(&wb, 5, 0), ("0.00".into(), true), "a formatted row");
    assert_eq!(
        shown(&wb, 5, 1),
        ("General".into(), true),
        "beside the range"
    );
    typed(&mut wb, 7, 0);
    assert_eq!(shown(&wb, 7, 0), ("0.00".into(), false));
}

/// Rows 2:1048576 bold, then columns B:XFD formatted (a number format, then a fill): A1 keeps
/// the default style, also for a value typed there afterwards. Row 1 holds the default as its own
/// format (a blank cell cannot), so the change goes to the range's cells in that row.
#[test]
fn most_columns_over_a_row_that_holds_the_default() {
    let fill = || StylePatch {
        fill: Patch::Set(Fill::solid(Color::hex("FFEE00").expect("colour"))),
        ..StylePatch::default()
    };
    for (name, patch) in [("number format", num("0.00")), ("fill", fill())] {
        let mut wb = checked(
            &|wb| {
                style(wb, rows(1, LAST_ROW), bold());
            },
            &|wb| style(wb, cols(1, LAST_COL), patch.clone()),
            MAX_COLS as usize + 10,
        );
        let sh = wb.sheet(sheet(&wb)).expect("sheet");
        let a1 = wb.styles().get(sh.style_at(0, 0));
        let b1 = wb.styles().get(sh.style_at(0, 1));
        let b5 = wb.styles().get(sh.style_at(4, 1));
        assert_eq!(a1, wb.styles().get(0), "{name}: A1 keeps the default");
        assert_ne!(b1, wb.styles().get(0), "{name}: B1 takes the format");
        assert!(b5.font.bold, "{name}: B5 stays bold");
        assert!(
            wb.styles().get(sh.style_at(4, 0)).font.bold,
            "{name}: A5 stays bold"
        );
        assert_eq!(
            wb.styles().get(sh.style_at(4, 0)).num_fmt.as_ref(),
            "General",
            "{name}: A5"
        );
        typed(&mut wb, 0, 0);
        let sh = wb.sheet(sheet(&wb)).expect("sheet");
        assert_eq!(
            wb.styles().get(sh.style_at(0, 0)),
            wb.styles().get(0),
            "{name}: typed in A1"
        );
    }
}

/// n and 8n rows that hold the default as their own format, made two ways (the default set on
/// whole rows over formatted columns, and the rows left outside a format of most rows), then a
/// format of most columns (B:XFD), a number format and a fill: a cell for each such row in the
/// columns outside the range at most, never one for each of the range's 16,383 columns.
#[test]
fn rows_that_hold_the_default_meet_most_columns_with_a_few_cells() {
    let fill = || StylePatch {
        fill: Patch::Set(Fill::solid(Color::hex("FFEE00").expect("colour"))),
        ..StylePatch::default()
    };
    let by_default = |n: u32| {
        let mut wb = Workbook::new();
        style(&mut wb, all(), num("0.0"));
        let s = sheet(&wb);
        wb.apply(&Command::SetStyleIndex {
            sheet: s,
            ranges: vec![rows(0, n - 1)],
            style: 0,
        })
        .expect("default");
        wb
    };
    let outside = |n: u32| {
        let mut wb = Workbook::new();
        style(&mut wb, rows(n, LAST_ROW), bold());
        wb
    };
    for (how, build) in [
        (
            "the default on whole rows",
            &by_default as &dyn Fn(u32) -> Workbook,
        ),
        ("rows outside a format of most rows", &outside),
    ] {
        for (name, patch) in [("number format", num("0.00")), ("fill", fill())] {
            let mut counts = Vec::new();
            for n in [16, 128] {
                let mut wb = build(n);
                let before = state(&wb);
                let base = records(&wb);
                let inv = style(&mut wb, cols(1, LAST_COL), patch.clone());
                let added = records(&wb) - base;
                assert!(
                    added <= 500 + 8 * n as usize,
                    "{name} of B:XFD over {n} rows ({how}): {added} cells added"
                );
                let sh = wb.sheet(sheet(&wb)).expect("sheet");
                let a1 = wb.styles().get(sh.style_at(0, 0));
                let b1 = wb.styles().get(sh.style_at(0, 1));
                assert_eq!(a1.num_fmt.as_ref(), "General", "{how}, {name}: A1");
                assert_eq!(a1.fill, wb.styles().get(0).fill, "{how}, {name}: A1's fill");
                assert_ne!(b1, a1, "{how}, {name}: B1 takes the format");
                typed(&mut wb, 0, 0);
                let sh = wb.sheet(sheet(&wb)).expect("sheet");
                assert_eq!(
                    wb.styles().get(sh.style_at(0, 0)).num_fmt.as_ref(),
                    "General",
                    "{how}, {name}: typed in A1"
                );
                let mut wb = build(n);
                let inv2 = style(&mut wb, cols(1, LAST_COL), patch.clone());
                wb.apply(&inv2).expect("undo");
                assert_eq!(state(&wb), before, "{how}, {name}: undo");
                let _ = inv;
                counts.push(added);
            }
            assert!(
                counts[1] <= 8 * counts[0] + 500,
                "{how}, {name}: {counts:?}"
            );
        }
    }
}
