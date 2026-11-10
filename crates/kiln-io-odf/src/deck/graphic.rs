// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fills and outlines: DrawingML [`Fill`] and [`Line`] ⇄ `style:graphic-properties`.
//!
//! | DrawingML | ODF |
//! |---|---|
//! | `a:noFill` / `a:solidFill` (with alpha) | `draw:fill="none"` / `"solid"` + `draw:fill-color` (+ `draw:opacity`) |
//! | `a:gradFill` (linear or path) | `draw:fill="gradient"` + a `draw:gradient` (`linear`, `radial`) from the first to the last stop, angle in tenths of a degree |
//! | `a:pattFill` | a solid fill of the foreground blended over the background |
//! | `a:blipFill` | `draw:fill="bitmap"` + a `draw:fill-image` (stretched or tiled) |
//! | `a:ln` width, colour, dashes | `svg:stroke-width`, `svg:stroke-color`, `draw:stroke="dash"` + a `draw:stroke-dash` |
//! | `p:style` fill and line references | resolved through the theme's format scheme first |

use std::collections::HashMap;

use kiln_deck::ShapeProps;
use kiln_ooxml::drawingml::color::{ColorContext, DrawingColor, resolve_color, rgba_to_hex};
use kiln_ooxml::drawingml::fill::{
    BlipFill, BlipFillMode, Fill, GradientFill, GradientStop, LinearShade,
};
use kiln_ooxml::drawingml::line::{Line, LineDash};
use kiln_ooxml::drawingml::theme::Theme;
use kiln_ooxml::st::PresetLineDashVal;
use kiln_ooxml::xml::XmlElement;

use crate::el::El;
use crate::text::styles::{Scope, StyleIndex};
use crate::units::{emu, emu_str, percent};

/// Named drawing definitions of `styles.xml` (`draw:gradient`, `draw:fill-image`,
/// `draw:stroke-dash`) shared by equal definitions.
#[derive(Debug, Default)]
pub struct GraphicDefs {
    /// The definitions, in creation order.
    pub list: Vec<El>,
    index: HashMap<El, String>,
}

impl GraphicDefs {
    fn add(&mut self, prefix: &str, el: El) -> String {
        if let Some(n) = self.index.get(&el) {
            return n.clone();
        }
        let name = format!("{prefix}_{}", self.list.len() + 1);
        let mut named = el.clone();
        named
            .attrs
            .insert(0, ("draw:name".to_owned(), name.clone()));
        self.list.push(named);
        self.index.insert(el, name.clone());
        name
    }
}

/// `#rrggbb` and alpha of a DrawingML colour.
#[must_use]
pub fn css(c: &DrawingColor, ctx: &ColorContext) -> (String, f64) {
    let rgba = resolve_color(c, ctx);
    (
        format!("#{}", rgba_to_hex(rgba).to_ascii_lowercase()),
        rgba.a.clamp(0.0, 1.0),
    )
}

fn pct(v: f64) -> String {
    let p = (v * 100.0 * 100.0).round() / 100.0;
    format!("{}%", crate::units::number_str(p))
}

