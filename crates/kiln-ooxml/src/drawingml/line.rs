// SPDX-License-Identifier: AGPL-3.0-or-later
//! DrawingML outlines, `a:ln` (ECMA-376 Part 1 §20.1.2.2.24), and their mapping to `kurbo`
//! strokes.

use kurbo::{Cap, Join, Stroke};

use super::fill::{Fill, find_fill, serialize_fill};
use crate::st::{CompoundLine, LineCap, LineEndSize, LineEndType, PenAlignment, PresetLineDashVal};
use crate::units::{emu_to_px, round_to, round_to_i64};
use crate::xml::XmlElement;

/// An arrowhead (`a:headEnd`, `a:tailEnd`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineEnd {
    /// The decoration.
    pub end_type: LineEndType,
    /// Width relative to the line.
    pub w: Option<LineEndSize>,
    /// Length relative to the line.
    pub len: Option<LineEndSize>,
}

/// A dash pattern.
#[derive(Debug, Clone, PartialEq)]
pub enum LineDash {
    /// `a:prstDash`.
    Preset(PresetLineDashVal),
    /// `a:custDash`: `(dash, space)` pairs in percent of the line width.
    Custom(Vec<(f64, f64)>),
}

/// How segments join.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineJoin {
    /// `a:round`.
    Round,
    /// `a:bevel`.
    Bevel,
    /// `a:miter` with an optional limit (a ratio; `lim="800000"` is 8).
    Miter(Option<f64>),
}

/// `a:ln`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Line {
    /// Width in EMU.
    pub width: Option<i64>,
    /// End caps.
    pub cap: Option<LineCap>,
    /// Compound type.
    pub compound: Option<CompoundLine>,
    /// Pen alignment.
    pub align: Option<PenAlignment>,
    /// The line's fill.
    pub fill: Option<Fill>,
    /// Dashing.
    pub dash: Option<LineDash>,
    /// Joins.
    pub join: Option<LineJoin>,
    /// Arrowhead at the start.
    pub head: Option<LineEnd>,
    /// Arrowhead at the end.
    pub tail: Option<LineEnd>,
}

fn parse_end(node: Option<&XmlElement>) -> Option<LineEnd> {
    let node = node?;
    Some(LineEnd {
        end_type: node
            .attr("type")
            .and_then(LineEndType::parse)
            .unwrap_or(LineEndType::None),
        w: node.attr("w").and_then(LineEndSize::parse),
        len: node.attr("len").and_then(LineEndSize::parse),
    })
}

/// Parses `a:ln` (or any element of the same type, such as `a:lnL` in table cells).
#[must_use]
pub fn parse_line(node: &XmlElement) -> Line {
    #[allow(clippy::cast_precision_loss)]
    let thousandth = |v: Option<i64>| v.unwrap_or(0) as f64 / 1000.0;
    let dash = if let Some(p) = node.child("a:prstDash") {
        Some(LineDash::Preset(
            p.attr("val")
                .and_then(PresetLineDashVal::parse)
                .unwrap_or(PresetLineDashVal::Solid),
        ))
    } else {
        node.child("a:custDash").map(|c| {
            LineDash::Custom(
                c.children_named("a:ds")
                    .map(|ds| (thousandth(ds.attr_int("d")), thousandth(ds.attr_int("sp"))))
                    .collect(),
            )
        })
    };
    let join = if node.child("a:round").is_some() {
        Some(LineJoin::Round)
    } else if node.child("a:bevel").is_some() {
        Some(LineJoin::Bevel)
    } else {
        #[allow(clippy::cast_precision_loss)]
        node.child("a:miter")
            .map(|m| LineJoin::Miter(m.attr_int("lim").map(|l| l as f64 / 100_000.0)))
    };
    Line {
        width: node.attr_int("w"),
        cap: node.attr("cap").and_then(LineCap::parse),
        compound: node.attr("cmpd").and_then(CompoundLine::parse),
        align: node.attr("algn").and_then(PenAlignment::parse),
        fill: find_fill(node),
        dash,
        join,
        head: parse_end(node.child("a:headEnd")),
        tail: parse_end(node.child("a:tailEnd")),
    }
}

