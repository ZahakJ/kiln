// SPDX-License-Identifier: AGPL-3.0-or-later
//! Round trips over the fixture corpus: every workbook is read, written back (with its source,
//! and as a new package without one) and read again; the model facts, the cell values and
//! formulas must be the same, and every part the writer did not regenerate byte-identical.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use kiln_io_xlsx::{WriteOptions, read_xlsx, snapshot_cells, snapshot_facts, write_xlsx};
use kiln_ooxml::opc::OpcPackage;
use kiln_testing::fidelity::package::compare_packages;

fn corpus() -> Vec<PathBuf> {
    let root = kiln_testing::fixtures::fixtures_dir();
    let mut out = Vec::new();
    for dir in ["xlsx/features", "xlsx/manual", "external/xlsx"] {
        let Ok(rd) = std::fs::read_dir(root.join(dir)) else {
            continue;
        };
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "xlsx"))
            .collect();
        files.sort();
        out.extend(files);
    }
    out
}

fn multiset_diff(a: &[String], b: &[String]) -> (Vec<String>, Vec<String>) {
    let mut m: BTreeMap<&str, i64> = BTreeMap::new();
    for x in a {
        *m.entry(x).or_default() += 1;
    }
    for x in b {
        *m.entry(x).or_default() -= 1;
    }
    let lost = m
        .iter()
        .filter(|(_, n)| **n > 0)
        .map(|(k, _)| (*k).to_owned())
        .collect();
    let added = m
        .iter()
        .filter(|(_, n)| **n < 0)
        .map(|(k, _)| (*k).to_owned())
        .collect();
    (lost, added)
}

fn short(v: &[String]) -> String {
    v.iter()
        .take(6)
        .map(|s| s.chars().take(300).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n    ")
}

fn check(path: &Path, with_source: bool) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let a = read_xlsx(&bytes).map_err(|e| format!("read: {e}"))?;
    let written = write_xlsx(
        &a.workbook,
        with_source.then_some(&a.source),
        &WriteOptions::default(),
    )
    .map_err(|e| format!("write: {e}"))?;
    let b = read_xlsx(&written.bytes).map_err(|e| format!("re-read: {e}"))?;
    // Drawing objects (charts, pictures) live in parts only a save with the source carries.
    let facts = |wb: &kiln_sheet::Workbook| -> Vec<String> {
        snapshot_facts(wb)
            .into_iter()
            .filter(|f| with_source || !f.contains("/drawing="))
            .collect()
    };
    let (fa, fb) = (facts(&a.workbook), facts(&b.workbook));
    let (lost, added) = multiset_diff(&fa, &fb);
    let mut problems = Vec::new();
    if !lost.is_empty() || !added.is_empty() {
        problems.push(format!(
            "facts differ:\n  lost:\n    {}\n  added:\n    {}",
            short(&lost),
            short(&added)
        ));
    }
    let (ca, cb) = (snapshot_cells(&a.workbook), snapshot_cells(&b.workbook));
    let diff: Vec<String> = ca
        .keys()
        .chain(cb.keys())
        .filter(|k| ca.get(*k) != cb.get(*k))
        .map(|k| format!("{k}: {:?} → {:?}", ca.get(k), cb.get(k)))
        .collect();
    if !diff.is_empty() {
        problems.push(format!(
            "{} cells differ:\n    {}",
            diff.len(),
            short(&diff)
        ));
    }
    if with_source {
        let mut regenerated = written.regenerated.clone();
        regenerated.extend(written.dropped.clone());
        let cmp = compare_packages(
            &OpcPackage::open(&bytes).map_err(|e| e.to_string())?,
            &OpcPackage::open(&written.bytes).map_err(|e| e.to_string())?,
            &regenerated,
        );
        if !cmp.differing.is_empty() || !cmp.missing.is_empty() {
            problems.push(format!("untouched parts changed: {cmp:?}"));
        }
        // A second save of the re-read workbook is identical (deterministic, stable).
        let again = write_xlsx(&b.workbook, Some(&b.source), &WriteOptions::default())
            .map_err(|e| format!("second write: {e}"))?;
        let third = read_xlsx(&again.bytes).map_err(|e| format!("third read: {e}"))?;
        if facts(&third.workbook) != fb {
            problems.push("a second round trip changes the facts".into());
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

#[test]
fn every_fixture_round_trips_with_its_source() {
    let files = corpus();
    assert!(files.len() >= 20, "fixtures: {}", files.len());
    let mut failures = Vec::new();
    for f in &files {
        if let Err(e) = check(f, true) {
            failures.push(format!("{}: {e}", f.display()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn every_fixture_round_trips_as_a_new_package() {
    let mut failures = Vec::new();
    for f in &corpus() {
        if let Err(e) = check(f, false) {
            failures.push(format!("{}: {e}", f.display()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
