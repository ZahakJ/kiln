// SPDX-License-Identifier: AGPL-3.0-or-later
//! The glyph atlas: glyphs rasterised by swash at (font, pixel size, quarter-pixel x offset) and
//! packed into 1024 × 1024 pages (A8 coverage for text, RGBA8 for colour glyphs). The Scene refers to
//! glyphs by page id and rectangle; the painter uploads each page once and blits from it. Glyphs at
//! or above [`OUTLINE_THRESHOLD`] px are exported as outline paths instead.

use std::collections::{BTreeSet, HashMap};

use kurbo::{BezPath, Point};
use skrifa::MetadataProvider;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Angle, Format, Transform, Vector};

use crate::fonts::{FontId, FontStore, skrifa_coords};
use crate::output::GlyphRun;

/// Width and height of an atlas page in pixels.
pub const ATLAS_PAGE_SIZE: u32 = 1024;

/// Glyphs rendered at this device size (px) or larger are drawn as outline paths, not bitmaps.
pub const OUTLINE_THRESHOLD: f32 = 96.0;

/// Number of horizontal sub-pixel positions per pixel.
pub const SUBPIXEL_STEPS: u8 = 4;

/// Gap between packed glyphs, so bilinear sampling never bleeds between neighbours.
const PADDING: u32 = 1;

/// Pixel format of an atlas page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtlasFormat {
    /// 8-bit coverage (text, tinted by the painter).
    A8,
    /// 8-bit-per-channel premultiplied RGBA (colour emoji, COLR/CBDT glyphs).
    Rgba8,
}

impl AtlasFormat {
    /// Bytes per pixel.
    #[must_use]
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::A8 => 1,
            Self::Rgba8 => 4,
        }
    }
}

/// Identifies an atlas page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PageId(pub u32);

#[derive(Debug, Clone)]
struct Shelf {
    y: u32,
    height: u32,
    x: u32,
}

/// One page of glyph bitmaps.
#[derive(Debug, Clone)]
pub struct AtlasPage {
    /// The page id.
    pub id: PageId,
    /// Pixel format.
    pub format: AtlasFormat,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixels, row-major, `width * bytes_per_pixel` bytes per row.
    pub data: Vec<u8>,
    shelves: Vec<Shelf>,
    next_y: u32,
}

impl AtlasPage {
    fn new(id: PageId, format: AtlasFormat) -> Self {
        let len = (ATLAS_PAGE_SIZE * ATLAS_PAGE_SIZE) as usize * format.bytes_per_pixel();
        Self {
            id,
            format,
            width: ATLAS_PAGE_SIZE,
            height: ATLAS_PAGE_SIZE,
            data: vec![0; len],
            shelves: Vec::new(),
            next_y: 0,
        }
    }

    /// Shelf packing: glyphs of similar height share a shelf; a new shelf opens below the last one.
    fn allocate(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let pw = w + PADDING;
        // Shelf heights are rounded up to a multiple of 4 so similar glyphs share shelves.
        let ph = (h + PADDING).div_ceil(4) * 4;
        let mut best: Option<usize> = None;
        for (i, s) in self.shelves.iter().enumerate() {
            if s.height >= ph
                && s.height <= ph + ph / 2 + 4
                && s.x + pw <= self.width
                && best.is_none_or(|b| self.shelves[b].height > s.height)
            {
                best = Some(i);
            }
        }
        if let Some(i) = best {
            let s = &mut self.shelves[i];
            let pos = (s.x, s.y);
            s.x += pw;
            return Some(pos);
        }
        if self.next_y + ph <= self.height && pw <= self.width {
            let y = self.next_y;
            self.shelves.push(Shelf {
                y,
                height: ph,
                x: pw,
            });
            self.next_y += ph;
            return Some((0, y));
        }
        None
    }

    fn blit(&mut self, x: u32, y: u32, w: u32, h: u32, src: &[u8]) {
        let bpp = self.format.bytes_per_pixel();
        let row = w as usize * bpp;
        for r in 0..h as usize {
            let dst = ((y as usize + r) * self.width as usize + x as usize) * bpp;
            if let (Some(d), Some(s)) = (
                self.data.get_mut(dst..dst + row),
                src.get(r * row..(r + 1) * row),
            ) {
                d.copy_from_slice(s);
            }
        }
    }

    /// Fraction of the page's height already used by shelves.
    #[must_use]
    pub fn fill_ratio(&self) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let r = self.next_y as f32 / self.height as f32;
        r
    }
}

