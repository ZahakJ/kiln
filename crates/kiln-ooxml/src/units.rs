// SPDX-License-Identifier: AGPL-3.0-or-later
//! OOXML measurement units.
//!
//! Office stores geometry in EMU (English Metric Units: 914 400 per inch, 12 700 per point,
//! 360 000 per centimetre), WordprocessingML lengths in twips (twentieths of a point), font sizes
//! in half-points, border widths in eighths of a point, DrawingML font sizes in hundredths of a
//! point, and DrawingML percentages and angles in thousandths of a percent and sixty-thousandths
//! of a degree. The typed units in `kiln_common` ([`kiln_common::Emu`], [`kiln_common::Twip`], …)
//! are the model's currency; these functions convert the raw attribute values.

/// EMU per inch.
pub const EMU_PER_INCH: f64 = 914_400.0;
/// EMU per point.
pub const EMU_PER_POINT: f64 = 12_700.0;
/// EMU per centimetre.
pub const EMU_PER_CM: f64 = 360_000.0;
/// EMU per millimetre.
pub const EMU_PER_MM: f64 = 36_000.0;
/// EMU per CSS pixel (1/96 inch).
pub const EMU_PER_PX: f64 = 9_525.0;
/// Twips per point.
pub const TWIPS_PER_POINT: f64 = 20.0;
/// Twips per inch.
pub const TWIPS_PER_INCH: f64 = 1_440.0;
/// Points per inch.
pub const POINTS_PER_INCH: f64 = 72.0;
/// DrawingML angle units (ST_Angle) per degree.
pub const ANGLE_UNITS_PER_DEGREE: f64 = 60_000.0;
/// ST_Percentage units for 100 %.
pub const PERCENT_UNITS: f64 = 100_000.0;

/// Rounds half towards positive infinity, as JavaScript's `Math.round` and Office do.
#[must_use]
pub fn round_half_up(x: f64) -> f64 {
    let f = x.floor();
    if x - f == 0.5 { f + 1.0 } else { x.round() }
}

/// Rounds to an integer for an attribute value (saturating at the `i64` range; NaN becomes 0).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn round_to_i64(x: f64) -> i64 {
    // `as` saturates and maps NaN to 0, which is the safe outcome for a malformed value.
    round_half_up(x) as i64
}

/// EMU → points.
#[must_use]
pub fn emu_to_pt(emu: f64) -> f64 {
    emu / EMU_PER_POINT
}
/// Points → EMU (rounded).
#[must_use]
pub fn pt_to_emu(pt: f64) -> i64 {
    round_to_i64(pt * EMU_PER_POINT)
}
/// EMU → CSS pixels.
#[must_use]
pub fn emu_to_px(emu: f64) -> f64 {
    emu / EMU_PER_PX
}
/// CSS pixels → EMU (rounded).
#[must_use]
pub fn px_to_emu(px: f64) -> i64 {
    round_to_i64(px * EMU_PER_PX)
}
/// EMU → inches.
#[must_use]
pub fn emu_to_inch(emu: f64) -> f64 {
    emu / EMU_PER_INCH
}
/// Inches → EMU (rounded).
#[must_use]
pub fn inch_to_emu(inch: f64) -> i64 {
    round_to_i64(inch * EMU_PER_INCH)
}
/// EMU → centimetres.
#[must_use]
pub fn emu_to_cm(emu: f64) -> f64 {
    emu / EMU_PER_CM
}
/// Centimetres → EMU (rounded).
#[must_use]
pub fn cm_to_emu(cm: f64) -> i64 {
    round_to_i64(cm * EMU_PER_CM)
}
/// EMU → millimetres.
#[must_use]
pub fn emu_to_mm(emu: f64) -> f64 {
    emu / EMU_PER_MM
}
/// Millimetres → EMU (rounded).
#[must_use]
pub fn mm_to_emu(mm: f64) -> i64 {
    round_to_i64(mm * EMU_PER_MM)
}

/// Twips → points.
#[must_use]
pub fn twips_to_pt(twips: f64) -> f64 {
    twips / TWIPS_PER_POINT
}
/// Points → twips (rounded).
#[must_use]
pub fn pt_to_twips(pt: f64) -> i64 {
    round_to_i64(pt * TWIPS_PER_POINT)
}
/// Twips → EMU (rounded).
#[must_use]
pub fn twips_to_emu(twips: f64) -> i64 {
    round_to_i64(twips * (EMU_PER_POINT / TWIPS_PER_POINT))
}
/// EMU → twips (rounded).
#[must_use]
pub fn emu_to_twips(emu: f64) -> i64 {
    round_to_i64(emu / (EMU_PER_POINT / TWIPS_PER_POINT))
}
/// Twips → CSS pixels.
#[must_use]
pub fn twips_to_px(twips: f64) -> f64 {
    twips / TWIPS_PER_INCH * 96.0
}
/// CSS pixels → twips (rounded).
#[must_use]
pub fn px_to_twips(px: f64) -> i64 {
    round_to_i64(px / 96.0 * TWIPS_PER_INCH)
}

