// SPDX-License-Identifier: AGPL-3.0-or-later
//! Chart editing: new charts from a range (with Recommended Charts), change type, switch
//! row/column, select data, chart elements, quick layouts, styles and colours, series and axis
//! formatting — everything `SuiteCommand::Chart*` asks for.
//!
//! [`apply`] runs one command on a chart. Hosts keep the chart before the call for undo (a
//! [`ChartSpace`] is a plain value) and give a [`ChartData`] when the chart reads cells.

use kiln_common::{KilnError, Result};
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::drawingml::line::Line;
use kiln_protocol::commands::{
    ChartAxisTarget, ChartBound, ChartElementKind, ChartKind, ChartLabelPosition,
    ChartLegendPosition, SuiteCommand,
};
use kiln_sheet::Range;

use crate::data::{
    ChartData, RangeShape, SeriesIn, analyse_range, data_range, parse_reference, refresh,
    series_from_shape,
};
use crate::model::*;

/// Axis ids Kiln gives new charts (Excel uses random ones; any distinct values work).
const CAT_AX: i64 = 500_000_001;
const VAL_AX: i64 = 500_000_002;
const CAT_AX2: i64 = 500_000_003;
const VAL_AX2: i64 = 500_000_004;

fn invalid(msg: impl Into<String>) -> KilnError {
    KilnError::InvalidInput(msg.into())
}

/// The kind a chart's first group corresponds to (what Change Chart Type shows as current).
#[must_use]
pub fn kind_of(cs: &ChartSpace) -> Option<ChartKind> {
    let groups = &cs.chart.plot_area.groups;
    let g = groups.first()?;
    if groups.len() >= 2
        && matches!(g.kind, GroupKind::Bar { .. })
        && groups
            .iter()
            .any(|x| matches!(x.kind, GroupKind::Line { .. }))
    {
        let secondary = groups.iter().any(|x| x.series.iter().any(|s| s.secondary));
        return Some(if secondary {
            ChartKind::ComboColumnLineSecondary
        } else {
            ChartKind::ComboColumnLine
        });
    }
    let grouping = g.kind.grouping();
    Some(match &g.kind {
        GroupKind::Bar { dir, .. } => match (dir, grouping) {
            (BarDir::Column, Some(Grouping::Stacked)) => ChartKind::ColumnStacked,
            (BarDir::Column, Some(Grouping::PercentStacked)) => ChartKind::ColumnPercent,
            (BarDir::Column, _) => ChartKind::ColumnClustered,
            (BarDir::Bar, Some(Grouping::Stacked)) => ChartKind::BarStacked,
            (BarDir::Bar, Some(Grouping::PercentStacked)) => ChartKind::BarPercent,
            (BarDir::Bar, _) => ChartKind::BarClustered,
        },
        GroupKind::Line { .. } => {
            let smooth = g.series.iter().any(|s| s.smooth == Some(true));
            let markers = g.series.iter().any(|s| {
                s.marker
                    .as_ref()
                    .is_none_or(|m| m.symbol != Some(MarkerSymbol::None))
            });
            match grouping {
                Some(Grouping::Stacked) => ChartKind::LineStacked,
                Some(Grouping::PercentStacked) => ChartKind::LinePercent,
                _ if smooth => ChartKind::LineSmooth,
                _ if markers => ChartKind::LineMarkers,
                _ => ChartKind::Line,
            }
        }
        GroupKind::Area { .. } => match grouping {
            Some(Grouping::Stacked) => ChartKind::AreaStacked,
            Some(Grouping::PercentStacked) => ChartKind::AreaPercent,
            _ => ChartKind::Area,
        },
        GroupKind::Pie { .. } => {
            if g.series.iter().any(|s| {
                s.explosion.unwrap_or(0) > 0
                    || s.points.iter().any(|p| p.explosion.unwrap_or(0) > 0)
            }) {
                ChartKind::PieExploded
            } else {
                ChartKind::Pie
            }
        }
        GroupKind::Doughnut { .. } => ChartKind::Doughnut,
        GroupKind::Scatter { style } => match style {
            Some(ScatterStyle::Smooth | ScatterStyle::SmoothMarker) => ChartKind::ScatterSmooth,
            _ if g.series.iter().all(|s| {
                matches!(
                    s.shape.line.as_ref().and_then(|l| l.fill.as_ref()),
                    Some(Fill::None)
                )
            }) =>
            {
                ChartKind::ScatterMarkers
            }
            _ => ChartKind::ScatterLines,
        },
        GroupKind::Bubble { .. } => ChartKind::Bubble,
        GroupKind::Radar { style } => match style {
            Some(RadarStyle::Filled) => ChartKind::RadarFilled,
            Some(RadarStyle::Standard) => ChartKind::Radar,
            _ => ChartKind::RadarMarkers,
        },
        GroupKind::Hierarchy { layout, .. } => match layout {
            HierarchyLayout::Treemap => ChartKind::Treemap,
            HierarchyLayout::Sunburst => ChartKind::Sunburst,
        },
        GroupKind::Other(_) => return None,
    })
}

/// The Recommended Charts for a range's shape, best first (Insert ▸ Recommended Charts).
#[must_use]
pub fn recommend(data: &dyn ChartData, shape: &RangeShape) -> Vec<ChartKind> {
    let r = shape.range;
    let label_cols = label_columns(data, shape);
    let series_n = match shape.series_in {
        SeriesIn::Columns => r
            .width()
            .saturating_sub(label_cols.max(u32::from(shape.header_col))),
        SeriesIn::Rows => r.height().saturating_sub(u32::from(shape.header_row)),
    };
    let points = match shape.series_in {
        SeriesIn::Columns => r.height().saturating_sub(u32::from(shape.header_row)),
        SeriesIn::Rows => r.width().saturating_sub(u32::from(shape.header_col)),
    };
    let mut out: Vec<ChartKind> = Vec::new();
    let mut push = |k: ChartKind| {
        if !out.contains(&k) {
            out.push(k);
        }
    };
    if label_cols >= 2 {
        push(ChartKind::Treemap);
        push(ChartKind::Sunburst);
    }
    if shape.numeric_labels {
        if series_n == 2 && r.width() == 3 {
            push(ChartKind::Bubble);
        }
        push(ChartKind::ScatterMarkers);
        push(ChartKind::ScatterLines);
        push(ChartKind::Line);
    }
    if shape.date_labels || points > 12 {
        push(ChartKind::LineMarkers);
        push(ChartKind::Line);
        push(ChartKind::ColumnClustered);
        push(ChartKind::Area);
    }
    let magnitudes = series_magnitudes(data, shape);
    if magnitudes.len() >= 2 {
        let (lo, hi) = magnitudes
            .iter()
            .fold((f64::INFINITY, 0.0f64), |(a, b), m| (a.min(*m), b.max(*m)));
        if lo > 0.0 && hi / lo >= 10.0 {
            push(ChartKind::ComboColumnLineSecondary);
        }
    }
    if series_n == 1 && (2..=7).contains(&points) && all_positive(data, shape) {
        push(ChartKind::ColumnClustered);
        push(ChartKind::Pie);
        push(ChartKind::Doughnut);
        push(ChartKind::BarClustered);
    }
    let long_labels = average_label_len(data, shape) > 12.0;
    if long_labels {
        push(ChartKind::BarClustered);
    }
    push(ChartKind::ColumnClustered);
    push(ChartKind::BarClustered);
    if series_n >= 2 {
        push(ChartKind::ColumnStacked);
    }
    push(ChartKind::LineMarkers);
    if series_n >= 2 && points >= 3 {
        push(ChartKind::RadarMarkers);
    }
    out
}

fn label_columns(data: &dyn ChartData, shape: &RangeShape) -> u32 {
    let r = shape.range;
    let rows = (r.r0 + u32::from(shape.header_row))..=r.r1;
    let mut n = 0;
    for col in r.c0..r.c1 {
        let all_text = rows.clone().all(|row| {
            matches!(
                data.cell(&shape.sheet, row, col),
                Some(crate::data::ChartCell::Text(_) | crate::data::ChartCell::Blank)
            )
        });
        let any_text = rows.clone().any(|row| {
            matches!(
                data.cell(&shape.sheet, row, col),
                Some(crate::data::ChartCell::Text(_))
            )
        });
        if all_text && any_text {
            n += 1;
        } else {
            break;
        }
    }
    n
}

fn numbers_of(data: &dyn ChartData, shape: &RangeShape) -> Vec<Vec<f64>> {
    let r = shape.range;
    let mut out = Vec::new();
    let (h, c) = (u32::from(shape.header_row), u32::from(shape.header_col));
    match shape.series_in {
        SeriesIn::Columns => {
            for col in (r.c0 + c)..=r.c1 {
                out.push(
                    ((r.r0 + h)..=r.r1)
                        .filter_map(|row| match data.cell(&shape.sheet, row, col) {
                            Some(crate::data::ChartCell::Number(v, _)) => Some(v),
                            _ => None,
                        })
                        .collect(),
                );
            }
        }
        SeriesIn::Rows => {
            for row in (r.r0 + h)..=r.r1 {
                out.push(
                    ((r.c0 + c)..=r.c1)
                        .filter_map(|col| match data.cell(&shape.sheet, row, col) {
                            Some(crate::data::ChartCell::Number(v, _)) => Some(v),
                            _ => None,
                        })
                        .collect(),
                );
            }
        }
    }
    out
}

