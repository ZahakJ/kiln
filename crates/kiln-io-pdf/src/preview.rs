// SPDX-License-Identifier: AGPL-3.0-or-later
//! The print preview: one sheet of [`crate::sheet::impose`] painted as a Scene, so the
//! Backstage ▸ Print pane shows exactly what the PDF will contain (same imposition, same
//! pages, grayscale applied) through the ordinary Surface painter.
//!
//! The sheet is one tile in preview pixels (1 px = 1 sheet point ÷ 0.75, so 100 % shows the
//! paper at its real size); pages are mapped onto it without Scene transforms, because glyphs
//! are bitmaps positioned in device pixels. Text is rasterised into a glyph atlas at the
//! preview's device scale.

use kiln_common::Result;
use kiln_scene::{
    Bounds, DrawOp, Gradient, GradientKind, ImageId, ImageUpload, Paint, Palette, SceneDelta,
    Shadow, ShadowStyle, Tile, TileId,
};
use rustc_hash::FxHashSet;

use crate::options::PdfOptions;
use crate::printable::{LinkTarget, Mark, PageCanvas, PrintablePages, StructId, TextRun};
use crate::sheet::{PT_PER_PX, Sheet, Slot, impose};

/// The preview of one sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct Preview {
    /// Sheets the options produce (all copies).
    pub sheets: usize,
    /// The sheet shown (0-based, clamped to the sheets there are).
    pub sheet: usize,
    /// Sheet width, pt.
    pub width_pt: f32,
    /// Sheet height, pt.
    pub height_pt: f32,
    /// The pages on the sheet (0-based).
    pub pages: Vec<usize>,
    /// The painter's delta: a reset, the sheet tile, its glyph atlas and images. Document
    /// coordinates are preview px; the sheet sits at `(MARGIN, MARGIN)`.
    pub delta: SceneDelta,
}

/// Room around the sheet for its shadow, preview px.
pub const MARGIN: f32 = 24.0;

/// Renders sheet `sheet` of `doc` for a painter at `scale` device pixels per preview px.
///
/// # Errors
///
/// As [`crate::sheet::impose`].
pub fn preview(
    doc: &dyn PrintablePages,
    options: &PdfOptions,
    sheet: usize,
    scale: f32,
) -> Result<Preview> {
    let pages = doc.pages();
    let sheets = impose(&pages, options)?;
    let index = sheet.min(sheets.len().saturating_sub(1));
    let s = &sheets[index];
    let scale = if scale.is_finite() && scale > 0.0 {
        scale.clamp(0.05, 8.0)
    } else {
        1.0
    };
    let (w, h) = (s.width / PT_PER_PX, s.height / PT_PER_PX);
    let rect = Bounds::new(MARGIN, MARGIN, w, h);
    let mut canvas = SceneCanvas::new(doc, scale, options.grayscale);
    canvas.sheet(s, rect);
    let tile = Tile::new(
        TileId::content(0),
        Bounds::new(0.0, 0.0, w + 2.0 * MARGIN, h + 2.0 * MARGIN),
        canvas.out,
    );
    let delta = SceneDelta {
        revision: 0,
        reset: true,
        scale,
        extent: tile.rect,
        // Transparent: the pane behind the sheet shows through.
        background: Paint::Rgba(0),
        tiles: vec![tile],
        removed: Vec::new(),
        atlas: canvas.glyphs.updates(true),
        images: canvas.uploads,
        overlay: None,
    };
    Ok(Preview {
        sheets: sheets.len(),
        sheet: index,
        width_pt: s.width,
        height_pt: s.height,
        pages: s.slots.iter().map(|slot| slot.page).collect(),
        delta,
    })
}

/// A page canvas that records Scene ops, mapped from page px into the tile.
struct SceneCanvas<'a> {
    doc: &'a dyn PrintablePages,
    glyphs: kiln_doc_layout::glyphs::AtlasGlyphs,
    scale: f32,
    grayscale: bool,
    palette: Palette,
    out: Vec<DrawOp>,
    uploads: Vec<ImageUpload>,
    uploaded: FxHashSet<ImageId>,
    /// Page px → tile px: `x × s + tx`.
    map: (f32, f32, f32),
}

impl<'a> SceneCanvas<'a> {
    fn new(doc: &'a dyn PrintablePages, scale: f32, grayscale: bool) -> Self {
        let mut glyphs = kiln_doc_layout::glyphs::AtlasGlyphs::new();
        glyphs.set_scale(scale);
        Self {
            doc,
            glyphs,
            scale,
            grayscale,
            palette: Palette::light(),
            out: Vec::new(),
            uploads: Vec::new(),
            uploaded: FxHashSet::default(),
            map: (1.0, 0.0, 0.0),
        }
    }

