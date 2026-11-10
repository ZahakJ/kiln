// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sparklines: the worksheet extension `x14:sparklineGroups`
//! (`{05C60535-1F16-4fd2-B633-F4F36F0B64E0}`), read into [`SparklineGroup`]s and written back
//! when they change.

use kiln_ooxml::xml::{XmlElement, XmlNode};
use kiln_sheet::num::F64;
use kiln_sheet::pivot::cache::split_reference;
use kiln_sheet::sparkline::{EmptyCells, SparkAxis, Sparkline, SparklineGroup, SparklineKind};
use kiln_sheet::{CellPos, Range};

use crate::xmlx::{abool_or, anum, color_el, parse_color};

/// The extension's URI.
pub(crate) const EXT_URI: &str = "{05C60535-1F16-4fd2-B633-F4F36F0B64E0}";
const NS_X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const NS_XM: &str = "http://schemas.microsoft.com/office/excel/2006/main";

fn local(e: &XmlElement) -> &str {
    e.name.rsplit(':').next().unwrap_or("")
}

fn kids<'a>(e: &'a XmlElement, name: &'a str) -> impl Iterator<Item = &'a XmlElement> {
    e.elements().filter(move |c| local(c) == name)
}

/// The groups of one extension element (`ext` or `x14:sparklineGroups`).
pub(crate) fn parse(ext: &XmlElement, own_sheet: &str) -> Vec<SparklineGroup> {
    let mut out = Vec::new();
    for groups in ext.descendants("x14:sparklineGroups") {
        for g in kids(groups, "sparklineGroup") {
            let mut sg = SparklineGroup::new(
                match g.attr("type") {
                    Some("column") => SparklineKind::Column,
                    Some("stacked") => SparklineKind::WinLoss,
                    _ => SparklineKind::Line,
                },
                Vec::new(),
            );
            sg.line_weight = F64(anum(g, "lineWeight").unwrap_or(0.75));
            sg.markers = abool_or(g, "markers", false);
            sg.high = abool_or(g, "high", false);
            sg.low = abool_or(g, "low", false);
            sg.first = abool_or(g, "first", false);
            sg.last = abool_or(g, "last", false);
            sg.negative = abool_or(g, "negative", false);
            sg.show_axis = abool_or(g, "displayXAxis", false);
            sg.show_hidden = abool_or(g, "displayHidden", false);
            sg.right_to_left = abool_or(g, "rightToLeft", false);
            sg.empty = match g.attr("displayEmptyCellsAs") {
                Some("gap") => EmptyCells::Gap,
                Some("span") => EmptyCells::Span,
                // The schema's default.
                _ => EmptyCells::Zero,
            };
            let axis = |ty: Option<&str>, manual: Option<f64>| match ty {
                Some("group") => SparkAxis::Group,
                Some("custom") => SparkAxis::Custom(F64(manual.unwrap_or(0.0))),
                _ => SparkAxis::Individual,
            };
            sg.min = axis(g.attr("minAxisType"), anum(g, "manualMin"));
            sg.max = axis(g.attr("maxAxisType"), anum(g, "manualMax"));
            let c = &mut sg.colors;
            for (name, slot) in [
                ("colorSeries", &mut c.series),
                ("colorNegative", &mut c.negative),
                ("colorAxis", &mut c.axis),
                ("colorMarkers", &mut c.markers),
                ("colorFirst", &mut c.first),
                ("colorLast", &mut c.last),
                ("colorHigh", &mut c.high),
                ("colorLow", &mut c.low),
            ] {
                if let Some(col) = kids(g, name).next().and_then(parse_color) {
                    *slot = col;
                }
            }
            for list in kids(g, "sparklines") {
                for s in kids(list, "sparkline") {
                    let f = kids(s, "f")
                        .next()
                        .map(XmlElement::text)
                        .unwrap_or_default();
                    let at = kids(s, "sqref")
                        .next()
                        .map(XmlElement::text)
                        .unwrap_or_default();
                    let (Some((sheet, data)), Some(at)) =
                        (split_reference(&f), Range::parse_a1(at.trim()))
                    else {
                        continue;
                    };
                    sg.sparklines.push(Sparkline {
                        at: CellPos::new(at.r0, at.c0),
                        sheet: sheet.filter(|n| n != own_sheet),
                        data,
                    });
                }
            }
            if !sg.sparklines.is_empty() {
                out.push(sg);
            }
        }
    }
    out
}

