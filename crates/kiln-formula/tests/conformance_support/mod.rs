// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared by `tests/conformance.rs` and `tests/docs.rs`: the CSV reader, the conformance workbook
//! (`conformance/fixture.json` on a `Data` sheet, an empty `Calc` sheet, the full library and a
//! fixed clock) and the type-aware comparison of a value with an expected formula literal
//! (`6`, `"text"`, `TRUE`, `#N/A`, `{1,2;3,4}`), as in the v1.0 runner.
#![allow(dead_code)]

use std::path::Path;
use std::sync::Arc;

use kiln_formula::engine::{CellInput, Engine, EngineOptions};
use kiln_formula::functions;
use kiln_formula::refs::parse_cell_address;
use kiln_formula::values::{Array2D, ErrorValue, Value};

/// 2024-01-15 12:00, the clock of the v1.0 suite (`TODAY`, `NOW`, `YEAR(NOW())` rows).
pub const NOW: f64 = 45306.5;

pub fn conformance_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance")
}

pub fn read_csv(text: &str) -> Vec<Vec<String>> {
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

pub fn make_engine() -> Engine {
    let mut e = Engine::with_options(EngineOptions {
        clock: Some(Arc::new(|| NOW)),
        ..functions::engine_options()
    });
    e.add_sheet("Calc").expect("Calc");
    let text = std::fs::read_to_string(conformance_dir().join("fixture.json")).expect("fixture");
    let fixture: serde_json::Value = serde_json::from_str(&text).expect("fixture json");
    let sheet = fixture["sheet"].as_str().expect("sheet name").to_owned();
    e.add_sheet(&sheet).expect("Data");
    let cells = fixture["cells"].as_object().expect("cells");
    for (addr, v) in cells {
        let (row, col) = parse_cell_address(addr).expect("fixture address");
        let input = match v {
            serde_json::Value::String(s) if s.starts_with('=') => CellInput::Formula {
                text: s.clone(),
                legacy: false,
            },
            serde_json::Value::String(s) => {
                CellInput::Value(Value::from(s.strip_prefix('\'').unwrap_or(s)))
            }
            serde_json::Value::Bool(b) => CellInput::Value(Value::Bool(*b)),
            serde_json::Value::Number(n) => {
                CellInput::Value(Value::Number(n.as_f64().expect("number")))
            }
            serde_json::Value::Object(o) => {
                let code = o.get("error").and_then(|c| c.as_str()).expect("error code");
                CellInput::Value(Value::Error(
                    ErrorValue::from_code(code).expect("known error"),
                ))
            }
            other => panic!("unexpected fixture value {other}"),
        };
        e.set_cell(sheet.as_str(), row, col, input)
            .expect("fixture cell");
    }
    e.recalc();
    e
}

/// Splits an array literal's body into element texts, marking row ends.
pub fn split_elements(body: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            if in_str && chars.peek() == Some(&'"') {
                cur.push_str("\"\"");
                chars.next();
                continue;
            }
            in_str = !in_str;
            cur.push(ch);
        } else if !in_str && (ch == ',' || ch == ';') {
            out.push((cur.trim().to_owned(), ch == ';'));
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    out.push((cur.trim().to_owned(), true));
    out
}

pub fn expected_rows(text: &str) -> (Vec<Vec<String>>, bool) {
    let t = text.trim();
    if !t.starts_with('{') {
        return (vec![vec![t.to_owned()]], false);
    }
    let mut rows: Vec<Vec<String>> = vec![Vec::new()];
    for (el, row_end) in split_elements(&t[1..t.len() - 1]) {
        if let Some(r) = rows.last_mut() {
            r.push(el);
        }
        if row_end {
            rows.push(Vec::new());
        }
    }
    rows.pop();
    (rows, true)
}

/// Tolerance implied by how many digits a number literal was written with.
pub fn display_tolerance(lit: &str) -> f64 {
    let t = lit.trim().trim_start_matches(['+', '-']);
    let (mant, exp) = match t.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().unwrap_or(0)),
        None => (t, 0),
    };
    let (int, frac) = mant.split_once('.').unwrap_or((mant, ""));
    let sig = format!("{}{frac}", int.trim_start_matches('0'))
        .trim_start_matches('0')
        .len();
    let v: f64 = lit.trim().parse().unwrap_or(0.0);
    if sig >= 15 {
        return v.abs() * 1e-13;
    }
    0.5 * 10f64.powi(exp - frac.len() as i32) * (1.0 + 1e-9)
}

pub fn scalar_matches(actual: &Value, lit: &str, rounded: bool) -> bool {
    let lit = lit.trim();
    if lit.len() >= 2 && lit.starts_with('"') && lit.ends_with('"') {
        let want = lit[1..lit.len() - 1].replace("\"\"", "\"");
        return actual.as_str() == Some(want.as_str());
    }
    if lit.eq_ignore_ascii_case("TRUE") {
        return *actual == Value::Bool(true);
    }
    if lit.eq_ignore_ascii_case("FALSE") {
        return *actual == Value::Bool(false);
    }
    if lit.starts_with('#') {
        return actual.as_error() == ErrorValue::from_code(lit);
    }
    if lit.is_empty() {
        return matches!(actual, Value::Blank)
            || *actual == Value::Number(0.0)
            || actual.as_str() == Some("");
    }
    let Ok(want) = lit.parse::<f64>() else {
        return false;
    };
    let Some(got) = actual.as_number() else {
        return false;
    };
    let tol = if rounded {
        display_tolerance(lit)
    } else {
        (want.abs() * 1e-9).max(1e-14)
    };
    (got - want).abs() <= tol
}

pub fn compare(actual: &Value, expected: &str, rounded: bool) -> Result<(), String> {
    let (rows, is_array) = expected_rows(expected);
    if !is_array {
        let v = match actual {
            Value::Array(a) if a.len() == 1 => a.get(0, 0).clone(),
            other => other.clone(),
        };
        if matches!(v, Value::Array(_)) {
            return Err(format!("expected {expected}, got array {v:?}"));
        }
        return if scalar_matches(&v, &rows[0][0], rounded) {
            Ok(())
        } else {
            Err(format!("expected {expected}, got {v:?}"))
        };
    }
    let arr = match actual {
        Value::Array(a) => (**a).clone(),
        other => Array2D::filled(1, 1, other.clone()),
    };
    let (r, c) = (rows.len(), rows[0].len());
    if arr.rows() != r || arr.cols() != c {
        return Err(format!("expected {expected} ({r}×{c}), got {arr:?}"));
    }
    for (i, row) in rows.iter().enumerate() {
        for (j, lit) in row.iter().enumerate() {
            if !scalar_matches(arr.get(i, j), lit, rounded) {
                return Err(format!("expected {expected}, got {arr:?}"));
            }
        }
    }
    Ok(())
}
