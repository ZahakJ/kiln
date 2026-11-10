// SPDX-License-Identifier: AGPL-3.0-or-later
//! Charts: ODF chart objects (`Object N/content.xml`, an `office:chart` document embedded with
//! `draw:object`) ⇄ the chart model of `kiln-charts` ([`ChartSpace`]).
//!
//! | kiln-charts | ODF |
//! |---|---|
//! | bar (column, bar; clustered, stacked, percent) | `chart:bar` (`chart:vertical` for bars), `chart:stacked`, `chart:percentage` |
//! | line, area, radar (filled) | `chart:line`, `chart:area`, `chart:radar`, `chart:filled-radar` |
//! | pie, doughnut | `chart:circle`, `chart:ring` (`chart:angle-offset`) |
//! | scatter, bubble | `chart:scatter`, `chart:bubble`: x values (and for bubbles y values) as `chart:domain` |
//! | combo charts | a `chart:class` on each series; secondary axes as `chart:attached-axis` |
//! | series name, values, categories | `chart:label-cell-address`, `chart:values-cell-range-address`, the x axis's `chart:categories` |
//! | series and point fill, outline, markers, labels | the series' and `chart:data-point`s' styles |
//! | title, axis titles, legend, gridlines, axis scaling | `chart:title`, `chart:legend`, `chart:grid`, axis styles |
//!
//! Data: in a spreadsheet the ranges are cells of the sheets (the caches are refreshed from them
//! after reading); in text documents and presentations they point into the object's own
//! `local-table`, which every chart written here also carries with the cached values (LibreOffice
//! shows that table when the ranges cannot be resolved).
//!
//! Stock, surface, of-pie and the Office 2016 chart types have no ODF class here and are not
//! written (the save reports them).

use std::collections::HashMap;

use kiln_charts::model::{
    Axis, AxisKind, AxisPos, BarDir, ChartGroup, ChartSpace, DataLabels, DataPoint, DataSource,
    GroupKind, Grouping, LabelPosition, Legend, LegendPos, Marker, MarkerSymbol, NumData,
    RadarStyle, ScatterStyle, Series, SeriesText, ShapeProps, StrData, TickLabelPos, Title,
    TitleText,
};
use kiln_charts::palette::ChartColors;
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::drawingml::line::Line;
use kiln_ooxml::xml::XmlElement;

use crate::el::El;
use crate::formula::{excel_ref_to_range_address, parse_range_list, range_address_to_excel};
use crate::units::{emu, emu_str};
use crate::xml::{XmlOut, all_text, kid, kids};

/// The media type of an embedded chart object.
pub const CHART_MEDIA_TYPE: &str = "application/vnd.oasis.opendocument.chart";

/// The name of the table of cached values inside a chart object.
const LOCAL: &str = "local-table";

// ------------------------------------------------------------------ reading

/// One cell of the object's local table.
#[derive(Debug, Clone, PartialEq)]
enum Cell {
    Empty,
    Num(f64),
    Str(String),
}

/// The local table: rows of cells, header row and column included.
#[derive(Debug, Default)]
struct LocalTable {
    rows: Vec<Vec<Cell>>,
}

impl LocalTable {
    fn read(t: &XmlElement) -> Self {
        let mut out = Self::default();
        let mut rows = Vec::new();
        collect_rows(t, &mut rows);
        for r in rows.into_iter().take(10_000) {
            let reps = r
                .attr("table:number-rows-repeated")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, 1_000);
            let mut cells = Vec::new();
            for c in r.elements().filter(|c| c.name.ends_with("table-cell")) {
                let n = c
                    .attr("table:number-columns-repeated")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(1)
                    .clamp(1, 256);
                let cell = match c.attr("office:value-type") {
                    Some("float" | "percentage" | "currency") => c
                        .attr("office:value")
                        .and_then(|v| v.parse::<f64>().ok())
                        .filter(|v| v.is_finite())
                        .map_or(Cell::Empty, Cell::Num),
                    Some(_) => Cell::Str(
                        kids(c, "text:p")
                            .map(all_text)
                            .collect::<Vec<_>>()
                            .join("\n"),
                    ),
                    None => Cell::Empty,
                };
                for _ in 0..n {
                    if cells.len() < 1_024 {
                        cells.push(cell.clone());
                    }
                }
            }
            for _ in 0..reps {
                if out.rows.len() < 100_000 {
                    out.rows.push(cells.clone());
                }
            }
        }
        out
    }

    fn get(&self, row: u32, col: u32) -> Cell {
        self.rows
            .get(row as usize)
            .and_then(|r| r.get(col as usize))
            .cloned()
            .unwrap_or(Cell::Empty)
    }

    /// The cells of a range list into the table (every range, row by row).
    fn cells(&self, address: &str) -> Option<Vec<Cell>> {
        let ranges = parse_range_list(address);
        if ranges.is_empty() {
            return None;
        }
        let mut out = Vec::new();
        for (sheet, r) in ranges {
            if sheet.as_deref().is_some_and(|s| s != LOCAL) {
                return None;
            }
            for row in r.r0..=r.r1.min(r.r0 + 10_000) {
                for col in r.c0..=r.c1.min(r.c0 + 1_000) {
                    out.push(self.get(row, col));
                }
            }
        }
        Some(out)
    }
}

fn collect_rows<'a>(e: &'a XmlElement, out: &mut Vec<&'a XmlElement>) {
    for c in e.elements() {
        match c.name.as_str() {
            "table:table-row" => out.push(c),
            "table:table-header-rows" | "table:table-rows" | "table:table-row-group" => {
                collect_rows(c, out);
            }
            _ => {}
        }
    }
}

/// Whether an address points into the local table (or names no sheet at all).
fn is_local(address: &str) -> bool {
    parse_range_list(address)
        .iter()
        .all(|(s, _)| s.as_deref().is_none_or(|s| s == LOCAL))
}

/// The automatic styles of a chart object by name.
struct Styles<'a> {
    by_name: HashMap<&'a str, &'a XmlElement>,
}

impl<'a> Styles<'a> {
    fn new(content: &'a XmlElement, styles: Option<&'a XmlElement>) -> Self {
        let mut by_name = HashMap::new();
        for root in std::iter::once(content).chain(styles) {
            for holder in ["office:automatic-styles", "office:styles"] {
                if let Some(h) = kid(root, holder) {
                    for s in kids(h, "style:style") {
                        if let Some(n) = s.attr("style:name") {
                            by_name.entry(n).or_insert(s);
                        }
                    }
                }
            }
        }
        Self { by_name }
    }

    fn props(&self, name: Option<&str>, kind: &str) -> Option<&'a XmlElement> {
        let s = self.by_name.get(name?)?;
        kid(s, kind)
    }

    fn chart(&self, name: Option<&str>) -> Option<&'a XmlElement> {
        self.props(name, "style:chart-properties")
    }

    fn graphic(&self, name: Option<&str>) -> Option<&'a XmlElement> {
        self.props(name, "style:graphic-properties")
    }

    fn text(&self, name: Option<&str>) -> Option<&'a XmlElement> {
        self.props(name, "style:text-properties")
    }
}

fn flag(e: Option<&XmlElement>, attr: &str) -> bool {
    e.and_then(|e| e.attr(attr)) == Some("true")
}

fn hex_color(v: &str) -> Option<DrawingColor> {
    let h = v.trim().strip_prefix('#')?;
    (h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| DrawingColor::srgb(&h.to_ascii_uppercase()))
}

