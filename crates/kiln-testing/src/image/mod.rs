// SPDX-License-Identifier: AGPL-3.0-or-later
//! Image utilities for visual tests: PNG I/O, a perceptual per-pixel diff that tolerates images
//! of different sizes, and the page-by-page visual score of the fidelity report.
//!
//! The per-pixel metric is `pixelmatch`'s YIQ distance (shared with `kiln_raster::golden`, so
//! golden tests, painter parity and fidelity agree on what "differs" means).

use std::path::Path;

pub use kiln_raster::Rgba;
use kiln_raster::golden::color_delta;

/// pixelmatch's default per-pixel threshold.
pub const DEFAULT_THRESHOLD: f64 = 0.1;

/// The largest squared YIQ distance (black against white).
const MAX_YIQ_DELTA: f64 = 35_215.0;

/// Reads a PNG file.
///
/// # Errors
///
/// I/O and decoding errors, with the path.
pub fn load_png(path: &Path) -> Result<Rgba, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Rgba::decode_png(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes a PNG file (creating the parent directory).
///
/// # Errors
///
/// I/O and encoding errors, with the path.
pub fn save_png(img: &Rgba, path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let png = img.encode_png()?;
    std::fs::write(path, png).map_err(|e| format!("{}: {e}", path.display()))
}

/// A solid image (tests, padding).
#[must_use]
pub fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Rgba {
    let n = width as usize * height as usize;
    Rgba {
        width,
        height,
        data: rgba.repeat(n),
    }
}

/// The result of [`diff`].
#[derive(Debug, Clone, PartialEq)]
pub struct Diff {
    /// Pixels that differ beyond the threshold, including pixels only one image covers.
    pub differing: u64,
    /// Pixels of the union of both images' areas.
    pub total: u64,
    /// The diff image over the union area: differing pixels red, matching ones a faded grey copy
    /// of `a`, area covered by only one image yellow.
    pub image: Rgba,
}

impl Diff {
    /// Share of differing pixels (0 when both images are empty).
    #[must_use]
    pub fn ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let r = self.differing as f64 / self.total as f64;
            r
        }
    }

    /// `1 − ratio`: how similar the images are.
    #[must_use]
    pub fn score(&self) -> f64 {
        1.0 - self.ratio()
    }
}

fn pixel(img: &Rgba, x: u32, y: u32) -> Option<&[u8]> {
    if x < img.width && y < img.height {
        let i = (y as usize * img.width as usize + x as usize) * 4;
        img.data.get(i..i + 4)
    } else {
        None
    }
}

/// Compares two images pixel by pixel over the union of their areas (top-left aligned); a pixel
/// that only one image covers counts as differing, so a page rendered at another size scores low
/// rather than failing.
///
/// `threshold` is pixelmatch's (0 = exact, 1 = anything matches); values outside 0..=1 are
/// clamped.
#[must_use]
pub fn diff(a: &Rgba, b: &Rgba, threshold: f64) -> Diff {
    let t = threshold.clamp(0.0, 1.0);
    let max_delta = MAX_YIQ_DELTA * t * t;
    let (w, h) = (a.width.max(b.width), a.height.max(b.height));
    let mut image = Vec::with_capacity(w as usize * h as usize * 4);
    let mut differing = 0u64;
    for y in 0..h {
        for x in 0..w {
            match (pixel(a, x, y), pixel(b, x, y)) {
                (Some(pa), Some(pb)) => {
                    if color_delta(pa, pb) > max_delta {
                        differing += 1;
                        image.extend([255, 0, 0, 255]);
                    } else {
                        let luma = (f64::from(pa[0]) * 0.299
                            + f64::from(pa[1]) * 0.587
                            + f64::from(pa[2]) * 0.114)
                            * f64::from(pa[3])
                            / 255.0;
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let g = (255.0 - (255.0 - luma) * 0.1).round().clamp(0.0, 255.0) as u8;
                        image.extend([g, g, g, 255]);
                    }
                }
                _ => {
                    differing += 1;
                    image.extend([255, 210, 0, 255]);
                }
            }
        }
    }
    Diff {
        differing,
        total: u64::from(w) * u64::from(h),
        image: Rgba {
            width: w,
            height: h,
            data: image,
        },
    }
}

/// The visual score of two page sequences: the mean over pages of `1 − differing ratio`, where a
/// page that only one side has scores 0. Two empty sequences score 1.
#[must_use]
pub fn visual_score(a: &[Rgba], b: &[Rgba], threshold: f64) -> f64 {
    let pages = a.len().max(b.len());
    if pages == 0 {
        return 1.0;
    }
    let sum: f64 = a
        .iter()
        .zip(b)
        .map(|(pa, pb)| diff(pa, pb, threshold).score())
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let score = sum / pages as f64;
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_images_do_not_differ() {
        let a = solid(4, 3, [10, 20, 30, 255]);
        let d = diff(&a, &a.clone(), DEFAULT_THRESHOLD);
        assert_eq!((d.differing, d.total), (0, 12));
        assert!((d.score() - 1.0).abs() < f64::EPSILON);
        assert_eq!((d.image.width, d.image.height), (4, 3));
    }

    #[test]
    fn counts_changed_pixels_and_uncovered_area() {
        let a = solid(4, 4, [255, 255, 255, 255]);
        let mut b = a.clone();
        b.data[0..4].copy_from_slice(&[0, 0, 0, 255]);
        let d = diff(&a, &b, DEFAULT_THRESHOLD);
        assert_eq!(d.differing, 1);
        assert_eq!(&d.image.data[0..4], &[255, 0, 0, 255]);
        // A wider image: the extra column is uncovered on one side.
        let c = solid(5, 4, [255, 255, 255, 255]);
        let d = diff(&a, &c, DEFAULT_THRESHOLD);
        assert_eq!((d.differing, d.total), (4, 20));
        assert_eq!(&d.image.data[16..20], &[255, 210, 0, 255]);
        // Slight colour noise stays under the threshold; threshold 0 sees it.
        let mut e = a.clone();
        e.data[0] = 250;
        assert_eq!(diff(&a, &e, DEFAULT_THRESHOLD).differing, 0);
        assert_eq!(diff(&a, &e, 0.0).differing, 1);
        assert_eq!(
            diff(&solid(0, 0, [0; 4]), &solid(0, 0, [0; 4]), 0.1).ratio(),
            0.0
        );
    }

    #[test]
    fn visual_score_penalises_missing_pages() {
        let p = solid(2, 2, [0, 0, 0, 255]);
        let one = std::slice::from_ref(&p);
        assert!((visual_score(one, one, 0.1) - 1.0).abs() < 1e-12);
        assert!((visual_score(&[p.clone(), p.clone()], one, 0.1) - 0.5).abs() < 1e-12);
        assert!((visual_score(&[], &[], 0.1) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn png_round_trip_through_files() {
        let dir = std::env::temp_dir().join(format!("kiln-testing-png-{}", std::process::id()));
        let path = dir.join("nested/a.png");
        let img = solid(3, 2, [1, 2, 3, 255]);
        save_png(&img, &path).unwrap();
        assert_eq!(load_png(&path).unwrap(), img);
        assert!(load_png(&dir.join("missing.png")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
