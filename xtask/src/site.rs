// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask site [--check]`: the figures on the landing page and the README, and the generated
//! manual page.
//!
//! The few figures the site shows a visitor are written from their sources, not typed by hand:
//!
//! - `docs/data/perf.json` and `suite.json`: small JSON files read from `docs/qa/perf.md` (the web
//!   app's time to open the 1M-cell sheet), the formula documentation and the bundled templates;
//! - `docs/index.html`: every `data-stat="NAME"` element, and the comparison between the
//!   `<!-- site:compare -->` markers, read from `docs/qa/compare-facts.json` (published facts, each
//!   with the pages that say it) and, once it exists, `docs/qa/compare.json` (head-to-head
//!   measurements), so the page needs no script and cannot drift from them;
//! - `README.md`: every `data-stat="NAME"` element, as on the page;
//! - `docs/manual/functions.md`: the function list, from `crates/kiln-formula/docs/*.json`.
//!
//! `--check` writes nothing and fails when a file differs or when `docs/qa/fidelity.md` no longer
//! matches its inputs; `tools/pages-build.sh` runs it.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use regex::Regex;
use serde_json::{Value, json};

use crate::util;

/// The row of `docs/qa/perf.md` with the web core's time on the 1M-cell sheet.
const WEB_OPEN_ROW: &str = "| Web core: open the 1M-cell .xlsx to first paint |";

const PERF_MD: &str = "docs/qa/perf.md";
const FACTS: &str = "docs/qa/compare-facts.json";
const MEASURED: &str = "docs/qa/compare.json";
const INDEX: &str = "docs/index.html";
const FUNCTIONS_MD: &str = "docs/manual/functions.md";
const README: &str = "README.md";

/// Runs the command.
pub fn run(args: &[String]) -> Result<(), String> {
    let check = match args {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => return Err("usage: cargo xtask site [--check]".to_owned()),
    };
    if check {
        // The fidelity report the README and the manual link to must come from the code here.
        util::run(
            &util::cargo(),
            &util::args(&[
                "run",
                "-q",
                "-p",
                "kiln-cli",
                "--",
                "fidelity",
                "--check-stamp",
                "docs/qa/fidelity.md",
            ]),
        )?;
    }
    let root = util::repo_root();
    let read = |rel: &str| {
        fs::read_to_string(root.join(rel)).map_err(|e| format!("cannot read {rel}: {e}"))
    };

    let perf = parse_perf(&read(PERF_MD)?)?;
    let functions = load_functions(&root)?;
    let suite = suite_data(&root, &read("package.json")?, &functions)?;
    let facts: Value = serde_json::from_str(&read(FACTS)?).map_err(|e| format!("{FACTS}: {e}"))?;
    let measured: Option<Value> = match fs::read_to_string(root.join(MEASURED)) {
        Ok(text) => Some(serde_json::from_str(&text).map_err(|e| format!("{MEASURED}: {e}"))?),
        Err(_) => None,
    };

    let mut page = read(INDEX)?;
    page = fill_block(&page, "compare", &compare_html(&facts, measured.as_ref())?)?;
    let numbers = stats(&perf, &suite);
    page = fill_stats(INDEX, &page, &numbers)?;
    let readme = fill_stats(README, &read(README)?, &numbers)?;

    let outputs = [
        ("docs/data/perf.json".to_owned(), pretty(&perf)?),
        ("docs/data/suite.json".to_owned(), pretty(&suite)?),
        (INDEX.to_owned(), page),
        (README.to_owned(), readme),
        (FUNCTIONS_MD.to_owned(), functions_md(&functions)),
    ];
    let mut stale = Vec::new();
    for (rel, text) in &outputs {
        let path = root.join(rel);
        if fs::read_to_string(&path).ok().as_deref() == Some(text.as_str()) {
            continue;
        }
        if check {
            stale.push(rel.clone());
        } else {
            fs::write(&path, text).map_err(|e| format!("cannot write {rel}: {e}"))?;
            println!("wrote {rel}");
        }
    }
    if !stale.is_empty() {
        return Err(format!(
            "out of date, run `cargo xtask site`: {}",
            stale.join(", ")
        ));
    }
    Ok(())
}

fn pretty(value: &Value) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map(|s| s + "\n")
        .map_err(|e| e.to_string())
}

fn cells(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|c| c.trim().to_owned())
        .collect()
}

