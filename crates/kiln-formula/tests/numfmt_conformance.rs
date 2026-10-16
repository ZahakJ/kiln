// SPDX-License-Identifier: AGPL-3.0-or-later
//! Excel number formats against `conformance/numfmt/*.csv` (generated from the `numfmt` oracle by
//! `tools/gen-numfmt-cases.mjs`): every row must format exactly as expected.

use std::path::Path;

use kiln_formula::numfmt::{FormatOptions, NumberFormat};
use kiln_formula::values::Value;

fn read_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_q {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_q = false;
                }
            } else {
                cur.push(ch);
            }
        } else if ch == '"' {
            in_q = true;
        } else if ch == ',' {
            row.push(std::mem::take(&mut cur));
        } else if ch == '\n' {
            row.push(std::mem::take(&mut cur));
            rows.push(std::mem::take(&mut row));
        } else if ch != '\r' {
            cur.push(ch);
        }
    }
    if !cur.is_empty() || !row.is_empty() {
        row.push(cur);
        rows.push(row);
    }
    rows
}

#[test]
fn every_numfmt_case_formats_as_expected() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/numfmt");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("conformance/numfmt")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "csv"))
        .collect();
    files.sort();
    let opts = FormatOptions::default();
    let mut total = 0;
    let mut failures = Vec::new();
    for f in &files {
        let rows = read_csv(&std::fs::read_to_string(f).expect("csv"));
        assert_eq!(rows[0].join(","), "format,value,kind,expected");
        for r in &rows[1..] {
            let (code, raw, kind, expected) = (&r[0], &r[1], &r[2], &r[3]);
            let value = match kind.as_str() {
                "number" => Value::Number(raw.parse().expect("number")),
                "bool" => Value::Bool(raw == "true"),
                _ => Value::from(raw.as_str()),
            };
            total += 1;
            let got = NumberFormat::parse(code).format(&value, &opts);
            if got.as_deref().ok() != Some(expected.as_str()) {
                failures.push(format!(
                    "{code:?} {raw}: expected {expected:?}, got {got:?}"
                ));
            }
        }
    }
    println!(
        "numfmt conformance: {total} rows, {} failed",
        failures.len()
    );
    assert!(total >= 600, "only {total} rows");
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures[..failures.len().min(60)].join("\n")
    );
}
