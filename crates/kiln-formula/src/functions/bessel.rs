// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bessel functions of integer order to near double precision.
//!
//! Small arguments use the power series (no cancellation there); otherwise the integral
//! representations are evaluated with composite 20-point Gauss–Legendre quadrature:
//!
//! - Jₙ(x) = 1/π ∫₀^π cos(nτ − x sin τ) dτ
//! - Iₙ(x) = 1/π ∫₀^π e^{x cos τ} cos(nτ) dτ
//! - Kₙ(x) = ∫₀^∞ e^{−x cosh t} cosh(nt) dt
//! - Yₙ(x) = 1/π ∫₀^π sin(x sin τ − nτ) dτ − 1/π ∫₀^∞ (e^{nt} + (−1)ⁿ e^{−nt}) e^{−x sinh t} dt

use std::f64::consts::PI;
use std::sync::OnceLock;

const GL_N: usize = 20;

/// Nodes and weights of the n-point Gauss–Legendre rule on [-1, 1] (Newton on Pₙ).
fn gauss_legendre() -> &'static ([f64; GL_N], [f64; GL_N]) {
    static RULE: OnceLock<([f64; GL_N], [f64; GL_N])> = OnceLock::new();
    RULE.get_or_init(|| {
        let n = GL_N as f64;
        let mut nodes = [0.0; GL_N];
        let mut weights = [0.0; GL_N];
        for i in 1..=GL_N {
            let mut x = (PI * (i as f64 - 0.25) / (n + 0.5)).cos();
            let mut dp = 0.0;
            for _ in 0..100 {
                let (mut p0, mut p1) = (1.0, x);
                for k in 2..=GL_N {
                    let k = k as f64;
                    let p2 = ((2.0 * k - 1.0) * x * p1 - (k - 1.0) * p0) / k;
                    p0 = p1;
                    p1 = p2;
                }
                dp = n * (x * p1 - p0) / (x * x - 1.0);
                let dx = p1 / dp;
                x -= dx;
                if dx.abs() < 1e-16 {
                    break;
                }
            }
            nodes[i - 1] = x;
            weights[i - 1] = 2.0 / ((1.0 - x * x) * dp * dp);
        }
        (nodes, weights)
    })
}

fn integrate(f: impl Fn(f64) -> f64, a: f64, b: f64, panels: usize) -> f64 {
    let (nodes, weights) = gauss_legendre();
    let h = (b - a) / panels as f64;
    let mut total = 0.0;
    for p in 0..panels {
        let mid = a + (p as f64 + 0.5) * h;
        let s: f64 = nodes
            .iter()
            .zip(weights)
            .map(|(x, w)| w * f(mid + x * (h / 2.0)))
            .sum();
        total += s * h / 2.0;
    }
    total
}

/// (x/2)ⁿ / n!, exactly for the orders whose factorial is finite and in log space beyond (where
/// it underflows to 0 for any argument the series is used for), so huge orders cost nothing.
fn leading_term(h: f64, n: f64) -> f64 {
    if n <= 170.0 {
        let mut f = 1.0;
        let mut i = 2.0;
        while i <= n {
            f *= i;
            i += 1.0;
        }
        h.powf(n) / f
    } else {
        (n * h.ln() - super::special::log_gamma(n + 1.0)).exp()
    }
}

/// Σₖ (±1)ᵏ (x/2)^{2k+n} / (k! (n+k)!).
fn series(x: f64, n: f64, alternating: bool) -> f64 {
    let h = x / 2.0;
    let mut term = leading_term(h, n);
    let mut sum = term;
    let h2 = h * h;
    for k in 1..500 {
        let k = f64::from(k);
        term *= if alternating { -h2 } else { h2 } / (k * (n + k));
        sum += term;
        if term.abs() < 1e-17 * sum.abs() {
            break;
        }
    }
    sum
}

/// Quadrature panels for an argument and order (bounded so huge inputs stay fast).
fn panels_for(x: f64, n: f64) -> usize {
    let p = ((x.abs() + n) / 2.0).ceil() + 6.0;
    p.clamp(8.0, 200_000.0) as usize
}

