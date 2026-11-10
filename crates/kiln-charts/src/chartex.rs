// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cx:chartSpace` (chartex, Office 2016): treemap and sunburst, and the statistic and flow
//! charts (histogram, Pareto, box and whisker, waterfall, funnel).
//!
//! The data lives in `cx:chartData/cx:data` blocks (`cx:strDim type="cat"` with one `cx:lvl` per
//! hierarchy level, leaf level first; `cx:numDim type="size"` for the values) and each
//! `cx:series` points at its block with `cx:dataId`. The reader maps a series and its block onto
//! the ordinary [`Series`] (multi-level categories and numeric values) inside a
//! [`GroupKind::Hierarchy`] group, so layout, rendering and editing treat it like any chart.

use kiln_ooxml::xml::{XmlElement, format_number};

use crate::model::*;
use crate::read::{read_num_fmt, read_sp_pr, read_tx_pr, rich_plain_text};
use crate::write::{num_fmt, sp_pr, tx_pr};
use crate::xmlx::{Extras, build, child_text, text_el, val, val_el};

const MAX_POINTS: usize = 1 << 20;

fn attrs_of(el: &XmlElement, skip: &[&str]) -> Vec<(String, String)> {
    el.attrs
        .iter()
        .filter(|(k, _)| !skip.contains(k))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

/// One `cx:data` block.
#[derive(Debug, Default)]
struct DataBlock {
    categories: Option<MultiLevelData>,
    values: Option<NumData>,
}

fn level_points(lvl: &XmlElement) -> Vec<Option<String>> {
    let declared = lvl
        .attr_int("ptCount")
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0);
    let max_idx = lvl
        .children_named("cx:pt")
        .filter_map(|p| p.attr_int("idx"))
        .filter_map(|i| usize::try_from(i).ok())
        .map(|i| i + 1)
        .max()
        .unwrap_or(0);
    let n = declared.max(max_idx).min(MAX_POINTS);
    let mut pts = vec![None; n];
    for p in lvl.children_named("cx:pt") {
        if let Some(slot) = p
            .attr_int("idx")
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| pts.get_mut(i))
        {
            *slot = Some(p.text());
        }
    }
    pts
}

fn read_block(data: &XmlElement) -> DataBlock {
    let mut block = DataBlock::default();
    for dim in data.elements() {
        match dim.name.as_str() {
            "cx:strDim" if block.categories.is_none() => {
                block.categories = Some(MultiLevelData {
                    formula: child_text(dim, "cx:f").unwrap_or_default(),
                    levels: dim.children_named("cx:lvl").map(level_points).collect(),
                    has_cache: dim.child("cx:lvl").is_some(),
                    extras: Extras::default(),
                });
            }
            "cx:numDim" if block.values.is_none() => {
                let lvl = dim.child("cx:lvl");
                block.values = Some(NumData {
                    formula: child_text(dim, "cx:f"),
                    format_code: lvl.and_then(|l| l.attr("formatCode")).map(str::to_owned),
                    points: lvl
                        .map(level_points)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|p| {
                            p.and_then(|t| t.trim().parse::<f64>().ok())
                                .filter(|v| v.is_finite())
                        })
                        .collect(),
                    has_cache: lvl.is_some(),
                    extras: Extras::default(),
                });
            }
            _ => {}
        }
    }
    block
}

fn read_text(tx: &XmlElement) -> Option<TitleText> {
    if let Some(rich) = tx.child("cx:rich") {
        return Some(TitleText::Rich {
            plain: rich_plain_text(rich),
            run: RunProps::default(),
            raw: Some(rich.clone()),
        });
    }
    let data = tx.child("cx:txData")?;
    match child_text(data, "cx:f") {
        Some(formula) => Some(TitleText::Ref {
            formula,
            cache: child_text(data, "cx:v"),
        }),
        None => child_text(data, "cx:v").map(|v| TitleText::Rich {
            plain: v,
            run: RunProps::default(),
            raw: None,
        }),
    }
}

pub(crate) fn read_title(el: &XmlElement) -> Title {
    Title {
        text: el.child("cx:tx").and_then(read_text),
        layout: None,
        overlay: el.attr_bool("overlay"),
        shape: read_sp_pr(el.child("cx:spPr")),
        text_props: read_tx_pr(el.child("cx:txPr")),
        extras: Extras::capture(el, &["cx:tx", "cx:spPr", "cx:txPr"]),
        attrs: attrs_of(el, &["overlay"]),
    }
}