fn series_magnitudes(data: &dyn ChartData, shape: &RangeShape) -> Vec<f64> {
    numbers_of(data, shape)
        .into_iter()
        .filter(|v| !v.is_empty())
        .map(|v| v.iter().map(|x| x.abs()).fold(0.0, f64::max))
        .collect()
}

fn all_positive(data: &dyn ChartData, shape: &RangeShape) -> bool {
    numbers_of(data, shape).iter().flatten().all(|v| *v >= 0.0)
}

fn average_label_len(data: &dyn ChartData, shape: &RangeShape) -> f64 {
    if !shape.header_col {
        return 0.0;
    }
    let r = shape.range;
    let labels: Vec<usize> = ((r.r0 + u32::from(shape.header_row))..=r.r1)
        .filter_map(|row| match data.cell(&shape.sheet, row, r.c0) {
            Some(crate::data::ChartCell::Text(t)) => Some(t.chars().count()),
            _ => None,
        })
        .collect();
    if labels.is_empty() {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    {
        labels.iter().sum::<usize>() as f64 / labels.len() as f64
    }
}

fn group(kind: GroupKind, series: Vec<Series>, axes: Vec<i64>) -> ChartGroup {
    let vary = matches!(
        kind,
        GroupKind::Pie { .. } | GroupKind::Doughnut { .. } | GroupKind::Hierarchy { .. }
    );
    ChartGroup {
        kind,
        vary_colors: Some(vary),
        series,
        data_labels: None,
        axis_ids: axes,
        extras: Extras::default(),
    }
}

fn gridlines() -> ShapeProps {
    ShapeProps::default()
}

fn value_axis(id: i64, cross: i64, pos: AxisPos) -> Axis {
    let mut a = Axis::new(AxisKind::Value, id, pos, cross);
    a.major_gridlines = Some(gridlines());
    // Office's modern charts have no value axis line.
    a.shape.set_line(Some(Line {
        fill: Some(Fill::None),
        ..Line::default()
    }));
    a
}

fn category_axis(id: i64, cross: i64, pos: AxisPos) -> Axis {
    Axis::new(AxisKind::Category, id, pos, cross)
}

/// Builds the plot area of `kind` around `series` (their references and caches stay), keeping
/// series formatting that the new type can show.
/// The groups and axes of a `kind` chart over `series`, with every kept unknown element the new
/// types do not allow, or now model themselves, dropped (so a radar series' stray `c:smooth`
/// is not written next to the line chart's own).
fn plot_for(kind: ChartKind, series: Vec<Series>) -> (Vec<ChartGroup>, Vec<Axis>) {
    let (mut groups, axes) = plot_groups(kind, series);
    for g in &mut groups {
        let allowed = crate::read::series_schema(&g.kind);
        let chartex = matches!(g.kind, GroupKind::Hierarchy { .. });
        let modelled = if chartex {
            crate::chartex::SERIES_KNOWN.to_vec()
        } else {
            crate::read::series_known(&g.kind)
        };
        let point_modelled = crate::read::point_known(&g.kind);
        for s in &mut g.series {
            s.extras
                .retain_named(|n| allowed.contains(&n) && !modelled.contains(&n));
            if !chartex {
                s.attrs.clear();
            }
            for p in &mut s.points {
                p.extras
                    .retain_named(|n| n.starts_with("c:") && !point_modelled.contains(&n));
            }
        }
    }
    (groups, axes)
}

fn plot_groups(kind: ChartKind, mut series: Vec<Series>) -> (Vec<ChartGroup>, Vec<Axis>) {
    for (i, s) in series.iter_mut().enumerate() {
        let i = u32::try_from(i).unwrap_or(u32::MAX);
        s.idx = i;
        s.order = i;
        s.secondary = false;
    }
    let col_axes = || {
        (
            vec![CAT_AX, VAL_AX],
            vec![
                category_axis(CAT_AX, VAL_AX, AxisPos::Bottom),
                value_axis(VAL_AX, CAT_AX, AxisPos::Left),
            ],
        )
    };
    let bar_axes = || {
        (
            vec![CAT_AX, VAL_AX],
            vec![
                category_axis(CAT_AX, VAL_AX, AxisPos::Left),
                value_axis(VAL_AX, CAT_AX, AxisPos::Bottom),
            ],
        )
    };
    let bar = |dir: BarDir, grouping: Grouping, series: Vec<Series>| {
        let (ids, axes) = if dir == BarDir::Bar {
            bar_axes()
        } else {
            col_axes()
        };
        (
            vec![group(
                GroupKind::Bar {
                    dir,
                    grouping: Some(grouping),
                    gap_width: Some(if grouping == Grouping::Clustered {
                        219
                    } else {
                        150
                    }),
                    overlap: Some(if grouping == Grouping::Clustered {
                        -27
                    } else {
                        100
                    }),
                    three_d: false,
                },
                series,
                ids,
            )],
            axes,
        )
    };
    let markers = |series: &mut Vec<Series>, on: bool| {
        for s in series.iter_mut() {
            s.marker = Some(Marker {
                symbol: Some(if on {
                    MarkerSymbol::Circle
                } else {
                    MarkerSymbol::None
                }),
                size: on.then_some(5),
                ..Marker::default()
            });
        }
    };
    let line = |grouping: Grouping, mut series: Vec<Series>, with_markers: bool, smooth: bool| {
        markers(&mut series, with_markers);
        for s in &mut series {
            s.smooth = Some(smooth);
        }
        let (ids, axes) = col_axes();
        (
            vec![group(
                GroupKind::Line {
                    grouping: Some(grouping),
                    markers: Some(true),
                    three_d: false,
                },
                series,
                ids,
            )],
            axes,
        )
    };
    let area = |grouping: Grouping, series: Vec<Series>| {
        let (ids, mut axes) = col_axes();
        axes[1].cross_between = Some(false);
        (
            vec![group(
                GroupKind::Area {
                    grouping: Some(grouping),
                    three_d: false,
                },
                series,
                ids,
            )],
            axes,
        )
    };
    let xy_axes = || {
        let mut x = value_axis(CAT_AX, VAL_AX, AxisPos::Bottom);
        x.major_gridlines = None;
        x.cross_between = Some(false);
        let mut y = value_axis(VAL_AX, CAT_AX, AxisPos::Left);
        y.cross_between = Some(false);
        (vec![CAT_AX, VAL_AX], vec![x, y])
    };
    let numeric_categories = |series: &mut Vec<Series>| {
        for s in series.iter_mut() {
            if let Some(DataSource::Str(st)) = &s.categories {
                let all_numeric = st
                    .points
                    .iter()
                    .flatten()
                    .all(|t| t.trim().parse::<f64>().is_ok());
                if all_numeric {
                    s.categories = Some(DataSource::Num(NumData {
                        formula: st.formula.clone(),
                        format_code: Some("General".into()),
                        points: st
                            .points
                            .iter()
                            .map(|p| p.as_deref().and_then(|t| t.trim().parse().ok()))
                            .collect(),
                        has_cache: st.has_cache,
                        extras: Extras::default(),
                    }));
                }
            }
        }
    };
    match kind {
        ChartKind::ColumnClustered => bar(BarDir::Column, Grouping::Clustered, series),
        ChartKind::ColumnStacked => bar(BarDir::Column, Grouping::Stacked, series),
        ChartKind::ColumnPercent => bar(BarDir::Column, Grouping::PercentStacked, series),
        ChartKind::BarClustered => bar(BarDir::Bar, Grouping::Clustered, series),
        ChartKind::BarStacked => bar(BarDir::Bar, Grouping::Stacked, series),
        ChartKind::BarPercent => bar(BarDir::Bar, Grouping::PercentStacked, series),
        ChartKind::Line => line(Grouping::Standard, series, false, false),
        ChartKind::LineMarkers => line(Grouping::Standard, series, true, false),
        ChartKind::LineStacked => line(Grouping::Stacked, series, false, false),
        ChartKind::LinePercent => line(Grouping::PercentStacked, series, false, false),
        ChartKind::LineSmooth => line(Grouping::Standard, series, true, true),
        ChartKind::Area => area(Grouping::Standard, series),
        ChartKind::AreaStacked => area(Grouping::Stacked, series),
        ChartKind::AreaPercent => area(Grouping::PercentStacked, series),
        ChartKind::Pie | ChartKind::PieExploded => {
            series.truncate(1);
            if kind == ChartKind::Pie {
                // A plain pie: pull every slice back in (Excel does the same).
                for s in &mut series {
                    s.explosion = None;
                    for p in &mut s.points {
                        p.explosion = None;
                    }
                }
            }
            if kind == ChartKind::PieExploded
                && let Some(s) = series.first_mut()
            {
                s.points.retain(|p| p.idx != 0);
                s.points.push(DataPoint {
                    idx: 0,
                    explosion: Some(12),
                    ..DataPoint::default()
                });
            }
            (
                vec![group(
                    GroupKind::Pie {
                        first_slice_angle: Some(0),
                        three_d: false,
                    },
                    series,
                    Vec::new(),
                )],
                Vec::new(),
            )
        }
        ChartKind::Doughnut => (
            vec![group(
                GroupKind::Doughnut {
                    first_slice_angle: Some(0),
                    hole_size: Some(55),
                },
                series,
                Vec::new(),
            )],
            Vec::new(),
        ),
        ChartKind::ScatterMarkers | ChartKind::ScatterLines | ChartKind::ScatterSmooth => {
            numeric_categories(&mut series);
            let smooth = kind == ChartKind::ScatterSmooth;
            for s in &mut series {
                s.smooth = Some(smooth);
                if kind == ChartKind::ScatterMarkers {
                    s.shape.set_line(Some(Line {
                        fill: Some(Fill::None),
                        ..Line::default()
                    }));
                } else if matches!(
                    s.shape.line.as_ref().and_then(|l| l.fill.as_ref()),
                    Some(Fill::None)
                ) {
                    s.shape.set_line(None);
                }
                s.marker = Some(Marker {
                    symbol: Some(if smooth {
                        MarkerSymbol::None
                    } else {
                        MarkerSymbol::Circle
                    }),
                    size: (!smooth).then_some(5),
                    ..Marker::default()
                });
            }
            let (ids, axes) = xy_axes();
            (
                vec![group(
                    GroupKind::Scatter {
                        style: Some(if smooth {
                            ScatterStyle::SmoothMarker
                        } else {
                            ScatterStyle::LineMarker
                        }),
                    },
                    series,
                    ids,
                )],
                axes,
            )
        }
        ChartKind::Bubble => {
            numeric_categories(&mut series);
            // Bubble sizes come from the series after each x/y pair when the user picked three
            // columns; otherwise every bubble has the same size.
            if series.len() >= 2 && series.iter().all(|s| s.bubble_sizes.is_none()) {
                let mut paired = Vec::new();
                let mut it = series.into_iter();
                while let Some(mut s) = it.next() {
                    if let Some(sizes) = it.next() {
                        s.bubble_sizes = sizes.values;
                    }
                    paired.push(s);
                }
                series = paired;
                for (i, s) in series.iter_mut().enumerate() {
                    s.idx = u32::try_from(i).unwrap_or(u32::MAX);
                    s.order = s.idx;
                }
            }
            for s in &mut series {
                s.bubble_3d = Some(false);
                s.invert_if_negative = Some(false);
            }
            let (ids, axes) = xy_axes();
            (
                vec![group(
                    GroupKind::Bubble {
                        bubble_3d: None,
                        scale: Some(100),
                        show_negative: Some(false),
                        size_is_width: None,
                    },
                    series,
                    ids,
                )],
                axes,
            )
        }
        ChartKind::Radar | ChartKind::RadarMarkers | ChartKind::RadarFilled => {
            let style = match kind {
                ChartKind::Radar => RadarStyle::Standard,
                ChartKind::RadarFilled => RadarStyle::Filled,
                _ => RadarStyle::Marker,
            };
            if style != RadarStyle::Filled {
                markers(&mut series, style == RadarStyle::Marker);
            }
            let (ids, mut axes) = col_axes();
            axes[1].shape = ShapeProps::default();
            axes[1].major_tick = Some(TickMark::Cross);
            (
                vec![group(GroupKind::Radar { style: Some(style) }, series, ids)],
                axes,
            )
        }
        ChartKind::ComboColumnLine | ChartKind::ComboColumnLineSecondary => {
            let secondary = kind == ChartKind::ComboColumnLineSecondary;
            let split = if series.len() > 1 {
                series.len() - 1
            } else {
                series.len()
            };
            let lines = series.split_off(split);
            let (_, mut axes) = col_axes();
            let mut groups = vec![group(
                GroupKind::Bar {
                    dir: BarDir::Column,
                    grouping: Some(Grouping::Clustered),
                    gap_width: Some(219),
                    overlap: Some(-27),
                    three_d: false,
                },
                series,
                vec![CAT_AX, VAL_AX],
            )];
            let mut lines = lines;
            markers(&mut lines, true);
            let line_axes = if secondary {
                let mut cat2 = category_axis(CAT_AX2, VAL_AX2, AxisPos::Bottom);
                cat2.delete = Some(true);
                let mut val2 = value_axis(VAL_AX2, CAT_AX2, AxisPos::Right);
                val2.major_gridlines = None;
                val2.crosses = Some(Crosses::Max);
                axes.push(cat2);
                axes.push(val2);
                for s in &mut lines {
                    s.secondary = true;
                }
                vec![CAT_AX2, VAL_AX2]
            } else {
                vec![CAT_AX, VAL_AX]
            };
            groups.push(group(
                GroupKind::Line {
                    grouping: Some(Grouping::Standard),
                    markers: Some(true),
                    three_d: false,
                },
                lines,
                line_axes,
            ));
            (groups, axes)
        }
        ChartKind::Treemap | ChartKind::Sunburst => {
            series.truncate(1);
            let layout = if kind == ChartKind::Treemap {
                HierarchyLayout::Treemap
            } else {
                HierarchyLayout::Sunburst
            };
            for s in &mut series {
                s.data_labels = Some(DataLabels {
                    position: Some(if kind == ChartKind::Treemap {
                        LabelPosition::InsideEnd
                    } else {
                        LabelPosition::Center
                    }),
                    show_category: Some(true),
                    show_value: Some(false),
                    show_series: Some(false),
                    ..DataLabels::default()
                });
                if let Some(DataSource::Str(st)) = &s.categories {
                    s.categories = Some(DataSource::MultiLevel(MultiLevelData {
                        formula: st.formula.clone().unwrap_or_default(),
                        levels: vec![st.points.clone()],
                        has_cache: true,
                        extras: Extras::default(),
                    }));
                }
            }
            (
                vec![group(
                    GroupKind::Hierarchy {
                        layout,
                        parent_label_layout: (kind == ChartKind::Treemap)
                            .then(|| "overlapping".to_owned()),
                    },
                    series,
                    Vec::new(),
                )],
                Vec::new(),
            )
        }
    }
}

/// A new chart of `kind` over `series`, with Office 2013+ defaults: automatic title for a single
/// series, legend at the bottom for several, major gridlines, no value axis line.
#[must_use]
pub fn new_chart(kind: ChartKind, series: Vec<Series>) -> ChartSpace {
    let hierarchy = matches!(kind, ChartKind::Treemap | ChartKind::Sunburst);
    let (groups, axes) = plot_for(kind, series);
    let n: usize = groups.iter().map(|g| g.series.len()).sum();
    let mut cs = ChartSpace {
        format: if hierarchy {
            PartFormat::ChartEx
        } else {
            PartFormat::Classic
        },
        date1904: (!hierarchy).then_some(false),
        lang: (!hierarchy).then(|| "en-US".to_owned()),
        rounded_corners: (!hierarchy).then_some(false),
        chart: Chart {
            title: Some(Title {
                overlay: Some(false),
                ..Title::default()
            }),
            auto_title_deleted: Some(false),
            plot_area: PlotArea {
                layout: None,
                groups,
                axes,
                shape: ShapeProps::default(),
                extras: Extras::default(),
            },
            legend: (n > 1
                || matches!(
                    kind,
                    ChartKind::Pie | ChartKind::PieExploded | ChartKind::Doughnut
                ))
            .then(|| Legend {
                position: Some(LegendPos::Bottom),
                overlay: Some(false),
                ..Legend::default()
            }),
            plot_vis_only: (!hierarchy).then_some(true),
            disp_blanks_as: (!hierarchy).then_some(BlanksAs::Gap),
            extras: Extras::default(),
        },
        ..ChartSpace::default()
    };
    if hierarchy {
        cs.chart.legend = Some(Legend {
            position: Some(LegendPos::Top),
            overlay: Some(false),
            ..Legend::default()
        });
        cs.chart.title = Some(Title {
            overlay: Some(false),
            attrs: vec![("pos".into(), "t".into()), ("align".into(), "ctr".into())],
            ..Title::default()
        });
        cs.root_attrs = Vec::new();
        cs.chart.auto_title_deleted = None;
    }
    cs
}

/// Builds a chart of `kind` (or the first recommendation) from `range` on `sheet` and fills its
/// caches. Treemaps and sunbursts take every leading text column as a hierarchy level.
///
/// # Errors
///
/// `InvalidInput` when the range has no numbers to plot or the sheet does not exist.
pub fn insert_from_range(
    data: &dyn ChartData,
    sheet: &str,
    range: Range,
    kind: Option<ChartKind>,
) -> Result<ChartSpace> {
    let shape = analyse_range(data, sheet, range)
        .ok_or_else(|| invalid(format!("no data at {sheet}!{}", range.to_a1(false))))?;
    let kind = kind.unwrap_or_else(|| {
        recommend(data, &shape)
            .first()
            .copied()
            .unwrap_or(ChartKind::ColumnClustered)
    });
    let series = series_for(data, &shape, kind);
    if series.is_empty() {
        return Err(invalid("the selection has no values to chart"));
    }
    let mut cs = new_chart(kind, series);
    refresh(&mut cs, data);
    Ok(cs)
}

fn literal_numbers(values: &[f64]) -> DataSource {
    DataSource::Num(NumData {
        formula: None,
        format_code: Some("General".into()),
        points: values.iter().copied().map(Some).collect(),
        has_cache: true,
        extras: Extras::default(),
    })
}

fn literal_labels(labels: &[&str]) -> DataSource {
    DataSource::Str(StrData {
        formula: None,
        points: labels.iter().map(|l| Some((*l).to_owned())).collect(),
        has_cache: true,
        extras: Extras::default(),
    })
}

/// The sample series a new chart of `kind` gets where there are no cells to chart (Insert ▸
/// Chart in Write and Deck): the values Office uses, held as literal data (`c:numLit`,
/// `c:strLit`) in the chart itself.
#[must_use]
pub fn sample_series(kind: ChartKind) -> Vec<Series> {
    let named = |name: &str, categories: DataSource, values: &[f64]| Series {
        name: Some(SeriesText::Literal(name.to_owned())),
        categories: Some(categories),
        values: Some(literal_numbers(values)),
        ..Series::default()
    };
    match kind {
        ChartKind::Pie | ChartKind::PieExploded | ChartKind::Doughnut => vec![named(
            "Sales",
            literal_labels(&["1st Qtr", "2nd Qtr", "3rd Qtr", "4th Qtr"]),
            &[8.2, 3.2, 1.4, 1.2],
        )],
        ChartKind::ScatterMarkers | ChartKind::ScatterLines | ChartKind::ScatterSmooth => {
            vec![named(
                "Y-Values",
                literal_numbers(&[0.7, 1.8, 2.6]),
                &[2.7, 3.2, 0.8],
            )]
        }
        ChartKind::Bubble => vec![Series {
            bubble_sizes: Some(literal_numbers(&[10.0, 4.0, 8.0])),
            ..named(
                "Y-Values",
                literal_numbers(&[0.7, 1.8, 2.6]),
                &[2.7, 3.2, 0.8],
            )
        }],
        ChartKind::Treemap | ChartKind::Sunburst => {
            let leaves = [
                "Leaf 1", "Leaf 2", "Leaf 3", "Leaf 4", "Leaf 5", "Leaf 6", "Leaf 7",
            ];
            let branches = [
                Some("Branch 1"),
                None,
                None,
                Some("Branch 2"),
                None,
                Some("Branch 3"),
                None,
            ];
            vec![Series {
                categories: Some(DataSource::MultiLevel(MultiLevelData {
                    formula: String::new(),
                    levels: vec![
                        leaves.iter().map(|l| Some((*l).to_owned())).collect(),
                        branches.iter().map(|b| b.map(str::to_owned)).collect(),
                    ],
                    has_cache: true,
                    extras: Extras::default(),
                })),
                ..named(
                    "Sales",
                    literal_labels(&[]),
                    &[22.0, 12.0, 18.0, 87.0, 88.0, 17.0, 14.0],
                )
            }]
        }
        _ => {
            let categories =
                literal_labels(&["Category 1", "Category 2", "Category 3", "Category 4"]);
            vec![
                named("Series 1", categories.clone(), &[4.3, 2.5, 3.5, 4.5]),
                named("Series 2", categories.clone(), &[2.4, 4.4, 1.8, 2.8]),
                named("Series 3", categories, &[2.0, 2.0, 3.0, 5.0]),
            ]
        }
    }
}

/// A new chart of `kind` (clustered column when `None`, as in Office) over [`sample_series`].
#[must_use]
pub fn sample_chart(kind: Option<ChartKind>) -> ChartSpace {
    let kind = kind.unwrap_or(ChartKind::ColumnClustered);
    new_chart(kind, sample_series(kind))
}

fn series_for(data: &dyn ChartData, shape: &RangeShape, kind: ChartKind) -> Vec<Series> {
    let xy = matches!(
        kind,
        ChartKind::ScatterMarkers
            | ChartKind::ScatterLines
            | ChartKind::ScatterSmooth
            | ChartKind::Bubble
    );
    if matches!(kind, ChartKind::Treemap | ChartKind::Sunburst) {
        let levels = label_columns(data, shape).max(1);
        let r = shape.range;
        let d0 = r.r0 + u32::from(shape.header_row);
        if d0 > r.r1 || r.c0 + levels > r.c1 {
            return series_from_shape(shape, false);
        }
        let area = |r0, c0, r1, c1| {
            crate::data::Area {
                sheet: shape.sheet.clone(),
                range: Range::new(r0, c0, r1, c1),
            }
            .to_formula()
        };
        let value_col = r.c0 + levels;
        return vec![Series {
            name: shape.header_row.then(|| SeriesText::Ref {
                formula: area(r.r0, value_col, r.r0, value_col),
                cache: None,
            }),
            categories: Some(DataSource::MultiLevel(MultiLevelData {
                formula: area(d0, r.c0, r.r1, r.c0 + levels - 1),
                levels: Vec::new(),
                has_cache: true,
                extras: Extras::default(),
            })),
            values: Some(DataSource::Num(NumData {
                formula: Some(area(d0, value_col, r.r1, value_col)),
                has_cache: true,
                ..NumData::default()
            })),
            ..Series::default()
        }];
    }
    series_from_shape(shape, xy)
}

fn all_series(cs: &ChartSpace) -> Vec<Series> {
    let mut v: Vec<Series> = cs
        .chart
        .plot_area
        .groups
        .iter()
        .flat_map(|g| g.series.iter().cloned())
        .collect();
    v.sort_by_key(|s| s.order);
    v
}

/// Changes the chart type, keeping the data, titles, legend and series formatting.
pub fn change_type(cs: &mut ChartSpace, kind: ChartKind) {
    let mut series = all_series(cs);
    let hierarchy = matches!(kind, ChartKind::Treemap | ChartKind::Sunburst);
    if !hierarchy {
        // Leaving a hierarchy: the innermost level becomes the categories.
        for s in &mut series {
            if let Some(DataSource::MultiLevel(m)) = &s.categories
                && cs.format == PartFormat::ChartEx
            {
                s.categories = Some(DataSource::Str(StrData {
                    formula: (!m.formula.is_empty()).then(|| m.formula.clone()),
                    points: m.levels.first().cloned().unwrap_or_default(),
                    has_cache: true,
                    extras: Extras::default(),
                }));
                s.data_labels = None;
            }
        }
    }
    let old_axes = cs.chart.plot_area.axes.clone();
    let (groups, mut axes) = plot_for(kind, series);
    // Keep axis titles and formats across types with the same axis roles.
    for a in &mut axes {
        if let Some(old) = old_axes.iter().find(|o| {
            o.kind == a.kind
                && o.position.map(AxisPos::is_horizontal) == a.position.map(AxisPos::is_horizontal)
        }) {
            a.title.clone_from(&old.title);
            a.num_fmt.clone_from(&old.num_fmt);
            a.text = old.text.clone();
        }
    }
    cs.chart.plot_area.groups = groups;
    cs.chart.plot_area.axes = axes;
    let was_ex = cs.format == PartFormat::ChartEx;
    cs.format = if hierarchy {
        PartFormat::ChartEx
    } else {
        PartFormat::Classic
    };
    if was_ex != hierarchy {
        // The part's vocabulary changed: root attributes and unknown root content do not carry.
        cs.root_attrs.clear();
        cs.extras = Extras::default();
        cs.chart.extras = Extras::default();
        cs.chart.plot_area.extras = Extras::default();
        if let Some(t) = &mut cs.chart.title {
            t.extras = Extras::default();
            t.attrs.clear();
            if let Some(TitleText::Rich { raw, .. }) = &mut t.text {
                *raw = None;
            }
        }
        if let Some(l) = &mut cs.chart.legend {
            l.extras = Extras::default();
            l.attrs.clear();
            l.entries.clear();
        }
        if hierarchy {
            // chartex has none of these.
            cs.date1904 = None;
            cs.lang = None;
            cs.rounded_corners = None;
            cs.style = None;
            cs.chart.auto_title_deleted = None;
            cs.chart.plot_vis_only = None;
            cs.chart.disp_blanks_as = None;
            cs.chart.plot_area.layout = None;
            if let Some(t) = &mut cs.chart.title {
                t.layout = None;
            }
            if let Some(l) = &mut cs.chart.legend {
                l.layout = None;
            }
        } else {
            cs.date1904 = Some(false);
            cs.rounded_corners = Some(false);
            cs.plot_vis_only_default();
        }
    }
}

impl ChartSpace {
    fn plot_vis_only_default(&mut self) {
        self.chart.plot_vis_only.get_or_insert(true);
        self.chart.disp_blanks_as.get_or_insert(BlanksAs::Gap);
    }
}

/// Swaps rows and columns of the chart's source range and refreshes.
///
/// # Errors
///
/// `InvalidInput` when the series do not come from one rectangle on one sheet.
pub fn switch_row_column(cs: &mut ChartSpace, data: &dyn ChartData) -> Result<()> {
    let area = data_range(cs).ok_or_else(|| {
        invalid("Switch Row/Column needs the chart's data in one range on one sheet")
    })?;
    let mut shape = analyse_range(data, &area.sheet, area.range)
        .ok_or_else(|| invalid("the chart's data range is empty"))?;
    // The current direction: series names in the top row means series run in columns.
    let current = cs
        .chart
        .plot_area
        .groups
        .iter()
        .flat_map(|g| g.series.iter())
        .find_map(|s| s.values.as_ref().and_then(DataSource::formula))
        .and_then(parse_reference)
        .and_then(|a| a.first().map(|a| a.range))
        .map_or(shape.series_in, |r| {
            if r.width() == 1 && r.height() > 1 {
                SeriesIn::Columns
            } else {
                SeriesIn::Rows
            }
        });
    shape.series_in = match current {
        SeriesIn::Columns => SeriesIn::Rows,
        SeriesIn::Rows => SeriesIn::Columns,
    };
    rebuild_series(cs, data, &shape);
    Ok(())
}

/// Points the chart at another range (Select Data).
///
/// # Errors
///
/// `InvalidInput` for a reference that is not one area on a sheet.
pub fn select_data(cs: &mut ChartSpace, data: &dyn ChartData, range: &str) -> Result<()> {
    let areas = parse_reference(range).ok_or_else(|| invalid(format!("not a range: {range}")))?;
    let [area] = areas.as_slice() else {
        return Err(invalid("Select Data takes one rectangular range"));
    };
    let shape = analyse_range(data, &area.sheet, area.range)
        .ok_or_else(|| invalid(format!("no sheet named {}", area.sheet)))?;
    rebuild_series(cs, data, &shape);
    Ok(())
}

fn rebuild_series(cs: &mut ChartSpace, data: &dyn ChartData, shape: &RangeShape) {
    let kind = kind_of(cs).unwrap_or(ChartKind::ColumnClustered);
    let old = all_series(cs);
    let mut series = series_for(data, shape, kind);
    // Series keep their formatting by position.
    for (new, old) in series.iter_mut().zip(&old) {
        new.shape = old.shape.clone();
        new.marker.clone_from(&old.marker);
        new.data_labels.clone_from(&old.data_labels);
        new.trendlines.clone_from(&old.trendlines);
        new.smooth = old.smooth;
    }
    let groups_before = cs.chart.plot_area.groups.clone();
    let axes = cs.chart.plot_area.axes.clone();
    change_type(cs, kind);
    // Restore group options (gap width, hole size, …) and axes the user formatted.
    let (groups, _) = plot_for(kind, series);
    cs.chart.plot_area.groups = groups;
    for (g, before) in cs.chart.plot_area.groups.iter_mut().zip(&groups_before) {
        if std::mem::discriminant(&g.kind) == std::mem::discriminant(&before.kind) {
            g.kind = before.kind.clone();
            g.data_labels.clone_from(&before.data_labels);
        }
    }
    if axes.len() == cs.chart.plot_area.axes.len() {
        cs.chart.plot_area.axes = axes;
    }
    crate::read::mark_secondary(&mut cs.chart.plot_area);
    refresh(cs, data);
}

/// The id of the primary or secondary axis drawn horizontally or vertically.
pub(crate) fn axis_role_id(cs: &ChartSpace, horizontal: bool, secondary: bool) -> Option<i64> {
    let groups = &cs.chart.plot_area.groups;
    let pair: Vec<i64> = groups
        .iter()
        .filter(|g| !g.axis_ids.is_empty())
        .map(|g| g.axis_ids.clone())
        .nth(usize::from(secondary))?;
    // The first id is the category (or x) axis.
    let id = if horizontal != bar_swapped(cs) {
        pair.first()
    } else {
        pair.get(1)
    };
    id.copied()
}

/// Whether the chart's bars run horizontally (value axis at the bottom).
pub(crate) fn bar_swapped(cs: &ChartSpace) -> bool {
    cs.chart.plot_area.groups.iter().any(|g| {
        matches!(
            g.kind,
            GroupKind::Bar {
                dir: BarDir::Bar,
                ..
            }
        )
    })
}

fn axis_by_role(cs: &mut ChartSpace, horizontal: bool, secondary: bool) -> Option<&mut Axis> {
    let id = axis_role_id(cs, horizontal, secondary)?;
    cs.chart.plot_area.axes.iter_mut().find(|a| a.id == id)
}

/// Adds or removes a chart element.
pub fn set_element(cs: &mut ChartSpace, element: ChartElementKind, on: bool) {
    match element {
        ChartElementKind::ChartTitle => {
            if on {
                cs.chart.title.get_or_insert_with(|| Title {
                    overlay: Some(false),
                    ..Title::default()
                });
                cs.chart.auto_title_deleted = Some(false);
            } else {
                cs.chart.title = None;
                cs.chart.auto_title_deleted = Some(true);
            }
        }
        ChartElementKind::PrimaryHorizontalAxis | ChartElementKind::PrimaryVerticalAxis => {
            let h = element == ChartElementKind::PrimaryHorizontalAxis;
            if let Some(a) = axis_by_role(cs, h, false) {
                a.delete = Some(!on);
            }
        }
        ChartElementKind::PrimaryHorizontalAxisTitle
        | ChartElementKind::PrimaryVerticalAxisTitle => {
            let h = element == ChartElementKind::PrimaryHorizontalAxisTitle;
            if let Some(a) = axis_by_role(cs, h, false) {
                a.title = on.then(|| Title {
                    text: Some(TitleText::rich("Axis Title")),
                    overlay: Some(false),
                    ..Title::default()
                });
            }
        }
        ChartElementKind::MajorGridlines | ChartElementKind::MinorGridlines => {
            let value_h = cs.chart.plot_area.groups.iter().any(|g| {
                matches!(
                    g.kind,
                    GroupKind::Bar {
                        dir: BarDir::Bar,
                        ..
                    }
                )
            });
            if let Some(a) = axis_by_role(cs, value_h, false) {
                let slot = if element == ChartElementKind::MajorGridlines {
                    &mut a.major_gridlines
                } else {
                    &mut a.minor_gridlines
                };
                *slot = on.then(ShapeProps::default);
            }
        }
        ChartElementKind::CategoryGridlines => {
            let cat_h = !cs.chart.plot_area.groups.iter().any(|g| {
                matches!(
                    g.kind,
                    GroupKind::Bar {
                        dir: BarDir::Bar,
                        ..
                    }
                )
            });
            if let Some(a) = axis_by_role(cs, cat_h, false) {
                a.major_gridlines = on.then(ShapeProps::default);
            }
        }
        ChartElementKind::Legend => set_legend(cs, on.then_some(ChartLegendPosition::Right)),
        ChartElementKind::DataLabels => {
            set_data_labels(cs, on.then_some(ChartLabelPosition::OutsideEnd));
        }
        ChartElementKind::LinearTrendline
        | ChartElementKind::ExponentialTrendline
        | ChartElementKind::MovingAverage => {
            let kind = match element {
                ChartElementKind::LinearTrendline => TrendlineKind::Linear,
                ChartElementKind::ExponentialTrendline => TrendlineKind::Exponential,
                _ => TrendlineKind::MovingAverage,
            };
            for g in &mut cs.chart.plot_area.groups {
                let allowed = matches!(
                    g.kind,
                    GroupKind::Bar { .. }
                        | GroupKind::Line { .. }
                        | GroupKind::Area { .. }
                        | GroupKind::Scatter { .. }
                        | GroupKind::Bubble { .. }
                ) && !g.kind.grouping().is_some_and(Grouping::is_stacked);
                if !allowed {
                    continue;
                }
                for s in &mut g.series {
                    s.trendlines.retain(|t| t.kind != kind);
                    if on {
                        s.trendlines.push(Trendline {
                            name: None,
                            shape: ShapeProps::default(),
                            kind,
                            order: None,
                            period: (kind == TrendlineKind::MovingAverage).then_some(2),
                            forward: None,
                            backward: None,
                            intercept: None,
                            show_r_squared: Some(false),
                            show_equation: Some(false),
                            extras: Extras::default(),
                        });
                    }
                }
            }
        }
    }
}

/// Places or removes the legend.
pub fn set_legend(cs: &mut ChartSpace, position: Option<ChartLegendPosition>) {
    cs.chart.legend = position.map(|p| {
        let mut l = cs.chart.legend.take().unwrap_or_else(|| Legend {
            overlay: Some(false),
            ..Legend::default()
        });
        l.position = Some(match p {
            ChartLegendPosition::Right => LegendPos::Right,
            ChartLegendPosition::Top => LegendPos::Top,
            ChartLegendPosition::Left => LegendPos::Left,
            ChartLegendPosition::Bottom => LegendPos::Bottom,
            ChartLegendPosition::TopRight => LegendPos::TopRight,
        });
        l.layout = None;
        l
    });
}

/// Shows data labels on every series at `position`, or removes them.
pub fn set_data_labels(cs: &mut ChartSpace, position: Option<ChartLabelPosition>) {
    for g in &mut cs.chart.plot_area.groups {
        g.data_labels = None;
        let circular = matches!(g.kind, GroupKind::Pie { .. } | GroupKind::Doughnut { .. });
        let hierarchy = matches!(g.kind, GroupKind::Hierarchy { .. });
        let allowed = |p: LabelPosition| match &g.kind {
            GroupKind::Bar { grouping, .. } => {
                !(grouping.is_some_and(Grouping::is_stacked) && p == LabelPosition::OutsideEnd)
                    && matches!(
                        p,
                        LabelPosition::Center
                            | LabelPosition::InsideEnd
                            | LabelPosition::InsideBase
                            | LabelPosition::OutsideEnd
                    )
            }
            GroupKind::Pie { .. } => matches!(
                p,
                LabelPosition::Center
                    | LabelPosition::InsideEnd
                    | LabelPosition::OutsideEnd
                    | LabelPosition::BestFit
            ),
            GroupKind::Line { .. } | GroupKind::Scatter { .. } | GroupKind::Bubble { .. } => {
                matches!(
                    p,
                    LabelPosition::Center
                        | LabelPosition::Left
                        | LabelPosition::Right
                        | LabelPosition::Top
                        | LabelPosition::Bottom
                )
            }
            _ => false,
        };
        for s in &mut g.series {
            s.data_labels = position.map(|p| {
                let mut pos = match p {
                    ChartLabelPosition::Center => LabelPosition::Center,
                    ChartLabelPosition::InsideEnd => LabelPosition::InsideEnd,
                    ChartLabelPosition::InsideBase => LabelPosition::InsideBase,
                    ChartLabelPosition::OutsideEnd => LabelPosition::OutsideEnd,
                    ChartLabelPosition::BestFit => LabelPosition::BestFit,
                    ChartLabelPosition::Left => LabelPosition::Left,
                    ChartLabelPosition::Right => LabelPosition::Right,
                    ChartLabelPosition::Above => LabelPosition::Top,
                    ChartLabelPosition::Below => LabelPosition::Bottom,
                };
                if !allowed(pos) {
                    pos = match &g.kind {
                        GroupKind::Bar { grouping, .. }
                            if grouping.is_some_and(Grouping::is_stacked) =>
                        {
                            LabelPosition::Center
                        }
                        GroupKind::Bar { .. } => LabelPosition::OutsideEnd,
                        GroupKind::Pie { .. } => LabelPosition::BestFit,
                        GroupKind::Line { .. } | GroupKind::Scatter { .. } => LabelPosition::Right,
                        _ => LabelPosition::Center,
                    };
                }
                let explicit_position = !matches!(
                    g.kind,
                    GroupKind::Area { .. } | GroupKind::Radar { .. } | GroupKind::Doughnut { .. }
                );
                DataLabels {
                    position: (explicit_position || hierarchy).then_some(pos),
                    show_legend_key: Some(false),
                    show_value: Some(!circular && !hierarchy),
                    show_category: Some(hierarchy),
                    show_series: Some(false),
                    show_percent: Some(circular),
                    show_bubble_size: Some(false),
                    show_leader_lines: circular.then_some(true),
                    ..DataLabels::default()
                }
            });
        }
    }
}

/// Sets the chart title text (empty removes the title).
pub fn set_title(cs: &mut ChartSpace, text: &str) {
    if text.trim().is_empty() {
        set_element(cs, ChartElementKind::ChartTitle, false);
        return;
    }
    let t = cs.chart.title.get_or_insert_with(|| Title {
        overlay: Some(false),
        ..Title::default()
    });
    let run = match &t.text {
        Some(TitleText::Rich { run, .. }) => run.clone(),
        _ => RunProps::default(),
    };
    t.text = Some(TitleText::Rich {
        plain: text.to_owned(),
        run,
        raw: None,
    });
    cs.chart.auto_title_deleted = Some(false);
}

/// Applies Office's quick layout `n` (1–11): which of title, legend, axis titles, data labels and
/// gridlines are shown and where.
///
/// # Errors
///
/// `InvalidInput` outside 1–11.
pub fn quick_layout(cs: &mut ChartSpace, n: u8) -> Result<()> {
    use ChartElementKind as E;
    use ChartLegendPosition as L;
    let (title, legend, axis_titles, labels, major, minor): (
        bool,
        Option<L>,
        bool,
        Option<ChartLabelPosition>,
        bool,
        bool,
    ) = match n {
        1 => (true, Some(L::Right), false, None, true, false),
        2 => (
            true,
            Some(L::Top),
            false,
            Some(ChartLabelPosition::OutsideEnd),
            false,
            false,
        ),
        3 => (true, Some(L::Bottom), false, None, true, false),
        4 => (
            false,
            Some(L::Bottom),
            false,
            Some(ChartLabelPosition::OutsideEnd),
            false,
            false,
        ),
        5 => (true, None, true, None, true, false),
        6 => (true, Some(L::Right), true, None, true, false),
        7 => (false, Some(L::Right), true, None, true, true),
        8 => (true, None, true, None, false, false),
        9 => (true, Some(L::Right), true, None, true, false),
        10 => (
            true,
            Some(L::Right),
            false,
            Some(ChartLabelPosition::OutsideEnd),
            true,
            false,
        ),
        11 => (false, Some(L::Right), false, None, false, false),
        _ => return Err(invalid(format!("quick layout {n} does not exist (1–11)"))),
    };
    set_element(cs, E::ChartTitle, title);
    set_legend(cs, legend);
    set_element(cs, E::PrimaryHorizontalAxisTitle, axis_titles);
    set_element(cs, E::PrimaryVerticalAxisTitle, axis_titles);
    set_data_labels(cs, labels);
    set_element(cs, E::MajorGridlines, major);
    set_element(cs, E::MinorGridlines, minor);
    Ok(())
}

/// Applies chart style `n` (1–48): the style number picks the colour scheme; explicit series
/// colours are cleared so the style shows.
///
/// # Errors
///
/// `InvalidInput` outside 1–48.
pub fn set_style(cs: &mut ChartSpace, n: u8) -> Result<()> {
    if !(1..=48).contains(&n) {
        return Err(invalid(format!("chart style {n} does not exist (1–48)")));
    }
    cs.style = Some(u32::from(n));
    // The c14:style alternate content would override the new style in Office.
    cs.extras.remove_named("mc:AlternateContent");
    for g in &mut cs.chart.plot_area.groups {
        for s in &mut g.series {
            s.shape.set_fill(None);
            if !matches!(
                s.shape.line.as_ref().and_then(|l| l.fill.as_ref()),
                Some(Fill::None)
            ) {
                s.shape.set_line(None);
            }
            for p in &mut s.points {
                p.shape.set_fill(None);
            }
            if let Some(m) = &mut s.marker {
                m.shape = ShapeProps::default();
            }
        }
    }
    Ok(())
}

/// Change Colors: 0 colourful, 1–6 shades of accent n, 7 greys (keeps the style's row).
///
/// # Errors
///
/// `InvalidInput` above 7.
pub fn set_colors(cs: &mut ChartSpace, palette: u8) -> Result<()> {
    if palette > 7 {
        return Err(invalid(format!(
            "colour set {palette} does not exist (0–7)"
        )));
    }
    let row = cs.style.map_or(0, |s| (s.clamp(1, 48) - 1) / 8);
    let column = match palette {
        0 => 1,
        7 => 0,
        k => u32::from(k) + 1,
    };
    let style = u8::try_from(row * 8 + column + 1).unwrap_or(2);
    set_style(cs, style)
}

fn color_from_hex(hex: &str) -> Result<Option<DrawingColor>> {
    let h = hex.trim().trim_start_matches('#');
    if h.is_empty() {
        return Ok(None);
    }
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid(format!("not an RRGGBB colour: {hex}")));
    }
    Ok(Some(DrawingColor::srgb(h)))
}

