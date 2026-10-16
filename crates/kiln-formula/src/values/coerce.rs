// SPDX-License-Identifier: AGPL-3.0-or-later
//! Coercion rules (Excel, en-US).
//!
//! - Text → number accepts what Excel's arithmetic accepts: spaces, sign, `$`, thousands
//!   separators, accounting parentheses `(5)`, exponents, a trailing `%`, mixed fractions `1 1/2`,
//!   and dates and times in en-US forms (`2024-01-15`, `1/15/2024`, `15-Jan-2024`,
//!   `January 15, 2024`, `13:30`, `1:30 PM`, `1/15/2024 13:30`). Anything else is `#VALUE!`.
//! - Logicals are 1/0 in arithmetic; blanks are 0, `""` and FALSE.

use std::sync::Arc;

use super::dates::{date_to_serial, days_in_month, time_to_fraction};
use super::error::ErrorValue;
use super::number::number_to_text;
use super::value::Value;

/// Options that change how text turns into numbers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CoercionOptions {
    /// Use the 1904 date system for dates typed as text.
    pub date1904: bool,
    /// Year assumed for dates typed without one (`1/15`). The engine passes the year of its
    /// clock; when `None` the year 2000 is assumed (deterministic, never the system clock).
    pub current_year: Option<i32>,
}

/// Parses a number written as text the way Excel's arithmetic does.
pub fn parse_number_text(text: &str, opts: &CoercionOptions) -> Option<f64> {
    let s = text.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(v) = parse_plain_number(s) {
        return Some(v);
    }
    if let Some(v) = parse_mixed_fraction(s) {
        return Some(v);
    }
    parse_date_time_text(s, opts)
}

fn is_digit(b: u8) -> bool {
    b.is_ascii_digit()
}

/// `^(?:(\d{1,3}(?:,\d{3})+)|(\d*))(\.\d*)?(?:[eE]([+-]?\d+))?$` → normalised literal.
fn match_number_body(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && is_digit(b[i]) {
        i += 1;
    }
    let mut int_part = s[..i].to_owned();
    if i < b.len() && b[i] == b',' {
        if !(1..=3).contains(&i) {
            return None;
        }
        let mut groups = 0;
        while i < b.len() && b[i] == b',' {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && is_digit(b[j]) {
                j += 1;
            }
            if j - start != 3 {
                return None;
            }
            int_part.push_str(&s[start..j]);
            i = j;
            groups += 1;
        }
        if groups == 0 {
            return None;
        }
    }
    let mut frac = String::new();
    if i < b.len() && b[i] == b'.' {
        let start = i;
        i += 1;
        while i < b.len() && is_digit(b[i]) {
            i += 1;
        }
        frac = s[start..i].to_owned();
    }
    let mut exp = String::new();
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let start = i + 1;
        let mut j = start;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let digits_start = j;
        while j < b.len() && is_digit(b[j]) {
            j += 1;
        }
        if j == digits_start {
            return None;
        }
        exp = s[start..j].to_owned();
        i = j;
    }
    if i != b.len() {
        return None;
    }
    if int_part.is_empty() && frac.len() <= 1 {
        return None;
    }
    let mut lit = if int_part.is_empty() {
        "0".to_owned()
    } else {
        int_part
    };
    if frac != "." {
        lit.push_str(&frac);
    }
    if !exp.is_empty() {
        lit.push('e');
        lit.push_str(&exp);
    }
    Some(lit)
}

fn parse_plain_number(input: &str) -> Option<f64> {
    let mut s = input;
    let mut negative = false;
    let mut percent = false;
    if s.len() >= 2 && s.starts_with('(') && s.ends_with(')') {
        negative = true;
        s = s[1..s.len() - 1].trim();
    }
    if let Some(rest) = s.strip_suffix('%') {
        percent = true;
        s = rest.trim();
    }
    let mut saw_sign = false;
    let mut saw_currency = false;
    loop {
        if !saw_sign && !negative && (s.starts_with('+') || s.starts_with('-')) {
            saw_sign = true;
            negative = s.starts_with('-');
            s = s[1..].trim_start();
        } else if !saw_currency && s.starts_with('$') {
            saw_currency = true;
            s = s[1..].trim_start();
        } else {
            break;
        }
    }
    if !saw_currency && let Some(rest) = s.strip_suffix('$') {
        s = rest.trim_end();
    }
    let lit = match_number_body(s)?;
    let mut v: f64 = lit.parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    if percent {
        v /= 100.0;
    }
    Some(if negative { -v } else { v })
}