/// Fill and outline of a chart style's graphic properties.
fn shape_of(gp: Option<&XmlElement>, line_chart: bool) -> ShapeProps {
    let Some(gp) = gp else {
        return ShapeProps::default();
    };
    let fill = match gp.attr("draw:fill") {
        Some("none") => Some(Fill::None),
        Some("solid") | None => gp
            .attr("draw:fill-color")
            .and_then(hex_color)
            .map(Fill::Solid),
        _ => gp
            .attr("draw:fill-color")
            .and_then(hex_color)
            .map(Fill::Solid),
    };
    let stroke = gp.attr("draw:stroke");
    let line = if stroke == Some("none") {
        Some(Line {
            fill: Some(Fill::None),
            ..Line::default()
        })
    } else {
        let color = gp.attr("svg:stroke-color").and_then(hex_color);
        let width = gp.attr("svg:stroke-width").and_then(emu);
        (color.is_some() || width.is_some() || stroke == Some("solid")).then(|| Line {
            width,
            fill: color.map(Fill::Solid),
            ..Line::default()
        })
    };
    ShapeProps {
        // A line chart's series colour is its line.
        fill: if line_chart { None } else { fill },
        line,
        shadow: false,
        raw: None,
    }
}

fn run_props(tp: Option<&XmlElement>) -> kiln_charts::model::RunProps {
    let mut r = kiln_charts::model::RunProps::default();
    let Some(tp) = tp else {
        return r;
    };
    if let Some(s) = tp
        .attr("fo:font-size")
        .and_then(|s| s.strip_suffix("pt"))
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|s| (1.0..=400.0).contains(s))
    {
        // In range by the filter.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            r.size = Some((s * 100.0).round() as u32);
        }
    }
    if let Some(w) = tp.attr("fo:font-weight") {
        r.bold = Some(w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600));
    }
    if let Some(s) = tp.attr("fo:font-style") {
        r.italic = Some(s == "italic" || s == "oblique");
    }
    r.color = tp.attr("fo:color").and_then(hex_color);
    r.latin = tp.attr("fo:font-family").map(str::to_owned);
    r
}

fn title_of(t: &XmlElement, styles: &Styles<'_>) -> Title {
    let text = kids(t, "text:p")
        .map(all_text)
        .collect::<Vec<_>>()
        .join("\n");
    let span_style = t
        .descendants("text:span")
        .into_iter()
        .next()
        .and_then(|s| s.attr("text:style-name"));
    let mut run = run_props(styles.text(t.attr("chart:style-name")));
    let span = run_props(styles.text(span_style));
    if span.size.is_some() {
        run.size = span.size;
    }
    if span.bold.is_some() {
        run.bold = span.bold;
    }
    if span.color.is_some() {
        run.color = span.color;
    }
    Title {
        text: Some(TitleText::Rich {
            plain: text,
            run,
            raw: None,
        }),
        overlay: Some(false),
        ..Title::default()
    }
}

fn legend_pos(v: &str) -> LegendPos {
    match v {
        "start" => LegendPos::Left,
        "top" => LegendPos::Top,
        "bottom" => LegendPos::Bottom,
        "top-end" | "top-start" => LegendPos::TopRight,
        _ => LegendPos::Right,
    }
}

fn label_pos(v: &str) -> Option<LabelPosition> {
    Some(match v {
        "avoid-overlap" => LabelPosition::BestFit,
        "center" => LabelPosition::Center,
        "top" => LabelPosition::Top,
        "bottom" => LabelPosition::Bottom,
        "left" => LabelPosition::Left,
        "right" => LabelPosition::Right,
        "inside" => LabelPosition::InsideEnd,
        "outside" => LabelPosition::OutsideEnd,
        "near-origin" => LabelPosition::InsideBase,
        _ => return None,
    })
}

fn labels_of(cp: Option<&XmlElement>) -> Option<DataLabels> {
    let cp = cp?;
    let number = cp.attr("chart:data-label-number").unwrap_or("none");
    let value = matches!(number, "value" | "value-and-percentage");
    let percent = matches!(number, "percentage" | "value-and-percentage");
    let category = flag(Some(cp), "chart:data-label-text");
    let series =
        flag(Some(cp), "loext:data-label-series") || flag(Some(cp), "chart:data-label-series");
    if !(value || percent || category || series) {
        return None;
    }
    Some(DataLabels {
        show_value: Some(value),
        show_percent: Some(percent),
        show_category: Some(category),
        show_series: Some(series),
        show_legend_key: Some(flag(Some(cp), "chart:data-label-symbol")),
        position: cp.attr("chart:label-position").and_then(label_pos),
        ..DataLabels::default()
    })
}

fn marker_of(cp: Option<&XmlElement>) -> Option<Marker> {
    let cp = cp?;
    let symbol = match cp.attr("chart:symbol-type")? {
        "none" => MarkerSymbol::None,
        "automatic" => MarkerSymbol::Auto,
        "named-symbol" => match cp.attr("chart:symbol-name").unwrap_or_default() {
            "square" => MarkerSymbol::Square,
            "diamond" => MarkerSymbol::Diamond,
            "circle" => MarkerSymbol::Circle,
            "x" => MarkerSymbol::X,
            "plus" => MarkerSymbol::Plus,
            "star" | "asterisk" => MarkerSymbol::Star,
            "horizontal-bar" => MarkerSymbol::Dash,
            "arrow-up" | "arrow-down" | "arrow-left" | "arrow-right" => MarkerSymbol::Triangle,
            _ => MarkerSymbol::Auto,
        },
        _ => return None,
    };
    let size = cp
        .attr("chart:symbol-width")
        .and_then(emu)
        // EMU to points.
        .map(|e| (e as f64 / 12_700.0).round().clamp(2.0, 72.0) as u32);
    Some(Marker {
        symbol: Some(symbol),
        size,
        ..Marker::default()
    })
}

/// What the model calls a chart class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Class {
    Bar,
    Line,
    Area,
    Pie,
    Ring,
    Scatter,
    Bubble,
    Radar,
    FilledRadar,
}

fn class_of(v: &str) -> Option<Class> {
    Some(match v.strip_prefix("chart:").unwrap_or(v) {
        "bar" => Class::Bar,
        "line" => Class::Line,
        "area" => Class::Area,
        "circle" => Class::Pie,
        "ring" => Class::Ring,
        "scatter" => Class::Scatter,
        "bubble" => Class::Bubble,
        "radar" => Class::Radar,
        "filled-radar" => Class::FilledRadar,
        _ => return None,
    })
}

/// A data source: a reference with cached values, or literal values from the local table.
fn source(address: Option<&str>, local: &LocalTable, numbers: bool) -> Option<DataSource> {
    let address = address.filter(|a| !a.trim().is_empty())?;
    let formula = (!is_local(address)).then(|| range_address_to_excel(address));
    let cells = if formula.is_none() {
        local.cells(address).unwrap_or_default()
    } else {
        Vec::new()
    };
    let has_cache = formula.is_none();
    if numbers || cells.iter().all(|c| !matches!(c, Cell::Str(_))) && !cells.is_empty() {
        Some(DataSource::Num(NumData {
            formula,
            format_code: Some("General".to_owned()),
            points: cells
                .iter()
                .map(|c| match c {
                    Cell::Num(n) => Some(*n),
                    Cell::Str(s) => s.trim().parse().ok(),
                    Cell::Empty => None,
                })
                .collect(),
            has_cache,
            extras: Default::default(),
        }))
    } else {
        Some(DataSource::Str(StrData {
            formula,
            points: cells
                .iter()
                .map(|c| match c {
                    Cell::Num(n) => Some(kiln_ooxml::xml::format_number(*n)),
                    Cell::Str(s) => Some(s.clone()),
                    Cell::Empty => None,
                })
                .collect(),
            has_cache,
            extras: Default::default(),
        }))
    }
}

