// SPDX-License-Identifier: AGPL-3.0-or-later
//! Financial functions: time value of money, cash-flow analysis, depreciation and securities.
//!
//! Coupon schedules follow Excel's rules (coupon dates step back from maturity in whole months,
//! keeping month-end maturities on month ends); day counts follow the five `basis` conventions.
//! `DOLLARDE`/`DOLLARFR` use exact decimal arithmetic (`rust_decimal`) so fractional prices
//! convert exactly.

use std::str::FromStr;

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use super::datetime::yearfrac;
use super::support::*;
use crate::engine::{Arg, CallContext};
use crate::registry::FunctionSpec;
use crate::values::{
    ErrorValue, Value, date_to_serial, days_in_month, is_leap_year, parse_number_text, to_number,
};

// ── time value of money ────────────────────────────────────────────────────────────────────

pub(crate) fn pmt(r: f64, n: f64, pv: f64, fv: f64, t: f64) -> f64 {
    if r == 0.0 {
        return -(pv + fv) / n;
    }
    let f = (1.0 + r).powf(n);
    -(r * (fv + pv * f)) / ((1.0 + r * t) * (f - 1.0))
}

pub(crate) fn fv(r: f64, n: f64, p: f64, pv: f64, t: f64) -> f64 {
    if r == 0.0 {
        return -(pv + p * n);
    }
    let f = (1.0 + r).powf(n);
    -(pv * f + p * (1.0 + r * t) * (f - 1.0) / r)
}

pub(crate) fn pv(r: f64, n: f64, p: f64, fvv: f64, t: f64) -> f64 {
    if r == 0.0 {
        return -(fvv + p * n);
    }
    let f = (1.0 + r).powf(n);
    -(fvv + p * (1.0 + r * t) * (f - 1.0) / r) / f
}

fn nper(r: f64, p: f64, pvv: f64, fvv: f64, t: f64) -> R<f64> {
    if r == 0.0 {
        return if p == 0.0 {
            Err(ErrorValue::Num)
        } else {
            Ok(-(pvv + fvv) / p)
        };
    }
    let num1 = p * (1.0 + r * t) - fvv * r;
    let den = p * (1.0 + r * t) + pvv * r;
    if den == 0.0 || num1 / den <= 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok((num1 / den).ln() / (1.0 + r).ln())
}

fn ipmt(r: f64, per: f64, n: f64, pvv: f64, fvv: f64, t: f64) -> R<f64> {
    if per < 1.0 || per > n {
        return Err(ErrorValue::Num);
    }
    let p = pmt(r, n, pvv, fvv, t);
    if t == 1.0 && per == 1.0 {
        return Ok(0.0);
    }
    let mut i = fv(r, per - 1.0, p, pvv, t) * r;
    if t == 1.0 {
        i /= 1.0 + r;
    }
    Ok(i)
}

fn rate(n: f64, p: f64, pvv: f64, fvv: f64, t: f64, guess: f64) -> R<f64> {
    if n <= 0.0 {
        return Err(ErrorValue::Num);
    }
    if pvv + p * n + fvv == 0.0 {
        return Ok(0.0);
    }
    let f = |r: f64| {
        if r.abs() < 1e-12 {
            return pvv + p * n + fvv;
        }
        let g = (1.0 + r).powf(n);
        pvv * g + p * (1.0 + r * t) * (g - 1.0) / r + fvv
    };
    let mut r = guess;
    for _ in 0..200 {
        let y = f(r);
        let h = (r.abs() * 1e-7).max(1e-7);
        let d = (f(r + h) - f(r - h)) / (2.0 * h);
        if d == 0.0 || !d.is_finite() {
            break;
        }
        let next = r - y / d;
        if !next.is_finite() || next <= -1.0 {
            break;
        }
        if (next - r).abs() < 1e-12 {
            return Ok(next);
        }
        r = next;
    }
    if f(r).abs() < 1e-7 {
        Ok(r)
    } else {
        Err(ErrorValue::Num)
    }
}

// ── cash flows ─────────────────────────────────────────────────────────────────────────────

fn npv(r: f64, values: &[f64]) -> f64 {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| v / (1.0 + r).powf(i as f64 + 1.0))
        .sum()
}

/// A root of `f` near `guess`: Newton first, then a bracketing scan from the guess outwards
/// with bisection.
fn solve(f: impl Fn(f64) -> f64, df: impl Fn(f64) -> f64, guess: f64) -> R<f64> {
    let mut x = guess;
    for _ in 0..100 {
        let (y, d) = (f(x), df(x));
        if !y.is_finite() || !d.is_finite() || d == 0.0 {
            break;
        }
        let next = x - y / d;
        if !next.is_finite() || next <= -1.0 {
            break;
        }
        if (next - x).abs() <= 1e-12 * next.abs().max(1.0) {
            return Ok(next);
        }
        x = next;
    }
    let mut grid: Vec<f64> = (0..1099).map(|k| -0.99 + f64::from(k) * 0.01).collect();
    grid.sort_by(|a, b| (a - guess).abs().total_cmp(&(b - guess).abs()));
    for a in grid {
        let b = a + 0.01;
        let (fa, fb) = (f(a), f(b));
        if fa.is_finite() && fb.is_finite() && fa * fb <= 0.0 {
            let (mut lo, mut hi, mut flo) = (a, b, fa);
            for _ in 0..200 {
                let m = (lo + hi) / 2.0;
                let fm = f(m);
                if fm == 0.0 {
                    return Ok(m);
                }
                if flo * fm < 0.0 {
                    hi = m;
                } else {
                    lo = m;
                    flo = fm;
                }
            }
            return Ok((lo + hi) / 2.0);
        }
    }
    Err(ErrorValue::Num)
}

fn irr(values: &[f64], guess: f64) -> R<f64> {
    if !values.iter().any(|v| *v > 0.0) || !values.iter().any(|v| *v < 0.0) {
        return Err(ErrorValue::Num);
    }
    let f = |r: f64| {
        values
            .iter()
            .enumerate()
            .map(|(i, v)| v / (1.0 + r).powf(i as f64))
            .sum::<f64>()
    };
    let df = |r: f64| {
        values
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, v)| -(i as f64) * v / (1.0 + r).powf(i as f64 + 1.0))
            .sum::<f64>()
    };
    solve(f, df, guess)
}

fn xnpv(r: f64, values: &[f64], dates: &[f64]) -> f64 {
    let d0 = dates[0];
    values
        .iter()
        .zip(dates)
        .map(|(v, d)| v / (1.0 + r).powf((d - d0) / 365.0))
        .sum()
}

