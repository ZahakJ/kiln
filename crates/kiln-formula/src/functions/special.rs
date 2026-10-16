// SPDX-License-Identifier: AGPL-3.0-or-later
//! Special functions and probability distributions to double precision (about 1e-14 relative),
//! so the statistical functions agree with Excel and LibreOffice well inside the conformance
//! tolerance.
//!
//! The gamma function, its logarithm and the error functions come from `statrs` (Lanczos and
//! Boost-derived rational approximations). The regularised incomplete gamma and beta functions
//! are evaluated here with series and modified-Lentz continued fractions run to convergence:
//! `statrs::function::beta::beta_reg` stops after 140 iterations, which loses digits for the
//! large shape parameters of `BINOM.DIST`, `T.DIST` or `F.DIST` with large degrees of freedom,
//! and `gamma_lr` forms the upper tail by subtraction. The normal quantile is Acklam's rational
//! approximation refined with Halley steps; other quantiles use safeguarded bisection.

use statrs::function::{erf as serf, gamma as sgamma};

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

/// Regularised lower incomplete gamma P(a, x).
pub(crate) fn gamma_p(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x < a + 1.0 {
        gamma_series(a, x)
    } else {
        1.0 - gamma_cf(a, x)
    }
}

/// Regularised upper incomplete gamma Q(a, x) = 1 − P(a, x), accurate in the tail.
pub(crate) fn gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    if x < a + 1.0 {
        1.0 - gamma_series(a, x)
    } else {
        gamma_cf(a, x)
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
            break;
        }
    }
    sum * (-x + a * x.ln() - log_gamma(a)).exp()
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
            break;
        }
    }
    (-x + a * x.ln() - log_gamma(a)).exp() * h
}

/// Regularised incomplete beta I_x(a, b).
pub(crate) fn beta_i(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let bt = (a * x.ln() + b * (-x).ln_1p() - log_beta(a, b)).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        bt * beta_cf(x, a, b) / a
    } else {
        1.0 - bt * beta_cf(1.0 - x, b, a) / b
    }
}

fn beta_cf(x: f64, a: f64, b: f64) -> f64 {
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < FPMIN {
        d = FPMIN;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=MAX_ITER {
        let m = m as f64;
        let m2 = 2.0 * m;
        let mut aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        h *= d * c;
        aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
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
    h
}

/// The error function.
pub(crate) fn erf(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    serf::erf(x)
}

/// The complementary error function, accurate in the tails.
pub(crate) fn erfc(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    serf::erfc(x)
}

// ── normal ─────────────────────────────────────────────────────────────────────────────────

pub(crate) fn norm_pdf(z: f64) -> f64 {
    (-0.5 * z * z - LN_SQRT_2PI).exp()
}

pub(crate) fn norm_cdf(z: f64) -> f64 {
    0.5 * erfc(-z / std::f64::consts::SQRT_2)
}

fn norm_cdf_upper(z: f64) -> f64 {
    0.5 * erfc(z / std::f64::consts::SQRT_2)
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
        x -= u / (1.0 + x * u / 2.0);
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
    ((a - 1.0) * x.ln() + (b - 1.0) * (-x).ln_1p() - log_beta(a, b)).exp()
}

/// Student's t CDF.
pub(crate) fn t_cdf(t: f64, df: f64) -> f64 {
    let x = df / (df + t * t);
    let tail = 0.5 * beta_i(x, df / 2.0, 0.5);
    if t >= 0.0 { 1.0 - tail } else { tail }
}

pub(crate) fn t_pdf(t: f64, df: f64) -> f64 {
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
    beta_i(d1 * x / (d1 * x + d2), d1 / 2.0, d2 / 2.0)
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
    (0.5 * (d1 * (d1 * x).ln() + d2 * d2.ln() - (d1 + d2) * (d1 * x + d2).ln())
        - x.ln()
        - log_beta(d1 / 2.0, d2 / 2.0))
    .exp()
}

/// Inverts a continuous increasing CDF on `[lo, hi]` by bisection to full precision; with
/// `unbounded`, `hi` doubles until it brackets `p`.
pub(crate) fn invert_cdf(
    cdf: impl Fn(f64) -> f64,
    p: f64,
    lo: f64,
    hi: f64,
    unbounded: bool,
) -> f64 {
    let (mut a, mut b) = (lo, hi);
    if unbounded {
        let mut guard = 0;
        while cdf(b) < p && guard < 2000 {
            a = b;
            b *= 2.0;
            guard += 1;
        }
    }
    for _ in 0..300 {
        let m = (a + b) / 2.0;
        if m == a || m == b {
            break;
        }
        if cdf(m) < p {
            a = m;
        } else {
            b = m;
        }
    }
    (a + b) / 2.0
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
    (log_choose(n, k) + k * p.ln() + (n - k) * (-p).ln_1p()).exp()
}

pub(crate) fn binom_cdf(k: f64, n: f64, p: f64) -> f64 {
    if k < 0.0 {
        return 0.0;
    }
    if k >= n {
        return 1.0;
    }
    // I_{1-p}(n-k, k+1) is exact and fast.
    beta_i(1.0 - p, n - k, k + 1.0)
}

pub(crate) fn poisson_pmf(k: f64, mean: f64) -> f64 {
    if k < 0.0 {
        return 0.0;
    }
    if mean == 0.0 {
        return if k == 0.0 { 1.0 } else { 0.0 };
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
    (log_choose(big_k, k) + log_choose(big_n - big_k, n - k) - log_choose(big_n, n)).exp()
}

pub(crate) fn negbinom_pmf(f: f64, s: f64, p: f64) -> f64 {
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
        // BINOM.DIST(4900, 10000, 0.5, TRUE) needs far more than statrs' 140 iterations.
        let p = binom_cdf(4900.0, 10000.0, 0.5);
        assert!(close(p, 0.023_292_763_852_355_21, 1e-12), "{p}");
        assert!(close(beta_i(0.3, 2.0, 3.0), 0.3483, 1e-12));
    }

    #[test]
    fn normal_quantile_round_trips() {
        for p in [1e-12, 0.001, 0.025, 0.3, 0.5, 0.9, 0.999_999] {
            let x = norm_inv(p);
            assert!(close(norm_cdf(x), p, 1e-13), "{p}");
        }
        assert!(norm_inv(0.0).is_nan());
    }
}
