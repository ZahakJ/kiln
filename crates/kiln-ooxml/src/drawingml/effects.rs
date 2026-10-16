// SPDX-License-Identifier: AGPL-3.0-or-later
//! DrawingML effects, `a:effectLst` (ECMA-376 Part 1 §20.1.8.26). Shadows, glow, reflection, soft
//! edges and blur are modelled; anything else is preserved verbatim.

use kurbo::Vec2;

use super::color::{
    ColorContext, DrawingColor, parse_color_child, resolve_color, rgba_to_css, serialize_color,
};
use crate::st::RectAlignment;
use crate::units::{emu_to_px, round_to, round_to_i64};
use crate::xml::{XmlElement, format_number};

/// `a:outerShdw`.
#[derive(Debug, Clone, PartialEq)]
pub struct OuterShadow {
    /// Blur radius, EMU.
    pub blur_rad: i64,
    /// Distance, EMU.
    pub dist: i64,
    /// Direction, degrees clockwise from the x axis.
    pub dir: f64,
    /// Horizontal scale (1 = 100 %).
    pub sx: f64,
    /// Vertical scale.
    pub sy: f64,
    /// Horizontal skew, degrees.
    pub kx: f64,
    /// Vertical skew, degrees.
    pub ky: f64,
    /// Alignment.
    pub algn: RectAlignment,
    /// Whether the shadow rotates with the shape.
    pub rot_with_shape: bool,
    /// Colour.
    pub color: DrawingColor,
}

/// `a:innerShdw`.
#[derive(Debug, Clone, PartialEq)]
pub struct InnerShadow {
    /// Blur radius, EMU.
    pub blur_rad: i64,
    /// Distance, EMU.
    pub dist: i64,
    /// Direction, degrees.
    pub dir: f64,
    /// Colour.
    pub color: DrawingColor,
}

/// `a:glow`.
#[derive(Debug, Clone, PartialEq)]
pub struct Glow {
    /// Radius, EMU.
    pub rad: i64,
    /// Colour.
    pub color: DrawingColor,
}

/// `a:reflection`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reflection {
    /// Blur radius, EMU.
    pub blur_rad: i64,
    /// Start opacity (1 = 100 %).
    pub st_a: f64,
    /// Start position.
    pub st_pos: f64,
    /// End opacity.
    pub end_a: f64,
    /// End position.
    pub end_pos: f64,
    /// Distance, EMU.
    pub dist: i64,
    /// Direction, degrees.
    pub dir: f64,
    /// Fade direction, degrees.
    pub fade_dir: f64,
    /// Horizontal scale.
    pub sx: f64,
    /// Vertical scale.
    pub sy: f64,
    /// Horizontal skew, degrees.
    pub kx: f64,
    /// Vertical skew, degrees.
    pub ky: f64,
    /// Alignment.
    pub algn: RectAlignment,
    /// Whether the reflection rotates with the shape.
    pub rot_with_shape: bool,
}

/// `a:blur`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blur {
    /// Radius, EMU.
    pub rad: i64,
    /// Whether the bounds grow by the radius.
    pub grow: bool,
}

/// `a:effectLst`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectList {
    /// Outer shadow.
    pub outer_shadow: Option<OuterShadow>,
    /// Inner shadow.
    pub inner_shadow: Option<InnerShadow>,
    /// Glow.
    pub glow: Option<Glow>,
    /// Reflection.
    pub reflection: Option<Reflection>,
    /// Soft edge radius, EMU.
    pub soft_edge: Option<i64>,
    /// Blur.
    pub blur: Option<Blur>,
    /// Effects Kiln does not model (`a:prstShdw`, `a:fillOverlay`, …), in order.
    pub other: Vec<XmlElement>,
}

#[allow(clippy::cast_precision_loss)]
fn pct(n: &XmlElement, k: &str, d: i64) -> f64 {
    n.attr_int(k).unwrap_or(d) as f64 / 100_000.0
}
#[allow(clippy::cast_precision_loss)]
fn ang(n: &XmlElement, k: &str) -> f64 {
    n.attr_int(k).unwrap_or(0) as f64 / 60_000.0
}
fn black() -> DrawingColor {
    DrawingColor::srgb("000000")
}