/// Values and dates for `XNPV`/`XIRR`: equal lengths, numbers only, dates truncated and not
/// before the first.
fn flows_and_dates(
    args: &mut [Arg<'_>],
    vi: usize,
    di: usize,
    ctx: &mut dyn CallContext,
) -> R<(Vec<f64>, Vec<f64>)> {
    let v = array_of(args, vi, ctx)?;
    let d = array_of(args, di, ctx)?;
    if v.len() != d.len() {
        return Err(ErrorValue::Num);
    }
    let opts = ctx.coercion();
    let (mut values, mut dates) = (Vec::with_capacity(v.len()), Vec::with_capacity(v.len()));
    for (x, t) in v.values().iter().zip(d.values()) {
        if let Value::Error(e) = x {
            return Err(*e);
        }
        if let Value::Error(e) = t {
            return Err(*e);
        }
        let Value::Number(x) = x else {
            return Err(ErrorValue::Value);
        };
        let tn = match t {
            Value::Number(n) => *n,
            Value::Str(s) => parse_number_text(s, &opts).ok_or(ErrorValue::Value)?,
            _ => return Err(ErrorValue::Value),
        };
        values.push(*x);
        dates.push(tn.floor());
    }
    let d0 = dates.first().copied().ok_or(ErrorValue::Num)?;
    if dates.iter().any(|t| *t < d0 || *t < 0.0) {
        return Err(ErrorValue::Num);
    }
    Ok((values, dates))
}

// ── depreciation ───────────────────────────────────────────────────────────────────────────

fn ddb(cost: f64, salvage: f64, life: f64, period: f64, factor: f64) -> f64 {
    let mut r = factor / life;
    let old_value;
    if r >= 1.0 {
        r = 1.0;
        old_value = if period == 1.0 { cost } else { 0.0 };
    } else {
        old_value = cost * (1.0 - r).powf(period - 1.0);
    }
    let new_value = cost * (1.0 - r).powf(period);
    let d = if new_value < salvage {
        old_value - salvage
    } else {
        old_value - new_value
    };
    d.max(0.0)
}

fn inter_vdb(cost: f64, salvage: f64, life: f64, life1: f64, period: f64, factor: f64) -> f64 {
    let mut vdb = 0.0;
    let int_end = (period - 1e-12).ceil();
    let mut sln = 0.0;
    let mut remaining = cost - salvage;
    let mut now_sln = false;
    let mut i = 1.0;
    while i <= int_end {
        let mut term;
        if now_sln {
            term = sln;
        } else {
            let d = ddb(cost, salvage, life, i, factor);
            sln = remaining / (life1 - (i - 1.0));
            if sln > d {
                term = sln;
                now_sln = true;
            } else {
                term = d;
                remaining -= d;
            }
        }
        if i == int_end {
            term *= period + 1.0 - int_end;
        }
        vdb += term;
        i += 1.0;
    }
    vdb
}

fn vdb(
    cost: f64,
    salvage: f64,
    life: f64,
    start: f64,
    end: f64,
    factor: f64,
    no_switch: f64,
) -> R<f64> {
    if start < 0.0
        || end < start
        || end > life
        || cost < 0.0
        || salvage < 0.0
        || factor <= 0.0
        || life <= 0.0
    {
        return Err(ErrorValue::Num);
    }
    if life > 1e6 {
        // Periods are summed one by one; no real asset has a million periods.
        return Err(ErrorValue::Num);
    }
    let int_start = (start + 1e-12).floor();
    let int_end = (end - 1e-12).ceil();
    if no_switch != 0.0 {
        let mut v = 0.0;
        let mut i = int_start + 1.0;
        while i <= int_end {
            let mut term = ddb(cost, salvage, life, i, factor);
            if i == int_start + 1.0 {
                term *= end.min(int_start + 1.0) - start;
            } else if i == int_end {
                term *= end + 1.0 - int_end;
            }
            v += term;
            i += 1.0;
        }
        return Ok(v);
    }
    let life1 = if life.fract() == 0.0 {
        life
    } else {
        life.floor() + 1.0
    };
    let mut part = 0.0;
    if start != int_start {
        let temp = cost - inter_vdb(cost, salvage, life, life1, int_start, factor);
        part += (start - int_start) * inter_vdb(temp, salvage, life, life - int_start, 1.0, factor);
    }
    if end != int_end {
        let temp_start = int_end - 1.0;
        let temp = cost - inter_vdb(cost, salvage, life, life1, temp_start, factor);
        part += (int_end - end) * inter_vdb(temp, salvage, life, life - temp_start, 1.0, factor);
    }
    let c = cost - inter_vdb(cost, salvage, life, life1, int_start, factor);
    Ok(inter_vdb(
        c,
        salvage,
        life,
        life - int_start,
        int_end - int_start,
        factor,
    ) - part)
}

fn db(cost: f64, salvage: f64, life0: f64, period0: f64, month0: f64) -> R<f64> {
    let (life, period, month) = (life0.trunc(), period0.trunc(), month0.trunc());
    if cost < 0.0 || salvage < 0.0 || life <= 0.0 || period <= 0.0 || !(1.0..=12.0).contains(&month)
    {
        return Err(ErrorValue::Num);
    }
    if period > life + 1.0 || (month == 12.0 && period > life) {
        return Err(ErrorValue::Num);
    }
    if cost == 0.0 {
        return Ok(0.0);
    }
    let r = super::special::js_round((1.0 - (salvage / cost).powf(1.0 / life)) * 1000.0) / 1000.0;
    let mut total = cost * r * month / 12.0;
    if period == 1.0 {
        return Ok(total);
    }
    if period > 10_000.0 {
        // The book value after the first (partial) year falls by (1 − r) a year: closed form, so
        // a life of 10^15 periods costs nothing (short schedules keep the loop, digit for digit).
        let after_first = cost - total;
        return Ok(if period == life + 1.0 {
            after_first * (1.0 - r).powf(life - 1.0) * r * (12.0 - month) / 12.0
        } else {
            after_first * (1.0 - r).powf(period - 2.0) * r
        });
    }
    let mut p = 2.0;
    while p <= period {
        let d = if p == life + 1.0 {
            (cost - total) * r * (12.0 - month) / 12.0
        } else {
            (cost - total) * r
        };
        if p == period {
            return Ok(d);
        }
        total += d;
        p += 1.0;
    }
    Ok(0.0)
}

// ── French depreciation (AMORLINC, AMORDEGRC) ────────────────────────────────────────────────

/// The arguments of `AMORLINC` and `AMORDEGRC`.
struct Amor {
    cost: f64,
    date: f64,
    first: f64,
    salvage: f64,
    period: f64,
    rate: f64,
    basis: f64,
}

impl Amor {
    /// Validates the arguments; returns the whole period and the year fraction of the first
    /// (partial) period.
    fn check(&self, ctx: &dyn CallContext) -> R<(f64, f64)> {
        let basis = self.basis.trunc();
        if self.cost < 0.0
            || self.salvage < 0.0
            || self.period < 0.0
            || self.rate <= 0.0
            || ![0.0, 1.0, 3.0, 4.0].contains(&basis)
        {
            return Err(ErrorValue::Num);
        }
        let first_fraction = yearfrac(self.date.trunc(), self.first.trunc(), basis, ctx)?;
        Ok((self.period.trunc(), first_fraction))
    }
}

/// `AMORLINC`: straight-line depreciation in the French accounting system, with the first period
/// prorated from the purchase date.
fn amorlinc(x: &Amor, ctx: &dyn CallContext) -> R<f64> {
    let (per, first_fraction) = x.check(ctx)?;
    let (cost, rate) = (x.cost, x.rate);
    let full = cost * rate;
    let first_rate = first_fraction * rate * cost;
    let depreciable = cost - x.salvage;
    let full_periods = ((depreciable - first_rate) / full).floor().max(0.0);
    let d = if per == 0.0 {
        first_rate
    } else if per <= full_periods {
        full
    } else if per == full_periods + 1.0 {
        depreciable - full * full_periods - first_rate
    } else {
        0.0
    };
    Ok(d.max(0.0))
}

/// `AMORDEGRC`: declining-balance depreciation in the French system. The rate is multiplied by
/// a coefficient for the asset's life (1/rate): 1.5 for 3–4 years, 2 for 5–6 years, 2.5 beyond;
/// lives strictly between 0 and 1, 1 and 2, 2 and 3 or 4 and 5 years are `#NUM!`, as Excel
/// documents. Depreciation is rounded to whole units; the period before the residual value would
/// be crossed takes half the remaining value, and later periods 0.
fn amordegrc(x: &Amor, ctx: &dyn CallContext) -> R<f64> {
    let (per, first_fraction) = x.check(ctx)?;
    let (cost, rate) = (x.cost, x.rate);
    let life = 1.0 / rate;
    let between = |a: f64, b: f64| life > a && life < b;
    if between(0.0, 1.0) || between(1.0, 2.0) || between(2.0, 3.0) || between(4.0, 5.0) {
        return Err(ErrorValue::Num);
    }
    let coefficient = if life < 3.0 {
        1.0
    } else if life <= 4.0 {
        1.5
    } else if life <= 6.0 {
        2.0
    } else {
        2.5
    };
    let r = rate * coefficient;
    let mut d = (first_fraction * r * cost).round();
    let mut value = cost - d;
    let mut rest = value - x.salvage;
    // The value falls geometrically, so real schedules end within a few hundred periods; a rate
    // so small that a million periods do not reach the residual value is `#NUM!`.
    let mut n = 0.0;
    while n < per {
        if n >= 1e6 {
            return Err(ErrorValue::Num);
        }
        d = (r * value).round();
        rest -= d;
        if rest < 0.0 {
            return Ok(if per - n <= 1.0 {
                (value * 0.5).round()
            } else {
                0.0
            });
        }
        value -= d;
        n += 1.0;
        if d <= 0.0 && n < per {
            return Ok(0.0);
        }
    }
    Ok(d)
}

// ── EUROCONVERT ────────────────────────────────────────────────────────────────────────────

/// The euro and the fixed conversion rates of the currencies it replaced, with the decimals a
/// converted amount is rounded to (Microsoft's calculation precision; currencies that joined
/// after the Euro Currency Tools add-in round to cents).
const EURO_RATES: [(&str, f64, i32); 21] = [
    ("EUR", 1.0, 2),
    ("BEF", 40.3399, 0),
    ("LUF", 40.3399, 0),
    ("DEM", 1.95583, 2),
    ("ESP", 166.386, 0),
    ("FRF", 6.55957, 2),
    ("IEP", 0.787_564, 2),
    ("ITL", 1936.27, 0),
    ("NLG", 2.20371, 2),
    ("ATS", 13.7603, 2),
    ("PTE", 200.482, 1),
    ("FIM", 5.94573, 2),
    ("GRD", 340.75, 0),
    ("SIT", 239.64, 2),
    ("CYP", 0.585_274, 2),
    ("MTL", 0.4293, 2),
    ("SKK", 30.126, 2),
    ("EEK", 15.6466, 2),
    ("LVL", 0.702_804, 2),
    ("LTL", 3.4528, 2),
    ("HRK", 7.5345, 2),
];

fn euro_rate(code: &str) -> R<(f64, i32)> {
    EURO_RATES
        .iter()
        .find(|(c, _, _)| c.eq_ignore_ascii_case(code.trim()))
        .map(|&(_, rate, dec)| (rate, dec))
        .ok_or(ErrorValue::Value)
}

fn euroconvert(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let x = num(args, 0, ctx, 0.0)?;
    let from = text(args, 1, ctx)?;
    let to = text(args, 2, ctx)?;
    let full = flag(args, 3, ctx, false)?;
    let triangulation = if omitted(args, 4) {
        None
    } else {
        let p = int(args, 4, ctx, 0.0)?;
        if p < 3.0 {
            return Err(ErrorValue::Value);
        }
        Some(p.min(15.0))
    };
    let (from_rate, _) = euro_rate(&from)?;
    let (to_rate, to_dec) = euro_rate(&to)?;
    if from.trim().eq_ignore_ascii_case(to.trim()) {
        return Ok(x);
    }
    let result = if from.trim().eq_ignore_ascii_case("EUR") {
        x * to_rate
    } else {
        let mut euros = x / from_rate;
        if let Some(p) = triangulation {
            euros = crate::values::excel_round(euros, p);
        }
        euros * to_rate
    };
    finite(if full {
        result
    } else {
        crate::values::excel_round(result, f64::from(to_dec))
    })
}

// ── securities: day counts and coupon schedules ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Ymd {
    y: i64,
    m: u32,
    d: u32,
}