fn odd(n: f64) -> bool {
    n % 2.0 != 0.0
}

/// Whether Hankel's asymptotic expansion is accurate to double precision for this argument and
/// order (its terms shrink until about k ≈ 2x, the smallest being near e^{−2x}·(4n²/8x)^k).
fn asymptotic_ok(x: f64, n: f64) -> bool {
    x >= 25.0 && x >= n * n
}

/// Hankel's expansion: (Jₙ(x), Yₙ(x)) = √(2/πx) (P cos χ − Q sin χ, P sin χ + Q cos χ) with
/// χ = x − (n/2 + 1/4)π. cos χ and sin χ are formed from cos x and sin x so that a large x
/// keeps its digits.
fn hankel(x: f64, n: f64) -> (f64, f64) {
    let mu = 4.0 * n * n;
    let (mut p, mut q) = (1.0f64, 0.0f64);
    let mut term = 1.0f64;
    let mut prev = f64::INFINITY;
    for k in 1..200 {
        let kf = f64::from(k);
        let odd = 2.0 * kf - 1.0;
        term *= (mu - odd * odd) / (kf * 8.0 * x);
        if term.abs() >= prev || term == 0.0 {
            break;
        }
        prev = term.abs();
        // a_k / x^k with the sign pattern (−1)^{k/2} on P (even k) and (−1)^{(k−1)/2} on Q.
        match k % 4 {
            1 => q += term,
            2 => p -= term,
            3 => q -= term,
            _ => p += term,
        }
        if term.abs() < 1e-17 {
            break;
        }
    }
    let phase = (n / 2.0 + 0.25) * PI;
    let (sx, cx) = x.sin_cos();
    let (sp, cp) = phase.sin_cos();
    let (cos_chi, sin_chi) = (cx * cp + sx * sp, sx * cp - cx * sp);
    let amp = (2.0 / (PI * x)).sqrt();
    (
        amp * (p * cos_chi - q * sin_chi),
        amp * (p * sin_chi + q * cos_chi),
    )
}

/// Jₙ(x).
pub(crate) fn bessel_j(x: f64, n: f64) -> f64 {
    if x < 0.0 {
        return if odd(n) { -1.0 } else { 1.0 } * bessel_j(-x, n);
    }
    if x == 0.0 {
        return if n == 0.0 { 1.0 } else { 0.0 };
    }
    if x * x / 4.0 < n + 1.0 {
        return series(x, n, true);
    }
    if asymptotic_ok(x, n) {
        return hankel(x, n).0;
    }
    integrate(|t| (n * t - x * t.sin()).cos(), 0.0, PI, panels_for(x, n)) / PI
}

/// Iₙ(x).
pub(crate) fn bessel_i(x: f64, n: f64) -> f64 {
    if x < 0.0 {
        return if odd(n) { -1.0 } else { 1.0 } * bessel_i(-x, n);
    }
    if x == 0.0 {
        return if n == 0.0 { 1.0 } else { 0.0 };
    }
    if x <= 30.0 + n {
        return series(x, n, false);
    }
    // Scaled integrand to avoid overflow: e^{x(cos τ − 1)} · e^x.
    let s = integrate(
        |t| (x * (t.cos() - 1.0)).exp() * (n * t).cos(),
        0.0,
        PI,
        panels_for(x, n),
    );
    s / PI * x.exp()
}

/// Kₙ(x) for x > 0.
pub(crate) fn bessel_k(x: f64, n: f64) -> f64 {
    // Integrand e^{-x(cosh t − 1)} cosh(nt) · e^{-x}; cut off where it is below 1e-18 of its peak.
    let mut t_end = 1.0f64;
    while x * (t_end.cosh() - 1.0) - n * t_end < 45.0 && t_end < 50.0 {
        t_end += 0.5;
    }
    let f = |t: f64| (-x * (t.cosh() - 1.0)).exp() * (n * t).cosh();
    let panels = (t_end * 8.0).ceil().max(16.0) as usize;
    integrate(f, 0.0, t_end, panels) * (-x).exp()
}