fn series_name(s: &XmlElement, local: &LocalTable) -> Option<SeriesText> {
    if let Some(a) = s
        .attr("chart:label-cell-address")
        .filter(|a| !a.trim().is_empty())
    {
        if is_local(a) {
            let text = local
                .cells(a)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|c| match c {
                    Cell::Str(s) => Some(s),
                    Cell::Num(n) => Some(kiln_ooxml::xml::format_number(n)),
                    Cell::Empty => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            return Some(SeriesText::Literal(text));
        }
        return Some(SeriesText::Ref {
            formula: range_address_to_excel(a),
            cache: None,
        });
    }
    s.attr("loext:label-string")
        .map(|v| SeriesText::Literal(v.trim().trim_matches('"').replace("\"\"", "\"")))
}

/// Reads a chart object's `content.xml` (with its `styles.xml`).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn read_object(content: &XmlElement, styles_root: Option<&XmlElement>) -> Option<ChartSpace> {
    let body = kid(content, "office:body")?;
    let chart = kid(kid(body, "office:chart")?, "chart:chart")?;
    let styles = Styles::new(content, styles_root);
    let main_class = chart.attr("chart:class").and_then(class_of)?;
    let local = chart
        .elements()
        .find(|e| e.name == "table:table")
        .map(LocalTable::read)
        .unwrap_or_default();
    let plot = kid(chart, "chart:plot-area")?;
    let pcp = styles.chart(plot.attr("chart:style-name"));
    let three_d = flag(pcp, "chart:three-dimensional");
    let grouping = if flag(pcp, "chart:percentage") {
        Some(Grouping::PercentStacked)
    } else if flag(pcp, "chart:stacked") {
        Some(Grouping::Stacked)
    } else {
        None
    };
    let smooth = pcp
        .and_then(|p| p.attr("chart:interpolation"))
        .is_some_and(|v| v != "none");
    let mut cs = ChartSpace {
        date1904: Some(false),
        ..ChartSpace::default()
    };
    // Title and legend.
    cs.chart.title = kid(chart, "chart:title").map(|t| title_of(t, &styles));
    cs.chart.auto_title_deleted = Some(cs.chart.title.is_none());
    cs.chart.legend = kid(chart, "chart:legend").map(|l| Legend {
        position: Some(legend_pos(l.attr("chart:legend-position").unwrap_or("end"))),
        overlay: Some(false),
        ..Legend::default()
    });
    cs.chart.plot_vis_only = Some(true);
    cs.shape = shape_of(styles.graphic(chart.attr("chart:style-name")), false);
    // Axes.
    let axes: Vec<&XmlElement> = kids(plot, "chart:axis").collect();
    let x_axis = axes
        .iter()
        .find(|a| a.attr("chart:dimension") == Some("x"))
        .copied();
    let categories = x_axis
        .and_then(|a| kid(a, "chart:categories"))
        .and_then(|c| c.attr("table:cell-range-address"));
    // Series, grouped by class and axis.
    let mut groups: Vec<(Class, bool, Vec<Series>)> = Vec::new();
    let mut first_domain: Option<String> = None;
    for (i, s) in kids(plot, "chart:series").enumerate().take(255) {
        let class = match s.attr("chart:class").and_then(class_of) {
            // LibreOffice writes a doughnut's series as circles.
            Some(Class::Pie) if main_class == Class::Ring => Class::Ring,
            Some(c) => c,
            None => main_class,
        };
        let secondary = s.attr("chart:attached-axis") == Some("secondary-y");
        let sname = s.attr("chart:style-name");
        let scp = styles.chart(sname);
        let lineish = matches!(class, Class::Line | Class::Scatter | Class::Radar);
        let mut series = Series {
            idx: u32::try_from(i).unwrap_or(0),
            order: u32::try_from(i).unwrap_or(0),
            name: series_name(s, &local),
            shape: shape_of(styles.graphic(sname), lineish),
            data_labels: labels_of(scp),
            marker: if lineish { marker_of(scp) } else { None },
            smooth: (class == Class::Line || class == Class::Scatter).then_some(smooth),
            secondary,
            ..Series::default()
        };
        if lineish
            && series.shape.line.is_none()
            && let Some(c) = styles
                .graphic(sname)
                .and_then(|g| g.attr("draw:fill-color"))
                .and_then(hex_color)
        {
            series.shape.line = Some(Line {
                fill: Some(Fill::Solid(c)),
                ..Line::default()
            });
        }
        let values = s.attr("chart:values-cell-range-address");
        let domains: Vec<&str> = kids(s, "chart:domain")
            .filter_map(|d| d.attr("table:cell-range-address"))
            .collect();
        match class {
            Class::Scatter => {
                let x = domains
                    .first()
                    .map(|d| (*d).to_owned())
                    .or_else(|| first_domain.clone());
                if first_domain.is_none() {
                    first_domain.clone_from(&x);
                }
                series.categories = source(x.as_deref(), &local, true);
                series.values = source(values, &local, true);
            }
            Class::Bubble => {
                // Domains: the y values, then the x values.
                series.values = source(domains.first().copied(), &local, true);
                series.categories = source(domains.get(1).copied(), &local, true);
                series.bubble_sizes = source(values, &local, true);
            }
            _ => {
                series.categories = source(categories, &local, false);
                series.values = source(values, &local, true);
            }
        }
        // Points with their own look.
        let mut idx = 0u32;
        for p in kids(s, "chart:data-point") {
            let n = p
                .attr("chart:repeated")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(1)
                .clamp(1, 100_000);
            if let Some(st) = p.attr("chart:style-name") {
                let shape = shape_of(styles.graphic(Some(st)), lineish);
                let explosion = styles
                    .chart(Some(st))
                    .and_then(|c| c.attr("chart:pie-offset"))
                    .and_then(|v| v.parse::<u32>().ok());
                if !shape.is_empty() || explosion.is_some() {
                    for k in 0..n.min(1_000) {
                        series.points.push(DataPoint {
                            idx: idx + k,
                            shape: shape.clone(),
                            explosion,
                            ..DataPoint::default()
                        });
                    }
                }
            }
            idx = idx.saturating_add(n);
        }
        if let Some(o) = scp
            .and_then(|c| c.attr("chart:pie-offset"))
            .and_then(|v| v.parse::<u32>().ok())
        {
            series.explosion = Some(o);
        }
        match groups
            .iter_mut()
            .find(|(c, sec, _)| *c == class && *sec == secondary)
        {
            Some(g) => g.2.push(series),
            None => groups.push((class, secondary, vec![series])),
        }
    }
    if groups.is_empty() {
        return None;
    }
    let bars_horizontal = flag(pcp, "chart:vertical");
    let gap = pcp
        .and_then(|p| p.attr("chart:gap-width"))
        .and_then(|v| v.parse::<u32>().ok());
    let overlap = pcp
        .and_then(|p| p.attr("chart:overlap"))
        .and_then(|v| v.parse::<i32>().ok());
    let angle = pcp
        .and_then(|p| p.attr("chart:angle-offset"))
        .and_then(|v| v.parse::<i32>().ok());
    let has_secondary = groups.iter().any(|g| g.1);
    for (class, secondary, series) in groups {
        let axis_ids = match class {
            Class::Pie | Class::Ring => Vec::new(),
            _ if secondary => vec![3, 4],
            _ => vec![1, 2],
        };
        let kind = match class {
            Class::Bar => GroupKind::Bar {
                dir: if bars_horizontal {
                    BarDir::Bar
                } else {
                    BarDir::Column
                },
                grouping: Some(grouping.unwrap_or(Grouping::Clustered)),
                gap_width: gap,
                overlap: overlap.or(grouping.map(|_| 100)),
                three_d,
            },
            Class::Line => GroupKind::Line {
                grouping: Some(grouping.unwrap_or(Grouping::Standard)),
                markers: Some(true),
                three_d,
            },
            Class::Area => GroupKind::Area {
                grouping: Some(grouping.unwrap_or(Grouping::Standard)),
                three_d,
            },
            Class::Pie => GroupKind::Pie {
                // ODF counts counter-clockwise from three o'clock; Office clockwise from noon.
                first_slice_angle: angle.map(|a| (90 - a).rem_euclid(360).unsigned_abs()),
                three_d,
            },
            Class::Ring => GroupKind::Doughnut {
                first_slice_angle: angle.map(|a| (90 - a).rem_euclid(360).unsigned_abs()),
                hole_size: Some(50),
            },
            Class::Scatter => {
                let lines = series.iter().any(|s| {
                    s.shape
                        .line
                        .as_ref()
                        .is_none_or(|l| l.fill != Some(Fill::None))
                });
                let markers = series.iter().any(|s| {
                    s.marker
                        .as_ref()
                        .is_none_or(|m| m.symbol != Some(MarkerSymbol::None))
                });
                GroupKind::Scatter {
                    style: Some(match (lines, markers, smooth) {
                        (true, true, true) => ScatterStyle::SmoothMarker,
                        (true, false, true) => ScatterStyle::Smooth,
                        (true, true, false) => ScatterStyle::LineMarker,
                        (true, false, false) => ScatterStyle::Line,
                        _ => ScatterStyle::Marker,
                    }),
                }
            }
            Class::Bubble => GroupKind::Bubble {
                bubble_3d: Some(false),
                scale: Some(100),
                show_negative: Some(false),
                size_is_width: None,
            },
            Class::Radar => GroupKind::Radar {
                style: Some(RadarStyle::Marker),
            },
            Class::FilledRadar => GroupKind::Radar {
                style: Some(RadarStyle::Filled),
            },
        };
        cs.chart.plot_area.groups.push(ChartGroup {
            vary_colors: Some(matches!(class, Class::Pie | Class::Ring)),
            kind,
            series,
            data_labels: None,
            axis_ids,
            extras: Default::default(),
        });
    }
    // Axes of the groups.
    let circular = cs
        .chart
        .plot_area
        .groups
        .iter()
        .all(|g| g.kind.is_circular());
    if !circular {
        let xy = cs.chart.plot_area.groups.iter().any(|g| g.kind.is_xy());
        let horizontal = cs.chart.plot_area.groups.iter().any(|g| {
            matches!(
                g.kind,
                GroupKind::Bar {
                    dir: BarDir::Bar,
                    ..
                }
            )
        });
        let find = |name: &str| {
            axes.iter()
                .find(|a| a.attr("chart:name") == Some(name))
                .copied()
        };
        let mut out_axes: Vec<Axis> = Vec::new();
        let mut push =
            |el: Option<&XmlElement>, kind: AxisKind, id: i64, pos: AxisPos, cross: i64| {
                let mut a = Axis::new(kind, id, pos, cross);
                if let Some(el) = el {
                    read_axis(el, &styles, &mut a);
                } else {
                    a.delete = Some(true);
                }
                out_axes.push(a);
            };
        let x_kind = if xy {
            AxisKind::Value
        } else if x_axis.and_then(|a| a.attr("chartooo:axis-type")) == Some("date") {
            AxisKind::Date
        } else {
            AxisKind::Category
        };
        let (xp, yp) = if horizontal {
            (AxisPos::Left, AxisPos::Bottom)
        } else {
            (AxisPos::Bottom, AxisPos::Left)
        };
        push(x_axis.or_else(|| find("primary-x")), x_kind, 1, xp, 2);
        push(find("primary-y"), AxisKind::Value, 2, yp, 1);
        if has_secondary {
            let (xs, ys) = if horizontal {
                (AxisPos::Right, AxisPos::Top)
            } else {
                (AxisPos::Top, AxisPos::Right)
            };
            push(None, x_kind, 3, xs, 4);
            push(find("secondary-y"), AxisKind::Value, 4, ys, 3);
        }
        cs.chart.plot_area.axes = out_axes;
    }
    Some(cs)
}