/// Parses `a:effectLst`.
#[must_use]
pub fn parse_effect_list(node: &XmlElement) -> EffectList {
    let mut out = EffectList::default();
    let algn = |c: &XmlElement| {
        c.attr("algn")
            .and_then(RectAlignment::parse)
            .unwrap_or(RectAlignment::B)
    };
    for c in node.elements() {
        match c.name.as_str() {
            "a:outerShdw" => {
                out.outer_shadow = Some(OuterShadow {
                    blur_rad: c.attr_int("blurRad").unwrap_or(0),
                    dist: c.attr_int("dist").unwrap_or(0),
                    dir: ang(c, "dir"),
                    sx: pct(c, "sx", 100_000),
                    sy: pct(c, "sy", 100_000),
                    kx: ang(c, "kx"),
                    ky: ang(c, "ky"),
                    algn: algn(c),
                    rot_with_shape: c.attr_bool("rotWithShape").unwrap_or(true),
                    color: parse_color_child(Some(c)).unwrap_or_else(black),
                });
            }
            "a:innerShdw" => {
                out.inner_shadow = Some(InnerShadow {
                    blur_rad: c.attr_int("blurRad").unwrap_or(0),
                    dist: c.attr_int("dist").unwrap_or(0),
                    dir: ang(c, "dir"),
                    color: parse_color_child(Some(c)).unwrap_or_else(black),
                });
            }
            "a:glow" => {
                out.glow = Some(Glow {
                    rad: c.attr_int("rad").unwrap_or(0),
                    color: parse_color_child(Some(c)).unwrap_or_else(black),
                });
            }
            "a:reflection" => {
                #[allow(clippy::cast_precision_loss)]
                let fade_dir = c.attr_int("fadeDir").unwrap_or(5_400_000) as f64 / 60_000.0;
                out.reflection = Some(Reflection {
                    blur_rad: c.attr_int("blurRad").unwrap_or(0),
                    st_a: pct(c, "stA", 100_000),
                    st_pos: pct(c, "stPos", 0),
                    end_a: pct(c, "endA", 0),
                    end_pos: pct(c, "endPos", 100_000),
                    dist: c.attr_int("dist").unwrap_or(0),
                    dir: ang(c, "dir"),
                    fade_dir,
                    sx: pct(c, "sx", 100_000),
                    sy: pct(c, "sy", 100_000),
                    kx: ang(c, "kx"),
                    ky: ang(c, "ky"),
                    algn: algn(c),
                    rot_with_shape: c.attr_bool("rotWithShape").unwrap_or(true),
                });
            }
            "a:softEdge" => out.soft_edge = Some(c.attr_int("rad").unwrap_or(0)),
            "a:blur" => {
                out.blur = Some(Blur {
                    rad: c.attr_int("rad").unwrap_or(0),
                    grow: c.attr_bool("grow").unwrap_or(true),
                });
            }
            _ => out.other.push(c.clone()),
        }
    }
    out
}

fn nz(v: i64, default: i64) -> Option<i64> {
    (v != default).then_some(v)
}
fn to_pct(v: f64, default: f64) -> Option<i64> {
    nz(
        round_to_i64(v * 100_000.0),
        round_to_i64(default * 100_000.0),
    )
}
fn to_ang(v: f64) -> Option<i64> {
    nz(round_to_i64(v * 60_000.0), 0)
}

