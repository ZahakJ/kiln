// SPDX-License-Identifier: AGPL-3.0-or-later
//! Whole-column AutoFill timing: one source cell filled down all 1,048,576 rows of a column
//! through the command log (validation, undo record, store write, recalculation), then undo,
//! redo and undo again, for the fill modes that generate values (Copy, Series, Growth) and for a
//! relative formula (one formula per row, all bound into the engine).
//!
//! No budget covers this; it is measured so a regression is visible (Excel fills a column in
//! about a second). Run with `cargo run --release -p kiln-sheet --example fill_bench`.

use std::time::Instant;

use kiln_sheet::commands::{CellInput, Command};
use kiln_sheet::fill::FillMode;
use kiln_sheet::{CellValue, CommandLog, MAX_ROWS, Range, Workbook};

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn run(label: &str, source: CellInput, mode: FillMode) {
    let mut wb = Workbook::new();
    let sheet = wb.sheets()[0].id();
    let mut log = CommandLog::new();
    log.exec(
        &mut wb,
        Command::SetCells {
            sheet,
            cells: vec![source],
        },
    )
    .expect("source");
    let t = Instant::now();
    log.exec(
        &mut wb,
        Command::AutoFill {
            sheet,
            source: Range::cell(0, 0),
            target: Range::new(0, 0, MAX_ROWS - 1, 0),
            mode,
        },
    )
    .expect("fill");
    let fill = ms(t);
    let last = wb.value(sheet, MAX_ROWS - 1, 0);
    let t = Instant::now();
    log.undo(&mut wb).expect("undo");
    let undo = ms(t);
    let t = Instant::now();
    log.redo(&mut wb).expect("redo");
    let redo = ms(t);
    let t = Instant::now();
    log.undo(&mut wb).expect("undo");
    let undo2 = ms(t);
    assert_eq!(
        wb.value(sheet, 1, 0),
        CellValue::Blank,
        "undo cleared the fill"
    );
    println!(
        "{label}: fill_ms={fill:.1} undo_ms={undo:.1} redo_ms={redo:.1} undo2_ms={undo2:.1} last={last:?}"
    );
}

fn main() {
    run("copy", CellInput::value(0, 0, 7.0), FillMode::Copy);
    run("series", CellInput::value(0, 0, 1.0), FillMode::Series);
    run("growth", CellInput::value(0, 0, 1.0), FillMode::Growth);
    run(
        "formula",
        CellInput::formula(0, 0, "=ROW()*2"),
        FillMode::Auto,
    );
}