fn read_labels(el: &XmlElement) -> DataLabels {
    let vis = el.child("cx:visibility");
    DataLabels {
        delete: None,
        num_fmt: el.child("cx:numFmt").map(read_num_fmt),
        shape: read_sp_pr(el.child("cx:spPr")),
        text: read_tx_pr(el.child("cx:txPr")),
        position: el.attr("pos").and_then(LabelPosition::parse),
        show_legend_key: None,
        show_value: vis.and_then(|v| v.attr_bool("value")),
        show_category: vis.and_then(|v| v.attr_bool("categoryName")),
        show_series: vis.and_then(|v| v.attr_bool("seriesName")),
        show_percent: None,
        show_bubble_size: None,
        separator: child_text(el, "cx:separator"),
        show_leader_lines: None,
        point_labels: Vec::new(),
        extras: Extras::capture(
            el,
            &[
                "cx:numFmt",
                "cx:spPr",
                "cx:txPr",
                "cx:visibility",
                "cx:separator",
            ],
        ),
    }
}

/// The `cx:series` children the model covers.
pub(crate) const SERIES_KNOWN: &[&str] = &[
    "cx:tx",
    "cx:spPr",
    "cx:dataLabels",
    "cx:dataId",
    "cx:layoutPr",
];

/// Reads a `cx:chartSpace` element.
#[must_use]
pub fn read_chartex(root: &XmlElement) -> ChartSpace {
    let chart_data = root.child("cx:chartData");
    let blocks: Vec<(i64, DataBlock)> = chart_data
        .map(|d| {
            d.children_named("cx:data")
                .map(|b| (b.attr_int("id").unwrap_or(0), read_block(b)))
                .collect()
        })
        .unwrap_or_default();
    let chart = root.child("cx:chart");
    let region = chart.and_then(|c| c.child_path(&["cx:plotArea", "cx:plotAreaRegion"]));
    let mut groups = Vec::new();
    if let Some(region) = region {
        let mut layout_kind: Option<GroupKind> = None;
        let mut parent_label_layout = None;
        let mut pareto = false;
        let mut series = Vec::new();
        for (order, s) in region.children_named("cx:series").enumerate() {
            let kind = match s.attr("layoutId") {
                Some("treemap") => GroupKind::Hierarchy {
                    layout: HierarchyLayout::Treemap,
                    parent_label_layout: None,
                },
                Some("sunburst") => GroupKind::Hierarchy {
                    layout: HierarchyLayout::Sunburst,
                    parent_label_layout: None,
                },
                Some("clusteredColumn") => GroupKind::Statistic {
                    layout: StatLayout::Histogram,
                },
                Some("boxWhisker") => GroupKind::Statistic {
                    layout: StatLayout::BoxWhisker,
                },
                Some("waterfall") => GroupKind::Statistic {
                    layout: StatLayout::Waterfall,
                },
                Some("funnel") => GroupKind::Statistic {
                    layout: StatLayout::Funnel,
                },
                // The Pareto line follows its owner's data; the writer makes it again.
                Some("paretoLine") => {
                    pareto = true;
                    continue;
                }
                _ => continue,
            };
            let statistic = matches!(kind, GroupKind::Statistic { .. });
            if layout_kind.is_none() {
                layout_kind = Some(kind);
            }
            if parent_label_layout.is_none() && !statistic {
                parent_label_layout = s
                    .child_path(&["cx:layoutPr", "cx:parentLabelLayout"])
                    .and_then(|p| p.attr("val"))
                    .map(str::to_owned);
            }
            let data_id = val(s, "cx:dataId").and_then(|v| v.parse::<i64>().ok());
            let block = blocks.iter().find(|(id, _)| Some(*id) == data_id);
            let name = s.child("cx:tx").and_then(read_text).map(|t| match t {
                TitleText::Ref { formula, cache } => SeriesText::Ref { formula, cache },
                TitleText::Rich { plain, .. } => SeriesText::Literal(plain),
            });
            // A statistic series keeps its options (`cx:layoutPr`) as read.
            let known: Vec<&str> = SERIES_KNOWN
                .iter()
                .copied()
                .filter(|k| !(statistic && *k == "cx:layoutPr"))
                .collect();
            series.push(Series {
                idx: u32::try_from(order).unwrap_or(u32::MAX),
                order: u32::try_from(order).unwrap_or(u32::MAX),
                name,
                shape: read_sp_pr(s.child("cx:spPr")),
                data_labels: s.child("cx:dataLabels").map(read_labels),
                categories: block.and_then(|(_, b)| b.categories.clone()).map(|c| {
                    if statistic && c.levels.len() == 1 {
                        DataSource::Str(StrData {
                            formula: (!c.formula.is_empty()).then(|| c.formula.clone()),
                            points: c.levels.into_iter().next().unwrap_or_default(),
                            has_cache: c.has_cache,
                            extras: Extras::default(),
                        })
                    } else {
                        DataSource::MultiLevel(c)
                    }
                }),
                values: block
                    .and_then(|(_, b)| b.values.clone())
                    .map(DataSource::Num),
                extras: Extras::capture(s, &known),
                attrs: attrs_of(s, &["layoutId"]),
                ..Series::default()
            });
        }
        if let Some(mut kind) = layout_kind {
            match &mut kind {
                GroupKind::Hierarchy {
                    parent_label_layout: p,
                    ..
                } => *p = parent_label_layout,
                GroupKind::Statistic { layout } if pareto => *layout = StatLayout::Pareto,
                _ => {}
            }
            groups.push(ChartGroup {
                kind,
                vary_colors: Some(true),
                series,
                data_labels: None,
                axis_ids: Vec::new(),
                extras: Extras::capture(region, &["cx:series"]),
            });
        }
    }
    let plot_area = chart.and_then(|c| c.child("cx:plotArea"));
    ChartSpace {
        format: PartFormat::ChartEx,
        date1904: None,
        lang: None,
        rounded_corners: None,
        style: None,
        chart: Chart {
            title: chart.and_then(|c| c.child("cx:title")).map(read_title),
            auto_title_deleted: None,
            plot_area: PlotArea {
                layout: None,
                groups,
                axes: Vec::new(),
                shape: read_sp_pr(plot_area.and_then(|p| p.child("cx:spPr"))),
                extras: plot_area
                    .map(|p| Extras::capture(p, &["cx:plotAreaRegion", "cx:spPr"]))
                    .unwrap_or_default(),
            },
            legend: chart.and_then(|c| c.child("cx:legend")).map(|l| Legend {
                position: l.attr("pos").and_then(LegendPos::parse),
                overlay: l.attr_bool("overlay"),
                shape: read_sp_pr(l.child("cx:spPr")),
                text: read_tx_pr(l.child("cx:txPr")),
                extras: Extras::capture(l, &["cx:spPr", "cx:txPr"]),
                attrs: attrs_of(l, &["pos", "overlay"]),
                ..Legend::default()
            }),
            plot_vis_only: None,
            disp_blanks_as: None,
            extras: chart
                .map(|c| Extras::capture(c, &["cx:title", "cx:plotArea", "cx:legend"]))
                .unwrap_or_default(),
        },
        shape: read_sp_pr(root.child("cx:spPr")),
        text: read_tx_pr(root.child("cx:txPr")),
        external_data: chart_data
            .and_then(|d| d.child("cx:externalData"))
            .map(|e| ExternalData {
                rel_id: e.attr("r:id").unwrap_or_default().to_owned(),
                auto_update: e.attr_bool("cx:autoUpdate"),
            }),
        extras: Extras::capture(root, &["cx:chartData", "cx:chart", "cx:spPr", "cx:txPr"]),
        root_attrs: crate::read::root_attributes(root),
    }
}

