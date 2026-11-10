// SPDX-License-Identifier: AGPL-3.0-or-later
//! Displayed text: a cell value through its number format (`kiln-formula`'s Excel number
//! formats), with the section colour, the default alignment and the "does not fit a date" case.
//! Filters, conditional formats ("text contains"), clipboard text and Find all compare what a
//! cell shows.

use kiln_formula::SheetId;
use kiln_formula::numfmt::{FormatError, FormatOptions, NumberFormat};
use kiln_formula::values::{ErrorValue, Value, excel_round};

use crate::value::CellValue;
use crate::workbook::Workbook;

/// The default horizontal placement of a value (cells with General alignment).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Text.
    Left,
    /// Logicals and errors.
    Center,
    /// Numbers and dates.
    Right,
}

/// A value as a cell shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formatted {
    /// Display text.
    pub text: String,
    /// `0xRRGGBB` from a `[Red]`/`[ColorN]` section.
    pub color: Option<u32>,
    /// Default placement.
    pub placement: Placement,
    /// The value cannot be shown with this format (a negative or too large date): Excel fills
    /// the cell with `#`.
    pub overflow: bool,
}

fn trim_zeros(s: &str) -> &str {
    if !s.contains('.') {
        return s;
    }
    s.trim_end_matches('0').trim_end_matches('.')
}

/// `a` with `d` decimals, rounded half away from zero on the 15-digit decimal value (as Excel).
fn fixed(a: f64, d: usize) -> String {
    let r = excel_round(a, d as f64);
    trim_zeros(&format!("{r:.d$}")).to_owned()
}

fn scientific(a: f64, width: usize) -> String {
    // Mantissa digits that fit: the width less "E+XX" and the decimal point.
    let mut sig = width.saturating_sub(5).max(1);
    loop {
        let s = format!("{a:.prec$e}", prec = sig - 1);
        let (m, e) = s.split_once('e').unwrap_or((s.as_str(), "0"));
        let exp: i32 = e.parse().unwrap_or(0);
        let out = format!(
            "{}E{}{:02}",
            trim_zeros(m),
            if exp < 0 { '-' } else { '+' },
            exp.unsigned_abs()
        );
        if out.len() <= width || sig == 1 {
            return out;
        }
        sig -= 1;
    }
}

/// Excel's General format for a number in a cell: at most `width` characters for the digits,
/// decimal point and exponent (the minus sign is extra), rounding rather than truncating;
/// integers up to `width` digits print in full, larger magnitudes switch to scientific notation
/// with as many significant digits as fit (`1.23457E+11`); small numbers stay decimal while
/// every significant digit fits, else scientific (`1.234E-07`). Excel's standard column holds 11.
pub fn general(x: f64, width: usize) -> String {
    if !x.is_finite() {
        return ErrorValue::Num.code().to_owned();
    }
    if x == 0.0 {
        return "0".to_owned();
    }
    let width = width.max(1);
    let sign = if x < 0.0 { "-" } else { "" };
    let a = x.abs();
    let exp: i32 = format!("{a:e}")
        .split_once('e')
        .and_then(|(_, e)| e.parse().ok())
        .unwrap_or(0);
    let mut out: Option<String> = None;
    if exp >= 0 {
        let int_digits = exp as usize + 1;
        if int_digits <= width {
            let decimals = width.saturating_sub(int_digits + 1);
            let s = fixed(a, decimals.min(20));
            let int_len = s.split('.').next().map_or(0, str::len);
            if int_len <= width {
                out = Some(if s.len() <= width {
                    s
                } else {
                    fixed(a, width.saturating_sub(int_len + 1))
                });
            }
        }
    } else if exp >= -4 {
        out = Some(fixed(a, width.saturating_sub(2)));
    } else {
        let s = fixed(a, width.min(20));
        if s.len() <= width && s != "0" {
            out = Some(s);
        }
    }
    let out = match out {
        Some(s) if s != "0" => s,
        _ => scientific(a, width),
    };
    format!("{sign}{out}")
}

fn is_general(code: &str) -> bool {
    let c = code.trim();
    c.is_empty() || c.eq_ignore_ascii_case("general")
}

/// Formats a value with a number format code.
pub fn format_value(v: &CellValue, code: &str, date1904: bool) -> Formatted {
    if is_general(code)
        && let CellValue::Number(n) = v
    {
        return Formatted {
            text: general(*n, 11),
            color: None,
            placement: Placement::Right,
            overflow: false,
        };
    }
    let fmt = NumberFormat::parse(code);
    let opts = FormatOptions {
        date1904,
        nbsp: false,
    };
    let value: Value = v.to_value();
    let placement = match v {
        CellValue::Number(_) => Placement::Right,
        CellValue::Bool(_) | CellValue::Error(_) => Placement::Center,
        _ => Placement::Left,
    };
    let (text, overflow) = match fmt.format(&value, &opts) {
        Ok(t) => (t, false),
        Err(FormatError::DateOutOfRange) => ("#".repeat(8), true),
        Err(FormatError::InvalidPattern(_)) => (v.display_general(), false),
    };
    Formatted {
        text,
        color: fmt.color(&value).map(|c| c.rgb()),
        placement,
        overflow,
    }
}

impl Workbook {
    /// The cell's value as displayed with its number format.
    pub fn display(&self, sheet: SheetId, row: u32, col: u32) -> Formatted {
        let style = self.sheet(sheet).map_or(0, |s| s.style_at(row, col));
        format_value(
            &self.value(sheet, row, col),
            self.styles.num_fmt(style),
            self.props.date1904,
        )
    }

    /// The displayed text of a cell.
    pub fn display_text(&self, sheet: SheetId, row: u32, col: u32) -> String {
        self.display(sheet, row, col).text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_values_with_codes() {
        let f = format_value(&CellValue::Number(1234.5), "#,##0.00", false);
        assert_eq!(f.text, "1,234.50");
        assert_eq!(f.placement, Placement::Right);
        let red = format_value(&CellValue::Number(-3.0), "0;[Red]-0", false);
        assert_eq!(red.color, Some(0xFF0000));
        assert_eq!(
            format_value(&CellValue::Number(45306.0), "yyyy-mm-dd", false).text,
            "2024-01-15"
        );
        assert!(format_value(&CellValue::Number(-1.0), "yyyy-mm-dd", false).overflow);
        assert_eq!(
            format_value(&CellValue::Bool(true), "General", false).text,
            "TRUE"
        );
        assert_eq!(
            format_value(&CellValue::text("x"), "@", false).placement,
            Placement::Left
        );
        assert_eq!(
            format_value(&CellValue::Number(0.5), "[[[bad", false).text,
            "0.5"
        );
    }

    #[test]
    fn general_rounds_and_switches_to_scientific() {
        assert_eq!(general(999_999_999_999.0, 11), "1E+12");
        assert_eq!(general(99_999_999_999.6, 11), "1E+11");
        assert_eq!(general(1.234_567_89e100, 11), "1.2346E+100");
        assert_eq!(general(123_456.789, 6), "123457");
        assert_eq!(general(1_234_567.0, 6), "1E+06");
        assert_eq!(general(0.123_456, 5), "0.123");
        assert_eq!(general(-1.0 / 3.0, 11), "-0.333333333");
        assert_eq!(general(f64::NAN, 11), "#NUM!");
    }
}
