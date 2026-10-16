// SPDX-License-Identifier: AGPL-3.0-or-later
//! Chart layout: from a [`ChartSpace`] and a size to positioned geometry.
//!
//! The frame is laid out the way Excel does it — title at the top, legend on its side (wrapping
//! into rows when it sits above or below the plot), axis titles and tick labels measured with
//! `kiln-text` and the plot area taking what is left (or the manual layout from the file) — and
//! then every chart group draws its marks into the plot area ([`crate::layout::plot`]). The result,
//! [`ChartLayout`], holds marks with resolved paints and placed text, ready for
//! [`crate::render`], and the shapes for hit testing ([`ChartLayout::hit_test`]).

pub mod plot;

use kiln_ooxml::drawingml::color::Rgba;
use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::drawingml::line::Line;
use kiln_scene::{Bounds, LineCap, LineJoin, Paint, PathData, Stroke};
use kiln_text::Alignment;

use crate::format::{PERCENT, format_number_with};
use crate::model::*;
use crate::palette::{ChartColors, to_paint};
use crate::scale::Scale;
use crate::text::{TextBlock, TextMeasurer, TextStyle};

/// A selectable part of a chart (what the Format pane and the Chart Elements list name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChartElement {
    /// The whole chart.
    ChartArea,
    /// The plot area.
    PlotArea,
    /// The chart title.
    Title,
    /// The legend.
    Legend,
    /// One legend entry.
    LegendEntry(usize),
    /// An axis (by id).
    Axis(i64),
    /// An axis title (by axis id).
    AxisTitle(i64),
    /// Major gridlines of an axis.
    MajorGridlines(i64),
    /// Minor gridlines of an axis.
    MinorGridlines(i64),
    /// A series.
    Series {
        /// Group index.
        group: usize,
        /// Series index in the group.
        series: usize,
    },
    /// One data point.
    Point {
        /// Group index.
        group: usize,
        /// Series index in the group.
        series: usize,
        /// Point index.
        point: usize,
    },
    /// The data labels of a series.
    DataLabels {
        /// Group index.
        group: usize,
        /// Series index in the group.
        series: usize,
    },
    /// A trendline.
    Trendline {
        /// Group index.
        group: usize,
        /// Series index in the group.
        series: usize,
        /// Trendline index.
        index: usize,
    },
}

impl ChartElement {
    /// The series this element belongs to, if any.
    #[must_use]
    pub fn series(&self) -> Option<(usize, usize)> {
        match *self {
            Self::Series { group, series }
            | Self::Point { group, series, .. }
            | Self::DataLabels { group, series }
            | Self::Trendline { group, series, .. } => Some((group, series)),
            _ => None,
        }
    }
}

/// The shape a hit test checks.
#[derive(Debug, Clone, PartialEq)]
pub enum HitShape {
    /// A rectangle.
    Rect(Bounds),
    /// A ring sector (pie slice, doughnut segment, sunburst arc): centre, radii, start angle and
    /// sweep in radians clockwise from 12 o'clock.
    Wedge {
        /// Centre x.
        cx: f32,
        /// Centre y.
        cy: f32,
        /// Inner radius.
        r0: f32,
        /// Outer radius.
        r1: f32,
        /// Start angle.
        start: f32,
        /// Sweep.
        sweep: f32,
    },
    /// A closed polygon (areas, radar fills).
    Polygon(Vec<(f32, f32)>),
    /// A polyline (lines), hit within `tolerance` px.
    Polyline {
        /// Points.
        points: Vec<(f32, f32)>,
        /// Distance that counts as a hit.
        tolerance: f32,
    },
}

impl HitShape {
    fn contains(&self, x: f32, y: f32) -> bool {
        match self {
            Self::Rect(b) => b.contains(x, y),
            Self::Wedge {
                cx,
                cy,
                r0,
                r1,
                start,
                sweep,
            } => {
                let (dx, dy) = (x - cx, y - cy);
                let r = dx.hypot(dy);
                if r < *r0 || r > *r1 {
                    return false;
                }
                let a = dx.atan2(-dy).rem_euclid(std::f32::consts::TAU);
                let rel = (a - start).rem_euclid(std::f32::consts::TAU);
                rel <= *sweep
            }
            Self::Polygon(points) => point_in_polygon(points, x, y),
            Self::Polyline { points, tolerance } => {
                points
                    .windows(2)
                    .any(|w| seg_distance((x, y), w[0], w[1]) <= *tolerance)
                    || points
                        .iter()
                        .any(|p| (p.0 - x).hypot(p.1 - y) <= *tolerance)
            }
        }
    }