    fn sheet(&mut self, sheet: &Sheet, rect: Bounds) {
        // The paper: white in both themes (it is paper), with the theme's page shadow.
        self.out.push(DrawOp::Shadow(Shadow {
            rect,
            radius: 0.0,
            style: ShadowStyle::Page,
        }));
        self.out.push(DrawOp::FillRect {
            rect,
            paint: Paint::Rgba(0xFFFF_FFFF),
            radius: 0.0,
        });
        for slot in &sheet.slots {
            self.slot(slot, rect);
        }
        // Several pages on a sheet: a hairline shows where each page ends (preview only; the
        // paper has no lines).
        if sheet.slots.len() > 1 {
            let pages = self.doc.pages();
            let k = 1.0 / PT_PER_PX;
            for slot in &sheet.slots {
                let Some(p) = pages.get(slot.page) else {
                    continue;
                };
                let frame = Bounds::new(
                    rect.x + slot.dx * k,
                    rect.y + slot.dy * k,
                    p.width * slot.scale * k,
                    p.height * slot.scale * k,
                );
                self.out.push(DrawOp::StrokeRect {
                    rect: frame,
                    paint: Paint::Rgba(0xC8C2_B8FF),
                    width: 0.75,
                    radius: 0.0,
                });
            }
        }
    }

    fn slot(&mut self, slot: &Slot, rect: Bounds) {
        // Sheet pt → preview px.
        let k = 1.0 / PT_PER_PX;
        self.map = (slot.scale * k, rect.x + slot.dx * k, rect.y + slot.dy * k);
        let clip = Bounds::new(
            rect.x + slot.clip[0] * k,
            rect.y + slot.clip[1] * k,
            slot.clip[2] * k,
            slot.clip[3] * k,
        );
        self.out.push(DrawOp::PushClip {
            rect: clip,
            path: None,
        });
        let doc = self.doc;
        doc.paint(slot.page, self);
        self.out.push(DrawOp::PopClip);
    }

    fn pt(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.map.0 + self.map.1, y * self.map.0 + self.map.2)
    }

    fn rect(&self, b: Bounds) -> Bounds {
        let (x, y) = self.pt(b.x, b.y);
        Bounds::new(x, y, b.w * self.map.0, b.h * self.map.0)
    }

    fn paint(&self, p: Paint) -> Paint {
        let c = self.palette.resolve(p);
        if !self.grayscale {
            return Paint::Rgba(c);
        }
        let [r, g, b, a] = c.to_be_bytes();
        let l = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let l = l.round().clamp(0.0, 255.0) as u8;
        Paint::Rgba(u32::from_be_bytes([l, l, l, a]))
    }

    fn gradient(&self, g: &Gradient) -> Gradient {
        let s = self.map.0;
        let kind = match g.kind {
            GradientKind::Linear { x0, y0, x1, y1 } => {
                let (x0, y0) = self.pt(x0, y0);
                let (x1, y1) = self.pt(x1, y1);
                GradientKind::Linear { x0, y0, x1, y1 }
            }
            GradientKind::Radial { cx, cy, r } => {
                let (cx, cy) = self.pt(cx, cy);
                GradientKind::Radial { cx, cy, r: r * s }
            }
        };
        Gradient {
            kind,
            stops: g
                .stops
                .iter()
                .map(|st| kiln_scene::GradientStop {
                    offset: st.offset,
                    paint: self.paint(st.paint),
                })
                .collect(),
        }
    }

    fn upload(&mut self, id: ImageId) {
        if !self.uploaded.insert(id) {
            return;
        }
        let Some(img) = self.doc.image(id) else {
            return;
        };
        let mut data = img.rgba.as_ref().clone();
        if self.grayscale {
            for px in data.as_chunks_mut::<4>().0 {
                let l =
                    0.299 * f32::from(px[0]) + 0.587 * f32::from(px[1]) + 0.114 * f32::from(px[2]);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let l = l.round().clamp(0.0, 255.0) as u8;
                px[..3].fill(l);
            }
        }
        let upload = ImageUpload {
            id,
            width: img.width,
            height: img.height,
            data,
        };
        if upload.is_valid() {
            self.uploads.push(upload);
        }
    }
}

