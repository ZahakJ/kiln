// SPDX-License-Identifier: AGPL-3.0-or-later
//! The legend, the axes and the marks of every chart type.

use std::f32::consts::{FRAC_PI_2, TAU};

use kiln_ooxml::drawingml::color::Rgba;
use kiln_scene::{Bounds, Paint, PathData, Stroke};
use kiln_text::Alignment;

use super::{
    AxisLayout, ChartElement, ChartLayout, Ctx, HitShape, Mark, PlacedText, axis_format,
    fill_paint, fill_rgba, fmt, line_stroke, pt, solid,
};
use crate::model::*;
use crate::palette::{to_paint, transform};
use crate::scale::{Scale, nice_unit, value_scale};
use crate::text::TextStyle;

// ───────────────────────────────────────────────────────────── series appearance

/// How a series (or point) looks.
#[derive(Debug, Clone)]
pub(crate) struct Look {
    pub fill: Option<Paint>,
    pub stroke: Option<Stroke>,
    pub color: Rgba,
}

/// The automatic colour index of a series or point.
fn auto_index(s: &Series) -> usize {
    s.idx as usize
}

/// Whether a group colours each point differently.
fn varies(g: &ChartGroup) -> bool {
    let circular = matches!(g.kind, GroupKind::Pie { .. } | GroupKind::Doughnut { .. });
    match g.vary_colors {
        Some(v) => v && (circular || g.series.len() == 1),
        None => circular,
    }
}

pub(crate) fn series_color(
    ctx: &Ctx<'_>,
    g: &ChartGroup,
    s: &Series,
    point: Option<usize>,
) -> Rgba {
    let i = match point {
        Some(p) if varies(g) => p,
        _ => auto_index(s),
    };
    ctx.colors.series_color(i, ctx.cs.style)
}

/// The look of a filled mark (bar, slice, area, bubble) of series `s`, point `point`.
pub(crate) fn fill_look(
    ctx: &Ctx<'_>,
    g: &ChartGroup,
    s: &Series,
    point: Option<usize>,
    _area: bool,
) -> Look {
    let auto = series_color(ctx, g, s, point);
    let dpt = point.and_then(|p| s.points.iter().find(|d| d.idx as usize == p));
    let sp_layers: Vec<&ShapeProps> = dpt
        .map(|d| &d.shape)
        .into_iter()
        .chain([&s.shape])
        .collect();
    // A varied-colour point without its own fill takes its automatic colour, not the series fill.
    let explicit_fill = sp_layers
        .iter()
        .take(if point.is_some() && varies(g) { 1 } else { 2 })
        .find_map(|sp| fill_paint(sp, ctx.colors));
    let color = sp_layers
        .iter()
        .find_map(|sp| fill_rgba(sp, ctx.colors))
        .unwrap_or(auto);
    let fill = match explicit_fill {
        Some(f) => f,
        None => Some(to_paint(auto)),
    };
    let circular = matches!(
        g.kind,
        GroupKind::Pie { .. } | GroupKind::Doughnut { .. } | GroupKind::Hierarchy { .. }
    );
    let stroke = sp_layers
        .iter()
        .find_map(|sp| line_stroke(sp.line.as_ref(), ctx.colors, color, 1.0))
        .unwrap_or_else(|| {
            // Slices and tiles are separated by the background colour, as Office draws them.
            circular.then(|| solid(to_paint(ctx.colors.background()), 1.5))
        });
    Look {
        fill,
        stroke,
        color,
    }
}

/// The look of a line series: its stroke (automatic 2.25 pt in the series colour).
fn line_look(ctx: &Ctx<'_>, g: &ChartGroup, s: &Series, default_on: bool) -> Option<Stroke> {
    let auto = series_color(ctx, g, s, None);
    match line_stroke(s.shape.line.as_ref(), ctx.colors, auto, pt(2.25)) {
        Some(explicit) => explicit,
        None => default_on.then(|| {
            let mut st = solid(to_paint(auto), pt(2.25));
            st.cap = kiln_scene::LineCap::Round;
            st
        }),
    }
}

/// Marker shape and look of a series point, if one is drawn.
fn marker_look(
    ctx: &Ctx<'_>,
    g: &ChartGroup,
    s: &Series,
    point: usize,
    default_on: bool,
) -> Option<(MarkerSymbol, f32, Option<Paint>, Option<Stroke>)> {
    let dpt = s.points.iter().find(|d| d.idx as usize == point);
    let m = dpt.and_then(|d| d.marker.as_ref()).or(s.marker.as_ref());
    let symbol = match m.and_then(|m| m.symbol) {
        Some(MarkerSymbol::None) => return None,
        Some(MarkerSymbol::Auto) | None => {
            if !default_on {
                return None;
            }
            MarkerSymbol::Circle
        }
        Some(sym) => sym,
    };
    #[allow(clippy::cast_precision_loss)]
    let size = pt(m.and_then(|m| m.size).unwrap_or(5).clamp(2, 72) as f32);
    let auto = series_color(ctx, g, s, Some(point));
    let sp = m.map(|m| &m.shape);
    let fill = sp
        .and_then(|sp| fill_paint(sp, ctx.colors))
        .unwrap_or(Some(to_paint(auto)));
    let stroke = sp
        .and_then(|sp| line_stroke(sp.line.as_ref(), ctx.colors, auto, 1.0))
        .unwrap_or_else(|| Some(solid(to_paint(auto), 1.0)));
    Some((symbol, size, fill, stroke))
}

fn marker_path(symbol: MarkerSymbol, cx: f32, cy: f32, size: f32) -> PathData {
    let r = size / 2.0;
    let mut p = PathData::new();
    match symbol {
        MarkerSymbol::Square | MarkerSymbol::Picture | MarkerSymbol::Auto => {
            p = PathData::rect(Bounds::new(cx - r, cy - r, size, size));
        }
        MarkerSymbol::Diamond => {
            p.move_to(cx, cy - r)
                .line_to(cx + r, cy)
                .line_to(cx, cy + r)
                .line_to(cx - r, cy)
                .close();
        }
        MarkerSymbol::Triangle => {
            p.move_to(cx, cy - r)
                .line_to(cx + r, cy + r)
                .line_to(cx - r, cy + r)
                .close();
        }
        MarkerSymbol::X => {
            p.move_to(cx - r, cy - r)
                .line_to(cx + r, cy + r)
                .move_to(cx + r, cy - r)
                .line_to(cx - r, cy + r);
        }
        MarkerSymbol::Plus => {
            p.move_to(cx - r, cy)
                .line_to(cx + r, cy)
                .move_to(cx, cy - r)
                .line_to(cx, cy + r);
        }
        MarkerSymbol::Star => {
            p.move_to(cx - r, cy - r)
                .line_to(cx + r, cy + r)
                .move_to(cx + r, cy - r)
                .line_to(cx - r, cy + r)
                .move_to(cx, cy - r)
                .line_to(cx, cy + r);
        }
        MarkerSymbol::Dash => {
            p = PathData::rect(Bounds::new(cx - r, cy - size / 10.0, size, size / 5.0));
        }
        MarkerSymbol::Dot => {
            p = PathData::rect(Bounds::new(cx - r / 2.0, cy - r / 2.0, r, r));
        }
        MarkerSymbol::Circle | MarkerSymbol::None => {
            p = circle(cx, cy, r);
        }
    }
    p
}

/// A circle as four cubic arcs.
pub(crate) fn circle(cx: f32, cy: f32, r: f32) -> PathData {
    let k = 0.552_284_8 * r;
    let mut p = PathData::new();
    p.move_to(cx + r, cy)
        .cubic_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r)
        .cubic_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy)
        .cubic_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r)
        .cubic_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy)
        .close();
    p
}

fn push_marker(
    out: &mut ChartLayout,
    element: ChartElement,
    look: &(MarkerSymbol, f32, Option<Paint>, Option<Stroke>),
    x: f32,
    y: f32,
) {
    let (symbol, size, fill, stroke) = look;
    let open = matches!(
        symbol,
        MarkerSymbol::X | MarkerSymbol::Plus | MarkerSymbol::Star
    );
    out.marks.push(Mark {
        element,
        path: marker_path(*symbol, x, y, *size),
        fill: if open { None } else { *fill },
        stroke: if open {
            stroke.clone().or_else(|| fill.map(|f| solid(f, 1.5)))
        } else {
            stroke.clone()
        },
    });
    let r = size / 2.0 + 2.0;
    out.hits.push((
        element,
        HitShape::Rect(Bounds::new(x - r, y - r, 2.0 * r, 2.0 * r)),
    ));
}

// ───────────────────────────────────────────────────────────── legend

struct LegendItem {
    text: String,
    element: ChartElement,
    key: KeyLook,
}

enum KeyLook {
    Box(Option<Paint>, Option<Stroke>),
    Line(
        Option<Stroke>,
        Option<(MarkerSymbol, f32, Option<Paint>, Option<Stroke>)>,
    ),
}

fn legend_items(ctx: &Ctx<'_>, legend: &Legend) -> Vec<LegendItem> {
    let deleted = legend.deleted_entries();
    let mut items = Vec::new();
    let groups = &ctx.cs.chart.plot_area.groups;
    let single_varied = groups.len() == 1
        && groups
            .first()
            .is_some_and(|g| varies(g) && !g.series.is_empty());
    if let Some(g) = groups.first().filter(|_| single_varied) {
        if let GroupKind::Hierarchy { .. } = g.kind {
            if let Some(s) = g.series.first() {
                for (i, name) in hierarchy_roots(s).into_iter().enumerate() {
                    let c = ctx.colors.series_color(i, ctx.cs.style);
                    items.push(LegendItem {
                        text: name,
                        element: ChartElement::LegendEntry(i),
                        key: KeyLook::Box(Some(to_paint(c)), None),
                    });
                }
            }
            return items;
        }
        if let Some(s) = g.series.first() {
            let n = s
                .categories
                .as_ref()
                .map_or(0, DataSource::len)
                .max(s.values.as_ref().map_or(0, DataSource::len));
            for i in 0..n.min(256) {
                if deleted.contains(&u32::try_from(i).unwrap_or(u32::MAX)) {
                    continue;
                }
                let text = s
                    .categories
                    .as_ref()
                    .and_then(|c| c.label(i))
                    .unwrap_or_else(|| (i + 1).to_string());
                let look = fill_look(ctx, g, s, Some(i), false);
                items.push(LegendItem {
                    text,
                    element: ChartElement::LegendEntry(i),
                    key: KeyLook::Box(look.fill, None),
                });
            }
            return items;
        }
    }
    let mut n = 0usize;
    for g in groups {
        for s in &g.series {
            let entry = n;
            n += 1;
            if deleted.contains(&s.idx) {
                continue;
            }
            let text = s
                .name
                .as_ref()
                .and_then(SeriesText::text)
                .map_or_else(|| format!("Series{}", s.idx + 1), str::to_owned);
            let key = match &g.kind {
                GroupKind::Line { .. }
                | GroupKind::Radar {
                    style: Some(RadarStyle::Standard | RadarStyle::Marker) | None,
                } => {
                    let markers_default = !matches!(
                        g.kind,
                        GroupKind::Radar {
                            style: Some(RadarStyle::Standard)
                        }
                    );
                    KeyLook::Line(
                        line_look(ctx, g, s, true),
                        marker_look(ctx, g, s, usize::MAX, markers_default),
                    )
                }
                GroupKind::Scatter { style } => {
                    let lines = scatter_lines_default(*style);
                    let st = line_look(ctx, g, s, lines);
                    KeyLook::Line(
                        st,
                        marker_look(ctx, g, s, usize::MAX, scatter_markers_default(*style)),
                    )
                }
                _ => {
                    let look = fill_look(ctx, g, s, None, false);
                    KeyLook::Box(look.fill, look.stroke)
                }
            };
            items.push(LegendItem {
                text,
                element: ChartElement::LegendEntry(entry),
                key,
            });
        }
    }
    items
}

