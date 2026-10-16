// SPDX-License-Identifier: AGPL-3.0-or-later
//! Data tools: typed-entry recognition (what Excel makes of `12%`, `$1,234.50`, `1/15/2024` or
//! `TRUE`), Remove Duplicates and Text to Columns.

use kiln_formula::SheetId;
use kiln_formula::values::{
    CoercionOptions, ErrorValue, compare_text, date_to_serial, parse_date_time_text,
    parse_number_text,
};
use serde::{Deserialize, Serialize};

use crate::address::Range;
use crate::commands::TextToColumns;
use crate::error::{ModelError, Result};
use crate::value::CellValue;
use crate::workbook::{CellRecord, Workbook};

/// What a typed or pasted entry becomes, and the number format Excel would give the cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// The value.
    pub value: CellValue,
    /// A number format implied by the typing (`0%`, `#,##0.00`, `m/d/yyyy`…), if any.
    pub num_fmt: Option<String>,
}

/// Recognises typed input the way Excel's cell entry does. A leading `'` keeps the rest as text;
/// formulas (`=`) are the caller's business.
pub fn parse_input(text: &str, date1904: bool) -> Parsed {
    let plain = |v: CellValue| Parsed {
        value: v,
        num_fmt: None,
    };
    if let Some(rest) = text.strip_prefix('\'') {
        return plain(CellValue::text(rest));
    }
    let t = text.trim();
    if t.is_empty() {
        return plain(if text.is_empty() {
            CellValue::Blank
        } else {
            CellValue::text(text)
        });
    }
    if t.eq_ignore_ascii_case("TRUE") {
        return plain(CellValue::Bool(true));
    }
    if t.eq_ignore_ascii_case("FALSE") {
        return plain(CellValue::Bool(false));
    }
    if let Some(e) = ErrorValue::from_code(t) {
        return plain(CellValue::Error(e));
    }
    let opts = CoercionOptions {
        date1904,
        current_year: None,
    };
    let body = t.trim_start_matches(['+', '-']);
    let date_like = body.contains(['/', ':', '-'])
        || body
            .chars()
            .any(|c| c.is_alphabetic() && !matches!(c, 'e' | 'E'));
    if !date_like && let Some(n) = parse_number_text(t, &opts) {
        let fmt = number_format_for(t);
        return Parsed {
            value: CellValue::number(n),
            num_fmt: fmt,
        };
    }
    if let Some(n) = parse_date_time_text(t, &opts) {
        let has_date = t.contains(['/', '-'])
            || t.chars()
                .any(|c| c.is_alphabetic() && !matches!(c, 'a' | 'A' | 'p' | 'P' | 'm' | 'M'));
        let has_time = t.contains(':');
        let fmt = match (has_date, has_time) {
            (true, true) => "m/d/yyyy h:mm",
            (false, true) if t.to_ascii_uppercase().contains('M') => "h:mm AM/PM",
            (false, true) => {
                if t.matches(':').count() >= 2 {
                    "h:mm:ss"
                } else {
                    "h:mm"
                }
            }
            _ => {
                if t.chars().any(char::is_alphabetic) {
                    "d-mmm-yy"
                } else {
                    "m/d/yyyy"
                }
            }
        };
        return Parsed {
            value: CellValue::number(n),
            num_fmt: Some(fmt.to_owned()),
        };
    }
    if let Some(n) = parse_number_text(t, &opts) {
        return Parsed {
            value: CellValue::number(n),
            num_fmt: number_format_for(t),
        };
    }
    plain(CellValue::text(text))
}

/// The number format Excel applies when a number is typed this way.
fn number_format_for(t: &str) -> Option<String> {
    let s = t.trim_start_matches(['+', '-', '(']).trim_end_matches(')');
    let decimals = s.split_once('.').map_or(0, |(_, d)| {
        d.chars().take_while(char::is_ascii_digit).count()
    });
    let dec = if decimals > 0 {
        format!(".{}", "0".repeat(decimals.min(2)))
    } else {
        String::new()
    };
    if s.ends_with('%') {
        return Some(format!("0{dec}%"));
    }
    if s.contains(['e', 'E']) && s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return Some("0.00E+00".into());
    }
    if s.starts_with('$') {
        return Some(format!("\"$\"#,##0{dec}_);\\(\"$\"#,##0{dec}\\)"));
    }
    if s.contains(',') {
        return Some(format!("#,##0{dec}"));
    }
    None
}

/// How Text to Columns converts one output column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColumnFormat {
    /// Numbers, dates and logicals are recognised.
    #[default]
    General,
    /// Kept as text.
    Text,
    /// A date in the given order.
    Date(DateOrder),
    /// Not imported.
    Skip,
}

/// Field order of a date column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(missing_docs, clippy::upper_case_acronyms)]
pub enum DateOrder {
    MDY,
    DMY,
    YMD,
    MYD,
    DYM,
    YDM,
}

