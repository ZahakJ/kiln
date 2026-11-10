// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shape geometry: DrawingML preset names ⇄ `draw:enhanced-geometry`.
//!
//! LibreOffice's own shape names (`rectangle`, `round-rectangle`, `isosceles-triangle`,
//! `right-arrow`, `flowchart-process`, …) map to the DrawingML preset of the same shape; any
//! other preset is written as `ooxml-<preset>`, the name LibreOffice itself gives shapes it
//! imported from Office files. Either way the outline is written out as `draw:enhanced-path`
//! (evaluated from Kiln's preset table at the shape's size), so every consumer draws the shape
//! without knowing the preset.

use std::fmt::Write as _;

use kiln_ooxml::drawingml::presets::{is_preset_shape_type, preset_geometry};
use kiln_ooxml::st::PathFillMode;

use crate::xml::XmlOut;

/// `(LibreOffice name, DrawingML preset)`.
const NATIVE: &[(&str, &str)] = &[
    ("rectangle", "rect"),
    ("round-rectangle", "roundRect"),
    ("ellipse", "ellipse"),
    ("circle", "ellipse"),
    ("diamond", "diamond"),
    ("isosceles-triangle", "triangle"),
    ("right-triangle", "rtTriangle"),
    ("parallelogram", "parallelogram"),
    ("trapezoid", "trapezoid"),
    ("pentagon", "pentagon"),
    ("hexagon", "hexagon"),
    ("octagon", "octagon"),
    ("cross", "plus"),
    ("star4", "star4"),
    ("star5", "star5"),
    ("star6", "star6"),
    ("star8", "star8"),
    ("star12", "star12"),
    ("star24", "star24"),
    ("right-arrow", "rightArrow"),
    ("left-arrow", "leftArrow"),
    ("up-arrow", "upArrow"),
    ("down-arrow", "downArrow"),
    ("left-right-arrow", "leftRightArrow"),
    ("up-down-arrow", "upDownArrow"),
    ("chevron", "chevron"),
    ("pentagon-right", "homePlate"),
    ("smiley", "smileyFace"),
    ("heart", "heart"),
    ("moon", "moon"),
    ("sun", "sun"),
    ("cloud", "cloudCallout"),
    ("can", "can"),
    ("cube", "cube"),
    ("ring", "donut"),
    ("block-arc", "blockArc"),
    ("frame", "frame"),
    ("lightning", "lightningBolt"),
    ("flowchart-process", "flowChartProcess"),
    ("flowchart-decision", "flowChartDecision"),
    ("flowchart-terminator", "flowChartTerminator"),
    ("flowchart-data", "flowChartInputOutput"),
    ("flowchart-document", "flowChartDocument"),
    ("flowchart-connector", "flowChartConnector"),
    ("flowchart-predefined-process", "flowChartPredefinedProcess"),
    ("flowchart-alternate-process", "flowChartAlternateProcess"),
    ("flowchart-manual-input", "flowChartManualInput"),
    ("flowchart-preparation", "flowChartPreparation"),
    ("rectangular-callout", "wedgeRectCallout"),
    ("round-rectangular-callout", "wedgeRoundRectCallout"),
    ("round-callout", "wedgeEllipseCallout"),
    ("octagon-bevel", "bevel"),
    ("quad-bevel", "bevel"),
    ("paper", "foldedCorner"),
    ("non-primitive", "rect"),
];

/// The DrawingML preset of a `draw:type` (`rect` when unknown).
#[must_use]
pub fn preset_of_odf(draw_type: &str) -> String {
    if let Some(p) = draw_type.strip_prefix("ooxml-")
        && is_preset_shape_type(p)
    {
        return p.to_owned();
    }
    NATIVE
        .iter()
        .find(|(n, _)| *n == draw_type)
        .map_or("rect", |(_, p)| *p)
        .to_owned()
}

/// The `draw:type` of a DrawingML preset.
#[must_use]
pub fn odf_type_of(preset: &str) -> String {
    match preset {
        "rect" => "rectangle".to_owned(),
        "ellipse" => "ellipse".to_owned(),
        _ => NATIVE
            .iter()
            .find(|(n, p)| *p == preset && *n != "circle" && *n != "quad-bevel")
            .map_or_else(|| format!("ooxml-{preset}"), |(n, _)| (*n).to_owned()),
    }
}