pub(crate) fn layout_legend(
    ctx: &mut Ctx<'_>,
    legend: &Legend,
    free: &mut Bounds,
    pad: f32,
    out: &mut ChartLayout,
) {
    let items = legend_items(ctx, legend);
    if items.is_empty() {
        return;
    }
    let style = ctx.style(&[&legend.text], 1.0, false, ctx.colors.text());
    let pos = legend.position.unwrap_or(LegendPos::Right);
    let vertical = matches!(
        pos,
        LegendPos::Right | LegendPos::Left | LegendPos::TopRight
    );
    let key_w = style.size * 0.75;
    let gap = style.size * 0.4;
    let item_gap = style.size * 1.1;
    let max_text = if vertical { free.w * 0.3 } else { free.w * 0.8 };
    let key_of = |it: &LegendItem| match it.key {
        KeyLook::Line(..) => key_w * 2.2,
        KeyLook::Box(..) => key_w,
    };
    let blocks: Vec<_> = items
        .iter()
        .map(|it| ctx.block(&it.text, &style, max_text.max(20.0), Alignment::Left))
        .collect();
    let line_h = blocks
        .iter()
        .map(|b| b.height)
        .fold(style.size * 1.2, f32::max);
    // Arrange: rows of (item index, x) and a total size.
    let mut placed: Vec<(usize, f32, f32)> = Vec::new();
    let (lw, lh);
    if vertical {
        let w = blocks
            .iter()
            .zip(&items)
            .map(|(b, it)| key_of(it) + gap + b.width)
            .fold(0.0f32, f32::max);
        let mut y = 0.0;
        for (i, b) in blocks.iter().enumerate() {
            if y + b.height > free.h && i > 0 {
                break;
            }
            placed.push((i, 0.0, y));
            y += b.height.max(line_h) + style.size * 0.35;
        }
        lw = w;
        lh = (y - style.size * 0.35).max(0.0);
    } else {
        let avail = free.w;
        let mut rows: Vec<Vec<(usize, f32)>> = vec![Vec::new()];
        let mut x = 0.0;
        for (i, b) in blocks.iter().enumerate() {
            let w = key_of(&items[i]) + gap + b.width;
            if x > 0.0 && x + w > avail {
                rows.push(Vec::new());
                x = 0.0;
            }
            if let Some(r) = rows.last_mut() {
                r.push((i, x));
            }
            x += w + item_gap;
        }
        // At most a third of the chart height of legend rows.
        let max_rows = ((free.h / 3.0) / line_h).floor().max(1.0) as usize;
        rows.truncate(max_rows);
        let mut widest = 0.0f32;
        for (r, row) in rows.iter().enumerate() {
            let row_w = row.last().map_or(0.0, |(i, x)| {
                x + key_of(&items[*i]) + gap + blocks[*i].width
            });
            widest = widest.max(row_w);
            let offset_row = row_w;
            #[allow(clippy::cast_precision_loss)]
            let y = r as f32 * (line_h + style.size * 0.3);
            for (i, x) in row {
                placed.push((*i, *x - offset_row / 2.0, y));
            }
        }
        lw = widest;
        #[allow(clippy::cast_precision_loss)]
        let n = rows.len() as f32;
        lh = n * line_h + (n - 1.0).max(0.0) * style.size * 0.3;
    }
    let overlay = legend.overlay == Some(true);
    let (lx, ly) = match pos {
        LegendPos::Right => (free.right() - lw, free.y + (free.h - lh) / 2.0),
        LegendPos::TopRight => (free.right() - lw, free.y),
        LegendPos::Left => (free.x, free.y + (free.h - lh) / 2.0),
        LegendPos::Top => (free.x + free.w / 2.0, free.y),
        LegendPos::Bottom => (free.x + free.w / 2.0, free.bottom() - lh),
    };
    let rect = if vertical {
        Bounds::new(lx, ly, lw, lh)
    } else {
        Bounds::new(lx - lw / 2.0, ly, lw, lh)
    };
    // Legend box formatting.
    let lfill = fill_paint(&legend.shape, ctx.colors).flatten();
    let lstroke = line_stroke(
        legend.shape.line.as_ref(),
        ctx.colors,
        ctx.colors.gridline(),
        1.0,
    )
    .flatten();
    if lfill.is_some() || lstroke.is_some() {
        out.marks.push(Mark {
            element: ChartElement::Legend,
            path: PathData::rect(rect.inflate(4.0, 3.0)),
            fill: lfill,
            stroke: lstroke,
        });
    }
    out.hits
        .push((ChartElement::Legend, HitShape::Rect(rect.inflate(4.0, 3.0))));
    for (i, x, y) in placed {
        let (x, y) = (lx + x, ly + y);
        let b = &blocks[i];
        let first_line_h = b.layout.lines.first().map_or(line_h, |l| l.height);
        let cy = y + first_line_h / 2.0;
        match &items[i].key {
            KeyLook::Box(fill, stroke) => {
                let k = key_w;
                out.marks.push(Mark {
                    element: items[i].element,
                    path: kiln_scene::shapes::rounded_rect(
                        Bounds::new(x, cy - k / 2.0, k, k),
                        if ctx.colors.uses_kiln_palette() {
                            2.0
                        } else {
                            0.0
                        },
                    ),
                    fill: *fill,
                    stroke: stroke.clone().filter(|s| s.width <= 2.0),
                });
            }
            KeyLook::Line(stroke, marker) => {
                if let Some(st) = stroke {
                    let mut p = PathData::new();
                    p.move_to(x, cy).line_to(x + key_w * 2.2, cy);
                    let mut st = st.clone();
                    st.width = st.width.min(3.0);
                    out.marks.push(Mark {
                        element: items[i].element,
                        path: p,
                        fill: None,
                        stroke: Some(st),
                    });
                }
                if let Some((sym, size, f, s)) = marker {
                    let look = (*sym, size.min(key_w * 1.2), *f, s.clone());
                    push_marker(out, items[i].element, &look, x + key_w * 1.1, cy);
                }
            }
        }
        let kw = key_of(&items[i]);
        let tx = x + kw + gap;
        let bounds = Bounds::new(tx + b.left, y, b.width, b.height);
        out.hits.push((
            items[i].element,
            HitShape::Rect(bounds.union(&Bounds::new(x, y, kw, b.height))),
        ));
        out.texts.push(PlacedText {
            element: items[i].element,
            block: b.clone(),
            x: tx,
            y,
            rotation: 0.0,
            bounds,
        });
    }
    if !overlay {
        match pos {
            LegendPos::Right | LegendPos::TopRight => free.w -= lw + pad,
            LegendPos::Left => {
                free.x += lw + pad;
                free.w -= lw + pad;
            }
            LegendPos::Top => {
                free.y += lh + pad;
                free.h -= lh + pad;
            }
            LegendPos::Bottom => free.h -= lh + pad,
        }
    }
}

// ───────────────────────────────────────────────────────────── data access

fn values(s: &Series) -> Vec<Option<f64>> {
    s.values
        .as_ref()
        .map(|v| (0..v.len()).map(|i| v.number(i)).collect())
        .unwrap_or_default()
}

fn point_count(g: &ChartGroup) -> usize {
    g.series
        .iter()
        .map(|s| {
            s.values
                .as_ref()
                .map_or(0, DataSource::len)
                .max(s.categories.as_ref().map_or(0, DataSource::len))
        })
        .max()
        .unwrap_or(0)
        .min(100_000)
}

fn category_labels(g: &ChartGroup, n: usize) -> Vec<String> {
    let cats = g.series.iter().find_map(|s| s.categories.as_ref());
    (0..n)
        .map(|i| {
            cats.and_then(|c| c.label(i))
                .unwrap_or_else(|| (i + 1).to_string())
        })
        .collect()
}

/// Per series, per point: (base, top) after stacking; `None` where the value is missing.
fn stacked(g: &ChartGroup) -> Vec<Vec<Option<(f64, f64)>>> {
    let n = point_count(g);
    let grouping = g.kind.grouping().unwrap_or(Grouping::Standard);
    let all: Vec<Vec<Option<f64>>> = g.series.iter().map(values).collect();
    let mut out = vec![vec![None; n]; all.len()];
    if !grouping.is_stacked() {
        for (row, vals) in out.iter_mut().zip(&all) {
            for (i, slot) in row.iter_mut().enumerate() {
                *slot = vals.get(i).copied().flatten().map(|v| (0.0, v));
            }
        }
        return out;
    }
    let percent = grouping == Grouping::PercentStacked;
    let bar = matches!(g.kind, GroupKind::Bar { .. });
    #[allow(clippy::needless_range_loop)]
    for i in 0..n {
        let total: f64 = all
            .iter()
            .filter_map(|v| v.get(i).copied().flatten())
            .map(f64::abs)
            .sum();
        let (mut pos, mut neg) = (0.0, 0.0);
        for (si, vals) in all.iter().enumerate() {
            let Some(mut v) = vals.get(i).copied().flatten() else {
                continue;
            };
            if percent {
                v = if total > 0.0 { v / total } else { 0.0 };
            }
            if bar && v < 0.0 {
                out[si][i] = Some((neg, neg + v));
                neg += v;
            } else {
                out[si][i] = Some((pos, pos + v));
                pos += v;
            }
        }
    }
    out
}

fn data_extent(g: &ChartGroup) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for s in stacked(g) {
        for (b, t) in s.into_iter().flatten() {
            lo = lo.min(b.min(t));
            hi = hi.max(b.max(t));
        }
    }
    (lo <= hi).then_some((lo, hi))
}

fn xy_values(s: &Series, n: usize) -> Vec<(Option<f64>, Option<f64>)> {
    let ys = values(s);
    let has_x = s
        .categories
        .as_ref()
        .is_some_and(|c| (0..c.len()).any(|i| c.number(i).is_some()));
    (0..n)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let x = if has_x {
                s.categories.as_ref().and_then(|c| c.number(i))
            } else {
                Some(i as f64 + 1.0)
            };
            (x, ys.get(i).copied().flatten())
        })
        .collect()
}

fn hierarchy_roots(s: &Series) -> Vec<String> {
    let mut roots: Vec<String> = Vec::new();
    if let Some(DataSource::MultiLevel(m)) = &s.categories {
        let top = m.levels.len().saturating_sub(1);
        let mut last: Option<String> = None;
        if let Some(level) = m.levels.get(top) {
            for p in level {
                let name = p.clone().or_else(|| last.clone()).unwrap_or_default();
                last = Some(name.clone());
                if !roots.contains(&name) {
                    roots.push(name);
                }
            }
        }
    } else if let Some(c) = &s.categories {
        for i in 0..c.len() {
            let name = c.label(i).unwrap_or_default();
            if !roots.contains(&name) {
                roots.push(name);
            }
        }
    }
    roots
}

// ───────────────────────────────────────────────────────────── axes

fn find_axis<'a>(ctx: &'a Ctx<'_>, id: i64) -> Option<&'a Axis> {
    ctx.cs.chart.plot_area.axes.iter().find(|a| a.id == id)
}

/// The geometry of one pair of axes and how groups map onto it.
#[derive(Debug, Clone)]
struct Frame {
    /// Category (or x) axis id, value (or y) axis id.
    x_id: i64,
    y_id: i64,
    /// Categories run vertically (bar charts).
    swapped: bool,
    /// Scatter/bubble: both axes are value axes.
    xy: bool,
    /// Number of categories.
    n: usize,
    /// Categories sit between tick marks.
    between: bool,
    /// Category axis reversed.
    cat_reversed: bool,
    /// The value scale.
    val: Scale,
    /// The x scale of an X–Y frame.
    xs: Option<Scale>,
    /// Value label format.
    val_format: String,
    x_format: String,
    secondary: bool,
}

impl Frame {
    /// Fraction along the category axis of category `i` (centre).
    fn cat_t(&self, i: f64) -> f64 {
        let n = self.n.max(1) as f64;
        let t = if self.between {
            (i + 0.5) / n
        } else if self.n <= 1 {
            0.5
        } else {
            i / (n - 1.0)
        };
        if self.cat_reversed { 1.0 - t } else { t }
    }

    /// Width of one category band as a fraction.
    fn band(&self) -> f64 {
        let n = self.n.max(1) as f64;
        if self.between {
            1.0 / n
        } else {
            1.0 / (n - 1.0).max(1.0)
        }
    }

    /// A point in chart px from category/x fraction and value fraction.
    #[allow(clippy::cast_possible_truncation)]
    fn at(&self, plot: Bounds, cat_t: f64, val_t: f64) -> (f32, f32) {
        if self.swapped {
            (
                plot.x + (val_t as f32) * plot.w,
                plot.bottom() - (cat_t as f32) * plot.h,
            )
        } else {
            (
                plot.x + (cat_t as f32) * plot.w,
                plot.bottom() - (val_t as f32) * plot.h,
            )
        }
    }

    /// Where the category axis crosses the value axis (the bars' base), as a value.
    fn baseline(&self, cat_axis: Option<&Axis>) -> f64 {
        let s = &self.val;
        let v = match cat_axis.and_then(|a| a.crosses) {
            Some(Crosses::Min) => s.min,
            Some(Crosses::Max) => s.max,
            Some(Crosses::At(v)) => v,
            Some(Crosses::AutoZero) | None => {
                if s.log_base.is_some() {
                    s.min
                } else {
                    0.0
                }
            }
        };
        v.clamp(s.min, s.max)
    }
}

/// Side of the plot an axis is drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

fn axis_side(horizontal: bool, own: Option<&Axis>, crossing_reversed: bool) -> Side {
    let at_max = matches!(own.and_then(|a| a.crosses), Some(Crosses::Max)) != crossing_reversed;
    match (horizontal, at_max) {
        (true, false) => Side::Bottom,
        (true, true) => Side::Top,
        (false, false) => Side::Left,
        (false, true) => Side::Right,
    }
}

fn group_order(k: &GroupKind) -> u8 {
    match k {
        GroupKind::Area { .. } => 0,
        GroupKind::Bar { .. } => 1,
        GroupKind::Radar { .. } => 2,
        GroupKind::Line { .. } => 3,
        GroupKind::Bubble { .. } => 4,
        GroupKind::Scatter { .. } => 5,
        _ => 6,
    }
}

/// Lays out the plot area: the axes and every group's marks.
pub(crate) fn layout_plot(
    ctx: &mut Ctx<'_>,
    outer: Bounds,
    inner_fixed: bool,
    out: &mut ChartLayout,
) {
    let groups = &ctx.cs.chart.plot_area.groups;
    if let Some(gi) = groups.iter().position(|g| g.kind.is_circular()) {
        out.plot = outer;
        plot_area_background(ctx, outer, out);
        match &groups[gi].kind {
            GroupKind::Hierarchy {
                layout: HierarchyLayout::Treemap,
                ..
            } => treemap(ctx, gi, outer, out),
            GroupKind::Hierarchy {
                layout: HierarchyLayout::Sunburst,
                ..
            } => sunburst(ctx, gi, outer, out),
            _ => pie(ctx, gi, outer, out),
        }
        return;
    }
    if let Some(gi) = groups
        .iter()
        .position(|g| matches!(g.kind, GroupKind::Radar { .. }))
    {
        out.plot = outer;
        plot_area_background(ctx, outer, out);
        radar(ctx, gi, outer, out);
        return;
    }
    axis_plot(ctx, outer, inner_fixed, out);
}

fn plot_area_background(ctx: &Ctx<'_>, plot: Bounds, out: &mut ChartLayout) {
    let sp = &ctx.cs.chart.plot_area.shape;
    let fill = fill_paint(sp, ctx.colors).flatten();
    let stroke = line_stroke(sp.line.as_ref(), ctx.colors, ctx.colors.gridline(), 1.0).flatten();
    if fill.is_some() || stroke.is_some() {
        out.marks.push(Mark {
            element: ChartElement::PlotArea,
            path: PathData::rect(plot),
            fill,
            stroke,
        });
    }
    out.hits
        .push((ChartElement::PlotArea, HitShape::Rect(plot)));
}

/// Text style of an axis' tick labels.
fn tick_style(ctx: &Ctx<'_>, a: Option<&Axis>) -> TextStyle {
    let props: Vec<&TextProps> = a.map(|a| &a.text).into_iter().collect();
    ctx.style(&props, 1.0, false, ctx.colors.text())
}

struct AxisPlan {
    id: i64,
    horizontal: bool,
    side: Side,
    visible: bool,
    labels: bool,
    /// Tick labels to draw: text and position fraction.
    ticks: Vec<(String, f64)>,
    /// Gridline fractions.
    grid_major: Vec<f64>,
    grid_minor: Vec<f64>,
    style: TextStyle,
    rotation: f32,
    /// Show every n-th label.
    skip: usize,
    /// Size reserved for labels perpendicular to the axis.
    label_extent: f32,
    title: Option<(String, TextStyle)>,
    title_extent: f32,
    is_value: bool,
    /// How far rotated labels reach left of the axis start.
    overhang: f32,
}

