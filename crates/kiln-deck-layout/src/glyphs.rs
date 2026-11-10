// SPDX-License-Identifier: AGPL-3.0-or-later
//! Glyph runs from `kiln-text` as Scene ops: upright text becomes atlas bitmaps blitted at the
//! device scale (crisp, sub-pixel positioned); turned, mirrored or very large text becomes
//! outline paths, which painters transform like any path.
//!
//! The same conversion as the session's and Write's atlases; it lives here too so slides render
//! without a session (the CLI's PNGs, golden tests, thumbnails).
//!
//! A recording sink ([`Glyphs::recording`]) keeps each run with its text instead and leaves a
//! marker op in its place, so print can write real text where the run was drawn
//! ([`crate::print`]).

use std::collections::BTreeSet;

use kiln_scene::{AtlasFormat, AtlasPageId, AtlasUpdate, DrawOp, GlyphRun, Paint, PathData};
use kiln_text::{FontStore, GlyphAtlas, PageId, RunGlyphs, glyph_outline};
use kurbo::Affine;

/// A glyph atlas for one painter, with the pages changed since the last upload.
#[derive(Debug, Default)]
pub struct Glyphs {
    atlas: GlyphAtlas,
    dirty: BTreeSet<PageId>,
    scale: f32,
    /// Every glyph as an outline (thumbnails and tests that want no atlas).
    outlines_only: bool,
    /// Runs kept for print instead of drawn (see [`Glyphs::recording`]).
    recorded: Option<Vec<RecordedRun>>,
}

/// A run a recording sink kept: the glyphs, the text their clusters index into, where the run
/// was placed and how it was painted.
#[derive(Debug, Clone)]
pub struct RecordedRun {
    /// The glyphs.
    pub run: kiln_text::GlyphRun,
    /// The paragraph's text.
    pub text: String,
    /// The paragraph origin: scene px, or before `matrix` when there is one.
    pub origin: (f32, f32),
    /// The map of turned or mirrored text.
    pub matrix: Option<Affine>,
    /// The colour.
    pub paint: Paint,
}

/// Atlas pages of marker ops start here (a real atlas never has this many pages).
const RECORDED_PAGE: u32 = 0x8000_0000;

/// The recorded run a marker op of a recording sink stands for.
#[must_use]
pub fn recorded_index(op: &DrawOp) -> Option<usize> {
    match op {
        DrawOp::Glyphs(g) if g.page.0 >= RECORDED_PAGE && g.quads.is_empty() => {
            usize::try_from(g.page.0 - RECORDED_PAGE).ok()
        }
        _ => None,
    }
}

