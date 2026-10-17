// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pages as Scene tiles (PLAN.md §3.6): one tile per page (tall pages of the web and draft views
//! in 1024-px bands), the page with its shadow from the theme's tokens, then floating objects
//! behind the text, headers and footers, the body, footnotes, column rules and floating objects
//! in front. Text in the automatic colour is painted with the theme's ink so the dark theme stays
//! legible; pictures are uploaded once per document view.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use kiln_doc::{Document, DrawingKind};
use kiln_scene::{
    Bounds, ColorRole, DrawOp, FillRule, HitIndex, ImageId, ImageUpload, Paint, Palette, PathData,
    Scene, Shadow, ShadowStyle, Stroke, Tile, TileId, TileSlot,
};
use kiln_text::FontStore;
use rustc_hash::{FxHashSet, FxHasher};

use crate::drawing::shape_visual;
use crate::engine::DocLayout;
use crate::flow::View;
use crate::glyphs::AtlasGlyphs;
use crate::media::MediaCache;
use crate::page::{DrawingItem, Item, LineItem, Page};
use crate::style::{Rule, RuleKind};

/// Pages taller than this are cut into bands of [`BAND`] px.
pub const TALL: f32 = 2048.0;
/// Height of a band of a tall page.
pub const BAND: f32 = 1024.0;
/// Tiles per page in the tile index (`page × 64 + band`).
pub const BANDS_PER_PAGE: u32 = 64;
/// Height of the tiles of pages in Print Layout and Read Mode: an edit sends and repaints only
/// the bands whose content changed, not the whole page (typing latency, B5).
pub const PAGE_BAND: f32 = 256.0;

fn ink(color: u32, auto: bool) -> Paint {
    if auto {
        Paint::Role(ColorRole::Ink)
    } else {
        Paint::Rgba(color)
    }
}

/// Word's automatic text colour over a shading: black on light fills, white on dark ones.
#[must_use]
pub fn contrast(bg: u32) -> u32 {
    let [r, g, b, _] = bg.to_be_bytes();
    let luma = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
    if luma < 110.0 {
        0xFFFF_FFFF
    } else {
        0x0000_00FF
    }
}

/// The page shadow's reach beyond the page in both themes: (left, top, right, bottom).
#[must_use]
pub fn shadow_reach() -> (f32, f32, f32, f32) {
    let a = Palette::light().page_shadow_reach();
    let b = Palette::dark().page_shadow_reach();
    (a.0.max(b.0), a.1.max(b.1), a.2.max(b.2), a.3.max(b.3))
}

/// A version of a page's content (changes whenever anything on it does).
#[must_use]
pub fn page_version(p: &Page) -> u64 {
    let mut h = FxHasher::default();
    p.number_text.hash(&mut h);
    p.origin.0.to_bits().hash(&mut h);
    p.origin.1.to_bits().hash(&mut h);
    p.width.to_bits().hash(&mut h);
    p.height.to_bits().hash(&mut h);
    p.body_dy.to_bits().hash(&mut h);
    for list in [
        &p.behind, &p.header, &p.items, &p.notes, &p.footer, &p.front,
    ] {
        list.len().hash(&mut h);
        for it in list {
            match it {
                Item::Line(l) => {
                    (Arc::as_ptr(&l.para) as usize).hash(&mut h);
                    l.para.id().hash(&mut h);
                    l.line.hash(&mut h);
                    l.x.to_bits().hash(&mut h);
                    l.y.to_bits().hash(&mut h);
                }
                Item::Fill { rect, color } => {
                    rect_bits(rect).hash(&mut h);
                    color.hash(&mut h);
                }
                Item::Rule {
                    x0,
                    y0,
                    x1,
                    y1,
                    rule,
                } => {
                    [x0, y0, x1, y1, &rule.width]
                        .map(|v| v.to_bits())
                        .hash(&mut h);
                    rule.color.hash(&mut h);
                }
                Item::Drawing(d) => {
                    rect_bits(&d.rect).hash(&mut h);
                    (Arc::as_ptr(&d.drawing) as usize).hash(&mut h);
                }
            }
        }
    }
    p.column_rules.len().hash(&mut h);
    h.finish()
}

