// SPDX-License-Identifier: AGPL-3.0-or-later
//! Date & Time functions on serial numbers in the 1900 date system (with the Lotus 1900-02-29) or
//! the 1904 system, following the workbook setting.

use rustc_hash::FxHashSet;

use super::support::*;
use crate::engine::{Arg, CallContext};
use crate::registry::{ArgMask, FunctionSpec};
use crate::values::{
    DateParts, ErrorValue, MAX_DATE_SERIAL_1900, MAX_DATE_SERIAL_1904, Value, date_to_serial,
    days_in_month, is_leap_year, parse_date_time_text, serial_to_date, serial_weekday, to_number,
};

pub(crate) fn max_serial(ctx: &dyn CallContext) -> f64 {
    if ctx.date1904() {
        MAX_DATE_SERIAL_1904
    } else {
        MAX_DATE_SERIAL_1900
    }
}

/// A date serial from a value: numbers, date text, blanks (0); out of range is `#NUM!`.
fn serial_of_value(v: &Value, ctx: &dyn CallContext) -> R<f64> {
    let n = to_number(v, &ctx.coercion())?;
    if n < 0.0 || n >= max_serial(ctx) + 1.0 {
        return Err(ErrorValue::Num);
    }
    Ok(n)
}

/// A date serial from an argument (absent and empty arguments are 0).
fn serial_arg(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<f64> {
    let v = scalar(args, i, ctx);
    serial_of_value(&v, ctx)
}

/// `#NUM!` unless `serial` is a date of the workbook's calendar (0 to 9999-12-31).
pub(crate) fn check_serial(serial: f64, ctx: &dyn CallContext) -> R<f64> {
    if (0.0..max_serial(ctx) + 1.0).contains(&serial) {
        Ok(serial)
    } else {
        Err(ErrorValue::Num)
    }
}

/// The calendar date of a serial. Callers validate their arguments; the clamp keeps this total
/// (no overflow on a serial that slipped through).
pub(crate) fn parts(serial: f64, ctx: &dyn CallContext) -> DateParts {
    let s = if serial.is_nan() {
        0.0
    } else {
        serial.clamp(0.0, max_serial(ctx))
    };
    serial_to_date(s, ctx.date1904())
}

fn serial(y: i64, m: u32, d: u32, ctx: &dyn CallContext) -> R<f64> {
    date_to_serial(y as f64, f64::from(m), f64::from(d), ctx.date1904()).ok_or(ErrorValue::Num)
}

/// Hour, minute and second of a serial, rounded to the nearest second but never into the next
/// day (0.999999 is 23:59:59).
fn hms(serial: f64) -> (f64, f64, f64) {
    let mut secs = ((serial - serial.floor()) * 86_400.0 + 0.5).floor();
    if secs >= 86_400.0 {
        secs = 86_399.0;
    }
    let h = (secs / 3600.0).floor();
    secs -= h * 3600.0;
    let m = (secs / 60.0).floor();
    (h, m, secs - m * 60.0)
}

// ── EDATE / EOMONTH ────────────────────────────────────────────────────────────────────────

fn add_months(serial_n: f64, months: f64, ctx: &dyn CallContext, end_of_month: bool) -> R<f64> {
    let p = parts(serial_n, ctx);
    let total = f64::from(p.year) * 12.0 + f64::from(p.month) - 1.0 + months;
    let y = (total / 12.0).floor();
    let m = (total - y * 12.0) as u32 + 1;
    if !(1900.0..=9999.0).contains(&y) {
        return Err(ErrorValue::Num);
    }
    let y = y as i64;
    let dim = days_in_month(y, m);
    let d = if end_of_month {
        dim
    } else {
        (if p.day == 0 { 1 } else { p.day }).min(dim)
    };
    // The fictitious 1900-02-29 exists only as serial 60.
    if !ctx.date1904() && y == 1900 && m == 2 && d == 29 {
        return Ok(60.0);
    }
    serial(y, m, d, ctx)
}

// ── DATEDIF ────────────────────────────────────────────────────────────────────────────────

fn datedif(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let s = serial_arg(args, 0, ctx)?;
    let e = serial_arg(args, 1, ctx)?;
    let unit = text(args, 2, ctx)?;
    let (start, end) = (s.floor(), e.floor());
    if start > end {
        return Err(ErrorValue::Num);
    }
    let (a, b) = (parts(start, ctx), parts(end, ctx));
    let months = f64::from(b.year - a.year) * 12.0 + f64::from(b.month)
        - f64::from(a.month)
        - if b.day < a.day { 1.0 } else { 0.0 };
    match unit.to_uppercase().as_str() {
        "D" => Ok(end - start),
        "M" => Ok(months),
        "Y" => Ok((months / 12.0).floor()),
        "YM" => Ok(months % 12.0),
        // Days left after moving the start forward by the whole months (clamped to the month
        // end).
        "MD" => Ok(end - add_months(start, months, ctx, false)?),
        "YD" => {
            let years = (months / 12.0).floor();
            Ok(end - add_months(start, years * 12.0, ctx, false)?)
        }
        _ => Err(ErrorValue::Num),
    }
}

// ── WEEKDAY / WEEKNUM ──────────────────────────────────────────────────────────────────────

/// First day of the week (0 = Sunday) and numbering base for a `WEEKDAY` return type.
fn weekday_type(t: f64) -> Option<(u32, f64)> {
    Some(match t as i64 {
        1 => (0, 1.0),
        2 => (1, 1.0),
        3 => (1, 0.0),
        11 => (1, 1.0),
        12 => (2, 1.0),
        13 => (3, 1.0),
        14 => (4, 1.0),
        15 => (5, 1.0),
        16 => (6, 1.0),
        17 => (0, 1.0),
        _ => return None,
    })
}

fn weeknum_first(t: f64) -> Option<u32> {
    Some(match t as i64 {
        1 | 17 => 0,
        2 | 11 => 1,
        12 => 2,
        13 => 3,
        14 => 4,
        15 => 5,
        16 => 6,
        _ => return None,
    })
}

fn iso_weeknum(serial_n: f64, ctx: &dyn CallContext) -> R<f64> {
    let n = serial_n.floor();
    let wd = f64::from((serial_weekday(n, ctx.date1904()) + 6) % 7); // Monday = 0
    let thursday = n - wd + 3.0;
    let year = parts(thursday, ctx).year;
    let jan1 = serial(i64::from(year), 1, 1, ctx)?;
    Ok(((thursday - jan1) / 7.0).floor() + 1.0)
}

fn weekday(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let s = serial_arg(args, 0, ctx)?;
    let t = int(args, 1, ctx, 1.0)?;
    let (first, base) = weekday_type(t).ok_or(ErrorValue::Num)?;
    let wd = serial_weekday(s.floor(), ctx.date1904());
    Ok(f64::from((wd + 7 - first) % 7) + base)
}

fn weeknum(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let s = serial_arg(args, 0, ctx)?;
    let t = int(args, 1, ctx, 1.0)?;
    if t == 21.0 {
        return iso_weeknum(s, ctx);
    }
    let first = weeknum_first(t).ok_or(ErrorValue::Num)?;
    let n = s.floor();
    let year = parts(n, ctx).year;
    let jan1 = serial(i64::from(year), 1, 1, ctx)?;
    let offset = f64::from((serial_weekday(jan1, ctx.date1904()) + 7 - first) % 7);
    Ok(((n - jan1 + offset) / 7.0).floor() + 1.0)
}

// ── working days ───────────────────────────────────────────────────────────────────────────

/// A 7-element weekend mask indexed by weekday (0 = Sunday).
fn weekend_mask(args: &mut [Arg<'_>], i: Option<usize>, ctx: &mut dyn CallContext) -> R<[bool; 7]> {
    let v = match i {
        Some(i) if !omitted(args, i) => {
            let v = value_at(args, i, ctx);
            ctx.to_scalar(v)
        }
        _ => Value::Number(1.0),
    };
    let mut mask = [false; 7];
    match v {
        Value::Error(e) => Err(e),
        Value::Str(s) => {
            let b = s.as_bytes();
            if b.len() != 7 || !b.iter().all(|c| *c == b'0' || *c == b'1') || &*s == "1111111" {
                return Err(ErrorValue::Value);
            }
            // Positions are Monday … Sunday.
            for (k, c) in b.iter().enumerate() {
                mask[(k + 1) % 7] = *c == b'1';
            }
            Ok(mask)
        }
        other => {
            let other = if other.is_blank() {
                Value::Number(1.0)
            } else {
                other
            };
            let code = to_number(&other, &ctx.coercion())?.trunc();
            if (1.0..=7.0).contains(&code) {
                // 1: Saturday+Sunday, 2: Sunday+Monday … 7: Friday+Saturday.
                let first = (code as usize + 5) % 7;
                mask[first] = true;
                mask[(first + 1) % 7] = true;
                Ok(mask)
            } else if (11.0..=17.0).contains(&code) {
                mask[code as usize - 11] = true;
                Ok(mask)
            } else {
                Err(ErrorValue::Num)
            }
        }
    }
}

fn holiday_set(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<FxHashSet<i64>> {
    let mut out = FxHashSet::default();
    if omitted(args, i) {
        return Ok(out);
    }
    for v in scalars_of(args, i, ctx) {
        match v {
            Value::Blank => {}
            Value::Error(e) => return Err(e),
            other => {
                let n = to_number(&other, &ctx.coercion())?;
                if n < 0.0 {
                    return Err(ErrorValue::Num);
                }
                out.insert(n.floor() as i64);
            }
        }
    }
    Ok(out)
}

fn networkdays(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, intl: bool) -> R<f64> {
    let s = serial_arg(args, 0, ctx)?;
    let e = serial_arg(args, 1, ctx)?;
    let mask = weekend_mask(args, intl.then_some(2), ctx)?;
    let hol = holiday_set(args, if intl { 3 } else { 2 }, ctx)?;
    let (mut a, mut b) = (s.floor() as i64, e.floor() as i64);
    let sign = if a <= b { 1.0 } else { -1.0 };
    if a > b {
        std::mem::swap(&mut a, &mut b);
    }
    let date1904 = ctx.date1904();
    let count = (a..=b)
        .filter(|&d| !mask[serial_weekday(d as f64, date1904) as usize] && !hol.contains(&d))
        .count();
    Ok(sign * count as f64)
}

fn workday(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, intl: bool) -> R<f64> {
    let s = serial_arg(args, 0, ctx)?;
    let nv = scalar(args, 1, ctx);
    let n = to_number(&nv, &ctx.coercion())?;
    let mask = weekend_mask(args, intl.then_some(2), ctx)?;
    let hol = holiday_set(args, if intl { 3 } else { 2 }, ctx)?;
    let mut days = n.trunc() as i64;
    let mut d = s.floor() as i64;
    let step = if days >= 0 { 1 } else { -1 };
    let max = max_serial(ctx) as i64;
    let date1904 = ctx.date1904();
    while days != 0 {
        d += step;
        if d < 0 || d > max {
            return Err(ErrorValue::Num);
        }
        if !mask[serial_weekday(d as f64, date1904) as usize] && !hol.contains(&d) {
            days -= step;
        }
    }
    Ok(d as f64)
}

// ── DAYS360 / YEARFRAC ─────────────────────────────────────────────────────────────────────

fn last_day_of_feb(y: i32, m: u32, d: u32) -> bool {
    m == 2 && d == days_in_month(i64::from(y), 2)
}

/// `DAYS360` between two dates (US/NASD or European method).
pub(crate) fn days360(a: DateParts, b: DateParts, european: bool) -> f64 {
    let (mut d1, mut d2) = (a.day, b.day);
    if european {
        if d1 == 31 {
            d1 = 30;
        }
        if d2 == 31 {
            d2 = 30;
        }
    } else {
        if d1 == 31 || last_day_of_feb(a.year, a.month, a.day) {
            d1 = 30;
        }
        if d2 == 31 && d1 == 30 {
            d2 = 30;
        }
    }
    f64::from(b.year - a.year) * 360.0
        + (f64::from(b.month) - f64::from(a.month)) * 30.0
        + (f64::from(d2) - f64::from(d1))
}

/// US (NASD) 30/360 as `YEARFRAC` basis 0 uses it: both February month ends adjust.
fn days360_us(a: DateParts, b: DateParts) -> f64 {
    let (mut d1, mut d2) = (a.day, b.day);
    let feb1 = last_day_of_feb(a.year, a.month, a.day);
    let feb2 = last_day_of_feb(b.year, b.month, b.day);
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
    f64::from(b.year - a.year) * 360.0
        + (f64::from(b.month) - f64::from(a.month)) * 30.0
        + (f64::from(d2) - f64::from(d1))
}

fn year_len(y: i32) -> f64 {
    if is_leap_year(i64::from(y)) {
        366.0
    } else {
        365.0
    }
}

/// `YEARFRAC` for whole-day serials and a basis 0–4.
pub(crate) fn yearfrac(s: f64, e: f64, basis: f64, ctx: &dyn CallContext) -> R<f64> {
    let (mut start, mut end) = (check_serial(s, ctx)?.floor(), check_serial(e, ctx)?.floor());
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }
    let (a, b) = (parts(start, ctx), parts(end, ctx));
    if basis.fract() != 0.0 {
        return Err(ErrorValue::Num);
    }
    match basis as i64 {
        0 => Ok(days360_us(a, b) / 360.0),
        1 => {
            if start == end {
                return Ok(0.0);
            }
            let same_year = a.year == b.year;
            let within_year = same_year
                || (b.year == a.year + 1
                    && (b.month < a.month || (b.month == a.month && b.day <= a.day)));
            if within_year {
                let leap = |y: i32| is_leap_year(i64::from(y));
                let denom = if same_year {
                    year_len(a.year)
                } else if (leap(a.year) && a.month < 3)
                    || (leap(b.year) && (b.month > 2 || (b.month == 2 && b.day == 29)))
                {
                    366.0
                } else {
                    365.0
                };
                return Ok((end - start) / denom);
            }
            let total: f64 = (a.year..=b.year).map(year_len).sum();
            Ok((end - start) / (total / f64::from(b.year - a.year + 1)))
        }
        2 => Ok((end - start) / 360.0),
        3 => Ok((end - start) / 365.0),
        4 => Ok(days360(a, b, true) / 360.0),
        _ => Err(ErrorValue::Num),
    }
}

fn datevalue(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, time: bool) -> R<f64> {
    match scalar(args, 0, ctx) {
        Value::Error(e) => Err(e),
        Value::Str(s) => {
            let n = parse_date_time_text(&s, &ctx.coercion()).ok_or(ErrorValue::Value)?;
            Ok(if time { n - n.floor() } else { n.floor() })
        }
        _ => Err(ErrorValue::Value),
    }
}

fn part(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, which: u8) -> R<f64> {
    let s = serial_arg(args, 0, ctx)?;
    let p = parts(s, ctx);
    Ok(match which {
        0 => f64::from(p.year),
        1 => f64::from(p.month),
        _ => f64::from(p.day),
    })
}

fn time_part(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, which: u8) -> R<f64> {
    let v = scalar(args, 0, ctx);
    let n = to_number(&v, &ctx.coercion())?;
    if n < 0.0 {
        return Err(ErrorValue::Num);
    }
    let (h, m, s) = hms(n);
    Ok(match which {
        0 => h,
        1 => m,
        _ => s,
    })
}

fn time(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let h = int(args, 0, ctx, 0.0)?;
    let m = int(args, 1, ctx, 0.0)?;
    let s = int(args, 2, ctx, 0.0)?;
    if h > 32767.0 || m > 32767.0 || s > 32767.0 {
        return Err(ErrorValue::Num);
    }
    let total = h * 3600.0 + m * 60.0 + s;
    if total < 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok(total % 86_400.0 / 86_400.0)
}

fn date(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let y = int(args, 0, ctx, 0.0)?;
    let m = int(args, 1, ctx, 0.0)?;
    let d = int(args, 2, ctx, 0.0)?;
    date_to_serial(y, m, d, ctx.date1904()).ok_or(ErrorValue::Num)
}

fn edate(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, eom: bool) -> R<f64> {
    let s = serial_arg(args, 0, ctx)?;
    let m = int(args, 1, ctx, 0.0)?;
    add_months(s.floor(), m, ctx, eom)
}

fn days(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let e = serial_arg(args, 0, ctx)?;
    let s = serial_arg(args, 1, ctx)?;
    Ok(e.floor() - s.floor())
}

fn days360_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let s = serial_arg(args, 0, ctx)?;
    let e = serial_arg(args, 1, ctx)?;
    let eu = num(args, 2, ctx, 0.0)?;
    Ok(days360(parts(s, ctx), parts(e, ctx), eu != 0.0))
}

fn yearfrac_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let s = serial_arg(args, 0, ctx)?;
    let e = serial_arg(args, 1, ctx)?;
    let b = int(args, 2, ctx, 0.0)?;
    yearfrac(s, e, b, ctx)
}

