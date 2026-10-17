// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Grid scene: 256 × 256 px tiles of cells, header strips and the corner, built for the
//! regions a frame shows (every pane, its prefetch margin, the header bands), versioned so the
//! painter repaints only what changed.
//!
//! **Tile ids.** Cell tiles live in layers [`CELL_LAYER`] and up (the high bits of the tile row
//! and column pick the layer, so ids stay unique on the whole 1,048,576 × 16,384 sheet); column
//! headers are layer [`COL_HEADER_LAYER`] (index = tile column), row headers
//! [`ROW_HEADER_LAYER`] (index = tile row), the corner [`CORNER_LAYER`]. Painters composite the
//! cell layers into each pane and the header layers into the header bands, all from one cache.
//!
//! **Invalidation.** A cell edit marks the tiles under the cell and the stretch of its row its
//! text may overflow into; structure, size, style and view changes renew every tile
//! ([`GridScene::invalidate_all`]). A tile's version mixes the epoch of its last change, its
//! position and the device scale, so an unchanged tile is never resent.

use kiln_scene::{
    AtlasUpdate, Bounds, ColorRole, DrawOp, HitIndex, Paint, Scene, Tile, TileId, TileSlot,
};
use kiln_sheet::{CellPos, Range, Sheet, Workbook};
use kiln_text::{FontStore, TextEngine};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::geom::{Pane, SheetGeometry, Snap, View};
use crate::glyphs::GlyphCache;
use crate::headers::{HeaderMarks, column_header_ops, corner_ops, row_header_ops};
use crate::paint::{CellPainter, LookCache, PaintCx, ViewOptions};

/// Side of a cell tile in CSS pixels.
pub const TILE: f32 = 256.0;
/// The first layer of cell tiles.
pub const CELL_LAYER: u16 = 16;
/// Column header tiles.
pub const COL_HEADER_LAYER: u16 = 2;
/// Row header tiles.
pub const ROW_HEADER_LAYER: u16 = 3;
/// The corner box.
pub const CORNER_LAYER: u16 = 4;
/// Drawings (charts) above the cells: one tile each, index = drawing number.
pub const DRAWING_LAYER: u16 = 8192;
/// Prefetched (not yet visible) tiles built per frame.
pub const PREFETCH_PER_FRAME: usize = 6;

/// The id of the cell tile at tile column `tx`, row `ty`.
#[must_use]
pub fn cell_tile_id(tx: u32, ty: u32) -> TileId {
    let layer = CELL_LAYER + (((ty >> 16) & 0x7F) << 4 | ((tx >> 16) & 0xF)) as u16;
    TileId {
        layer,
        index: ((ty & 0xFFFF) << 16) | (tx & 0xFFFF),
    }
}

/// Whether a tile id is a cell tile.
#[must_use]
pub fn is_cell_layer(layer: u16) -> bool {
    (CELL_LAYER..DRAWING_LAYER).contains(&layer)
}

fn mix(mut h: u64, v: u64) -> u64 {
    h ^= v
        .wrapping_add(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(h << 6)
        .wrapping_add(h >> 2);
    h
}

/// What a region holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// Cells (a pane).
    Cells,
    /// Column headers.
    ColumnHeaders,
    /// Row headers.
    RowHeaders,
    /// The corner box.
    Corner,
}

/// A region a frame shows: the sheet area of a pane or header band, with its prefetch margin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    /// What it holds.
    pub kind: RegionKind,
    /// What is visible.
    pub visible: Bounds,
    /// Tiles to build and send.
    pub send: Bounds,
    /// Tiles the painter keeps.
    pub keep: Bounds,
}