/// Formats a series (by `idx`).
///
/// # Errors
///
/// `InvalidInput` for an unknown series or a malformed colour.
pub fn format_series(
    cs: &mut ChartSpace,
    idx: u32,
    fill: Option<&str>,
    line: Option<&str>,
    line_width: Option<f32>,
    secondary: Option<bool>,
) -> Result<()> {
    let fill = fill.map(color_from_hex).transpose()?;
    let line = line.map(color_from_hex).transpose()?;
    let s = cs
        .chart
        .plot_area
        .groups
        .iter_mut()
        .flat_map(|g| g.series.iter_mut())
        .find(|s| s.idx == idx)
        .ok_or_else(|| invalid(format!("no series {idx}")))?;
    if let Some(f) = fill {
        s.shape.set_fill(f.map(Fill::Solid));
    }
    if line.is_some() || line_width.is_some() {
        let mut l = s.shape.line.clone().unwrap_or_default();
        if let Some(c) = line {
            l.fill = c.map(Fill::Solid);
        }
        if let Some(w) = line_width.filter(|w| w.is_finite() && *w >= 0.0) {
            l.width = Some((f64::from(w.min(1584.0)) * 12_700.0).round() as i64);
        }
        s.shape.set_line(Some(l));
    }
    if let Some(sec) = secondary {
        move_series_to_axis(cs, idx, sec);
    }
    Ok(())
}

