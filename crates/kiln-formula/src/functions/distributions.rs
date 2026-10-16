// SPDX-License-Identifier: AGPL-3.0-or-later
//! Probability distributions (Statistical) and their pre-2010 compatibility names. Every
//! function is numeric: arguments lifted over arrays, non-finite results `#NUM!`.

use super::special::*;
use super::support::*;
use crate::registry::FunctionSpec;
use crate::values::ErrorValue;

type F = R<f64>;

fn t(x: f64) -> f64 {
    x.trunc()
}

fn on(b: f64) -> bool {
    b != 0.0
}

// ── normal family ──────────────────────────────────────────────────────────────────────────

fn norm_dist(x: f64, m: f64, sd: f64, cum: f64) -> F {
    if sd <= 0.0 {
        return Err(ErrorValue::Num);
    }
    let z = (x - m) / sd;
    Ok(if on(cum) {
        norm_cdf(z)
    } else {
        norm_pdf(z) / sd
    })
}

fn norm_inv_fn(p: f64, m: f64, sd: f64) -> F {
    if p <= 0.0 || p >= 1.0 || sd <= 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok(m + sd * norm_inv(p))
}

fn lognorm_dist(x: f64, m: f64, sd: f64, cum: f64) -> F {
    if x <= 0.0 || sd <= 0.0 {
        return Err(ErrorValue::Num);
    }
    let z = (x.ln() - m) / sd;
    Ok(if on(cum) {
        norm_cdf(z)
    } else {
        norm_pdf(z) / (sd * x)
    })
}

fn lognorm_inv(p: f64, m: f64, sd: f64) -> F {
    if p <= 0.0 || p >= 1.0 || sd <= 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok((m + sd * norm_inv(p)).exp())
}

// ── t ──────────────────────────────────────────────────────────────────────────────────────

fn t_inv_left(p: f64, df: f64) -> f64 {
    if p == 0.5 {
        return 0.0;
    }
    if p < 0.5 {
        return -t_inv_left(1.0 - p, df);
    }
    // Solve on the upper tail for precision.
    let q = 1.0 - p;
    invert_cdf(|x| -t_cdf(-x, df), -q, 0.0, 1.0, true)
}

fn t_dist(x: f64, df0: f64, cum: f64) -> F {
    let df = t(df0);
    if df < 1.0 {
        return Err(ErrorValue::Num);
    }
    Ok(if on(cum) { t_cdf(x, df) } else { t_pdf(x, df) })
}

fn t_dist_rt(x: f64, df0: f64) -> F {
    let df = t(df0);
    if df < 1.0 {
        return Err(ErrorValue::Num);
    }
    Ok(t_cdf(-x, df))
}

fn t_dist_2t(x: f64, df0: f64) -> F {
    let df = t(df0);
    if df < 1.0 || x < 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok(prob(2.0 * t_cdf(-x, df)))
}

fn tdist_legacy(x: f64, df0: f64, tails0: f64) -> F {
    let (df, tails) = (t(df0), t(tails0));
    if df < 1.0 || x < 0.0 || (tails != 1.0 && tails != 2.0) {
        return Err(ErrorValue::Num);
    }
    Ok(prob(tails * t_cdf(-x, df)))
}

fn t_inv(p: f64, df0: f64) -> F {
    let df = t(df0);
    if p <= 0.0 || p >= 1.0 || df < 1.0 {
        return Err(ErrorValue::Num);
    }
    Ok(t_inv_left(p, df))
}

fn t_inv_2t(p: f64, df0: f64) -> F {
    let df = t(df0);
    if p <= 0.0 || p > 1.0 || df < 1.0 {
        return Err(ErrorValue::Num);
    }
    Ok(t_inv_left(1.0 - p / 2.0, df))
}

// ── chi-squared ────────────────────────────────────────────────────────────────────────────

fn chisq_dist(x: f64, df0: f64, cum: f64) -> F {
    let df = t(df0);
    if x < 0.0 || !(1.0..=1e10).contains(&df) {
        return Err(ErrorValue::Num);
    }
    Ok(if on(cum) {
        chisq_cdf(x, df)
    } else {
        chisq_pdf(x, df)
    })
}

