// SPDX-License-Identifier: AGPL-3.0-or-later
//! Engineering functions: base conversions, bitwise operations, complex numbers, `CONVERT`, the
//! error function and Bessel functions. Text results match Excel (upper-case hexadecimal digits,
//! 15-significant-digit complex numbers).

use super::bessel::{bessel_i, bessel_j, bessel_k, bessel_y};
use super::special::{erf, erfc};
use super::support::*;
use super::units::{Cat, from_kelvin, lookup_unit, to_kelvin};
use crate::engine::{Arg, CallContext};
use crate::registry::{ArgMask, FunctionSpec};
use crate::values::{ErrorValue, Value, number_to_text, round_significant, to_text};

// ── base conversions ───────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Base {
    radix: u32,
    /// Two's-complement width in bits (ten digits).
    bits: u32,
    min: f64,
    max: f64,
}

const BIN: Base = Base {
    radix: 2,
    bits: 10,
    min: -512.0,
    max: 511.0,
};
const OCT: Base = Base {
    radix: 8,
    bits: 30,
    min: -536_870_912.0,
    max: 536_870_911.0,
};
const HEX: Base = Base {
    radix: 16,
    bits: 40,
    min: -549_755_813_888.0,
    max: 549_755_813_887.0,
};

/// The input text of a base conversion (numbers are read as their digits).
fn digits_arg(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    match slot(args, 0, ctx) {
        Slot::Absent | Slot::Empty => Ok(String::new()),
        Slot::Val(Value::Bool(_)) => Err(ErrorValue::Value),
        Slot::Val(Value::Number(n)) => {
            if n < 0.0 || n.fract() != 0.0 {
                Err(ErrorValue::Num)
            } else {
                Ok(number_to_text(n))
            }
        }
        Slot::Val(v) => to_text(&v).map(|t| t.to_string()),
    }
}

fn parse_base(text: &str, base: Base) -> R<f64> {
    let s = text.trim();
    if s.is_empty() {
        return Ok(0.0);
    }
    if s.len() > 10 || !s.chars().all(|c| c.is_digit(base.radix)) {
        return Err(ErrorValue::Num);
    }
    let v = u64::from_str_radix(s, base.radix).map_err(|_| ErrorValue::Num)? as f64;
    let limit = 2f64.powi(base.bits as i32);
    // Ten digits with the top bit set are negative (two's complement).
    Ok(if s.len() == 10 && v >= limit / 2.0 {
        v - limit
    } else {
        v
    })
}

