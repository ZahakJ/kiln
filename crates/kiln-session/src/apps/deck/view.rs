// SPDX-License-Identifier: AGPL-3.0-or-later
//! The stage: the current slide drawn by `kiln-deck-layout` on
//! its pasteboard as tiles of whole device pixels, and the overlay — the selection's outline,
//! its eight handles and rotation handle, smart guides while dragging, the marquee, the shape
//! being drawn, the View guides and gridlines, and the text caret, selection and IME
//! composition.

use kiln_common::NodeId;
use kiln_deck_layout::Glyphs;
use kiln_deck_layout::geom::{px, rect_px};
use kiln_deck_layout::guides::Guide;
use kiln_deck_layout::picture::Uploads;
use kiln_deck_layout::render::DrawnSlide;
use kiln_deck_layout::scene::{stage_extent, stage_slots, stage_tile};
use kiln_deck_layout::select::{handles, unflipped_matrix};
use kiln_scene::{
    AtlasUpdate, Bounds, ColorRole, DrawOp, FillRule, ImageUpload, LineCap, LineJoin, Paint,
    PathData, Scene, Stroke,
};

use super::DeckDoc;
use super::stage::DragKind;
use crate::frame::{FrameRequest, FrameSource};

/// What the stage painted last.
#[derive(Debug)]
pub(crate) struct Stage {
    pub scene: Scene,
    pub glyphs: Glyphs,
    pub uploads: Uploads,
    pub drawn: Option<DrawnSlide>,
    /// `(slide, epoch, editing shape)` the drawing is for.
    pub key: Option<(NodeId, u64, Option<NodeId>)>,
    /// Tile content version.
    pub version: u64,
    pub extent: Bounds,
}

impl Default for Stage {
    fn default() -> Self {
        Self {
            scene: Scene::new(
                1.0,
                Bounds::new(0.0, 0.0, 1280.0, 720.0),
                Paint::Role(ColorRole::Canvas),
            ),
            glyphs: Glyphs::new(1.0),
            uploads: Uploads::default(),
            drawn: None,
            key: None,
            version: 1,
            extent: Bounds::new(-64.0, -64.0, 1408.0, 848.0),
        }
    }
}

/// The accent of the app (handles, outlines, guides).
const ACCENT: Paint = Paint::Role(ColorRole::AppFill);

pub(super) fn stroke(paint: Paint, width: f32, dash: Vec<f32>) -> Stroke {
    Stroke {
        paint,
        width,
        join: LineJoin::Miter,
        cap: LineCap::Butt,
        dash,
    }
}

pub(super) fn quad(points: &[(f32, f32)]) -> PathData {
    let mut p = PathData::new();
    for (i, (x, y)) in points.iter().enumerate() {
        if i == 0 {
            p.move_to(*x, *y);
        } else {
            p.line_to(*x, *y);
        }
    }
    p.close();
    p
}

fn circle(cx: f32, cy: f32, r: f32) -> PathData {
    kiln_scene::shapes::rounded_rect(Bounds::new(cx - r, cy - r, r * 2.0, r * 2.0), r)
}

impl DeckDoc {
    /// Brings the drawing of the current slide up to date.
    fn ensure_drawn(&mut self) {
        // Fonts the host registered since (Arabic, symbols, mathematics on the web).
        let seen = crate::apps::scratch::fonts::registered_count();
        if seen > self.fonts_seen {
            let fonts = crate::apps::scratch::fonts::registered_after(self.fonts_seen);
            self.fonts_seen = seen;
            self.renderer.get_mut().add_fonts(&fonts);
            self.epoch += 1;
            self.global_version += 1;
        }
        let (Ok(slide), Some(owner)) = (self.slide_id(), self.owner()) else {
            self.stage.drawn = None;
            self.stage.key = None;
            return;
        };
        let editing = self.view.text.as_ref().map(|t| t.shape);
        let key = (slide, self.epoch, editing);
        if self.stage.key == Some(key) && self.stage.drawn.is_some() {
            return;
        }
        let opts = self.draw_options();
        let drawn = self
            .renderer
            .get_mut()
            .draw(&self.pres, owner, &opts, &mut self.stage.glyphs);
        self.stage.drawn = Some(drawn);
        self.stage.key = Some(key);
        self.stage.version += 1;
    }