fn chisq_dist_rt(x: f64, df0: f64) -> F {
    let df = t(df0);
    if x < 0.0 || !(1.0..=1e10).contains(&df) {
        return Err(ErrorValue::Num);
    }
    Ok(gamma_q(df / 2.0, x / 2.0))
}

fn chisq_inv(p: f64, df0: f64) -> F {
    let df = t(df0);
    if !(0.0..1.0).contains(&p) || !(1.0..=1e10).contains(&df) {
        return Err(ErrorValue::Num);
    }
    if p == 0.0 {
        return Ok(0.0);
    }
    Ok(invert_cdf(|x| chisq_cdf(x, df), p, 0.0, df.max(1.0), true))
}

fn chisq_inv_rt(p: f64, df0: f64) -> F {
    let df = t(df0);
    if p <= 0.0 || p > 1.0 || !(1.0..=1e10).contains(&df) {
        return Err(ErrorValue::Num);
    }
    if p == 1.0 {
        return Ok(0.0);
    }
    // Solve 1 − cdf = p on the upper tail.
    Ok(invert_cdf(
        |x| -gamma_q(df / 2.0, x / 2.0),
        -p,
        0.0,
        df.max(1.0),
        true,
    ))
}

// ── F ──────────────────────────────────────────────────────────────────────────────────────

fn f_dist(x: f64, a: f64, b: f64, cum: f64) -> F {
    let (d1, d2) = (t(a), t(b));
    if x < 0.0 || d1 < 1.0 || d2 < 1.0 {
        return Err(ErrorValue::Num);
    }
    Ok(if on(cum) {
        f_cdf(x, d1, d2)
    } else {
        f_pdf(x, d1, d2)
    })
}

fn f_dist_rt(x: f64, a: f64, b: f64) -> F {
    let (d1, d2) = (t(a), t(b));
    if x < 0.0 || d1 < 1.0 || d2 < 1.0 {
        return Err(ErrorValue::Num);
    }
    if x == 0.0 {
        return Ok(1.0);
    }
    Ok(f_sf(x, d1, d2))
}

fn f_inv(p: f64, a: f64, b: f64) -> F {
    let (d1, d2) = (t(a), t(b));
    if !(0.0..=1.0).contains(&p) || d1 < 1.0 || d2 < 1.0 {
        return Err(ErrorValue::Num);
    }
    if p == 0.0 {
        return Ok(0.0);
    }
    if p == 1.0 {
        return Err(ErrorValue::Num);
    }
    Ok(invert_cdf(|x| f_cdf(x, d1, d2), p, 0.0, 1.0, true))
}

fn f_inv_rt(p: f64, a: f64, b: f64) -> F {
    let (d1, d2) = (t(a), t(b));
    if p <= 0.0 || p > 1.0 || d1 < 1.0 || d2 < 1.0 {
        return Err(ErrorValue::Num);
    }
    if p == 1.0 {
        return Ok(0.0);
    }
    Ok(invert_cdf(|x| -f_sf(x, d1, d2), -p, 0.0, 1.0, true))
}

// ── beta, gamma, exponential, Weibull ──────────────────────────────────────────────────────

fn beta_dist(x: f64, a: f64, b: f64, cum: f64, lo: f64, hi: f64) -> F {
    if a <= 0.0 || b <= 0.0 || x < lo || x > hi || lo == hi {
        return Err(ErrorValue::Num);
    }
    let z = (x - lo) / (hi - lo);
    Ok(if on(cum) {
        beta_ixy(z, (hi - x) / (hi - lo), a, b)
    } else {
        beta_pdf(z, a, b) / (hi - lo)
    })
}

fn beta_inv(p: f64, a: f64, b: f64, lo: f64, hi: f64) -> F {
    if p <= 0.0 || p > 1.0 || a <= 0.0 || b <= 0.0 || lo >= hi {
        return Err(ErrorValue::Num);
    }
    if p == 1.0 {
        return Ok(hi);
    }
    let z = invert_cdf(|x| beta_i(x, a, b), p, 0.0, 1.0, false);
    Ok(lo + z * (hi - lo))
}