fn build_frames(ctx: &Ctx<'_>, length_hint: (f32, f32)) -> Vec<(Frame, Vec<usize>)> {
    let groups = &ctx.cs.chart.plot_area.groups;
    let mut frames: Vec<(Frame, Vec<usize>)> = Vec::new();
    for (gi, g) in groups.iter().enumerate() {
        if g.kind.is_circular() || matches!(g.kind, GroupKind::Other(_)) {
            continue;
        }
        let (x_id, y_id) = match g.axis_ids.as_slice() {
            [a, b, ..] => (*a, *b),
            _ => (i64::MIN, i64::MIN + 1),
        };
        if let Some((_, members)) = frames
            .iter_mut()
            .find(|(f, _)| f.x_id == x_id && f.y_id == y_id)
        {
            members.push(gi);
            continue;
        }
        let swapped = matches!(
            g.kind,
            GroupKind::Bar {
                dir: BarDir::Bar,
                ..
            }
        );
        let xy = g.kind.is_xy();
        frames.push((
            Frame {
                x_id,
                y_id,
                swapped,
                xy,
                n: 0,
                between: true,
                cat_reversed: false,
                val: value_scale(0.0, 1.0, None, 5, false),
                xs: None,
                val_format: "General".into(),
                x_format: "General".into(),
                secondary: !frames.is_empty(),
            },
            vec![gi],
        ));
    }
    for (f, members) in &mut frames {
        let cat_axis = find_axis(ctx, f.x_id);
        let val_axis = find_axis(ctx, f.y_id);
        f.n = members
            .iter()
            .map(|&gi| point_count(&groups[gi]))
            .max()
            .unwrap_or(0);
        f.cat_reversed = cat_axis.is_some_and(Axis::is_reversed);
        let has_area = members
            .iter()
            .any(|&gi| matches!(groups[gi].kind, GroupKind::Area { .. }));
        f.between = val_axis.and_then(|a| a.cross_between).unwrap_or(!has_area);
        let percent = members
            .iter()
            .any(|&gi| groups[gi].kind.grouping() == Some(Grouping::PercentStacked));
        let val_len = if f.swapped {
            length_hint.0
        } else {
            length_hint.1
        };
        let max_intervals = ((val_len / 34.0).floor() as usize).clamp(2, 12);
        let data_fmt = members.iter().find_map(|&gi| {
            groups[gi].series.iter().find_map(|s| match &s.values {
                Some(DataSource::Num(n)) => n.format_code.clone(),
                _ => None,
            })
        });
        if f.xy {
            let (mut xlo, mut xhi, mut ylo, mut yhi) = (
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            );
            for &gi in members.iter() {
                let g = &groups[gi];
                let n = point_count(g);
                for s in &g.series {
                    for (x, y) in xy_values(s, n) {
                        if let (Some(x), Some(y)) = (x, y) {
                            xlo = xlo.min(x);
                            xhi = xhi.max(x);
                            ylo = ylo.min(y);
                            yhi = yhi.max(y);
                        }
                    }
                }
            }
            let x_intervals = ((length_hint.0 / 60.0).floor() as usize).clamp(2, 12);
            f.xs = Some(value_scale(xlo, xhi, cat_axis, x_intervals, false));
            f.val = value_scale(ylo, yhi, val_axis, max_intervals, false);
            let x_fmt = members.iter().find_map(|&gi| {
                groups[gi].series.iter().find_map(|s| match &s.categories {
                    Some(DataSource::Num(n)) => n.format_code.clone(),
                    _ => None,
                })
            });
            f.x_format = axis_format(cat_axis, x_fmt.as_deref(), false);
        } else {
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for &gi in members.iter() {
                if let Some((a, b)) = data_extent(&groups[gi]) {
                    lo = lo.min(a);
                    hi = hi.max(b);
                }
            }
            f.val = value_scale(lo, hi, val_axis, max_intervals, percent);
        }
        f.val_format = axis_format(val_axis, data_fmt.as_deref(), percent);
    }
    frames
}

fn axis_plot(ctx: &mut Ctx<'_>, outer: Bounds, inner_fixed: bool, out: &mut ChartLayout) {
    let mut frames = build_frames(ctx, (outer.w * 0.8, outer.h * 0.8));
    if frames.is_empty() {
        out.plot = outer;
        plot_area_background(ctx, outer, out);
        return;
    }
    let mut plot = outer;
    for pass in 0..2 {
        let plans = plan_axes(ctx, &frames, plot);
        let mut reserve = [0.0f32; 4]; // left, right, top, bottom
        for p in &plans {
            let total = p.label_extent + p.title_extent;
            let i = match p.side {
                Side::Left => 0,
                Side::Right => 1,
                Side::Top => 2,
                Side::Bottom => 3,
            };
            reserve[i] = reserve[i].max(total);
            reserve[0] = reserve[0].max(p.overhang.min(outer.w * 0.3));
        }
        if !inner_fixed {
            // Half a label of room at the ends of horizontal axes so the first and last labels
            // are not cut.
            plot = Bounds::new(
                outer.x + reserve[0],
                outer.y + reserve[2].max(4.0),
                (outer.w - reserve[0] - reserve[1] - 4.0).max(4.0),
                (outer.h - reserve[2].max(4.0) - reserve[3]).max(4.0),
            );
        }
        if pass == 0 {
            frames = build_frames(ctx, (plot.w, plot.h));
        } else {
            out.plot = plot;
            plot_area_background(ctx, plot, out);
            draw_gridlines(ctx, &frames, &plans, plot, out);
            let mut order: Vec<(usize, usize)> = frames
                .iter()
                .enumerate()
                .flat_map(|(fi, (_, m))| m.iter().map(move |gi| (fi, *gi)))
                .collect();
            order.sort_by_key(|(_, gi)| group_order(&ctx.cs.chart.plot_area.groups[*gi].kind));
            let mut labels = LabelPlacer::default();
            for (fi, gi) in order {
                let f = &frames[fi].0;
                draw_group(ctx, gi, f, plot, out, &mut labels);
            }
            draw_axes(ctx, &frames, &plans, plot, out);
        }
    }
}

fn plan_axes(ctx: &mut Ctx<'_>, frames: &[(Frame, Vec<usize>)], plot: Bounds) -> Vec<AxisPlan> {
    let mut plans = Vec::new();
    for (f, members) in frames {
        let groups = &ctx.cs.chart.plot_area.groups;
        let cat_axis = find_axis(ctx, f.x_id).cloned();
        let val_axis = find_axis(ctx, f.y_id).cloned();
        let val_reversed = val_axis.as_ref().is_some_and(Axis::is_reversed);
        // Category / x axis.
        {
            let horizontal = !f.swapped;
            let a = cat_axis.as_ref();
            let side = axis_side(horizontal, a, val_reversed);
            let style = tick_style(ctx, a);
            let mut ticks = Vec::new();
            let mut grid_major = Vec::new();
            let mut grid_minor = Vec::new();
            if let Some(xs) = &f.xs {
                for t in xs.ticks() {
                    ticks.push((fmt(t, &f.x_format, ctx.cs), xs.fraction(t)));
                    grid_major.push(xs.fraction(t));
                }
                grid_minor = xs.minor_ticks().iter().map(|t| xs.fraction(*t)).collect();
            } else {
                let g0 = members.first().map(|gi| &groups[*gi]);
                let labels = g0.map(|g| category_labels(g, f.n)).unwrap_or_default();
                let cat_fmt = g0.and_then(|g| {
                    g.series.iter().find_map(|s| match &s.categories {
                        Some(DataSource::Num(n)) => n.format_code.clone(),
                        _ => None,
                    })
                });
                let custom = a
                    .and_then(|a| a.num_fmt.as_ref())
                    .filter(|nf| nf.source_linked != Some(true))
                    .map(|nf| nf.code.clone());
                for (i, l) in labels.iter().enumerate() {
                    #[allow(clippy::cast_precision_loss)]
                    let t = f.cat_t(i as f64);
                    let text = match (
                        &custom,
                        g0.and_then(|g| g.series.iter().find_map(|s| s.categories.as_ref()))
                            .and_then(|c| c.number(i)),
                    ) {
                        (Some(code), Some(v)) => fmt(v, code, ctx.cs),
                        (None, Some(v)) if cat_fmt.is_some() => {
                            fmt(v, cat_fmt.as_deref().unwrap_or("General"), ctx.cs)
                        }
                        _ => l.clone(),
                    };
                    ticks.push((text, t));
                }
                let n = f.n.max(1);
                for i in 0..=n {
                    #[allow(clippy::cast_precision_loss)]
                    let t = if f.between {
                        i as f64 / n as f64
                    } else {
                        f.cat_t(i as f64)
                    };
                    if f.between || i < n {
                        grid_major.push(t);
                    }
                }
            }
            plans.push(finish_plan(
                ctx,
                a,
                f.x_id,
                horizontal,
                side,
                ticks,
                grid_major,
                grid_minor,
                style,
                plot,
                f.xs.is_none(),
                f.secondary,
            ));
        }
        // Value / y axis.
        {
            let horizontal = f.swapped;
            let a = val_axis.as_ref();
            let side = axis_side(horizontal, a, f.cat_reversed);
            let style = tick_style(ctx, a);
            let ticks: Vec<(String, f64)> = f
                .val
                .ticks()
                .into_iter()
                .map(|t| (fmt(t, &f.val_format, ctx.cs), f.val.fraction(t)))
                .collect();
            let grid_major: Vec<f64> = ticks.iter().map(|(_, t)| *t).collect();
            let grid_minor: Vec<f64> = f
                .val
                .minor_ticks()
                .into_iter()
                .map(|t| f.val.fraction(t))
                .collect();
            plans.push(finish_plan(
                ctx,
                a,
                f.y_id,
                horizontal,
                side,
                ticks,
                grid_major,
                grid_minor,
                style,
                plot,
                false,
                f.secondary,
            ));
        }
    }
    plans
}

#[allow(clippy::too_many_arguments)]
fn finish_plan(
    ctx: &mut Ctx<'_>,
    a: Option<&Axis>,
    id: i64,
    horizontal: bool,
    side: Side,
    ticks: Vec<(String, f64)>,
    grid_major: Vec<f64>,
    grid_minor: Vec<f64>,
    style: TextStyle,
    plot: Bounds,
    category: bool,
    secondary: bool,
) -> AxisPlan {
    let visible = a.map_or(!secondary, Axis::is_visible);
    let labels = visible && a.and_then(|a| a.tick_label_pos) != Some(TickLabelPos::None);
    let mut rotation = a
        .and_then(|a| a.text.rotation)
        .filter(|r| *r != -60_000_000 && r.abs() <= 5_400_000)
        .map_or(0.0, |r| {
            #[allow(clippy::cast_precision_loss)]
            let deg = r as f32 / 60_000.0;
            deg
        });
    let mut skip = a
        .and_then(|a| a.tick_label_skip)
        .map_or(1, |s| (s as usize).max(1));
    let mut label_extent = 0.0;
    if labels && !ticks.is_empty() {
        let widths: Vec<f32> = ticks
            .iter()
            .map(|(t, _)| ctx.text.width(t, &style))
            .collect();
        let max_w = widths.iter().copied().fold(0.0f32, f32::max);
        let line_h = style.size * 1.25;
        if horizontal {
            let n = ticks.len().max(1);
            #[allow(clippy::cast_precision_loss)]
            let slot = plot.w / n as f32;
            if category && a.and_then(|a| a.tick_label_skip).is_none() && rotation == 0.0 {
                // Excel: rotate when labels collide, then skip labels until they fit.
                if max_w > slot * 0.95 {
                    if max_w > slot * 1.3 && line_h * 1.4 < slot * 2.0 {
                        rotation = -45.0;
                    }
                    // Rotated labels need their baselines a line apart, measured across the
                    // slant.
                    let needed = if rotation == 0.0 {
                        max_w * 1.05
                    } else {
                        line_h * 1.15 / rotation.abs().to_radians().sin().max(0.2)
                    };
                    skip = ((needed / slot.max(1.0)).ceil() as usize).max(1);
                }
            } else if !category {
                // Value labels never collide in practice (the tick count follows the length).
                skip = 1;
            }
            label_extent = if rotation == 0.0 {
                line_h
            } else {
                let r = rotation.abs().to_radians();
                (max_w.min(plot.h * 0.45) * r.sin() + line_h * r.cos()).max(line_h)
            } + 4.0;
        } else {
            let n = ticks.len().max(1);
            #[allow(clippy::cast_precision_loss)]
            let slot = plot.h / n as f32;
            if category && a.and_then(|a| a.tick_label_skip).is_none() && line_h > slot {
                skip = ((line_h / slot.max(1.0)).ceil() as usize).max(1);
            }
            label_extent = max_w.min(plot.w * 0.4) + 6.0;
        }
    }
    let title = a
        .and_then(|a| a.title.as_ref())
        .filter(|_| visible)
        .map(|t| {
            let text = t
                .text
                .as_ref()
                .and_then(TitleText::plain)
                .unwrap_or("Axis Title")
                .to_owned();
            let style = ctx.style(&[&t.text_props], 10.0 / 9.0, false, ctx.colors.text());
            (text, style)
        });
    let title_extent = title.as_ref().map_or(0.0, |(_, s)| s.size * 1.3 + 6.0);
    let overhang = if horizontal && rotation != 0.0 && labels {
        let first_w = ticks
            .first()
            .map_or(0.0, |(t, _)| ctx.text.width(t, &style))
            .min(plot.h * 0.45);
        let first_t = ticks.first().map_or(0.0, |(_, t)| *t as f32);
        (first_w * rotation.abs().to_radians().cos() - first_t * plot.w).max(0.0)
    } else {
        0.0
    };
    AxisPlan {
        id,
        horizontal,
        side,
        visible,
        labels,
        ticks,
        grid_major,
        grid_minor,
        style,
        rotation,
        skip,
        label_extent,
        title,
        title_extent,
        is_value: !category,
        overhang,
    }
}

#[allow(clippy::cast_possible_truncation)]
fn draw_gridlines(
    ctx: &Ctx<'_>,
    frames: &[(Frame, Vec<usize>)],
    plans: &[AxisPlan],
    plot: Bounds,
    out: &mut ChartLayout,
) {
    for p in plans {
        let Some(axis) = find_axis(ctx, p.id) else {
            continue;
        };
        let _ = frames;
        for (major, grid, sp) in [
            (false, &p.grid_minor, axis.minor_gridlines.as_ref()),
            (true, &p.grid_major, axis.major_gridlines.as_ref()),
        ] {
            let Some(sp) = sp else { continue };
            let auto = if major {
                ctx.colors.gridline()
            } else {
                transform(ctx.colors.gridline(), 100_000, 0)
            };
            let Some(stroke) = line_stroke(sp.line.as_ref(), ctx.colors, auto, 1.0)
                .unwrap_or_else(|| Some(solid(to_paint(auto), if major { 1.0 } else { 0.75 })))
            else {
                continue;
            };
            let element = if major {
                ChartElement::MajorGridlines(p.id)
            } else {
                ChartElement::MinorGridlines(p.id)
            };
            let mut path = PathData::new();
            let mut hit_points = Vec::new();
            for &t in grid
                .iter()
                .filter(|t| t.is_finite() && (-1e-6..=1.0 + 1e-6).contains(*t))
            {
                let t = t as f32;
                // Gridlines of a horizontal axis are vertical lines.
                let (a, b) = if p.horizontal {
                    let x = snap_px(plot.x + t * plot.w, stroke.width);
                    ((x, plot.y), (x, plot.bottom()))
                } else {
                    let y = snap_px(plot.bottom() - t * plot.h, stroke.width);
                    ((plot.x, y), (plot.right(), y))
                };
                path.move_to(a.0, a.1).line_to(b.0, b.1);
                hit_points.push(vec![a, b]);
            }
            if path.is_empty() {
                continue;
            }
            out.marks.push(Mark {
                element,
                path,
                fill: None,
                stroke: Some(stroke),
            });
            for pts in hit_points {
                out.hits.push((
                    element,
                    HitShape::Polyline {
                        points: pts,
                        tolerance: 2.5,
                    },
                ));
            }
        }
    }
}