/// `^([+-]?)(\d+)\s+(\d+)/(\d+)$`
fn parse_mixed_fraction(s: &str) -> Option<f64> {
    let (neg, body) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let (whole, rest) = body.split_once(|c: char| c.is_whitespace())?;
    let rest = rest.trim_start();
    let (num, den) = rest.split_once('/')?;
    let all_digits = |t: &str| !t.is_empty() && t.bytes().all(is_digit);
    if !(all_digits(whole) && all_digits(num) && all_digits(den)) {
        return None;
    }
    let den: f64 = den.parse().ok()?;
    if den == 0.0 {
        return None;
    }
    let v = whole.parse::<f64>().ok()? + num.parse::<f64>().ok()? / den;
    Some(if neg { -v } else { v })
}

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

fn month_from_name(name: &str) -> Option<u32> {
    let n = name.to_ascii_lowercase();
    if n.len() < 3 {
        return None;
    }
    if n == "sept" {
        return Some(9);
    }
    MONTHS
        .iter()
        .position(|m| *m == n || m[..3] == n)
        .map(|i| i as u32 + 1)
}

fn full_year(y: &str) -> f64 {
    let n: f64 = y.parse().unwrap_or(0.0);
    if y.len() <= 2 {
        if n < 30.0 { 2000.0 + n } else { 1900.0 + n }
    } else {
        n
    }
}

/// A tiny cursor for the hand-written date and time patterns below.
struct Cur<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Cur<'a> {
    fn new(s: &'a str) -> Cur<'a> {
        Cur {
            s: s.as_bytes(),
            i: 0,
        }
    }
    fn done(&self) -> bool {
        self.i >= self.s.len()
    }
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn digits(&mut self) -> &'a str {
        let start = self.i;
        while self.peek().is_some_and(is_digit) {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[start..self.i]).unwrap_or("")
    }
    fn letters(&mut self) -> &'a str {
        let start = self.i;
        while self.peek().is_some_and(|b| b.is_ascii_alphabetic()) {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[start..self.i]).unwrap_or("")
    }
    /// Consumes one or more bytes from `set`; returns how many.
    fn many(&mut self, set: &[u8]) -> usize {
        let start = self.i;
        while self.peek().is_some_and(|b| set.contains(&b)) {
            self.i += 1;
        }
        self.i - start
    }
    fn one(&mut self, set: &[u8]) -> Option<u8> {
        let b = self.peek().filter(|b| set.contains(b))?;
        self.i += 1;
        Some(b)
    }
}

fn time_value(mut hour: f64, minute: f64, second: f64, ampm: Option<u8>) -> Option<f64> {
    if minute >= 60.0 || second >= 60.0 {
        return None;
    }
    if let Some(ap) = ampm {
        if !(1.0..=12.0).contains(&hour) {
            return None;
        }
        let pm = ap.eq_ignore_ascii_case(&b'p');
        if hour == 12.0 {
            hour = if pm { 12.0 } else { 0.0 };
        } else if pm {
            hour += 12.0;
        }
    } else if hour > 9999.0 {
        return None;
    }
    Some(time_to_fraction(hour, minute, second))
}

/// Optional `\s*([AaPp])[Mm]?` at the end of a time.
fn ampm_suffix(c: &mut Cur<'_>) -> Option<Option<u8>> {
    let save = c.i;
    c.many(b" ");
    match c.one(b"AaPp") {
        Some(ap) => {
            c.one(b"Mm");
            Some(Some(ap))
        }
        None => {
            c.i = save;
            Some(None)
        }
    }
}