fn ymd(serial: f64, ctx: &dyn CallContext) -> Ymd {
    let p = super::datetime::parts(serial, ctx);
    Ymd {
        y: i64::from(p.year),
        m: p.month,
        d: p.day,
    }
}

fn serial(p: Ymd, ctx: &dyn CallContext) -> f64 {
    date_to_serial(p.y as f64, f64::from(p.m), f64::from(p.d), ctx.date1904()).unwrap_or(f64::NAN)
}

/// A coupon date `months` from `base`, keeping month-end bases on month ends.
fn shift_months(base: Ymd, months: i64) -> Ymd {
    let total = base.y * 12 + i64::from(base.m) - 1 + months;
    let y = total.div_euclid(12);
    let m = total.rem_euclid(12) as u32 + 1;
    let eom = base.d >= days_in_month(base.y, base.m);
    let d = if eom {
        days_in_month(y, m)
    } else {
        base.d.min(days_in_month(y, m))
    };
    Ymd { y, m, d }
}

/// The previous coupon date (≤ settlement), the next one and the number of coupons left,
/// stepping back from maturity.
fn coupon_dates(settle: Ymd, mat: Ymd, freq: f64) -> (Ymd, Ymd, f64) {
    let step = (12.0 / freq) as i64;
    let mut k = 0i64;
    let mut pcd = mat;
    while pcd > settle {
        k += 1;
        pcd = shift_months(mat, -k * step);
    }
    let ncd = shift_months(mat, -(k - 1) * step);
    (pcd, ncd, k as f64)
}