fn read_axis(el: &XmlElement, styles: &Styles<'_>, a: &mut Axis) {
    let sname = el.attr("chart:style-name");
    let cp = styles.chart(sname);
    if let Some(cp) = cp {
        if cp.attr("chart:display-label") == Some("false") {
            a.tick_label_pos = Some(TickLabelPos::None);
        }
        if cp.attr("chart:logarithmic") == Some("true") {
            a.scaling.log_base = Some(10.0);
        }
        a.scaling.min = cp.attr("chart:minimum").and_then(|v| v.parse().ok());
        a.scaling.max = cp.attr("chart:maximum").and_then(|v| v.parse().ok());
        a.major_unit = cp
            .attr("chart:interval-major")
            .and_then(|v| v.parse().ok())
            .filter(|v: &f64| *v > 0.0);
        if cp.attr("chart:reverse-direction") == Some("true") {
            a.scaling.reversed = Some(true);
        }
        if cp.attr("chart:visible") == Some("false") {
            a.delete = Some(true);
        }
    }
    let line = shape_of(styles.graphic(sname), true);
    a.shape = line;
    for g in kids(el, "chart:grid") {
        let shape = shape_of(styles.graphic(g.attr("chart:style-name")), true);
        if g.attr("chart:class") == Some("minor") {
            a.minor_gridlines = Some(shape);
        } else {
            a.major_gridlines = Some(shape);
        }
    }
    a.title = kid(el, "chart:title").map(|t| title_of(t, styles));
}

// ------------------------------------------------------------------ writing

/// How a chart's data is referenced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataMode {
    /// Ranges of the spreadsheet's sheets (`.ods`).
    Sheet,
    /// The object's own local table (`.odt`, `.odp`).
    Local,
}

/// Whether a chart can be written as an ODF chart.
#[must_use]
pub fn supported(cs: &ChartSpace) -> bool {
    let groups = &cs.chart.plot_area.groups;
    !groups.is_empty()
        && groups.iter().all(|g| {
            !matches!(
                g.kind,
                GroupKind::Hierarchy { .. } | GroupKind::Statistic { .. } | GroupKind::Other(_)
            )
        })
}

fn class_name(k: &GroupKind) -> &'static str {
    match k {
        GroupKind::Line { .. } => "chart:line",
        GroupKind::Area { .. } => "chart:area",
        GroupKind::Pie { .. } => "chart:circle",
        GroupKind::Doughnut { .. } => "chart:ring",
        GroupKind::Scatter { .. } => "chart:scatter",
        GroupKind::Bubble { .. } => "chart:bubble",
        GroupKind::Radar {
            style: Some(RadarStyle::Filled),
        } => "chart:filled-radar",
        GroupKind::Radar { .. } => "chart:radar",
        _ => "chart:bar",
    }
}

