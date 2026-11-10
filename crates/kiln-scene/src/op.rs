// SPDX-License-Identifier: AGPL-3.0-or-later
//! Draw operations: the vocabulary every painter (canvas, raster, PDF) implements.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geom::Bounds;
use crate::paint::Paint;
use crate::path::{FillRule, PathData, Stroke};

/// Identifies an image uploaded with [`crate::ImageUpload`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub struct ImageId(pub u32);

/// Identifies a glyph atlas page uploaded with [`crate::AtlasUpdate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub struct AtlasPageId(pub u32);

/// Glyph bitmaps from one atlas page in one colour.
///
/// Glyphs are positioned in **device pixels** relative to the tile's device origin
/// (`round(tile.x * scale)`, `round(tile.y * scale)`), because they were rasterised for the scene's
/// `scale` at those exact sub-pixel positions: a painter copies them 1:1 without resampling, which
/// is what keeps text crisp. Glyphs that need a transform are emitted as
/// [`DrawOp::OutlineGlyphs`] instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GlyphRun {
    /// The atlas page.
    pub page: AtlasPageId,
    /// Text colour (A8 pages are tinted with it; colour pages ignore it).
    pub paint: Paint,
    /// Six integers per glyph: atlas `x, y, width, height`, then device `x, y` of the bitmap's
    /// top-left corner relative to the tile's device origin.
    pub quads: Vec<i32>,
}

impl GlyphRun {
    /// Integers per glyph in [`GlyphRun::quads`].
    pub const STRIDE: usize = 6;

    /// Number of glyphs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.quads.len() / Self::STRIDE
    }

    /// Whether the run has no glyphs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.quads.len() < Self::STRIDE
    }

    /// Appends a glyph.
    pub fn push(&mut self, src: [i32; 4], dst: [i32; 2]) {
        self.quads.extend(src);
        self.quads.extend(dst);
    }

    /// The bounds of the glyphs in tile-local document pixels, for a scene at `scale`.
    #[must_use]
    pub fn bounds(&self, scale: f32) -> Bounds {
        let s = if scale > 0.0 { scale } else { 1.0 };
        #[allow(clippy::cast_precision_loss)]
        self.quads
            .as_chunks::<{ Self::STRIDE }>()
            .0
            .iter()
            .map(|q| {
                Bounds::new(
                    q[4] as f32 / s,
                    q[5] as f32 / s,
                    q[2] as f32 / s,
                    q[3] as f32 / s,
                )
            })
            .fold(Bounds::ZERO, |a, b| a.union(&b))
    }
}

/// A gradient stop.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct GradientStop {
    /// Position, 0–1.
    pub offset: f32,
    /// Colour.
    pub paint: Paint,
}

/// The geometry of a gradient, in the coordinates of the op.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub enum GradientKind {
    /// Along the line from `(x0, y0)` to `(x1, y1)`.
    Linear {
        /// Start x.
        x0: f32,
        /// Start y.
        y0: f32,
        /// End x.
        x1: f32,
        /// End y.
        y1: f32,
    },
    /// Circular around `(cx, cy)` out to radius `r`.
    Radial {
        /// Centre x.
        cx: f32,
        /// Centre y.
        cy: f32,
        /// Radius.
        r: f32,
    },
}

/// A gradient fill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Gradient {
    /// Linear or radial.
    pub kind: GradientKind,
    /// Stops, by increasing offset.
    pub stops: Vec<GradientStop>,
}

/// What a [`DrawOp::Shadow`] looks like.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub enum ShadowStyle {
    /// The theme's page shadow ([`crate::Palette::page_shadow`]).
    Page,
    /// A single drop shadow.
    Drop {
        /// Horizontal offset.
        dx: f32,
        /// Vertical offset.
        dy: f32,
        /// Blur radius (CSS semantics: σ = blur / 2).
        blur: f32,
        /// Spread.
        spread: f32,
        /// Colour.
        paint: Paint,
    },
}

