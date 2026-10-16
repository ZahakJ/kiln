// SPDX-License-Identifier: AGPL-3.0-or-later
//! PNG encoding and decoding of RGBA bitmaps (png 0.18).

use std::io::Cursor;

use tiny_skia::Pixmap;

/// A straight-alpha RGBA8 image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// `width × height × 4` bytes, straight alpha.
    pub data: Vec<u8>,
}

impl Rgba {
    /// The straight-alpha pixels of a (premultiplied) pixmap.
    #[must_use]
    pub fn from_pixmap(p: &Pixmap) -> Self {
        let mut data = Vec::with_capacity(p.data().len());
        for px in p.pixels() {
            let c = px.demultiply();
            data.extend([c.red(), c.green(), c.blue(), c.alpha()]);
        }
        Self {
            width: p.width(),
            height: p.height(),
            data,
        }
    }

    /// Encodes as PNG.
    ///
    /// # Errors
    ///
    /// When the encoder fails (sizes that disagree with the data).
    pub fn encode_png(&self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.width, self.height);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_compression(png::Compression::High);
            let mut w = enc.write_header().map_err(|e| e.to_string())?;
            w.write_image_data(&self.data).map_err(|e| e.to_string())?;
        }
        Ok(out)
    }

    /// Decodes a PNG (8-bit RGB or RGBA, as written by Kiln and by browsers).
    ///
    /// # Errors
    ///
    /// For other formats or malformed data.
    pub fn decode_png(bytes: &[u8]) -> Result<Self, String> {
        let mut dec = png::Decoder::new(Cursor::new(bytes));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = dec.read_info().map_err(|e| e.to_string())?;
        let size = reader
            .output_buffer_size()
            .ok_or_else(|| "PNG too large".to_owned())?;
        let mut buf = vec![0; size];
        let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
        buf.truncate(info.buffer_size());
        let data = match info.color_type {
            png::ColorType::Rgba => buf,
            png::ColorType::Rgb => buf
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            png::ColorType::GrayscaleAlpha => buf
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[0], p[0], p[1]])
                .collect(),
            png::ColorType::Grayscale => buf.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
            png::ColorType::Indexed => return Err("indexed PNG was not expanded".into()),
        };
        Ok(Self {
            width: info.width,
            height: info.height,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trip() {
        let img = Rgba {
            width: 2,
            height: 1,
            data: vec![255, 0, 0, 255, 0, 0, 255, 128],
        };
        let png = img.encode_png().unwrap();
        assert_eq!(Rgba::decode_png(&png).unwrap(), img);
        assert!(Rgba::decode_png(b"not a png").is_err());
    }
}
