// SPDX-License-Identifier: AGPL-3.0-or-later
//! WordprocessingML measurement parsing.
//!
//! Attributes are usually bare integers in a native unit (twips, half-points, eighths of a point,
//! EMU), but ECMA-376 2nd+ edition and Strict files may use `ST_UniversalMeasure` strings such as
//! `"2.5cm"` or `"12pt"`, and table widths may be percentages (`"50%"`). Every parser returns
//! `None` for absent or malformed input and never panics; results are clamped to the target
//! integer range.

use kiln_doc::TableWidth;
use kiln_doc::st::WidthType;
use kiln_ooxml::units::parse_universal_measure;

/// Clamps a finite value into `i64` after rounding half away from zero.
fn round_clamp(v: f64) -> Option<i64> {
    if !v.is_finite() {
        return None;
    }
    // Saturating float → int conversion (`as` saturates and maps NaN to 0, excluded above).
    #[allow(clippy::cast_possible_truncation)]
    Some(v.round() as i64)
}

fn parse_native(v: Option<&str>, units_per_point: f64) -> Option<i64> {
    let s = v?.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(points) = parse_universal_measure(s) {
        return round_clamp(points * units_per_point);
    }
    round_clamp(s.parse::<f64>().ok()?)
}

fn to_i32(v: i64) -> i32 {
    i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX })
}

/// `ST_TwipsMeasure` / `ST_SignedTwipsMeasure` → twips.
#[must_use]
pub fn parse_twips(v: Option<&str>) -> Option<i32> {
    parse_native(v, 20.0).map(to_i32)
}

/// `ST_HpsMeasure` / `ST_SignedHpsMeasure` → half-points.
#[must_use]
pub fn parse_half_points(v: Option<&str>) -> Option<i32> {
    parse_native(v, 2.0).map(to_i32)
}

/// `ST_Coordinate` / `ST_PositiveCoordinate` → EMU.
#[must_use]
pub fn parse_emu(v: Option<&str>) -> Option<i64> {
    parse_native(v, 12_700.0)
}

/// `ST_DecimalNumber`, truncated toward zero; `None` when absent or malformed.
#[must_use]
pub fn parse_decimal(v: Option<&str>) -> Option<i64> {
    let s = v?.trim();
    if s.is_empty() {
        return None;
    }
    let n = s.parse::<f64>().ok()?;
    if !n.is_finite() {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some(n.trunc() as i64)
}

/// A non-negative decimal as `u32` (clamped).
#[must_use]
pub fn parse_u32(v: Option<&str>) -> Option<u32> {
    parse_decimal(v).map(|n| u32::try_from(n.max(0)).unwrap_or(u32::MAX))
}

/// `ST_OnOff` (`true|false|1|0|on|off`); absent means `fallback`. Anything that is not a
/// recognised "off" spelling is on, as Word reads it.
#[must_use]
pub fn parse_on_off(v: Option<&str>, fallback: bool) -> bool {
    match v {
        None => fallback,
        Some(s) => !matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "false" | "0" | "off" | "none"
        ),
    }
}

/// `ST_MeasurementOrPercent` with `w:type` (`w:tblW`, `w:tcW`, `w:tblInd`, cell margins): the
/// width in the model's units (twips for `dxa`, fiftieths of a percent for `pct`) whatever
/// notation the file used (`"2.5in"`, `"50%"`, `"2500"`).
#[must_use]
pub fn parse_table_width(w: Option<&str>, kind: Option<&str>) -> Option<TableWidth> {
    let kind = match kind.map(str::trim) {
        None | Some("dxa") => WidthType::Dxa,
        Some("pct") => WidthType::Pct,
        Some("auto") => WidthType::Auto,
        Some("nil") => WidthType::Nil,
        Some(_) => return None,
    };
    if matches!(kind, WidthType::Auto | WidthType::Nil) {
        return Some(TableWidth {
            kind,
            value: parse_twips(w).unwrap_or(0).max(0),
        });
    }
    let Some(s) = w.map(str::trim) else {
        return (kind == WidthType::Pct).then_some(TableWidth::pct(0));
    };
    if let Some(pct) = s.strip_suffix('%') {
        let n = pct.trim().parse::<f64>().ok()?;
        return round_clamp(n * 50.0).map(|v| TableWidth::pct(to_i32(v)));
    }
    if kind == WidthType::Pct {
        return round_clamp(s.parse::<f64>().ok()?).map(|v| TableWidth::pct(to_i32(v)));
    }
    parse_twips(Some(s)).map(TableWidth::dxa)
}

