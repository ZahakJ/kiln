// SPDX-License-Identifier: AGPL-3.0-or-later
//! Round trip of every chart part in the fixture corpus: read → write → read must keep every
//! modelled fact, and the written XML must keep ≥ 97 % of the original part's structure
//! (elements, attributes and text, numbers compared by value).

use std::path::{Path, PathBuf};

use kiln_charts::facts::{chart_facts, overlap};
use kiln_charts::{read_chart, write_chart};
use kiln_ooxml::opc::OpcPackage;
use kiln_ooxml::xml::{XmlElement, XmlNode, format_number, parse_xml};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every chart part of every package under the fixture directories.
fn chart_parts() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for dir in [
        "fixtures/xlsx/features",
        "fixtures/xlsx/charts",
        "fixtures/pptx/features",
        "fixtures/docx/features",
    ] {
        let Ok(rd) = std::fs::read_dir(repo().join(dir)) else {
            continue;
        };
        let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        files.sort();
        for f in files {
            let Ok(bytes) = std::fs::read(&f) else {
                continue;
            };
            let Ok(pkg) = OpcPackage::open(&bytes) else {
                continue;
            };
            let names: Vec<String> = pkg.part_names().map(str::to_owned).collect();
            for name in names {
                let lower = name.to_ascii_lowercase();
                let is_chart = lower.contains("/charts/")
                    && lower.ends_with(".xml")
                    && !lower.contains("/_rels/")
                    && (lower.contains("/chart")
                        && !lower.contains("/style")
                        && !lower.contains("/colors"));
                if !is_chart {
                    continue;
                }
                let part = pkg.part_bytes(&name).expect("part").into_owned();
                out.push((
                    format!("{}:{name}", f.file_name().unwrap().to_string_lossy()),
                    part,
                ));
            }
        }
    }
    out
}

fn norm_text(t: &str) -> String {
    match t.trim().parse::<f64>() {
        Ok(v) if v.is_finite() => format_number(v),
        _ => t.to_owned(),
    }
}

fn xml_facts(el: &XmlElement, path: &str, out: &mut Vec<String>) {
    let here = format!("{path}/{}", el.name);
    out.push(here.clone());
    for (k, v) in el.attrs.iter() {
        if k.starts_with("xmlns") || k.starts_with("mc:Ignorable") {
            continue;
        }
        out.push(format!("{here}@{k}={}", norm_text(v)));
    }
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for c in &el.children {
        match c {
            XmlNode::Element(e) => {
                let n = seen.entry(e.name.as_str()).or_default();
                xml_facts(e, &format!("{here}[{n}]"), out);
                *n += 1;
            }
            XmlNode::Text(t) if !t.text.trim().is_empty() => {
                out.push(format!("{here}#{}", norm_text(&t.text)));
            }
            _ => {}
        }
    }
}

#[test]
fn every_fixture_chart_round_trips() {
    let parts = chart_parts();
    assert!(parts.len() >= 18, "found only {} chart parts", parts.len());
    let mut report = Vec::new();
    for (name, bytes) in &parts {
        let cs = read_chart(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let written = write_chart(&cs);
        let again = read_chart(&written).unwrap_or_else(|e| panic!("{name} (written): {e}"));
        let (fa, fb) = (chart_facts(&cs), chart_facts(&again));
        let model = overlap(&fa, &fb);
        if (model - 1.0).abs() > 1e-12 {
            let lost: Vec<&String> = fa.iter().filter(|f| !fb.contains(f)).take(10).collect();
            let added: Vec<&String> = fb.iter().filter(|f| !fa.contains(f)).take(10).collect();
            panic!("{name}: model facts changed ({model})\nlost: {lost:#?}\nadded: {added:#?}");
        }
        let mut a = Vec::new();
        let mut b = Vec::new();
        xml_facts(&parse_xml(bytes).unwrap().root, "", &mut a);
        xml_facts(&parse_xml(&written).unwrap().root, "", &mut b);
        let structural = overlap(&a, &b);
        report.push(format!("{name}: {structural:.4}"));
        if structural < 0.97 {
            let lost: Vec<&String> = a.iter().filter(|f| !b.contains(f)).take(20).collect();
            let added: Vec<&String> = b.iter().filter(|f| !a.contains(f)).take(20).collect();
            panic!("{name}: structural {structural:.4} < 0.97\nlost: {lost:#?}\nadded: {added:#?}");
        }
    }
    eprintln!("{}", report.join("\n"));
}

/// The children `CT_*Ser` allows, in schema order (ECMA-376 Part 1, dml-chart.xsd), for the
/// element that holds the series.
fn series_schema(group: &str) -> Option<&'static [&'static str]> {
    Some(match group {
        "c:barChart" | "c:bar3DChart" => &[
            "c:idx",
            "c:order",
            "c:tx",
            "c:spPr",
            "c:invertIfNegative",
            "c:pictureOptions",
            "c:dPt",
            "c:dLbls",
            "c:trendline",
            "c:errBars",
            "c:cat",
            "c:val",
            "c:shape",
            "c:extLst",
        ],
        "c:lineChart" | "c:line3DChart" => &[
            "c:idx",
            "c:order",
            "c:tx",
            "c:spPr",
            "c:marker",
            "c:dPt",
            "c:dLbls",
            "c:trendline",
            "c:errBars",
            "c:cat",
            "c:val",
            "c:smooth",
            "c:extLst",
        ],
        "c:areaChart" | "c:area3DChart" => &[
            "c:idx",
            "c:order",
            "c:tx",
            "c:spPr",
            "c:pictureOptions",
            "c:dPt",
            "c:dLbls",
            "c:trendline",
            "c:errBars",
            "c:cat",
            "c:val",
            "c:extLst",
        ],
        "c:pieChart" | "c:pie3DChart" | "c:doughnutChart" | "c:ofPieChart" => &[
            "c:idx",
            "c:order",
            "c:tx",
            "c:spPr",
            "c:explosion",
            "c:dPt",
            "c:dLbls",
            "c:cat",
            "c:val",
            "c:extLst",
        ],
        "c:scatterChart" => &[
            "c:idx",
            "c:order",
            "c:tx",
            "c:spPr",
            "c:marker",
            "c:dPt",
            "c:dLbls",
            "c:trendline",
            "c:errBars",
            "c:xVal",
            "c:yVal",
            "c:smooth",
            "c:extLst",
        ],
        "c:bubbleChart" => &[
            "c:idx",
            "c:order",
            "c:tx",
            "c:spPr",
            "c:invertIfNegative",
            "c:dPt",
            "c:dLbls",
            "c:trendline",
            "c:errBars",
            "c:xVal",
            "c:yVal",
            "c:bubbleSize",
            "c:bubble3D",
            "c:extLst",
        ],
        "c:radarChart" => &[
            "c:idx", "c:order", "c:tx", "c:spPr", "c:marker", "c:dPt", "c:dLbls", "c:cat", "c:val",
            "c:extLst",
        ],
        _ => return None,
    })
}

