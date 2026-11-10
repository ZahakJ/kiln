// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pictures and shapes on a sheet: one Scene tile each above the cells, like charts. A picture is
//! its image (cropped) with an optional outline; a shape is its DrawingML preset geometry
//! (`kiln-ooxml`) filled and outlined, with its text centred in the preset's text rectangle.
//! The same ops print, with the text as outlines.

use kiln_charts::text::{GlyphSink, TextMeasurer, TextStyle, draw_block};
use kiln_ooxml::drawingml::presets::preset_geometry;
use kiln_ooxml::drawingml::theme::{Theme, office_theme, theme_color_table};
use kiln_ooxml::st::{ColorSchemeIndex, PathFillMode};
use kiln_scene::{Bounds, DrawOp, ImageId, Paint, PathData, Stroke};
use kiln_sheet::meta::DrawingLook;
use kiln_text::Alignment;

/// Excel's size for a shape inserted with a click: one inch square.
pub const NEW_SHAPE_SIZE: (f32, f32) = (96.0, 96.0);

/// What an object tile draws.
#[derive(Debug, Clone, Copy)]
pub enum ObjectKind {
    /// A picture: its image and the image's size in pixels.
    Picture {
        /// The uploaded image (`None` while it is not decoded: a grey box).
        image: Option<ImageId>,
        /// Width and height of the image in pixels.
        size: (u32, u32),
    },
    /// A shape with a preset geometry.
    Shape,
    /// A line or connector.
    Line,
}

/// One picture or shape for [`crate::GridScene::prepare`] and printing.
#[derive(Debug, Clone, Copy)]
pub struct ObjectTile<'a> {
    /// The tile index (the drawing's number on the sheet).
    pub index: u32,
    /// Where the tile is, in sheet pixels: the object's rectangle grown by `pad` on every side.
    pub rect: Bounds,
    /// The margin around a shape for the half of its outline outside its geometry (0 for
    /// pictures, which draw their outline inside).
    pub pad: f32,
    /// Changes whenever the object, its size or its image change.
    pub version: u64,
    /// What it is.
    pub kind: ObjectKind,
    /// How it looks.
    pub look: &'a DrawingLook,
}

/// A colour of a look (`RRGGBB`, `scheme:accent1`, `none`) as a paint; `None` for `none` and
/// colours that do not parse.
#[must_use]
pub fn look_paint(color: &str, theme: Option<&Theme>) -> Option<Paint> {
    if color.is_empty() || color == "none" {
        return None;
    }
    if let Some(slot) = color.strip_prefix("scheme:") {
        let slot = match slot {
            "tx1" => "dk1",
            "bg1" => "lt1",
            "tx2" => "dk2",
            "bg2" => "lt2",
            s => s,
        };
        let idx: ColorSchemeIndex = slot.parse().ok()?;
        let table = match theme {
            Some(t) => theme_color_table(t),
            None => theme_color_table(&office_theme()),
        };
        let c = table[idx.index()].to_color();
        return Some(Paint::Rgba(u32::from_be_bytes([c.r, c.g, c.b, 255])));
    }
    let v = u32::from_str_radix(color.trim_start_matches('#'), 16).ok()?;
    (color.trim_start_matches('#').len() == 6).then_some(Paint::Rgba((v << 8) | 0xff))
}

/// The tile margin of a shape or line with this look.
#[must_use]
pub fn shape_pad(look: &DrawingLook) -> f32 {
    (line_px(look.line_width) / 2.0 + 2.0).ceil()
}

/// A line width in EMU as CSS pixels (at least a hairline).
fn line_px(emu: Option<i64>) -> f32 {
    (emu.unwrap_or(12_700) as f32 / 9_525.0).max(0.75)
}

