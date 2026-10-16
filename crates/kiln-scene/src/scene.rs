// SPDX-License-Identifier: AGPL-3.0-or-later
//! The retained scene: tile slots for the whole document, built tiles for the region a frame
//! needs, the overlay, and a hit-test index per tile.

use std::hash::Hasher;

use rustc_hash::{FxHashMap, FxHasher};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geom::Bounds;
use crate::op::DrawOp;
use crate::paint::Paint;

/// Z-order layers of tiles (lower paints first).
pub mod layer {
    /// Pages, sheets, slides.
    pub const CONTENT: u16 = 0;
    /// Floating content above the pages (comments' anchors, drawing canvases).
    pub const FLOATING: u16 = 1;
}

/// Identifies a tile. Painters composite tiles in `(layer, index)` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub struct TileId {
    /// Z-order layer (see [`layer`]).
    pub layer: u16,
    /// Index within the layer (Write: page × 64 + band).
    pub index: u32,
}

impl TileId {
    /// A tile of the content layer.
    #[must_use]
    pub const fn content(index: u32) -> Self {
        Self {
            layer: layer::CONTENT,
            index,
        }
    }
}

/// Keeps the value within the 53 bits a JavaScript number holds exactly.
pub const HASH_MASK: u64 = (1 << 53) - 1;

/// One tile on the wire: its place in the document and its ops in tile-local coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Tile {
    /// Identity.
    pub id: TileId,
    /// Position and size in document CSS pixels. Ops are relative to `(rect.x, rect.y)`.
    pub rect: Bounds,
    /// FxHash of `rect` and `ops` (53 bits), so painters can skip re-rendering identical content.
    #[ts(type = "number")]
    pub hash: u64,
    /// Drawing operations.
    pub ops: Vec<DrawOp>,
}

impl Tile {
    /// A tile with its hash computed.
    #[must_use]
    pub fn new(id: TileId, rect: Bounds, ops: Vec<DrawOp>) -> Self {
        let hash = hash_ops(&rect, &ops);
        Self {
            id,
            rect,
            hash,
            ops,
        }
    }
}

/// FxHash (53 bits) over the postcard encoding of a rectangle and its ops: exact (bit patterns of
/// floats included), deterministic across platforms, and streamed into the hasher without an
/// intermediate buffer.
#[must_use]
pub fn hash_ops(rect: &Bounds, ops: &[DrawOp]) -> u64 {
    struct HashFlavor(FxHasher);
    impl postcard::ser_flavors::Flavor for HashFlavor {
        type Output = u64;
        fn try_push(&mut self, data: u8) -> postcard::Result<()> {
            self.0.write_u8(data);
            Ok(())
        }
        fn try_extend(&mut self, data: &[u8]) -> postcard::Result<()> {
            self.0.write(data);
            Ok(())
        }
        fn finalize(self) -> postcard::Result<u64> {
            Ok(self.0.finish())
        }
    }
    // Plain data always serialises; the fallback only guards the type system's error path.
    postcard::serialize_with_flavor(&(rect, ops), HashFlavor(FxHasher::default())).unwrap_or(0)
        & HASH_MASK
}

/// One entry of a tile's hit-test index: a rectangle (document pixels) and the producer's key
/// (Write: the paragraph and line it covers).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitEntry {
    /// Area.
    pub rect: Bounds,
    /// Producer-defined key.
    pub key: u64,
}

/// The hit-test index of one tile, sorted by top edge.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HitIndex {
    entries: Vec<HitEntry>,
}

impl HitIndex {
    /// An index from entries in any order.
    #[must_use]
    pub fn new(mut entries: Vec<HitEntry>) -> Self {
        entries.retain(|e| e.rect.is_finite());
        entries.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y));
        Self { entries }
    }

    /// The entries, by top edge.
    #[must_use]
    pub fn entries(&self) -> &[HitEntry] {
        &self.entries
    }

    /// Whether the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry under the point, or else the nearest one: entries whose vertical span contains
    /// `y` win (nearest horizontally), then the nearest by distance. Like Word, a click in a margin
    /// or between lines lands on the closest line.
    #[must_use]
    pub fn nearest(&self, x: f32, y: f32) -> Option<HitEntry> {
        if !(x.is_finite() && y.is_finite()) {
            return None;
        }
        // Entries are sorted by top; those starting below y cannot contain it.
        let end = self.entries.partition_point(|e| e.rect.y <= y);
        let containing = self.entries[..end]
            .iter()
            .filter(|e| y < e.rect.bottom())
            .min_by(|a, b| {
                a.rect
                    .distance_to(x, y)
                    .total_cmp(&b.rect.distance_to(x, y))
            });
        if let Some(e) = containing {
            return Some(*e);
        }
        self.entries
            .iter()
            .min_by(|a, b| {
                let da = (a.rect.y - y).max(y - a.rect.bottom()).max(0.0);
                let db = (b.rect.y - y).max(y - b.rect.bottom()).max(0.0);
                da.total_cmp(&db).then(
                    a.rect
                        .distance_to(x, y)
                        .total_cmp(&b.rect.distance_to(x, y)),
                )
            })
            .copied()
    }
}