/// Puts a hairline on a pixel centre so it stays crisp.
fn snap_px(v: f32, width: f32) -> f32 {
    if width <= 1.0 + 1e-3 {
        v.floor() + 0.5
    } else {
        v
    }
}

#[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
fn draw_axes(
    ctx: &mut Ctx<'_>,
    frames: &[(Frame, Vec<usize>)],
    plans: &[AxisPlan],
    plot: Bounds,
    out: &mut ChartLayout,
) {
    for p in plans {
        let axis = find_axis(ctx, p.id).cloned();
        let frame = frames
            .iter()
            .map(|(f, _)| f)
            .find(|f| f.x_id == p.id || f.y_id == p.id);
        // Where the axis line sits: at the crossing value of the other axis.
        let pos = match p.side {
            Side::Left => plot.x,
            Side::Right => plot.right(),
            Side::Top => plot.y,
            Side::Bottom => plot.bottom(),
        };
        let mut line_pos = pos;
        if let Some(f) = frame
            && !p.is_value
            && f.xs.is_none()
            && !matches!(axis.as_ref().and_then(|a| a.crosses), Some(Crosses::Max))
        {
            // A category axis crosses the value axis at zero.
            let cat_axis = find_axis(ctx, f.x_id);
            let base = f.val.fraction(f.baseline(cat_axis)) as f32;
            if base.is_finite() {
                line_pos = if p.horizontal {
                    plot.bottom() - base * plot.h
                } else {
                    plot.x + base * plot.w
                };
            }
        }
        if let Some(f) = frame
            && f.xs.is_some()
            && p.id == f.x_id
            && let Some(ya) = find_axis(ctx, f.y_id)
            && !matches!(
                axis.as_ref().and_then(|a| a.crosses),
                Some(Crosses::Max | Crosses::Min)
            )
            && ya.scaling.log_base.is_none()
        {
            let base = f.val.fraction(0.0f64.clamp(f.val.min, f.val.max)) as f32;
            line_pos = plot.bottom() - base * plot.h;
        }
        if let Some(f) = frame
            && f.xs.is_some()
            && p.id == f.y_id
            && let Some(xs) = &f.xs
            && !matches!(
                axis.as_ref().and_then(|a| a.crosses),
                Some(Crosses::Max | Crosses::Min)
            )
            && xs.log_base.is_none()
        {
            let base = xs.fraction(0.0f64.clamp(xs.min, xs.max)) as f32;
            line_pos = plot.x + base * plot.w;
        }
        let (a0, a1) = if p.horizontal {
            let y = snap_px(line_pos, 1.0);
            ((plot.x, y), (plot.right(), y))
        } else {
            let x = snap_px(line_pos, 1.0);
            ((x, plot.bottom()), (x, plot.y))
        };
        out.axes.push(AxisLayout {
            id: p.id,
            horizontal: p.horizontal,
            line: (a0, a1),
            scale: frame.and_then(|f| if p.id == f.y_id { Some(f.val) } else { f.xs }),
        });
        if !p.visible {
            continue;
        }
        let a = axis.as_ref();
        // Axis line: automatic for category axes; value axes follow Office's modern default
        // (no line) on screen.
        let auto_line = !p.is_value || !ctx.colors.uses_kiln_palette();
        let stroke = line_stroke(
            a.and_then(|a| a.shape.line.as_ref()),
            ctx.colors,
            ctx.colors.axis_line(),
            1.0,
        )
        .unwrap_or_else(|| auto_line.then(|| solid(to_paint(ctx.colors.axis_line()), 1.0)));
        let element = ChartElement::Axis(p.id);
        if let Some(st) = &stroke {
            let mut path = PathData::new();
            path.move_to(a0.0, a0.1).line_to(a1.0, a1.1);
            // Tick marks.
            let tick_len = 5.0;
            let major = a.and_then(|a| a.major_tick).unwrap_or(TickMark::None);
            let (inside, outside) = match major {
                TickMark::Out => (0.0, tick_len),
                TickMark::In => (tick_len, 0.0),
                TickMark::Cross => (tick_len, tick_len),
                TickMark::None => (0.0, 0.0),
            };
            if inside + outside > 0.0 {
                let dir = match p.side {
                    Side::Bottom | Side::Right => 1.0,
                    Side::Top | Side::Left => -1.0,
                };
                for &t in &p.grid_major {
                    let t = t as f32;
                    if !(-1e-4..=1.0001).contains(&t) {
                        continue;
                    }
                    if p.horizontal {
                        let x = snap_px(plot.x + t * plot.w, 1.0);
                        path.move_to(x, a0.1 - inside * dir)
                            .line_to(x, a0.1 + outside * dir);
                    } else {
                        let y = snap_px(plot.bottom() - t * plot.h, 1.0);
                        path.move_to(a0.0 - inside * dir, y)
                            .line_to(a0.0 + outside * dir, y);
                    }
                }
            }
            out.marks.push(Mark {
                element,
                path,
                fill: None,
                stroke: Some(st.clone()),
            });
        }
        out.hits.push((
            element,
            HitShape::Polyline {
                points: vec![a0, a1],
                tolerance: 3.0,
            },
        ));
        // Tick labels.
        if p.labels {
            let low_high = a.and_then(|a| a.tick_label_pos);
            let base = match (low_high, p.side) {
                (Some(TickLabelPos::Low), Side::Bottom | Side::Top) => plot.bottom(),
                (Some(TickLabelPos::Low), _) => plot.x,
                (Some(TickLabelPos::High), Side::Bottom | Side::Top) => plot.y,
                (Some(TickLabelPos::High), _) => plot.right(),
                _ => {
                    if p.horizontal {
                        a0.1
                    } else {
                        a0.0
                    }
                }
            };
            let mut last_end = f32::NEG_INFINITY;
            for (k, (text, t)) in p.ticks.iter().enumerate() {
                if k % p.skip != 0 {
                    continue;
                }
                let t = *t as f32;
                if !t.is_finite() || !(-1e-3..=1.001).contains(&t) {
                    continue;
                }
                let block = ctx.block(text, &p.style, 10_000.0, Alignment::Left);
                let (w, h) = (block.width, block.height);
                let (x, y, rot, bounds);
                if p.horizontal {
                    let cx = plot.x + t * plot.w;
                    let below = !matches!(p.side, Side::Top);
                    if p.rotation == 0.0 {
                        // Centred under the tick, but kept inside the chart.
                        let left = (cx - w / 2.0).clamp(1.0, (out.width - 1.0 - w).max(1.0));
                        x = left - block.left;
                        y = if below { base + 4.0 } else { base - 4.0 - h };
                        rot = 0.0;
                        bounds = Bounds::new(left, y, w, h);
                        // Value labels that would overlap the previous one are dropped.
                        if bounds.x < last_end + 2.0 {
                            continue;
                        }
                        last_end = bounds.right();
                    } else {
                        // Rotated labels hang from their end at the tick.
                        let r = p.rotation.to_radians();
                        let ext_x = w * r.cos().abs();
                        let ext_y = w * r.sin().abs() + h * r.cos().abs();
                        // Pivot is the text centre; place the centre so the text's end meets
                        // the tick.
                        let pcx = cx - ext_x / 2.0;
                        let pcy = if below {
                            base + 4.0 + ext_y / 2.0
                        } else {
                            base - 4.0 - ext_y / 2.0
                        };
                        x = pcx - w / 2.0 - block.left;
                        y = pcy - h / 2.0;
                        rot = p.rotation;
                        bounds = Bounds::new(pcx - ext_x / 2.0, pcy - ext_y / 2.0, ext_x, ext_y);
                    }
                } else {
                    let cy = plot.bottom() - t * plot.h;
                    let left = !matches!(p.side, Side::Right);
                    x = if left { base - 6.0 - w } else { base + 6.0 } - block.left;
                    y = cy - h / 2.0;
                    rot = 0.0;
                    bounds = Bounds::new(x + block.left, y, w, h);
                }
                out.texts.push(PlacedText {
                    element,
                    block,
                    x,
                    y,
                    rotation: rot,
                    bounds,
                });
                out.hits.push((element, HitShape::Rect(bounds)));
            }
        }
        // Axis title.
        if let Some((text, style)) = &p.title {
            let element = ChartElement::AxisTitle(p.id);
            let max = if p.horizontal { plot.w } else { plot.h };
            let block = ctx.block(text, style, max, Alignment::Center);
            let (w, h) = (block.width, block.height);
            let gap = p.label_extent + 2.0;
            let (x, y, rot, bounds) = match p.side {
                Side::Bottom => {
                    let y = plot.bottom() + gap;
                    let x = plot.x + (plot.w - block.layout.width) / 2.0;
                    (x, y, 0.0, Bounds::new(x + block.left, y, w, h))
                }
                Side::Top => {
                    let y = plot.y - gap - h;
                    let x = plot.x + (plot.w - block.layout.width) / 2.0;
                    (x, y, 0.0, Bounds::new(x + block.left, y, w, h))
                }
                Side::Left | Side::Right => {
                    let cx = if p.side == Side::Left {
                        plot.x - gap - h / 2.0
                    } else {
                        plot.right() + gap + h / 2.0
                    };
                    let cy = plot.y + plot.h / 2.0;
                    let x = cx - w / 2.0 - block.left;
                    let y = cy - h / 2.0;
                    let rot = if p.side == Side::Left { -90.0 } else { 90.0 };
                    (x, y, rot, Bounds::new(cx - h / 2.0, cy - w / 2.0, h, w))
                }
            };
            out.hits.push((element, HitShape::Rect(bounds)));
            out.texts.push(PlacedText {
                element,
                block,
                x,
                y,
                rotation: rot,
                bounds,
            });
        }
    }
}

// ───────────────────────────────────────────────────────────── data labels

/// Places data labels, nudging a label that would cover an earlier one.
#[derive(Default)]
struct LabelPlacer {
    taken: Vec<Bounds>,
}

impl LabelPlacer {
    fn place(&mut self, b: Bounds, area: Bounds) -> Bounds {
        let step = b.h + 1.0;
        for k in 0..7 {
            let dy = match k {
                0 => 0.0,
                k if k % 2 == 1 => -step * f32::from(((k + 1) / 2) as u8),
                k => step * f32::from((k / 2) as u8),
            };
            let cand = b.translate(0.0, dy);
            let inside = cand.y >= area.y - b.h && cand.bottom() <= area.bottom() + b.h;
            if inside && !self.taken.iter().any(|t| t.intersects(&cand)) {
                self.taken.push(cand);
                return cand;
            }
        }
        self.taken.push(b);
        b
    }
}

fn label_props<'a>(g: &'a ChartGroup, s: &'a Series) -> Option<&'a DataLabels> {
    s.data_labels
        .as_ref()
        .or(g.data_labels.as_ref())
        .filter(|d| d.shows_anything())
}

/// The text of a data label.
fn label_text(
    ctx: &Ctx<'_>,
    d: &DataLabels,
    s: &Series,
    point: usize,
    value: Option<f64>,
    percent: Option<f64>,
    bubble: Option<f64>,
) -> Option<String> {
    let mut parts = Vec::new();
    if d.show_series == Some(true)
        && let Some(n) = s.name.as_ref().and_then(SeriesText::text)
    {
        parts.push(n.to_owned());
    }
    if d.show_category == Some(true)
        && let Some(c) = s.categories.as_ref().and_then(|c| c.label(point))
    {
        parts.push(c);
    }
    let value_fmt = d
        .num_fmt
        .as_ref()
        .filter(|f| f.source_linked != Some(true))
        .map(|f| f.code.clone())
        .or_else(|| match &s.values {
            Some(DataSource::Num(n)) => n.format_code.clone(),
            _ => None,
        })
        .unwrap_or_else(|| "General".into());
    if d.show_value == Some(true)
        && let Some(v) = value
    {
        parts.push(fmt(v, &value_fmt, ctx.cs));
    }
    if d.show_percent == Some(true)
        && let Some(p) = percent
    {
        let code = d
            .num_fmt
            .as_ref()
            .filter(|f| f.source_linked != Some(true) && f.code.contains('%'))
            .map_or("0%", |f| f.code.as_str());
        parts.push(fmt(p, code, ctx.cs));
    }
    if d.show_bubble_size == Some(true)
        && let Some(b) = bubble
    {
        parts.push(fmt(b, "General", ctx.cs));
    }
    if parts.is_empty() {
        return None;
    }
    let sep = d.separator.clone().unwrap_or_else(|| ", ".into());
    Some(parts.join(&sep))
}

/// Where a label goes relative to its anchor.
#[derive(Debug, Clone, Copy)]
enum Anchor {
    /// Centred on the point.
    Center(f32, f32),
    /// Above (or below, when negative) a point.
    Above(f32, f32, bool),
    /// Right of a point.
    Right(f32, f32),
    /// Left of a point.
    Left(f32, f32),
    /// Centred on a point inside a bar segment. The label is dropped when it
    /// is longer than the segment along the value axis (`true` when that axis
    /// runs horizontally), as Excel does for slivers of a stacked bar.
    Within(f32, f32, Bounds, bool),
}

