// SPDX-License-Identifier: AGPL-3.0-or-later
//! Executes [`DrawOp`]s on a tiny-skia pixmap.

use kiln_scene::shapes::{self, blurred_interval};
use kiln_scene::{
    AtlasFormat, AtlasPageId, Bounds, DrawOp, FillRule, GlyphRun, Gradient, GradientKind, ImageId,
    LineCap, LineJoin, LineStyle, Paint, Palette, PathData, Segment, SelectionBlend, Shadow,
    ShadowLayer, ShadowStyle, channels,
};
use rustc_hash::FxHashMap;
use tiny_skia::{
    BlendMode, Color, FillRule as SkFillRule, FilterQuality, LinearGradient, Mask, Path,
    PathBuilder, Pattern, Pixmap, Point, RadialGradient, Rect, Shader, SpreadMode, Stroke,
    StrokeDash, Transform,
};

/// An atlas page as the painter keeps it.
#[derive(Debug, Clone)]
pub struct AtlasPixels {
    /// Pixel format.
    pub format: AtlasFormat,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Row-major pixels (`A8`: coverage; `Rgba8`: straight RGBA).
    pub data: Vec<u8>,
}

/// What ops can refer to.
#[derive(Debug, Default)]
pub struct Resources {
    /// Atlas pages.
    pub atlas: FxHashMap<AtlasPageId, AtlasPixels>,
    /// Images (premultiplied).
    pub images: FxHashMap<ImageId, Pixmap>,
}

/// Options of one paint pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaintOptions {
    /// Draw carets (the blink's "on" phase).
    pub caret_visible: bool,
    /// Draw touch selection handles.
    pub touch: bool,
}

impl Default for PaintOptions {
    fn default() -> Self {
        Self {
            caret_visible: true,
            touch: false,
        }
    }
}

/// Paints ops onto one pixmap. `base` maps op coordinates to device pixels; `glyph_origin` is the
/// device position glyph runs are relative to.
pub struct OpPainter<'a> {
    pixmap: &'a mut Pixmap,
    palette: &'a Palette,
    resources: &'a Resources,
    options: PaintOptions,
    scale: f32,
    glyph_origin: (i32, i32),
    transforms: Vec<Transform>,
    clips: Vec<Option<Mask>>,
}

impl std::fmt::Debug for OpPainter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpPainter")
            .field("size", &(self.pixmap.width(), self.pixmap.height()))
            .field("scale", &self.scale)
            .field("depth", &(self.transforms.len(), self.clips.len()))
            .finish_non_exhaustive()
    }
}

fn color(rgba: u32) -> Color {
    let [r, g, b, a] = channels(rgba);
    Color::from_rgba8(r, g, b, a)
}

fn solid(rgba: u32) -> tiny_skia::Paint<'static> {
    let mut p = tiny_skia::Paint::default();
    p.set_color(color(rgba));
    p.anti_alias = true;
    p
}

/// Converts a scene path to a tiny-skia path (`None` when empty or invalid).
#[must_use]
pub fn sk_path(p: &PathData) -> Option<Path> {
    let mut b = PathBuilder::new();
    for s in p.segments() {
        match s {
            Segment::Move([x, y]) => b.move_to(x, y),
            Segment::Line([x, y]) => b.line_to(x, y),
            Segment::Quad([a, c, x, y]) => b.quad_to(a, c, x, y),
            Segment::Cubic([a, c, d, e, x, y]) => b.cubic_to(a, c, d, e, x, y),
            Segment::Close => b.close(),
        }
    }
    b.finish()
}

fn sk_rect(b: Bounds) -> Option<Rect> {
    if !b.is_finite() {
        return None;
    }
    Rect::from_xywh(b.x, b.y, b.w, b.h)
}