fn gamma_dist(x: f64, a: f64, b: f64, cum: f64) -> F {
    if x < 0.0 || a <= 0.0 || b <= 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok(if on(cum) {
        gamma_cdf(x, a, b)
    } else {
        gamma_pdf(x, a, b)
    })
}

fn gamma_inv(p: f64, a: f64, b: f64) -> F {
    if !(0.0..1.0).contains(&p) || a <= 0.0 || b <= 0.0 {
        return Err(ErrorValue::Num);
    }
    if p == 0.0 {
        return Ok(0.0);
    }
    Ok(invert_cdf(
        |x| gamma_cdf(x, a, b),
        p,
        0.0,
        (a * b).max(1.0),
        true,
    ))
}

fn expon_dist(x: f64, lambda: f64, cum: f64) -> F {
    if x < 0.0 || lambda <= 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok(if on(cum) {
        -(-lambda * x).exp_m1()
    } else {
        lambda * (-lambda * x).exp()
    })
}

fn weibull_dist(x: f64, a: f64, b: f64, cum: f64) -> F {
    if x < 0.0 || a <= 0.0 || b <= 0.0 {
        return Err(ErrorValue::Num);
    }
    let r = (x / b).powf(a);
    Ok(if on(cum) {
        -(-r).exp_m1()
    } else {
        a / b.powf(a) * x.powf(a - 1.0) * (-r).exp()
    })
}

// ── discrete ───────────────────────────────────────────────────────────────────────────────

fn binom_dist(s0: f64, n0: f64, p: f64, cum: f64) -> F {
    let (s, n) = (t(s0), t(n0));
    if s < 0.0 || s > n || !(0.0..=1.0).contains(&p) {
        return Err(ErrorValue::Num);
    }
    Ok(if on(cum) {
        binom_cdf(s, n, p)
    } else {
        binom_pmf(s, n, p)
    })
}

fn binom_dist_range(n0: f64, p: f64, s0: f64, s20: f64) -> F {
    let (n, s, s2) = (t(n0), t(s0), t(s20));
    if n < 0.0 || !(0.0..=1.0).contains(&p) || s < 0.0 || s > n || s2 < s || s2 > n {
        return Err(ErrorValue::Num);
    }
    if s2 - s > LOOP_LIMIT {
        // Long ranges: the difference of the cumulative distribution (same value, bounded time).
        let below = if s > 0.0 {
            binom_cdf(s - 1.0, n, p)
        } else {
            0.0
        };
        return Ok(prob(binom_cdf(s2, n, p) - below));
    }
    let mut acc = 0.0;
    let mut k = s;
    while k <= s2 {
        acc += binom_pmf(k, n, p);
        k += 1.0;
    }
    Ok(prob(acc))
}

/// Longest term-by-term summation; beyond it the closed forms are used.
const LOOP_LIMIT: f64 = 100_000.0;

fn binom_inv(n0: f64, p: f64, alpha: f64) -> F {
    let n = t(n0);
    if n < 0.0 || !(0.0..=1.0).contains(&p) || !(0.0..=1.0).contains(&alpha) {
        return Err(ErrorValue::Num);
    }
    let target = alpha * (1.0 - 1e-15);
    if n > LOOP_LIMIT {
        // The smallest k whose cumulative probability reaches alpha, by bisection.
        let (mut lo, mut hi) = (0.0f64, n);
        while lo < hi {
            let mid = ((lo + hi) / 2.0).floor();
            let c = binom_cdf(mid, n, p);
            if c.is_nan() {
                return Err(ErrorValue::Num);
            }
            if c >= target {
                hi = mid;
            } else {
                lo = mid + 1.0;
            }
        }
        return Ok(lo);
    }
    let mut acc = 0.0;
    let mut k = 0.0;
    while k <= n {
        acc += binom_pmf(k, n, p);
        if acc >= target {
            return Ok(k);
        }
        k += 1.0;
    }
    Ok(n)
}

fn poisson_dist(x0: f64, mean: f64, cum: f64) -> F {
    let x = t(x0);
    if x < 0.0 || mean < 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok(if on(cum) {
        poisson_cdf(x, mean)
    } else {
        poisson_pmf(x, mean)
    })
}