fn tx_element(t: &TitleText) -> XmlElement {
    match t {
        TitleText::Rich { raw: Some(r), .. } => XmlElement::new("cx:tx").with_child(r.clone()),
        TitleText::Rich { plain, .. } => XmlElement::new("cx:tx")
            .with_child(XmlElement::new("cx:txData").with_child(text_el("cx:v", plain))),
        TitleText::Ref { formula, cache } => XmlElement::new("cx:tx").with_child(
            XmlElement::new("cx:txData")
                .with_child(text_el("cx:f", formula))
                .with_opt_child(cache.as_deref().map(|c| text_el("cx:v", c))),
        ),
    }
}

fn str_level(points: &[Option<String>]) -> XmlElement {
    XmlElement::new("cx:lvl")
        .with_attr("ptCount", points.len())
        .with_children(points.iter().enumerate().filter_map(|(i, p)| {
            p.as_ref().map(|v| {
                XmlElement::new("cx:pt")
                    .with_attr("idx", i)
                    .with_text(v.as_str())
            })
        }))
}

/// Gives a new statistic chart Excel's default series options and axes (kept as extras, so
/// the model reads back what it wrote).
pub(crate) fn stat_defaults(cs: &mut ChartSpace) {
    let Some(g) = cs.chart.plot_area.groups.first_mut() else {
        return;
    };
    let GroupKind::Statistic { layout } = g.kind else {
        return;
    };
    for s in &mut g.series {
        if !s.extras.elements().any(|e| e.name == "cx:layoutPr")
            && let Some(l) = stat_layout_pr(layout)
        {
            s.extras.push_after("cx:dataId", l);
        }
    }
    let pa = &mut cs.chart.plot_area.extras;
    pa.remove_named("cx:axis");
    for (i, a) in stat_axes(layout).into_iter().enumerate() {
        pa.push_after(
            if i == 0 {
                "cx:plotAreaRegion"
            } else {
                "cx:axis"
            },
            a,
        );
    }
}