fn parse_ordered_date(text: &str, order: DateOrder, date1904: bool) -> Option<f64> {
    let parts: Vec<&str> = text
        .split(['/', '-', '.', ' '])
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() != 3 {
        return None;
    }
    let n: Vec<f64> = parts
        .iter()
        .map(|p| p.parse::<f64>().ok())
        .collect::<Option<_>>()?;
    let (y, m, d) = match order {
        DateOrder::MDY => (n[2], n[0], n[1]),
        DateOrder::DMY => (n[2], n[1], n[0]),
        DateOrder::YMD => (n[0], n[1], n[2]),
        DateOrder::MYD => (n[1], n[0], n[2]),
        DateOrder::DYM => (n[1], n[2], n[0]),
        DateOrder::YDM => (n[0], n[2], n[1]),
    };
    let y = if y < 30.0 {
        y + 2000.0
    } else if y < 100.0 {
        y + 1900.0
    } else {
        y
    };
    if !(1.0..=12.0).contains(&m) || !(1.0..=31.0).contains(&d) {
        return None;
    }
    date_to_serial(y, m, d, date1904)
}

/// Splits one line into fields.
pub fn split_line(line: &str, o: &TextToColumns) -> Vec<String> {
    if !o.delimited {
        let chars: Vec<char> = line.chars().collect();
        let mut cuts: Vec<usize> = o
            .breaks
            .iter()
            .copied()
            .filter(|&b| b > 0 && b < chars.len())
            .collect();
        cuts.sort_unstable();
        cuts.dedup();
        let mut out = Vec::new();
        let mut start = 0;
        for c in cuts.into_iter().chain(std::iter::once(chars.len())) {
            out.push(chars[start..c].iter().collect::<String>().trim().to_owned());
            start = c;
        }
        return out;
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    let mut last_was_delim = false;
    while let Some(c) = chars.next() {
        if Some(c) == o.qualifier {
            if quoted && chars.peek() == Some(&c) {
                cur.push(c);
                chars.next();
            } else {
                quoted = !quoted;
            }
            last_was_delim = false;
            continue;
        }
        if !quoted && o.delimiters.contains(&c) {
            if !(o.merge_delimiters && last_was_delim) {
                out.push(std::mem::take(&mut cur));
            }
            last_was_delim = true;
            continue;
        }
        last_was_delim = false;
        cur.push(c);
    }
    out.push(cur);
    out
}

/// The records Text to Columns writes.
pub fn text_to_columns_records(
    wb: &Workbook,
    sheet: SheetId,
    range: Range,
    o: &TextToColumns,
) -> Result<Vec<CellRecord>> {
    range.check()?;
    if range.width() != 1 {
        return Err(ModelError::invalid(
            "Text to Columns works on one column at a time",
        ));
    }
    if o.delimited && o.delimiters.is_empty() {
        return Err(ModelError::invalid("choose at least one delimiter"));
    }
    let s = wb
        .sheet(sheet)
        .ok_or_else(|| ModelError::UnknownSheet(format!("with id {}", sheet.0)))?;
    let dest = o
        .destination
        .unwrap_or(crate::address::CellPos::new(range.r0, range.c0));
    let date1904 = wb.props().date1904;
    let rows: Vec<u32> = s.cells.iter_range(range).map(|(r, _, _)| r).collect();
    let mut out = Vec::new();
    for row in rows {
        let line = wb.display_text(sheet, row, range.c0);
        let fields = split_line(&line, o);
        let mut col = dest.col;
        let r = dest.row + (row - range.r0);
        for (i, f) in fields.into_iter().enumerate() {
            let fmt = o.formats.get(i).copied().unwrap_or_default();
            let value = match fmt {
                ColumnFormat::Skip => continue,
                ColumnFormat::Text => CellValue::text(f.as_str()),
                ColumnFormat::General => {
                    if f.is_empty() {
                        CellValue::Blank
                    } else {
                        parse_input(&f, date1904).value
                    }
                }
                ColumnFormat::Date(order) => parse_ordered_date(&f, order, date1904)
                    .map_or_else(|| CellValue::text(f.as_str()), CellValue::Number),
            };
            crate::address::check_cell(r, col)?;
            let style = s.style_at(r, col);
            out.push(CellRecord {
                row: r,
                col,
                value,
                formula: None,
                legacy: false,
                style,
            });
            col += 1;
        }
    }
    Ok(out)
}

/// The records Remove Duplicates writes: unique rows packed at the top of the range (first
/// occurrence kept), the rest of the range emptied. Rows compare on `columns` (all when empty);
/// text ignores case.
pub fn remove_duplicates_records(
    wb: &Workbook,
    sheet: SheetId,
    range: Range,
    columns: &[u32],
    header: bool,
) -> Result<Vec<CellRecord>> {
    range.check()?;
    let s = wb
        .sheet(sheet)
        .ok_or_else(|| ModelError::UnknownSheet(format!("with id {}", sheet.0)))?;
    if columns.iter().any(|c| *c < range.c0 || *c > range.c1) {
        return Err(ModelError::invalid(
            "a compared column is outside the range",
        ));
    }
    let cols: Vec<u32> = if columns.is_empty() {
        (range.c0..=range.c1).collect()
    } else {
        columns.to_vec()
    };
    let first = range.r0 + u32::from(header);
    if first > range.r1 {
        return Ok(Vec::new());
    }
    let last = s
        .cells
        .used_range(true)
        .map_or(first, |u| u.r1.min(range.r1));
    let key = |r: u32| -> Vec<CellValue> { cols.iter().map(|&c| wb.value(sheet, r, c)).collect() };
    let same = |a: &[CellValue], b: &[CellValue]| {
        a.iter().zip(b).all(|(x, y)| match (x, y) {
            (CellValue::Number(p), CellValue::Number(q)) => p == q,
            _ if x.as_text().is_some() && y.as_text().is_some() => {
                compare_text(x.as_text().unwrap_or(""), y.as_text().unwrap_or(""))
                    == std::cmp::Ordering::Equal
            }
            _ => x == y,
        })
    };
    let mut kept_keys: Vec<Vec<CellValue>> = Vec::new();
    let mut kept_rows: Vec<u32> = Vec::new();
    for r in first..=last.max(first) {
        let k = key(r);
        if kept_keys.iter().any(|x| same(x, &k)) {
            continue;
        }
        kept_keys.push(k);
        kept_rows.push(r);
    }
    if kept_rows.len() as u32 == last.max(first) - first + 1 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for (i, &src) in kept_rows.iter().enumerate() {
        let dst = first + i as u32;
        for c in range.c0..=range.c1 {
            let rec = wb.record(sheet, src, c);
            let formula = rec.formula.as_ref().map(|f| {
                kiln_formula::refs::copy_formula(f, i64::from(dst) - i64::from(src), 0).map_or_else(
                    |_| f.clone(),
                    |t| crate::workbook::strip_equals(&t).to_owned(),
                )
            });
            out.push(CellRecord {
                row: dst,
                col: c,
                formula,
                ..rec
            });
        }
    }
    for r in first + kept_rows.len() as u32..=last {
        for c in range.c0..=range.c1 {
            let style = wb.record(sheet, r, c).style;
            out.push(CellRecord {
                style,
                ..CellRecord::empty(r, c)
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(t: &str) -> CellValue {
        parse_input(t, false).value
    }

    #[test]
    fn recognises_typed_entries() {
        assert_eq!(v("42"), CellValue::Number(42.0));
        assert_eq!(v("TRUE"), CellValue::Bool(true));
        assert_eq!(v("#N/A"), CellValue::Error(ErrorValue::NA));
        assert_eq!(v("'00123"), CellValue::text("00123"));
        assert_eq!(v("hello"), CellValue::text("hello"));
        let p = parse_input("12.5%", false);
        assert_eq!(p.value, CellValue::Number(0.125));
        assert_eq!(p.num_fmt.as_deref(), Some("0.0%"));
        let p = parse_input("$1,234.50", false);
        assert_eq!(p.value, CellValue::Number(1234.5));
        assert!(p.num_fmt.is_some_and(|f| f.contains("#,##0.00")));
        assert_eq!(
            parse_input("1,234", false).num_fmt.as_deref(),
            Some("#,##0")
        );
        let d = parse_input("1/15/2024", false);
        assert_eq!(d.value, CellValue::Number(45306.0));
        assert_eq!(d.num_fmt.as_deref(), Some("m/d/yyyy"));
        let t = parse_input("10:30", false);
        assert_eq!(t.num_fmt.as_deref(), Some("h:mm"));
        assert!((t.value.as_number().unwrap_or(0.0) - 0.4375).abs() < 1e-12);
    }

    #[test]
    fn splits_delimited_and_fixed_width_lines() {
        let o = TextToColumns {
            delimited: true,
            delimiters: vec![',', ';'],
            merge_delimiters: false,
            qualifier: Some('"'),
            breaks: Vec::new(),
            formats: Vec::new(),
            destination: None,
        };
        assert_eq!(split_line("a,\"b,c\";d", &o), vec!["a", "b,c", "d"]);
        assert_eq!(split_line("a,,b", &o), vec!["a", "", "b"]);
        let merged = TextToColumns {
            merge_delimiters: true,
            delimiters: vec![' '],
            ..o.clone()
        };
        assert_eq!(split_line("a   b", &merged), vec!["a", "b"]);
        let fixed = TextToColumns {
            delimited: false,
            breaks: vec![3, 5],
            ..o
        };
        assert_eq!(split_line("abcdefgh", &fixed), vec!["abc", "de", "fgh"]);
    }

    #[test]
    fn parses_dates_in_any_order() {
        assert_eq!(
            parse_ordered_date("15.01.2024", DateOrder::DMY, false),
            Some(45306.0)
        );
        assert_eq!(
            parse_ordered_date("2024-01-15", DateOrder::YMD, false),
            Some(45306.0)
        );
        assert_eq!(
            parse_ordered_date("13/13/2024", DateOrder::MDY, false),
            None
        );
    }
}
