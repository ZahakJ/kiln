// SPDX-License-Identifier: AGPL-3.0-or-later
//! Open + repack benchmark for the large spreadsheet fixtures (PLAN.md §3.9 budget: < 300 ms).
//!
//! ```sh
//! python3 -I tools/make-fixtures.py --out fixtures --large --only xlsx  # once: the gitignored workbook
//! cargo run --release -p kiln-ooxml --example repack_bench            # fixtures/xlsx/large/*.xlsx
//! cargo run --release -p kiln-ooxml --example repack_bench -- a.xlsx  # any packages
//! ```
//!
//! For each package it reports the median of several runs of `OpcPackage::open` + `save` (no
//! edits), checks that the output is byte-identical part by part, and times a full read (open,
//! inflate every part, repack). The budget applies to both figures; the full read is the one to
//! register in `bench/budgets.toml`. Exits with status 1 when a package misses the budget.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use kiln_ooxml::opc::OpcPackage;

const BUDGET_MS: f64 = 300.0;
const RUNS: usize = 7;

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort_unstable();
    v[v.len() / 2]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn default_inputs() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/xlsx/large");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "xlsx"))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn bench(path: &Path) -> Result<bool, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );

    let mut repack = Vec::with_capacity(RUNS);
    let mut saved = Vec::new();
    for _ in 0..RUNS {
        let t = Instant::now();
        let pkg = OpcPackage::open(&bytes).map_err(|e| format!("{name}: {e}"))?;
        saved = pkg.save().map_err(|e| format!("{name}: {e}"))?;
        repack.push(t.elapsed());
    }

    // Correctness: every part of the output equals the input's.
    let before = OpcPackage::open(&bytes).map_err(|e| e.to_string())?;
    let after = OpcPackage::open(&saved).map_err(|e| e.to_string())?;
    let mut uncompressed = 0usize;
    for part in before.all_part_names() {
        let a = before.part_bytes(part).map_err(|e| e.to_string())?;
        let b = after.part_bytes(part).map_err(|e| e.to_string())?;
        if a != b {
            return Err(format!("{name}: {part} differs after the repack"));
        }
        uncompressed += a.len();
    }

    let mut full = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let t = Instant::now();
        let pkg = OpcPackage::open(&bytes).map_err(|e| e.to_string())?;
        for part in pkg.all_part_names() {
            std::hint::black_box(pkg.part_bytes(part).map_err(|e| e.to_string())?.len());
        }
        std::hint::black_box(pkg.save().map_err(|e| e.to_string())?.len());
        full.push(t.elapsed());
    }

    let repack_ms = ms(median(repack));
    let full_ms = ms(median(full));
    let ok = repack_ms < BUDGET_MS && full_ms < BUDGET_MS;
    println!(
        "{name}: open + inflate every part + repack {full_ms:.1} ms (budget {BUDGET_MS:.0} ms, {}) | \
         open + raw repack {repack_ms:.1} ms | {:.1} MB zip, {:.1} MB uncompressed, \
         {} parts, parts byte-identical",
        if ok { "ok" } else { "OVER BUDGET" },
        bytes.len() as f64 / 1e6,
        uncompressed as f64 / 1e6,
        before.len(),
    );
    Ok(ok)
}

fn main() -> ExitCode {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let inputs = if args.is_empty() {
        default_inputs()
    } else {
        args
    };
    if inputs.is_empty() {
        eprintln!(
            "repack_bench: no packages: fixtures/xlsx/large/ is empty (it is gitignored); \
             generate it with `python3 -I tools/make-fixtures.py --out fixtures --large --only xlsx`"
        );
        return ExitCode::FAILURE;
    }
    let mut all_ok = true;
    for p in &inputs {
        match bench(p) {
            Ok(ok) => all_ok &= ok,
            Err(e) => {
                eprintln!("repack_bench: {e}");
                all_ok = false;
            }
        }
    }
    if all_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