    /// The overlay ops for the current view (slide px).
    #[allow(clippy::too_many_lines)]
    pub(crate) fn overlay(&self) -> Vec<DrawOp> {
        let mut ops = Vec::new();
        let z = self.zoom.max(0.05);
        let (sw, sh) = (px(self.pres.slide_size.cx), px(self.pres.slide_size.cy));
        if self.view.gridlines {
            let mut p = PathData::new();
            let step = 48.0;
            let mut x = step;
            while x < sw {
                p.move_to(x, 0.0).line_to(x, sh);
                x += step;
            }
            let mut y = step;
            while y < sh {
                p.move_to(0.0, y).line_to(sw, y);
                y += step;
            }
            ops.push(DrawOp::Path {
                path: p,
                fill: None,
                stroke: Some(stroke(
                    Paint::Role(ColorRole::Gridline),
                    1.0 / z,
                    vec![1.0 / z, 3.0 / z],
                )),
                rule: FillRule::NonZero,
            });
        }
        if self.view.guides {
            for (x0, y0, x1, y1) in self.guides.iter().map(|g| {
                if g.horizontal {
                    (-32.0, g.pos, sw + 32.0, g.pos)
                } else {
                    (g.pos, -32.0, g.pos, sh + 32.0)
                }
            }) {
                ops.push(DrawOp::Guide {
                    x0,
                    y0,
                    x1,
                    y1,
                    paint: Paint::Role(ColorRole::Guide),
                    dashed: true,
                });
            }
        }
        // The selection.
        let editing = self.view.text.as_ref().map(|t| t.shape);
        let many = self.view.shapes.len() > 1;
        for id in &self.view.shapes {
            let Some(x) = self.xfrm_of(*id) else { continue };
            let r = rect_px(&x);
            let m = unflipped_matrix(&x);
            let corner = |fx: f32, fy: f32| {
                let p = m * kurbo::Point::new(f64::from(fx * r.w), f64::from(fy * r.h));
                #[allow(clippy::cast_possible_truncation)]
                (p.x as f32, p.y as f32)
            };
            let outline = quad(&[
                corner(0.0, 0.0),
                corner(1.0, 0.0),
                corner(1.0, 1.0),
                corner(0.0, 1.0),
            ]);
            let dash = if editing == Some(*id) {
                vec![4.0 / z, 3.0 / z]
            } else {
                Vec::new()
            };
            ops.push(DrawOp::Path {
                path: outline,
                fill: None,
                stroke: Some(stroke(ACCENT, 1.25 / z, dash)),
                rule: FillRule::NonZero,
            });
        }
        self.crop_overlay(&mut ops, z);
        self.cells_overlay(&mut ops);
        if editing.is_none()
            && !self.view.crop
            && self
                .drag
                .as_ref()
                .is_none_or(|d| !d.active || !matches!(d.kind, DragKind::Move))
            && let Some(sx) = self.selection_xfrm()
        {
            let (hs, rot) = handles(&sx, z);
            let line = self.view.shapes.len() == 1 && self.is_line(self.view.shapes[0]);
            if many {
                let r = rect_px(&sx);
                ops.push(DrawOp::Path {
                    path: PathData::rect(r),
                    fill: None,
                    stroke: Some(stroke(ACCENT, 1.0 / z, vec![3.0 / z, 3.0 / z])),
                    rule: FillRule::NonZero,
                });
            }
            if !line {
                // The rotation handle on a short stem.
                if !many {
                    let top = hs[1];
                    let mut stem = PathData::new();
                    stem.move_to(top.0, top.1).line_to(rot.0, rot.1);
                    ops.push(DrawOp::Path {
                        path: stem,
                        fill: None,
                        stroke: Some(stroke(ACCENT, 1.0 / z, Vec::new())),
                        rule: FillRule::NonZero,
                    });
                    ops.push(DrawOp::Path {
                        path: circle(rot.0, rot.1, 5.5 / z),
                        fill: Some(Paint::Rgba(0xffff_ffff)),
                        stroke: Some(stroke(ACCENT, 1.5 / z, Vec::new())),
                        rule: FillRule::NonZero,
                    });
                    // A turning arrow inside.
                    let mut arc = PathData::new();
                    let rr = 2.6 / z;
                    arc.move_to(rot.0 + rr, rot.1).cubic_to(
                        rot.0 + rr,
                        rot.1 + rr * 1.4,
                        rot.0 - rr * 1.4,
                        rot.1 + rr * 1.2,
                        rot.0 - rr,
                        rot.1,
                    );
                    ops.push(DrawOp::Path {
                        path: arc,
                        fill: None,
                        stroke: Some(stroke(ACCENT, 1.1 / z, Vec::new())),
                        rule: FillRule::NonZero,
                    });
                }
                let small = {
                    let r = rect_px(&sx);
                    r.w * z < 24.0 || r.h * z < 24.0
                };
                for (i, (hx, hy)) in hs.iter().enumerate() {
                    if small && i % 2 == 1 {
                        continue;
                    }
                    ops.push(DrawOp::Path {
                        path: circle(*hx, *hy, 4.5 / z),
                        fill: Some(Paint::Rgba(0xffff_ffff)),
                        stroke: Some(stroke(ACCENT, 1.5 / z, Vec::new())),
                        rule: FillRule::NonZero,
                    });
                }
            } else {
                // Lines and connectors: a handle at each end.
                for (hx, hy) in [hs[0], hs[4]] {
                    ops.push(DrawOp::Path {
                        path: circle(hx, hy, 4.5 / z),
                        fill: Some(Paint::Rgba(0xffff_ffff)),
                        stroke: Some(stroke(ACCENT, 1.5 / z, Vec::new())),
                        rule: FillRule::NonZero,
                    });
                }
            }
        }
        // The gesture.
        if let Some(d) = &self.drag {
            for g in &d.guides {
                push_guide(&mut ops, g, z);
            }
            match &d.kind {
                DragKind::Marquee { .. } if d.active || d.start != d.now => {
                    let r = Bounds::from_corners(
                        d.start.0.min(d.now.0),
                        d.start.1.min(d.now.1),
                        d.start.0.max(d.now.0),
                        d.start.1.max(d.now.1),
                    );
                    ops.push(DrawOp::SelectionRects {
                        rects: vec![r],
                        paint: Paint::Role(ColorRole::Selection),
                    });
                    ops.push(DrawOp::Path {
                        path: PathData::rect(r),
                        fill: None,
                        stroke: Some(stroke(ACCENT, 1.0 / z, Vec::new())),
                        rule: FillRule::NonZero,
                    });
                }
                DragKind::Draw { preset } if kiln_deck_layout::render::is_line_preset(preset) => {
                    let mut line = PathData::new();
                    line.move_to(d.start.0, d.start.1).line_to(d.now.0, d.now.1);
                    ops.push(DrawOp::Path {
                        path: line,
                        fill: None,
                        stroke: Some(stroke(ACCENT, 1.25 / z, vec![4.0 / z, 3.0 / z])),
                        rule: FillRule::NonZero,
                    });
                    // The connection sites the ends attach to.
                    for (px, py) in [d.start, d.now] {
                        if let Some((_, q)) = self.site_near(px, py, None) {
                            ops.push(DrawOp::Path {
                                path: circle(q.0, q.1, 4.0 / z),
                                fill: Some(Paint::Rgba(0x40a0_40ff)),
                                stroke: Some(stroke(Paint::Rgba(0xffff_ffff), 1.0 / z, Vec::new())),
                                rule: FillRule::NonZero,
                            });
                        }
                    }
                }
                DragKind::Draw { preset } => {
                    let r = Bounds::from_corners(
                        d.start.0.min(d.now.0),
                        d.start.1.min(d.now.1),
                        d.start.0.max(d.now.0),
                        d.start.1.max(d.now.1),
                    );
                    let mut path = PathData::rect(r);
                    if preset != "textBox"
                        && let Ok(g) = kiln_ooxml::drawingml::presets::preset_geometry(
                            preset,
                            f64::from(r.w.max(1.0)),
                            f64::from(r.h.max(1.0)),
                            &[],
                        )
                    {
                        let mut p = PathData::new();
                        for gp in &g.paths {
                            p.extend(&PathData::from(&gp.to_bez_path(0.25)));
                        }
                        p.translate(r.x, r.y);
                        path = p;
                    }
                    ops.push(DrawOp::Path {
                        path,
                        fill: None,
                        stroke: Some(stroke(ACCENT, 1.25 / z, vec![4.0 / z, 3.0 / z])),
                        rule: FillRule::NonZero,
                    });
                }
                _ => {}
            }
        }
        // The Animations tab: each animated shape's effect numbers beside it.
        if self.view.badges {
            self.badge_ops(&mut ops, z);
        }
        // Text: selection, caret, composition.
        if let Some((caret, sel, comp)) = self.text_marks() {
            let rotated = sel.iter().any(|q| (q[0].1 - q[1].1).abs() > 0.01)
                || (caret[0].0 - caret[1].0).abs() > 0.01;
            if !sel.is_empty() {
                if rotated {
                    let mut p = PathData::new();
                    for q in &sel {
                        p.extend(&quad(q));
                    }
                    ops.push(DrawOp::Path {
                        path: p,
                        fill: Some(Paint::Role(ColorRole::Selection)),
                        stroke: None,
                        rule: FillRule::NonZero,
                    });
                } else {
                    ops.push(DrawOp::SelectionRects {
                        rects: sel
                            .iter()
                            .map(|q| Bounds::from_corners(q[0].0, q[0].1, q[2].0, q[2].1))
                            .collect(),
                        paint: Paint::Role(ColorRole::Selection),
                    });
                }
            }
            let (a, b) = (caret[0], caret[1]);
            if let Some(h) = comp
                && let Some(t) = &self.view.text
                && let Some(text) = &t.composition
            {
                self.composition_ops(&mut ops, text, a, h);
            } else if rotated {
                let mut p = PathData::new();
                p.move_to(a.0, a.1).line_to(b.0, b.1);
                ops.push(DrawOp::Path {
                    path: p,
                    fill: None,
                    stroke: Some(stroke(Paint::Role(ColorRole::Caret), 1.5 / z, Vec::new())),
                    rule: FillRule::NonZero,
                });
            } else {
                ops.push(DrawOp::Caret {
                    rect: Bounds::new(a.0 - 0.75 / z, a.1.min(b.1), 1.5 / z, (b.1 - a.1).abs()),
                    paint: Paint::Role(ColorRole::Caret),
                    spark: false,
                });
            }
        }
        // the others' selections and carets.
        ops.extend(self.shared_overlay(z));
        ops
    }

