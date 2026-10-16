// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reading `c:chartSpace` parts (DrawingML charts) into the model.
//!
//! The reader is total: every element it does not model is kept in the nearest [`Extras`], values
//! it cannot parse read as absent, and nothing in a hostile part can make it panic. Only a part
//! that is not XML or whose root is not `c:chartSpace` is refused.

use kiln_common::{KilnError, Result};
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::drawingml::fill::{Fill, find_fill};
use kiln_ooxml::drawingml::line::parse_line;
use kiln_ooxml::xml::{XmlElement, parse_xml};

use crate::model::*;
use crate::xmlx::{Extras, child_text, val, val_bool, val_int, val_num};

pub(crate) fn format_error(message: impl Into<String>) -> KilnError {
    KilnError::Format {
        format: "chart".into(),
        message: message.into(),
    }
}

/// Reads a chart part: `c:chartSpace` (DrawingML) or `cx:chartSpace` (chartex).
///
/// # Errors
///
/// `Format` when the bytes are not XML or the root is neither chart space.
pub fn read_chart(bytes: &[u8]) -> Result<ChartSpace> {
    let doc = parse_xml(bytes).map_err(|e| format_error(e.to_string()))?;
    match doc.root.name.as_str() {
        "c:chartSpace" => Ok(read_chart_space(&doc.root)),
        "cx:chartSpace" => Ok(crate::chartex::read_chartex(&doc.root)),
        other => Err(format_error(format!(
            "expected c:chartSpace or cx:chartSpace, found {other}"
        ))),
    }
}

fn u32_of(v: Option<i64>) -> Option<u32> {
    v.and_then(|v| u32::try_from(v).ok())
}

/// Reads a `c:chartSpace` element.
#[must_use]
pub fn read_chart_space(root: &XmlElement) -> ChartSpace {
    const KNOWN: &[&str] = &[
        "c:date1904",
        "c:lang",
        "c:roundedCorners",
        "c:style",
        "c:chart",
        "c:spPr",
        "c:txPr",
        "c:externalData",
    ];
    ChartSpace {
        format: PartFormat::Classic,
        date1904: val_bool(root, "c:date1904"),
        lang: val(root, "c:lang").map(str::to_owned),
        rounded_corners: val_bool(root, "c:roundedCorners"),
        style: u32_of(val_int(root, "c:style")),
        chart: root.child("c:chart").map(read_chart_el).unwrap_or_default(),
        shape: read_sp_pr(root.child("c:spPr")),
        text: read_tx_pr(root.child("c:txPr")),
        external_data: root.child("c:externalData").map(|e| ExternalData {
            rel_id: e.attr("r:id").unwrap_or_default().to_owned(),
            auto_update: val_bool(e, "c:autoUpdate"),
        }),
        extras: Extras::capture(root, KNOWN),
        root_attrs: root_attributes(root),
    }
}

/// The root's attributes plus every namespace declared deeper in the part, so kept elements
/// whose prefixes were declared on a regenerated ancestor stay well-formed.
pub(crate) fn root_attributes(root: &XmlElement) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = root
        .attrs
        .iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    let mut stack: Vec<&XmlElement> = root.elements().collect();
    let mut seen = 0usize;
    while let Some(el) = stack.pop() {
        seen += 1;
        if seen > 200_000 {
            break;
        }
        for (k, v) in el.attrs.iter() {
            if k.starts_with("xmlns:") && !out.iter().any(|(ok, _)| ok == k) {
                out.push((k.to_owned(), v.to_owned()));
            }
        }
        stack.extend(el.elements());
    }
    out
}

fn read_chart_el(el: &XmlElement) -> Chart {
    const KNOWN: &[&str] = &[
        "c:title",
        "c:autoTitleDeleted",
        "c:plotArea",
        "c:legend",
        "c:plotVisOnly",
        "c:dispBlanksAs",
    ];
    Chart {
        title: el.child("c:title").map(read_title),
        auto_title_deleted: val_bool(el, "c:autoTitleDeleted"),
        plot_area: el
            .child("c:plotArea")
            .map(read_plot_area)
            .unwrap_or_default(),
        legend: el.child("c:legend").map(read_legend),
        plot_vis_only: val_bool(el, "c:plotVisOnly"),
        disp_blanks_as: val(el, "c:dispBlanksAs").and_then(|v| match v {
            "gap" => Some(BlanksAs::Gap),
            "zero" => Some(BlanksAs::Zero),
            "span" => Some(BlanksAs::Span),
            _ => None,
        }),
        extras: Extras::capture(el, KNOWN),
    }
}