/// Days between two dates for a basis (the 30/360 variants or actual days).
fn day_count(a: Ymd, b: Ymd, basis: f64, ctx: &dyn CallContext) -> f64 {
    if basis == 0.0 || basis == 4.0 {
        let (mut d1, mut d2) = (a.d, b.d);
        if basis == 0.0 {
            let feb1 = a.m == 2 && d1 == days_in_month(a.y, 2);
            let feb2 = b.m == 2 && d2 == days_in_month(b.y, 2);
            if feb1 && feb2 {
                d2 = 30;
            }
            if feb1 {
                d1 = 30;
            }
            if d2 == 31 && d1 >= 30 {
                d2 = 30;
            }
            if d1 == 31 {
                d1 = 30;
            }
        } else {
            if d1 == 31 {
                d1 = 30;
            }
            if d2 == 31 {
                d2 = 30;
            }
        }
        return (b.y - a.y) as f64 * 360.0
            + (f64::from(b.m) - f64::from(a.m)) * 30.0
            + (f64::from(d2) - f64::from(d1));
    }
    serial(b, ctx) - serial(a, ctx)
}

struct Bond {
    settle: f64,
    s: Ymd,
    m: Ymd,
    freq: f64,
    basis: f64,
}

/// Validates the common settlement / maturity / frequency / basis arguments.
fn bond(settle: f64, mat: f64, freq: f64, basis: f64, ctx: &dyn CallContext) -> R<Bond> {
    let (settle, mat, freq, basis) = (settle.trunc(), mat.trunc(), freq.trunc(), basis.trunc());
    if ![1.0, 2.0, 4.0].contains(&freq)
        || !(0.0..=4.0).contains(&basis)
        || settle >= mat
        || settle < 0.0
        || mat > super::datetime::max_serial(ctx)
    {
        return Err(ErrorValue::Num);
    }
    Ok(Bond {
        settle,
        s: ymd(settle, ctx),
        m: ymd(mat, ctx),
        freq,
        basis,
    })
}

fn coupdays(b: &Bond, ctx: &dyn CallContext) -> f64 {
    if b.basis == 1.0 {
        let (pcd, ncd, _) = coupon_dates(b.s, b.m, b.freq);
        return serial(ncd, ctx) - serial(pcd, ctx);
    }
    (if b.basis == 3.0 { 365.0 } else { 360.0 }) / b.freq
}

fn coupdaybs(b: &Bond, ctx: &dyn CallContext) -> f64 {
    let (pcd, _, _) = coupon_dates(b.s, b.m, b.freq);
    day_count(pcd, b.s, b.basis, ctx)
}

fn coupdaysnc(b: &Bond, ctx: &dyn CallContext) -> f64 {
    if b.basis != 0.0 && b.basis != 4.0 {
        let (_, ncd, _) = coupon_dates(b.s, b.m, b.freq);
        return serial(ncd, ctx) - b.settle;
    }
    coupdays(b, ctx) - coupdaybs(b, ctx)
}

fn price(b: &Bond, rate_v: f64, yld: f64, redemption: f64, ctx: &dyn CallContext) -> f64 {
    let e = coupdays(b, ctx);
    let a = coupdaybs(b, ctx);
    let dsc = coupdaysnc(b, ctx);
    let n = coupon_dates(b.s, b.m, b.freq).2;
    let f = b.freq;
    let c = 100.0 * rate_v / f;
    if n == 1.0 {
        return (redemption + c) / (1.0 + dsc / e * yld / f) - c * a / e;
    }
    let base = 1.0 + yld / f;
    let mut p = redemption / base.powf(n - 1.0 + dsc / e);
    let mut k = 1.0;
    while k <= n {
        p += c / base.powf(k - 1.0 + dsc / e);
        k += 1.0;
    }
    p - c * a / e
}

fn yield_fn(b: &Bond, rate_v: f64, pr: f64, redemption: f64, ctx: &dyn CallContext) -> R<f64> {
    let n = coupon_dates(b.s, b.m, b.freq).2;
    if n == 1.0 {
        let e = coupdays(b, ctx);
        let a = coupdaybs(b, ctx);
        let dsr = e - a;
        let c = rate_v / b.freq;
        let num1 = redemption / 100.0 + c - (pr / 100.0 + a / e * c);
        let den = pr / 100.0 + a / e * c;
        return Ok(num1 / den * b.freq * e / dsr);
    }
    let f = |y: f64| price(b, rate_v, y, redemption, ctx) - pr;
    let df = |y: f64| (f(y + 1e-7) - f(y - 1e-7)) / 2e-7;
    solve(f, df, 0.05)
}

// ── odd first and last periods (ODDFPRICE, ODDFYIELD, ODDLPRICE, ODDLYIELD) ─────────────────

/// The normal length in days of a (quasi-)coupon period `[a, b]` for a basis: actual days for
/// basis 1, 365/frequency for basis 3, 360/frequency otherwise.
fn normal_length(a: Ymd, b: Ymd, freq: f64, basis: f64, ctx: &dyn CallContext) -> f64 {
    match basis as i64 {
        1 => serial(b, ctx) - serial(a, ctx),
        3 => 365.0 / freq,
        _ => 360.0 / freq,
    }
}

/// Days from `a` to `b` for a basis: 30/360 for bases 0 and 4, actual days otherwise.
fn basis_days(a: Ymd, b: Ymd, basis: f64, ctx: &dyn CallContext) -> f64 {
    if basis == 0.0 || basis == 4.0 {
        day_count(a, b, basis, ctx)
    } else {
        serial(b, ctx) - serial(a, ctx)
    }
}

/// The quasi-coupon fraction of `[from, to]` on the coupon schedule through `anchor`: days in the
/// partial periods at either end divided by their normal length, plus 1 for each whole period in
/// between (Microsoft's Σ DCᵢ/NLᵢ, Σ Aᵢ/NLᵢ and Σ DSCᵢ/NLᵢ for odd periods, as Gnumeric computes
/// them).
fn date_ratio(
    from: Ymd,
    to: Ymd,
    anchor: Ymd,
    freq: f64,
    basis: f64,
    ctx: &dyn CallContext,
) -> R<f64> {
    if from >= to {
        return Ok(0.0);
    }
    let step = (12.0 / freq) as i64;
    // The schedule date after `from` (and the one before it).
    let mut k = 0i64;
    let (mut prev, mut next);
    loop {
        next = shift_months(anchor, k * step);
        prev = shift_months(anchor, (k - 1) * step);
        if prev <= from && from < next {
            break;
        }
        k += if next <= from { 1 } else { -1 };
        if k.abs() > 40_000 {
            return Err(ErrorValue::Num);
        }
    }
    if next >= to {
        return Ok(basis_days(from, to, basis, ctx) / normal_length(prev, next, freq, basis, ctx));
    }
    let mut ratio =
        basis_days(from, next, basis, ctx) / normal_length(prev, next, freq, basis, ctx);
    for _ in 0..40_000 {
        prev = next;
        k += 1;
        next = shift_months(anchor, k * step);
        if next >= to {
            return Ok(ratio
                + basis_days(prev, to, basis, ctx) / normal_length(prev, next, freq, basis, ctx));
        }
        ratio += 1.0;
    }
    Err(ErrorValue::Num)
}

struct OddFirst {
    /// Σ DC/NL (issue to first coupon) and Σ A/NL (issue to settlement), in coupon periods.
    dc: f64,
    a: f64,
    /// Coupon periods from settlement to the first coupon.
    t_first: f64,
    /// Coupons after the first one, up to maturity.
    n: f64,
}

