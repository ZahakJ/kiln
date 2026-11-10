// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pictures: media parts decoded once (PNG, JPEG, GIF, BMP, WebP) at a bounded resolution and
//! cached by part name with a stable image id the painter caches the pixels under.

use std::sync::Arc;

use kiln_deck::Presentation;
use kiln_scene::{ImageId, ImageUpload};
use rustc_hash::{FxHashMap, FxHashSet};

/// Longest side a picture is decoded at (larger pictures are downscaled once).
pub const MAX_SIDE: u32 = 2560;

/// A decoded picture.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    /// Stable id.
    pub id: ImageId,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Straight-alpha RGBA.
    pub rgba: Vec<u8>,
    /// The picture's own size in px at 96 dpi: its pixels at the resolution the file records
    /// (JFIF density, PNG `pHYs`), 96 dpi when it records none. Tiled fills repeat it at this size.
    pub natural: (f32, f32),
}

/// Decodes image bytes, downscaled to [`MAX_SIDE`]. `None` for formats Kiln does not draw (EMF,
/// WMF, TIFF) and damaged data.
#[must_use]
pub fn decode(bytes: &[u8], id: ImageId) -> Option<Decoded> {
    let img = image::load_from_memory(bytes).ok()?;
    let (dx, dy) = density(bytes).unwrap_or((96.0, 96.0));
    #[allow(clippy::cast_precision_loss)]
    let natural = (
        img.width() as f32 * 96.0 / dx,
        img.height() as f32 * 96.0 / dy,
    );
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
    Some(Decoded {
        id,
        width,
        height,
        rgba: rgba.into_raw(),
        natural,
    })
}