/// The vertical extent of what an item paints, in page coordinates, as the painter decides
/// whether to draw it in a region (`None`: the whole page).
fn item_extent(it: &Item, dy: f32) -> Option<(f32, f32)> {
    let b = match it {
        Item::Line(l) => {
            let mut r = l.rect().inflate(8.0, 16.0);
            r.y += dy;
            r
        }
        Item::Fill { rect, .. } => rect.translate(0.0, dy),
        Item::Rule {
            x0,
            y0,
            x1,
            y1,
            rule,
        } => Bounds::from_corners(*x0, *y0 + dy, *x1, *y1 + dy).inflate(rule.width, rule.width),
        Item::Drawing(d) => d.rect.translate(0.0, dy),
    };
    (b.h.is_finite() && b.y.is_finite()).then_some((b.y, b.bottom()))
}

/// What an item contributes to a version.
fn hash_item(it: &Item, h: &mut FxHasher) {
    match it {
        Item::Line(l) => {
            (Arc::as_ptr(&l.para) as usize).hash(h);
            l.para.id().hash(h);
            l.line.hash(h);
            l.x.to_bits().hash(h);
            l.y.to_bits().hash(h);
        }
        Item::Fill { rect, color } => {
            rect_bits(rect).hash(h);
            color.hash(h);
        }
        Item::Rule {
            x0,
            y0,
            x1,
            y1,
            rule,
        } => {
            [x0, y0, x1, y1, &rule.width].map(|v| v.to_bits()).hash(h);
            rule.color.hash(h);
        }
        Item::Drawing(d) => {
            rect_bits(&d.rect).hash(h);
            (Arc::as_ptr(&d.drawing) as usize).hash(h);
        }
    }
}

/// Versions of horizontal bands of a page (`bands`: top and bottom in page coordinates, in
/// order): each changes whenever what the painter draws in that band does, so an edit re-sends
/// only the bands it touched. Everything that concerns the whole page (number, place, size,
/// column rules) is in every band's version.
#[must_use]
pub fn band_versions(p: &Page, bands: &[(f32, f32)]) -> Vec<u64> {
    let mut base = FxHasher::default();
    p.number_text.hash(&mut base);
    p.origin.0.to_bits().hash(&mut base);
    p.origin.1.to_bits().hash(&mut base);
    p.width.to_bits().hash(&mut base);
    p.height.to_bits().hash(&mut base);
    p.body_dy.to_bits().hash(&mut base);
    for &(x, y0, y1) in &p.column_rules {
        [x, y0, y1].map(f32::to_bits).hash(&mut base);
    }
    let base = base.finish();
    let mut hashers: Vec<FxHasher> = bands
        .iter()
        .map(|_| {
            let mut h = FxHasher::default();
            base.hash(&mut h);
            h
        })
        .collect();
    let lists: [(&[Item], f32); 6] = [
        (&p.behind, 0.0),
        (&p.header, 0.0),
        (&p.footer, 0.0),
        (&p.items, p.body_dy),
        (&p.notes, 0.0),
        (&p.front, 0.0),
    ];
    for (n, (list, dy)) in lists.into_iter().enumerate() {
        for (i, it) in list.iter().enumerate() {
            let extent = item_extent(it, dy);
            for (b, &(top, bottom)) in bands.iter().enumerate() {
                // The painter's test: the item inflated as above against the band inflated by 4.
                let hit = extent.is_none_or(|(y0, y1)| y0 < bottom + 4.0 && top - 4.0 < y1);
                if hit && let Some(h) = hashers.get_mut(b) {
                    (n, i).hash(h);
                    hash_item(it, h);
                }
            }
        }
    }
    hashers.into_iter().map(|h| h.finish()).collect()
}

fn rect_bits(r: &Bounds) -> [u32; 4] {
    [r.x.to_bits(), r.y.to_bits(), r.w.to_bits(), r.h.to_bits()]
}

/// Paints into a tile's op list.
struct PaintCx<'a> {
    fonts: &'a FontStore,
    glyphs: &'a mut AtlasGlyphs,
    media: &'a mut MediaCache,
    doc: &'a Document,
    uploads: &'a mut Vec<ImageUpload>,
    uploaded: &'a mut FxHashSet<u32>,
    /// Document position of the tile's top-left corner.
    tile: (f32, f32),
    /// Document position of the page's top-left corner.
    page: (f32, f32),
    /// The region painted, in page coordinates (bands of tall pages).
    region: Bounds,
    out: Vec<DrawOp>,
}

