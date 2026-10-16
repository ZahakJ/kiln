// SPDX-License-Identifier: AGPL-3.0-or-later
//! Workbook-level recalculation benchmark for the performance budgets B8 and B9 (PLAN.md §3.9),
//! measured through the sheet model (store, command application, formula engine, write-back of
//! cached results), not the bare engine.
//!
//! - **B8** (`full_recalc_ms`): 1,000,008 formula cells — arithmetic, IF/AND/OR, row SUMs, a
//!   sliding 10-row SUM window and an INDEX lookup into a 1,000-row table — fully recalculated
//!   on a 4-thread pool, including writing every result back to the store. Median of 3 runs.
//! - **B9** (`incremental_ms_p95`): the same shape with 100,008 formulas; 200 single-cell edits
//!   applied as `SetCells` commands (validation, undo record, store write, engine notification,
//!   automatic recalculation, write-back); the 95th percentile.
//!
//! Run with `cargo run --release -p kiln-sheet --example edit_bench`. Exits with status 1 when a
//! budget is missed.

use std::sync::Arc;
use std::time::Instant;

use kiln_sheet::commands::{CellInput, Command};
use kiln_sheet::{CellValue, SheetId, Workbook, WorkbookOptions};

const B8_MS: f64 = 2000.0;
const B9_MS: f64 = 2.0;

/// Formula columns B..J for row `r` (1-based).
fn row_formulas(r: u32) -> [String; 9] {
    let window = if r >= 10 {
        format!("SUM(G{}:G{r})", r - 9)
    } else {
        format!("SUM(G1:G{r})")
    };
    [
        format!("A{r}*2+1"),
        format!("IF(B{r}>1000,B{r}-1000,B{r}+0.5)"),
        format!("SUM(A{r}:C{r})"),
        format!("INDEX(Data!$B$1:$B$1000,A{r}+1)"),
        format!("E{r}*C{r}-D{r}"),
        format!("IF(AND(F{r}>0,E{r}<500),F{r},-F{r})"),
        window,
        format!("H{r}/2+D{r}"),
        format!("IF(OR(I{r}<0,G{r}>100),1,0)*I{r}"),
    ]
}

fn build(rows: u32) -> (Workbook, SheetId) {
    let mut wb = Workbook::with_options(WorkbookOptions {
        sheets: Some(vec!["Model".into(), "Data".into()]),
        clock: Some(Arc::new(|| 45306.5)),
        ..WorkbookOptions::default()
    });
    let (model, data) = (wb.sheets()[0].id(), wb.sheets()[1].id());
    let mut props = wb.props().clone();
    // Build without recalculating after every cell, as a file reader does.
    props.calc.mode = kiln_sheet::meta::CalcMode::Manual;
    wb.apply(&Command::SetProps {
        props: Box::new(props),
    })
    .expect("props");
    for i in 0..1000u32 {
        wb.load_value(data, i, 0, CellValue::Number(f64::from(i)))
            .expect("data");
        wb.load_value(
            data,
            i,
            1,
            CellValue::Number(f64::from((i * 7919) % 1000) / 4.0),
        )
        .expect("data");
    }
    for r in 1..=rows {
        wb.load_value(
            model,
            r - 1,
            0,
            CellValue::Number(f64::from((r * 37) % 1000)),
        )
        .expect("input");
        for (i, f) in row_formulas(r).iter().enumerate() {
            wb.load_formula(model, r - 1, 1 + i as u32, f, CellValue::Blank, false)
                .expect("formula");
        }
    }
    let mut props = wb.props().clone();
    props.calc.mode = kiln_sheet::meta::CalcMode::Auto;
    wb.apply(&Command::SetProps {
        props: Box::new(props),
    })
    .expect("props");
    (wb, model)
}

fn rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok())
        })
        .map_or(0.0, |kb| kb / 1024.0)
}

fn percentile(sorted: &[f64], p: usize) -> f64 {
    sorted[(sorted.len() * p).div_ceil(100) - 1]
}

fn main() {
    let threads = std::env::var("THREADS")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(4);
    // The engine evaluates topological levels on the current rayon pool.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .stack_size(8 << 20)
        .build()
        .expect("thread pool");

    // B8: full recalculation of 1M formula cells.
    let t = Instant::now();
    let (mut big, model) = pool.install(|| build(111_112));
    let build_ms = t.elapsed().as_secs_f64() * 1000.0;
    let formulas = big.engine().formula_count();
    let mut runs: Vec<f64> = (0..3)
        .map(|_| {
            pool.install(|| {
                let t = Instant::now();
                let r = big.recalc_all();
                assert_eq!(r.evaluated, formulas);
                t.elapsed().as_secs_f64() * 1000.0
            })
        })
        .collect();
    runs.sort_by(f64::total_cmp);
    let full_ms = runs[1];
    let check = big.value(model, 50_000, 9);
    assert!(
        matches!(check, CellValue::Number(_)),
        "unexpected result {check:?}"
    );
    let cached = big.sheet(model).map(|s| s.cells().value(50_000, 9).clone());
    assert_eq!(cached, Some(check), "results are written back to the store");
    let rss_big = rss_mb();
    drop(big);

    // B9: single-cell edits as commands in a 100k-formula workbook.
    let (mut small, model) = pool.install(|| build(11_112));
    let mut samples = Vec::with_capacity(200);
    let mut evaluated = 0usize;
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    for _ in 0..200 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let row = (seed % 11_112) as u32;
        let value = (seed % 997) as f64;
        let cmd = Command::SetCells {
            sheet: model,
            cells: vec![CellInput::value(row, 0, value)],
        };
        let ms = pool.install(|| {
            let t = Instant::now();
            small.apply(&cmd).expect("edit");
            t.elapsed().as_secs_f64() * 1000.0
        });
        evaluated += small.last_evaluated();
        samples.push(ms);
    }
    samples.sort_by(f64::total_cmp);
    let p50 = percentile(&samples, 50);
    let p95 = percentile(&samples, 95);

    println!("formulas={formulas} threads={threads}");
    println!("build_ms={build_ms:.0} rss_mb={rss_big:.0}");
    println!("full_recalc_ms={full_ms:.1}");
    println!(
        "incremental_ms_p50={p50:.3} incremental_ms_p95={p95:.3} evaluated_per_edit={:.1}",
        evaluated as f64 / 200.0
    );
    let ok = full_ms < B8_MS && p95 < B9_MS;
    println!(
        "budgets: B8 full_recalc_ms < {B8_MS} {} · B9 incremental_ms_p95 < {B9_MS} {}",
        if full_ms < B8_MS { "ok" } else { "MISSED" },
        if p95 < B9_MS { "ok" } else { "MISSED" }
    );
    if !ok {
        std::process::exit(1);
    }
}