/// Milliseconds in `< 400 ms`, `32.7 ms (p95)` or `2 s`.
fn millis(text: &str) -> Option<f64> {
    let re = Regex::new(r"([0-9][0-9,.]*)\s*(ms|s)\b").ok()?;
    let c = re.captures(text)?;
    let n: f64 = c[1].replace(',', "").parse().ok()?;
    Some(if &c[2] == "s" { n * 1000.0 } else { n })
}

/// The date and the machine of the generated block of `docs/qa/perf.md`, and the web core's time
/// to open the 1M-cell sheet ([`WEB_OPEN_ROW`]).
fn parse_perf(md: &str) -> Result<Value, String> {
    let re = Regex::new(r"Measured by `cargo xtask budgets` on (\d{4}-\d{2}-\d{2}) \(([^)]*)\)")
        .map_err(|e| e.to_string())?;
    let c = re
        .captures(md)
        .ok_or("perf.md: no generated block (cargo xtask budgets)")?;
    let web = md
        .lines()
        .find(|l| l.starts_with(WEB_OPEN_ROW))
        .map(cells)
        .and_then(|c| c.get(1).and_then(|m| millis(m)))
        .ok_or("perf.md: no web row for the 1M-cell sheet")?;
    Ok(json!({ "date": &c[1], "machine": &c[2], "webOpenMs": web }))
}

#[derive(Debug)]
struct Function {
    name: String,
    category: String,
    summary: String,
    syntax: String,
}

fn load_functions(root: &Path) -> Result<Vec<Function>, String> {
    let dir = root.join("crates/kiln-formula/docs");
    let mut paths: Vec<_> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let list: Vec<Value> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        for f in list {
            let field = |k: &str| f[k].as_str().unwrap_or_default().to_owned();
            out.push(Function {
                name: field("name"),
                category: field("category"),
                summary: field("summary"),
                syntax: field("syntax"),
            });
        }
    }
    out.sort_by(|a, b| (&a.category, &a.name).cmp(&(&b.category, &b.name)));
    Ok(out)
}

fn suite_data(root: &Path, package: &str, functions: &[Function]) -> Result<Value, String> {
    let package: Value = serde_json::from_str(package).map_err(|e| e.to_string())?;
    let mut templates = 0_u64;
    for app in ["write", "grid", "deck"] {
        for entry in fs::read_dir(root.join("assets/templates").join(app))
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let ext = entry.path();
            if ext.extension().is_some_and(|x| x != "png") {
                templates += 1;
            }
        }
    }
    Ok(json!({
        "version": package["version"],
        "functions": functions.len(),
        "templates": templates,
        "interfaceLanguages": 3,
    }))
}

/// The value of every `data-stat` name the page and the README may use.
fn stats(perf: &Value, suite: &Value) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let count = |key: &str| suite[key].as_u64().unwrap_or(0).to_string();
    map.insert("functions".to_owned(), count("functions"));
    map.insert("templates".to_owned(), count("templates"));
    map.insert("languages".to_owned(), count("interfaceLanguages"));
    map.insert(
        "perf.web".to_owned(),
        ms2(perf["webOpenMs"].as_f64().unwrap_or(0.0)),
    );
    map
}

/// `2.75 s`: the web figures, which carry a hundredth.
fn ms2(v: f64) -> String {
    format!("{:.2} s", v / 1000.0)
}

fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The links to a cell's sources; a comparison cell without one is an error.
fn source_links(cell: &Value, label: &str) -> Result<String, String> {
    let urls = cell["urls"]
        .as_array()
        .filter(|u| !u.is_empty())
        .ok_or_else(|| format!("compare-facts.json: {label} has no source"))?;
    let links: Vec<String> = urls
        .iter()
        .enumerate()
        .map(|(i, url)| {
            let word = if urls.len() == 1 {
                "source".to_owned()
            } else if i == 0 {
                "source 1".to_owned()
            } else {
                (i + 1).to_string()
            };
            format!(
                "<a class=\"src\" href=\"{}\" aria-label=\"Source {} for {}\">{word}</a>",
                esc(url.as_str().unwrap_or_default()),
                i + 1,
                esc(label),
            )
        })
        .collect();
    Ok(links.join(" "))
}