    /// The effect numbers of the Animations tab: a small box per click a shape animates on,
    /// stacked at its top-left corner; the selected effects' boxes in the accent colour.
    fn badge_ops(&self, ops: &mut Vec<DrawOp>, z: f32) {
        let Some(slide) = self.current_slide().and_then(|id| self.pres.slide(id)) else {
            return;
        };
        let selected = self.selected_effects();
        let mut shapes: Vec<(NodeId, Vec<(usize, bool)>)> = Vec::new();
        for (e, at) in slide
            .timeline
            .sequence
            .iter()
            .zip(slide.timeline.schedule())
        {
            let top = self.top_of(e.target.shape);
            let on = selected.contains(&e.id);
            let i = match shapes.iter().position(|(id, _)| *id == top) {
                Some(i) => i,
                None => {
                    shapes.push((top, Vec::new()));
                    shapes.len() - 1
                }
            };
            let list = &mut shapes[i].1;
            // One box per click (a later effect on the same click joins it).
            match list.iter_mut().find(|(c, _)| *c == at.click) {
                Some(b) => b.1 |= on,
                None => list.push((at.click, on)),
            }
        }
        let h = 15.0 / z;
        let gap = 2.0 / z;
        let mut r = self.renderer.borrow_mut();
        for (shape, labels) in shapes {
            let Some(x) = self.xfrm_of(shape) else {
                continue;
            };
            let b = rect_px(&x);
            for (k, (click, on)) in labels.iter().enumerate() {
                let text = click.to_string();
                #[allow(clippy::cast_precision_loss)]
                let w = (h * 0.62 * text.len() as f32 + 6.0 / z).max(h);
                #[allow(clippy::cast_precision_loss)]
                let top = b.y + (h + gap) * k as f32;
                let rect = Bounds::new(b.x - w - gap * 2.0, top, w, h);
                ops.push(DrawOp::FillRect {
                    rect,
                    paint: if *on {
                        ACCENT
                    } else {
                        Paint::Rgba(0xf2f2_f2ff)
                    },
                    radius: 2.0 / z,
                });
                ops.push(DrawOp::StrokeRect {
                    rect,
                    paint: if *on {
                        ACCENT
                    } else {
                        Paint::Rgba(0x8c8c_8cff)
                    },
                    width: 1.0 / z,
                    radius: 2.0 / z,
                });
                let style = kiln_text::SpanStyle {
                    size: h * 0.72,
                    weight: 600,
                    ..kiln_text::SpanStyle::default()
                };
                let spec = kiln_text::ParagraphSpec::plain(text, style, 1000.0);
                let layout = r.engine.layout(&spec);
                let Some(line) = layout.lines.first() else {
                    continue;
                };
                let tx = rect.x + (rect.w - line.width) / 2.0;
                let ty = rect.y + (rect.h - line.height) / 2.0;
                let mut g = Glyphs::outlines();
                let paint = if *on {
                    Paint::Rgba(0xffff_ffff)
                } else {
                    Paint::Rgba(0x2020_20ff)
                };
                for run in &line.runs {
                    g.push(r.engine.fonts(), run, (tx, ty), None, paint, ops);
                }
            }
        }
    }