/// Excel's default options of a new statistic series.
pub(crate) fn stat_layout_pr(l: StatLayout) -> Option<XmlElement> {
    match l {
        StatLayout::Histogram => Some(
            XmlElement::new("cx:layoutPr")
                .with_child(XmlElement::new("cx:binning").with_attr("intervalClosed", "r")),
        ),
        StatLayout::Pareto => {
            Some(XmlElement::new("cx:layoutPr").with_child(XmlElement::new("cx:aggregation")))
        }
        StatLayout::BoxWhisker => Some(
            XmlElement::new("cx:layoutPr")
                .with_child(
                    XmlElement::new("cx:visibility")
                        .with_attr("meanLine", "0")
                        .with_attr("meanMarker", "1")
                        .with_attr("nonoutliers", "0")
                        .with_attr("outliers", "1"),
                )
                .with_child(
                    XmlElement::new("cx:statistics").with_attr("quartileMethod", "exclusive"),
                ),
        ),
        StatLayout::Waterfall | StatLayout::Funnel => None,
    }
}

/// Excel's axes of a new statistic chart.
pub(crate) fn stat_axes(l: StatLayout) -> Vec<XmlElement> {
    let cat = |gap: &str| {
        XmlElement::new("cx:axis")
            .with_attr("id", "0")
            .with_child(XmlElement::new("cx:catScaling").with_attr("gapWidth", gap))
            .with_child(XmlElement::new("cx:tickLabels"))
    };
    let val = |id: &str| {
        XmlElement::new("cx:axis")
            .with_attr("id", id)
            .with_child(XmlElement::new("cx:valScaling"))
            .with_child(XmlElement::new("cx:majorGridlines"))
            .with_child(XmlElement::new("cx:tickLabels"))
    };
    match l {
        StatLayout::Histogram => vec![cat("0"), val("1")],
        StatLayout::Pareto => vec![
            cat("0"),
            val("1"),
            XmlElement::new("cx:axis")
                .with_attr("id", "2")
                .with_child(
                    XmlElement::new("cx:valScaling")
                        .with_attr("max", "1")
                        .with_attr("min", "0"),
                )
                .with_child(XmlElement::new("cx:units").with_attr("unit", "percentage"))
                .with_child(XmlElement::new("cx:tickLabels")),
        ],
        StatLayout::BoxWhisker | StatLayout::Waterfall => vec![cat("0.5"), val("1")],
        StatLayout::Funnel => vec![
            cat("0.06"),
            XmlElement::new("cx:axis")
                .with_attr("id", "1")
                .with_attr("hidden", "1")
                .with_child(XmlElement::new("cx:valScaling")),
        ],
    }
}

