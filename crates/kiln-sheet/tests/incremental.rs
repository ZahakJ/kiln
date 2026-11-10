// SPDX-License-Identifier: AGPL-3.0-or-later
//! Incremental recalculation through the workbook: in the 100,008-formula generated workbook (the
//! shape `examples/edit_bench.rs` times for budget B9), editing one input recalculates exactly
//! the formulas that depend on it.

#[path = "support/b9.rs"]
mod b9;

use kiln_sheet::CellValue;
use kiln_sheet::commands::{CellInput, Command};

#[test]
fn a_single_edit_recalculates_only_its_dependents() {
    const ROWS: u32 = 11_112;
    let (mut wb, model) = b9::build(ROWS);
    assert_eq!(wb.engine().formula_count(), 100_008);
    let full = wb.recalc_all();
    assert_eq!(full.evaluated, 100_008);

    // Editing A5000 dirties B..G of its row (6), and H, I, J of the ten rows whose 10-row
    // window covers G5000 (30): 36 evaluations, not 100,008.
    wb.apply(&Command::SetCells {
        sheet: model,
        cells: vec![CellInput::value(4999, 0, 123.0)],
    })
    .expect("edit");
    assert_eq!(wb.last_evaluated(), 36);
    // B5000 = A5000*2+1, and J5004 (whose window G4995:G5004 covers G5000) is current: it
    // equals its own formula evaluated against the row's present values.
    assert_eq!(wb.value(model, 4999, 1), CellValue::Number(247.0));
    let num = |c: u32| match wb.value(model, 5003, c) {
        CellValue::Number(x) => x,
        other => panic!("expected a number in column {c}, got {other:?}"),
    };
    let (g, i) = (num(6), num(8));
    let expected_j = if i < 0.0 || g > 100.0 { i } else { 0.0 };
    assert_eq!(num(9), expected_j);
    let window: f64 = (4994..=5003)
        .map(|r| match wb.value(model, r, 6) {
            CellValue::Number(x) => x,
            other => panic!("G{} is {other:?}", r + 1),
        })
        .sum();
    assert!((num(7) - window).abs() <= 1e-9 * window.abs().max(1.0));
    // An edit nothing depends on evaluates nothing.
    wb.apply(&Command::SetCells {
        sheet: model,
        cells: vec![CellInput::value(4999, 20, 1.0)],
    })
    .expect("edit");
    assert_eq!(wb.last_evaluated(), 0);
    // The first row's window is shorter: H1…H10 depend on G1.
    wb.apply(&Command::SetCells {
        sheet: model,
        cells: vec![CellInput::value(0, 0, 5.0)],
    })
    .expect("edit");
    assert_eq!(wb.last_evaluated(), 36);
    // Results equal a full recalculation.
    let snapshot: Vec<CellValue> = (0..ROWS)
        .step_by(997)
        .map(|r| wb.value(model, r, 9))
        .collect();
    wb.recalc_all();
    let again: Vec<CellValue> = (0..ROWS)
        .step_by(997)
        .map(|r| wb.value(model, r, 9))
        .collect();
    assert_eq!(snapshot, again);
}
