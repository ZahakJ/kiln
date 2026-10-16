// SPDX-License-Identifier: AGPL-3.0-or-later
//! Value-axis scales: Excel's automatic bounds and "nice" major units (1, 2 or 5 × 10ⁿ), fixed
//! bounds and units from the file, logarithmic and reversed axes.

use crate::model::Axis;

/// A resolved value scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    /// Lowest value on the axis.
    pub min: f64,
    /// Highest value on the axis.
    pub max: f64,
    /// Distance between major ticks (for log scales, the factor between ticks).
    pub major: f64,
    /// Distance between minor ticks.
    pub minor: f64,
    /// Logarithm base, for log scales.
    pub log_base: Option<f64>,
    /// The axis runs from maximum to minimum.
    pub reversed: bool,
}

/// Most major ticks a scale produces (bounds a hostile tiny major unit).
pub const MAX_TICKS: usize = 1_000;

impl Scale {
    /// The position of `v` along the axis, 0 at the start and 1 at the end (outside 0–1 when `v`
    /// is outside the bounds; NaN for non-positive values on a log scale).
    #[must_use]
    pub fn fraction(&self, v: f64) -> f64 {
        let f = match self.log_base {
            Some(b) => {
                if v <= 0.0 {
                    return f64::NAN;
                }
                let lb = b.ln();
                ((v.ln() - self.min.ln()) / lb) / ((self.max.ln() - self.min.ln()) / lb)
            }
            None => {
                let span = self.max - self.min;
                if span <= 0.0 {
                    0.5
                } else {
                    (v - self.min) / span
                }
            }
        };
        if self.reversed { 1.0 - f } else { f }
    }

    /// The major tick values from `min` to `max`.
    #[must_use]
    pub fn ticks(&self) -> Vec<f64> {
        self.steps(self.major)
    }

    /// The minor tick values (including the major ones).
    #[must_use]
    pub fn minor_ticks(&self) -> Vec<f64> {
        match self.log_base {
            // Minor ticks of a log axis: 2…9 × each power.
            Some(b) => {
                let mut out = Vec::new();
                for t in self.ticks() {
                    for k in 1..(b.round() as i64).clamp(2, 100) {
                        #[allow(clippy::cast_precision_loss)]
                        let v = t * k as f64;
                        if v <= self.max * (1.0 + 1e-9) {
                            out.push(v);
                        }
                    }
                    if out.len() > MAX_TICKS * 10 {
                        break;
                    }
                }
                out
            }
            None => self.steps(self.minor),
        }
    }

    fn steps(&self, unit: f64) -> Vec<f64> {
        let mut out = Vec::new();
        if !(self.min.is_finite() && self.max.is_finite()) {
            return out;
        }
        match self.log_base {
            Some(b) => {
                if b.partial_cmp(&1.0) != Some(std::cmp::Ordering::Greater) || self.min <= 0.0 {
                    return out;
                }
                let mut v = self.min;
                while v <= self.max * (1.0 + 1e-9) && out.len() < MAX_TICKS {
                    out.push(v);
                    v *= b;
                }
            }
            None => {
                if unit.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                    return vec![self.min, self.max];
                }
                let n = ((self.max - self.min) / unit + 1e-9).floor();
                if !(n.is_finite()) || n > MAX_TICKS as f64 {
                    return vec![self.min, self.max];
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                for i in 0..=(n as usize) {
                    #[allow(clippy::cast_precision_loss)]
                    let v = self.min + unit * i as f64;
                    // Snap float noise (0.30000000000000004 → 0.3).
                    out.push(snap(v, unit));
                }
            }
        }
        out
    }
}

fn snap(v: f64, unit: f64) -> f64 {
    if unit <= 0.0 || !v.is_finite() {
        return v;
    }
    let digits = (-unit.log10().floor() + 2.0).clamp(0.0, 15.0);
    let f = 10f64.powf(digits);
    let s = (v * f).round() / f;
    if s == 0.0 { 0.0 } else { s }
}

