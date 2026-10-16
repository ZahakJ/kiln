// SPDX-License-Identifier: AGPL-3.0-or-later
//! Writing the model back to a `c:chartSpace` part.
//!
//! Children come out in schema order (ECMA-376 Part 1 §21.2.2), only the ones the series' chart
//! type allows are written (so a series moved from a line to a bar chart drops its marker), and
//! every kept unknown element goes back where it was read. Unedited `c:spPr`, `c:txPr`, `c:rich`
//! and `c:layout` elements are written back as they were.

use kiln_ooxml::drawingml::fill::serialize_fill;
use kiln_ooxml::drawingml::line::serialize_line;
use kiln_ooxml::xml::strip_invalid_xml_chars;
use kiln_ooxml::xml::{
    WriteOptions, XmlDocument, XmlElement, XmlNode, format_number, write_document,
};

use crate::model::*;
use crate::xmlx::{Extras, build, opt_val, text_el, val_el};

/// Serialises a chart part (classic or chartex, by [`ChartSpace::format`]) to UTF-8 bytes with
/// Office's XML declaration.
#[must_use]
pub fn write_chart(cs: &ChartSpace) -> Vec<u8> {
    let mut root = match cs.format {
        PartFormat::Classic => chart_space_element(cs),
        PartFormat::ChartEx => crate::chartex::chartex_element(cs),
    };
    sanitize(&mut root);
    let mut doc = XmlDocument::new(root);
    match write_document(&doc, &WriteOptions::default()) {
        Ok(text) => text.into_bytes(),
        Err(_) => {
            // A damaged part can carry kept elements whose namespace is no longer declared:
            // drop them rather than write a part Office cannot open.
            prune_undeclared(&mut doc.root);
            write_document(&doc, &WriteOptions::default())
                .map(String::into_bytes)
                .unwrap_or_default()
        }
    }
}

fn bad_name(name: &str) -> bool {
    name.is_empty()
        || name.chars().any(|c| {
            c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '&' | '"' | '\'' | '=')
        })
}

/// Removes elements and attributes whose prefix is neither declared nor one of Office's.
fn prune_undeclared(root: &mut XmlElement) {
    use kiln_ooxml::xml::namespaces::{namespace_for_prefix, split_qname};
    let bad: Vec<String> = kiln_ooxml::xml::undeclared_prefixes(root)
        .into_iter()
        .filter(|p| namespace_for_prefix(p).is_none())
        .collect();
    let is_bad = |name: &str| {
        let (p, _) = split_qname(name);
        bad_name(name) || bad.iter().any(|b| b == p)
    };
    let mut stack: Vec<&mut XmlElement> = vec![root];
    while let Some(e) = stack.pop() {
        e.children
            .retain(|c| !matches!(c, XmlNode::Element(child) if is_bad(&child.name)));
        let keep: Vec<(String, String)> = e
            .attrs
            .iter()
            .filter(|(k, _)| k.starts_with("xmlns") || !is_bad(k))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        e.attrs = keep.into_iter().collect();
        for c in &mut e.children {
            if let XmlNode::Element(child) = c {
                stack.push(child);
            }
        }
    }
}

/// The root attributes to write: the ones read, minus declarations that would rebind one of
/// Office's prefixes to another namespace (the writer always uses the canonical prefixes).
pub(crate) fn root_attrs(cs: &ChartSpace) -> Vec<(&str, String)> {
    cs.root_attrs
        .iter()
        .filter(|(k, v)| {
            let Some(prefix) = k.strip_prefix("xmlns:") else {
                return k != "xmlns";
            };
            kiln_ooxml::xml::namespaces::namespace_for_prefix(prefix).is_none_or(|uri| uri == v)
        })
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect()
}

/// The `c:chartSpace` element.
#[must_use]
pub fn chart_space_element(cs: &ChartSpace) -> XmlElement {
    build(
        "c:chartSpace",
        &root_attrs(cs),
        vec![
            opt_val("c:date1904", cs.date1904),
            opt_val("c:lang", cs.lang.as_deref()),
            opt_val("c:roundedCorners", cs.rounded_corners),
            opt_val("c:style", cs.style),
            Some(chart_element(&cs.chart)),
            sp_pr("c:spPr", &cs.shape),
            tx_pr("c:txPr", &cs.text),
            cs.external_data.as_ref().map(|e| {
                XmlElement::new("c:externalData")
                    .with_attr("r:id", e.rel_id.as_str())
                    .with_opt_child(opt_val("c:autoUpdate", e.auto_update))
            }),
        ],
        &cs.extras,
    )
}

