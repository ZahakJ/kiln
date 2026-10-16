// SPDX-License-Identifier: AGPL-3.0-or-later
//! Golden-image tests: compare a rendering with a committed PNG using a perceptual per-pixel
//! difference (the YIQ metric of `pixelmatch`, threshold 0.1) and fail when more than 0.3 % of the
//! pixels differ. On failure the actual image and a diff (differences in red over a faded copy)
//! are written next to each other for review.

use std::path::{Path, PathBuf};

use crate::png_io::Rgba;

/// Share of differing pixels a golden comparison tolerates (0.3 %).
pub const MAX_DIFF_RATIO: f64 = 0.003;
/// Per-pixel colour threshold (pixelmatch's default).
pub const PIXEL_THRESHOLD: f64 = 0.1;

/// The outcome of a comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// Pixels that differ beyond [`PIXEL_THRESHOLD`].
    pub differing: usize,
    /// Pixels compared.
    pub total: usize,
    /// The diff image (red where pixels differ).
    pub diff: Rgba,
}

impl Comparison {
    /// Share of differing pixels.
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
}

fn blend_white(c: f64, a: f64) -> f64 {
    255.0 + (c - 255.0) * a
}

/// pixelmatch's squared YIQ distance between two straight-alpha pixels, blended over white.
#[must_use]
pub fn color_delta(a: &[u8], b: &[u8]) -> f64 {
    let (aa, ba) = (f64::from(a[3]) / 255.0, f64::from(b[3]) / 255.0);
    let p = |px: &[u8], al: f64| {
        (
            blend_white(f64::from(px[0]), al),
            blend_white(f64::from(px[1]), al),
            blend_white(f64::from(px[2]), al),
        )
    };
    let (r1, g1, b1) = p(a, aa);
    let (r2, g2, b2) = p(b, ba);
    let y = |r: f64, g: f64, b: f64| r * 0.298_895_31 + g * 0.586_622_47 + b * 0.114_482_23;
    let i = |r: f64, g: f64, b: f64| r * 0.595_977_99 - g * 0.274_176_47 - b * 0.321_801_52;
    let q = |r: f64, g: f64, b: f64| r * 0.211_470_17 - g * 0.522_617_24 + b * 0.311_147_07;
    let dy = y(r1, g1, b1) - y(r2, g2, b2);
    let di = i(r1, g1, b1) - i(r2, g2, b2);
    let dq = q(r1, g1, b1) - q(r2, g2, b2);
    0.5053 * dy * dy + 0.299 * di * di + 0.1957 * dq * dq
}

/// Compares two images of the same size.
///
/// # Errors
///
/// When the sizes differ.
pub fn compare(actual: &Rgba, expected: &Rgba) -> Result<Comparison, String> {
    if (actual.width, actual.height) != (expected.width, expected.height) {
        return Err(format!(
            "size {}×{} differs from the golden {}×{}",
            actual.width, actual.height, expected.width, expected.height
        ));
    }
    let max_delta = 35_215.0 * PIXEL_THRESHOLD * PIXEL_THRESHOLD;
    let mut differing = 0;
    let mut diff = Vec::with_capacity(actual.data.len());
    for (a, e) in actual
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.data.as_chunks::<4>().0)
    {
        if color_delta(a, e) > max_delta {
            differing += 1;
            diff.extend([230, 30, 30, 255]);
        } else {
            // A faded grey copy of the expected pixel, for orientation.
            let l = (u16::from(e[0]) + u16::from(e[1]) + u16::from(e[2])) / 3;
            let l = u8::try_from(255 - (255 - l) / 4).unwrap_or(255);
            diff.extend([l, l, l, 255]);
        }
    }
    Ok(Comparison {
        differing,
        total: actual.data.len() / 4,
        diff: Rgba {
            width: actual.width,
            height: actual.height,
            data: diff,
        },
    })
}

/// Where failing comparisons write `<name>.actual.png` and `<name>.diff.png`:
/// `$KILN_GOLDEN_DIFF_DIR`, or `kiln-golden` in the system temporary directory.
#[must_use]
pub fn diff_dir() -> PathBuf {
    std::env::var_os("KILN_GOLDEN_DIFF_DIR")
        .map_or_else(|| std::env::temp_dir().join("kiln-golden"), PathBuf::from)
}

