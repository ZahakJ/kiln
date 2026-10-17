// SPDX-License-Identifier: AGPL-3.0-or-later
//! What a cell shows: its value through its number format, sized to its column.
//!
//! - General numbers are rounded to the characters that fit the column (Excel's General
//!   format adapts to the width: `0.333333333` in a wide column, `0.3333` in a narrow one,
//!   scientific notation for large magnitudes).
//! - Numbers in other formats never overflow: when the text is wider than the cell Excel fills it
//!   with `#` (the caller measures and decides, [`Shown::is_number`]).
//! - A `*x` fill in the format (Accounting's `_($* #,##0.00_)`) splits the text: the part
//!   before it sits at the left edge, the part after it at the right, `x` repeated between.
//! - Formula view (Ctrl+`) shows formulas instead of results.

use kiln_formula::numfmt::{FormatError, FormatOptions, NumberFormat};
use kiln_formula::values::Value;
use kiln_sheet::CellValue;
use kiln_sheet::format::general;

/// Private-use marker standing for a format's `*x` fill.
const FILL_MARK: char = '\u{E000}';

/// Default horizontal placement of a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Text.
    Left,
    /// Logicals and errors.
    Center,
    /// Numbers.
    Right,
}

/// A cell's displayed text.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    /// The text (without the fill part when [`Shown::fill`] is set).
    pub text: String,
    /// `0xRRGGBB` from a `[Red]`/`[ColorN]` section.
    pub color: Option<u32>,
    /// Where General alignment puts it.
    pub placement: Placement,
    /// A number (it never overflows into neighbours).
    pub is_number: bool,
    /// The value cannot be shown in this format at all (a negative date): fill with `#`.
    pub hashes: bool,
    /// A `*x` fill: the text after the fill (right-aligned) and the repeated character.
    pub fill: Option<(String, char)>,
}

impl Shown {
    fn empty() -> Self {
        Self {
            text: String::new(),
            color: None,
            placement: Placement::Left,
            is_number: false,
            hashes: false,
            fill: None,
        }
    }

    /// Whether nothing shows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.fill.as_ref().is_none_or(|(t, _)| t.is_empty())
    }
}

/// The code with its first `*x` fill replaced by a quoted marker, and the fill character.
fn mark_fill(code: &str) -> (String, Option<char>) {
    let mut out = String::with_capacity(code.len() + 4);
    let mut chars = code.chars().peekable();
    let mut fill = None;
    let mut quoted = false;
    let mut bracket = false;
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                out.push(c);
            }
            '\\' if !quoted => {
                out.push(c);
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            '[' if !quoted => {
                bracket = true;
                out.push(c);
            }
            ']' if !quoted => {
                bracket = false;
                out.push(c);
            }
            ';' if !quoted && !bracket => {
                // Every section may have its own fill; the marker stands for each.
                out.push(c);
            }
            '*' if !quoted && !bracket => {
                let f = chars.next().unwrap_or(' ');
                fill.get_or_insert(f);
                out.push('"');
                out.push(FILL_MARK);
                out.push('"');
            }
            _ => out.push(c),
        }
    }
    (out, fill)
}

fn is_general(code: &str) -> bool {
    let c = code.trim();
    c.is_empty() || c.eq_ignore_ascii_case("general")
}

/// The text of a value in a cell `chars` digits wide.
#[must_use]
pub fn shown(
    value: &CellValue,
    code: &str,
    date1904: bool,
    chars: usize,
    show_zeros: bool,
) -> Shown {
    let placement = match value {
        CellValue::Number(_) => Placement::Right,
        CellValue::Bool(_) | CellValue::Error(_) => Placement::Center,
        _ => Placement::Left,
    };
    if let CellValue::Number(n) = value {
        if *n == 0.0 && !show_zeros {
            return Shown::empty();
        }
        if is_general(code) {
            return Shown {
                text: general(*n, chars.clamp(1, 11)),
                color: None,
                placement,
                is_number: true,
                hashes: false,
                fill: None,
            };
        }
    }
    if value.is_blank() {
        return Shown::empty();
    }
    let (marked, fill_char) = mark_fill(code);
    let fmt = NumberFormat::parse(&marked);
    let opts = FormatOptions {
        date1904,
        nbsp: false,
    };
    let v: Value = value.to_value();
    let (text, hashes) = match fmt.format(&v, &opts) {
        Ok(t) => (t, false),
        Err(FormatError::DateOutOfRange) => (String::new(), true),
        Err(FormatError::InvalidPattern(_)) => (value.display_general(), false),
    };
    let color = fmt.color(&v).map(|c| c.rgb());
    let (text, fill) = match (fill_char, text.split_once(FILL_MARK)) {
        (Some(f), Some((a, b))) => (a.to_owned(), Some((b.replace(FILL_MARK, ""), f))),
        _ => (text.replace(FILL_MARK, ""), None),
    };
    Shown {
        text,
        color,
        placement,
        is_number: matches!(value, CellValue::Number(_)),
        hashes,
        fill,
    }
}

/// The characters of `max_digit_width` that fit `width` pixels of cell (General's width rule:
/// the cell's padding off, then whole digits).
#[must_use]
pub fn digits_that_fit(width: f32, max_digit_width: f32) -> usize {
    let mdw = if max_digit_width > 0.5 {
        max_digit_width
    } else {
        7.0
    };
    let avail = (width - 5.0).max(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (avail / mdw).floor() as usize;
    n.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_numbers_fit_the_column() {
        let wide = shown(&CellValue::Number(1.0 / 3.0), "General", false, 11, true);
        assert_eq!(wide.text, "0.333333333");
        assert!(wide.is_number);
        let narrow = shown(&CellValue::Number(1.0 / 3.0), "General", false, 6, true);
        assert_eq!(narrow.text, "0.3333");
        let big = shown(
            &CellValue::Number(123_456_789_012.0),
            "General",
            false,
            8,
            true,
        );
        assert_eq!(big.text, "1.23E+11");
        assert_eq!(digits_that_fit(64.0, 7.0), 8);
        assert!(shown(&CellValue::Number(0.0), "General", false, 8, false).is_empty());
    }

    #[test]
    fn formats_split_at_the_fill() {
        let acct = shown(
            &CellValue::Number(1234.5),
            r#"_($* #,##0.00_);_($* (#,##0.00);_($* "-"??_);_(@_)"#,
            false,
            11,
            true,
        );
        assert_eq!(acct.text, " $");
        assert_eq!(acct.fill, Some(("1,234.50 ".to_owned(), ' ')));
        let neg = shown(&CellValue::Number(-5.0), "0;[Red]-0", false, 11, true);
        assert_eq!(neg.color, Some(0xFF0000));
        assert_eq!(neg.text, "-5");
        let date = shown(&CellValue::Number(-1.0), "yyyy-mm-dd", false, 11, true);
        assert!(date.hashes);
        let text = shown(&CellValue::text("Kiln"), "@", false, 11, true);
        assert_eq!(
            (text.text.as_str(), text.placement),
            ("Kiln", Placement::Left)
        );
        assert_eq!(
            shown(&CellValue::Bool(true), "General", false, 11, true).placement,
            Placement::Center
        );
        let (code, f) = mark_fill(r#""*"0*-"#);
        assert_eq!(f, Some('-'));
        assert!(code.starts_with(r#""*"0""#));
    }
}
