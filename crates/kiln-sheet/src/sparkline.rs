// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sparklines (Insert ▸ Sparklines): small line, column and win/loss charts drawn inside cells,
//! in groups that share their type, colours, markers and axis settings (SpreadsheetML's
//! `x14:sparklineGroups`), and the geometry of one sparkline in its cell.

use serde::{Deserialize, Serialize};

use crate::address::{CellPos, Range};
use crate::color::Color;
use crate::num::F64;
use crate::value::CellValue;
use crate::{SheetId, Workbook};

/// The type of a sparkline group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SparklineKind {
    /// A line through the values.
    #[default]
    Line,
    /// A column per value.
    Column,
    /// A column up for each positive value and down for each negative one, all the same height.
    WinLoss,
}

/// How empty cells show (Hidden & Empty Cell Settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EmptyCells {
    /// Gaps.
    #[default]
    Gap,
    /// Zero.
    Zero,
    /// Connect data points with a line.
    Span,
}

/// The minimum or maximum of the vertical axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SparkAxis {
    /// Automatic for each sparkline.
    #[default]
    Individual,
    /// Same for all sparklines of the group.
    Group,
    /// A custom value.
    Custom(F64),
}

/// One sparkline: the cell it is drawn in and the values it shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sparkline {
    /// The cell.
    pub at: CellPos,
    /// The sheet of the data (`None`: the sparkline's own sheet).
    #[serde(default)]
    pub sheet: Option<String>,
    /// The data: one row or one column.
    pub data: Range,
}

/// The colours of a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct SparkColors {
    pub series: Color,
    pub negative: Color,
    pub axis: Color,
    pub markers: Color,
    pub first: Color,
    pub last: Color,
    pub high: Color,
    pub low: Color,
}

impl Default for SparkColors {
    /// Excel's default style (Sparkline Style Accent 1, darker 50 %): a dark blue series, red
    /// negatives and high/low points.
    fn default() -> SparkColors {
        SparkColors {
            series: Color::rgb(0x37_6092),
            negative: Color::rgb(0xD0_0000),
            axis: Color::rgb(0x00_0000),
            markers: Color::rgb(0xD0_0000),
            first: Color::rgb(0xD0_0000),
            last: Color::rgb(0xD0_0000),
            high: Color::rgb(0xD0_0000),
            low: Color::rgb(0xD0_0000),
        }
    }
}

/// A group of sparklines that share their settings.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SparklineGroup {
    /// Type.
    #[serde(default)]
    pub kind: SparklineKind,
    /// The sparklines.
    pub sparklines: Vec<Sparkline>,
    /// Colours.
    #[serde(default)]
    pub colors: SparkColors,
    /// Line weight in points.
    #[serde(default = "line_weight")]
    pub line_weight: F64,
    /// Markers on every point (line sparklines).
    #[serde(default)]
    pub markers: bool,
    /// High point.
    #[serde(default)]
    pub high: bool,
    /// Low point.
    #[serde(default)]
    pub low: bool,
    /// First point.
    #[serde(default)]
    pub first: bool,
    /// Last point.
    #[serde(default)]
    pub last: bool,
    /// Negative points.
    #[serde(default)]
    pub negative: bool,
    /// Show the horizontal axis (where the values cross zero).
    #[serde(default)]
    pub show_axis: bool,
    /// Empty cells.
    #[serde(default)]
    pub empty: EmptyCells,
    /// Show data in hidden rows and columns.
    #[serde(default)]
    pub show_hidden: bool,
    /// Vertical axis minimum.
    #[serde(default)]
    pub min: SparkAxis,
    /// Vertical axis maximum.
    #[serde(default)]
    pub max: SparkAxis,
    /// Plot data right to left.
    #[serde(default)]
    pub right_to_left: bool,
}

fn line_weight() -> F64 {
    F64(0.75)
}

impl SparklineGroup {
    /// A group with Excel's defaults.
    pub fn new(kind: SparklineKind, sparklines: Vec<Sparkline>) -> SparklineGroup {
        SparklineGroup {
            kind,
            sparklines,
            colors: SparkColors::default(),
            line_weight: line_weight(),
            markers: false,
            high: false,
            low: false,
            first: false,
            last: false,
            negative: false,
            show_axis: false,
            empty: EmptyCells::Gap,
            show_hidden: false,
            min: SparkAxis::Individual,
            max: SparkAxis::Individual,
            right_to_left: false,
        }
    }

    /// The sparkline in a cell.
    pub fn at(&self, row: u32, col: u32) -> Option<&Sparkline> {
        self.sparklines
            .iter()
            .find(|s| s.at.row == row && s.at.col == col)
    }
}