/// A tile of the document: always known (cheap), built only when a frame needs its ops.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileSlot {
    /// Identity.
    pub id: TileId,
    /// Position and size in document pixels.
    pub rect: Bounds,
    /// A cheap signature of the content (the producer's own key, e.g. the page's lines). A
    /// painter that holds a tile with another version holds stale content.
    pub version: u64,
}

/// A built tile with its hit-test index.
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltTile {
    /// The tile.
    pub tile: Tile,
    /// The version of the slot it was built for.
    pub version: u64,
    /// Hit testing.
    pub hits: HitIndex,
}

/// What a hit test found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// The tile.
    pub tile: TileId,
    /// The entry.
    pub entry: HitEntry,
}

/// A document's scene for one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    /// Device pixels per document pixel (zoom × DPR) the glyphs were rasterised for.
    pub scale: f32,
    /// Size of the whole document surface in document pixels.
    pub extent: Bounds,
    /// What is painted behind the tiles.
    pub background: Paint,
    slots: Vec<TileSlot>,
    built: FxHashMap<TileId, BuiltTile>,
    /// Overlay ops in document pixels (caret, selection, composition, handles).
    pub overlay: Vec<DrawOp>,
}

impl Scene {
    /// An empty scene.
    #[must_use]
    pub fn new(scale: f32, extent: Bounds, background: Paint) -> Self {
        Self {
            scale,
            extent,
            background,
            slots: Vec::new(),
            built: FxHashMap::default(),
            overlay: Vec::new(),
        }
    }

    /// Declares the tiles of the document (any order; kept sorted by id).
    pub fn set_slots(&mut self, mut slots: Vec<TileSlot>) {
        slots.sort_by_key(|s| s.id);
        slots.dedup_by_key(|s| s.id);
        self.built.retain(|id, b| {
            slots
                .binary_search_by_key(id, |s| s.id)
                .is_ok_and(|i| slots[i].version == b.version)
        });
        self.slots = slots;
    }

    /// Every tile slot, in paint order.
    #[must_use]
    pub fn slots(&self) -> &[TileSlot] {
        &self.slots
    }

    /// The slot of a tile.
    #[must_use]
    pub fn slot(&self, id: TileId) -> Option<&TileSlot> {
        self.slots
            .binary_search_by_key(&id, |s| s.id)
            .ok()
            .map(|i| &self.slots[i])
    }