fn data_block(id: usize, s: &Series, dim: &str) -> XmlElement {
    let cats = s.categories.as_ref().map(|c| {
        let (formula, levels): (Option<&str>, Vec<Vec<Option<String>>>) = match c {
            DataSource::MultiLevel(m) => (
                (!m.formula.is_empty()).then_some(m.formula.as_str()),
                m.levels.clone(),
            ),
            DataSource::Str(st) => (st.formula.as_deref(), vec![st.points.clone()]),
            DataSource::Num(n) => (
                n.formula.as_deref(),
                vec![
                    n.points
                        .iter()
                        .map(|p| p.map(format_number))
                        .collect::<Vec<_>>(),
                ],
            ),
        };
        XmlElement::new("cx:strDim")
            .with_attr("type", "cat")
            .with_opt_child(formula.map(|f| text_el("cx:f", f)))
            .with_children(levels.iter().map(|l| str_level(l)))
    });
    let vals = s.values.as_ref().map(|v| {
        let (formula, format, points): (Option<&str>, Option<&str>, Vec<Option<f64>>) = match v {
            DataSource::Num(n) => (
                n.formula.as_deref(),
                n.format_code.as_deref(),
                n.points.clone(),
            ),
            other => (
                other.formula(),
                None,
                (0..other.len()).map(|i| other.number(i)).collect(),
            ),
        };
        XmlElement::new("cx:numDim")
            .with_attr("type", dim)
            .with_opt_child(formula.map(|f| text_el("cx:f", f)))
            .with_child(
                XmlElement::new("cx:lvl")
                    .with_attr("ptCount", points.len())
                    .with_opt_attr("formatCode", format)
                    .with_children(points.iter().enumerate().filter_map(|(i, p)| {
                        p.map(|v| {
                            XmlElement::new("cx:pt")
                                .with_attr("idx", i)
                                .with_text(format_number(v))
                        })
                    })),
            )
    });
    XmlElement::new("cx:data")
        .with_attr("id", id)
        .with_opt_child(cats)
        .with_opt_child(vals)
}

fn labels_element(d: &DataLabels) -> XmlElement {
    let vis = XmlElement::new("cx:visibility")
        .with_opt_attr("seriesName", d.show_series)
        .with_opt_attr("categoryName", d.show_category)
        .with_opt_attr("value", d.show_value);
    let mut el = build(
        "cx:dataLabels",
        &[],
        vec![
            d.num_fmt.as_ref().map(|f| num_fmt("cx:numFmt", f)),
            sp_pr("cx:spPr", &d.shape),
            tx_pr("cx:txPr", &d.text),
            Some(vis),
            d.separator.as_deref().map(|s| text_el("cx:separator", s)),
        ],
        &d.extras,
    );
    if let Some(p) = d.position {
        el.attrs.insert(0, "pos", p.as_str());
    }
    el
}

fn set_attrs(el: &mut XmlElement, attrs: &[(String, String)]) {
    for (k, v) in attrs {
        el.attrs.set(k.clone(), v.clone());
    }
}