fn chart_element(c: &Chart) -> XmlElement {
    build(
        "c:chart",
        &[],
        vec![
            c.title.as_ref().map(|t| title("c:title", t)),
            opt_val("c:autoTitleDeleted", c.auto_title_deleted),
            Some(plot_area(&c.plot_area)),
            c.legend.as_ref().map(legend),
            opt_val("c:plotVisOnly", c.plot_vis_only),
            opt_val(
                "c:dispBlanksAs",
                c.disp_blanks_as.map(|b| match b {
                    BlanksAs::Gap => "gap",
                    BlanksAs::Zero => "zero",
                    BlanksAs::Span => "span",
                }),
            ),
        ],
        &c.extras,
    )
}

fn plot_area(pa: &PlotArea) -> XmlElement {
    let mut kids: Vec<Option<XmlElement>> = vec![pa.layout.as_ref().map(layout)];
    kids.extend(pa.groups.iter().map(|g| Some(group(g))));
    kids.extend(pa.axes.iter().map(|a| Some(axis(a))));
    kids.push(sp_pr("c:spPr", &pa.shape));
    build("c:plotArea", &[], kids, &pa.extras)
}

pub(crate) fn layout(l: &ManualLayout) -> XmlElement {
    if let Some(raw) = &l.raw {
        return raw.clone();
    }
    let mode = if l.factor_mode { "factor" } else { "edge" };
    let any = l.x.is_some() || l.y.is_some() || l.w.is_some() || l.h.is_some();
    let mut el = XmlElement::new("c:layout");
    if any || l.inner.is_some() {
        el = el.with_child(
            XmlElement::new("c:manualLayout")
                .with_opt_child(opt_val(
                    "c:layoutTarget",
                    l.inner.map(|i| if i { "inner" } else { "outer" }),
                ))
                .with_opt_child(l.x.map(|_| val_el("c:xMode", mode)))
                .with_opt_child(l.y.map(|_| val_el("c:yMode", mode)))
                .with_opt_child(opt_val("c:x", l.x))
                .with_opt_child(opt_val("c:y", l.y))
                .with_opt_child(opt_val("c:w", l.w))
                .with_opt_child(opt_val("c:h", l.h)),
        );
    }
    el
}

fn grouping_str(g: Grouping) -> &'static str {
    match g {
        Grouping::Standard => "standard",
        Grouping::Clustered => "clustered",
        Grouping::Stacked => "stacked",
        Grouping::PercentStacked => "percentStacked",
    }
}

/// The element name of a group.
#[must_use]
pub fn group_element_name(kind: &GroupKind) -> &str {
    match kind {
        GroupKind::Bar { three_d: true, .. } => "c:bar3DChart",
        GroupKind::Bar { .. } => "c:barChart",
        GroupKind::Line { three_d: true, .. } => "c:line3DChart",
        GroupKind::Line { .. } => "c:lineChart",
        GroupKind::Area { three_d: true, .. } => "c:area3DChart",
        GroupKind::Area { .. } => "c:areaChart",
        GroupKind::Pie { three_d: true, .. } => "c:pie3DChart",
        GroupKind::Pie { .. } => "c:pieChart",
        GroupKind::Doughnut { .. } => "c:doughnutChart",
        GroupKind::Scatter { .. } => "c:scatterChart",
        GroupKind::Bubble { .. } => "c:bubbleChart",
        GroupKind::Radar { .. } => "c:radarChart",
        GroupKind::Hierarchy { .. } => "cx:plotAreaRegion",
        GroupKind::Other(el) => &el.name,
    }
}

fn axis_ids(g: &ChartGroup) -> Vec<Option<XmlElement>> {
    g.axis_ids
        .iter()
        .map(|id| Some(val_el("c:axId", *id)))
        .collect()
}

