// SPDX-License-Identifier: AGPL-3.0-or-later
//! DrawingML fills (ECMA-376 Part 1 §20.1.8): none, solid, gradient, pattern, picture and group.

use super::color::{
    ColorContext, DrawingColor, Rgba, parse_color_child, resolve_color, rgba_to_css,
    serialize_color,
};
use crate::st::{PathShadeType, PresetPatternVal, RectAlignment, TileFlipMode};
use crate::units::{parse_percentage, round_to_i64};
use crate::xml::{XmlElement, format_number};

/// Insets from each edge as fractions (1 = 100 %): `a:fillToRect`, `a:srcRect`, `a:fillRect`,
/// `a:tileRect`. Negative values extend beyond the edge.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RelativeRect {
    /// Left inset.
    pub l: f64,
    /// Top inset.
    pub t: f64,
    /// Right inset.
    pub r: f64,
    /// Bottom inset.
    pub b: f64,
}

impl RelativeRect {
    /// True when every inset is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.l == 0.0 && self.t == 0.0 && self.r == 0.0 && self.b == 0.0
    }

    fn parse(node: Option<&XmlElement>) -> Option<Self> {
        let node = node?;
        let f = |k: &str| node.attr(k).and_then(parse_percentage).unwrap_or(0.0);
        Some(Self {
            l: f("l"),
            t: f("t"),
            r: f("r"),
            b: f("b"),
        })
    }

    fn to_element(self, name: &str) -> XmlElement {
        let v = |x: f64| (x != 0.0).then(|| round_to_i64(x * 100_000.0));
        XmlElement::new(name)
            .with_opt_attr("l", v(self.l))
            .with_opt_attr("t", v(self.t))
            .with_opt_attr("r", v(self.r))
            .with_opt_attr("b", v(self.b))
    }
}

/// A gradient stop.
#[derive(Debug, Clone, PartialEq)]
pub struct GradientStop {
    /// Position, 0–1.
    pub pos: f64,
    /// Colour at the stop.
    pub color: DrawingColor,
}

/// A linear gradient direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearShade {
    /// Angle in degrees, clockwise from left-to-right.
    pub angle: f64,
    /// Whether the angle scales with the shape's aspect ratio.
    pub scaled: bool,
}

/// A path (radial/rectangular) gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathShade {
    /// The path shape.
    pub shade: PathShadeType,
    /// The focus rectangle.
    pub fill_to_rect: Option<RelativeRect>,
}

/// `a:gradFill`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GradientFill {
    /// Stops in document order.
    pub stops: Vec<GradientStop>,
    /// A linear gradient.
    pub linear: Option<LinearShade>,
    /// A path gradient.
    pub path: Option<PathShade>,
    /// The tile rectangle.
    pub tile_rect: Option<RelativeRect>,
    /// Tile flipping.
    pub flip: Option<TileFlipMode>,
    /// Whether the fill rotates with the shape.
    pub rot_with_shape: Option<bool>,
}

/// `a:pattFill`.
#[derive(Debug, Clone, PartialEq)]
pub struct PatternFill {
    /// The pattern.
    pub preset: PresetPatternVal,
    /// Foreground colour.
    pub fg: Option<DrawingColor>,
    /// Background colour.
    pub bg: Option<DrawingColor>,
}

/// How a picture fills its area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlipFillMode {
    /// `a:stretch`.
    Stretch,
    /// `a:tile`.
    Tile,
    /// Neither (the picture is drawn at its size).
    #[default]
    None,
}

/// `a:tile` settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileInfo {
    /// Horizontal offset, EMU.
    pub tx: i64,
    /// Vertical offset, EMU.
    pub ty: i64,
    /// Horizontal scale (1 = 100 %).
    pub sx: f64,
    /// Vertical scale.
    pub sy: f64,
    /// Flipping.
    pub flip: TileFlipMode,
    /// Alignment.
    pub algn: RectAlignment,
}