impl PaintCx<'_> {
    /// Page coordinates to tile-local document px.
    fn local(&self, b: Bounds) -> Bounds {
        b.translate(self.page.0 - self.tile.0, self.page.1 - self.tile.1)
    }

    fn visible(&self, b: &Bounds) -> bool {
        b.intersects(&self.region.inflate(4.0, 4.0))
    }

    fn item(&mut self, it: &Item, dy: f32) {
        match it {
            Item::Line(l) => {
                let mut l = l.clone();
                l.y += dy;
                if self.visible(&l.rect().inflate(8.0, 16.0)) {
                    self.line(&l);
                }
            }
            Item::Fill { rect, color } => {
                let r = rect.translate(0.0, dy);
                if self.visible(&r) {
                    let rect = self.local(r);
                    self.out.push(DrawOp::FillRect {
                        rect,
                        paint: Paint::Rgba(*color),
                        radius: 0.0,
                    });
                }
            }
            Item::Rule {
                x0,
                y0,
                x1,
                y1,
                rule,
            } => {
                self.rule(*x0, *y0 + dy, *x1, *y1 + dy, rule);
            }
            Item::Drawing(d) => {
                let mut d = d.clone();
                d.rect = d.rect.translate(0.0, dy);
                if self.visible(&d.rect) {
                    self.drawing(&d);
                }
            }
        }
    }

    fn line(&mut self, l: &LineItem) {
        let Some(info) = l.para.lines.get(l.line as usize) else {
            return;
        };
        let Some(piece) = l.para.pieces.get(info.piece as usize) else {
            return;
        };
        let Some(line) = piece.layout.lines.get(info.line as usize) else {
            return;
        };
        let src = &l.para.src;
        let s = self.glyphs.scale();
        // Device origin of the piece relative to the tile's device origin.
        let ox = (self.page.0 + l.x + piece.x) * s - (self.tile.0 * s).round();
        let oy = (self.page.1 + l.y + piece.y) * s - (self.tile.1 * s).round();
        for run in &line.runs {
            let span = piece.span_base + run.span;
            if src.hidden_spans.get(span).copied().unwrap_or(false) {
                continue;
            }
            let auto = src.ink_spans.get(span).copied().unwrap_or(false);
            let paint = match (auto, l.fill) {
                (true, Some(bg)) => Paint::Rgba(contrast(bg)),
                _ => ink(run.color, auto),
            };
            self.glyphs
                .push_run(self.fonts, run, (ox, oy), paint, &mut self.out);
        }
        for d in &line.decorations {
            let auto = d.color == kiln_text::BLACK;
            let paint = match (auto, l.fill) {
                (true, Some(bg)) => Paint::Rgba(contrast(bg)),
                _ => ink(d.color, auto),
            };
            let r = Bounds::new(
                l.x + piece.x + d.x0,
                l.y + piece.y + d.y,
                d.x1 - d.x0,
                d.thickness,
            );
            let rect = self.local(r);
            self.out.push(DrawOp::FillRect {
                rect,
                paint,
                radius: 0.0,
            });
        }
    }

    fn rule(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, rule: &Rule) {
        let paint = rule.color.map_or(Paint::Role(ColorRole::Ink), Paint::Rgba);
        let horizontal = (y1 - y0).abs() < (x1 - x0).abs();
        let w = rule.width;
        let bounds = Bounds::from_corners(x0, y0, x1, y1).inflate(w, w);
        if !self.visible(&bounds) {
            return;
        }
        match rule.kind {
            RuleKind::Single | RuleKind::Double => {
                let lines: &[(f32, f32)] = if rule.kind == RuleKind::Double {
                    &[(-w / 3.0, w / 3.0), (w / 3.0, w / 3.0)]
                } else {
                    &[(0.0, 1.0)]
                };
                for &(off, frac) in lines {
                    let t = if rule.kind == RuleKind::Double {
                        w * frac
                    } else {
                        w
                    };
                    let r = if horizontal {
                        Bounds::new(x0.min(x1), y0 + off - t / 2.0, (x1 - x0).abs(), t)
                    } else {
                        Bounds::new(x0 + off - t / 2.0, y0.min(y1), t, (y1 - y0).abs())
                    };
                    let rect = self.local(r);
                    self.out.push(DrawOp::FillRect {
                        rect,
                        paint,
                        radius: 0.0,
                    });
                }
            }
            RuleKind::Dotted | RuleKind::Dashed => {
                let mut path = PathData::new();
                let a = self.local(Bounds::new(x0, y0, 0.0, 0.0));
                let b = self.local(Bounds::new(x1, y1, 0.0, 0.0));
                path.move_to(a.x, a.y).line_to(b.x, b.y);
                let unit = w.max(0.75);
                let dash = if rule.kind == RuleKind::Dotted {
                    vec![unit, unit * 2.0]
                } else {
                    vec![unit * 4.0, unit * 3.0]
                };
                self.out.push(DrawOp::Path {
                    path,
                    fill: None,
                    stroke: Some(Stroke {
                        dash,
                        ..Stroke::solid(paint, w)
                    }),
                    rule: FillRule::NonZero,
                });
            }
        }
    }

    fn drawing(&mut self, d: &DrawingItem) {
        let rect = self.local(d.rect);
        let dr = &d.drawing;
        let rotated = dr.rotation != 0 || dr.flip_h || dr.flip_v;
        if rotated {
            #[allow(clippy::cast_precision_loss)]
            let angle = (dr.rotation as f32 / 60_000.0).to_radians();
            let (c, s) = (angle.cos(), angle.sin());
            let (fx, fy) = (
                if dr.flip_h { -1.0 } else { 1.0 },
                if dr.flip_v { -1.0 } else { 1.0 },
            );
            let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
            // Translate to the centre, rotate, flip, translate back.
            let (a, b, c2, d2) = (c * fx, s * fx, -s * fy, c * fy);
            let e = cx - a * cx - c2 * cy;
            let f = cy - b * cx - d2 * cy;
            self.out.push(DrawOp::PushTransform {
                matrix: [a, b, c2, d2, e, f],
            });
        }
        match &dr.kind {
            DrawingKind::Picture { media, crop } => {
                if let Some(pic) = self.media.get(self.doc, media) {
                    if self.uploaded.insert(pic.id) {
                        self.uploads.push(ImageUpload {
                            id: ImageId(pic.id),
                            width: pic.width,
                            height: pic.height,
                            data: pic.rgba.clone(),
                        });
                    }
                    #[allow(clippy::cast_precision_loss)]
                    let (w, h) = (pic.width as f32, pic.height as f32);
                    let c = crop.unwrap_or_default();
                    #[allow(clippy::cast_precision_loss)]
                    let f = |v: i32| (v as f32 / 100_000.0).clamp(-1.0, 1.0);
                    let src = Bounds::new(
                        w * f(c.left).max(0.0),
                        h * f(c.top).max(0.0),
                        (w * (1.0 - f(c.left).max(0.0) - f(c.right).max(0.0))).max(1.0),
                        (h * (1.0 - f(c.top).max(0.0) - f(c.bottom).max(0.0))).max(1.0),
                    );
                    self.out.push(DrawOp::Image {
                        image: ImageId(pic.id),
                        src,
                        dst: rect,
                        opacity: 1.0,
                    });
                } else {
                    self.placeholder(rect);
                }
            }
            DrawingKind::Shape { .. } => {
                let v = shape_visual(dr, d.rect.w, d.rect.h, &self.doc.theme);
                for p in v.paths {
                    let mut path = p.path;
                    path.translate(rect.x, rect.y);
                    self.out.push(DrawOp::Path {
                        path,
                        fill: p.fill.map(Paint::Rgba),
                        stroke: p.stroke.map(|(c, w)| Stroke::solid(Paint::Rgba(c), w)),
                        rule: FillRule::NonZero,
                    });
                }
            }
            DrawingKind::Chart { .. } | DrawingKind::Group | DrawingKind::Other => {
                self.placeholder(rect);
            }
        }
        if rotated {
            self.out.push(DrawOp::PopTransform);
        }
    }

    /// A picture that cannot be shown (EMF, WMF, charts until they render): a quiet frame.
    fn placeholder(&mut self, rect: Bounds) {
        self.out.push(DrawOp::FillRect {
            rect,
            paint: Paint::Rgba(0xF2_F0_EC_FF),
            radius: 0.0,
        });
        self.out.push(DrawOp::StrokeRect {
            rect: rect.inflate(-0.5, -0.5),
            paint: Paint::Rgba(0xC8_C2_B8_FF),
            width: 1.0,
            radius: 0.0,
        });
    }
}