fn group(g: &ChartGroup) -> XmlElement {
    let name = group_element_name(&g.kind);
    let series = || g.series.iter().map(|s| Some(series(s, &g.kind)));
    let dlbls = g.data_labels.as_ref().map(data_labels);
    let vary = opt_val("c:varyColors", g.vary_colors);
    let mut kids: Vec<Option<XmlElement>> = Vec::new();
    match &g.kind {
        GroupKind::Bar {
            dir,
            grouping,
            gap_width,
            overlap,
            ..
        } => {
            kids.push(Some(val_el(
                "c:barDir",
                if *dir == BarDir::Bar { "bar" } else { "col" },
            )));
            kids.push(opt_val("c:grouping", grouping.map(grouping_str)));
            kids.push(vary);
            kids.extend(series());
            kids.push(dlbls);
            kids.push(opt_val("c:gapWidth", *gap_width));
            kids.push(opt_val("c:overlap", *overlap));
            kids.extend(axis_ids(g));
        }
        GroupKind::Line {
            grouping, markers, ..
        } => {
            kids.push(opt_val("c:grouping", grouping.map(grouping_str)));
            kids.push(vary);
            kids.extend(series());
            kids.push(dlbls);
            kids.push(opt_val("c:marker", *markers));
            kids.extend(axis_ids(g));
        }
        GroupKind::Area { grouping, .. } => {
            kids.push(opt_val("c:grouping", grouping.map(grouping_str)));
            kids.push(vary);
            kids.extend(series());
            kids.push(dlbls);
            kids.extend(axis_ids(g));
        }
        GroupKind::Pie {
            first_slice_angle,
            three_d,
        } => {
            kids.push(vary);
            kids.extend(series());
            kids.push(dlbls);
            if !three_d {
                kids.push(opt_val("c:firstSliceAng", *first_slice_angle));
            }
        }
        GroupKind::Doughnut {
            first_slice_angle,
            hole_size,
        } => {
            kids.push(vary);
            kids.extend(series());
            kids.push(dlbls);
            kids.push(opt_val("c:firstSliceAng", *first_slice_angle));
            kids.push(opt_val("c:holeSize", *hole_size));
        }
        GroupKind::Scatter { style } => {
            kids.push(opt_val(
                "c:scatterStyle",
                style.map(|s| match s {
                    ScatterStyle::Marker => "marker",
                    ScatterStyle::LineMarker => "lineMarker",
                    ScatterStyle::Line => "line",
                    ScatterStyle::Smooth => "smooth",
                    ScatterStyle::SmoothMarker => "smoothMarker",
                    ScatterStyle::None => "none",
                }),
            ));
            kids.push(vary);
            kids.extend(series());
            kids.push(dlbls);
            kids.extend(axis_ids(g));
        }
        GroupKind::Bubble {
            bubble_3d,
            scale,
            show_negative,
            size_is_width,
        } => {
            kids.push(vary);
            kids.extend(series());
            kids.push(dlbls);
            kids.push(opt_val("c:bubble3D", *bubble_3d));
            kids.push(opt_val("c:bubbleScale", *scale));
            kids.push(opt_val("c:showNegBubbles", *show_negative));
            kids.push(opt_val(
                "c:sizeRepresents",
                size_is_width.map(|w| if w { "w" } else { "area" }),
            ));
            kids.extend(axis_ids(g));
        }
        GroupKind::Radar { style } => {
            kids.push(Some(val_el(
                "c:radarStyle",
                match style.unwrap_or(RadarStyle::Marker) {
                    RadarStyle::Standard => "standard",
                    RadarStyle::Marker => "marker",
                    RadarStyle::Filled => "filled",
                },
            )));
            kids.push(vary);
            kids.extend(series());
            kids.push(dlbls);
            kids.extend(axis_ids(g));
        }
        GroupKind::Hierarchy { .. } => {
            // A hierarchy group in a classic part has no representation; the chartex writer
            // handles it. Write a column chart's skeleton so the part stays valid.
            kids.push(Some(val_el("c:barDir", "col")));
            kids.extend(series());
            kids.extend(axis_ids(g));
            return build("c:barChart", &[], kids, &Extras::default());
        }
        GroupKind::Other(el) => return el.clone(),
    }
    build(name, &[], kids, &g.extras)
}

