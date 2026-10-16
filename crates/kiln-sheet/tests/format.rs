// SPDX-License-Identifier: AGPL-3.0-or-later
//! Displayed text: the v1.0 number-format expectations, run through the sheet model's binding
//! of `kiln-formula`'s number formats (`kiln_sheet::format`). The engine's documented deviations
//! (docs/formula-deviations.md, WP-56) are listed in `KNOWN`; locale-specific names and
//! separators and the renderer's fill/pad runs are not part of the model.

use kiln_formula::values::ErrorValue;
use kiln_sheet::CellValue;
use kiln_sheet::format::{Placement, format_value};
use kiln_sheet::styles::builtin_num_fmt;

const GENERAL: &[(f64, &str)] = &[
    (0.0, "0"),
    (1.0, "1"),
    (-1.0, "-1"),
    (42.0, "42"),
    (3.5, "3.5"),
    (-3.25, "-3.25"),
    (0.1, "0.1"),
    (0.1 + 0.2, "0.3"),
    (1.0 / 3.0, "0.333333333"),
    (2.0 / 3.0, "0.666666667"),
    (-1.0 / 3.0, "-0.333333333"),
    (10.0 / 3.0, "3.333333333"),
    (123.456, "123.456"),
    (123456789.0, "123456789"),
    (1234567890.0, "1234567890"),
    (12345678901.0, "12345678901"),
    (99999999999.0, "99999999999"),
    (123456789012.0, "1.23457E+11"),
    (999999999999.0, "1E+12"),
    (99999999999.6, "1E+11"),
    (1234567890.5, "1234567891"),
    (1234567890.4, "1234567890"),
    (-1234567890.5, "-1234567891"),
    (123456.7890123, "123456.789"),
    (1e15, "1E+15"),
    (1.5e20, "1.5E+20"),
    (-2.5e30, "-2.5E+30"),
    (1e100, "1E+100"),
    (1.23456789e100, "1.2346E+100"),
    (0.0001, "0.0001"),
    (0.000123456789, "0.000123457"),
    (0.00001, "0.00001"),
    (0.00001234, "0.00001234"),
    (0.0000123456789, "1.23457E-05"),
    (0.000001, "0.000001"),
    (1.234e-7, "1.234E-07"),
    (1e-10, "1E-10"),
    (-1e-10, "-1E-10"),
    (9.99999999999, "10"),
    (0.99999999999, "1"),
    (45000.5, "45000.5"),
    (std::f64::consts::PI, "3.141592654"),
    (std::f64::consts::E, "2.718281828"),
    (1e-300, "1E-300"),
];
const NUMBERS: &[(&str, f64, &str)] = &[
    ("0", 1234.567, "1235"),
    ("0", -0.4, "-0"),
    ("0.0", -0.04, "-0.0"),
    ("0;(0)", -0.4, "(0)"),
    ("0", 0.5, "1"),
    ("0", 2.5, "3"),
    ("0.00", 1234.567, "1234.57"),
    ("0.00", 0.005, "0.01"),
    ("0.00", -1.005, "-1.01"),
    ("#,##0", 1234567.89, "1,234,568"),
    ("#,##0", -1234.0, "-1,234"),
    ("#,##0", 0.0, "0"),
    ("#,##0.00", 1234.5, "1,234.50"),
    ("#,##0.00", 0.1, "0.10"),
    ("#,##0,", 1234567.0, "1,235"),
    ("#,##0,,", 1234567890.0, "1,235"),
    ("0.0,,\"M\"", 12345678.0, "12.3M"),
    ("#.##", 1.5, "1.5"),
    ("#.##", 0.5, ".5"),
    ("#.00", 0.5, ".50"),
    ("0.0#", 1.0, "1.0"),
    ("0.0#", 1.234, "1.23"),
    ("00000", 123.0, "00123"),
    ("000-00-0000", 123456789.0, "123-45-6789"),
    ("(###) ###-####", 5551234567.0, "(555) 123-4567"),
    ("\"$\"#,##0.00", 1234.5, "$1,234.50"),
    ("\"$\"#,##0.00", -1234.5, "-$1,234.50"),
    ("$#,##0.00", 99.999, "$100.00"),
    ("\"Total: \"0", 3.0, "Total: 3"),
    ("0\" units\"", 12.0, "12 units"),
    ("0 \"kg\"", 5.0, "5 kg"),
    ("\\$0.00", 3.0, "$3.00"),
    ("0%", 0.256, "26%"),
    ("0%", 1.0, "100%"),
    ("0.00%", 0.12345, "12.35%"),
    ("0.0%", -0.05, "-5.0%"),
    ("0%", 0.0, "0%"),
    ("0.00E+00", 12345.0, "1.23E+04"),
    ("0.00E+00", 0.000123, "1.23E-04"),
    ("0.0E+0", 1234567.0, "1.2E+6"),
    ("##0.0E+0", 12345.0, "12.3E+3"),
    ("##0.0E+0", 1234567.0, "1.2E+6"),
    ("0.00E+00", -5.0, "-5.00E+00"),
    ("# ?/?", 1.25, "1 1/4"),
    ("# ?/?", 0.5, " 1/2"),
    ("# ?/?", 3.0, "3    "),
    ("# ??/??", 2.3125, "2  5/16"),
    ("# ???/???", std::f64::consts::PI, "3  16/113"),
    ("?/?", 0.75, "3/4"),
    ("# ?/4", 1.3, "1 1/4"),
    ("# ?/8", 2.6, "2 5/8"),
    ("# ??/100", 0.37, " 37/100"),
    ("0;-0;\"zero\"", 0.0, "zero"),
    ("0;-0;\"zero\"", -3.0, "-3"),
    ("0;(0)", -3.0, "(3)"),
    ("0.00;[Red]-0.00", -2.0, "-2.00"),
    ("#,##0_);(#,##0)", 1000.0, "1,000 "),
    ("#,##0_);(#,##0)", -1000.0, "(1,000)"),
    ("[<0]\"neg\";[>0]\"pos\";\"zero\"", -1.0, "neg"),
    ("[<0]\"neg\";[>0]\"pos\";\"zero\"", 1.0, "pos"),
    ("[<0]\"neg\";[>0]\"pos\";\"zero\"", 0.0, "zero"),
    ("[>=1000]#,##0,\"K\";0", 2500.0, "3K"),
    ("[>=1000]#,##0,\"K\";0", 999.0, "999"),
    ("[=1]\"one\";[=2]\"two\";0", 2.0, "two"),
    ("0;-0;;@", 0.0, ""),
    (";;;", 123.0, ""),
    ("0.0", 1e15, "1000000000000000.0"),
    ("#", 0.0, ""),
    ("?", 0.0, " "),
    ("0.???", 1.5, "1.5  "),
    ("???.???", 12.34, " 12.34 "),
];
const DATES: &[(&str, f64, &str)] = &[
    ("m/d/yyyy", 45000.0, "3/15/2023"),
    ("mm/dd/yyyy", 45000.0, "03/15/2023"),
    ("d-mmm-yy", 45000.0, "15-Mar-23"),
    ("d-mmm", 45000.0, "15-Mar"),
    ("mmm-yy", 45000.0, "Mar-23"),
    ("mmmm d, yyyy", 45000.0, "March 15, 2023"),
    ("dddd", 45000.0, "Wednesday"),
    ("ddd", 45000.0, "Wed"),
    ("dddd, mmmm dd, yyyy", 45000.0, "Wednesday, March 15, 2023"),
    ("yyyy-mm-dd", 45000.0, "2023-03-15"),
    ("mmmmm", 45000.0, "M"),
    ("yy", 45000.0, "23"),
    ("h:mm AM/PM", 45000.75, "6:00 PM"),
    ("h:mm:ss AM/PM", 0.5, "12:00:00 PM"),
    ("h:mm AM/PM", 0.0, "12:00 AM"),
    ("h:mm", 45000.75, "18:00"),
    ("hh:mm:ss", 0.123456, "02:57:47"),
    ("h:mm:ss", 1.0 / 86400.0, "0:00:01"),
    ("[h]:mm:ss", 1.5, "36:00:00"),
    ("[h]:mm", 2.25, "54:00"),
    ("[mm]:ss", 0.5, "720:00"),
    ("mm:ss", 0.000694444, "01:00"),
    ("mm:ss.0", 0.0000115740741, "00:01.0"),
    ("m/d/yyyy h:mm", 45000.5, "3/15/2023 12:00"),
    (
        "yyyy-mm-dd hh:mm:ss",
        45000.999988426,
        "2023-03-15 23:59:59",
    ),
    ("m/d/yyyy", 1.0, "1/1/1900"),
    ("m/d/yyyy", 60.0, "2/29/1900"),
    ("m/d/yyyy", 61.0, "3/1/1900"),
    ("m/d/yyyy", 2958465.0, "12/31/9999"),
    ("dd/mm/yyyy", 36526.0, "01/01/2000"),
    ("d mmmm yyyy", 44927.0, "1 January 2023"),
    ("h AM/PM", 0.25, "6 AM"),
];