/// Cache key of a rasterised glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    /// Font instance.
    pub font: FontId,
    /// Glyph id.
    pub glyph: u32,
    /// Device pixel size in 1/64 px.
    pub size_64: u32,
    /// Horizontal sub-pixel offset in quarters of a pixel (0–3).
    pub subpixel: u8,
}

impl GlyphKey {
    /// The key for a glyph whose pen sits at device x `x`; returns the key and the integer pixel x
    /// the bitmap is placed relative to.
    #[must_use]
    pub fn at(font: FontId, glyph: u32, size: f32, x: f32) -> (Self, i32) {
        let steps = f32::from(SUBPIXEL_STEPS);
        let q = (x * steps).round();
        let whole = (q / steps).floor();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let subpixel = (q - whole * steps) as u8 % SUBPIXEL_STEPS;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let size_64 = (size.clamp(0.0, 16_384.0) * 64.0).round() as u32;
        #[allow(clippy::cast_possible_truncation)]
        let whole = whole as i32;
        (
            Self {
                font,
                glyph,
                size_64,
                subpixel,
            },
            whole,
        )
    }

    /// Device pixel size.
    #[must_use]
    pub fn size(&self) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let s = self.size_64 as f32 / 64.0;
        s
    }
}

/// Where a rasterised glyph lives in the atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasGlyph {
    /// Page.
    pub page: PageId,
    /// Pixel format of the page.
    pub format: AtlasFormat,
    /// Left of the bitmap in the page.
    pub x: u16,
    /// Top of the bitmap in the page.
    pub y: u16,
    /// Bitmap width.
    pub width: u16,
    /// Bitmap height.
    pub height: u16,
    /// Offset from the pen's integer x to the bitmap's left edge.
    pub left: i16,
    /// Offset from the baseline up to the bitmap's top edge.
    pub top: i16,
}

/// A glyph bitmap placed in device pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedGlyph {
    /// The atlas entry.
    pub glyph: AtlasGlyph,
    /// Device x of the bitmap's left edge.
    pub x: i32,
    /// Device y of the bitmap's top edge.
    pub y: i32,
}

/// Atlas failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtlasError {
    /// The font id is not in the store.
    UnknownFont(FontId),
    /// The glyph bitmap is larger than a page (draw it as an outline).
    TooLarge {
        /// Width.
        width: u32,
        /// Height.
        height: u32,
    },
}

impl std::fmt::Display for AtlasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownFont(id) => write!(f, "font {} is not in the font store", id.0),
            Self::TooLarge { width, height } => {
                write!(
                    f,
                    "glyph bitmap {width}×{height} does not fit an atlas page"
                )
            }
        }
    }
}

impl std::error::Error for AtlasError {}

/// A run prepared for painting: bitmap glyphs from the atlas, or outlines for large sizes.
#[derive(Debug, Clone)]
pub enum RunGlyphs {
    /// Glyph bitmaps.
    Bitmaps(Vec<PlacedGlyph>),
    /// Glyph outlines in device pixels (y down), for sizes at or above [`OUTLINE_THRESHOLD`].
    Outlines(Vec<BezPath>),
}

/// The glyph atlas.
pub struct GlyphAtlas {
    pages: Vec<AtlasPage>,
    entries: HashMap<GlyphKey, Option<AtlasGlyph>>,
    scale: ScaleContext,
    dirty: BTreeSet<PageId>,
}