fn move_series_to_axis(cs: &mut ChartSpace, idx: u32, secondary: bool) {
    let pa = &mut cs.chart.plot_area;
    let Some(gi) = pa
        .groups
        .iter()
        .position(|g| g.series.iter().any(|s| s.idx == idx))
    else {
        return;
    };
    let currently = pa.groups[gi]
        .series
        .iter()
        .any(|s| s.idx == idx && s.secondary);
    if currently == secondary || pa.groups[gi].kind.is_circular() {
        return;
    }
    let Some(si) = pa.groups[gi].series.iter().position(|s| s.idx == idx) else {
        return;
    };
    let series = pa.groups[gi].series.remove(si);
    if pa.groups[gi].series.is_empty() && pa.groups.len() > 1 {
        pa.groups.remove(gi);
    }
    let kind = pa
        .groups
        .get(gi.min(pa.groups.len().saturating_sub(1)))
        .map_or(
            GroupKind::Line {
                grouping: Some(Grouping::Standard),
                markers: Some(true),
                three_d: false,
            },
            |g| g.kind.clone(),
        );
    if secondary && !pa.axes.iter().any(|a| a.id == VAL_AX2) {
        let mut cat2 = category_axis(CAT_AX2, VAL_AX2, AxisPos::Bottom);
        cat2.delete = Some(true);
        let mut val2 = value_axis(VAL_AX2, CAT_AX2, AxisPos::Right);
        val2.major_gridlines = None;
        val2.crosses = Some(Crosses::Max);
        pa.axes.push(cat2);
        pa.axes.push(val2);
    }
    let ids = if secondary {
        vec![CAT_AX2, VAL_AX2]
    } else {
        pa.groups
            .iter()
            .find(|g| !g.series.iter().any(|s| s.secondary) && !g.axis_ids.is_empty())
            .map_or(vec![CAT_AX, VAL_AX], |g| g.axis_ids.clone())
    };
    if let Some(g) = pa.groups.iter_mut().find(|g| {
        g.axis_ids == ids && std::mem::discriminant(&g.kind) == std::mem::discriminant(&kind)
    }) {
        g.series.push(series);
        g.series.sort_by_key(|s| s.order);
    } else {
        pa.groups.push(group(kind, vec![series], ids));
    }
    if !secondary {
        let used: Vec<i64> = pa.groups.iter().flat_map(|g| g.axis_ids.clone()).collect();
        pa.axes.retain(|a| used.contains(&a.id));
    }
    crate::read::mark_secondary(pa);
}

