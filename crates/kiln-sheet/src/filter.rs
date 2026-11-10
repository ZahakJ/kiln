// SPDX-License-Identifier: AGPL-3.0-or-later
//! AutoFilter evaluation: which rows of a sheet's AutoFilter and of its tables' filters pass —
//! value lists (with blanks and date groups), custom comparisons with `*`/`?` wildcards, top or
//! bottom N items or percent, dynamic filters (above/below average, today, this week…, quarters
//! and months of any year) and colour and icon filters.

use std::cmp::Ordering;

use kiln_formula::SheetId;
use kiln_formula::values::{
    CoercionOptions, compare_text, parse_number_text, serial_to_date, serial_to_time,
    serial_weekday,
};
use rustc_hash::FxHashMap;

use crate::address::{CellPos, Range};
use crate::axis::RowProps;
use crate::error::{ModelError, Result};
use crate::meta::{
    ColumnFilter, CustomCriterion, DateGroupItem, DateGrouping, DynamicFilter, FilterColumn,
    FilterOperator,
};
use crate::value::CellValue;
use crate::workbook::Workbook;

/// Matches Excel filter wildcards: `*` any run, `?` one character, `~` escapes. Ignores case.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    // Tokens: (char, literal) or star / any.
    #[derive(Clone, Copy, PartialEq)]
    enum Tok {
        Lit(char),
        One,
        Star,
    }
    let mut toks = Vec::new();
    let mut i = 0;
    while i < p.len() {
        match p[i] {
            '~' if i + 1 < p.len() => {
                toks.push(Tok::Lit(p[i + 1]));
                i += 2;
                continue;
            }
            '*' => toks.push(Tok::Star),
            '?' => toks.push(Tok::One),
            c => toks.push(Tok::Lit(c)),
        }
        i += 1;
    }
    let (mut ti, mut pi) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < toks.len() && (toks[pi] == Tok::One || toks[pi] == Tok::Lit(t[ti])) {
            ti += 1;
            pi += 1;
        } else if pi < toks.len() && toks[pi] == Tok::Star {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < toks.len() && toks[pi] == Tok::Star {
        pi += 1;
    }
    pi == toks.len()
}

struct ColumnCtx {
    numbers_sorted: Vec<f64>,
    mean: f64,
}

fn date_group_hit(g: &DateGroupItem, serial: f64, date1904: bool) -> bool {
    let d = serial_to_date(serial, date1904);
    let t = serial_to_time(serial);
    if i64::from(g.year) != i64::from(d.year) {
        return false;
    }
    let levels = [
        (DateGrouping::Month, u32::from(g.month), d.month),
        (DateGrouping::Day, u32::from(g.day), d.day),
        (DateGrouping::Hour, u32::from(g.hour), t.hour),
        (DateGrouping::Minute, u32::from(g.minute), t.minute),
        (DateGrouping::Second, u32::from(g.second), t.second),
    ];
    for (level, want, have) in levels {
        if want != have {
            return false;
        }
        if g.grouping == level {
            return true;
        }
    }
    true
}

fn custom_hit(c: &CustomCriterion, value: &CellValue, shown: &str) -> bool {
    let opts = CoercionOptions::default();
    let target_num = parse_number_text(&c.value, &opts);
    let ord = match (value.as_number(), target_num) {
        (Some(x), Some(y)) => x.partial_cmp(&y),
        _ => {
            if matches!(c.op, FilterOperator::Equal | FilterOperator::NotEqual) {
                let eq = if c.value.is_empty() {
                    shown.is_empty()
                } else {
                    wildcard_match(&c.value, shown)
                };
                return eq == (c.op == FilterOperator::Equal);
            }
            if value.is_blank() || value.as_number().is_some() != target_num.is_some() {
                return false;
            }
            Some(compare_text(shown, &c.value))
        }
    };
    let Some(o) = ord else { return false };
    match c.op {
        FilterOperator::Equal => o == Ordering::Equal,
        FilterOperator::NotEqual => o != Ordering::Equal,
        FilterOperator::GreaterThan => o == Ordering::Greater,
        FilterOperator::GreaterThanOrEqual => o != Ordering::Less,
        FilterOperator::LessThan => o == Ordering::Less,
        FilterOperator::LessThanOrEqual => o != Ordering::Greater,
    }
}

