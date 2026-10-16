// SPDX-License-Identifier: AGPL-3.0-or-later
//! Number helpers implementing Excel's 15-significant-digit behaviour.

/// Decimal digits and exponent of a finite, non-zero `x` rounded to `sig` significant digits with
/// round-half-up on the exact binary value (what JavaScript's `toPrecision` and Excel do).
/// Returns `(negative, digits, exponent)` with `x ≈ 0.d1d2… × 10^(exponent + 1)`, i.e. the
/// exponent of the first digit.
fn round_digits(x: f64, sig: usize) -> (bool, Vec<u8>, i32) {
    let neg = x < 0.0;
    // 40 significant digits: enough to see whether the tail after `sig` digits is exactly half.
    let s = format!("{:.39e}", x.abs());
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let mut exp: i32 = exp.parse().unwrap_or(0);
    let all: Vec<u8> = mant
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    let mut digits: Vec<u8> = all.iter().copied().take(sig).collect();
    let tail = &all[sig.min(all.len())..];
    let round_up = tail.first().is_some_and(|&d| d >= 5);
    if round_up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, 1);
                digits.pop();
                exp += 1;
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
    (neg, digits, exp)
}

fn digits_to_f64(neg: bool, digits: &[u8], exp: i32) -> f64 {
    let mut s = String::with_capacity(digits.len() + 8);
    if neg {
        s.push('-');
    }
    for (i, d) in digits.iter().enumerate() {
        if i == 1 {
            s.push('.');
        }
        s.push(char::from(b'0' + d));
    }
    s.push('e');
    s.push_str(&exp.to_string());
    s.parse().unwrap_or(0.0)
}

/// Rounds to `sig` significant digits (round half away from zero on the exact value).
pub fn round_significant(x: f64, sig: usize) -> f64 {
    if x == 0.0 || !x.is_finite() || sig == 0 {
        return if x == 0.0 { 0.0 } else { x };
    }
    let (neg, digits, exp) = round_digits(x, sig);
    digits_to_f64(neg, &digits, exp)
}

/// Rounds to 15 significant digits, as Excel does before displaying and comparing numbers.
pub fn round15(x: f64) -> f64 {
    if x == 0.0 {
        return 0.0;
    }
    if !x.is_finite() || (x.fract() == 0.0 && x.abs() < 1e15) {
        return x;
    }
    round_significant(x, 15)
}

/// Excel's "almost equal" for numbers: equal after rounding both to 15 significant digits.
pub fn numbers_equal(a: f64, b: f64) -> bool {
    a == b || round15(a) == round15(b)
}