fn series(s: &Series, kind: &GroupKind) -> XmlElement {
    let bar = matches!(kind, GroupKind::Bar { .. });
    let line = matches!(kind, GroupKind::Line { .. });
    let pie = matches!(kind, GroupKind::Pie { .. } | GroupKind::Doughnut { .. });
    let scatter = matches!(kind, GroupKind::Scatter { .. });
    let bubble = matches!(kind, GroupKind::Bubble { .. });
    let radar = matches!(kind, GroupKind::Radar { .. });
    let xy = scatter || bubble;
    let has_marker = line || scatter || radar;
    let mut kids: Vec<Option<XmlElement>> = vec![
        Some(val_el("c:idx", s.idx)),
        Some(val_el("c:order", s.order)),
        s.name.as_ref().map(series_text),
        sp_pr("c:spPr", &s.shape),
    ];
    if bar || bubble {
        kids.push(opt_val("c:invertIfNegative", s.invert_if_negative));
    }
    if has_marker {
        kids.push(s.marker.as_ref().map(marker));
    }
    if pie {
        kids.push(opt_val("c:explosion", s.explosion));
    }
    kids.extend(s.points.iter().map(|p| Some(data_point(p, kind))));
    kids.push(s.data_labels.as_ref().map(data_labels));
    if !(pie || radar) {
        kids.extend(s.trendlines.iter().map(|t| Some(trendline(t))));
    }
    let (cat, val) = if xy {
        ("c:xVal", "c:yVal")
    } else {
        ("c:cat", "c:val")
    };
    kids.push(s.categories.as_ref().map(|d| data_source(cat, d)));
    kids.push(s.values.as_ref().map(|d| data_source(val, d)));
    if bubble {
        kids.push(
            s.bubble_sizes
                .as_ref()
                .map(|d| data_source("c:bubbleSize", d)),
        );
        kids.push(opt_val("c:bubble3D", s.bubble_3d));
    }
    if line || scatter {
        kids.push(opt_val("c:smooth", s.smooth));
    }
    build("c:ser", &[], kids, &s.extras)
}

fn series_text(t: &SeriesText) -> XmlElement {
    let inner = match t {
        SeriesText::Ref { formula, cache } => XmlElement::new("c:strRef")
            .with_child(text_el("c:f", formula))
            .with_opt_child(cache.as_ref().map(|c| {
                XmlElement::new("c:strCache")
                    .with_child(val_el("c:ptCount", 1))
                    .with_child(
                        XmlElement::new("c:pt")
                            .with_attr("idx", 0)
                            .with_child(text_el("c:v", c)),
                    )
            })),
        SeriesText::Literal(v) => text_el("c:v", v),
    };
    XmlElement::new("c:tx").with_child(inner)
}

fn num_points(points: &[Option<f64>]) -> impl Iterator<Item = XmlElement> + '_ {
    points.iter().enumerate().filter_map(|(i, p)| {
        p.map(|v| {
            XmlElement::new("c:pt")
                .with_attr("idx", i)
                .with_child(text_el("c:v", &format_number(v)))
        })
    })
}

fn str_points(points: &[Option<String>]) -> impl Iterator<Item = XmlElement> + '_ {
    points.iter().enumerate().filter_map(|(i, p)| {
        p.as_ref().map(|v| {
            XmlElement::new("c:pt")
                .with_attr("idx", i)
                .with_child(text_el("c:v", v))
        })
    })
}

/// A data source as `name` (`c:cat`, `c:val`, `c:xVal`, …).
pub(crate) fn data_source(name: &str, d: &DataSource) -> XmlElement {
    let inner = match d {
        DataSource::Num(n) => {
            let cache = |el: &str| {
                XmlElement::new(el)
                    .with_opt_child(n.format_code.as_deref().map(|f| text_el("c:formatCode", f)))
                    .with_child(val_el("c:ptCount", n.points.len()))
                    .with_children(num_points(&n.points))
            };
            match &n.formula {
                Some(f) => build(
                    "c:numRef",
                    &[],
                    vec![
                        Some(text_el("c:f", f)),
                        n.has_cache.then(|| cache("c:numCache")),
                    ],
                    &n.extras,
                ),
                None => cache("c:numLit"),
            }
        }
        DataSource::Str(s) => {
            let cache = |el: &str| {
                XmlElement::new(el)
                    .with_child(val_el("c:ptCount", s.points.len()))
                    .with_children(str_points(&s.points))
            };
            match &s.formula {
                Some(f) => build(
                    "c:strRef",
                    &[],
                    vec![
                        Some(text_el("c:f", f)),
                        s.has_cache.then(|| cache("c:strCache")),
                    ],
                    &s.extras,
                ),
                None => cache("c:strLit"),
            }
        }
        DataSource::MultiLevel(m) => {
            let n = m.levels.first().map_or(0, Vec::len);
            build(
                "c:multiLvlStrRef",
                &[],
                vec![
                    Some(text_el("c:f", &m.formula)),
                    m.has_cache.then(|| {
                        XmlElement::new("c:multiLvlStrCache")
                            .with_child(val_el("c:ptCount", n))
                            .with_children(
                                m.levels
                                    .iter()
                                    .map(|l| XmlElement::new("c:lvl").with_children(str_points(l))),
                            )
                    }),
                ],
                &m.extras,
            )
        }
    };
    XmlElement::new(name).with_child(inner)
}

