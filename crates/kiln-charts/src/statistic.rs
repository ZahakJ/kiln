// SPDX-License-Identifier: AGPL-3.0-or-later
//! Drawing Excel 2016's statistic and flow charts (histogram, Pareto, box and whisker, waterfall,
//! funnel). Each is laid out as the classic chart its picture is made of, computed from the
//! chartex series the way Excel computes it:
//!
//! - histogram: bins of the values by Scott's normal reference rule (`3.5 σ / ∛n`), or the bin
//!   width or count of `cx:binning`; the first bin closed at both ends, the others `(a, b]`;
//! - Pareto: the categories' totals in descending order, with their cumulative percentage on a
//!   secondary 0–100 % axis;
//! - box and whisker: per series the exclusive quartiles (`QUARTILE.EXC`), whiskers to the
//!   furthest values within 1.5 × the interquartile range and the mean marked with an ×;
//! - waterfall: each value floats from the running total, rises and falls in their own colours,
//!   and points set as totals (`cx:subtotals`) stand on the axis;
//! - funnel: centred horizontal bars, first category at the top.

use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::drawingml::line::Line;
use kiln_ooxml::xml::XmlElement;
use kiln_protocol::commands::ChartKind;

use crate::edit::new_chart;
use crate::model::*;
use crate::xmlx::Extras;

pub(crate) mod axes;

/// The classic chart that draws a statistic chart, or `None` for any other chart.
#[must_use]
pub fn derive(cs: &ChartSpace) -> Option<ChartSpace> {
    let g = cs.chart.plot_area.groups.first()?;
    let GroupKind::Statistic { layout } = g.kind else {
        return None;
    };
    let first = g.series.first()?;
    let mut out = match layout {
        StatLayout::Histogram => histogram(first),
        StatLayout::Pareto => pareto(first),
        StatLayout::BoxWhisker => box_whisker(&g.series, &visibility(first)),
        StatLayout::Waterfall => waterfall(first),
        StatLayout::Funnel => funnel(first),
    }?;
    // The chart's own look: title, legend, areas, text and style.
    out.chart.title.clone_from(&cs.chart.title);
    out.chart.auto_title_deleted = cs.chart.auto_title_deleted;
    // The title the chart itself shows (the drawn chart's series would name another).
    if let Some(t) = &mut out.chart.title
        && t.text.as_ref().and_then(TitleText::plain).is_none()
        && let Some(shown) = crate::layout::title_text(cs)
    {
        t.text = Some(TitleText::Rich {
            plain: shown,
            run: RunProps::default(),
            raw: None,
        });
    }
    if layout == StatLayout::BoxWhisker {
        // The boxes are named on the category axis.
        out.chart.legend = None;
    } else if layout != StatLayout::Waterfall {
        out.chart.legend.clone_from(&cs.chart.legend);
    } else if cs.chart.legend.is_none() {
        out.chart.legend = None;
    } else if let (Some(l), Some(own)) = (&mut out.chart.legend, &cs.chart.legend) {
        l.position = own.position;
        l.overlay = own.overlay;
    }
    out.chart
        .plot_area
        .shape
        .clone_from(&cs.chart.plot_area.shape);
    out.shape.clone_from(&cs.shape);
    out.text.clone_from(&cs.text);
    out.style = cs.style;
    out.rounded_corners = cs.rounded_corners;
    axes::apply(cs, &mut out);
    Some(out)
}

fn values(s: &Series) -> Vec<f64> {
    s.values
        .as_ref()
        .map(|v| {
            (0..v.len())
                .filter_map(|i| v.number(i))
                .filter(|v| v.is_finite())
                .collect()
        })
        .unwrap_or_default()
}

fn labelled(s: &Series) -> Vec<(String, f64)> {
    let Some(v) = &s.values else {
        return Vec::new();
    };
    (0..v.len())
        .filter_map(|i| {
            let n = v.number(i).filter(|n| n.is_finite())?;
            let label = s
                .categories
                .as_ref()
                .and_then(|c| c.label(i))
                .unwrap_or_else(|| (i + 1).to_string());
            Some((label, n))
        })
        .collect()
}

fn num(points: Vec<Option<f64>>, format: &str) -> DataSource {
    DataSource::Num(NumData {
        formula: None,
        format_code: Some(format.to_owned()),
        points,
        has_cache: true,
        extras: Extras::default(),
    })
}

fn text(labels: Vec<String>) -> DataSource {
    DataSource::Str(StrData {
        formula: None,
        points: labels.into_iter().map(Some).collect(),
        has_cache: true,
        extras: Extras::default(),
    })
}

fn series(idx: u32, name: &str, cats: &[String], vals: Vec<Option<f64>>, format: &str) -> Series {
    Series {
        idx,
        order: idx,
        name: Some(SeriesText::Literal(name.to_owned())),
        categories: Some(text(cats.to_vec())),
        values: Some(num(vals, format)),
        ..Series::default()
    }
}

fn hidden(s: &mut Series) {
    s.shape.set_fill(Some(Fill::None));
    s.shape.set_line(Some(Line {
        fill: Some(Fill::None),
        ..Line::default()
    }));
}

