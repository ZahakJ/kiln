// SPDX-License-Identifier: AGPL-3.0-or-later
//! Drawings: where a floating object goes on the page (`wp:anchor` positions relative to the
//! page, margins, column, paragraph, line or character), the outline text wraps around (square,
//! tight and through with the wrap polygon, a picture's alpha or a shape's geometry, top and
//! bottom), and how shapes look (DrawingML presets from `kiln-ooxml`, fills and outlines).

use kiln_doc::model::Anchor;
use kiln_doc::st::{AlignH, AlignV, RelativeFromH, RelativeFromV, WrapSide, WrapType};
use kiln_doc::{DocTheme, Drawing, DrawingKind, PosH, PosV};
use kiln_ooxml::drawingml::color::{ColorContext, parse_color_child, resolve_color};
use kiln_ooxml::drawingml::fill::{Fill, find_fill};
use kiln_ooxml::drawingml::geometry::adjust_values;
use kiln_ooxml::drawingml::line::parse_line;
use kiln_ooxml::drawingml::presets::preset_geometry;
use kiln_ooxml::xml::{XmlElement, parse_xml_str};
use kiln_scene::{Bounds, PathData};
use kiln_text::Rgba;

use crate::media::{BAND, Picture, alpha_profile};
use crate::para::Obstacle;
use crate::units::emu;

/// The page geometry floating objects are positioned against (page coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageFrame {
    /// Page width.
    pub width: f32,
    /// Page height.
    pub height: f32,
    /// Top margin.
    pub top: f32,
    /// Bottom margin.
    pub bottom: f32,
    /// Left margin.
    pub left: f32,
    /// Right margin.
    pub right: f32,
    /// The current column (x, width).
    pub column: (f32, f32),
}

/// Where the anchor is (page coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnchorPoint {
    /// Top of the anchor paragraph.
    pub para_top: f32,
    /// Top of the anchor's line.
    pub line_top: f32,
    /// x of the anchor character.
    pub char_x: f32,
}

/// The anchor of a floating drawing.
#[must_use]
pub fn anchor_of(d: &Drawing) -> Option<&Anchor> {
    match &d.placement {
        kiln_doc::Placement::Anchor(a) => Some(a),
        kiln_doc::Placement::Inline => None,
    }
}

/// The rectangle of a floating object on its page.
#[must_use]
pub fn float_rect(d: &Drawing, a: &Anchor, frame: &PageFrame, at: &AnchorPoint) -> Bounds {
    let w = emu(d.width.clamp(0, 20_116_800));
    let h = emu(d.height.clamp(0, 20_116_800));
    let content_w = (frame.width - frame.left - frame.right).max(1.0);
    let content_h = (frame.height - frame.top - frame.bottom).max(1.0);
    let (base_x, span_w) = match a.relative_h {
        RelativeFromH::Page => (0.0, frame.width),
        RelativeFromH::Margin => (frame.left, content_w),
        RelativeFromH::LeftMargin | RelativeFromH::InsideMargin => (0.0, frame.left),
        RelativeFromH::RightMargin | RelativeFromH::OutsideMargin => {
            (frame.width - frame.right, frame.right)
        }
        RelativeFromH::Character => (at.char_x, 0.0),
        _ => frame.column,
    };
    let x = match a.h {
        PosH::Offset(o) => base_x + emu(o.clamp(-50_000_000, 50_000_000)),
        PosH::Align(al) => match al {
            AlignH::Center => base_x + (span_w - w) / 2.0,
            AlignH::Right | AlignH::Outside => base_x + span_w - w,
            _ => base_x,
        },
    };
    let (base_y, span_h, flowing) = match a.relative_v {
        RelativeFromV::Page => (0.0, frame.height, false),
        RelativeFromV::Margin => (frame.top, content_h, false),
        RelativeFromV::TopMargin | RelativeFromV::InsideMargin => (0.0, frame.top, false),
        RelativeFromV::BottomMargin | RelativeFromV::OutsideMargin => {
            (frame.height - frame.bottom, frame.bottom, false)
        }
        RelativeFromV::Line => (at.line_top, 0.0, true),
        _ => (at.para_top, 0.0, true),
    };
    let mut y = match a.v {
        PosV::Offset(o) => base_y + emu(o.clamp(-50_000_000, 50_000_000)),
        PosV::Align(al) => match al {
            AlignV::Center => base_y + (span_h - h) / 2.0,
            AlignV::Bottom | AlignV::Outside => base_y + span_h - h,
            _ => base_y,
        },
    };
    if flowing {
        // Objects positioned from the text stay on the page (Word moves them up).
        y = y.min(frame.height - h).max(0.0);
    }
    Bounds::new(x, y, w, h)
}

