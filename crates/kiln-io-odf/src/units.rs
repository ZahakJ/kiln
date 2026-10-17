// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lengths, percentages, colours and dates as ODF writes them.
//!
//! ODF lengths carry a unit (`2.54cm`, `1in`, `12pt`, `10mm`, `1pc`, `96px`). The models use
//! twips (1/1440 in, WordprocessingML), EMU (1/914 400 in, DrawingML), half-points (font sizes)
//! and points. Lengths are written in inches with enough decimals to read back to the same
//! twip or EMU.

use kiln_common::Color;

/// Twips per inch.
pub const TWIPS_PER_INCH: f64 = 1440.0;
/// EMU per inch.
pub const EMU_PER_INCH: f64 = 914_400.0;

/// Parses an ODF length into inches. Unitless numbers are refused (ODF always has a unit);
/// non-finite values too.
#[must_use]
pub fn parse_inches(s: &str) -> Option<f64> {
    let s = s.trim();
    let split = s
        .find(|c: char| c.is_ascii_alphabetic() || c == '%')
        .unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let v: f64 = num.trim().parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    let inches = match unit.trim().to_ascii_lowercase().as_str() {
        "in" | "inch" => v,
        "cm" => v / 2.54,
        "mm" => v / 25.4,
        "pt" => v / 72.0,
        "pc" => v / 6.0,
        "px" => v / 96.0,
        "twip" => v / TWIPS_PER_INCH,
        _ => return None,
    };
    Some(inches)
}

fn round_i64(v: f64) -> Option<i64> {
    let r = v.round();
    if r.is_finite() && r.abs() < 9.0e15 {
        // Exact: the rounded value is an integer well inside the i64 range.
        #[allow(clippy::cast_possible_truncation)]
        Some(r as i64)
    } else {
        None
    }
}

/// A length in twips.
#[must_use]
pub fn twips(s: &str) -> Option<i32> {
    parse_inches(s)
        .and_then(|i| round_i64(i * TWIPS_PER_INCH))
        .and_then(|v| i32::try_from(v).ok())
}

/// A length in EMU.
#[must_use]
pub fn emu(s: &str) -> Option<i64> {
    parse_inches(s).and_then(|i| round_i64(i * EMU_PER_INCH))
}

/// A length in points.
#[must_use]
pub fn points(s: &str) -> Option<f64> {
    parse_inches(s).map(|i| i * 72.0)
}

/// A font size in half-points (`12pt` → 24). Percentages are not sizes and give `None`.
#[must_use]
pub fn half_points(s: &str) -> Option<u32> {
    points(s)
        .and_then(|p| round_i64(p * 2.0))
        .and_then(|v| u32::try_from(v).ok())
}

fn trim_decimals(mut s: String) -> String {
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" { "0".to_owned() } else { s }
}

/// Twips as inches (`1440` → `1in`, `1` → `0.0007in`), four decimals: reads back exactly.
#[must_use]
pub fn twips_str(tw: i64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let v = tw as f64 / TWIPS_PER_INCH;
    format!("{}in", trim_decimals(format!("{v:.4}")))
}

/// EMU as millimetres with five decimals: reads back exactly.
#[must_use]
pub fn emu_str(e: i64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let v = e as f64 / 36_000.0;
    format!("{}mm", trim_decimals(format!("{v:.5}")))
}

/// Points (`12pt`).
#[must_use]
pub fn pt_str(pt: f64) -> String {
    format!("{}pt", trim_decimals(format!("{pt:.2}")))
}

/// Half-points as points (`24` → `12pt`).
#[must_use]
pub fn half_points_str(hp: u32) -> String {
    pt_str(f64::from(hp) / 2.0)
}

/// Eighths of a point (border widths of WordprocessingML) as points.
#[must_use]
pub fn eighth_points_str(v: u32) -> String {
    pt_str(f64::from(v) / 8.0)
}