/// A picture fill (`a:blipFill`, `p:blipFill`, `pic:blipFill`, `xdr:blipFill`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlipFill {
    /// Relationship id of the embedded image (`r:embed`).
    pub embed: Option<String>,
    /// Relationship id of a linked image (`r:link`).
    pub link: Option<String>,
    /// Crop (`a:srcRect`).
    pub src_rect: Option<RelativeRect>,
    /// Stretch, tile or neither.
    pub mode: BlipFillMode,
    /// `a:stretch/a:fillRect` insets.
    pub fill_rect: Option<RelativeRect>,
    /// Tile settings.
    pub tile: Option<TileInfo>,
    /// `dpi`.
    pub dpi: Option<i64>,
    /// `rotWithShape`.
    pub rot_with_shape: Option<bool>,
    /// `a:blip/a:alphaModFix/@amt` as a fraction.
    pub alpha: Option<f64>,
    /// `a:blip/@cstate` (`print`, `screen`, `email`, `hqprint`).
    pub compression_state: Option<String>,
    /// Other `a:blip` children (effects, `a:extLst` with SVG or HD images), preserved as
    /// written.
    pub blip_extras: Vec<XmlElement>,
}

/// A DrawingML fill.
#[derive(Debug, Clone, PartialEq)]
pub enum Fill {
    /// `a:noFill`.
    None,
    /// `a:solidFill`.
    Solid(DrawingColor),
    /// `a:gradFill`.
    Gradient(GradientFill),
    /// `a:pattFill`.
    Pattern(PatternFill),
    /// A picture fill.
    Blip(BlipFill),
    /// `a:grpFill`: the group's fill.
    Group,
}

/// The fill elements.
pub const FILL_ELEMENTS: &[&str] = &[
    "a:noFill",
    "a:solidFill",
    "a:gradFill",
    "a:pattFill",
    "a:blipFill",
    "a:grpFill",
];