/// A shadow cast by a (rounded) rectangle. Only the part outside the rectangle is painted, as the
/// caster covers the rest.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct Shadow {
    /// The rectangle casting the shadow.
    pub rect: Bounds,
    /// Corner radius of the caster.
    pub radius: f32,
    /// Shape and colour.
    pub style: ShadowStyle,
}

/// Dash style of [`DrawOp::Underline`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum LineStyle {
    /// Solid.
    Solid,
    /// Dotted (IME composition clauses in Word).
    Dotted,
    /// Dashed.
    Dashed,
}

/// Which end of a text selection a touch handle belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum HandleSide {
    /// The start (the teardrop leans left).
    Start,
    /// The end (the teardrop leans right).
    End,
    /// The caret of a collapsed selection (the teardrop points straight up).
    Caret,
}

/// One drawing operation. Coordinates are CSS pixels: tile-local inside tiles, document pixels in
/// the overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum DrawOp {
    /// A filled (optionally rounded) rectangle.
    FillRect {
        /// Rectangle.
        rect: Bounds,
        /// Colour.
        paint: Paint,
        /// Corner radius (0 = square).
        radius: f32,
    },
    /// A stroked rectangle; the stroke is centred on the edge.
    StrokeRect {
        /// Rectangle.
        rect: Bounds,
        /// Colour.
        paint: Paint,
        /// Line width.
        width: f32,
        /// Corner radius.
        radius: f32,
    },
    /// An anti-aliased path, filled and/or stroked.
    Path {
        /// Geometry.
        path: PathData,
        /// Fill colour.
        fill: Option<Paint>,
        /// Stroke.
        stroke: Option<Stroke>,
        /// Fill rule.
        rule: FillRule,
    },
    /// Glyph bitmaps blitted from an atlas page.
    Glyphs(GlyphRun),
    /// Glyph outlines (large sizes, transformed text), filled non-zero.
    OutlineGlyphs {
        /// Every glyph's outline.
        path: PathData,
        /// Colour.
        paint: Paint,
    },
    /// An uploaded image.
    Image {
        /// The image.
        image: ImageId,
        /// Source rectangle in image pixels.
        src: Bounds,
        /// Destination rectangle.
        dst: Bounds,
        /// Opacity, 0–1.
        opacity: f32,
    },
    /// Intersects the clip with a rectangle, or with a path when one is given (anti-aliased).
    PushClip {
        /// Clip rectangle (also the path's bounds when `path` is set).
        rect: Bounds,
        /// Clip path.
        path: Option<PathData>,
    },
    /// Restores the clip of the matching `PushClip`.
    PopClip,
    /// Multiplies the current transform by `[a, b, c, d, e, f]` (canvas `transform()` order).
    PushTransform {
        /// The affine matrix.
        matrix: [f32; 6],
    },
    /// Restores the transform of the matching `PushTransform`.
    PopTransform,
    /// A rectangle filled with a gradient.
    Gradient {
        /// Rectangle.
        rect: Bounds,
        /// Corner radius.
        radius: f32,
        /// The gradient.
        gradient: Gradient,
    },
    /// A box shadow.
    Shadow(Shadow),
    /// A wavy spelling/grammar underline from `x` to `x + width` centred on `y`.
    Squiggle {
        /// Start x.
        x: f32,
        /// Centre line.
        y: f32,
        /// Length.
        width: f32,
        /// Colour.
        paint: Paint,
    },
    /// A straight underline (IME composition, hyperlinks in overlays).
    Underline {
        /// Start x.
        x: f32,
        /// Top of the line.
        y: f32,
        /// Length.
        width: f32,
        /// Thickness.
        thickness: f32,
        /// Colour.
        paint: Paint,
        /// Dashes.
        style: LineStyle,
    },
    /// The text caret. Painters blink it and, with `spark`, crown it with the Kiln spark.
    Caret {
        /// The caret bar.
        rect: Bounds,
        /// Colour.
        paint: Paint,
        /// Draw the brand spark at the top.
        spark: bool,
    },
    /// Selection highlight rectangles (blended per [`crate::Palette::selection_blend`]).
    SelectionRects {
        /// Rectangles.
        rects: Vec<Bounds>,
        /// Colour.
        paint: Paint,
    },
    /// A non-printing guide line (margins, alignment guides).
    Guide {
        /// Start x.
        x0: f32,
        /// Start y.
        y0: f32,
        /// End x.
        x1: f32,
        /// End y.
        y1: f32,
        /// Colour.
        paint: Paint,
        /// Dashed instead of solid.
        dashed: bool,
    },
    /// A touch selection handle (a teardrop under the caret line), shown only for touch input.
    Handle {
        /// x of the caret line it belongs to.
        x: f32,
        /// Bottom of the caret line (the teardrop hangs below it).
        y: f32,
        /// Diameter of the teardrop's round part.
        size: f32,
        /// Which end.
        side: HandleSide,
        /// Colour.
        paint: Paint,
    },
}

