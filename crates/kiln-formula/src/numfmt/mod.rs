// SPDX-License-Identifier: AGPL-3.0-or-later
//! Excel number format codes.
//!
//! A code has up to four sections (`positive;negative;zero;text`) or up to two conditional
//! sections plus a fallback (`[>=1000]#,##0,"K";0`). Supported: digit placeholders `0 # ?`,
//! thousands separators and scaling commas, percent, scientific (`0.00E+00`), vulgar fractions
//! (`# ?/?`, `# ??/16`), literal and escaped text (`"kg"`, `\x`), `_x` skips and `*x` fills,
//! colours (`[Red]`, `[Color10]`), conditions, locale and currency codes (`[$€-409]`), dates and
//! times (`d dd ddd dddd m mm mmm mmmm mmmmm yy yyyy h hh m mm s ss .000 AM/PM A/P`), elapsed time
//! (`[h]:mm:ss`), Buddhist years (`bbbb`), the Hijri calendar (`B2`) and `General` with Excel's
//! 11-character rule.
//!
//! The section logic follows the behaviour documented by the MIT-licensed `numfmt` library (by
//! Borgar Þorsteinsson), which the conformance cases in `conformance/numfmt/` are generated from;
//! rounding is decimal (like Excel: `1.005` with `0.00` shows `1.01`) rather than binary.
//! Locale data is English for now; other locale codes format with the English names.

mod run;
mod section;
mod tokenize;

pub use run::{FormatError, FormatOptions};
pub use section::FormatColor;
pub use tokenize::CondOp;

use crate::values::{ErrorValue, Value, number_to_text};
use run::{locale_data, run_number, run_text};
use section::{Part, Pattern, parse_pattern};

/// A parsed number format code.
#[derive(Debug, Clone)]
pub struct NumberFormat {
    code: String,
    parsed: Result<Pattern, String>,
    default_text: Part,
}

impl NumberFormat {
    /// Parses a format code. An invalid code still yields a `NumberFormat`; formatting with it
    /// reports [`FormatError::InvalidPattern`].
    pub fn parse(code: &str) -> NumberFormat {
        let code = if code.is_empty() { "General" } else { code };
        let default_text = parse_pattern("@")
            .ok()
            .and_then(|p| p.partitions.into_iter().nth(3))
            .unwrap_or_else(Part::error_part);
        NumberFormat {
            code: code.to_owned(),
            parsed: parse_pattern(code),
            default_text,
        }
    }

    /// The code as given.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// True when the code parsed.
    pub fn is_valid(&self) -> bool {
        self.parsed.is_ok()
    }

    /// Why the code is invalid.
    pub fn error(&self) -> Option<&str> {
        self.parsed.as_ref().err().map(String::as_str)
    }

    fn pattern(&self) -> Result<&Pattern, FormatError> {
        self.parsed
            .as_ref()
            .map_err(|e| FormatError::InvalidPattern(e.clone()))
    }

    fn part_for(parts: &[Part], value: f64) -> Option<&Part> {
        parts.iter().take(3).find(|p| match p.condition {
            Some((op, operand)) => op.test(value, operand),
            None => true,
        })
    }

    /// Formats a number.
    pub fn format_number(&self, value: f64, opts: &FormatOptions) -> Result<String, FormatError> {
        let p = self.pattern()?;
        let l10n = p.locale.as_deref().and_then(locale_data);
        if !value.is_finite() {
            return Ok(ErrorValue::Num.code().to_owned());
        }
        match NumberFormat::part_for(&p.partitions, value) {
            Some(part) => run_number(value, part, opts, l10n),
            None => Ok("######".to_owned()),
        }
    }

    /// Formats text through the text section (`@`).
    pub fn format_text(&self, text: &str, opts: &FormatOptions) -> Result<String, FormatError> {
        let p = self.pattern()?;
        let part = p.partitions.get(3).unwrap_or(&self.default_text);
        Ok(run_text(text, part, opts))
    }

    /// Formats a cell value: numbers through the numeric sections, text and logicals through the
    /// text section, blanks as nothing, errors as their code.
    pub fn format(&self, value: &Value, opts: &FormatOptions) -> Result<String, FormatError> {
        match value {
            Value::Number(n) => self.format_number(*n, opts),
            Value::Str(s) => self.format_text(s, opts),
            Value::Bool(b) => self.format_text(if *b { "TRUE" } else { "FALSE" }, opts),
            Value::Blank => Ok(String::new()),
            Value::Error(e) => Ok(e.code().to_owned()),
            Value::Array(a) => self.format(a.get(0, 0), opts),
            Value::Lambda(_) => Ok(ErrorValue::Calc.code().to_owned()),
        }
    }

    /// The colour of the section a value is shown with, if it has one.
    pub fn color(&self, value: &Value) -> Option<FormatColor> {
        let p = self.parsed.as_ref().ok()?;
        match value {
            Value::Number(n) if n.is_finite() => {
                NumberFormat::part_for(&p.partitions, *n).and_then(|part| part.color)
            }
            _ => p.partitions.get(3).and_then(|part| part.color),
        }
    }