impl Glyphs {
    /// An empty atlas for a device scale.
    #[must_use]
    pub fn new(scale: f32) -> Self {
        Self {
            scale: if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            },
            ..Self::default()
        }
    }

    /// A sink that draws every glyph as an outline.
    #[must_use]
    pub fn outlines() -> Self {
        Self {
            scale: 1.0,
            outlines_only: true,
            ..Self::default()
        }
    }

    /// A sink that keeps every run with its text ([`Glyphs::take_recorded`]) and draws a marker op
    /// in its place ([`recorded_index`]), for print. Chart labels, which come without their
    /// text, become outlines.
    #[must_use]
    pub fn recording() -> Self {
        Self {
            scale: 1.0,
            outlines_only: true,
            recorded: Some(Vec::new()),
            ..Self::default()
        }
    }

    /// The runs kept so far (marker `i` is entry `i`); the sink starts a new list.
    pub fn take_recorded(&mut self) -> Vec<RecordedRun> {
        self.recorded
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// Switches to another device scale (the atlas starts over). Returns whether it changed.
    pub fn set_scale(&mut self, scale: f32) -> bool {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        if self.scale.to_bits() == scale.to_bits() {
            return false;
        }
        self.scale = scale;
        self.atlas.clear();
        self.dirty.clear();
        true
    }

    /// The device scale.
    #[must_use]
    pub fn scale(&self) -> f32 {
        if self.scale > 0.0 { self.scale } else { 1.0 }
    }

    /// Forgets every glyph (the painter was reset).
    pub fn clear(&mut self) {
        self.atlas.clear();
        self.dirty.clear();
    }

    /// Number of atlas pages.
    #[must_use]
    pub fn pages(&self) -> usize {
        self.atlas.pages().len()
    }

    /// Appends a run whose paragraph origin is at `origin` (CSS px in scene coordinates). With
    /// `matrix` (turned or mirrored text) the glyphs become outlines mapped by it; otherwise
    /// they are blitted from the atlas.
    pub fn push(
        &mut self,
        fonts: &FontStore,
        run: &kiln_text::GlyphRun,
        origin: (f32, f32),
        matrix: Option<Affine>,
        paint: Paint,
        out: &mut Vec<DrawOp>,
    ) {
        self.push_text(fonts, run, "", origin, matrix, paint, out);
    }

    /// As [`Glyphs::push`], with the paragraph text the run's clusters index into (kept by a
    /// recording sink).
    #[allow(clippy::too_many_arguments)]
    pub fn push_text(
        &mut self,
        fonts: &FontStore,
        run: &kiln_text::GlyphRun,
        text: &str,
        origin: (f32, f32),
        matrix: Option<Affine>,
        paint: Paint,
        out: &mut Vec<DrawOp>,
    ) {
        if let Some(rec) = &mut self.recorded
            && !text.is_empty()
        {
            // Print has no dark page to adapt to: the colour stays as the slide gives it.
            let k = u32::try_from(rec.len())
                .unwrap_or(0)
                .min(u32::MAX - RECORDED_PAGE);
            rec.push(RecordedRun {
                run: run.clone(),
                text: text.to_owned(),
                origin,
                matrix,
                paint,
            });
            out.push(DrawOp::Glyphs(GlyphRun {
                page: AtlasPageId(RECORDED_PAGE + k),
                paint,
                quads: Vec::new(),
            }));
            return;
        }
        let paint = on_slide(paint);
        if self.outlines_only || matrix.is_some() {
            outline(
                fonts,
                run,
                origin,
                matrix.unwrap_or(Affine::IDENTITY),
                paint,
                out,
            );
            return;
        }
        let scale = self.scale();
        let Ok(prepared) =
            self.atlas
                .prepare_run(fonts, run, origin.0 * scale, origin.1 * scale, scale)
        else {
            return;
        };
        self.dirty.extend(self.atlas.take_dirty());
        match prepared {
            RunGlyphs::Bitmaps(placed) => {
                let mut runs: Vec<GlyphRun> = Vec::new();
                for g in placed {
                    let page = AtlasPageId(g.glyph.page.0);
                    let idx = if let Some(i) = runs.iter().position(|r| r.page == page) {
                        i
                    } else {
                        runs.push(GlyphRun {
                            page,
                            paint,
                            quads: Vec::with_capacity(run.glyphs.len() * GlyphRun::STRIDE),
                        });
                        runs.len() - 1
                    };
                    runs[idx].push(
                        [
                            i32::from(g.glyph.x),
                            i32::from(g.glyph.y),
                            i32::from(g.glyph.width),
                            i32::from(g.glyph.height),
                        ],
                        [g.x, g.y],
                    );
                }
                out.extend(runs.into_iter().map(DrawOp::Glyphs));
            }
            RunGlyphs::Outlines(paths) => {
                let mut path = PathData::new();
                for p in &paths {
                    path.extend(&PathData::from(p));
                }
                if !path.is_empty() {
                    path.scale(1.0 / scale);
                    out.push(DrawOp::OutlineGlyphs { path, paint });
                }
            }
        }
    }

    /// Atlas pixels for the painter: every page with `all`, else the pages changed since the
    /// last call (only the rows in use).
    pub fn updates(&mut self, all: bool) -> Vec<AtlasUpdate> {
        self.dirty.extend(self.atlas.take_dirty());
        let ids: Vec<PageId> = if all {
            self.atlas.pages().iter().map(|p| p.id).collect()
        } else {
            self.dirty.iter().copied().collect()
        };
        self.dirty.clear();
        ids.into_iter()
            .filter_map(|id| self.atlas.page(id))
            .filter_map(|page| {
                let (format, bpp) = match page.format {
                    kiln_text::AtlasFormat::A8 => (AtlasFormat::A8, 1usize),
                    kiln_text::AtlasFormat::Rgba8 => (AtlasFormat::Rgba8, 4usize),
                };
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    clippy::cast_precision_loss
                )]
                let rows =
                    ((page.fill_ratio() * page.height as f32).ceil() as u32).min(page.height);
                if rows == 0 {
                    return None;
                }
                let len = rows as usize * page.width as usize * bpp;
                let mut data = page.data.get(..len)?.to_vec();
                if format == AtlasFormat::Rgba8 {
                    unpremultiply(&mut data);
                }
                Some(AtlasUpdate {
                    page: AtlasPageId(page.id.0),
                    format,
                    width: page.width,
                    height: page.height,
                    y: 0,
                    rows,
                    data,
                })
            })
            .collect()
    }
}

/// Glyph outlines of a run whose origin is at `origin`, mapped by `m`, as one op.
pub fn outline_run(
    fonts: &FontStore,
    run: &kiln_text::GlyphRun,
    origin: (f32, f32),
    m: Affine,
    paint: Paint,
    out: &mut Vec<DrawOp>,
) {
    outline(fonts, run, origin, m, paint, out);
}