fn marker(m: &Marker) -> XmlElement {
    build(
        "c:marker",
        &[],
        vec![
            opt_val("c:symbol", m.symbol.map(MarkerSymbol::as_str)),
            opt_val("c:size", m.size),
            sp_pr("c:spPr", &m.shape),
        ],
        &m.extras,
    )
}

fn data_point(p: &DataPoint, kind: &GroupKind) -> XmlElement {
    let bar_or_bubble = matches!(kind, GroupKind::Bar { .. } | GroupKind::Bubble { .. });
    let marker_kind = matches!(
        kind,
        GroupKind::Line { .. } | GroupKind::Scatter { .. } | GroupKind::Radar { .. }
    );
    let pie = matches!(kind, GroupKind::Pie { .. } | GroupKind::Doughnut { .. });
    build(
        "c:dPt",
        &[],
        vec![
            Some(val_el("c:idx", p.idx)),
            if bar_or_bubble {
                opt_val("c:invertIfNegative", p.invert_if_negative)
            } else {
                None
            },
            if marker_kind {
                p.marker.as_ref().map(marker)
            } else {
                None
            },
            if matches!(kind, GroupKind::Bubble { .. }) {
                opt_val("c:bubble3D", p.bubble_3d)
            } else {
                None
            },
            if pie {
                opt_val("c:explosion", p.explosion)
            } else {
                None
            },
            sp_pr("c:spPr", &p.shape),
        ],
        &p.extras,
    )
}

pub(crate) fn num_fmt(name: &str, f: &NumFmt) -> XmlElement {
    XmlElement::new(name)
        .with_attr("formatCode", f.code.as_str())
        .with_opt_attr("sourceLinked", f.source_linked)
}

fn data_labels(d: &DataLabels) -> XmlElement {
    let mut kids: Vec<Option<XmlElement>> =
        d.point_labels.iter().map(|p| Some(p.clone())).collect();
    if d.delete == Some(true) {
        kids.push(Some(val_el("c:delete", true)));
    } else {
        kids.extend([
            opt_val("c:delete", d.delete),
            d.num_fmt.as_ref().map(|f| num_fmt("c:numFmt", f)),
            sp_pr("c:spPr", &d.shape),
            tx_pr("c:txPr", &d.text),
            opt_val("c:dLblPos", d.position.map(LabelPosition::as_str)),
            opt_val("c:showLegendKey", d.show_legend_key),
            opt_val("c:showVal", d.show_value),
            opt_val("c:showCatName", d.show_category),
            opt_val("c:showSerName", d.show_series),
            opt_val("c:showPercent", d.show_percent),
            opt_val("c:showBubbleSize", d.show_bubble_size),
            d.separator.as_deref().map(|s| text_el("c:separator", s)),
            opt_val("c:showLeaderLines", d.show_leader_lines),
        ]);
    }
    build("c:dLbls", &[], kids, &d.extras)
}

fn trendline(t: &Trendline) -> XmlElement {
    build(
        "c:trendline",
        &[],
        vec![
            t.name.as_deref().map(|n| text_el("c:name", n)),
            sp_pr("c:spPr", &t.shape),
            Some(val_el("c:trendlineType", t.kind.as_str())),
            opt_val("c:order", t.order),
            opt_val("c:period", t.period),
            opt_val("c:forward", t.forward),
            opt_val("c:backward", t.backward),
            opt_val("c:intercept", t.intercept),
            opt_val("c:dispRSqr", t.show_r_squared),
            opt_val("c:dispEq", t.show_equation),
        ],
        &t.extras,
    )
}