/// The chart group elements, modelled or kept.
pub(crate) const GROUP_ELEMENTS: &[&str] = &[
    "c:areaChart",
    "c:area3DChart",
    "c:lineChart",
    "c:line3DChart",
    "c:stockChart",
    "c:radarChart",
    "c:scatterChart",
    "c:pieChart",
    "c:pie3DChart",
    "c:doughnutChart",
    "c:barChart",
    "c:bar3DChart",
    "c:ofPieChart",
    "c:surfaceChart",
    "c:surface3DChart",
    "c:bubbleChart",
];

const AXIS_ELEMENTS: &[&str] = &["c:valAx", "c:catAx", "c:dateAx", "c:serAx"];

fn read_plot_area(el: &XmlElement) -> PlotArea {
    let mut known: Vec<&str> = vec!["c:layout", "c:spPr"];
    known.extend_from_slice(GROUP_ELEMENTS);
    known.extend_from_slice(AXIS_ELEMENTS);
    let groups: Vec<ChartGroup> = el
        .elements()
        .filter(|e| GROUP_ELEMENTS.contains(&e.name.as_str()))
        .map(read_group)
        .collect();
    let axes: Vec<Axis> = el
        .elements()
        .filter(|e| AXIS_ELEMENTS.contains(&e.name.as_str()))
        .map(read_axis)
        .collect();
    let mut pa = PlotArea {
        layout: el.child("c:layout").map(read_layout),
        groups,
        axes,
        shape: read_sp_pr(el.child("c:spPr")),
        extras: Extras::capture(el, &known),
    };
    mark_secondary(&mut pa);
    pa
}

/// Marks the series of groups that plot on the second pair of axes.
pub(crate) fn mark_secondary(pa: &mut PlotArea) {
    let primary: Option<Vec<i64>> = pa
        .groups
        .iter()
        .find(|g| !g.axis_ids.is_empty())
        .map(|g| g.axis_ids.clone());
    for g in &mut pa.groups {
        let secondary = primary
            .as_ref()
            .is_some_and(|p| !g.axis_ids.is_empty() && &g.axis_ids != p);
        for s in &mut g.series {
            s.secondary = secondary;
        }
    }
}

pub(crate) fn read_layout(el: &XmlElement) -> ManualLayout {
    let ml = el.child("c:manualLayout");
    let num = |name: &str| ml.and_then(|m| val_num(m, name));
    ManualLayout {
        inner: ml
            .and_then(|m| val(m, "c:layoutTarget"))
            .map(|v| v == "inner"),
        x: num("c:x"),
        y: num("c:y"),
        w: num("c:w"),
        h: num("c:h"),
        factor_mode: ml.and_then(|m| val(m, "c:xMode")) == Some("factor"),
        raw: Some(el.clone()),
    }
}

fn grouping(el: &XmlElement) -> Option<Grouping> {
    val(el, "c:grouping").and_then(|g| match g {
        "standard" => Some(Grouping::Standard),
        "clustered" => Some(Grouping::Clustered),
        "stacked" => Some(Grouping::Stacked),
        "percentStacked" => Some(Grouping::PercentStacked),
        _ => None,
    })
}