impl DrawOp {
    /// The area the op can touch, in its own coordinates, or `None` for state ops (clip and
    /// transform pushes and pops) and for shadows, whose reach depends on the theme. `scale` is the
    /// scene's device scale (glyph runs are positioned in device pixels).
    #[must_use]
    pub fn bounds(&self, scale: f32) -> Option<Bounds> {
        Some(match self {
            DrawOp::FillRect { rect, .. }
            | DrawOp::Gradient { rect, .. }
            | DrawOp::Caret { rect, .. } => *rect,
            DrawOp::StrokeRect { rect, width, .. } => rect.inflate(width / 2.0, width / 2.0),
            DrawOp::Path { path, stroke, .. } => {
                let w = stroke.as_ref().map_or(0.0, |s| s.width);
                // Miter joins can reach further than half the width; 4× covers the default limit.
                path.bounds().inflate(w * 2.0, w * 2.0)
            }
            DrawOp::Glyphs(run) => run.bounds(scale),
            DrawOp::OutlineGlyphs { path, .. } => path.bounds(),
            DrawOp::Image { dst, .. } => *dst,
            DrawOp::Squiggle { x, y, width, .. } => Bounds::new(*x, y - 2.0, *width, 4.0),
            DrawOp::Underline {
                x,
                y,
                width,
                thickness,
                ..
            } => Bounds::new(*x, *y, *width, *thickness),
            DrawOp::SelectionRects { rects, .. } => {
                rects.iter().fold(Bounds::ZERO, |a, r| a.union(r))
            }
            DrawOp::Guide { x0, y0, x1, y1, .. } => {
                Bounds::from_corners(*x0, *y0, *x1, *y1).inflate(1.0, 1.0)
            }
            DrawOp::Handle { x, y, size, .. } => Bounds::new(x - size, *y, size * 2.0, size * 1.6),
            DrawOp::PushClip { .. }
            | DrawOp::PopClip
            | DrawOp::PushTransform { .. }
            | DrawOp::PopTransform
            | DrawOp::Shadow(_) => return None,
        })
    }

    /// Whether the op changes painter state rather than drawing.
    #[must_use]
    pub fn is_state(&self) -> bool {
        matches!(
            self,
            DrawOp::PushClip { .. }
                | DrawOp::PopClip
                | DrawOp::PushTransform { .. }
                | DrawOp::PopTransform
        )
    }
}