#[allow(clippy::too_many_arguments)]
fn odd_first(
    settle: f64,
    mat: f64,
    issue: f64,
    first: f64,
    freq: f64,
    basis: f64,
    ctx: &dyn CallContext,
) -> R<OddFirst> {
    let (settle, mat, issue, first) = (settle.trunc(), mat.trunc(), issue.trunc(), first.trunc());
    let (freq, basis) = (freq.trunc(), basis.trunc());
    if ![1.0, 2.0, 4.0].contains(&freq)
        || !(0.0..=4.0).contains(&basis)
        || !(issue < settle && settle < first && first < mat)
        || issue < 0.0
        || mat > super::datetime::max_serial(ctx)
    {
        return Err(ErrorValue::Num);
    }
    let (s, m, i, f) = (
        ymd(settle, ctx),
        ymd(mat, ctx),
        ymd(issue, ctx),
        ymd(first, ctx),
    );
    let b = Bond {
        settle,
        s,
        m,
        freq,
        basis,
    };
    let e = coupdays(&b, ctx);
    let mut a = basis_days(i, s, basis, ctx);
    let mut ds = basis_days(s, f, basis, ctx);
    let mut df = basis_days(i, f, basis, ctx);
    let mut n = coupon_dates(s, m, freq).2;
    if ds > e {
        // A long first period: count the coupons from the first one, and for the actual-day
        // bases split the odd period into quasi-coupon periods.
        let step = (12.0 / freq) as i64;
        if basis == 0.0 || basis == 4.0 {
            n = 1.0 + (basis_days(f, m, basis, ctx) / e).ceil();
        } else {
            let mut k = 0i64;
            loop {
                let prev = shift_months(f, k * step);
                let next = shift_months(f, (k + 1) * step);
                if next >= m {
                    let tail = basis_days(prev, m, basis, ctx)
                        / normal_length(prev, next, freq, basis, ctx);
                    n = k as f64 + tail.ceil() + 1.0;
                    break;
                }
                k += 1;
                if k > 40_000 {
                    return Err(ErrorValue::Num);
                }
            }
            a = e * date_ratio(i, s, f, freq, basis, ctx)?;
            ds = e * date_ratio(s, f, f, freq, basis, ctx)?;
            df = e * date_ratio(i, f, f, freq, basis, ctx)?;
        }
    }
    Ok(OddFirst {
        dc: df / e,
        a: a / e,
        t_first: ds / e,
        n: n - 1.0,
    })
}

fn oddfprice_of(o: &OddFirst, rate: f64, yld: f64, redemption: f64, freq: f64) -> f64 {
    let c = 100.0 * rate / freq;
    let v = 1.0 + yld / freq;
    let mut p = redemption / v.powf(o.n + o.t_first) + c * o.dc / v.powf(o.t_first);
    let mut k = 1.0;
    while k <= o.n {
        p += c / v.powf(k + o.t_first);
        k += 1.0;
    }
    p - c * o.a
}

struct OddLast {
    dc: f64,
    a: f64,
    dsc: f64,
}

fn odd_last(
    settle: f64,
    mat: f64,
    last: f64,
    freq: f64,
    basis: f64,
    ctx: &dyn CallContext,
) -> R<OddLast> {
    let (settle, mat, last) = (settle.trunc(), mat.trunc(), last.trunc());
    let (freq, basis) = (freq.trunc(), basis.trunc());
    if ![1.0, 2.0, 4.0].contains(&freq)
        || !(0.0..=4.0).contains(&basis)
        || !(last < settle && settle < mat)
        || last < 0.0
        || mat > super::datetime::max_serial(ctx)
    {
        return Err(ErrorValue::Num);
    }
    let (s, m, l) = (ymd(settle, ctx), ymd(mat, ctx), ymd(last, ctx));
    Ok(OddLast {
        dc: date_ratio(l, m, l, freq, basis, ctx)?,
        a: date_ratio(l, s, l, freq, basis, ctx)?,
        dsc: date_ratio(s, m, l, freq, basis, ctx)?,
    })
}

/// Macaulay duration in years: cash flows at (k − 1 + DSC/E) coupon periods from settlement,
/// weighted by their present values.
fn duration(b: &Bond, coupon: f64, yld: f64, ctx: &dyn CallContext) -> f64 {
    let n = coupon_dates(b.s, b.m, b.freq).2;
    let first = coupdaysnc(b, ctx) / coupdays(b, ctx);
    let c = coupon * 100.0 / b.freq;
    let y = 1.0 + yld / b.freq;
    let (mut dur, mut p) = (0.0, 0.0);
    let mut k = 1.0;
    while k <= n {
        let t = k - 1.0 + first;
        let cf = if k == n { c + 100.0 } else { c };
        let pvk = cf / y.powf(t);
        dur += t * pvk;
        p += pvk;
        k += 1.0;
    }
    dur / p / b.freq
}

/// Year fraction between two serials with a validated basis.
fn yf(a: f64, b: f64, basis: f64, ctx: &dyn CallContext) -> R<f64> {
    let bs = basis.trunc();
    if !(0.0..=4.0).contains(&bs) {
        return Err(ErrorValue::Num);
    }
    yearfrac(a.trunc(), b.trunc(), bs, ctx)
}

/// The fraction of a year for the discount-security formulas: days / B, where for basis 1 B is
/// the length of the settlement (or issue) year.
fn day_fraction(a: f64, b: f64, basis: f64, ctx: &dyn CallContext) -> R<f64> {
    let bs = basis.trunc();
    if bs != 1.0 {
        return yf(a, b, bs, ctx);
    }
    let start = super::datetime::check_serial(a.trunc(), ctx)?;
    super::datetime::check_serial(b.trunc(), ctx)?;
    let year = super::datetime::parts(start, ctx).year;
    Ok((b.trunc() - start)
        / if is_leap_year(i64::from(year)) {
            366.0
        } else {
            365.0
        })
}

fn tbill_days(s: f64, m: f64) -> R<f64> {
    let (settle, mat) = (s.trunc(), m.trunc());
    if settle >= mat || mat - settle > 365.0 {
        return Err(ErrorValue::Num);
    }
    Ok(mat - settle)
}

// ── DOLLARDE / DOLLARFR ────────────────────────────────────────────────────────────────────

fn dollar_convert(x: f64, fraction: f64, to_decimal: bool) -> R<f64> {
    let f = fraction.trunc();
    if f < 0.0 {
        return Err(ErrorValue::Num);
    }
    if f == 0.0 {
        return Err(ErrorValue::Div0);
    }
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let digits = f.log10().ceil();
    let exact = (|| -> Option<f64> {
        let ax = Decimal::from_str(&format!("{}", x.abs())).ok()?;
        let fd = Decimal::from_str(&format!("{f}")).ok()?;
        let scale =
            Decimal::from_str(&format!("1{}", "0".repeat(digits.max(0.0) as usize))).ok()?;
        let int_part = ax.floor();
        let frac = ax.checked_sub(int_part)?;
        let res = if to_decimal {
            int_part.checked_add(frac.checked_mul(scale)?.checked_div(fd)?)?
        } else {
            int_part.checked_add(frac.checked_mul(fd)?.checked_div(scale)?)?
        };
        res.to_f64()
    })();
    let r = match exact {
        Some(r) => r,
        None => {
            // Beyond the 28 digits of exact decimals: binary arithmetic.
            let ax = x.abs();
            let (int_part, frac) = (ax.floor(), ax - ax.floor());
            let scale = 10f64.powf(digits);
            if to_decimal {
                int_part + frac * scale / f
            } else {
                int_part + frac * f / scale
            }
        }
    };
    Ok(sign * r)
}