fn quote(name: &str) -> String {
    if name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
    {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// The extension element for a sheet's groups.
pub(crate) fn ext_el(groups: &[SparklineGroup], own_sheet: &str) -> XmlElement {
    let mut list = XmlElement::new("x14:sparklineGroups").with_attr("xmlns:xm", NS_XM);
    for g in groups {
        let mut e = XmlElement::new("x14:sparklineGroup");
        if g.line_weight.0 != 0.75 {
            e.attrs.set(
                "lineWeight",
                kiln_ooxml::xml::format_number(g.line_weight.0),
            );
        }
        match g.kind {
            SparklineKind::Line => {}
            SparklineKind::Column => e.attrs.set("type", "column"),
            SparklineKind::WinLoss => e.attrs.set("type", "stacked"),
        }
        e.attrs.set(
            "displayEmptyCellsAs",
            match g.empty {
                EmptyCells::Gap => "gap",
                EmptyCells::Zero => "zero",
                EmptyCells::Span => "span",
            },
        );
        for (name, on) in [
            ("markers", g.markers),
            ("high", g.high),
            ("low", g.low),
            ("first", g.first),
            ("last", g.last),
            ("negative", g.negative),
            ("displayXAxis", g.show_axis),
            ("displayHidden", g.show_hidden),
            ("rightToLeft", g.right_to_left),
        ] {
            if on {
                e.attrs.set(name, "1");
            }
        }
        for (ty, manual, a) in [
            ("minAxisType", "manualMin", g.min),
            ("maxAxisType", "manualMax", g.max),
        ] {
            match a {
                SparkAxis::Individual => {}
                SparkAxis::Group => e.attrs.set(ty, "group"),
                SparkAxis::Custom(v) => {
                    e.attrs.set(ty, "custom");
                    e.attrs.set(manual, kiln_ooxml::xml::format_number(v.0));
                }
            }
        }
        let c = &g.colors;
        for (name, col) in [
            ("x14:colorSeries", &c.series),
            ("x14:colorNegative", &c.negative),
            ("x14:colorAxis", &c.axis),
            ("x14:colorMarkers", &c.markers),
            ("x14:colorFirst", &c.first),
            ("x14:colorLast", &c.last),
            ("x14:colorHigh", &c.high),
            ("x14:colorLow", &c.low),
        ] {
            e.children.push(XmlNode::Element(color_el(name, col)));
        }
        let mut lines = XmlElement::new("x14:sparklines");
        for s in &g.sparklines {
            let sheet = s.sheet.as_deref().unwrap_or(own_sheet);
            lines.children.push(XmlNode::Element(
                XmlElement::new("x14:sparkline")
                    .with_child(XmlElement::new("xm:f").with_text(format!(
                        "{}!{}",
                        quote(sheet),
                        s.data.to_a1(false)
                    )))
                    .with_child(
                        XmlElement::new("xm:sqref")
                            .with_text(Range::cell(s.at.row, s.at.col).to_a1(false)),
                    ),
            ));
        }
        e.children.push(XmlNode::Element(lines));
        list.children.push(XmlNode::Element(e));
    }
    XmlElement::new("ext")
        .with_attr("uri", EXT_URI)
        .with_attr("xmlns:x14", NS_X14)
        .with_child(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_round_trip_through_the_extension() {
        let mut g = SparklineGroup::new(
            SparklineKind::Column,
            vec![
                Sparkline {
                    at: CellPos::new(1, 5),
                    sheet: None,
                    data: Range::new(1, 0, 1, 4),
                },
                Sparkline {
                    at: CellPos::new(2, 5),
                    sheet: Some("Raw data".into()),
                    data: Range::new(2, 0, 2, 4),
                },
            ],
        );
        g.high = true;
        g.negative = true;
        g.max = SparkAxis::Custom(F64(100.0));
        g.min = SparkAxis::Group;
        g.empty = EmptyCells::Span;
        g.colors.series = kiln_sheet::Color::theme(4, -0.5);
        let ext = ext_el(std::slice::from_ref(&g), "Sales");
        let text = kiln_ooxml::xml::write_element(&ext, &kiln_ooxml::xml::WriteOptions::default())
            .expect("xml");
        assert!(text.contains("<xm:f>Sales!A2:E2</xm:f>"), "{text}");
        assert!(text.contains("<xm:f>'Raw data'!A3:E3</xm:f>"));
        assert_eq!(parse(&ext, "Sales"), vec![g]);
    }
}
