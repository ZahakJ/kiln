// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the chart chrome shows about a chart ([`ChartState`]): type, style and colour set,
//! elements, series with their colours, and every axis with the bounds it is drawn with. The
//! chrome renders this and sends `SuiteCommand::Chart*` back; it computes nothing itself.

use kiln_ooxml::drawingml::color::Rgba;
use kiln_ooxml::drawingml::fill::Fill;
use kiln_protocol::commands::{
    ChartAxisState, ChartAxisTarget, ChartElementKind, ChartLabelPosition, ChartLegendPosition,
    ChartSeriesState, ChartState,
};

use crate::data::data_range;
use crate::edit::{axis_role_id, bar_swapped, kind_of};
use crate::layout::{ChartLayout, title_text};
use crate::model::*;
use crate::palette::ChartColors;

/// EMU per point.
const EMU_PER_PT: f64 = 12_700.0;

fn hex(c: Rgba) -> String {
    let c = c.to_color();
    format!("{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

/// The Change Colors set of a chart style (Office numbering 1–48): column 2 of a row is
/// colourful (0), columns 3–8 shade accents 1–6, column 1 is greys (7).
fn color_set_of(style: Option<u32>) -> u8 {
    match style.map_or(1, |s| (s.clamp(1, 48) - 1) % 8) {
        0 => 7,
        1 => 0,
        // 2..=7 here, so the set is 1..=6.
        c => u8::try_from(c - 1).unwrap_or(0),
    }
}

/// The style a Change Colors set gives within the chart's style row (as `edit::set_colors`).
fn style_for_set(style: Option<u32>, set: u32) -> u32 {
    let row = style.map_or(0, |s| (s.clamp(1, 48) - 1) / 8);
    let column = match set {
        0 => 1,
        7 => 0,
        k => k + 1,
    };
    row * 8 + column + 1
}

fn solid(fill: Option<&Fill>, colors: &ChartColors) -> Option<Rgba> {
    match fill? {
        Fill::Solid(c) => Some(colors.resolve(c)),
        _ => None,
    }
}

fn is_line_kind(kind: &GroupKind) -> bool {
    matches!(
        kind,
        GroupKind::Line { .. } | GroupKind::Scatter { .. } | GroupKind::Radar { .. }
    )
}

fn series_state(
    g: &ChartGroup,
    s: &Series,
    style: Option<u32>,
    colors: &ChartColors,
) -> ChartSeriesState {
    let auto = colors.series_color(s.idx as usize, style);
    let fill = solid(s.shape.fill.as_ref(), colors);
    let line_fill = s.shape.line.as_ref().and_then(|l| l.fill.as_ref());
    let line = solid(line_fill, colors);
    let line_kind = is_line_kind(&g.kind);
    let color = if line_kind {
        line.unwrap_or(auto)
    } else {
        fill.unwrap_or(auto)
    };
    let explicit_width = s
        .shape
        .line
        .as_ref()
        .and_then(|l| l.width)
        .map(|w| w as f64 / EMU_PER_PT);
    let no_line = matches!(line_fill, Some(Fill::None));
    let line_width = if no_line {
        0.0
    } else {
        explicit_width.unwrap_or(if line_kind { 2.25 } else { 0.0 })
    };
    ChartSeriesState {
        idx: s.idx,
        name: s
            .name
            .as_ref()
            .and_then(SeriesText::text)
            .map_or_else(|| format!("Series{}", s.idx + 1), str::to_owned),
        color: hex(color),
        fill: fill.map(hex),
        line: line.map(hex),
        line_width,
        secondary: s.secondary,
    }
}

fn legend_position(l: &Legend) -> ChartLegendPosition {
    match l.position {
        Some(LegendPos::Top) => ChartLegendPosition::Top,
        Some(LegendPos::Left) => ChartLegendPosition::Left,
        Some(LegendPos::Bottom) => ChartLegendPosition::Bottom,
        Some(LegendPos::TopRight) => ChartLegendPosition::TopRight,
        Some(LegendPos::Right) | None => ChartLegendPosition::Right,
    }
}

fn label_position(groups: &[ChartGroup]) -> Option<ChartLabelPosition> {
    let (g, labels) = groups.iter().find_map(|g| {
        g.series
            .iter()
            .find_map(|s| s.data_labels.as_ref())
            .or(g.data_labels.as_ref())
            .filter(|l| l.shows_anything())
            .map(|l| (g, l))
    })?;
    Some(match labels.position {
        Some(LabelPosition::BestFit) => ChartLabelPosition::BestFit,
        Some(LabelPosition::Bottom) => ChartLabelPosition::Below,
        Some(LabelPosition::Top) => ChartLabelPosition::Above,
        Some(LabelPosition::Center) => ChartLabelPosition::Center,
        Some(LabelPosition::InsideBase) => ChartLabelPosition::InsideBase,
        Some(LabelPosition::InsideEnd) => ChartLabelPosition::InsideEnd,
        Some(LabelPosition::Left) => ChartLabelPosition::Left,
        Some(LabelPosition::OutsideEnd) => ChartLabelPosition::OutsideEnd,
        Some(LabelPosition::Right) => ChartLabelPosition::Right,
        // Office's defaults where the file sets none.
        None => match g.kind {
            GroupKind::Pie { .. } => ChartLabelPosition::BestFit,
            GroupKind::Bar { grouping, .. } if !grouping.is_some_and(Grouping::is_stacked) => {
                ChartLabelPosition::OutsideEnd
            }
            GroupKind::Line { .. } | GroupKind::Scatter { .. } => ChartLabelPosition::Right,
            _ => ChartLabelPosition::Center,
        },
    })
}

fn axis(cs: &ChartSpace, horizontal: bool, secondary: bool) -> Option<&Axis> {
    let id = axis_role_id(cs, horizontal, secondary)?;
    cs.chart.plot_area.axes.iter().find(|a| a.id == id)
}

fn elements(cs: &ChartSpace) -> Vec<ChartElementKind> {
    let swapped = bar_swapped(cs);
    let value = axis(cs, swapped, false);
    let category = axis(cs, !swapped, false);
    let h = axis(cs, true, false);
    let v = axis(cs, false, false);
    let series = || {
        cs.chart
            .plot_area
            .groups
            .iter()
            .flat_map(|g| g.series.iter())
    };
    let trend = |k: TrendlineKind| series().any(|s| s.trendlines.iter().any(|t| t.kind == k));
    let checks = [
        (ChartElementKind::ChartTitle, title_text(cs).is_some()),
        (
            ChartElementKind::PrimaryHorizontalAxis,
            h.is_some_and(Axis::is_visible),
        ),
        (
            ChartElementKind::PrimaryVerticalAxis,
            v.is_some_and(Axis::is_visible),
        ),
        (
            ChartElementKind::PrimaryHorizontalAxisTitle,
            h.is_some_and(|a| a.title.is_some()),
        ),
        (
            ChartElementKind::PrimaryVerticalAxisTitle,
            v.is_some_and(|a| a.title.is_some()),
        ),
        (
            ChartElementKind::MajorGridlines,
            value.is_some_and(|a| a.major_gridlines.is_some()),
        ),
        (
            ChartElementKind::MinorGridlines,
            value.is_some_and(|a| a.minor_gridlines.is_some()),
        ),
        (
            ChartElementKind::CategoryGridlines,
            category.is_some_and(|a| a.major_gridlines.is_some()),
        ),
        (ChartElementKind::Legend, cs.chart.legend.is_some()),
        (
            ChartElementKind::DataLabels,
            label_position(&cs.chart.plot_area.groups).is_some(),
        ),
        (
            ChartElementKind::LinearTrendline,
            trend(TrendlineKind::Linear),
        ),
        (
            ChartElementKind::ExponentialTrendline,
            trend(TrendlineKind::Exponential),
        ),
        (
            ChartElementKind::MovingAverage,
            trend(TrendlineKind::MovingAverage),
        ),
    ];
    checks
        .into_iter()
        .filter(|(_, on)| *on)
        .map(|(k, _)| k)
        .collect()
}

fn axis_states(cs: &ChartSpace, layout: &ChartLayout) -> Vec<ChartAxisState> {
    let swapped = bar_swapped(cs);
    let targets = [
        (ChartAxisTarget::PrimaryValue, true, false),
        (ChartAxisTarget::PrimaryCategory, false, false),
        (ChartAxisTarget::SecondaryValue, true, true),
        (ChartAxisTarget::SecondaryCategory, false, true),
    ];
    let mut out = Vec::new();
    for (target, value_role, secondary) in targets {
        let horizontal = value_role == swapped;
        let Some(a) = axis(cs, horizontal, secondary) else {
            continue;
        };
        if a.delete == Some(true) && secondary {
            // The hidden second category axis of a combo chart.
            continue;
        }
        let scale = layout
            .axes
            .iter()
            .find(|l| l.id == a.id)
            .and_then(|l| l.scale.as_ref());
        let is_value = a.kind == AxisKind::Value;
        let side = match (horizontal, secondary) {
            (true, false) => "Horizontal",
            (false, false) => "Vertical",
            (true, true) => "Secondary Horizontal",
            (false, true) => "Secondary Vertical",
        };
        let role = if is_value { "Value" } else { "Category" };
        let described = format!("{side} ({role}) Axis");
        let label = a
            .title
            .as_ref()
            .and_then(|t| t.text.as_ref())
            .and_then(TitleText::plain)
            .filter(|t| !t.trim().is_empty())
            .map_or(described.clone(), |t| format!("{described}: {t}"));
        out.push(ChartAxisState {
            target,
            label,
            is_value,
            min: scale.map(|s| s.min).or(a.scaling.min).filter(|_| is_value),
            max: scale.map(|s| s.max).or(a.scaling.max).filter(|_| is_value),
            major_unit: scale.map(|s| s.major).or(a.major_unit).filter(|_| is_value),
            min_auto: a.scaling.min.is_none(),
            max_auto: a.scaling.max.is_none(),
            major_unit_auto: a.major_unit.is_none(),
            log_base: a.scaling.log_base,
            reversed: a.is_reversed(),
            number_format: a
                .num_fmt
                .as_ref()
                .filter(|f| f.source_linked != Some(true))
                .map(|f| f.code.clone()),
        });
    }
    out
}

/// The chrome's view of `cs` as laid out in `layout` (the bounds of automatic axes come from
/// it) with `colors` (the theme the chart is drawn in).
#[must_use]
pub fn chart_state(cs: &ChartSpace, layout: &ChartLayout, colors: &ChartColors) -> ChartState {
    let style = cs.style;
    let mut series: Vec<(&ChartGroup, &Series)> = cs
        .chart
        .plot_area
        .groups
        .iter()
        .flat_map(|g| g.series.iter().map(move |s| (g, s)))
        .collect();
    series.sort_by_key(|(_, s)| s.order);
    let has_cells = series.iter().any(|(_, s)| {
        [&s.categories, &s.values, &s.bubble_sizes]
            .into_iter()
            .flatten()
            .any(|d| d.formula().is_some_and(|f| !f.is_empty()))
    });
    let categories = series
        .first()
        .and_then(|(_, s)| s.categories.as_ref())
        .map(|c| {
            (0..c.len())
                .map(|i| c.label(i).unwrap_or_default())
                .collect()
        })
        .unwrap_or_default();
    ChartState {
        kind: kind_of(cs),
        style: style.and_then(|s| u8::try_from(s).ok()),
        color_set: color_set_of(style),
        color_sets: (0..8)
            .map(|set| {
                let st = style_for_set(style, set);
                (0..6)
                    .map(|i| hex(colors.series_color(i, Some(st))))
                    .collect()
            })
            .collect(),
        title: title_text(cs),
        legend: cs.chart.legend.as_ref().map(legend_position),
        data_labels: label_position(&cs.chart.plot_area.groups),
        elements: elements(cs),
        data_range: data_range(cs).map(|a| a.to_formula()).unwrap_or_default(),
        series: series
            .iter()
            .map(|(g, s)| series_state(g, s, style, colors))
            .collect(),
        categories,
        has_cells,
        axes: axis_states(cs, layout),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{HostContext, apply, sample_chart};
    use crate::layout::layout_chart;
    use crate::palette::ColorMode;
    use crate::text::{TextMeasurer, default_fonts};
    use kiln_protocol::commands::{ChartBound, ChartKind, SuiteCommand};

    fn state_of(cs: &ChartSpace) -> ChartState {
        let colors = ChartColors::new(None, ColorMode::Screen { dark: false });
        let mut text = TextMeasurer::new(default_fonts());
        let layout = layout_chart(cs, 560.0, 360.0, &colors, &mut text);
        chart_state(cs, &layout, &colors)
    }

    #[test]
    fn sample_column_chart_state() {
        let cs = sample_chart(None);
        let st = state_of(&cs);
        assert_eq!(st.kind, Some(ChartKind::ColumnClustered));
        assert!(!st.has_cells);
        assert_eq!(st.data_range, "");
        assert_eq!(st.categories.len(), 4);
        assert_eq!(st.series.len(), 3);
        assert_eq!(st.series[0].name, "Series 1");
        assert_eq!(st.series[0].fill, None);
        assert_eq!(st.color_set, 0);
        assert_eq!(st.color_sets.len(), 8);
        assert!(st.color_sets.iter().all(|s| s.len() == 6));
        // The first series takes the first colour of the current set.
        assert_eq!(st.series[0].color, st.color_sets[0][0]);
        assert_eq!(st.legend, Some(ChartLegendPosition::Bottom));
        assert!(st.elements.contains(&ChartElementKind::MajorGridlines));
        assert!(st.elements.contains(&ChartElementKind::Legend));
        let v = st
            .axes
            .iter()
            .find(|a| a.target == ChartAxisTarget::PrimaryValue)
            .unwrap();
        assert!(v.is_value && v.min_auto && v.max_auto && v.major_unit_auto);
        // The sample's values run to 5: the drawn scale is 0–6 by 1.
        assert_eq!(
            (v.min, v.max, v.major_unit),
            (Some(0.0), Some(6.0), Some(1.0))
        );
        let c = st
            .axes
            .iter()
            .find(|a| a.target == ChartAxisTarget::PrimaryCategory)
            .unwrap();
        assert!(!c.is_value && c.min.is_none());
    }

    #[test]
    fn state_follows_edits() {
        let mut cs = sample_chart(None);
        let run = |cs: &mut ChartSpace, cmd: SuiteCommand| {
            apply(cs, &cmd, HostContext::default()).unwrap();
        };
        // Change Colors first: like Office, it clears colours set on series.
        run(&mut cs, SuiteCommand::ChartSetColors { palette: 3 });
        run(
            &mut cs,
            SuiteCommand::ChartFormatAxis {
                axis: ChartAxisTarget::PrimaryValue,
                min: Some(ChartBound::Fixed(1.0)),
                max: Some(ChartBound::Fixed(9.0)),
                major_unit: Some(ChartBound::Fixed(2.0)),
                log_base: None,
                reversed: Some(true),
                number_format: Some("0.0".into()),
            },
        );
        run(
            &mut cs,
            SuiteCommand::ChartFormatSeries {
                series: 1,
                fill: Some("3159C6".into()),
                line: None,
                line_width: None,
                secondary: None,
            },
        );
        run(
            &mut cs,
            SuiteCommand::ChartSetLegend {
                position: Some(ChartLegendPosition::Right),
            },
        );
        let st = state_of(&cs);
        let v = &st.axes[0];
        assert_eq!(v.target, ChartAxisTarget::PrimaryValue);
        assert_eq!(
            (v.min, v.max, v.major_unit),
            (Some(1.0), Some(9.0), Some(2.0))
        );
        assert!(!v.min_auto && !v.max_auto && !v.major_unit_auto && v.reversed);
        assert_eq!(v.number_format.as_deref(), Some("0.0"));
        assert_eq!(st.series[1].fill.as_deref(), Some("3159C6"));
        assert_eq!(st.series[1].color, "3159C6");
        assert_eq!(st.color_set, 3);
        assert_eq!(st.series[0].color, st.color_sets[3][0]);
        assert_eq!(st.legend, Some(ChartLegendPosition::Right));
    }

    #[test]
    fn colour_sets_round_trip_through_styles() {
        for row in 0..6 {
            for set in 0..8 {
                let style = style_for_set(Some(row * 8 + 1), set);
                assert_eq!(u32::from(color_set_of(Some(style))), set);
            }
        }
        assert_eq!(color_set_of(None), 0);
    }

    #[test]
    fn line_series_report_their_line() {
        let cs = sample_chart(Some(ChartKind::LineMarkers));
        let st = state_of(&cs);
        assert!(st.series.iter().all(|s| (s.line_width - 2.25).abs() < 1e-9));
        let pie = state_of(&sample_chart(Some(ChartKind::Pie)));
        assert!(pie.axes.is_empty());
        assert_eq!(pie.data_labels, None);
    }
}