/// Checks `actual` against the golden PNG at `path`. With `KILN_UPDATE_GOLDEN` set, (re)writes the
/// golden instead. On failure writes the actual and diff images to [`diff_dir`].
///
/// # Errors
///
/// A message naming the share of differing pixels and the diff files.
pub fn check(actual: &Rgba, path: &Path) -> Result<Comparison, String> {
    if std::env::var_os("KILN_UPDATE_GOLDEN").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, actual.encode_png()?).map_err(|e| e.to_string())?;
    }
    check_in(actual, path, &diff_dir())
}

/// [`check`] with an explicit directory for the failure images, never updating the golden.
///
/// # Errors
///
/// As [`check`].
pub fn check_in(actual: &Rgba, path: &Path, diff_dir: &Path) -> Result<Comparison, String> {
    let bytes = std::fs::read(path).map_err(|e| {
        format!(
            "{}: {e} (run the test with KILN_UPDATE_GOLDEN=1 to create it)",
            path.display()
        )
    })?;
    let expected = Rgba::decode_png(&bytes)?;
    let name = path
        .file_stem()
        .map_or_else(|| "golden".into(), |s| s.to_string_lossy().into_owned());
    let write_failure = |diff: Option<&Rgba>| -> String {
        let dir = diff_dir;
        let _ = std::fs::create_dir_all(dir);
        let actual_path = dir.join(format!("{name}.actual.png"));
        let _ = actual.encode_png().map(|b| std::fs::write(&actual_path, b));
        let mut msg = format!("actual: {}", actual_path.display());
        if let Some(d) = diff {
            let diff_path = dir.join(format!("{name}.diff.png"));
            let _ = d.encode_png().map(|b| std::fs::write(&diff_path, b));
            msg.push_str(&format!(", diff: {}", diff_path.display()));
        }
        msg
    };
    let cmp = match compare(actual, &expected) {
        Ok(c) => c,
        Err(e) => return Err(format!("{}: {e}; {}", path.display(), write_failure(None))),
    };
    if cmp.ratio() > MAX_DIFF_RATIO {
        return Err(format!(
            "{}: {} of {} pixels differ ({:.3} % > {:.1} %); {}",
            path.display(),
            cmp.differing,
            cmp.total,
            cmp.ratio() * 100.0,
            MAX_DIFF_RATIO * 100.0,
            write_failure(Some(&cmp.diff))
        ));
    }
    Ok(cmp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, px: [u8; 4]) -> Rgba {
        Rgba {
            width: w,
            height: h,
            data: px.repeat((w * h) as usize),
        }
    }

    #[test]
    fn identical_images_match_and_small_noise_is_tolerated() {
        let a = solid(10, 10, [200, 100, 50, 255]);
        let c = compare(&a, &a).unwrap();
        assert_eq!(c.differing, 0);
        let mut b = a.clone();
        b.data[0] = 201; // imperceptible
        assert_eq!(compare(&a, &b).unwrap().differing, 0);
        b.data[4..8].copy_from_slice(&[0, 0, 0, 255]); // a real difference
        let c = compare(&a, &b).unwrap();
        assert_eq!(c.differing, 1);
        assert!((c.ratio() - 0.01).abs() < 1e-12);
        assert_eq!(&c.diff.data[4..8], &[230, 30, 30, 255]);
        assert!(compare(&a, &solid(9, 10, [0; 4])).is_err());
    }

    #[test]
    fn check_writes_diffs_on_failure() {
        let dir = std::env::temp_dir().join(format!("kiln-golden-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let golden = dir.join("g.png");
        let a = solid(20, 20, [255, 255, 255, 255]);
        std::fs::write(&golden, a.encode_png().unwrap()).unwrap();
        assert!(check(&a, &golden).is_ok());
        let mut b = a.clone();
        for px in b.data.as_chunks_mut::<4>().0.iter_mut().take(10) {
            px.copy_from_slice(&[0, 0, 0, 255]);
        }
        let err = check_in(&b, &golden, &dir).unwrap_err();
        assert!(err.contains("10 of 400 pixels differ"), "{err}");
        assert!(dir.join("g.diff.png").exists() && dir.join("g.actual.png").exists());
        let small = Rgba {
            width: 2,
            height: 2,
            data: vec![0; 16],
        };
        assert!(
            check_in(&small, &golden, &dir)
                .unwrap_err()
                .contains("differs from the golden")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