/// The regions of a view: its panes (with prefetch along the axes that scroll) and the header
/// bands over them.
#[must_use]
pub fn regions(geo: &SheetGeometry, view: &View, header_w: f32) -> Vec<Region> {
    let mut out = Vec::new();
    let hh = geo.header_h();
    let mut x_spans: Vec<(f32, f32, bool)> = Vec::new();
    let mut y_spans: Vec<(f32, f32, bool)> = Vec::new();
    for (pane, vis) in geo.visible_regions(view, header_w) {
        let scroll_x = matches!(pane, Pane::TopRight | Pane::BottomRight);
        let scroll_y = matches!(pane, Pane::BottomLeft | Pane::BottomRight);
        let (mx, my) = (
            if scroll_x { vis.w } else { 0.0 },
            if scroll_y { vis.h } else { 0.0 },
        );
        out.push(Region {
            kind: RegionKind::Cells,
            visible: vis,
            send: vis.inflate(mx, my),
            keep: vis.inflate(mx * 1.5, my * 1.5),
        });
        x_spans.push((vis.x, vis.w, scroll_x));
        y_spans.push((vis.y, vis.h, scroll_y));
    }
    if geo.headings {
        for (x, w, scrolls) in dedup(x_spans) {
            let m = if scrolls { w } else { 0.0 };
            let vis = Bounds::new(x, -hh, w, hh);
            out.push(Region {
                kind: RegionKind::ColumnHeaders,
                visible: vis,
                send: Bounds::new(x - m, -hh, w + 2.0 * m, hh),
                keep: Bounds::new(x - 1.5 * m, -hh, w + 3.0 * m, hh),
            });
        }
        for (y, h, scrolls) in dedup(y_spans) {
            let m = if scrolls { h } else { 0.0 };
            let vis = Bounds::new(-header_w, y, header_w, h);
            out.push(Region {
                kind: RegionKind::RowHeaders,
                visible: vis,
                send: Bounds::new(-header_w, y - m, header_w, h + 2.0 * m),
                keep: Bounds::new(-header_w, y - 1.5 * m, header_w, h + 3.0 * m),
            });
        }
        let corner = Bounds::new(-header_w, -hh, header_w, hh);
        out.push(Region {
            kind: RegionKind::Corner,
            visible: corner,
            send: corner,
            keep: corner,
        });
    }
    out
}

fn dedup(mut spans: Vec<(f32, f32, bool)>) -> Vec<(f32, f32, bool)> {
    spans.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    spans.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    spans
}

/// What a frame needs from the scene.
#[derive(Debug, Clone)]
pub struct SceneRequest<'a> {
    /// The view.
    pub view: View,
    /// Device pixels per CSS pixel (zoom × DPR).
    pub scale: f32,
    /// Row header width.
    pub header_w: f32,
    /// The scrollable size of the sheet (cells), for the scroll bars.
    pub extent: (f32, f32),
    /// The selection for the headers.
    pub marks: &'a HeaderMarks,
    /// Whether everything is selected (the corner lights up).
    pub all_selected: bool,
    /// View options.
    pub opts: &'a ViewOptions,
    /// The overlay ops (sheet pixels).
    pub overlay: Vec<DrawOp>,
}

/// The scene of one sheet view.
#[derive(Debug)]
pub struct GridScene {
    scene: Scene,
    glyphs: GlyphCache,
    text: TextEngine,
    looks: LookCache,
    /// Counts invalidations (partial ones stamp tiles with it).
    epoch: u64,
    /// The epoch of the last [`GridScene::invalidate_all`].
    full: u64,
    /// Per cell tile `(tx, ty)`: the epoch of its last partial invalidation.
    dirty: FxHashMap<(u32, u32), u64>,
    /// Header epoch (sizes; selection marks are in the version).
    header_epoch: u64,
    regions: Vec<Region>,
    tiles_built: u64,
    /// Prefetch tiles still to build after the last frame.
    pending_prefetch: usize,
}

impl GridScene {
    /// A scene drawing text with `fonts`.
    #[must_use]
    pub fn new(fonts: FontStore) -> Self {
        Self {
            scene: Scene::new(1.0, Bounds::ZERO, Paint::Role(ColorRole::Page)),
            glyphs: GlyphCache::new(),
            text: TextEngine::new(fonts),
            looks: LookCache::default(),
            epoch: 1,
            full: 1,
            dirty: FxHashMap::default(),
            header_epoch: 1,
            regions: Vec::new(),
            tiles_built: 0,
            pending_prefetch: 0,
        }
    }

    /// The scene of the last [`GridScene::prepare`].
    #[must_use]
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Text shaping (the in-cell editor and autofit measure with it).
    pub fn text_engine(&mut self) -> &mut TextEngine {
        &mut self.text
    }

    /// The regions of the last frame (for the frame's send and keep lists).
    #[must_use]
    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    /// Prefetch tiles the last frame left for later frames.
    #[must_use]
    pub fn pending_prefetch(&self) -> usize {
        self.pending_prefetch
    }

    /// Tiles built since the scene was made (diagnostics, benches).
    #[must_use]
    pub fn tiles_built(&self) -> u64 {
        self.tiles_built
    }