fn parse_time_text(s: &str) -> Option<f64> {
    let mut c = Cur::new(s);
    let h = c.digits();
    if h.is_empty() {
        return None;
    }
    if c.one(b":").is_none() {
        // `^(\d{1,2})\s*([AaPp])[Mm]?$`
        if h.len() > 2 {
            return None;
        }
        c.many(b" ");
        let ap = c.one(b"AaPp")?;
        c.one(b"Mm");
        if !c.done() {
            return None;
        }
        return time_value(h.parse().ok()?, 0.0, 0.0, Some(ap));
    }
    let m = c.digits();
    if m.is_empty() || m.len() > 2 {
        return None;
    }
    // `mm:ss.0` — `^(\d{1,4}):(\d{1,2}\.\d+)$`
    if c.peek() == Some(b'.') {
        if h.len() > 4 {
            return None;
        }
        c.i += 1;
        let f = c.digits();
        if f.is_empty() || !c.done() {
            return None;
        }
        let sec: f64 = format!("{m}.{f}").parse().ok()?;
        if sec >= 60.0 {
            return None;
        }
        return Some(time_to_fraction(0.0, h.parse().ok()?, sec));
    }
    if h.len() > 5 {
        return None;
    }
    let mut sec = 0.0;
    if c.one(b":").is_some() {
        let sd = c.digits();
        if sd.is_empty() || sd.len() > 2 {
            return None;
        }
        let mut text = sd.to_owned();
        if c.one(b".").is_some() {
            text.push('.');
            text.push_str(c.digits());
        }
        sec = text.trim_end_matches('.').parse().ok()?;
    }
    let ampm = ampm_suffix(&mut c)?;
    if !c.done() {
        return None;
    }
    time_value(h.parse().ok()?, m.parse().ok()?, sec, ampm)
}

fn valid_date(y: f64, m: f64, d: f64, opts: &CoercionOptions) -> Option<f64> {
    if !(1.0..=12.0).contains(&m) || d < 1.0 || !(1900.0..=9999.0).contains(&y) {
        return None;
    }
    if d > f64::from(days_in_month(y as i64, m as u32)) {
        return None;
    }
    date_to_serial(y, m, d, opts.date1904)
}