/// The date and time functions.
pub fn specs() -> Vec<FunctionSpec> {
    let all = ArgMask::ALL;
    vec![
        spec("TODAY", 0, 0, imp!(|_a, c| c.now().floor())).volatile(),
        spec("NOW", 0, 0, imp!(|_a, c| c.now())).volatile(),
        spec("DATE", 3, 3, imp!(date)).lift(all),
        spec("DATEVALUE", 1, 1, imp!(|a, c| datevalue(a, c, false))).lift(ArgMask::of(&[0])),
        spec("TIMEVALUE", 1, 1, imp!(|a, c| datevalue(a, c, true))).lift(ArgMask::of(&[0])),
        spec("YEAR", 1, 1, imp!(|a, c| part(a, c, 0))).lift(all),
        spec("MONTH", 1, 1, imp!(|a, c| part(a, c, 1))).lift(all),
        spec("DAY", 1, 1, imp!(|a, c| part(a, c, 2))).lift(all),
        spec("HOUR", 1, 1, imp!(|a, c| time_part(a, c, 0))).lift(all),
        spec("MINUTE", 1, 1, imp!(|a, c| time_part(a, c, 1))).lift(all),
        spec("SECOND", 1, 1, imp!(|a, c| time_part(a, c, 2))).lift(all),
        spec("TIME", 3, 3, imp!(time)).lift(all),
        spec("WEEKDAY", 1, 2, imp!(weekday)).lift(all),
        spec("WEEKNUM", 1, 2, imp!(weeknum)).lift(all),
        spec(
            "ISOWEEKNUM",
            1,
            1,
            imp!(|a, c| serial_arg(a, 0, c).and_then(|s| iso_weeknum(s, c))),
        )
        .lift(all),
        spec("EDATE", 2, 2, imp!(|a, c| edate(a, c, false))).lift(all),
        spec("EOMONTH", 2, 2, imp!(|a, c| edate(a, c, true))).lift(all),
        spec("DAYS", 2, 2, imp!(days)).lift(all),
        spec("DAYS360", 2, 3, imp!(days360_fn)).lift(all),
        spec("YEARFRAC", 2, 3, imp!(yearfrac_fn)).lift(all),
        spec("DATEDIF", 3, 3, imp!(datedif)).lift(all),
        spec("NETWORKDAYS", 2, 3, imp!(|a, c| networkdays(a, c, false))),
        spec(
            "NETWORKDAYS.INTL",
            2,
            4,
            imp!(|a, c| networkdays(a, c, true)),
        ),
        spec("WORKDAY", 2, 3, imp!(|a, c| workday(a, c, false))),
        spec("WORKDAY.INTL", 2, 4, imp!(|a, c| workday(a, c, true))),
    ]
}
