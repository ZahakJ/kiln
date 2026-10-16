// SPDX-License-Identifier: AGPL-3.0-or-later
//! `SceneDelta`: what a painter must change since the previous frame, and the tracker that keeps
//! the core's model of the painter's cache.

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geom::Bounds;
use crate::op::{AtlasPageId, DrawOp, ImageId};
use crate::paint::Paint;
use crate::scene::{HASH_MASK, Scene, Tile, TileId, hash_ops};

/// Pixel format of an atlas page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum AtlasFormat {
    /// One byte of coverage per pixel (text; tinted by the glyph run's paint).
    A8,
    /// Straight-alpha RGBA, four bytes per pixel (colour emoji).
    Rgba8,
}

impl AtlasFormat {
    /// Bytes per pixel.
    #[must_use]
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            AtlasFormat::A8 => 1,
            AtlasFormat::Rgba8 => 4,
        }
    }
}

/// New pixels for rows `y .. y + rows` of an atlas page (the page is created on first update).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AtlasUpdate {
    /// The page.
    pub page: AtlasPageId,
    /// Pixel format.
    pub format: AtlasFormat,
    /// Page width in pixels.
    pub width: u32,
    /// Page height in pixels.
    pub height: u32,
    /// First row of `data`.
    pub y: u32,
    /// Number of rows in `data`.
    pub rows: u32,
    /// `rows × width × bytes_per_pixel` bytes.
    #[serde(with = "crate::bytes")]
    #[ts(type = "Array<number>")]
    pub data: Vec<u8>,
}

impl AtlasUpdate {
    /// Whether the sizes agree with the data and stay inside the page.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let row = self.width as usize * self.format.bytes_per_pixel();
        self.width > 0
            && self.height > 0
            && self
                .y
                .checked_add(self.rows)
                .is_some_and(|end| end <= self.height)
            && self.data.len() == row * self.rows as usize
    }
}