/// Yₙ(x) for x > 0.
pub(crate) fn bessel_y(x: f64, n: f64) -> f64 {
    if asymptotic_ok(x, n) {
        return hankel(x, n).1;
    }
    let first = integrate(|t| (x * t.sin() - n * t).sin(), 0.0, PI, panels_for(x, n)) / PI;
    let mut t_end = 1.0f64;
    while x * t_end.sinh() - n * t_end < 45.0 && t_end < 50.0 {
        t_end += 0.5;
    }
    let sign = if odd(n) { -1.0 } else { 1.0 };
    let g = |t: f64| ((n * t).exp() + sign * (-n * t).exp()) * (-x * t.sinh()).exp();
    // The integrand changes fastest near 0 for small x: use more panels there.
    let second = integrate(g, 0.0, t_end.min(4.0), 64)
        + if t_end > 4.0 {
            integrate(g, 4.0, t_end, ((t_end - 4.0) * 8.0).ceil() as usize)
        } else {
            0.0
        };
    first - second / PI
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Microsoft's examples, against the exact values (Excel's own approximations print
    /// 0.329925829 for `BESSELJ(1.9,2)` and 0.277387804 for `BESSELK(1.5,1)`; see
    /// docs/formula-deviations.md).
    #[test]
    fn microsoft_examples_to_full_precision() {
        let cases = [
            (bessel_i(1.5, 1.0), 0.981_666_428_577_907_5),
            (bessel_j(1.9, 2.0), 0.329_925_727_692_387_2),
            (bessel_k(1.5, 1.0), 0.277_387_800_456_844),
            (bessel_y(2.5, 1.0), 0.145_918_137_966_786),
        ];
        for (got, want) in cases {
            assert!((got - want).abs() < 1e-12, "{got} vs {want}");
        }
    }

    #[test]
    fn hankel_agrees_with_the_quadrature_and_known_values() {
        // At the switch the two methods agree.
        for (x, n) in [
            (25.0, 0.0),
            (25.0, 1.0),
            (30.0, 3.0),
            (49.0, 7.0),
            (100.0, 10.0),
        ] {
            let q_j = integrate(|t| (n * t - x * t.sin()).cos(), 0.0, PI, panels_for(x, n)) / PI;
            let (h_j, h_y) = hankel(x, n);
            assert!((q_j - h_j).abs() < 1e-14, "J{n}({x}): {q_j} vs {h_j}");
            let first = integrate(|t| (x * t.sin() - n * t).sin(), 0.0, PI, panels_for(x, n)) / PI;
            assert!(h_y.is_finite() && first.is_finite());
        }
        // J0(100), Y0(100), J1(1000) from tables (A&S, DLMF).
        assert!((bessel_j(100.0, 0.0) - 0.019_985_850_304_223_12).abs() < 1e-15);
        assert!((bessel_y(100.0, 0.0) - (-0.077_244_313_365_083_15)).abs() < 1e-15);
        assert!((bessel_j(1000.0, 1.0) - 0.004_728_311_907_089_523).abs() < 1e-15);
        // A large argument is fast and stays bounded by the envelope √(2/πx).
        let big = bessel_j(2_958_466.0, 1.0);
        assert!(big.abs() <= (2.0 / (PI * 2_958_466.0)).sqrt() * 1.000_001);
    }

    #[test]
    fn huge_orders_are_fast_and_finite_or_overflow() {
        assert_eq!(bessel_j(1.0, 1e15), 0.0);
        assert_eq!(bessel_i(2.0, 1e15), 0.0);
        assert!(!bessel_k(2.0, 1e15).is_finite());
        assert!(bessel_j(3.0, 171.0) >= 0.0);
        // Continuity across the switch to log space.
        let (a, b) = (
            leading_term(50.0, 170.0),
            leading_term(50.0, 171.0) * 171.0 / 50.0,
        );
        assert!((a - b).abs() <= 1e-9 * a, "{a} vs {b}");
    }
}
