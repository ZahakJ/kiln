// SPDX-License-Identifier: AGPL-3.0-or-later
//! A second independent reader for the feature fixtures exceljs 4.4 cannot open (`charts`,
//! `comments`, `tables`: it fails on their drawings): `fixtures/xlsx/read/<name>.openpyxl.json`,
//! written by `tests/oracle/openpyxl-xlsx.py` with openpyxl 3.1. Every constant, formula, merged
//! range, comment and table openpyxl reports must be what Kiln reads, before and after a save,
//! and Kiln must hold no cell openpyxl does not.

use std::collections::BTreeSet;

use kiln_io_xlsx::formula_text::from_file_formula;
use kiln_io_xlsx::text::{cell_ref, parse_cell_ref};
use kiln_io_xlsx::{WriteOptions, read_xlsx, write_xlsx};
use kiln_sheet::{CellValue, Range, Workbook};
use serde_json::Value;

const FILES: &[&str] = &["charts", "comments", "tables"];

/// A formula with prefixes stripped, whitespace outside strings removed and names upper-cased.
fn normalize_formula(f: &str) -> String {
    let bare = from_file_formula(f.trim().trim_start_matches('='), None);
    let mut out = String::new();
    let mut in_str = false;
    for c in bare.chars() {
        if c == '"' {
            in_str = !in_str;
        }
        if in_str {
            out.push(c);
        } else if !c.is_whitespace() {
            out.extend(c.to_uppercase());
        }
    }
    out
}

fn range_text(r: &Range) -> String {
    let a = cell_ref(r.r0, r.c0);
    if (r.r0, r.c0) == (r.r1, r.c1) {
        a
    } else {
        format!("{a}:{}", cell_ref(r.r1, r.c1))
    }
}

fn compare(name: &str, wb: &Workbook, oracle: &Value, failures: &mut Vec<String>) -> usize {
    let mut checked = 0usize;
    let sheets = oracle["sheets"].as_array().cloned().unwrap_or_default();
    assert_eq!(wb.sheets().len(), sheets.len(), "{name}: sheet count");
    for (s, o) in wb.sheets().iter().zip(&sheets) {
        let sheet = o["name"].as_str().unwrap_or_default();
        if s.name() != sheet {
            failures.push(format!("{name}: sheet {:?}, openpyxl {sheet:?}", s.name()));
            continue;
        }
        let cells = o["cells"].as_object().cloned().unwrap_or_default();
        let mut seen = BTreeSet::new();
        for (at, want) in &cells {
            let Some((r, c)) = parse_cell_ref(at.as_bytes()) else {
                failures.push(format!("{name} {sheet}!{at}: bad reference"));
                continue;
            };
            seen.insert((r, c));
            checked += 1;
            let value = wb.value(s.id(), r, c);
            let ok = if let Some(f) = want.get("f").and_then(Value::as_str) {
                wb.formula(s.id(), r, c)
                    .is_some_and(|ours| normalize_formula(&ours) == normalize_formula(f))
            } else if let Some(n) = want.get("n").and_then(Value::as_f64) {
                matches!(value, CellValue::Number(x) if (x - n).abs() <= 1e-9 * n.abs().max(1.0))
            } else if let Some(t) = want.get("s").and_then(Value::as_str) {
                match &value {
                    CellValue::Text(x) => &**x == t,
                    CellValue::RichText(x) => &**x.plain() == t,
                    _ => false,
                }
            } else if let Some(b) = want.get("b").and_then(Value::as_bool) {
                value == CellValue::Bool(b)
            } else if let Some(e) = want.get("e").and_then(Value::as_str) {
                matches!(&value, CellValue::Error(x) if x.code() == e)
            } else {
                false
            };
            if !ok {
                failures.push(format!(
                    "{name} {sheet}!{at}: openpyxl {want}, Kiln {value:?} / {:?}",
                    wb.formula(s.id(), r, c)
                ));
            }
        }
        for (r, c, _) in s.cells().iter() {
            if !seen.contains(&(r, c)) && wb.value(s.id(), r, c) != CellValue::Blank {
                failures.push(format!(
                    "{name} {sheet}!{}: Kiln holds a value openpyxl does not",
                    cell_ref(r, c)
                ));
            }
        }
        let merges: BTreeSet<String> = s.merges().iter().map(range_text).collect();
        let want: BTreeSet<String> = o["merges"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        if merges != want {
            failures.push(format!(
                "{name} {sheet}: merges {merges:?}, openpyxl {want:?}"
            ));
        }
        let meta = s.meta();
        for cm in o["comments"].as_array().into_iter().flatten() {
            checked += 1;
            let at = cm["ref"].as_str().unwrap_or_default();
            let author = cm["author"].as_str().unwrap_or_default();
            let text = cm["text"].as_str().unwrap_or_default();
            let pos = parse_cell_ref(at.as_bytes());
            let found =
                meta.notes.iter().any(|n| {
                    Some((n.at.row, n.at.col)) == pos && n.author == author && n.text == text
                }) || meta
                    .threads
                    .iter()
                    .any(|t| Some((t.at.row, t.at.col)) == pos && text.contains(t.text.as_str()));
            if !found {
                failures.push(format!(
                    "{name} {sheet}!{at}: openpyxl comment {author:?} {text:?} not read"
                ));
            }
        }
        let tables: BTreeSet<(String, String)> = s
            .tables()
            .iter()
            .map(|t| (t.name.clone(), range_text(&t.range)))
            .collect();
        let want: BTreeSet<(String, String)> = o["tables"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| {
                (
                    t["name"].as_str().unwrap_or_default().to_owned(),
                    t["ref"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        checked += want.len();
        if tables != want {
            failures.push(format!(
                "{name} {sheet}: tables {tables:?}, openpyxl {want:?}"
            ));
        }
    }
    checked
}

#[test]
fn reader_and_writer_agree_with_openpyxl() {
    let fixtures = kiln_testing::fixtures::fixtures_dir().join("xlsx");
    let mut failures = Vec::new();
    for name in FILES {
        let oracle: Value = serde_json::from_slice(
            &std::fs::read(fixtures.join(format!("read/{name}.openpyxl.json"))).expect("oracle"),
        )
        .expect("json");
        let bytes = std::fs::read(fixtures.join(format!("features/{name}.xlsx"))).expect("fixture");
        let read = read_xlsx(&bytes).expect("read");
        let checked = compare(name, &read.workbook, &oracle, &mut failures);
        assert!(checked >= 10, "{name}: only {checked} items compared");
        // The same after a save (reusing the source) and after a fresh save.
        for source in [Some(&read.source), None] {
            let w = write_xlsx(&read.workbook, source, &WriteOptions::default()).expect("write");
            let again = read_xlsx(&w.bytes).expect("read again");
            compare(name, &again.workbook, &oracle, &mut failures);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
