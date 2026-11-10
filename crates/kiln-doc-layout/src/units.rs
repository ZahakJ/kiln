// SPDX-License-Identifier: AGPL-3.0-or-later
//! Units. Layout works in CSS pixels (1 px = 1/96 in = 0.75 pt) at zoom 1; the model keeps the
//! file's units: twips (1/20 pt), half-points, eighths of a point (borders) and EMU (drawings).

/// Twips (twentieths of a point) to px.
#[must_use]
pub fn twips(v: i32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let v = v as f32;
    v / 15.0
}

/// Half-points (run sizes) to px.
#[must_use]
pub fn half_points(v: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let v = v as f32;
    v * 2.0 / 3.0
}

/// Signed half-points (run position) to px.
#[must_use]
pub fn half_points_signed(v: i32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let v = v as f32;
    v * 2.0 / 3.0
}

/// Eighths of a point (border widths) to px.
#[must_use]
pub fn eighths(v: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let v = v as f32;
    v / 6.0
}

/// EMU (English Metric Units, 914,400 per inch) to px.
#[must_use]
pub fn emu(v: i64) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let v = v as f64;
    #[allow(clippy::cast_possible_truncation)]
    let px = (v / 9525.0) as f32;
    px
}

/// Points to px.
#[must_use]
pub fn pt(v: f32) -> f32 {
    v * 4.0 / 3.0
}

/// A finite value or a fallback (layout never propagates NaN or infinities from a file).
#[must_use]
pub fn finite_or(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_match_word() {
        assert!((twips(1440) - 96.0).abs() < 1e-4);
        assert!((half_points(22) - pt(11.0)).abs() < 1e-4);
        assert!((eighths(4) - pt(0.5)).abs() < 1e-4);
        assert!((emu(914_400) - 96.0).abs() < 1e-3);
        assert_eq!(finite_or(f32::NAN, 3.0), 3.0);
    }
}