/// Source-over of a straight-alpha colour with coverage `cov` (0–255) onto a premultiplied pixel,
/// rounding like the Canvas 2D implementations do (8-bit premultiplied arithmetic).
#[inline]
fn blend_px(dst: &mut [u8], rgb: [u8; 3], alpha: u32) {
    if alpha == 0 {
        return;
    }
    let a = alpha.min(255);
    let inv = 255 - a;
    for i in 0..3 {
        let src = (u32::from(rgb[i]) * a + 127) / 255;
        let d = u32::from(dst[i]);
        dst[i] = u8::try_from((src + (d * inv + 127) / 255).min(255)).unwrap_or(255);
    }
    let da = u32::from(dst[3]);
    dst[3] = u8::try_from((a + (da * inv + 127) / 255).min(255)).unwrap_or(255);
}

impl<'a> OpPainter<'a> {
    /// A painter for `pixmap` with a base transform of `scale` then a device offset.
    pub fn new(
        pixmap: &'a mut Pixmap,
        palette: &'a Palette,
        resources: &'a Resources,
        options: PaintOptions,
        scale: f32,
        offset: (f32, f32),
        glyph_origin: (i32, i32),
    ) -> Self {
        let base = Transform::from_row(scale, 0.0, 0.0, scale, offset.0, offset.1);
        Self {
            pixmap,
            palette,
            resources,
            options,
            scale,
            glyph_origin,
            transforms: vec![base],
            clips: vec![None],
        }
    }

    fn transform(&self) -> Transform {
        self.transforms.last().copied().unwrap_or_default()
    }

    fn mask(&self) -> Option<&Mask> {
        self.clips.last().and_then(Option::as_ref)
    }

    fn resolve(&self, paint: Paint) -> u32 {
        self.palette.resolve(paint)
    }