/// Children a series may repeat.
const REPEATABLE: &[&str] = &["c:dPt", "c:trendline", "c:errBars"];

/// Every `c:ser` under `el` has only children its chart type allows, each at most once unless
/// repeatable, in schema order.
fn check_series_schema(el: &XmlElement, what: &str) {
    for group in el.elements() {
        if let Some(schema) = series_schema(&group.name) {
            for ser in group.children_named("c:ser") {
                let mut last = 0usize;
                let mut prev: Option<&str> = None;
                for c in ser.elements() {
                    let name = c.name.as_str();
                    let Some(pos) = schema.iter().position(|n| *n == name) else {
                        panic!("{what}: {name} is not allowed in a {} series", group.name);
                    };
                    assert!(
                        pos >= last,
                        "{what}: {name} is out of schema order in a {} series",
                        group.name
                    );
                    assert!(
                        prev != Some(name) || REPEATABLE.contains(&name),
                        "{what}: {name} appears twice in a {} series",
                        group.name
                    );
                    last = pos;
                    prev = Some(name);
                }
            }
        }
        check_series_schema(group, what);
    }
}

/// Change Chart Type on every fixture chart to every type: the written part is schema-valid in
/// its series, and reading it back gives the type the user picked (fuzzer seed 1174 found a radar
/// series' kept `c:smooth` written next to the line chart's own, read back as Line with Markers).
#[test]
fn every_fixture_chart_changes_to_every_type() {
    use kiln_charts::edit::{HostContext, apply, kind_of};
    use kiln_protocol::commands::{ChartKind, SuiteCommand};
    let parts = chart_parts();
    let mut checked = 0usize;
    let mut wrong = Vec::new();
    for (name, bytes) in &parts {
        let original = read_chart(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let series: usize = original
            .chart
            .plot_area
            .groups
            .iter()
            .map(|g| g.series.len())
            .sum();
        for kind in ChartKind::ALL {
            let what = format!("{name} → {kind:?}");
            let mut cs = original.clone();
            let combo = matches!(
                kind,
                ChartKind::ComboColumnLine | ChartKind::ComboColumnLineSecondary
            );
            let result = apply(
                &mut cs,
                &SuiteCommand::ChartChangeType { kind },
                HostContext::default(),
            );
            if combo && series < 2 {
                assert!(
                    result.is_err(),
                    "{what}: a one-series combo must be refused"
                );
                assert_eq!(
                    cs, original,
                    "{what}: a refused change must not touch the chart"
                );
                continue;
            }
            result.unwrap_or_else(|e| panic!("{what}: {e}"));
            let written = write_chart(&cs);
            check_series_schema(&parse_xml(&written).unwrap().root, &what);
            let again = read_chart(&written).unwrap_or_else(|e| panic!("{what} (written): {e}"));
            let read_back = kind_of(&again);
            if read_back != Some(kind) {
                wrong.push(format!("{what}: read back as {read_back:?}"));
            }
            checked += 1;
        }
    }
    assert!(
        wrong.is_empty(),
        "{} wrong:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    assert!(
        checked >= parts.len() * 20,
        "only {checked} changes checked"
    );
}
