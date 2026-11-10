// SPDX-License-Identifier: AGPL-3.0-or-later
//! Show frames: each slide of the show becomes tiles — runs of shapes that do not move (the
//! background with them) and every animated shape on its own, in z-order — and each frame
//! composes them: the transition's plates (the outgoing slide, the incoming one, black or
//! white) as groups, and within a slide every animated shape (or paragraph range) as a layer
//! with the pose its effects give it at the show clock's time.
//!
//! Tiles are drawn once per slide and size and sent once; a frame in the middle of an
//! animation carries only the composition. The next slide's tiles are sent ahead of the
//! click that shows it.

use kiln_common::NodeId;
use kiln_deck::Owner;
use kiln_deck::animation::{AnimationEffect, Timeline};
use kiln_deck_layout::Glyphs;
use kiln_deck_layout::geom::{px, rect_px, text_matrix};
use kiln_deck_layout::motion::effects::{self, Pose, Stage, Target};
use kiln_deck_layout::motion::timeline::{self, Playhead};
use kiln_deck_layout::motion::transition::{self, Plate};
use kiln_deck_layout::picture::Uploads;
use kiln_deck_layout::render::{Options, Renderer, absolute_xfrm};
use kiln_protocol::commands::deck_show::{
    DeckShowFrame, DeckShowGroup, DeckShowLayer, DeckShowMode, DeckShowRamp, DeckShowScreen,
};
use kiln_scene::{Bounds, DrawOp, ImageId, Paint, PathData, SceneDelta, Tile, TileId, layer};
use kurbo::Affine;
use rustc_hash::{FxHashMap, FxHashSet};

use super::Show;
use crate::apps::deck::DeckDoc;

/// Tiles per slide (two per unit: the unit and its Colour Pulse tint).
const PER_SLIDE: u32 = 1024;
/// Units a slide is cut into at most (the rest of the shapes join the last static run).
const MAX_UNITS: usize = 500;

/// One piece of a slide.
#[derive(Debug, Clone)]
enum Unit {
    /// Shapes that do not move (and the background, in the first).
    Static(TileId),
    /// An animated top-level shape.
    Shape {
        tile: TileId,
        /// The same shape in its Colour Pulse colour.
        tint: Option<TileId>,
        shape: NodeId,
        /// Its frame, slide px.
        rect: Bounds,
        /// What it touches.
        visual: Bounds,
        /// Paragraph ranges animated on their own, with their bands (slide px).
        bands: Vec<((u32, u32), Bounds)>,
    },
}

/// The tiles of one slide.
#[derive(Debug, Clone, Default)]
struct SlideTiles {
    units: Vec<Unit>,
    images: Vec<ImageId>,
}

/// The show's tiles, glyphs and pictures, and what the painter holds.
#[derive(Debug, Default)]
pub(crate) struct ShowScene {
    /// Device pixels per slide px.
    scale: f32,
    glyphs: Option<Glyphs>,
    uploads: Uploads,
    slides: FxHashMap<usize, SlideTiles>,
    tiles: FxHashMap<TileId, Tile>,
    sent: FxHashSet<TileId>,
    /// The next delta resets the painter.
    reset: bool,
    /// The document epoch the tiles are for.
    epoch: u64,
    revision: u64,
}

impl ShowScene {
    /// Starts over at a scale (and for a document epoch).
    fn restart(&mut self, scale: f32, epoch: u64) {
        self.scale = scale;
        self.glyphs = Some(Glyphs::new(scale));
        self.uploads.reset();
        self.slides.clear();
        self.tiles.clear();
        self.sent.clear();
        self.reset = true;
        self.epoch = epoch;
    }
}

fn tile_id(slide: usize, k: u32) -> TileId {
    TileId {
        layer: layer::CONTENT,
        index: u32::try_from(slide)
            .unwrap_or(0)
            .saturating_mul(PER_SLIDE)
            .saturating_add(k),
    }
}