/// Serialises in schema order (blur, fillOverlay, glow, innerShdw, outerShdw, prstShdw,
/// reflection, softEdge), then any other preserved effects.
#[must_use]
pub fn serialize_effect_list(fx: &EffectList) -> XmlElement {
    let others = |name: &str| {
        fx.other
            .iter()
            .filter(move |x| x.name == name)
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut el = XmlElement::new("a:effectLst");
    if let Some(b) = fx.blur {
        el = el.with_child(
            XmlElement::new("a:blur")
                .with_opt_attr("rad", nz(b.rad, 0))
                .with_opt_attr("grow", (!b.grow).then_some("0")),
        );
    }
    el = el.with_children(others("a:fillOverlay"));
    if let Some(g) = &fx.glow {
        el = el.with_child(
            XmlElement::new("a:glow")
                .with_opt_attr("rad", nz(g.rad, 0))
                .with_child(serialize_color(&g.color)),
        );
    }
    if let Some(s) = &fx.inner_shadow {
        el = el.with_child(
            XmlElement::new("a:innerShdw")
                .with_opt_attr("blurRad", nz(s.blur_rad, 0))
                .with_opt_attr("dist", nz(s.dist, 0))
                .with_opt_attr("dir", to_ang(s.dir))
                .with_child(serialize_color(&s.color)),
        );
    }
    if let Some(s) = &fx.outer_shadow {
        el = el.with_child(
            XmlElement::new("a:outerShdw")
                .with_opt_attr("blurRad", nz(s.blur_rad, 0))
                .with_opt_attr("dist", nz(s.dist, 0))
                .with_opt_attr("dir", to_ang(s.dir))
                .with_opt_attr("sx", to_pct(s.sx, 1.0))
                .with_opt_attr("sy", to_pct(s.sy, 1.0))
                .with_opt_attr("kx", to_ang(s.kx))
                .with_opt_attr("ky", to_ang(s.ky))
                .with_opt_attr("algn", (s.algn != RectAlignment::B).then_some(s.algn))
                .with_opt_attr("rotWithShape", (!s.rot_with_shape).then_some("0"))
                .with_child(serialize_color(&s.color)),
        );
    }
    el = el.with_children(others("a:prstShdw"));
    if let Some(r) = fx.reflection {
        el = el.with_child(
            XmlElement::new("a:reflection")
                .with_opt_attr("blurRad", nz(r.blur_rad, 0))
                .with_opt_attr("stA", to_pct(r.st_a, 1.0))
                .with_opt_attr("stPos", to_pct(r.st_pos, 0.0))
                .with_opt_attr("endA", to_pct(r.end_a, 0.0))
                .with_opt_attr("endPos", to_pct(r.end_pos, 1.0))
                .with_opt_attr("dist", nz(r.dist, 0))
                .with_opt_attr("dir", to_ang(r.dir))
                .with_opt_attr(
                    "fadeDir",
                    nz(round_to_i64(r.fade_dir * 60_000.0), 5_400_000),
                )
                .with_opt_attr("sx", to_pct(r.sx, 1.0))
                .with_opt_attr("sy", to_pct(r.sy, 1.0))
                .with_opt_attr("kx", to_ang(r.kx))
                .with_opt_attr("ky", to_ang(r.ky))
                .with_opt_attr("algn", (r.algn != RectAlignment::B).then_some(r.algn))
                .with_opt_attr("rotWithShape", (!r.rot_with_shape).then_some("0")),
        );
    }
    if let Some(rad) = fx.soft_edge {
        el = el.with_child(XmlElement::new("a:softEdge").with_attr("rad", rad));
    }
    el.with_children(
        fx.other
            .iter()
            .filter(|x| x.name != "a:fillOverlay" && x.name != "a:prstShdw")
            .cloned(),
    )
}

/// An outer shadow ready for a renderer or CSS.
#[derive(Debug, Clone, PartialEq)]
pub struct ShadowCss {
    /// Horizontal offset, px.
    pub x: f64,
    /// Vertical offset, px.
    pub y: f64,
    /// Blur extent, px (DrawingML's `blurRad`).
    pub blur: f64,
    /// CSS colour.
    pub color: String,
    /// A ready `filter: drop-shadow(…)` value (drop-shadow follows the outline, as PowerPoint's
    /// shadow does; its blur is a standard deviation, half the extent).
    pub filter: String,
}

impl OuterShadow {
    /// The offset in CSS pixels.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn offset_px(&self) -> Vec2 {
        let d = emu_to_px(self.dist as f64);
        let rad = self.dir.to_radians();
        Vec2::new(rad.cos() * d, rad.sin() * d)
    }

    /// The shadow as CSS.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn to_css(&self, ctx: &ColorContext) -> ShadowCss {
        let o = self.offset_px();
        let x = round_to(o.x, 2);
        let y = round_to(o.y, 2);
        let blur = round_to(emu_to_px(self.blur_rad as f64), 2);
        let color = rgba_to_css(resolve_color(&self.color, ctx));
        let filter = format!(
            "drop-shadow({}px {}px {}px {color})",
            format_number(x),
            format_number(y),
            format_number(round_to(blur / 2.0, 2))
        );
        ShadowCss {
            x,
            y,
            blur,
            color,
            filter,
        }
    }
}
