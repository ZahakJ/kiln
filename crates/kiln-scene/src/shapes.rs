// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shapes every painter draws the same way: rounded rectangles, the caret spark, squiggles and
//! touch handles. `packages/surface/src/shapes.ts` is the TypeScript twin of this file; keep the
//! numbers in step (the painter-parity e2e test compares the two painters pixel by pixel).

use crate::geom::Bounds;
use crate::op::HandleSide;
use crate::path::PathData;

/// Cubic Bézier constant for a quarter circle.
pub const KAPPA: f32 = 0.552_284_8;

/// A rectangle with circular corners of radius `r` (clamped to half the shorter side).
#[must_use]
pub fn rounded_rect(b: Bounds, r: f32) -> PathData {
    let r = r.clamp(0.0, b.w.min(b.h) / 2.0);
    if r <= 0.0 {
        return PathData::rect(b);
    }
    let k = r * (1.0 - KAPPA);
    let (x0, y0, x1, y1) = (b.x, b.y, b.right(), b.bottom());
    let mut p = PathData::new();
    p.move_to(x0 + r, y0)
        .line_to(x1 - r, y0)
        .cubic_to(x1 - k, y0, x1, y0 + k, x1, y0 + r)
        .line_to(x1, y1 - r)
        .cubic_to(x1, y1 - k, x1 - k, y1, x1 - r, y1)
        .line_to(x0 + r, y1)
        .cubic_to(x0 + k, y1, x0, y1 - k, x0, y1 - r)
        .line_to(x0, y0 + r)
        .cubic_to(x0, y0 + k, x0 + k, y0, x0 + r, y0)
        .close();
    p
}

/// The Kiln spark (brand kit `spark.svg`, a four-point star with shortened side arms) centred on
/// `(cx, cy)`, `size` wide and tall.
#[must_use]
pub fn spark(cx: f32, cy: f32, size: f32) -> PathData {
    // spark.svg, viewBox 0 0 24 24.
    const PTS: [[f32; 2]; 13] = [
        [12.0, 1.0],
        [13.85, 7.38],
        [16.62, 10.15],
        [20.14, 12.0],
        [16.62, 13.85],
        [13.85, 16.62],
        [12.0, 23.0],
        [10.15, 16.62],
        [7.38, 13.85],
        [3.86, 12.0],
        [7.38, 10.15],
        [10.15, 7.38],
        [12.0, 1.0],
    ];
    let s = size / 24.0;
    let m = |p: [f32; 2]| ((p[0] - 12.0) * s + cx, (p[1] - 12.0) * s + cy);
    let mut path = PathData::new();
    let (x, y) = m(PTS[0]);
    path.move_to(x, y);
    for c in PTS[1..].as_chunks::<3>().0 {
        let (a, b) = m(c[0]);
        let (cx2, cy2) = m(c[1]);
        let (ex, ey) = m(c[2]);
        path.cubic_to(a, b, cx2, cy2, ex, ey);
    }
    path.close();
    path
}

/// The spark's size and position for a caret bar: 8 px wide, its centre on the bar's centre line
/// and 3 px above the bar's top (as in the brand mockups).
#[must_use]
pub fn caret_spark(caret: Bounds) -> PathData {
    spark(caret.x + caret.w / 2.0, caret.y - 3.0, 8.0)
}

/// A wavy squiggle from `x` to `x + width` centred on `y`, as a centre line to stroke 1 px wide:
/// half-waves 2 px long and 1.5 px high.
#[must_use]
pub fn squiggle(x: f32, y: f32, width: f32) -> PathData {
    let mut p = PathData::new();
    if !(width > 0.0 && x.is_finite() && y.is_finite() && width.is_finite()) {
        return p;
    }
    // Bounded: at most 8,192 half-waves (16,384 px), far wider than any page.
    let width = width.min(16_384.0);
    p.move_to(x, y);
    let mut at = 0.0f32;
    let mut up = true;
    while at < width {
        let step = 2.0f32.min(width - at);
        let peak = if up { y - 1.5 } else { y + 1.5 };
        p.quad_to(x + at + step / 2.0, peak, x + at + step, y);
        at += step;
        up = !up;
    }
    p
}

