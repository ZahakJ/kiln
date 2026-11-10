// SPDX-License-Identifier: AGPL-3.0-or-later
//! Spreadsheets: every `.ods` of the corpus (made by LibreOffice from the `.xlsx` feature files)
//! reads, writes and reads back with the same cells, formulas and model facts, and every `.xlsx`
//! feature file converts to `.ods` and back into the model with its values and formulas.

use std::collections::BTreeMap;
use std::path::PathBuf;

use kiln_io_odf::{WriteOptions, read_ods, write_ods};

fn dir(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel)
}

fn files(rel: &str, ext: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir(rel))
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == ext))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn overlap(a: &[String], b: &[String]) -> (f64, Vec<String>) {
    let mut m = std::collections::HashMap::new();
    for x in a {
        *m.entry(x.as_str()).or_insert(0i64) += 1;
    }
    let mut common = 0usize;
    let mut extra = Vec::new();
    for y in b {
        match m.get_mut(y.as_str()) {
            Some(n) if *n > 0 => {
                *n -= 1;
                common += 1;
            }
            _ => extra.push(format!("+ {y}")),
        }
    }
    for (k, n) in m {
        for _ in 0..n {
            extra.push(format!("- {k}"));
        }
    }
    let d = a.len().max(b.len());
    (
        if d == 0 {
            1.0
        } else {
            common as f64 / d as f64
        },
        extra,
    )
}

/// Formula text compared without white space, case, sheet-name quotes and the `()` of
/// `TRUE()`/`FALSE()` (OpenFormula has only the functions, Excel both forms).
fn norm(f: &Option<String>) -> Option<String> {
    f.as_ref().map(|s| {
        s.chars()
            .filter(|c| !c.is_whitespace() && *c != '\'')
            .collect::<String>()
            .to_uppercase()
            .replace("TRUE()", "TRUE")
            .replace("FALSE()", "FALSE")
    })
}

fn dump(name: &std::ffi::OsStr, sub: &str, bytes: &[u8]) {
    if std::env::var_os("KILN_ODF_DUMP").is_some() {
        let o = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/scratch")
            .join(sub);
        std::fs::create_dir_all(&o).unwrap();
        std::fs::write(o.join(name), bytes).unwrap();
    }
}

#[test]
fn ods_fixtures_round_trip() {
    let all = files("odf/features", "ods");
    assert!(all.len() >= 20, "{}", all.len());
    let mut failures = Vec::new();
    for f in &all {
        let bytes = std::fs::read(f).unwrap();
        let a = read_ods(&bytes).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let out = write_ods(
            &a.workbook,
            &WriteOptions {
                charts: a.charts.clone(),
                ..WriteOptions::default()
            },
        )
        .unwrap();
        dump(f.file_name().unwrap(), "rt", &out);
        let b = read_ods(&out).unwrap_or_else(|e| panic!("{} (written): {e}", f.display()));
        let (ca, cb): (BTreeMap<_, _>, BTreeMap<_, _>) = (
            kiln_io_xlsx::snapshot_cells(&a.workbook),
            kiln_io_xlsx::snapshot_cells(&b.workbook),
        );
        let diff_cells: Vec<String> = ca
            .iter()
            .filter(|(k, v)| cb.get(*k).map(|w| (&w.0, norm(&w.1))) != Some((&v.0, norm(&v.1))))
            .map(|(k, v)| format!("{k}: {v:?} -> {:?}", cb.get(k)))
            .chain(
                cb.keys()
                    .filter(|k| !ca.contains_key(*k))
                    .map(|k| format!("added {k}")),
            )
            .take(8)
            .collect();
        let (s, extra) = overlap(
            &kiln_io_xlsx::snapshot_facts(&a.workbook),
            &kiln_io_xlsx::snapshot_facts(&b.workbook),
        );
        eprintln!("{} cells={} structural={s:.3}", f.display(), ca.len());
        // LibreOffice's data styles cannot hold every Excel code exactly (digit counts around
        // embedded text): 0.98 is the floor here; the corpus threshold is 0.90.
        if !diff_cells.is_empty() || s < 0.98 {
            failures.push(format!(
                "{}: structural {s:.3}\n  cells: {diff_cells:#?}\n  facts: {:#?}",
                f.display(),
                extra.iter().take(10).collect::<Vec<_>>()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn xlsx_features_convert_to_ods() {
    let mut failures = Vec::new();
    for f in files("xlsx/features", "xlsx") {
        let bytes = std::fs::read(&f).unwrap();
        let a = kiln_io_xlsx::read_xlsx(&bytes).unwrap();
        let charts = a
            .source
            .charts()
            .into_iter()
            .filter_map(|(n, c)| c.ok().map(|c| (n, c)))
            .collect();
        let out = write_ods(
            &a.workbook,
            &WriteOptions {
                charts,
                ..WriteOptions::default()
            },
        )
        .unwrap();
        dump(
            f.with_extension("ods").file_name().unwrap(),
            "from-xlsx",
            &out,
        );
        let b = read_ods(&out).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let (ca, cb) = (
            kiln_io_xlsx::snapshot_cells(&a.workbook),
            kiln_io_xlsx::snapshot_cells(&b.workbook),
        );
        // Structured references are written as the ranges they denote: their values must match.
        let bad: Vec<String> = ca
            .iter()
            .filter(|(k, v)| {
                let w = cb.get(*k);
                if v.1.as_deref().is_some_and(|f| f.contains('[')) {
                    return w.map(|w| &w.0) != Some(&v.0);
                }
                // ODF stores dates as text, to the microsecond at best.
                let near = |x: &str, y: &str| match (x.parse::<f64>(), y.parse::<f64>()) {
                    (Ok(a), Ok(b)) => (a - b).abs() < 1e-8,
                    _ => x == y,
                };
                !w.is_some_and(|w| near(&w.0, &v.0) && norm(&w.1) == norm(&v.1))
            })
            .map(|(k, v)| format!("{k}: {v:?} -> {:?}", cb.get(k)))
            .take(8)
            .collect();
        if !bad.is_empty() {
            failures.push(format!("{}: {bad:#?}", f.display()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