/// Glyph outlines of a run, mapped by `m`, as one op.
fn outline(
    fonts: &FontStore,
    run: &kiln_text::GlyphRun,
    origin: (f32, f32),
    m: Affine,
    paint: Paint,
    out: &mut Vec<DrawOp>,
) {
    let mut path = kurbo::BezPath::new();
    for g in &run.glyphs {
        if let Some(mut p) = glyph_outline(fonts, run.font, g.id, run.size) {
            p.apply_affine(
                m * Affine::translate((f64::from(origin.0 + g.x), f64::from(origin.1 + g.y))),
            );
            path.extend(p);
        }
    }
    let path = PathData::from(&path);
    if !path.is_empty() {
        out.push(DrawOp::OutlineGlyphs { path, paint });
    }
}

/// Premultiplied to straight alpha (the wire format of colour pages).
fn unpremultiply(data: &mut [u8]) {
    for px in data.as_chunks_mut::<4>().0 {
        let a = u16::from(px[3]);
        if a == 0 || a == 255 {
            continue;
        }
        for c in &mut px[..3] {
            #[allow(clippy::cast_possible_truncation)]
            let v = ((u16::from(*c) * 255 + a / 2) / a).min(255) as u8;
            *c = v;
        }
    }
}

/// Charts draw their labels through this atlas too (one set of pages per painter). Chart ops are
/// in chart px; `offset` is where the chart's origin sits in scene px.
#[derive(Debug)]
pub struct ChartGlyphs<'a> {
    /// The atlas.
    pub glyphs: &'a mut Glyphs,
    /// The chart origin in scene px.
    pub offset: (f32, f32),
    /// The shape's matrix when the chart is turned (glyphs become outlines).
    pub matrix: Option<Affine>,
}

impl kiln_charts::text::GlyphSink for ChartGlyphs<'_> {
    fn push_run(
        &mut self,
        fonts: &FontStore,
        run: &kiln_text::GlyphRun,
        origin: (f32, f32),
        rotation: Option<(f32, (f32, f32))>,
        paint: Paint,
        out: &mut Vec<DrawOp>,
    ) {
        let paint = on_slide(paint);
        // Chart ops are drawn under a translation to the chart's origin; bitmaps are placed in
        // scene px, so they add the offset themselves (painters only translate them).
        match (rotation, self.matrix) {
            (None, None) if !self.glyphs.outlines_only => {
                // The translation pushed for the chart moves bitmaps by `offset`; place them
                // relative to the chart origin.
                let scale = self.glyphs.scale();
                let Ok(prepared) = self.glyphs.atlas.prepare_run(
                    fonts,
                    run,
                    origin.0 * scale,
                    origin.1 * scale,
                    scale,
                ) else {
                    return;
                };
                self.glyphs.dirty.extend(self.glyphs.atlas.take_dirty());
                match prepared {
                    RunGlyphs::Bitmaps(placed) => {
                        let mut runs: Vec<GlyphRun> = Vec::new();
                        for g in placed {
                            let page = AtlasPageId(g.glyph.page.0);
                            let idx = if let Some(i) = runs.iter().position(|r| r.page == page) {
                                i
                            } else {
                                runs.push(GlyphRun {
                                    page,
                                    paint,
                                    quads: Vec::new(),
                                });
                                runs.len() - 1
                            };
                            runs[idx].push(
                                [
                                    i32::from(g.glyph.x),
                                    i32::from(g.glyph.y),
                                    i32::from(g.glyph.width),
                                    i32::from(g.glyph.height),
                                ],
                                [g.x, g.y],
                            );
                        }
                        out.extend(runs.into_iter().map(DrawOp::Glyphs));
                    }
                    RunGlyphs::Outlines(paths) => {
                        let mut path = PathData::new();
                        for p in &paths {
                            path.extend(&PathData::from(p));
                        }
                        if !path.is_empty() {
                            path.scale(1.0 / scale);
                            out.push(DrawOp::OutlineGlyphs { path, paint });
                        }
                    }
                }
            }
            (rot, _) => {
                let m = rot.map_or(Affine::IDENTITY, |(deg, (px, py))| {
                    let (px, py) = (f64::from(px), f64::from(py));
                    Affine::translate((px, py))
                        * Affine::rotate(f64::from(deg).to_radians())
                        * Affine::translate((-px, -py))
                });
                outline(fonts, run, origin, m, paint, out);
            }
        }
    }
}

/// Text on a slide: its colours were chosen for the slide's own background, so an opaque colour
/// gets alpha `0xFE`, which tells the web painter not to adapt it for a dark page (as Grid's
/// cell text, `kiln_sheet_layout::look::rgb_on_fill`). One alpha step is invisible.
#[must_use]
pub fn on_slide(paint: Paint) -> Paint {
    match paint {
        Paint::Rgba(c) if c & 0xFF == 0xFF => Paint::Rgba((c & !0xFF) | 0xFE),
        p => p,
    }
}
