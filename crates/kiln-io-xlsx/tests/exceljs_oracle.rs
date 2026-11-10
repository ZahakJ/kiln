// SPDX-License-Identifier: AGPL-3.0-or-later
//! The reader against an independent one: `fixtures/xlsx/read/<name>.exceljs.json`, written by
//! `node packages/testing/scripts/exceljs-xlsx.mjs` with exceljs 4.4 from every feature fixture.
//! Every value, formula, sheet and merged range exceljs reports must be what Kiln reads (1.00;
//! cells a merge covers are not compared, as exceljs repeats the merge's value in them);
//! the format summary (number format, font, fill, alignment, borders) must agree in at least 97 %
//! of the properties of every workbook.

use std::collections::BTreeMap;

use kiln_io_xlsx::formula_text::from_file_formula;
use kiln_io_xlsx::read_xlsx;
use kiln_sheet::styles::{BorderStyle, CellStyle, Fill, HAlign, PatternType, Underline, VAlign};
use kiln_sheet::{CellValue, Color, Workbook};
use serde_json::Value;

fn normalize_formula(f: &str) -> String {
    let bare = from_file_formula(f.trim().trim_start_matches('='), None);
    let mut out = String::new();
    let mut in_str = false;
    for c in bare.chars() {
        if c == '"' {
            in_str = !in_str;
        }
        if !in_str && c.is_whitespace() {
            continue;
        }
        out.extend(if in_str {
            vec![c]
        } else {
            c.to_uppercase().collect()
        });
    }
    out
}

fn color_text(c: Option<&Color>) -> Option<String> {
    match c? {
        Color::Rgb { argb, .. } => Some(format!("argb:{argb:08X}")),
        Color::Theme { index, tint } => Some(if tint.0 == 0.0 {
            format!("theme:{index}")
        } else {
            format!("theme:{index}:{}", tint.0)
        }),
        Color::Indexed { index, .. } => Some(format!("indexed:{index}")),
        Color::Auto => None,
    }
}

fn border_name(b: BorderStyle) -> &'static str {
    match b {
        BorderStyle::None => "none",
        BorderStyle::Thin => "thin",
        BorderStyle::Medium => "medium",
        BorderStyle::Dashed => "dashed",
        BorderStyle::Dotted => "dotted",
        BorderStyle::Thick => "thick",
        BorderStyle::Double => "double",
        BorderStyle::Hair => "hair",
        BorderStyle::MediumDashed => "mediumDashed",
        BorderStyle::DashDot => "dashDot",
        BorderStyle::MediumDashDot => "mediumDashDot",
        BorderStyle::DashDotDot => "dashDotDot",
        BorderStyle::MediumDashDotDot => "mediumDashDotDot",
        BorderStyle::SlantDashDot => "slantDashDot",
    }
}

