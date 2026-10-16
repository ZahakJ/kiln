// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pictures: decoding the document's media once, at a bounded resolution, and the outline of a
//! picture's opaque pixels for tight and through wrapping.

use std::sync::Arc;

use kiln_doc::Document;
use kiln_scene::Bounds;
use rustc_hash::FxHashMap;

/// Longest side a picture is decoded at (larger pictures are downscaled once).
pub const MAX_SIDE: u32 = 2048;
/// Height of the bands of a wrap outline, px (Word approximates the outline similarly).
pub const BAND: f32 = 8.0;
/// Alpha at or below which a pixel counts as transparent for wrapping.
const CLEAR: u8 = 16;

/// A decoded picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// Stable id (the painter caches the pixels by it).
    pub id: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Straight-alpha RGBA, `width × height × 4` bytes.
    pub rgba: Vec<u8>,
}

/// Decoded pictures by media key.
#[derive(Debug, Default)]
pub struct MediaCache {
    pictures: FxHashMap<String, Option<Arc<Picture>>>,
    next_id: u32,
}

impl MediaCache {
    /// The decoded picture of a media key; `None` for missing or undecodable media (EMF, WMF,
    /// TIFF, corrupt data), which are painted as placeholders.
    pub fn get(&mut self, doc: &Document, key: &str) -> Option<Arc<Picture>> {
        if let Some(p) = self.pictures.get(key) {
            return p.clone();
        }
        self.next_id += 1;
        let id = self.next_id;
        let decoded = doc
            .media
            .get(key)
            .and_then(|m| decode(&m.data, id))
            .map(Arc::new);
        self.pictures.insert(key.to_owned(), decoded.clone());
        decoded
    }

    /// Every picture decoded so far.
    pub fn decoded(&self) -> impl Iterator<Item = &Arc<Picture>> {
        self.pictures.values().flatten()
    }

    /// Forgets everything.
    pub fn clear(&mut self) {
        self.pictures.clear();
    }
}

/// Decodes PNG, JPEG, GIF or BMP bytes, downscaled to [`MAX_SIDE`].
#[must_use]
pub fn decode(bytes: &[u8], id: u32) -> Option<Picture> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = if img.width() > MAX_SIDE || img.height() > MAX_SIDE {
        img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    Some(Picture {
        id,
        width,
        height,
        rgba: rgba.into_raw(),
    })
}

/// The horizontal extent of the opaque pixels of `pic` drawn into `rect`, per band of
/// [`BAND`] px: `[y0, y1, x0, x1]` in the coordinates of `rect`, widened by `dx` and `dy`.
/// Fully transparent bands are left out.
#[must_use]
pub fn alpha_profile(pic: &Picture, rect: Bounds, dx: f32, dy: f32) -> Vec<[f32; 4]> {
    let mut out: Vec<[f32; 4]> = Vec::new();
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return out;
    }
    #[allow(clippy::cast_precision_loss)]
    let (pw, ph) = (pic.width as f32, pic.height as f32);
    let mut y = 0.0_f32;
    while y < rect.h {
        let y1 = (y + BAND).min(rect.h);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let r0 = ((y / rect.h) * ph).floor().max(0.0) as u32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let r1 = (((y1 / rect.h) * ph).ceil() as u32)
            .min(pic.height)
            .max(r0 + 1);
        let mut lo = u32::MAX;
        let mut hi = 0u32;
        for row in r0..r1.min(pic.height) {
            let base = (row * pic.width) as usize * 4;
            let Some(px) = pic.rgba.get(base..base + pic.width as usize * 4) else {
                continue;
            };
            if let Some(first) = px.as_chunks::<4>().0.iter().position(|p| p[3] > CLEAR) {
                let first = u32::try_from(first).unwrap_or(u32::MAX);
                let last = px
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .rposition(|p| p[3] > CLEAR)
                    .map_or(first, |l| u32::try_from(l).unwrap_or(first));
                lo = lo.min(first);
                hi = hi.max(last + 1);
            }
        }
        if lo < hi {
            #[allow(clippy::cast_precision_loss)]
            let (x0, x1) = (lo as f32 / pw * rect.w, hi as f32 / pw * rect.w);
            out.push([
                rect.y + y - dy,
                rect.y + y1 + dy,
                rect.x + x0 - dx,
                rect.x + x1 + dx,
            ]);
        }
        y = y1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut img = image::RgbaImage::new(w, h);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba(f(x, y));
        }
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn decodes_and_downscales() {
        let p = decode(&png(4, 2, |_, _| [255, 0, 0, 255]), 1).unwrap();
        assert_eq!((p.width, p.height, p.rgba.len()), (4, 2, 32));
        assert!(decode(b"not an image", 2).is_none());
    }

    #[test]
    fn alpha_outline_follows_opaque_pixels() {
        // A triangle: row y is opaque for x < y.
        let p = decode(
            &png(16, 16, |x, y| [0, 0, 0, if x < y { 255 } else { 0 }]),
            1,
        )
        .unwrap();
        let prof = alpha_profile(&p, Bounds::new(100.0, 0.0, 16.0, 16.0), 0.0, 0.0);
        assert_eq!(prof.len(), 2);
        assert_eq!(prof[0][2], 100.0);
        assert!(prof[0][3] <= 108.0, "{prof:?}");
        assert!((prof[1][3] - 115.0).abs() <= 1.0, "{prof:?}");
    }
}