    /// A rectangle around the shape.
    #[must_use]
    pub fn bounds(&self) -> Bounds {
        match self {
            Self::Rect(b) => *b,
            Self::Wedge { cx, cy, r1, .. } => Bounds::new(cx - r1, cy - r1, 2.0 * r1, 2.0 * r1),
            Self::Polygon(points) => Self::Polyline {
                points: points.clone(),
                tolerance: 0.0,
            }
            .bounds(),
            Self::Polyline { points, tolerance } => {
                let (mut x0, mut y0, mut x1, mut y1) = (
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                );
                for (x, y) in points {
                    x0 = x0.min(*x);
                    y0 = y0.min(*y);
                    x1 = x1.max(*x);
                    y1 = y1.max(*y);
                }
                if x0 > x1 {
                    return Bounds::ZERO;
                }
                Bounds::from_corners(x0, y0, x1, y1).inflate(*tolerance, *tolerance)
            }
        }
    }
}

fn point_in_polygon(points: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut inside = false;
    let n = points.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = points[i];
        let (xj, yj) = points[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn seg_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let len2 = vx * vx + vy * vy;
    let t = if len2 > 0.0 {
        (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.0 - (a.0 + t * vx)).hypot(p.1 - (a.1 + t * vy))
}

/// A filled and/or stroked path.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    /// What the mark belongs to.
    pub element: ChartElement,
    /// Geometry in chart px.
    pub path: PathData,
    /// Fill.
    pub fill: Option<Paint>,
    /// Outline.
    pub stroke: Option<Stroke>,
}

/// Text at a position.
#[derive(Debug, Clone)]
pub struct PlacedText {
    /// What the text belongs to.
    pub element: ChartElement,
    /// The laid-out text.
    pub block: TextBlock,
    /// Layout origin x (top-left of the layout box).
    pub x: f32,
    /// Layout origin y.
    pub y: f32,
    /// Rotation in degrees (clockwise) around the visible text's centre.
    pub rotation: f32,
    /// The area the visible text covers (after rotation).
    pub bounds: Bounds,
}

impl PlacedText {
    /// The rotation pivot: the centre of the visible text.
    #[must_use]
    pub fn pivot(&self) -> (f32, f32) {
        (
            self.x + self.block.left + self.block.width / 2.0,
            self.y + self.block.height / 2.0,
        )
    }
}

/// The laid-out axis.
#[derive(Debug, Clone, PartialEq)]
pub struct AxisLayout {
    /// The axis id.
    pub id: i64,
    /// Horizontal axis.
    pub horizontal: bool,
    /// The axis line from start to end.
    pub line: ((f32, f32), (f32, f32)),
    /// The value scale (value axes).
    pub scale: Option<Scale>,
}

/// A laid-out chart.
#[derive(Debug, Clone, Default)]
pub struct ChartLayout {
    /// Chart width.
    pub width: f32,
    /// Chart height.
    pub height: f32,
    /// The plot area's inner rectangle (where the data is drawn).
    pub plot: Bounds,
    /// Marks in paint order.
    pub marks: Vec<Mark>,
    /// Text, painted after the marks.
    pub texts: Vec<PlacedText>,
    /// Hit shapes in paint order (the last match wins).
    pub hits: Vec<(ChartElement, HitShape)>,
    /// The axes.
    pub axes: Vec<AxisLayout>,
}

impl ChartLayout {
    /// The topmost element at (`x`, `y`), in chart px.
    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32) -> Option<ChartElement> {
        if !(x >= 0.0 && y >= 0.0 && x <= self.width && y <= self.height) {
            return None;
        }
        self.hits
            .iter()
            .rev()
            .find(|(_, s)| s.contains(x, y))
            .map(|(e, _)| *e)
            .or(Some(ChartElement::ChartArea))
    }

    /// The area an element covers (for selection outlines), if it is drawn.
    #[must_use]
    pub fn element_bounds(&self, element: ChartElement) -> Option<Bounds> {
        if element == ChartElement::ChartArea {
            return Some(Bounds::new(0.0, 0.0, self.width, self.height));
        }
        let mut acc: Option<Bounds> = None;
        for (e, s) in &self.hits {
            let matches = *e == element
                || (matches!(element, ChartElement::Series { .. })
                    && e.series() == element.series()
                    && matches!(e, ChartElement::Point { .. }));
            if matches {
                let b = s.bounds();
                acc = Some(acc.map_or(b, |a| a.union(&b)));
            }
        }
        acc
    }
}