fn format_of(s: &Series) -> String {
    match &s.values {
        Some(DataSource::Num(n)) => n.format_code.clone().unwrap_or_else(|| "General".into()),
        _ => "General".into(),
    }
}

fn bar_options(cs: &mut ChartSpace, gap: u32, overlap: Option<i32>) {
    for g in &mut cs.chart.plot_area.groups {
        if let GroupKind::Bar {
            gap_width,
            overlap: o,
            ..
        } = &mut g.kind
        {
            *gap_width = Some(gap);
            if overlap.is_some() {
                *o = overlap;
            }
        }
    }
}

/// A short label for a bin edge (Excel shows the edges in the data's format, rounded).
fn edge(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r.fract() == 0.0 {
        format!("{r:.0}")
    } else {
        let s = format!("{r:.2}");
        s.trim_end_matches('0').to_owned()
    }
}

/// `cx:binning`'s width or count, when the file sets one.
fn binning(s: &Series) -> (Option<f64>, Option<usize>) {
    let b = s
        .extras
        .elements()
        .find(|e| e.name == "cx:layoutPr")
        .and_then(|l| l.child("cx:binning"));
    let val = |e: Option<&XmlElement>| {
        e.and_then(|e| {
            e.attr("val")
                .map_or_else(|| Some(e.text()), |v| Some(v.to_owned()))
        })
        .and_then(|v| v.trim().parse::<f64>().ok())
    };
    let size = val(b.and_then(|b| b.child("cx:binSize"))).filter(|w| *w > 0.0);
    let count = val(b.and_then(|b| b.child("cx:binCount")))
        .filter(|c| *c >= 1.0)
        .map(|c| c as usize);
    (size, count)
}

fn histogram(s: &Series) -> Option<ChartSpace> {
    let mut v = values(s);
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let (lo, hi) = (v[0], v[v.len() - 1]);
    #[allow(clippy::cast_precision_loss)]
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let sd = if n > 1.0 {
        (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt()
    } else {
        0.0
    };
    let (size, count) = binning(s);
    let width = match (size, count) {
        (Some(w), _) => w,
        (None, Some(c)) => (hi - lo) / c as f64,
        _ => 3.5 * sd / n.cbrt(),
    };
    let bins = if width > 0.0 && hi > lo {
        (((hi - lo) / width).ceil() as usize).clamp(1, 1000)
    } else {
        1
    };
    let width = if width > 0.0 && hi > lo { width } else { 1.0 };
    let mut counts = vec![0u32; bins];
    for x in &v {
        // (a, b] bins; the first one also takes its lower edge.
        let k = if *x <= lo {
            0
        } else {
            (((x - lo) / width).ceil() as usize)
                .saturating_sub(1)
                .min(bins - 1)
        };
        counts[k] += 1;
    }
    let labels: Vec<String> = (0..bins)
        .map(|k| {
            let a = lo + width * k as f64;
            let b = a + width;
            if k == 0 {
                format!("[{}, {}]", edge(a), edge(b))
            } else {
                format!("({}, {}]", edge(a), edge(b))
            }
        })
        .collect();
    let name = s
        .name
        .as_ref()
        .and_then(SeriesText::text)
        .unwrap_or("Series1");
    let bars = series(
        0,
        name,
        &labels,
        counts.into_iter().map(|c| Some(f64::from(c))).collect(),
        "General",
    );
    let mut cs = new_chart(
        ChartKind::ColumnClustered,
        vec![Series {
            shape: s.shape.clone(),
            ..bars
        }],
    );
    bar_options(&mut cs, 0, None);
    Some(cs)
}

fn pareto(s: &Series) -> Option<ChartSpace> {
    let mut totals: Vec<(String, f64)> = Vec::new();
    for (label, v) in labelled(s) {
        match totals.iter_mut().find(|(l, _)| *l == label) {
            Some(t) => t.1 += v,
            None => totals.push((label, v)),
        }
    }
    if totals.is_empty() {
        return None;
    }
    totals.sort_by(|a, b| b.1.total_cmp(&a.1));
    let sum: f64 = totals.iter().map(|t| t.1).sum();
    let labels: Vec<String> = totals.iter().map(|t| t.0.clone()).collect();
    let mut run = 0.0;
    let cumulative: Vec<Option<f64>> = totals
        .iter()
        .map(|t| {
            run += t.1;
            Some(if sum == 0.0 { 0.0 } else { run / sum })
        })
        .collect();
    let name = s
        .name
        .as_ref()
        .and_then(SeriesText::text)
        .unwrap_or("Series1");
    let bars = Series {
        shape: s.shape.clone(),
        ..series(
            0,
            name,
            &labels,
            totals.iter().map(|t| Some(t.1)).collect(),
            &format_of(s),
        )
    };
    let line = series(1, "Cumulative %", &labels, cumulative, "0%");
    let mut cs = new_chart(ChartKind::ComboColumnLineSecondary, vec![bars, line]);
    bar_options(&mut cs, 0, None);
    for g in &mut cs.chart.plot_area.groups {
        if let GroupKind::Line { markers, .. } = &mut g.kind {
            *markers = Some(false);
        }
        for s in g.series.iter_mut().filter(|s| s.idx == 1) {
            s.marker = Some(Marker {
                symbol: Some(MarkerSymbol::None),
                size: None,
                shape: ShapeProps::default(),
                extras: Extras::default(),
            });
        }
    }
    // The secondary value axis runs from 0 to 100 %.
    let secondary: Vec<i64> = cs
        .chart
        .plot_area
        .groups
        .iter()
        .filter(|g| g.series.iter().any(|s| s.secondary))
        .flat_map(|g| g.axis_ids.clone())
        .collect();
    for a in &mut cs.chart.plot_area.axes {
        if a.kind == AxisKind::Value && secondary.contains(&a.id) {
            a.scaling.min = Some(0.0);
            a.scaling.max = Some(1.0);
            a.num_fmt = Some(NumFmt {
                code: "0%".into(),
                source_linked: Some(false),
            });
        }
    }
    Some(cs)
}

/// `QUARTILE.EXC` of sorted values (`p` in 0..1), clamped to the data.
fn quartile_exc(v: &[f64], p: f64) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let h = p * (v.len() as f64 + 1.0);
    let k = h.floor();
    let lo = (k as usize).clamp(1, v.len()) - 1;
    let hi = (k as usize + 1).clamp(1, v.len()) - 1;
    v[lo] + (h - k) * (v[hi] - v[lo])
}