fn dynamic_hit(f: DynamicFilter, value: &CellValue, today: f64, mean: f64, date1904: bool) -> bool {
    let Some(x) = value.as_number() else {
        return false;
    };
    let d = x.floor();
    let t = today.floor();
    let date = serial_to_date(d, date1904);
    let now = serial_to_date(t, date1904);
    let month_index = |y: i32, m: u32| i64::from(y) * 12 + i64::from(m) - 1;
    let (dm, tm) = (
        month_index(date.year, date.month),
        month_index(now.year, now.month),
    );
    let (dq, tq) = (dm.div_euclid(3), tm.div_euclid(3));
    let week_start = |s: f64| s - f64::from(serial_weekday(s, date1904));
    let (dw, tw) = (week_start(d), week_start(t));
    match f {
        DynamicFilter::AboveAverage => x > mean,
        DynamicFilter::BelowAverage => x < mean,
        DynamicFilter::Today => d == t,
        DynamicFilter::Yesterday => d == t - 1.0,
        DynamicFilter::Tomorrow => d == t + 1.0,
        DynamicFilter::ThisWeek => dw == tw,
        DynamicFilter::LastWeek => dw == tw - 7.0,
        DynamicFilter::NextWeek => dw == tw + 7.0,
        DynamicFilter::ThisMonth => dm == tm,
        DynamicFilter::LastMonth => dm == tm - 1,
        DynamicFilter::NextMonth => dm == tm + 1,
        DynamicFilter::ThisQuarter => dq == tq,
        DynamicFilter::LastQuarter => dq == tq - 1,
        DynamicFilter::NextQuarter => dq == tq + 1,
        DynamicFilter::ThisYear => date.year == now.year,
        DynamicFilter::LastYear => date.year == now.year - 1,
        DynamicFilter::NextYear => date.year == now.year + 1,
        DynamicFilter::YearToDate => date.year == now.year && d <= t,
        DynamicFilter::Quarter(q) => (date.month - 1) / 3 + 1 == u32::from(q),
        DynamicFilter::Month(m) => date.month == u32::from(m),
    }
}

impl Workbook {
    /// Whether one row passes one column filter.
    fn filter_hit(
        &mut self,
        sheet: SheetId,
        at: CellPos,
        filter: &ColumnFilter,
        ctx: &ColumnCtx,
        today: f64,
    ) -> bool {
        let value = self.value(sheet, at.row, at.col);
        let shown = self.display_text(sheet, at.row, at.col);
        let date1904 = self.props.date1904;
        match filter {
            ColumnFilter::Values {
                values,
                blank,
                date_groups,
            } => {
                if value.is_empty_text_or_blank() {
                    return *blank;
                }
                if values
                    .iter()
                    .any(|v| compare_text(v, &shown) == Ordering::Equal)
                {
                    return true;
                }
                value
                    .as_number()
                    .is_some_and(|n| date_groups.iter().any(|g| date_group_hit(g, n, date1904)))
            }
            ColumnFilter::Custom { and, criteria } => {
                let mut hits = criteria.iter().map(|c| custom_hit(c, &value, &shown));
                if *and {
                    hits.all(|h| h)
                } else {
                    hits.any(|h| h)
                }
            }
            ColumnFilter::Top10 {
                top,
                percent,
                value: n,
            } => {
                let Some(x) = value.as_number() else {
                    return false;
                };
                let len = ctx.numbers_sorted.len();
                if len == 0 {
                    return false;
                }
                let k = if *percent {
                    ((len as f64 * n / 100.0).floor() as usize).max(1)
                } else {
                    *n as usize
                }
                .clamp(1, len);
                if *top {
                    x >= ctx.numbers_sorted[len - k]
                } else {
                    x <= ctx.numbers_sorted[k - 1]
                }
            }
            ColumnFilter::Dynamic(f) => dynamic_hit(*f, &value, today, ctx.mean, date1904),
            ColumnFilter::Color { cell_color, dxf } => {
                let Some(d) = self.styles.dxf(*dxf).cloned() else {
                    return false;
                };
                let theme = self.theme.clone();
                let style = self.sheet(sheet).map_or(0, |s| s.style_at(at.row, at.col));
                if *cell_color {
                    let want = d
                        .fill
                        .as_ref()
                        .and_then(|f| f.main_color())
                        .map(|c| c.resolve(&theme, 0xFFFFFF));
                    let have = self
                        .styles
                        .fill_of(style)
                        .main_color()
                        .map(|c| c.resolve(&theme, 0xFFFFFF));
                    want == have
                } else {
                    let want = d
                        .font
                        .as_ref()
                        .and_then(|f| f.color)
                        .map(|c| c.resolve(&theme, 0));
                    let have = self
                        .styles
                        .font_of(style)
                        .color
                        .map_or(0, |c| c.resolve(&theme, 0));
                    want.unwrap_or(0) == have
                }
            }
            ColumnFilter::Icon { icon_set, icon_id } => {
                let cf = self.conditional_formats(sheet, Range::cell(at.row, at.col));
                cf.get(&at)
                    .and_then(|v| v.icon)
                    .is_some_and(|i| i.set == *icon_set && i.index == *icon_id)
            }
        }
    }

