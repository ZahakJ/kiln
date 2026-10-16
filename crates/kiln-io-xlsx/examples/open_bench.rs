// SPDX-License-Identifier: AGPL-3.0-or-later
//! Times opening a workbook (budget B7, PLAN.md §3.9): reading the package into the model
//! (`parse_ms`) and the first full calculation (`calc_ms`), then saving it again (`write_ms`,
//! for information).
//!
//! ```text
//! cargo run --release -p kiln-io-xlsx --example open_bench fixtures/large/sheet-1m.xlsx [--runs 3]
//! ```
//!
//! Each run starts from the file's bytes in memory; the median of the runs is reported (B7:
//! `parse_ms` < 1200, `calc_ms` < 2500 on the 4-CPU reference machine).

use std::time::Instant;

use kiln_io_xlsx::{ReadOptions, WriteOptions, read_xlsx_with, write_xlsx};

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut path = None;
    let mut runs = 3usize;
    while let Some(a) = args.next() {
        if a == "--runs" {
            runs = args.next().and_then(|n| n.parse().ok()).unwrap_or(3).max(1);
        } else {
            path = Some(a);
        }
    }
    let Some(path) = path else {
        eprintln!("usage: open_bench <workbook.xlsx> [--runs N]");
        std::process::exit(2);
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{path}: {e}");
            std::process::exit(1);
        }
    };
    let (mut parse, mut calc, mut write) = (Vec::new(), Vec::new(), Vec::new());
    let mut summary = String::new();
    for _ in 0..runs {
        let t0 = Instant::now();
        let mut read = match read_xlsx_with(
            &bytes,
            &ReadOptions {
                defer_calculation: true,
                ..ReadOptions::default()
            },
        ) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{path}: {e}");
                std::process::exit(1);
            }
        };
        let t1 = Instant::now();
        if let Err(e) = read.calculate() {
            eprintln!("{path}: {e}");
            std::process::exit(1);
        }
        let t2 = Instant::now();
        let out = write_xlsx(&read.workbook, Some(&read.source), &WriteOptions::default());
        let t3 = Instant::now();
        parse.push((t1 - t0).as_secs_f64() * 1000.0);
        calc.push((t2 - t1).as_secs_f64() * 1000.0);
        write.push((t3 - t2).as_secs_f64() * 1000.0);
        let cells: usize = read.workbook.sheets().iter().map(|s| s.cells().len()).sum();
        let formulas: usize = read
            .workbook
            .sheets()
            .iter()
            .map(|s| {
                s.cells()
                    .iter()
                    .filter(|(_, _, d)| d.kind.is_formula())
                    .count()
            })
            .sum();
        summary = format!(
            "file: {path} ({} bytes)\nsheets: {}\ncells: {cells}\nformulas: {formulas}\nwarnings: {}\nwritten_bytes: {}",
            bytes.len(),
            read.workbook.sheets().len(),
            read.warnings.len(),
            out.map_or(0, |o| o.bytes.len()),
        );
    }
    println!("{summary}");
    println!("runs: {runs}");
    println!("parse_ms: {:.0}", median(parse));
    println!("calc_ms: {:.0}", median(calc));
    println!("write_ms: {:.0}", median(write));
}
