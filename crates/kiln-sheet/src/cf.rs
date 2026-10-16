// SPDX-License-Identifier: AGPL-3.0-or-later
//! Conditional formatting evaluation: what every rule type of Excel does to the cells of a range
//! (for the renderer's viewport), as per-cell visuals — the differential format to layer on the
//! cell format, colour-scale fills, data bars (with the x14 negative-value and axis options) and
//! icons.
//!
//! Rules apply in priority order; a higher-priority format wins where formats conflict, the
//! highest-priority colour scale, data bar and icon set win, and "Stop If True" stops
//! lower-priority rules for the cells it applied to. Statistics (averages, ranks, duplicates,
//! scale end points) are computed once per rule over all of its ranges.

use std::cmp::Ordering;

use kiln_formula::SheetId;
use kiln_formula::refs::copy_formula;
use kiln_formula::values::{Value, compare_values, lookup_equals, serial_to_date, serial_weekday};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::address::{CellPos, Range};
use crate::color::mix;
use crate::meta::{
    AxisPosition, BarDirection, CfKind, CfRule, Cfvo, CfvoType, DataBar, IconSet, IconSetName,
    Operator, TimePeriod,
};
use crate::styles::Dxf;
use crate::value::CellValue;
use crate::workbook::Workbook;

/// A data bar as drawn in one cell. Positions are fractions of the cell width from the start
/// edge (the left edge, or the right edge for right-to-left bars).
#[derive(Debug, Clone, PartialEq)]
pub struct DataBarVisual {
    /// Where the bar starts.
    pub start: f64,
    /// Where it ends.
    pub end: f64,
    /// Fill `0xRRGGBB`.
    pub color: u32,
    /// Border `0xRRGGBB`.
    pub border: Option<u32>,
    /// Gradient fill.
    pub gradient: bool,
    /// A negative value (drawn from the axis towards the start edge).
    pub negative: bool,
    /// The axis position and colour, when the range has negative values.
    pub axis: Option<(f64, u32)>,
    /// Drawn right to left.
    pub right_to_left: bool,
}

/// An icon from an icon set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconVisual {
    /// The set the icon comes from.
    pub set: IconSetName,
    /// Index in the set (0 = the lowest values' icon).
    pub index: u32,
}

/// What conditional formatting does to one cell.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CellVisual {
    /// Differential format layered over the cell format.
    pub dxf: Option<Dxf>,
    /// Colour-scale fill `0xRRGGBB`.
    pub scale_color: Option<u32>,
    /// Data bar.
    pub data_bar: Option<DataBarVisual>,
    /// Icon.
    pub icon: Option<IconVisual>,
    /// Hide the value (a bar or icon set with "show value" off).
    pub hide_value: bool,
}

struct Stats {
    numbers: Vec<f64>,
    min: f64,
    max: f64,
    mean: f64,
    std_dev: f64,
    counts: Vec<(Value, usize)>,
}

fn cell_values(wb: &Workbook, sheet: SheetId, ranges: &[Range]) -> Vec<(CellPos, CellValue)> {
    let Some(s) = wb.sheet(sheet) else {
        return Vec::new();
    };
    let mut seen = FxHashSet::default();
    let mut out = Vec::new();
    for r in ranges {
        for (row, col, _) in s.cells.iter_range(*r) {
            if seen.insert((row, col)) {
                out.push((CellPos::new(row, col), wb.value(sheet, row, col)));
            }
        }
        // Values spilled into the range are not stored cells; small ranges are scanned whole.
        if r.cell_count() <= 10_000 {
            for p in r.cells() {
                if !seen.contains(&(p.row, p.col)) {
                    let v = wb.value(sheet, p.row, p.col);
                    if !v.is_blank() {
                        seen.insert((p.row, p.col));
                        out.push((p, v));
                    }
                }
            }
        }
    }
    out
}