fn hypgeom_dist(s0: f64, n0: f64, k0: f64, big_n0: f64, cum: f64) -> F {
    let (s, n, k, big_n) = (t(s0), t(n0), t(k0), t(big_n0));
    if s < 0.0
        || n <= 0.0
        || k <= 0.0
        || big_n <= 0.0
        || n > big_n
        || k > big_n
        || s > n
        || s > k
        || n - s > big_n - k
    {
        return Err(ErrorValue::Num);
    }
    if !on(cum) {
        return Ok(hypgeom_pmf(s, n, k, big_n));
    }
    let mut acc = 0.0;
    let mut i = (n + k - big_n).max(0.0);
    if s - i > 100.0 * LOOP_LIMIT {
        return Err(ErrorValue::Num);
    }
    while i <= s {
        acc += hypgeom_pmf(i, n, k, big_n);
        i += 1.0;
    }
    Ok(acc.min(1.0))
}

fn negbinom_dist(f0: f64, s0: f64, p: f64, cum: f64) -> F {
    let (f, s) = (t(f0), t(s0));
    if f < 0.0 || s < 1.0 || !(0.0..=1.0).contains(&p) {
        return Err(ErrorValue::Num);
    }
    if !on(cum) {
        return Ok(negbinom_pmf(f, s, p));
    }
    Ok(beta_ixy(p, 1.0 - p, s, f + 1.0))
}