/// Format Axis options; `None` leaves an option alone.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AxisOptions {
    /// Minimum.
    pub min: Option<ChartBound>,
    /// Maximum.
    pub max: Option<ChartBound>,
    /// Major unit (must be positive).
    pub major_unit: Option<ChartBound>,
    /// Logarithm base, 2–1000 (`Auto` turns the log scale off).
    pub log_base: Option<ChartBound>,
    /// Values in reverse order.
    pub reversed: Option<bool>,
    /// Number format code (empty links to the source).
    pub number_format: Option<String>,
}

/// Applies Format Axis options.
///
/// # Errors
///
/// `InvalidInput` when the chart has no such axis or a value is out of range (non-finite, a
/// non-positive unit, a log base outside 2–1000, a minimum not below the maximum).
pub fn format_axis(cs: &mut ChartSpace, target: ChartAxisTarget, o: &AxisOptions) -> Result<()> {
    let fixed = |b: Option<ChartBound>, what: &str| -> Result<Option<Option<f64>>> {
        match b {
            None => Ok(None),
            Some(ChartBound::Auto) => Ok(Some(None)),
            Some(ChartBound::Fixed(v)) if v.is_finite() => Ok(Some(Some(v))),
            Some(ChartBound::Fixed(v)) => Err(invalid(format!("{what} {v} is not a number"))),
        }
    };
    let min = fixed(o.min, "minimum")?;
    let max = fixed(o.max, "maximum")?;
    let unit = fixed(o.major_unit, "major unit")?;
    if let Some(Some(u)) = unit
        && u <= 0.0
    {
        return Err(invalid(format!("the major unit must be positive, not {u}")));
    }
    let log = fixed(o.log_base, "log base")?;
    if let Some(Some(b)) = log
        && !(2.0..=1000.0).contains(&b)
    {
        return Err(invalid(format!("the log base must be 2–1000, not {b}")));
    }
    let swapped = cs.chart.plot_area.groups.iter().any(|g| {
        matches!(
            g.kind,
            GroupKind::Bar {
                dir: BarDir::Bar,
                ..
            }
        )
    });
    let (value, secondary) = match target {
        ChartAxisTarget::PrimaryValue => (true, false),
        ChartAxisTarget::PrimaryCategory => (false, false),
        ChartAxisTarget::SecondaryValue => (true, true),
        ChartAxisTarget::SecondaryCategory => (false, true),
    };
    let horizontal = value == swapped;
    let a = axis_by_role(cs, horizontal, secondary)
        .ok_or_else(|| invalid(format!("the chart has no {target:?} axis")))?;
    let new_min = min.unwrap_or(a.scaling.min);
    let new_max = max.unwrap_or(a.scaling.max);
    if let (Some(lo), Some(hi)) = (new_min, new_max)
        && lo >= hi
    {
        return Err(invalid(format!(
            "the minimum ({lo}) must be below the maximum ({hi})"
        )));
    }
    a.scaling.min = new_min;
    a.scaling.max = new_max;
    if let Some(u) = unit {
        a.major_unit = u;
    }
    if let Some(b) = log {
        a.scaling.log_base = b;
    }
    if let Some(r) = o.reversed {
        a.scaling.reversed = Some(r);
    }
    if let Some(code) = &o.number_format {
        a.num_fmt = Some(if code.is_empty() {
            NumFmt {
                code: "General".into(),
                source_linked: Some(true),
            }
        } else {
            NumFmt {
                code: code.clone(),
                source_linked: Some(false),
            }
        });
    }
    Ok(())
}

