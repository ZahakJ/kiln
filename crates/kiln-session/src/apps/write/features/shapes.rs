// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shape Format for shapes and text boxes: Shape Fill and Shape Outline (solid colours, none,
//! the outline's weight), Align Text (top, middle, bottom) and the text's internal margins.
//!
//! The look of a shape lives in its kept DrawingML (`Drawing::xml`): the layout reads the fill,
//! the outline and `wps:bodyPr` from it, and the DOCX writer keeps it, so editing that XML is
//! what Word stores (`wps:spPr` fills and `a:ln`, `wps:bodyPr/@anchor` and the `*Ins` insets).

use kiln_common::{KilnError, Result};
use kiln_doc::{Drawing, DrawingKind};
use kiln_ooxml::xml::{WriteOptions, XmlElement, XmlNode, parse_xml_str, write_element};
use kiln_protocol::commands::write_features::{ShapeLook, ShapeMargins, TextAnchor};

use super::super::WriteDoc;

/// EMU per point.
const EMU_PT: f64 = 12_700.0;
/// DrawingML's default insets: 0.1 in left and right, 0.05 in top and bottom (points).
const DEFAULT_INSETS: ShapeMargins = ShapeMargins {
    left: 7.2,
    top: 3.6,
    right: 7.2,
    bottom: 3.6,
};
/// The fill elements of `spPr` and `a:ln`.
pub(in crate::apps::write) const FILLS: [&str; 6] = [
    "a:noFill",
    "a:solidFill",
    "a:gradFill",
    "a:blipFill",
    "a:pattFill",
    "a:grpFill",
];
/// What follows the fill in `spPr` (schema order).
pub(in crate::apps::write) const AFTER_FILL: [&str; 6] = [
    "a:ln",
    "a:effectLst",
    "a:effectDag",
    "a:scene3d",
    "a:sp3d",
    "a:extLst",
];

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

pub(in crate::apps::write) fn find_mut<'a>(
    el: &'a mut XmlElement,
    name: &str,
) -> Option<&'a mut XmlElement> {
    if local(&el.name) == name {
        return Some(el);
    }
    el.elements_mut().find_map(|c| find_mut(c, name))
}

fn find<'a>(el: &'a XmlElement, name: &str) -> Option<&'a XmlElement> {
    if local(&el.name) == name {
        return Some(el);
    }
    el.elements().find_map(|c| find(c, name))
}

fn solid(hex: &str) -> XmlElement {
    XmlElement::new("a:solidFill").with_child(XmlElement::new("a:srgbClr").with_attr("val", hex))
}

fn hex_of(fill: Option<&XmlElement>) -> Option<String> {
    let f = fill?;
    if local(&f.name) != "solidFill" {
        return None;
    }
    f.child("a:srgbClr")
        .and_then(|c| c.attr("val"))
        .map(str::to_ascii_uppercase)
}

/// A colour argument: six hex digits.
pub(in crate::apps::write) fn check_hex(color: Option<&str>) -> Result<Option<String>> {
    match color {
        None => Ok(None),
        Some(c) if c.len() == 6 && c.chars().all(|ch| ch.is_ascii_hexdigit()) => {
            Ok(Some(c.to_ascii_uppercase()))
        }
        Some(c) => Err(KilnError::InvalidInput(format!(
            "'{c}' is not a colour (six hex digits, RRGGBB)"
        ))),
    }
}

#[allow(clippy::cast_possible_truncation)]
fn emu(pt: f32) -> i64 {
    (f64::from(pt) * EMU_PT).round() as i64
}

#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn pt(emu: i64) -> f32 {
    (emu as f64 / EMU_PT) as f32
}

/// The look of a shape for Shape Format (`None` for drawings that are not editable shapes).
#[must_use]
pub(super) fn shape_look(d: &Drawing) -> Option<ShapeLook> {
    if !matches!(d.kind, DrawingKind::Shape { .. }) {
        return None;
    }
    let parsed = parse_xml_str(d.xml.as_deref()?).ok()?;
    let root = &parsed.root;
    let sp_pr = find(root, "spPr");
    let fill = sp_pr.and_then(|s| s.elements().find(|e| FILLS.contains(&e.name.as_str())));
    let ln = sp_pr.and_then(|s| s.child("a:ln"));
    let ln_fill = ln.and_then(|l| l.elements().find(|e| FILLS.contains(&e.name.as_str())));
    let body = find(root, "bodyPr");
    let inset = |name: &str, default: f32| {
        body.and_then(|b| b.attr(name))
            .and_then(|v| v.parse::<i64>().ok())
            .map_or(default, pt)
    };
    Some(ShapeLook {
        fill: hex_of(fill),
        outline: hex_of(ln_fill),
        outline_weight: ln
            .and_then(|l| l.attr("w"))
            .and_then(|w| w.parse::<i64>().ok())
            .map_or(0.75, pt),
        anchor: match body.and_then(|b| b.attr("anchor")) {
            Some("ctr") => TextAnchor::Middle,
            Some("b") => TextAnchor::Bottom,
            _ => TextAnchor::Top,
        },
        margins: ShapeMargins {
            left: inset("lIns", DEFAULT_INSETS.left),
            top: inset("tIns", DEFAULT_INSETS.top),
            right: inset("rIns", DEFAULT_INSETS.right),
            bottom: inset("bIns", DEFAULT_INSETS.bottom),
        },
    })
}