/// The distribution functions.
pub fn specs() -> Vec<FunctionSpec> {
    vec![
        numeric!("NORM.DIST", 4, 4, [], |x, m, sd, c| norm_dist(x, m, sd, c)),
        numeric!("NORMDIST", 4, 4, [], |x, m, sd, c| norm_dist(x, m, sd, c)),
        numeric!("NORM.INV", 3, 3, [], |p, m, sd| norm_inv_fn(p, m, sd)),
        numeric!("NORMINV", 3, 3, [], |p, m, sd| norm_inv_fn(p, m, sd)),
        numeric!("NORM.S.DIST", 2, 2, [], |z, c| norm_dist(z, 0.0, 1.0, c)),
        numeric!("NORMSDIST", 1, 1, [], |z| norm_cdf(z)),
        numeric!("NORM.S.INV", 1, 1, [], |p| norm_inv_fn(p, 0.0, 1.0)),
        numeric!("NORMSINV", 1, 1, [], |p| norm_inv_fn(p, 0.0, 1.0)),
        numeric!("LOGNORM.DIST", 4, 4, [], |x, m, sd, c| lognorm_dist(
            x, m, sd, c
        )),
        numeric!("LOGNORMDIST", 3, 3, [], |x, m, sd| lognorm_dist(
            x, m, sd, 1.0
        )),
        numeric!("LOGNORM.INV", 3, 3, [], |p, m, sd| lognorm_inv(p, m, sd)),
        numeric!("LOGINV", 3, 3, [], |p, m, sd| lognorm_inv(p, m, sd)),
        numeric!("T.DIST", 3, 3, [], |x, df, c| t_dist(x, df, c)),
        numeric!("T.DIST.RT", 2, 2, [], |x, df| t_dist_rt(x, df)),
        numeric!("T.DIST.2T", 2, 2, [], |x, df| t_dist_2t(x, df)),
        numeric!("TDIST", 3, 3, [], |x, df, tails| tdist_legacy(x, df, tails)),
        numeric!("T.INV", 2, 2, [], |p, df| t_inv(p, df)),
        numeric!("T.INV.2T", 2, 2, [], |p, df| t_inv_2t(p, df)),
        numeric!("TINV", 2, 2, [], |p, df| t_inv_2t(p, df)),
        numeric!("CHISQ.DIST", 3, 3, [], |x, df, c| chisq_dist(x, df, c)),
        numeric!("CHISQ.DIST.RT", 2, 2, [], |x, df| chisq_dist_rt(x, df)),
        numeric!("CHIDIST", 2, 2, [], |x, df| chisq_dist_rt(x, df)),
        numeric!("CHISQ.INV", 2, 2, [], |p, df| chisq_inv(p, df)),
        numeric!("CHISQ.INV.RT", 2, 2, [], |p, df| chisq_inv_rt(p, df)),
        numeric!("CHIINV", 2, 2, [], |p, df| chisq_inv_rt(p, df)),
        numeric!("F.DIST", 4, 4, [], |x, a, b, c| f_dist(x, a, b, c)),
        numeric!("F.DIST.RT", 3, 3, [], |x, a, b| f_dist_rt(x, a, b)),
        numeric!("FDIST", 3, 3, [], |x, a, b| f_dist_rt(x, a, b)),
        numeric!("F.INV", 3, 3, [], |p, a, b| f_inv(p, a, b)),
        numeric!("F.INV.RT", 3, 3, [], |p, a, b| f_inv_rt(p, a, b)),
        numeric!("FINV", 3, 3, [], |p, a, b| f_inv_rt(p, a, b)),
        numeric!("BETA.DIST", 4, 6, [0.0, 1.0], |x, a, b, c, lo, hi| {
            beta_dist(x, a, b, c, lo, hi)
        }),
        numeric!("BETADIST", 3, 5, [0.0, 1.0], |x, a, b, lo, hi| beta_dist(
            x, a, b, 1.0, lo, hi
        )),
        numeric!("BETA.INV", 3, 5, [0.0, 1.0], |p, a, b, lo, hi| beta_inv(
            p, a, b, lo, hi
        )),
        numeric!("BETAINV", 3, 5, [0.0, 1.0], |p, a, b, lo, hi| beta_inv(
            p, a, b, lo, hi
        )),
        numeric!("GAMMA.DIST", 4, 4, [], |x, a, b, c| gamma_dist(x, a, b, c)),
        numeric!("GAMMADIST", 4, 4, [], |x, a, b, c| gamma_dist(x, a, b, c)),
        numeric!("GAMMA.INV", 3, 3, [], |p, a, b| gamma_inv(p, a, b)),
        numeric!("GAMMAINV", 3, 3, [], |p, a, b| gamma_inv(p, a, b)),
        numeric!("EXPON.DIST", 3, 3, [], |x, l, c| expon_dist(x, l, c)),
        numeric!("EXPONDIST", 3, 3, [], |x, l, c| expon_dist(x, l, c)),
        numeric!("WEIBULL.DIST", 4, 4, [], |x, a, b, c| weibull_dist(
            x, a, b, c
        )),
        numeric!("WEIBULL", 4, 4, [], |x, a, b, c| weibull_dist(x, a, b, c)),
        numeric!("BINOM.DIST", 4, 4, [], |s, n, p, c| binom_dist(s, n, p, c)),
        numeric!("BINOMDIST", 4, 4, [], |s, n, p, c| binom_dist(s, n, p, c)),
        numeric!("BINOM.DIST.RANGE", 3, 4, [f64::NAN], |n, p, s, s2| {
            binom_dist_range(n, p, s, if s2.is_nan() { s } else { s2 })
        }),
        numeric!("BINOM.INV", 3, 3, [], |n, p, a| binom_inv(n, p, a)),
        numeric!("CRITBINOM", 3, 3, [], |n, p, a| binom_inv(n, p, a)),
        numeric!("POISSON.DIST", 3, 3, [], |x, m, c| poisson_dist(x, m, c)),
        numeric!("POISSON", 3, 3, [], |x, m, c| poisson_dist(x, m, c)),
        numeric!("HYPGEOM.DIST", 5, 5, [], |s, n, k, nn, c| hypgeom_dist(
            s, n, k, nn, c
        )),
        numeric!("HYPGEOMDIST", 4, 4, [], |s, n, k, nn| hypgeom_dist(
            s, n, k, nn, 0.0
        )),
        numeric!("NEGBINOM.DIST", 4, 4, [], |f, s, p, c| negbinom_dist(
            f, s, p, c
        )),
        numeric!("NEGBINOMDIST", 3, 3, [], |f, s, p| negbinom_dist(
            f, s, p, 0.0
        )),
    ]
}