/// The values of a sparkline (`None` for an empty or non-numeric cell), in data order.
pub fn values(wb: &Workbook, sheet: SheetId, s: &Sparkline, show_hidden: bool) -> Vec<Option<f64>> {
    let sid = match &s.sheet {
        Some(name) => match wb.sheet_by_name(name) {
            Some(x) => x.id(),
            None => return Vec::new(),
        },
        None => sheet,
    };
    let Some(sh) = wb.sheet(sid) else {
        return Vec::new();
    };
    let r = s.data;
    let mut out = Vec::new();
    // A sparkline reads its range along its longer side.
    let cells: Vec<(u32, u32)> = if r.height() >= r.width() {
        (r.r0..=r.r1.min(r.r0 + 4_095))
            .map(|row| (row, r.c0))
            .collect()
    } else {
        (r.c0..=r.c1.min(r.c0 + 4_095))
            .map(|col| (r.r0, col))
            .collect()
    };
    for (row, col) in cells {
        if !show_hidden && (sh.is_row_hidden(row) || sh.is_col_hidden(col)) {
            continue;
        }
        out.push(match wb.value(sid, row, col) {
            CellValue::Number(n) => Some(n),
            _ => None,
        });
    }
    out
}

/// A point or column of a sparkline, in the unit square of its plot area (x to the right, y
/// down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SparkMark {
    /// The value's index.
    pub index: usize,
    /// Centre (line) or left edge (columns), 0…1.
    pub x: f32,
    /// The point's height, or the column's top, 0…1.
    pub y0: f32,
    /// The column's bottom (the axis); equal to `y0` for points.
    pub y1: f32,
    /// Column width, 0…1.
    pub w: f32,
    /// The colour that marks it (negative, high, low, first, last, marker), if any.
    pub mark: Option<Color>,
}

/// A sparkline laid out: its line segments (runs of points) or columns, and the axis.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SparkShape {
    /// Line: runs of connected points (a gap splits them).
    pub runs: Vec<Vec<(f32, f32)>>,
    /// Columns and marked points.
    pub marks: Vec<SparkMark>,
    /// The horizontal axis height (0…1), when shown and within the range.
    pub axis: Option<f32>,
}

/// The vertical range the group's settings give one sparkline (`group` is the range over every
/// sparkline of the group).
pub fn axis_range(g: &SparklineGroup, own: (f64, f64), group: (f64, f64)) -> (f64, f64) {
    let lo = match g.min {
        SparkAxis::Individual => own.0,
        SparkAxis::Group => group.0,
        SparkAxis::Custom(v) => v.0,
    };
    let hi = match g.max {
        SparkAxis::Individual => own.1,
        SparkAxis::Group => group.1,
        SparkAxis::Custom(v) => v.0,
    };
    (lo, hi)
}

/// The minimum and maximum of some values.
pub fn min_max(vals: &[Option<f64>]) -> Option<(f64, f64)> {
    let mut it = vals.iter().flatten();
    let first = *it.next()?;
    Some(it.fold((first, first), |(a, b), &v| (a.min(v), b.max(v))))
}

