// SPDX-License-Identifier: AGPL-3.0-or-later
//! The formula conformance suite: every `conformance/curated-*.csv` and `conformance/generated-*.csv`
//! row, evaluated with the full function library on a `Calc` sheet next to the shared `Data` sheet
//! of `conformance/fixture.json`, at the fixed time 2024-01-15 12:00.
//!
//! - `curated-*.csv`: Microsoft's documented examples and edge cases, typed in by family, plus the
//!   dynamic-array and LAMBDA rows added with the Rust port;
//! - `generated-*.csv`: produced from the LibreOffice oracle by the v1.0 generator (rows the oracle
//!   could not be trusted on are listed with reasons in `conformance/oracle-exclusions.csv`).
//!
//! Each row is `formula,expected,type,notes`. Comparison is type-aware, as in the v1.0 runner:
//! numbers and dates (serials) to 1e-9 relative (1e-14 absolute near zero, or the displayed
//! precision when the notes say `rounded`), errors by code, text and logicals exactly, arrays
//! element by element. Rows typed `skip` are not evaluated, and each one must be explained in
//! `docs/formula-deviations.md`.

mod conformance_support;

use std::path::{Path, PathBuf};

use conformance_support::{compare, conformance_dir, make_engine, read_csv};
use kiln_formula::engine::Engine;
use kiln_formula::values::{ErrorValue, Value};

struct Row {
    file: String,
    line: usize,
    formula: String,
    expected: String,
    kind: String,
    notes: String,
}

fn load_rows() -> Vec<Row> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(conformance_dir())
        .expect("conformance/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            (name.starts_with("curated-") || name.starts_with("generated-"))
                && name.ends_with(".csv")
        })
        .collect();
    files.sort();
    let mut out = Vec::new();
    for f in &files {
        let name = f
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = std::fs::read_to_string(f).expect("csv");
        let rows = read_csv(&text);
        assert_eq!(
            rows[0].join(","),
            "formula,expected,type,notes",
            "{name}: bad header"
        );
        for (i, r) in rows.into_iter().enumerate().skip(1) {
            if r.len() == 1 && r[0].is_empty() {
                continue;
            }
            assert_eq!(r.len(), 4, "{name}:{} has {} fields", i + 1, r.len());
            let mut it = r.into_iter();
            let mut next = || it.next().unwrap_or_default();
            out.push(Row {
                file: name.clone(),
                line: i + 1,
                formula: next(),
                expected: next(),
                kind: next(),
                notes: next(),
            });
        }
    }
    out
}

fn check(engine: &mut Engine, calc: kiln_formula::engine::SheetId, r: &Row) -> Result<(), String> {
    if !["number", "date", "text", "boolean", "error", "array"].contains(&r.kind.as_str()) {
        return Err(format!("unknown type {}", r.kind));
    }
    if r.kind == "error" && ErrorValue::from_code(&r.expected).is_none() {
        return Err(format!("unknown error code {}", r.expected));
    }
    let mut actual = engine
        .evaluate_at(&r.formula, calc, 0, 0, false)
        .map_err(|e| format!("does not parse: {e}"))?;
    // A single cell shows the top-left value of an array result (what the oracle recorded).
    if r.kind != "array"
        && let Value::Array(a) = &actual
    {
        actual = a.get(0, 0).clone();
    }
    let want = if r.kind == "text" {
        format!("\"{}\"", r.expected.replace('"', "\"\""))
    } else {
        r.expected.clone()
    };
    let rounded = r
        .notes
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w == "rounded");
    compare(&actual, &want, rounded)
}

#[test]
fn every_conformance_row_passes() {
    let rows = load_rows();
    let mut engine = make_engine();
    let calc = engine.sheet_id("Calc").expect("Calc");
    let deviations = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/formula-deviations.md"),
    )
    .expect("docs/formula-deviations.md");
    let mut failures = Vec::new();
    let mut by_file: std::collections::BTreeMap<&str, usize> = Default::default();
    let (mut evaluated, mut skipped, mut curated) = (0usize, 0usize, 0usize);
    for r in &rows {
        if r.kind == "skip" {
            skipped += 1;
            if !deviations.contains(&r.formula) {
                failures.push(format!(
                    "{}:{} {} is skipped but not explained in docs/formula-deviations.md",
                    r.file, r.line, r.formula
                ));
            }
            continue;
        }
        evaluated += 1;
        if r.file.starts_with("curated-") {
            curated += 1;
        }
        *by_file.entry(r.file.as_str()).or_default() += 1;
        if let Err(msg) = check(&mut engine, calc, r) {
            failures.push(format!("{}:{} {}: {msg}", r.file, r.line, r.formula));
        }
    }
    let files: Vec<String> = by_file.iter().map(|(f, n)| format!("{f} {n}")).collect();
    println!(
        "conformance: {evaluated} rows evaluated ({curated} curated, {} LibreOffice-generated), {} passed, {} failed, {skipped} skipped\n  {}",
        evaluated - curated,
        evaluated - failures.len().min(evaluated),
        failures.len(),
        files.join(", ")
    );
    assert!(evaluated >= 4_700, "only {evaluated} conformance rows");
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