/// Rounds half up (the non-negative values used below), exact even above 2^52.
fn round_half_up(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// Excel's `ROUND`: half away from zero, applied to the 15-digit value so that `ROUND(1.005, 2)`
/// is 1.01 (binary 1.00499999… would otherwise round down).
pub fn excel_round(x: f64, digits: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let d = digits.trunc();
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let a = x.abs();
    if d > 300.0 {
        return x;
    }
    if d >= 0.0 {
        let f = 10f64.powi(d as i32);
        let scaled = round_significant(a * f, 15);
        let r = round_half_up(scaled) / f;
        return sign * if r.is_finite() { r } else { a };
    }
    if d < -308.0 {
        return 0.0;
    }
    let f = 10f64.powi(-d as i32);
    let scaled = round_significant(a / f, 15);
    sign * round_half_up(scaled) * f
}

/// Truncates toward zero at `digits` decimals (Excel's `TRUNC` / `ROUNDDOWN`), 15-digit aware.
pub fn excel_trunc(x: f64, digits: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let d = digits.trunc().clamp(-400.0, 400.0) as i32;
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let a = x.abs();
    if d >= 0 {
        let f = 10f64.powi(d);
        if !f.is_finite() {
            return x;
        }
        return sign * round_significant(a * f, 15).floor() / f;
    }
    let f = 10f64.powi(-d);
    sign * round_significant(a / f, 15).floor() * f
}

/// Converts a number to text the way Excel does when a number is used as text (`=A1&""`,
/// `LEN(A1)`, `CONCAT`): at most 15 significant digits, no trailing zeros, scientific notation
/// (`1E+15`, `1.5E-10`) for very large or very small magnitudes.
pub fn number_to_text(x: f64) -> String {
    if x == 0.0 {
        return "0".to_owned();
    }
    if !x.is_finite() {
        return if x > 0.0 { "1E+308" } else { "-1E+308" }.to_owned();
    }
    let (neg, mut digits, e) = round_digits(x, 15);
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    let n_sig = digits.len() as i32;
    let ds: String = digits.iter().map(|d| char::from(b'0' + d)).collect();
    let scientific = e >= 15 || e <= -10 || (e < 0 && -e - 1 + n_sig > 17);
    let body = if scientific {
        let m = if ds.len() > 1 {
            format!("{}.{}", &ds[..1], &ds[1..])
        } else {
            ds
        };
        let abs_e = e.abs();
        format!(
            "{m}E{}{}{abs_e}",
            if e < 0 { '-' } else { '+' },
            if abs_e < 10 { "0" } else { "" }
        )
    } else if e >= 0 {
        let e1 = (e + 1) as usize;
        if ds.len() <= e1 {
            format!("{ds}{}", "0".repeat(e1 - ds.len()))
        } else {
            format!("{}.{}", &ds[..e1], &ds[e1..])
        }
    } else {
        format!("0.{}{ds}", "0".repeat((-e - 1) as usize))
    };
    if neg { format!("-{body}") } else { body }
}

/// Shortest text that reads back as the same double, in JavaScript's `Number#toString` layout
/// with an upper-case `E` (`0.1`, `1E+21`, `1.5E-7`). Used to print number literals.
pub fn shortest_number_text(x: f64) -> String {
    if x == 0.0 {
        return "0".to_owned();
    }
    if !x.is_finite() {
        return if x > 0.0 { "1E+308" } else { "-1E+308" }.to_owned();
    }
    let s = format!("{:e}", x.abs());
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    let k = digits.len() as i32;
    let n = exp + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let m = if digits.len() > 1 {
            format!("{}.{}", &digits[..1], &digits[1..])
        } else {
            digits
        };
        format!("{m}E{}{}", if n - 1 < 0 { '-' } else { '+' }, (n - 1).abs())
    };
    if x < 0.0 { format!("-{body}") } else { body }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round15_and_almost_equal() {
        assert_eq!(round15(0.1 + 0.2), 0.3);
        assert!(numbers_equal(0.1 + 0.2, 0.3));
        assert!(!numbers_equal(1.0, 1.000_000_000_000_1));
        assert_eq!(round15(0.0), 0.0);
        assert_eq!(round15(-0.1 - 0.2), -0.3);
        // A tie at the 16th digit rounds up, as toPrecision does.
        assert_eq!(round15(1_234_567_890_123_465.0), 1_234_567_890_123_470.0);
    }

    #[test]
    fn excel_round_cases() {
        for (x, d, r) in [
            (2.345, 2.0, 2.35),
            (1.005, 2.0, 1.01),
            (-1.005, 2.0, -1.01),
            (2.5, 0.0, 3.0),
            (-2.5, 0.0, -3.0),
            (1234.5678, -2.0, 1200.0),
            (1250.0, -2.0, 1300.0),
            (0.285, 2.0, 0.29),
            (1e-20, 2.0, 0.0),
        ] {
            assert_eq!(excel_round(x, d), r, "ROUND({x},{d})");
        }
        assert_eq!(excel_trunc(-2.789, 1.0), -2.7);
        assert_eq!(excel_trunc(1234.0, -2.0), 1200.0);
    }

    #[test]
    fn numbers_as_text() {
        for (n, s) in [
            (0.0, "0"),
            (42.0, "42"),
            (-1.5, "-1.5"),
            (0.1 + 0.2, "0.3"),
            (1.0 / 3.0, "0.333333333333333"),
            (2.0 / 3.0, "0.666666666666667"),
            (123_456_789_012_345.0, "123456789012345"),
            (1e15, "1E+15"),
            (1_234_567_890_123_456.0, "1.23456789012346E+15"),
            (1e21, "1E+21"),
            (0.0001, "0.0001"),
            (1e-10, "1E-10"),
            (1.5e-12, "1.5E-12"),
            ((1.0 / 3.0) * 1e-6, "3.33333333333333E-07"),
            (-2.5e20, "-2.5E+20"),
        ] {
            assert_eq!(number_to_text(n), s, "{n}");
        }
    }

    #[test]
    fn shortest_literals_follow_javascript() {
        for (n, s) in [
            (1.0, "1"),
            (0.5, "0.5"),
            (0.25, "0.25"),
            (1e21, "1E+21"),
            (1.5e-7, "1.5E-7"),
            (1e-6, "0.000001"),
            (123_456_789_012_345.0, "123456789012345"),
            (1e20, "100000000000000000000"),
            (-2.5, "-2.5"),
            (0.1 + 0.2, "0.30000000000000004"),
        ] {
            assert_eq!(shortest_number_text(n), s);
            assert_eq!(s.parse::<f64>().ok(), Some(n));
        }
    }
}
