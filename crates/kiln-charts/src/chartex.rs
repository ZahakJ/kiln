// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cx:chartSpace` (chartex, Office 2016) for treemap and sunburst charts.
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

fn read_title(el: &XmlElement) -> Title {
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
        let mut layout_kind = None;
        let mut parent_label_layout = None;
        let mut series = Vec::new();
        for (order, s) in region.children_named("cx:series").enumerate() {
            let kind = match s.attr("layoutId") {
                Some("treemap") => HierarchyLayout::Treemap,
                Some("sunburst") => HierarchyLayout::Sunburst,
                _ => continue,
            };
            layout_kind.get_or_insert(kind);
            if parent_label_layout.is_none() {
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
            series.push(Series {
                idx: u32::try_from(order).unwrap_or(u32::MAX),
                order: u32::try_from(order).unwrap_or(u32::MAX),
                name,
                shape: read_sp_pr(s.child("cx:spPr")),
                data_labels: s.child("cx:dataLabels").map(read_labels),
                categories: block
                    .and_then(|(_, b)| b.categories.clone())
                    .map(DataSource::MultiLevel),
                values: block
                    .and_then(|(_, b)| b.values.clone())
                    .map(DataSource::Num),
                extras: Extras::capture(
                    s,
                    &[
                        "cx:tx",
                        "cx:spPr",
                        "cx:dataLabels",
                        "cx:dataId",
                        "cx:layoutPr",
                    ],
                ),
                attrs: attrs_of(s, &["layoutId"]),
                ..Series::default()
            });
        }
        if let Some(layout) = layout_kind {
            groups.push(ChartGroup {
                kind: GroupKind::Hierarchy {
                    layout,
                    parent_label_layout,
                },
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

fn data_block(id: usize, s: &Series) -> XmlElement {
    let cats = s.categories.as_ref().map(|c| {
        let (formula, levels): (Option<&str>, Vec<Vec<Option<String>>>) = match c {
            DataSource::MultiLevel(m) => (Some(&m.formula), m.levels.clone()),
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
            .with_attr("type", "size")
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
    let group = cs
        .chart
        .plot_area
        .groups
        .iter()
        .find(|g| matches!(g.kind, GroupKind::Hierarchy { .. }));
    let (layout, parent_label_layout) = match group.map(|g| &g.kind) {
        Some(GroupKind::Hierarchy {
            layout,
            parent_label_layout,
        }) => (*layout, parent_label_layout.clone()),
        _ => (HierarchyLayout::Treemap, None),
    };
    let series: Vec<&Series> = group.map(|g| g.series.iter().collect()).unwrap_or_default();
    let chart_data = XmlElement::new("cx:chartData")
        .with_opt_child(cs.external_data.as_ref().map(|e| {
            XmlElement::new("cx:externalData")
                .with_attr("r:id", e.rel_id.as_str())
                .with_opt_attr("cx:autoUpdate", e.auto_update)
        }))
        .with_children(series.iter().enumerate().map(|(i, s)| data_block(i, s)));
    let layout_id = match layout {
        HierarchyLayout::Treemap => "treemap",
        HierarchyLayout::Sunburst => "sunburst",
    };
    let series_els = series.iter().enumerate().map(|(i, s)| {
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
                (layout == HierarchyLayout::Treemap)
                    .then(|| {
                        parent_label_layout.as_ref().map(|p| {
                            XmlElement::new("cx:layoutPr")
                                .with_child(val_el("cx:parentLabelLayout", p.as_str()))
                        })
                    })
                    .flatten(),
            ],
            &s.extras,
        );
        set_attrs(&mut el, &s.attrs);
        el
    });
    let region = build(
        "cx:plotAreaRegion",
        &[],
        series_els.map(Some).collect(),
        &group.map(|g| g.extras.clone()).unwrap_or_default(),
    );
    let pa = &cs.chart.plot_area;
    let plot_area = build(
        "cx:plotArea",
        &[],
        vec![Some(region), sp_pr("cx:spPr", &pa.shape)],
        &pa.extras,
    );
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