/// Our value of each property the oracle's summary names.
fn property(style: &CellStyle, path: &str) -> Option<Value> {
    let f = &style.font;
    let a = &style.alignment;
    let side = |s: Option<kiln_sheet::styles::BorderSide>| {
        s.map(|x| Value::String(border_name(x.style).into()))
    };
    Some(match path {
        "numFmt" => Value::String(style.num_fmt.to_string()),
        "font.bold" => Value::Bool(f.bold),
        "font.italic" => Value::Bool(f.italic),
        "font.strike" => Value::Bool(f.strike),
        "font.underline" => Value::String(
            match f.underline {
                Underline::None => "none",
                Underline::Single => "single",
                Underline::Double => "double",
                Underline::SingleAccounting => "singleAccounting",
                Underline::DoubleAccounting => "doubleAccounting",
            }
            .into(),
        ),
        "font.size" => serde_json::json!(f.size.0),
        "font.name" => Value::String(f.name.to_string()),
        "font.color" => Value::String(color_text(f.color.as_ref())?),
        "fill.pattern" => match &style.fill {
            Fill::Pattern { pattern, .. } => Value::String(match pattern {
                PatternType::Solid => "solid".to_owned(),
                other => {
                    let s = format!("{other:?}");
                    let mut c = s.chars();
                    c.next()
                        .map(|h| h.to_lowercase().chain(c).collect())
                        .unwrap_or_default()
                }
            }),
            Fill::Gradient(_) => Value::String("gradient".into()),
        },
        "fill.fg" => match &style.fill {
            Fill::Pattern { fg, .. } => Value::String(color_text(fg.as_ref())?),
            Fill::Gradient(_) => return None,
        },
        "alignment.horizontal" => Value::String(
            match a.horizontal {
                HAlign::General => return None,
                HAlign::Left => "left",
                HAlign::Center => "center",
                HAlign::Right => "right",
                HAlign::Fill => "fill",
                HAlign::Justify => "justify",
                HAlign::CenterContinuous => "centerContinuous",
                HAlign::Distributed => "distributed",
            }
            .into(),
        ),
        "alignment.vertical" => Value::String(
            match a.vertical {
                VAlign::Top => "top",
                VAlign::Center => "middle",
                VAlign::Bottom => "bottom",
                VAlign::Justify => "justify",
                VAlign::Distributed => "distributed",
            }
            .into(),
        ),
        "alignment.wrapText" => Value::Bool(a.wrap_text),
        "alignment.indent" => serde_json::json!(a.indent),
        "alignment.textRotation" => serde_json::json!(a.text_rotation),
        "border.left" => side(style.border.left)?,
        "border.right" => side(style.border.right)?,
        "border.top" => side(style.border.top)?,
        "border.bottom" => side(style.border.bottom)?,
        _ => return None,
    })
}

fn flatten(prefix: &str, v: &Value, out: &mut Vec<(String, Value)>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                let p = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(&p, x, out);
            }
        }
        other => out.push((prefix.to_owned(), other.clone())),
    }
}

fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            (x.as_f64().unwrap_or(f64::NAN) - y.as_f64().unwrap_or(f64::NAN)).abs() < 1e-9
        }
        (Value::String(x), Value::String(y)) => {
            // Colours: exceljs keeps tints as written; formats as written.
            x == y || x.eq_ignore_ascii_case(y)
        }
        _ => a == b,
    }
}

fn value_agrees(ours: &CellValue, oracle: &Value) -> bool {
    let t = oracle.get("type").and_then(Value::as_str).unwrap_or("");
    let v = oracle.get("value");
    match t {
        "number" => match (ours, v.and_then(Value::as_f64)) {
            (CellValue::Number(n), Some(x)) => (n - x).abs() <= 1e-9 * n.abs().max(1.0),
            _ => false,
        },
        "string" => ours.as_text() == v.and_then(Value::as_str),
        "boolean" => {
            matches!((ours, v.and_then(Value::as_bool)), (CellValue::Bool(a), Some(b)) if *a == b)
        }
        "error" => {
            matches!(ours, CellValue::Error(e) if Some(e.code()) == v.and_then(Value::as_str))
        }
        "blank" => ours.is_blank(),
        _ => true,
    }
}

struct Score {
    values: (usize, usize),
    styles: (usize, usize),
    failures: Vec<String>,
    style_misses: Vec<String>,
}