/// Serialises a line as `name` (`a:ln`, `a:lnL`, …).
#[must_use]
pub fn serialize_line_named(line: &Line, name: &str) -> XmlElement {
    let end = |n: &str, e: Option<LineEnd>| {
        e.map(|e| {
            XmlElement::new(n)
                .with_attr("type", e.end_type)
                .with_opt_attr("w", e.w)
                .with_opt_attr("len", e.len)
        })
    };
    let dash = line.dash.as_ref().map(|d| match d {
        LineDash::Preset(p) => XmlElement::new("a:prstDash").with_attr("val", *p),
        LineDash::Custom(pairs) => {
            XmlElement::new("a:custDash").with_children(pairs.iter().map(|(d, sp)| {
                XmlElement::new("a:ds")
                    .with_attr("d", round_to_i64(d * 1000.0))
                    .with_attr("sp", round_to_i64(sp * 1000.0))
            }))
        }
    });
    let join = line.join.map(|j| match j {
        LineJoin::Round => XmlElement::new("a:round"),
        LineJoin::Bevel => XmlElement::new("a:bevel"),
        LineJoin::Miter(lim) => XmlElement::new("a:miter")
            .with_opt_attr("lim", lim.map(|l| round_to_i64(l * 100_000.0))),
    });
    XmlElement::new(name)
        .with_opt_attr("w", line.width)
        .with_opt_attr("cap", line.cap)
        .with_opt_attr("cmpd", line.compound)
        .with_opt_attr("algn", line.align)
        .with_opt_child(line.fill.as_ref().map(serialize_fill))
        .with_opt_child(dash)
        .with_opt_child(join)
        .with_opt_child(end("a:headEnd", line.head))
        .with_opt_child(end("a:tailEnd", line.tail))
}

/// Serialises a line as `a:ln`.
#[must_use]
pub fn serialize_line(line: &Line) -> XmlElement {
    serialize_line_named(line, "a:ln")
}

impl PresetLineDashVal {
    /// The dash pattern as multiples of the line width (PowerPoint's rendering); empty for solid.
    #[must_use]
    pub fn pattern(self) -> &'static [f64] {
        match self {
            PresetLineDashVal::Solid => &[],
            PresetLineDashVal::Dot => &[1.0, 3.0],
            PresetLineDashVal::Dash => &[4.0, 3.0],
            PresetLineDashVal::LgDash => &[8.0, 3.0],
            PresetLineDashVal::DashDot => &[4.0, 3.0, 1.0, 3.0],
            PresetLineDashVal::LgDashDot => &[8.0, 3.0, 1.0, 3.0],
            PresetLineDashVal::LgDashDotDot => &[8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
            PresetLineDashVal::SysDash => &[3.0, 1.0],
            PresetLineDashVal::SysDot => &[1.0, 1.0],
            PresetLineDashVal::SysDashDot => &[3.0, 1.0, 1.0, 1.0],
            PresetLineDashVal::SysDashDotDot => &[3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        }
    }
}

impl Line {
    /// The dash pattern as multiples of the line width; empty for a solid line.
    #[must_use]
    pub fn dash_pattern(&self) -> Vec<f64> {
        match &self.dash {
            None => Vec::new(),
            Some(LineDash::Preset(p)) => p.pattern().to_vec(),
            Some(LineDash::Custom(pairs)) => pairs
                .iter()
                .flat_map(|(d, sp)| [d / 100.0, sp / 100.0])
                .collect(),
        }
    }

    /// The width in CSS pixels (Office's default hairline, 9 525 EMU = 1 px, when unset).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn width_px(&self) -> f64 {
        emu_to_px(self.width.unwrap_or(9_525) as f64)
    }

    /// The dash lengths in pixels (rounded to 0.01 px, at least 1 px line width), or `None` for
    /// a solid line.
    #[must_use]
    pub fn dash_array_px(&self) -> Option<Vec<f64>> {
        let w = self.width_px().max(1.0);
        let p = self.dash_pattern();
        (!p.is_empty()).then(|| p.iter().map(|m| round_to(m * w, 2)).collect())
    }

    /// The cap and join a renderer should use (round joins when unset, as PowerPoint draws).
    #[must_use]
    pub fn cap_join(&self) -> (Cap, Join) {
        let cap = match self.cap {
            Some(LineCap::Rnd) => Cap::Round,
            Some(LineCap::Sq) => Cap::Square,
            Some(LineCap::Flat) | None => Cap::Butt,
        };
        let join = match self.join {
            Some(LineJoin::Bevel) => Join::Bevel,
            Some(LineJoin::Miter(_)) => Join::Miter,
            Some(LineJoin::Round) | None => Join::Round,
        };
        (cap, join)
    }

    /// A `kurbo` stroke for this line at `scale` pixels per EMU-pixel (1.0 = CSS pixels).
    #[must_use]
    pub fn to_stroke(&self, scale: f64) -> Stroke {
        let width = self.width_px() * scale;
        let (cap, join) = self.cap_join();
        let mut stroke = Stroke::new(width).with_caps(cap).with_join(join);
        if let Some(LineJoin::Miter(lim)) = self.join {
            stroke = stroke.with_miter_limit(lim.unwrap_or(8.0));
        }
        let unit = width.max(scale);
        let dashes: Vec<f64> = self.dash_pattern().iter().map(|m| m * unit).collect();
        if !dashes.is_empty() {
            stroke = stroke.with_dashes(0.0, dashes);
        }
        stroke
    }
}