impl std::fmt::Debug for GlyphAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlyphAtlas")
            .field("pages", &self.pages.len())
            .field("entries", &self.entries.len())
            .field("dirty", &self.dirty)
            .finish_non_exhaustive()
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphAtlas {
    /// An empty atlas.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            entries: HashMap::new(),
            scale: ScaleContext::new(),
            dirty: BTreeSet::new(),
        }
    }

    /// The pages.
    #[must_use]
    pub fn pages(&self) -> &[AtlasPage] {
        &self.pages
    }

    /// One page.
    #[must_use]
    pub fn page(&self, id: PageId) -> Option<&AtlasPage> {
        self.pages.get(id.0 as usize)
    }

    /// Number of cached glyphs (including empty ones such as spaces).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing has been rasterised.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Pages changed since the last call (the painter re-uploads them).
    pub fn take_dirty(&mut self) -> Vec<PageId> {
        std::mem::take(&mut self.dirty).into_iter().collect()
    }

    /// Drops every page and glyph (e.g. after a zoom change makes the cache useless).
    pub fn clear(&mut self) {
        self.pages.clear();
        self.entries.clear();
        self.dirty.clear();
    }

    /// The atlas entry for a glyph, rasterising it on first use. `Ok(None)` for glyphs without
    /// pixels (spaces).
    pub fn glyph(
        &mut self,
        fonts: &FontStore,
        key: GlyphKey,
    ) -> Result<Option<AtlasGlyph>, AtlasError> {
        if let Some(e) = self.entries.get(&key) {
            return Ok(*e);
        }
        let inst = fonts
            .font(key.font)
            .ok_or(AtlasError::UnknownFont(key.font))?;
        let font = swash::FontRef::from_index(inst.bytes(), inst.index as usize)
            .ok_or(AtlasError::UnknownFont(key.font))?;
        let mut scaler = self
            .scale
            .builder(font)
            .size(key.size())
            .hint(false)
            .normalized_coords(inst.coords.iter().copied())
            .build();
        let mut render = Render::new(&[
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ]);
        render.format(Format::Alpha).offset(Vector::new(
            f32::from(key.subpixel) / f32::from(SUBPIXEL_STEPS),
            0.0,
        ));
        if inst.embolden {
            render.embolden(key.size() / 48.0);
        }
        if let Some(angle) = inst.skew {
            render.transform(Some(Transform::skew(
                Angle::from_degrees(angle),
                Angle::ZERO,
            )));
        }
        let gid = u16::try_from(key.glyph).unwrap_or(0);
        let Some(image) = render.render(&mut scaler, gid) else {
            self.entries.insert(key, None);
            return Ok(None);
        };
        let (w, h) = (image.placement.width, image.placement.height);
        if w == 0 || h == 0 {
            self.entries.insert(key, None);
            return Ok(None);
        }
        if w + PADDING > ATLAS_PAGE_SIZE || h + PADDING > ATLAS_PAGE_SIZE {
            return Err(AtlasError::TooLarge {
                width: w,
                height: h,
            });
        }
        let (format, pixels): (AtlasFormat, Vec<u8>) = match image.content {
            Content::Mask => (AtlasFormat::A8, image.data),
            Content::Color => (AtlasFormat::Rgba8, premultiply(image.data)),
            // Never requested (we ask for Format::Alpha), but reduce to coverage if it happens.
            Content::SubpixelMask => (
                AtlasFormat::A8,
                image.data.as_chunks::<4>().0.iter().map(|p| p[1]).collect(),
            ),
        };
        let (page, x, y) = self.allocate(format, w, h);
        self.pages[page.0 as usize].blit(x, y, w, h, &pixels);
        self.dirty.insert(page);
        let to16 = |v: u32| u16::try_from(v).unwrap_or(u16::MAX);
        let entry = AtlasGlyph {
            page,
            format,
            x: to16(x),
            y: to16(y),
            width: to16(w),
            height: to16(h),
            left: i16::try_from(image.placement.left).unwrap_or(0),
            top: i16::try_from(image.placement.top).unwrap_or(0),
        };
        self.entries.insert(key, Some(entry));
        Ok(Some(entry))
    }

    fn allocate(&mut self, format: AtlasFormat, w: u32, h: u32) -> (PageId, u32, u32) {
        for page in self.pages.iter_mut().filter(|p| p.format == format) {
            if let Some((x, y)) = page.allocate(w, h) {
                return (page.id, x, y);
            }
        }
        let id = PageId(u32::try_from(self.pages.len()).unwrap_or(u32::MAX));
        let mut page = AtlasPage::new(id, format);
        let (x, y) = page.allocate(w, h).unwrap_or((0, 0));
        self.pages.push(page);
        (id, x, y)
    }

    /// Prepares a glyph run for painting at `scale` device pixels per layout pixel, with the
    /// paragraph's origin at device (`origin_x`, `origin_y`).
    pub fn prepare_run(
        &mut self,
        fonts: &FontStore,
        run: &GlyphRun,
        origin_x: f32,
        origin_y: f32,
        scale: f32,
    ) -> Result<RunGlyphs, AtlasError> {
        let device_size = run.size * scale;
        if device_size >= OUTLINE_THRESHOLD {
            let mut paths = Vec::with_capacity(run.glyphs.len());
            for g in &run.glyphs {
                if let Some(mut path) = glyph_outline(fonts, run.font, g.id, device_size) {
                    path.apply_affine(kurbo::Affine::translate((
                        f64::from(origin_x + g.x * scale),
                        f64::from(origin_y + g.y * scale),
                    )));
                    paths.push(path);
                }
            }
            return Ok(RunGlyphs::Outlines(paths));
        }
        let mut placed = Vec::with_capacity(run.glyphs.len());
        for g in &run.glyphs {
            let (key, ix) = GlyphKey::at(run.font, g.id, device_size, origin_x + g.x * scale);
            if let Some(entry) = self.glyph(fonts, key)? {
                #[allow(clippy::cast_possible_truncation)]
                let baseline = (origin_y + g.y * scale).round() as i32;
                placed.push(PlacedGlyph {
                    glyph: entry,
                    x: ix + i32::from(entry.left),
                    y: baseline - i32::from(entry.top),
                });
            }
        }
        Ok(RunGlyphs::Bitmaps(placed))
    }
}