/// A percentage (`115%` → 115.0).
#[must_use]
pub fn percent(s: &str) -> Option<f64> {
    s.trim()
        .strip_suffix('%')
        .and_then(|n| n.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

/// A colour `#rrggbb`.
#[must_use]
pub fn color(s: &str) -> Option<Color> {
    let hex = s.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    Color::parse_hex(hex).ok()
}

/// `#rrggbb` (lower case, as LibreOffice writes it).
#[must_use]
pub fn color_str(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// An angle in degrees from `rotate(…)`-style or plain numbers, with optional `deg`, `rad` or
/// `grad` units.
#[must_use]
pub fn degrees(s: &str) -> Option<f64> {
    let s = s.trim();
    let (num, factor) = if let Some(n) = s.strip_suffix("deg") {
        (n, 1.0)
    } else if let Some(n) = s.strip_suffix("grad") {
        (n, 0.9)
    } else if let Some(n) = s.strip_suffix("rad") {
        (n, 180.0 / std::f64::consts::PI)
    } else {
        (s, 1.0)
    };
    num.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| v * factor)
}

/// Days from 1899-12-30 (the spreadsheet serial epoch) to a civil date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    // Howard Hinnant's algorithm.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The Unix day of 1899-12-30.
const SERIAL_EPOCH: i64 = -25_569;

/// An `office:date-value` (`2024-01-31`, `2024-01-31T12:30:00`, with optional fraction) as a
/// spreadsheet serial (days since 1899-12-30, the 1900 system).
#[must_use]
pub fn date_serial(s: &str) -> Option<f64> {
    let s = s.trim();
    let (date, time) = match s.find('T') {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    let neg = date.starts_with('-');
    let mut parts = date.trim_start_matches('-').splitn(3, '-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.get(..2)?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if neg { -y } else { y };
    #[allow(clippy::cast_precision_loss)]
    let mut serial = (days_from_civil(y, m, d) - SERIAL_EPOCH) as f64;
    if let Some(t) = time {
        let t = t.trim_end_matches('Z');
        let t = t.split(['+']).next().unwrap_or(t);
        let mut hms = t.splitn(3, ':');
        let h: f64 = hms.next()?.parse().ok()?;
        let mi: f64 = hms.next().unwrap_or("0").parse().ok()?;
        let se: f64 = hms.next().unwrap_or("0").parse().ok()?;
        serial += (h * 3600.0 + mi * 60.0 + se) / 86_400.0;
    }
    Some(serial)
}

/// A serial as `office:date-value` (`2024-01-31` or `2024-01-31T12:30:00` with fractional
/// seconds when present).
#[must_use]
pub fn serial_date_str(serial: f64) -> String {
    let days = serial.floor();
    let Some(d) = round_i64(days) else {
        return "1899-12-30".to_owned();
    };
    let (y, m, dd) = civil_from_days(d + SERIAL_EPOCH);
    let frac = serial - days;
    let date = if y < 0 {
        format!("-{:04}-{m:02}-{dd:02}", -y)
    } else {
        format!("{y:04}-{m:02}-{dd:02}")
    };
    if frac.abs() < 1e-12 {
        return date;
    }
    let secs = frac * 86_400.0;
    let whole = secs.floor();
    let h = (whole / 3600.0).floor();
    let mi = ((whole - h * 3600.0) / 60.0).floor();
    let s = secs - h * 3600.0 - mi * 60.0;
    let s_str = if (s - s.round()).abs() < 1e-6 {
        format!("{:02}", s.round())
    } else {
        let mut t = format!("{s:09.6}");
        while t.ends_with('0') {
            t.pop();
        }
        t
    };
    format!("{date}T{h:02}:{mi:02}:{s_str}")
}

/// An `office:time-value` duration (`PT12H30M15S`, `PT36H`) in days.
#[must_use]
pub fn duration_days(s: &str) -> Option<f64> {
    let s = s.trim();
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let rest = s.strip_prefix('P')?;
    let (date_part, time_part) = match rest.find('T') {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };
    let mut total = 0.0;
    let mut num = String::new();
    for c in date_part.chars() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
        } else {
            let v: f64 = num.parse().ok()?;
            num.clear();
            total += match c {
                'D' => v,
                'W' => v * 7.0,
                _ => return None,
            };
        }
    }
    for c in time_part.chars() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
        } else {
            let v: f64 = num.parse().ok()?;
            num.clear();
            total += match c {
                'H' => v / 24.0,
                'M' => v / 1440.0,
                'S' => v / 86_400.0,
                _ => return None,
            };
        }
    }
    if !num.is_empty() || !total.is_finite() {
        return None;
    }
    Some(if neg { -total } else { total })
}