#[allow(clippy::too_many_arguments)]
fn push_label(
    ctx: &mut Ctx<'_>,
    out: &mut ChartLayout,
    placer: &mut LabelPlacer,
    d: &DataLabels,
    element: ChartElement,
    text: &str,
    anchor: Anchor,
    contrast_on: Option<Rgba>,
    area: Bounds,
) {
    let auto = match contrast_on {
        Some(bg) => {
            let lum = 0.2126 * bg.r + 0.7152 * bg.g + 0.0722 * bg.b;
            if lum > 150.0 {
                Rgba::rgb(43.0, 28.0, 20.0)
            } else {
                Rgba::rgb(255.0, 255.0, 255.0)
            }
        }
        None => ctx.colors.text(),
    };
    let style = ctx.style(
        &[&d.text],
        1.0,
        contrast_on.is_some() && ctx.colors.uses_kiln_palette(),
        auto,
    );
    let block = ctx.block(text, &style, 400.0, Alignment::Center);
    let (w, h) = (block.width, block.height);
    let gap = 3.0;
    let raw = match anchor {
        Anchor::Center(x, y) => Bounds::new(x - w / 2.0, y - h / 2.0, w, h),
        Anchor::Within(x, y, segment, horizontal) => {
            let fits = if horizontal {
                w <= segment.w
            } else {
                h <= segment.h
            };
            if !fits {
                return;
            }
            // Keep the label inside the segment along the value axis.
            let (x, y) = if horizontal {
                (x.clamp(segment.x + w / 2.0, segment.right() - w / 2.0), y)
            } else {
                (x, y.clamp(segment.y + h / 2.0, segment.bottom() - h / 2.0))
            };
            Bounds::new(x - w / 2.0, y - h / 2.0, w, h)
        }
        Anchor::Above(x, y, up) => {
            if up {
                Bounds::new(x - w / 2.0, y - gap - h, w, h)
            } else {
                Bounds::new(x - w / 2.0, y + gap, w, h)
            }
        }
        Anchor::Right(x, y) => Bounds::new(x + gap + 2.0, y - h / 2.0, w, h),
        Anchor::Left(x, y) => Bounds::new(x - gap - 2.0 - w, y - h / 2.0, w, h),
    };
    let b = if matches!(anchor, Anchor::Center(..) | Anchor::Within(..)) {
        raw
    } else {
        placer.place(raw, area)
    };
    let fill = fill_paint(&d.shape, ctx.colors).flatten();
    let stroke = line_stroke(d.shape.line.as_ref(), ctx.colors, ctx.colors.text(), 1.0).flatten();
    if fill.is_some() || stroke.is_some() {
        out.marks.push(Mark {
            element,
            path: PathData::rect(b.inflate(3.0, 1.0)),
            fill,
            stroke,
        });
    }
    out.hits.push((element, HitShape::Rect(b)));
    out.texts.push(PlacedText {
        element,
        x: b.x - block.left,
        y: b.y,
        block,
        rotation: 0.0,
        bounds: b,
    });
}

// ───────────────────────────────────────────────────────────── axis chart groups

fn scatter_lines_default(style: Option<ScatterStyle>) -> bool {
    !matches!(style, Some(ScatterStyle::Marker | ScatterStyle::None))
}

fn scatter_markers_default(style: Option<ScatterStyle>) -> bool {
    !matches!(
        style,
        Some(ScatterStyle::Line | ScatterStyle::Smooth | ScatterStyle::None)
    )
}

#[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
fn draw_group(
    ctx: &mut Ctx<'_>,
    gi: usize,
    f: &Frame,
    plot: Bounds,
    out: &mut ChartLayout,
    placer: &mut LabelPlacer,
) {
    let g = ctx.cs.chart.plot_area.groups[gi].clone();
    let cat_axis = find_axis(ctx, f.x_id).cloned();
    let base_v = f.baseline(cat_axis.as_ref());
    let stack = stacked(&g);
    match &g.kind {
        GroupKind::Bar {
            grouping,
            gap_width,
            overlap,
            ..
        } => {
            let n_series = g.series.len().max(1);
            let stacked_g = grouping.is_some_and(Grouping::is_stacked);
            let gap = f64::from(gap_width.unwrap_or(150).min(500)) / 100.0;
            let ov = if stacked_g {
                1.0
            } else {
                f64::from(overlap.unwrap_or(0).clamp(-100, 100)) / 100.0
            };
            let slots = if stacked_g {
                1.0
            } else {
                n_series as f64 - (n_series as f64 - 1.0) * ov
            };
            let band = f.band();
            let bar = band / (slots + gap);
            for (si, s) in g.series.iter().enumerate() {
                let offset = if stacked_g {
                    0.0
                } else {
                    si as f64 * bar * (1.0 - ov)
                };
                for (i, v) in stack[si].iter().enumerate() {
                    let Some((b, t)) = *v else { continue };
                    let center = f.cat_t(i as f64);
                    let dir = if f.cat_reversed { -1.0 } else { 1.0 };
                    let start = center - dir * (band / 2.0 - bar * gap / 2.0) + dir * offset;
                    let end = start + dir * bar;
                    let b = if stacked_g { b } else { base_v };
                    let (vb, vt) = (f.val.fraction(b), f.val.fraction(t));
                    if !vb.is_finite() || !vt.is_finite() {
                        continue;
                    }
                    let vb = vb.clamp(-0.01, 1.01);
                    let vt = vt.clamp(-0.01, 1.01);
                    let p0 = f.at(plot, start, vb);
                    let p1 = f.at(plot, end, vt);
                    let rect = Bounds::from_corners(
                        p0.0.min(p1.0),
                        p0.1.min(p1.1),
                        p0.0.max(p1.0),
                        p0.1.max(p1.1),
                    );
                    let mut look = fill_look(ctx, &g, s, Some(i), false);
                    let raw_value = values(s).get(i).copied().flatten();
                    if raw_value.is_some_and(|v| v < 0.0)
                        && s.points
                            .iter()
                            .find(|p| p.idx as usize == i)
                            .and_then(|p| p.invert_if_negative)
                            .or(s.invert_if_negative)
                            == Some(true)
                    {
                        look.fill = Some(to_paint(ctx.colors.background()));
                        look.stroke = Some(solid(to_paint(look.color), 1.0));
                    }
                    let element = ChartElement::Point {
                        group: gi,
                        series: si,
                        point: i,
                    };
                    out.marks.push(Mark {
                        element,
                        path: PathData::rect(rect),
                        fill: look.fill,
                        stroke: look.stroke,
                    });
                    out.hits.push((element, HitShape::Rect(rect)));
                    if let Some(d) = label_props(&g, s)
                        && let Some(text) = label_text(ctx, d, s, i, raw_value, None, None)
                    {
                        let positive = t >= b;
                        let pos = d.position.unwrap_or(if stacked_g {
                            LabelPosition::Center
                        } else {
                            LabelPosition::OutsideEnd
                        });
                        let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
                        let anchor = match (pos, f.swapped) {
                            (LabelPosition::Center, swapped) => {
                                Anchor::Within(cx, cy, rect, swapped)
                            }
                            (LabelPosition::InsideEnd, false) => Anchor::Within(
                                cx,
                                if positive {
                                    rect.y + 10.0
                                } else {
                                    rect.bottom() - 10.0
                                },
                                rect,
                                false,
                            ),
                            (LabelPosition::InsideBase, false) => Anchor::Within(
                                cx,
                                if positive {
                                    rect.bottom() - 10.0
                                } else {
                                    rect.y + 10.0
                                },
                                rect,
                                false,
                            ),
                            (LabelPosition::InsideEnd, true) => Anchor::Left(
                                if positive {
                                    rect.right()
                                } else {
                                    rect.x + 30.0
                                },
                                cy,
                            ),
                            (LabelPosition::InsideBase, true) => Anchor::Right(
                                if positive {
                                    rect.x
                                } else {
                                    rect.right() - 30.0
                                },
                                cy,
                            ),
                            (_, false) => Anchor::Above(
                                cx,
                                if positive { rect.y } else { rect.bottom() },
                                positive,
                            ),
                            (_, true) => {
                                if positive {
                                    Anchor::Right(rect.right(), cy)
                                } else {
                                    Anchor::Left(rect.x, cy)
                                }
                            }
                        };
                        let inside = matches!(anchor, Anchor::Center(..) | Anchor::Within(..));
                        push_label(
                            ctx,
                            out,
                            placer,
                            d,
                            ChartElement::DataLabels {
                                group: gi,
                                series: si,
                            },
                            &text,
                            anchor,
                            inside.then_some(look.color),
                            plot,
                        );
                    }
                }
                trendlines(ctx, &g, gi, si, s, f, plot, out);
            }
        }
        GroupKind::Line { .. } | GroupKind::Area { .. } => {
            let area = matches!(g.kind, GroupKind::Area { .. });
            let blanks = ctx.cs.chart.disp_blanks_as.unwrap_or(BlanksAs::Gap);
            let mut prev_tops: Option<Vec<Option<(f32, f32)>>> = None;
            for (si, s) in g.series.iter().enumerate() {
                let pts: Vec<Option<(f32, f32)>> = stack[si]
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let v = match (v, blanks) {
                            (Some((_, t)), _) => Some(*t),
                            (None, BlanksAs::Zero) => Some(0.0),
                            _ => None,
                        };
                        v.map(|t| {
                            f.at(
                                plot,
                                f.cat_t(i as f64),
                                f.val.fraction(t).clamp(-0.05, 1.05),
                            )
                        })
                    })
                    .collect();
                if area {
                    let base_pts: Vec<(f32, f32)> = match &prev_tops {
                        Some(prev) if g.kind.grouping().is_some_and(Grouping::is_stacked) => prev
                            .iter()
                            .enumerate()
                            .map(|(i, p)| {
                                p.unwrap_or_else(|| {
                                    f.at(plot, f.cat_t(i as f64), f.val.fraction(base_v))
                                })
                            })
                            .collect(),
                        _ => (0..pts.len())
                            .map(|i| f.at(plot, f.cat_t(i as f64), f.val.fraction(base_v)))
                            .collect(),
                    };
                    let tops: Vec<(f32, f32)> = pts
                        .iter()
                        .enumerate()
                        .map(|(i, p)| p.unwrap_or(base_pts[i]))
                        .collect();
                    if tops.len() >= 2 {
                        let mut path = PathData::new();
                        let mut poly = Vec::new();
                        for (i, p) in tops.iter().enumerate() {
                            if i == 0 {
                                path.move_to(p.0, p.1);
                            } else {
                                path.line_to(p.0, p.1);
                            }
                            poly.push(*p);
                        }
                        for p in base_pts.iter().rev() {
                            path.line_to(p.0, p.1);
                            poly.push(*p);
                        }
                        path.close();
                        let look = fill_look(ctx, &g, s, None, true);
                        let element = ChartElement::Series {
                            group: gi,
                            series: si,
                        };
                        out.marks.push(Mark {
                            element,
                            path,
                            fill: look.fill,
                            stroke: look.stroke,
                        });
                        out.hits.push((element, HitShape::Polygon(poly)));
                    }
                    prev_tops = Some(pts.clone());
                } else {
                    let stroke = line_look(ctx, &g, s, true);
                    let smooth = s.smooth == Some(true);
                    draw_polyline(out, gi, si, &pts, stroke, smooth, blanks == BlanksAs::Span);
                    let markers_default = !matches!(
                        g.kind,
                        GroupKind::Line {
                            markers: Some(false),
                            ..
                        }
                    );
                    for (i, p) in pts.iter().enumerate() {
                        if let Some((x, y)) = p
                            && let Some(m) = marker_look(ctx, &g, s, i, markers_default)
                        {
                            push_marker(
                                out,
                                ChartElement::Point {
                                    group: gi,
                                    series: si,
                                    point: i,
                                },
                                &m,
                                *x,
                                *y,
                            );
                        }
                    }
                }
                // Labels.
                if let Some(d) = label_props(&g, s) {
                    let raw = values(s);
                    for (i, p) in pts.iter().enumerate() {
                        let Some((x, y)) = p else { continue };
                        if let Some(text) =
                            label_text(ctx, d, s, i, raw.get(i).copied().flatten(), None, None)
                        {
                            let anchor = match d.position {
                                Some(LabelPosition::Center) => Anchor::Center(*x, *y),
                                Some(LabelPosition::Left) => Anchor::Left(*x, *y),
                                Some(LabelPosition::Top) => Anchor::Above(*x, *y - 3.0, true),
                                Some(LabelPosition::Bottom) => Anchor::Above(*x, *y + 3.0, false),
                                _ if area => Anchor::Center(*x, *y + 10.0),
                                _ => Anchor::Right(*x, *y),
                            };
                            push_label(
                                ctx,
                                out,
                                placer,
                                d,
                                ChartElement::DataLabels {
                                    group: gi,
                                    series: si,
                                },
                                &text,
                                anchor,
                                None,
                                plot,
                            );
                        }
                    }
                }
                trendlines(ctx, &g, gi, si, s, f, plot, out);
            }
        }
        GroupKind::Scatter { style } => {
            let n = point_count(&g);
            let xs = f.xs.unwrap_or(f.val);
            for (si, s) in g.series.iter().enumerate() {
                let pts: Vec<Option<(f32, f32)>> = xy_values(s, n)
                    .into_iter()
                    .map(|(x, y)| match (x, y) {
                        (Some(x), Some(y)) => {
                            let (fx, fy) = (xs.fraction(x), f.val.fraction(y));
                            (fx.is_finite() && fy.is_finite()).then(|| {
                                (
                                    plot.x + fx.clamp(-0.05, 1.05) as f32 * plot.w,
                                    plot.bottom() - fy.clamp(-0.05, 1.05) as f32 * plot.h,
                                )
                            })
                        }
                        _ => None,
                    })
                    .collect();
                let stroke = line_look(ctx, &g, s, scatter_lines_default(*style));
                let smooth = s.smooth.unwrap_or(matches!(
                    style,
                    Some(ScatterStyle::Smooth | ScatterStyle::SmoothMarker)
                ));
                draw_polyline(out, gi, si, &pts, stroke, smooth, false);
                for (i, p) in pts.iter().enumerate() {
                    if let Some((x, y)) = p
                        && let Some(m) = marker_look(ctx, &g, s, i, scatter_markers_default(*style))
                    {
                        push_marker(
                            out,
                            ChartElement::Point {
                                group: gi,
                                series: si,
                                point: i,
                            },
                            &m,
                            *x,
                            *y,
                        );
                    }
                }
                if let Some(d) = label_props(&g, s) {
                    let raw = values(s);
                    for (i, p) in pts.iter().enumerate() {
                        if let Some((x, y)) = p
                            && let Some(text) =
                                label_text(ctx, d, s, i, raw.get(i).copied().flatten(), None, None)
                        {
                            push_label(
                                ctx,
                                out,
                                placer,
                                d,
                                ChartElement::DataLabels {
                                    group: gi,
                                    series: si,
                                },
                                &text,
                                Anchor::Right(*x, *y),
                                None,
                                plot,
                            );
                        }
                    }
                }
                trendlines(ctx, &g, gi, si, s, f, plot, out);
            }
        }
        GroupKind::Bubble {
            scale,
            size_is_width,
            show_negative,
            ..
        } => {
            let n = point_count(&g);
            let xs = f.xs.unwrap_or(f.val);
            let max_size = g
                .series
                .iter()
                .flat_map(|s| {
                    let b = s.bubble_sizes.as_ref();
                    (0..n).filter_map(move |i| b.and_then(|b| b.number(i)))
                })
                .map(f64::abs)
                .fold(0.0f64, f64::max);
            let scale_f = f64::from(scale.unwrap_or(100).min(300)) / 100.0;
            let max_d = f64::from(plot.w.min(plot.h)) * 0.25 * scale_f;
            for (si, s) in g.series.iter().enumerate() {
                let sizes = s.bubble_sizes.as_ref();
                for (i, (x, y)) in xy_values(s, n).into_iter().enumerate() {
                    let (Some(x), Some(y)) = (x, y) else { continue };
                    let size = sizes.and_then(|b| b.number(i)).unwrap_or(1.0);
                    if size < 0.0 && show_negative != &Some(true) {
                        continue;
                    }
                    let rel = if max_size > 0.0 {
                        size.abs() / max_size
                    } else {
                        1.0
                    };
                    let d = if size_is_width == &Some(true) {
                        rel * max_d
                    } else {
                        rel.sqrt() * max_d
                    };
                    let (fx, fy) = (xs.fraction(x), f.val.fraction(y));
                    if !fx.is_finite() || !fy.is_finite() {
                        continue;
                    }
                    let cx = plot.x + fx as f32 * plot.w;
                    let cy = plot.bottom() - fy as f32 * plot.h;
                    let r = (d as f32 / 2.0).max(1.5);
                    let mut look = fill_look(ctx, &g, s, Some(i), false);
                    if ctx.colors.uses_kiln_palette() || look.stroke.is_none() {
                        // Translucent bubbles let overlaps read, with a crisp rim.
                        if let Some(Paint::Rgba(c)) = look.fill
                            && c & 0xFF == 0xFF
                            && s.shape.fill.is_none()
                        {
                            look.fill = Some(Paint::Rgba((c & 0xFFFF_FF00) | 0xC0));
                            look.stroke = Some(solid(Paint::Rgba(c), 1.0));
                        }
                    }
                    let element = ChartElement::Point {
                        group: gi,
                        series: si,
                        point: i,
                    };
                    out.marks.push(Mark {
                        element,
                        path: circle(cx, cy, r),
                        fill: look.fill,
                        stroke: look.stroke,
                    });
                    out.hits.push((
                        element,
                        HitShape::Wedge {
                            cx,
                            cy,
                            r0: 0.0,
                            r1: r,
                            start: 0.0,
                            sweep: TAU,
                        },
                    ));
                    if let Some(dl) = label_props(&g, s)
                        && let Some(text) = label_text(ctx, dl, s, i, Some(y), None, Some(size))
                    {
                        push_label(
                            ctx,
                            out,
                            placer,
                            dl,
                            ChartElement::DataLabels {
                                group: gi,
                                series: si,
                            },
                            &text,
                            Anchor::Center(cx, cy),
                            Some(look.color),
                            plot,
                        );
                    }
                }
            }
        }
        _ => {}
    }
}

