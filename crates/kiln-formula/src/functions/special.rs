// SPDX-License-Identifier: AGPL-3.0-or-later
//! Special functions and probability distributions to double precision (about 1e-13 relative or
//! better at any parameter size), so the statistical functions agree with Excel and LibreOffice
//! well inside the conformance tolerance and stay right where series stop converging.
//!
//! The gamma function and its logarithm come from `statrs` (Lanczos). Everything else is here:
//!
//! - `erf`/`erfc`: a positive-term series and Legendre's continued fraction, with `e^(−x²)` formed
//!   from an exact split of `x` (`statrs`' rational approximations are off by up to 1e-10).
//! - The regularised incomplete gamma and beta functions: power series and modified-Lentz
//!   continued fractions run to convergence for small and moderate shapes (the beta fraction in
//!   its even contraction, written in `λ = a − (a+b)x` and `1 − x`, so tails within an ulp of 1
//!   keep their digits); Temme's uniform asymptotic expansions for large shapes near the mean,
//!   where the series would need O(√a) terms; prefactors through Stirling's series and
//!   `ln(1+u) − u`. A method that does not converge returns NaN (`#NUM!`), never a partial sum.
//! - The normal quantile is Acklam's rational approximation refined with Halley steps; other
//!   quantiles use safeguarded bisection, which propagates a NaN.

use statrs::function::gamma as sgamma;

const EPS: f64 = 1e-16;
const FPMIN: f64 = 1e-300;
const LN_SQRT_2PI: f64 = 0.918_938_533_204_672_8;
const MAX_ITER: usize = 10_000;

/// `n!` for integers 0…170 (infinity above).
pub(crate) fn factorial(n: f64) -> f64 {
    if n < 0.0 {
        return f64::NAN;
    }
    if n > 170.0 {
        return f64::INFINITY;
    }
    let mut r = 1.0;
    let mut i = 2.0;
    while i <= n.floor() {
        r *= i;
        i += 1.0;
    }
    r
}

/// ln|Γ(x)|.
pub(crate) fn log_gamma(x: f64) -> f64 {
    if x.fract() == 0.0 && x > 0.0 && x <= 171.0 {
        return factorial(x - 1.0).ln();
    }
    if x < 0.5 {
        return (std::f64::consts::PI / (std::f64::consts::PI * x).sin().abs()).ln()
            - log_gamma(1.0 - x);
    }
    sgamma::ln_gamma(x)
}

/// Γ(x); NaN at the poles.
pub(crate) fn gamma(x: f64) -> f64 {
    if x.fract() == 0.0 {
        if x <= 0.0 {
            return f64::NAN;
        }
        return factorial(x - 1.0);
    }
    if x > 171.7 {
        return f64::INFINITY;
    }
    sgamma::gamma(x)
}

/// ln B(a, b).
pub(crate) fn log_beta(a: f64, b: f64) -> f64 {
    log_gamma(a) + log_gamma(b) - log_gamma(a + b)
}

/// `ln(1 + u) − u`, without the cancellation of the direct form for small `|u|`.
pub(crate) fn log1pmx(u: f64) -> f64 {
    if u.is_nan() || u.abs() > 0.5 {
        return u.ln_1p() - u;
    }
    // ln(1 + u) = 2 atanh(w) with w = u / (2 + u), and 2w − u = −u·w.
    let w = u / (2.0 + u);
    let w2 = w * w;
    let mut power = w;
    let mut sum = 0.0;
    let mut k = 3.0;
    while k < 80.0 {
        power *= w2;
        let term = power / k;
        sum += term;
        if term.abs() <= sum.abs() * 1e-17 {
            break;
        }
        k += 2.0;
    }
    2.0 * sum - u * w
}

/// `ln(ρ) − (ρ − 1)` given both `ρ` and `u = ρ − 1`, each formed directly by the caller. Near
/// `u = −1` (a gamma or beta tail at a tiny fraction of its mean) `1 + u` keeps only the digits
/// `u` has left above the spacing of doubles near 1, so `ln(1 + u)` is taken from `ρ` instead: at
/// `x/a = 10⁻¹⁵` it would otherwise be wrong in the second digit.
pub(crate) fn log_ratio_m1(rho: f64, u: f64) -> f64 {
    if u.abs() <= 0.5 {
        log1pmx(u)
    } else {
        rho.ln() - u
    }
}

/// The error of Stirling's approximation, `δ(z) = ln Γ(z) − ((z − ½) ln z − z + ½ ln 2π)`.
pub(crate) fn stirling_err(z: f64) -> f64 {
    if z >= 10.0 {
        let r = 1.0 / z;
        let r2 = r * r;
        r * (1.0 / 12.0
            - r2 * (1.0 / 360.0
                - r2 * (1.0 / 1260.0
                    - r2 * (1.0 / 1680.0
                        - r2 * (1.0 / 1188.0 - r2 * (691.0 / 360_360.0 - r2 / 156.0))))))
    } else {
        log_gamma(z) - (z - 0.5) * z.ln() + z - LN_SQRT_2PI
    }
}

/// Clamps a probability that rounding may have pushed just outside `[0, 1]`; NaN passes through.
pub(crate) fn prob(p: f64) -> f64 {
    if p.is_nan() { p } else { p.clamp(0.0, 1.0) }
}

/// Regularised lower incomplete gamma P(a, x); NaN where no method converges.
pub(crate) fn gamma_p(a: f64, x: f64) -> f64 {
    gamma_pq(a, x).0
}

/// Regularised upper incomplete gamma Q(a, x) = 1 − P(a, x), accurate in the tail.
pub(crate) fn gamma_q(a: f64, x: f64) -> f64 {
    gamma_pq(a, x).1
}

/// Shape parameters from which the uniform asymptotic expansion is tried.
const UNIFORM_MIN: f64 = 100.0;