fn read_group(el: &XmlElement) -> ChartGroup {
    let name = el.name.as_str();
    let kind = match name {
        "c:barChart" | "c:bar3DChart" => GroupKind::Bar {
            dir: if val(el, "c:barDir") == Some("bar") {
                BarDir::Bar
            } else {
                BarDir::Column
            },
            grouping: grouping(el),
            gap_width: u32_of(val_int(el, "c:gapWidth")),
            overlap: val_int(el, "c:overlap").and_then(|v| i32::try_from(v).ok()),
            three_d: name == "c:bar3DChart",
        },
        "c:lineChart" | "c:line3DChart" => GroupKind::Line {
            grouping: grouping(el),
            markers: val_bool(el, "c:marker"),
            three_d: name == "c:line3DChart",
        },
        "c:areaChart" | "c:area3DChart" => GroupKind::Area {
            grouping: grouping(el),
            three_d: name == "c:area3DChart",
        },
        "c:pieChart" | "c:pie3DChart" => GroupKind::Pie {
            first_slice_angle: u32_of(val_int(el, "c:firstSliceAng")),
            three_d: name == "c:pie3DChart",
        },
        "c:doughnutChart" => GroupKind::Doughnut {
            first_slice_angle: u32_of(val_int(el, "c:firstSliceAng")),
            hole_size: u32_of(val_int(el, "c:holeSize")),
        },
        "c:scatterChart" => GroupKind::Scatter {
            style: val(el, "c:scatterStyle").and_then(|s| match s {
                "marker" => Some(ScatterStyle::Marker),
                "lineMarker" => Some(ScatterStyle::LineMarker),
                "line" => Some(ScatterStyle::Line),
                "smooth" => Some(ScatterStyle::Smooth),
                "smoothMarker" => Some(ScatterStyle::SmoothMarker),
                "none" => Some(ScatterStyle::None),
                _ => None,
            }),
        },
        "c:bubbleChart" => GroupKind::Bubble {
            bubble_3d: val_bool(el, "c:bubble3D"),
            scale: u32_of(val_int(el, "c:bubbleScale")),
            show_negative: val_bool(el, "c:showNegBubbles"),
            size_is_width: val(el, "c:sizeRepresents").map(|v| v == "w"),
        },
        "c:radarChart" => GroupKind::Radar {
            style: val(el, "c:radarStyle").and_then(|s| match s {
                "standard" => Some(RadarStyle::Standard),
                "marker" => Some(RadarStyle::Marker),
                "filled" => Some(RadarStyle::Filled),
                _ => None,
            }),
        },
        _ => GroupKind::Other(el.clone()),
    };
    if let GroupKind::Other(_) = kind {
        return ChartGroup {
            kind,
            vary_colors: None,
            series: Vec::new(),
            data_labels: None,
            axis_ids: el
                .children_named("c:axId")
                .filter_map(|a| a.attr_int("val"))
                .collect(),
            extras: Extras::default(),
        };
    }
    let known = group_known(&kind);
    ChartGroup {
        vary_colors: val_bool(el, "c:varyColors"),
        series: el
            .children_named("c:ser")
            .map(|s| read_series(s, &kind))
            .collect(),
        data_labels: el.child("c:dLbls").map(read_data_labels),
        axis_ids: el
            .children_named("c:axId")
            .filter_map(|a| a.attr_int("val"))
            .collect(),
        extras: Extras::capture(el, known),
        kind,
    }
}

/// The children of a group element the model covers.
pub(crate) fn group_known(kind: &GroupKind) -> &'static [&'static str] {
    match kind {
        GroupKind::Bar { .. } => &[
            "c:barDir",
            "c:grouping",
            "c:varyColors",
            "c:ser",
            "c:dLbls",
            "c:gapWidth",
            "c:overlap",
            "c:axId",
        ],
        GroupKind::Line { .. } => &[
            "c:grouping",
            "c:varyColors",
            "c:ser",
            "c:dLbls",
            "c:marker",
            "c:axId",
        ],
        GroupKind::Area { .. } => &["c:grouping", "c:varyColors", "c:ser", "c:dLbls", "c:axId"],
        GroupKind::Pie { .. } => &["c:varyColors", "c:ser", "c:dLbls", "c:firstSliceAng"],
        GroupKind::Doughnut { .. } => &[
            "c:varyColors",
            "c:ser",
            "c:dLbls",
            "c:firstSliceAng",
            "c:holeSize",
        ],
        GroupKind::Scatter { .. } => &[
            "c:scatterStyle",
            "c:varyColors",
            "c:ser",
            "c:dLbls",
            "c:axId",
        ],
        GroupKind::Bubble { .. } => &[
            "c:varyColors",
            "c:ser",
            "c:dLbls",
            "c:bubble3D",
            "c:bubbleScale",
            "c:showNegBubbles",
            "c:sizeRepresents",
            "c:axId",
        ],
        GroupKind::Radar { .. } => &["c:radarStyle", "c:varyColors", "c:ser", "c:dLbls", "c:axId"],
        GroupKind::Hierarchy { .. } | GroupKind::Other(_) => &[],
    }
}