fn premultiply(mut rgba: Vec<u8>) -> Vec<u8> {
    for p in rgba.as_chunks_mut::<4>().0 {
        let a = u16::from(p[3]);
        for c in &mut p[..3] {
            #[allow(clippy::cast_possible_truncation)]
            let v = ((u16::from(*c) * a + 127) / 255) as u8;
            *c = v;
        }
    }
    rgba
}

/// Collects a skrifa outline into a kurbo path, flipping y so it grows downwards.
struct PathPen {
    path: BezPath,
    skew: f64,
}

impl PathPen {
    fn pt(&self, x: f32, y: f32) -> Point {
        let (x, y) = (f64::from(x), f64::from(y));
        Point::new(x + y * self.skew, -y)
    }
}

impl OutlinePen for PathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.path.move_to(p);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.path.line_to(p);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (c, p) = (self.pt(cx0, cy0), self.pt(x, y));
        self.path.quad_to(c, p);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (c0, c1, p) = (self.pt(cx0, cy0), self.pt(cx1, cy1), self.pt(x, y));
        self.path.curve_to(c0, c1, p);
    }
    fn close(&mut self) {
        self.path.close_path();
    }
}

/// The outline of a glyph at `size` px, with the pen at the origin and y growing downwards (ready
/// for `Path2D`). Synthetic oblique is applied; synthetic bold is not (painters stroke the path).
#[must_use]
pub fn glyph_outline(fonts: &FontStore, font: FontId, glyph: u32, size: f32) -> Option<BezPath> {
    let inst = fonts.font(font)?;
    let f = skrifa::FontRef::from_index(inst.bytes(), inst.index).ok()?;
    let outlines = f.outline_glyphs();
    let g = outlines.get(skrifa::GlyphId::new(glyph))?;
    let coords = skrifa_coords(&inst.coords);
    let mut pen = PathPen {
        path: BezPath::new(),
        skew: inst
            .skew
            .map_or(0.0, |deg| f64::from(deg).to_radians().tan()),
    };
    g.draw(
        DrawSettings::unhinted(Size::new(size), LocationRef::new(&coords)),
        &mut pen,
    )
    .ok()?;
    Some(pen.path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subpixel_keys_quantise_to_quarters() {
        let f = FontId(0);
        assert_eq!(GlyphKey::at(f, 1, 12.0, 10.0).0.subpixel, 0);
        assert_eq!(GlyphKey::at(f, 1, 12.0, 10.26).0.subpixel, 1);
        assert_eq!(GlyphKey::at(f, 1, 12.0, 10.5).0.subpixel, 2);
        let (k, x) = GlyphKey::at(f, 1, 12.0, 10.9);
        assert_eq!((k.subpixel, x), (0, 11));
        let (k, x) = GlyphKey::at(f, 1, 12.0, -0.3);
        assert_eq!((k.subpixel, x), (3, -1));
        assert!((k.size() - 12.0).abs() < 1e-6);
    }

    #[test]
    fn shelves_pack_without_overlap() {
        let mut page = AtlasPage::new(PageId(0), AtlasFormat::A8);
        let mut rects = Vec::new();
        for i in 0..400_u32 {
            let (w, h) = (5 + i % 23, 7 + i % 17);
            let (x, y) = page.allocate(w, h).expect("space");
            assert!(x + w <= ATLAS_PAGE_SIZE && y + h <= ATLAS_PAGE_SIZE);
            rects.push((x, y, w, h));
        }
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                let disjoint =
                    a.0 + a.2 <= b.0 || b.0 + b.2 <= a.0 || a.1 + a.3 <= b.1 || b.1 + b.3 <= a.1;
                assert!(disjoint, "{a:?} overlaps {b:?}");
            }
        }
        assert!(page.fill_ratio() < 0.5);
    }
}