/// The obstacle a floating object makes for text; `None` for objects text ignores (behind,
/// in front, no wrapping).
#[must_use]
pub fn obstacle(
    d: &Drawing,
    a: &Anchor,
    rect: Bounds,
    picture: Option<&Picture>,
    theme: &DocTheme,
) -> Option<Obstacle> {
    let (dl, dr) = (
        emu(a.dist_left.clamp(0, 9_144_000)),
        emu(a.dist_right.clamp(0, 9_144_000)),
    );
    let (dt, db) = (
        emu(a.dist_top.clamp(0, 9_144_000)),
        emu(a.dist_bottom.clamp(0, 9_144_000)),
    );
    let outer = Bounds::new(rect.x - dl, rect.y - dt, rect.w + dl + dr, rect.h + dt + db);
    let side = a.wrap_side.unwrap_or_default();
    match a.wrap {
        WrapType::Square => Some(Obstacle {
            rect: outer,
            profile: None,
            side,
            top_bottom: false,
        }),
        WrapType::TopAndBottom => Some(Obstacle {
            rect: outer,
            profile: None,
            side: WrapSide::BothSides,
            top_bottom: true,
        }),
        WrapType::Tight | WrapType::Through => {
            let profile = a
                .wrap_polygon_xml
                .as_deref()
                .and_then(wrap_polygon)
                .map(|poly| polygon_profile(&poly, rect, dl.max(dr), dt.max(db)))
                .or_else(|| match (&d.kind, picture) {
                    (DrawingKind::Picture { .. }, Some(p)) => Some(alpha_profile(p, rect, dl, dt)),
                    (DrawingKind::Shape { .. }, _) => {
                        let v = shape_visual(d, rect.w, rect.h, theme);
                        let mut all = PathData::new();
                        for p in &v.paths {
                            all.extend(&p.path);
                        }
                        Some(path_profile(&all, rect, dl, dt))
                    }
                    _ => None,
                })
                .filter(|p| !p.is_empty());
            Some(Obstacle {
                rect: outer,
                profile,
                side,
                top_bottom: false,
            })
        }
        _ => None,
    }
}

/// The points of a `wp:wrapPolygon` (`wp:start` and `wp:lineTo`), normalised to 0..1 of the
/// object (the polygon's coordinates are 21,600ths of the extent).
#[must_use]
pub fn wrap_polygon(xml: &str) -> Option<Vec<(f32, f32)>> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<wp:") {
        rest = &rest[i + 4..];
        if !(rest.starts_with("start") || rest.starts_with("lineTo")) {
            continue;
        }
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        let attr = |name: &str| -> Option<f32> {
            let k = tag.find(&format!(" {name}=\""))? + name.len() + 3;
            let v = &tag[k..];
            let e = v.find('"')?;
            v[..e].parse::<f32>().ok().filter(|f| f.is_finite())
        };
        if let (Some(x), Some(y)) = (attr("x"), attr("y")) {
            out.push((x / 21_600.0, y / 21_600.0));
        }
    }
    (out.len() >= 3).then_some(out)
}

/// Per-band horizontal extents of a polygon (normalised coordinates) drawn into `rect`.
#[must_use]
pub fn polygon_profile(poly: &[(f32, f32)], rect: Bounds, dx: f32, dy: f32) -> Vec<[f32; 4]> {
    let mut path = PathData::new();
    for (i, (x, y)) in poly.iter().enumerate() {
        let (px, py) = (x * rect.w, y * rect.h);
        if i == 0 {
            path.move_to(px, py);
        } else {
            path.line_to(px, py);
        }
    }
    path.close();
    path_profile(&path, rect, dx, dy)
}

