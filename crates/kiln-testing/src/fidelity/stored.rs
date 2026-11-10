// SPDX-License-Identifier: AGPL-3.0-or-later
//! What a file itself says, read from its XML without any tool's model: the values a workbook
//! stores and the elements a package holds. Kiln's reader and engine stay out of it, so these
//! scores can see what they would miss and treat every tool's output alike.

use std::collections::{BTreeMap, HashMap};

use kiln_ooxml::opc::{OpcPackage, rel_type};
use kiln_ooxml::xml::namespaces::local_name;
use kiln_ooxml::xml::{XmlElement, parse_xml};

fn local(e: &XmlElement) -> &str {
    local_name(&e.name)
}

fn children<'a>(e: &'a XmlElement, name: &'a str) -> impl Iterator<Item = &'a XmlElement> {
    e.elements().filter(move |c| local(c) == name)
}

fn child<'a>(e: &'a XmlElement, name: &str) -> Option<&'a XmlElement> {
    e.elements().find(|c| local(c) == name)
}

/// The text of a string item: its `t` elements, phonetic runs left out.
fn item_text(e: &XmlElement) -> String {
    let mut out = String::new();
    for c in e.elements() {
        match local(c) {
            "t" => out.push_str(&c.text()),
            "r" => out.push_str(&item_text(c)),
            _ => {}
        }
    }
    out
}

fn shared_strings(pkg: &OpcPackage, workbook: &str) -> Vec<String> {
    let rels = pkg.relationships(workbook).ok();
    let Some(part) = rels.as_ref().and_then(|r| {
        r.first_by_type(rel_type::SHARED_STRINGS)
            .and_then(|x| r.resolve(x))
    }) else {
        return Vec::new();
    };
    let Ok(bytes) = pkg.part_bytes(&part) else {
        return Vec::new();
    };
    parse_xml(&bytes).map_or_else(
        |_| Vec::new(),
        |d| {
            d.root
                .elements()
                .filter(|e| matches!(local(e), "si" | "sstItem"))
                .map(item_text)
                .collect()
        },
    )
}

/// A number in one form; anything else as it stands.
fn canonical(v: &str) -> String {
    v.trim()
        .parse::<f64>()
        .map_or_else(|_| v.to_owned(), |n| format!("{n}"))
}

fn cell_value(c: &XmlElement, strings: &[String]) -> Option<String> {
    let t = c.attr("t").unwrap_or("n");
    if t == "inlineStr" {
        return child(c, "is").map(item_text);
    }
    let v = child(c, "v")?.text();
    Some(match t {
        "s" => strings
            .get(v.trim().parse::<usize>().ok()?)
            .cloned()
            .unwrap_or_default(),
        "b" => if v.trim() == "1" { "TRUE" } else { "FALSE" }.to_owned(),
        "str" | "e" => v,
        _ => canonical(&v),
    })
}

/// The values a workbook stores, keyed `Sheet!A1`: what the file holds, not what anything would
/// calculate from it. `None` for a package with no readable workbook part.
#[must_use]
pub fn stored_cells(bytes: &[u8]) -> Option<BTreeMap<String, String>> {
    let pkg = OpcPackage::open(bytes).ok()?;
    let workbook = pkg.main_document_part().ok()??;
    let root = parse_xml(&pkg.part_bytes(&workbook).ok()?).ok()?.root;
    let strings = shared_strings(&pkg, &workbook);
    let rels = pkg.relationships(&workbook).ok()?;
    let mut out = BTreeMap::new();
    for sheet in child(&root, "sheets")
        .into_iter()
        .flat_map(|s| children(s, "sheet"))
    {
        let name = sheet.attr("name").unwrap_or_default();
        let Some(part) = sheet
            .attr("r:id")
            .and_then(|id| rels.get(id))
            .and_then(|r| rels.resolve(r))
        else {
            continue;
        };
        let Some(xml) = pkg.part_bytes(&part).ok().and_then(|b| parse_xml(&b).ok()) else {
            continue;
        };
        for row in child(&xml.root, "sheetData")
            .into_iter()
            .flat_map(|d| children(d, "row"))
        {
            for c in children(row, "c") {
                if let (Some(r), Some(v)) = (c.attr("r"), cell_value(c, &strings))
                    && !v.is_empty()
                {
                    out.insert(format!("{name}!{r}"), v);
                }
            }
        }
    }
    Some(out)
}