/// Lays out a sparkline of a group over `vals` with the vertical range `range`.
pub fn shape(g: &SparklineGroup, vals: &[Option<f64>], range: (f64, f64)) -> SparkShape {
    let mut out = SparkShape::default();
    let n = vals.len();
    if n == 0 {
        return out;
    }
    let vals: Vec<Option<f64>> = vals
        .iter()
        .map(|v| match (v, g.empty) {
            (None, EmptyCells::Zero) => Some(0.0),
            (v, _) => *v,
        })
        .collect();
    let order = |i: usize| if g.right_to_left { n - 1 - i } else { i };
    let (lo, hi) = if g.kind == SparklineKind::WinLoss {
        (-1.0, 1.0)
    } else {
        range
    };
    let span = hi - lo;
    let y_of = |v: f64| -> f32 {
        if span.abs() < f64::EPSILON {
            0.5
        } else {
            (1.0 - ((v - lo) / span).clamp(0.0, 1.0)) as f32
        }
    };
    let zero = if lo <= 0.0 && hi >= 0.0 {
        Some(y_of(0.0))
    } else {
        None
    };
    if g.show_axis || g.kind == SparklineKind::WinLoss {
        out.axis = zero;
    }
    let numbers: Vec<(usize, f64)> = vals
        .iter()
        .enumerate()
        .filter_map(|(i, v)| v.map(|x| (i, x)))
        .collect();
    let high = numbers
        .iter()
        .map(|x| x.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let low = numbers.iter().map(|x| x.1).fold(f64::INFINITY, f64::min);
    let first = numbers.first().map(|x| x.0);
    let last = numbers.last().map(|x| x.0);
    let c = &g.colors;
    let mark_of = |i: usize, v: f64, line: bool| -> Option<Color> {
        if g.high && v == high {
            return Some(c.high);
        }
        if g.low && v == low {
            return Some(c.low);
        }
        if g.first && Some(i) == first {
            return Some(c.first);
        }
        if g.last && Some(i) == last {
            return Some(c.last);
        }
        if g.negative && v < 0.0 {
            return Some(c.negative);
        }
        if line && g.markers {
            return Some(c.markers);
        }
        None
    };
    match g.kind {
        SparklineKind::Line => {
            let x_of = |i: usize| -> f32 {
                if n == 1 {
                    0.5
                } else {
                    order(i) as f32 / (n - 1) as f32
                }
            };
            let mut run: Vec<(f32, f32)> = Vec::new();
            for (i, v) in vals.iter().enumerate() {
                match v {
                    Some(x) => {
                        let p = (x_of(i), y_of(*x));
                        run.push(p);
                        if let Some(m) = mark_of(i, *x, true) {
                            out.marks.push(SparkMark {
                                index: i,
                                x: p.0,
                                y0: p.1,
                                y1: p.1,
                                w: 0.0,
                                mark: Some(m),
                            });
                        }
                    }
                    None if g.empty == EmptyCells::Span => {}
                    None => {
                        if !run.is_empty() {
                            out.runs.push(std::mem::take(&mut run));
                        }
                    }
                }
            }
            if !run.is_empty() {
                out.runs.push(run);
            }
            if g.right_to_left {
                for r in &mut out.runs {
                    r.reverse();
                }
            }
        }
        SparklineKind::Column | SparklineKind::WinLoss => {
            let slot = 1.0 / n as f32;
            let w = slot * 0.8;
            let base = zero.unwrap_or(if lo > 0.0 { 1.0 } else { 0.0 });
            for (i, v) in vals.iter().enumerate() {
                let Some(x) = v else { continue };
                let x = *x;
                let left = order(i) as f32 * slot + (slot - w) / 2.0;
                let top = if g.kind == SparklineKind::WinLoss {
                    if x > 0.0 {
                        0.0
                    } else if x < 0.0 {
                        1.0
                    } else {
                        continue;
                    }
                } else {
                    y_of(x)
                };
                let (y0, y1) = if top <= base {
                    (top, base)
                } else {
                    (base, top)
                };
                // A column always shows at least a sliver.
                let (y0, y1) = if (y1 - y0) < 0.02 {
                    (y0.min(base) - 0.01, y1.max(base) + 0.01)
                } else {
                    (y0, y1)
                };
                out.marks.push(SparkMark {
                    index: i,
                    x: left,
                    y0: y0.clamp(0.0, 1.0),
                    y1: y1.clamp(0.0, 1.0),
                    w,
                    mark: mark_of(i, x, false),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(kind: SparklineKind) -> SparklineGroup {
        SparklineGroup::new(
            kind,
            vec![Sparkline {
                at: CellPos::new(0, 5),
                sheet: None,
                data: Range::new(0, 0, 0, 4),
            }],
        )
    }

    #[test]
    fn a_line_splits_at_gaps_and_marks_high_and_low() {
        let mut g = group(SparklineKind::Line);
        g.high = true;
        g.low = true;
        let vals = [Some(1.0), Some(4.0), None, Some(2.0), Some(0.0)];
        let s = shape(&g, &vals, (0.0, 4.0));
        assert_eq!(s.runs.len(), 2);
        assert_eq!(s.runs[0], vec![(0.0, 0.75), (0.25, 0.0)]);
        assert_eq!(s.marks.len(), 2);
        assert_eq!(s.marks[0].index, 1);
        assert_eq!(s.marks[0].mark, Some(g.colors.high));
        assert_eq!(s.marks[1].index, 4);
        g.empty = EmptyCells::Span;
        assert_eq!(shape(&g, &vals, (0.0, 4.0)).runs.len(), 1);
        g.empty = EmptyCells::Zero;
        assert_eq!(shape(&g, &vals, (0.0, 4.0)).runs[0].len(), 5);
    }

    #[test]
    fn columns_stand_on_the_axis_and_win_loss_is_all_or_nothing() {
        let mut g = group(SparklineKind::Column);
        g.negative = true;
        g.show_axis = true;
        let vals = [Some(2.0), Some(-1.0), Some(1.0)];
        let s = shape(&g, &vals, (-1.0, 2.0));
        let axis = s.axis.expect("axis");
        assert!((axis - 2.0 / 3.0).abs() < 1e-6);
        assert_eq!(s.marks.len(), 3);
        assert!((s.marks[0].y0 - 0.0).abs() < 1e-6 && (s.marks[0].y1 - axis).abs() < 1e-6);
        assert!((s.marks[1].y0 - axis).abs() < 1e-6 && (s.marks[1].y1 - 1.0).abs() < 1e-6);
        assert_eq!(s.marks[1].mark, Some(g.colors.negative));
        let w = shape(
            &group(SparklineKind::WinLoss),
            &[Some(5.0), Some(-0.1), Some(0.0)],
            (0.0, 0.0),
        );
        assert_eq!(w.marks.len(), 2);
        assert!((w.marks[0].y0 - 0.0).abs() < 1e-6 && (w.marks[0].y1 - 0.5).abs() < 1e-6);
        assert!((w.marks[1].y1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn axis_settings() {
        let mut g = group(SparklineKind::Line);
        assert_eq!(axis_range(&g, (1.0, 3.0), (0.0, 9.0)), (1.0, 3.0));
        g.min = SparkAxis::Group;
        g.max = SparkAxis::Custom(F64(10.0));
        assert_eq!(axis_range(&g, (1.0, 3.0), (0.0, 9.0)), (0.0, 10.0));
        assert_eq!(min_max(&[None, Some(3.0), Some(-2.0)]), Some((-2.0, 3.0)));
        assert_eq!(min_max(&[None]), None);
    }
}