/// The resolution an image file records, in pixels per inch (x, y): a JPEG's JFIF density or a
/// PNG's `pHYs` chunk. `None` when it records none, or only an aspect ratio.
#[must_use]
pub fn density(bytes: &[u8]) -> Option<(f32, f32)> {
    let be16 = |at: usize| -> Option<u16> {
        Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
    };
    let be32 = |at: usize| -> Option<u32> {
        Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let dpi = |x: f32, y: f32| (x >= 1.0 && y >= 1.0).then_some((x, y));
    if bytes.starts_with(&[0xff, 0xd8]) {
        // APP0 "JFIF\0": version (2), units (1), x and y density (2 each).
        let mut at = 2;
        while bytes.get(at) == Some(&0xff) {
            let marker = *bytes.get(at + 1)?;
            let len = usize::from(be16(at + 2)?);
            if marker == 0xe0 && bytes.get(at + 4..at + 9) == Some(b"JFIF\0") {
                let (x, y) = (f32::from(be16(at + 12)?), f32::from(be16(at + 14)?));
                return match bytes.get(at + 11)? {
                    1 => dpi(x, y),
                    2 => dpi(x * 2.54, y * 2.54),
                    _ => None,
                };
            }
            if marker == 0xda {
                break;
            }
            at += 2 + len;
        }
        return None;
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        // Chunks: length (4), type (4), data, CRC (4); pHYs comes before IDAT.
        let mut at = 8;
        while let Some(len) = be32(at) {
            let kind = bytes.get(at + 4..at + 8)?;
            if kind == b"pHYs" {
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (be32(at + 8)? as f32, be32(at + 12)? as f32);
                // Unit 1: pixels per metre.
                return (bytes.get(at + 16) == Some(&1)).then(|| dpi(x * 0.0254, y * 0.0254))?;
            }
            if kind == b"IDAT" {
                break;
            }
            at += 12 + usize::try_from(len).ok()?;
        }
    }
    None
}

/// Pictures by part name, decoded on first use. Renders report the image ids they drew; each
/// painter's owner keeps track of which pixels it sent ([`Pictures::upload`]).
#[derive(Debug, Default)]
pub struct Pictures {
    by_part: FxHashMap<String, Option<Arc<Decoded>>>,
    by_id: FxHashMap<u32, Arc<Decoded>>,
    next: u32,
}

impl Pictures {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The decoded picture of a part (decoding it now if needed).
    pub fn get(&mut self, pres: &Presentation, part: &str) -> Option<Arc<Decoded>> {
        if let Some(p) = self.by_part.get(part) {
            return p.clone();
        }
        self.next += 1;
        let id = ImageId(self.next);
        let decoded = pres
            .parts
            .get(part)
            .and_then(|p| decode(&p.bytes, id))
            .map(Arc::new);
        if let Some(d) = &decoded {
            self.by_id.insert(id.0, d.clone());
        }
        self.by_part.insert(part.to_owned(), decoded.clone());
        decoded
    }

    /// The tile of a pattern fill in two colours (made on first use, like a picture).
    pub fn pattern(
        &mut self,
        preset: kiln_ooxml::st::PresetPatternVal,
        fg: u32,
        bg: u32,
    ) -> Arc<Decoded> {
        let key = format!("#pattern:{preset:?}:{fg:08x}:{bg:08x}");
        if let Some(Some(p)) = self.by_part.get(&key) {
            return p.clone();
        }
        self.next += 1;
        let d = Arc::new(Decoded {
            id: ImageId(self.next),
            width: crate::patterns::TILE,
            height: crate::patterns::TILE,
            rgba: crate::patterns::tile(preset, fg, bg),
            #[allow(clippy::cast_precision_loss)]
            natural: (crate::patterns::TILE as f32, crate::patterns::TILE as f32),
        });
        self.by_id.insert(d.id.0, d.clone());
        self.by_part.insert(key, Some(d.clone()));
        d
    }

    /// The media part a picture was decoded from (`None` for pattern tiles).
    #[must_use]
    pub fn part_of(&self, id: ImageId) -> Option<&str> {
        self.by_part.iter().find_map(|(part, d)| {
            d.as_ref()
                .filter(|d| d.id == id && !part.starts_with("#pattern:"))
                .map(|_| part.as_str())
        })
    }

    /// The pixels of a picture for a painter.
    #[must_use]
    pub fn upload(&self, id: ImageId) -> Option<ImageUpload> {
        self.by_id.get(&id.0).map(|d| ImageUpload {
            id: d.id,
            width: d.width,
            height: d.height,
            data: d.rgba.clone(),
        })
    }

    /// Forgets a part (its bytes changed).
    pub fn forget(&mut self, part: &str) {
        if let Some(Some(d)) = self.by_part.remove(part) {
            self.by_id.remove(&d.id.0);
        }
    }
}

/// Which pictures one painter has.
#[derive(Debug, Default, Clone)]
pub struct Uploads {
    sent: FxHashSet<u32>,
}

impl Uploads {
    /// The painter lost everything.
    pub fn reset(&mut self) {
        self.sent.clear();
    }

    /// Pixels of `used` pictures the painter lacks (all of them after a reset).
    pub fn take(&mut self, pictures: &Pictures, used: &[ImageId], all: bool) -> Vec<ImageUpload> {
        if all {
            self.sent.clear();
        }
        let mut out = Vec::new();
        for id in used {
            if self.sent.insert(id.0)
                && let Some(u) = pictures.upload(*id)
            {
                out.push(u);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_png() {
        let mut img = image::RgbaImage::new(3, 2);
        for p in img.pixels_mut() {
            *p = image::Rgba([10, 20, 30, 255]);
        }
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        let d = decode(&out.into_inner(), ImageId(7)).unwrap();
        assert_eq!((d.width, d.height, d.rgba.len()), (3, 2, 24));
        assert!(decode(b"not an image", ImageId(1)).is_none());
        // No pHYs: 96 dpi.
        assert_eq!(d.natural, (3.0, 2.0));
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut c = !0u32;
        for b in data {
            c ^= u32::from(*b);
            for _ in 0..8 {
                c = if c & 1 == 1 {
                    0xedb8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
        }
        !c
    }

    #[test]
    fn reads_the_resolution_the_file_records() {
        // A JFIF header at 75 dpi (as the textures of Office's sample decks).
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10];
        jpeg.extend_from_slice(b"JFIF\0");
        jpeg.extend_from_slice(&[1, 1, 1, 0, 75, 0, 75, 0, 0]);
        assert_eq!(density(&jpeg), Some((75.0, 75.0)));
        // Units 0: an aspect ratio only.
        jpeg[13] = 0;
        assert_eq!(density(&jpeg), None);
        // A PNG at 144 dpi (5669 px per metre).
        let mut img = image::RgbaImage::new(4, 4);
        img.put_pixel(0, 0, image::Rgba([1, 2, 3, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        let png = out.into_inner();
        let mut phys = 9u32.to_be_bytes().to_vec();
        phys.extend_from_slice(b"pHYs");
        phys.extend_from_slice(&5669u32.to_be_bytes());
        phys.extend_from_slice(&5669u32.to_be_bytes());
        phys.push(1);
        phys.extend_from_slice(&crc32(&phys[4..]).to_be_bytes());
        let with = [&png[..33], &phys, &png[33..]].concat();
        let (x, y) = density(&with).unwrap();
        assert!((x - 144.0).abs() < 0.1 && (y - 144.0).abs() < 0.1);
        let d = decode(&with, ImageId(2)).unwrap();
        assert!((d.natural.0 - 4.0 * 96.0 / 144.0).abs() < 0.01);
    }
}