fn hex_of(c: kiln_ooxml::drawingml::color::Rgba) -> String {
    let c = c.to_color();
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// The automatic styles of the object being written, deduplicated.
#[derive(Default)]
struct Autos {
    list: Vec<(String, El)>,
}

impl Autos {
    fn add(&mut self, prefix: &str, el: El) -> String {
        if let Some((n, _)) = self.list.iter().find(|(_, e)| *e == el) {
            return n.clone();
        }
        let name = format!("{prefix}{}", self.list.len() + 1);
        let mut el = el;
        el.set("style:name", name.clone());
        self.list.push((name.clone(), el));
        name
    }
}

fn style(children: Vec<El>) -> El {
    let mut s = El::new("style:style").with("style:family", "chart");
    for c in children {
        s.push_nonempty(c);
    }
    s
}

/// Graphic properties of a fill and outline (resolved colours).
fn graphic_el(
    shape: &ShapeProps,
    colors: &ChartColors,
    auto: Option<String>,
    line_chart: bool,
) -> El {
    let mut gp = El::new("style:graphic-properties");
    match &shape.fill {
        Some(Fill::None) => gp.set("draw:fill", "none"),
        Some(Fill::Solid(c)) => {
            gp.set("draw:fill", "solid");
            gp.set("draw:fill-color", hex_of(colors.resolve(c)));
        }
        Some(Fill::Gradient(g)) => {
            if let Some(s) = g.stops.first() {
                gp.set("draw:fill", "solid");
                gp.set("draw:fill-color", hex_of(colors.resolve(&s.color)));
            }
        }
        _ => {
            if let Some(a) = &auto
                && !line_chart
            {
                gp.set("draw:fill", "solid");
                gp.set("draw:fill-color", a.clone());
            }
        }
    }
    match &shape.line {
        Some(l) if l.fill == Some(Fill::None) => gp.set("draw:stroke", "none"),
        Some(l) => {
            gp.set("draw:stroke", "solid");
            match &l.fill {
                Some(Fill::Solid(c)) => gp.set("svg:stroke-color", hex_of(colors.resolve(c))),
                _ => {
                    if let Some(a) = &auto
                        && line_chart
                    {
                        gp.set("svg:stroke-color", a.clone());
                    }
                }
            }
            if let Some(w) = l.width {
                gp.set("svg:stroke-width", emu_str(w));
            }
        }
        None if line_chart => {
            gp.set("draw:stroke", "solid");
            if let Some(a) = &auto {
                gp.set("svg:stroke-color", a.clone());
                gp.set("draw:fill-color", a.clone());
            }
            gp.set("svg:stroke-width", "0.079cm");
        }
        None => {
            if !line_chart && auto.is_some() {
                gp.set("draw:stroke", "none");
            }
        }
    }
    gp
}

fn text_el(run: &kiln_charts::model::RunProps, colors: &ChartColors) -> El {
    let mut tp = El::new("style:text-properties");
    if let Some(s) = run.size {
        let pt = format!("{}pt", f64::from(s) / 100.0);
        tp.set("fo:font-size", pt.clone());
        tp.set("style:font-size-asian", pt.clone());
        tp.set("style:font-size-complex", pt);
    }
    if let Some(b) = run.bold {
        tp.set("fo:font-weight", if b { "bold" } else { "normal" });
    }
    if let Some(i) = run.italic {
        tp.set("fo:font-style", if i { "italic" } else { "normal" });
    }
    if let Some(c) = &run.color {
        tp.set("fo:color", hex_of(colors.resolve(c)));
    }
    if let Some(f) = &run.latin
        && !f.starts_with('+')
    {
        tp.set("fo:font-family", f.clone());
    }
    tp
}

/// The columns of the local table one series uses.
#[derive(Default)]
struct SeriesCols {
    values: Option<u32>,
    x: Option<u32>,
    sizes: Option<u32>,
    name_col: u32,
}

fn points_of(src: Option<&DataSource>) -> Vec<Cell> {
    let Some(src) = src else {
        return Vec::new();
    };
    (0..src.len().min(10_000))
        .map(|i| match src {
            DataSource::Num(n) => n
                .points
                .get(i)
                .copied()
                .flatten()
                .map_or(Cell::Empty, Cell::Num),
            _ => src.label(i).map_or(Cell::Empty, Cell::Str),
        })
        .collect()
}

fn local_address(col: u32, rows: usize) -> String {
    let c = kiln_formula::refs::column_to_letters(col);
    if rows <= 1 {
        format!("{LOCAL}.${c}$2")
    } else {
        format!("{LOCAL}.${c}$2:.${c}${}", rows + 1)
    }
}

fn sheet_address(src: Option<&DataSource>) -> Option<String> {
    src.and_then(DataSource::formula)
        .and_then(excel_ref_to_range_address)
}

/// Writes a chart object's `content.xml`: the chart with its styles and its local table.
/// `size` is the frame's in EMU; `colors` resolve theme colours and the automatic series colours.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn write_object(
    cs: &ChartSpace,
    colors: &ChartColors,
    size: (i64, i64),
    mode: DataMode,
) -> Vec<u8> {
    let groups = &cs.chart.plot_area.groups;
    let main = groups.first().map_or("chart:bar", |g| class_name(&g.kind));
    let mut autos = Autos::default();
    // The local table: categories in column A, then each series' columns.
    let all: Vec<(&ChartGroup, &Series)> = groups
        .iter()
        .flat_map(|g| g.series.iter().map(move |s| (g, s)))
        .collect();
    let categories = all
        .iter()
        .find(|(g, s)| !g.kind.is_xy() && s.categories.is_some())
        .and_then(|(_, s)| s.categories.as_ref());
    let mut columns: Vec<(String, Vec<Cell>)> = vec![(String::new(), points_of(categories))];
    let mut cols: Vec<SeriesCols> = Vec::new();
    for (g, s) in &all {
        let name = s
            .name
            .as_ref()
            .and_then(SeriesText::text)
            .unwrap_or_default()
            .to_owned();
        let mut sc = SeriesCols::default();
        let push = |columns: &mut Vec<(String, Vec<Cell>)>, label: &str, cells: Vec<Cell>| {
            columns.push((label.to_owned(), cells));
            u32::try_from(columns.len() - 1).unwrap_or(0)
        };
        match g.kind {
            GroupKind::Bubble { .. } => {
                sc.x = Some(push(&mut columns, "", points_of(s.categories.as_ref())));
                sc.values = Some(push(&mut columns, "", points_of(s.values.as_ref())));
                sc.sizes = Some(push(
                    &mut columns,
                    &name,
                    points_of(s.bubble_sizes.as_ref()),
                ));
                sc.name_col = sc.sizes.unwrap_or(0);
            }
            GroupKind::Scatter { .. } => {
                sc.x = Some(push(&mut columns, "", points_of(s.categories.as_ref())));
                sc.values = Some(push(&mut columns, &name, points_of(s.values.as_ref())));
                sc.name_col = sc.values.unwrap_or(0);
            }
            _ => {
                sc.values = Some(push(&mut columns, &name, points_of(s.values.as_ref())));
                sc.name_col = sc.values.unwrap_or(0);
            }
        }
        cols.push(sc);
    }
    let rows = columns.iter().map(|c| c.1.len()).max().unwrap_or(0).max(1);
    let addr = |col: Option<u32>, src: Option<&DataSource>| -> Option<String> {
        match mode {
            DataMode::Sheet => sheet_address(src).or_else(|| col.map(|c| local_address(c, rows))),
            DataMode::Local => col.map(|c| local_address(c, rows)),
        }
    };

    let mut body = XmlOut::fragment();
    let chart_style = autos.add(
        "ch",
        style(vec![graphic_el(&cs.shape, colors, None, false)]),
    );
    body.start("chart:chart")
        .attr("svg:width", emu_str(size.0.max(1)))
        .attr("svg:height", emu_str(size.1.max(1)))
        .attr("xlink:href", "..")
        .attr("xlink:type", "simple")
        .attr("chart:class", main)
        .attr("chart:style-name", chart_style);
    // Title.
    let title_text = |t: &Title| -> Option<String> {
        t.text
            .as_ref()
            .and_then(TitleText::plain)
            .map(str::to_owned)
    };
    let auto_title =
        cs.chart.title.is_none() && cs.chart.auto_title_deleted != Some(true) && all.len() == 1;
    let title = cs.chart.title.as_ref().and_then(title_text).or_else(|| {
        auto_title
            .then(|| {
                all.first()
                    .and_then(|(_, s)| s.name.as_ref())
                    .and_then(SeriesText::text)
            })
            .flatten()
            .map(str::to_owned)
    });
    if let Some(t) = title {
        let run = cs
            .chart
            .title
            .as_ref()
            .and_then(|t| match &t.text {
                Some(TitleText::Rich { run, .. }) => Some(run.clone()),
                _ => None,
            })
            .unwrap_or_else(|| kiln_charts::model::RunProps {
                size: Some(1_400),
                ..Default::default()
            });
        let ts = autos.add("ch", style(vec![text_el(&run, colors)]));
        body.start("chart:title").attr("chart:style-name", ts);
        for line in t.split('\n') {
            body.text_element("text:p", line);
        }
        body.end();
    }
    if let Some(l) = &cs.chart.legend {
        let pos = match l.position.unwrap_or(LegendPos::Right) {
            LegendPos::Bottom => "bottom",
            LegendPos::Top => "top",
            LegendPos::Left => "start",
            LegendPos::TopRight => "top-end",
            LegendPos::Right => "end",
        };
        let ls = autos.add(
            "ch",
            style(vec![
                El::new("style:text-properties").with("fo:font-size", "10pt"),
            ]),
        );
        body.start("chart:legend")
            .attr("chart:legend-position", pos)
            .attr("style:legend-expansion", "high")
            .attr("chart:style-name", ls)
            .end();
    }
    // Plot area.
    let first = groups.first();
    let mut pcp = El::new("style:chart-properties");
    if let Some(g) = first {
        match &g.kind {
            GroupKind::Bar {
                dir,
                grouping,
                gap_width,
                overlap,
                three_d,
            } => {
                if *dir == BarDir::Bar {
                    pcp.set("chart:vertical", "true");
                }
                if let Some(w) = gap_width {
                    pcp.set("chart:gap-width", w.to_string());
                }
                if let Some(o) = overlap {
                    pcp.set("chart:overlap", o.to_string());
                }
                if *three_d {
                    pcp.set("chart:three-dimensional", "true");
                }
                let _ = grouping;
            }
            GroupKind::Pie {
                first_slice_angle, ..
            }
            | GroupKind::Doughnut {
                first_slice_angle, ..
            } => {
                let a = i64::from(first_slice_angle.unwrap_or(0));
                pcp.set("chart:angle-offset", (90 - a).rem_euclid(360).to_string());
            }
            _ => {}
        }
        match g.kind.grouping() {
            Some(Grouping::Stacked) => pcp.set("chart:stacked", "true"),
            Some(Grouping::PercentStacked) => pcp.set("chart:percentage", "true"),
            _ => {}
        }
        if g.series.iter().any(|s| s.smooth == Some(true))
            || matches!(
                g.kind,
                GroupKind::Scatter {
                    style: Some(ScatterStyle::Smooth | ScatterStyle::SmoothMarker)
                }
            )
        {
            pcp.set("chart:interpolation", "cubic-spline");
        }
    }
    pcp.set("chart:auto-position", "true");
    pcp.set("chart:auto-size", "true");
    let ps = autos.add(
        "ch",
        style(vec![
            pcp,
            graphic_el(&cs.chart.plot_area.shape, colors, None, false),
        ]),
    );
    body.start("chart:plot-area").attr("chart:style-name", ps);
    let circular = groups.iter().all(|g| g.kind.is_circular());
    let cat_address = addr(Some(0), categories).filter(|_| categories.is_some());
    // Axes (pies still get an x axis for their categories).
    let axes = &cs.chart.plot_area.axes;
    let primary: Vec<&Axis> = axes
        .iter()
        .filter(|a| first.is_none_or(|g| g.axis_ids.contains(&a.id)))
        .collect();
    let x = primary
        .iter()
        .find(|a| {
            a.kind != AxisKind::Value
                || groups.iter().any(|g| g.kind.is_xy())
                    && a.position
                        .is_some_and(|p| matches!(p, AxisPos::Bottom | AxisPos::Top))
        })
        .copied();
    let y = primary
        .iter()
        .find(|a| a.kind == AxisKind::Value && x.is_none_or(|x| x.id != a.id))
        .copied();
    let write_axis = |body: &mut XmlOut,
                      autos: &mut Autos,
                      dim: &str,
                      name: &str,
                      a: Option<&Axis>,
                      cats: Option<&str>| {
        let mut cp = El::new("style:chart-properties");
        let visible = !circular && a.is_none_or(Axis::is_visible);
        cp.set(
            "chart:display-label",
            if visible && a.is_none_or(|a| a.tick_label_pos != Some(TickLabelPos::None)) {
                "true"
            } else {
                "false"
            },
        );
        if !visible {
            cp.set("chart:visible", "false");
        }
        if let Some(a) = a {
            if a.scaling.log_base.is_some() {
                cp.set("chart:logarithmic", "true");
            }
            if let Some(v) = a.scaling.min {
                cp.set("chart:minimum", kiln_ooxml::xml::format_number(v));
            }
            if let Some(v) = a.scaling.max {
                cp.set("chart:maximum", kiln_ooxml::xml::format_number(v));
            }
            if let Some(v) = a.major_unit {
                cp.set("chart:interval-major", kiln_ooxml::xml::format_number(v));
            }
            if a.is_reversed() {
                cp.set("chart:reverse-direction", "true");
            }
        }
        let mut gp = El::new("style:graphic-properties");
        gp.set("svg:stroke-color", "#b3b3b3");
        let st = autos.add(
            "ch",
            style(vec![
                cp,
                gp,
                El::new("style:text-properties").with("fo:font-size", "9pt"),
            ]),
        );
        body.start("chart:axis")
            .attr("chart:dimension", dim)
            .attr("chart:name", name)
            .attr("chart:style-name", st);
        if let Some(t) = a.and_then(|a| a.title.as_ref()).and_then(title_text) {
            let ts = autos.add(
                "ch",
                style(vec![
                    El::new("style:text-properties").with("fo:font-size", "10pt"),
                ]),
            );
            body.start("chart:title").attr("chart:style-name", ts);
            body.text_element("text:p", &t);
            body.end();
        }
        if let Some(c) = cats {
            body.start("chart:categories")
                .attr("table:cell-range-address", c)
                .end();
        }
        if let Some(a) = a {
            for (grid, class) in [(&a.major_gridlines, "major"), (&a.minor_gridlines, "minor")] {
                if grid.is_some() {
                    let gs = autos.add(
                        "ch",
                        style(vec![
                            El::new("style:graphic-properties").with("svg:stroke-color", "#d9d9d9"),
                        ]),
                    );
                    body.start("chart:grid")
                        .attr("chart:style-name", gs)
                        .attr("chart:class", class)
                        .end();
                }
            }
        }
        body.end();
    };
    write_axis(
        &mut body,
        &mut autos,
        "x",
        "primary-x",
        x,
        cat_address.as_deref(),
    );
    write_axis(&mut body, &mut autos, "y", "primary-y", y, None);
    let has_secondary = groups
        .iter()
        .any(|g| g.axis_ids.len() >= 2 && first.is_some_and(|f| f.axis_ids != g.axis_ids));
    if has_secondary {
        let sy = groups
            .iter()
            .skip(1)
            .flat_map(|g| g.axis_ids.iter())
            .filter_map(|id| {
                axes.iter()
                    .find(|a| a.id == *id && a.kind == AxisKind::Value)
            })
            .next();
        write_axis(&mut body, &mut autos, "y", "secondary-y", sy, None);
    }
    // Series.
    for (k, ((g, s), sc)) in all.iter().zip(&cols).enumerate() {
        let lineish = matches!(
            g.kind,
            GroupKind::Line { .. } | GroupKind::Scatter { .. } | GroupKind::Radar { .. }
        ) && !matches!(
            g.kind,
            GroupKind::Radar {
                style: Some(RadarStyle::Filled)
            }
        );
        let vary = g.vary_colors == Some(true) && g.kind.is_circular();
        let auto = hex_of(colors.series_color(s.idx as usize, cs.style));
        let mut cp = El::new("style:chart-properties");
        if let Some(m) = &s.marker {
            match m.symbol {
                Some(MarkerSymbol::None) => cp.set("chart:symbol-type", "none"),
                Some(sym) if sym != MarkerSymbol::Auto => {
                    cp.set("chart:symbol-type", "named-symbol");
                    cp.set(
                        "chart:symbol-name",
                        match sym {
                            MarkerSymbol::Square => "square",
                            MarkerSymbol::Diamond => "diamond",
                            MarkerSymbol::Triangle => "arrow-up",
                            MarkerSymbol::X => "x",
                            MarkerSymbol::Plus => "plus",
                            MarkerSymbol::Star => "star",
                            MarkerSymbol::Dash => "horizontal-bar",
                            _ => "circle",
                        },
                    );
                }
                _ => cp.set("chart:symbol-type", "automatic"),
            }
        } else if matches!(
            g.kind,
            GroupKind::Line {
                markers: Some(false),
                ..
            } | GroupKind::Radar {
                style: Some(RadarStyle::Standard)
            } | GroupKind::Scatter {
                style: Some(ScatterStyle::Line | ScatterStyle::Smooth)
            }
        ) {
            cp.set("chart:symbol-type", "none");
        } else if lineish {
            cp.set("chart:symbol-type", "automatic");
        }
        let labels = s.data_labels.as_ref().or(g.data_labels.as_ref());
        if let Some(l) = labels.filter(|l| l.shows_anything()) {
            let number = match (l.show_value == Some(true), l.show_percent == Some(true)) {
                (true, true) => Some("value-and-percentage"),
                (true, false) => Some("value"),
                (false, true) => Some("percentage"),
                _ => None,
            };
            if let Some(n) = number {
                cp.set("chart:data-label-number", n);
            }
            if l.show_category == Some(true) {
                cp.set("chart:data-label-text", "true");
            }
            if l.show_series == Some(true) {
                cp.set("loext:data-label-series", "true");
            }
        }
        if let Some(e) = s.explosion.filter(|e| *e > 0) {
            cp.set("chart:pie-offset", e.to_string());
        }
        let scatter_lines = !matches!(
            g.kind,
            GroupKind::Scatter {
                style: Some(ScatterStyle::Marker | ScatterStyle::None)
            }
        );
        let mut shape = s.shape.clone();
        if matches!(g.kind, GroupKind::Scatter { .. }) && !scatter_lines {
            shape.line = Some(Line {
                fill: Some(Fill::None),
                ..Line::default()
            });
        }
        let gp = graphic_el(
            &shape,
            colors,
            (!vary).then(|| auto.clone()),
            lineish && scatter_lines,
        );
        let ss = autos.add("ch", style(vec![cp, gp]));
        body.start("chart:series").attr("chart:style-name", ss);
        let values = match g.kind {
            GroupKind::Bubble { .. } => addr(sc.sizes, s.bubble_sizes.as_ref()),
            _ => addr(sc.values, s.values.as_ref()),
        };
        if let Some(v) = values {
            body.attr("chart:values-cell-range-address", v);
        }
        let name_ref = match (&s.name, mode) {
            (Some(SeriesText::Ref { formula, .. }), DataMode::Sheet) => {
                excel_ref_to_range_address(formula)
            }
            _ => None,
        };
        match name_ref {
            Some(r) => {
                body.attr("chart:label-cell-address", r);
            }
            None if s.name.is_some() => {
                body.attr(
                    "chart:label-cell-address",
                    format!(
                        "{LOCAL}.${}$1",
                        kiln_formula::refs::column_to_letters(sc.name_col)
                    ),
                );
            }
            None => {}
        }
        // A doughnut's series are circles, as LibreOffice writes them (it drops the name of a
        // `chart:ring` series).
        body.attr(
            "chart:class",
            match g.kind {
                GroupKind::Doughnut { .. } => "chart:circle",
                ref k => class_name(k),
            },
        );
        if has_secondary && first.is_some_and(|f| f.axis_ids != g.axis_ids) {
            body.attr("chart:attached-axis", "secondary-y");
        }
        match g.kind {
            GroupKind::Bubble { .. } => {
                for (col, src) in [
                    (sc.values, s.values.as_ref()),
                    (sc.x, s.categories.as_ref()),
                ] {
                    if let Some(a) = addr(col, src) {
                        body.start("chart:domain")
                            .attr("table:cell-range-address", a)
                            .end();
                    }
                }
            }
            GroupKind::Scatter { .. } => {
                if let Some(a) = addr(sc.x, s.categories.as_ref()) {
                    body.start("chart:domain")
                        .attr("table:cell-range-address", a)
                        .end();
                }
            }
            _ => {}
        }
        // Points: their own look, or one automatic colour each for varied pies.
        let n = s.values.as_ref().map_or(0, DataSource::len).min(10_000);
        for i in 0..n {
            let own = s.points.iter().find(|p| p.idx as usize == i);
            let ps = match own {
                Some(p) => {
                    let mut pcp = El::new("style:chart-properties");
                    if let Some(e) = p.explosion.filter(|e| *e > 0) {
                        pcp.set("chart:pie-offset", e.to_string());
                    }
                    let auto = vary.then(|| hex_of(colors.series_color(i, cs.style)));
                    Some(autos.add(
                        "ch",
                        style(vec![pcp, graphic_el(&p.shape, colors, auto, lineish)]),
                    ))
                }
                None if vary => {
                    let auto = hex_of(colors.series_color(i, cs.style));
                    Some(autos.add(
                        "ch",
                        style(vec![El::new("style:graphic-properties")
                            .with("draw:fill", "solid")
                            .with("draw:fill-color", auto)]),
                    ))
                }
                None => None,
            };
            body.start("chart:data-point");
            if let Some(ps) = ps {
                body.attr("chart:style-name", ps);
            }
            body.end();
        }
        body.end();
        let _ = k;
    }
    body.start("chart:wall")
        .attr(
            "chart:style-name",
            autos.add(
                "ch",
                style(vec![
                    El::new("style:graphic-properties")
                        .with("draw:fill", "none")
                        .with("draw:stroke", "none"),
                ]),
            ),
        )
        .end();
    body.end(); // plot-area
    // The local table.
    body.start("table:table").attr("table:name", LOCAL);
    body.start("table:table-header-columns");
    body.start("table:table-column").end();
    body.end();
    body.start("table:table-columns");
    body.start("table:table-column")
        .attr(
            "table:number-columns-repeated",
            columns.len().saturating_sub(1).max(1).to_string(),
        )
        .end();
    body.end();
    let cell = |body: &mut XmlOut, c: &Cell| {
        match c {
            Cell::Num(n) => {
                let v = kiln_ooxml::xml::format_number(*n);
                body.start("table:table-cell")
                    .attr("office:value-type", "float")
                    .attr("office:value", v.clone());
                body.text_element("text:p", &v);
            }
            Cell::Str(s) => {
                body.start("table:table-cell")
                    .attr("office:value-type", "string");
                body.text_element("text:p", s);
            }
            Cell::Empty => {
                body.start("table:table-cell");
                body.start("text:p").end();
            }
        }
        body.end();
    };
    body.start("table:table-header-rows");
    body.start("table:table-row");
    for (label, _) in &columns {
        cell(
            &mut body,
            &if label.is_empty() {
                Cell::Empty
            } else {
                Cell::Str(label.clone())
            },
        );
    }
    body.end();
    body.end();
    body.start("table:table-rows");
    for r in 0..rows {
        body.start("table:table-row");
        for (_, cells) in &columns {
            cell(&mut body, cells.get(r).unwrap_or(&Cell::Empty));
        }
        body.end();
    }
    body.end();
    body.end(); // table
    body.end(); // chart:chart

    let mut w = XmlOut::new();
    w.root(
        "office:document-content",
        &[
            "office", "style", "text", "table", "draw", "fo", "xlink", "svg", "chart", "number",
            "loext", "chartooo",
        ],
    )
    .attr("office:version", "1.3");
    w.start("office:automatic-styles");
    for (_, el) in &autos.list {
        el.write(&mut w);
    }
    w.end();
    w.start("office:body").start("office:chart");
    w.raw(&body.into_string());
    w.end().end();
    w.finish()
}

