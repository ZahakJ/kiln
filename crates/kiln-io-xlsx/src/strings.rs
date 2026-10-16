// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared and inline strings: `<si>`/`<is>` items ↔ cell values (plain text or rich text with
//! runs of character formatting), and the shared string table the writer builds.

use std::sync::Arc;

use kiln_ooxml::xml::namespaces::local_name;
use kiln_ooxml::xml::{XmlElement, XmlNode};
use kiln_sheet::CellValue;
use kiln_sheet::value::{RichText, TextRun};
use rustc_hash::FxHashMap;

use crate::styles::{font_patch_el, parse_font_patch};
use crate::text::{decode_x_escapes, encode_x_escapes};
use crate::xmlx::text_el;

/// A string item (`si` of the shared strings, or a cell's `is`) as a cell value. Phonetic runs
/// (`rPh`) and properties are not part of the value.
pub(crate) fn parse_string_item(e: &XmlElement) -> CellValue {
    let mut runs: Vec<TextRun> = Vec::new();
    let mut plain = String::new();
    let mut rich = false;
    for c in e.elements() {
        match local_name(&c.name) {
            "t" => plain.push_str(&decode_x_escapes(&c.text())),
            "r" => {
                rich = true;
                if !plain.is_empty() {
                    runs.push(TextRun {
                        text: std::mem::take(&mut plain),
                        font: None,
                    });
                }
                let text: String = c
                    .elements()
                    .filter(|t| local_name(&t.name) == "t")
                    .map(|t| decode_x_escapes(&t.text()).into_owned())
                    .collect();
                let font = c
                    .elements()
                    .find(|p| local_name(&p.name) == "rPr")
                    .map(parse_font_patch)
                    .filter(|p| *p != kiln_sheet::styles::FontPatch::default());
                runs.push(TextRun { text, font });
            }
            _ => {}
        }
    }
    if !rich {
        return CellValue::Text(Arc::from(plain));
    }
    if !plain.is_empty() {
        runs.push(TextRun {
            text: plain,
            font: None,
        });
    }
    CellValue::RichText(Arc::new(RichText::new(runs)))
}

/// A string item element (`si` or `is`) for text or rich text.
pub(crate) fn string_item_el(name: &str, v: &CellValue) -> XmlElement {
    let mut e = XmlElement::new(name);
    match v {
        CellValue::RichText(r) => {
            for run in r.runs() {
                let mut re = XmlElement::new("r");
                if let Some(f) = &run.font {
                    re.children
                        .push(XmlNode::Element(font_patch_el(f, "rPr", true)));
                }
                re.children
                    .push(XmlNode::Element(text_el("t", &encode_x_escapes(&run.text))));
                e.children.push(XmlNode::Element(re));
            }
        }
        other => {
            let t = other.as_text().unwrap_or_default();
            e.children
                .push(XmlNode::Element(text_el("t", &encode_x_escapes(t))));
        }
    }
    e
}

/// The shared string table being built by the writer: each distinct string (rich text by its
/// runs) once, in order of first use.
#[derive(Debug, Default)]
pub(crate) struct SstBuilder {
    plain: FxHashMap<Arc<str>, u32>,
    rich: FxHashMap<Arc<RichText>, u32>,
    pub(crate) items: Vec<CellValue>,
    /// Total references (the `count` attribute).
    pub(crate) uses: u64,
}

impl SstBuilder {
    /// The index of a text value, adding it when new.
    pub(crate) fn index(&mut self, v: &CellValue) -> u32 {
        self.uses += 1;
        match v {
            CellValue::RichText(r) => {
                if let Some(&i) = self.rich.get(r) {
                    return i;
                }
                let i = self.items.len() as u32;
                self.rich.insert(r.clone(), i);
                self.items.push(v.clone());
                i
            }
            CellValue::Text(s) => {
                if let Some(&i) = self.plain.get(s) {
                    return i;
                }
                let i = self.items.len() as u32;
                self.plain.insert(s.clone(), i);
                self.items.push(v.clone());
                i
            }
            _ => 0,
        }
    }

    /// Serialises the table as `sharedStrings.xml` text.
    pub(crate) fn to_xml(&self) -> String {
        let mut out = String::with_capacity(64 + self.items.len() * 24);
        out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n");
        out.push_str(
            "<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"",
        );
        out.push_str(&self.uses.to_string());
        out.push_str("\" uniqueCount=\"");
        out.push_str(&self.items.len().to_string());
        out.push_str("\">");
        for v in &self.items {
            match v {
                CellValue::Text(s) => {
                    let enc = encode_x_escapes(s);
                    if enc.starts_with(char::is_whitespace) || enc.ends_with(char::is_whitespace) {
                        out.push_str("<si><t xml:space=\"preserve\">");
                    } else {
                        out.push_str("<si><t>");
                    }
                    out.push_str(&kiln_ooxml::xml::escape_text(&enc));
                    out.push_str("</t></si>");
                }
                other => {
                    let el = string_item_el("si", other);
                    let opts = kiln_ooxml::xml::WriteOptions {
                        declaration: kiln_ooxml::xml::Declaration::Omit,
                        declare_namespaces: false,
                    };
                    if let Ok(s) = kiln_ooxml::xml::write_element(&el, &opts) {
                        out.push_str(&s);
                    }
                }
            }
        }
        out.push_str("</sst>");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::xml::parse_xml_str;
    use kiln_sheet::styles::FontPatch;

    #[test]
    fn rich_and_plain_items_round_trip() {
        let rich = CellValue::RichText(Arc::new(RichText::new(vec![
            TextRun {
                text: "Plain, ".into(),
                font: None,
            },
            TextRun {
                text: "bold\r".into(),
                font: Some(FontPatch {
                    bold: Some(true),
                    name: Some(Arc::from("Courier New")),
                    ..FontPatch::default()
                }),
            },
        ])));
        for v in [
            CellValue::text("  spaced "),
            CellValue::text("_x0041_"),
            rich,
        ] {
            let el = string_item_el("si", &v);
            let xml =
                kiln_ooxml::xml::write_element(&el, &kiln_ooxml::xml::WriteOptions::fragment())
                    .expect("xml");
            let back = parse_xml_str(&xml).expect("parse").root;
            assert_eq!(parse_string_item(&back), v, "{xml}");
        }
    }

    #[test]
    fn the_table_deduplicates_and_counts() {
        let mut b = SstBuilder::default();
        assert_eq!(b.index(&CellValue::text("a")), 0);
        assert_eq!(b.index(&CellValue::text("b")), 1);
        assert_eq!(b.index(&CellValue::text("a")), 0);
        let xml = b.to_xml();
        assert!(xml.contains("count=\"3\" uniqueCount=\"2\""), "{xml}");
        let doc = parse_xml_str(&xml).expect("sst");
        let items: Vec<CellValue> = doc
            .root
            .children_named("si")
            .map(parse_string_item)
            .collect();
        assert_eq!(items, vec![CellValue::text("a"), CellValue::text("b")]);
    }
}
