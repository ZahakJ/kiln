// SPDX-License-Identifier: AGPL-3.0-or-later
//! `w:sectPr` → [`SectionProps`] (ECMA-376 Part 1 §17.6) plus the section extras.

use kiln_doc::section::{
    Column, Columns, DocGrid, LineNumbering, PageNumbering, Paper, SectionProps,
};
use kiln_doc::st::{
    HeaderFooterKind, LineNumberRestart, NumberFormat, Orientation, PageVAlign, SectionBreak,
};
use kiln_ooxml::xml::XmlElement;

use super::props::{Raw, parse_border};
use super::styles::parse_note_settings;
use crate::common::extras::{PageBorders, SectionExtras};
use crate::common::units::{parse_decimal, parse_on_off, parse_twips, parse_u32};
use crate::common::xml::{attr, val};

/// A parsed `w:sectPr`.
pub(crate) struct ParsedSection {
    pub props: SectionProps,
    pub extras: SectionExtras,
    /// Header references: kind → relationship id, in file order.
    pub headers: Vec<(HeaderFooterKind, String)>,
    /// Footer references.
    pub footers: Vec<(HeaderFooterKind, String)>,
}

fn hf_kind(v: Option<&str>) -> HeaderFooterKind {
    v.and_then(HeaderFooterKind::parse).unwrap_or_default()
}

fn parse_page_borders(node: &XmlElement) -> PageBorders {
    let s = |k: &str| attr(node, k).map(str::to_owned);
    let edge = |e: &str| node.child(e).and_then(parse_border);
    PageBorders {
        offset_from: s("w:offsetFrom"),
        display: s("w:display"),
        z_order: s("w:zOrder"),
        top: edge("w:top"),
        left: edge("w:left").or_else(|| edge("w:start")),
        bottom: edge("w:bottom"),
        right: edge("w:right").or_else(|| edge("w:end")),
    }
}

/// Parses a section's properties. Values the file omits take Word's defaults (Letter, 1"
/// margins, one column), as Word does for a bare `<w:sectPr/>`.
pub(crate) fn parse_section(node: Option<&XmlElement>, raw: Raw<'_>) -> ParsedSection {
    let mut props = SectionProps::new(Paper::Letter);
    props.doc_grid = None;
    let mut out = ParsedSection {
        props,
        extras: SectionExtras::default(),
        headers: Vec::new(),
        footers: Vec::new(),
    };
    let Some(node) = node else {
        return out;
    };
    out.extras.attrs = node
        .attrs
        .iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    let p = &mut out.props;
    let x = &mut out.extras;
    for c in node.elements() {
        match c.name.as_str() {
            "w:headerReference" => {
                if let Some(id) = attr(c, "r:id") {
                    out.headers
                        .push((hf_kind(attr(c, "w:type")), id.to_owned()));
                }
            }
            "w:footerReference" => {
                if let Some(id) = attr(c, "r:id") {
                    out.footers
                        .push((hf_kind(attr(c, "w:type")), id.to_owned()));
                }
            }
            "w:type" => {
                if let Some(t) = val(Some(c)).and_then(SectionBreak::parse) {
                    p.break_type = t;
                }
            }
            "w:pgSz" => {
                let w = parse_twips(attr(c, "w:w"));
                let h = parse_twips(attr(c, "w:h"));
                if let Some(w) = w {
                    p.page_size.width = w.max(1);
                }
                if let Some(h) = h {
                    p.page_size.height = h.max(1);
                }
                p.page_size.orientation = match attr(c, "w:orient") {
                    Some("landscape") => Orientation::Landscape,
                    Some(_) => Orientation::Portrait,
                    None if matches!((w, h), (Some(w), Some(h)) if w > h) => Orientation::Landscape,
                    None => Orientation::Portrait,
                };
                p.page_size.code = parse_u32(attr(c, "w:code"));
            }
            "w:pgMar" => {
                let m = &mut p.margins;
                for (k, slot) in [
                    ("w:top", &mut m.top),
                    ("w:right", &mut m.right),
                    ("w:bottom", &mut m.bottom),
                    ("w:left", &mut m.left),
                    ("w:header", &mut m.header),
                    ("w:footer", &mut m.footer),
                    ("w:gutter", &mut m.gutter),
                ] {
                    if let Some(v) = parse_twips(attr(c, k)) {
                        *slot = v;
                    }
                }
            }
            "w:cols" => {
                let cols: Vec<Column> = c
                    .children_named("w:col")
                    .map(|col| Column {
                        width: parse_twips(attr(col, "w:w")).unwrap_or(0),
                        space: parse_twips(attr(col, "w:space")).unwrap_or(0),
                    })
                    .collect();
                let count = parse_u32(attr(c, "w:num"))
                    .unwrap_or(u32::try_from(cols.len()).unwrap_or(1))
                    .clamp(1, 45);
                p.columns = Columns {
                    count,
                    space: parse_twips(attr(c, "w:space")).unwrap_or(720),
                    equal_width: match attr(c, "w:equalWidth") {
                        Some(v) => parse_on_off(Some(v), true),
                        None => cols.is_empty(),
                    },
                    separator: parse_on_off(attr(c, "w:sep"), false),
                    cols,
                };
            }
            "w:titlePg" => p.title_page = parse_on_off(val(Some(c)), true),
            "w:pgNumType" => {
                p.page_numbering = Some(PageNumbering {
                    start: parse_decimal(attr(c, "w:start")),
                    format: attr(c, "w:fmt").map(NumberFormat::parse),
                });
                // Chapter numbering attributes are kept verbatim.
                if attr(c, "w:chapStyle").is_some() || attr(c, "w:chapSep").is_some() {
                    x.xml.push(raw.xml(c));
                }
            }
            "w:vAlign" => p.v_align = val(Some(c)).and_then(PageVAlign::parse),
            "w:bidi" => p.bidi = Some(parse_on_off(val(Some(c)), true)),
            "w:docGrid" => {
                p.doc_grid = Some(DocGrid {
                    kind: attr(c, "w:type").map(str::to_owned),
                    line_pitch: parse_decimal(attr(c, "w:linePitch"))
                        .and_then(|n| i32::try_from(n).ok()),
                    char_space: parse_decimal(attr(c, "w:charSpace"))
                        .and_then(|n| i32::try_from(n).ok()),
                });
            }
            "w:lnNumType" => {
                p.line_numbering = Some(LineNumbering {
                    count_by: parse_u32(attr(c, "w:countBy")).unwrap_or(1).max(1),
                    start: parse_u32(attr(c, "w:start")),
                    distance: parse_twips(attr(c, "w:distance")),
                    restart: attr(c, "w:restart")
                        .and_then(LineNumberRestart::parse)
                        .unwrap_or_default(),
                });
            }
            "w:pgBorders" => x.page_borders = Some(parse_page_borders(c)),
            "w:footnotePr" => x.footnote_pr = parse_note_settings(Some(c)),
            "w:endnotePr" => x.endnote_pr = parse_note_settings(Some(c)),
            "w:textDirection" => x.text_direction = val(Some(c)).map(str::to_owned),
            "w:rtlGutter" => x.rtl_gutter = parse_on_off(val(Some(c)), true),
            "w:formProt" => x.form_prot = parse_on_off(val(Some(c)), true),
            "w:noEndnote" => x.no_endnote = parse_on_off(val(Some(c)), true),
            "w:sectPrChange" => x.sect_pr_change = Some(raw.xml(c)),
            // w:paperSrc, w:printerSettings, …
            _ => x.xml.push(raw.xml(c)),
        }
    }
    out
}
