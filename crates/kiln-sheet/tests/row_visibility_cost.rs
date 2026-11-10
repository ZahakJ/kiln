// SPDX-License-Identifier: AGPL-3.0-or-later
//! What a row change costs once many rows are hidden (rows 2:1048576 hidden below the data is a
//! common sheet). A change to one row's height compares only that row's visibility, never every
//! hidden row of the sheet: 1,000 one-row changes cost about the same over n and 8n hidden rows
//! (never 8 times as much), and a SUBTOTAL over the rows still sees a row hidden or shown.

use std::time::{Duration, Instant};

use kiln_sheet::commands::{CellInput, RowPatch};
use kiln_sheet::num::F64;
use kiln_sheet::styles::Patch;
use kiln_sheet::{CellValue, Command, MAX_ROWS, Workbook};

fn rows(wb: &mut Workbook, at: u32, count: u32, patch: RowPatch) -> Command {
    let sheet = wb.first_sheet().expect("a sheet");
    wb.apply(&Command::SetRowProps {
        sheet,
        at,
        count,
        patch,
    })
    .expect("row props")
}

fn hide(n: u32) -> Workbook {
    let mut wb = Workbook::new();
    rows(
        &mut wb,
        1_000,
        n,
        RowPatch {
            hidden: Some(true),
            ..RowPatch::default()
        },
    );
    wb
}

/// 1,000 one-row height changes (AutoFit's), timed.
fn heights(wb: &mut Workbook) -> Duration {
    let t = Instant::now();
    for i in 0..1_000 {
        rows(
            wb,
            10 + i,
            1,
            RowPatch {
                height: Patch::Set(F64(24.0)),
                custom_height: Some(false),
                ..RowPatch::default()
            },
        );
    }
    t.elapsed()
}

#[test]
fn one_row_changes_cost_the_same_over_n_and_8n_hidden_rows() {
    let (n, big) = (16_000, 128_000);
    let mut small = hide(n);
    let t_small = heights(&mut small);
    assert!(
        t_small < Duration::from_secs(2),
        "1,000 row heights over {n} hidden rows: {t_small:?}"
    );
    let mut large = hide(big);
    let t_large = heights(&mut large);
    assert!(
        t_large <= t_small * 2 + Duration::from_millis(50),
        "1,000 row heights: {t_small:?} over {n} hidden rows, {t_large:?} over {big}"
    );
}

#[test]
fn rows_2_to_the_end_hidden_then_a_row_changed_at_once() {
    let mut wb = Workbook::new();
    let sheet = wb.first_sheet().expect("a sheet");
    wb.apply(&Command::SetCells {
        sheet,
        cells: vec![
            CellInput::value(0, 0, CellValue::Number(5.0)),
            CellInput::value(1, 0, CellValue::Number(7.0)),
            CellInput::formula(0, 1, "SUBTOTAL(109,A1:A2)"),
        ],
    })
    .expect("cells");
    let t = Instant::now();
    rows(
        &mut wb,
        1,
        MAX_ROWS - 1,
        RowPatch {
            hidden: Some(true),
            ..RowPatch::default()
        },
    );
    let hidden = t.elapsed();
    let t = Instant::now();
    let d = heights(&mut wb);
    assert!(
        d < Duration::from_secs(2),
        "1,000 row heights after a mass hide: {d:?}"
    );
    assert!(hidden < Duration::from_secs(10), "hiding: {hidden:?}");
    wb.recalc();
    assert_eq!(
        wb.value(sheet, 0, 1),
        CellValue::Number(5.0),
        "SUBTOTAL skips the hidden row"
    );
    rows(
        &mut wb,
        1,
        1,
        RowPatch {
            hidden: Some(false),
            ..RowPatch::default()
        },
    );
    wb.recalc();
    assert_eq!(
        wb.value(sheet, 0, 1),
        CellValue::Number(12.0),
        "and counts it again"
    );
    assert!(t.elapsed() < Duration::from_secs(4), "{:?}", t.elapsed());
}