/// A `w:id`-style revision or annotation id as `u32` (0 when absent or not a number).
#[must_use]
pub fn parse_id(v: Option<&str>) -> u32 {
    parse_decimal(v)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twips_from_integers_and_universal_measures() {
        assert_eq!(parse_twips(Some("1440")), Some(1440));
        assert_eq!(parse_twips(Some("1in")), Some(1440));
        assert_eq!(parse_twips(Some("2.54cm")), Some(1440));
        assert_eq!(parse_twips(Some("12pt")), Some(240));
        assert_eq!(parse_twips(Some("-0.5in")), Some(-720));
        assert_eq!(parse_twips(Some("")), None);
        assert_eq!(parse_twips(Some("abc")), None);
        assert_eq!(parse_twips(None), None);
        assert_eq!(parse_twips(Some("1e300")), Some(i32::MAX));
    }

    #[test]
    fn half_points_and_emu() {
        assert_eq!(parse_half_points(Some("24")), Some(24));
        assert_eq!(parse_half_points(Some("10.5pt")), Some(21));
        assert_eq!(parse_emu(Some("1pt")), Some(12_700));
        assert_eq!(parse_emu(Some("914400")), Some(914_400));
    }

    #[test]
    fn table_widths_in_every_type_and_notation() {
        assert_eq!(
            parse_table_width(Some("5000"), Some("pct")),
            Some(TableWidth::pct(5000))
        );
        assert_eq!(
            parse_table_width(Some("100%"), Some("pct")),
            Some(TableWidth::pct(5000))
        );
        assert_eq!(
            parse_table_width(Some("33.3%"), None),
            Some(TableWidth::pct(1665))
        );
        assert_eq!(
            parse_table_width(Some("2in"), Some("dxa")),
            Some(TableWidth::dxa(2880))
        );
        assert_eq!(
            parse_table_width(Some("1000"), None),
            Some(TableWidth::dxa(1000))
        );
        assert_eq!(
            parse_table_width(Some("0"), Some("auto")),
            Some(TableWidth {
                kind: WidthType::Auto,
                value: 0
            })
        );
        assert_eq!(
            parse_table_width(None, Some("nil")),
            Some(TableWidth {
                kind: WidthType::Nil,
                value: 0
            })
        );
        assert_eq!(parse_table_width(None, Some("dxa")), None);
        assert_eq!(parse_table_width(Some("1"), Some("bogus")), None);
    }

    #[test]
    fn on_off_and_decimals() {
        assert!(parse_on_off(None, true));
        assert!(!parse_on_off(None, false));
        for off in ["0", "false", "off", "OFF", " none "] {
            assert!(!parse_on_off(Some(off), true), "{off}");
        }
        for on in ["1", "true", "on", ""] {
            assert!(parse_on_off(Some(on), false), "{on}");
        }
        assert_eq!(parse_decimal(Some(" 7 ")), Some(7));
        assert_eq!(parse_decimal(Some("2.9")), Some(2));
        assert_eq!(parse_decimal(Some("x")), None);
        assert_eq!(parse_u32(Some("-4")), Some(0));
        assert_eq!(parse_id(Some("12")), 12);
        assert_eq!(parse_id(Some("-1")), 0);
    }
}
