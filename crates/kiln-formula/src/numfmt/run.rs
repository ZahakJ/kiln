// SPDX-License-Identifier: AGPL-3.0-or-later
//! Renders a value through one section of a format code.

use super::section::{DateKind, DateSystem, NumKind, Out, Part, Rule, U_CSEC, U_DSEC, U_MSEC};
use crate::values::{excel_round, round_significant};

const DAYSIZE: f64 = 86_400.0;
/// The first serial after 9999-12-31 in the 1900 system.
const MAX_SERIAL: f64 = 2_958_466.0;

/// Text the formatter needs for one locale.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Locale {
    pub(crate) group: &'static str,
    pub(crate) decimal: &'static str,
    pub(crate) positive: &'static str,
    pub(crate) negative: &'static str,
    pub(crate) exponent: &'static str,
    pub(crate) ampm: [&'static str; 2],
    pub(crate) mmmm: [&'static str; 12],
    pub(crate) mmm: [&'static str; 12],
    pub(crate) mmmm6: [&'static str; 12],
    pub(crate) mmm6: [&'static str; 12],
    pub(crate) dddd: [&'static str; 7],
    pub(crate) ddd: [&'static str; 7],
}

pub(crate) const EN: Locale = Locale {
    group: ",",
    decimal: ".",
    positive: "+",
    negative: "-",
    exponent: "E",
    ampm: ["AM", "PM"],
    mmmm: [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ],
    mmm: [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    mmmm6: [
        "Muharram",
        "Safar",
        "Rabiʻ I",
        "Rabiʻ II",
        "Jumada I",
        "Jumada II",
        "Rajab",
        "Shaʻban",
        "Ramadan",
        "Shawwal",
        "Dhuʻl-Qiʻdah",
        "Dhuʻl-Hijjah",
    ],
    mmm6: [
        "Muh.",
        "Saf.",
        "Rab. I",
        "Rab. II",
        "Jum. I",
        "Jum. II",
        "Raj.",
        "Sha.",
        "Ram.",
        "Shaw.",
        "Dhuʻl-Q.",
        "Dhuʻl-H.",
    ],
    dddd: [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ],
    ddd: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
};

pub(crate) const EN_IE: Locale = Locale {
    mmm: [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sept", "Oct", "Nov", "Dec",
    ],
    ..EN
};

/// Locale data for a resolved tag: English variants only for now (other languages format with
/// the default English data until the localisation work package adds theirs).
pub(crate) fn locale_data(tag: &str) -> Option<&'static Locale> {
    let lang = tag.split(['_', '-']).next().unwrap_or("");
    if !lang.eq_ignore_ascii_case("en") {
        return None;
    }
    if tag.eq_ignore_ascii_case("en_IE") {
        Some(&EN_IE)
    } else {
        Some(&EN)
    }
}

/// Rendering options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormatOptions {
    /// Use the 1904 date system.
    pub date1904: bool,
    /// Emit non-breaking spaces for `?` padding and `_x` skips.
    pub nbsp: bool,
}

/// Why a value could not be formatted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatError {
    /// The format code is invalid.
    #[error("invalid number format: {0}")]
    InvalidPattern(String),
    /// A date or time format was applied to a number outside the date range (Excel shows
    /// `#####`; `TEXT` returns `#VALUE!`).
    #[error("the value is outside the date range")]
    DateOutOfRange,
}

fn round(n: f64, places: usize) -> f64 {
    excel_round(n, places as f64)
}

/// JavaScript's `Math.round` (halves round towards positive infinity), exact for large values.
fn round0(n: f64) -> f64 {
    let f = n.floor();
    if n - f >= 0.5 { f + 1.0 } else { f }
}

/// Decimal text of a finite number without exponent.
fn plain(v: f64) -> String {
    let s = format!("{v}");
    if s == "-0" { "0".to_owned() } else { s }
}

/// `x.toFixed(f)` for non-negative `x`: exactly `f` decimals, ties rounded up.
pub(crate) fn to_fixed(x: f64, f: usize) -> String {
    let long = format!("{:.*}", f + 25, x.abs());
    let (int, frac) = long.split_once('.').unwrap_or((&long, ""));
    let mut digits: Vec<u8> = int
        .bytes()
        .chain(frac.bytes().take(f))
        .map(|b| b - b'0')
        .collect();
    let round_up = frac.as_bytes().get(f).is_some_and(|&d| d >= b'5');
    if round_up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, 1);
                break;
            }
            i -= 1;
            if digits[i] == 9 {
                digits[i] = 0;
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let int_len = digits.len() - f;
    let mut out: String = digits[..int_len]
        .iter()
        .map(|d| char::from(b'0' + d))
        .collect();
    if f > 0 {
        out.push('.');
        out.extend(digits[int_len..].iter().map(|d| char::from(b'0' + d)));
    }
    if x < 0.0 && out.bytes().any(|b| b != b'0' && b != b'.') {
        out.insert(0, '-');
    }
    out
}

/// `x.toPrecision(p)` for `1e-6 <= x < 1` (fixed notation).
fn to_precision_small(x: f64, p: usize) -> String {
    let r = round_significant(x, p);
    // Decimals needed: p significant digits after the leading zeros.
    let exp = r.log10().floor() as i32;
    let decimals = (p as i32 - 1 - exp).max(0) as usize;
    to_fixed(r, decimals)
}

fn exponent_of(n: f64, int_max: usize) -> i32 {
    let exp = n.log10().floor() as i32;
    if int_max > 1 {
        let m = int_max as i32;
        exp.div_euclid(m) * m
    } else {
        exp
    }
}

fn significand(n: f64, exp: i32) -> f64 {
    if exp < -300 {
        let s = format!("{n:e}");
        return s
            .split('e')
            .next()
            .and_then(|m| m.parse().ok())
            .unwrap_or(n);
    }
    n * 10f64.powi(-exp)
}

#[derive(Debug, Default, Clone, Copy)]
struct NumDec {
    total: i32,
    frac: i32,
}

/// The count of characters (sign, digits, point) needed to show a number in full.
fn numdec(value: f64) -> NumDec {
    let v = value.abs();
    if v == 0.0 {
        return NumDec { total: 1, frac: 0 };
    }
    let sign = i32::from(value < 0.0);
    let int_part = v.floor();
    let int_size = (v.log10() + 1.0).floor() as i32;
    let mut period = 0;
    let mut frac = 0;
    if int_part != v {
        period = 1;
        let n = plain(round_significant(v * 10f64.powi(-int_size), 15).min(1.0));
        let n: Vec<char> = n.chars().collect();
        let mut f = n.len() as i32;
        let mut z = true;
        for c in &n {
            if *c == '.' {
                f -= 1;
                break;
            } else if *c == '0' && z {
                f -= 1;
            } else {
                z = false;
            }
        }
        frac = f - int_size;
        if frac < 0 {
            frac = 0;
            period = 0;
        }
    }
    NumDec {
        total: sign + int_size.max(1) + period + frac,
        frac,
    }
}

fn exp_text(n: f64, exp: i32, l: &Locale) -> String {
    let m = if n == 1.0 { 1.0 } else { round(n, 5) };
    let x = exp.abs();
    format!(
        "{}{}{}{}{x}",
        plain(m).replace('.', l.decimal),
        l.exponent,
        if exp < 0 { l.negative } else { l.positive },
        if x < 10 { "0" } else { "" }
    )
}

/// `General`: the number in full when it fits 11 characters, otherwise rounded or scientific.
pub(crate) fn general(value: f64, l: &Locale) -> String {
    let v = value.abs();
    if v.fract() == 0.0 && v < 1e10 {
        return plain(v);
    }
    let mut exp = v.log10().floor() as i32;
    let mut n = significand(v, exp);
    if n == 10.0 {
        n = 1.0;
        exp += 1;
    }
    let num_dig = numdec(v);
    if (-4..=-1).contains(&exp) {
        let s = to_precision_small(v, (10 + exp) as usize);
        let s = if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            s
        };
        s.replace('.', l.decimal)
    } else if exp == 10 {
        plain(round(v, 0))
    } else if exp.abs() <= 9 {
        if num_dig.total <= 11 {
            to_fixed(round(v, 9), num_dig.frac.max(0) as usize).replace('.', l.decimal)
        } else if exp == 9 {
            plain(round(v, 0))
        } else if exp >= 0 {
            plain(round(v, (9 - exp) as usize)).replace('.', l.decimal)
        } else {
            exp_text(n, exp, l)
        }
    } else if num_dig.total >= 12 {
        exp_text(n, exp, l)
    } else {
        to_fixed(round(v, 9), num_dig.frac.max(0) as usize).replace('.', l.decimal)
    }
}

/// Excel's serial → (year, month, day) in the 1900 system with the Lotus leap day.
fn to_ymd_1900(ord: i64) -> (i64, i64, i64) {
    if ord >= 0 {
        if ord == 0 {
            return (1900, 1, 0);
        }
        if ord == 60 {
            return (1900, 2, 29);
        }
        if ord < 60 {
            return (1900, if ord < 32 { 1 } else { 2 }, ((ord - 1) % 31) + 1);
        }
    }
    let mut l = ord + 68_569 + 2_415_019;
    let n = (4 * l).div_euclid(146_097);
    l -= (146_097 * n + 3).div_euclid(4);
    let i = (4000 * (l + 1)).div_euclid(1_461_001);
    l = l - (1461 * i).div_euclid(4) + 31;
    let j = (80 * l).div_euclid(2447);
    let day = l - (2447 * j).div_euclid(80);
    l = j.div_euclid(11);
    let month = j + 2 - 12 * l;
    let year = 100 * (n - 49) + i + l;
    (year, month, day)
}

/// Hijri (Kuwaiti algorithm, as Microsoft products use).
fn to_ymd_1317(ord: i64) -> (i64, i64, i64) {
    if ord <= 1 {
        return (1317, 8, 29);
    }
    if ord < 60 {
        return (1317, if ord < 32 { 9 } else { 10 }, 1 + ((ord - 2) % 30));
    }
    let y = 10631.0 / 30.0;
    let shift1 = 8.01 / 60.0;
    let mut z = ord as f64 + 466_935.0;
    let cyc = (z / 10631.0).floor();
    z -= 10631.0 * cyc;
    let j = ((z - shift1) / y).floor();
    z -= (j * y + shift1).floor();
    let m = ((z + 28.5001) / 29.5).floor();
    if m == 13.0 {
        return ((30.0 * cyc + j) as i64, 12, 30);
    }
    (
        (30.0 * cyc + j) as i64,
        m as i64,
        (z - (29.5001 * m - 29.0).floor()) as i64,
    )
}

fn pad(c: char, nbsp: bool) -> &'static str {
    match c {
        '0' => "0",
        '?' => {
            if nbsp {
                "\u{a0}"
            } else {
                " "
            }
        }
        _ => "",
    }
}

/// Clamps floating noise after scaling (`0.07 * 100` → 7).
fn clamp(n: f64) -> f64 {
    if n == 0.0 {
        return n;
    }
    let d = n.abs().log10().ceil();
    let mag = 10f64.powf(16.0 - d.floor());
    if mag.is_finite() {
        round0(n * mag) / mag
    } else {
        0.0
    }
}

/// Continued-fraction approximation with a bounded denominator.
fn dec2frac(number: f64, den_max_digits: usize) -> (f64, f64) {
    const PRECISION: f64 = 1e-13;
    let sign = if number < 0.0 { -1.0 } else { 1.0 };
    let maxdigits_d = 10f64.powi(if den_max_digits == 0 {
        2
    } else {
        den_max_digits as i32
    });
    let number = number.abs();
    let mut z = number;
    let (mut last_d, mut curr_n, mut curr_d) = (0.0f64, 0.0f64, 1.0f64);
    if number % 1.0 == 0.0 {
        return (number * sign, 1.0);
    }
    if number < 1e-19 {
        return (sign, 1e19);
    }
    if number > 1e19 {
        return (1e19 * sign, 1.0);
    }
    loop {
        z = 1.0 / (z - z.floor());
        let tmp = curr_d;
        curr_d = curr_d * z.floor() + last_d;
        last_d = tmp;
        let last_n = curr_n;
        curr_n = (number * curr_d + 0.5).floor();
        if curr_d >= maxdigits_d {
            return (sign * last_n, last_d);
        }
        if !((number - curr_n / curr_d).abs() >= PRECISION && z != z.floor()) {
            break;
        }
    }
    (sign * curr_n, curr_d)
}

fn digits_start(
    ret: &mut String,
    numstr: &[char],
    pattern: &str,
    prt: &[char],
    offset: i64,
    nbsp: bool,
) -> usize {
    let plen = pattern.chars().count() as i64;
    let nlen = numstr.len() as i64;
    let l = if offset == 0 && nlen > plen {
        prt.len() as i64 + nlen - plen
    } else {
        prt.len() as i64
    };
    let mut offset = offset;
    if nlen < plen {
        offset += nlen - plen;
    }
    for i in 0..l.max(0) {
        let idx = i + offset;
        match (idx >= 0).then(|| numstr.get(idx as usize)).flatten() {
            Some(c) => ret.push(*c),
            None => ret.push_str(pad(prt.get(i as usize).copied().unwrap_or('#'), nbsp)),
        }
    }
    l.max(0) as usize
}

/// Renders a text value through a section.
pub(crate) fn run_text(value: &str, part: &Part, opts: &FormatOptions) -> String {
    let pad_q = pad('?', opts.nbsp);
    let mut ret = String::new();
    for tok in &part.tokens {
        match tok {
            Out::Str { value: s, .. } => ret.push_str(&s.replace(' ', pad_q)),
            Out::Space { .. } | Out::Skip => ret.push_str(pad_q),
            Out::Text | Out::General => ret.push_str(value),
            Out::Plus => ret.push('+'),
            Out::Minus { volatile: false } => ret.push('-'),
            Out::Point => ret.push('.'),
            Out::Error => ret.push_str("######"),
            _ => {}
        }
    }
    ret
}

/// Renders a number through a section.
pub(crate) fn run_number(
    value: f64,
    part: &Part,
    opts: &FormatOptions,
    l10n: Option<&Locale>,
) -> Result<String, FormatError> {
    let l = l10n.unwrap_or(&EN);
    let nbsp = opts.nbsp;
    let pad_q = pad('?', nbsp);
    let mut value = value;
    let mut mantissa = String::new();
    let mut mantissa_sign = "";
    let mut numerator = String::new();
    let mut denominator = String::new();
    let mut fraction = String::new();
    let mut integer = String::new();
    let mut exp: i32 = 0;
    let mut date = value.trunc();
    let mut time = 0.0f64;
    let (mut year, mut month, mut day) = (0i64, 1i64, 0i64);
    let mut weekday = 0i64;
    let (mut hour, mut minute, mut second) = (0i64, 0i64, 0i64);
    let mut subsec = 0.0f64;

    if !part.text && part.scale.is_finite() && part.scale != 1.0 {
        value = clamp(value * part.scale);
    }
    if part.exponential {
        let mut v = value.abs();
        if v != 0.0 {
            exp = exponent_of(v, part.int_max);
        }
        if value != 0.0 && !part.integer {
            exp += 1;
        }
        v = significand(v, exp);
        if part.int_max == 1 && round(v, part.frac_max) == 10.0 {
            v = 1.0;
            exp += 1;
        }
        value = if value < 0.0 { -v } else { v };
        mantissa.push_str(&exp.abs().to_string());
    }
    if part.integer {
        let i = round(value, if part.fractions { 1 } else { part.frac_max }).abs();
        if i >= 1.0 {
            integer.push_str(&plain(i.floor()));
        }
    }
    if part.dec_fractions {
        let s = plain(round(value, part.frac_max));
        fraction = s
            .split_once('.')
            .map(|x| x.1.to_owned())
            .unwrap_or_default();
    }
    let fixed_slash = !part.error && (part.num_p.contains('0') || part.den_p.contains('0'));
    let mut have_fraction = fixed_slash;
    if part.fractions {
        have_fraction = fixed_slash || value % 1.0 != 0.0;
        let dec = if part.integer { value % 1.0 } else { value }.abs();
        if dec != 0.0 {
            have_fraction = true;
            match part.denominator.filter(|d| d.is_finite() && *d != 0.0) {
                Some(d) => {
                    denominator.push_str(&plain(d));
                    numerator.push_str(&plain(round0(dec * d)));
                    if numerator == "0" {
                        numerator.clear();
                        denominator.clear();
                        have_fraction = fixed_slash;
                    }
                }
                None => {
                    let (n, d) = dec2frac(dec, part.den_max);
                    numerator.push_str(&plain(n));
                    denominator.push_str(&plain(d));
                    if part.integer && numerator == "0" {
                        numerator.clear();
                        denominator.clear();
                        have_fraction = fixed_slash;
                    }
                }
            }
        } else if value == 0.0 && !part.integer {
            have_fraction = true;
            numerator = "0".into();
            denominator = "1".into();
        }
        if part.integer && !have_fraction && value.trunc() == 0.0 {
            integer = "0".into();
        }
    }
    if part.date != 0 {
        date = value.trunc();
        let t = DAYSIZE * (value - date);
        time = t.floor();
        subsec = t - time;
        if subsec.abs() < 1e-6 {
            subsec = 0.0;
        } else if subsec > 0.9999 {
            subsec = 0.0;
            time += 1.0;
            if time == DAYSIZE {
                time = 0.0;
                date += 1.0;
            }
        }
        if subsec != 0.0 {
            let min_u = if part.date & U_MSEC != 0 {
                U_MSEC
            } else if part.date & U_CSEC != 0 {
                U_CSEC
            } else if part.date & U_DSEC != 0 {
                U_DSEC
            } else {
                0
            };
            if (min_u == U_MSEC && subsec > 0.9995)
                || (min_u == U_CSEC && subsec > 0.995)
                || (min_u == U_DSEC && subsec > 0.95)
                || (min_u == 0 && subsec >= 0.5)
            {
                time += 1.0;
                subsec = 0.0;
            }
        }
        if part.date_eval && (value < 0.0 || date + time / DAYSIZE >= MAX_SERIAL) {
            return Err(FormatError::DateOutOfRange);
        }
        let ord = value.floor() as i64;
        let (y, m, d) = match part.date_system {
            DateSystem::Hijri => to_ymd_1317(ord),
            DateSystem::Gregorian if opts.date1904 => to_ymd_1900(ord + 1462),
            DateSystem::Gregorian => to_ymd_1900(ord),
        };
        (year, month, day) = (y, m, d);
        if time != 0.0 {
            let x = if time < 0.0 { DAYSIZE + time } else { time };
            second = (x.floor() as i64) % 60;
            minute = ((x / 60.0).floor() as i64) % 60;
            hour = ((x / 60.0 / 60.0).floor() as i64) % 60;
        }
        let shift = if opts.date1904 { 1462 } else { 0 };
        weekday = (6 + date as i64 + shift).rem_euclid(7);
    }
    if exp < 0 {
        mantissa_sign = "-";
    } else if part.exp_plus {
        mantissa_sign = "+";
    }

    let mut ret = String::new();
    let int_chars: Vec<char> = integer.chars().collect();
    let man_chars: Vec<char> = mantissa.chars().collect();
    let num_chars: Vec<char> = numerator.chars().collect();
    let den_chars: Vec<char> = denominator.chars().collect();
    let frac_chars: Vec<char> = fraction.chars().collect();
    let mut denominator_fixed = false;
    let (mut c_int, mut c_frac, mut c_man, mut c_num, mut c_den) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let num_or_den_min = part.num_min > 0 || part.den_min > 0;
    for tok in &part.tokens {
        match tok {
            Out::Str { value: s, rule } => match rule {
                Rule::None => ret.push_str(&s.replace(' ', pad_q)),
                Rule::Num => {
                    if have_fraction {
                        ret.push_str(&s.replace(' ', pad_q));
                    } else if num_or_den_min {
                        ret.push_str(&pad_q.repeat(s.chars().count()));
                    }
                }
                Rule::NumInt => {
                    if have_fraction && !integer.is_empty() {
                        ret.push_str(&s.replace(' ', pad_q));
                    } else if part.den_min > 0 && (!integer.is_empty() || part.num_min > 0) {
                        ret.push_str(&pad_q.repeat(s.chars().count()));
                    }
                }
                Rule::Den => {
                    if have_fraction {
                        ret.push_str(&s.replace(' ', pad_q));
                    } else if part.den_min > 0 {
                        ret.push_str(&pad_q.repeat(s.chars().count()));
                    }
                }
            },
            Out::Space { rule } => {
                if *rule == Rule::NumInt {
                    if (have_fraction || part.num_min > 0 || part.den_min > 0)
                        && (!integer.is_empty() || part.num_min > 0)
                    {
                        ret.push_str(pad_q);
                    }
                } else {
                    ret.push_str(pad_q);
                }
            }
            Out::Error => ret.push_str("######"),
            Out::Point => ret.push_str(if part.date != 0 { "." } else { l.decimal }),
            Out::General => ret.push_str(&general(value, l)),
            Out::Exp => ret.push_str(l.exponent),
            Out::Minus { volatile } => {
                if *volatile && (part.date != 0 || value >= 0.0) {
                    // No sign for dates or non-negative numbers.
                } else if *volatile && !part.fractions && (part.integer || part.dec_fractions) {
                    if (value < 0.0 && !integer.is_empty() && integer != "0")
                        || !fraction.is_empty()
                    {
                        ret.push_str(l.negative);
                    }
                } else {
                    ret.push_str(l.negative);
                }
            }
            Out::Plus => ret.push_str(l.positive),
            Out::Text => ret.push_str(&plain(value)),
            Out::Fill => {}
            Out::Skip => ret.push_str(if nbsp { "\u{a0}" } else { " " }),
            Out::Div => {
                if have_fraction {
                    ret.push('/');
                } else if num_or_den_min {
                    ret.push_str(pad_q);
                }
            }
            Out::Number { kind, num } => {
                let prt: Vec<char> = num.chars().collect();
                match kind {
                    NumKind::Int => {
                        if part.int_pattern.len() == 1 {
                            let pt: Vec<char> = part.int_p.chars().collect();
                            let len = part.int_min.max(int_chars.len());
                            let mut digits = String::new();
                            for i in (1..=len).rev() {
                                let d = int_chars
                                    .len()
                                    .checked_sub(i)
                                    .and_then(|k| int_chars.get(k))
                                    .copied();
                                let p = if d.is_some() {
                                    ' '
                                } else {
                                    pt.len()
                                        .checked_sub(i)
                                        .and_then(|k| pt.get(k))
                                        .copied()
                                        .or_else(|| pt.first().copied())
                                        .unwrap_or('#')
                                };
                                let mut sep = "";
                                if part.grouping {
                                    let n = i as i64 - 1 - 3;
                                    if n >= 0 && n % 3 == 0 {
                                        sep = if d.is_some() || p == '0' {
                                            l.group
                                        } else {
                                            pad('?', nbsp)
                                        };
                                    }
                                }
                                match d {
                                    Some(c) => digits.push(c),
                                    None => digits.push_str(pad(p, nbsp)),
                                }
                                digits.push_str(sep);
                            }
                            ret.push_str(&digits);
                        } else {
                            c_int += digits_start(
                                &mut ret,
                                &int_chars,
                                &part.int_p,
                                &prt,
                                c_int as i64,
                                nbsp,
                            );
                        }
                    }
                    NumKind::Frac => {
                        for (i, p) in prt.iter().enumerate() {
                            match frac_chars.get(i + c_frac) {
                                Some(c) => ret.push(*c),
                                None => ret.push_str(pad(*p, nbsp)),
                            }
                        }
                        c_frac += prt.len();
                    }
                    NumKind::Man => {
                        if c_man == 0 {
                            ret.push_str(mantissa_sign);
                        }
                        c_man += digits_start(
                            &mut ret,
                            &man_chars,
                            &part.man_p,
                            &prt,
                            c_man as i64,
                            nbsp,
                        );
                    }
                    NumKind::Num => {
                        c_num += digits_start(
                            &mut ret,
                            &num_chars,
                            &part.num_p,
                            &prt,
                            c_num as i64,
                            nbsp,
                        );
                    }
                    NumKind::Den => {
                        let len = prt.len();
                        for (i, &ch) in prt.iter().enumerate() {
                            match den_chars.get(i + c_den) {
                                Some(c) => ret.push(*c),
                                None => {
                                    if ('1'..='9').contains(&ch) || (denominator_fixed && ch == '0')
                                    {
                                        denominator_fixed = true;
                                        ret.push_str(if nbsp { "\u{a0}" } else { " " });
                                    } else if !denominator_fixed
                                        && i == len - 1
                                        && ch == '0'
                                        && denominator.is_empty()
                                    {
                                        ret.push('1');
                                    } else {
                                        ret.push_str(pad(ch, nbsp));
                                    }
                                }
                            }
                        }
                        c_den += len;
                    }
                }
            }
            Out::Date(bit) => {
                let two = |ret: &mut String, v: i64, padded: bool| {
                    if padded && v < 10 {
                        ret.push('0');
                    }
                    ret.push_str(&v.to_string());
                };
                let hijri = part.date_system == DateSystem::Hijri;
                let mi = (month - 1).clamp(0, 11) as usize;
                match bit.kind {
                    DateKind::Year => {
                        if year < 0 {
                            ret.push_str(l.negative);
                        }
                        ret.push_str(&format!("{:04}", year.abs()));
                    }
                    DateKind::YearShort => two(&mut ret, year % 100, true),
                    DateKind::Month => two(&mut ret, month, bit.pad > 0),
                    DateKind::MonthNameSingle => {
                        let name = if hijri { l.mmmm6[mi] } else { l.mmmm[mi] };
                        ret.extend(name.chars().next());
                    }
                    DateKind::MonthNameShort => {
                        ret.push_str(if hijri { l.mmm6[mi] } else { l.mmm[mi] });
                    }
                    DateKind::MonthName => {
                        ret.push_str(if hijri { l.mmmm6[mi] } else { l.mmmm[mi] });
                    }
                    DateKind::WeekdayShort => ret.push_str(l.ddd[weekday as usize]),
                    DateKind::Weekday => ret.push_str(l.dddd[weekday as usize]),
                    DateKind::Day => two(&mut ret, day, bit.pad > 0),
                    DateKind::Hour => {
                        let clock = i64::from(part.clock);
                        let h = match hour % clock {
                            0 if clock < 24 => clock,
                            h => h,
                        };
                        two(&mut ret, h, bit.pad > 0);
                    }
                    DateKind::Min => two(&mut ret, minute, bit.pad > 0),
                    DateKind::Sec => two(&mut ret, second, bit.pad > 0),
                    DateKind::HourElap | DateKind::MinElap | DateKind::SecElap => {
                        if value < 0.0 {
                            ret.push_str(l.negative);
                        }
                        let t = time.abs();
                        let v = match bit.kind {
                            DateKind::HourElap => date * 24.0 + (t / 3600.0).floor(),
                            DateKind::MinElap => date * 1440.0 + (t / 60.0).floor(),
                            _ => date * DAYSIZE + t,
                        };
                        let s = plain(v.abs());
                        for _ in s.len()..bit.pad {
                            ret.push('0');
                        }
                        ret.push_str(&s);
                    }
                    DateKind::BYear => ret.push_str(&(year + 543).to_string()),
                    DateKind::BYearShort => two(&mut ret, (year + 543) % 100, true),
                    DateKind::Era => {}
                }
            }
            Out::Subsec { decimals } => {
                ret.push_str(l.decimal);
                let f = to_fixed(subsec, part.sec_decimals);
                ret.push_str(f.get(2..2 + decimals).unwrap_or(""));
            }
            Out::AmPm { short } => {
                let idx = usize::from(hour >= 12);
                if *short && l10n.is_none() {
                    ret.push(['A', 'P'][idx]);
                } else {
                    ret.push_str(l.ampm[idx]);
                }
            }
        }
    }
    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_fixed_rounds_ties_up() {
        assert_eq!(to_fixed(0.25, 1), "0.3");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(0.0, 2), "0.00");
        assert_eq!(to_fixed(9.995, 2), "9.99");
        assert_eq!(to_fixed(99.5, 0), "100");
        assert_eq!(to_fixed(1234.5678, 3), "1234.568");
    }

    #[test]
    fn general_fits_eleven_characters() {
        for (v, s) in [
            (0.1 + 0.2, "0.3"),
            (123_456_789_012.0, "1.23457E+11"),
            (1.0 / 3.0, "0.333333333"),
            (0.000_012_345, "0.000012345"),
            (1e21, "1E+21"),
            (1_234.567_890_123, "1234.56789"),
            (0.000_000_123, "0.000000123"),
            (0.000_000_123_4, "1.234E-07"),
            (12_345_678.9, "12345678.9"),
        ] {
            assert_eq!(general(v, &EN), s, "{v}");
        }
    }

    #[test]
    fn fractions_and_dates_helpers() {
        assert_eq!(dec2frac(0.25, 1), (1.0, 4.0));
        assert_eq!(dec2frac(0.333, 1), (1.0, 3.0));
        assert_eq!(to_ymd_1900(45306), (2024, 1, 15));
        assert_eq!(to_ymd_1900(60), (1900, 2, 29));
        assert_eq!(to_ymd_1900(61), (1900, 3, 1));
        assert_eq!(clamp(0.07 * 100.0), 7.000_000_000_000_001);
        assert_eq!(round0(-2.5), -2.0);
        assert_eq!(round0(4_503_599_627_370_497.0), 4_503_599_627_370_497.0);
    }
}
