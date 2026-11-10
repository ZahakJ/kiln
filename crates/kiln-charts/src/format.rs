// SPDX-License-Identifier: AGPL-3.0-or-later
//! Number formatting for tick and data labels, through `kiln-formula`'s Excel format codes.

use kiln_formula::Value;
use kiln_formula::numfmt::{FormatOptions, NumberFormat, format_general};

/// Formats `v` with an Excel format code. Invalid codes and out-of-range dates fall back to
/// `General`, like Excel's chart labels (which never show `#####`).
#[must_use]
pub fn format_number(v: f64, code: &str) -> String {
    format_number_with(v, code, false)
}

/// [`format_number`] in the 1904 date system when `date1904` is set.
#[must_use]
pub fn format_number_with(v: f64, code: &str, date1904: bool) -> String {
    if !v.is_finite() {
        return String::new();
    }
    if code.is_empty() || code.eq_ignore_ascii_case("general") {
        return format_general(v);
    }
    let opts = FormatOptions {
        date1904,
        nbsp: false,
    };
    NumberFormat::parse(code)
        .format(&Value::Number(v), &opts)
        .unwrap_or_else(|_| format_general(v))
}

/// Whether a format code shows dates or times.
#[must_use]
pub fn is_date_format(code: &str) -> bool {
    !code.is_empty() && !code.eq_ignore_ascii_case("general") && NumberFormat::parse(code).is_date()
}

/// The format Office gives value-axis labels of a 100 % stacked chart.
pub const PERCENT: &str = "0%";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_excel_labels() {
        assert_eq!(format_number(1234.5, "General"), "1234.5");
        assert_eq!(format_number(1234.5, "#,##0"), "1,235");
        assert_eq!(format_number(0.25, PERCENT), "25%");
        assert_eq!(format_number(45658.0, "mmm yy"), "Jan 25");
        assert_eq!(format_number(1.0, "[invalid"), "1");
        assert!(is_date_format("d-mmm"));
        assert!(!is_date_format("0.00"));
        assert_eq!(format_number(f64::NAN, "0"), "");
    }
}