fn num(v: f64) -> String {
    let r = v.round();
    if r.is_finite() && r != 0.0 {
        format!("{r}")
    } else {
        "0".to_owned()
    }
}

/// The `draw:enhanced-path` of a preset for a shape of `w` × `h` (any unit; the path is in a
/// `w` × `h` view box rounded to whole units), or `None` for an unknown preset.
#[must_use]
pub fn enhanced_path(preset: &str, w: f64, h: f64) -> Option<String> {
    let g = preset_geometry(preset, w.max(1.0), h.max(1.0), &[]).ok()?;
    let mut out = String::new();
    for p in &g.paths {
        let bez = p.to_bez_path(0.5);
        if bez.elements().is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        if p.fill == PathFillMode::None {
            out.push_str("F ");
        }
        if !p.stroke {
            out.push_str("S ");
        }
        for el in bez.elements() {
            match *el {
                kurbo::PathEl::MoveTo(a) => {
                    let _ = write!(out, "M {} {} ", num(a.x), num(a.y));
                }
                kurbo::PathEl::LineTo(a) => {
                    let _ = write!(out, "L {} {} ", num(a.x), num(a.y));
                }
                kurbo::PathEl::QuadTo(a, b) => {
                    let _ = write!(
                        out,
                        "Q {} {} {} {} ",
                        num(a.x),
                        num(a.y),
                        num(b.x),
                        num(b.y)
                    );
                }
                kurbo::PathEl::CurveTo(a, b, c) => {
                    let _ = write!(
                        out,
                        "C {} {} {} {} {} {} ",
                        num(a.x),
                        num(a.y),
                        num(b.x),
                        num(b.y),
                        num(c.x),
                        num(c.y)
                    );
                }
                kurbo::PathEl::ClosePath => out.push_str("Z "),
            }
        }
        out.push('N');
    }
    Some(out)
}

/// Writes `draw:enhanced-geometry` for a preset shape of `width` × `height` EMU.
pub fn enhanced_geometry(w: &mut XmlOut, preset: &str, width: i64, height: i64) {
    // Hundredths of a millimetre keep the path small and precise enough.
    #[allow(clippy::cast_precision_loss)]
    let (vw, vh) = (
        (width as f64 / 360.0).max(1.0).round(),
        (height as f64 / 360.0).max(1.0).round(),
    );
    // The `ooxml-` name tells LibreOffice the outline is exactly the path given (its own shape
    // names bring their own handles); without `draw:text-areas` the text uses the whole box.
    let preset = if is_preset_shape_type(preset) {
        preset
    } else {
        "rect"
    };
    w.start("draw:enhanced-geometry")
        .attr("svg:viewBox", format!("0 0 {} {}", num(vw), num(vh)))
        .attr("draw:type", format!("ooxml-{preset}"));
    if let Some(path) = enhanced_path(preset, vw, vh) {
        w.attr("draw:enhanced-path", path);
    }
    w.end();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_map_both_ways() {
        assert_eq!(preset_of_odf("rectangle"), "rect");
        assert_eq!(preset_of_odf("ooxml-wave"), "wave");
        assert_eq!(preset_of_odf("ooxml-nonsense"), "rect");
        assert_eq!(preset_of_odf("isosceles-triangle"), "triangle");
        assert_eq!(odf_type_of("triangle"), "isosceles-triangle");
        assert_eq!(odf_type_of("wave"), "ooxml-wave");
        for (n, p) in NATIVE {
            if *n != "circle" && *n != "quad-bevel" && *n != "non-primitive" && *p != "rect" {
                assert_eq!(preset_of_odf(&odf_type_of(p)), *p, "{n}");
            }
        }
    }

    #[test]
    fn paths_are_written_for_presets() {
        let p = enhanced_path("rect", 100.0, 50.0).unwrap();
        assert!(p.starts_with("M 0 0 L 100 0"), "{p}");
        assert!(p.ends_with("Z N"), "{p}");
        let e = enhanced_path("ellipse", 100.0, 100.0).unwrap();
        assert!(e.contains('C'), "{e}");
        assert!(enhanced_path("nope", 1.0, 1.0).is_none());
    }
}