/// What a chart command needs from the host besides the chart.
#[derive(Clone, Copy, Default)]
pub struct HostContext<'a> {
    /// The cells the chart reads.
    pub data: Option<&'a dyn ChartData>,
    /// The selected range (Grid), for `ChartInsert`: sheet and range.
    pub selection: Option<(&'a str, Range)>,
}

impl std::fmt::Debug for HostContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostContext")
            .field("data", &self.data.is_some())
            .field("selection", &self.selection)
            .finish()
    }
}

/// Applies a chart command to `cs` (for `ChartInsert`, `cs` is replaced by the new chart).
/// Returns whether the chart changed. `ChartMove` is the host's business (it moves the drawing
/// anchor) and returns `false` here.
///
/// # Errors
///
/// `InvalidInput` for bad arguments, `Unsupported` for a non-chart command.
pub fn apply(cs: &mut ChartSpace, cmd: &SuiteCommand, host: HostContext<'_>) -> Result<bool> {
    let before = cs.clone();
    let need_data = || {
        host.data
            .ok_or_else(|| invalid("this chart has no cells to read (no workbook)"))
    };
    match cmd {
        SuiteCommand::ChartInsert { kind } if host.data.is_none() && host.selection.is_none() => {
            // Write and Deck: no cells to chart, so the chart carries its own sample data.
            *cs = sample_chart(*kind);
        }
        SuiteCommand::ChartInsert { kind } => {
            let data = need_data()?;
            let (sheet, range) = host
                .selection
                .ok_or_else(|| invalid("select the cells to chart first"))?;
            *cs = insert_from_range(data, sheet, range, *kind)?;
        }
        SuiteCommand::ChartChangeType { kind } => {
            let combo = matches!(
                kind,
                ChartKind::ComboColumnLine | ChartKind::ComboColumnLineSecondary
            );
            let n: usize = cs
                .chart
                .plot_area
                .groups
                .iter()
                .map(|g| g.series.len())
                .sum();
            if combo && n < 2 {
                return Err(invalid(
                    "a combo chart needs at least two series: add a series with Select Data first",
                ));
            }
            change_type(cs, *kind);
        }
        SuiteCommand::ChartSwitchRowColumn => switch_row_column(cs, need_data()?)?,
        SuiteCommand::ChartSelectData { range } => select_data(cs, need_data()?, range)?,
        SuiteCommand::ChartSetElement { element, on } => set_element(cs, *element, *on),
        SuiteCommand::ChartSetLegend { position } => set_legend(cs, *position),
        SuiteCommand::ChartSetDataLabels { position } => set_data_labels(cs, *position),
        SuiteCommand::ChartQuickLayout { layout } => quick_layout(cs, *layout)?,
        SuiteCommand::ChartSetStyle { style } => set_style(cs, *style)?,
        SuiteCommand::ChartSetColors { palette } => set_colors(cs, *palette)?,
        SuiteCommand::ChartSetTitle { text } => set_title(cs, text),
        SuiteCommand::ChartMove { .. } => return Ok(false),
        SuiteCommand::ChartFormatSeries {
            series,
            fill,
            line,
            line_width,
            secondary,
        } => format_series(
            cs,
            *series,
            fill.as_deref(),
            line.as_deref(),
            *line_width,
            *secondary,
        )?,
        SuiteCommand::ChartFormatAxis {
            axis,
            min,
            max,
            major_unit,
            log_base,
            reversed,
            number_format,
        } => format_axis(
            cs,
            *axis,
            &AxisOptions {
                min: *min,
                max: *max,
                major_unit: *major_unit,
                log_base: *log_base,
                reversed: *reversed,
                number_format: number_format.clone(),
            },
        )?,
        other => {
            return Err(KilnError::Unsupported(format!(
                "{} is not a chart command",
                other.meta().id
            )));
        }
    }
    if let Some(data) = host.data {
        refresh(cs, data);
    }
    Ok(*cs != before)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{chart_facts, overlap};
    use crate::{read_chart, write_chart};
    use kiln_sheet::{CellValue, Workbook};

    pub(crate) fn budget() -> Workbook {
        let mut wb = Workbook::new();
        let id = wb.add_sheet(Some("Budget"), None, None).unwrap();
        let rows: [(&str, [f64; 3]); 4] = [
            ("Q1", [730.0, 1180.0, 0.0]),
            ("Q2", [505.0, 640.0, 0.0]),
            ("Q3", [745.0, 1240.0, 120.0]),
            ("Q4", [650.0, 1320.0, 380.0]),
        ];
        for (c, h) in ["Quarter", "Materials", "Firing", "Events"]
            .iter()
            .enumerate()
        {
            wb.load_value(id, 0, c as u32, CellValue::text(*h)).unwrap();
        }
        for (r, (q, vals)) in rows.iter().enumerate() {
            let r = r as u32 + 1;
            wb.load_value(id, r, 0, CellValue::text(*q)).unwrap();
            for (c, v) in vals.iter().enumerate() {
                wb.load_value(id, r, c as u32 + 1, CellValue::Number(*v))
                    .unwrap();
            }
        }
        wb
    }

    fn host(wb: &Workbook) -> HostContext<'_> {
        HostContext {
            data: Some(wb),
            selection: Some(("Budget", Range::new(0, 0, 4, 3))),
        }
    }

    #[test]
    fn every_kind_inserts_writes_and_reads_back() {
        let wb = budget();
        for kind in ChartKind::ALL {
            let cs = insert_from_range(&wb, "Budget", Range::new(0, 0, 4, 3), Some(kind)).unwrap();
            assert_eq!(kind_of(&cs), Some(kind), "{kind:?}");
            let bytes = write_chart(&cs);
            let back = read_chart(&bytes).unwrap();
            assert_eq!(kind_of(&back), Some(kind), "{kind:?} after a round trip");
            let (fa, fb) = (chart_facts(&cs), chart_facts(&back));
            let o = overlap(&fa, &fb);
            let lost: Vec<&String> = fa.iter().filter(|f| !fb.contains(f)).collect();
            let added: Vec<&String> = fb.iter().filter(|f| !fa.contains(f)).collect();
            assert!(o > 0.999, "{kind:?}: {o}\nlost {lost:#?}\nadded {added:#?}");
        }
    }

    #[test]
    fn insert_change_type_switch_and_undo_by_value() {
        let wb = budget();
        let mut cs = ChartSpace::default();
        assert!(
            apply(
                &mut cs,
                &SuiteCommand::ChartInsert { kind: None },
                host(&wb)
            )
            .unwrap()
        );
        assert_eq!(kind_of(&cs), Some(ChartKind::ColumnClustered));
        let n = |cs: &ChartSpace| {
            cs.chart
                .plot_area
                .groups
                .iter()
                .map(|g| g.series.len())
                .sum::<usize>()
        };
        assert_eq!(n(&cs), 3);
        let before = cs.clone();
        apply(&mut cs, &SuiteCommand::ChartSwitchRowColumn, host(&wb)).unwrap();
        assert_eq!(n(&cs), 4, "quarters become series");
        assert_eq!(
            cs.chart.plot_area.groups[0].series[0]
                .name
                .as_ref()
                .unwrap()
                .text(),
            Some("Q1")
        );
        apply(&mut cs, &SuiteCommand::ChartSwitchRowColumn, host(&wb)).unwrap();
        assert_eq!(n(&cs), 3);
        apply(
            &mut cs,
            &SuiteCommand::ChartChangeType {
                kind: ChartKind::LineMarkers,
            },
            host(&wb),
        )
        .unwrap();
        assert_eq!(kind_of(&cs), Some(ChartKind::LineMarkers));
        apply(
            &mut cs,
            &SuiteCommand::ChartChangeType {
                kind: ChartKind::Treemap,
            },
            host(&wb),
        )
        .unwrap();
        assert_eq!(cs.format, PartFormat::ChartEx);
        apply(
            &mut cs,
            &SuiteCommand::ChartChangeType {
                kind: ChartKind::ColumnStacked,
            },
            host(&wb),
        )
        .unwrap();
        assert_eq!(cs.format, PartFormat::Classic);
        assert_ne!(cs, before);
        let mut other = before.clone();
        assert!(
            !apply(
                &mut other,
                &SuiteCommand::ChartSetLegend {
                    position: Some(ChartLegendPosition::Bottom)
                },
                host(&wb)
            )
            .unwrap()
        );
    }

    #[test]
    fn elements_layouts_styles_and_formatting() {
        let wb = budget();
        let mut cs = insert_from_range(
            &wb,
            "Budget",
            Range::new(0, 0, 4, 3),
            Some(ChartKind::ColumnClustered),
        )
        .unwrap();
        for n in 1..=11 {
            quick_layout(&mut cs, n).unwrap();
        }
        assert!(quick_layout(&mut cs, 12).is_err());
        set_element(&mut cs, ChartElementKind::PrimaryVerticalAxisTitle, true);
        assert!(
            cs.chart
                .plot_area
                .axes
                .iter()
                .any(|a| a.kind == AxisKind::Value && a.title.is_some())
        );
        set_data_labels(&mut cs, Some(ChartLabelPosition::InsideEnd));
        assert!(
            cs.chart.plot_area.groups[0].series.iter().all(|s| s
                .data_labels
                .as_ref()
                .unwrap()
                .position
                == Some(LabelPosition::InsideEnd))
        );
        set_title(&mut cs, "Spend by quarter");
        assert_eq!(
            crate::layout::title_text(&cs).as_deref(),
            Some("Spend by quarter")
        );
        format_series(&mut cs, 1, Some("E05A27"), None, Some(2.0), None).unwrap();
        assert!(format_series(&mut cs, 1, Some("nope"), None, None, None).is_err());
        format_series(&mut cs, 2, None, None, None, Some(true)).unwrap();
        assert!(
            cs.chart
                .plot_area
                .groups
                .iter()
                .any(|g| g.series.iter().any(|s| s.secondary))
        );
        let fixed = |v| Some(ChartBound::Fixed(v));
        format_axis(
            &mut cs,
            ChartAxisTarget::PrimaryValue,
            &AxisOptions {
                min: fixed(0.0),
                max: fixed(2000.0),
                major_unit: fixed(500.0),
                reversed: Some(false),
                number_format: Some("#,##0".into()),
                ..AxisOptions::default()
            },
        )
        .unwrap();
        let bad = |o: AxisOptions| {
            format_axis(&mut cs.clone(), ChartAxisTarget::PrimaryValue, &o).is_err()
        };
        assert!(
            bad(AxisOptions {
                min: fixed(5000.0),
                ..AxisOptions::default()
            }),
            "min above max"
        );
        assert!(bad(AxisOptions {
            major_unit: fixed(-1.0),
            ..AxisOptions::default()
        }));
        assert!(bad(AxisOptions {
            log_base: fixed(1.0),
            ..AxisOptions::default()
        }));
        assert!(bad(AxisOptions {
            max: fixed(f64::INFINITY),
            ..AxisOptions::default()
        }));
        format_axis(
            &mut cs,
            ChartAxisTarget::PrimaryValue,
            &AxisOptions {
                max: Some(ChartBound::Auto),
                ..AxisOptions::default()
            },
        )
        .unwrap();
        set_style(&mut cs, 5).unwrap();
        assert_eq!(cs.style, Some(5));
        set_colors(&mut cs, 7).unwrap();
        assert_eq!(cs.style, Some(1));
        set_element(&mut cs, ChartElementKind::LinearTrendline, true);
        let back = read_chart(&write_chart(&cs)).unwrap();
        assert!(overlap(&chart_facts(&cs), &chart_facts(&back)) > 0.999);
    }

    #[test]
    fn recommendations_follow_the_shape() {
        let wb = budget();
        let shape = analyse_range(&wb, "Budget", Range::new(0, 0, 4, 3)).unwrap();
        assert_eq!(recommend(&wb, &shape)[0], ChartKind::ColumnClustered);
        let single = analyse_range(&wb, "Budget", Range::new(0, 0, 4, 1)).unwrap();
        assert!(recommend(&wb, &single).contains(&ChartKind::Pie));
    }
}