fn axis(a: &Axis) -> XmlElement {
    let scaling = build(
        "c:scaling",
        &[],
        vec![
            opt_val("c:logBase", a.scaling.log_base),
            opt_val(
                "c:orientation",
                a.scaling
                    .reversed
                    .map(|r| if r { "maxMin" } else { "minMax" }),
            ),
            opt_val("c:max", a.scaling.max),
            opt_val("c:min", a.scaling.min),
        ],
        &a.scaling.extras,
    );
    let gridlines = |name: &str, g: &Option<ShapeProps>| {
        g.as_ref()
            .map(|sp| XmlElement::new(name).with_opt_child(sp_pr("c:spPr", sp)))
    };
    let (crosses, crosses_at) = match a.crosses {
        Some(Crosses::At(v)) => (None, Some(v)),
        Some(Crosses::AutoZero) => (Some("autoZero"), None),
        Some(Crosses::Min) => (Some("min"), None),
        Some(Crosses::Max) => (Some("max"), None),
        None => (None, None),
    };
    let mut kids: Vec<Option<XmlElement>> = vec![
        Some(val_el("c:axId", a.id)),
        Some(scaling),
        opt_val("c:delete", a.delete),
        opt_val("c:axPos", a.position.map(AxisPos::as_str)),
        gridlines("c:majorGridlines", &a.major_gridlines),
        gridlines("c:minorGridlines", &a.minor_gridlines),
        a.title.as_ref().map(|t| title("c:title", t)),
        a.num_fmt.as_ref().map(|f| num_fmt("c:numFmt", f)),
        opt_val("c:majorTickMark", a.major_tick.map(TickMark::as_str)),
        opt_val("c:minorTickMark", a.minor_tick.map(TickMark::as_str)),
        opt_val("c:tickLblPos", a.tick_label_pos.map(TickLabelPos::as_str)),
        sp_pr("c:spPr", &a.shape),
        tx_pr("c:txPr", &a.text),
        Some(val_el("c:crossAx", a.cross_axis)),
        opt_val("c:crosses", crosses),
        opt_val("c:crossesAt", crosses_at),
    ];
    match a.kind {
        AxisKind::Value => {
            kids.extend([
                opt_val(
                    "c:crossBetween",
                    a.cross_between
                        .map(|b| if b { "between" } else { "midCat" }),
                ),
                opt_val("c:majorUnit", a.major_unit),
                opt_val("c:minorUnit", a.minor_unit),
            ]);
        }
        AxisKind::Category => {
            kids.extend([
                opt_val("c:auto", a.auto),
                opt_val("c:lblAlgn", a.label_align.as_deref()),
                opt_val("c:lblOffset", a.label_offset),
                opt_val("c:tickLblSkip", a.tick_label_skip),
                opt_val("c:tickMarkSkip", a.tick_mark_skip),
                opt_val("c:noMultiLvlLbl", a.no_multi_level),
            ]);
        }
        AxisKind::Date => {
            kids.extend([
                opt_val("c:auto", a.auto),
                opt_val("c:lblOffset", a.label_offset),
                opt_val("c:baseTimeUnit", a.base_time_unit.map(TimeUnit::as_str)),
                opt_val("c:majorUnit", a.major_unit),
                opt_val("c:majorTimeUnit", a.major_time_unit.map(TimeUnit::as_str)),
                opt_val("c:minorUnit", a.minor_unit),
                opt_val("c:minorTimeUnit", a.minor_time_unit.map(TimeUnit::as_str)),
            ]);
        }
        AxisKind::Series => {
            kids.extend([
                opt_val("c:tickLblSkip", a.tick_label_skip),
                opt_val("c:tickMarkSkip", a.tick_mark_skip),
            ]);
        }
    }
    build(a.kind.element(), &[], kids, &a.extras)
}

fn title(name: &str, t: &Title) -> XmlElement {
    let tx = t.text.as_ref().map(|text| match text {
        TitleText::Rich { raw: Some(r), .. } => XmlElement::new("c:tx").with_child(r.clone()),
        TitleText::Rich { plain, run, .. } => {
            XmlElement::new("c:tx").with_child(rich_body("c:rich", plain, run))
        }
        TitleText::Ref { formula, cache } => XmlElement::new("c:tx").with_child(
            XmlElement::new("c:strRef")
                .with_child(text_el("c:f", formula))
                .with_opt_child(cache.as_ref().map(|c| {
                    XmlElement::new("c:strCache")
                        .with_child(val_el("c:ptCount", 1))
                        .with_child(
                            XmlElement::new("c:pt")
                                .with_attr("idx", 0)
                                .with_child(text_el("c:v", c)),
                        )
                })),
        ),
    });
    build(
        name,
        &[],
        vec![
            tx,
            t.layout.as_ref().map(layout),
            opt_val("c:overlay", t.overlay),
            sp_pr("c:spPr", &t.shape),
            tx_pr("c:txPr", &t.text_props),
        ],
        &t.extras,
    )
}