    /// The IME composition drawn at the caret with its underline.
    fn composition_ops(&self, ops: &mut Vec<DrawOp>, text: &str, at: (f32, f32), height: f32) {
        let style = self
            .effective_run()
            .map_or_else(kiln_text::SpanStyle::default, |r| {
                let mut s = kiln_text::SpanStyle::default();
                #[allow(clippy::cast_precision_loss)]
                {
                    s.size = r.size.map_or(24.0, |hp| hp as f32 / 2.0 * 4.0 / 3.0);
                }
                if let Some(theme) = self.master().map(|m| m.theme.clone()) {
                    s.family = kiln_deck_layout::text::family(&r, &theme);
                }
                s
            });
        let mut r = self.renderer.borrow_mut();
        let spec = kiln_text::ParagraphSpec::plain(text, style, 100_000.0);
        let layout = r.engine.layout(&spec);
        let Some(line) = layout.lines.first() else {
            return;
        };
        let top = at.1;
        let base = top + (height - line.height).max(0.0) / 2.0;
        let mut g = Glyphs::outlines();
        for run in &line.runs {
            g.push(
                r.engine.fonts(),
                run,
                (at.0, base),
                None,
                Paint::Role(ColorRole::Ink),
                ops,
            );
        }
        ops.push(DrawOp::Underline {
            x: at.0,
            y: base + line.baseline + 2.0,
            width: line.width.max(1.0),
            thickness: 1.0,
            paint: Paint::Role(ColorRole::Composition),
            style: kiln_scene::LineStyle::Solid,
        });
    }
}