/// Elements that say nothing by themselves, or whose number changes when equal neighbours merge:
/// run wrappers and text, the formatting of a run, editors' marks, empty containers.
const NOT_CONTENT: &[&str] = &[
    "r",
    "t",
    "rPr",
    "pPr",
    "is",
    "si",
    "v",
    "c",
    "row",
    "proofErr",
    "lastRenderedPageBreak",
    "rFonts",
    "b",
    "bCs",
    "i",
    "iCs",
    "sz",
    "szCs",
    "color",
    "lang",
    "u",
    "noProof",
    "rStyle",
    "latin",
    "ea",
    "cs",
    "defRPr",
    "solidFill",
    "srgbClr",
    "schemeClr",
    "sysClr",
    "prstClr",
    "lumMod",
    "lumOff",
    "tint",
    "shade",
    "alpha",
    "ext",
    "extLst",
];

fn element_counts(pkg: &OpcPackage) -> HashMap<String, u64> {
    let mut counts: HashMap<String, u64> = HashMap::new();
    for name in pkg.all_part_names() {
        let lower = name.to_ascii_lowercase();
        if !lower.ends_with(".xml") || lower.starts_with("/docprops/") {
            continue;
        }
        let Ok(bytes) = pkg.part_bytes(name) else {
            continue;
        };
        let mut rest: &[u8] = &bytes;
        while let Some(i) = rest.iter().position(|b| *b == b'<') {
            rest = &rest[i + 1..];
            let end = rest
                .iter()
                .position(|b| {
                    !(b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-' | b'.'))
                })
                .unwrap_or(rest.len());
            if end == 0 {
                continue;
            }
            let tag = String::from_utf8_lossy(&rest[..end]);
            let tag = local_name(&tag);
            if !NOT_CONTENT.contains(&tag) {
                *counts.entry(tag.to_owned()).or_default() += 1;
            }
        }
    }
    counts
}

/// How much of a package's markup is still there in the output: for each kind of element the
/// original holds, the share of its occurrences the output has, averaged over the kinds. Compares
/// raw XML, so it sees what a tool's model does not hold. Added elements do not count.
#[must_use]
pub fn raw_elements(original: &OpcPackage, output: &OpcPackage) -> f64 {
    let (a, b) = (element_counts(original), element_counts(output));
    if a.is_empty() {
        return 1.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let sum: f64 = a
        .iter()
        .map(|(tag, n)| {
            let m = b.get(tag).copied().unwrap_or(0);
            (m.min(*n)) as f64 / *n as f64
        })
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let r = sum / a.len() as f64;
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(rel: &str) -> Vec<u8> {
        std::fs::read(crate::fixtures::fixtures_dir().join(rel)).expect("fixture")
    }

    #[test]
    fn stored_values_come_from_the_file() {
        let cells = stored_cells(&fixture("xlsx/manual/minimal.xlsx")).unwrap();
        assert_eq!(cells.get("Only!A1").map(String::as_str), Some("Total"));
        assert_eq!(cells.get("Only!D1").map(String::as_str), Some("42"));
        // The 2006 preview format's strings too.
        let beta = stored_cells(&fixture("xlsx/manual/beta-strings.xlsx")).unwrap();
        assert_eq!(beta.get("Only!A1").map(String::as_str), Some("Lorem"));
    }

    #[test]
    fn raw_elements_see_lost_markup_and_ignore_merged_runs() {
        let open = |x: &str| {
            let mut pkg = OpcPackage::open(&fixture("docx/manual/minimal.docx")).expect("package");
            pkg.set_part("/word/document.xml", x.as_bytes().to_vec())
                .expect("part");
            OpcPackage::open(&pkg.save().expect("save")).expect("package")
        };
        let a = open(
            "<w:d><w:p><w:r><w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r><w:ruby/><w:ruby/></w:p></w:d>",
        );
        let merged = open("<w:d><w:p><w:r><w:t>ab</w:t></w:r><w:ruby/><w:ruby/></w:p></w:d>");
        let lost = open("<w:d><w:p><w:r><w:t>ab</w:t></w:r><w:ruby/></w:p></w:d>");
        assert!((raw_elements(&a, &merged) - 1.0).abs() < 1e-12);
        assert!(raw_elements(&a, &lost) < 1.0);
    }
}
