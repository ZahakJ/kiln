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