/// `(P(a, x), Q(a, x))`, each computed directly in its own tail.
///
/// Small shapes use the power series below the mean and the continued fraction above it, both
/// run to convergence. Large shapes near the mean, where those need O(√a) terms (10⁸ terms at
/// a = 10¹⁵), use Temme's uniform asymptotic expansion instead; it is accurate to double
/// precision there and every evaluation checks its own convergence.
fn gamma_pq(a: f64, x: f64) -> (f64, f64) {
    if a.is_nan() || x.is_nan() || a <= 0.0 {
        return (f64::NAN, f64::NAN);
    }
    if x <= 0.0 {
        return (0.0, 1.0);
    }
    if x.is_infinite() {
        return (1.0, 0.0);
    }
    if a >= UNIFORM_MIN
        && ((x - a) / a).abs() < 1.0
        && let Some(pq) = gamma_uniform(a, x)
    {
        return pq;
    }
    if x < a + 1.0 {
        let p = prob(gamma_series(a, x));
        (p, 1.0 - p)
    } else {
        // Q ≤ xᵃe⁻ˣ/Γ(a) · 1/(x + 1 − a) here: below e⁻⁸⁰⁰ it is zero in double precision (and
        // the continued fraction would overflow near x = 10³⁰⁸).
        if gamma_prefix_ln(a, x) < -800.0 {
            return (1.0, 0.0);
        }
        let q = prob(gamma_cf(a, x));
        (1.0 - q, q)
    }
}

/// `ln(xᵃ e⁻ˣ / Γ(a))`, through Stirling's series for large `a` so the huge terms cancel exactly.
fn gamma_prefix_ln(a: f64, x: f64) -> f64 {
    if a < 10.0 {
        a * x.ln() - x - log_gamma(a)
    } else {
        a * log_ratio_m1(x / a, (x - a) / a) + 0.5 * (a / (2.0 * std::f64::consts::PI)).ln()
            - stirling_err(a)
    }
}

fn gamma_series(a: f64, x: f64) -> f64 {
    let mut ap = a;
    let mut sum = 1.0 / a;
    let mut del = sum;
    for _ in 0..MAX_ITER {
        ap += 1.0;
        del *= x / ap;
        sum += del;
        if del.abs() < sum.abs() * EPS {
            return sum * gamma_prefix_ln(a, x).exp();
        }
    }
    f64::NAN
}

fn gamma_cf(a: f64, x: f64) -> f64 {
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / FPMIN;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..MAX_ITER {
        let i = i as f64;
        let an = -i * (i - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = b + an / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            return gamma_prefix_ln(a, x).exp() * h;
        }
    }
    f64::NAN
}

/// Temme's expansion `Q(a, x) = ½ erfc(η √(a/2)) + e^(−aη²/2) / (√(2πa) Γ*(a)) · Σ gₖ(η) a⁻ᵏ`
/// with `η²/2 = λ − 1 − ln λ`, `λ = x/a`, `Γ*(a) = e^δ(a)` (DLMF 8.12.8; the gₖ absorb DLMF's
/// 1/Γ* factor).
fn gamma_uniform(a: f64, x: f64) -> Option<(f64, f64)> {
    static SERIES: std::sync::OnceLock<Uniform> = std::sync::OnceLock::new();
    let mu = (x - a) / a;
    let e = log_ratio_m1(x / a, mu); // −η²/2
    let eta = (-2.0 * e).max(0.0).sqrt().copysign(mu);
    let s = SERIES
        .get_or_init(|| Uniform::new(1.0, 1.0, 0.0, 1.0))
        .sum(eta, a)?;
    let corr = (a * e - stirling_err(a)).exp() / (2.0 * std::f64::consts::PI * a).sqrt() * s;
    let z = eta * (a / 2.0).sqrt();
    Some((prob(0.5 * erfc(-z) - corr), prob(0.5 * erfc(z) + corr)))
}

/// The coefficient functions gₖ(ζ) of a uniform asymptotic expansion around a Gaussian saddle,
/// as power series in ζ.
///
/// The integrand's natural variable `s` (`s = λ − 1` for the incomplete gamma function,
/// `s = t − x₀` for the incomplete beta function) is linked to the Gaussian variable ζ by the
/// differential equation `s·s′ = ζ (c₀ + c₁ s + c₂ s²)`, solved term by term. With
/// `f(ζ) = s₁ ζ / s(ζ)` (so `f(0) = 1`), integration by parts gives
/// `√(r/2π) ∫ e^(−rζ²/2) f(ζ) dζ = Σ Aₖ r⁻ᵏ · Φ − e^(−rη²/2)/√(2πr) · Σ gₖ(η) r⁻ᵏ` with
/// `g₀ = (f − 1)/ζ` and `gₖ₊₁ = (gₖ′ − gₖ′(0))/ζ`.
///
/// The series are stored in the scaled variable `ξ = ζ/κ`, where `κ²` is the natural width of the
/// problem (`min(x₀, y₀)` for the beta function, so its radius of convergence in ξ stays near
/// 2√π however skewed `a : b` is and no coefficient overflows): `gₖ(ζ) = κ^(−2k−1) Gₖ(ζ/κ)`.
struct Uniform {
    g: Vec<Vec<f64>>,
    kappa: f64,
}

impl Uniform {
    const TERMS: usize = 48;
    const LEVELS: usize = 7;

    fn new(c0: f64, c1: f64, c2: f64, kappa2: f64) -> Self {
        let n = Self::TERMS;
        // Sₘ = sₘ κᵐ satisfy the recurrence of the sₘ with c₁, c₂ scaled by κ².
        let mut s = vec![0.0; n + 1];
        s[1] = (c0 * kappa2).sqrt();
        for m in 2..=n {
            let mut rhs = kappa2 * c1 * s[m - 1];
            for i in 1..m - 1 {
                rhs += kappa2 * c2 * s[i] * s[m - 1 - i];
            }
            for i in 2..m {
                let j = m + 1 - i;
                if j >= 2 {
                    rhs -= j as f64 * s[i] * s[j];
                }
            }
            s[m] = rhs / ((m + 1) as f64 * s[1]);
        }
        // h = s / (s₁ ζ) = 1 + h₁ ζ + …; f = 1/h.
        let h: Vec<f64> = (1..=n).map(|k| s[k] / s[1]).collect();
        let mut f = vec![0.0; n];
        f[0] = 1.0;
        for m in 1..n {
            f[m] = -(1..=m).map(|k| h[k] * f[m - k]).sum::<f64>();
        }
        let mut g = vec![f[1..].to_vec()];
        for _ in 1..Self::LEVELS {
            let prev = g.last().map_or(&[][..], Vec::as_slice);
            let next: Vec<f64> = (2..prev.len()).map(|j| j as f64 * prev[j]).collect();
            g.push(next);
        }
        Self {
            g,
            kappa: kappa2.sqrt(),
        }
    }

