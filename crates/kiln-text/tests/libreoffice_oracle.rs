// SPDX-License-Identifier: AGPL-3.0-or-later
//! Line breaks of single-paragraph documents compared with LibreOffice.
//!
//! `fixtures/docx/text/` holds ten `.docx` files written by `make_fixtures.py`, each with a JSON
//! description of the same paragraph, and `oracle.json` with LibreOffice's rendering of every file
//! (via `tools/oracle-soffice.sh` → PDF → `pdftotext -layout`). This test lays the JSON paragraphs out
//! with kiln-text and requires the same number of lines and the same text on every line.
//!
//! Set `KILN_ORACLE_LIVE=1` to re-render the `.docx` files with LibreOffice during the test instead of
//! trusting the recorded `oracle.json` (needs `soffice` and `pdftotext`).

mod common;

use std::path::{Path, PathBuf};

use kiln_text::{Alignment, ParagraphLayout, ParagraphSpec, SpanStyle, TextRun};
use serde_json::Value;

const NAMES: [&str; 10] = [
    "carlito_left_11pt",
    "caladea_justify_12pt",
    "times_left_12pt_narrow",
    "arial_center_10pt",
    "courier_right_10pt",
    "carlito_first_line_indent",
    "caladea_hanging_indent",
    "liberation_sans_both_indents",
    "carlito_mixed_runs",
    "times_16pt_justify",
];

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/docx/text")
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn pt(v: Option<&Value>) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    v.and_then(Value::as_f64)
        .map_or(0.0, |v| v as f32 * 4.0 / 3.0)
}

/// The paragraph described by a fixture's JSON, styled as Word would lay it out (no kerning: Word
/// only kerns when a run asks for it).
fn spec_of(fx: &Value) -> ParagraphSpec {
    let font = fx["font"].as_str().expect("font");
    let size = fx["size_pt"].as_f64().expect("size");
    let runs = fx["runs"]
        .as_array()
        .expect("runs")
        .iter()
        .map(|r| {
            #[allow(clippy::cast_possible_truncation)]
            let size_pt = r.get("size_pt").and_then(Value::as_f64).unwrap_or(size) as f32;
            let mut style = SpanStyle::new(
                r.get("font").and_then(Value::as_str).unwrap_or(font),
                size_pt * 4.0 / 3.0,
            );
            style.weight = if r.get("bold").and_then(Value::as_bool).unwrap_or(false) {
                700
            } else {
                400
            };
            style.italic = r.get("italic").and_then(Value::as_bool).unwrap_or(false);
            style.kerning = false;
            TextRun::new(r["text"].as_str().expect("text"), style)
        })
        .collect();
    let mut spec = ParagraphSpec {
        runs,
        width: pt(fx.get("width_pt")),
        ..ParagraphSpec::default()
    };
    spec.props.alignment = match fx["alignment"].as_str() {
        Some("center") => Alignment::Center,
        Some("right") => Alignment::Right,
        Some("justify") => Alignment::Justify,
        _ => Alignment::Left,
    };
    spec.props.indent_start = pt(fx.get("left_pt"));
    spec.props.indent_end = pt(fx.get("right_pt"));
    spec.props.first_line = pt(fx.get("first_line_pt"));
    spec
}

fn normalise(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn kiln_lines(layout: &ParagraphLayout) -> Vec<String> {
    layout
        .lines
        .iter()
        .map(|l| normalise(&layout.text()[l.text_range.clone()]))
        .collect()
}

/// LibreOffice's lines for a fixture: recorded, or rendered now when `KILN_ORACLE_LIVE` is set.
fn oracle_lines(name: &str, recorded: &Value) -> Vec<String> {
    if std::env::var_os("KILN_ORACLE_LIVE").is_some() {
        return render_live(name);
    }
    let entry = &recorded[name];
    let lines: Vec<String> = entry["lines"]
        .as_array()
        .expect("oracle lines")
        .iter()
        .map(|v| v.as_str().expect("line").to_owned())
        .collect();
    assert_eq!(
        entry["line_count"].as_u64(),
        Some(lines.len() as u64),
        "oracle.json is inconsistent for {name}"
    );
    lines
}

fn render_live(name: &str) -> Vec<String> {
    let out = std::env::temp_dir().join(format!("kiln-text-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&out).expect("temp dir");
    let docx = dir().join(format!("{name}.docx"));
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/oracle-soffice.sh");
    let status = std::process::Command::new(script)
        .arg(&docx)
        .arg("pdf")
        .arg(&out)
        .output()
        .expect("run tools/oracle-soffice.sh");
    assert!(status.status.success(), "LibreOffice failed for {name}");
    let pdf = out.join(format!("{name}.pdf"));
    let text = std::process::Command::new("pdftotext")
        .arg("-layout")
        .arg(&pdf)
        .arg("-")
        .output()
        .expect("run pdftotext");
    let _ = std::fs::remove_dir_all(&out);
    String::from_utf8_lossy(&text.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(normalise)
        .collect()
}

#[test]
fn line_breaks_match_libreoffice_for_all_ten_fixtures() {
    let recorded = read_json(&dir().join("oracle.json"));
    let mut engine = common::engine();
    let mut failures = Vec::new();
    for name in NAMES {
        let fx = read_json(&dir().join(format!("{name}.json")));
        assert!(
            dir().join(format!("{name}.docx")).is_file(),
            "{name}.docx missing"
        );
        let layout = engine.layout(&spec_of(&fx));
        let ours = kiln_lines(&layout);
        let theirs = oracle_lines(name, &recorded);
        if ours.len() != theirs.len() {
            failures.push(format!(
                "{name}: kiln {} lines, LibreOffice {} lines\n  kiln: {ours:#?}\n  lo:   {theirs:#?}",
                ours.len(),
                theirs.len()
            ));
        } else if ours != theirs {
            failures.push(format!(
                "{name}: same line count, different breaks\n  kiln: {ours:#?}\n  lo:   {theirs:#?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
