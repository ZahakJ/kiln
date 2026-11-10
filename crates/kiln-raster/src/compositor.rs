// SPDX-License-Identifier: AGPL-3.0-or-later
//! The raster twin of `packages/surface`'s tile cache: applies [`SceneDelta`]s and composites the
//! cached tiles and the overlay for a viewport.

use std::collections::BTreeMap;

use kiln_scene::{
    AtlasUpdate, Bounds, DrawOp, ImageUpload, Paint, Palette, SceneDelta, Tile, TileId, channels,
};
use tiny_skia::{IntSize, Pixmap, PixmapPaint, Transform};

use crate::painter::{AtlasPixels, OpPainter, PaintOptions, Resources};

/// Why a delta could not be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RasterError {
    /// An atlas update's sizes disagree with its data or with the page.
    BadAtlas(u32),
    /// An image's sizes disagree with its data.
    BadImage(u32),
    /// The scale is not a positive finite number.
    BadScale,
    /// The requested output is empty or too large.
    BadSize {
        /// Width.
        width: u32,
        /// Height.
        height: u32,
    },
}

impl std::fmt::Display for RasterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadAtlas(p) => write!(f, "atlas update for page {p} is malformed"),
            Self::BadImage(i) => write!(f, "image {i} is malformed"),
            Self::BadScale => f.write_str("the frame's scale is not a positive number"),
            Self::BadSize { width, height } => {
                write!(f, "cannot render a {width}×{height} bitmap")
            }
        }
    }
}

impl std::error::Error for RasterError {}

/// Largest output side in device pixels.
pub const MAX_SIDE: u32 = 16_384;

/// The visible part of the surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// Output width in device pixels.
    pub width: u32,
    /// Output height in device pixels.
    pub height: u32,
    /// Horizontal scroll in document pixels.
    pub scroll_x: f32,
    /// Vertical scroll in document pixels.
    pub scroll_y: f32,
    /// Blink phase and touch handles.
    pub options: PaintOptions,
}

/// Device position of a document coordinate at `scale` (shared rounding with the canvas painter).
#[must_use]
pub fn device(v: f32, scale: f32) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let d = (v * scale).round().clamp(-1.0e9, 1.0e9) as i32;
    d
}

/// Device size of a tile's bitmap.
#[must_use]
pub fn tile_device_size(rect: Bounds, scale: f32) -> (u32, u32) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let f = |v: f32| (v * scale).ceil().clamp(1.0, MAX_SIDE as f32) as u32;
    (f(rect.w), f(rect.h))
}

#[derive(Debug)]
struct CachedTile {
    tile: Tile,
    bitmap: Option<(String, Pixmap)>,
}

/// Mirrors a painter's cache and renders it.
#[derive(Debug)]
pub struct Compositor {
    tiles: BTreeMap<TileId, CachedTile>,
    resources: Resources,
    overlay: Vec<DrawOp>,
    scale: f32,
    extent: Bounds,
    background: Paint,
    revision: u64,
}

impl Default for Compositor {
    fn default() -> Self {
        Self::new()
    }
}