/// A rectangle grown to whole device pixels.
fn snap(b: Bounds, scale: f32) -> Bounds {
    let s = if scale > 0.0 { scale } else { 1.0 };
    let x0 = (b.x * s).floor() / s;
    let y0 = (b.y * s).floor() / s;
    let x1 = (b.right() * s).ceil() / s;
    let y1 = (b.bottom() * s).ceil() / s;
    Bounds::from_corners(x0, y0, x1.max(x0 + 1.0 / s), y1.max(y0 + 1.0 / s))
}

/// A tile of ops in slide px.
fn make_tile(id: TileId, rect: Bounds, ops: Vec<DrawOp>, clip: Option<Bounds>) -> Tile {
    let mut v = Vec::with_capacity(ops.len() + 4);
    v.push(DrawOp::PushTransform {
        matrix: [1.0, 0.0, 0.0, 1.0, -rect.x, -rect.y],
    });
    if let Some(c) = clip {
        v.push(DrawOp::PushClip {
            rect: c,
            path: None,
        });
    }
    v.extend(ops);
    if clip.is_some() {
        v.push(DrawOp::PopClip);
    }
    v.push(DrawOp::PopTransform);
    Tile::new(id, rect, v)
}

/// A paint in another colour, keeping its alpha.
fn tinted(p: &Paint, rgb: u32) -> Paint {
    let alpha = match p {
        Paint::Rgba(c) => c & 0xff,
        Paint::Role(_) => 0xff,
    };
    Paint::Rgba((rgb & 0xffff_ff00) | alpha)
}

/// A shape's ops in one colour (Colour Pulse): fills, outlines and text.
fn recolour(ops: &[DrawOp], rgb: u32) -> Vec<DrawOp> {
    ops.iter()
        .map(|op| match op {
            DrawOp::FillRect {
                rect,
                paint,
                radius,
            } => DrawOp::FillRect {
                rect: *rect,
                paint: tinted(paint, rgb),
                radius: *radius,
            },
            DrawOp::Path {
                path,
                fill,
                stroke,
                rule,
            } => DrawOp::Path {
                path: path.clone(),
                fill: fill.as_ref().map(|f| tinted(f, rgb)),
                stroke: stroke.as_ref().map(|s| kiln_scene::Stroke {
                    paint: tinted(&s.paint, rgb),
                    ..s.clone()
                }),
                rule: *rule,
            },
            DrawOp::Glyphs(run) => DrawOp::Glyphs(kiln_scene::GlyphRun {
                paint: tinted(&run.paint, rgb),
                ..run.clone()
            }),
            DrawOp::OutlineGlyphs { path, paint } => DrawOp::OutlineGlyphs {
                path: path.clone(),
                paint: tinted(paint, rgb),
            },
            DrawOp::Gradient { rect, radius, .. } => DrawOp::FillRect {
                rect: *rect,
                paint: Paint::Rgba(rgb | 0xff),
                radius: *radius,
            },
            other => other.clone(),
        })
        .collect()
}

/// `r` minus `holes`, as rectangles.
fn subtract(r: Bounds, holes: &[Bounds]) -> Vec<Bounds> {
    let mut parts = vec![r];
    for h in holes {
        let mut next = Vec::new();
        for p in parts {
            let Some(i) = p.intersection(h) else {
                next.push(p);
                continue;
            };
            // Above, below, left and right of the hole.
            let pieces = [
                Bounds::from_corners(p.x, p.y, p.right(), i.y),
                Bounds::from_corners(p.x, i.bottom(), p.right(), p.bottom()),
                Bounds::from_corners(p.x, i.y, i.x, i.bottom()),
                Bounds::from_corners(i.right(), i.y, p.right(), i.bottom()),
            ];
            next.extend(pieces.into_iter().filter(|b| b.w > 0.01 && b.h > 0.01));
        }
        parts = next;
    }
    parts
}

fn rects_path(rects: &[Bounds]) -> PathData {
    let mut p = PathData::new();
    for r in rects {
        p.extend(&PathData::rect(*r));
    }
    p
}

/// A ramp as the protocol has it.
fn ramp_of(r: &kiln_deck_layout::motion::Ramp) -> DeckShowRamp {
    DeckShowRamp {
        from: [r.from.0, r.from.1],
        to: [r.to.0, r.to.1],
        stops: r.stops.clone(),
    }
}

fn coeffs(m: Affine) -> [f32; 6] {
    kiln_deck_layout::geom::coeffs(m)
}