fn stats(values: &[(CellPos, CellValue)]) -> Stats {
    let numbers: Vec<f64> = values.iter().filter_map(|(_, v)| v.as_number()).collect();
    let n = numbers.len() as f64;
    let min = numbers.iter().copied().fold(f64::INFINITY, f64::min);
    let max = numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mean = if numbers.is_empty() {
        0.0
    } else {
        numbers.iter().sum::<f64>() / n
    };
    let std_dev = if numbers.len() < 2 {
        0.0
    } else {
        (numbers.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt()
    };
    let mut counts: Vec<(Value, usize)> = Vec::new();
    for (_, v) in values {
        if v.is_blank() {
            continue;
        }
        let ev = v.to_value();
        match counts.iter_mut().find(|(x, _)| lookup_equals(x, &ev)) {
            Some((_, c)) => *c += 1,
            None => counts.push((ev, 1)),
        }
    }
    Stats {
        numbers,
        min,
        max,
        mean,
        std_dev,
        counts,
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let p = p.clamp(0.0, 100.0) / 100.0;
    let pos = p * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

struct Ctx<'a> {
    wb: &'a mut Workbook,
    sheet: SheetId,
    today: f64,
}

impl Ctx<'_> {
    /// Evaluates a rule formula written for `anchor`, as if in `at`.
    fn eval(&mut self, formula: &str, anchor: CellPos, at: CellPos) -> Value {
        let dr = i64::from(at.row) - i64::from(anchor.row);
        let dc = i64::from(at.col) - i64::from(anchor.col);
        let text = if dr == 0 && dc == 0 {
            formula.to_owned()
        } else {
            match copy_formula(formula, dr, dc) {
                Ok(t) => t,
                Err(_) => return Value::Error(kiln_formula::values::ErrorValue::Name),
            }
        };
        self.wb
            .evaluate_at(&text, self.sheet, at.row, at.col)
            .map(|v| v.top_left())
            .unwrap_or(Value::Error(kiln_formula::values::ErrorValue::Name))
    }

    fn cfvo(
        &mut self,
        c: &Cfvo,
        st: &Stats,
        sorted: &[f64],
        anchor: CellPos,
        is_min: bool,
    ) -> Option<f64> {
        let num = |v: &Option<String>| v.as_deref().and_then(|t| t.trim().parse::<f64>().ok());
        Some(match c.kind {
            CfvoType::Min => st.min,
            CfvoType::Max => st.max,
            CfvoType::AutoMin => st.min.min(0.0),
            CfvoType::AutoMax => st.max.max(0.0),
            CfvoType::Num => match num(&c.value) {
                Some(n) => n,
                None => self.formula_number(c, anchor)?,
            },
            CfvoType::Percent => {
                let p = num(&c.value).unwrap_or(if is_min { 0.0 } else { 100.0 });
                st.min + (st.max - st.min) * p / 100.0
            }
            CfvoType::Percentile => percentile(sorted, num(&c.value).unwrap_or(50.0)),
            CfvoType::Formula => self.formula_number(c, anchor)?,
        })
    }

    fn formula_number(&mut self, c: &Cfvo, anchor: CellPos) -> Option<f64> {
        let f = c.value.as_deref()?;
        self.eval(f, anchor, anchor).as_number()
    }
}

fn text_of(v: &CellValue) -> Option<String> {
    match v {
        CellValue::Blank => None,
        CellValue::Error(_) => None,
        other => Some(other.display_general().to_lowercase()),
    }
}

fn compare_op(op: Operator, v: &Value, a: &Value, b: Option<&Value>) -> bool {
    let cmp = |x: &Value| compare_values(v, x).ok();
    match op {
        Operator::Equal => cmp(a) == Some(Ordering::Equal),
        Operator::NotEqual => cmp(a).is_some_and(|o| o != Ordering::Equal),
        Operator::GreaterThan => cmp(a) == Some(Ordering::Greater),
        Operator::LessThan => cmp(a) == Some(Ordering::Less),
        Operator::GreaterThanOrEqual => cmp(a).is_some_and(|o| o != Ordering::Less),
        Operator::LessThanOrEqual => cmp(a).is_some_and(|o| o != Ordering::Greater),
        Operator::Between | Operator::NotBetween => {
            let Some(b) = b else { return false };
            // Excel accepts the bounds in either order.
            let (lo, hi) = match compare_values(a, b) {
                Ok(Ordering::Greater) => (b, a),
                _ => (a, b),
            };
            let inside = compare_values(v, lo).is_ok_and(|o| o != Ordering::Less)
                && compare_values(v, hi).is_ok_and(|o| o != Ordering::Greater);
            let err = compare_values(v, lo).is_err() || compare_values(v, hi).is_err();
            !err && (inside == (op == Operator::Between))
        }
    }
}

fn in_period(p: TimePeriod, serial: f64, today: f64, date1904: bool) -> bool {
    let d = serial.floor();
    let t = today.floor();
    let wd = f64::from(serial_weekday(t, date1904)) + 1.0; // WEEKDAY: 1 = Sunday
    let (dy, dm) = {
        let x = serial_to_date(d, date1904);
        (x.year, x.month)
    };
    let today_date = serial_to_date(t, date1904);
    let month_index = |y: i32, m: u32| i64::from(y) * 12 + i64::from(m);
    let ti = month_index(today_date.year, today_date.month);
    let di = month_index(dy, dm);
    match p {
        TimePeriod::Today => d == t,
        TimePeriod::Yesterday => d == t - 1.0,
        TimePeriod::Tomorrow => d == t + 1.0,
        TimePeriod::Last7Days => t - d <= 6.0 && d <= t,
        TimePeriod::ThisWeek => t - d <= wd - 1.0 && d - t <= 7.0 - wd,
        TimePeriod::LastWeek => t - d >= wd && t - d < wd + 7.0,
        TimePeriod::NextWeek => d - t > 7.0 - wd && d - t < 15.0 - wd,
        TimePeriod::ThisMonth => di == ti,
        TimePeriod::LastMonth => di == ti - 1,
        TimePeriod::NextMonth => di == ti + 1,
    }
}

fn icon_thresholds(
    ctx: &mut Ctx<'_>,
    set: &IconSet,
    st: &Stats,
    sorted: &[f64],
    anchor: CellPos,
) -> Vec<(f64, bool)> {
    set.cfvos
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let v = ctx
                .cfvo(c, st, sorted, anchor, i == 0)
                .unwrap_or(f64::NEG_INFINITY);
            (v, c.gte)
        })
        .collect()
}