fn text(v: f64, code: &str) -> String {
    format_value(&CellValue::Number(v), code, false).text
}

/// Expectations where the engine follows its oracle rather than the v1.0 table (code, value,
/// what Kiln shows).
const KNOWN: &[(&str, f64, &str)] = &[
    // Negative values that round to zero show no sign (the numfmt oracle and LibreOffice agree;
    // docs/formula-deviations.md).
    ("0", -0.4, "0"),
    ("0.0", -0.04, "0.0"),
];

fn check(cases: &[(&str, f64, &str)]) -> Vec<String> {
    cases
        .iter()
        .filter_map(|&(code, v, want)| {
            let want = KNOWN
                .iter()
                .find(|k| k.0 == code && k.1 == v)
                .map_or(want, |k| k.2);
            let got = text(v, code);
            (got != want).then(|| format!("{code:?} {v} → {got:?}, expected {want:?}"))
        })
        .collect()
}

#[test]
fn general_follows_excels_eleven_character_rule() {
    let bad: Vec<String> = GENERAL
        .iter()
        .filter_map(|&(v, want)| {
            let got = text(v, "General");
            (got != want).then(|| format!("{v} → {got:?}, expected {want:?}"))
        })
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn number_formats() {
    let bad = check(NUMBERS);
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn dates_and_times() {
    let bad = check(DATES);
    assert!(bad.is_empty(), "{bad:#?}");
    assert!(format_value(&CellValue::Number(-1.0), "m/d/yyyy", false).overflow);
    assert!(format_value(&CellValue::Number(2_958_466.0), "m/d/yyyy", false).overflow);
    assert_eq!(
        format_value(&CellValue::Number(0.0), "m/d/yyyy", true).text,
        "1/1/1904"
    );
    assert_eq!(
        format_value(&CellValue::Number(43538.0), "yyyy-mm-dd", true).text,
        "2023-03-15"
    );
}

#[test]
fn text_logicals_and_errors() {
    for (code, v, want) in [
        ("@", "hello", "hello"),
        ("\"Name: \"@", "Ann", "Name: Ann"),
        ("0;-0;0;\"<\"@\">\"", "x", "<x>"),
        ("0.00", "text", "text"),
        ("General", "abc", "abc"),
        ("@", "", ""),
    ] {
        assert_eq!(
            format_value(&CellValue::text(v), code, false).text,
            want,
            "{code}"
        );
    }
    let r = format_value(&CellValue::Number(12.5), "@", false);
    assert_eq!(r.text, "12.5");
    for (v, want) in [
        (CellValue::Bool(true), "TRUE"),
        (CellValue::Bool(false), "FALSE"),
        (CellValue::Error(ErrorValue::NA), "#N/A"),
        (CellValue::Error(ErrorValue::Div0), "#DIV/0!"),
        (CellValue::Error(ErrorValue::Spill), "#SPILL!"),
        (CellValue::Blank, ""),
    ] {
        assert_eq!(format_value(&v, "0.00", false).text, want);
    }
    assert_eq!(
        format_value(&CellValue::Number(5.0), "General", false).placement,
        Placement::Right
    );
    assert_eq!(
        format_value(&CellValue::text("a"), "General", false).placement,
        Placement::Left
    );
    assert_eq!(
        format_value(&CellValue::Bool(true), "General", false).placement,
        Placement::Center
    );
}

#[test]
fn colours() {
    for (code, v, rgb) in [
        ("[Red]0", 5.0, 0xFF0000),
        ("[Blue]0;[Red]-0", -5.0, 0xFF0000),
        ("[Blue]0;[Red]-0", 5.0, 0x0000FF),
        ("[Green]0", 1.0, 0x00FF00),
        ("[Magenta]0", 1.0, 0xFF00FF),
        ("[Cyan]0", 1.0, 0x00FFFF),
        ("[Yellow]0", 1.0, 0xFFFF00),
        ("[White]0", 1.0, 0xFFFFFF),
        ("[Black]0", 1.0, 0x000000),
        ("[Color 10]0", 1.0, 0x008000),
        ("[Color 3]0", 1.0, 0xFF0000),
        ("#,##0.00_);[Red](#,##0.00)", -1.0, 0xFF0000),
    ] {
        assert_eq!(
            format_value(&CellValue::Number(v), code, false).color,
            Some(rgb),
            "{code}"
        );
    }
    assert_eq!(
        format_value(&CellValue::Number(5.0), "0;[Red]-0", false).color,
        None
    );
    assert_eq!(
        format_value(&CellValue::text("x"), "0;0;0;[Blue]@", false).color,
        Some(0x0000FF)
    );
}

#[test]
fn built_in_formats() {
    for (id, v, want) in [
        (0, 1234.5, "1234.5"),
        (1, 1234.5, "1235"),
        (2, 1234.5, "1234.50"),
        (3, 1234.5, "1,235"),
        (4, 1234.5, "1,234.50"),
        (5, -1234.5, "($1,235)"),
        (6, 1234.5, "$1,235 "),
        (7, 1234.5, "$1,234.50 "),
        (8, -1234.5, "($1,234.50)"),
        (9, 0.5, "50%"),
        (10, 0.5, "50.00%"),
        (11, 1234.5, "1.23E+03"),
        (12, 0.5, " 1/2"),
        (13, 0.25, "  1/4 "),
        (14, 45000.0, "3/15/2023"),
        (15, 45000.0, "15-Mar-23"),
        (16, 45000.0, "15-Mar"),
        (17, 45000.0, "Mar-23"),
        (18, 0.75, "6:00 PM"),
        (19, 0.75, "6:00:00 PM"),
        (20, 0.75, "18:00"),
        (21, 0.75, "18:00:00"),
        (22, 45000.75, "3/15/2023 18:00"),
        (37, -5.0, "(5)"),
        (38, 5.0, "5 "),
        (39, 1.5, "1.50 "),
        (40, -1.5, "(1.50)"),
        (41, 1234.0, " 1,234 "),
        (41, -1234.0, " (1,234)"),
        (42, 1234.0, " $1,234 "),
        (43, 0.5, " 0.50 "),
        (44, 1234.5, " $1,234.50 "),
        (44, -1234.5, " $(1,234.50)"),
        (45, 0.001, "01:26"),
        (46, 1.25, "30:00:00"),
        (47, 0.001, "01:26.4"),
        (48, 12345.0, "12.3E+3"),
        (49, 12.0, "12"),
    ] {
        let code = builtin_num_fmt(id).unwrap_or("General");
        assert_eq!(text(v, code), want, "numFmtId {id}");
    }
}

#[test]
fn invalid_formats_fall_back_to_general() {
    assert_eq!(text(1.5, "0.0.0\""), "1.5");
}