/// The fill and outline a shape shows: its own, else what its style references in the theme.
#[must_use]
pub fn effective(
    props: &ShapeProps,
    theme: &Theme,
    ctx: &ColorContext,
) -> (Option<Fill>, Option<Line>, ColorContext) {
    let mut fill_ctx = *ctx;
    let mut line_ctx = *ctx;
    let mut fill = props.fill.clone();
    let mut line = props.line.clone();
    if let Some(st) = &props.style {
        if fill.is_none() && st.fill.idx > 0 {
            let fs = &theme.format_scheme;
            let picked = if st.fill.idx >= 1001 {
                fs.bg_fill_styles.get((st.fill.idx - 1001) as usize)
            } else {
                fs.fill_styles.get((st.fill.idx - 1) as usize)
            };
            fill = picked.cloned();
            fill_ctx.ph_clr = st.fill.color.as_ref().map(|c| resolve_color(c, ctx));
        }
        if st.line.idx > 0
            && let Some(base) = theme
                .format_scheme
                .line_styles
                .get((st.line.idx - 1) as usize)
        {
            line_ctx.ph_clr = st.line.color.as_ref().map(|c| resolve_color(c, ctx));
            let mut merged = base.clone();
            if let Some(own) = &line {
                if own.width.is_some() {
                    merged.width = own.width;
                }
                if own.fill.is_some() {
                    merged.fill.clone_from(&own.fill);
                    line_ctx = *ctx;
                }
                if own.dash.is_some() {
                    merged.dash.clone_from(&own.dash);
                }
                if own.head.is_some() {
                    merged.head.clone_from(&own.head);
                }
                if own.tail.is_some() {
                    merged.tail.clone_from(&own.tail);
                }
            }
            // Colours of the style line resolve with its own placeholder colour.
            if let Some(Fill::Solid(c)) = &merged.fill {
                let (hex, a) = css(c, &line_ctx);
                let mut d = DrawingColor::srgb(&hex);
                if a < 1.0 {
                    d.transforms
                        .push(kiln_ooxml::drawingml::color::ColorTransform::new(
                            "alpha",
                            (a * 100_000.0).round() as i64,
                        ));
                }
                merged.fill = Some(Fill::Solid(d));
            }
            line = Some(merged);
        }
    }
    // Fill colours from the style resolve with its placeholder colour.
    if let Some(f) = &fill {
        fill = Some(flatten_fill(f, &fill_ctx));
    }
    (fill, line, *ctx)
}

/// A fill with its colours resolved to sRGB (so a later context cannot change them).
#[must_use]
pub fn flatten_fill(f: &Fill, ctx: &ColorContext) -> Fill {
    let flat = |c: &DrawingColor| {
        let (hex, a) = css(c, ctx);
        let mut d = DrawingColor::srgb(&hex);
        if a < 1.0 {
            d.transforms
                .push(kiln_ooxml::drawingml::color::ColorTransform::new(
                    "alpha",
                    (a * 100_000.0).round() as i64,
                ));
        }
        d
    };
    match f {
        Fill::Solid(c) => Fill::Solid(flat(c)),
        Fill::Gradient(g) => {
            let mut g = g.clone();
            for s in &mut g.stops {
                s.color = flat(&s.color);
            }
            Fill::Gradient(g)
        }
        Fill::Pattern(p) => {
            let mut p = p.clone();
            p.fg = p.fg.as_ref().map(flat);
            p.bg = p.bg.as_ref().map(flat);
            Fill::Pattern(p)
        }
        other => other.clone(),
    }
}