    /// Slots whose rectangle intersects `region` (culling), in paint order.
    pub fn slots_in<'a>(&'a self, region: &'a Bounds) -> impl Iterator<Item = &'a TileSlot> + 'a {
        self.slots.iter().filter(move |s| s.rect.intersects(region))
    }

    /// Whether a slot's current version is built.
    #[must_use]
    pub fn is_built(&self, id: TileId) -> bool {
        match (self.slot(id), self.built.get(&id)) {
            (Some(s), Some(b)) => s.version == b.version,
            _ => false,
        }
    }

    /// Stores a built tile for its slot (ignored when no slot has that id).
    pub fn insert_built(&mut self, version: u64, tile: Tile, hits: HitIndex) {
        if self.slot(tile.id).is_some() {
            self.built.insert(
                tile.id,
                BuiltTile {
                    tile,
                    version,
                    hits,
                },
            );
        }
    }

    /// A built tile.
    #[must_use]
    pub fn built(&self, id: TileId) -> Option<&BuiltTile> {
        self.built.get(&id).filter(|_| self.is_built(id))
    }

    /// Drops built tiles outside `keep` to bound memory.
    pub fn evict_outside(&mut self, keep: &Bounds) {
        self.built.retain(|_, b| b.tile.rect.intersects(keep));
    }

    /// Number of built tiles.
    #[must_use]
    pub fn built_count(&self) -> usize {
        self.built.len()
    }

    /// Hit test among built tiles: the tile under the point (or the nearest one), then the nearest
    /// entry of its index.
    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32) -> Option<Hit> {
        if !(x.is_finite() && y.is_finite()) {
            return None;
        }
        let mut tiles: Vec<&BuiltTile> = self
            .built
            .values()
            .filter(|b| !b.hits.is_empty() && self.is_built(b.tile.id))
            .collect();
        tiles.sort_by(|a, b| {
            a.tile
                .rect
                .distance_to(x, y)
                .total_cmp(&b.tile.rect.distance_to(x, y))
                .then(a.tile.id.cmp(&b.tile.id))
        });
        let best = tiles.first()?;
        best.hits.nearest(x, y).map(|entry| Hit {
            tile: best.tile.id,
            entry,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ColorRole;

    fn page(i: u32) -> TileSlot {
        #[allow(clippy::cast_precision_loss)]
        TileSlot {
            id: TileId::content(i),
            rect: Bounds::new(0.0, i as f32 * 1100.0, 816.0, 1056.0),
            version: u64::from(i),
        }
    }

    fn ops(color: u32) -> Vec<DrawOp> {
        vec![DrawOp::FillRect {
            rect: Bounds::new(0.0, 0.0, 816.0, 1056.0),
            paint: Paint::Rgba(color),
            radius: 0.0,
        }]
    }

    #[test]
    fn hashes_are_stable_and_content_sensitive() {
        let r = Bounds::new(0.0, 0.0, 816.0, 1056.0);
        let a = hash_ops(&r, &ops(1));
        assert_eq!(a, hash_ops(&r, &ops(1)));
        assert_ne!(a, hash_ops(&r, &ops(2)));
        assert_ne!(a, hash_ops(&r.translate(0.0, 1.0), &ops(1)));
        assert!(a <= HASH_MASK);
        // -0.0 and 0.0 hash differently: bit patterns, not values (painters see the bits).
        let z = |v: f32| {
            vec![DrawOp::Caret {
                rect: Bounds::new(v, 0.0, 1.0, 1.0),
                paint: Paint::Role(ColorRole::Caret),
                spark: false,
            }]
        };
        assert_ne!(hash_ops(&r, &z(0.0)), hash_ops(&r, &z(-0.0)));
    }

    #[test]
    fn slots_cull_and_track_built_versions() {
        let mut s = Scene::new(
            1.0,
            Bounds::new(0.0, 0.0, 816.0, 5500.0),
            Paint::Role(ColorRole::Canvas),
        );
        s.set_slots((0..5).rev().map(page).collect());
        assert_eq!(s.slots()[0].id, TileId::content(0), "sorted");
        let viewport = Bounds::new(0.0, 1000.0, 800.0, 600.0);
        let visible: Vec<u32> = s.slots_in(&viewport).map(|t| t.id.index).collect();
        assert_eq!(visible, vec![0, 1]);
        s.insert_built(
            1,
            Tile::new(TileId::content(1), page(1).rect, ops(3)),
            HitIndex::default(),
        );
        assert!(s.is_built(TileId::content(1)));
        // A new version of the slot invalidates the built tile.
        let mut changed = page(1);
        changed.version = 99;
        s.set_slots(vec![page(0), changed, page(2)]);
        assert!(!s.is_built(TileId::content(1)));
        assert!(s.built(TileId::content(1)).is_none());
        // Unknown ids are ignored.
        s.insert_built(
            0,
            Tile::new(TileId::content(42), page(1).rect, ops(3)),
            HitIndex::default(),
        );
        assert_eq!(s.built_count(), 0);
        s.insert_built(
            0,
            Tile::new(TileId::content(0), page(0).rect, ops(3)),
            HitIndex::default(),
        );
        s.evict_outside(&Bounds::new(0.0, 5000.0, 10.0, 10.0));
        assert_eq!(s.built_count(), 0);
    }

    #[test]
    fn hit_index_finds_the_line_under_or_nearest_the_point() {
        let line = |y: f32, key: u64| HitEntry {
            rect: Bounds::new(96.0, y, 600.0, 20.0),
            key,
        };
        let idx = HitIndex::new(vec![line(140.0, 2), line(100.0, 0), line(120.0, 1)]);
        assert_eq!(idx.entries()[0].key, 0);
        assert_eq!(idx.nearest(200.0, 125.0).map(|e| e.key), Some(1));
        assert_eq!(
            idx.nearest(10.0, 125.0).map(|e| e.key),
            Some(1),
            "left margin"
        );
        assert_eq!(idx.nearest(200.0, 20.0).map(|e| e.key), Some(0), "above");
        assert_eq!(idx.nearest(200.0, 900.0).map(|e| e.key), Some(2), "below");
        assert_eq!(idx.nearest(f32::NAN, 0.0), None);
        assert_eq!(HitIndex::default().nearest(0.0, 0.0), None);
    }

    #[test]
    fn scene_hit_test_prefers_the_tile_under_the_point() {
        let mut s = Scene::new(
            1.0,
            Bounds::new(0.0, 0.0, 816.0, 2200.0),
            Paint::Role(ColorRole::Canvas),
        );
        s.set_slots(vec![page(0), page(1)]);
        for i in 0..2u32 {
            #[allow(clippy::cast_precision_loss)]
            let y = i as f32 * 1100.0 + 96.0;
            s.insert_built(
                u64::from(i),
                Tile::new(TileId::content(i), page(i).rect, ops(0)),
                HitIndex::new(vec![HitEntry {
                    rect: Bounds::new(96.0, y, 600.0, 20.0),
                    key: u64::from(i),
                }]),
            );
        }
        let hit = s.hit_test(300.0, 1500.0).expect("hit");
        assert_eq!((hit.tile.index, hit.entry.key), (1, 1));
        // Between the pages: the nearest page wins.
        assert_eq!(s.hit_test(300.0, 1060.0).map(|h| h.entry.key), Some(0));
        assert_eq!(s.hit_test(300.0, 1095.0).map(|h| h.entry.key), Some(1));
    }
}