/// What `cx:layoutPr/cx:visibility` shows: the mean marker and the outliers of a box and
/// whisker chart, the connector lines of a waterfall (each on unless the file turns it off).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Visibility {
    pub(crate) mean_marker: bool,
    pub(crate) outliers: bool,
    pub(crate) connector_lines: bool,
}

pub(crate) fn visibility(s: &Series) -> Visibility {
    let v = s
        .extras
        .elements()
        .find(|e| e.name == "cx:layoutPr")
        .and_then(|l| l.child("cx:visibility"));
    let on = |name: &str| v.and_then(|v| v.attr_bool(name)) != Some(false);
    Visibility {
        mean_marker: on("meanMarker"),
        outliers: on("outliers"),
        connector_lines: on("connectorLines"),
    }
}

fn box_whisker(all: &[Series], vis: &Visibility) -> Option<ChartSpace> {
    let mut names = Vec::new();
    let (mut base, mut lower, mut upper, mut mean) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut down, mut up) = (Vec::new(), Vec::new());
    // The values beyond the whiskers, per box.
    let mut outliers: Vec<Vec<f64>> = Vec::new();
    for (i, s) in all.iter().enumerate() {
        let mut v = values(s);
        if v.is_empty() {
            continue;
        }
        v.sort_by(f64::total_cmp);
        let q1 = quartile_exc(&v, 0.25);
        let q2 = quartile_exc(&v, 0.5);
        let q3 = quartile_exc(&v, 0.75);
        let iqr = q3 - q1;
        let lo = v
            .iter()
            .copied()
            .find(|x| *x >= q1 - 1.5 * iqr)
            .unwrap_or(q1);
        let hi = v
            .iter()
            .rev()
            .copied()
            .find(|x| *x <= q3 + 1.5 * iqr)
            .unwrap_or(q3);
        #[allow(clippy::cast_precision_loss)]
        let m = v.iter().sum::<f64>() / v.len() as f64;
        names.push(
            s.name
                .as_ref()
                .and_then(SeriesText::text)
                .map_or_else(|| format!("Series{}", i + 1), str::to_owned),
        );
        outliers.push(v.iter().copied().filter(|x| *x < lo || *x > hi).collect());
        base.push(Some(q1));
        lower.push(Some(q2 - q1));
        upper.push(Some(q3 - q2));
        mean.push(Some(m));
        down.push((q1 - lo, 0.0));
        up.push((0.0, hi - q3));
    }
    if names.is_empty() {
        return None;
    }
    let whiskers = |custom: Vec<(f64, f64)>, bar_type| ErrorBars {
        dir: None,
        bar_type,
        val_type: ErrorValueType::Custom,
        no_end_cap: Some(false),
        value: None,
        shape: ShapeProps::default(),
        extras: Extras::default(),
        custom,
    };
    let mut b = series(3, "", &names, base, "General");
    hidden(&mut b);
    b.error_bars = vec![whiskers(down, ErrorBarType::Minus)];
    let l = series(0, "Q1 – median", &names, lower, "General");
    let mut u = series(1, "Median – Q3", &names, upper, "General");
    u.error_bars = vec![whiskers(up, ErrorBarType::Plus)];
    let mut cs = new_chart(ChartKind::ColumnStacked, vec![b, l, u]);
    bar_options(&mut cs, 100, Some(100));
    let axes: Vec<i64> = cs.chart.plot_area.groups[0].axis_ids.clone();
    let boxes = names.len();
    for g in &mut cs.chart.plot_area.groups {
        // The first series is the invisible base (new_chart numbers the series afresh).
        for s in g.series.iter_mut().skip(1) {
            // Both halves of a box in its series' accent, outlined in the background colour,
            // which draws the median between them (Excel's default style).
            s.shape.set_line(Some(Line {
                width: Some(12700),
                fill: Some(Fill::Solid(
                    kiln_ooxml::drawingml::color::DrawingColor::scheme("bg1"),
                )),
                ..Line::default()
            }));
            s.points = (0..boxes)
                .map(|i| {
                    let mut shape = ShapeProps::default();
                    shape.set_fill(Some(Fill::Solid(
                        kiln_ooxml::drawingml::color::DrawingColor::scheme(&format!(
                            "accent{}",
                            i % 6 + 1
                        )),
                    )));
                    DataPoint {
                        idx: u32::try_from(i).unwrap_or(u32::MAX),
                        shape,
                        ..DataPoint::default()
                    }
                })
                .collect();
        }
    }
    let mut m = series(2, "Mean", &names, mean, "General");
    // The mean as a cross in the background colour over the box.
    let mut cross = ShapeProps::default();
    cross.set_fill(Some(Fill::None));
    cross.set_line(Some(Line {
        width: Some(19050),
        fill: Some(Fill::Solid(
            kiln_ooxml::drawingml::color::DrawingColor::scheme("bg1"),
        )),
        ..Line::default()
    }));
    m.marker = Some(Marker {
        symbol: Some(MarkerSymbol::X),
        size: Some(7),
        shape: cross,
        extras: Extras::default(),
    });
    m.shape.set_line(Some(Line {
        fill: Some(Fill::None),
        ..Line::default()
    }));
    let mut marks = Vec::new();
    if vis.mean_marker {
        marks.push(m);
    }
    // Outliers as dots in their box's accent: the k-th outlier of every box in one series.
    let most = if vis.outliers {
        outliers.iter().map(Vec::len).max().unwrap_or(0)
    } else {
        0
    };
    for k in 0..most {
        let idx = u32::try_from(4 + k).unwrap_or(u32::MAX);
        let points = outliers.iter().map(|o| o.get(k).copied()).collect();
        let mut o = series(idx, "Outliers", &names, points, "General");
        o.shape.set_line(Some(Line {
            fill: Some(Fill::None),
            ..Line::default()
        }));
        o.points = (0..boxes)
            .map(|i| {
                let accent = Fill::Solid(kiln_ooxml::drawingml::color::DrawingColor::scheme(
                    &format!("accent{}", i % 6 + 1),
                ));
                let mut shape = ShapeProps::default();
                shape.set_fill(Some(accent.clone()));
                shape.set_line(Some(Line {
                    fill: Some(accent),
                    ..Line::default()
                }));
                DataPoint {
                    idx: u32::try_from(i).unwrap_or(u32::MAX),
                    marker: Some(Marker {
                        symbol: Some(MarkerSymbol::Circle),
                        size: Some(5),
                        shape,
                        extras: Extras::default(),
                    }),
                    ..DataPoint::default()
                }
            })
            .collect();
        marks.push(o);
    }
    if marks.is_empty() {
        return Some(cs);
    }
    cs.chart.plot_area.groups.push(ChartGroup {
        kind: GroupKind::Line {
            grouping: Some(Grouping::Standard),
            markers: Some(true),
            three_d: false,
        },
        vary_colors: Some(false),
        series: marks,
        data_labels: None,
        axis_ids: axes,
        extras: Extras::default(),
    });
    Some(cs)
}