    /// `Σ gₖ(η) r⁻ᵏ`, or `None` when a power series or the asymptotic sum has not converged to
    /// double precision at this `η` and `r`.
    fn sum(&self, eta: f64, r: f64) -> Option<f64> {
        let k = self.kappa;
        let (eta, r) = (eta / k, r * k * k);
        let mut total = 0.0;
        let mut scale = 1.0 / k;
        let mut last = 0.0;
        for coeffs in &self.g {
            let (mut v, mut p, mut biggest) = (0.0, 1.0, 0.0f64);
            for c in coeffs {
                let t = c * p;
                v += t;
                biggest = biggest.max(t.abs());
                p *= eta;
            }
            let n = coeffs.len();
            let tail = (coeffs[n - 1] * eta.powi(n as i32 - 1)).abs()
                + (coeffs[n - 2] * eta.powi(n as i32 - 2)).abs();
            if tail.is_nan() || tail > 1e-16 * biggest.max(1e-300) {
                return None;
            }
            last = v * scale;
            total += last;
            scale /= r;
        }
        (total.is_finite() && last.abs() <= 1e-17 * (total * k).abs().max(1.0) / k).then_some(total)
    }
}

/// Regularised incomplete beta I_x(a, b) given both `x` and `y = 1 − x`, so callers that know `y`
/// more precisely than `1 − x` (the t distribution's `t²/(ν + t²)`) keep its digits. NaN where no
/// method converges.
pub(crate) fn beta_ixy(x: f64, y: f64, a: f64, b: f64) -> f64 {
    if x.is_nan() || y.is_nan() || a.is_nan() || b.is_nan() || a <= 0.0 || b <= 0.0 {
        return f64::NAN;
    }
    if x <= 0.0 {
        return 0.0;
    }
    if y <= 0.0 {
        return 1.0;
    }
    if a.min(b) >= UNIFORM_MIN
        && let Some(i) = beta_uniform(x, y, a, b)
    {
        return i;
    }
    let ln_bt = beta_prefix_ln(x, y, a, b);
    // λ = a − (a+b)x = (a+b)y − b, formed from whichever of x and y is the smaller.
    let r = a + b;
    let lambda = if x <= y {
        -x.mul_add(r, -a)
    } else {
        y.mul_add(r, -b)
    };
    // Below the mean the continued fraction in x converges fast, above it the one in y; the
    // test is made in whichever of x and y is the smaller.
    let below = if x <= y {
        x < (a + 1.0) / (r + 2.0)
    } else {
        y > (b + 1.0) / (r + 2.0)
    };
    // The continued fraction is at most O(a + b): far tails are exactly 0 in double precision.
    if ln_bt + r.ln() < -760.0 {
        return if below { 0.0 } else { 1.0 };
    }
    if below {
        prob(ln_bt.exp() * beta_cf(x, y, a, b, lambda) / a)
    } else {
        prob(1.0 - ln_bt.exp() * beta_cf(y, x, b, a, -lambda) / b)
    }
}

/// `(u, v)` with `x = x₀(1 + u)`, `y = y₀(1 + v)`, `x₀ = a/(a+b)`; `a·u + b·v = 0`. The one
/// computed from the smaller of `x`, `y` keeps its digits, and `x(a+b) − a` is formed with a
/// single rounding (a fused multiply-add): near the mean of `BINOM.DIST(5E14,1E15,0.5,TRUE)` it
/// is 0.5 against terms of 5·10¹⁴.
fn beta_uv(x: f64, y: f64, a: f64, b: f64) -> (f64, f64) {
    let r = a + b;
    if x <= y {
        let u = x.mul_add(r, -a) / a;
        (u, -(a / b) * u)
    } else {
        let v = y.mul_add(r, -b) / b;
        (-(b / a) * v, v)
    }
}

/// `ln(xᵃ yᵇ / B(a, b))`; for large parameters through Stirling's series and `ln(1+u) − u`, so
/// the terms of size a·ln x cancel exactly.
fn beta_prefix_ln(x: f64, y: f64, a: f64, b: f64) -> f64 {
    let r = a + b;
    if r < 20.0 {
        return a * x.ln() + b * y.ln() - log_beta(a, b);
    }
    let (u, v) = beta_uv(x, y, a, b);
    a * log_ratio_m1(x * r / a, u)
        + b * log_ratio_m1(y * r / b, v)
        + 0.5 * (a.ln() + b.ln() - r.ln() - (2.0 * std::f64::consts::PI).ln())
        + stirling_err(r)
        - stirling_err(a)
        - stirling_err(b)
}

/// Temme's uniform expansion for large `a` and `b` (DLMF 8.18.9 form):
/// `I_x(a, b) = ½ erfc(−η √(r/2)) − e^(δ(r) − δ(a) − δ(b)) e^(−rη²/2) / √(2πr) · Σ gₖ(η) r⁻ᵏ`
/// with `r = a + b`, `−η²/2 = x₀ ln(x/x₀) + y₀ ln(y/y₀)`, sign(η) = sign(x − x₀).
fn beta_uniform(x: f64, y: f64, a: f64, b: f64) -> Option<f64> {
    let r = a + b;
    let (x0, y0) = (a / r, b / r);
    let (u, v) = beta_uv(x, y, a, b);
    let e = a * log_ratio_m1(x / x0, u) + b * log_ratio_m1(y / y0, v); // −rη²/2
    let eta = (-2.0 * e / r).max(0.0).sqrt().copysign(u);
    let s = Uniform::new(x0 * y0, y0 - x0, -1.0, x0.min(y0)).sum(eta, r)?;
    let corr = (e + stirling_err(r) - stirling_err(a) - stirling_err(b)).exp()
        / (2.0 * std::f64::consts::PI * r).sqrt()
        * s;
    let z = eta * (r / 2.0).sqrt();
    Some(prob(0.5 * erfc(-z) - corr))
}