/// Writes a fill into graphic (or drawing-page) properties. `image` maps a picture part to its
/// package path.
pub fn write_fill(
    gp: &mut El,
    fill: Option<&Fill>,
    ctx: &ColorContext,
    defs: &mut GraphicDefs,
    image: &mut dyn FnMut(&str) -> Option<String>,
) {
    match fill {
        None => {}
        Some(Fill::None | Fill::Group) => gp.set("draw:fill", "none"),
        Some(Fill::Solid(c)) => {
            let (hex, a) = css(c, ctx);
            gp.set("draw:fill", "solid");
            gp.set("draw:fill-color", hex);
            if a < 1.0 {
                gp.set("draw:opacity", pct(a));
            }
        }
        Some(Fill::Gradient(g)) => {
            let (Some(first), Some(last)) = (g.stops.first(), g.stops.last()) else {
                gp.set("draw:fill", "none");
                return;
            };
            let (start, _) = css(&first.color, ctx);
            let (end, _) = css(&last.color, ctx);
            // DrawingML angles run clockwise from the x axis towards the end colour; ODF angles
            // run counter-clockwise from the vertical, start colour at the top.
            let angle = g.linear.as_ref().map_or(0.0, |l| {
                ((450.0 - l.angle).rem_euclid(360.0) * 10.0).round()
            });
            let style = if g.path.is_some() { "radial" } else { "linear" };
            let el = El::new("draw:gradient")
                .with("draw:style", style)
                .with("draw:start-color", start)
                .with("draw:end-color", end)
                .with("draw:start-intensity", "100%")
                .with("draw:end-intensity", "100%")
                .with("draw:angle", format!("{}deg", angle / 10.0))
                .with("draw:border", "0%");
            let name = defs.add("Gradient", el);
            gp.set("draw:fill", "gradient");
            gp.set("draw:fill-gradient-name", name);
        }
        Some(Fill::Pattern(p)) => {
            // A hatch of the foreground colour over the background.
            let name = p.preset.as_str();
            let lower = name.to_ascii_lowercase();
            let (style, rotation) = if lower.contains("cross")
                || lower.contains("grid")
                || lower.contains("check")
                || lower.contains("plaid")
            {
                ("double", if lower.contains("diag") { 450 } else { 0 })
            } else if lower.contains("horz") {
                ("single", 0)
            } else if lower.contains("vert") {
                ("single", 900)
            } else if lower.contains("dndiag") {
                ("single", 1350)
            } else {
                ("single", 450)
            };
            let distance =
                if name.starts_with("dk") || name.starts_with("nar") || name.starts_with("sm") {
                    "0.05cm"
                } else if name.starts_with("wd") || name.starts_with("lg") {
                    "0.2cm"
                } else {
                    "0.1cm"
                };
            let fg =
                p.fg.as_ref()
                    .map_or_else(|| "#000000".to_owned(), |c| css(c, ctx).0);
            let el = El::new("draw:hatch")
                .with("draw:style", style)
                .with("draw:color", fg)
                .with("draw:distance", distance)
                .with("draw:rotation", rotation.to_string())
                .with("loext:preset", name);
            let hatch = defs.add("Hatch", el);
            gp.set("draw:fill", "hatch");
            gp.set("draw:fill-hatch-name", hatch);
            match &p.bg {
                Some(bg) => {
                    gp.set("draw:fill-hatch-solid", "true");
                    gp.set("draw:fill-color", css(bg, ctx).0);
                }
                None => gp.set("draw:fill-hatch-solid", "false"),
            }
        }
        Some(Fill::Blip(b)) => {
            let Some(path) = b.embed.as_deref().and_then(image) else {
                gp.set("draw:fill", "none");
                return;
            };
            let el = El::new("draw:fill-image")
                .with("xlink:href", path)
                .with("xlink:type", "simple")
                .with("xlink:show", "embed")
                .with("xlink:actuate", "onLoad");
            let name = defs.add("Image", el);
            gp.set("draw:fill", "bitmap");
            gp.set("draw:fill-image-name", name);
            gp.set(
                "style:repeat",
                if b.mode == BlipFillMode::Tile {
                    "repeat"
                } else {
                    "stretch"
                },
            );
        }
    }
}

/// Writes an outline into graphic properties (`None` = not stated).
pub fn write_line(gp: &mut El, line: Option<&Line>, ctx: &ColorContext, defs: &mut GraphicDefs) {
    let Some(l) = line else {
        return;
    };
    match &l.fill {
        Some(Fill::None) => {
            gp.set("draw:stroke", "none");
            return;
        }
        Some(Fill::Solid(c)) => {
            let (hex, a) = css(c, ctx);
            gp.set("svg:stroke-color", hex);
            if a < 1.0 {
                gp.set("svg:stroke-opacity", pct(a));
            }
        }
        Some(Fill::Gradient(g)) => {
            if let Some(s) = g.stops.first() {
                gp.set("svg:stroke-color", css(&s.color, ctx).0);
            }
        }
        _ => {}
    }
    if let Some(w) = l.width {
        gp.set("svg:stroke-width", emu_str(w));
    }
    let dash = match &l.dash {
        Some(LineDash::Preset(p)) if *p != PresetLineDashVal::Solid => Some(*p),
        Some(LineDash::Custom(_)) => Some(PresetLineDashVal::Dash),
        _ => None,
    };
    match dash {
        Some(p) => {
            let (dots1, dots2, len1, len2, dist) = match p {
                PresetLineDashVal::Dot | PresetLineDashVal::SysDot => (1, 0, "100%", "0%", "100%"),
                PresetLineDashVal::Dash | PresetLineDashVal::SysDash => {
                    (1, 0, "300%", "0%", "100%")
                }
                PresetLineDashVal::LgDash => (1, 0, "800%", "0%", "300%"),
                PresetLineDashVal::DashDot | PresetLineDashVal::SysDashDot => {
                    (1, 1, "300%", "100%", "100%")
                }
                PresetLineDashVal::LgDashDot => (1, 1, "800%", "100%", "300%"),
                _ => (1, 2, "800%", "100%", "300%"),
            };
            let el = El::new("draw:stroke-dash")
                .with("draw:style", "rect")
                .with("draw:dots1", dots1.to_string())
                .with("draw:dots1-length", len1)
                .with("draw:dots2", dots2.to_string())
                .with("draw:dots2-length", len2)
                .with("draw:distance", dist)
                .with("loext:preset", p.as_str());
            let name = defs.add("Dash", el);
            gp.set("draw:stroke", "dash");
            gp.set("draw:stroke-dash", name);
        }
        None => gp.set("draw:stroke", "solid"),
    }
    let marker = |e: &kiln_ooxml::drawingml::line::LineEnd| match e.end_type {
        kiln_ooxml::st::LineEndType::None => None,
        kiln_ooxml::st::LineEndType::Oval => Some("Circle"),
        kiln_ooxml::st::LineEndType::Diamond => Some("Square_20_45"),
        _ => Some("Arrow"),
    };
    let width = |e: &kiln_ooxml::drawingml::line::LineEnd, line: i64| {
        let k = match e.w {
            Some(kiln_ooxml::st::LineEndSize::Sm) => 2,
            Some(kiln_ooxml::st::LineEndSize::Lg) => 5,
            _ => 3,
        };
        emu_str((line * k).max(108_000))
    };
    let lw = l.width.unwrap_or(9_525);
    if let Some(m) = l
        .head
        .as_ref()
        .and_then(|e| marker(e).map(|m| (m, width(e, lw))))
    {
        gp.set("draw:marker-start", m.0);
        gp.set("draw:marker-start-width", m.1);
    }
    if let Some(m) = l
        .tail
        .as_ref()
        .and_then(|e| marker(e).map(|m| (m, width(e, lw))))
    {
        gp.set("draw:marker-end", m.0);
        gp.set("draw:marker-end-width", m.1);
    }
}