fn parse_date_text(s: &str, opts: &CoercionOptions) -> Option<f64> {
    let year = || f64::from(opts.current_year.unwrap_or(2000));
    let num = |t: &str| t.parse::<f64>().ok();
    // Patterns starting with digits.
    {
        let mut c = Cur::new(s);
        let a = c.digits();
        if !a.is_empty() {
            let after_a = c.i;
            // ^(\d{4})[-/.](\d{1,2})[-/.](\d{1,2})$
            if a.len() == 4 && c.one(b"-/.").is_some() {
                let m = c.digits();
                if (1..=2).contains(&m.len()) && c.one(b"-/.").is_some() {
                    let d = c.digits();
                    if (1..=2).contains(&d.len()) && c.done() {
                        return valid_date(num(a)?, num(m)?, num(d)?, opts);
                    }
                }
            }
            c.i = after_a;
            if (1..=2).contains(&a.len()) && c.one(b"-/").is_some() {
                let after_sep = c.i;
                let b = c.digits();
                // ^(\d{1,2})[-/](\d{4})$
                if b.len() == 4 && c.done() {
                    return valid_date(num(b)?, num(a)?, 1.0, opts);
                }
                if (1..=2).contains(&b.len()) {
                    // ^(\d{1,2})[-/](\d{1,2})$
                    if c.done() {
                        return valid_date(year(), num(a)?, num(b)?, opts);
                    }
                    // ^(\d{1,2})[-/](\d{1,2})[-/](\d{4}|\d{1,2})$
                    if c.one(b"-/").is_some() {
                        let y = c.digits();
                        if (y.len() == 4 || (1..=2).contains(&y.len())) && c.done() {
                            return valid_date(full_year(y), num(a)?, num(b)?, opts);
                        }
                    }
                }
                c.i = after_sep;
            }
            c.i = after_a;
            // ^(\d{1,2})[-\s]+([A-Za-z]+)\.?(?:[-\s,]+(\d{4}|\d{2}))?$
            if (1..=2).contains(&a.len()) && c.many(b"- ") > 0 {
                let mon = c.letters();
                if !mon.is_empty() {
                    c.one(b".");
                    let y = if c.done() {
                        None
                    } else if c.many(b"- ,") > 0 {
                        let y = c.digits();
                        if (y.len() == 4 || y.len() == 2) && c.done() {
                            Some(y)
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    };
                    let m = f64::from(month_from_name(mon)?);
                    return valid_date(y.map_or_else(year, full_year), m, num(a)?, opts);
                }
            }
            return None;
        }
    }
    // Patterns starting with a month name.
    let mut c = Cur::new(s);
    let mon = c.letters();
    if mon.is_empty() {
        return None;
    }
    c.one(b".");
    let after_name = c.i;
    // ^([A-Za-z]+)\.?[-\s]+(\d{1,2})(?:(?:,\s*|[-\s]+)(\d{4}|\d{2}))?$
    if c.many(b"- ") > 0 {
        let d = c.digits();
        if (1..=2).contains(&d.len()) {
            if c.done() {
                return valid_date(year(), f64::from(month_from_name(mon)?), num(d)?, opts);
            }
            let sep_ok = if c.one(b",").is_some() {
                c.many(b" ");
                true
            } else {
                c.many(b"- ") > 0
            };
            if sep_ok {
                let y = c.digits();
                if (y.len() == 4 || y.len() == 2) && c.done() {
                    let m = f64::from(month_from_name(mon)?);
                    return valid_date(full_year(y), m, num(d)?, opts);
                }
            }
        }
    }
    // ^([A-Za-z]+)\.?[-\s,]+(\d{4})$
    c.i = after_name;
    if c.many(b"- ,") > 0 {
        let y = c.digits();
        if y.len() == 4 && c.done() {
            return valid_date(num(y)?, f64::from(month_from_name(mon)?), 1.0, opts);
        }
    }
    None
}

/// True when `rest` matches `\d{1,5}:\d.*` or `\d{1,2}\s*[AaPp][Mm]?` completely.
fn looks_like_time(rest: &str) -> bool {
    let mut c = Cur::new(rest);
    let h = c.digits();
    if h.is_empty() {
        return false;
    }
    if h.len() <= 5 && c.peek() == Some(b':') {
        c.i += 1;
        return c.peek().is_some_and(is_digit);
    }
    if h.len() > 2 {
        return false;
    }
    c.many(b" ");
    if c.one(b"AaPp").is_none() {
        return false;
    }
    c.one(b"Mm");
    c.done()
}

/// Parses an en-US date, time or date-time written as text into a serial.
pub fn parse_date_time_text(text: &str, opts: &CoercionOptions) -> Option<f64> {
    let s = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() || !s.is_ascii() {
        return None;
    }
    if let Some(t) = parse_time_text(&s) {
        return Some(t);
    }
    if let Some(d) = parse_date_text(&s, opts) {
        return Some(d);
    }
    // ^(.+?)(?:\s+|T)(\d{1,5}:\d.*|\d{1,2}\s*[AaPp][Mm]?)$ — the shortest date part wins.
    let b = s.as_bytes();
    for i in 1..b.len() {
        if b[i] != b' ' && b[i] != b'T' {
            continue;
        }
        let rest = &s[i + 1..];
        if looks_like_time(rest) {
            let date = parse_date_text(&s[..i], opts)?;
            let time = parse_time_text(rest)?;
            return Some(date + time);
        }
    }
    None
}

/// Converts a scalar to a number for arithmetic: TRUE→1, blank→0, numeric text parsed, otherwise
/// `#VALUE!`. Errors propagate. Arrays and lambdas are `#VALUE!`.
pub fn to_number(v: &Value, opts: &CoercionOptions) -> Result<f64, ErrorValue> {
    match v {
        Value::Number(n) => Ok(*n),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Value::Blank => Ok(0.0),
        Value::Str(s) => parse_number_text(s, opts).ok_or(ErrorValue::Value),
        Value::Error(e) => Err(*e),
        Value::Array(_) | Value::Lambda(_) => Err(ErrorValue::Value),
    }
}

/// Converts a scalar to text: numbers as `General`, TRUE/FALSE, blank → `""`.
pub fn to_text(v: &Value) -> Result<Arc<str>, ErrorValue> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        Value::Number(n) => Ok(Arc::from(number_to_text(*n))),
        Value::Bool(b) => Ok(Arc::from(if *b { "TRUE" } else { "FALSE" })),
        Value::Blank => Ok(Arc::from("")),
        Value::Error(e) => Err(*e),
        Value::Array(_) | Value::Lambda(_) => Err(ErrorValue::Value),
    }
}

/// Converts a scalar to a logical: numbers ≠ 0 are TRUE, `"TRUE"`/`"FALSE"` text, blank → FALSE.
pub fn to_bool(v: &Value) -> Result<bool, ErrorValue> {
    match v {
        Value::Bool(b) => Ok(*b),
        Value::Number(n) => Ok(*n != 0.0),
        Value::Blank => Ok(false),
        Value::Str(s) => {
            let t = s.trim();
            if t.eq_ignore_ascii_case("TRUE") {
                Ok(true)
            } else if t.eq_ignore_ascii_case("FALSE") {
                Ok(false)
            } else {
                Err(ErrorValue::Value)
            }
        }
        Value::Error(e) => Err(*e),
        Value::Array(_) | Value::Lambda(_) => Err(ErrorValue::Value),
    }
}