/// The bands of a text shape's paragraph ranges, in slide px.
fn bands(
    renderer: &mut Renderer,
    pres: &kiln_deck::Presentation,
    shape: NodeId,
    ranges: &[(u32, u32)],
    opts: &Options,
) -> Vec<((u32, u32), Bounds)> {
    let Some(layout) = renderer.text_layout(pres, shape, opts) else {
        return Vec::new();
    };
    let Some(xfrm) = absolute_xfrm(pres, shape) else {
        return Vec::new();
    };
    let m = text_matrix(&xfrm) * layout.flow_matrix();
    let mut out = Vec::new();
    for &(a, b) in ranges {
        let (Some(pa), Some(pb)) = (
            layout.paras.get(usize::try_from(a).unwrap_or(usize::MAX)),
            layout.paras.get(usize::try_from(b).unwrap_or(usize::MAX)),
        ) else {
            continue;
        };
        let rects = layout.selection((pa.id, 0), (pb.id, pb.map.doc_len()));
        let Some(first) = rects.first() else {
            continue;
        };
        let mut u = rects.iter().fold(*first, |acc, r| acc.union(r));
        // Full width (bullets included), a little room for ascenders and descenders.
        let area = layout.area;
        u = Bounds::from_corners(
            u.x.min(area.x),
            u.y - 3.0,
            u.right().max(area.right()),
            u.bottom() + 3.0,
        );
        let corners = [
            (u.x, u.y),
            (u.right(), u.y),
            (u.right(), u.bottom()),
            (u.x, u.bottom()),
        ];
        let mut bb: Option<Bounds> = None;
        for (x, y) in corners {
            let p = m * kurbo::Point::new(f64::from(x), f64::from(y));
            #[allow(clippy::cast_possible_truncation)]
            let q = Bounds::new(p.x as f32, p.y as f32, 0.0, 0.0);
            bb = Some(bb.map_or(q, |b| b.union(&q)));
        }
        if let Some(band) = bb {
            out.push(((a, b), band));
        }
    }
    out
}