/// The `draw:marker`s line ends use: arrow, circle and diamond.
#[must_use]
pub fn markers() -> Vec<El> {
    vec![
        El::new("draw:marker")
            .with("draw:name", "Arrow")
            .with("svg:viewBox", "0 0 20 30")
            .with("svg:d", "M10 0l-10 30h20z"),
        El::new("draw:marker")
            .with("draw:name", "Circle")
            .with("draw:display-name", "Circle")
            .with("svg:viewBox", "0 0 20 20")
            .with(
                "svg:d",
                "M10 0c5.5 0 10 4.5 10 10s-4.5 10-10 10-10-4.5-10-10 4.5-10 10-10z",
            ),
        El::new("draw:marker")
            .with("draw:name", "Square_20_45")
            .with("draw:display-name", "Square 45")
            .with("svg:viewBox", "0 0 20 20")
            .with("svg:d", "M10 0l10 10-10 10-10-10z"),
    ]
}

/// Writes the effects LibreOffice shows: the outer shadow, glow and soft edges.
pub fn write_effects(
    gp: &mut El,
    effects: Option<&kiln_ooxml::drawingml::effects::EffectList>,
    ctx: &ColorContext,
) {
    let Some(e) = effects else {
        gp.set("draw:shadow", "hidden");
        return;
    };
    match &e.outer_shadow {
        Some(sh) => {
            let (hex, a) = css(&sh.color, ctx);
            let rad = sh.dir.to_radians();
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            let (dx, dy) = (
                (sh.dist as f64 * rad.cos()).round() as i64,
                (sh.dist as f64 * rad.sin()).round() as i64,
            );
            gp.set("draw:shadow", "visible");
            gp.set("draw:shadow-offset-x", emu_str(dx));
            gp.set("draw:shadow-offset-y", emu_str(dy));
            gp.set("draw:shadow-color", hex);
            gp.set("draw:shadow-opacity", pct(a));
            gp.set("loext:shadow-blur", emu_str(sh.blur_rad));
        }
        None => gp.set("draw:shadow", "hidden"),
    }
    if let Some(g) = &e.glow {
        let (hex, a) = css(&g.color, ctx);
        gp.set("loext:glow-radius", emu_str(g.rad));
        gp.set("loext:glow-color", hex);
        gp.set("loext:glow-transparency", pct(1.0 - a));
    }
    if let Some(r) = e.soft_edge {
        gp.set("loext:softedge-radius", emu_str(r));
    }
}