fn draw_polyline(
    out: &mut ChartLayout,
    gi: usize,
    si: usize,
    pts: &[Option<(f32, f32)>],
    stroke: Option<Stroke>,
    smooth: bool,
    span: bool,
) {
    let element = ChartElement::Series {
        group: gi,
        series: si,
    };
    let mut segments: Vec<Vec<(f32, f32)>> = vec![Vec::new()];
    for p in pts {
        match p {
            Some(p) => {
                if let Some(seg) = segments.last_mut() {
                    seg.push(*p);
                }
            }
            None if !span && segments.last().is_some_and(|s| !s.is_empty()) => {
                segments.push(Vec::new());
            }
            None => {}
        }
    }
    let Some(stroke) = stroke else {
        for seg in segments.into_iter().filter(|s| s.len() >= 2) {
            out.hits.push((
                element,
                HitShape::Polyline {
                    points: seg,
                    tolerance: 4.0,
                },
            ));
        }
        return;
    };
    let mut path = PathData::new();
    for seg in &segments {
        if seg.len() < 2 {
            continue;
        }
        path.move_to(seg[0].0, seg[0].1);
        if smooth && seg.len() > 2 {
            // Catmull–Rom through the points as cubic Béziers (Office's smoothing is close to
            // tension 1/6).
            for i in 0..seg.len() - 1 {
                let p0 = if i == 0 { seg[0] } else { seg[i - 1] };
                let (p1, p2) = (seg[i], seg[i + 1]);
                let p3 = if i + 2 < seg.len() { seg[i + 2] } else { p2 };
                let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
                let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
                path.cubic_to(c1.0, c1.1, c2.0, c2.1, p2.0, p2.1);
            }
        } else {
            for p in &seg[1..] {
                path.line_to(p.0, p.1);
            }
        }
    }
    if !path.is_empty() {
        out.marks.push(Mark {
            element,
            path,
            fill: None,
            stroke: Some(stroke.clone()),
        });
    }
    for seg in segments.into_iter().filter(|s| s.len() >= 2) {
        out.hits.push((
            element,
            HitShape::Polyline {
                points: seg,
                tolerance: (stroke.width / 2.0 + 3.0).max(4.0),
            },
        ));
    }
}

// ───────────────────────────────────────────────────────────── trendlines

/// Fits a trendline to (x, y) points; returns y(x).
fn fit(
    kind: TrendlineKind,
    order: u32,
    intercept: Option<f64>,
    pts: &[(f64, f64)],
) -> Option<Box<dyn Fn(f64) -> f64>> {
    let n = pts.len();
    if n < 2 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let nf = n as f64;
    let linreg = |xs: &[f64], ys: &[f64]| -> Option<(f64, f64)> {
        let mx = xs.iter().sum::<f64>() / nf;
        let my = ys.iter().sum::<f64>() / nf;
        let sxx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
        if sxx <= 0.0 {
            return None;
        }
        let sxy: f64 = xs.iter().zip(ys).map(|(x, y)| (x - mx) * (y - my)).sum();
        let b = sxy / sxx;
        Some((my - b * mx, b))
    };
    let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
    match kind {
        TrendlineKind::Linear => {
            if let Some(b0) = intercept {
                let sxx: f64 = xs.iter().map(|x| x * x).sum();
                if sxx <= 0.0 {
                    return None;
                }
                let b = xs.iter().zip(&ys).map(|(x, y)| x * (y - b0)).sum::<f64>() / sxx;
                return Some(Box::new(move |x| b0 + b * x));
            }
            let (a, b) = linreg(&xs, &ys)?;
            Some(Box::new(move |x| a + b * x))
        }
        TrendlineKind::Exponential => {
            if ys.iter().any(|y| *y <= 0.0) {
                return None;
            }
            let ly: Vec<f64> = ys.iter().map(|y| y.ln()).collect();
            let (a, b) = linreg(&xs, &ly)?;
            Some(Box::new(move |x| (a + b * x).exp()))
        }
        TrendlineKind::Logarithmic => {
            if xs.iter().any(|x| *x <= 0.0) {
                return None;
            }
            let lx: Vec<f64> = xs.iter().map(|x| x.ln()).collect();
            let (a, b) = linreg(&lx, &ys)?;
            Some(Box::new(move |x| a + b * x.ln()))
        }
        TrendlineKind::Power => {
            if xs.iter().chain(&ys).any(|v| *v <= 0.0) {
                return None;
            }
            let lx: Vec<f64> = xs.iter().map(|x| x.ln()).collect();
            let ly: Vec<f64> = ys.iter().map(|y| y.ln()).collect();
            let (a, b) = linreg(&lx, &ly)?;
            Some(Box::new(move |x| a.exp() * x.powf(b)))
        }
        TrendlineKind::Polynomial => {
            let k = (order.clamp(2, 6) as usize).min(n - 1);
            // Normal equations, solved by Gaussian elimination with partial pivoting.
            let m = k + 1;
            let mut a = vec![vec![0.0f64; m + 1]; m];
            for (x, y) in pts {
                let mut pw = vec![1.0f64; 2 * m];
                for j in 1..2 * m {
                    pw[j] = pw[j - 1] * x;
                }
                for r in 0..m {
                    for c in 0..m {
                        a[r][c] += pw[r + c];
                    }
                    a[r][m] += pw[r] * y;
                }
            }
            for col in 0..m {
                let piv = (col..m).max_by(|i, j| a[*i][col].abs().total_cmp(&a[*j][col].abs()))?;
                if a[piv][col].abs() < 1e-12 {
                    return None;
                }
                a.swap(col, piv);
                for r in 0..m {
                    if r != col {
                        let f = a[r][col] / a[col][col];
                        let pivot_row = a[col].clone();
                        for (c, v) in a[r].iter_mut().enumerate().skip(col) {
                            *v -= f * pivot_row[c];
                        }
                    }
                }
            }
            let coef: Vec<f64> = (0..m).map(|i| a[i][m] / a[i][i]).collect();
            Some(Box::new(move |x| {
                coef.iter().rev().fold(0.0, |acc, c| acc * x + c)
            }))
        }
        TrendlineKind::MovingAverage => None,
    }
}

#[allow(clippy::too_many_arguments, clippy::cast_possible_truncation)]
fn trendlines(
    ctx: &Ctx<'_>,
    g: &ChartGroup,
    gi: usize,
    si: usize,
    s: &Series,
    f: &Frame,
    plot: Bounds,
    out: &mut ChartLayout,
) {
    if s.trendlines.is_empty() {
        return;
    }
    let n = point_count(g);
    let pts: Vec<(f64, f64)> = if f.xs.is_some() {
        xy_values(s, n)
            .into_iter()
            .filter_map(|(x, y)| Some((x?, y?)))
            .collect()
    } else {
        #[allow(clippy::cast_precision_loss)]
        values(s)
            .into_iter()
            .enumerate()
            .filter_map(|(i, v)| Some((i as f64 + 1.0, v?)))
            .collect()
    };
    let to_px = |x: f64, y: f64| -> Option<(f32, f32)> {
        let fy = f.val.fraction(y);
        let fx = match &f.xs {
            Some(xs) => xs.fraction(x),
            None => f.cat_t(x - 1.0),
        };
        (fx.is_finite() && fy.is_finite()).then(|| {
            if f.swapped {
                (
                    plot.x + fy as f32 * plot.w,
                    plot.bottom() - fx as f32 * plot.h,
                )
            } else {
                (
                    plot.x + fx as f32 * plot.w,
                    plot.bottom() - fy as f32 * plot.h,
                )
            }
        })
    };
    let color = series_color(ctx, g, s, None);
    for (ti, t) in s.trendlines.iter().enumerate() {
        let element = ChartElement::Trendline {
            group: gi,
            series: si,
            index: ti,
        };
        let stroke =
            line_stroke(t.shape.line.as_ref(), ctx.colors, color, pt(1.5)).unwrap_or_else(|| {
                let mut st = solid(to_paint(color), pt(1.5));
                st.dash = vec![2.0, 3.0];
                st.cap = kiln_scene::LineCap::Round;
                Some(st)
            });
        let Some(stroke) = stroke else { continue };
        let mut line: Vec<(f32, f32)> = Vec::new();
        if t.kind == TrendlineKind::MovingAverage {
            let period = t.period.unwrap_or(2).max(2) as usize;
            for w in pts.windows(period) {
                #[allow(clippy::cast_precision_loss)]
                let avg = w.iter().map(|p| p.1).sum::<f64>() / period as f64;
                if let Some(p) = to_px(w[period - 1].0, avg) {
                    line.push(p);
                }
            }
        } else if let Some(model) = fit(t.kind, t.order.unwrap_or(2), t.intercept, &pts) {
            let x0 =
                pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min) - t.backward.unwrap_or(0.0);
            let x1 = pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max)
                + t.forward.unwrap_or(0.0);
            let steps = if t.kind == TrendlineKind::Linear {
                1
            } else {
                64
            };
            for k in 0..=steps {
                let x = x0 + (x1 - x0) * f64::from(k) / f64::from(steps);
                let y = model(x);
                if y.is_finite()
                    && let Some(p) = to_px(x, y)
                {
                    line.push((p.0, p.1.clamp(plot.y - 1.0, plot.bottom() + 1.0)));
                }
            }
        }
        if line.len() < 2 {
            continue;
        }
        let mut path = PathData::new();
        path.move_to(line[0].0, line[0].1);
        for p in &line[1..] {
            path.line_to(p.0, p.1);
        }
        out.marks.push(Mark {
            element,
            path,
            fill: None,
            stroke: Some(stroke),
        });
        out.hits.push((
            element,
            HitShape::Polyline {
                points: line,
                tolerance: 4.0,
            },
        ));
    }
}

// ───────────────────────────────────────────────────────────── pie and doughnut

/// A ring sector path: angles in radians clockwise from 12 o'clock.
pub(crate) fn wedge_path(cx: f32, cy: f32, r0: f32, r1: f32, start: f32, sweep: f32) -> PathData {
    let mut p = PathData::new();
    let pt_at = |r: f32, a: f32| (cx + r * a.sin(), cy - r * a.cos());
    let segs = ((sweep / (FRAC_PI_2 / 2.0)).ceil() as usize).max(1);
    let step = sweep / segs as f32;
    let arc = |p: &mut PathData, r: f32, a0: f32, da: f32| {
        // Cubic approximation of an arc of `da` radians.
        let k = 4.0 / 3.0 * (da / 4.0).tan();
        let (x0, y0) = pt_at(r, a0);
        let (x3, y3) = pt_at(r, a0 + da);
        let (t0x, t0y) = (a0.cos(), a0.sin());
        let (t1x, t1y) = ((a0 + da).cos(), (a0 + da).sin());
        p.cubic_to(
            x0 + k * r * t0x,
            y0 + k * r * t0y,
            x3 - k * r * t1x,
            y3 - k * r * t1y,
            x3,
            y3,
        );
    };
    if sweep >= TAU - 1e-4 {
        // A full ring or disc.
        let (x, y) = pt_at(r1, 0.0);
        p.move_to(x, y);
        for i in 0..segs {
            arc(&mut p, r1, i as f32 * step, step);
        }
        p.close();
        if r0 > 0.0 {
            let (x, y) = pt_at(r0, 0.0);
            p.move_to(x, y);
            for i in (0..segs).rev() {
                arc(&mut p, r0, (i + 1) as f32 * step, -step);
            }
            p.close();
        }
        return p;
    }
    let (x, y) = pt_at(r1, start);
    p.move_to(x, y);
    for i in 0..segs {
        arc(&mut p, r1, start + i as f32 * step, step);
    }
    if r0 > 0.0 {
        let (x, y) = pt_at(r0, start + sweep);
        p.line_to(x, y);
        for i in (0..segs).rev() {
            arc(&mut p, r0, start + (i + 1) as f32 * step, -step);
        }
    } else {
        p.line_to(cx, cy);
    }
    p.close();
    p
}