impl DeckDoc {
    /// The running show's frame at `now` (the show clock), for a `width` × `height` CSS px screen
    /// at `dpr`. Moves the show on when time says so (slide timings, previews).
    pub(crate) fn show_frame(
        &mut self,
        now: f64,
        width: f32,
        height: f32,
        dpr: f32,
    ) -> Option<DeckShowFrame> {
        self.show.as_ref()?;
        self.advance_clock(now);
        if self.show.is_none() {
            // The show ended on this frame (a preview finished): the last frame says so.
            return Some(DeckShowFrame {
                delta: SceneDelta::default(),
                groups: Vec::new(),
                ink: Vec::new(),
                slide_width: px(self.pres.slide_size.cx),
                slide_height: px(self.pres.slide_size.cy),
                animating: false,
                advance_in_ms: None,
                status: self.show_status(),
            });
        }
        let opts = Options {
            edit: false,
            editing: None,
            today: self.draw_options().today,
            ..Options::default()
        };
        let epoch = self.epoch;
        let pres = &self.pres;
        let renderer = self.renderer.get_mut();
        let show = self.show.as_mut()?;
        let (sw, sh) = (
            px(pres.slide_size.cx).max(1.0),
            px(pres.slide_size.cy).max(1.0),
        );
        let fit = if width > 0.0 && height > 0.0 && width.is_finite() && height.is_finite() {
            (width / sw).min(height / sh)
        } else {
            1.0
        };
        let scale = (fit * dpr.clamp(0.25, 4.0)).clamp(0.05, 8.0);
        if show.scene.glyphs.is_none()
            || (show.scene.scale - scale).abs() > 1e-4
            || show.scene.epoch != epoch
        {
            show.scene.restart(scale, epoch);
        }
        let status = show.status(pres);
        let slide = show.slide();
        let mut groups = Vec::new();
        let mut needed: Vec<usize> = Vec::new();
        let fill_group = |rgba: u32, opacity: f32| DeckShowGroup {
            matrix: coeffs(Affine::IDENTITY),
            opacity,
            clip: None,
            ramp: None,
            layers: vec![DeckShowLayer {
                tile: None,
                fill: Some(Paint::Rgba(rgba)),
                matrix: coeffs(Affine::IDENTITY),
                opacity: 1.0,
                clip: None,
                ramp: None,
            }],
        };
        let now_ms = show.now;
        let since = show.since.unwrap_or(now_ms);
        let elapsed = (now_ms - since).max(0.0);
        let mut animating = false;
        match show.screen {
            DeckShowScreen::Black | DeckShowScreen::End => {
                groups.push(fill_group(transition::BLACK, 1.0));
            }
            DeckShowScreen::White => groups.push(fill_group(transition::WHITE, 1.0)),
            DeckShowScreen::Slide => {
                let here = playhead(show, pres, slide, elapsed);
                if let Some(e) = show.entering {
                    let p = if e.duration_ms > 0.0 {
                        #[allow(clippy::cast_possible_truncation)]
                        let p = (elapsed / e.duration_ms) as f32;
                        p
                    } else {
                        1.0
                    };
                    animating = p < 1.0;
                    for l in transition::frame(e.effect, p, sw, sh) {
                        let (which, at) = match l.plate {
                            Plate::Fill(c) => {
                                let mut g = fill_group(c, l.xform.opacity);
                                g.clip = l.xform.clip.clone();
                                g.ramp = l.xform.ramp.as_ref().map(ramp_of);
                                g.matrix = coeffs(l.xform.matrix);
                                groups.push(g);
                                continue;
                            }
                            Plate::Outgoing => match e.from {
                                Some((s, click)) => (s, Playhead::settled(click)),
                                None => {
                                    let mut g = fill_group(transition::BLACK, l.xform.opacity);
                                    g.clip = l.xform.clip.clone();
                                    g.ramp = l.xform.ramp.as_ref().map(ramp_of);
                                    g.matrix = coeffs(l.xform.matrix);
                                    groups.push(g);
                                    continue;
                                }
                            },
                            Plate::Incoming => (slide, Playhead::at(0, 0)),
                        };
                        if !needed.contains(&which) {
                            needed.push(which);
                        }
                        ensure_slide(
                            &mut show.scene,
                            renderer,
                            pres,
                            which,
                            &opts,
                            show.no_animation,
                        );
                        groups.push(DeckShowGroup {
                            matrix: coeffs(l.xform.matrix),
                            opacity: l.xform.opacity,
                            clip: l.xform.clip.clone(),
                            ramp: l.xform.ramp.as_ref().map(ramp_of),
                            layers: slide_layers(&show.scene, pres, which, at, show),
                        });
                    }
                } else {
                    needed.push(slide);
                    ensure_slide(
                        &mut show.scene,
                        renderer,
                        pres,
                        slide,
                        &opts,
                        show.no_animation,
                    );
                    if let Some(s) = pres.slides.get(slide) {
                        animating = timeline::running(&s.timeline, here);
                    }
                    groups.push(DeckShowGroup {
                        matrix: coeffs(Affine::IDENTITY),
                        opacity: 1.0,
                        clip: None,
                        ramp: None,
                        layers: slide_layers(&show.scene, pres, slide, here, show),
                    });
                }
            }
        }
        // The next slide's tiles go ahead of it.
        if let Some(n) = status.next.and_then(|n| usize::try_from(n).ok())
            && !needed.contains(&n)
            && show.mode == DeckShowMode::Show
        {
            ensure_slide(&mut show.scene, renderer, pres, n, &opts, show.no_animation);
            needed.push(n);
        }
        if !needed.contains(&slide) && show.scene.slides.contains_key(&slide) {
            needed.push(slide);
        }
        // Tiles of slides no longer near drop out.
        let keep: FxHashSet<usize> = needed.iter().copied().collect();
        show.scene.slides.retain(|s, _| keep.contains(s));
        let live: FxHashSet<TileId> = show
            .scene
            .slides
            .values()
            .flat_map(|t| t.units.iter())
            .flat_map(|u| match u {
                Unit::Static(id) => vec![*id],
                Unit::Shape { tile, tint, .. } => std::iter::once(*tile).chain(*tint).collect(),
            })
            .collect();
        show.scene.tiles.retain(|id, _| live.contains(id));
        let mut tiles: Vec<Tile> = show
            .scene
            .tiles
            .iter()
            .filter(|(id, _)| !show.scene.sent.contains(id))
            .map(|(_, t)| t.clone())
            .collect();
        tiles.sort_by_key(|t| t.id);
        let removed: Vec<TileId> = show
            .scene
            .sent
            .iter()
            .filter(|id| !live.contains(id))
            .copied()
            .collect();
        for t in &tiles {
            show.scene.sent.insert(t.id);
        }
        for r in &removed {
            show.scene.sent.remove(r);
        }
        let reset = std::mem::take(&mut show.scene.reset);
        let used: Vec<ImageId> = show
            .scene
            .slides
            .values()
            .flat_map(|t| t.images.iter().copied())
            .collect();
        let images = show.scene.uploads.take(&renderer.pictures, &used, reset);
        let atlas = show
            .scene
            .glyphs
            .as_mut()
            .map(|g| g.updates(reset))
            .unwrap_or_default();
        show.scene.revision += 1;
        let ink = if show.screen == DeckShowScreen::Slide {
            super::ink::ops(
                show.ink.get(&slide).map_or(&[][..], Vec::as_slice),
                show.laser,
            )
        } else {
            Vec::new()
        };
        let advance_in_ms = advance_in(show, pres, now_ms);
        // Previews and rehearsals keep the clock going (they end or count by themselves).
        let pending_preview = matches!(
            show.mode,
            DeckShowMode::PreviewTransition
                | DeckShowMode::PreviewAnimations
                | DeckShowMode::Rehearse
        );
        Some(DeckShowFrame {
            delta: SceneDelta {
                revision: show.scene.revision,
                reset,
                scale,
                extent: Bounds::new(0.0, 0.0, sw, sh),
                background: Paint::Rgba(transition::BLACK),
                tiles,
                removed,
                atlas,
                images,
                overlay: None,
            },
            groups,
            ink,
            slide_width: sw,
            slide_height: sh,
            animating: animating || advance_in_ms.is_some() || pending_preview,
            advance_in_ms,
            status,
        })
    }