impl Compositor {
    /// An empty compositor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tiles: BTreeMap::new(),
            resources: Resources::default(),
            overlay: Vec::new(),
            scale: 1.0,
            extent: Bounds::ZERO,
            background: Paint::Rgba(0),
            revision: 0,
        }
    }

    /// The scene scale of the last delta.
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The document extent of the last delta.
    #[must_use]
    pub fn extent(&self) -> Bounds {
        self.extent
    }

    /// The revision of the last delta.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Cached tile ids in paint order.
    #[must_use]
    pub fn tile_ids(&self) -> Vec<TileId> {
        self.tiles.keys().copied().collect()
    }

    /// The cached overlay.
    #[must_use]
    pub fn overlay(&self) -> &[DrawOp] {
        &self.overlay
    }

    /// Applies a delta. On error nothing is changed.
    ///
    /// # Errors
    ///
    /// Malformed atlas updates or images, or a non-positive scale.
    pub fn apply(&mut self, delta: &SceneDelta) -> Result<(), RasterError> {
        if !(delta.scale.is_finite() && delta.scale > 0.0) {
            return Err(RasterError::BadScale);
        }
        for u in &delta.atlas {
            if !u.is_valid() {
                return Err(RasterError::BadAtlas(u.page.0));
            }
            if !delta.reset
                && let Some(p) = self.resources.atlas.get(&u.page)
                && (p.width != u.width || p.height != u.height || p.format != u.format)
            {
                return Err(RasterError::BadAtlas(u.page.0));
            }
        }
        let mut images = Vec::with_capacity(delta.images.len());
        for img in &delta.images {
            images.push((
                img.id,
                image_pixmap(img).ok_or(RasterError::BadImage(img.id.0))?,
            ));
        }
        if delta.reset {
            self.tiles.clear();
            self.resources.atlas.clear();
            self.resources.images.clear();
            self.overlay.clear();
        }
        self.scale = delta.scale;
        self.extent = delta.extent;
        self.background = delta.background;
        self.revision = delta.revision;
        for u in &delta.atlas {
            apply_atlas(&mut self.resources, u);
        }
        self.resources.images.extend(images);
        for id in &delta.removed {
            self.tiles.remove(id);
        }
        for t in &delta.tiles {
            self.tiles.insert(
                t.id,
                CachedTile {
                    tile: t.clone(),
                    bitmap: None,
                },
            );
        }
        if let Some(o) = &delta.overlay {
            self.overlay.clone_from(&o.ops);
        }
        // Atlas updates never change pixels a rendered tile uses (pages only grow, and a tile
        // arrives with the rows its glyphs need), so cached tile bitmaps stay valid.
        Ok(())
    }

    /// Composites the cached tiles and the overlay for `view`.
    ///
    /// # Errors
    ///
    /// For an empty or oversized output.
    pub fn render(&mut self, view: &View, palette: &Palette) -> Result<Pixmap, RasterError> {
        let bad = RasterError::BadSize {
            width: view.width,
            height: view.height,
        };
        if view.width == 0 || view.height == 0 || view.width > MAX_SIDE || view.height > MAX_SIDE {
            return Err(bad);
        }
        let mut out = Pixmap::new(view.width, view.height).ok_or_else(|| bad.clone())?;
        let [r, g, b, a] = channels(palette.resolve(self.background));
        out.fill(tiny_skia::Color::from_rgba8(r, g, b, a));
        let s = self.scale;
        let origin = (device(view.scroll_x, s), device(view.scroll_y, s));
        #[allow(clippy::cast_precision_loss)]
        let visible = Bounds::new(
            view.scroll_x,
            view.scroll_y,
            view.width as f32 / s,
            view.height as f32 / s,
        );
        let key = palette.name.clone();
        let ids: Vec<TileId> = self.tiles.keys().copied().collect();
        for id in ids {
            let Some(cached) = self.tiles.get(&id) else {
                continue;
            };
            if !cached.tile.rect.intersects(&visible) {
                continue;
            }
            let stale = cached.bitmap.as_ref().is_none_or(|(k, _)| *k != key);
            if stale {
                let bitmap = self.render_tile(&cached.tile, palette, view.options);
                if let Some(c) = self.tiles.get_mut(&id) {
                    c.bitmap = bitmap.map(|b| (key.clone(), b));
                }
            }
            let Some(cached) = self.tiles.get(&id) else {
                continue;
            };
            if let Some((_, bmp)) = &cached.bitmap {
                let x = device(cached.tile.rect.x, s) - origin.0;
                let y = device(cached.tile.rect.y, s) - origin.1;
                out.draw_pixmap(
                    x,
                    y,
                    bmp.as_ref(),
                    &PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            }
        }
        // Overlay ops paint straight onto the content in order; the selection wash (first)
        // blends per the palette (darken/lighten) so it reads as painted under the text. The
        // canvas painter draws the wash onto its content canvas the same way and the remaining
        // marks on its overlay canvas.
        #[allow(clippy::cast_precision_loss)]
        let offset = (-origin.0 as f32, -origin.1 as f32);
        let mut p = OpPainter::new(
            &mut out,
            palette,
            &self.resources,
            view.options,
            s,
            offset,
            (-origin.0, -origin.1),
        );
        p.paint_all(&self.overlay);
        Ok(out)
    }

    fn render_tile(&self, tile: &Tile, palette: &Palette, options: PaintOptions) -> Option<Pixmap> {
        let (w, h) = tile_device_size(tile.rect, self.scale);
        let mut bmp = Pixmap::new(w, h)?;
        let mut p = OpPainter::new(
            &mut bmp,
            palette,
            &self.resources,
            options,
            self.scale,
            (0.0, 0.0),
            (0, 0),
        );
        p.paint_all(&tile.ops);
        Some(bmp)
    }
}

fn apply_atlas(res: &mut Resources, u: &AtlasUpdate) {
    let bpp = u.format.bytes_per_pixel();
    let page = res.atlas.entry(u.page).or_insert_with(|| AtlasPixels {
        format: u.format,
        width: u.width,
        height: u.height,
        data: vec![0; u.width as usize * u.height as usize * bpp],
    });
    let row = u.width as usize * bpp;
    let start = u.y as usize * row;
    if let Some(dst) = page.data.get_mut(start..start + u.data.len()) {
        dst.copy_from_slice(&u.data);
    }
}

fn image_pixmap(img: &ImageUpload) -> Option<Pixmap> {
    if !img.is_valid() || img.width > MAX_SIDE || img.height > MAX_SIDE {
        return None;
    }
    let mut data = img.data.clone();
    for px in data.as_chunks_mut::<4>().0 {
        let a = u16::from(px[3]);
        for c in &mut px[..3] {
            *c = u8::try_from((u16::from(*c) * a + 127) / 255).unwrap_or(255);
        }
    }
    Pixmap::from_vec(data, IntSize::from_wh(img.width, img.height)?)
}