/// The continued fraction of `I_x(a, b) = xᵃ yᵇ / (a B(a, b)) · h` (DLMF 8.17.22) in its even
/// contraction, `h = 1/(D₀ + N₁/(D₁ + N₂/(D₂ + …)))` with `Dₘ = 1 + d₂ₘ + d₂ₘ₊₁` and
/// `Nₘ = −d₂ₘ₋₁ d₂ₘ`. The denominators are written in `λ = a − (a+b)x` and `y`, so the
/// cancellation of `1 − (a+b)x/(a+1)` near the mean costs no digits even when `x` is within an
/// ulp of 1 (TOMS 708's BFRAC does the same). Run to convergence; NaN if it does not converge.
fn beta_cf(x: f64, y: f64, a: f64, b: f64, lambda: f64) -> f64 {
    let d_odd = |m: f64| -(a + m) * (a + b + m) * x / ((a + 2.0 * m) * (a + 2.0 * m + 1.0));
    let d_even = |m: f64| m * (b - m) * x / ((a + 2.0 * m - 1.0) * (a + 2.0 * m));
    let tiny = |v: f64| if v.abs() < FPMIN { FPMIN } else { v };
    let mut f = tiny((lambda + 1.0) / (a + 1.0));
    let mut c = f;
    let mut d = 0.0;
    for m in 1..=MAX_ITER {
        let m = m as f64;
        let m2 = 2.0 * m;
        let num = -d_odd(m - 1.0) * d_even(m);
        // 1 + d₂ₘ + d₂ₘ₊₁ over a common denominator, grouped so that no term is a difference of
        // two large numbers that λ or x already holds exactly.
        let den = (a * lambda + m2 * (a + m) * (1.0 + y) + (a + b) * x - 1.0)
            / ((a + m2 - 1.0) * (a + m2 + 1.0));
        d = tiny(den + num * d);
        c = tiny(den + num / c);
        d = 1.0 / d;
        let del = c * d;
        f *= del;
        if (del - 1.0).abs() < EPS {
            return 1.0 / f;
        }
    }
    f64::NAN
}

/// The error function, to about 1e-16 relative.
pub(crate) fn erf(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let ax = x.abs();
    let v = if ax < ERF_SWITCH {
        erf_series(ax)
    } else {
        1.0 - erfc_cf(ax)
    };
    v.copysign(x)
}

/// The complementary error function, to about 1e-15 relative also in the far tail.
pub(crate) fn erfc(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x < 0.0 {
        return 2.0 - erfc(-x);
    }
    if x < ERF_SWITCH {
        1.0 - erf_series(x)
    } else {
        erfc_cf(x)
    }
}

/// Below this, `erf` comes from its series and `erfc = 1 − erf` loses under four bits.
const ERF_SWITCH: f64 = 1.25;

/// `erf(x) = 2/√π · e^(−x²) · Σ (2x²)ⁿ x / (1·3·…·(2n+1))`, all terms positive (0 ≤ x < 1.25).
fn erf_series(x: f64) -> f64 {
    let x2 = 2.0 * x * x;
    let mut term = x;
    let mut sum = x;
    let mut k = 1.0;
    while k < 200.0 {
        k += 2.0;
        term *= x2 / k;
        sum += term;
        if term <= sum * 1e-17 {
            break;
        }
    }
    std::f64::consts::FRAC_2_SQRT_PI * exp_neg_sq(x) * sum
}

/// `erfc(x) = Q(½, x²)` by Legendre's continued fraction (modified Lentz), x ≥ 1.25.
fn erfc_cf(x: f64) -> f64 {
    if x > 27.3 {
        return 0.0;
    }
    erfc_cf_scaled(x, exp_neg_sq(x))
}

/// [`erfc_cf`] with `e^(−x²)` supplied, for callers that can form it more precisely than from
/// the rounded `x` (the normal distribution, whose `x = z/√2`).
fn erfc_cf_scaled(x: f64, exp_neg_x2: f64) -> f64 {
    let xx = x * x;
    let mut b = xx + 0.5;
    let mut c = 1.0 / FPMIN;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..1000 {
        let i = f64::from(i);
        let an = -i * (i - 0.5);
        b += 2.0;
        d = an * d + b;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = b + an / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            break;
        }
    }
    exp_neg_x2 * x / std::f64::consts::PI.sqrt() * h
}

/// `e^(−x²)` without the rounding error of forming `x²`, which the exponential would amplify by
/// a factor x²: `x = hi + lo` with `hi²` exact.
fn exp_neg_sq(x: f64) -> f64 {
    let (hi, lo) = split(x);
    (-hi * hi).exp() * (-(2.0 * hi + lo) * lo).exp()
}

/// `e^(−z²/2)`, likewise.
fn exp_neg_half_sq(z: f64) -> f64 {
    let (hi, lo) = split(z);
    (-0.5 * hi * hi).exp() * (-0.5 * (2.0 * hi + lo) * lo).exp()
}

/// `x = hi + lo` with `hi` carrying 26 significant bits, so `hi²` is exact.
fn split(x: f64) -> (f64, f64) {
    let hi = f64::from_bits(x.to_bits() & !((1u64 << 27) - 1));
    (hi, x - hi)
}

// ── normal ─────────────────────────────────────────────────────────────────────────────────

pub(crate) fn norm_pdf(z: f64) -> f64 {
    (-0.5 * z * z - LN_SQRT_2PI).exp()
}

pub(crate) fn norm_cdf(z: f64) -> f64 {
    norm_cdf_upper(-z)
}

/// `1 − Φ(z) = ½ erfc(z/√2)`; in the upper tail `e^(−z²/2)` is formed from `z` itself, as the
/// rounding of `z/√2` would cost up to z²·2⁻⁵³ relative (7·10⁻¹⁵ at z = 8).
fn norm_cdf_upper(z: f64) -> f64 {
    let x = z / std::f64::consts::SQRT_2;
    if x >= ERF_SWITCH {
        if x > 27.3 {
            return 0.0;
        }
        0.5 * erfc_cf_scaled(x, exp_neg_half_sq(z))
    } else {
        0.5 * erfc(x)
    }
}