impl PageCanvas for SceneCanvas<'_> {
    fn op(&mut self, op: &DrawOp) {
        let s = self.map.0;
        let mapped = match op {
            DrawOp::FillRect {
                rect,
                paint,
                radius,
            } => DrawOp::FillRect {
                rect: self.rect(*rect),
                paint: self.paint(*paint),
                radius: radius * s,
            },
            DrawOp::StrokeRect {
                rect,
                paint,
                width,
                radius,
            } => DrawOp::StrokeRect {
                rect: self.rect(*rect),
                paint: self.paint(*paint),
                width: width * s,
                radius: radius * s,
            },
            DrawOp::Path {
                path,
                fill,
                stroke,
                rule,
            } => {
                let mut path = path.clone();
                path.scale(s);
                path.translate(self.map.1, self.map.2);
                DrawOp::Path {
                    path,
                    fill: fill.map(|p| self.paint(p)),
                    stroke: stroke.as_ref().map(|st| kiln_scene::Stroke {
                        paint: self.paint(st.paint),
                        width: st.width * s,
                        dash: st.dash.iter().map(|d| d * s).collect(),
                        ..st.clone()
                    }),
                    rule: *rule,
                }
            }
            DrawOp::OutlineGlyphs { path, paint } => {
                let mut path = path.clone();
                path.scale(s);
                path.translate(self.map.1, self.map.2);
                DrawOp::OutlineGlyphs {
                    path,
                    paint: self.paint(*paint),
                }
            }
            DrawOp::Image {
                image,
                src,
                dst,
                opacity,
            } => {
                self.upload(*image);
                DrawOp::Image {
                    image: *image,
                    src: *src,
                    dst: self.rect(*dst),
                    opacity: *opacity,
                }
            }
            DrawOp::PushClip { rect, path } => DrawOp::PushClip {
                rect: self.rect(*rect),
                path: path.as_ref().map(|p| {
                    let mut p = p.clone();
                    p.scale(s);
                    p.translate(self.map.1, self.map.2);
                    p
                }),
            },
            DrawOp::PushTransform { matrix } => {
                // Conjugate by the page mapping: T·M·T⁻¹ keeps the transform's meaning.
                let [a, b, c, d, e, f] = *matrix;
                let (tx, ty) = (self.map.1, self.map.2);
                DrawOp::PushTransform {
                    matrix: [
                        a,
                        b,
                        c,
                        d,
                        s * e + tx - (a * tx + c * ty),
                        s * f + ty - (b * tx + d * ty),
                    ],
                }
            }
            DrawOp::PopClip => DrawOp::PopClip,
            DrawOp::PopTransform => DrawOp::PopTransform,
            DrawOp::Gradient {
                rect,
                radius,
                gradient,
            } => DrawOp::Gradient {
                rect: self.rect(*rect),
                radius: radius * s,
                gradient: self.gradient(gradient),
            },
            DrawOp::Shadow(sh) => match sh.style {
                ShadowStyle::Page => return,
                ShadowStyle::Drop {
                    dx,
                    dy,
                    blur,
                    spread,
                    paint,
                } => DrawOp::Shadow(Shadow {
                    rect: self.rect(sh.rect),
                    radius: sh.radius * s,
                    style: ShadowStyle::Drop {
                        dx: dx * s,
                        dy: dy * s,
                        blur: blur * s,
                        spread: spread * s,
                        paint: self.paint(paint),
                    },
                }),
            },
            DrawOp::Underline {
                x,
                y,
                width,
                thickness,
                paint,
                style,
            } => {
                let (x, y) = self.pt(*x, *y);
                DrawOp::Underline {
                    x,
                    y,
                    width: width * s,
                    thickness: thickness * s,
                    paint: self.paint(*paint),
                    style: *style,
                }
            }
            DrawOp::Glyphs(_)
            | DrawOp::Squiggle { .. }
            | DrawOp::Caret { .. }
            | DrawOp::SelectionRects { .. }
            | DrawOp::Guide { .. }
            | DrawOp::Handle { .. } => return,
        };
        self.out.push(mapped);
    }

    fn text(&mut self, run: &TextRun<'_>) {
        let glyphs = &run.run.glyphs;
        let range = run.glyphs.start.min(glyphs.len())..run.glyphs.end.min(glyphs.len());
        if range.is_empty() {
            return;
        }
        let s = self.map.0;
        // The run at the preview's size: glyph positions and size scaled with the page.
        let scaled = kiln_text::GlyphRun {
            font: run.run.font,
            size: run.run.size * s,
            span: run.run.span,
            color: run.run.color,
            level: run.run.level,
            text_range: run.run.text_range.clone(),
            glyphs: glyphs[range]
                .iter()
                .map(|g| kiln_text::PositionedGlyph {
                    x: g.x * s,
                    y: g.y * s,
                    advance: g.advance * s,
                    ..*g
                })
                .collect(),
        };
        let (ox, oy) = self.pt(run.origin.0, run.origin.1);
        // Device px relative to the tile's device origin (the tile sits at 0, 0).
        let origin = (ox * self.scale, oy * self.scale);
        let paint = self.paint(run.paint);
        let fonts = run.fonts;
        self.glyphs
            .push_run(fonts, &scaled, origin, paint, &mut self.out);
    }

    fn begin(&mut self, _mark: Mark) {}

    fn end(&mut self) {}

    fn link(&mut self, _rect: Bounds, _target: &LinkTarget, _node: StructId) {}
}