/// Display text for any scalar (errors show their code, arrays their top-left value).
pub fn display_text(v: &Value) -> String {
    match v {
        Value::Error(e) => e.code().to_owned(),
        Value::Array(a) => display_text(a.get(0, 0)),
        Value::Lambda(_) => ErrorValue::Calc.code().to_owned(),
        other => to_text(other).map(|s| s.to_string()).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPTS: CoercionOptions = CoercionOptions {
        date1904: false,
        current_year: Some(2024),
    };

    #[test]
    fn text_to_number_accepts_excel_forms() {
        let cases: &[(&str, f64)] = &[
            ("42", 42.0),
            (" 42 ", 42.0),
            ("-3.5", -3.5),
            ("+7", 7.0),
            ("1,234,567.5", 1_234_567.5),
            ("$1,000", 1000.0),
            ("-$5", -5.0),
            ("$-5", -5.0),
            ("(250)", -250.0),
            ("($1,250.50)", -1250.5),
            ("50%", 0.5),
            ("-12.5%", -0.125),
            ("1e3", 1000.0),
            ("2.5E-2", 0.025),
            (".5", 0.5),
            ("1 1/2", 1.5),
            ("-2 3/4", -2.75),
            ("2024-01-15", 45306.0),
            ("1/15/2024", 45306.0),
            ("01/15/24", 45306.0),
            ("15-Jan-2024", 45306.0),
            ("15 Jan 2024", 45306.0),
            ("Jan 15, 2024", 45306.0),
            ("January 15, 2024", 45306.0),
            ("Jan 2024", 45292.0),
            ("1/15", 45306.0),
            ("12:00", 0.5),
            ("6:00 PM", 0.75),
            ("12:00 AM", 0.0),
            ("1 PM", 13.0 / 24.0),
            ("13:30:15", (13.0 * 3600.0 + 30.0 * 60.0 + 15.0) / 86400.0),
            ("25:00", 25.0 / 24.0),
            ("2024-01-15 12:00", 45306.5),
            ("1/15/2024 6:00 PM", 45306.75),
            ("2024-01-15T12:00", 45306.5),
            ("1:30.5", (90.5) / 86400.0),
        ];
        for (text, want) in cases {
            let got = parse_number_text(text, &OPTS);
            assert!(
                got.is_some_and(|g| (g - want).abs() < 1e-9),
                "{text:?} → {got:?}, want {want}"
            );
        }
    }

    #[test]
    fn text_to_number_rejects_the_rest() {
        for text in [
            "",
            "abc",
            "1,23",
            "1.2.3",
            "12/45/2024",
            "Feb 30, 2024",
            "13:61",
            "TRUE",
            "$",
            "%",
            "1-2-3-4",
            "1 / 2 x",
            "1e",
            "()",
            "1 1/0",
            "日本",
        ] {
            assert_eq!(parse_number_text(text, &OPTS), None, "{text:?}");
        }
    }

    #[test]
    fn coercions() {
        let o = CoercionOptions::default();
        assert_eq!(to_number(&Value::Bool(true), &o), Ok(1.0));
        assert_eq!(to_number(&Value::Blank, &o), Ok(0.0));
        assert_eq!(to_number(&Value::from(""), &o), Err(ErrorValue::Value));
        assert_eq!(
            to_number(&Value::Error(ErrorValue::NA), &o),
            Err(ErrorValue::NA)
        );
        assert_eq!(to_text(&Value::Bool(true)).as_deref(), Ok("TRUE"));
        assert_eq!(to_text(&Value::Blank).as_deref(), Ok(""));
        assert_eq!(to_text(&Value::Number(12.5)).as_deref(), Ok("12.5"));
        assert_eq!(to_bool(&Value::Number(0.0)), Ok(false));
        assert_eq!(to_bool(&Value::Number(-2.0)), Ok(true));
        assert_eq!(to_bool(&Value::from("true")), Ok(true));
        assert_eq!(to_bool(&Value::from(" False ")), Ok(false));
        assert_eq!(to_bool(&Value::from("yes")), Err(ErrorValue::Value));
        assert_eq!(to_bool(&Value::Blank), Ok(false));
        assert_eq!(display_text(&Value::Error(ErrorValue::Div0)), "#DIV/0!");
        let o1904 = CoercionOptions {
            date1904: true,
            current_year: None,
        };
        assert_eq!(parse_number_text("1904-01-02", &o1904), Some(1.0));
    }
}