/// The Scene of a document view: tiles for pages near the viewport, glyph atlas and pictures.
#[derive(Debug)]
pub struct DocScene {
    scene: Scene,
    glyphs: AtlasGlyphs,
    uploads: Vec<ImageUpload>,
    uploaded: FxHashSet<u32>,
    versions: Vec<u64>,
}

impl Default for DocScene {
    fn default() -> Self {
        Self::new()
    }
}

impl DocScene {
    /// An empty scene.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scene: Scene::new(1.0, Bounds::ZERO, Paint::Role(ColorRole::Canvas)),
            glyphs: AtlasGlyphs::new(),
            uploads: Vec::new(),
            uploaded: FxHashSet::default(),
            versions: Vec::new(),
        }
    }

    /// The scene.
    #[must_use]
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Replaces the overlay (selection, caret) without touching the tiles.
    pub fn set_overlay(&mut self, overlay: Vec<DrawOp>) {
        self.scene.overlay = overlay;
    }

    /// Glyph atlas pixels the painter lacks.
    pub fn atlas_updates(&mut self, all: bool) -> Vec<kiln_scene::AtlasUpdate> {
        self.glyphs.updates(all)
    }

    /// Pictures the painter lacks (`all` after the painter was reset: every picture again).
    pub fn image_updates(&mut self, all: bool, layout: &mut DocLayout) -> Vec<ImageUpload> {
        if all {
            self.uploads.clear();
            self.uploaded.clear();
            for pic in layout.media_mut().decoded() {
                self.uploaded.insert(pic.id);
                self.uploads.push(ImageUpload {
                    id: ImageId(pic.id),
                    width: pic.width,
                    height: pic.height,
                    data: pic.rgba.clone(),
                });
            }
        }
        std::mem::take(&mut self.uploads)
    }

    /// The tile slots of every page (bands for tall pages).
    fn slots(layout: &DocLayout, versions: &[u64], scale: f32) -> Vec<TileSlot> {
        let (l, t, r, b) = shadow_reach();
        let paged = !matches!(layout.view(), View::Web { .. } | View::Draft);
        let scale_bits = u64::from(scale.to_bits()).rotate_left(17);
        let mut out = Vec::new();
        for i in 0..layout.display_len() {
            let Some(rect) = layout.page_rect(i) else {
                continue;
            };
            let v = versions.get(i).copied().unwrap_or(0) ^ scale_bits;
            let page = u32::try_from(i).unwrap_or(u32::MAX);
            if paged && let Some(p) = layout.display_page(i) {
                // Bands of PAGE_BAND px from the top of the page's shadow to the bottom of it.
                let top = rect.y - t;
                let bottom = rect.bottom() + b;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let n = (((bottom - top) / PAGE_BAND).ceil() as u32).clamp(1, BANDS_PER_PAGE);
                let edges: Vec<(f32, f32)> = (0..n)
                    .map(|k| {
                        #[allow(clippy::cast_precision_loss)]
                        let y0 = top + k as f32 * PAGE_BAND;
                        let y1 = if k + 1 == n { bottom } else { y0 + PAGE_BAND };
                        (y0, y1)
                    })
                    .collect();
                let local: Vec<(f32, f32)> = edges
                    .iter()
                    .map(|&(y0, y1)| (y0 - rect.y, y1 - rect.y))
                    .collect();
                let versions = band_versions(p, &local);
                for (k, (&(y0, y1), bv)) in edges.iter().zip(versions).enumerate() {
                    let k = u32::try_from(k).unwrap_or(0);
                    out.push(TileSlot {
                        id: TileId::content(page * BANDS_PER_PAGE + k),
                        rect: Bounds::new(rect.x - l, y0, rect.w + l + r, y1 - y0),
                        version: bv ^ scale_bits ^ u64::from(k).rotate_left(40),
                    });
                }
            } else if rect.h > TALL {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let bands = (rect.h / BAND).ceil() as u32;
                for band in 0..bands {
                    #[allow(clippy::cast_precision_loss)]
                    let y = rect.y + band as f32 * BAND;
                    let h = BAND.min(rect.bottom() - y);
                    let index = if paged {
                        page * BANDS_PER_PAGE + band.min(BANDS_PER_PAGE - 1)
                    } else {
                        band
                    };
                    out.push(TileSlot {
                        id: TileId::content(index),
                        rect: Bounds::new(rect.x - l, y, rect.w + l + r, h),
                        version: v ^ u64::from(band).rotate_left(40),
                    });
                }
            } else {
                out.push(TileSlot {
                    id: TileId::content(page * BANDS_PER_PAGE),
                    rect: Bounds::new(rect.x - l, rect.y - t, rect.w + l + r, rect.h + t + b),
                    version: v,
                });
            }
        }
        out
    }

    /// Updates the scene for a frame: slots for every page, tiles for pages in `send`, eviction
    /// outside `keep`, and the overlay.
    pub fn prepare(
        &mut self,
        layout: &mut DocLayout,
        doc: &Document,
        scale: f32,
        send: &Bounds,
        keep: &Bounds,
        overlay: Vec<DrawOp>,
    ) {
        let rescaled = self.glyphs.set_scale(scale);
        if rescaled || self.scene.scale.to_bits() != scale.to_bits() {
            self.scene = Scene::new(scale, layout.extent(), Paint::Role(ColorRole::Canvas));
        }
        self.scene.extent = layout.extent();
        // Pages in Print Layout and Read Mode are versioned per band (`slots`).
        self.versions = if matches!(layout.view(), View::Web { .. } | View::Draft) {
            (0..layout.display_len())
                .filter_map(|i| layout.display_page(i).map(page_version))
                .collect()
        } else {
            Vec::new()
        };
        let slots = Self::slots(layout, &self.versions, scale);
        self.scene.set_slots(slots);
        let wanted: Vec<TileSlot> = self.scene.slots_in(send).copied().collect();
        for slot in wanted {
            if !self.scene.is_built(slot.id) {
                let tile = self.build(layout, doc, slot);
                self.scene
                    .insert_built(slot.version, tile, HitIndex::new(Vec::new()));
            }
        }
        self.scene.evict_outside(keep);
        self.scene.overlay = overlay;
    }

    fn build(&mut self, layout: &mut DocLayout, doc: &Document, slot: TileSlot) -> Tile {
        let paged = !matches!(layout.view(), View::Web { .. } | View::Draft);
        let page_index = if paged {
            (slot.id.index / BANDS_PER_PAGE) as usize
        } else {
            0
        };
        let ops = self.page_ops(layout, doc, page_index, slot.rect);
        Tile::new(slot.id, slot.rect, ops)
    }

    /// The ops of the part of page `index` inside `tile` (document coordinates), in tile-local
    /// document px.
    pub fn page_ops(
        &mut self,
        layout: &mut DocLayout,
        doc: &Document,
        index: usize,
        tile: Bounds,
    ) -> Vec<DrawOp> {
        let Some(page) = layout.display_page(index).cloned() else {
            return Vec::new();
        };
        let Some(prect) = layout.page_rect(index) else {
            return Vec::new();
        };
        let view = layout.view();
        let region = tile.translate(-prect.x, -prect.y);
        let mut out = Vec::new();
        let local_page = prect.translate(-tile.x, -tile.y);
        if matches!(view, View::Print | View::Read { .. }) {
            out.push(DrawOp::Shadow(Shadow {
                rect: local_page,
                radius: 0.0,
                style: ShadowStyle::Page,
            }));
        }
        out.push(DrawOp::FillRect {
            rect: local_page,
            paint: Paint::Role(ColorRole::Page),
            radius: 0.0,
        });
        // Split borrows: fonts from the engine, pictures from the media cache.
        let (fonts, media) = layout.fonts_and_media();
        let mut p = PaintCx {
            fonts,
            glyphs: &mut self.glyphs,
            media,
            doc,
            uploads: &mut self.uploads,
            uploaded: &mut self.uploaded,
            tile: (tile.x, tile.y),
            page: (prect.x, prect.y),
            region,
            out,
        };
        for it in &page.behind {
            p.item(it, 0.0);
        }
        for it in page.header.iter().chain(&page.footer) {
            p.item(it, 0.0);
        }
        for it in &page.items {
            p.item(it, page.body_dy);
        }
        for it in &page.notes {
            p.item(it, 0.0);
        }
        for &(x, y0, y1) in &page.column_rules {
            let rule = Rule {
                width: 0.75,
                color: None,
                kind: RuleKind::Single,
                space: 0.0,
                weight: 0.0,
            };
            p.rule(x, y0, x, y1, &rule);
        }
        for it in &page.front {
            p.item(it, 0.0);
        }
        p.out
    }
}