#[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
fn pie(ctx: &mut Ctx<'_>, gi: usize, area: Bounds, out: &mut ChartLayout) {
    let g = ctx.cs.chart.plot_area.groups[gi].clone();
    let (first_angle, hole) = match g.kind {
        GroupKind::Pie {
            first_slice_angle, ..
        } => (first_slice_angle.unwrap_or(0), 0.0),
        GroupKind::Doughnut {
            first_slice_angle,
            hole_size,
        } => (
            first_slice_angle.unwrap_or(0),
            f32::from(u8::try_from(hole_size.unwrap_or(50).clamp(10, 90)).unwrap_or(50)) / 100.0,
        ),
        _ => (0, 0.0),
    };
    let series: Vec<&Series> = if hole > 0.0 {
        g.series.iter().collect()
    } else {
        g.series.iter().take(1).collect()
    };
    if series.is_empty() {
        return;
    }
    let any_labels_outside = series.iter().any(|s| {
        label_props(&g, s).is_some_and(|d| {
            matches!(
                d.position,
                Some(LabelPosition::OutsideEnd | LabelPosition::BestFit) | None
            ) && hole == 0.0
        })
    });
    let max_explode = series
        .iter()
        .flat_map(|s| {
            std::iter::once(s.explosion.unwrap_or(0))
                .chain(s.points.iter().map(|p| p.explosion.unwrap_or(0)))
        })
        .max()
        .unwrap_or(0)
        .min(400) as f32
        / 100.0;
    let label_room = if any_labels_outside {
        pt(ctx.base_pt()) * 2.6
    } else {
        4.0
    };
    let r_total = ((area.w.min(area.h) / 2.0 - label_room).max(8.0)) / (1.0 + max_explode);
    let (cx, cy) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
    let start0 = (first_angle as f32).to_radians();
    let ring_w = if hole > 0.0 {
        r_total * (1.0 - hole) / series.len() as f32
    } else {
        r_total
    };
    let mut placer = LabelPlacer::default();
    for (ri, s) in series.iter().enumerate() {
        let si = g
            .series
            .iter()
            .position(|x| std::ptr::eq(x, *s))
            .unwrap_or(ri);
        let vals = values(s);
        let total: f64 = vals.iter().flatten().filter(|v| **v > 0.0).sum();
        if total <= 0.0 {
            continue;
        }
        let (r0, r1) = if hole > 0.0 {
            let r0 = r_total * hole + ring_w * ri as f32;
            (r0, r0 + ring_w)
        } else {
            (0.0, r_total)
        };
        let mut a = start0;
        for (i, v) in vals.iter().enumerate() {
            let Some(v) = v.filter(|v| *v > 0.0) else {
                continue;
            };
            let sweep = (v / total) as f32 * TAU;
            let explode = s
                .points
                .iter()
                .find(|p| p.idx as usize == i)
                .and_then(|p| p.explosion)
                .or(s.explosion)
                .unwrap_or(0)
                .min(400) as f32
                / 100.0
                * r_total;
            let mid = a + sweep / 2.0;
            let (ox, oy) = (explode * mid.sin(), -explode * mid.cos());
            let look = fill_look(ctx, &g, s, Some(i), false);
            let element = ChartElement::Point {
                group: gi,
                series: si,
                point: i,
            };
            out.marks.push(Mark {
                element,
                path: wedge_path(cx + ox, cy + oy, r0, r1, a, sweep),
                fill: look.fill,
                stroke: look.stroke.clone(),
            });
            out.hits.push((
                element,
                HitShape::Wedge {
                    cx: cx + ox,
                    cy: cy + oy,
                    r0,
                    r1,
                    start: a,
                    sweep,
                },
            ));
            if let Some(d) = label_props(&g, s)
                && let Some(text) = label_text(ctx, d, s, i, Some(v), Some(v / total), None)
            {
                let pos = d.position.unwrap_or(LabelPosition::BestFit);
                let probe_style = ctx.style(&[&d.text], 1.0, false, ctx.colors.text());
                let tw = ctx.text.width(&text, &probe_style);
                let inside_r = if hole > 0.0 {
                    (r0 + r1) / 2.0
                } else {
                    r1 * 0.62
                };
                let arc_len = inside_r * sweep;
                let fits = arc_len > tw * 0.9 && (r1 - r0) > probe_style.size * 1.6;
                let outside = hole == 0.0
                    && (matches!(pos, LabelPosition::OutsideEnd)
                        || (matches!(pos, LabelPosition::BestFit) && !fits));
                let (lx, ly, anchor) = if outside {
                    let r = r1 + 6.0;
                    let (x, y) = (cx + ox + r * mid.sin(), cy + oy - r * mid.cos());
                    let anchor = if mid.sin() >= 0.0 {
                        Anchor::Right(x - 2.0, y)
                    } else {
                        Anchor::Left(x + 2.0, y)
                    };
                    (x, y, anchor)
                } else {
                    let r = match pos {
                        LabelPosition::InsideEnd => r1 * 0.8,
                        _ => inside_r,
                    };
                    let (x, y) = (cx + ox + r * mid.sin(), cy + oy - r * mid.cos());
                    (x, y, Anchor::Center(x, y))
                };
                if outside && d.show_leader_lines != Some(false) {
                    let mut lp = PathData::new();
                    let (ex, ey) = (cx + ox + r1 * mid.sin(), cy + oy - r1 * mid.cos());
                    lp.move_to(ex, ey).line_to(lx, ly);
                    out.marks.push(Mark {
                        element: ChartElement::DataLabels {
                            group: gi,
                            series: si,
                        },
                        path: lp,
                        fill: None,
                        stroke: Some(solid(to_paint(ctx.colors.axis_line()), 0.75)),
                    });
                }
                push_label(
                    ctx,
                    out,
                    &mut placer,
                    d,
                    ChartElement::DataLabels {
                        group: gi,
                        series: si,
                    },
                    &text,
                    anchor,
                    (!outside).then_some(look.color),
                    area.inflate(label_room, label_room),
                );
            }
            a += sweep;
        }
    }
}

// ───────────────────────────────────────────────────────────── radar