/// The indexes `cx:subtotals` marks as totals.
fn subtotals(s: &Series) -> Vec<usize> {
    s.extras
        .elements()
        .filter(|e| e.name == "cx:layoutPr")
        .filter_map(|l| l.child("cx:subtotals"))
        .flat_map(|t| t.children_named("cx:idx"))
        .filter_map(|i| i.attr("val").and_then(|v| v.parse().ok()))
        .collect()
}

fn waterfall(s: &Series) -> Option<ChartSpace> {
    let points = labelled(s);
    if points.is_empty() {
        return None;
    }
    let totals = subtotals(s);
    let labels: Vec<String> = points.iter().map(|p| p.0.clone()).collect();
    let (mut base, mut rise, mut fall, mut total) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut run = 0.0f64;
    for (i, (_, v)) in points.iter().enumerate() {
        if totals.contains(&i) {
            run = *v;
            base.push(None);
            rise.push(None);
            fall.push(None);
            total.push(Some(*v));
            continue;
        }
        let (a, b) = (run, run + v);
        run = b;
        // A step that crosses the axis is drawn from the axis.
        let floor = if a.signum() == b.signum() || a == 0.0 || b == 0.0 {
            if a.abs() < b.abs() { a } else { b }
        } else {
            0.0
        };
        base.push((floor != 0.0).then_some(floor));
        let size = (b - floor).abs().max((a - floor).abs());
        let size = if floor == 0.0 {
            b.abs().max(a.abs())
        } else {
            size
        };
        let signed = if b < 0.0 || (floor < 0.0) {
            -size
        } else {
            size
        };
        if *v >= 0.0 {
            rise.push(Some(signed));
            fall.push(None);
        } else {
            rise.push(None);
            fall.push(Some(signed));
        }
        total.push(None);
    }
    let format = format_of(s);
    // Excel's colours: increases accent 1, decreases accent 2, totals accent 3.
    let accent = |mut s: Series, slot: &str| {
        s.shape.set_fill(Some(Fill::Solid(
            kiln_ooxml::drawingml::color::DrawingColor::scheme(slot),
        )));
        s
    };
    let r = accent(series(0, "Increase", &labels, rise, &format), "accent1");
    let f = accent(series(1, "Decrease", &labels, fall, &format), "accent2");
    let t = accent(series(2, "Total", &labels, total, &format), "accent3");
    let mut b = series(3, "", &labels, base, &format);
    hidden(&mut b);
    let mut cs = new_chart(ChartKind::ColumnStacked, vec![b, r, f, t]);
    bar_options(&mut cs, 50, Some(100));
    // The invisible base has no legend entry.
    if let Some(l) = &mut cs.chart.legend {
        l.entries = vec![
            XmlElement::new("c:legendEntry")
                .with_child(XmlElement::new("c:idx").with_attr("val", "0"))
                .with_child(XmlElement::new("c:delete").with_attr("val", "1")),
        ];
    }
    Some(cs)
}