fn horner(coeffs: &[f64], x: f64) -> f64 {
    coeffs.iter().fold(0.0, |r, k| r * x + k)
}

/// The standard normal quantile.
pub(crate) fn norm_inv(p: f64) -> f64 {
    if !(p > 0.0 && p < 1.0) {
        return f64::NAN;
    }
    const A: [f64; 6] = [
        -39.696_830_286_653_76,
        220.946_098_424_520_5,
        -275.928_510_446_968_7,
        138.357_751_867_269,
        -30.664_798_066_147_16,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -54.476_098_798_224_06,
        161.585_836_858_040_9,
        -155.698_979_859_886_6,
        66.801_311_887_719_72,
        -13.280_681_552_885_72,
    ];
    const C: [f64; 6] = [
        -0.007_784_894_002_430_293,
        -0.322_396_458_041_136_5,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        0.007_784_695_709_041_462,
        0.322_467_129_070_039_8,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    const PL: f64 = 0.02425;
    let mut x = if p < PL {
        let q = (-2.0 * p.ln()).sqrt();
        horner(&C, q) / (horner(&D, q) * q + 1.0)
    } else if p <= 1.0 - PL {
        let q = p - 0.5;
        let r = q * q;
        horner(&A, r) * q / (horner(&B, r) * r + 1.0)
    } else {
        let q = (-2.0 * (-p).ln_1p()).sqrt();
        -horner(&C, q) / (horner(&D, q) * q + 1.0)
    };
    // Halley refinement: two steps reach full double precision.
    for _ in 0..2 {
        let e = if p < 0.5 {
            norm_cdf(x) - p
        } else {
            -(norm_cdf_upper(x) - (1.0 - p))
        };
        let u = e * (2.0 * std::f64::consts::PI).sqrt() * (x * x / 2.0).exp();
        let step = u / (1.0 + x * u / 2.0);
        // Past z ≈ 37.6 (subnormal p) the density underflows; the rational start is already
        // accurate to 10⁻⁹ there, which is more than a subnormal p carries.
        if !step.is_finite() {
            break;
        }
        x -= step;
    }
    x
}

// ── other continuous distributions ─────────────────────────────────────────────────────────

pub(crate) fn chisq_cdf(x: f64, df: f64) -> f64 {
    if x <= 0.0 {
        0.0
    } else {
        gamma_p(df / 2.0, x / 2.0)
    }
}

pub(crate) fn chisq_pdf(x: f64, df: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    if x == 0.0 {
        return if df == 2.0 {
            0.5
        } else if df < 2.0 {
            f64::INFINITY
        } else {
            0.0
        };
    }
    let k = df / 2.0;
    if k >= 10.0 {
        return gamma_pdf(x, k, 2.0);
    }
    ((k - 1.0) * x.ln() - x / 2.0 - k * std::f64::consts::LN_2 - log_gamma(k)).exp()
}

pub(crate) fn gamma_cdf(x: f64, alpha: f64, beta: f64) -> f64 {
    if x <= 0.0 {
        0.0
    } else {
        gamma_p(alpha, x / beta)
    }
}

pub(crate) fn gamma_pdf(x: f64, alpha: f64, beta: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    if x == 0.0 {
        return if alpha == 1.0 {
            1.0 / beta
        } else if alpha < 1.0 {
            f64::INFINITY
        } else {
            0.0
        };
    }
    if alpha >= 10.0 {
        // (x/β)^α e^(−x/β) / Γ(α) / x, with the large terms cancelled analytically.
        return gamma_prefix_ln(alpha, x / beta).exp() / x;
    }
    ((alpha - 1.0) * x.ln() - x / beta - log_gamma(alpha) - alpha * beta.ln()).exp()
}

pub(crate) fn beta_pdf(x: f64, a: f64, b: f64) -> f64 {
    if !(0.0..=1.0).contains(&x) {
        return 0.0;
    }
    if x == 0.0 {
        return if a == 1.0 {
            b
        } else if a < 1.0 {
            f64::INFINITY
        } else {
            0.0
        };
    }
    if x == 1.0 {
        return if b == 1.0 {
            a
        } else if b < 1.0 {
            f64::INFINITY
        } else {
            0.0
        };
    }
    if a + b >= 20.0 {
        let y = 1.0 - x;
        return beta_prefix_ln(x, y, a, b).exp() / (x * y);
    }
    ((a - 1.0) * x.ln() + (b - 1.0) * (-x).ln_1p() - log_beta(a, b)).exp()
}

/// Student's t CDF.
pub(crate) fn t_cdf(t: f64, df: f64) -> f64 {
    if t.is_nan() || df.is_nan() {
        return f64::NAN;
    }
    // x = ν/(ν + t²) and 1 − x = t²/(ν + t²), formed without cancellation (for ν = 10¹⁵ and
    // t = 0.5, 1 − x is 2.5·10⁻¹⁶, below the spacing of doubles near 1).
    let z = t / df.sqrt();
    let z2 = z * z;
    let tail = if z2 > 1e30 {
        // Far tail, x = 1/(1 + z²) < 10⁻³⁰: I_x(a, ½) = xᵃ/(a B(a, ½))·(1 + O(x)), in logs as z²
        // overflows past |z| = 1.3·10¹⁵⁴ (T.INV.2T(1E-300, 1) is 6.4·10²⁹⁹).
        let a = df / 2.0;
        let ln_x = -2.0 * z.abs().ln() - (1.0 / z2).ln_1p();
        0.5 * (a * ln_x - a.ln() - log_beta(a, 0.5)).exp()
    } else {
        0.5 * beta_ixy(1.0 / (1.0 + z2), z2 / (1.0 + z2), df / 2.0, 0.5)
    };
    prob(if t >= 0.0 { 1.0 - tail } else { tail })
}

pub(crate) fn t_pdf(t: f64, df: f64) -> f64 {
    if df >= 20.0 {
        // ln Γ(z + ½) − ln Γ(z) = ½ ln z + z·(ln(1 + 1/2z) − 1/2z) + δ(z + ½) − δ(z), z = ν/2.
        let z = df / 2.0;
        let ratio = 0.5 * z.ln() + z * log1pmx(0.5 / z) + stirling_err(z + 0.5) - stirling_err(z);
        return (ratio
            - 0.5 * (df * std::f64::consts::PI).ln()
            - (df + 1.0) / 2.0 * (t * t / df).ln_1p())
        .exp();
    }
    (log_gamma((df + 1.0) / 2.0)
        - log_gamma(df / 2.0)
        - 0.5 * (df * std::f64::consts::PI).ln()
        - (df + 1.0) / 2.0 * (t * t / df).ln_1p())
    .exp()
}

pub(crate) fn f_cdf(x: f64, d1: f64, d2: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let (p, q) = f_xy(x, d1, d2);
    beta_ixy(p, q, d1 / 2.0, d2 / 2.0)
}

/// The F distribution's upper tail, `I_y(d2/2, d1/2)`.
pub(crate) fn f_sf(x: f64, d1: f64, d2: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    let (p, q) = f_xy(x, d1, d2);
    beta_ixy(q, p, d2 / 2.0, d1 / 2.0)
}

/// `(d1·x / (d1·x + d2), d2 / (d1·x + d2))`, finite for huge `x` and degrees of freedom.
fn f_xy(x: f64, d1: f64, d2: f64) -> (f64, f64) {
    let w = d1 / d2 * x;
    if w.is_infinite() {
        // 1/(1 + w) = (d2/d1)/x to double precision; it may still be a (sub)normal number.
        return (1.0, d2 / d1 / x);
    }
    (w / (1.0 + w), 1.0 / (1.0 + w))
}

pub(crate) fn f_pdf(x: f64, d1: f64, d2: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    if x == 0.0 {
        return if d1 == 2.0 {
            1.0
        } else if d1 < 2.0 {
            f64::INFINITY
        } else {
            0.0
        };
    }
    if d1 + d2 >= 40.0 {
        // The beta density of p = d1·x/(d1·x + d2) times dp/dx = p(1 − p)/x.
        let (p, q) = f_xy(x, d1, d2);
        return beta_prefix_ln(p, q, d1 / 2.0, d2 / 2.0).exp() / x;
    }
    // ln(d1·x) and ln(d1·x + d2) from the logarithms when d1·x overflows.
    let dx = d1 * x;
    let (ln_dx, ln_sum) = if dx.is_finite() {
        (dx.ln(), (dx + d2).ln())
    } else {
        let l = d1.ln() + x.ln();
        (l, l + (d2 / d1 / x).ln_1p())
    };
    (0.5 * (d1 * ln_dx + d2 * d2.ln() - (d1 + d2) * ln_sum) - x.ln() - log_beta(d1 / 2.0, d2 / 2.0))
        .exp()
}

/// Inverts a continuous increasing CDF over non-negative arguments: the double `x` in `[lo, hi]`
/// (`[lo, f64::MAX]` with `unbounded`, when `hi` does not reach `p`) whose CDF is nearest `p`;
/// `lo` when the CDF there already reaches `p`, `hi` when a bounded search never does.
///
/// The search bisects the bit patterns of the bracket rather than its values. Non-negative
/// doubles order like their bits, so every step halves the number of doubles left and at most 64
/// steps reach two adjacent doubles, whatever the scale: quantiles of 10⁻³⁰⁰ (gamma with a tiny
/// shape) and 10¹⁷ (Student's t with one degree of freedom far in the tail) come out to full
/// precision in the same 64 evaluations. NaN when the CDF fails or never reaches `p`.
pub(crate) fn invert_cdf(
    cdf: impl Fn(f64) -> f64,
    p: f64,
    lo: f64,
    hi: f64,
    unbounded: bool,
) -> f64 {
    if !(lo >= 0.0 && hi > lo) || p.is_nan() {
        return f64::NAN;
    }
    let (c_lo, mut c_hi) = (cdf(lo), cdf(hi));
    if c_lo.is_nan() || c_hi.is_nan() {
        return f64::NAN;
    }
    if c_lo >= p {
        return lo;
    }
    let (mut a, mut b) = (lo, hi);
    if c_hi < p {
        if !unbounded {
            // A bounded search ends where its caller knows the CDF reaches p (rounding may leave
            // it a few ulps short there).
            return hi;
        }
        c_hi = cdf(f64::MAX);
        if c_hi.is_nan() || c_hi < p {
            return f64::NAN;
        }
        (a, b) = (hi, f64::MAX);
    }
    // Invariant: cdf(a) < p <= cdf(b).
    let (mut ia, mut ib) = (a.to_bits(), b.to_bits());
    let mut c_a = f64::NEG_INFINITY;
    while ib - ia > 1 {
        let im = ia + (ib - ia) / 2;
        let c = cdf(f64::from_bits(im));
        if c.is_nan() {
            return f64::NAN;
        }
        if c < p {
            (ia, c_a) = (im, c);
        } else {
            (ib, c_hi) = (im, c);
        }
    }
    let (a, b) = (f64::from_bits(ia), f64::from_bits(ib));
    if c_a.is_finite() && p - c_a < c_hi - p {
        a
    } else {
        b
    }
}

/// The Student's t quantile of the upper tail: the `x >= 0` with `P(T > x) = q`, for
/// `0 < q <= 0.5`. Taking the tail probability itself keeps its digits (T.INV(10⁻¹⁶, 2) would
/// otherwise solve for `1 − (1 − 10⁻¹⁶)`, which is 1.1·10⁻¹⁶).
pub(crate) fn t_quantile_upper(q: f64, df: f64) -> f64 {
    if q == 0.5 {
        return 0.0;
    }
    invert_cdf(|x| -t_cdf(-x, df), -q, 0.0, 1.0, true)
}

// ── discrete distributions ─────────────────────────────────────────────────────────────────

/// ln C(n, k).
pub(crate) fn log_choose(n: f64, k: f64) -> f64 {
    log_gamma(n + 1.0) - log_gamma(k + 1.0) - log_gamma(n - k + 1.0)
}

/// C(n, k) exactly for moderate sizes, rounded to an integer.
pub(crate) fn choose(n: f64, k: f64) -> f64 {
    if k < 0.0 || k > n {
        return 0.0;
    }
    let kk = k.min(n - k);
    let mut r = 1.0f64;
    let mut i = 1.0;
    while i <= kk {
        r = r * (n - kk + i) / i;
        if !r.is_finite() {
            return f64::INFINITY;
        }
        i += 1.0;
    }
    js_round(r)
}

/// JavaScript's `Math.round`: halves go toward positive infinity.
pub(crate) fn js_round(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

pub(crate) fn binom_pmf(k: f64, n: f64, p: f64) -> f64 {
    if k < 0.0 || k > n {
        return 0.0;
    }
    if p == 0.0 {
        return if k == 0.0 { 1.0 } else { 0.0 };
    }
    if p == 1.0 {
        return if k == n { 1.0 } else { 0.0 };
    }
    if n <= 1000.0 {
        return choose(n, k) * p.powf(k) * (1.0 - p).powf(n - k);
    }
    // C(n, k) pᵏ qⁿ⁻ᵏ = p^(k+1) q^(n−k+1) / B(k+1, n−k+1) / (p q (n+1)).
    let q = 1.0 - p;
    beta_prefix_ln(p, q, k + 1.0, n - k + 1.0).exp() / (p * q * (n + 1.0))
}

pub(crate) fn binom_cdf(k: f64, n: f64, p: f64) -> f64 {
    if k < 0.0 {
        return 0.0;
    }
    if k >= n {
        return 1.0;
    }
    // I_{1-p}(n-k, k+1) is exact and fast.
    beta_ixy(1.0 - p, p, n - k, k + 1.0)
}

pub(crate) fn poisson_pmf(k: f64, mean: f64) -> f64 {
    if k < 0.0 {
        return 0.0;
    }
    if mean == 0.0 {
        return if k == 0.0 { 1.0 } else { 0.0 };
    }
    if k >= 10.0 {
        // λ^(k+1) e^(−λ) / Γ(k+1) / λ.
        return gamma_prefix_ln(k + 1.0, mean).exp() / mean;
    }
    (k * mean.ln() - mean - log_gamma(k + 1.0)).exp()
}

pub(crate) fn poisson_cdf(k: f64, mean: f64) -> f64 {
    if k < 0.0 {
        return 0.0;
    }
    gamma_q(k + 1.0, mean)
}

pub(crate) fn hypgeom_pmf(k: f64, n: f64, big_k: f64, big_n: f64) -> f64 {
    if k < (n + big_k - big_n).max(0.0) || k > n.min(big_k) {
        return 0.0;
    }
    if n >= big_n {
        return if k == big_k { 1.0 } else { 0.0 };
    }
    // C(K,k)·C(N−K,n−k)/C(N,n) as a ratio of binomial probabilities with p = n/N (the p-powers
    // cancel), each free of the cancellation of ln C(N, n) at large N (as R's dhyper).
    let p = n / big_n;
    prob(binom_pmf(k, big_k, p) * binom_pmf(n - k, big_n - big_k, p) / binom_pmf(n, big_n, p))
}

pub(crate) fn negbinom_pmf(f: f64, s: f64, p: f64) -> f64 {
    if f + s >= 20.0 && p > 0.0 && p < 1.0 {
        // C(f+s−1, s−1) pˢ q^f = pˢ q^(f+1) / B(s, f+1) / ((s+f) q).
        let q = 1.0 - p;
        return beta_prefix_ln(p, q, s, f + 1.0).exp() / ((s + f) * q);
    }
    (log_choose(f + s - 1.0, s - 1.0) + s * p.ln() + f * (-p).ln_1p()).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * b.abs().max(1e-300)
    }

    #[test]
    fn gamma_family() {
        assert_eq!(gamma(5.0), 24.0);
        assert!(close(gamma(0.5), std::f64::consts::PI.sqrt(), 1e-14));
        assert!(close(log_gamma(100.5), 361.435_540_467_777_6, 1e-14));
        assert!(gamma(-2.0).is_nan());
        assert!(close(gamma_p(2.5, 3.0), 0.693_781_081_586_719_8, 1e-13));
        assert!(close(gamma_q(30.0, 80.0), 4.903_229_555_810_698e-11, 1e-10));
    }

    #[test]
    fn incomplete_beta_converges_for_large_parameters() {
        // BINOM.DIST(4900, 10000, 0.5, TRUE): Σ C(10000, k) / 2¹⁰⁰⁰⁰ for k ≤ 4900, exactly.
        let p = binom_cdf(4900.0, 10000.0, 0.5);
        assert!(close(p, 0.023_292_763_852_473_694, 1e-14), "{p}");
        assert!(close(beta_ixy(0.3, 0.7, 2.0, 3.0), 0.3483, 1e-12));
    }

    /// 50-digit references (mpmath: quadrature of the density, both tails normalised by their
    /// sum, and Temme's expansion with DLMF's closed-form coefficients).
    #[test]
    fn large_parameters_match_references() {
        let cases = [
            (gamma_p(1e7, 1e7), 0.500_042_052_208_723_7),
            (gamma_p(1e8, 1e8), 0.500_013_298_076_014_1),
            (gamma_p(1e15, 1e15), 0.500_000_004_205_220_8),
            (gamma_p(1e8, 99_990_000.0), 0.158_655_253_528_143_8),
            (gamma_q(1e8 + 1.0, 1e8), 0.500_026_596_151_992_8),
            // Bin(10¹⁵, ½) at its median: ½ + ½·P(X = n/2).
            (binom_cdf(5e14, 1e15, 0.5), 0.500_000_012_615_662_6),
            (binom_cdf(3.0, 1e15, 2e-15), 0.857_123_460_498_547_2),
            (
                beta_ixy(0.5, 0.5, 1e15, 1e15 + 1.0),
                0.500_000_008_920_620_6,
            ),
            (t_cdf(0.5, 1e15), 0.691_462_461_274_013),
            (t_cdf(-8.0, 1e15), 6.220_960_574_278_352e-16),
        ];
        for (i, (got, want)) in cases.into_iter().enumerate() {
            assert!(close(got, want, 1e-13), "case {i}: {got} vs {want}");
        }
    }

    #[test]
    fn uniform_expansion_agrees_with_the_classic_methods() {
        // Where both apply, Temme's expansion and the series / continued fraction agree.
        for (a, x) in [
            (400.0, 390.0),
            (500.0, 480.0),
            (1000.0, 1030.0),
            (300.0, 250.0),
        ] {
            let (p, q) = gamma_uniform(a, x).expect("converges");
            let classic = if x < a + 1.0 {
                gamma_series(a, x)
            } else {
                1.0 - gamma_cf(a, x)
            };
            assert!(close(p, classic, 1e-13), "P({a}, {x}): {p} vs {classic}");
            assert!((p + q - 1.0).abs() < 1e-15);
        }
        for (a, b, x) in [
            (500.0, 500.0, 0.48),
            (5100.0, 4901.0, 0.5),
            (200.0, 300.0, 0.41),
            (1000.0, 400.0, 0.71),
        ] {
            let y = 1.0 - x;
            let u = beta_uniform(x, y, a, b).expect("converges");
            let r = a + b;
            let lambda = a - r * x;
            let ln_bt = beta_prefix_ln(x, y, a, b);
            let classic = if x < (a + 1.0) / (r + 2.0) {
                ln_bt.exp() * beta_cf(x, y, a, b, lambda) / a
            } else {
                1.0 - ln_bt.exp() * beta_cf(y, x, b, a, -lambda) / b
            };
            assert!(
                close(u, classic, 1e-12),
                "I({x}; {a}, {b}): {u} vs {classic}"
            );
        }
    }

    #[test]
    fn distributions_stay_probabilities_and_monotone_at_any_size() {
        let sizes: [f64; 11] = [1e-3, 0.5, 1.0, 5.0, 99.0, 100.0, 1e3, 1e6, 1e10, 1e15, 1e18];
        let steps = [
            -1e3, -40.0, -10.0, -3.0, -1.0, -0.1, 0.0, 0.1, 1.0, 3.0, 10.0, 40.0, 1e3,
        ];
        for a in sizes {
            for b in sizes {
                let r = a + b;
                let (x0, y0) = (a / r, b / r);
                let sd = (x0 * y0 / (r + 1.0)).sqrt();
                let mut prev = 0.0;
                for k in steps {
                    // Step in the smaller of x and y, which carries the digits.
                    let (x, y) = if x0 <= y0 {
                        let x = x0 + k * sd;
                        (x, 1.0 - x)
                    } else {
                        let y = y0 - k * sd;
                        (1.0 - y, y)
                    };
                    if !(x > 0.0 && y > 0.0) {
                        continue;
                    }
                    let v = beta_ixy(x, y, a, b);
                    assert!((0.0..=1.0).contains(&v), "I({x}, {y}; {a}, {b}) = {v}");
                    assert!(v >= prev - 1e-12, "I({x}, {y}; {a}, {b}) = {v} < {prev}");
                    prev = v;
                }
            }
            let mut prev = 0.0;
            for k in steps {
                let x = a + k * a.sqrt();
                if x <= 0.0 {
                    continue;
                }
                let (p, q) = gamma_pq(a, x);
                assert!(
                    (0.0..=1.0).contains(&p) && (p + q - 1.0).abs() < 1e-14,
                    "{a} {x}"
                );
                assert!(p >= prev - 1e-12, "P({a}, {x}) = {p} < {prev}");
                prev = p;
            }
        }
    }

    #[test]
    fn error_functions_are_accurate_to_the_last_digits() {
        // mpmath at 40 digits.
        let cases = [
            (erf(1e-5), 1.128_379_167_057_9e-5),
            (erf(0.7), 0.677_801_193_837_418_4),
            (erf(1.3), 0.934_007_944_940_652_5),
            (erfc(1.25), 0.077_099_871_743_541_77),
            (erfc(5.0), 1.537_459_794_428_035e-12),
            (erfc(20.0), 5.395_865_611_607_901e-176),
            (erfc(-1.5), 1.966_105_146_475_310_7),
            (norm_cdf(-8.0), 6.220_960_574_271_784e-16),
        ];
        for (i, (got, want)) in cases.into_iter().enumerate() {
            assert!(close(got, want, 4e-15), "case {i}: {got:e} vs {want:e}");
        }
        assert_eq!(erfc(30.0), 0.0);
        assert_eq!(erf(f64::INFINITY), 1.0);
        assert!(erf(f64::NAN).is_nan());
    }

    #[test]
    fn inversion_propagates_failure() {
        assert!(invert_cdf(|_| f64::NAN, 0.5, 0.0, 1.0, true).is_nan());
        assert!(
            invert_cdf(
                |x| if x > 0.25 { f64::NAN } else { x },
                0.5,
                0.0,
                1.0,
                false
            )
            .is_nan()
        );
    }

    #[test]
    fn inversion_reaches_every_scale() {
        // 1 − e⁻ˣ = p has x = −ln(1 − p) ≈ p for tiny p; x/(1 + x) = p has x = p/(1 − p).
        let tiny = invert_cdf(|x| -(-x).exp_m1(), 1e-300, 0.0, 1.0, true);
        assert!(close(tiny, 1e-300, 1e-15), "{tiny:e}");
        let huge = invert_cdf(|x| -1.0 / (1.0 + x), -1e-200, 0.0, 1.0, true);
        assert!(close(huge, 1e200, 1e-15), "{huge:e}");
        // Beyond the largest double: no answer rather than a wrong one.
        assert!(invert_cdf(|x| -1.0 / (1.0 + x), -1e-320, 0.0, 1.0, true).is_nan());
        assert_eq!(invert_cdf(|x| x, 0.0, 0.0, 1.0, false), 0.0);
    }

    #[test]
    fn normal_quantile_round_trips() {
        for p in [1e-12, 0.001, 0.025, 0.3, 0.5, 0.9, 0.999_999] {
            let x = norm_inv(p);
            assert!(close(norm_cdf(x), p, 1e-13), "{p}");
        }
        assert!(norm_inv(0.0).is_nan());
        // A subnormal probability: the density underflows, the rational start is kept.
        assert!(close(norm_inv(1e-320), -38.269_125_343_032_65, 1e-8));
    }
}