    /// Rows of `data` (a filter's range without its header row) that pass every column filter.
    pub fn filter_pass(
        &mut self,
        sheet: SheetId,
        data: Range,
        columns: &[FilterColumn],
    ) -> Vec<bool> {
        let today = self.today();
        let mut pass = vec![true; data.height() as usize];
        for fc in columns {
            let Some(filter) = &fc.filter else { continue };
            let Some(col) = data.c0.checked_add(fc.col_id).filter(|c| *c <= data.c1) else {
                continue;
            };
            let numbers: Vec<f64> = (data.r0..=data.r1)
                .filter_map(|r| self.value(sheet, r, col).as_number())
                .collect();
            let mean = if numbers.is_empty() {
                0.0
            } else {
                numbers.iter().sum::<f64>() / numbers.len() as f64
            };
            let mut numbers_sorted = numbers;
            numbers_sorted.sort_by(f64::total_cmp);
            let ctx = ColumnCtx {
                numbers_sorted,
                mean,
            };
            for (i, r) in (data.r0..=data.r1).enumerate() {
                if pass[i] && !self.filter_hit(sheet, CellPos::new(r, col), filter, &ctx, today) {
                    pass[i] = false;
                }
            }
        }
        pass
    }
}

/// The row properties that apply the sheet's AutoFilter and its tables' filters: rows that fail
/// become hidden and filtered, rows that pass are shown.
pub fn filter_row_entries(
    wb: &mut Workbook,
    sheet: SheetId,
) -> Result<Vec<(u32, Option<RowProps>)>> {
    let s = wb
        .sheet(sheet)
        .ok_or_else(|| ModelError::UnknownSheet(format!("with id {}", sheet.0)))?;
    let mut filters: Vec<(Range, Vec<FilterColumn>)> = Vec::new();
    if let Some(f) = &s.meta.auto_filter
        && f.range.height() > 1
    {
        filters.push((
            Range {
                r0: f.range.r0 + 1,
                ..f.range
            },
            f.columns.clone(),
        ));
    }
    for t in &s.meta.tables {
        if let (Some(f), Some(data)) = (&t.auto_filter, t.data_range()) {
            filters.push((data, f.columns.clone()));
        }
    }
    if filters.is_empty() {
        return Err(ModelError::invalid("the sheet has no filter to apply"));
    }
    let mut verdict: FxHashMap<u32, bool> = FxHashMap::default();
    for (data, cols) in filters {
        let pass = wb.filter_pass(sheet, data, &cols);
        for (i, ok) in pass.into_iter().enumerate() {
            let row = data.r0 + i as u32;
            let e = verdict.entry(row).or_insert(true);
            *e &= ok;
        }
    }
    let s = wb
        .sheet(sheet)
        .ok_or_else(|| ModelError::UnknownSheet(format!("with id {}", sheet.0)))?;
    let mut rows: Vec<(u32, bool)> = verdict.into_iter().collect();
    rows.sort_unstable();
    let mut out = Vec::new();
    for (row, ok) in rows {
        let prev = s.rows().get(row).copied().unwrap_or_default();
        let next = RowProps {
            hidden: !ok,
            filtered: !ok,
            ..prev
        };
        if next != prev {
            out.push((row, Some(next)));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_wildcards_like_excel() {
        assert!(wildcard_match("a*", "Apple"));
        assert!(wildcard_match("*an*", "banana"));
        assert!(wildcard_match("b?n", "BAN"));
        assert!(!wildcard_match("b?n", "bean"));
        assert!(wildcard_match("~*", "*"));
        assert!(!wildcard_match("~*", "x"));
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("a*c*e", "abcde"));
    }

    #[test]
    fn custom_criteria_compare_numbers_and_text() {
        let c = |op, v: &str| CustomCriterion {
            op,
            value: v.into(),
        };
        assert!(custom_hit(
            &c(FilterOperator::GreaterThan, "10"),
            &CellValue::Number(11.0),
            "11"
        ));
        assert!(!custom_hit(
            &c(FilterOperator::GreaterThan, "10"),
            &CellValue::text("zz"),
            "zz"
        ));
        assert!(custom_hit(
            &c(FilterOperator::Equal, "ap*"),
            &CellValue::text("Apple"),
            "Apple"
        ));
        assert!(custom_hit(
            &c(FilterOperator::NotEqual, "ap*"),
            &CellValue::text("Pear"),
            "Pear"
        ));
        assert!(custom_hit(
            &c(FilterOperator::LessThan, "m"),
            &CellValue::text("apple"),
            "apple"
        ));
        assert!(custom_hit(
            &c(FilterOperator::Equal, ""),
            &CellValue::Blank,
            ""
        ));
    }

    #[test]
    fn dynamic_filters_use_calendar_periods() {
        // Wednesday 2024-01-17 = 45308.
        let t = 45308.0;
        let v = |n: f64| CellValue::Number(n);
        assert!(dynamic_hit(
            DynamicFilter::ThisWeek,
            &v(45305.0),
            t,
            0.0,
            false
        ));
        assert!(dynamic_hit(
            DynamicFilter::LastWeek,
            &v(45304.0),
            t,
            0.0,
            false
        ));
        assert!(dynamic_hit(
            DynamicFilter::ThisQuarter,
            &v(45382.0),
            t,
            0.0,
            false
        ));
        assert!(dynamic_hit(
            DynamicFilter::LastQuarter,
            &v(45291.0),
            t,
            0.0,
            false
        ));
        assert!(dynamic_hit(
            DynamicFilter::YearToDate,
            &v(45292.0),
            t,
            0.0,
            false
        ));
        assert!(!dynamic_hit(
            DynamicFilter::YearToDate,
            &v(45309.0),
            t,
            0.0,
            false
        ));
        assert!(dynamic_hit(
            DynamicFilter::Month(1),
            &v(44927.0),
            t,
            0.0,
            false
        ));
        assert!(dynamic_hit(
            DynamicFilter::Quarter(4),
            &v(45291.0),
            t,
            0.0,
            false
        ));
        assert!(dynamic_hit(
            DynamicFilter::AboveAverage,
            &v(5.0),
            t,
            4.0,
            false
        ));
    }

    #[test]
    fn date_groups_match_down_to_their_level() {
        let g = DateGroupItem {
            grouping: DateGrouping::Month,
            year: 2024,
            month: 1,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
        };
        assert!(date_group_hit(&g, 45308.25, false));
        assert!(!date_group_hit(&g, 45323.0, false));
    }
}