fn npv_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let r = number_of(args, 0, ctx, 0.0)?;
    let xs = numbers(&mut args[1..], ctx)?;
    if r == -1.0 {
        return Err(ErrorValue::Div0);
    }
    Ok(npv(r, &xs))
}

fn xnpv_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let r = number_of(args, 0, ctx, 0.0)?;
    let (values, dates) = flows_and_dates(args, 1, 2, ctx)?;
    if r <= -1.0 {
        return Err(ErrorValue::Num);
    }
    Ok(xnpv(r, &values, &dates))
}

fn irr_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let v = numbers(&mut args[..1], ctx)?;
    let g = number_of(args, 1, ctx, 0.1)?;
    irr(&v, g)
}

fn xirr_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let (values, dates) = flows_and_dates(args, 0, 1, ctx)?;
    let g = number_of(args, 2, ctx, 0.1)?;
    if !values.iter().any(|v| *v > 0.0) || !values.iter().any(|v| *v < 0.0) {
        return Err(ErrorValue::Num);
    }
    let d0 = dates[0];
    let f = |r: f64| xnpv(r, &values, &dates);
    let df = |r: f64| {
        values
            .iter()
            .zip(&dates)
            .map(|(v, d)| {
                let t = (d - d0) / 365.0;
                -t * v / (1.0 + r).powf(t + 1.0)
            })
            .sum::<f64>()
    };
    solve(f, df, g)
}

fn mirr_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let v = numbers(&mut args[..1], ctx)?;
    let fr = number_of(args, 1, ctx, 0.0)?;
    let rr = number_of(args, 2, ctx, 0.0)?;
    let n = v.len() as f64;
    let pos: Vec<f64> = v.iter().map(|x| x.max(0.0)).collect();
    let neg: Vec<f64> = v.iter().map(|x| x.min(0.0)).collect();
    if !pos.iter().any(|x| *x > 0.0) || !neg.iter().any(|x| *x < 0.0) {
        return Err(ErrorValue::Div0);
    }
    let top = -npv(rr, &pos) * (1.0 + rr).powf(n);
    let bottom = npv(fr, &neg) * (1.0 + fr);
    Ok((top / bottom).powf(1.0 / (n - 1.0)) - 1.0)
}

fn fvschedule(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let p = number_of(args, 0, ctx, 0.0)?;
    let arr = array_of(args, 1, ctx)?;
    let mut v = p;
    for x in arr.values() {
        match x {
            Value::Error(e) => return Err(*e),
            Value::Blank => {}
            other => {
                let r = to_number(other, &ctx.coercion()).map_err(|_| ErrorValue::Value)?;
                v *= 1.0 + r;
            }
        }
    }
    Ok(v)
}

fn cum(r: f64, n0: f64, v: f64, s0: f64, e0: f64, t: f64, interest: bool) -> R<f64> {
    let (n, s, e) = (n0.trunc(), s0.trunc(), e0.trunc());
    if r <= 0.0 || n <= 0.0 || v <= 0.0 || s < 1.0 || e < s || e > n || (t != 0.0 && t != 1.0) {
        return Err(ErrorValue::Num);
    }
    let p = pmt(r, n, v, 0.0, t);
    let mut acc = 0.0;
    let mut per = s;
    while per <= e {
        let i = ipmt(r, per, n, v, 0.0, t)?;
        acc += if interest { i } else { p - i };
        per += 1.0;
    }
    Ok(acc)
}

fn flag01(t: f64) -> f64 {
    if t != 0.0 { 1.0 } else { 0.0 }
}

