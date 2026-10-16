// SPDX-License-Identifier: AGPL-3.0-or-later
//! AutoFill: what the fill handle and Home › Fill › Series put into the cells beyond a source
//! block — linear and growth series, dates by day, weekday, month or year, text with a trailing
//! number, built-in and custom lists, and copies (formulas adjusted as if copied).
//!
//! In the default mode a single number is copied, a date or a text ending in a number counts
//! up, list members continue the list, and two or more numbers extend their best-fit linear trend
//! (Excel's fill handle behaviour). Filling up or left produces the series backwards.

use kiln_formula::SheetId;
use kiln_formula::numfmt::NumberFormat;
use kiln_formula::refs::copy_formula;
use kiln_formula::values::{date_to_serial, days_in_month, serial_to_date, serial_weekday};
use serde::{Deserialize, Serialize};

use crate::address::Range;
use crate::error::{ModelError, Result};
use crate::value::CellValue;
use crate::workbook::{CellRecord, Workbook, strip_equals};

/// Fill handle options (Auto Fill Options) and Home › Fill › Series types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FillMode {
    /// What dragging the fill handle does.
    #[default]
    Auto,
    /// Copy Cells.
    Copy,
    /// Fill Series (linear; a single value steps by 1).
    Series,
    /// Linear Trend (best fit).
    LinearTrend,
    /// Growth Trend (geometric).
    Growth,
    /// Fill Days.
    Days,
    /// Fill Weekdays.
    Weekdays,
    /// Fill Months.
    Months,
    /// Fill Years.
    Years,
    /// Fill Formatting Only.
    FormatsOnly,
    /// Fill Without Formatting.
    WithoutFormatting,
}