/// A DrawingML text body named `name` holding `text` (one paragraph per line).
pub(crate) fn rich_body(name: &str, text: &str, run: &RunProps) -> XmlElement {
    XmlElement::new(name)
        .with_child(XmlElement::new("a:bodyPr"))
        .with_child(XmlElement::new("a:lstStyle"))
        .with_children(text.split('\n').map(|line| {
            XmlElement::new("a:p")
                .with_child(XmlElement::new("a:pPr").with_child(run_props("a:defRPr", run)))
                .with_child(
                    XmlElement::new("a:r")
                        .with_child(run_props("a:rPr", run).with_attr("lang", "en-US"))
                        .with_child(text_el("a:t", line)),
                )
        }))
}

pub(crate) fn run_props(name: &str, r: &RunProps) -> XmlElement {
    XmlElement::new(name)
        .with_opt_attr("sz", r.size)
        .with_opt_attr("b", r.bold)
        .with_opt_attr("i", r.italic)
        .with_opt_child(
            r.color
                .as_ref()
                .map(|c| serialize_fill(&kiln_ooxml::drawingml::fill::Fill::Solid(c.clone()))),
        )
        .with_opt_child(
            r.latin
                .as_deref()
                .map(|f| XmlElement::new("a:latin").with_attr("typeface", f)),
        )
}

fn legend(l: &Legend) -> XmlElement {
    let mut kids: Vec<Option<XmlElement>> =
        vec![opt_val("c:legendPos", l.position.map(LegendPos::as_str))];
    kids.extend(l.entries.iter().map(|e| Some(e.clone())));
    kids.extend([
        l.layout.as_ref().map(layout),
        opt_val("c:overlay", l.overlay),
        sp_pr("c:spPr", &l.shape),
        tx_pr("c:txPr", &l.text),
    ]);
    build("c:legend", &[], kids, &l.extras)
}

/// `c:spPr` (or `cx:spPr`) when anything is set.
pub(crate) fn sp_pr(name: &str, sp: &ShapeProps) -> Option<XmlElement> {
    if let Some(raw) = &sp.raw {
        let mut el = raw.clone();
        el.name = name.to_owned();
        return Some(el);
    }
    if sp.fill.is_none() && sp.line.is_none() {
        return None;
    }
    Some(
        XmlElement::new(name)
            .with_opt_child(sp.fill.as_ref().map(serialize_fill))
            .with_opt_child(sp.line.as_ref().map(serialize_line)),
    )
}

/// `c:txPr` (or `cx:txPr`) when anything is set.
pub(crate) fn tx_pr(name: &str, t: &TextProps) -> Option<XmlElement> {
    if let Some(raw) = &t.raw {
        let mut el = raw.clone();
        el.name = name.to_owned();
        return Some(el);
    }
    if t.is_empty() && t.vertical.is_none() {
        return None;
    }
    Some(
        XmlElement::new(name)
            .with_child(
                XmlElement::new("a:bodyPr")
                    .with_opt_attr("rot", t.rotation)
                    .with_opt_attr("vert", t.vertical.as_deref()),
            )
            .with_child(XmlElement::new("a:lstStyle"))
            .with_child(
                XmlElement::new("a:p")
                    .with_child(XmlElement::new("a:pPr").with_child(run_props("a:defRPr", &t.run)))
                    .with_child(XmlElement::new("a:endParaRPr").with_attr("lang", "en-US")),
            ),
    )
}

/// Removes characters XML 1.0 cannot hold from text and attribute values (a lenient read of a
/// damaged part can let control characters through; the written part must stay well-formed).
fn sanitize(el: &mut XmlElement) {
    let mut stack: Vec<&mut XmlElement> = vec![el];
    while let Some(e) = stack.pop() {
        for (_, v) in e.attrs.iter_mut() {
            if let std::borrow::Cow::Owned(clean) = strip_invalid_xml_chars(v) {
                *v = clean;
            }
        }
        for c in &mut e.children {
            match c {
                XmlNode::Text(t) => {
                    if let std::borrow::Cow::Owned(clean) = strip_invalid_xml_chars(&t.text) {
                        t.text = clean;
                    }
                }
                XmlNode::Element(child) => stack.push(child),
                _ => {}
            }
        }
    }
}