/// The financial functions.
pub fn specs() -> Vec<FunctionSpec> {
    vec![
        numeric_ctx!(
            "AMORLINC",
            6,
            7,
            [0.0],
            |ctx, cost, date, first, salvage, period, rate, basis| {
                let x = Amor {
                    cost,
                    date,
                    first,
                    salvage,
                    period,
                    rate,
                    basis,
                };
                amorlinc(&x, ctx)
            }
        ),
        numeric_ctx!(
            "AMORDEGRC",
            6,
            7,
            [0.0],
            |ctx, cost, date, first, salvage, period, rate, basis| {
                let x = Amor {
                    cost,
                    date,
                    first,
                    salvage,
                    period,
                    rate,
                    basis,
                };
                amordegrc(&x, ctx)
            }
        ),
        spec("EUROCONVERT", 3, 5, imp!(euroconvert)).lift(crate::registry::ArgMask::ALL),
        numeric!("PMT", 3, 5, [0.0, 0.0], |r, n, p, f, t| if n == 0.0 {
            Err(ErrorValue::Num)
        } else {
            Ok(pmt(r, n, p, f, flag01(t)))
        }),
        numeric!("FV", 3, 5, [0.0, 0.0], |r, n, p, v, t| fv(
            r,
            n,
            p,
            v,
            flag01(t)
        )),
        numeric!("PV", 3, 5, [0.0, 0.0], |r, n, p, f, t| pv(
            r,
            n,
            p,
            f,
            flag01(t)
        )),
        numeric!("NPER", 3, 5, [0.0, 0.0], |r, p, v, f, t| nper(
            r,
            p,
            v,
            f,
            flag01(t)
        )),
        numeric!("RATE", 3, 6, [0.0, 0.0, 0.1], |n, p, v, f, t, g| rate(
            n,
            p,
            v,
            f,
            flag01(t),
            g
        )),
        numeric!("IPMT", 4, 6, [0.0, 0.0], |r, per, n, v, f, t| ipmt(
            r,
            per,
            n,
            v,
            f,
            flag01(t)
        )),
        numeric!("PPMT", 4, 6, [0.0, 0.0], |r, per, n, v, f, t| {
            let i = ipmt(r, per, n, v, f, flag01(t))?;
            Ok::<f64, ErrorValue>(pmt(r, n, v, f, flag01(t)) - i)
        }),
        numeric!("CUMIPMT", 6, 6, [], |r, n, v, s, e, t| cum(
            r, n, v, s, e, t, true
        )),
        numeric!("CUMPRINC", 6, 6, [], |r, n, v, s, e, t| cum(
            r, n, v, s, e, t, false
        )),
        numeric!("ISPMT", 4, 4, [], |r, per, n, v| if n == 0.0 {
            Err(ErrorValue::Div0)
        } else {
            Ok(v * r * (per / n - 1.0))
        }),
        spec("NPV", 2, VARIADIC, imp!(npv_fn)),
        spec("XNPV", 3, 3, imp!(xnpv_fn)),
        spec("IRR", 1, 2, imp!(irr_fn)),
        spec("XIRR", 2, 3, imp!(xirr_fn)),
        spec("MIRR", 3, 3, imp!(mirr_fn)),
        spec("FVSCHEDULE", 2, 2, imp!(fvschedule)),
        numeric!("EFFECT", 2, 2, [], |nom, p0| {
            let p = p0.trunc();
            if nom <= 0.0 || p < 1.0 {
                Err(ErrorValue::Num)
            } else {
                Ok((1.0 + nom / p).powf(p) - 1.0)
            }
        }),
        numeric!("NOMINAL", 2, 2, [], |eff, p0| {
            let p = p0.trunc();
            if eff <= 0.0 || p < 1.0 {
                Err(ErrorValue::Num)
            } else {
                Ok(p * ((1.0 + eff).powf(1.0 / p) - 1.0))
            }
        }),
        numeric!(
            "PDURATION",
            3,
            3,
            [],
            |r, a, b| if r <= 0.0 || a <= 0.0 || b <= 0.0 {
                Err(ErrorValue::Num)
            } else {
                Ok((b.ln() - a.ln()) / r.ln_1p())
            }
        ),
        numeric!("RRI", 3, 3, [], |n, a, b| if n <= 0.0 || a == 0.0 {
            Err(ErrorValue::Num)
        } else {
            Ok((b / a).powf(1.0 / n) - 1.0)
        }),
        numeric!("SLN", 3, 3, [], |c, s, l| if l == 0.0 {
            Err(ErrorValue::Div0)
        } else {
            Ok((c - s) / l)
        }),
        numeric!("SYD", 4, 4, [], |c, s, l, per| {
            if l <= 0.0 || per <= 0.0 || per > l || s < 0.0 {
                Err(ErrorValue::Num)
            } else {
                Ok((c - s) * (l - per + 1.0) * 2.0 / (l * (l + 1.0)))
            }
        }),
        numeric!("DB", 4, 5, [12.0], |c, s, l, p, m| db(c, s, l, p, m)),
        numeric!("DDB", 4, 5, [2.0], |c, s, l, per, f| {
            if c < 0.0 || s < 0.0 || l <= 0.0 || per <= 0.0 || per > l || f <= 0.0 {
                Err(ErrorValue::Num)
            } else {
                Ok(ddb(c, s, l, per, f))
            }
        }),
        numeric!("VDB", 5, 7, [2.0, 0.0], |c, s, l, a, b, f, ns| vdb(
            c, s, l, a, b, f, ns
        )),
        numeric!("DOLLARDE", 2, 2, [], |x, f| dollar_convert(x, f, true)),
        numeric!("DOLLARFR", 2, 2, [], |x, f| dollar_convert(x, f, false)),
        numeric_ctx!(
            "ACCRINTM",
            3,
            5,
            [1000.0, 0.0],
            |ctx, issue, s, r, par, basis| {
                if r <= 0.0 || par <= 0.0 || issue.trunc() >= s.trunc() {
                    Err(ErrorValue::Num)
                } else {
                    day_fraction(issue, s, basis, ctx).map(|y| par * r * y)
                }
            }
        ),
        numeric_ctx!(
            "ACCRINT",
            6,
            8,
            [0.0, 1.0],
            |ctx, issue, _first, s, r, par, freq, basis, _method| {
                let f = freq.trunc();
                if r <= 0.0
                    || par <= 0.0
                    || ![1.0, 2.0, 4.0].contains(&f)
                    || issue.trunc() >= s.trunc()
                {
                    Err(ErrorValue::Num)
                } else {
                    yf(issue, s, basis, ctx).map(|y| par * r * y)
                }
            }
        ),
        numeric_ctx!("COUPDAYBS", 3, 4, [0.0], |ctx, s, m, f, b| bond(
            s, m, f, b, ctx
        )
        .map(|bd| coupdaybs(&bd, ctx))),
        numeric_ctx!("COUPDAYS", 3, 4, [0.0], |ctx, s, m, f, b| bond(
            s, m, f, b, ctx
        )
        .map(|bd| coupdays(&bd, ctx))),
        numeric_ctx!("COUPDAYSNC", 3, 4, [0.0], |ctx, s, m, f, b| bond(
            s, m, f, b, ctx
        )
        .map(|bd| coupdaysnc(&bd, ctx))),
        numeric_ctx!("COUPNCD", 3, 4, [0.0], |ctx, s, m, f, b| bond(
            s, m, f, b, ctx
        )
        .map(|bd| serial(coupon_dates(bd.s, bd.m, bd.freq).1, ctx))),
        numeric_ctx!("COUPPCD", 3, 4, [0.0], |ctx, s, m, f, b| bond(
            s, m, f, b, ctx
        )
        .map(|bd| serial(coupon_dates(bd.s, bd.m, bd.freq).0, ctx))),
        numeric_ctx!("COUPNUM", 3, 4, [0.0], |ctx, s, m, f, b| bond(
            s, m, f, b, ctx
        )
        .map(|bd| coupon_dates(bd.s, bd.m, bd.freq).2)),
        numeric_ctx!("PRICE", 6, 7, [0.0], |ctx, s, m, r, y, red, f, b| {
            let bd = bond(s, m, f, b, ctx)?;
            if r < 0.0 || y < 0.0 || red <= 0.0 {
                Err(ErrorValue::Num)
            } else {
                Ok(price(&bd, r, y, red, ctx))
            }
        }),
        numeric_ctx!(
            "ODDFPRICE",
            8,
            9,
            [0.0],
            |ctx, s, m, iss, fc, r, y, red, f, b| {
                if r < 0.0 || y < 0.0 || red <= 0.0 {
                    return Err(ErrorValue::Num);
                }
                let o = odd_first(s, m, iss, fc, f, b, ctx)?;
                Ok(oddfprice_of(&o, r, y, red, f.trunc()))
            }
        ),
        numeric_ctx!(
            "ODDFYIELD",
            8,
            9,
            [0.0],
            |ctx, s, m, iss, fc, r, pr, red, f, b| {
                if r < 0.0 || pr <= 0.0 || red <= 0.0 {
                    return Err(ErrorValue::Num);
                }
                let o = odd_first(s, m, iss, fc, f, b, ctx)?;
                let fr = f.trunc();
                let g = |y: f64| oddfprice_of(&o, r, y, red, fr) - pr;
                let dg = |y: f64| (g(y + 1e-7) - g(y - 1e-7)) / 2e-7;
                solve(g, dg, r.max(0.05))
            }
        ),
        numeric_ctx!(
            "ODDLPRICE",
            7,
            8,
            [0.0],
            |ctx, s, m, last, r, y, red, f, b| {
                if r < 0.0 || y < 0.0 || red <= 0.0 {
                    return Err(ErrorValue::Num);
                }
                let o = odd_last(s, m, last, f, b, ctx)?;
                let c = 100.0 * r / f.trunc();
                Ok((red + o.dc * c) / (1.0 + o.dsc * y / f.trunc()) - o.a * c)
            }
        ),
        numeric_ctx!(
            "ODDLYIELD",
            7,
            8,
            [0.0],
            |ctx, s, m, last, r, pr, red, f, b| {
                if r < 0.0 || pr <= 0.0 || red <= 0.0 {
                    return Err(ErrorValue::Num);
                }
                let o = odd_last(s, m, last, f, b, ctx)?;
                let c = 100.0 * r / f.trunc();
                let dirty = pr + o.a * c;
                Ok((red + o.dc * c - dirty) / dirty * f.trunc() / o.dsc)
            }
        ),
        numeric_ctx!("YIELD", 6, 7, [0.0], |ctx, s, m, r, pr, red, f, b| {
            let bd = bond(s, m, f, b, ctx)?;
            if r < 0.0 || pr <= 0.0 || red <= 0.0 {
                Err(ErrorValue::Num)
            } else {
                yield_fn(&bd, r, pr, red, ctx)
            }
        }),
        numeric_ctx!("DURATION", 5, 6, [0.0], |ctx, s, m, c, y, f, b| {
            let bd = bond(s, m, f, b, ctx)?;
            if c < 0.0 || y < 0.0 {
                Err(ErrorValue::Num)
            } else {
                Ok(duration(&bd, c, y, ctx))
            }
        }),
        numeric_ctx!("MDURATION", 5, 6, [0.0], |ctx, s, m, c, y, f, b| {
            let bd = bond(s, m, f, b, ctx)?;
            if c < 0.0 || y < 0.0 {
                Err(ErrorValue::Num)
            } else {
                Ok(duration(&bd, c, y, ctx) / (1.0 + y / bd.freq))
            }
        }),
        numeric_ctx!("DISC", 4, 5, [0.0], |ctx, s, m, pr, red, b| {
            if pr <= 0.0 || red <= 0.0 || s.trunc() >= m.trunc() {
                Err(ErrorValue::Num)
            } else {
                yf(s, m, b, ctx).map(|y| (red - pr) / red / y)
            }
        }),
        numeric_ctx!("INTRATE", 4, 5, [0.0], |ctx, s, m, inv, red, b| {
            if inv <= 0.0 || red <= 0.0 || s.trunc() >= m.trunc() {
                Err(ErrorValue::Num)
            } else {
                day_fraction(s, m, b, ctx).map(|y| (red - inv) / inv / y)
            }
        }),
        numeric_ctx!("RECEIVED", 4, 5, [0.0], |ctx, s, m, inv, disc, b| {
            if inv <= 0.0 || disc <= 0.0 || s.trunc() >= m.trunc() {
                Err(ErrorValue::Num)
            } else {
                let y = day_fraction(s, m, b, ctx)?;
                let d = 1.0 - disc * y;
                if d == 0.0 {
                    Err(ErrorValue::Num)
                } else {
                    Ok(inv / d)
                }
            }
        }),
        numeric_ctx!("PRICEDISC", 4, 5, [0.0], |ctx, s, m, disc, red, b| {
            if disc <= 0.0 || red <= 0.0 || s.trunc() >= m.trunc() {
                Err(ErrorValue::Num)
            } else {
                day_fraction(s, m, b, ctx).map(|y| red * (1.0 - disc * y))
            }
        }),
        numeric_ctx!("YIELDDISC", 4, 5, [0.0], |ctx, s, m, pr, red, b| {
            if pr <= 0.0 || red <= 0.0 || s.trunc() >= m.trunc() {
                Err(ErrorValue::Num)
            } else {
                yf(s, m, b, ctx).map(|y| (red / pr - 1.0) / y)
            }
        }),
        numeric_ctx!("PRICEMAT", 5, 6, [0.0], |ctx, s, m, issue, r, y, b| {
            if r < 0.0 || y < 0.0 || s.trunc() >= m.trunc() {
                Err(ErrorValue::Num)
            } else {
                let im = yf(issue, m, b, ctx)?;
                let is = yf(issue, s, b, ctx)?;
                let sm = yf(s, m, b, ctx)?;
                Ok(((1.0 + im * r) / (1.0 + sm * y) - is * r) * 100.0)
            }
        }),
        numeric_ctx!("YIELDMAT", 5, 6, [0.0], |ctx, s, m, issue, r, pr, b| {
            if r < 0.0 || pr <= 0.0 || s.trunc() >= m.trunc() {
                Err(ErrorValue::Num)
            } else {
                let im = yf(issue, m, b, ctx)?;
                let is = yf(issue, s, b, ctx)?;
                let sm = yf(s, m, b, ctx)?;
                Ok(((1.0 + im * r) / (pr / 100.0 + is * r) - 1.0) / sm)
            }
        }),
        numeric!("TBILLPRICE", 3, 3, [], |s, m, disc| {
            let dsm = tbill_days(s, m)?;
            if disc <= 0.0 {
                return Err(ErrorValue::Num);
            }
            let p = 100.0 * (1.0 - disc * dsm / 360.0);
            if p <= 0.0 {
                Err(ErrorValue::Num)
            } else {
                Ok(p)
            }
        }),
        numeric!("TBILLYIELD", 3, 3, [], |s, m, pr| {
            let dsm = tbill_days(s, m)?;
            if pr <= 0.0 {
                Err(ErrorValue::Num)
            } else {
                Ok((100.0 - pr) / pr * (360.0 / dsm))
            }
        }),
        numeric!("TBILLEQ", 3, 3, [], |s, m, disc| {
            let dsm = tbill_days(s, m)?;
            if disc <= 0.0 {
                Err(ErrorValue::Num)
            } else if dsm <= 182.0 {
                Ok(365.0 * disc / (360.0 - disc * dsm))
            } else {
                let p = 100.0 * (1.0 - disc * dsm / 360.0);
                if p <= 0.0 {
                    Err(ErrorValue::Num)
                } else {
                    let t = dsm / 365.0;
                    Ok(
                        (-2.0 * t + 2.0 * (t * t - (2.0 * t - 1.0) * (1.0 - 100.0 / p)).sqrt())
                            / (2.0 * t - 1.0),
                    )
                }
            }
        }),
    ]
}

#[cfg(test)]
mod db_tests {
    use super::db;

    #[test]
    fn long_schedules_use_the_closed_form() {
        // Both sides of the switch agree with the period-by-period loop.
        for (life, period) in [
            (20_000.0, 10_000.0),
            (20_000.0, 10_001.0),
            (10_500.0, 10_501.0),
        ] {
            let r = super::super::special::js_round((1.0 - (0.5f64).powf(1.0 / life)) * 1000.0)
                / 1000.0;
            let mut total = 1e6 * r * 6.0 / 12.0;
            let mut d = 0.0;
            let mut p = 2.0;
            while p <= period {
                d = if p == life + 1.0 {
                    (1e6 - total) * r * 6.0 / 12.0
                } else {
                    (1e6 - total) * r
                };
                total += d;
                p += 1.0;
            }
            let got = db(1e6, 5e5, life, period, 6.0).unwrap_or(f64::NAN);
            assert!(
                (got - d).abs() <= 1e-9 * d.abs(),
                "{life} {period}: {got} vs {d}"
            );
        }
        assert!(db(1.0, 0.5, 1e15, 4_294_967_296.0, 4.0).is_ok());
    }
}
