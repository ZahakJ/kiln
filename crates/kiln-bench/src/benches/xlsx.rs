// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Grid file benchmarks (WP-70), on `fixtures/large/sheet-1m.xlsx` (1,000,000 populated
//! cells, 200,000 of them formulas; generated on demand by `tools/make-fixtures.py
//! --synthetic-large`, which the benchmark runs when the file is missing):
//!
//! - `xlsx-open` (B7): read the workbook into the model — values, formats and metadata, ready
//!   to paint with the stored results — the time to the first paint. The extra figures are
//!   `calc_ms` (binding the formulas and the first full calculation) and `full_ms` (both: open
//!   until fully calculated), each the p95 of the runs.
//!
//! In quick mode it uses `fixtures/xlsx/features/formulas.xlsx` instead.

use std::path::{Path, PathBuf};
use std::time::Instant;

use kiln_io_xlsx::{ReadOptions, read_xlsx_with};

use crate::harness::{Config, Sample, percentile, summarize};
use crate::scenarios::RunOptions;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The input workbook's bytes (generating the large fixture when it is missing).
fn input(options: &RunOptions) -> Result<Vec<u8>, String> {
    if options.quick {
        let p = repo().join("fixtures/xlsx/features/formulas.xlsx");
        return std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()));
    }
    let p = repo().join("fixtures/large/sheet-1m.xlsx");
    if !p.exists() {
        let status = std::process::Command::new("python3")
            .args([
                "-I",
                "tools/make-fixtures.py",
                "--synthetic-large",
                "--only",
                "xlsx",
            ])
            .current_dir(repo())
            .status()
            .map_err(|e| format!("python3 tools/make-fixtures.py: {e}"))?;
        if !status.success() {
            return Err(format!("tools/make-fixtures.py failed ({status})"));
        }
    }
    std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))
}

/// `xlsx-open`.
pub fn open(options: &RunOptions) -> Result<Sample, String> {
    let bytes = input(options)?;
    let config = if options.quick {
        Config::new(0, 2)
    } else {
        Config::new(1, 6)
    };
    let (mut paint, mut calc, mut full) = (Vec::new(), Vec::new(), Vec::new());
    let mut cells = 0usize;
    for run in 0..config.warmup + config.iterations {
        let t0 = Instant::now();
        let mut read = read_xlsx_with(
            &bytes,
            &ReadOptions {
                defer_calculation: true,
                ..ReadOptions::default()
            },
        )
        .map_err(|e| e.to_string())?;
        let t1 = Instant::now();
        read.calculate().map_err(|e| e.to_string())?;
        let t2 = Instant::now();
        cells = read.workbook.sheets().iter().map(|s| s.cells().len()).sum();
        // Freeing the workbook is not part of opening it.
        drop(read);
        if run >= config.warmup {
            paint.push(t1 - t0);
            calc.push((t2 - t1).as_secs_f64() * 1000.0);
            full.push((t2 - t0).as_secs_f64() * 1000.0);
        }
    }
    let mut sample = summarize("xlsx-open", &paint, None);
    calc.sort_by(f64::total_cmp);
    full.sort_by(f64::total_cmp);
    sample
        .extra
        .insert("calc_ms".into(), percentile(&calc, 95.0));
    sample
        .extra
        .insert("full_ms".into(), percentile(&full, 95.0));
    #[allow(clippy::cast_precision_loss)]
    sample.extra.insert("cells".into(), cells as f64);
    if !options.quick && cells < 1_000_000 {
        return Err(format!("sheet-1m.xlsx read as {cells} cells"));
    }
    Ok(sample)
}