/// The series children the model covers for a chart type (the rest are kept as extras, even
/// elements the schema does not allow for the type, so they are written back as read).
fn series_known(kind: &GroupKind) -> Vec<&'static str> {
    let mut k = vec!["c:idx", "c:order", "c:tx", "c:spPr", "c:dPt", "c:dLbls"];
    match kind {
        GroupKind::Bar { .. } => k.extend(["c:invertIfNegative", "c:trendline", "c:cat", "c:val"]),
        GroupKind::Line { .. } => {
            k.extend(["c:marker", "c:trendline", "c:cat", "c:val", "c:smooth"]);
        }
        GroupKind::Area { .. } => k.extend(["c:trendline", "c:cat", "c:val"]),
        GroupKind::Pie { .. } | GroupKind::Doughnut { .. } => {
            k.extend(["c:explosion", "c:cat", "c:val"]);
        }
        GroupKind::Scatter { .. } => {
            k.extend(["c:marker", "c:trendline", "c:xVal", "c:yVal", "c:smooth"]);
        }
        GroupKind::Bubble { .. } => k.extend([
            "c:invertIfNegative",
            "c:trendline",
            "c:xVal",
            "c:yVal",
            "c:bubbleSize",
            "c:bubble3D",
        ]),
        GroupKind::Radar { .. } => k.extend(["c:marker", "c:cat", "c:val"]),
        GroupKind::Hierarchy { .. } | GroupKind::Other(_) => {}
    }
    k
}

fn read_series(el: &XmlElement, kind: &GroupKind) -> Series {
    let known = series_known(kind);
    let has = |n: &str| known.contains(&n);
    let source = |a: &str, b: &str| {
        if has(a) {
            el.child(a).and_then(read_data_source)
        } else if has(b) {
            el.child(b).and_then(read_data_source)
        } else {
            None
        }
    };
    Series {
        idx: u32_of(val_int(el, "c:idx")).unwrap_or(0),
        order: u32_of(val_int(el, "c:order")).unwrap_or(0),
        name: el.child("c:tx").and_then(read_series_text),
        shape: read_sp_pr(el.child("c:spPr")),
        invert_if_negative: if has("c:invertIfNegative") {
            val_bool(el, "c:invertIfNegative")
        } else {
            None
        },
        explosion: if has("c:explosion") {
            u32_of(val_int(el, "c:explosion"))
        } else {
            None
        },
        marker: if has("c:marker") {
            el.child("c:marker").map(read_marker)
        } else {
            None
        },
        points: el
            .children_named("c:dPt")
            .map(|p| read_data_point(p, kind))
            .collect(),
        data_labels: el.child("c:dLbls").map(read_data_labels),
        trendlines: if has("c:trendline") {
            el.children_named("c:trendline")
                .filter_map(read_trendline)
                .collect()
        } else {
            Vec::new()
        },
        categories: source("c:cat", "c:xVal"),
        values: source("c:val", "c:yVal"),
        bubble_sizes: source("c:bubbleSize", "c:bubbleSize"),
        smooth: if has("c:smooth") {
            val_bool(el, "c:smooth")
        } else {
            None
        },
        bubble_3d: if has("c:bubble3D") {
            val_bool(el, "c:bubble3D")
        } else {
            None
        },
        secondary: false,
        extras: Extras::capture(el, &known),
        attrs: Vec::new(),
    }
}

fn read_series_text(tx: &XmlElement) -> Option<SeriesText> {
    if let Some(r) = tx.child("c:strRef") {
        let cache = r
            .child("c:strCache")
            .and_then(|c| c.children_named("c:pt").next())
            .and_then(|p| child_text(p, "c:v"));
        return Some(SeriesText::Ref {
            formula: child_text(r, "c:f").unwrap_or_default(),
            cache,
        });
    }
    tx.child("c:v").map(|v| SeriesText::Literal(v.text()))
}

/// Points of a cache (`ptCount` + sparse `pt idx`), capped so a hostile `ptCount` cannot allocate
/// gigabytes.
const MAX_POINTS: usize = 1 << 20;