    /// Moves the show's clock to `now`: transitions finish, slide timings and previews advance.
    fn advance_clock(&mut self, now: f64) {
        let pres = &self.pres;
        let Some(show) = self.show.as_mut() else {
            return;
        };
        let now = if now.is_finite() {
            now.max(show.now)
        } else {
            show.now
        };
        show.now = now;
        // A few steps at most per frame (a long pause does not replay a whole deck).
        for _ in 0..8 {
            let since = *show.since.get_or_insert(now);
            show.slide_since.get_or_insert(now);
            if show.ended || show.screen != DeckShowScreen::Slide {
                break;
            }
            if let Some(e) = show.entering {
                if now - since >= e.duration_ms {
                    // The slide's own effects time from the end of its transition.
                    show.entering = None;
                    show.since = Some(since + e.duration_ms);
                    if show.mode == DeckShowMode::PreviewTransition {
                        show.end();
                        break;
                    }
                    continue;
                }
                break;
            }
            let slide = show.slide();
            let Some(s) = pres.slides.get(slide) else {
                break;
            };
            let here = playhead(show, pres, slide, now - since);
            let busy = timeline::running(&s.timeline, here);
            if busy {
                break;
            }
            match show.mode {
                DeckShowMode::PreviewAnimations => {
                    if show.click < s.timeline.clicks() {
                        show.next(pres);
                        show.since = Some(now);
                        continue;
                    }
                    show.end();
                    break;
                }
                DeckShowMode::PreviewTransition => {
                    show.end();
                    break;
                }
                DeckShowMode::Show | DeckShowMode::Rehearse => {}
            }
            if show.manual {
                break;
            }
            let after = s.transition.as_ref().and_then(|t| t.advance_after_ms);
            let done_clicks = show.click >= show.clicks(pres);
            match after {
                Some(ms) if done_clicks => {
                    let entered = show.slide_since.unwrap_or(now);
                    if now - entered >= f64::from(ms) {
                        let at = entered + f64::from(ms);
                        show.next(pres);
                        // The next step starts when the time was up.
                        show.since = Some(at.min(now));
                        show.slide_since = Some(at.min(now));
                        continue;
                    }
                    break;
                }
                // With timings and clicks left, the clicks play by themselves too.
                Some(_) if !show.settled => {
                    show.next(pres);
                    show.since = Some(now);
                    continue;
                }
                _ => break,
            }
        }
        self.after_show_step();
    }
}

