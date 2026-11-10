// SPDX-License-Identifier: AGPL-3.0-or-later
//! Malformed and hostile files (`fixtures/odf/malformed`): truncated and invalid packages, bad
//! XML, absurd units and attribute values, deep nesting, style cycles and huge repeat counts.
//! Every reader must answer each with a document or an error, within seconds and bounded memory.

use std::path::Path;
use std::time::{Duration, Instant};

/// Peak resident memory of this process, in KB (Linux; `None` elsewhere).
fn peak_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
}

fn read(path: &Path) -> Result<Vec<String>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    match path.extension().and_then(|e| e.to_str()) {
        Some("odt") => kiln_io_odf::read_odt(&bytes)
            .map(|r| r.warnings)
            .map_err(|e| e.to_string()),
        Some("ods") => kiln_io_odf::read_ods(&bytes)
            .map(|r| r.warnings)
            .map_err(|e| e.to_string()),
        Some("odp") => kiln_io_odf::read_odp(&bytes)
            .map(|r| r.warnings)
            .map_err(|e| e.to_string()),
        _ => Err("not an ODF file".into()),
    }
}

fn run(filter: &dyn Fn(&str) -> bool) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/odf/malformed");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("fixtures/odf/malformed")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(filter))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    let mut slow = Vec::new();
    for f in &files {
        let start = Instant::now();
        let result = read(f);
        let took = start.elapsed();
        eprintln!(
            "{}: {:?} in {took:.2?}",
            f.file_name().unwrap().to_string_lossy(),
            result.as_ref().map(Vec::len)
        );
        if took > Duration::from_secs(10) {
            slow.push(format!("{}: {took:.1?}", f.display()));
        }
    }
    assert!(slow.is_empty(), "too slow: {slow:?}");
    if let Some(kb) = peak_rss_kb() {
        assert!(kb < 2_000_000, "peak memory {kb} KB");
    }
}

#[test]
fn malformed_text_documents() {
    run(&|n| n.ends_with(".odt"));
}

#[test]
fn malformed_presentations() {
    run(&|n| n.ends_with(".odp"));
}

#[test]
fn malformed_spreadsheets() {
    run(&|n| n.ends_with(".ods") && !n.contains("repeat") && n != "numfmt.ods");
}

/// A data style asking for billions of digits.
#[test]
fn absurd_digit_counts_are_clamped() {
    run(&|n| n == "numfmt.ods");
}

/// Repeat counts covering the whole grid, with values, text, formulas or formatting.
#[test]
fn huge_repeat_counts_are_bounded() {
    for name in [
        "bigrepeat.ods",
        "bigrepeat2.ods",
        "bigrepeatformula.ods",
        "styledrepeat.ods",
        "valuerepeat.ods",
    ] {
        run(&|n| n == name);
    }
    let bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/odf/malformed/valuerepeat.ods"),
    )
    .unwrap();
    let read = kiln_io_odf::read_ods(&bytes).expect("valuerepeat.ods");
    assert!(
        read.warnings.iter().any(|w| w.contains("repeats cells")),
        "{:?}",
        read.warnings
    );
}