fn point_count(cache: &XmlElement) -> usize {
    let declared = val_int(cache, "c:ptCount")
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0);
    let max_idx = cache
        .children_named("c:pt")
        .filter_map(|p| p.attr_int("idx"))
        .filter_map(|i| usize::try_from(i).ok())
        .map(|i| i + 1)
        .max()
        .unwrap_or(0);
    declared.max(max_idx).min(MAX_POINTS)
}

fn read_num_cache(cache: &XmlElement) -> (Option<String>, Vec<Option<f64>>) {
    let n = point_count(cache);
    let mut points = vec![None; n];
    for p in cache.children_named("c:pt") {
        let Some(i) = p.attr_int("idx").and_then(|i| usize::try_from(i).ok()) else {
            continue;
        };
        if let Some(slot) = points.get_mut(i) {
            *slot = child_text(p, "c:v")
                .and_then(|t| t.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite());
        }
    }
    (child_text(cache, "c:formatCode"), points)
}

fn read_str_points(cache: &XmlElement) -> Vec<Option<String>> {
    let n = point_count(cache);
    let mut points = vec![None; n];
    for p in cache.children_named("c:pt") {
        let Some(i) = p.attr_int("idx").and_then(|i| usize::try_from(i).ok()) else {
            continue;
        };
        if let Some(slot) = points.get_mut(i) {
            *slot = child_text(p, "c:v");
        }
    }
    points
}