/// The `styles.xml` of a chart object.
#[must_use]
pub fn object_styles() -> Vec<u8> {
    let mut w = XmlOut::new();
    w.root("office:document-styles", &["office", "style"])
        .attr("office:version", "1.3");
    w.start("office:styles").end();
    w.finish()
}

/// Adds a chart object to a package being written: `Object N/` with its content and styles.
/// Returns the object's path (`./Object 3`) for `draw:object xlink:href`.
pub fn add_object(
    pkg: &mut crate::package::PackageWriter,
    cs: &ChartSpace,
    colors: &ChartColors,
    size: (i64, i64),
    mode: DataMode,
) -> String {
    let n = (1u32..)
        .find(|n| !pkg.has(&format!("Object {n}/content.xml")))
        .unwrap_or(1);
    let dir = format!("Object {n}");
    pkg.add(
        &format!("{dir}/content.xml"),
        "text/xml",
        write_object(cs, colors, size, mode),
    );
    pkg.add(&format!("{dir}/styles.xml"), "text/xml", object_styles());
    pkg.add_entry(&format!("{dir}/"), CHART_MEDIA_TYPE);
    format!("./{dir}")
}

/// Reads the chart object a `draw:object` points at (`./Object 1`), if it is a chart.
#[must_use]
pub fn read_linked(pkg: &crate::package::Package, href: &str) -> Option<ChartSpace> {
    let dir = href.trim_start_matches("./").trim_end_matches('/');
    if dir.is_empty() || dir.contains("..") || dir.starts_with('/') {
        return None;
    }
    let content = pkg.file(&format!("{dir}/content.xml"))?;
    let root = crate::xml::parse_part("chart content.xml", content).ok()?;
    let styles = pkg
        .file(&format!("{dir}/styles.xml"))
        .and_then(|b| crate::xml::parse_part("chart styles.xml", b).ok());
    read_object(&root, styles.as_ref())
}