    /// Paints every op in order. Unbalanced pops are ignored; unclosed pushes end with the call.
    pub fn paint_all<'o>(&mut self, ops: impl IntoIterator<Item = &'o DrawOp>) {
        for op in ops {
            self.paint(op);
        }
    }

    fn fill_path(&mut self, path: &Path, paint: &tiny_skia::Paint<'_>, rule: SkFillRule) {
        let t = self.transform();
        let mask = self.clips.last().and_then(Option::as_ref);
        self.pixmap.fill_path(path, paint, rule, t, mask);
    }

    fn fill_bounds(&mut self, b: Bounds, radius: f32, paint: &tiny_skia::Paint<'_>) {
        if b.is_empty() || !b.is_finite() {
            return;
        }
        if let Some(p) = sk_path(&shapes::rounded_rect(b, radius)) {
            self.fill_path(&p, paint, SkFillRule::Winding);
        }
    }

    fn stroke(&mut self, path: &Path, paint: &tiny_skia::Paint<'_>, stroke: &Stroke) {
        let t = self.transform();
        let mask = self.clips.last().and_then(Option::as_ref);
        self.pixmap.stroke_path(path, paint, stroke, t, mask);
    }

    /// Paints one op.
    pub fn paint(&mut self, op: &DrawOp) {
        match op {
            DrawOp::FillRect {
                rect,
                paint,
                radius,
            } => {
                let p = solid(self.resolve(*paint));
                self.fill_bounds(*rect, *radius, &p);
            }
            DrawOp::StrokeRect {
                rect,
                paint,
                width,
                radius,
            } => {
                if let Some(path) = sk_path(&shapes::rounded_rect(*rect, *radius)) {
                    let p = solid(self.resolve(*paint));
                    let s = Stroke {
                        width: width.max(0.0),
                        ..Stroke::default()
                    };
                    self.stroke(&path, &p, &s);
                }
            }
            DrawOp::Path {
                path,
                fill,
                stroke,
                rule,
            } => {
                let Some(sp) = sk_path(path) else { return };
                if let Some(f) = fill {
                    let p = solid(self.resolve(*f));
                    let r = match rule {
                        FillRule::NonZero => SkFillRule::Winding,
                        FillRule::EvenOdd => SkFillRule::EvenOdd,
                    };
                    self.fill_path(&sp, &p, r);
                }
                if let Some(s) = stroke {
                    let p = solid(self.resolve(s.paint));
                    let sk = Stroke {
                        width: s.width.max(0.0),
                        line_cap: match s.cap {
                            LineCap::Butt => tiny_skia::LineCap::Butt,
                            LineCap::Round => tiny_skia::LineCap::Round,
                            LineCap::Square => tiny_skia::LineCap::Square,
                        },
                        line_join: match s.join {
                            LineJoin::Miter => tiny_skia::LineJoin::Miter,
                            LineJoin::Round => tiny_skia::LineJoin::Round,
                            LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
                        },
                        dash: if s.dash.len() >= 2 {
                            StrokeDash::new(s.dash.clone(), 0.0)
                        } else {
                            None
                        },
                        ..Stroke::default()
                    };
                    self.stroke(&sp, &p, &sk);
                }
            }
            DrawOp::Glyphs(run) => self.glyphs(run),
            DrawOp::OutlineGlyphs { path, paint } => {
                if let Some(sp) = sk_path(path) {
                    let p = solid(self.resolve(*paint));
                    self.fill_path(&sp, &p, SkFillRule::Winding);
                }
            }
            DrawOp::Image {
                image,
                src,
                dst,
                opacity,
            } => self.image(*image, *src, *dst, *opacity),
            DrawOp::PushClip { rect, path } => self.push_clip(*rect, path.as_ref()),
            DrawOp::PopClip => {
                if self.clips.len() > 1 {
                    self.clips.pop();
                }
            }
            DrawOp::PushTransform { matrix } => {
                let [a, b, c, d, e, f] = *matrix;
                let m = Transform::from_row(a, b, c, d, e, f);
                let t = if matrix.iter().all(|v| v.is_finite()) {
                    self.transform().pre_concat(m)
                } else {
                    self.transform()
                };
                self.transforms.push(t);
            }
            DrawOp::PopTransform => {
                if self.transforms.len() > 1 {
                    self.transforms.pop();
                }
            }
            DrawOp::Gradient {
                rect,
                radius,
                gradient,
            } => self.gradient(*rect, *radius, gradient),
            DrawOp::Shadow(shadow) => self.shadow(shadow),
            DrawOp::Squiggle { x, y, width, paint } => {
                if let Some(sp) = sk_path(&shapes::squiggle(*x, *y, *width)) {
                    let p = solid(self.resolve(*paint));
                    let s = Stroke {
                        width: 1.0,
                        ..Stroke::default()
                    };
                    self.stroke(&sp, &p, &s);
                }
            }
            DrawOp::Underline {
                x,
                y,
                width,
                thickness,
                paint,
                style,
            } => {
                let p = solid(self.resolve(*paint));
                for seg in underline_segments(*x, *width, *thickness, *style) {
                    self.fill_bounds(Bounds::new(seg.0, *y, seg.1, *thickness), 0.0, &p);
                }
            }
            DrawOp::Caret { rect, paint, spark } => {
                if !self.options.caret_visible {
                    return;
                }
                let p = solid(self.resolve(*paint));
                self.fill_bounds(*rect, 0.0, &p);
                if *spark && let Some(sp) = sk_path(&shapes::caret_spark(*rect)) {
                    let p = solid(self.palette.spark);
                    self.fill_path(&sp, &p, SkFillRule::Winding);
                }
            }
            DrawOp::SelectionRects { rects, paint } => {
                let mut p = solid(self.resolve(*paint));
                p.blend_mode = match self.palette.selection_blend {
                    SelectionBlend::Darken => BlendMode::Darken,
                    SelectionBlend::Lighten => BlendMode::Lighten,
                };
                for r in rects {
                    self.fill_bounds(*r, 0.0, &p);
                }
            }
            DrawOp::Guide {
                x0,
                y0,
                x1,
                y1,
                paint,
                dashed,
            } => {
                let mut pb = PathBuilder::new();
                pb.move_to(*x0, *y0);
                pb.line_to(*x1, *y1);
                if let Some(path) = pb.finish() {
                    let p = solid(self.resolve(*paint));
                    let w = 1.0 / self.scale.max(0.01);
                    let s = Stroke {
                        width: w,
                        dash: if *dashed {
                            StrokeDash::new(vec![4.0 * w, 3.0 * w], 0.0)
                        } else {
                            None
                        },
                        ..Stroke::default()
                    };
                    self.stroke(&path, &p, &s);
                }
            }
            DrawOp::Handle {
                x,
                y,
                size,
                side,
                paint,
            } => {
                if self.options.touch
                    && let Some(sp) = sk_path(&shapes::teardrop(*x, *y, *size, *side))
                {
                    let p = solid(self.resolve(*paint));
                    self.fill_path(&sp, &p, SkFillRule::Winding);
                }
            }
        }
    }

    fn push_clip(&mut self, rect: Bounds, path: Option<&PathData>) {
        let shape = match path {
            Some(p) => sk_path(p),
            None => sk_rect(rect).map(PathBuilder::from_rect),
        };
        let t = self.transform();
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        let mask = match (shape, self.mask()) {
            (Some(shape), Some(current)) => {
                let mut m = current.clone();
                m.intersect_path(&shape, SkFillRule::Winding, true, t);
                Some(m)
            }
            (Some(shape), None) => Mask::new(w, h).map(|mut m| {
                m.fill_path(&shape, SkFillRule::Winding, true, t);
                m
            }),
            // An empty or invalid clip hides everything.
            (None, _) => Mask::new(w, h),
        };
        self.clips.push(mask);
    }

    fn glyphs(&mut self, run: &GlyphRun) {
        let Some(page) = self.resources.atlas.get(&run.page) else {
            return;
        };
        let rgba = self.resolve(run.paint);
        let [r, g, b, a] = channels(rgba);
        let t = self.transform();
        // Glyphs are device-positioned; a transform pushed inside the tile only moves them.
        let base = self.transforms.first().copied().unwrap_or_default();
        #[allow(clippy::cast_possible_truncation)]
        let (shift_x, shift_y) = (
            (t.tx - base.tx).round() as i32,
            (t.ty - base.ty).round() as i32,
        );
        let (pw, ph) = (
            i32::try_from(self.pixmap.width()).unwrap_or(i32::MAX),
            i32::try_from(self.pixmap.height()).unwrap_or(i32::MAX),
        );
        let mask = self.clips.last().and_then(Option::as_ref).map(Mask::data);
        let stride = self.pixmap.width() as usize * 4;
        let data = self.pixmap.data_mut();
        let bpp = page.format.bytes_per_pixel();
        for q in run.quads.as_chunks::<{ GlyphRun::STRIDE }>().0 {
            let (sx, sy, w, h) = (q[0], q[1], q[2], q[3]);
            let dx = self.glyph_origin.0 + q[4] + shift_x;
            let dy = self.glyph_origin.1 + q[5] + shift_y;
            if w <= 0 || h <= 0 || sx < 0 || sy < 0 {
                continue;
            }
            let (Ok(pw_u), Ok(ph_u)) = (i32::try_from(page.width), i32::try_from(page.height))
            else {
                continue;
            };
            if sx.saturating_add(w) > pw_u || sy.saturating_add(h) > ph_u {
                continue;
            }
            for row in 0..h {
                let y = dy + row;
                if y < 0 || y >= ph {
                    continue;
                }
                for col in 0..w {
                    let x = dx + col;
                    if x < 0 || x >= pw {
                        continue;
                    }
                    #[allow(clippy::cast_sign_loss)]
                    let src_i =
                        ((sy + row) as usize * page.width as usize + (sx + col) as usize) * bpp;
                    #[allow(clippy::cast_sign_loss)]
                    let (xu, yu) = (x as usize, y as usize);
                    let clip = mask.map_or(255u32, |m| u32::from(m[yu * (stride / 4) + xu]));
                    let di = yu * stride + xu * 4;
                    match page.format {
                        AtlasFormat::A8 => {
                            let cov = u32::from(page.data[src_i]);
                            let alpha = (cov * u32::from(a) + 127) / 255;
                            let alpha = (alpha * clip + 127) / 255;
                            blend_px(&mut data[di..di + 4], [r, g, b], alpha);
                        }
                        AtlasFormat::Rgba8 => {
                            let px = &page.data[src_i..src_i + 4];
                            let alpha = (u32::from(px[3]) * clip + 127) / 255;
                            blend_px(&mut data[di..di + 4], [px[0], px[1], px[2]], alpha);
                        }
                    }
                }
            }
        }
    }

    fn image(&mut self, id: ImageId, src: Bounds, dst: Bounds, opacity: f32) {
        let Some(img) = self.resources.images.get(&id) else {
            return;
        };
        if src.is_empty() || dst.is_empty() || !src.is_finite() || !dst.is_finite() {
            return;
        }
        let sx = dst.w / src.w;
        let sy = dst.h / src.h;
        let map = Transform::from_row(sx, 0.0, 0.0, sy, dst.x - src.x * sx, dst.y - src.y * sy);
        let shader = Pattern::new(
            img.as_ref(),
            SpreadMode::Pad,
            FilterQuality::Bilinear,
            opacity.clamp(0.0, 1.0),
            map,
        );
        let paint = tiny_skia::Paint {
            shader,
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        self.fill_bounds(dst, 0.0, &paint);
    }

    fn gradient(&mut self, rect: Bounds, radius: f32, g: &Gradient) {
        let stops: Vec<tiny_skia::GradientStop> = g
            .stops
            .iter()
            .filter(|s| s.offset.is_finite())
            .map(|s| {
                tiny_skia::GradientStop::new(s.offset.clamp(0.0, 1.0), color(self.resolve(s.paint)))
            })
            .collect();
        let shader: Option<Shader<'static>> = match g.kind {
            GradientKind::Linear { x0, y0, x1, y1 } => LinearGradient::new(
                Point::from_xy(x0, y0),
                Point::from_xy(x1, y1),
                stops,
                SpreadMode::Pad,
                Transform::identity(),
            ),
            GradientKind::Radial { cx, cy, r } => RadialGradient::new(
                Point::from_xy(cx, cy),
                0.0,
                Point::from_xy(cx, cy),
                r.max(0.0),
                stops,
                SpreadMode::Pad,
                Transform::identity(),
            ),
        };
        if let Some(shader) = shader {
            let paint = tiny_skia::Paint {
                shader,
                anti_alias: true,
                ..tiny_skia::Paint::default()
            };
            self.fill_bounds(rect, radius, &paint);
        }
    }

    fn shadow(&mut self, shadow: &Shadow) {
        let layers: Vec<ShadowLayer> = match shadow.style {
            ShadowStyle::Page => self.palette.page_shadow.clone(),
            ShadowStyle::Drop {
                dx,
                dy,
                blur,
                spread,
                paint,
            } => vec![ShadowLayer {
                dx,
                dy,
                blur,
                spread,
                rgba: self.resolve(paint),
            }],
        };
        let t = self.transform();
        if t.has_skew() || !shadow.rect.is_finite() {
            return;
        }
        let caster = device_rect(t, shadow.rect);
        for l in layers {
            let r = shadow
                .rect
                .translate(l.dx, l.dy)
                .inflate(l.spread, l.spread);
            let dev = device_rect(t, r);
            let sigma = f64::from(l.blur.max(0.0) / 2.0 * t.sx.abs());
            paint_shadow(
                self.pixmap,
                dev,
                caster,
                sigma,
                l.rgba,
                self.clips.last().and_then(Option::as_ref),
            );
        }
    }
}