/// Per-band horizontal extents of a path in object coordinates drawn into `rect`.
#[must_use]
pub fn path_profile(path: &PathData, rect: Bounds, dx: f32, dy: f32) -> Vec<[f32; 4]> {
    let bez: kurbo::BezPath = path.into();
    let mut pts: Vec<(kurbo::Point, kurbo::Point)> = Vec::new();
    let mut last = kurbo::Point::ZERO;
    let mut start = kurbo::Point::ZERO;
    kurbo::flatten(bez.iter(), 0.5, |el| match el {
        kurbo::PathEl::MoveTo(p) => {
            last = p;
            start = p;
        }
        kurbo::PathEl::LineTo(p) => {
            pts.push((last, p));
            last = p;
        }
        kurbo::PathEl::ClosePath => {
            pts.push((last, start));
            last = start;
        }
        _ => {}
    });
    let mut out = Vec::new();
    let mut y = 0.0_f32;
    while y < rect.h {
        let y1 = (y + BAND).min(rect.h);
        let (fy0, fy1) = (f64::from(y), f64::from(y1));
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for (a, b) in &pts {
            let (ya, yb) = (a.y.min(b.y), a.y.max(b.y));
            if yb < fy0 || ya > fy1 {
                continue;
            }
            // The segment's part inside the band.
            let clip = |t: f64| kurbo::Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            let dyab = b.y - a.y;
            let (t0, t1) = if dyab.abs() < 1e-9 {
                (0.0, 1.0)
            } else {
                let ta = ((fy0 - a.y) / dyab).clamp(0.0, 1.0);
                let tb = ((fy1 - a.y) / dyab).clamp(0.0, 1.0);
                (ta.min(tb), ta.max(tb))
            };
            for p in [clip(t0), clip(t1)] {
                lo = lo.min(p.x);
                hi = hi.max(p.x);
            }
        }
        if lo.is_finite() && hi.is_finite() && hi > lo {
            #[allow(clippy::cast_possible_truncation)]
            out.push([
                rect.y + y - dy,
                rect.y + y1 + dy,
                rect.x + lo as f32 - dx,
                rect.x + hi as f32 + dx,
            ]);
        }
        y = y1;
    }
    out
}

/// Text box body properties (`wps:bodyPr`): insets left, top, right, bottom in px (DrawingML's
/// defaults 0.1 in / 0.05 in) and the vertical anchor (`t`, `ctr`, `b`).
#[must_use]
pub fn body_props(d: &Drawing) -> ([f32; 4], &'static str) {
    let mut insets = [9.6, 4.8, 9.6, 4.8];
    let mut anchor = "t";
    let Some(xml) = d.xml.as_deref() else {
        return (insets, anchor);
    };
    let Some(i) = xml.find("bodyPr") else {
        return (insets, anchor);
    };
    let tag = &xml[i..xml[i..].find('>').map_or(xml.len(), |e| i + e)];
    let attr = |name: &str| -> Option<&str> {
        let k = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        let v = &tag[k..];
        Some(&v[..v.find('"')?])
    };
    for (slot, name) in ["lIns", "tIns", "rIns", "bIns"].iter().enumerate() {
        if let Some(v) = attr(name).and_then(|v| v.parse::<i64>().ok()) {
            insets[slot] = emu(v.clamp(0, 9_144_000));
        }
    }
    anchor = match attr("anchor") {
        Some("ctr") => "ctr",
        Some("b") => "b",
        _ => "t",
    };
    (insets, anchor)
}

/// One path of a shape, in object coordinates (px from the shape's top-left corner).
#[derive(Debug, Clone, PartialEq)]
pub struct ShapePath {
    /// The outline.
    pub path: PathData,
    /// Fill colour.
    pub fill: Option<Rgba>,
    /// Outline colour and width (px).
    pub stroke: Option<(Rgba, f32)>,
}

/// How a shape looks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShapeVisual {
    /// Paths in paint order.
    pub paths: Vec<ShapePath>,
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn find<'a>(el: &'a XmlElement, name: &str) -> Option<&'a XmlElement> {
    if local(&el.name) == name {
        return Some(el);
    }
    el.elements().find_map(|c| find(c, name))
}

fn color_context(theme: &DocTheme) -> ColorContext {
    ColorContext {
        theme_colors: theme.colors.map(|c| Some(c.into())),
        ..ColorContext::default()
    }
}

fn rgba(c: kiln_ooxml::drawingml::color::Rgba) -> Rgba {
    let c = c.to_color();
    u32::from_be_bytes([c.r, c.g, c.b, c.a])
}