impl WriteDoc {
    /// Edits the selected shape's kept DrawingML (one undo step).
    pub(in crate::apps::write) fn edit_shape_xml(
        &mut self,
        f: impl FnOnce(&mut XmlElement) -> Result<()>,
    ) -> Result<()> {
        self.update_object(None, |_, d, _| {
            if !matches!(d.kind, DrawingKind::Shape { .. }) {
                return Err(KilnError::InvalidInput(
                    "select a shape or a text box first".into(),
                ));
            }
            let xml = d
                .xml
                .as_deref()
                .ok_or_else(|| KilnError::Unsupported("changing the look of this shape".into()))?;
            let mut parsed = parse_xml_str(xml).map_err(|e| {
                KilnError::Internal(format!("the shape's DrawingML could not be read: {e}"))
            })?;
            f(&mut parsed.root)?;
            d.xml = Some(
                write_element(&parsed.root, &WriteOptions::fragment()).map_err(|e| {
                    KilnError::Internal(format!("the shape's DrawingML could not be written: {e}"))
                })?,
            );
            Ok(())
        })
    }

    /// Shape Fill: a solid colour, or No Fill.
    pub(in crate::apps::write) fn shape_fill(&mut self, color: Option<&str>) -> Result<()> {
        let hex = check_hex(color)?;
        self.edit_shape_xml(|root| {
            let sp_pr = find_mut(root, "spPr")
                .ok_or_else(|| KilnError::Unsupported("shapes without spPr".into()))?;
            for f in FILLS {
                sp_pr.remove_children(f);
            }
            let fill = hex
                .as_deref()
                .map_or_else(|| XmlElement::new("a:noFill"), solid);
            sp_pr.upsert_child(fill, &AFTER_FILL);
            Ok(())
        })
    }

    /// Shape Outline: a solid colour, or No Outline.
    pub(super) fn shape_outline(&mut self, color: Option<&str>) -> Result<()> {
        let hex = check_hex(color)?;
        self.edit_shape_xml(|root| {
            let sp_pr = find_mut(root, "spPr")
                .ok_or_else(|| KilnError::Unsupported("shapes without spPr".into()))?;
            if sp_pr.child("a:ln").is_none() {
                sp_pr.upsert_child(
                    XmlElement::new("a:ln").with_attr("w", "9525"),
                    &AFTER_FILL[1..],
                );
            }
            let ln = sp_pr
                .child_mut("a:ln")
                .ok_or_else(|| KilnError::Internal("no outline".into()))?;
            for f in FILLS {
                ln.remove_children(f);
            }
            let fill = hex
                .as_deref()
                .map_or_else(|| XmlElement::new("a:noFill"), solid);
            // The fill is the first child of `a:ln`.
            ln.children.insert(0, XmlNode::Element(fill));
            Ok(())
        })
    }

    /// Shape Outline ▸ Weight.
    pub(super) fn shape_outline_weight(&mut self, points: f32) -> Result<()> {
        if !points.is_finite() || !(0.25..=20.0).contains(&points) {
            return Err(KilnError::InvalidInput(
                "an outline of 0.25 to 20 points is needed".into(),
            ));
        }
        self.edit_shape_xml(|root| {
            let sp_pr = find_mut(root, "spPr")
                .ok_or_else(|| KilnError::Unsupported("shapes without spPr".into()))?;
            if sp_pr.child("a:ln").is_none() {
                sp_pr.upsert_child(XmlElement::new("a:ln"), &AFTER_FILL[1..]);
            }
            if let Some(ln) = sp_pr.child_mut("a:ln") {
                ln.set_attr("w", Some(emu(points).to_string().as_str()));
                // A weight on a shape without an outline colour shows the theme's line.
                if ln.child("a:noFill").is_some() {
                    ln.remove_children("a:noFill");
                }
            }
            Ok(())
        })
    }

    /// Align Text and the internal margins.
    pub(super) fn shape_text_layout(
        &mut self,
        anchor: Option<TextAnchor>,
        margins: Option<ShapeMargins>,
    ) -> Result<()> {
        if let Some(m) = margins {
            let all = [m.left, m.top, m.right, m.bottom];
            if all
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=288.0).contains(v))
            {
                return Err(KilnError::InvalidInput(
                    "margins of 0 to 288 points (4 inches) are needed".into(),
                ));
            }
        }
        self.edit_shape_xml(|root| {
            let body = find_mut(root, "bodyPr")
                .ok_or_else(|| KilnError::Unsupported("shapes without text".into()))?;
            if let Some(a) = anchor {
                let v = match a {
                    TextAnchor::Top => "t",
                    TextAnchor::Middle => "ctr",
                    TextAnchor::Bottom => "b",
                };
                body.set_attr("anchor", Some(v));
            }
            if let Some(m) = margins {
                for (name, v) in [
                    ("lIns", m.left),
                    ("tIns", m.top),
                    ("rIns", m.right),
                    ("bIns", m.bottom),
                ] {
                    body.set_attr(name, Some(emu(v).to_string().as_str()));
                }
            }
            Ok(())
        })
    }
}