/// The `cx:chartSpace` element for a chart whose first group is a hierarchy.
#[must_use]
pub fn chartex_element(cs: &ChartSpace) -> XmlElement {
    let group = cs.chart.plot_area.groups.iter().find(|g| {
        matches!(
            g.kind,
            GroupKind::Hierarchy { .. } | GroupKind::Statistic { .. }
        )
    });
    let (layout, parent_label_layout, stat) = match group.map(|g| &g.kind) {
        Some(GroupKind::Hierarchy {
            layout,
            parent_label_layout,
        }) => (*layout, parent_label_layout.clone(), None),
        Some(GroupKind::Statistic { layout }) => (HierarchyLayout::Treemap, None, Some(*layout)),
        _ => (HierarchyLayout::Treemap, None, None),
    };
    let series: Vec<&Series> = group.map(|g| g.series.iter().collect()).unwrap_or_default();
    let dim = if stat.is_some() { "val" } else { "size" };
    let chart_data = XmlElement::new("cx:chartData")
        .with_opt_child(cs.external_data.as_ref().map(|e| {
            XmlElement::new("cx:externalData")
                .with_attr("r:id", e.rel_id.as_str())
                .with_opt_attr("cx:autoUpdate", e.auto_update)
        }))
        .with_children(
            series
                .iter()
                .enumerate()
                .map(|(i, s)| data_block(i, s, dim)),
        );
    let layout_id = match (stat, layout) {
        (Some(l), _) => l.layout_id(),
        (None, HierarchyLayout::Treemap) => "treemap",
        (None, HierarchyLayout::Sunburst) => "sunburst",
    };
    let mut series_els: Vec<XmlElement> = series
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let kept_layout = s.extras.elements().any(|e| e.name == "cx:layoutPr");
            let layout_pr = match stat {
                Some(l) if !kept_layout => stat_layout_pr(l),
                Some(_) => None,
                None => (layout == HierarchyLayout::Treemap)
                    .then(|| {
                        parent_label_layout.as_ref().map(|p| {
                            XmlElement::new("cx:layoutPr")
                                .with_child(val_el("cx:parentLabelLayout", p.as_str()))
                        })
                    })
                    .flatten(),
            };
            let mut el = build(
                "cx:series",
                &[("layoutId", layout_id.to_owned())],
                vec![
                    s.name.as_ref().map(|n| {
                        tx_element(&match n {
                            SeriesText::Ref { formula, cache } => TitleText::Ref {
                                formula: formula.clone(),
                                cache: cache.clone(),
                            },
                            SeriesText::Literal(v) => TitleText::Rich {
                                plain: v.clone(),
                                run: RunProps::default(),
                                raw: None,
                            },
                        })
                    }),
                    sp_pr("cx:spPr", &s.shape),
                    s.data_labels.as_ref().map(labels_element),
                    Some(val_el("cx:dataId", i)),
                    layout_pr,
                ],
                &s.extras,
            );
            set_attrs(&mut el, &s.attrs);
            el
        })
        .collect();
    if stat == Some(StatLayout::Pareto) {
        series_els.push(
            XmlElement::new("cx:series")
                .with_attr("layoutId", "paretoLine")
                .with_attr("ownerIdx", "0")
                .with_child(val_el("cx:axisId", 2)),
        );
    }
    let region = build(
        "cx:plotAreaRegion",
        &[],
        series_els.into_iter().map(Some).collect(),
        &group.map(|g| g.extras.clone()).unwrap_or_default(),
    );
    let pa = &cs.chart.plot_area;
    // A statistic chart's axes, as Excel writes them, unless kept from the file.
    let has_axes = pa.extras.elements().any(|e| e.name == "cx:axis");
    let mut kids = vec![Some(region)];
    if let Some(l) = stat.filter(|_| !has_axes) {
        kids.extend(stat_axes(l).into_iter().map(Some));
    }
    kids.push(sp_pr("cx:spPr", &pa.shape));
    let plot_area = build("cx:plotArea", &[], kids, &pa.extras);
    let title = cs.chart.title.as_ref().map(|t| {
        let mut el = build(
            "cx:title",
            &[],
            vec![
                t.text.as_ref().map(tx_element),
                sp_pr("cx:spPr", &t.shape),
                tx_pr("cx:txPr", &t.text_props),
            ],
            &t.extras,
        );
        set_attrs(&mut el, &t.attrs);
        if let Some(o) = t.overlay {
            el.attrs.set("overlay", if o { "1" } else { "0" });
        }
        el
    });
    let legend = cs.chart.legend.as_ref().map(|l| {
        let mut el = build(
            "cx:legend",
            &[],
            vec![sp_pr("cx:spPr", &l.shape), tx_pr("cx:txPr", &l.text)],
            &l.extras,
        );
        if let Some(p) = l.position {
            el.attrs.set("pos", p.as_str());
        }
        set_attrs(&mut el, &l.attrs);
        if let Some(o) = l.overlay {
            el.attrs.set("overlay", if o { "1" } else { "0" });
        }
        el
    });
    let chart = build(
        "cx:chart",
        &[],
        vec![title, Some(plot_area), legend],
        &cs.chart.extras,
    );
    let attrs = crate::write::root_attrs(cs);
    build(
        "cx:chartSpace",
        &attrs,
        vec![
            Some(chart_data),
            Some(chart),
            sp_pr("cx:spPr", &cs.shape),
            tx_pr("cx:txPr", &cs.text),
        ],
        &cs.extras,
    )
}