/// Points to CSS px.
pub(crate) fn pt(points: f32) -> f32 {
    points * 4.0 / 3.0
}

/// EMU to CSS px.
#[allow(clippy::cast_precision_loss)]
pub(crate) fn emu(v: i64) -> f32 {
    v as f32 / 9525.0
}

/// Everything layout needs about colours, fonts and the chart's text defaults.
pub struct Ctx<'a> {
    /// The chart.
    pub cs: &'a ChartSpace,
    /// Colours.
    pub colors: &'a ChartColors,
    /// Text.
    pub text: &'a mut TextMeasurer,
}

impl std::fmt::Debug for Ctx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ctx").finish_non_exhaustive()
    }
}

impl Ctx<'_> {
    /// The chart's base text size in points (`c:txPr` of the chart space, else Office's 9 pt for
    /// modern charts or 10 pt for charts without a style).
    pub fn base_pt(&self) -> f32 {
        self.cs
            .text
            .run
            .size
            .map_or(if self.cs.style.is_some() { 10.0 } else { 9.0 }, |s| {
                #[allow(clippy::cast_precision_loss)]
                let v = s as f32 / 100.0;
                v.clamp(1.0, 400.0)
            })
    }

    /// A text style from the most specific text properties first, falling back to the chart's
    /// default; `scale` multiplies the base size when nothing sets one.
    pub fn style(&self, props: &[&TextProps], scale: f32, bold: bool, color: Rgba) -> TextStyle {
        let all: Vec<&TextProps> = props.iter().copied().chain([&self.cs.text]).collect();
        let size = all
            .iter()
            .find_map(|p| p.run.size)
            .filter(|_| props.iter().any(|p| p.run.size.is_some()));
        let size_px = match size {
            #[allow(clippy::cast_precision_loss)]
            Some(s) => pt((s as f32 / 100.0).clamp(1.0, 400.0)),
            None => pt(self.base_pt() * scale),
        };
        let family = all.iter().find_map(|p| p.run.latin.clone()).map_or_else(
            || self.colors.minor_font.clone(),
            |f| match f.as_str() {
                "+mn-lt" | "+mn-ea" | "+mn-cs" => self.colors.minor_font.clone(),
                "+mj-lt" | "+mj-ea" | "+mj-cs" => self.colors.major_font.clone(),
                _ => f,
            },
        );
        let bold = all.iter().find_map(|p| p.run.bold).unwrap_or(bold);
        let italic = all.iter().find_map(|p| p.run.italic).unwrap_or(false);
        let paint = all
            .iter()
            .find_map(|p| p.run.color.as_ref())
            .map_or_else(|| to_paint(color), |c| self.colors.paint(c));
        TextStyle {
            family,
            size: size_px,
            bold,
            italic,
            paint,
        }
    }

    /// Lays a block out.
    pub fn block(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: f32,
        align: Alignment,
    ) -> TextBlock {
        self.text.block(text, style, max_width, align)
    }
}

/// The fill paint of shape properties: `Some(Some(p))` an explicit paint, `Some(None)` explicitly
/// no fill, `None` automatic.
pub(crate) fn fill_paint(sp: &ShapeProps, colors: &ChartColors) -> Option<Option<Paint>> {
    match sp.fill.as_ref()? {
        Fill::None => Some(None),
        Fill::Solid(c) => Some(Some(colors.paint(c))),
        Fill::Gradient(g) => Some(
            g.stops
                .iter()
                .min_by(|a, b| a.pos.total_cmp(&b.pos))
                .map(|s| colors.paint(&s.color)),
        ),
        Fill::Pattern(p) => Some(p.fg.as_ref().map(|c| colors.paint(c))),
        Fill::Blip(_) | Fill::Group => None,
    }
}