/// The look of a shape drawing: its preset geometry (or a rectangle), with the fill and outline
/// from `spPr`, falling back to the shape style's references (Word's default: accent 1 fill
/// with a darker outline).
#[must_use]
pub fn shape_visual(d: &Drawing, w: f32, h: f32, theme: &DocTheme) -> ShapeVisual {
    let ctx = color_context(theme);
    let parsed = d.xml.as_deref().and_then(|x| parse_xml_str(x).ok());
    let root = parsed.as_ref().map(|p| &p.root);
    let sp_pr = root.and_then(|r| find(r, "spPr"));
    let style = root.and_then(|r| find(r, "style"));
    let preset = match &d.kind {
        DrawingKind::Shape { preset } => preset.clone(),
        _ => sp_pr
            .and_then(|s| find(s, "prstGeom"))
            .and_then(|g| g.attrs.get("prst"))
            .unwrap_or("rect")
            .to_owned(),
    };
    let adj = sp_pr
        .and_then(|s| find(s, "prstGeom"))
        .map(|g| adjust_values(g.child("a:avLst")))
        .unwrap_or_default();
    let adj_ref: Vec<(&str, f64)> = adj.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let style_color = |name: &str| -> Option<Rgba> {
        let r = style.and_then(|s| find(s, name))?;
        let idx = r
            .attrs
            .get("idx")
            .and_then(|i| i.parse::<u32>().ok())
            .unwrap_or(0);
        if idx == 0 {
            return None;
        }
        parse_color_child(Some(r)).map(|c| rgba(resolve_color(&c, &ctx)))
    };
    let fill = match sp_pr.and_then(find_fill) {
        Some(Fill::Solid(c)) => Some(rgba(resolve_color(&c, &ctx))),
        Some(Fill::None) => None,
        Some(Fill::Gradient(g)) => g.stops.first().map(|s| rgba(resolve_color(&s.color, &ctx))),
        Some(_) => Some(0xBF_BF_BF_FF),
        None => style_color("fillRef"),
    };
    let ln = sp_pr.and_then(|s| s.child("a:ln")).map(parse_line);
    let stroke = match &ln {
        Some(l) if matches!(l.fill, Some(Fill::None)) => None,
        Some(l) => {
            let color = match &l.fill {
                Some(Fill::Solid(c)) => Some(rgba(resolve_color(c, &ctx))),
                _ => style_color("lnRef"),
            };
            #[allow(clippy::cast_possible_truncation)]
            let width = (l.width_px() as f32).max(0.75);
            color.map(|c| (c, width))
        }
        None => style_color("lnRef").map(|c| (c, 1.0)),
    };
    let geom = preset_geometry(&preset, f64::from(w), f64::from(h), &adj_ref)
        .or_else(|_| preset_geometry("rect", f64::from(w), f64::from(h), &[]));
    let mut out = ShapeVisual::default();
    if let Ok(g) = geom {
        for p in &g.paths {
            let bez = p.to_bez_path(0.25);
            let path = PathData::from(&bez);
            let filled = !matches!(p.fill, kiln_ooxml::st::PathFillMode::None);
            out.paths.push(ShapePath {
                path,
                fill: if filled { fill } else { None },
                stroke: if p.stroke { stroke } else { None },
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_common::NodeId;
    use kiln_doc::model::Anchor;

    fn anchor(h: PosH, rh: RelativeFromH, v: PosV, rv: RelativeFromV, wrap: WrapType) -> Anchor {
        Anchor {
            wrap,
            wrap_side: None,
            relative_h: rh,
            h,
            relative_v: rv,
            v,
            dist_top: 0,
            dist_bottom: 0,
            dist_left: 114_300,
            dist_right: 114_300,
            allow_overlap: true,
            layout_in_cell: true,
            locked: false,
            z_order: 0,
            wrap_polygon_xml: None,
        }
    }

    const FRAME: PageFrame = PageFrame {
        width: 816.0,
        height: 1056.0,
        top: 96.0,
        bottom: 96.0,
        left: 96.0,
        right: 96.0,
        column: (96.0, 624.0),
    };

    #[test]
    fn positions_relative_to_every_frame() {
        let d = Drawing::picture(NodeId(1), "m", 952_500, 952_500);
        let at = AnchorPoint {
            para_top: 300.0,
            line_top: 320.0,
            char_x: 150.0,
        };
        let r = float_rect(
            &d,
            &anchor(
                PosH::Offset(0),
                RelativeFromH::Column,
                PosV::Offset(95_250),
                RelativeFromV::Paragraph,
                WrapType::Square,
            ),
            &FRAME,
            &at,
        );
        assert_eq!((r.x, r.y, r.w, r.h), (96.0, 310.0, 100.0, 100.0));
        let r = float_rect(
            &d,
            &anchor(
                PosH::Align(AlignH::Center),
                RelativeFromH::Page,
                PosV::Align(AlignV::Bottom),
                RelativeFromV::Margin,
                WrapType::Square,
            ),
            &FRAME,
            &at,
        );
        assert_eq!((r.x, r.y), (358.0, 860.0));
        let r = float_rect(
            &d,
            &anchor(
                PosH::Align(AlignH::Right),
                RelativeFromH::Margin,
                PosV::Offset(0),
                RelativeFromV::Line,
                WrapType::Square,
            ),
            &FRAME,
            &at,
        );
        assert_eq!((r.x, r.y), (620.0, 320.0));
        // Below the page: moved up onto it.
        let low = AnchorPoint {
            para_top: 1040.0,
            ..at
        };
        let r = float_rect(
            &d,
            &anchor(
                PosH::Offset(0),
                RelativeFromH::Character,
                PosV::Offset(0),
                RelativeFromV::Paragraph,
                WrapType::Square,
            ),
            &FRAME,
            &low,
        );
        assert_eq!((r.x, r.y), (150.0, 956.0));
    }

    #[test]
    fn obstacles_per_wrap_mode() {
        let d = Drawing::picture(NodeId(1), "m", 952_500, 952_500);
        let theme = DocTheme::default();
        let rect = Bounds::new(100.0, 100.0, 100.0, 100.0);
        let mk = |w| {
            anchor(
                PosH::Offset(0),
                RelativeFromH::Column,
                PosV::Offset(0),
                RelativeFromV::Paragraph,
                w,
            )
        };
        let sq = obstacle(&d, &mk(WrapType::Square), rect, None, &theme).unwrap();
        assert_eq!(sq.rect, Bounds::new(88.0, 100.0, 124.0, 100.0));
        assert!(!sq.top_bottom && sq.profile.is_none());
        assert!(
            obstacle(&d, &mk(WrapType::TopAndBottom), rect, None, &theme)
                .unwrap()
                .top_bottom
        );
        assert!(obstacle(&d, &mk(WrapType::Behind), rect, None, &theme).is_none());
        assert!(obstacle(&d, &mk(WrapType::InFront), rect, None, &theme).is_none());
        let mut tight = mk(WrapType::Tight);
        tight.wrap_polygon_xml = Some(
            r#"<wp:wrapPolygon edited="0"><wp:start x="10800" y="0"/><wp:lineTo x="21600" y="21600"/><wp:lineTo x="0" y="21600"/><wp:lineTo x="10800" y="0"/></wp:wrapPolygon>"#.into(),
        );
        let t = obstacle(&d, &tight, rect, None, &theme).unwrap();
        let prof = t.profile.unwrap();
        // A triangle: narrow at the top, full width at the bottom.
        let top = prof.first().unwrap();
        let bottom = prof.last().unwrap();
        assert!(top[3] - top[2] < bottom[3] - bottom[2]);
    }

    #[test]
    fn shapes_use_presets_and_style_colours() {
        let mut d = Drawing::picture(NodeId(1), "m", 952_500, 476_250);
        d.kind = DrawingKind::Shape {
            preset: "ellipse".into(),
        };
        d.xml = Some(
            r#"<a:graphic xmlns:a="a"><a:graphicData><wps:wsp xmlns:wps="w"><wps:spPr><a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill><a:ln w="12700"><a:solidFill><a:srgbClr val="0000FF"/></a:solidFill></a:ln></wps:spPr></wps:wsp></a:graphicData></a:graphic>"#.into(),
        );
        let v = shape_visual(&d, 100.0, 50.0, &DocTheme::default());
        assert_eq!(v.paths.len(), 1);
        assert_eq!(v.paths[0].fill, Some(0xFF00_00FF));
        let (c, w) = v.paths[0].stroke.unwrap();
        assert_eq!(c, 0x0000_FFFF);
        assert!((w - 4.0 / 3.0).abs() < 0.01);
        let b = v.paths[0].path.bounds();
        assert!((b.w - 100.0).abs() < 1.0 && (b.h - 50.0).abs() < 1.0);
    }
}