#[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
fn radar(ctx: &mut Ctx<'_>, gi: usize, area: Bounds, out: &mut ChartLayout) {
    let g = ctx.cs.chart.plot_area.groups[gi].clone();
    let style = match g.kind {
        GroupKind::Radar { style } => style.unwrap_or(RadarStyle::Marker),
        _ => RadarStyle::Marker,
    };
    let n = point_count(&g);
    if n == 0 {
        return;
    }
    let cat_axis = g
        .axis_ids
        .first()
        .and_then(|id| find_axis(ctx, *id))
        .cloned();
    let val_axis = g
        .axis_ids
        .get(1)
        .and_then(|id| find_axis(ctx, *id))
        .cloned();
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for s in &g.series {
        for v in values(s).into_iter().flatten() {
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    let tick_style = tick_style(ctx, cat_axis.as_ref());
    let labels = category_labels(&g, n);
    let label_w = labels
        .iter()
        .map(|l| ctx.text.width(l, &tick_style))
        .fold(0.0f32, f32::max)
        .min(area.w * 0.25);
    let r = ((area.w / 2.0 - label_w - 8.0).min(area.h / 2.0 - tick_style.size * 1.6)).max(8.0);
    let intervals = ((r / 30.0).floor() as usize).clamp(2, 8);
    let scale = value_scale(lo, hi, val_axis.as_ref(), intervals, false);
    let (cx, cy) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
    let angle = |i: usize| i as f32 / n as f32 * TAU;
    let at = |i: usize, t: f64| {
        let rr = r * t.clamp(0.0, 1.1) as f32;
        (cx + rr * angle(i).sin(), cy - rr * angle(i).cos())
    };
    // Web: value gridlines as polygons, category spokes.
    let grid = to_paint(ctx.colors.gridline());
    let show_val_grid = val_axis
        .as_ref()
        .is_none_or(|a| a.major_gridlines.is_some());
    if show_val_grid {
        let mut web = PathData::new();
        for t in scale.ticks() {
            let f = scale.fraction(t);
            for i in 0..=n {
                let (x, y) = at(i % n, f);
                if i == 0 {
                    web.move_to(x, y);
                } else {
                    web.line_to(x, y);
                }
            }
        }
        out.marks.push(Mark {
            element: ChartElement::MajorGridlines(val_axis.as_ref().map_or(0, |a| a.id)),
            path: web,
            fill: None,
            stroke: Some(solid(grid, 1.0)),
        });
    }
    let mut spokes = PathData::new();
    for i in 0..n {
        let (x, y) = at(i, 1.0);
        spokes.move_to(cx, cy).line_to(x, y);
    }
    out.marks.push(Mark {
        element: ChartElement::Axis(cat_axis.as_ref().map_or(0, |a| a.id)),
        path: spokes,
        fill: None,
        stroke: Some(solid(to_paint(ctx.colors.axis_line()), 1.0)),
    });
    // Series.
    let mut placer = LabelPlacer::default();
    for (si, s) in g.series.iter().enumerate() {
        let vals = values(s);
        let pts: Vec<(f32, f32)> = (0..n)
            .map(|i| {
                at(
                    i,
                    scale.fraction(vals.get(i).copied().flatten().unwrap_or(scale.min)),
                )
            })
            .collect();
        let element = ChartElement::Series {
            group: gi,
            series: si,
        };
        let mut path = PathData::new();
        for (i, p) in pts.iter().enumerate() {
            if i == 0 {
                path.move_to(p.0, p.1);
            } else {
                path.line_to(p.0, p.1);
            }
        }
        path.close();
        if style == RadarStyle::Filled {
            let mut look = fill_look(ctx, &g, s, None, true);
            if s.shape.fill.is_none()
                && let Some(Paint::Rgba(c)) = look.fill
            {
                look.fill = Some(Paint::Rgba((c & 0xFFFF_FF00) | 0x99));
                look.stroke = Some(solid(Paint::Rgba(c), 1.5));
            }
            out.marks.push(Mark {
                element,
                path,
                fill: look.fill,
                stroke: look.stroke,
            });
            out.hits.push((element, HitShape::Polygon(pts.clone())));
        } else {
            if let Some(st) = line_look(ctx, &g, s, true) {
                out.marks.push(Mark {
                    element,
                    path,
                    fill: None,
                    stroke: Some(st),
                });
            }
            let mut ring = pts.clone();
            if let Some(f) = pts.first() {
                ring.push(*f);
            }
            out.hits.push((
                element,
                HitShape::Polyline {
                    points: ring,
                    tolerance: 4.0,
                },
            ));
            for (i, p) in pts.iter().enumerate() {
                if let Some(m) = marker_look(ctx, &g, s, i, style == RadarStyle::Marker) {
                    push_marker(
                        out,
                        ChartElement::Point {
                            group: gi,
                            series: si,
                            point: i,
                        },
                        &m,
                        p.0,
                        p.1,
                    );
                }
            }
        }
        if let Some(d) = label_props(&g, s) {
            for (i, p) in pts.iter().enumerate() {
                if let Some(text) =
                    label_text(ctx, d, s, i, vals.get(i).copied().flatten(), None, None)
                {
                    push_label(
                        ctx,
                        out,
                        &mut placer,
                        d,
                        ChartElement::DataLabels {
                            group: gi,
                            series: si,
                        },
                        &text,
                        Anchor::Above(p.0, p.1, true),
                        None,
                        area,
                    );
                }
            }
        }
    }
    // Category labels around the web.
    let cat_visible = cat_axis.as_ref().is_none_or(Axis::is_visible);
    if cat_visible {
        let element = ChartElement::Axis(cat_axis.as_ref().map_or(0, |a| a.id));
        for (i, l) in labels.iter().enumerate() {
            let a = angle(i);
            let block = ctx.block(l, &tick_style, area.w * 0.25, Alignment::Center);
            let (w, h) = (block.width, block.height);
            let (px, py) = (cx + (r + 6.0) * a.sin(), cy - (r + 6.0) * a.cos());
            let x = if a.sin().abs() < 0.2 {
                px - w / 2.0
            } else if a.sin() > 0.0 {
                px
            } else {
                px - w
            };
            let y = if a.cos() > 0.7 {
                py - h
            } else if a.cos() < -0.7 {
                py
            } else {
                py - h / 2.0
            };
            let bounds = Bounds::new(x, y, w, h);
            out.texts.push(PlacedText {
                element,
                x: x - block.left,
                y,
                block,
                rotation: 0.0,
                bounds,
            });
            out.hits.push((element, HitShape::Rect(bounds)));
        }
    }
    // Value labels up the first spoke.
    if let Some(va) = &val_axis
        && va.is_visible()
        && va.tick_label_pos != Some(TickLabelPos::None)
    {
        let vs = self_tick_style(ctx, va);
        let data_fmt = g.series.iter().find_map(|s| match &s.values {
            Some(DataSource::Num(n)) => n.format_code.clone(),
            _ => None,
        });
        let code = axis_format(Some(va), data_fmt.as_deref(), false);
        for t in scale.ticks() {
            let (x, y) = at(0, scale.fraction(t));
            let text = fmt(t, &code, ctx.cs);
            let block = ctx.block(&text, &vs, 200.0, Alignment::Left);
            let bounds = Bounds::new(
                x - block.width - 4.0,
                y - block.height / 2.0,
                block.width,
                block.height,
            );
            out.texts.push(PlacedText {
                element: ChartElement::Axis(va.id),
                x: bounds.x - block.left,
                y: bounds.y,
                block,
                rotation: 0.0,
                bounds,
            });
        }
    }
    out.axes.push(AxisLayout {
        id: val_axis.as_ref().map_or(0, |a| a.id),
        horizontal: false,
        line: ((cx, cy), (cx, cy - r)),
        scale: Some(scale),
    });
}

fn self_tick_style(ctx: &Ctx<'_>, a: &Axis) -> TextStyle {
    tick_style(ctx, Some(a))
}

// ───────────────────────────────────────────────────────────── hierarchy (treemap, sunburst)

/// A node of a hierarchy built from multi-level categories.
#[derive(Debug, Clone, Default)]
struct Node {
    name: String,
    value: f64,
    /// Index of the leaf point (leaves only).
    point: Option<usize>,
    children: Vec<Node>,
}

fn hierarchy(s: &Series) -> Node {
    let vals = values(s);
    let mut root = Node::default();
    let levels: Vec<Vec<Option<String>>> = match &s.categories {
        Some(DataSource::MultiLevel(m)) => m.levels.clone(),
        Some(c) => vec![(0..c.len()).map(|i| c.label(i)).collect()],
        None => vec![(0..vals.len()).map(|i| Some((i + 1).to_string())).collect()],
    };
    let n = vals
        .len()
        .min(levels.first().map_or(0, Vec::len))
        .min(100_000);
    // Parent labels may be written once for a run of points: carry the last one down.
    let mut carry: Vec<Option<String>> = vec![None; levels.len()];
    for (i, v) in vals.iter().enumerate().take(n) {
        let Some(v) = v.filter(|v| *v > 0.0) else {
            continue;
        };
        let mut path: Vec<String> = Vec::new();
        for (l, level) in levels.iter().enumerate().rev() {
            if let Some(Some(name)) = level.get(i) {
                carry[l] = Some(name.clone());
                // A new parent resets the carried labels of the levels below it.
                for c in carry.iter_mut().take(l) {
                    *c = None;
                }
            }
            if let Some(name) = carry[l].clone() {
                path.push(name);
            }
        }
        let mut node = &mut root;
        node.value += v;
        let last = path.len().saturating_sub(1);
        for (d, name) in path.into_iter().enumerate() {
            let idx = match node
                .children
                .iter()
                .position(|c| c.name == name && c.point.is_none())
            {
                Some(k) if d < last => k,
                _ => {
                    node.children.push(Node {
                        name,
                        ..Node::default()
                    });
                    node.children.len() - 1
                }
            };
            node = &mut node.children[idx];
            node.value += v;
            if d == last {
                node.point = Some(i);
            }
        }
    }
    root
}

/// Squarified treemap layout of `values` in `rect` (Bruls, Huijing, van Wijk).
fn squarify(values: &[f64], rect: Bounds) -> Vec<Bounds> {
    let total: f64 = values.iter().sum();
    let mut out = vec![Bounds::ZERO; values.len()];
    if total <= 0.0 || rect.w <= 0.0 || rect.h <= 0.0 {
        return out;
    }
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|a, b| values[*b].total_cmp(&values[*a]));
    let scale = f64::from(rect.w) * f64::from(rect.h) / total;
    let areas: Vec<f64> = order.iter().map(|i| values[*i] * scale).collect();
    let mut r = rect;
    let mut start = 0;
    while start < areas.len() {
        let short = f64::from(r.w.min(r.h));
        let worst = |row: &[f64]| -> f64 {
            let s: f64 = row.iter().sum();
            let (mx, mn) = row
                .iter()
                .fold((0.0f64, f64::INFINITY), |(a, b), v| (a.max(*v), b.min(*v)));
            if s <= 0.0 || mn <= 0.0 {
                return f64::INFINITY;
            }
            let s2 = s * s;
            let w2 = short * short;
            (w2 * mx / s2).max(s2 / (w2 * mn))
        };
        let mut end = start + 1;
        while end < areas.len() && worst(&areas[start..=end]) <= worst(&areas[start..end]) {
            end += 1;
        }
        let row = &areas[start..end];
        let sum: f64 = row.iter().sum();
        #[allow(clippy::cast_possible_truncation)]
        if r.w >= r.h {
            let w = (sum / f64::from(r.h)) as f32;
            let mut y = r.y;
            for (k, a) in row.iter().enumerate() {
                let h = (a / sum) as f32 * r.h;
                out[order[start + k]] = Bounds::new(r.x, y, w.min(r.w), h);
                y += h;
            }
            r = Bounds::new(r.x + w, r.y, (r.w - w).max(0.0), r.h);
        } else {
            let h = (sum / f64::from(r.w)) as f32;
            let mut x = r.x;
            for (k, a) in row.iter().enumerate() {
                let w = (a / sum) as f32 * r.w;
                out[order[start + k]] = Bounds::new(x, r.y, w, h.min(r.h));
                x += w;
            }
            r = Bounds::new(r.x, r.y + h, r.w, (r.h - h).max(0.0));
        }
        start = end;
    }
    out
}

fn hierarchy_labels(g: &ChartGroup, s: &Series) -> (bool, bool) {
    let d = s.data_labels.as_ref().or(g.data_labels.as_ref());
    let cat = d.is_none_or(|d| d.show_category != Some(false) && d.delete != Some(true));
    let val = d.is_some_and(|d| d.show_value == Some(true));
    (cat, val)
}

#[allow(clippy::too_many_lines)]
fn treemap(ctx: &mut Ctx<'_>, gi: usize, area: Bounds, out: &mut ChartLayout) {
    let g = ctx.cs.chart.plot_area.groups[gi].clone();
    let Some(s) = g.series.first() else { return };
    let banner = matches!(
        g.kind,
        GroupKind::Hierarchy { parent_label_layout: Some(ref p), .. } if p == "banner"
    );
    let root = hierarchy(s);
    let (show_cat, show_val) = hierarchy_labels(&g, s);
    let bg = to_paint(ctx.colors.background());
    let tops: Vec<f64> = root.children.iter().map(|c| c.value).collect();
    let rects = squarify(&tops, area);
    let label_style = {
        let props: Vec<&TextProps> = s.data_labels.iter().map(|d| &d.text).collect();
        ctx.style(&props, 1.0, false, Rgba::rgb(255.0, 255.0, 255.0))
    };
    for (ti, (top, rect)) in root.children.iter().zip(&rects).enumerate() {
        let color = ctx.colors.series_color(ti, ctx.cs.style);
        let banner_h = if banner { label_style.size * 1.6 } else { 0.0 };
        let inner = Bounds::new(
            rect.x,
            rect.y + banner_h,
            rect.w,
            (rect.h - banner_h).max(0.0),
        );
        // Leaves of this branch (flattened below the top level).
        let mut leaves: Vec<&Node> = Vec::new();
        let mut stack: Vec<&Node> = vec![top];
        while let Some(n) = stack.pop() {
            if n.children.is_empty() {
                leaves.push(n);
            } else {
                stack.extend(n.children.iter().rev());
            }
        }
        let leaf_rects = squarify(&leaves.iter().map(|l| l.value).collect::<Vec<_>>(), inner);
        if banner {
            out.marks.push(Mark {
                element: ChartElement::Series {
                    group: gi,
                    series: 0,
                },
                path: PathData::rect(Bounds::new(rect.x, rect.y, rect.w, banner_h)),
                fill: Some(to_paint(transform(color, 75_000, 0))),
                stroke: Some(solid(bg, 1.5)),
            });
        }
        for (leaf, lr) in leaves.iter().zip(&leaf_rects) {
            let point = leaf.point.unwrap_or(0);
            let dpt = s.points.iter().find(|p| p.idx as usize == point);
            let fill = dpt
                .and_then(|p| fill_paint(&p.shape, ctx.colors))
                .unwrap_or(Some(to_paint(color)));
            let element = ChartElement::Point {
                group: gi,
                series: 0,
                point,
            };
            out.marks.push(Mark {
                element,
                path: PathData::rect(*lr),
                fill,
                stroke: Some(solid(bg, 1.5)),
            });
            out.hits.push((element, HitShape::Rect(*lr)));
            if show_cat || show_val {
                let mut text = String::new();
                if show_cat {
                    text.push_str(&leaf.name);
                }
                if show_val {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&fmt(leaf.value, "General", ctx.cs));
                }
                let mut st = label_style.clone();
                if s.data_labels
                    .as_ref()
                    .is_none_or(|d| d.text.run.color.is_none())
                {
                    let lum = 0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b;
                    st.paint = if lum > 150.0 {
                        to_paint(Rgba::rgb(43.0, 28.0, 20.0))
                    } else {
                        to_paint(Rgba::rgb(255.0, 255.0, 255.0))
                    };
                }
                let block = ctx.block(&text, &st, (lr.w - 8.0).max(4.0), Alignment::Left);
                if block.height <= lr.h - 6.0 && block.width <= lr.w - 6.0 {
                    let (x, y) = (lr.x + 5.0, lr.y + 4.0);
                    let bounds = Bounds::new(x + block.left, y, block.width, block.height);
                    out.texts.push(PlacedText {
                        element: ChartElement::DataLabels {
                            group: gi,
                            series: 0,
                        },
                        block,
                        x,
                        y,
                        rotation: 0.0,
                        bounds,
                    });
                }
            }
        }
        // Parent label.
        let mut st = label_style.clone();
        st.bold = true;
        st.size *= 1.15;
        let lum = 0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b;
        st.paint = if lum > 150.0 {
            to_paint(Rgba::rgb(43.0, 28.0, 20.0))
        } else {
            to_paint(Rgba::rgb(255.0, 255.0, 255.0))
        };
        let overlapping = !banner
            && !matches!(g.kind, GroupKind::Hierarchy { parent_label_layout: Some(ref p), .. } if p == "none");
        if (banner || overlapping) && rect.w > 30.0 && rect.h > 20.0 {
            let block = ctx.block(&top.name, &st, (rect.w - 10.0).max(4.0), Alignment::Left);
            if overlapping {
                // Overlapping parent labels sit at the bottom-right corner of the branch, so
                // they do not cover the leaf labels at the top-left.
                let x = rect.right() - 6.0 - block.width - block.left;
                let y = rect.bottom() - 4.0 - block.height;
                let bounds = Bounds::new(x + block.left, y, block.width, block.height);
                out.texts.push(PlacedText {
                    element: ChartElement::DataLabels {
                        group: gi,
                        series: 0,
                    },
                    block,
                    x,
                    y,
                    rotation: 0.0,
                    bounds,
                });
            } else {
                let (x, y) = (rect.x + 5.0, rect.y + (banner_h - block.height) / 2.0);
                let bounds = Bounds::new(x + block.left, y, block.width, block.height);
                out.texts.push(PlacedText {
                    element: ChartElement::DataLabels {
                        group: gi,
                        series: 0,
                    },
                    block,
                    x,
                    y,
                    rotation: 0.0,
                    bounds,
                });
            }
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
fn sunburst(ctx: &mut Ctx<'_>, gi: usize, area: Bounds, out: &mut ChartLayout) {
    let g = ctx.cs.chart.plot_area.groups[gi].clone();
    let Some(s) = g.series.first() else { return };
    let root = hierarchy(s);
    if root.value <= 0.0 {
        return;
    }
    fn depth(n: &Node) -> usize {
        1 + n.children.iter().map(depth).max().unwrap_or(0)
    }
    let levels = depth(&root).saturating_sub(1).max(1);
    let r_max = area.w.min(area.h) / 2.0 - 2.0;
    let hole = r_max * 0.12;
    let ring = (r_max - hole) / levels as f32;
    let (cx, cy) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
    let bg = to_paint(ctx.colors.background());
    let (show_cat, _) = hierarchy_labels(&g, s);
    let label_style = {
        let props: Vec<&TextProps> = s.data_labels.iter().map(|d| &d.text).collect();
        ctx.style(&props, 1.0, false, Rgba::rgb(255.0, 255.0, 255.0))
    };
    // Depth-first with angle ranges.
    let mut stack: Vec<(&Node, usize, f32, f32, usize)> = Vec::new();
    let mut a = 0.0f32;
    for (ti, c) in root.children.iter().enumerate() {
        let sweep = (c.value / root.value) as f32 * TAU;
        stack.push((c, 0, a, sweep, ti));
        a += sweep;
    }
    while let Some((node, d, start, sweep, branch)) = stack.pop() {
        let base = ctx.colors.series_color(branch, ctx.cs.style);
        let color = if d == 0 {
            base
        } else {
            let k = (d as i64 * 18_000).min(54_000);
            transform(base, 100_000 - k, k)
        };
        let (r0, r1) = (hole + ring * d as f32, hole + ring * (d as f32 + 1.0));
        // A leaf above the deepest level fills out to the edge, as Excel draws it.
        let r1 = if node.children.is_empty() { r_max } else { r1 };
        let element = match node.point {
            Some(p) => ChartElement::Point {
                group: gi,
                series: 0,
                point: p,
            },
            None => ChartElement::Series {
                group: gi,
                series: 0,
            },
        };
        out.marks.push(Mark {
            element,
            path: wedge_path(cx, cy, r0, r1, start, sweep),
            fill: Some(to_paint(color)),
            stroke: Some(solid(bg, 1.5)),
        });
        out.hits.push((
            element,
            HitShape::Wedge {
                cx,
                cy,
                r0,
                r1,
                start,
                sweep,
            },
        ));
        if show_cat {
            let mid = start + sweep / 2.0;
            let rm =
                (r0 + (if node.children.is_empty() {
                    r0 + ring
                } else {
                    r1
                })) / 2.0;
            let mut st = label_style.clone();
            let lum = 0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b;
            st.paint = if lum > 150.0 {
                to_paint(Rgba::rgb(43.0, 28.0, 20.0))
            } else {
                to_paint(Rgba::rgb(255.0, 255.0, 255.0))
            };
            let block = ctx.block(&node.name, &st, 400.0, Alignment::Left);
            let arc = rm * sweep;
            if block.width < arc * 0.9 && block.height < ring * 0.9 {
                let (px, py) = (cx + rm * mid.sin(), cy - rm * mid.cos());
                let x = px - block.width / 2.0 - block.left;
                let y = py - block.height / 2.0;
                let bounds = Bounds::new(px - block.width / 2.0, y, block.width, block.height);
                out.texts.push(PlacedText {
                    element: ChartElement::DataLabels {
                        group: gi,
                        series: 0,
                    },
                    block,
                    x,
                    y,
                    rotation: 0.0,
                    bounds,
                });
            }
        }
        let mut a = start;
        for c in &node.children {
            let sw = if node.value > 0.0 {
                (c.value / node.value) as f32 * sweep
            } else {
                0.0
            };
            stack.push((c, d + 1, a, sw, branch));
            a += sw;
        }
    }
    let _ = nice_unit;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squarify_fills_the_rectangle() {
        let rects = squarify(
            &[6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0],
            Bounds::new(0.0, 0.0, 60.0, 40.0),
        );
        let total: f32 = rects.iter().map(|r| r.w * r.h).sum();
        assert!((total - 2400.0).abs() < 1.0, "{total}");
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                let o = a.intersection(b).map_or(0.0, |o| o.w * o.h);
                assert!(o < 0.01, "{a:?} overlaps {b:?}");
            }
        }
        assert!(squarify(&[0.0], Bounds::new(0.0, 0.0, 1.0, 1.0))[0].w == 0.0);
    }

    #[test]
    fn trendline_fits() {
        let pts: Vec<(f64, f64)> = (1..=5)
            .map(|x| (f64::from(x), 2.0 * f64::from(x) + 1.0))
            .collect();
        let f = fit(TrendlineKind::Linear, 2, None, &pts).unwrap();
        assert!((f(10.0) - 21.0).abs() < 1e-9);
        let pts: Vec<(f64, f64)> = (1..=5)
            .map(|x| (f64::from(x), 3.0 * (0.5 * f64::from(x)).exp()))
            .collect();
        let f = fit(TrendlineKind::Exponential, 2, None, &pts).unwrap();
        assert!((f(2.0) - 3.0 * 1f64.exp()).abs() < 1e-6);
        let pts: Vec<(f64, f64)> = (0..6)
            .map(|x| {
                let x = f64::from(x);
                (x, x * x - 2.0 * x + 1.0)
            })
            .collect();
        let f = fit(TrendlineKind::Polynomial, 2, None, &pts).unwrap();
        assert!((f(7.0) - 36.0).abs() < 1e-6);
        assert!(fit(TrendlineKind::Linear, 2, None, &[(1.0, 1.0)]).is_none());
    }

    #[test]
    fn wedges_hit_test_clockwise_from_noon() {
        let h = HitShape::Wedge {
            cx: 0.0,
            cy: 0.0,
            r0: 0.0,
            r1: 10.0,
            start: 0.0,
            sweep: FRAC_PI_2,
        };
        assert!(h.contains(5.0, -5.0), "upper right quadrant");
        assert!(!h.contains(-5.0, -5.0));
        assert!(!h.contains(5.0, 5.0));
    }
}