/// Reads the effects of graphic properties (nearest first).
#[must_use]
pub fn read_effects(chain: &[&XmlElement]) -> Option<kiln_ooxml::drawingml::effects::EffectList> {
    use kiln_ooxml::drawingml::effects::{EffectList, Glow, OuterShadow};
    let mut list = EffectList::default();
    let mut any = false;
    if chain_attr(chain, "draw:shadow") == Some("visible") {
        let dx = chain_attr(chain, "draw:shadow-offset-x")
            .and_then(emu)
            .unwrap_or(0);
        let dy = chain_attr(chain, "draw:shadow-offset-y")
            .and_then(emu)
            .unwrap_or(0);
        #[allow(clippy::cast_precision_loss)]
        let (fx, fy) = (dx as f64, dy as f64);
        let opacity = chain_attr(chain, "draw:shadow-opacity").and_then(percent);
        list.outer_shadow = Some(OuterShadow {
            blur_rad: chain_attr(chain, "loext:shadow-blur")
                .and_then(emu)
                .unwrap_or(0),
            #[allow(clippy::cast_possible_truncation)]
            dist: fx.hypot(fy).round() as i64,
            dir: fy.atan2(fx).to_degrees().rem_euclid(360.0),
            sx: 1.0,
            sy: 1.0,
            kx: 0.0,
            ky: 0.0,
            algn: kiln_ooxml::st::RectAlignment::Bl,
            rot_with_shape: false,
            color: with_alpha(
                color_attr(chain_attr(chain, "draw:shadow-color"))
                    .unwrap_or_else(|| DrawingColor::srgb("000000")),
                opacity,
            ),
        });
        any = true;
    }
    if let Some(r) = chain_attr(chain, "loext:glow-radius")
        .and_then(emu)
        .filter(|r| *r > 0)
    {
        let t = chain_attr(chain, "loext:glow-transparency")
            .and_then(percent)
            .unwrap_or(0.0);
        list.glow = Some(Glow {
            rad: r,
            color: with_alpha(
                color_attr(chain_attr(chain, "loext:glow-color"))
                    .unwrap_or_else(|| DrawingColor::srgb("ffff00")),
                Some(100.0 - t),
            ),
        });
        any = true;
    }
    if let Some(r) = chain_attr(chain, "loext:softedge-radius")
        .and_then(emu)
        .filter(|r| *r > 0)
    {
        list.soft_edge = Some(r);
        any = true;
    }
    any.then_some(list)
}

/// The effects a shape shows: its own, else its style's from the theme.
#[must_use]
pub fn effective_effects(
    props: &ShapeProps,
    theme: &Theme,
    ctx: &ColorContext,
) -> (
    Option<kiln_ooxml::drawingml::effects::EffectList>,
    ColorContext,
) {
    if let Some(e) = &props.effects {
        return (Some(e.clone()), *ctx);
    }
    let Some(st) = &props.style else {
        return (None, *ctx);
    };
    if st.effect.idx == 0 {
        return (None, *ctx);
    }
    let mut c = *ctx;
    c.ph_clr = st.effect.color.as_ref().map(|col| resolve_color(col, ctx));
    let e = theme
        .format_scheme
        .effect_styles
        .get((st.effect.idx - 1) as usize)
        .and_then(|s| s.effects.clone());
    (e, c)
}

fn named<'a>(styles: Option<&'a XmlElement>, element: &str, name: &str) -> Option<&'a XmlElement> {
    let s = styles?;
    s.descendants(element)
        .into_iter()
        .find(|e| e.attr("draw:name") == Some(name))
}

fn color_attr(v: Option<&str>) -> Option<DrawingColor> {
    let v = v?.trim();
    let hex = v.strip_prefix('#')?;
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| DrawingColor::srgb(hex))
}

fn with_alpha(mut c: DrawingColor, opacity: Option<f64>) -> DrawingColor {
    if let Some(o) = opacity.filter(|o| *o < 100.0) {
        c.transforms
            .push(kiln_ooxml::drawingml::color::ColorTransform::new(
                "alpha",
                (o.clamp(0.0, 100.0) * 1000.0).round() as i64,
            ));
    }
    c
}