    /// Every tile is stale (structure, sizes, styles, view options or another sheet changed).
    pub fn invalidate_all(&mut self) {
        self.epoch += 1;
        self.full = self.epoch;
        self.header_epoch += 1;
        self.dirty.clear();
        self.looks.clear();
    }

    /// The header tiles are stale (column widths, row heights).
    pub fn invalidate_headers(&mut self) {
        self.header_epoch += 1;
    }

    /// The cells of `area` changed: their tiles and the row stretch their text may overflow
    /// into (up to the neighbouring content in each row) are stale.
    pub fn invalidate_cells(&mut self, geo: &SheetGeometry, sheet: &Sheet, area: Range) {
        self.epoch += 1;
        let e = self.epoch;
        // Overflow: from the previous content cell to the next one in each changed row (whole
        // sheet width for long areas, which is cheap: only built tiles are affected).
        let mut c_lo = area.c0;
        let mut c_hi = area.c1;
        if area.height() <= 64 {
            for r in area.r0..=area.r1 {
                if let Some(chunk) = sheet.cells().chunk(r) {
                    let i = chunk.lower_bound(area.c0);
                    let prev = (0..i)
                        .rev()
                        .map(|k| chunk.at(k))
                        .find(|(_, d)| d.has_content())
                        .map(|(c, _)| c);
                    let j = chunk.lower_bound(area.c1.saturating_add(1));
                    let next = (j..chunk.len())
                        .map(|k| chunk.at(k))
                        .find(|(_, d)| d.has_content())
                        .map(|(c, _)| c);
                    c_lo = c_lo.min(prev.unwrap_or(0));
                    c_hi = c_hi.max(next.unwrap_or(geo.cols.len().saturating_sub(1)));
                } else {
                    c_lo = 0;
                    c_hi = geo.cols.len().saturating_sub(1);
                }
            }
        } else {
            c_lo = 0;
            c_hi = geo.cols.len().saturating_sub(1);
        }
        let rect = geo.range_rect(&Range::new(area.r0, c_lo, area.r1, c_hi));
        // Merges intersecting the area repaint whole.
        let rect = geo
            .merges
            .iter()
            .filter(|m| m.intersects(&area))
            .fold(rect, |acc, m| acc.union(&geo.range_rect(m)));
        // Borders reach one pixel into the neighbours.
        let rect = rect.inflate(3.0, 3.0);
        let tx0 = (rect.x.max(0.0) / TILE).floor() as u32;
        let tx1 = (rect.right().max(0.0) / TILE).floor() as u32;
        let ty0 = (rect.y.max(0.0) / TILE).floor() as u32;
        let ty1 = (rect.bottom().max(0.0) / TILE).floor() as u32;
        // Only built tiles need marking; unbuilt ones are built fresh.
        let built: FxHashSet<(u32, u32)> = self
            .scene
            .slots()
            .iter()
            .filter(|s| is_cell_layer(s.id.layer))
            .map(|s| tile_coords(s.rect))
            .collect();
        for (tx, ty) in built {
            if (tx0..=tx1).contains(&tx) && (ty0..=ty1).contains(&ty) {
                self.dirty.insert((tx, ty), e);
            }
        }
    }

    fn cell_version(&self, tx: u32, ty: u32, scale_bits: u64) -> u64 {
        let d = self.dirty.get(&(tx, ty)).copied().unwrap_or(0);
        let mut h = mix(0x51_7CC1_B727_220A, self.full);
        h = mix(h, d);
        h = mix(h, u64::from(tx) << 32 | u64::from(ty));
        mix(h, scale_bits)
    }

    fn full_epoch(&self) -> u64 {
        self.header_epoch
    }