/// Where the slide shown stands.
fn playhead(show: &Show, pres: &kiln_deck::Presentation, slide: usize, elapsed: f64) -> Playhead {
    let clicks = pres.slides.get(slide).map_or(0, |s| s.timeline.clicks());
    if show.no_animation {
        return Playhead::settled(clicks);
    }
    if show.settled {
        return Playhead::settled(show.click);
    }
    if show.entering.is_some() {
        return Playhead::at(0, 0);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ms = elapsed.max(0.0) as u64;
    Playhead::at(show.click, ms)
}

/// Milliseconds until the slide advances by itself.
fn advance_in(show: &Show, pres: &kiln_deck::Presentation, now: f64) -> Option<u32> {
    if show.manual || show.screen != DeckShowScreen::Slide || show.entering.is_some() {
        return None;
    }
    let s = pres.slides.get(show.slide())?;
    let ms = s.transition.as_ref()?.advance_after_ms?;
    let entered = show.slide_since?;
    let left = (entered + f64::from(ms) - now).max(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(left.min(f64::from(u32::MAX)) as u32)
}

/// The shapes a slide animates (top-level ids) with the paragraph ranges animated on their own.
fn animated(pres: &kiln_deck::Presentation, slide: usize) -> Vec<(NodeId, Vec<(u32, u32)>)> {
    let Some(s) = pres.slides.get(slide) else {
        return Vec::new();
    };
    let mut out: Vec<(NodeId, Vec<(u32, u32)>)> = Vec::new();
    for e in &s.timeline.sequence {
        let top = top_level(pres, s, e.target.shape);
        let i = match out.iter().position(|(id, _)| *id == top) {
            Some(i) => i,
            None => {
                out.push((top, Vec::new()));
                out.len() - 1
            }
        };
        if let Some(r) = e.target.paragraphs
            && top == e.target.shape
            && !out[i].1.contains(&r)
        {
            out[i].1.push(r);
        }
    }
    out
}

/// The top-level shape of a slide that holds `shape`.
fn top_level(pres: &kiln_deck::Presentation, slide: &kiln_deck::Slide, shape: NodeId) -> NodeId {
    pres.locate(shape)
        .and_then(|loc| loc.path.first().copied())
        .and_then(|i| slide.shapes.get(i))
        .map_or(shape, |s| s.id)
}

/// Draws a slide into tiles unless it is drawn already.
fn ensure_slide(
    scene: &mut ShowScene,
    renderer: &mut Renderer,
    pres: &kiln_deck::Presentation,
    slide: usize,
    opts: &Options,
    no_animation: bool,
) {
    if scene.slides.contains_key(&slide) {
        return;
    }
    let Some(s) = pres.slides.get(slide) else {
        return;
    };
    let scale = scene.scale;
    let Some(glyphs) = scene.glyphs.as_mut() else {
        return;
    };
    let drawn = renderer.draw(pres, Owner::Slide(s.id), opts, glyphs);
    let moving = if no_animation {
        Vec::new()
    } else {
        animated(pres, slide)
    };
    let sw = drawn.width;
    let sh = drawn.height;
    let slide_rect = Bounds::new(0.0, 0.0, sw, sh);
    let reach = slide_rect.inflate(sw, sh);
    let mut units = Vec::new();
    let mut run: Vec<DrawOp> = drawn.background.clone();
    let mut k = 0u32;
    let flush = |run: &mut Vec<DrawOp>,
                 units: &mut Vec<Unit>,
                 k: &mut u32,
                 tiles: &mut FxHashMap<TileId, Tile>| {
        if run.is_empty() {
            return;
        }
        let id = tile_id(slide, *k);
        *k += 2;
        tiles.insert(
            id,
            make_tile(
                id,
                snap(slide_rect, scale),
                std::mem::take(run),
                Some(slide_rect),
            ),
        );
        units.push(Unit::Static(id));
    };
    for d in &drawn.shapes {
        let hit = moving.iter().find(|(id, _)| *id == d.id);
        match hit {
            Some((id, ranges)) if units.len() < MAX_UNITS => {
                flush(&mut run, &mut units, &mut k, &mut scene.tiles);
                let visual = d.bounds.intersection(&reach).unwrap_or(d.bounds);
                let rect_b = snap(visual.inflate(1.0, 1.0), scale);
                let id_t = tile_id(slide, k);
                k += 2;
                scene
                    .tiles
                    .insert(id_t, make_tile(id_t, rect_b, d.ops.clone(), None));
                let pulse = s.timeline.sequence.iter().find_map(|e| match &e.effect {
                    AnimationEffect::ColorPulse { color }
                        if top_level(pres, s, e.target.shape) == *id =>
                    {
                        Some(color.clone())
                    }
                    _ => None,
                });
                let tint = pulse.map(|color| {
                    let ctx = pres.color_context(Owner::Slide(s.id)).unwrap_or_default();
                    let c = color.unwrap_or_else(|| {
                        kiln_ooxml::drawingml::color::DrawingColor::scheme("accent1")
                    });
                    let rgba = kiln_ooxml::drawingml::color::resolve_color(&c, &ctx).to_color();
                    let rgb = (u32::from(rgba.r) << 24)
                        | (u32::from(rgba.g) << 16)
                        | (u32::from(rgba.b) << 8);
                    let tid = TileId {
                        layer: id_t.layer,
                        index: id_t.index + 1,
                    };
                    scene
                        .tiles
                        .insert(tid, make_tile(tid, rect_b, recolour(&d.ops, rgb), None));
                    tid
                });
                let rect = absolute_xfrm(pres, *id).map_or(d.bounds, |x| rect_px(&x));
                let bands = if ranges.is_empty() {
                    Vec::new()
                } else {
                    bands(renderer, pres, *id, ranges, opts)
                };
                units.push(Unit::Shape {
                    tile: id_t,
                    tint,
                    shape: *id,
                    rect,
                    visual,
                    bands,
                });
            }
            _ => run.extend(d.ops.iter().cloned()),
        }
    }
    flush(&mut run, &mut units, &mut k, &mut scene.tiles);
    scene.slides.insert(
        slide,
        SlideTiles {
            units,
            images: drawn.images,
        },
    );
}

/// A layer of a tile with a pose (composed over a base pose).
fn posed(tile: TileId, pose: &Pose, clip: Option<PathData>) -> Option<DeckShowLayer> {
    if !pose.shows() {
        return None;
    }
    Some(DeckShowLayer {
        tile: Some(tile),
        fill: None,
        matrix: coeffs(pose.matrix),
        opacity: pose.opacity,
        clip: clip.or_else(|| pose.clip.clone()),
        ramp: pose.ramp.as_ref().map(ramp_of),
    })
}

/// The layers of a slide at a playhead.
fn slide_layers(
    scene: &ShowScene,
    pres: &kiln_deck::Presentation,
    slide: usize,
    at: Playhead,
    show: &Show,
) -> Vec<DeckShowLayer> {
    let Some(tiles) = scene.slides.get(&slide) else {
        return Vec::new();
    };
    let Some(s) = pres.slides.get(slide) else {
        return Vec::new();
    };
    let timeline: &Timeline = &s.timeline;
    let stages = if show.no_animation {
        vec![Stage::After; timeline.sequence.len()]
    } else if show.reduced_motion {
        // Reduced motion: each effect appears (or goes) at once, on its own click.
        timeline::stages(timeline, at)
            .into_iter()
            .map(|s| match s {
                Stage::Running(_) => Stage::After,
                other => other,
            })
            .collect()
    } else {
        timeline::stages(timeline, at)
    };
    let size = (px(pres.slide_size.cx), px(pres.slide_size.cy));
    let mut out = Vec::new();
    for u in &tiles.units {
        match u {
            Unit::Static(id) => out.push(DeckShowLayer {
                tile: Some(*id),
                fill: None,
                matrix: coeffs(Affine::IDENTITY),
                opacity: 1.0,
                clip: None,
                ramp: None,
            }),
            Unit::Shape {
                tile,
                tint,
                shape,
                rect,
                visual,
                bands,
            } => {
                let mine: Vec<(usize, &kiln_deck::animation::Effect)> = timeline
                    .sequence
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| top_level(pres, s, e.target.shape) == *shape)
                    .collect();
                let whole: Vec<(&AnimationEffect, Stage)> = mine
                    .iter()
                    .filter(|(_, e)| {
                        e.target.paragraphs.is_none()
                            || !bands.iter().any(|(r, _)| Some(*r) == e.target.paragraphs)
                    })
                    .map(|(i, e)| (&e.effect, stages[*i]))
                    .collect();
                let base_target = Target {
                    rect: *rect,
                    visual: *visual,
                    slide: size,
                };
                let base = effects::fold(whole, &base_target);
                if bands.is_empty() {
                    if let Some(l) = posed(*tile, &base, None) {
                        out.push(l);
                    }
                } else {
                    // The shape without its animated paragraphs, then each paragraph range.
                    let holes: Vec<Bounds> = bands.iter().map(|(_, b)| *b).collect();
                    let rest = subtract(visual.inflate(2.0, 2.0), &holes);
                    if !rest.is_empty()
                        && let Some(l) = posed(*tile, &base, Some(rects_path(&rest)))
                    {
                        out.push(l);
                    }
                    for (range, band) in bands {
                        let effs: Vec<(&AnimationEffect, Stage)> = mine
                            .iter()
                            .filter(|(_, e)| e.target.paragraphs == Some(*range))
                            .map(|(i, e)| (&e.effect, stages[*i]))
                            .collect();
                        let t = Target {
                            rect: *band,
                            visual: *band,
                            slide: size,
                        };
                        let p = effects::fold(effs, &t);
                        let combined = Pose {
                            visible: base.visible && p.visible,
                            opacity: base.opacity * p.opacity,
                            matrix: base.matrix * p.matrix,
                            clip: Some(p.clip.clone().unwrap_or_else(|| PathData::rect(*band))),
                            tint: p.tint.clone().or_else(|| base.tint.clone()),
                            ramp: p.ramp.clone(),
                        };
                        if let Some(l) = posed(*tile, &combined, None) {
                            out.push(l);
                        }
                    }
                }
                // Colour Pulse: the tinted copy over it.
                if let (Some(tid), Some((_, amount))) = (tint, base.tint.as_ref())
                    && *amount > 0.0
                {
                    let pose = Pose {
                        opacity: base.opacity * amount,
                        ..base.clone()
                    };
                    if let Some(l) = posed(*tid, &pose, None) {
                        out.push(l);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_subtract() {
        let r = Bounds::new(0.0, 0.0, 100.0, 100.0);
        let parts = subtract(r, &[Bounds::new(10.0, 20.0, 80.0, 10.0)]);
        let area: f32 = parts.iter().map(|b| b.w * b.h).sum();
        assert!((area - (10_000.0 - 800.0)).abs() < 1e-2);
        assert!(subtract(r, &[r]).is_empty());
        assert_eq!(subtract(r, &[Bounds::new(200.0, 0.0, 5.0, 5.0)]), vec![r]);
    }

    #[test]
    fn tiles_snap_to_device_pixels_and_recolour_keeps_alpha() {
        let b = snap(Bounds::new(10.3, 7.7, 20.2, 5.1), 2.0);
        assert_eq!((b.x, b.y), (10.0, 7.5));
        assert!((b.right() * 2.0 - (b.right() * 2.0).round()).abs() < 1e-4);
        let ops = recolour(
            &[DrawOp::FillRect {
                rect: b,
                paint: Paint::Rgba(0x1122_3380),
                radius: 0.0,
            }],
            0xaabb_cc00,
        );
        assert!(matches!(
            ops[0],
            DrawOp::FillRect {
                paint: Paint::Rgba(0xaabb_cc80),
                ..
            }
        ));
        assert_eq!(tile_id(3, 5).index, 3 * PER_SLIDE + 5);
    }
}