fn radix_text(mut n: u64, radix: u32) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    if n == 0 {
        return "0".to_owned();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(DIGITS[(n % u64::from(radix)) as usize]);
        n /= u64::from(radix);
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn format_base(
    value: f64,
    base: Base,
    args: &mut [Arg<'_>],
    ctx: &mut dyn CallContext,
) -> R<String> {
    if !value.is_finite() || value < base.min || value > base.max {
        return Err(ErrorValue::Num);
    }
    let v = value.trunc();
    if v < 0.0 {
        let twos = (2f64.powi(base.bits as i32) + v) as u64;
        return Ok(radix_text(twos, base.radix));
    }
    let s = radix_text(v as u64, base.radix);
    match slot(args, 1, ctx) {
        Slot::Absent | Slot::Empty => Ok(s),
        Slot::Val(p) => {
            let n = number(&p, ctx)?.trunc();
            if !(0.0..=10.0).contains(&n) || (n as usize) < s.len() {
                return Err(ErrorValue::Num);
            }
            Ok(format!("{}{s}", "0".repeat(n as usize - s.len())))
        }
    }
}

fn convert_base(
    args: &mut [Arg<'_>],
    ctx: &mut dyn CallContext,
    from: Base,
    to: Option<Base>,
) -> R<Value> {
    let t = digits_arg(args, ctx)?;
    let v = parse_base(&t, from)?;
    match to {
        None => Ok(Value::number(v)),
        Some(to) => format_base(v, to, args, ctx).map(Value::from),
    }
}

fn from_decimal(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, to: Base) -> R<String> {
    if let Slot::Val(Value::Bool(_)) = slot(args, 0, ctx) {
        return Err(ErrorValue::Value);
    }
    let n = num(args, 0, ctx, 0.0)?;
    format_base(n.trunc(), to, args, ctx)
}

// ── bitwise ────────────────────────────────────────────────────────────────────────────────

const BIT_LIMIT: f64 = 281_474_976_710_656.0; // 2^48

fn bit_arg(x: f64) -> R<u64> {
    if !(0.0..BIT_LIMIT).contains(&x) || x.fract() != 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok(x as u64)
}

fn bit_shift(a: f64, s0: f64, left: bool) -> R<f64> {
    let x = bit_arg(a)? as f64;
    let s = s0.trunc();
    if s.abs() > 53.0 {
        return Err(ErrorValue::Num);
    }
    let shift = if left { s } else { -s };
    let r = if shift >= 0.0 {
        x * 2f64.powf(shift)
    } else {
        (x / 2f64.powf(-shift)).floor()
    };
    if r >= BIT_LIMIT {
        Err(ErrorValue::Num)
    } else {
        Ok(r)
    }
}

// ── complex numbers ────────────────────────────────────────────────────────────────────────

/// A complex number with its suffix (`i`, `j`, or none for a real number).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Complex {
    /// Real part.
    pub re: f64,
    /// Imaginary part.
    pub im: f64,
    /// `Some('i')`, `Some('j')` or `None` (no imaginary suffix written).
    pub unit: Option<char>,
}

/// Reads an unsigned decimal number at the start of `s`: `\d+\.?\d*|\.\d+` with an optional
/// exponent. Returns the value and the bytes consumed.
fn read_unsigned(s: &str) -> Option<(f64, usize)> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i;
    let mut frac_digits = 0;
    if i < b.len() && b[i] == b'.' {
        let f = i + 1;
        let mut j = f;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        frac_digits = j - f;
        if int_digits > 0 || frac_digits > 0 {
            i = j;
        }
    }
    if int_digits == 0 && frac_digits == 0 {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let e = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > e {
            i = j;
        }
    }
    s[..i].parse::<f64>().ok().map(|v| (v, i))
}

/// Parses Excel complex text (`3+4i`, `-2j`, `i`, `5`); numbers are real values.
pub fn parse_complex(v: &Value) -> Result<Complex, ErrorValue> {
    let real = |re: f64| Complex {
        re,
        im: 0.0,
        unit: None,
    };
    let s = match v {
        Value::Error(e) => return Err(*e),
        Value::Number(n) => return Ok(real(*n)),
        Value::Bool(_) | Value::Array(_) | Value::Lambda(_) => return Err(ErrorValue::Value),
        Value::Blank => return Ok(real(0.0)),
        Value::Str(s) => s.trim(),
    };
    if s.is_empty() {
        return Ok(real(0.0));
    }
    let unit_of = |c: char| (c == 'i' || c == 'j').then_some(c);
    // Imaginary only: [+-]? number? [ij]
    {
        let (sign, rest) = match s.as_bytes()[0] {
            b'+' => (1.0, &s[1..]),
            b'-' => (-1.0, &s[1..]),
            _ => (1.0, s),
        };
        if let Some(u) = rest.chars().last().and_then(unit_of) {
            let body = &rest[..rest.len() - 1];
            let mag = if body.is_empty() {
                Some(1.0)
            } else {
                read_unsigned(body).and_then(|(v, n)| (n == body.len()).then_some(v))
            };
            if let Some(m) = mag {
                return Ok(Complex {
                    re: 0.0,
                    im: sign * m,
                    unit: Some(u),
                });
            }
        }
    }
    // Real part, then optionally [+-] number? [ij].
    let (rsign, rest) = match s.as_bytes()[0] {
        b'+' => (1.0, &s[1..]),
        b'-' => (-1.0, &s[1..]),
        _ => (1.0, s),
    };
    let (re, n) = read_unsigned(rest).ok_or(ErrorValue::Num)?;
    let re = rsign * re;
    let tail = &rest[n..];
    if tail.is_empty() {
        return Ok(real(re));
    }
    let (isign, t) = match tail.as_bytes()[0] {
        b'+' => (1.0, &tail[1..]),
        b'-' => (-1.0, &tail[1..]),
        _ => return Err(ErrorValue::Num),
    };
    let u = t.chars().last().and_then(unit_of).ok_or(ErrorValue::Num)?;
    let body = &t[..t.len() - 1];
    let mag = if body.is_empty() {
        1.0
    } else {
        match read_unsigned(body) {
            Some((v, k)) if k == body.len() => v,
            _ => return Err(ErrorValue::Num),
        }
    };
    Ok(Complex {
        re,
        im: isign * mag,
        unit: Some(u),
    })
}

/// Formats like Excel: 15 significant digits, a unit coefficient of 1 omitted, zero parts
/// dropped.
pub fn format_complex(re: f64, im: f64, unit: Option<char>) -> Result<String, ErrorValue> {
    if !re.is_finite() || !im.is_finite() {
        return Err(ErrorValue::Num);
    }
    let u = unit.unwrap_or('i');
    let r = round_significant(re, 15);
    let i = round_significant(im, 15);
    if i == 0.0 {
        return Ok(number_to_text(r));
    }
    let im_text = if i == 1.0 {
        String::new()
    } else if i == -1.0 {
        "-".to_owned()
    } else {
        number_to_text(i)
    };
    if r == 0.0 {
        return Ok(format!("{im_text}{u}"));
    }
    Ok(format!(
        "{}{}{im_text}{u}",
        number_to_text(r),
        if i > 0.0 { "+" } else { "" }
    ))
}

fn c_arg(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Complex> {
    let v = scalar(args, i, ctx);
    parse_complex(&v)
}

fn unit_of(zs: &[Complex]) -> R<Option<char>> {
    let mut u = None;
    for z in zs {
        let Some(zu) = z.unit else { continue };
        if u.is_some_and(|x| x != zu) {
            return Err(ErrorValue::Value);
        }
        u = Some(zu);
    }
    Ok(u)
}

type C = (f64, f64);

fn cmul((a, b): C, (c, d): C) -> C {
    (a * c - b * d, a * d + b * c)
}

fn cdiv((a, b): C, (c, d): C) -> C {
    let den = c * c + d * d;
    ((a * c + b * d) / den, (b * c - a * d) / den)
}

fn cexp((a, b): C) -> C {
    (a.exp() * b.cos(), a.exp() * b.sin())
}

fn cln((a, b): C) -> C {
    (a.hypot(b).ln(), b.atan2(a))
}

fn csin((a, b): C) -> C {
    (a.sin() * b.cosh(), a.cos() * b.sinh())
}

fn ccos((a, b): C) -> C {
    (a.cos() * b.cosh(), -a.sin() * b.sinh())
}

fn csinh((a, b): C) -> C {
    (a.sinh() * b.cos(), a.cosh() * b.sin())
}

fn ccosh((a, b): C) -> C {
    (a.cosh() * b.cos(), a.sinh() * b.sin())
}

const ONE: C = (1.0, 0.0);

fn is_zero((a, b): C) -> bool {
    a == 0.0 && b == 0.0
}

/// A complex function of one argument that returns a complex number.
fn c1(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, f: fn(C) -> R<C>) -> R<String> {
    let z = c_arg(args, 0, ctx)?;
    let (re, im) = f((z.re, z.im))?;
    format_complex(re, im, z.unit)
}

fn complex_fold(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, product: bool) -> R<String> {
    let mut acc: C = if product { (1.0, 0.0) } else { (0.0, 0.0) };
    let mut unit = None;
    for i in 0..args.len() {
        for v in scalars_of(args, i, ctx) {
            if v.is_blank() {
                continue;
            }
            let z = parse_complex(&v)?;
            unit = unit_of(&[
                Complex {
                    re: 0.0,
                    im: 0.0,
                    unit,
                },
                z,
            ])?;
            acc = if product {
                cmul(acc, (z.re, z.im))
            } else {
                (acc.0 + z.re, acc.1 + z.im)
            };
        }
    }
    format_complex(acc.0, acc.1, unit)
}

fn complex(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let re = num(args, 0, ctx, 0.0)?;
    let im = num(args, 1, ctx, 0.0)?;
    let u = match slot(args, 2, ctx) {
        Slot::Absent | Slot::Empty => "i".into(),
        Slot::Val(v) => to_text(&v)?,
    };
    match &*u {
        "i" => format_complex(re, im, Some('i')),
        "j" => format_complex(re, im, Some('j')),
        _ => Err(ErrorValue::Value),
    }
}

fn impower(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let z = c_arg(args, 0, ctx)?;
    let n = num(args, 1, ctx, 0.0)?;
    if is_zero((z.re, z.im)) {
        return if n > 0.0 {
            Ok("0".to_owned())
        } else {
            Err(ErrorValue::Num)
        };
    }
    let r = z.re.hypot(z.im).powf(n);
    let t = z.im.atan2(z.re) * n;
    format_complex(r * t.cos(), r * t.sin(), z.unit)
}

fn imdiv(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let a = c_arg(args, 0, ctx)?;
    let b = c_arg(args, 1, ctx)?;
    let u = unit_of(&[a, b])?;
    if is_zero((b.re, b.im)) {
        return Err(ErrorValue::Num);
    }
    let (re, im) = cdiv((a.re, a.im), (b.re, b.im));
    format_complex(re, im, u)
}

fn imsub(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let a = c_arg(args, 0, ctx)?;
    let b = c_arg(args, 1, ctx)?;
    let u = unit_of(&[a, b])?;
    format_complex(a.re - b.re, a.im - b.im, u)
}

// ── CONVERT ────────────────────────────────────────────────────────────────────────────────

fn convert(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let x = num(args, 0, ctx, 0.0)?;
    let from = text(args, 1, ctx)?;
    let to = text(args, 2, ctx)?;
    let (Some(a), Some(b)) = (lookup_unit(&from), lookup_unit(&to)) else {
        return Err(ErrorValue::NA);
    };
    if a.cat != b.cat {
        return Err(ErrorValue::NA);
    }
    if a.cat == Cat::Temperature {
        let (Some(ta), Some(tb)) = (a.temp, b.temp) else {
            return Err(ErrorValue::NA);
        };
        let k = to_kelvin(ta, x * a.factor);
        return Ok(from_kelvin(tb, k) / b.factor);
    }
    Ok(round_significant(x * a.factor / b.factor, 15))
}

fn bessel(x: f64, n0: f64, which: u8) -> R<f64> {
    let n = n0.trunc();
    if n < 0.0 {
        return Err(ErrorValue::Num);
    }
    if (which == b'K' || which == b'Y') && x <= 0.0 {
        return Err(ErrorValue::Num);
    }
    let r = match which {
        b'I' => bessel_i(x, n),
        b'J' => bessel_j(x, n),
        b'K' => bessel_k(x, n),
        _ => bessel_y(x, n),
    };
    if r.is_finite() {
        Ok(r)
    } else {
        Err(ErrorValue::Num)
    }
}

fn erf_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let lo = num(args, 0, ctx, 0.0)?;
    if args.len() < 2 {
        return Ok(erf(lo));
    }
    let hi = num(args, 1, ctx, 0.0)?;
    Ok(erf(hi) - erf(lo))
}

/// The engineering functions.
pub fn specs() -> Vec<FunctionSpec> {
    let all = ArgMask::ALL;
    vec![
        spec("BIN2DEC", 1, 1, imp!(|a, c| convert_base(a, c, BIN, None))).lift(all),
        spec(
            "BIN2HEX",
            1,
            2,
            imp!(|a, c| convert_base(a, c, BIN, Some(HEX))),
        )
        .lift(all),
        spec(
            "BIN2OCT",
            1,
            2,
            imp!(|a, c| convert_base(a, c, BIN, Some(OCT))),
        )
        .lift(all),
        spec("OCT2DEC", 1, 1, imp!(|a, c| convert_base(a, c, OCT, None))).lift(all),
        spec(
            "OCT2BIN",
            1,
            2,
            imp!(|a, c| convert_base(a, c, OCT, Some(BIN))),
        )
        .lift(all),
        spec(
            "OCT2HEX",
            1,
            2,
            imp!(|a, c| convert_base(a, c, OCT, Some(HEX))),
        )
        .lift(all),
        spec("HEX2DEC", 1, 1, imp!(|a, c| convert_base(a, c, HEX, None))).lift(all),
        spec(
            "HEX2BIN",
            1,
            2,
            imp!(|a, c| convert_base(a, c, HEX, Some(BIN))),
        )
        .lift(all),
        spec(
            "HEX2OCT",
            1,
            2,
            imp!(|a, c| convert_base(a, c, HEX, Some(OCT))),
        )
        .lift(all),
        spec("DEC2BIN", 1, 2, imp!(|a, c| from_decimal(a, c, BIN))).lift(all),
        spec("DEC2OCT", 1, 2, imp!(|a, c| from_decimal(a, c, OCT))).lift(all),
        spec("DEC2HEX", 1, 2, imp!(|a, c| from_decimal(a, c, HEX))).lift(all),
        numeric!("BITAND", 2, 2, [], |a, b| Ok::<f64, ErrorValue>(
            (bit_arg(a)? & bit_arg(b)?) as f64
        )),
        numeric!("BITOR", 2, 2, [], |a, b| Ok::<f64, ErrorValue>(
            (bit_arg(a)? | bit_arg(b)?) as f64
        )),
        numeric!("BITXOR", 2, 2, [], |a, b| Ok::<f64, ErrorValue>(
            (bit_arg(a)? ^ bit_arg(b)?) as f64
        )),
        numeric!("BITLSHIFT", 2, 2, [], |a, s| bit_shift(a, s, true)),
        numeric!("BITRSHIFT", 2, 2, [], |a, s| bit_shift(a, s, false)),
        numeric!("DELTA", 1, 2, [0.0], |a, b| if a == b { 1.0 } else { 0.0 }),
        numeric!("GESTEP", 1, 2, [0.0], |a, b| if a >= b { 1.0 } else { 0.0 }),
        spec("ERF", 1, 2, imp!(erf_fn)).lift(all),
        numeric!("ERF.PRECISE", 1, 1, [], |x| erf(x)),
        numeric!("ERFC", 1, 1, [], |x| erfc(x)),
        numeric!("ERFC.PRECISE", 1, 1, [], |x| erfc(x)),
        numeric!("BESSELI", 2, 2, [], |x, n| bessel(x, n, b'I')),
        numeric!("BESSELJ", 2, 2, [], |x, n| bessel(x, n, b'J')),
        numeric!("BESSELK", 2, 2, [], |x, n| bessel(x, n, b'K')),
        numeric!("BESSELY", 2, 2, [], |x, n| bessel(x, n, b'Y')),
        spec("CONVERT", 3, 3, imp!(convert)).lift(all),
        spec("COMPLEX", 2, 3, imp!(complex)).lift(all),
        spec("IMREAL", 1, 1, imp!(|a, c| c_arg(a, 0, c).map(|z| z.re))).lift(all),
        spec("IMAGINARY", 1, 1, imp!(|a, c| c_arg(a, 0, c).map(|z| z.im))).lift(all),
        spec(
            "IMABS",
            1,
            1,
            imp!(|a, c| c_arg(a, 0, c).map(|z| z.re.hypot(z.im))),
        )
        .lift(all),
        spec(
            "IMARGUMENT",
            1,
            1,
            imp!(|a, c| {
                let z = c_arg(a, 0, c)?;
                if z.re == 0.0 && z.im == 0.0 {
                    Err(ErrorValue::Div0)
                } else {
                    Ok(z.im.atan2(z.re))
                }
            }),
        )
        .lift(all),
        spec(
            "IMCONJUGATE",
            1,
            1,
            imp!(|a, c| c1(a, c, |(x, y)| Ok((x, -y)))),
        )
        .lift(all),
        spec("IMEXP", 1, 1, imp!(|a, c| c1(a, c, |z| Ok(cexp(z))))).lift(all),
        spec(
            "IMLN",
            1,
            1,
            imp!(|a, c| c1(a, c, |z| if is_zero(z) {
                Err(ErrorValue::Num)
            } else {
                Ok(cln(z))
            })),
        )
        .lift(all),
        spec(
            "IMLOG10",
            1,
            1,
            imp!(|a, c| c1(a, c, |z| if is_zero(z) {
                Err(ErrorValue::Num)
            } else {
                let (x, y) = cln(z);
                Ok((x / std::f64::consts::LN_10, y / std::f64::consts::LN_10))
            })),
        )
        .lift(all),
        spec(
            "IMLOG2",
            1,
            1,
            imp!(|a, c| c1(a, c, |z| if is_zero(z) {
                Err(ErrorValue::Num)
            } else {
                let (x, y) = cln(z);
                Ok((x / std::f64::consts::LN_2, y / std::f64::consts::LN_2))
            })),
        )
        .lift(all),
        spec(
            "IMSQRT",
            1,
            1,
            imp!(|a, c| c1(a, c, |(x, y)| {
                let r = x.hypot(y).sqrt();
                let t = y.atan2(x) / 2.0;
                Ok((r * t.cos(), r * t.sin()))
            })),
        )
        .lift(all),
        spec("IMSIN", 1, 1, imp!(|a, c| c1(a, c, |z| Ok(csin(z))))).lift(all),
        spec("IMCOS", 1, 1, imp!(|a, c| c1(a, c, |z| Ok(ccos(z))))).lift(all),
        spec(
            "IMTAN",
            1,
            1,
            imp!(|a, c| c1(a, c, |z| Ok(cdiv(csin(z), ccos(z))))),
        )
        .lift(all),
        spec(
            "IMCOT",
            1,
            1,
            imp!(|a, c| c1(a, c, |z| if is_zero(z) {
                Err(ErrorValue::Num)
            } else {
                Ok(cdiv(ccos(z), csin(z)))
            })),
        )
        .lift(all),
        spec(
            "IMSEC",
            1,
            1,
            imp!(|a, c| c1(a, c, |z| Ok(cdiv(ONE, ccos(z))))),
        )
        .lift(all),
        spec(
            "IMCSC",
            1,
            1,
            imp!(|a, c| c1(a, c, |z| if is_zero(z) {
                Err(ErrorValue::Num)
            } else {
                Ok(cdiv(ONE, csin(z)))
            })),
        )
        .lift(all),
        spec("IMSINH", 1, 1, imp!(|a, c| c1(a, c, |z| Ok(csinh(z))))).lift(all),
        spec("IMCOSH", 1, 1, imp!(|a, c| c1(a, c, |z| Ok(ccosh(z))))).lift(all),
        spec(
            "IMSECH",
            1,
            1,
            imp!(|a, c| c1(a, c, |z| Ok(cdiv(ONE, ccosh(z))))),
        )
        .lift(all),
        spec(
            "IMCSCH",
            1,
            1,
            imp!(|a, c| c1(a, c, |z| if is_zero(z) {
                Err(ErrorValue::Num)
            } else {
                Ok(cdiv(ONE, csinh(z)))
            })),
        )
        .lift(all),
        spec("IMPOWER", 2, 2, imp!(impower)).lift(all),
        spec("IMDIV", 2, 2, imp!(imdiv)).lift(all),
        spec("IMSUB", 2, 2, imp!(imsub)).lift(all),
        spec("IMSUM", 1, VARIADIC, imp!(|a, c| complex_fold(a, c, false))),
        spec(
            "IMPRODUCT",
            1,
            VARIADIC,
            imp!(|a, c| complex_fold(a, c, true)),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<Complex, ErrorValue> {
        parse_complex(&Value::from(s))
    }

    #[test]
    fn complex_text_round_trips() {
        assert_eq!(p("3+4i").map(|z| (z.re, z.im)), Ok((3.0, 4.0)));
        assert_eq!(
            p("-2j").map(|z| (z.re, z.im, z.unit)),
            Ok((0.0, -2.0, Some('j')))
        );
        assert_eq!(p("i").map(|z| z.im), Ok(1.0));
        assert_eq!(p("1.5e2-i").map(|z| (z.re, z.im)), Ok((150.0, -1.0)));
        assert_eq!(p("5").map(|z| (z.re, z.unit)), Ok((5.0, None)));
        assert_eq!(p("3+4"), Err(ErrorValue::Num));
        assert_eq!(p("abc"), Err(ErrorValue::Num));
        assert_eq!(format_complex(3.0, 4.0, None).as_deref(), Ok("3+4i"));
        assert_eq!(format_complex(0.0, -1.0, Some('j')).as_deref(), Ok("-j"));
        assert_eq!(format_complex(2.0, 0.0, None).as_deref(), Ok("2"));
    }

    #[test]
    fn base_parsing() {
        assert_eq!(parse_base("1111111111", BIN), Ok(-1.0));
        assert_eq!(parse_base("FF", HEX), Ok(255.0));
        assert_eq!(parse_base("12", BIN), Err(ErrorValue::Num));
        assert_eq!(radix_text(255, 16), "FF");
    }
}