/// The attribute along a graphic style's chain (the style first, then its parents).
#[must_use]
pub fn chain_attr<'a>(chain: &[&'a XmlElement], attr: &str) -> Option<&'a str> {
    chain.iter().find_map(|gp| gp.attr(attr))
}

/// Reads the fill of graphic properties (the chain of a style and its parents, nearest first).
pub fn read_fill(
    chain: &[&XmlElement],
    styles: Option<&XmlElement>,
    media: &mut dyn FnMut(&str) -> Option<String>,
) -> Option<Fill> {
    let kind = chain_attr(chain, "draw:fill")?;
    let opacity = chain_attr(chain, "draw:opacity").and_then(percent);
    Some(match kind {
        "none" => Fill::None,
        "solid" => Fill::Solid(with_alpha(
            color_attr(chain_attr(chain, "draw:fill-color"))
                .unwrap_or_else(|| DrawingColor::srgb("729fcf")),
            opacity,
        )),
        "gradient" => {
            let g = chain_attr(chain, "draw:fill-gradient-name")
                .and_then(|n| named(styles, "draw:gradient", n));
            let Some(g) = g else {
                return Some(Fill::Solid(DrawingColor::srgb("729fcf")));
            };
            let start = color_attr(g.attr("draw:start-color"))
                .unwrap_or_else(|| DrawingColor::srgb("000000"));
            let end = color_attr(g.attr("draw:end-color"))
                .unwrap_or_else(|| DrawingColor::srgb("ffffff"));
            let angle = g
                .attr("draw:angle")
                .and_then(|a| {
                    if let Some(d) = a.strip_suffix("deg") {
                        d.trim().parse::<f64>().ok().map(|v| v * 10.0)
                    } else if let Some(d) = a.strip_suffix("grad") {
                        d.trim().parse::<f64>().ok().map(|v| v * 9.0)
                    } else if let Some(d) = a.strip_suffix("rad") {
                        d.trim().parse::<f64>().ok().map(|v| v.to_degrees() * 10.0)
                    } else {
                        // Unitless: degrees since ODF 1.2, tenths of a degree before (values
                        // above a full turn can only be the old form).
                        a.trim()
                            .parse::<f64>()
                            .ok()
                            .map(|v| if v > 360.0 { v } else { v * 10.0 })
                    }
                })
                .unwrap_or(0.0);
            let radial = matches!(
                g.attr("draw:style"),
                Some("radial" | "ellipsoid" | "square" | "rectangular")
            );
            Fill::Gradient(GradientFill {
                stops: vec![
                    GradientStop {
                        pos: 0.0,
                        color: start,
                    },
                    GradientStop {
                        pos: 1.0,
                        color: end,
                    },
                ],
                linear: (!radial).then(|| LinearShade {
                    angle: (450.0 - angle / 10.0).rem_euclid(360.0),
                    scaled: false,
                }),
                path: radial.then_some(kiln_ooxml::drawingml::fill::PathShade {
                    shade: kiln_ooxml::st::PathShadeType::Circle,
                    fill_to_rect: None,
                }),
                tile_rect: None,
                flip: None,
                rot_with_shape: None,
            })
        }
        "bitmap" => {
            let key = chain_attr(chain, "draw:fill-image-name")
                .and_then(|n| named(styles, "draw:fill-image", n))
                .and_then(|i| i.attr("xlink:href"))
                .and_then(media);
            match key {
                Some(k) => Fill::Blip(BlipFill {
                    embed: Some(k),
                    mode: if chain_attr(chain, "style:repeat") == Some("repeat") {
                        BlipFillMode::Tile
                    } else {
                        BlipFillMode::Stretch
                    },
                    ..BlipFill::default()
                }),
                None => Fill::None,
            }
        }
        "hatch" => {
            let h = chain_attr(chain, "draw:fill-hatch-name")
                .and_then(|n| named(styles, "draw:hatch", n));
            let preset = h
                .and_then(|h| h.attr("loext:preset"))
                .and_then(kiln_ooxml::st::PresetPatternVal::parse)
                .unwrap_or_else(|| {
                    let rot = h
                        .and_then(|h| h.attr("draw:rotation"))
                        .and_then(|r| r.parse::<i32>().ok())
                        .unwrap_or(0);
                    match (h.and_then(|h| h.attr("draw:style")), rot.rem_euclid(1800)) {
                        (Some("double" | "triple"), r) if r != 0 => {
                            kiln_ooxml::st::PresetPatternVal::DiagCross
                        }
                        (Some("double" | "triple"), _) => kiln_ooxml::st::PresetPatternVal::Cross,
                        (_, 0) => kiln_ooxml::st::PresetPatternVal::Horz,
                        (_, 900) => kiln_ooxml::st::PresetPatternVal::Vert,
                        (_, r) if r > 900 => kiln_ooxml::st::PresetPatternVal::DnDiag,
                        _ => kiln_ooxml::st::PresetPatternVal::UpDiag,
                    }
                });
            let fg = color_attr(h.and_then(|h| h.attr("draw:color")))
                .unwrap_or_else(|| DrawingColor::srgb("3465a4"));
            let bg = (chain_attr(chain, "draw:fill-hatch-solid") != Some("false"))
                .then(|| color_attr(chain_attr(chain, "draw:fill-color")))
                .flatten();
            Fill::Pattern(kiln_ooxml::drawingml::fill::PatternFill {
                preset,
                fg: Some(fg),
                bg,
            })
        }
        _ => return None,
    })
}