/// The colour of shape properties' fill, if solid-ish.
pub(crate) fn fill_rgba(sp: &ShapeProps, colors: &ChartColors) -> Option<Rgba> {
    match sp.fill.as_ref()? {
        Fill::Solid(c) => Some(colors.resolve(c)),
        Fill::Gradient(g) => g.stops.first().map(|s| colors.resolve(&s.color)),
        Fill::Pattern(p) => p.fg.as_ref().map(|c| colors.resolve(c)),
        _ => None,
    }
}

/// The stroke of a line: `Some(Some(s))` explicit, `Some(None)` explicitly none, `None`
/// automatic. `auto_color` and `auto_width` fill in what the line leaves out.
pub(crate) fn line_stroke(
    line: Option<&Line>,
    colors: &ChartColors,
    auto_color: Rgba,
    auto_width: f32,
) -> Option<Option<Stroke>> {
    let line = line?;
    let paint = match &line.fill {
        Some(Fill::None) => return Some(None),
        Some(Fill::Solid(c)) => colors.paint(c),
        Some(Fill::Gradient(g)) => g
            .stops
            .first()
            .map_or_else(|| to_paint(auto_color), |s| colors.paint(&s.color)),
        _ => to_paint(auto_color),
    };
    #[allow(clippy::cast_possible_truncation)]
    let width = line.width.map_or(auto_width, |w| emu(w).clamp(0.0, 100.0));
    let dash: Vec<f32> = line
        .dash
        .as_ref()
        .map(|_| {
            line.dash_array_px()
                .unwrap_or_default()
                .into_iter()
                .map(|d| {
                    #[allow(clippy::cast_possible_truncation)]
                    let d = d as f32;
                    d * width.max(1.0) / (line.width_px().max(1.0) as f32)
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Some(Stroke {
        paint,
        width,
        join: LineJoin::Round,
        cap: if dash.is_empty() {
            LineCap::Round
        } else {
            LineCap::Butt
        },
        dash,
    }))
}

/// A solid stroke.
pub(crate) fn solid(paint: Paint, width: f32) -> Stroke {
    Stroke {
        paint,
        width,
        join: LineJoin::Round,
        cap: LineCap::Butt,
        dash: Vec::new(),
    }
}

/// The automatic title text of a chart: the series name of a single-series chart.
#[must_use]
pub fn auto_title(cs: &ChartSpace) -> Option<String> {
    let series: Vec<&Series> = cs
        .chart
        .plot_area
        .groups
        .iter()
        .flat_map(|g| g.series.iter())
        .collect();
    match series.as_slice() {
        [one] => one
            .name
            .as_ref()
            .and_then(SeriesText::text)
            .map(str::to_owned),
        _ => None,
    }
}

/// The title text to draw, if the chart shows a title.
#[must_use]
pub fn title_text(cs: &ChartSpace) -> Option<String> {
    match &cs.chart.title {
        Some(t) => Some(
            t.text
                .as_ref()
                .and_then(TitleText::plain)
                .map(str::to_owned)
                .or_else(|| auto_title(cs))
                .unwrap_or_else(|| "Chart Title".to_owned()),
        ),
        None => {
            if cs.chart.auto_title_deleted == Some(false) {
                auto_title(cs)
            } else {
                None
            }
        }
    }
}

/// Lays a chart out at `width` × `height` px.
pub fn layout_chart(
    cs: &ChartSpace,
    width: f32,
    height: f32,
    colors: &ChartColors,
    text: &mut TextMeasurer,
) -> ChartLayout {
    let width = if width.is_finite() {
        width.clamp(1.0, 20_000.0)
    } else {
        1.0
    };
    let height = if height.is_finite() {
        height.clamp(1.0, 20_000.0)
    } else {
        1.0
    };
    let mut ctx = Ctx { cs, colors, text };
    let mut out = ChartLayout {
        width,
        height,
        ..ChartLayout::default()
    };

    // Chart area.
    let area = Bounds::new(0.0, 0.0, width, height);
    let bg = fill_paint(&cs.shape, colors).unwrap_or_else(|| Some(to_paint(colors.background())));
    let border = line_stroke(cs.shape.line.as_ref(), colors, colors.gridline(), 1.0)
        .unwrap_or_else(|| {
            // Office draws a hairline border around charts in sheets; on screen Kiln draws none
            // (the selection outline and the sheet frame the chart).
            if colors.uses_kiln_palette() {
                None
            } else {
                Some(solid(to_paint(colors.gridline()), 1.0))
            }
        });
    let radius = if cs.rounded_corners == Some(true) {
        8.0
    } else {
        0.0
    };
    let inset = border.as_ref().map_or(0.0, |s| s.width / 2.0);
    out.marks.push(Mark {
        element: ChartElement::ChartArea,
        path: kiln_scene::shapes::rounded_rect(area.inflate(-inset, -inset), radius),
        fill: bg,
        stroke: border,
    });

    let pad = (width.min(height) * 0.03).clamp(4.0, 12.0);
    let mut free = area.inflate(-pad, -pad);

    // Title.
    if let Some(t) = title_text(cs) {
        let props: Vec<&TextProps> = cs.chart.title.iter().map(|t| &t.text_props).collect();
        let style = ctx.style(&props, 14.0 / 9.0, false, colors.title_text());
        let block = ctx.block(&t, &style, free.w * 0.9, Alignment::Center);
        let x = free.x + (free.w - block.layout.width) / 2.0;
        let y = free.y;
        let h = block.height;
        let overlay = cs.chart.title.as_ref().and_then(|t| t.overlay) == Some(true);
        let bounds = Bounds::new(x + block.left, y, block.width, h);
        out.hits.push((ChartElement::Title, HitShape::Rect(bounds)));
        out.texts.push(PlacedText {
            element: ChartElement::Title,
            block,
            x,
            y,
            rotation: 0.0,
            bounds,
        });
        if !overlay {
            free.y += h + pad * 0.75;
            free.h -= h + pad * 0.75;
        }
    }

    // Legend.
    if let Some(legend) = &cs.chart.legend {
        plot::layout_legend(&mut ctx, legend, &mut free, pad, &mut out);
    }

    // Plot area (manual layout: fractions of the chart area).
    let manual = cs.chart.plot_area.layout.as_ref().filter(|l| {
        !l.factor_mode && l.x.is_some() && l.y.is_some() && l.w.is_some() && l.h.is_some()
    });
    let (outer, inner_fixed) = match manual {
        Some(l) => {
            #[allow(clippy::cast_possible_truncation)]
            let b = Bounds::new(
                (l.x.unwrap_or(0.0) as f32).clamp(0.0, 1.0) * width,
                (l.y.unwrap_or(0.0) as f32).clamp(0.0, 1.0) * height,
                (l.w.unwrap_or(1.0) as f32).clamp(0.0, 1.0) * width,
                (l.h.unwrap_or(1.0) as f32).clamp(0.0, 1.0) * height,
            );
            (b, l.inner == Some(true))
        }
        None => (free, false),
    };
    if outer.w < 4.0 || outer.h < 4.0 {
        out.plot = outer;
        return out;
    }
    plot::layout_plot(&mut ctx, outer, inner_fixed, &mut out);
    out
}

/// The number format for value labels of an axis (or the data when the axis links to it).
pub(crate) fn axis_format(axis: Option<&Axis>, data_format: Option<&str>, percent: bool) -> String {
    if percent {
        if let Some(f) = axis.and_then(|a| a.num_fmt.as_ref())
            && f.source_linked != Some(true)
        {
            return f.code.clone();
        }
        return PERCENT.to_owned();
    }
    match axis.and_then(|a| a.num_fmt.as_ref()) {
        Some(f) if f.source_linked != Some(true) => f.code.clone(),
        _ => data_format.unwrap_or("General").to_owned(),
    }
}

/// Formats a value for a label.
pub(crate) fn fmt(v: f64, code: &str, cs: &ChartSpace) -> String {
    format_number_with(v, code, cs.date1904 == Some(true))
}