/// The level each waterfall step ends at (the running total, or a total's own value), when the
/// chart is a waterfall that shows its connector lines.
fn connector_levels(cs: &ChartSpace) -> Option<Vec<f64>> {
    if axes::layout_of(cs) != Some(StatLayout::Waterfall) {
        return None;
    }
    let s = cs.chart.plot_area.groups.first()?.series.first()?;
    if !visibility(s).connector_lines {
        return None;
    }
    let totals = subtotals(s);
    let mut run = 0.0;
    Some(
        labelled(s)
            .iter()
            .enumerate()
            .map(|(i, (_, v))| {
                run = if totals.contains(&i) { *v } else { run + v };
                run
            })
            .collect(),
    )
}

/// Excel's connector lines of a waterfall: a thin line from the end of each step to the next
/// bar, at the running total. `out` is the layout of the classic chart [`derive`] made.
pub(crate) fn draw_connectors(
    cs: &ChartSpace,
    out: &mut crate::layout::ChartLayout,
    colors: &crate::palette::ChartColors,
) {
    use crate::layout::{ChartElement, HitShape, Mark};
    let Some(levels) = connector_levels(cs) else {
        return;
    };
    let Some(scale) = out
        .axes
        .iter()
        .find(|a| a.id == crate::edit::VAL_AX)
        .and_then(|a| a.scale)
    else {
        return;
    };
    // The visible bars (increase, decrease, total: the series after the hidden base).
    let mut bars: Vec<Option<kiln_scene::Bounds>> = vec![None; levels.len()];
    for (el, shape) in &out.hits {
        if let (
            ChartElement::Point {
                group: 0,
                series: 1..=3,
                point,
            },
            HitShape::Rect(b),
        ) = (el, shape)
            && let Some(slot) = bars.get_mut(*point)
        {
            *slot = Some(*b);
        }
    }
    let plot = out.plot;
    let mut stroke = crate::layout::solid(crate::palette::to_paint(colors.gridline()), 1.0);
    stroke.cap = kiln_scene::LineCap::Butt;
    for i in 0..levels.len().saturating_sub(1) {
        let (Some(a), Some(b)) = (bars[i], bars[i + 1]) else {
            continue;
        };
        #[allow(clippy::cast_possible_truncation)]
        let f = scale.fraction(levels[i]) as f32;
        if !f.is_finite() || !(0.0..=1.0).contains(&f) {
            continue;
        }
        let y = (plot.bottom() - f * plot.h).round() + 0.5;
        let mut path = kiln_scene::PathData::new();
        path.move_to(a.right(), y).line_to(b.x, y);
        out.marks.push(Mark {
            element: ChartElement::PlotArea,
            path,
            fill: None,
            stroke: Some(stroke.clone()),
        });
    }
}