/// The ops of an object tile (pixels from the tile's top-left corner). `sink` places glyphs (the
/// sheet's atlas on screen, outlines in print).
pub fn object_ops(
    tile: &ObjectTile<'_>,
    theme: Option<&Theme>,
    text: &mut TextMeasurer,
    sink: &mut dyn GlyphSink,
) -> Vec<DrawOp> {
    let look = tile.look;
    let (w, h) = (tile.rect.w.max(0.0), tile.rect.h.max(0.0));
    let mut out = Vec::new();
    let line = look.line.as_deref().and_then(|c| look_paint(c, theme));
    match tile.kind {
        ObjectKind::Picture { image, size } => {
            let full = Bounds::new(0.0, 0.0, w, h);
            match image {
                Some(image) if size.0 > 0 && size.1 > 0 => {
                    let [l, t, r, b] = look.crop.map(|v| v as f32 / 100_000.0);
                    let (iw, ih) = (size.0 as f32, size.1 as f32);
                    let src = Bounds::new(
                        (l * iw).clamp(0.0, iw),
                        (t * ih).clamp(0.0, ih),
                        (iw * (1.0 - l - r)).max(1.0),
                        (ih * (1.0 - t - b)).max(1.0),
                    );
                    out.push(DrawOp::Image {
                        image,
                        src,
                        dst: full,
                        opacity: 1.0,
                    });
                }
                _ => {
                    // A picture Kiln cannot draw (EMF, a missing part): a grey box, as Excel's
                    // placeholder.
                    out.push(DrawOp::FillRect {
                        rect: full,
                        paint: Paint::Rgba(0xe7e6e6ff),
                        radius: 0.0,
                    });
                    out.push(DrawOp::StrokeRect {
                        rect: full.inflate(-0.5, -0.5),
                        paint: Paint::Rgba(0xa6a6a6ff),
                        width: 1.0,
                        radius: 0.0,
                    });
                }
            }
            if let Some(p) = line {
                let lw = line_px(look.line_width);
                out.push(DrawOp::StrokeRect {
                    rect: full.inflate(-lw / 2.0, -lw / 2.0),
                    paint: p,
                    width: lw,
                    radius: 0.0,
                });
            }
        }
        ObjectKind::Shape | ObjectKind::Line => {
            let pad = tile.pad;
            let (w, h) = ((w - 2.0 * pad).max(0.0), (h - 2.0 * pad).max(0.0));
            out.push(DrawOp::PushTransform {
                matrix: [1.0, 0.0, 0.0, 1.0, pad, pad],
            });
            let preset =
                look.preset
                    .as_deref()
                    .unwrap_or(if matches!(tile.kind, ObjectKind::Line) {
                        "line"
                    } else {
                        "rect"
                    });
            let fill = look.fill.as_deref().and_then(|c| look_paint(c, theme));
            let stroke_paint = match tile.kind {
                // A line without a colour draws in the theme's first accent.
                ObjectKind::Line => line.or_else(|| look_paint("scheme:accent1", theme)),
                _ => line,
            };
            let stroke = stroke_paint.map(|p| Stroke::solid(p, line_px(look.line_width)));
            let geometry =
                preset_geometry(preset, f64::from(w.max(0.1)), f64::from(h.max(0.1)), &[]).or_else(
                    |_| preset_geometry("rect", f64::from(w.max(0.1)), f64::from(h.max(0.1)), &[]),
                );
            let text_rect = match &geometry {
                Ok(g) => {
                    for p in &g.paths {
                        let path = PathData::from(&p.to_bez_path(0.1));
                        let fill = match p.fill {
                            PathFillMode::None => None,
                            _ => fill,
                        };
                        let stroke = if p.stroke { stroke.clone() } else { None };
                        if fill.is_some() || stroke.is_some() {
                            out.push(DrawOp::Path {
                                path,
                                fill,
                                stroke,
                                rule: kiln_scene::FillRule::NonZero,
                            });
                        }
                    }
                    #[allow(clippy::cast_possible_truncation)]
                    Bounds::from_corners(
                        g.text_rect.x0 as f32,
                        g.text_rect.y0 as f32,
                        g.text_rect.x1 as f32,
                        g.text_rect.y1 as f32,
                    )
                }
                Err(_) => Bounds::new(0.0, 0.0, w, h),
            };
            // Glyphs are placed in tile pixels (atlas quads do not follow transforms).
            out.push(DrawOp::PopTransform);
            if !look.text.is_empty() && !matches!(tile.kind, ObjectKind::Line) {
                let r = Bounds::new(
                    text_rect.x + pad,
                    text_rect.y + pad,
                    text_rect.w,
                    text_rect.h,
                );
                shape_text(look, r, theme, text, sink, &mut out);
            }
        }
    }
    out
}