/// Parses a fill element. `p:blipFill`, `xdr:blipFill` and `pic:blipFill` are accepted as picture
/// fills.
#[must_use]
pub fn parse_fill(node: &XmlElement) -> Option<Fill> {
    let local = crate::xml::namespaces::local_name(&node.name);
    Some(match local {
        "noFill" => Fill::None,
        "grpFill" => Fill::Group,
        "solidFill" => parse_color_child(Some(node)).map_or(Fill::None, Fill::Solid),
        "gradFill" => {
            let stops = node
                .child("a:gsLst")
                .map(|l| {
                    l.children_named("a:gs")
                        .filter_map(|gs| {
                            Some(GradientStop {
                                pos: gs.attr("pos").and_then(parse_percentage).unwrap_or(0.0),
                                color: parse_color_child(Some(gs))?,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let linear = node.child("a:lin").map(|lin| LinearShade {
                #[allow(clippy::cast_precision_loss)]
                angle: lin.attr_int("ang").unwrap_or(0) as f64 / 60_000.0,
                scaled: lin.attr_bool("scaled").unwrap_or(false),
            });
            let path = node.child("a:path").map(|p| PathShade {
                shade: p
                    .attr("path")
                    .and_then(PathShadeType::parse)
                    .unwrap_or(PathShadeType::Circle),
                fill_to_rect: RelativeRect::parse(p.child("a:fillToRect")),
            });
            Fill::Gradient(GradientFill {
                stops,
                linear,
                path,
                tile_rect: RelativeRect::parse(node.child("a:tileRect")),
                flip: node.attr("flip").and_then(TileFlipMode::parse),
                rot_with_shape: node.attr_bool("rotWithShape"),
            })
        }
        "pattFill" => Fill::Pattern(PatternFill {
            preset: node
                .attr("prst")
                .and_then(PresetPatternVal::parse)
                .unwrap_or(PresetPatternVal::Pct5),
            fg: parse_color_child(node.child("a:fgClr")),
            bg: parse_color_child(node.child("a:bgClr")),
        }),
        "blipFill" => {
            let blip = node.child("a:blip");
            let mut f = BlipFill {
                embed: blip
                    .and_then(|b| b.attr("r:embed"))
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned),
                link: blip
                    .and_then(|b| b.attr("r:link"))
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned),
                compression_state: blip
                    .and_then(|b| b.attr("cstate"))
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned),
                dpi: node.attr_int("dpi"),
                rot_with_shape: node.attr_bool("rotWithShape"),
                ..BlipFill::default()
            };
            for c in blip.into_iter().flat_map(XmlElement::elements) {
                if c.name == "a:alphaModFix" {
                    f.alpha = Some(c.attr("amt").and_then(parse_percentage).unwrap_or(1.0));
                } else {
                    f.blip_extras.push(c.clone());
                }
            }
            f.src_rect = RelativeRect::parse(node.child("a:srcRect")).filter(|r| !r.is_zero());
            if let Some(stretch) = node.child("a:stretch") {
                f.mode = BlipFillMode::Stretch;
                f.fill_rect =
                    RelativeRect::parse(stretch.child("a:fillRect")).filter(|r| !r.is_zero());
            } else if let Some(tile) = node.child("a:tile") {
                f.mode = BlipFillMode::Tile;
                f.tile = Some(TileInfo {
                    tx: tile.attr_int("tx").unwrap_or(0),
                    ty: tile.attr_int("ty").unwrap_or(0),
                    sx: tile.attr("sx").and_then(parse_percentage).unwrap_or(1.0),
                    sy: tile.attr("sy").and_then(parse_percentage).unwrap_or(1.0),
                    flip: tile
                        .attr("flip")
                        .and_then(TileFlipMode::parse)
                        .unwrap_or(TileFlipMode::None),
                    algn: tile
                        .attr("algn")
                        .and_then(RectAlignment::parse)
                        .unwrap_or(RectAlignment::Tl),
                });
            }
            Fill::Blip(f)
        }
        _ => return None,
    })
}

/// The first fill child of a properties element (`p:spPr`, `a:rPr`, `a:ln`, `a:tcPr`, …).
#[must_use]
pub fn find_fill(container: &XmlElement) -> Option<Fill> {
    container
        .elements()
        .find(|c| FILL_ELEMENTS.contains(&c.name.as_str()))
        .and_then(parse_fill)
}

/// Serialises a fill; picture fills use `blip_fill_name` (`p:blipFill` in a `p:pic`,
/// `pic:blipFill` in WordprocessingML, `a:blipFill` elsewhere).
#[must_use]
pub fn serialize_fill_named(fill: &Fill, blip_fill_name: &str) -> XmlElement {
    match fill {
        Fill::None => XmlElement::new("a:noFill"),
        Fill::Group => XmlElement::new("a:grpFill"),
        Fill::Solid(c) => XmlElement::new("a:solidFill").with_child(serialize_color(c)),
        Fill::Gradient(g) => XmlElement::new("a:gradFill")
            .with_opt_attr("flip", g.flip)
            .with_opt_attr("rotWithShape", g.rot_with_shape)
            .with_child(
                XmlElement::new("a:gsLst").with_children(g.stops.iter().map(|s| {
                    XmlElement::new("a:gs")
                        .with_attr("pos", round_to_i64(s.pos * 100_000.0))
                        .with_child(serialize_color(&s.color))
                })),
            )
            .with_opt_child(g.linear.map(|l| {
                XmlElement::new("a:lin")
                    .with_attr("ang", round_to_i64(l.angle * 60_000.0))
                    .with_attr("scaled", l.scaled)
            }))
            .with_opt_child(g.path.map(|p| {
                XmlElement::new("a:path")
                    .with_attr("path", p.shade)
                    .with_opt_child(p.fill_to_rect.map(|r| r.to_element("a:fillToRect")))
            }))
            .with_opt_child(g.tile_rect.map(|r| r.to_element("a:tileRect"))),
        Fill::Pattern(p) => XmlElement::new("a:pattFill")
            .with_attr("prst", p.preset)
            .with_opt_child(
                p.fg.as_ref()
                    .map(|c| XmlElement::new("a:fgClr").with_child(serialize_color(c))),
            )
            .with_opt_child(
                p.bg.as_ref()
                    .map(|c| XmlElement::new("a:bgClr").with_child(serialize_color(c))),
            ),
        Fill::Blip(f) => {
            let blip = XmlElement::new("a:blip")
                .with_opt_attr("r:embed", f.embed.as_deref())
                .with_opt_attr("r:link", f.link.as_deref())
                .with_opt_attr("cstate", f.compression_state.as_deref())
                .with_opt_child(f.alpha.map(|a| {
                    XmlElement::new("a:alphaModFix").with_attr("amt", round_to_i64(a * 100_000.0))
                }))
                .with_children(f.blip_extras.iter().cloned());
            let mode = match f.mode {
                BlipFillMode::Stretch => Some(
                    XmlElement::new("a:stretch")
                        .with_child(f.fill_rect.unwrap_or_default().to_element("a:fillRect")),
                ),
                BlipFillMode::Tile => f.tile.map(|t| {
                    XmlElement::new("a:tile")
                        .with_attr("tx", t.tx)
                        .with_attr("ty", t.ty)
                        .with_attr("sx", round_to_i64(t.sx * 100_000.0))
                        .with_attr("sy", round_to_i64(t.sy * 100_000.0))
                        .with_attr("flip", t.flip)
                        .with_attr("algn", t.algn)
                }),
                BlipFillMode::None => None,
            };
            XmlElement::new(blip_fill_name)
                .with_opt_attr("dpi", f.dpi)
                .with_opt_attr("rotWithShape", f.rot_with_shape)
                .with_child(blip)
                .with_opt_child(f.src_rect.map(|r| r.to_element("a:srcRect")))
                .with_opt_child(mode)
        }
    }
}

/// Serialises a fill (picture fills as `a:blipFill`).
#[must_use]
pub fn serialize_fill(fill: &Fill) -> XmlElement {
    serialize_fill_named(fill, "a:blipFill")
}

fn round2(n: f64) -> f64 {
    crate::units::round_to(n, 2)
}

/// A CSS `background` value for solid and gradient fills. Pattern fills fall back to their
/// foreground colour; picture fills need the caller's image URL and return `None`.
#[must_use]
pub fn fill_to_css(fill: &Fill, ctx: &ColorContext) -> Option<String> {
    match fill {
        Fill::None => Some("transparent".to_owned()),
        Fill::Solid(c) => Some(rgba_to_css(resolve_color(c, ctx))),
        Fill::Pattern(p) => p.fg.as_ref().map(|c| rgba_to_css(resolve_color(c, ctx))),
        Fill::Gradient(g) => {
            let mut stops: Vec<&GradientStop> = g.stops.iter().collect();
            stops.sort_by(|a, b| a.pos.total_cmp(&b.pos));
            let stops = stops
                .iter()
                .map(|s| {
                    format!(
                        "{} {}%",
                        rgba_to_css(resolve_color(&s.color, ctx)),
                        format_number(round2(s.pos * 100.0))
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            if let Some(path) = g.path {
                let r = path.fill_to_rect.unwrap_or(RelativeRect {
                    l: 0.5,
                    t: 0.5,
                    r: 0.5,
                    b: 0.5,
                });
                let cx = round2((r.l + (1.0 - r.r)) / 2.0 * 100.0);
                let cy = round2((r.t + (1.0 - r.b)) / 2.0 * 100.0);
                let shape = if path.shade == PathShadeType::Circle {
                    "circle"
                } else {
                    "ellipse"
                };
                return Some(format!(
                    "radial-gradient({shape} at {}% {}%, {stops})",
                    format_number(cx),
                    format_number(cy)
                ));
            }
            // DrawingML angles run clockwise from "left to right"; CSS's from "bottom to top".
            let angle = round2((g.linear.map_or(0.0, |l| l.angle) + 90.0) % 360.0);
            Some(format!(
                "linear-gradient({}deg, {stops})",
                format_number(angle)
            ))
        }
        Fill::Blip(_) | Fill::Group => None,
    }
}

/// The solid colour of a fill, if it has one.
#[must_use]
pub fn solid_fill_color(fill: Option<&Fill>, ctx: &ColorContext) -> Option<Rgba> {
    match fill {
        Some(Fill::Solid(c)) => Some(resolve_color(c, ctx)),
        _ => None,
    }
}