/// The nice major unit (1, 2 or 5 × 10ⁿ) for a value range split into at most `max_ticks`
/// intervals.
#[must_use]
pub fn nice_unit(range: f64, max_intervals: usize) -> f64 {
    if !(range.is_finite()) || range <= 0.0 {
        return 1.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let raw = range / max_intervals.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    for m in [1.0, 2.0, 5.0, 10.0] {
        if m * mag >= raw * (1.0 - 1e-9) {
            return m * mag;
        }
    }
    10.0 * mag
}

/// The scale of a value axis for data from `lo` to `hi`, honouring the axis' fixed bounds, units,
/// logarithm base and orientation. `max_intervals` comes from the axis length (Excel keeps major
/// gridlines at least about 30 px apart on screen).
#[must_use]
pub fn value_scale(
    lo: f64,
    hi: f64,
    axis: Option<&Axis>,
    max_intervals: usize,
    percent: bool,
) -> Scale {
    let s = axis.map(|a| &a.scaling);
    let fixed_min = s.and_then(|s| s.min).filter(|v| v.is_finite());
    let fixed_max = s.and_then(|s| s.max).filter(|v| v.is_finite());
    let fixed_major = axis
        .and_then(|a| a.major_unit)
        .filter(|v| *v > 0.0 && v.is_finite());
    let fixed_minor = axis
        .and_then(|a| a.minor_unit)
        .filter(|v| *v > 0.0 && v.is_finite());
    let reversed = axis.is_some_and(Axis::is_reversed);
    let log_base = s
        .and_then(|s| s.log_base)
        .filter(|b| b.is_finite() && *b >= 2.0 && *b <= 1000.0);

    let (mut lo, mut hi) = if lo.is_finite() && hi.is_finite() && lo <= hi {
        (lo, hi)
    } else {
        (0.0, 1.0)
    };
    if percent {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
        if lo >= 0.0 && hi <= 1.0 {
            lo = 0.0;
            hi = 1.0;
        }
    }

    if let Some(b) = log_base {
        let positive_lo = if lo > 0.0 { lo } else { hi.max(1.0) / b };
        let floor_pow = |v: f64| b.powf((v.ln() / b.ln()).floor());
        let ceil_pow = |v: f64| b.powf((v.ln() / b.ln() - 1e-9).ceil());
        let min = fixed_min
            .filter(|v| *v > 0.0)
            .unwrap_or_else(|| floor_pow(positive_lo));
        let mut max = fixed_max
            .filter(|v| *v > min)
            .unwrap_or_else(|| ceil_pow(hi.max(min * b)));
        if max <= min {
            max = min * b;
        }
        return Scale {
            min,
            max,
            major: b,
            minor: b,
            log_base: Some(b),
            reversed,
        };
    }

    // Excel's automatic minimum: zero unless the data sits far from it.
    let auto_min = || {
        if lo >= 0.0 {
            if hi > 0.0 && (hi - lo) / hi < 1.0 / 6.0 {
                lo
            } else {
                0.0
            }
        } else {
            lo
        }
    };
    let auto_max = || {
        if hi <= 0.0 {
            if lo < 0.0 && (hi - lo) / -lo < 1.0 / 6.0 {
                hi
            } else {
                0.0
            }
        } else {
            hi
        }
    };
    let mut min = fixed_min.unwrap_or_else(auto_min);
    let mut max = fixed_max.unwrap_or_else(auto_max);
    if max < min {
        std::mem::swap(&mut min, &mut max);
    }
    if (max - min).abs() < f64::EPSILON * max.abs().max(1.0) {
        // A single value: give it room.
        if fixed_max.is_none() {
            max = if min == 0.0 { 1.0 } else { min + min.abs() };
        } else if fixed_min.is_none() {
            min = max - max.abs().max(1.0);
        }
    }
    let headroom = |v: f64, span: f64| v + 0.05 * span;
    let span = max - min;
    let mut major = fixed_major.unwrap_or_else(|| {
        let top = if fixed_max.is_some() || (percent && hi <= 1.0) {
            max
        } else {
            headroom(max, span)
        };
        let bottom = if fixed_min.is_some() || min == 0.0 {
            min
        } else {
            min - 0.05 * span
        };
        nice_unit(top - bottom, max_intervals)
    });
    if percent && fixed_major.is_none() {
        major = major.max(0.1);
    }
    if fixed_min.is_none() {
        min = if min == 0.0 {
            0.0
        } else {
            let m = ((min - 0.05 * span) / major).floor() * major;
            if lo >= 0.0 && m < 0.0 { 0.0 } else { m }
        };
    }
    if fixed_max.is_none() {
        let top = if percent && hi <= 1.0 {
            max
        } else {
            headroom(max, span)
        };
        max = (top / major - 1e-9).ceil() * major;
        if hi <= 0.0 && max > 0.0 && auto_max() == 0.0 {
            max = 0.0;
        }
        if max <= min {
            max = min + major;
        }
    }
    let minor = fixed_minor.unwrap_or(major / 5.0);
    Scale {
        min: snap(min, major),
        max: snap(max, major),
        major,
        minor,
        log_base: None,
        reversed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AxisKind, AxisPos};

    #[test]
    fn nice_units() {
        assert_eq!(nice_unit(100.0, 10), 10.0);
        assert_eq!(nice_unit(130.0, 10), 20.0);
        assert_eq!(nice_unit(0.7, 7), 0.1);
        assert_eq!(nice_unit(1400.0, 6), 500.0);
        assert_eq!(nice_unit(-1.0, 5), 1.0);
    }

    #[test]
    fn excel_like_automatic_bounds() {
        // Budget quarters: 0 … 1,320 → 0 … 1,400 by 200.
        let s = value_scale(0.0, 1320.0, None, 8, false);
        assert_eq!((s.min, s.max, s.major), (0.0, 1400.0, 200.0));
        assert_eq!(s.ticks().len(), 8);
        // Data far from zero keeps its own minimum.
        let s = value_scale(1000.0, 1100.0, None, 8, false);
        assert!(s.min >= 980.0 && s.min <= 1000.0, "{s:?}");
        // Negative data.
        let s = value_scale(-40.0, 25.0, None, 8, false);
        assert!(
            s.min <= -40.0 && s.max >= 25.0 && s.ticks().contains(&0.0),
            "{s:?}"
        );
        // Percent stacked.
        let s = value_scale(0.0, 1.0, None, 10, true);
        assert_eq!((s.min, s.max), (0.0, 1.0));
        assert_eq!(s.ticks()[1], 0.1);
        // Constant data.
        let s = value_scale(5.0, 5.0, None, 8, false);
        assert!(s.max > 5.0);
    }

    #[test]
    fn fixed_bounds_log_and_reversed() {
        let mut a = Axis::new(AxisKind::Value, 1, AxisPos::Left, 2);
        a.scaling.min = Some(0.0);
        a.scaling.max = Some(600.0);
        a.major_unit = Some(100.0);
        a.scaling.reversed = Some(true);
        let s = value_scale(120.0, 505.0, Some(&a), 8, false);
        assert_eq!((s.min, s.max, s.major), (0.0, 600.0, 100.0));
        assert_eq!(s.fraction(600.0), 0.0);
        assert_eq!(s.fraction(0.0), 1.0);
        let mut l = Axis::new(AxisKind::Value, 1, AxisPos::Left, 2);
        l.scaling.log_base = Some(10.0);
        let s = value_scale(10.0, 9_000.0, Some(&l), 8, false);
        assert_eq!((s.min, s.max), (10.0, 10_000.0));
        assert_eq!(s.ticks(), vec![10.0, 100.0, 1000.0, 10_000.0]);
        assert!((s.fraction(100.0) - 1.0 / 3.0).abs() < 1e-9);
        assert!(s.fraction(-1.0).is_nan());
        // A hostile unit cannot explode the tick count.
        let mut h = Axis::new(AxisKind::Value, 1, AxisPos::Left, 2);
        h.major_unit = Some(1e-300);
        assert!(value_scale(0.0, 1e300, Some(&h), 8, false).ticks().len() <= MAX_TICKS + 1);
    }
}