    /// True when any section shows a date or time.
    pub fn is_date(&self) -> bool {
        self.parsed
            .as_ref()
            .is_ok_and(|p| p.partitions.iter().any(|s| s.date != 0))
    }

    /// True when any section shows a percentage.
    pub fn is_percent(&self) -> bool {
        self.parsed
            .as_ref()
            .is_ok_and(|p| p.partitions.iter().any(|s| s.percent))
    }

    /// True for a pure text format (`@`).
    pub fn is_text(&self) -> bool {
        self.parsed.as_ref().is_ok_and(|p| {
            p.partitions.iter().take(3).all(|s| s.generated)
                && p.partitions.get(3).is_some_and(|s| s.text && !s.generated)
        })
    }
}

/// Formats a value with a format code (parses the code each time; keep a [`NumberFormat`] to
/// reuse it).
pub fn format_value(
    code: &str,
    value: &Value,
    opts: &FormatOptions,
) -> Result<String, FormatError> {
    NumberFormat::parse(code).format(value, opts)
}

/// `General` for a number, as a cell shows it (at most 11 characters, sign included).
pub fn format_general(n: f64) -> String {
    if !n.is_finite() {
        return number_to_text(n);
    }
    let s = run::general(n, &run::EN);
    if n < 0.0 && s != "0" {
        format!("-{s}")
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(code: &str, v: f64) -> String {
        NumberFormat::parse(code)
            .format_number(v, &FormatOptions::default())
            .unwrap_or_else(|e| format!("ERR {e}"))
    }

    #[test]
    fn common_formats() {
        assert_eq!(f("0.00", 1.005), "1.01");
        assert_eq!(f("0.0", 0.25), "0.3");
        assert_eq!(f("#,##0", 1_234_567.891), "1,234,568");
        assert_eq!(f("0.00E+00", 12345.0), "1.23E+04");
        assert_eq!(f("# ?/?", 1.25), "1 1/4");
        assert_eq!(f("mm/dd/yyyy", 45306.75), "01/15/2024");
        assert_eq!(f("[h]:mm:ss", 1.5), "36:00:00");
        assert_eq!(f("General", -0.000_012_345), "-0.000012345");
        assert_eq!(f("0%", 0.125), "13%");
        assert_eq!(f("0.0%", 0.0005), "0.1%");
        assert_eq!(f("[Red]0;[Blue]-0", -5.0), "-5");
        assert_eq!(f("0;-0;\"zero\";@", 0.0), "zero");
        assert_eq!(f("0.00", -0.001), "0.00");
        assert_eq!(f("#.##", 0.0), ".");
        assert_eq!(f("dddd, mmmm d, yyyy", 45306.0), "Monday, January 15, 2024");
        assert_eq!(f("h:mm AM/PM", 0.75), "6:00 PM");
        assert_eq!(f("0.0000000000", 1e-7), "0.0000001000");
    }

    #[test]
    fn colours_text_and_errors() {
        let nf = NumberFormat::parse("[Red]0;[Blue]-0");
        assert_eq!(nf.color(&Value::Number(-1.0)), Some(FormatColor::Blue));
        assert_eq!(nf.color(&Value::Number(1.0)), Some(FormatColor::Red));
        assert_eq!(FormatColor::Indexed(3).rgb(), 0xFF0000);
        let t = NumberFormat::parse("@");
        assert!(t.is_text());
        assert_eq!(
            t.format(&Value::from("abc"), &FormatOptions::default())
                .ok(),
            Some("abc".into())
        );
        assert_eq!(
            NumberFormat::parse("General")
                .format(&Value::Bool(true), &FormatOptions::default())
                .ok(),
            Some("TRUE".into())
        );
        assert!(!NumberFormat::parse("0;0;0;0;0").is_valid());
        assert!(NumberFormat::parse("yyyy").is_date());
        assert_eq!(
            NumberFormat::parse("yyyy").format_number(-1.0, &FormatOptions::default()),
            Err(FormatError::DateOutOfRange)
        );
        assert_eq!(format_general(-1234.5), "-1234.5");
        assert_eq!(format_general(1e21), "1E+21");
    }

    proptest::proptest! {
        #[test]
        fn arbitrary_codes_never_panic(code in "\\PC{0,24}", v in proptest::num::f64::ANY) {
            let nf = NumberFormat::parse(&code);
            let _ = nf.format(&Value::Number(v), &FormatOptions::default());
            let _ = nf.format(&Value::from("text"), &FormatOptions::default());
            let _ = nf.color(&Value::Number(v));
        }

        #[test]
        fn format_like_codes_never_panic(
            code in "[0#?.,;%E+\\-/ @\\[\\]hmsdyAMP:\"$€<>=Red]{0,20}",
            v in -1e7f64..1e7,
        ) {
            let nf = NumberFormat::parse(&code);
            let _ = nf.format(&Value::Number(v), &FormatOptions { date1904: true, nbsp: true });
        }
    }
}