/// A teardrop touch handle whose sharp corner sits at `(x, y)` (the bottom of the caret line) and
/// whose round body, `size` across, hangs below: to the left for [`HandleSide::Start`], to the
/// right for [`HandleSide::End`], straight down for [`HandleSide::Caret`].
#[must_use]
pub fn teardrop(x: f32, y: f32, size: f32, side: HandleSide) -> PathData {
    let r = size / 2.0;
    let k = r * KAPPA;
    // The End shape relative to the tip: a 2r square below-right with three rounded corners.
    let segs: [[f32; 6]; 3] = [
        [r + k, 0.0, 2.0 * r, r - k, 2.0 * r, r],
        [2.0 * r, r + k, r + k, 2.0 * r, r, 2.0 * r],
        [r - k, 2.0 * r, 0.0, r + k, 0.0, r],
    ];
    let (cos, sin) = match side {
        HandleSide::End | HandleSide::Start => (1.0f32, 0.0f32),
        HandleSide::Caret => (
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
        ),
    };
    let mirror = if side == HandleSide::Start { -1.0 } else { 1.0 };
    let t = |px: f32, py: f32| {
        let px = px * mirror;
        (x + px * cos - py * sin, y + px * sin + py * cos)
    };
    let mut p = PathData::new();
    p.move_to(x, y);
    let (lx, ly) = t(r, 0.0);
    p.line_to(lx, ly);
    for s in segs {
        let (a, b) = t(s[0], s[1]);
        let (c, d) = t(s[2], s[3]);
        let (e, f) = t(s[4], s[5]);
        p.cubic_to(a, b, c, d, e, f);
    }
    p.close();
    p
}

/// Error function (Abramowitz & Stegun 7.1.26, |error| < 1.5e-7). The TypeScript painter uses the
/// same formula so both painters compute identical shadows.
#[must_use]
pub fn erf(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let y = 1.0
        - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t
            + 0.254_829_592)
            * t
            * (-x * x).exp();
    sign * y
}

/// Coverage (0–1) of a Gaussian-blurred interval `[a, b]` (σ = `sigma`) at `t`. With `sigma`
/// ≤ 0 it is the sharp interval.
#[must_use]
pub fn blurred_interval(t: f64, a: f64, b: f64, sigma: f64) -> f64 {
    if sigma <= 0.0 {
        return if t >= a && t < b { 1.0 } else { 0.0 };
    }
    let s = sigma * std::f64::consts::SQRT_2;
    0.5 * (erf((t - a) / s) - erf((t - b) / s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_are_valid_closed_paths() {
        for p in [
            rounded_rect(Bounds::new(0.0, 0.0, 10.0, 6.0), 4.0),
            rounded_rect(Bounds::new(0.0, 0.0, 10.0, 6.0), 0.0),
            spark(10.0, 10.0, 8.0),
            teardrop(5.0, 5.0, 20.0, HandleSide::Start),
            teardrop(5.0, 5.0, 20.0, HandleSide::End),
            teardrop(5.0, 5.0, 20.0, HandleSide::Caret),
        ] {
            assert!(p.is_valid() && !p.is_empty());
            assert_eq!(p.verbs.last(), Some(&crate::verb::CLOSE));
        }
        let b = spark(10.0, 10.0, 24.0).bounds();
        assert!((b.y - -1.0).abs() < 1e-4 && (b.bottom() - 21.0).abs() < 1e-4);
        assert!((b.x - 1.86).abs() < 1e-4, "{b:?}");
    }

    #[test]
    fn teardrops_hang_below_on_their_side() {
        let end = teardrop(100.0, 50.0, 20.0, HandleSide::End).bounds();
        assert_eq!((end.x, end.y), (100.0, 50.0));
        let start = teardrop(100.0, 50.0, 20.0, HandleSide::Start).bounds();
        assert!((start.right() - 100.0).abs() < 1e-4);
        let caret = teardrop(100.0, 50.0, 20.0, HandleSide::Caret).bounds();
        assert!((caret.x + caret.w / 2.0 - 100.0).abs() < 0.5, "{caret:?}");
        assert!(caret.y >= 50.0 - 1e-3);
    }

    #[test]
    fn squiggles_are_bounded_and_finite() {
        let s = squiggle(10.0, 20.0, 9.0);
        assert!(s.is_valid());
        let b = s.bounds();
        assert!((b.x - 10.0).abs() < 1e-6 && (b.right() - 19.0).abs() < 1e-6);
        assert!(b.y >= 18.5 && b.bottom() <= 21.5);
        assert!(squiggle(0.0, 0.0, f32::NAN).is_empty());
        assert!(squiggle(0.0, 0.0, -1.0).is_empty());
        assert!(squiggle(0.0, 0.0, f32::MAX).verbs.len() <= 8_193);
    }

    #[test]
    fn erf_matches_known_values() {
        for (x, want) in [
            (0.0, 0.0),
            (0.5, 0.520_499_9),
            (1.0, 0.842_700_8),
            (2.0, 0.995_322_3),
        ] {
            assert!((erf(x) - want).abs() < 2e-7, "erf({x})");
            assert!((erf(-x) + want).abs() < 2e-7);
        }
        assert!((blurred_interval(5.0, 0.0, 10.0, 0.0) - 1.0).abs() < 1e-12);
        assert!(blurred_interval(-5.0, 0.0, 10.0, 0.0).abs() < 1e-12);
        let edge = blurred_interval(0.0, 0.0, 1000.0, 4.0);
        assert!((edge - 0.5).abs() < 1e-6);
    }
}