fn push_guide(ops: &mut Vec<DrawOp>, g: &Guide, z: f32) {
    if g.spacing {
        let mut p = PathData::new();
        p.move_to(g.x0, g.y0).line_to(g.x1, g.y1);
        // Small end bars show the equal gaps.
        let horizontal = (g.y0 - g.y1).abs() < 0.01;
        let t = 4.0 / z;
        for (x, y) in [(g.x0, g.y0), (g.x1, g.y1)] {
            if horizontal {
                p.move_to(x, y - t).line_to(x, y + t);
            } else {
                p.move_to(x - t, y).line_to(x + t, y);
            }
        }
        ops.push(DrawOp::Path {
            path: p,
            fill: None,
            stroke: Some(stroke(ACCENT, 1.0 / z, Vec::new())),
            rule: FillRule::NonZero,
        });
    } else {
        ops.push(DrawOp::Guide {
            x0: g.x0,
            y0: g.y0,
            x1: g.x1,
            y1: g.y1,
            paint: ACCENT,
            dashed: true,
        });
    }
}

impl FrameSource for DeckDoc {
    fn prepare(&mut self, request: &FrameRequest) -> kiln_common::Result<()> {
        self.zoom = request.viewport.zoom;
        if self.stage.glyphs.set_scale(request.scale) {
            self.stage.key = None;
            self.stage.uploads.reset();
        }
        self.ensure_drawn();
        let scale = request.scale;
        let Some(drawn) = &self.stage.drawn else {
            self.stage.scene = Scene::new(
                scale,
                Bounds::new(0.0, 0.0, 1.0, 1.0),
                Paint::Role(ColorRole::Canvas),
            );
            return Ok(());
        };
        let extent = stage_extent(drawn);
        if self.stage.scene.scale.to_bits() != scale.to_bits() {
            self.stage.scene = Scene::new(scale, extent, Paint::Role(ColorRole::Canvas));
        }
        self.stage.extent = extent;
        self.stage.scene.extent = extent;
        let slots = stage_slots(extent, scale, self.stage.version);
        self.stage.scene.set_slots(slots.clone());
        for slot in slots.iter().filter(|s| s.rect.intersects(&request.send)) {
            if self.stage.scene.is_built(slot.id) {
                continue;
            }
            let (tile, hits) = stage_tile(drawn, slot);
            self.stage.scene.insert_built(slot.version, tile, hits);
        }
        self.stage.scene.evict_outside(&request.keep);
        self.stage.scene.overlay = self.overlay();
        Ok(())
    }

    fn scene(&self) -> &Scene {
        &self.stage.scene
    }

    fn atlas_updates(&mut self, all: bool) -> Vec<AtlasUpdate> {
        self.stage.glyphs.updates(all)
    }

    fn image_updates(&mut self, all: bool) -> Vec<ImageUpload> {
        let used = self
            .stage
            .drawn
            .as_ref()
            .map(|d| d.images.clone())
            .unwrap_or_default();
        let pictures = &self.renderer.borrow().pictures;
        self.stage.uploads.take(pictures, &used, all)
    }

    fn extent(&self) -> Bounds {
        self.stage.extent
    }
}
