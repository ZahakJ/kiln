// SPDX-License-Identifier: AGPL-3.0-or-later
//! Aggregations over number lists, shared by the statistical functions, `SUBTOTAL`,
//! `AGGREGATE` and the database functions. Each returns a number or the Excel error for an invalid
//! input.

use rustc_hash::FxHashMap;

use super::support::{R, devsq, mean, sorted_asc, sum};
use crate::values::{ErrorValue, round15};

/// An aggregation of a list of numbers.
pub(crate) type Agg = fn(&[f64]) -> R<f64>;

pub(crate) fn average(xs: &[f64]) -> R<f64> {
    if xs.is_empty() {
        Err(ErrorValue::Div0)
    } else {
        Ok(mean(xs))
    }
}

pub(crate) fn count(xs: &[f64]) -> R<f64> {
    Ok(xs.len() as f64)
}

pub(crate) fn max_of(xs: &[f64]) -> R<f64> {
    Ok(if xs.is_empty() {
        0.0
    } else {
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    })
}

pub(crate) fn min_of(xs: &[f64]) -> R<f64> {
    Ok(if xs.is_empty() {
        0.0
    } else {
        xs.iter().copied().fold(f64::INFINITY, f64::min)
    })
}

pub(crate) fn sum_of(xs: &[f64]) -> R<f64> {
    Ok(sum(xs))
}

pub(crate) fn product(xs: &[f64]) -> R<f64> {
    if xs.is_empty() {
        return Ok(0.0);
    }
    let p: f64 = xs.iter().product();
    if p.is_finite() {
        Ok(p)
    } else {
        Err(ErrorValue::Num)
    }
}

pub(crate) fn var_s(xs: &[f64]) -> R<f64> {
    if xs.len() < 2 {
        Err(ErrorValue::Div0)
    } else {
        Ok(devsq(xs) / (xs.len() - 1) as f64)
    }
}

pub(crate) fn var_p(xs: &[f64]) -> R<f64> {
    if xs.is_empty() {
        Err(ErrorValue::Div0)
    } else {
        Ok(devsq(xs) / xs.len() as f64)
    }
}

pub(crate) fn stdev_s(xs: &[f64]) -> R<f64> {
    var_s(xs).map(f64::sqrt)
}

pub(crate) fn stdev_p(xs: &[f64]) -> R<f64> {
    var_p(xs).map(f64::sqrt)
}

pub(crate) fn median(xs: &[f64]) -> R<f64> {
    if xs.is_empty() {
        return Err(ErrorValue::Num);
    }
    let s = sorted_asc(xs);
    let m = s.len() / 2;
    Ok(if s.len() % 2 == 1 {
        s[m]
    } else {
        (s[m - 1] + s[m]) / 2.0
    })
}

/// Most frequent value; ties go to the value that appears first. `#N/A` when nothing repeats.
pub(crate) fn mode_sngl(xs: &[f64]) -> R<f64> {
    modes(xs).first().copied().ok_or(ErrorValue::NA)
}

/// Every value with the highest frequency (more than 1), in order of first appearance.
pub(crate) fn modes(xs: &[f64]) -> Vec<f64> {
    let mut counts: FxHashMap<u64, usize> = FxHashMap::default();
    let mut order: Vec<f64> = Vec::new();
    for &x in xs {
        let k = round15(x);
        let c = counts.entry(k.to_bits()).or_insert(0);
        if *c == 0 {
            order.push(k);
        }
        *c += 1;
    }
    let best = counts.values().copied().max().unwrap_or(0);
    if best < 2 {
        return Vec::new();
    }
    order
        .into_iter()
        .filter(|k| counts.get(&k.to_bits()) == Some(&best))
        .collect()
}

/// k-th largest (1-based; k rounded up).
pub(crate) fn large(xs: &[f64], k: f64) -> R<f64> {
    let kk = k.ceil();
    if xs.is_empty() || kk < 1.0 || kk > xs.len() as f64 {
        return Err(ErrorValue::Num);
    }
    Ok(sorted_asc(xs)[xs.len() - kk as usize])
}

/// k-th smallest.
pub(crate) fn small(xs: &[f64], k: f64) -> R<f64> {
    let kk = k.ceil();
    if xs.is_empty() || kk < 1.0 || kk > xs.len() as f64 {
        return Err(ErrorValue::Num);
    }
    Ok(sorted_asc(xs)[kk as usize - 1])
}

pub(crate) fn percentile_inc(xs: &[f64], p: f64) -> R<f64> {
    if xs.is_empty() || !(0.0..=1.0).contains(&p) {
        return Err(ErrorValue::Num);
    }
    let s = sorted_asc(xs);
    let h = (s.len() - 1) as f64 * p;
    let lo = h.floor();
    let frac = h - lo;
    let lo = lo as usize;
    let a = s[lo];
    Ok(if frac == 0.0 {
        a
    } else {
        a + frac * (s[(lo + 1).min(s.len() - 1)] - a)
    })
}

pub(crate) fn percentile_exc(xs: &[f64], p: f64) -> R<f64> {
    let n = xs.len();
    if n == 0 || p <= 0.0 || p >= 1.0 {
        return Err(ErrorValue::Num);
    }
    let h = (n + 1) as f64 * p;
    if h < 1.0 || h > n as f64 {
        return Err(ErrorValue::Num);
    }
    let s = sorted_asc(xs);
    let lo = h.floor();
    let frac = h - lo;
    let lo = lo as usize;
    let a = s[lo - 1];
    Ok(if frac == 0.0 || lo >= n {
        a
    } else {
        a + frac * (s[lo] - a)
    })
}

pub(crate) fn quartile_inc(xs: &[f64], q: f64) -> R<f64> {
    let qq = q.trunc();
    if !(0.0..=4.0).contains(&qq) {
        return Err(ErrorValue::Num);
    }
    percentile_inc(xs, qq / 4.0)
}

pub(crate) fn quartile_exc(xs: &[f64], q: f64) -> R<f64> {
    let qq = q.trunc();
    if !(1.0..=3.0).contains(&qq) {
        return Err(ErrorValue::Num);
    }
    percentile_exc(xs, qq / 4.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_statistics() {
        let xs = [3.0, 1.0, 4.0, 1.0, 5.0];
        assert_eq!(median(&xs), Ok(3.0));
        assert_eq!(large(&xs, 1.0), Ok(5.0));
        assert_eq!(small(&xs, 2.0), Ok(1.0));
        assert_eq!(large(&xs, 6.0), Err(ErrorValue::Num));
        assert_eq!(mode_sngl(&xs), Ok(1.0));
        assert_eq!(mode_sngl(&[1.0, 2.0]), Err(ErrorValue::NA));
        assert_eq!(percentile_inc(&[1.0, 2.0, 3.0, 4.0], 0.3), Ok(1.9));
        assert_eq!(quartile_exc(&[1.0, 2.0], 0.0), Err(ErrorValue::Num));
    }
}