/// A shape's text: each paragraph centred, the block centred vertically, inside the text
/// rectangle less Office's insets (0.1 in left and right, 0.05 in top and bottom), clipped.
fn shape_text(
    look: &DrawingLook,
    rect: Bounds,
    theme: Option<&Theme>,
    text: &mut TextMeasurer,
    sink: &mut dyn GlyphSink,
    out: &mut Vec<DrawOp>,
) {
    let inner = Bounds::new(
        rect.x + 9.6,
        rect.y + 4.8,
        (rect.w - 19.2).max(1.0),
        (rect.h - 9.6).max(1.0),
    );
    let paint = look
        .text_color
        .as_deref()
        .and_then(|c| look_paint(c, theme))
        .unwrap_or(Paint::Rgba(0xffffffff));
    let size = look.text_size.map_or(11.0, |s| s as f32 / 100.0) * 96.0 / 72.0;
    let style = TextStyle {
        family: theme
            .map(|t| t.font_scheme.minor.latin.typeface.clone())
            .filter(|f| !f.is_empty())
            .unwrap_or_else(|| "Calibri".into()),
        size,
        bold: false,
        italic: false,
        paint,
    };
    let blocks: Vec<_> = look
        .text
        .split('\n')
        .map(|p| {
            text.block(
                if p.is_empty() { " " } else { p },
                &style,
                inner.w,
                Alignment::Center,
            )
        })
        .collect();
    let total: f32 = blocks.iter().map(|b| b.height).sum();
    let mut y = inner.y + ((inner.h - total) / 2.0).max(0.0);
    out.push(DrawOp::PushClip {
        rect: rect.inflate(1.0, 1.0),
        path: None,
    });
    for b in &blocks {
        draw_block(b, text.fonts(), inner.x, y, None, sink, out);
        y += b.height;
    }
    out.push(DrawOp::PopClip);
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_charts::text::OutlineSink;

    fn measurer() -> TextMeasurer {
        TextMeasurer::new(kiln_charts::text::default_fonts())
    }

    #[test]
    fn colours_resolve_from_hex_and_the_theme() {
        assert_eq!(look_paint("C2481C", None), Some(Paint::Rgba(0xc2481cff)));
        assert_eq!(look_paint("none", None), None);
        // Office's accent 1 is 156082 (2023 theme) or 4472C4.
        assert!(look_paint("scheme:accent1", None).is_some());
        assert_eq!(look_paint("scheme:nope", None), None);
    }

    #[test]
    fn a_shape_draws_its_geometry_and_text() {
        let look = DrawingLook {
            preset: Some("ellipse".into()),
            fill: Some("C2481C".into()),
            line: Some("000000".into()),
            text: "Kiln".into(),
            ..DrawingLook::default()
        };
        let tile = ObjectTile {
            index: 0,
            rect: Bounds::new(10.0, 10.0, 120.0, 80.0),
            pad: 3.0,
            version: 1,
            kind: ObjectKind::Shape,
            look: &look,
        };
        let ops = object_ops(&tile, None, &mut measurer(), &mut OutlineSink);
        assert!(ops.iter().any(|o| matches!(
            o,
            DrawOp::Path {
                fill: Some(_),
                stroke: Some(_),
                ..
            }
        )));
        assert!(
            ops.iter()
                .any(|o| matches!(o, DrawOp::OutlineGlyphs { .. }))
        );
    }

    #[test]
    fn a_cropped_picture_draws_part_of_its_image() {
        let look = DrawingLook {
            crop: [25_000, 0, 25_000, 50_000],
            ..DrawingLook::default()
        };
        let tile = ObjectTile {
            index: 0,
            rect: Bounds::new(0.0, 0.0, 50.0, 25.0),
            pad: 0.0,
            version: 1,
            kind: ObjectKind::Picture {
                image: Some(ImageId(3)),
                size: (200, 100),
            },
            look: &look,
        };
        let ops = object_ops(&tile, None, &mut measurer(), &mut OutlineSink);
        let DrawOp::Image { src, dst, .. } = &ops[0] else {
            panic!("{ops:?}")
        };
        assert_eq!((src.x, src.y, src.w, src.h), (50.0, 0.0, 100.0, 50.0));
        assert_eq!((dst.w, dst.h), (50.0, 25.0));
    }
}