/// `w:sz` and friends: half-points → points.
#[must_use]
pub fn half_points_to_pt(hp: f64) -> f64 {
    hp / 2.0
}
/// Points → half-points (rounded).
#[must_use]
pub fn pt_to_half_points(pt: f64) -> i64 {
    round_to_i64(pt * 2.0)
}
/// Border widths: eighths of a point → points.
#[must_use]
pub fn eighths_to_pt(e: f64) -> f64 {
    e / 8.0
}
/// Points → eighths of a point (rounded).
#[must_use]
pub fn pt_to_eighths(pt: f64) -> i64 {
    round_to_i64(pt * 8.0)
}
/// DrawingML `a:rPr/@sz`: hundredths of a point → points.
#[must_use]
pub fn centipoints_to_pt(cp: f64) -> f64 {
    cp / 100.0
}
/// Points → hundredths of a point (rounded).
#[must_use]
pub fn pt_to_centipoints(pt: f64) -> i64 {
    round_to_i64(pt * 100.0)
}

/// ST_Angle (60 000ths of a degree) → degrees.
#[must_use]
pub fn angle_to_degrees(a: f64) -> f64 {
    a / ANGLE_UNITS_PER_DEGREE
}
/// Degrees → ST_Angle (rounded).
#[must_use]
pub fn degrees_to_angle(deg: f64) -> i64 {
    round_to_i64(deg * ANGLE_UNITS_PER_DEGREE)
}
/// ST_Angle → radians.
#[must_use]
pub fn angle_to_radians(a: f64) -> f64 {
    (a / ANGLE_UNITS_PER_DEGREE).to_radians()
}

fn parse_finite(s: &str) -> Option<f64> {
    if s.is_empty() {
        return None;
    }
    s.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Parses ST_Percentage / ST_PositivePercentage / ST_FixedPercentage, either thousandths of a
/// percent (`50000`) or, in Strict documents, a percent string (`50%`). Returns a fraction
/// (`0.5`).
#[must_use]
pub fn parse_percentage(v: &str) -> Option<f64> {
    let s = v.trim();
    match s.strip_suffix('%') {
        Some(n) => parse_finite(n.trim()).map(|n| n / 100.0),
        None => parse_finite(s).map(|n| n / PERCENT_UNITS),
    }
}

/// Formats a fraction as ST_Percentage thousandths (`0.5` → `"50000"`).
#[must_use]
pub fn format_percentage(fraction: f64) -> String {
    round_to_i64(fraction * PERCENT_UNITS).to_string()
}

/// WordprocessingML table widths (`w:type="pct"`): fiftieths of a percent (`5000` = 100 %) or a
/// percent string. Returns a fraction.
#[must_use]
pub fn parse_fiftieths_percent(v: &str) -> Option<f64> {
    let s = v.trim();
    match s.strip_suffix('%') {
        Some(n) => parse_finite(n.trim()).map(|n| n / 100.0),
        None => parse_finite(s).map(|n| n / 5000.0),
    }
}

/// Parses ST_UniversalMeasure (`"2.5cm"`, `"1in"`, `"12pt"`, `"1pc"`, `"-3mm"`) into points.
/// Bare numbers return `None`: callers interpret them in the attribute's native unit.
#[must_use]
pub fn parse_universal_measure(v: &str) -> Option<f64> {
    let s = v.trim();
    let split = s.len().checked_sub(2)?;
    if !s.is_char_boundary(split) {
        return None;
    }
    let (num, unit) = s.split_at(split);
    let per = match unit {
        "mm" => 72.0 / 25.4,
        "cm" => 72.0 / 2.54,
        "in" => 72.0,
        "pt" => 1.0,
        "pc" | "pi" => 12.0,
        _ => return None,
    };
    let num = num.trim_end();
    // The schema pattern is `-?[0-9]+(\.[0-9]+)?`.
    let digits = num.strip_prefix('-').unwrap_or(num);
    let mut parts = digits.splitn(2, '.');
    let int = parts.next().unwrap_or("");
    let frac = parts.next();
    let ok = !int.is_empty()
        && int.bytes().all(|b| b.is_ascii_digit())
        && frac.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()));
    if !ok {
        return None;
    }
    parse_finite(num).map(|n| n * per)
}

/// Parses a length that is either a bare number in `native_units_per_point` units (twips = 20,
/// EMU = 12 700, half-points = 2) or a universal measure. Returns points.
#[must_use]
pub fn parse_length_pt(v: &str, native_units_per_point: f64) -> Option<f64> {
    if let Some(pt) = parse_universal_measure(v) {
        return Some(pt);
    }
    parse_finite(v.trim()).map(|n| n / native_units_per_point)
}

/// Rounds to `digits` decimals (half up) and removes `-0`.
#[must_use]
pub fn round_to(n: f64, digits: i32) -> f64 {
    let f = 10f64.powi(digits);
    let r = round_half_up(n * f) / f;
    if r == 0.0 { 0.0 } else { r }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_matches_office() {
        assert_eq!(round_half_up(2.5), 3.0);
        assert_eq!(round_half_up(-2.5), -2.0);
        assert_eq!(round_half_up(-2.6), -3.0);
        assert_eq!(round_to(-0.0004, 3), 0.0);
        assert!(round_to(-0.0004, 3).is_sign_positive());
        assert_eq!(round_to(29.289_321_881, 3), 29.289);
        assert_eq!(round_to_i64(f64::NAN), 0);
    }

    #[test]
    fn universal_measures_follow_the_schema_pattern() {
        assert_eq!(parse_universal_measure("-3mm"), Some(-3.0 * 72.0 / 25.4));
        assert_eq!(parse_universal_measure("1.in"), None);
        assert_eq!(parse_universal_measure("in"), None);
        assert_eq!(parse_universal_measure("é"), None);
        assert_eq!(parse_universal_measure("1e3pt"), None);
    }
}