/// Reads the outline of graphic properties (nearest first).
#[must_use]
pub fn read_line(chain: &[&XmlElement], styles: Option<&XmlElement>) -> Option<Line> {
    let stroke = chain_attr(chain, "draw:stroke");
    let width = chain_attr(chain, "svg:stroke-width").and_then(emu);
    let color = color_attr(chain_attr(chain, "svg:stroke-color"));
    if stroke.is_none() && width.is_none() && color.is_none() {
        return None;
    }
    let mut l = Line::default();
    if stroke == Some("none") {
        l.fill = Some(Fill::None);
        return Some(l);
    }
    l.width = width;
    l.fill = Some(Fill::Solid(with_alpha(
        color.unwrap_or_else(|| DrawingColor::srgb("3465a4")),
        chain_attr(chain, "svg:stroke-opacity").and_then(percent),
    )));
    if stroke == Some("dash") {
        let preset = chain_attr(chain, "draw:stroke-dash")
            .and_then(|n| named(styles, "draw:stroke-dash", n))
            .and_then(|d| {
                d.attr("loext:preset")
                    .and_then(PresetLineDashVal::parse)
                    .or_else(|| {
                        let len = d
                            .attr("draw:dots1-length")
                            .and_then(percent)
                            .unwrap_or(100.0);
                        let two = d
                            .attr("draw:dots2")
                            .and_then(|v| v.parse::<u32>().ok())
                            .unwrap_or(0);
                        Some(match (len >= 250.0, two) {
                            (false, 0) => PresetLineDashVal::Dot,
                            (true, 0) => PresetLineDashVal::Dash,
                            (_, 1) => PresetLineDashVal::DashDot,
                            _ => PresetLineDashVal::LgDashDotDot,
                        })
                    })
            })
            .unwrap_or(PresetLineDashVal::Dash);
        l.dash = Some(LineDash::Preset(preset));
    }
    let arrow = || kiln_ooxml::drawingml::line::LineEnd {
        end_type: kiln_ooxml::st::LineEndType::Triangle,
        w: None,
        len: None,
    };
    let end = |m: &str| {
        let mut e = arrow();
        let lower = m.to_ascii_lowercase();
        if lower.contains("circle") || lower.contains("oval") {
            e.end_type = kiln_ooxml::st::LineEndType::Oval;
        } else if lower.contains("square") || lower.contains("diamond") {
            e.end_type = kiln_ooxml::st::LineEndType::Diamond;
        }
        e
    };
    if let Some(m) = chain_attr(chain, "draw:marker-start").filter(|m| !m.is_empty()) {
        l.head = Some(end(m));
    }
    if let Some(m) = chain_attr(chain, "draw:marker-end").filter(|m| !m.is_empty()) {
        l.tail = Some(end(m));
    }
    Some(l)
}

