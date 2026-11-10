// SPDX-License-Identifier: AGPL-3.0-or-later
//! String and number helpers shared by the reader and the writer: SpreadsheetML's `_xHHHH_`
//! escapes, numbers in the shortest form that reads back exactly, booleans and fast A1 cell
//! references for the streaming paths.

use std::fmt::Write as _;

/// Decodes SpreadsheetML's `_xHHHH_` escapes (ECMA-376 Part 1 §22.4.2.4), used for characters
/// XML cannot carry (`_x000D_` for a carriage return, `_x0001_`…). `_x005F_` escapes a literal
/// underscore. Invalid code points are kept as written.
#[must_use]
pub fn decode_x_escapes(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains("_x") {
        return std::borrow::Cow::Borrowed(s);
    }
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    let mut last = 0usize;
    while i + 7 <= b.len() {
        if b[i] == b'_'
            && b[i + 1] == b'x'
            && b[i + 6] == b'_'
            && b[i + 2..i + 6].iter().all(u8::is_ascii_hexdigit)
        {
            let hex = &s[i + 2..i + 6];
            if let Some(c) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) {
                out.push_str(&s[last..i]);
                out.push(c);
                i += 7;
                last = i;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&s[last..]);
    std::borrow::Cow::Owned(out)
}

fn needs_x_escape(c: char) -> bool {
    matches!(c, '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\r' | '\u{fffe}' | '\u{ffff}')
}

/// Encodes text for a SpreadsheetML string: control characters (and `\r`) as `_xHHHH_`, and a
/// literal `_xHHHH_` sequence with its underscore escaped, so [`decode_x_escapes`] restores it.
#[must_use]
pub fn encode_x_escapes(s: &str) -> std::borrow::Cow<'_, str> {
    let literal = s.contains("_x") && has_literal_escape(s);
    if !literal && !s.chars().any(needs_x_escape) {
        return std::borrow::Cow::Borrowed(s);
    }
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len() + 8);
    for (i, c) in s.char_indices() {
        if c == '_' && is_escape_at(b, i) {
            out.push_str("_x005F_");
        } else if needs_x_escape(c) {
            let _ = write!(out, "_x{:04X}_", u32::from(c));
        } else {
            out.push(c);
        }
    }
    std::borrow::Cow::Owned(out)
}

fn is_escape_at(b: &[u8], i: usize) -> bool {
    b.len() >= i + 7
        && b[i] == b'_'
        && b[i + 1] == b'x'
        && b[i + 6] == b'_'
        && b[i + 2..i + 6].iter().all(u8::is_ascii_hexdigit)
}

fn has_literal_escape(s: &str) -> bool {
    let b = s.as_bytes();
    (0..b.len()).any(|i| is_escape_at(b, i))
}

/// A number in the shortest text that reads back to the same `f64`, the way Excel writes it
/// (`1E+21`, `1E-7` for very large and very small magnitudes).
#[must_use]
pub fn fmt_num(n: f64) -> String {
    if !n.is_finite() {
        return "0".to_owned();
    }
    if n == 0.0 {
        return "0".to_owned();
    }
    let a = n.abs();
    if (1e-6..1e21).contains(&a) {
        // Display is the shortest round-trip form without an exponent.
        return format!("{n}");
    }
    let e = format!("{n:e}");
    match e.split_once('e') {
        Some((m, x)) if x.starts_with('-') => format!("{m}E{x}"),
        Some((m, x)) => format!("{m}E+{x}"),
        None => e,
    }
}

/// `xsd:boolean`: `1`/`true` → true, `0`/`false` → false, anything else → `None`.
#[must_use]
pub fn parse_bool(v: &str) -> Option<bool> {
    match v.trim() {
        "1" | "true" | "True" | "TRUE" | "on" => Some(true),
        "0" | "false" | "False" | "FALSE" | "off" => Some(false),
        _ => None,
    }
}