/// Reads `c:cat`, `c:val`, `c:xVal`, `c:yVal` or `c:bubbleSize`.
pub(crate) fn read_data_source(el: &XmlElement) -> Option<DataSource> {
    if let Some(r) = el.child("c:numRef") {
        let cache = r.child("c:numCache");
        let (format_code, points) = cache.map(read_num_cache).unwrap_or_default();
        return Some(DataSource::Num(NumData {
            formula: child_text(r, "c:f"),
            format_code,
            points,
            has_cache: cache.is_some(),
            extras: Extras::capture(r, &["c:f", "c:numCache"]),
        }));
    }
    if let Some(l) = el.child("c:numLit") {
        let (format_code, points) = read_num_cache(l);
        return Some(DataSource::Num(NumData {
            formula: None,
            format_code,
            points,
            has_cache: true,
            extras: Extras::default(),
        }));
    }
    if let Some(r) = el.child("c:strRef") {
        let cache = r.child("c:strCache");
        return Some(DataSource::Str(StrData {
            formula: child_text(r, "c:f"),
            points: cache.map(read_str_points).unwrap_or_default(),
            has_cache: cache.is_some(),
            extras: Extras::capture(r, &["c:f", "c:strCache"]),
        }));
    }
    if let Some(l) = el.child("c:strLit") {
        return Some(DataSource::Str(StrData {
            formula: None,
            points: read_str_points(l),
            has_cache: true,
            extras: Extras::default(),
        }));
    }
    if let Some(r) = el.child("c:multiLvlStrRef") {
        let cache = r.child("c:multiLvlStrCache");
        let levels = cache
            .map(|c| {
                let n = point_count(c);
                c.children_named("c:lvl")
                    .map(|lvl| {
                        let mut pts = vec![None; n];
                        for p in lvl.children_named("c:pt") {
                            if let Some(slot) = p
                                .attr_int("idx")
                                .and_then(|i| usize::try_from(i).ok())
                                .and_then(|i| pts.get_mut(i))
                            {
                                *slot = child_text(p, "c:v");
                            }
                        }
                        pts
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Some(DataSource::MultiLevel(MultiLevelData {
            formula: child_text(r, "c:f").unwrap_or_default(),
            levels,
            has_cache: cache.is_some(),
            extras: Extras::capture(r, &["c:f", "c:multiLvlStrCache"]),
        }));
    }
    None
}

fn read_marker(el: &XmlElement) -> Marker {
    Marker {
        symbol: val(el, "c:symbol").and_then(MarkerSymbol::parse),
        size: u32_of(val_int(el, "c:size")),
        shape: read_sp_pr(el.child("c:spPr")),
        extras: Extras::capture(el, &["c:symbol", "c:size", "c:spPr"]),
    }
}

fn read_data_point(el: &XmlElement, kind: &GroupKind) -> DataPoint {
    let bar_or_bubble = matches!(kind, GroupKind::Bar { .. } | GroupKind::Bubble { .. });
    let marker_kind = matches!(
        kind,
        GroupKind::Line { .. } | GroupKind::Scatter { .. } | GroupKind::Radar { .. }
    );
    let bubble = matches!(kind, GroupKind::Bubble { .. });
    let pie = matches!(kind, GroupKind::Pie { .. } | GroupKind::Doughnut { .. });
    let mut known = vec!["c:idx", "c:spPr"];
    if bar_or_bubble {
        known.push("c:invertIfNegative");
    }
    if marker_kind {
        known.push("c:marker");
    }
    if bubble {
        known.push("c:bubble3D");
    }
    if pie {
        known.push("c:explosion");
    }
    DataPoint {
        idx: u32_of(val_int(el, "c:idx")).unwrap_or(0),
        invert_if_negative: if bar_or_bubble {
            val_bool(el, "c:invertIfNegative")
        } else {
            None
        },
        marker: if marker_kind {
            el.child("c:marker").map(read_marker)
        } else {
            None
        },
        bubble_3d: if bubble {
            val_bool(el, "c:bubble3D")
        } else {
            None
        },
        explosion: if pie {
            u32_of(val_int(el, "c:explosion"))
        } else {
            None
        },
        shape: read_sp_pr(el.child("c:spPr")),
        extras: Extras::capture(el, &known),
    }
}

pub(crate) const DLBLS_KNOWN: &[&str] = &[
    "c:dLbl",
    "c:delete",
    "c:numFmt",
    "c:spPr",
    "c:txPr",
    "c:dLblPos",
    "c:showLegendKey",
    "c:showVal",
    "c:showCatName",
    "c:showSerName",
    "c:showPercent",
    "c:showBubbleSize",
    "c:separator",
    "c:showLeaderLines",
];

fn read_data_labels(el: &XmlElement) -> DataLabels {
    DataLabels {
        delete: val_bool(el, "c:delete"),
        num_fmt: el.child("c:numFmt").map(read_num_fmt),
        shape: read_sp_pr(el.child("c:spPr")),
        text: read_tx_pr(el.child("c:txPr")),
        position: val(el, "c:dLblPos").and_then(LabelPosition::parse),
        show_legend_key: val_bool(el, "c:showLegendKey"),
        show_value: val_bool(el, "c:showVal"),
        show_category: val_bool(el, "c:showCatName"),
        show_series: val_bool(el, "c:showSerName"),
        show_percent: val_bool(el, "c:showPercent"),
        show_bubble_size: val_bool(el, "c:showBubbleSize"),
        separator: child_text(el, "c:separator"),
        show_leader_lines: val_bool(el, "c:showLeaderLines"),
        point_labels: el.children_named("c:dLbl").cloned().collect(),
        extras: Extras::capture(el, DLBLS_KNOWN),
    }
}

pub(crate) fn read_num_fmt(el: &XmlElement) -> NumFmt {
    NumFmt {
        code: el.attr("formatCode").unwrap_or("General").to_owned(),
        source_linked: el.attr_bool("sourceLinked"),
    }
}

fn read_trendline(el: &XmlElement) -> Option<Trendline> {
    Some(Trendline {
        name: child_text(el, "c:name"),
        shape: read_sp_pr(el.child("c:spPr")),
        kind: val(el, "c:trendlineType").and_then(TrendlineKind::parse)?,
        order: u32_of(val_int(el, "c:order")),
        period: u32_of(val_int(el, "c:period")),
        forward: val_num(el, "c:forward"),
        backward: val_num(el, "c:backward"),
        intercept: val_num(el, "c:intercept"),
        show_r_squared: val_bool(el, "c:dispRSqr"),
        show_equation: val_bool(el, "c:dispEq"),
        extras: Extras::capture(
            el,
            &[
                "c:name",
                "c:spPr",
                "c:trendlineType",
                "c:order",
                "c:period",
                "c:forward",
                "c:backward",
                "c:intercept",
                "c:dispRSqr",
                "c:dispEq",
            ],
        ),
    })
}

const AXIS_KNOWN: &[&str] = &[
    "c:axId",
    "c:scaling",
    "c:delete",
    "c:axPos",
    "c:majorGridlines",
    "c:minorGridlines",
    "c:title",
    "c:numFmt",
    "c:majorTickMark",
    "c:minorTickMark",
    "c:tickLblPos",
    "c:spPr",
    "c:txPr",
    "c:crossAx",
    "c:crosses",
    "c:crossesAt",
    "c:crossBetween",
    "c:majorUnit",
    "c:minorUnit",
    "c:auto",
    "c:lblAlgn",
    "c:lblOffset",
    "c:tickLblSkip",
    "c:tickMarkSkip",
    "c:noMultiLvlLbl",
    "c:baseTimeUnit",
    "c:majorTimeUnit",
    "c:minorTimeUnit",
];

fn read_axis(el: &XmlElement) -> Axis {
    let kind = match el.name.as_str() {
        "c:valAx" => AxisKind::Value,
        "c:dateAx" => AxisKind::Date,
        "c:serAx" => AxisKind::Series,
        _ => AxisKind::Category,
    };
    let scaling = el.child("c:scaling");
    Axis {
        kind,
        id: val_int(el, "c:axId").unwrap_or(0),
        scaling: Scaling {
            log_base: scaling.and_then(|s| val_num(s, "c:logBase")),
            reversed: scaling
                .and_then(|s| val(s, "c:orientation"))
                .map(|o| o == "maxMin"),
            max: scaling.and_then(|s| val_num(s, "c:max")),
            min: scaling.and_then(|s| val_num(s, "c:min")),
            extras: scaling
                .map(|s| Extras::capture(s, &["c:logBase", "c:orientation", "c:max", "c:min"]))
                .unwrap_or_default(),
        },
        delete: val_bool(el, "c:delete"),
        position: val(el, "c:axPos").and_then(AxisPos::parse),
        major_gridlines: el
            .child("c:majorGridlines")
            .map(|g| read_sp_pr(g.child("c:spPr"))),
        minor_gridlines: el
            .child("c:minorGridlines")
            .map(|g| read_sp_pr(g.child("c:spPr"))),
        title: el.child("c:title").map(read_title),
        num_fmt: el.child("c:numFmt").map(read_num_fmt),
        major_tick: val(el, "c:majorTickMark").and_then(TickMark::parse),
        minor_tick: val(el, "c:minorTickMark").and_then(TickMark::parse),
        tick_label_pos: val(el, "c:tickLblPos").and_then(TickLabelPos::parse),
        shape: read_sp_pr(el.child("c:spPr")),
        text: read_tx_pr(el.child("c:txPr")),
        cross_axis: val_int(el, "c:crossAx").unwrap_or(0),
        crosses: val_num(el, "c:crossesAt").map(Crosses::At).or_else(|| {
            val(el, "c:crosses").and_then(|c| match c {
                "autoZero" => Some(Crosses::AutoZero),
                "min" => Some(Crosses::Min),
                "max" => Some(Crosses::Max),
                _ => None,
            })
        }),
        cross_between: val(el, "c:crossBetween").map(|v| v == "between"),
        major_unit: val_num(el, "c:majorUnit").filter(|v| *v > 0.0),
        minor_unit: val_num(el, "c:minorUnit").filter(|v| *v > 0.0),
        auto: val_bool(el, "c:auto"),
        label_align: val(el, "c:lblAlgn").map(str::to_owned),
        label_offset: u32_of(val_int(el, "c:lblOffset")),
        tick_label_skip: u32_of(val_int(el, "c:tickLblSkip")),
        tick_mark_skip: u32_of(val_int(el, "c:tickMarkSkip")),
        no_multi_level: val_bool(el, "c:noMultiLvlLbl"),
        base_time_unit: val(el, "c:baseTimeUnit").and_then(TimeUnit::parse),
        major_time_unit: val(el, "c:majorTimeUnit").and_then(TimeUnit::parse),
        minor_time_unit: val(el, "c:minorTimeUnit").and_then(TimeUnit::parse),
        extras: Extras::capture(el, AXIS_KNOWN),
    }
}

fn read_title(el: &XmlElement) -> Title {
    let text = el.child("c:tx").and_then(|tx| {
        if let Some(rich) = tx.child("c:rich") {
            Some(TitleText::Rich {
                plain: rich_plain_text(rich),
                run: first_run_props(rich),
                raw: Some(rich.clone()),
            })
        } else {
            tx.child("c:strRef").map(|r| TitleText::Ref {
                formula: child_text(r, "c:f").unwrap_or_default(),
                cache: r
                    .child("c:strCache")
                    .and_then(|c| c.children_named("c:pt").next())
                    .and_then(|p| child_text(p, "c:v")),
            })
        }
    });
    Title {
        text,
        layout: el.child("c:layout").map(read_layout),
        overlay: val_bool(el, "c:overlay"),
        shape: read_sp_pr(el.child("c:spPr")),
        text_props: read_tx_pr(el.child("c:txPr")),
        extras: Extras::capture(el, &["c:tx", "c:layout", "c:overlay", "c:spPr", "c:txPr"]),
        attrs: Vec::new(),
    }
}

/// The text of a DrawingML text body (`c:rich`, `cx:rich`, `c:txPr`): paragraphs joined by `\n`.
pub(crate) fn rich_plain_text(body: &XmlElement) -> String {
    body.children_named("a:p")
        .map(|p| {
            p.elements()
                .map(|r| match r.name.as_str() {
                    "a:r" | "a:fld" => child_text(r, "a:t").unwrap_or_default(),
                    "a:br" => "\n".to_owned(),
                    _ => String::new(),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn first_run_props(body: &XmlElement) -> RunProps {
    let p = body.child("a:p");
    let rpr = p
        .and_then(|p| p.child("a:r"))
        .and_then(|r| r.child("a:rPr"))
        .or_else(|| p.and_then(|p| p.child_path(&["a:pPr", "a:defRPr"])));
    rpr.map(read_run_props).unwrap_or_default()
}

pub(crate) fn read_run_props(rpr: &XmlElement) -> RunProps {
    RunProps {
        size: rpr.attr_int("sz").and_then(|v| u32::try_from(v).ok()),
        bold: rpr.attr_bool("b"),
        italic: rpr.attr_bool("i"),
        color: match find_fill(rpr) {
            Some(Fill::Solid(c)) => Some(c),
            _ => None,
        },
        latin: rpr
            .child("a:latin")
            .and_then(|l| l.attr("typeface"))
            .map(str::to_owned),
    }
}

/// Reads `c:spPr` (or `cx:spPr`).
pub(crate) fn read_sp_pr(el: Option<&XmlElement>) -> ShapeProps {
    let Some(el) = el else {
        return ShapeProps::default();
    };
    ShapeProps {
        fill: find_fill(el),
        line: el.child("a:ln").map(parse_line),
        raw: Some(el.clone()),
    }
}

/// Reads `c:txPr` (or `cx:txPr`).
pub(crate) fn read_tx_pr(el: Option<&XmlElement>) -> TextProps {
    let Some(el) = el else {
        return TextProps::default();
    };
    let body = el.child("a:bodyPr");
    TextProps {
        run: el
            .child("a:p")
            .and_then(|p| p.child_path(&["a:pPr", "a:defRPr"]))
            .map(read_run_props)
            .unwrap_or_default(),
        rotation: body.and_then(|b| b.attr_int("rot")),
        vertical: body.and_then(|b| b.attr("vert")).map(str::to_owned),
        raw: Some(el.clone()),
    }
}

fn read_legend(el: &XmlElement) -> Legend {
    Legend {
        position: val(el, "c:legendPos").and_then(LegendPos::parse),
        entries: el.children_named("c:legendEntry").cloned().collect(),
        layout: el.child("c:layout").map(read_layout),
        overlay: val_bool(el, "c:overlay"),
        shape: read_sp_pr(el.child("c:spPr")),
        text: read_tx_pr(el.child("c:txPr")),
        extras: Extras::capture(
            el,
            &[
                "c:legendPos",
                "c:legendEntry",
                "c:layout",
                "c:overlay",
                "c:spPr",
                "c:txPr",
            ],
        ),
        attrs: Vec::new(),
    }
}

/// The colour of a solid fill, if the fill is one.
#[must_use]
pub fn solid_color(fill: Option<&Fill>) -> Option<&DrawingColor> {
    match fill {
        Some(Fill::Solid(c)) => Some(c),
        _ => None,
    }
}