/// Days as an `office:time-value` duration (`PT12H30M00S`; fractional seconds kept).
#[must_use]
pub fn duration_str(days: f64) -> String {
    let neg = days < 0.0;
    let secs = days.abs() * 86_400.0;
    let whole = secs.floor();
    let h = (whole / 3600.0).floor();
    let m = ((whole - h * 3600.0) / 60.0).floor();
    let s = secs - h * 3600.0 - m * 60.0;
    let s_str = if (s - s.round()).abs() < 1e-6 {
        format!("{:02}", s.round())
    } else {
        let mut t = format!("{s:09.6}");
        while t.ends_with('0') {
            t.pop();
        }
        t
    };
    format!("{}PT{h:02}H{m:02}M{s_str}S", if neg { "-" } else { "" })
}

/// Shortest round-trip text of a number (`3`, `0.1`, `1E+300` as Rust prints `1e300`).
#[must_use]
pub fn number_str(v: f64) -> String {
    kiln_ooxml::xml::format_number(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn lengths_convert() {
        assert_eq!(twips("1in"), Some(1440));
        assert_eq!(twips("2.54cm"), Some(1440));
        assert_eq!(twips("72pt"), Some(1440));
        assert_eq!(twips("25.4mm"), Some(1440));
        assert_eq!(emu("1in"), Some(914_400));
        assert_eq!(half_points("11pt"), Some(22));
        assert_eq!(half_points("10.5pt"), Some(21));
        assert_eq!(twips("12"), None);
        assert_eq!(twips("NaNin"), None);
        assert_eq!(twips_str(1440), "1in");
        assert_eq!(twips_str(-360), "-0.25in");
        assert_eq!(emu_str(360_000), "10mm");
        assert_eq!(half_points_str(21), "10.5pt");
    }

    proptest! {
        #[test]
        fn twips_round_trip(tw in -1_000_000i64..1_000_000) {
            prop_assert_eq!(twips(&twips_str(tw)).map(i64::from), Some(tw));
        }

        #[test]
        fn emu_round_trip(e in -50_000_000i64..50_000_000) {
            prop_assert_eq!(emu(&emu_str(e)), Some(e));
        }

        #[test]
        fn dates_round_trip(days in -600_000i64..2_900_000, frac in 0u32..86_400) {
            #[allow(clippy::cast_precision_loss)]
            let serial = days as f64 + f64::from(frac) / 86_400.0;
            let back = date_serial(&serial_date_str(serial)).unwrap();
            prop_assert!((back - serial).abs() < 1e-9, "{serial} -> {back}");
        }
    }

    #[test]
    fn dates_and_durations() {
        assert_eq!(date_serial("1899-12-30"), Some(0.0));
        assert_eq!(date_serial("1900-01-01"), Some(2.0));
        assert_eq!(date_serial("2024-01-31"), Some(45_322.0));
        assert_eq!(date_serial("2024-01-31T12:00:00"), Some(45_322.5));
        assert_eq!(serial_date_str(45_322.0), "2024-01-31");
        assert_eq!(serial_date_str(45_322.75), "2024-01-31T18:00:00");
        assert_eq!(duration_days("PT12H"), Some(0.5));
        assert_eq!(duration_days("PT36H00M00S"), Some(1.5));
        assert_eq!(duration_days("P1DT6H"), Some(1.25));
        assert_eq!(duration_str(0.5), "PT12H00M00S");
        assert_eq!(duration_days(&duration_str(1.0 / 3.0)).map(|d| (d * 3.0 - 1.0).abs() < 1e-9), Some(true));
        assert_eq!(duration_days("PT"), Some(0.0));
        assert_eq!(duration_days("12H"), None);
    }

    #[test]
    fn colours_and_percentages() {
        assert_eq!(color("#4f81BD"), Some(Color::rgb(0x4f, 0x81, 0xbd)));
        assert_eq!(color("red"), None);
        assert_eq!(color_str(Color::rgb(255, 0, 16)), "#ff0010");
        assert_eq!(percent("115%"), Some(115.0));
        assert_eq!(degrees("90deg"), Some(90.0));
    }
}