fn data_bar(bar: &DataBar, v: f64, lo: f64, hi: f64, theme: &crate::Theme) -> DataBarVisual {
    let (min_len, max_len) = (
        f64::from(bar.min_length.min(100)) / 100.0,
        f64::from(bar.max_length.clamp(bar.min_length, 100)) / 100.0,
    );
    let color = bar.color.resolve(theme, 0x638EC6);
    let border = bar.border_color.map(|c| c.resolve(theme, color));
    let rtl = bar.direction == BarDirection::RightToLeft;
    let has_negative = lo < 0.0 && bar.axis_position != AxisPosition::None;
    if !has_negative {
        let span = hi - lo;
        let f = if span > 0.0 {
            ((v - lo) / span).clamp(0.0, 1.0)
        } else {
            1.0
        };
        return DataBarVisual {
            start: 0.0,
            end: min_len + (max_len - min_len) * f,
            color,
            border,
            gradient: bar.gradient,
            negative: false,
            axis: None,
            right_to_left: rtl,
        };
    }
    // An axis divides the cell: negative bars grow from it towards the start edge.
    let axis = match bar.axis_position {
        AxisPosition::Middle => 0.5,
        _ => {
            let span = hi.max(0.0) - lo;
            if span > 0.0 { -lo / span } else { 0.5 }
        }
    };
    let axis_color = bar.axis_color.map_or(0x000000, |c| c.resolve(theme, 0));
    let (start, end, negative) = if v < 0.0 {
        let f = if lo < 0.0 {
            (v / lo).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (axis - axis * f, axis, true)
    } else {
        let f = if hi > 0.0 {
            (v / hi).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (axis, axis + (1.0 - axis) * f, false)
    };
    DataBarVisual {
        start,
        end,
        color: if negative {
            bar.negative_color
                .map_or(0xFF0000, |c| c.resolve(theme, 0xFF0000))
        } else {
            color
        },
        border: if negative {
            bar.negative_border_color
                .map(|c| c.resolve(theme, 0xFF0000))
                .or(border)
        } else {
            border
        },
        gradient: bar.gradient,
        negative,
        axis: Some((axis, axis_color)),
        right_to_left: rtl,
    }
}

impl Workbook {
    /// The serial number of today (the workbook clock).
    pub fn today(&mut self) -> f64 {
        self.evaluate("TODAY()")
            .ok()
            .and_then(|v| v.as_number())
            .unwrap_or(0.0)
    }

    /// Conditional-format visuals of the cells of `area` that any rule changes.
    pub fn conditional_formats(
        &mut self,
        sheet: SheetId,
        area: Range,
    ) -> FxHashMap<CellPos, CellVisual> {
        let mut out: FxHashMap<CellPos, CellVisual> = FxHashMap::default();
        let Some(s) = self.sheet(sheet) else {
            return out;
        };
        let mut rules: Vec<(CfRule, Vec<Range>)> = s
            .meta
            .cf
            .iter()
            .filter(|b| b.ranges.iter().any(|r| r.intersects(&area)))
            .flat_map(|b| b.rules.iter().map(move |r| (r.clone(), b.ranges.clone())))
            .collect();
        rules.sort_by_key(|(r, _)| r.priority);
        if rules.is_empty() {
            return out;
        }
        let theme = self.theme.clone();
        let date1904 = self.props.date1904;
        let today = self.today();
        let dxfs: FxHashMap<u32, Dxf> = rules
            .iter()
            .filter_map(|(r, _)| r.kind.dxf())
            .filter_map(|d| self.styles.dxf(d).map(|x| (d, x.clone())))
            .collect();
        let mut stopped: FxHashSet<CellPos> = FxHashSet::default();
        let mut ctx = Ctx {
            wb: self,
            sheet,
            today,
        };
        for (rule, ranges) in rules {
            let anchor = ranges
                .first()
                .map_or(CellPos::new(0, 0), |r| CellPos::new(r.r0, r.c0));
            let all = cell_values(ctx.wb, sheet, &ranges);
            let st = stats(&all);
            let mut sorted = st.numbers.clone();
            sorted.sort_by(f64::total_cmp);
            // The cells of this rule inside the requested area (blank cells included where a
            // rule can apply to them).
            let mut targets: Vec<(CellPos, CellValue)> = Vec::new();
            let mut seen = FxHashSet::default();
            for r in &ranges {
                let Some(x) = r.intersection(&area) else {
                    continue;
                };
                if x.cell_count() <= 250_000 {
                    for p in x.cells() {
                        if seen.insert(p) {
                            targets.push((p, ctx.wb.value(sheet, p.row, p.col)));
                        }
                    }
                } else {
                    for (p, v) in all.iter().filter(|(p, _)| x.contains(p.row, p.col)) {
                        if seen.insert(*p) {
                            targets.push((*p, v.clone()));
                        }
                    }
                }
            }
            let top_threshold = |rank: u32, percent: bool, bottom: bool| -> Option<f64> {
                let n = sorted.len();
                if n == 0 {
                    return None;
                }
                let k = if percent {
                    ((n as f64 * f64::from(rank) / 100.0).floor() as usize).max(1)
                } else {
                    rank as usize
                }
                .clamp(1, n);
                Some(if bottom { sorted[k - 1] } else { sorted[n - k] })
            };
            let scale = match &rule.kind {
                CfKind::ColorScale { cfvos, colors }
                    if cfvos.len() >= 2 && cfvos.len() == colors.len() =>
                {
                    let stops: Vec<(f64, u32)> = cfvos
                        .iter()
                        .enumerate()
                        .map(|(i, c)| {
                            (
                                ctx.cfvo(c, &st, &sorted, anchor, i == 0)
                                    .unwrap_or(f64::NAN),
                                colors[i].resolve(&theme, 0xFFFFFF),
                            )
                        })
                        .collect();
                    Some(stops)
                }
                _ => None,
            };
            let bar_range = match &rule.kind {
                CfKind::DataBar(b) => Some((
                    ctx.cfvo(&b.min, &st, &sorted, anchor, true)
                        .unwrap_or(st.min),
                    ctx.cfvo(&b.max, &st, &sorted, anchor, false)
                        .unwrap_or(st.max),
                )),
                _ => None,
            };
            let icons = match &rule.kind {
                CfKind::IconSet(set) => Some(icon_thresholds(&mut ctx, set, &st, &sorted, anchor)),
                _ => None,
            };
            for (p, v) in targets {
                if stopped.contains(&p) {
                    continue;
                }
                let applies = match &rule.kind {
                    CfKind::CellIs {
                        operator, formulas, ..
                    } => {
                        // Blank cells compare as 0 (or empty text), as in Excel.
                        let a = formulas.first().map(|f| ctx.eval(f, anchor, p));
                        let b = formulas.get(1).map(|f| ctx.eval(f, anchor, p));
                        match a {
                            Some(a) => compare_op(*operator, &v.to_value(), &a, b.as_ref()),
                            None => false,
                        }
                    }
                    CfKind::Expression { formula, .. } => match ctx.eval(formula, anchor, p) {
                        Value::Bool(b) => b,
                        Value::Number(n) => n != 0.0,
                        _ => false,
                    },
                    CfKind::ContainsText { text, .. } => {
                        text_of(&v).is_some_and(|t| t.contains(&text.to_lowercase()))
                    }
                    CfKind::NotContainsText { text, .. } => {
                        !text_of(&v).is_some_and(|t| t.contains(&text.to_lowercase()))
                    }
                    CfKind::BeginsWith { text, .. } => {
                        text_of(&v).is_some_and(|t| t.starts_with(&text.to_lowercase()))
                    }
                    CfKind::EndsWith { text, .. } => {
                        text_of(&v).is_some_and(|t| t.ends_with(&text.to_lowercase()))
                    }
                    CfKind::ContainsBlanks { .. } => {
                        v.as_text().map_or(v.is_blank(), |t| t.trim().is_empty())
                    }
                    CfKind::NotContainsBlanks { .. } => {
                        !v.as_text().map_or(v.is_blank(), |t| t.trim().is_empty())
                    }
                    CfKind::ContainsErrors { .. } => matches!(v, CellValue::Error(_)),
                    CfKind::NotContainsErrors { .. } => !matches!(v, CellValue::Error(_)),
                    CfKind::TimePeriod { period, .. } => v
                        .as_number()
                        .is_some_and(|n| in_period(*period, n, ctx.today, date1904)),
                    CfKind::DuplicateValues { .. } | CfKind::UniqueValues { .. } => {
                        if v.is_blank() {
                            false
                        } else {
                            let ev = v.to_value();
                            let count = st
                                .counts
                                .iter()
                                .find(|(x, _)| lookup_equals(x, &ev))
                                .map_or(1, |(_, c)| *c);
                            (count > 1) == matches!(rule.kind, CfKind::DuplicateValues { .. })
                        }
                    }
                    CfKind::Top10 {
                        rank,
                        percent,
                        bottom,
                        ..
                    } => match (v.as_number(), top_threshold(*rank, *percent, *bottom)) {
                        (Some(x), Some(t)) => {
                            if *bottom {
                                x <= t
                            } else {
                                x >= t
                            }
                        }
                        _ => false,
                    },
                    CfKind::AboveAverage {
                        above,
                        equal,
                        std_dev,
                        ..
                    } => match v.as_number() {
                        Some(x) if !st.numbers.is_empty() => {
                            let d = f64::from(*std_dev) * st.std_dev;
                            let limit = if *above { st.mean + d } else { st.mean - d };
                            match (above, equal) {
                                (true, true) => x >= limit,
                                (true, false) => x > limit,
                                (false, true) => x <= limit,
                                (false, false) => x < limit,
                            }
                        }
                        _ => false,
                    },
                    CfKind::ColorScale { .. } | CfKind::DataBar(_) | CfKind::IconSet(_) => {
                        v.as_number().is_some()
                    }
                };
                if !applies {
                    continue;
                }
                let vis = out.entry(p).or_default();
                match &rule.kind {
                    CfKind::ColorScale { .. } => {
                        if vis.scale_color.is_none()
                            && let (Some(stops), Some(x)) = (&scale, v.as_number())
                        {
                            vis.scale_color = scale_color(stops, x);
                        }
                    }
                    CfKind::DataBar(b) => {
                        if vis.data_bar.is_none()
                            && let (Some((lo, hi)), Some(x)) = (bar_range, v.as_number())
                        {
                            vis.data_bar = Some(data_bar(b, x, lo, hi, &theme));
                            vis.hide_value |= !b.show_value;
                        }
                    }
                    CfKind::IconSet(set) => {
                        if vis.icon.is_none()
                            && let (Some(th), Some(x)) = (&icons, v.as_number())
                        {
                            vis.icon = icon_for(set, th, x);
                            vis.hide_value |= !set.show_value;
                        }
                    }
                    other => {
                        if let Some(d) = other.dxf().and_then(|d| dxfs.get(&d)) {
                            vis.dxf = Some(match &vis.dxf {
                                // Formats from higher-priority rules win where they conflict.
                                Some(higher) => d.layered(higher),
                                None => d.clone(),
                            });
                        }
                    }
                }
                if rule.stop_if_true {
                    stopped.insert(p);
                }
            }
        }
        out.retain(|_, v| *v != CellVisual::default());
        out
    }
}

fn scale_color(stops: &[(f64, u32)], x: f64) -> Option<u32> {
    if stops.iter().any(|(v, _)| v.is_nan()) {
        return None;
    }
    let (first, last) = (stops[0], stops[stops.len() - 1]);
    if x <= first.0 {
        return Some(first.1);
    }
    if x >= last.0 {
        return Some(last.1);
    }
    for w in stops.windows(2) {
        let (a, b) = (w[0], w[1]);
        if x >= a.0 && x <= b.0 {
            let t = if b.0 > a.0 {
                (x - a.0) / (b.0 - a.0)
            } else {
                0.0
            };
            return Some(mix(a.1, b.1, t));
        }
    }
    Some(last.1)
}

fn icon_for(set: &IconSet, thresholds: &[(f64, bool)], x: f64) -> Option<IconVisual> {
    let n = set.icon_set.len().max(thresholds.len());
    if n == 0 {
        return None;
    }
    let mut idx = 0usize;
    for (i, (t, gte)) in thresholds.iter().enumerate().skip(1) {
        if (*gte && x >= *t) || (!*gte && x > *t) {
            idx = i;
        }
    }
    if set.reverse {
        idx = n - 1 - idx.min(n - 1);
    }
    if let Some(c) = set.custom_icons.get(idx) {
        return (c.icon_set != IconSetName::NoIcons).then_some(IconVisual {
            set: c.icon_set,
            index: c.icon_id,
        });
    }
    (set.icon_set != IconSetName::NoIcons).then_some(IconVisual {
        set: set.icon_set,
        index: idx as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_interpolate_between_stops() {
        let stops = vec![(0.0, 0xF8696B), (50.0, 0xFFEB84), (100.0, 0x63BE7B)];
        assert_eq!(scale_color(&stops, -5.0), Some(0xF8696B));
        assert_eq!(scale_color(&stops, 50.0), Some(0xFFEB84));
        assert_eq!(scale_color(&stops, 200.0), Some(0x63BE7B));
        assert_eq!(
            scale_color(&stops, 25.0),
            Some(mix(0xF8696B, 0xFFEB84, 0.5))
        );
    }

    #[test]
    fn periods_follow_excel_formulas() {
        // Wednesday 2024-01-17 is serial 45308.
        let t = 45308.0;
        assert!(in_period(TimePeriod::Today, t + 0.5, t, false));
        assert!(in_period(TimePeriod::Yesterday, t - 1.0, t, false));
        assert!(in_period(TimePeriod::Last7Days, t - 6.0, t, false));
        assert!(!in_period(TimePeriod::Last7Days, t - 7.0, t, false));
        // This week: Sunday 14 … Saturday 20 January.
        assert!(in_period(TimePeriod::ThisWeek, 45305.0, t, false));
        assert!(in_period(TimePeriod::ThisWeek, 45311.0, t, false));
        assert!(!in_period(TimePeriod::ThisWeek, 45312.0, t, false));
        assert!(in_period(TimePeriod::LastWeek, 45304.0, t, false));
        assert!(in_period(TimePeriod::NextWeek, 45312.0, t, false));
        assert!(in_period(TimePeriod::ThisMonth, 45292.0, t, false));
        assert!(in_period(TimePeriod::LastMonth, 45291.0, t, false));
        assert!(in_period(TimePeriod::NextMonth, 45323.0, t, false));
    }
}