    /// Brings the scene up to date for a frame.
    pub fn prepare(
        &mut self,
        wb: &mut Workbook,
        sheet_index: usize,
        geo: &SheetGeometry,
        req: SceneRequest<'_>,
    ) {
        let scale = req.scale;
        if self.glyphs.set_scale(scale) || self.scene.scale.to_bits() != scale.to_bits() {
            self.scene = Scene::new(scale, Bounds::ZERO, Paint::Role(ColorRole::Page));
        }
        let hw = req.header_w;
        let hh = geo.header_h();
        self.scene.extent = Bounds::new(-hw, -hh, req.extent.0 + hw, req.extent.1 + hh);
        self.regions = regions(geo, &req.view, hw);
        let scale_bits = u64::from(scale.to_bits());
        let snap = Snap(scale);

        // Slots: every tile in a keep region.
        let mut slots: Vec<TileSlot> = Vec::new();
        let mut seen: FxHashSet<TileId> = FxHashSet::default();
        let max_x = geo.cols.total() as f32;
        let max_y = geo.rows.total() as f32;
        for region in &self.regions {
            let k = region.keep;
            if region.kind == RegionKind::Cells {
                // Cells.
                let x0 = k.x.max(0.0);
                let y0 = k.y.max(0.0);
                let x1 = k.right().min(max_x);
                let y1 = k.bottom().min(max_y);
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                let (tx0, tx1) = ((x0 / TILE) as u32, ((x1 - 0.001) / TILE) as u32);
                let (ty0, ty1) = ((y0 / TILE) as u32, ((y1 - 0.001) / TILE) as u32);
                for ty in ty0..=ty1 {
                    for tx in tx0..=tx1 {
                        let id = cell_tile_id(tx, ty);
                        if seen.insert(id) {
                            let x = tx as f32 * TILE;
                            let y = ty as f32 * TILE;
                            slots.push(TileSlot {
                                id,
                                rect: Bounds::new(x, y, TILE.min(max_x - x), TILE.min(max_y - y)),
                                version: self.cell_version(tx, ty, scale_bits),
                            });
                        }
                    }
                }
            } else if region.kind == RegionKind::Corner {
                let id = TileId {
                    layer: CORNER_LAYER,
                    index: 0,
                };
                if seen.insert(id) {
                    let v = mix(
                        mix(self.full_epoch(), scale_bits),
                        u64::from(req.all_selected) | (hw.to_bits() as u64) << 1,
                    );
                    slots.push(TileSlot {
                        id,
                        rect: Bounds::new(-hw, -hh, hw, hh),
                        version: v,
                    });
                }
            } else if region.kind == RegionKind::ColumnHeaders {
                let x0 = k.x.max(0.0);
                let x1 = k.right().min(max_x);
                if x1 <= x0 {
                    continue;
                }
                for tx in (x0 / TILE) as u32..=((x1 - 0.001) / TILE) as u32 {
                    let id = TileId {
                        layer: COL_HEADER_LAYER,
                        index: tx,
                    };
                    if seen.insert(id) {
                        let x = tx as f32 * TILE;
                        let w = TILE.min(max_x - x);
                        let (c0, c1) = geo.cols.indices_in(f64::from(x), f64::from(x + w));
                        let v = mix(
                            mix(mix(self.full_epoch(), scale_bits), u64::from(tx)),
                            req.marks.col_signature(c0, c1) ^ u64::from(hh.to_bits()),
                        );
                        slots.push(TileSlot {
                            id,
                            rect: Bounds::new(x, -hh, w, hh),
                            version: v,
                        });
                    }
                }
            } else {
                let y0 = k.y.max(0.0);
                let y1 = k.bottom().min(max_y);
                if y1 <= y0 {
                    continue;
                }
                for ty in (y0 / TILE) as u32..=((y1 - 0.001) / TILE) as u32 {
                    let id = TileId {
                        layer: ROW_HEADER_LAYER,
                        index: ty,
                    };
                    if seen.insert(id) {
                        let y = ty as f32 * TILE;
                        let h = TILE.min(max_y - y);
                        let (r0, r1) = geo.rows.indices_in(f64::from(y), f64::from(y + h));
                        let v = mix(
                            mix(
                                mix(self.full_epoch(), scale_bits),
                                u64::from(ty) | u64::from(hw.to_bits()) << 32,
                            ),
                            req.marks.row_signature(r0, r1),
                        );
                        slots.push(TileSlot {
                            id,
                            rect: Bounds::new(-hw, y, hw, h),
                            version: v,
                        });
                    }
                }
            }
        }
        self.scene.set_slots(slots);

        // Build what the send regions need.
        // Visible tiles are built now; prefetched ones (the margins around the panes) a few per
        // frame, nearest first, so a jump paints at once and the margins fill in over the next
        // frames (the surface asks for frames while they bring tiles).
        let sends: Vec<Bounds> = self.regions.iter().map(|r| r.send).collect();
        let visible: Vec<Bounds> = self.regions.iter().map(|r| r.visible).collect();
        let mut wanted: Vec<(bool, f32, TileSlot)> = self
            .scene
            .slots()
            .iter()
            .filter(|s| !self.scene.is_built(s.id) && sends.iter().any(|r| s.rect.intersects(r)))
            .map(|s| {
                let seen = visible.iter().any(|v| s.rect.intersects(v));
                let dist = visible
                    .iter()
                    .map(|v| v.distance_to(s.rect.x + s.rect.w / 2.0, s.rect.y + s.rect.h / 2.0))
                    .fold(f32::INFINITY, f32::min);
                (!seen, dist, *s)
            })
            .collect();
        wanted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut prefetch = PREFETCH_PER_FRAME;
        let wanted: Vec<TileSlot> = wanted
            .into_iter()
            .filter(|(off_screen, _, _)| {
                if *off_screen {
                    if prefetch == 0 {
                        return false;
                    }
                    prefetch -= 1;
                }
                true
            })
            .map(|(_, _, s)| s)
            .collect();
        self.pending_prefetch = self
            .scene
            .slots()
            .iter()
            .filter(|s| !self.scene.is_built(s.id) && sends.iter().any(|r| s.rect.intersects(r)))
            .count()
            .saturating_sub(wanted.len());
        if !wanted.is_empty() {
            // Conditional formats are evaluated once for every cell tile this frame builds.
            let cell_rects: Vec<Bounds> = wanted
                .iter()
                .filter(|s| is_cell_layer(s.id.layer))
                .map(|s| s.rect)
                .collect();
            let sid = wb.sheets().get(sheet_index).map(Sheet::id);
            let cf = match (sid, cell_rects.iter().copied().reduce(|a, b| a.union(&b))) {
                (Some(sid), Some(u)) if wb.sheets()[sheet_index].meta().cf.iter().any(|_| true) => {
                    let (r0, r1) = geo.rows.indices_in(f64::from(u.y), f64::from(u.bottom()));
                    let (c0, c1) = geo.cols.indices_in(f64::from(u.x), f64::from(u.right()));
                    wb.conditional_formats(sid, Range::new(r0, c0, r1, c1))
                }
                _ => FxHashMap::<CellPos, kiln_sheet::cf::CellVisual>::default(),
            };
            let wb: &Workbook = wb;
            let Some(sheet) = wb.sheets().get(sheet_index) else {
                return;
            };
            for slot in wanted {
                let ops = if is_cell_layer(slot.id.layer) {
                    let cx = PaintCx {
                        wb,
                        sheet,
                        geo,
                        cf: &cf,
                        opts: req.opts,
                    };
                    let mut painter = CellPainter {
                        text: &mut self.text,
                        glyphs: &mut self.glyphs,
                        looks: &mut self.looks,
                    };
                    painter.paint(&cx, slot.rect)
                } else if slot.id.layer == COL_HEADER_LAYER {
                    column_header_ops(geo, slot.rect, req.marks, &mut self.text, &mut self.glyphs)
                } else if slot.id.layer == ROW_HEADER_LAYER {
                    row_header_ops(geo, slot.rect, req.marks, &mut self.text, &mut self.glyphs)
                } else {
                    corner_ops(slot.rect.w, slot.rect.h, req.all_selected, snap)
                };
                self.tiles_built += 1;
                self.scene.insert_built(
                    slot.version,
                    Tile::new(slot.id, slot.rect, ops),
                    HitIndex::default(),
                );
            }
        }
        self.scene.overlay = req.overlay;
    }

    /// Glyph pixels the painter lacks.
    pub fn atlas_updates(&mut self, all: bool) -> Vec<AtlasUpdate> {
        self.glyphs.updates(all)
    }

    /// The glyph cache (the overlay's editor text uses it).
    pub fn glyphs_and_text(&mut self) -> (&mut GlyphCache, &mut TextEngine) {
        (&mut self.glyphs, &mut self.text)
    }
}

/// The tile coordinates of a cell tile's rectangle.
#[must_use]
pub fn tile_coords(rect: Bounds) -> (u32, u32) {
    (
        (rect.x / TILE).round() as u32,
        (rect.y / TILE).round() as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_ids_are_unique_across_the_sheet() {
        let mut seen = FxHashSet::default();
        for ty in [0u32, 1, 65_535, 65_536, 81_920, 2_000_000] {
            for tx in [0u32, 1, 4095, 65_535, 65_536, 120_000] {
                let id = cell_tile_id(tx, ty);
                assert!(is_cell_layer(id.layer), "{tx},{ty}");
                assert!(seen.insert(id), "{tx},{ty}");
            }
        }
        assert!(!is_cell_layer(COL_HEADER_LAYER) && !is_cell_layer(DRAWING_LAYER));
    }
}