/// `(x, width)` of the dashes of an underline.
#[must_use]
pub fn underline_segments(x: f32, width: f32, thickness: f32, style: LineStyle) -> Vec<(f32, f32)> {
    if !(width > 0.0 && width.is_finite() && x.is_finite()) {
        return Vec::new();
    }
    let width = width.min(16_384.0);
    let (on, off) = match style {
        LineStyle::Solid => return vec![(x, width)],
        LineStyle::Dotted => (thickness.max(1.0), thickness.max(1.0)),
        LineStyle::Dashed => (thickness.max(1.0) * 3.0, thickness.max(1.0) * 2.0),
    };
    let mut out = Vec::new();
    let mut at = 0.0;
    while at < width {
        out.push((x + at, on.min(width - at)));
        at += on + off;
    }
    out
}

fn device_rect(t: Transform, b: Bounds) -> (f64, f64, f64, f64) {
    let x0 = f64::from(b.x * t.sx + t.tx);
    let y0 = f64::from(b.y * t.sy + t.ty);
    let x1 = f64::from(b.right() * t.sx + t.tx);
    let y1 = f64::from(b.bottom() * t.sy + t.ty);
    (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1))
}

/// Paints one Gaussian shadow layer of the device rectangle `rect` outside `caster`. Pixel alpha =
/// colour alpha × blurred coverage, rounded to 8 bits, then source-over (the TypeScript painter
/// builds an `ImageData` with the same numbers).
fn paint_shadow(
    pixmap: &mut Pixmap,
    rect: (f64, f64, f64, f64),
    caster: (f64, f64, f64, f64),
    sigma: f64,
    rgba: u32,
    mask: Option<&Mask>,
) {
    let [r, g, b, a] = channels(rgba);
    if a == 0 {
        return;
    }
    let reach = (sigma * 3.0).ceil();
    let (w, h) = (f64::from(pixmap.width()), f64::from(pixmap.height()));
    let x0 = (rect.0 - reach).floor().clamp(0.0, w);
    let x1 = (rect.2 + reach).ceil().clamp(0.0, w);
    let y0 = (rect.1 - reach).floor().clamp(0.0, h);
    let y1 = (rect.3 + reach).ceil().clamp(0.0, h);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (ix0, ix1, iy0, iy1) = (x0 as usize, x1 as usize, y0 as usize, y1 as usize);
    let cols: Vec<f64> = (ix0..ix1)
        .map(|x| {
            #[allow(clippy::cast_precision_loss)]
            let c = x as f64 + 0.5;
            blurred_interval(c, rect.0, rect.2, sigma)
        })
        .collect();
    let stride = pixmap.width() as usize;
    let mask = mask.map(Mask::data);
    let data = pixmap.data_mut();
    for y in iy0..iy1 {
        #[allow(clippy::cast_precision_loss)]
        let cy = y as f64 + 0.5;
        let fy = blurred_interval(cy, rect.1, rect.3, sigma);
        if fy <= 0.0 {
            continue;
        }
        for (i, fx) in cols.iter().enumerate() {
            let x = ix0 + i;
            #[allow(clippy::cast_precision_loss)]
            let cx = x as f64 + 0.5;
            if cx >= caster.0 && cx < caster.2 && cy >= caster.1 && cy < caster.3 {
                continue;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let alpha = (f64::from(a) * fx * fy).round().clamp(0.0, 255.0) as u32;
            let clip = mask.map_or(255, |m| u32::from(m[y * stride + x]));
            let alpha = (alpha * clip + 127) / 255;
            let di = (y * stride + x) * 4;
            blend_px(&mut data[di..di + 4], [r, g, b], alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blending_matches_premultiplied_source_over() {
        let mut px = [255u8, 255, 255, 255];
        blend_px(&mut px, [0, 0, 0], 255);
        assert_eq!(px, [0, 0, 0, 255]);
        let mut px = [255u8, 255, 255, 255];
        blend_px(&mut px, [0, 0, 0], 128);
        assert_eq!(px, [127, 127, 127, 255]);
        let mut px = [0u8, 0, 0, 0];
        blend_px(&mut px, [255, 0, 0], 128);
        assert_eq!(px, [128, 0, 0, 128]);
    }

    #[test]
    fn underline_dashes() {
        assert_eq!(
            underline_segments(0.0, 10.0, 1.0, LineStyle::Solid),
            vec![(0.0, 10.0)]
        );
        let dots = underline_segments(0.0, 5.0, 1.0, LineStyle::Dotted);
        assert_eq!(dots, vec![(0.0, 1.0), (2.0, 1.0), (4.0, 1.0)]);
        assert_eq!(
            underline_segments(0.0, 4.0, 1.0, LineStyle::Dashed),
            vec![(0.0, 3.0)]
        );
        assert!(underline_segments(0.0, f32::NAN, 1.0, LineStyle::Dotted).is_empty());
    }
}