/// Excel's built-in fill lists.
pub const BUILTIN_LISTS: [&[&str]; 4] = [
    &["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
    &[
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ],
    &[
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    &[
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ],
];

/// One source cell of a lane.
#[derive(Debug, Clone, PartialEq)]
pub struct FillCell {
    /// Content and style.
    pub record: CellRecord,
    /// The number format shows a date.
    pub is_date: bool,
}

/// What one filled cell receives.
#[derive(Debug, Clone, PartialEq)]
pub struct FillOut {
    /// Value.
    pub value: CellValue,
    /// Formula (already adjusted), if any.
    pub formula: Option<String>,
    /// Style index.
    pub style: u32,
}

fn split_trailing_number(s: &str) -> Option<(&str, &str, &str)> {
    let end = s.len();
    let digits_start = s
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map(|(i, _)| i)?;
    if digits_start < end {
        return Some((&s[..digits_start], &s[digits_start..end], ""));
    }
    None
}

fn split_number(s: &str) -> Option<(&str, &str, &str)> {
    if let Some(x) = split_trailing_number(s) {
        return Some(x);
    }
    let lead = s.chars().take_while(char::is_ascii_digit).count();
    (lead > 0).then(|| ("", &s[..lead], &s[lead..]))
}

fn find_list<'a>(lists: &'a [Vec<String>], text: &str) -> Option<(&'a [String], usize)> {
    lists.iter().find_map(|l| {
        l.iter()
            .position(|x| x.eq_ignore_ascii_case(text))
            .map(|i| (l.as_slice(), i))
    })
}

fn match_case(template: &str, word: &str) -> String {
    if template.chars().all(|c| !c.is_lowercase()) && template.chars().any(char::is_uppercase) {
        word.to_uppercase()
    } else if template.chars().all(|c| !c.is_uppercase()) {
        word.to_lowercase()
    } else {
        word.to_owned()
    }
}

/// Least-squares line through (i, ys[i]): (intercept, slope).
fn best_fit(ys: &[f64]) -> (f64, f64) {
    let n = ys.len() as f64;
    if ys.len() < 2 {
        return (ys.first().copied().unwrap_or(0.0), 0.0);
    }
    let mx = (n - 1.0) / 2.0;
    let my = ys.iter().sum::<f64>() / n;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (i, y) in ys.iter().enumerate() {
        let dx = i as f64 - mx;
        sxy += dx * (y - my);
        sxx += dx * dx;
    }
    let slope = if sxx == 0.0 { 0.0 } else { sxy / sxx };
    (my - slope * mx, slope)
}

fn add_months(serial: f64, months: i64, date1904: bool) -> Option<f64> {
    let d = serial_to_date(serial.floor(), date1904);
    let total = i64::from(d.year) * 12 + i64::from(d.month) - 1 + months;
    let (y, m) = (total.div_euclid(12), (total.rem_euclid(12) + 1) as u32);
    let day = d.day.min(days_in_month(y, m));
    let base = date_to_serial(y as f64, f64::from(m), f64::from(day), date1904)?;
    Some(base + (serial - serial.floor()))
}

fn months_between(a: f64, b: f64, date1904: bool) -> Option<i64> {
    let (x, y) = (serial_to_date(a, date1904), serial_to_date(b, date1904));
    if x.day != y.day || a.fract() != b.fract() {
        return None;
    }
    Some(
        (i64::from(y.year) * 12 + i64::from(y.month))
            - (i64::from(x.year) * 12 + i64::from(x.month)),
    )
}

fn add_weekdays(serial: f64, n: i64, date1904: bool) -> f64 {
    let mut s = serial;
    let step = if n >= 0 { 1.0 } else { -1.0 };
    let mut left = n.unsigned_abs();
    while left > 0 {
        s += step;
        let wd = serial_weekday(s, date1904);
        // 0 = Sunday … 6 = Saturday.
        if wd != 0 && wd != 6 {
            left -= 1;
        }
    }
    s
}

/// Generates `count` cells continuing `src` (positions `n, n+1, …` forward, or `-1, -2, …`
/// backward, where the source occupies `0..n`).
pub fn generate(
    src: &[FillCell],
    count: usize,
    backward: bool,
    mode: FillMode,
    date1904: bool,
    lists: &[Vec<String>],
) -> Vec<FillOut> {
    let n = src.len();
    if n == 0 || count == 0 {
        return Vec::new();
    }
    // Position k relative to the source start, and the source cell repeating there.
    let pos = |i: usize| -> i64 {
        if backward {
            -(i as i64) - 1
        } else {
            (n + i) as i64
        }
    };
    let pattern = |k: i64| -> &FillCell { &src[k.rem_euclid(n as i64) as usize] };
    let style_at = |k: i64| pattern(k).record.style;
    let copy_at = |k: i64, offset: i64| -> FillOut {
        let c = pattern(k);
        FillOut {
            value: c.record.value.clone(),
            formula: c.record.formula.as_ref().map(|f| {
                copy_formula(f, offset, 0)
                    .map_or_else(|_| f.clone(), |t| strip_equals(&t).to_owned())
            }),
            style: c.record.style,
        }
    };
    // The lane distance between position k and its source cell (formulas shift by it).
    let lane_offset = |k: i64| k - k.rem_euclid(n as i64);
    let any_formula = src.iter().any(|c| c.record.formula.is_some());
    let numbers: Option<Vec<f64>> = src.iter().map(|c| c.record.value.as_number()).collect();
    let numbers = numbers.filter(|_| !any_formula);
    let all_dates = src.iter().all(|c| c.is_date);
    let out = |value: CellValue, k: i64| FillOut {
        value,
        formula: None,
        style: style_at(k),
    };
    let copy_all = || {
        (0..count)
            .map(|i| copy_at(pos(i), lane_offset(pos(i))))
            .collect::<Vec<_>>()
    };
    match mode {
        FillMode::Copy | FillMode::FormatsOnly => return copy_all(),
        FillMode::Growth => {
            if let Some(ys) = &numbers
                && ys.iter().all(|y| *y > 0.0)
            {
                let logs: Vec<f64> = ys.iter().map(|y| y.ln()).collect();
                let (a, b) = if n == 1 {
                    (logs[0], 0.0)
                } else {
                    best_fit(&logs)
                };
                return (0..count)
                    .map(|i| out(CellValue::number((a + b * pos(i) as f64).exp()), pos(i)))
                    .collect();
            }
            return copy_all();
        }
        _ => {}
    }
    if let Some(ys) = &numbers {
        let date_mode = matches!(
            mode,
            FillMode::Days | FillMode::Weekdays | FillMode::Months | FillMode::Years
        ) || (mode == FillMode::Auto && all_dates)
            || (matches!(mode, FillMode::WithoutFormatting) && all_dates);
        if date_mode {
            let last = ys[n - 1];
            let first = ys[0];
            let anchor = if backward { first } else { last };
            let step_months = |unit: i64| -> i64 {
                if n >= 2 {
                    months_between(ys[n - 2], ys[n - 1], date1904)
                        .map_or(unit, |m| if m == 0 { unit } else { m })
                } else {
                    unit
                }
            };
            return (0..count)
                .map(|i| {
                    let j = i as i64 + 1;
                    let dir = if backward { -j } else { j };
                    let v = match mode {
                        FillMode::Weekdays => add_weekdays(anchor, dir, date1904),
                        FillMode::Months => {
                            add_months(anchor, dir * step_months(1), date1904).unwrap_or(anchor)
                        }
                        FillMode::Years => {
                            add_months(anchor, dir * step_months(12).max(12) / 12 * 12, date1904)
                                .unwrap_or(anchor)
                        }
                        _ => {
                            // Days (or the default for dates): month steps continue as months.
                            if n >= 2
                                && let Some(m) = months_between(ys[n - 2], ys[n - 1], date1904)
                                && m != 0
                                && mode == FillMode::Auto
                                && ys
                                    .windows(2)
                                    .all(|w| months_between(w[0], w[1], date1904) == Some(m))
                            {
                                add_months(anchor, dir * m, date1904).unwrap_or(anchor)
                            } else {
                                let step = if n >= 2 { ys[n - 1] - ys[n - 2] } else { 1.0 };
                                let step = if step == 0.0 { 1.0 } else { step };
                                anchor + step * dir as f64
                            }
                        }
                    };
                    out(CellValue::number(v), pos(i))
                })
                .collect();
        }
        let linear = match mode {
            FillMode::Auto | FillMode::WithoutFormatting => n >= 2,
            FillMode::Series | FillMode::LinearTrend => true,
            _ => false,
        };
        if linear {
            let (a, b) = if n == 1 { (ys[0], 1.0) } else { best_fit(ys) };
            return (0..count)
                .map(|i| out(CellValue::number(a + b * pos(i) as f64), pos(i)))
                .collect();
        }
        return copy_all();
    }
    if any_formula {
        return copy_all();
    }
    // Text: list members continue the list; a trailing number counts up.
    let texts: Option<Vec<&str>> = src.iter().map(|c| c.record.value.as_text()).collect();
    if let Some(texts) = texts
        && matches!(
            mode,
            FillMode::Auto | FillMode::Series | FillMode::WithoutFormatting
        )
    {
        let mut all_lists: Vec<Vec<String>> = lists.to_vec();
        all_lists.extend(
            BUILTIN_LISTS
                .iter()
                .map(|l| l.iter().map(|s| (*s).to_owned()).collect()),
        );
        let found: Option<Vec<(&[String], usize)>> =
            texts.iter().map(|t| find_list(&all_lists, t)).collect();
        if let Some(found) = found
            && found.iter().all(|(l, _)| std::ptr::eq(*l, found[0].0))
        {
            let list = found[0].0;
            let len = list.len() as i64;
            let idx: Vec<i64> = found.iter().map(|(_, i)| *i as i64).collect();
            let step = if n >= 2 {
                (idx[n - 1] - idx[n - 2]).rem_euclid(len).max(1)
            } else {
                1
            };
            let (anchor, template) = if backward {
                (idx[0], texts[0])
            } else {
                (idx[n - 1], texts[n - 1])
            };
            return (0..count)
                .map(|i| {
                    let j = i as i64 + 1;
                    let k = (anchor + if backward { -j } else { j } * step).rem_euclid(len);
                    out(
                        CellValue::text(match_case(template, &list[k as usize])),
                        pos(i),
                    )
                })
                .collect();
        }
        let parts: Option<Vec<(&str, &str, &str)>> =
            texts.iter().map(|t| split_number(t)).collect();
        if let Some(parts) = parts
            && parts.iter().all(|p| p.0 == parts[0].0 && p.2 == parts[0].2)
        {
            let nums: Vec<i64> = parts
                .iter()
                .map(|p| p.1.parse::<i64>().unwrap_or(0))
                .collect();
            let step = if n >= 2 { nums[n - 1] - nums[n - 2] } else { 1 };
            let width = parts.iter().map(|p| p.1.len()).max().unwrap_or(1);
            let padded = parts.iter().any(|p| p.1.len() > 1 && p.1.starts_with('0'));
            let anchor = if backward { nums[0] } else { nums[n - 1] };
            return (0..count)
                .map(|i| {
                    let j = i as i64 + 1;
                    let v = anchor + if backward { -j } else { j } * step;
                    let digits = if padded {
                        format!("{:0width$}", v.abs(), width = width)
                    } else {
                        v.abs().to_string()
                    };
                    out(
                        CellValue::text(format!("{}{digits}{}", parts[0].0, parts[0].2)),
                        pos(i),
                    )
                })
                .collect();
        }
    }
    copy_all()
}

/// The records AutoFill writes into `target` outside `source`.
pub fn autofill_records(
    wb: &Workbook,
    sheet: SheetId,
    source: Range,
    target: Range,
    mode: FillMode,
) -> Result<Vec<CellRecord>> {
    source.check()?;
    target.check()?;
    if !source.is_inside(&target) {
        return Err(ModelError::invalid(
            "the fill range must contain the source cells",
        ));
    }
    let s = wb
        .sheet(sheet)
        .ok_or_else(|| ModelError::UnknownSheet(format!("with id {}", sheet.0)))?;
    let vertical = target.c0 == source.c0 && target.c1 == source.c1;
    let horizontal = target.r0 == source.r0 && target.r1 == source.r1;
    if !vertical && !horizontal {
        return Err(ModelError::invalid("fill in one direction at a time"));
    }
    if target == source {
        return Ok(Vec::new());
    }
    let date_style = |style: u32| NumberFormat::parse(wb.styles().num_fmt(style)).is_date();
    let lanes: Vec<u32> = if vertical {
        (source.c0..=source.c1).collect()
    } else {
        (source.r0..=source.r1).collect()
    };
    let (s0, s1, t0, t1) = if vertical {
        (source.r0, source.r1, target.r0, target.r1)
    } else {
        (source.c0, source.c1, target.c0, target.c1)
    };
    let mut out = Vec::new();
    for lane in lanes {
        let at = |i: u32| if vertical { (i, lane) } else { (lane, i) };
        let src: Vec<FillCell> = (s0..=s1)
            .map(|i| {
                let (r, c) = at(i);
                let mut record = wb.record(sheet, r, c);
                if !s.cells.contains(r, c) {
                    record.style = s.style_at(r, c);
                }
                let is_date = date_style(record.style);
                FillCell { record, is_date }
            })
            .collect();
        let mut emit = |cells: Vec<FillOut>, positions: Vec<u32>| {
            for (o, i) in cells.into_iter().zip(positions) {
                let (r, c) = at(i);
                let current = wb.record(sheet, r, c);
                let rec = match mode {
                    FillMode::FormatsOnly => CellRecord {
                        style: o.style,
                        ..current
                    },
                    FillMode::WithoutFormatting => CellRecord {
                        row: r,
                        col: c,
                        value: o.value,
                        formula: o.formula,
                        legacy: false,
                        style: current.style,
                    },
                    _ => CellRecord {
                        row: r,
                        col: c,
                        value: o.value,
                        formula: o.formula,
                        legacy: false,
                        style: o.style,
                    },
                };
                out.push(rec);
            }
        };
        if t1 > s1 {
            let count = (t1 - s1) as usize;
            let cells = generate_lane(&src, count, false, mode, wb, vertical);
            emit(cells, (s1 + 1..=t1).collect());
        }
        if t0 < s0 {
            let count = (s0 - t0) as usize;
            let cells = generate_lane(&src, count, true, mode, wb, vertical);
            emit(cells, (t0..s0).rev().collect());
        }
    }
    Ok(out)
}

fn generate_lane(
    src: &[FillCell],
    count: usize,
    backward: bool,
    mode: FillMode,
    wb: &Workbook,
    vertical: bool,
) -> Vec<FillOut> {
    let cells = generate(
        src,
        count,
        backward,
        mode,
        wb.props().date1904,
        wb.custom_lists(),
    );
    if vertical {
        return cells;
    }
    // Horizontal fills shift formulas by columns: redo the copies with a column offset.
    let n = src.len() as i64;
    cells
        .into_iter()
        .enumerate()
        .map(|(i, mut o)| {
            let k = if backward {
                -(i as i64) - 1
            } else {
                n + i as i64
            };
            let base = &src[k.rem_euclid(n) as usize];
            if let Some(f) = &base.record.formula {
                let offset = k - k.rem_euclid(n);
                o.formula = Some(
                    copy_formula(f, 0, offset)
                        .map_or_else(|_| f.clone(), |t| strip_equals(&t).to_owned()),
                );
            }
            o
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(v: impl Into<CellValue>) -> FillCell {
        FillCell {
            record: CellRecord {
                value: v.into(),
                ..CellRecord::default()
            },
            is_date: false,
        }
    }

    fn date(serial: f64) -> FillCell {
        FillCell {
            is_date: true,
            ..cell(serial)
        }
    }

    fn texts(out: &[FillOut]) -> Vec<String> {
        out.iter().map(|o| o.value.display_general()).collect()
    }

    fn series(src: &[FillCell], count: usize, mode: FillMode) -> Vec<String> {
        texts(&generate(src, count, false, mode, false, &[]))
    }

    #[test]
    fn copies_a_single_number_and_extends_trends() {
        assert_eq!(series(&[cell(5.0)], 3, FillMode::Auto), vec!["5", "5", "5"]);
        assert_eq!(
            series(&[cell(5.0)], 3, FillMode::Series),
            vec!["6", "7", "8"]
        );
        assert_eq!(
            series(&[cell(1.0), cell(3.0)], 3, FillMode::Auto),
            vec!["5", "7", "9"]
        );
        // Best fit through 1, 2, 4: slope 1.5, intercept 0.8333…
        let fit = generate(
            &[cell(1.0), cell(2.0), cell(4.0)],
            1,
            false,
            FillMode::Auto,
            false,
            &[],
        );
        assert!((fit[0].value.as_number().unwrap_or(0.0) - 5.333_333_333_333_333).abs() < 1e-9);
        assert_eq!(
            series(&[cell(2.0), cell(4.0)], 2, FillMode::Growth),
            vec!["8", "16"]
        );
        let back = generate(
            &[cell(10.0), cell(20.0)],
            2,
            true,
            FillMode::Auto,
            false,
            &[],
        );
        assert_eq!(texts(&back), vec!["0", "-10"]);
    }

    #[test]
    fn fills_dates_by_day_weekday_month_and_year() {
        // 2024-01-31 is serial 45322 (a Wednesday).
        assert_eq!(
            generate(&[date(45322.0)], 2, false, FillMode::Auto, false, &[])
                .iter()
                .map(|o| o.value.as_number().unwrap_or(0.0))
                .collect::<Vec<_>>(),
            vec![45323.0, 45324.0]
        );
        let wd = generate(&[date(45324.0)], 2, false, FillMode::Weekdays, false, &[]);
        // Friday 2 Feb → Monday 5 Feb, Tuesday 6 Feb.
        assert_eq!(wd[0].value.as_number(), Some(45327.0));
        assert_eq!(wd[1].value.as_number(), Some(45328.0));
        let months = generate(&[date(45322.0)], 2, false, FillMode::Months, false, &[]);
        // 31 Jan → 29 Feb 2024 → 31 Mar? No: each step adds months to the anchor, clamped.
        assert_eq!(months[0].value.as_number(), Some(45351.0));
        assert_eq!(months[1].value.as_number(), Some(45382.0));
        let years = generate(&[date(45322.0)], 1, false, FillMode::Years, false, &[]);
        assert_eq!(years[0].value.as_number(), Some(45688.0));
        // Two month-apart dates continue by months in the default mode.
        let auto = generate(
            &[date(45306.0), date(45337.0)],
            1,
            false,
            FillMode::Auto,
            false,
            &[],
        );
        assert_eq!(auto[0].value.as_number(), Some(45366.0));
    }

    #[test]
    fn continues_lists_and_numbered_text() {
        assert_eq!(
            series(&[cell("Mon")], 3, FillMode::Auto),
            vec!["Tue", "Wed", "Thu"]
        );
        assert_eq!(
            series(&[cell("DECEMBER")], 2, FillMode::Auto),
            vec!["JANUARY", "FEBRUARY"]
        );
        assert_eq!(
            series(&[cell("jan"), cell("mar")], 2, FillMode::Auto),
            vec!["may", "jul"]
        );
        assert_eq!(
            series(&[cell("Item 9")], 2, FillMode::Auto),
            vec!["Item 10", "Item 11"]
        );
        assert_eq!(series(&[cell("Q007")], 1, FillMode::Auto), vec!["Q008"]);
        assert_eq!(
            series(&[cell("plain")], 2, FillMode::Auto),
            vec!["plain", "plain"]
        );
        let custom = vec![vec!["Low".to_owned(), "Mid".to_owned(), "High".to_owned()]];
        let out = generate(&[cell("Mid")], 2, false, FillMode::Auto, false, &custom);
        assert_eq!(texts(&out), vec!["High", "Low"]);
        assert_eq!(series(&[cell("Tue")], 1, FillMode::Copy), vec!["Tue"]);
    }

    #[test]
    fn repeats_mixed_blocks_and_adjusts_formulas() {
        let src = vec![
            FillCell {
                record: CellRecord {
                    formula: Some("A1*2".into()),
                    ..CellRecord::default()
                },
                is_date: false,
            },
            cell("x"),
        ];
        let out = generate(&src, 3, false, FillMode::Auto, false, &[]);
        assert_eq!(out[0].formula.as_deref(), Some("A3*2"));
        assert_eq!(out[1].value.as_text(), Some("x"));
        assert_eq!(out[2].formula.as_deref(), Some("A5*2"));
    }
}