/// The DrawingML part of a chart (what Write's media store and the Deck model keep).
#[must_use]
pub fn to_drawingml(cs: &ChartSpace) -> Vec<u8> {
    kiln_charts::write::write_chart(cs)
}

/// A chart from DrawingML bytes.
#[must_use]
pub fn from_drawingml(bytes: &[u8]) -> Option<ChartSpace> {
    kiln_charts::read::read_chart(bytes).ok()
}

/// Facts about a chart for tests: its types, series names and values, title and legend.
#[must_use]
pub fn facts(prefix: &str, cs: &ChartSpace) -> Vec<String> {
    let mut f = Vec::new();
    let c = &cs.chart;
    // The automatic title of a single-series chart is the series name.
    let series: Vec<&Series> = c.plot_area.groups.iter().flat_map(|g| &g.series).collect();
    let auto = (c.title.is_none() && c.auto_title_deleted != Some(true) && series.len() == 1)
        .then(|| series[0].name.as_ref().and_then(SeriesText::text))
        .flatten();
    if let Some(t) = c
        .title
        .as_ref()
        .and_then(|t| t.text.as_ref())
        .and_then(TitleText::plain)
        .or(auto)
    {
        f.push(format!("{prefix}/title={t}"));
    }
    f.push(format!(
        "{prefix}/legend={:?}",
        c.legend
            .as_ref()
            .map(|l| l.position.unwrap_or(LegendPos::Right))
    ));
    for (gi, g) in c.plot_area.groups.iter().enumerate() {
        let kind = match &g.kind {
            GroupKind::Bar { dir, .. } => format!("bar:{dir:?}"),
            GroupKind::Line { .. } => "line".to_owned(),
            GroupKind::Area { .. } => "area".to_owned(),
            GroupKind::Pie { .. } => "pie".to_owned(),
            GroupKind::Doughnut { .. } => "doughnut".to_owned(),
            GroupKind::Scatter { .. } => "scatter".to_owned(),
            GroupKind::Bubble { .. } => "bubble".to_owned(),
            GroupKind::Radar { style } => format!(
                "radar{}",
                if *style == Some(RadarStyle::Filled) {
                    ":filled"
                } else {
                    ""
                }
            ),
            other => format!("{other:?}").chars().take(12).collect(),
        };
        f.push(format!(
            "{prefix}/group{gi}={kind}:{:?}",
            g.kind
                .grouping()
                .filter(|g| matches!(g, Grouping::Stacked | Grouping::PercentStacked))
        ));
        for (si, s) in g.series.iter().enumerate() {
            let p = format!("{prefix}/group{gi}/series{si}");
            f.push(format!(
                "{p}/name={}",
                s.name
                    .as_ref()
                    .and_then(SeriesText::text)
                    .unwrap_or_default()
            ));
            if let Some(v) = &s.values {
                let vals: Vec<String> = (0..v.len())
                    .map(|i| {
                        v.number(i)
                            .map(kiln_ooxml::xml::format_number)
                            .unwrap_or_default()
                    })
                    .collect();
                f.push(format!("{p}/values={}", vals.join(",")));
            }
            if let Some(cat) = &s.categories {
                let vals: Vec<String> = (0..cat.len())
                    .map(|i| cat.label(i).unwrap_or_default())
                    .collect();
                f.push(format!("{p}/categories={}", vals.join(",")));
            }
        }
    }
    f
}