/// An image the painter caches by id (decoded by the core; straight-alpha RGBA).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ImageUpload {
    /// Identity.
    pub id: ImageId,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width × height × 4` bytes.
    #[serde(with = "crate::bytes")]
    #[ts(type = "Array<number>")]
    pub data: Vec<u8>,
}

impl ImageUpload {
    /// Whether the data has the declared size.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && (self.width as usize)
                .checked_mul(self.height as usize)
                .and_then(|n| n.checked_mul(4))
                .is_some_and(|n| n == self.data.len())
    }
}

/// The overlay layer (document pixels): caret, selection, composition, handles, squiggles.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct Overlay {
    /// 53-bit hash of the ops.
    #[ts(type = "number")]
    pub hash: u64,
    /// Ops.
    pub ops: Vec<DrawOp>,
}

/// The changes since the previous frame of a document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SceneDelta {
    /// The document revision this frame shows.
    #[ts(type = "number")]
    pub revision: u64,
    /// Drop every cached tile, atlas page and image before applying this delta.
    pub reset: bool,
    /// Device pixels per document pixel (zoom × DPR) the glyphs were rasterised for.
    pub scale: f32,
    /// The whole document surface in document pixels (scroll range).
    pub extent: Bounds,
    /// Painted behind everything.
    pub background: Paint,
    /// New or changed tiles.
    pub tiles: Vec<Tile>,
    /// Tiles to drop (deleted, stale, or far from the viewport).
    pub removed: Vec<TileId>,
    /// Atlas pixels, applied before the tiles are painted.
    pub atlas: Vec<AtlasUpdate>,
    /// New images.
    pub images: Vec<ImageUpload>,
    /// The overlay, when it changed.
    pub overlay: Option<Overlay>,
}

impl Default for SceneDelta {
    fn default() -> Self {
        Self {
            revision: 0,
            reset: false,
            scale: 1.0,
            extent: Bounds::ZERO,
            background: Paint::Rgba(0),
            tiles: Vec::new(),
            removed: Vec::new(),
            atlas: Vec::new(),
            images: Vec::new(),
            overlay: None,
        }
    }
}

impl SceneDelta {
    /// Whether applying the delta changes nothing (apart from the revision and extent).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.reset
            && self.tiles.is_empty()
            && self.removed.is_empty()
            && self.atlas.is_empty()
            && self.images.is_empty()
            && self.overlay.is_none()
    }

    /// The postcard encoding (what `frame()` returns across the bridges).
    ///
    /// # Errors
    ///
    /// Only if serialisation fails, which plain data never does.
    pub fn to_postcard(&self) -> Result<Vec<u8>, postcard::Error> {
        postcard::to_allocvec(self)
    }

    /// Decodes [`SceneDelta::to_postcard`] output.
    ///
    /// # Errors
    ///
    /// For truncated or malformed bytes.
    pub fn from_postcard(bytes: &[u8]) -> Result<Self, postcard::Error> {
        postcard::from_bytes(bytes)
    }
}

/// What the painter of one document holds, as far as the core knows: which version of each tile,
/// which overlay, at which scale. `delta` turns a [`Scene`] into the smallest [`SceneDelta`].
#[derive(Debug, Clone, Default)]
pub struct DeltaTracker {
    sent: FxHashMap<TileId, (u64, u64)>,
    overlay: Option<u64>,
    scale: Option<u32>,
    needs_reset: bool,
}

impl DeltaTracker {
    /// A tracker for a painter that holds nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The painter lost its cache (remounted, context lost): the next delta resets and resends.
    pub fn reset(&mut self) {
        self.needs_reset = true;
    }

    /// Whether the next delta will reset the painter.
    #[must_use]
    pub fn will_reset(&self, scale: f32) -> bool {
        self.needs_reset || self.scale != Some(scale.to_bits())
    }

    /// Number of tiles the painter holds.
    #[must_use]
    pub fn cached_tiles(&self) -> usize {
        self.sent.len()
    }

    /// The delta that brings the painter to `scene`: built tiles intersecting `send` that it lacks
    /// (or holds in another version), removals for tiles it holds that no longer exist, are stale,
    /// or lie outside `keep`, and the overlay when it changed. Atlas and image uploads are the
    /// producer's to add.
    pub fn delta(
        &mut self,
        scene: &Scene,
        send: &Bounds,
        keep: &Bounds,
        revision: u64,
    ) -> SceneDelta {
        let reset = self.will_reset(scene.scale);
        if reset {
            self.sent.clear();
            self.overlay = None;
            self.needs_reset = false;
            self.scale = Some(scene.scale.to_bits());
        }
        let mut tiles = Vec::new();
        for slot in scene.slots_in(send) {
            let Some(built) = scene.built(slot.id) else {
                continue;
            };
            match self.sent.get(&slot.id) {
                Some((v, _)) if *v == slot.version => {}
                Some((_, h)) if *h == built.tile.hash => {
                    // Same pixels under a new version: only the bookkeeping changes.
                    self.sent.insert(slot.id, (slot.version, *h));
                }
                _ => {
                    self.sent.insert(slot.id, (slot.version, built.tile.hash));
                    tiles.push(built.tile.clone());
                }
            }
        }
        let mut removed: Vec<TileId> = self
            .sent
            .iter()
            .filter(|(id, (version, _))| {
                scene
                    .slot(**id)
                    .is_none_or(|s| s.version != *version || !s.rect.intersects(keep))
            })
            .map(|(id, _)| *id)
            .collect();
        removed.sort_unstable();
        for id in &removed {
            self.sent.remove(id);
        }
        let overlay_hash = hash_ops(&scene.extent, &scene.overlay) & HASH_MASK;
        let overlay = (self.overlay != Some(overlay_hash)).then(|| {
            self.overlay = Some(overlay_hash);
            Overlay {
                hash: overlay_hash,
                ops: scene.overlay.clone(),
            }
        });
        SceneDelta {
            revision,
            reset,
            scale: scene.scale,
            extent: scene.extent,
            background: scene.background,
            tiles,
            removed,
            atlas: Vec::new(),
            images: Vec::new(),
            overlay,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::ColorRole;
    use crate::scene::{HitIndex, TileSlot};

    fn slot(i: u32, version: u64) -> TileSlot {
        #[allow(clippy::cast_precision_loss)]
        TileSlot {
            id: TileId::content(i),
            rect: Bounds::new(0.0, i as f32 * 1000.0, 800.0, 1000.0),
            version,
        }
    }

    fn scene(versions: &[u64]) -> Scene {
        let mut s = Scene::new(
            1.0,
            Bounds::new(0.0, 0.0, 800.0, 10_000.0),
            Paint::Role(ColorRole::Canvas),
        );
        let slots: Vec<TileSlot> = versions
            .iter()
            .enumerate()
            .map(|(i, v)| slot(u32::try_from(i).unwrap(), *v))
            .collect();
        s.set_slots(slots.clone());
        for sl in slots {
            let ops = vec![DrawOp::FillRect {
                rect: Bounds::new(0.0, 0.0, 10.0, 10.0),
                paint: Paint::Rgba(u32::try_from(sl.version).unwrap()),
                radius: 0.0,
            }];
            s.insert_built(
                sl.version,
                Tile::new(sl.id, sl.rect, ops),
                HitIndex::default(),
            );
        }
        s
    }

    fn ids(tiles: &[Tile]) -> Vec<u32> {
        tiles.iter().map(|t| t.id.index).collect()
    }

    #[test]
    fn sends_visible_tiles_once_then_only_changes() {
        let mut t = DeltaTracker::new();
        let s = scene(&[1, 2, 3, 4, 5]);
        let send = Bounds::new(0.0, 500.0, 800.0, 1000.0); // tiles 0 and 1
        let keep = Bounds::new(0.0, 0.0, 800.0, 3000.0);
        let d = t.delta(&s, &send, &keep, 1);
        assert!(d.reset);
        assert_eq!(ids(&d.tiles), vec![0, 1]);
        assert!(d.overlay.is_some());
        let d = t.delta(&s, &send, &keep, 1);
        assert!(d.is_empty(), "nothing changed: {d:?}");
        // Tile 1 changes; tile 0 does not.
        let s2 = scene(&[1, 20, 3, 4, 5]);
        let d = t.delta(&s2, &send, &keep, 2);
        assert!(!d.reset);
        assert_eq!(ids(&d.tiles), vec![1]);
        assert!(d.removed.is_empty());
        assert!(d.overlay.is_none());
    }

    #[test]
    fn removes_deleted_stale_and_distant_tiles() {
        let mut t = DeltaTracker::new();
        let all = Bounds::new(0.0, 0.0, 800.0, 10_000.0);
        let d = t.delta(&scene(&[1, 2, 3, 4]), &all, &all, 1);
        assert_eq!(ids(&d.tiles), vec![0, 1, 2, 3]);
        assert_eq!(t.cached_tiles(), 4);
        // Tile 3 is gone; tile 2 changed but is not in the send region (stale → dropped); keep
        // covers tiles 0–2 only.
        let send = Bounds::new(0.0, 0.0, 800.0, 1500.0);
        let keep = Bounds::new(0.0, 0.0, 800.0, 2500.0);
        let d = t.delta(&scene(&[1, 2, 30]), &send, &keep, 2);
        assert_eq!(d.removed, vec![TileId::content(2), TileId::content(3)]);
        assert!(d.tiles.is_empty());
        // Far away: tile 0 leaves the keep region.
        let send = Bounds::new(0.0, 1200.0, 800.0, 100.0);
        let keep = Bounds::new(0.0, 1100.0, 800.0, 400.0);
        let d = t.delta(&scene(&[1, 2, 30]), &send, &keep, 2);
        assert_eq!(d.removed, vec![TileId::content(0)]);
        assert_eq!(t.cached_tiles(), 1);
    }

    #[test]
    fn a_new_version_with_identical_pixels_is_not_resent() {
        let mut t = DeltaTracker::new();
        let all = Bounds::new(0.0, 0.0, 800.0, 10_000.0);
        let mut s = scene(&[7]);
        let _ = t.delta(&s, &all, &all, 1);
        let tile = s.built(TileId::content(0)).unwrap().tile.clone();
        let mut sl = slot(0, 8);
        sl.rect = tile.rect;
        s.set_slots(vec![sl]);
        s.insert_built(8, tile, HitIndex::default());
        let d = t.delta(&s, &all, &all, 2);
        assert!(d.is_empty());
    }

    #[test]
    fn scale_changes_and_explicit_resets_resend_everything() {
        let mut t = DeltaTracker::new();
        let all = Bounds::new(0.0, 0.0, 800.0, 10_000.0);
        let mut s = scene(&[1, 2]);
        let _ = t.delta(&s, &all, &all, 1);
        assert!(!t.will_reset(1.0));
        s.scale = 2.0;
        assert!(t.will_reset(2.0));
        let d = t.delta(&s, &all, &all, 1);
        assert!(d.reset);
        assert_eq!(ids(&d.tiles), vec![0, 1]);
        t.reset();
        let d = t.delta(&s, &all, &all, 1);
        assert!(d.reset && d.overlay.is_some());
        assert_eq!(d.tiles.len(), 2);
    }

    #[test]
    fn postcard_round_trip_and_validation() {
        let mut t = DeltaTracker::new();
        let all = Bounds::new(0.0, 0.0, 800.0, 10_000.0);
        let mut d = t.delta(&scene(&[1, 2]), &all, &all, 9);
        d.atlas.push(AtlasUpdate {
            page: AtlasPageId(0),
            format: AtlasFormat::A8,
            width: 4,
            height: 4,
            y: 1,
            rows: 2,
            data: vec![7; 8],
        });
        d.images.push(ImageUpload {
            id: ImageId(3),
            width: 1,
            height: 2,
            data: vec![255; 8],
        });
        assert!(d.atlas[0].is_valid() && d.images[0].is_valid());
        let bytes = d.to_postcard().unwrap();
        assert_eq!(SceneDelta::from_postcard(&bytes).unwrap(), d);
        assert!(SceneDelta::from_postcard(&bytes[..bytes.len() - 1]).is_err());
        // Pixel data is one length plus raw bytes, not one varint per byte.
        let json = serde_json::to_string(&d.atlas[0]).unwrap();
        assert!(json.contains("[7,7,7,7,7,7,7,7]"));
        let back: AtlasUpdate = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d.atlas[0]);
        let mut bad = d.atlas[0].clone();
        bad.rows = 4;
        assert!(!bad.is_valid());
        bad.rows = u32::MAX;
        assert!(!bad.is_valid());
        let mut img = d.images[0].clone();
        img.width = u32::MAX;
        assert!(!img.is_valid());
    }
}