/// A finite number attribute.
#[must_use]
pub fn parse_num(v: &str) -> Option<f64> {
    v.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Parses an A1 cell reference (`B12`, `$B$12`, case-insensitive) into zero-based (row, col).
/// Rows beyond 1,048,576 and columns beyond XFD are refused.
#[must_use]
pub fn parse_cell_ref(text: &[u8]) -> Option<(u32, u32)> {
    let mut i = 0usize;
    if text.first() == Some(&b'$') {
        i += 1;
    }
    let mut col: u32 = 0;
    let start = i;
    while let Some(&c) = text.get(i) {
        let up = c.to_ascii_uppercase();
        if up.is_ascii_uppercase() {
            col = col.checked_mul(26)?.checked_add(u32::from(up - b'A') + 1)?;
            i += 1;
        } else {
            break;
        }
    }
    if i == start || i - start > 3 || col > 16_384 {
        return None;
    }
    if text.get(i) == Some(&b'$') {
        i += 1;
    }
    let digits = &text[i..];
    if digits.is_empty() || digits.len() > 7 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let row: u32 = digits
        .iter()
        .fold(0u32, |acc, d| acc * 10 + u32::from(d - b'0'));
    if row == 0 || row > 1_048_576 {
        return None;
    }
    Some((row - 1, col - 1))
}

/// Column letters of a zero-based column (`0` → `A`, `16383` → `XFD`).
#[must_use]
pub fn col_letters(col: u32) -> String {
    let mut n = col + 1;
    let mut buf = [0u8; 4];
    let mut len = 0usize;
    while n > 0 && len < buf.len() {
        let rem = (n - 1) % 26;
        buf[len] = b'A' + rem as u8;
        len += 1;
        n = (n - 1) / 26;
    }
    buf[..len].reverse();
    String::from_utf8_lossy(&buf[..len]).into_owned()
}

/// Appends the A1 reference of a zero-based cell to `out`.
pub fn push_cell_ref(out: &mut String, row: u32, col: u32) {
    out.push_str(&col_letters(col));
    let _ = write!(out, "{}", u64::from(row) + 1);
}

/// The A1 reference of a zero-based cell.
#[must_use]
pub fn cell_ref(row: u32, col: u32) -> String {
    let mut s = String::with_capacity(8);
    push_cell_ref(&mut s, row, col);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_escapes_round_trip() {
        for s in [
            "plain",
            "a\rb",
            "_x0041_",
            "tab\u{1}x",
            "_x00ZZ_",
            "__x0041__",
        ] {
            let enc = encode_x_escapes(s);
            assert_eq!(decode_x_escapes(&enc), s, "{s:?} via {enc:?}");
        }
        assert_eq!(decode_x_escapes("a_x000D_b"), "a\rb");
        assert_eq!(encode_x_escapes("a\rb"), "a_x000D_b");
        assert_eq!(decode_x_escapes("_xD800_"), "_xD800_");
    }

    #[test]
    fn numbers_are_shortest_and_exact() {
        for n in [
            0.1,
            1.0,
            -2.5,
            1e21,
            1.5e-7,
            123_456_789.123,
            f64::MAX,
            f64::MIN_POSITIVE,
            0.30000000000000004,
        ] {
            let t = fmt_num(n);
            assert_eq!(t.parse::<f64>().ok(), Some(n), "{t}");
        }
        assert_eq!(fmt_num(1e21), "1E+21");
        assert_eq!(fmt_num(1e-7), "1E-7");
        assert_eq!(fmt_num(42.0), "42");
        assert_eq!(fmt_num(-0.0), "0");
    }

    #[test]
    fn cell_refs_parse_and_print() {
        assert_eq!(parse_cell_ref(b"A1"), Some((0, 0)));
        assert_eq!(parse_cell_ref(b"$xfd$1048576"), Some((1_048_575, 16_383)));
        assert_eq!(parse_cell_ref(b"XFE1"), None);
        assert_eq!(parse_cell_ref(b"A0"), None);
        assert_eq!(parse_cell_ref(b"A1048577"), None);
        assert_eq!(parse_cell_ref(b"1A"), None);
        assert_eq!(parse_cell_ref(b""), None);
        assert_eq!(cell_ref(0, 0), "A1");
        assert_eq!(cell_ref(9, 27), "AB10");
        assert_eq!(col_letters(16_383), "XFD");
        for c in [0, 25, 26, 701, 702, 16_383] {
            let s = cell_ref(4, c);
            assert_eq!(parse_cell_ref(s.as_bytes()), Some((4, c)));
        }
    }

    #[test]
    fn booleans() {
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool("false"), Some(false));
        assert_eq!(parse_bool("x"), None);
        assert_eq!(parse_num(" 2.5 "), Some(2.5));
        assert_eq!(parse_num("inf"), None);
    }
}