/// The `style:graphic-properties` of a style and its parents (nearest first), in a scope.
#[must_use]
pub fn graphic_chain<'a>(
    ix: &StyleIndex<'a>,
    scope: Scope,
    family: &str,
    name: Option<&str>,
) -> Vec<&'a XmlElement> {
    let mut out = Vec::new();
    let mut cur = name.and_then(|n| ix.get(scope, family, n));
    let mut guard = 0;
    while let Some(s) = cur {
        if let Some(gp) = crate::xml::kid(s, "style:graphic-properties") {
            out.push(gp);
        }
        guard += 1;
        if guard > 16 {
            break;
        }
        cur = s
            .attr("style:parent-style-name")
            .and_then(|p| ix.get(Scope::Styles, family, p));
    }
    out
}

/// The pixel size of a PNG, JPEG, GIF or BMP image (`None` for anything else).
#[must_use]
pub fn image_size(b: &[u8]) -> Option<(u32, u32)> {
    let be32 =
        |i: usize| -> Option<u32> { Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?)) };
    let be16 = |i: usize| -> Option<u32> {
        Some(u32::from(u16::from_be_bytes(
            b.get(i..i + 2)?.try_into().ok()?,
        )))
    };
    let le16 = |i: usize| -> Option<u32> {
        Some(u32::from(u16::from_le_bytes(
            b.get(i..i + 2)?.try_into().ok()?,
        )))
    };
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((be32(16)?, be32(20)?)).filter(|(w, h)| *w > 0 && *h > 0);
    }
    if b.starts_with(b"GIF8") {
        return Some((le16(6)?, le16(8)?)).filter(|(w, h)| *w > 0 && *h > 0);
    }
    if b.starts_with(b"BM") {
        let w = i32::from_le_bytes(b.get(18..22)?.try_into().ok()?);
        let h = i32::from_le_bytes(b.get(22..26)?.try_into().ok()?);
        return Some((w.unsigned_abs(), h.unsigned_abs())).filter(|(w, h)| *w > 0 && *h > 0);
    }
    if b.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 9 < b.len() {
            if b[i] != 0xFF {
                i += 1;
                continue;
            }
            let m = b[i + 1];
            if (0xC0..=0xCF).contains(&m) && m != 0xC4 && m != 0xC8 && m != 0xCC {
                return Some((be16(i + 7)?, be16(i + 5)?)).filter(|(w, h)| *w > 0 && *h > 0);
            }
            if m == 0xD8 || m == 0x01 || (0xD0..=0xD7).contains(&m) {
                i += 2;
                continue;
            }
            let len = be16(i + 2)? as usize;
            i += 2 + len.max(2);
        }
    }
    None
}

/// `fo:clip` for a crop (1000ths of a percent per edge) of an image of `px` pixels at 96 dpi.
#[must_use]
pub fn clip_attr(crop: &kiln_deck::shape::Crop, px: (u32, u32)) -> String {
    let len = |frac: i32, size: u32| -> String {
        let inches = f64::from(frac) / 100_000.0 * f64::from(size) / 96.0;
        #[allow(clippy::cast_possible_truncation)]
        emu_str((inches * 914_400.0).round() as i64)
    };
    format!(
        "rect({}, {}, {}, {})",
        len(crop.t, px.1),
        len(crop.r, px.0),
        len(crop.b, px.1),
        len(crop.l, px.0)
    )
}

/// A crop from `fo:clip` for an image of `px` pixels at 96 dpi.
#[must_use]
pub fn crop_of(clip: &str, px: (u32, u32)) -> kiln_deck::shape::Crop {
    let Some(inner) = clip
        .trim()
        .strip_prefix("rect(")
        .and_then(|r| r.strip_suffix(')'))
    else {
        return kiln_deck::shape::Crop::default();
    };
    let v: Vec<f64> = inner
        .split([',', ' '])
        .filter(|s| !s.is_empty() && *s != "auto")
        .map(|s| crate::units::parse_inches(s).unwrap_or(0.0))
        .collect();
    if v.len() != 4 {
        return kiln_deck::shape::Crop::default();
    }
    let frac = |inches: f64, size: u32| -> i32 {
        if size == 0 {
            return 0;
        }
        #[allow(clippy::cast_possible_truncation)]
        let r = (inches * 96.0 / f64::from(size) * 100_000.0).round() as i32;
        r
    };
    kiln_deck::shape::Crop {
        t: frac(v[0], px.1),
        r: frac(v[1], px.0),
        b: frac(v[2], px.1),
        l: frac(v[3], px.0),
    }
}