fn check(wb: &Workbook, oracle: &Value) -> Score {
    let mut s = Score {
        values: (0, 0),
        styles: (0, 0),
        failures: Vec::new(),
        style_misses: Vec::new(),
    };
    let sheets = oracle["sheets"].as_array().cloned().unwrap_or_default();
    let ours: Vec<&str> = wb.sheets().iter().map(kiln_sheet::Sheet::name).collect();
    let theirs: Vec<&str> = sheets.iter().filter_map(|x| x["name"].as_str()).collect();
    if ours != theirs {
        s.failures
            .push(format!("sheets: Kiln {ours:?}, exceljs {theirs:?}"));
    }
    for sh in &sheets {
        let Some(name) = sh["name"].as_str() else {
            continue;
        };
        let Some(sheet) = wb.sheet_by_name(name) else {
            continue;
        };
        let id = sheet.id();
        let mut merges: Vec<String> = sheet.merges().iter().map(|r| r.to_a1(false)).collect();
        merges.sort();
        let mut want: Vec<String> = sh["merges"]
            .as_array()
            .map(|m| {
                m.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        want.sort();
        s.values.1 += 1;
        if merges == want {
            s.values.0 += 1;
        } else {
            s.failures
                .push(format!("{name}: merges {merges:?} vs {want:?}"));
        }
        let cells: BTreeMap<String, Value> = sh["cells"]
            .as_object()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        for (addr, cell) in &cells {
            let Some(pos) = kiln_sheet::address::parse_cell_a1(addr) else {
                continue;
            };
            let (r, c) = (pos.row, pos.col);
            // exceljs gives the cells a merge covers the value of its first cell; in the file
            // (and in Kiln, as in Excel) they are empty.
            let covered = sheet
                .merges()
                .iter()
                .any(|m| m.contains(r, c) && (m.r0, m.c0) != (r, c));
            if covered && cell.get("formula").is_none() {
                continue;
            }
            s.values.1 += 1;
            if let Some(f) = cell.get("formula").and_then(Value::as_str) {
                let ours = wb.formula(id, r, c).map(|x| normalize_formula(&x));
                if ours.as_deref() == Some(normalize_formula(f).as_str()) {
                    s.values.0 += 1;
                } else {
                    s.failures.push(format!(
                        "{name}!{addr}: formula Kiln {ours:?}, exceljs {f:?}"
                    ));
                }
            } else if cell.get("type").is_some() {
                let ours = sheet
                    .cells()
                    .get(r, c)
                    .map_or(CellValue::Blank, |d| d.value.clone());
                if value_agrees(&ours, cell) {
                    s.values.0 += 1;
                } else {
                    s.failures.push(format!(
                        "{name}!{addr}: value Kiln {ours:?}, exceljs {cell}"
                    ));
                }
            } else {
                s.values.0 += 1;
            }
            if let Some(style) = cell.get("style") {
                let ours = wb.styles().get(sheet.style_at(r, c));
                let mut props = Vec::new();
                flatten("", style, &mut props);
                for (path, want) in props {
                    s.styles.1 += 1;
                    let got = property(&ours, &path);
                    if got.as_ref().is_some_and(|g| same(g, &want)) {
                        s.styles.0 += 1;
                    } else {
                        s.style_misses.push(format!(
                            "{name}!{addr} {path}: Kiln {got:?}, exceljs {want}"
                        ));
                    }
                }
            }
        }
    }
    s
}

#[test]
fn the_reader_agrees_with_exceljs() {
    let root = kiln_testing::fixtures::fixtures_dir();
    let dir = root.join("xlsx/read");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("fixtures/xlsx/read")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".exceljs.json"))
        .collect();
    files.sort();
    assert!(files.len() >= 20, "{} oracle files", files.len());
    let (mut checked, mut unreadable) = (0, Vec::new());
    let mut failures = Vec::new();
    for f in &files {
        let name = f
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .replace(".exceljs.json", ".xlsx");
        let oracle: Value =
            serde_json::from_str(&std::fs::read_to_string(f).expect("json")).expect("valid json");
        if oracle.get("error").is_some() {
            unreadable.push(name);
            continue;
        }
        let bytes = std::fs::read(root.join("xlsx/features").join(&name)).expect("fixture");
        let read = read_xlsx(&bytes).expect("read");
        let s = check(&read.workbook, &oracle);
        checked += 1;
        let styles = if s.styles.1 == 0 {
            1.0
        } else {
            s.styles.0 as f64 / s.styles.1 as f64
        };
        eprintln!(
            "{name}: values {}/{}, styles {}/{} ({styles:.3})",
            s.values.0, s.values.1, s.styles.0, s.styles.1
        );
        for m in s.style_misses.iter().take(5) {
            eprintln!("  style: {m}");
        }
        if !s.failures.is_empty() {
            failures.push(format!("{name}:\n  {}", s.failures.join("\n  ")));
        }
        if styles < 0.97 {
            failures.push(format!(
                "{name}: styles {styles:.3} < 0.97:\n  {}",
                s.style_misses.join("\n  ")
            ));
        }
    }
    eprintln!("checked {checked}; exceljs cannot read {unreadable:?}");
    assert!(checked >= 15, "only {checked} workbooks checked");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