fn compare_html(facts: &Value, measured: Option<&Value>) -> Result<String, String> {
    let products = facts["products"]
        .as_array()
        .ok_or("compare-facts.json: no products")?;
    let rows = facts["rows"]
        .as_array()
        .ok_or("compare-facts.json: no rows")?;
    let read = facts["read"].as_str().unwrap_or_default();
    let mut out = format!(
        "<div class=\"tablewrap compare\" tabindex=\"0\" role=\"region\" aria-label=\"Comparison \
         table\"><table><caption>From each product's own pages, read on {}. Every cell links its \
         source.</caption>\n<thead><tr><th scope=\"col\"><span class=\"vh\">Fact</span></th>",
        esc(read)
    );
    for p in products {
        let _ = write!(
            out,
            "<th scope=\"col\"{}>{}</th>",
            if p["id"] == "kiln" {
                " class=\"us\""
            } else {
                ""
            },
            esc(p["name"].as_str().unwrap_or_default()),
        );
    }
    out.push_str("</tr></thead>\n<tbody>\n");
    for row in rows {
        let id = row["id"].as_str().unwrap_or_default();
        let name = row["label"].as_str().unwrap_or_default();
        let _ = write!(out, "<tr><th scope=\"row\">{}</th>", esc(name));
        for p in products {
            let cell = &p["cells"][id];
            let label = format!("{}, {name}", p["name"].as_str().unwrap_or_default());
            let text = cell["text"]
                .as_str()
                .filter(|c| !c.is_empty())
                .ok_or_else(|| format!("compare-facts.json: {label} is empty"))?;
            let _ = write!(
                out,
                "<td{}>{} {}</td>",
                if p["id"] == "kiln" {
                    " class=\"us\""
                } else {
                    ""
                },
                esc(text),
                source_links(cell, &label)?,
            );
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody></table></div>\n");
    if let Some(m) = measured {
        out.push_str(&measured_html(m, products)?);
    }
    Ok(out)
}

/// The head-to-head table from `docs/qa/compare.json`: a product is a column only when it has a
/// value in every row, so the table never shows an empty cell. Schema: `{"date", "method",
/// "rows": [{"label", "unit", "values": {"<product id>": number}}]}`.
fn measured_html(measured: &Value, products: &[Value]) -> Result<String, String> {
    let rows = measured["rows"].as_array().ok_or("compare.json: no rows")?;
    let columns: Vec<&Value> = products
        .iter()
        .filter(|p| {
            let id = p["id"].as_str().unwrap_or_default();
            rows.iter().all(|r| r["values"][id].is_number())
        })
        .collect();
    if rows.is_empty() || columns.len() < 2 {
        return Ok(String::new());
    }
    let mut out = format!(
        "<div class=\"tablewrap compare\" tabindex=\"0\" role=\"region\" aria-label=\"Measured \
         comparison\"><table><caption>Measured on one machine, {}. {}</caption>\n<thead><tr>\
         <th scope=\"col\"><span class=\"vh\">Measure</span></th>",
        esc(measured["date"].as_str().unwrap_or_default()),
        esc(measured["method"].as_str().unwrap_or_default()),
    );
    for p in &columns {
        let _ = write!(
            out,
            "<th scope=\"col\">{}</th>",
            esc(p["name"].as_str().unwrap_or_default())
        );
    }
    out.push_str("</tr></thead>\n<tbody>\n");
    for r in rows {
        let unit = r["unit"].as_str().unwrap_or_default();
        let _ = write!(
            out,
            "<tr><th scope=\"row\">{}</th>",
            esc(r["label"].as_str().unwrap_or_default())
        );
        for p in &columns {
            let id = p["id"].as_str().unwrap_or_default();
            let _ = write!(out, "<td>{} {}</td>", r["values"][id], esc(unit));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody></table></div>\n");
    Ok(out)
}

/// Replaces what stands between `<!-- site:NAME -->` and `<!-- /site:NAME -->`.
fn fill_block(page: &str, name: &str, html: &str) -> Result<String, String> {
    let open = format!("<!-- site:{name} -->");
    let close = format!("<!-- /site:{name} -->");
    let start = page
        .find(&open)
        .ok_or_else(|| format!("{INDEX}: no {open}"))?
        + open.len();
    let end = start
        + page[start..]
            .find(&close)
            .ok_or_else(|| format!("{INDEX}: no {close}"))?;
    Ok(format!("{}\n{html}{}", &page[..start], &page[end..]))
}

/// Writes the text of every `data-stat="NAME"` element; a name with no value is an error.
fn fill_stats(file: &str, page: &str, stats: &BTreeMap<String, String>) -> Result<String, String> {
    let re = Regex::new(r#"(data-stat="([^"]+)"[^>]*>)[^<]*"#).map_err(|e| e.to_string())?;
    let mut missing = None;
    let out = re.replace_all(page, |c: &regex::Captures<'_>| {
        if let Some(v) = stats.get(&c[2]) {
            format!("{}{}", &c[1], esc(v))
        } else {
            missing = Some(c[2].to_owned());
            c[0].to_owned()
        }
    });
    match missing {
        Some(name) => Err(format!("{file}: no value for data-stat=\"{name}\"")),
        None => Ok(out.into_owned()),
    }
}

fn functions_md(functions: &[Function]) -> String {
    let mut out = format!(
        "# Function list\n\n<!-- Generated by `cargo xtask site` from crates/kiln-formula/docs/. \
         Do not edit by hand. -->\n\nKiln Grid has {} worksheet functions, spelled as Excel spells \
         them. **Formulas ▸ Insert Function** searches this list and shows each function's \
         arguments and examples. See [Formulas in Kiln Grid](grid-formulas.md).\n",
        functions.len()
    );
    let mut category = "";
    for f in functions {
        if f.category != category {
            category = &f.category;
            let _ = write!(
                out,
                "\n## {category}\n\n| Function | What it does |\n|---|---|\n"
            );
        }
        let syntax = f.syntax.replace('|', "\\|");
        let _ = writeln!(out, "| `{syntax}` | {} |", f.summary.replace('|', "\\|"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_web_figure() {
        let md = "Measured by `cargo xtask budgets` on 2026-11-03 (release build, linux x86_64, \
            16 CPUs).\n\n| Measure | Result |\n|---|---|\n\
            | Web core: open the 1M-cell .xlsx to first paint | 2,760 ms |\n";
        let perf = parse_perf(md).unwrap();
        assert_eq!(perf["date"], "2026-11-03");
        assert_eq!(perf["webOpenMs"], 2760.0);
        let suite = json!({"functions": 505, "templates": 36, "interfaceLanguages": 3});
        let map = stats(&perf, &suite);
        assert_eq!(map["perf.web"], "2.76 s");
        assert_eq!(map["functions"], "505");
        assert!(parse_perf("no block").is_err());
    }

    #[test]
    fn reads_times_in_either_unit() {
        assert_eq!(millis("< 400 ms"), Some(400.0));
        assert_eq!(millis("32.7 ms (p95)"), Some(32.7));
        assert_eq!(millis("< 2 s"), Some(2000.0));
        assert_eq!(millis("2,760 ms"), Some(2760.0));
        assert_eq!(ms2(2760.0), "2.76 s");
        assert_eq!(ms2(980.0), "0.98 s");
    }

    #[test]
    fn a_block_is_replaced_and_a_missing_one_is_an_error() {
        let page = "a<!-- site:x -->old<!-- /site:x -->b";
        assert_eq!(
            fill_block(page, "x", "new").unwrap(),
            "a<!-- site:x -->\nnew<!-- /site:x -->b"
        );
        assert!(fill_block(page, "y", "new").is_err());
    }

    #[test]
    fn a_stat_without_a_value_is_an_error() {
        let mut map = BTreeMap::new();
        map.insert("a".to_owned(), "1 < 2".to_owned());
        assert_eq!(
            fill_stats("t", "<b data-stat=\"a\">0</b>", &map).unwrap(),
            "<b data-stat=\"a\">1 &lt; 2</b>"
        );
        assert!(fill_stats("t", "<b data-stat=\"b\">0</b>", &map).is_err());
    }

    #[test]
    fn the_measured_table_drops_a_product_with_a_gap() {
        let products = vec![
            json!({"id": "kiln", "name": "Kiln"}),
            json!({"id": "a", "name": "A"}),
            json!({"id": "b", "name": "B"}),
        ];
        let m = json!({"date": "2026-12", "method": "m", "rows": [
            {"label": "Start", "unit": "ms", "values": {"kiln": 1, "a": 2}},
            {"label": "Open", "unit": "ms", "values": {"kiln": 3, "a": 4, "b": 5}},
        ]});
        let html = measured_html(&m, &products).unwrap();
        assert!(html.contains(">A<") && !html.contains(">B<"));
        let none = json!({"rows": [{"label": "x", "values": {"kiln": 1}}]});
        assert_eq!(measured_html(&none, &products).unwrap(), "");
    }

    #[test]
    fn a_comparison_cell_cannot_be_left_empty() {
        let empty = json!({"read": "2026-11-01", "rows": [{"id": "p", "label": "Price"}],
            "products": [{"id": "kiln", "name": "Kiln", "cells": {}}]});
        assert!(compare_html(&empty, None).is_err());
        let unsourced = json!({"read": "2026-11-01", "rows": [{"id": "p", "label": "Price"}],
            "products": [{"id": "kiln", "name": "Kiln", "cells": {"p": {"text": "Free", "urls": []}}}]});
        assert!(compare_html(&unsourced, None).is_err());
    }
}