/// The ops that can touch `region`: drawing ops outside it are dropped, state ops are kept, and
/// nothing is culled while a transform is active (its effect on bounds is not tracked).
#[must_use]
pub fn cull<'a>(ops: &'a [DrawOp], region: &Bounds, scale: f32) -> Vec<&'a DrawOp> {
    let mut transforms = 0usize;
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        match op {
            DrawOp::PushTransform { .. } => transforms += 1,
            DrawOp::PopTransform => transforms = transforms.saturating_sub(1),
            _ => {}
        }
        let keep = transforms > 0
            || op.is_state()
            || op.bounds(scale).is_none_or(|b| b.intersects(region));
        if keep {
            out.push(op);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ColorRole;

    fn fill(x: f32, y: f32) -> DrawOp {
        DrawOp::FillRect {
            rect: Bounds::new(x, y, 10.0, 10.0),
            paint: Paint::Role(ColorRole::Page),
            radius: 0.0,
        }
    }

    #[test]
    fn glyph_runs_report_document_bounds() {
        let mut run = GlyphRun {
            page: AtlasPageId(0),
            paint: Paint::BLACK,
            quads: Vec::new(),
        };
        assert!(run.is_empty());
        run.push([0, 0, 8, 10], [20, 40]);
        run.push([8, 0, 6, 10], [30, 42]);
        assert_eq!(run.len(), 2);
        assert_eq!(run.bounds(2.0), Bounds::new(10.0, 20.0, 8.0, 6.0));
        assert_eq!(
            DrawOp::Glyphs(run).bounds(2.0),
            Some(Bounds::new(10.0, 20.0, 8.0, 6.0))
        );
    }

    #[test]
    fn culling_keeps_state_and_transformed_ops() {
        let ops = vec![
            fill(0.0, 0.0),
            fill(100.0, 100.0),
            DrawOp::PushClip {
                rect: Bounds::new(500.0, 500.0, 1.0, 1.0),
                path: None,
            },
            fill(200.0, 0.0),
            DrawOp::PopClip,
            DrawOp::PushTransform {
                matrix: [1.0, 0.0, 0.0, 1.0, 300.0, 0.0],
            },
            fill(0.0, 0.0),
            fill(900.0, 900.0),
            DrawOp::PopTransform,
            fill(900.0, 900.0),
            DrawOp::Shadow(Shadow {
                rect: Bounds::new(900.0, 900.0, 1.0, 1.0),
                radius: 0.0,
                style: ShadowStyle::Page,
            }),
        ];
        let region = Bounds::new(0.0, 0.0, 50.0, 50.0);
        let kept = cull(&ops, &region, 1.0);
        // The first fill, both clip ops, the transform with both of its fills, and the shadow.
        assert_eq!(kept.len(), 8);
        assert_eq!(kept[0], &ops[0]);
        assert!(!kept.contains(&&ops[1]));
        assert!(!kept.contains(&&ops[3]));
        assert!(kept.contains(&&ops[7]));
        assert_eq!(kept.last(), Some(&&ops[10]));
    }

    #[test]
    fn every_drawing_op_has_bounds() {
        let ops = [
            DrawOp::StrokeRect {
                rect: Bounds::new(0.0, 0.0, 10.0, 10.0),
                paint: Paint::BLACK,
                width: 2.0,
                radius: 0.0,
            },
            DrawOp::Squiggle {
                x: 0.0,
                y: 10.0,
                width: 30.0,
                paint: Paint::BLACK,
            },
            DrawOp::Guide {
                x0: 5.0,
                y0: 0.0,
                x1: 5.0,
                y1: 100.0,
                paint: Paint::BLACK,
                dashed: true,
            },
            DrawOp::SelectionRects {
                rects: vec![
                    Bounds::new(0.0, 0.0, 5.0, 5.0),
                    Bounds::new(0.0, 5.0, 9.0, 5.0),
                ],
                paint: Paint::BLACK,
            },
        ];
        assert_eq!(
            ops[0].bounds(1.0),
            Some(Bounds::new(-1.0, -1.0, 12.0, 12.0))
        );
        assert_eq!(ops[1].bounds(1.0), Some(Bounds::new(0.0, 8.0, 30.0, 4.0)));
        assert_eq!(ops[2].bounds(1.0), Some(Bounds::new(4.0, -1.0, 2.0, 102.0)));
        assert_eq!(ops[3].bounds(1.0), Some(Bounds::new(0.0, 0.0, 9.0, 10.0)));
        assert!(DrawOp::PopClip.bounds(1.0).is_none());
    }
}