fn funnel(s: &Series) -> Option<ChartSpace> {
    let points = labelled(s);
    if points.is_empty() {
        return None;
    }
    let max = points.iter().map(|p| p.1.abs()).fold(0.0, f64::max);
    let labels: Vec<String> = points.iter().map(|p| p.0.clone()).collect();
    let mut b = series(
        1,
        "",
        &labels,
        points
            .iter()
            .map(|p| Some((max - p.1.abs()) / 2.0))
            .collect(),
        "General",
    );
    hidden(&mut b);
    let name = s
        .name
        .as_ref()
        .and_then(SeriesText::text)
        .unwrap_or("Series1");
    let mut shape = s.shape.clone();
    if shape.fill.is_none() {
        // The visible bars sit second in the stack; they keep the first accent, as in Excel.
        shape.set_fill(Some(Fill::Solid(
            kiln_ooxml::drawingml::color::DrawingColor::scheme("accent1"),
        )));
    }
    let v = Series {
        shape,
        ..series(
            0,
            name,
            &labels,
            points.iter().map(|p| Some(p.1.abs())).collect(),
            &format_of(s),
        )
    };
    let mut cs = new_chart(ChartKind::BarStacked, vec![b, v]);
    bar_options(&mut cs, 6, Some(100));
    for a in &mut cs.chart.plot_area.axes {
        match a.kind {
            // First category at the top, no value axis or gridlines.
            AxisKind::Category => a.scaling.reversed = Some(true),
            _ => {
                a.delete = Some(true);
                a.major_gridlines = None;
            }
        }
    }
    cs.chart.legend = None;
    Some(cs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{HostContext, apply, insert_from_range, kind_of};
    use crate::{read_chart, write_chart};
    use kiln_protocol::commands::SuiteCommand;
    use kiln_sheet::{CellValue, Range, Workbook};

    fn book(rows: &[(&str, f64)]) -> Workbook {
        let mut wb = Workbook::new();
        let id = wb.sheets()[0].id();
        wb.load_value(id, 0, 0, CellValue::text("Item")).unwrap();
        wb.load_value(id, 0, 1, CellValue::text("Amount")).unwrap();
        for (r, (l, v)) in rows.iter().enumerate() {
            let r = r as u32 + 1;
            wb.load_value(id, r, 0, CellValue::text(*l)).unwrap();
            wb.load_value(id, r, 1, CellValue::Number(*v)).unwrap();
        }
        wb
    }

    fn bars(cs: &ChartSpace) -> Vec<Vec<Option<f64>>> {
        cs.chart.plot_area.groups[0]
            .series
            .iter()
            .map(|s| {
                let v = s.values.as_ref().unwrap();
                (0..v.len()).map(|i| v.number(i)).collect()
            })
            .collect()
    }

    #[test]
    fn statistic_charts_insert_write_read_back_and_draw() {
        let rows = [
            ("Clay", 420.0),
            ("Glaze", 310.0),
            ("Gas", 1180.0),
            ("Shelves", -200.0),
            ("Open day", 380.0),
            ("Kiln hire", 640.0),
            ("Glaze", 90.0),
        ];
        let wb = book(&rows);
        let sheet = wb.sheets()[0].name().to_owned();
        for kind in [
            ChartKind::Histogram,
            ChartKind::Pareto,
            ChartKind::BoxWhisker,
            ChartKind::Waterfall,
            ChartKind::Funnel,
        ] {
            let cs = insert_from_range(&wb, &sheet, Range::new(0, 0, 7, 1), Some(kind)).unwrap();
            assert_eq!(cs.format, PartFormat::ChartEx, "{kind:?}");
            assert_eq!(kind_of(&cs), Some(kind));
            let bytes = write_chart(&cs);
            let text = String::from_utf8(bytes.clone()).unwrap();
            assert!(text.contains("cx:axis"), "{kind:?}: {text}");
            let back = read_chart(&bytes).unwrap();
            assert_eq!(kind_of(&back), Some(kind), "{kind:?} after a round trip");
            let d = derive(&back).unwrap_or_else(|| panic!("{kind:?} draws"));
            let colors = crate::palette::ChartColors::new(
                None,
                crate::palette::ColorMode::Screen { dark: false },
            );
            let mut t = crate::text::TextMeasurer::new(crate::text::default_fonts());
            let laid = crate::layout::layout_chart(&back, 480.0, 300.0, &colors, &mut t);
            assert!(laid.marks.len() > 4, "{kind:?} marks");
            match kind {
                ChartKind::Histogram => {
                    // Scott's rule over the seven values: σ ≈ 431.9, width ≈ 790, two bins.
                    let counts: Vec<f64> = bars(&d)[0].iter().flatten().copied().collect();
                    assert_eq!(counts.iter().sum::<f64>(), 7.0);
                    assert_eq!(counts.len(), 2);
                    assert!(
                        d.chart.plot_area.groups[0].series[0]
                            .categories
                            .as_ref()
                            .unwrap()
                            .label(0)
                            .unwrap()
                            .starts_with("[-200, ")
                    );
                }
                ChartKind::Pareto => {
                    // Glaze's two rows add up (400), descending order, cumulative to 100 %.
                    let cats = d.chart.plot_area.groups[0].series[0]
                        .categories
                        .as_ref()
                        .unwrap();
                    assert_eq!(cats.label(0).as_deref(), Some("Gas"));
                    assert_eq!(cats.len(), 6);
                    let line = &d.chart.plot_area.groups[1].series[0];
                    let v = line.values.as_ref().unwrap();
                    assert!((v.number(v.len() - 1).unwrap() - 1.0).abs() < 1e-12);
                }
                ChartKind::Waterfall => {
                    // Shelves (−200) falls from 1910 to 1710: base 1710, height 200.
                    let b = bars(&d);
                    assert_eq!(b[0][3], Some(1710.0));
                    assert_eq!(b[2][3], Some(200.0));
                    assert_eq!(b[1][0], Some(420.0));
                }
                ChartKind::BoxWhisker => {
                    let b = bars(&d);
                    // QUARTILE.EXC of −200, 90, 310, 380, 420, 640, 1180: Q1 = 90 (position 2),
                    // median 380, Q3 = 640 (position 6).
                    assert_eq!(b[0][0], Some(90.0));
                    assert_eq!(b[1][0], Some(290.0));
                    assert_eq!(b[2][0], Some(260.0));
                }
                _ => {
                    let b = bars(&d);
                    assert_eq!(b[0][2], Some(0.0), "the widest bar is not offset");
                }
            }
        }
    }

    fn host(wb: &Workbook) -> HostContext<'_> {
        HostContext {
            data: Some(wb),
            selection: None,
        }
    }

    fn laid(cs: &ChartSpace) -> crate::layout::ChartLayout {
        let colors = crate::palette::ChartColors::new(
            None,
            crate::palette::ColorMode::Screen { dark: false },
        );
        let mut t = crate::text::TextMeasurer::new(crate::text::default_fonts());
        crate::layout::layout_chart(cs, 480.0, 300.0, &colors, &mut t)
    }

    fn state(cs: &ChartSpace) -> kiln_protocol::commands::ChartState {
        let colors = crate::palette::ChartColors::new(
            None,
            crate::palette::ColorMode::Screen { dark: false },
        );
        crate::state::chart_state(cs, &laid(cs), &colors)
    }

    #[test]
    fn statistic_axes_take_format_axis_and_chart_elements() {
        use kiln_protocol::commands::{ChartAxisTarget, ChartBound, ChartElementKind};
        let wb = book(&[("a", 1.0), ("b", 3.0), ("c", 4.0), ("d", 8.0), ("e", 9.0)]);
        let sheet = wb.sheets()[0].name().to_owned();
        let mut cs = insert_from_range(
            &wb,
            &sheet,
            Range::new(0, 0, 5, 1),
            Some(ChartKind::Histogram),
        )
        .unwrap();
        let format = |min, max, unit, log, fmt: Option<&str>| SuiteCommand::ChartFormatAxis {
            axis: ChartAxisTarget::PrimaryValue,
            min,
            max,
            major_unit: unit,
            log_base: log,
            reversed: None,
            number_format: fmt.map(str::to_owned),
        };
        apply(
            &mut cs,
            &format(
                Some(ChartBound::Fixed(0.0)),
                Some(ChartBound::Fixed(10.0)),
                Some(ChartBound::Fixed(2.0)),
                None,
                Some("0.0"),
            ),
            host(&wb),
        )
        .unwrap();
        // Kept through a save and a read.
        let cs = read_chart(&write_chart(&cs)).unwrap();
        let st = state(&cs);
        let v = &st.axes[0];
        assert_eq!(v.target, ChartAxisTarget::PrimaryValue);
        assert_eq!(
            (v.min, v.max, v.major_unit),
            (Some(0.0), Some(10.0), Some(2.0))
        );
        assert!(!v.min_auto && !v.max_auto && !v.major_unit_auto);
        assert_eq!(v.number_format.as_deref(), Some("0.0"));
        assert_eq!(st.axes[1].target, ChartAxisTarget::PrimaryCategory);
        // The drawing uses them.
        let drawn = laid(&cs);
        let scale = drawn
            .axes
            .iter()
            .find(|a| a.id == crate::edit::VAL_AX)
            .and_then(|a| a.scale)
            .unwrap();
        assert_eq!((scale.min, scale.max, scale.major), (0.0, 10.0, 2.0));
        // Back to automatic.
        let mut cs = cs;
        apply(
            &mut cs,
            &format(
                Some(ChartBound::Auto),
                Some(ChartBound::Auto),
                Some(ChartBound::Auto),
                None,
                Some(""),
            ),
            host(&wb),
        )
        .unwrap();
        let v = &state(&cs).axes[0];
        assert!(v.min_auto && v.max_auto && v.major_unit_auto);
        assert_eq!(v.number_format, None);
        // These charts have no log scale, and a category axis has no bounds.
        assert!(
            apply(
                &mut cs,
                &format(None, None, None, Some(ChartBound::Fixed(10.0)), None),
                host(&wb)
            )
            .is_err()
        );
        let cat = SuiteCommand::ChartFormatAxis {
            axis: ChartAxisTarget::PrimaryCategory,
            min: Some(ChartBound::Fixed(1.0)),
            max: None,
            major_unit: None,
            log_base: None,
            reversed: None,
            number_format: None,
        };
        assert!(apply(&mut cs, &cat, host(&wb)).is_err());
        // Chart Elements: gridlines, the vertical axis and its title.
        assert!(
            state(&cs)
                .elements
                .contains(&ChartElementKind::MajorGridlines)
        );
        for (element, on) in [
            (ChartElementKind::MajorGridlines, false),
            (ChartElementKind::PrimaryVerticalAxis, false),
            (ChartElementKind::PrimaryHorizontalAxisTitle, true),
        ] {
            apply(
                &mut cs,
                &SuiteCommand::ChartSetElement { element, on },
                host(&wb),
            )
            .unwrap();
        }
        let cs = read_chart(&write_chart(&cs)).unwrap();
        let el = state(&cs).elements;
        assert!(!el.contains(&ChartElementKind::MajorGridlines));
        assert!(!el.contains(&ChartElementKind::PrimaryVerticalAxis));
        assert!(el.contains(&ChartElementKind::PrimaryHorizontalAxisTitle));
        assert!(el.contains(&ChartElementKind::PrimaryHorizontalAxis));
        let d = derive(&cs).unwrap();
        let val = d
            .chart
            .plot_area
            .axes
            .iter()
            .find(|a| a.id == crate::edit::VAL_AX)
            .unwrap();
        assert_eq!(val.delete, Some(true));
        assert!(val.major_gridlines.is_none());
        let cat = d
            .chart
            .plot_area
            .axes
            .iter()
            .find(|a| a.id == crate::edit::CAT_AX)
            .unwrap();
        assert!(cat.title.is_some());
    }

    #[test]
    fn box_and_whisker_draws_its_outliers() {
        let wb = book(&[
            ("a", 1.0),
            ("b", 2.0),
            ("c", 3.0),
            ("d", 4.0),
            ("e", 5.0),
            ("f", 6.0),
            ("g", 100.0),
        ]);
        let sheet = wb.sheets()[0].name().to_owned();
        let cs = insert_from_range(
            &wb,
            &sheet,
            Range::new(0, 0, 7, 1),
            Some(ChartKind::BoxWhisker),
        )
        .unwrap();
        let d = derive(&cs).unwrap();
        let dots = &d.chart.plot_area.groups[1].series;
        // The mean, then the one outlier (100, beyond Q3 + 1.5 × IQR).
        assert_eq!(dots.len(), 2);
        let v = dots[1].values.as_ref().unwrap();
        assert_eq!(v.number(0), Some(100.0));
        // The upper whisker stops at 6.
        let up = &d.chart.plot_area.groups[0].series[2].error_bars[0].custom;
        let q3 = quartile_exc(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 100.0], 0.75);
        assert!((up[0].1 - (6.0 - q3)).abs() < 1e-9);
        // Drawn as a circle marker.
        assert!(laid(&cs).marks.iter().any(|m| matches!(
            m.element,
            crate::layout::ChartElement::Point {
                group: 1,
                series: 1,
                point: 0
            }
        )));
    }

    #[test]
    fn waterfall_draws_connector_lines_between_its_steps() {
        let wb = book(&[
            ("Start", 100.0),
            ("Up", 30.0),
            ("Down", -20.0),
            ("Up", 10.0),
        ]);
        let sheet = wb.sheets()[0].name().to_owned();
        let mut cs = insert_from_range(
            &wb,
            &sheet,
            Range::new(0, 0, 4, 1),
            Some(ChartKind::Waterfall),
        )
        .unwrap();
        let connectors = |cs: &ChartSpace| {
            laid(cs)
                .marks
                .iter()
                .filter(|m| m.element == crate::layout::ChartElement::PlotArea && m.fill.is_none())
                .count()
        };
        assert_eq!(connectors(&cs), 3);
        // At the running total: the line after "Up" (130) sits on the top of its bar.
        let l = laid(&cs);
        let top_of_up = l
            .hits
            .iter()
            .find_map(|(e, h)| match (e, h) {
                (
                    crate::layout::ChartElement::Point {
                        group: 0,
                        series: 1,
                        point: 1,
                    },
                    crate::layout::HitShape::Rect(b),
                ) => Some(b.y),
                _ => None,
            })
            .unwrap();
        let line = l
            .marks
            .iter()
            .filter(|m| m.element == crate::layout::ChartElement::PlotArea && m.fill.is_none())
            .nth(1)
            .unwrap();
        let y = line.path.bounds().y;
        assert!((y - top_of_up).abs() <= 1.0, "{y} vs {top_of_up}");
        // Turned off in the file: none.
        let s = &mut cs.chart.plot_area.groups[0].series[0];
        s.extras.remove_named("cx:layoutPr");
        s.extras.push(
            XmlElement::new("cx:layoutPr")
                .with_child(XmlElement::new("cx:visibility").with_attr("connectorLines", "0")),
        );
        assert_eq!(connectors(&cs), 0);
    }

    #[test]
    fn a_column_chart_becomes_a_waterfall_and_back() {
        let wb = book(&[("Start", 100.0), ("Up", 30.0), ("Down", -20.0)]);
        let sheet = wb.sheets()[0].name().to_owned();
        let mut cs = insert_from_range(
            &wb,
            &sheet,
            Range::new(0, 0, 3, 1),
            Some(ChartKind::ColumnClustered),
        )
        .unwrap();
        let host = HostContext {
            data: Some(&wb),
            selection: None,
        };
        apply(
            &mut cs,
            &SuiteCommand::ChartChangeType {
                kind: ChartKind::Waterfall,
            },
            host,
        )
        .unwrap();
        assert_eq!(kind_of(&cs), Some(ChartKind::Waterfall));
        assert_eq!(cs.format, PartFormat::ChartEx);
        apply(
            &mut cs,
            &SuiteCommand::ChartChangeType {
                kind: ChartKind::ColumnClustered,
            },
            host,
        )
        .unwrap();
        assert_eq!(kind_of(&cs), Some(ChartKind::ColumnClustered));
        assert_eq!(cs.format, PartFormat::Classic);
    }
}
