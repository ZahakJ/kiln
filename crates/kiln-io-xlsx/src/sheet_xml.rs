// SPDX-License-Identifier: AGPL-3.0-or-later
//! Worksheet metadata ↔ SpreadsheetML elements, as parse/serialise pairs: views, column
//! properties, protection, AutoFilter and sort state, merged cells, conditional formatting
//! (with the Excel 2010 `x14` data bar and icon set options), data validation, page setup,
//! headers and footers and page breaks.
//!
//! Formulas are converted between file form (`_xlfn.` prefixes) and model form here.

use kiln_ooxml::xml::{XmlElement, XmlNode};
use kiln_sheet::address::{format_sqref, parse_sqref};
use kiln_sheet::axis::ColProps;
use kiln_sheet::meta::{
    AutoFilter, AxisPosition, BarDirection, CfKind, CfRule, Cfvo, CfvoType, ColumnFilter,
    ConditionalFormat, CustomCriterion, CustomIcon, DataBar, DataValidation, DateGroupItem,
    DateGrouping, DynamicFilter, ErrorStyle, FilterColumn, FilterOperator, Freeze, HeaderFooter,
    IconSet, IconSetName, Margins, Operator, Orientation, PasswordHash, PrintComments, PrintErrors,
    PrintSettings, Selection, SheetProtection, SheetView, SortBy, SortCondition, SortState,
    TimePeriod, ValidationType, ViewMode,
};
use kiln_sheet::num::F64;
use kiln_sheet::{CellPos, Color, Range};

use crate::formula_text::{from_file_formula, to_file_formula};
use crate::text::{cell_ref, fmt_num, parse_cell_ref};
use crate::xmlx::{
    abool_or, anum, au32, child_local, children_local, color_el, flag, flag_default_true,
    parse_color, set_attr, text_el,
};

/// No differential format (a conditional format rule that changes nothing).
pub const NO_DXF: u32 = u32::MAX;

/// The worksheet child order (CT_Worksheet).
pub(crate) const WORKSHEET_ORDER: &[&str] = &[
    "sheetPr",
    "dimension",
    "sheetViews",
    "sheetFormatPr",
    "cols",
    "sheetData",
    "sheetCalcPr",
    "sheetProtection",
    "protectedRanges",
    "scenarios",
    "autoFilter",
    "sortState",
    "dataConsolidate",
    "customSheetViews",
    "mergeCells",
    "phoneticPr",
    "conditionalFormatting",
    "dataValidations",
    "hyperlinks",
    "printOptions",
    "pageMargins",
    "pageSetup",
    "headerFooter",
    "rowBreaks",
    "colBreaks",
    "customProperties",
    "cellWatches",
    "ignoredErrors",
    "smartTags",
    "drawing",
    "legacyDrawing",
    "legacyDrawingHF",
    "drawingHF",
    "picture",
    "oleObjects",
    "controls",
    "webPublishItems",
    "tableParts",
    "extLst",
];

pub(crate) fn model_formula(text: &str) -> String {
    from_file_formula(text.trim().trim_start_matches('='), None)
}

pub(crate) fn file_formula(text: &str) -> String {
    to_file_formula(text, None)
}

fn range_of(text: &str) -> Option<Range> {
    let t = text.trim();
    match t.split_once(':') {
        Some((a, b)) => {
            let (r0, c0) = parse_cell_ref(a.as_bytes())?;
            let (r1, c1) = parse_cell_ref(b.as_bytes())?;
            Some(Range::new(r0, c0, r1, c1))
        }
        None => parse_cell_ref(t.as_bytes()).map(|(r, c)| Range::cell(r, c)),
    }
}

/// A range attribute (`A1:B2`, whole rows and columns too).
pub(crate) fn parse_ref(text: &str) -> Option<Range> {
    range_of(text).or_else(|| Range::parse_a1(text.trim()))
}

pub(crate) fn ref_text(r: &Range) -> String {
    if r.is_single_cell() {
        cell_ref(r.r0, r.c0)
    } else {
        r.to_a1(false)
    }
}

/// A space-separated range list (invalid parts are skipped).
pub(crate) fn parse_ranges(text: &str) -> Vec<Range> {
    parse_sqref(text).unwrap_or_else(|_| text.split_whitespace().filter_map(parse_ref).collect())
}

pub(crate) fn sqref_text(ranges: &[Range]) -> String {
    let parts: Vec<String> = ranges.iter().map(ref_text).collect();
    if parts.is_empty() {
        format_sqref(ranges)
    } else {
        parts.join(" ")
    }
}

// ── views ─────────────────────────────────────────────────────────────────

/// `twips → px` at 96 dpi.
fn twips_to_px(t: f64) -> u32 {
    (t / 15.0).round().clamp(0.0, f64::from(u32::MAX)) as u32
}

pub(crate) fn parse_sheet_view(sv: &XmlElement) -> SheetView {
    let mut v = SheetView {
        show_gridlines: abool_or(sv, "showGridLines", true),
        show_headings: abool_or(sv, "showRowColHeaders", true),
        show_zeros: abool_or(sv, "showZeros", true),
        show_formulas: abool_or(sv, "showFormulas", false),
        show_ruler: abool_or(sv, "showRuler", true),
        right_to_left: abool_or(sv, "rightToLeft", false),
        zoom: au32(sv, "zoomScale").map_or(100, |z| z.clamp(10, 400) as u16),
        mode: match sv.attr("view") {
            Some("pageBreakPreview") => ViewMode::PageBreakPreview,
            Some("pageLayout") => ViewMode::PageLayout,
            _ => ViewMode::Normal,
        },
        top_left: sv
            .attr("topLeftCell")
            .and_then(|t| parse_cell_ref(t.as_bytes()))
            .map_or(CellPos::new(0, 0), |(r, c)| CellPos::new(r, c)),
        ..SheetView::default()
    };
    if !abool_or(sv, "defaultGridColor", true)
        && let Some(id) = au32(sv, "colorId")
    {
        v.gridline_color = Some(Color::Indexed {
            index: u16::try_from(id).unwrap_or(64),
            tint: F64(0.0),
        });
    }
    let pane = sv.child("pane");
    let mut active_pane = "topLeft";
    if let Some(p) = pane {
        active_pane = p.attr("activePane").unwrap_or("topLeft");
        let xs = anum(p, "xSplit").unwrap_or(0.0);
        let ys = anum(p, "ySplit").unwrap_or(0.0);
        match p.attr("state") {
            Some("frozen" | "frozenSplit") => {
                v.freeze = Some(Freeze {
                    rows: ys.max(0.0) as u32,
                    cols: xs.max(0.0) as u32,
                });
            }
            _ => {
                if xs > 0.0 || ys > 0.0 {
                    v.split = Some((twips_to_px(xs), twips_to_px(ys)));
                }
            }
        }
    }
    let sels: Vec<&XmlElement> = sv.children_named("selection").collect();
    let sel = sels
        .iter()
        .find(|s| s.attr("pane").unwrap_or("topLeft") == active_pane)
        .or_else(|| sels.last());
    if let Some(s) = sel {
        let active = s
            .attr("activeCell")
            .and_then(|t| parse_cell_ref(t.as_bytes()))
            .map(|(r, c)| CellPos::new(r, c));
        let ranges = s.attr("sqref").map(parse_ranges).unwrap_or_default();
        if active.is_some() || !ranges.is_empty() {
            let active = active
                .or_else(|| ranges.first().map(|r| CellPos::new(r.r0, r.c0)))
                .unwrap_or_default();
            v.selection = Selection {
                ranges: if ranges.is_empty() {
                    vec![Range::cell(active.row, active.col)]
                } else {
                    ranges
                },
                active,
            };
        }
    }
    v
}

pub(crate) fn sheet_view_el(v: &SheetView, tab_selected: bool) -> XmlElement {
    let mut e = XmlElement::new("sheetView");
    if tab_selected {
        e.attrs.set("tabSelected", "1");
    }
    set_attr(&mut e, "showFormulas", flag(v.show_formulas));
    set_attr(&mut e, "showGridLines", flag_default_true(v.show_gridlines));
    set_attr(
        &mut e,
        "showRowColHeaders",
        flag_default_true(v.show_headings),
    );
    set_attr(&mut e, "showZeros", flag_default_true(v.show_zeros));
    set_attr(&mut e, "rightToLeft", flag(v.right_to_left));
    set_attr(&mut e, "showRuler", flag_default_true(v.show_ruler));
    match v.mode {
        ViewMode::Normal => {}
        ViewMode::PageBreakPreview => e.attrs.set("view", "pageBreakPreview"),
        ViewMode::PageLayout => e.attrs.set("view", "pageLayout"),
    }
    if v.top_left != CellPos::new(0, 0) {
        e.attrs
            .set("topLeftCell", cell_ref(v.top_left.row, v.top_left.col));
    }
    if let Some(Color::Indexed { index, .. }) = v.gridline_color {
        e.attrs.set("defaultGridColor", "0");
        e.attrs.set("colorId", index.to_string());
    }
    if v.zoom != 100 {
        e.attrs.set("zoomScale", v.zoom.to_string());
    }
    e.attrs.set("workbookViewId", "0");
    let mut pane_name = None;
    if let Some(f) = v.freeze.filter(|f| f.rows > 0 || f.cols > 0) {
        let name = match (f.rows > 0, f.cols > 0) {
            (true, true) => "bottomRight",
            (true, false) => "bottomLeft",
            _ => "topRight",
        };
        let mut p = XmlElement::new("pane");
        if f.cols > 0 {
            p.attrs.set("xSplit", f.cols.to_string());
        }
        if f.rows > 0 {
            p.attrs.set("ySplit", f.rows.to_string());
        }
        p.attrs.set(
            "topLeftCell",
            cell_ref(
                f.rows.max(v.top_left.row.saturating_add(f.rows)),
                f.cols.max(v.top_left.col.saturating_add(f.cols)),
            ),
        );
        p.attrs.set("activePane", name);
        p.attrs.set("state", "frozen");
        e.children.push(XmlNode::Element(p));
        pane_name = Some(name);
    } else if let Some((x, y)) = v.split.filter(|(x, y)| *x > 0 || *y > 0) {
        let name = match (y > 0, x > 0) {
            (true, true) => "bottomRight",
            (true, false) => "bottomLeft",
            _ => "topRight",
        };
        let mut p = XmlElement::new("pane");
        if x > 0 {
            p.attrs.set("xSplit", (u64::from(x) * 15).to_string());
        }
        if y > 0 {
            p.attrs.set("ySplit", (u64::from(y) * 15).to_string());
        }
        p.attrs.set("activePane", name);
        e.children.push(XmlNode::Element(p));
        pane_name = Some(name);
    }
    let s = &v.selection;
    let default_sel = *s == Selection::default();
    if !default_sel || pane_name.is_some() {
        let mut sel = XmlElement::new("selection");
        if let Some(p) = pane_name {
            sel.attrs.set("pane", p);
        }
        sel.attrs
            .set("activeCell", cell_ref(s.active.row, s.active.col));
        sel.attrs.set("sqref", sqref_text(&s.ranges));
        e.children.push(XmlNode::Element(sel));
    }
    e
}

// ── columns ───────────────────────────────────────────────────────────────

pub(crate) fn parse_cols(cols: &XmlElement) -> Vec<(u32, ColProps)> {
    let mut out = Vec::new();
    for c in cols.children_named("col") {
        let (Some(min), Some(max)) = (au32(c, "min"), au32(c, "max")) else {
            continue;
        };
        if min == 0 || max < min {
            continue;
        }
        let max = max.min(kiln_sheet::MAX_COLS);
        let props = ColProps {
            width: anum(c, "width").filter(|w| *w >= 0.0).map(F64),
            custom_width: abool_or(c, "customWidth", false),
            best_fit: abool_or(c, "bestFit", false),
            hidden: abool_or(c, "hidden", false),
            outline_level: au32(c, "outlineLevel").map_or(0, |l| l.min(7) as u8),
            collapsed: abool_or(c, "collapsed", false),
            style: au32(c, "style").filter(|s| *s != 0),
        };
        if props == ColProps::default() {
            continue;
        }
        for i in min..=max {
            out.push((i - 1, props));
        }
    }
    out
}

pub(crate) fn cols_el(cols: &[(u32, ColProps)]) -> Option<XmlElement> {
    if cols.is_empty() {
        return None;
    }
    let mut e = XmlElement::new("cols");
    let mut i = 0usize;
    while i < cols.len() {
        let (start, p) = cols[i];
        let mut end = start;
        let mut j = i + 1;
        while j < cols.len() && cols[j].0 == end + 1 && cols[j].1 == p {
            end += 1;
            j += 1;
        }
        let mut c = XmlElement::new("col")
            .with_attr("min", start + 1)
            .with_attr("max", end + 1);
        if let Some(w) = p.width {
            c.attrs.set("width", fmt_num(w.0));
        }
        if let Some(s) = p.style {
            c.attrs.set("style", s.to_string());
        }
        set_attr(&mut c, "hidden", flag(p.hidden));
        set_attr(&mut c, "bestFit", flag(p.best_fit));
        set_attr(&mut c, "customWidth", flag(p.custom_width));
        if p.outline_level > 0 {
            c.attrs.set("outlineLevel", p.outline_level.to_string());
        }
        set_attr(&mut c, "collapsed", flag(p.collapsed));
        e.children.push(XmlNode::Element(c));
        i = j;
    }
    Some(e)
}

// ── protection ────────────────────────────────────────────────────────────

pub(crate) fn parse_password(e: &XmlElement, legacy: &str, prefix: &str) -> Option<PasswordHash> {
    if let Some(h) = e
        .attr(legacy)
        .and_then(|p| u16::from_str_radix(p.trim(), 16).ok())
    {
        return Some(PasswordHash::Legacy(h));
    }
    let a = |n: &str| e.attr(&format!("{prefix}{n}")).map(str::to_owned);
    Some(PasswordHash::Modern {
        algorithm: a("AlgorithmName")?,
        hash: a("HashValue")?,
        salt: a("SaltValue").unwrap_or_default(),
        spin_count: e
            .attr(&format!("{prefix}SpinCount"))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0),
    })
}

pub(crate) fn set_password(
    e: &mut XmlElement,
    p: Option<&PasswordHash>,
    legacy: &str,
    prefix: &str,
) {
    match p {
        Some(PasswordHash::Legacy(h)) => e.attrs.set(legacy, format!("{h:04X}")),
        Some(PasswordHash::Modern {
            algorithm,
            hash,
            salt,
            spin_count,
        }) => {
            e.attrs
                .set(format!("{prefix}AlgorithmName"), algorithm.clone());
            e.attrs.set(format!("{prefix}HashValue"), hash.clone());
            if !salt.is_empty() {
                e.attrs.set(format!("{prefix}SaltValue"), salt.clone());
            }
            e.attrs
                .set(format!("{prefix}SpinCount"), spin_count.to_string());
        }
        None => {}
    }
}

/// `sheetProtection` (attributes say what is locked; the model says what users may still do).
pub(crate) fn parse_sheet_protection(e: &XmlElement) -> Option<SheetProtection> {
    if !abool_or(e, "sheet", false) {
        return None;
    }
    let locked = |n: &str, d: bool| abool_or(e, n, d);
    Some(SheetProtection {
        password: parse_password(e, "password", "").or_else(|| parse_password(e, "password", "")),
        select_locked_cells: !locked("selectLockedCells", false),
        select_unlocked_cells: !locked("selectUnlockedCells", false),
        format_cells: !locked("formatCells", true),
        format_columns: !locked("formatColumns", true),
        format_rows: !locked("formatRows", true),
        insert_columns: !locked("insertColumns", true),
        insert_rows: !locked("insertRows", true),
        insert_hyperlinks: !locked("insertHyperlinks", true),
        delete_columns: !locked("deleteColumns", true),
        delete_rows: !locked("deleteRows", true),
        sort: !locked("sort", true),
        auto_filter: !locked("autoFilter", true),
        pivot_tables: !locked("pivotTables", true),
        objects: locked("objects", false),
        scenarios: locked("scenarios", false),
    })
}

pub(crate) fn sheet_protection_el(p: &SheetProtection) -> XmlElement {
    let mut e = XmlElement::new("sheetProtection");
    set_password(&mut e, p.password.as_ref(), "password", "");
    e.attrs.set("sheet", "1");
    set_attr(&mut e, "objects", flag(p.objects));
    set_attr(&mut e, "scenarios", flag(p.scenarios));
    for (name, allowed) in [
        ("formatCells", p.format_cells),
        ("formatColumns", p.format_columns),
        ("formatRows", p.format_rows),
        ("insertColumns", p.insert_columns),
        ("insertRows", p.insert_rows),
        ("insertHyperlinks", p.insert_hyperlinks),
        ("deleteColumns", p.delete_columns),
        ("deleteRows", p.delete_rows),
    ] {
        set_attr(&mut e, name, allowed.then(|| "0".to_owned()));
    }
    set_attr(&mut e, "selectLockedCells", flag(!p.select_locked_cells));
    for (name, allowed) in [
        ("sort", p.sort),
        ("autoFilter", p.auto_filter),
        ("pivotTables", p.pivot_tables),
    ] {
        set_attr(&mut e, name, allowed.then(|| "0".to_owned()));
    }
    set_attr(
        &mut e,
        "selectUnlockedCells",
        flag(!p.select_unlocked_cells),
    );
    e
}

// ── AutoFilter ────────────────────────────────────────────────────────────

const FILTER_OPS: &[(FilterOperator, &str)] = &[
    (FilterOperator::Equal, "equal"),
    (FilterOperator::NotEqual, "notEqual"),
    (FilterOperator::GreaterThan, "greaterThan"),
    (FilterOperator::GreaterThanOrEqual, "greaterThanOrEqual"),
    (FilterOperator::LessThan, "lessThan"),
    (FilterOperator::LessThanOrEqual, "lessThanOrEqual"),
];

const DYNAMIC: &[(DynamicFilter, &str)] = &[
    (DynamicFilter::AboveAverage, "aboveAverage"),
    (DynamicFilter::BelowAverage, "belowAverage"),
    (DynamicFilter::Today, "today"),
    (DynamicFilter::Yesterday, "yesterday"),
    (DynamicFilter::Tomorrow, "tomorrow"),
    (DynamicFilter::ThisWeek, "thisWeek"),
    (DynamicFilter::LastWeek, "lastWeek"),
    (DynamicFilter::NextWeek, "nextWeek"),
    (DynamicFilter::ThisMonth, "thisMonth"),
    (DynamicFilter::LastMonth, "lastMonth"),
    (DynamicFilter::NextMonth, "nextMonth"),
    (DynamicFilter::ThisQuarter, "thisQuarter"),
    (DynamicFilter::LastQuarter, "lastQuarter"),
    (DynamicFilter::NextQuarter, "nextQuarter"),
    (DynamicFilter::ThisYear, "thisYear"),
    (DynamicFilter::LastYear, "lastYear"),
    (DynamicFilter::NextYear, "nextYear"),
    (DynamicFilter::YearToDate, "yearToDate"),
];

fn dynamic_from(v: &str) -> Option<DynamicFilter> {
    if let Some(q) = v.strip_prefix('Q').and_then(|q| q.parse::<u8>().ok()) {
        return (1..=4).contains(&q).then_some(DynamicFilter::Quarter(q));
    }
    if let Some(m) = v.strip_prefix('M').and_then(|m| m.parse::<u8>().ok()) {
        return (1..=12).contains(&m).then_some(DynamicFilter::Month(m));
    }
    DYNAMIC.iter().find(|(_, s)| *s == v).map(|(d, _)| *d)
}

fn dynamic_str(d: DynamicFilter) -> String {
    match d {
        DynamicFilter::Quarter(q) => format!("Q{q}"),
        DynamicFilter::Month(m) => format!("M{m}"),
        other => DYNAMIC
            .iter()
            .find(|(x, _)| *x == other)
            .map_or("today", |(_, s)| s)
            .to_owned(),
    }
}

const GROUPINGS: &[(DateGrouping, &str)] = &[
    (DateGrouping::Year, "year"),
    (DateGrouping::Month, "month"),
    (DateGrouping::Day, "day"),
    (DateGrouping::Hour, "hour"),
    (DateGrouping::Minute, "minute"),
    (DateGrouping::Second, "second"),
];

fn icon_set_from(v: &str) -> Option<IconSetName> {
    use IconSetName::*;
    [
        Arrows3,
        ArrowsGray3,
        Flags3,
        TrafficLights3,
        TrafficLightsRimmed3,
        Signs3,
        Symbols3,
        SymbolsUncircled3,
        Stars3,
        Triangles3,
        Arrows4,
        ArrowsGray4,
        RedToBlack4,
        Rating4,
        TrafficLights4,
        Arrows5,
        ArrowsGray5,
        Rating5,
        Quarters5,
        Boxes5,
        NoIcons,
    ]
    .into_iter()
    .find(|n| n.as_str() == v)
}

fn parse_filter_column(fc: &XmlElement) -> Option<FilterColumn> {
    let filter = if let Some(f) = child_local(fc, "filters") {
        Some(ColumnFilter::Values {
            values: children_local(f, "filter")
                .filter_map(|x| x.attr("val").map(str::to_owned))
                .collect(),
            blank: abool_or(f, "blank", false),
            date_groups: children_local(f, "dateGroupItem")
                .filter_map(|d| {
                    let grouping = GROUPINGS
                        .iter()
                        .find(|(_, s)| Some(*s) == d.attr("dateTimeGrouping"))?
                        .0;
                    let n = |k: &str| au32(d, k).unwrap_or(0);
                    Some(DateGroupItem {
                        grouping,
                        year: u16::try_from(n("year")).unwrap_or(0),
                        month: n("month").min(12) as u8,
                        day: n("day").min(31) as u8,
                        hour: n("hour").min(23) as u8,
                        minute: n("minute").min(59) as u8,
                        second: n("second").min(59) as u8,
                    })
                })
                .collect(),
        })
    } else if let Some(c) = child_local(fc, "customFilters") {
        Some(ColumnFilter::Custom {
            and: abool_or(c, "and", false),
            criteria: children_local(c, "customFilter")
                .map(|x| CustomCriterion {
                    op: x
                        .attr("operator")
                        .and_then(|o| FILTER_OPS.iter().find(|(_, s)| *s == o))
                        .map_or(FilterOperator::Equal, |(op, _)| *op),
                    value: x.attr("val").unwrap_or("").to_owned(),
                })
                .collect(),
        })
    } else if let Some(t) = child_local(fc, "top10") {
        Some(ColumnFilter::Top10 {
            top: abool_or(t, "top", true),
            percent: abool_or(t, "percent", false),
            value: anum(t, "val").unwrap_or(10.0),
        })
    } else if let Some(d) = child_local(fc, "dynamicFilter") {
        d.attr("type")
            .and_then(dynamic_from)
            .map(ColumnFilter::Dynamic)
    } else if let Some(c) = child_local(fc, "colorFilter") {
        au32(c, "dxfId").map(|dxf| ColumnFilter::Color {
            cell_color: abool_or(c, "cellColor", true),
            dxf,
        })
    } else if let Some(i) = child_local(fc, "iconFilter") {
        i.attr("iconSet")
            .and_then(icon_set_from)
            .map(|icon_set| ColumnFilter::Icon {
                icon_set,
                icon_id: au32(i, "iconId").unwrap_or(0),
            })
    } else {
        None
    };
    Some(FilterColumn {
        col_id: au32(fc, "colId")?,
        hidden_button: abool_or(fc, "hiddenButton", false),
        show_button: abool_or(fc, "showButton", true),
        filter,
    })
}

fn filter_column_el(c: &FilterColumn) -> XmlElement {
    let mut e = XmlElement::new("filterColumn").with_attr("colId", c.col_id);
    set_attr(&mut e, "hiddenButton", flag(c.hidden_button));
    set_attr(&mut e, "showButton", flag_default_true(c.show_button));
    let Some(f) = &c.filter else { return e };
    let inner = match f {
        ColumnFilter::Values {
            values,
            blank,
            date_groups,
        } => {
            let mut x = XmlElement::new("filters");
            set_attr(&mut x, "blank", flag(*blank));
            for v in values {
                x.children.push(XmlNode::Element(
                    XmlElement::new("filter").with_attr("val", v.clone()),
                ));
            }
            for d in date_groups {
                let mut g = XmlElement::new("dateGroupItem").with_attr("year", d.year);
                let level = GROUPINGS
                    .iter()
                    .position(|(k, _)| *k == d.grouping)
                    .unwrap_or(0);
                for (i, (name, v)) in [
                    ("month", d.month),
                    ("day", d.day),
                    ("hour", d.hour),
                    ("minute", d.minute),
                    ("second", d.second),
                ]
                .into_iter()
                .enumerate()
                {
                    if i < level {
                        g.attrs.set(name, v.to_string());
                    }
                }
                let gname = GROUPINGS[level].1;
                g.attrs.set("dateTimeGrouping", gname);
                x.children.push(XmlNode::Element(g));
            }
            x
        }
        ColumnFilter::Custom { and, criteria } => {
            let mut x = XmlElement::new("customFilters");
            set_attr(&mut x, "and", flag(*and));
            for c in criteria {
                let mut cf = XmlElement::new("customFilter");
                if c.op != FilterOperator::Equal {
                    let op = FILTER_OPS
                        .iter()
                        .find(|(o, _)| *o == c.op)
                        .map_or("equal", |(_, s)| s);
                    cf.attrs.set("operator", op);
                }
                cf.attrs.set("val", c.value.clone());
                x.children.push(XmlNode::Element(cf));
            }
            x
        }
        ColumnFilter::Top10 {
            top,
            percent,
            value,
        } => {
            let mut x = XmlElement::new("top10");
            set_attr(&mut x, "top", flag_default_true(*top));
            set_attr(&mut x, "percent", flag(*percent));
            x.attrs.set("val", fmt_num(*value));
            x
        }
        ColumnFilter::Dynamic(d) => {
            XmlElement::new("dynamicFilter").with_attr("type", dynamic_str(*d))
        }
        ColumnFilter::Color { cell_color, dxf } => {
            let mut x = XmlElement::new("colorFilter").with_attr("dxfId", *dxf);
            set_attr(&mut x, "cellColor", flag_default_true(*cell_color));
            x
        }
        ColumnFilter::Icon { icon_set, icon_id } => XmlElement::new("iconFilter")
            .with_attr("iconSet", icon_set.as_str())
            .with_attr("iconId", *icon_id),
    };
    e.with_child(inner)
}

pub(crate) fn parse_sort_state(s: &XmlElement) -> Option<SortState> {
    Some(SortState {
        range: parse_ref(s.attr("ref")?)?,
        case_sensitive: abool_or(s, "caseSensitive", false),
        column_sort: abool_or(s, "columnSort", false),
        conditions: children_local(s, "sortCondition")
            .filter_map(|c| {
                Some(SortCondition {
                    range: parse_ref(c.attr("ref")?)?,
                    descending: abool_or(c, "descending", false),
                    sort_by: match c.attr("sortBy") {
                        Some("cellColor") => SortBy::CellColor,
                        Some("fontColor") => SortBy::FontColor,
                        Some("icon") => SortBy::Icon,
                        _ => SortBy::Value,
                    },
                    custom_list: c
                        .attr("customList")
                        .map(|l| l.split(',').map(str::to_owned).collect()),
                    dxf: au32(c, "dxfId"),
                })
            })
            .collect(),
    })
}

pub(crate) fn sort_state_el(s: &SortState) -> XmlElement {
    let mut e = XmlElement::new("sortState");
    set_attr(&mut e, "columnSort", flag(s.column_sort));
    set_attr(&mut e, "caseSensitive", flag(s.case_sensitive));
    e.attrs.set("ref", ref_text(&s.range));
    for c in &s.conditions {
        let mut x = XmlElement::new("sortCondition");
        set_attr(&mut x, "descending", flag(c.descending));
        match c.sort_by {
            SortBy::Value => {}
            SortBy::CellColor => x.attrs.set("sortBy", "cellColor"),
            SortBy::FontColor => x.attrs.set("sortBy", "fontColor"),
            SortBy::Icon => x.attrs.set("sortBy", "icon"),
        }
        x.attrs.set("ref", ref_text(&c.range));
        if let Some(l) = &c.custom_list {
            x.attrs.set("customList", l.join(","));
        }
        if let Some(d) = c.dxf {
            x.attrs.set("dxfId", d.to_string());
        }
        e.children.push(XmlNode::Element(x));
    }
    e
}

pub(crate) fn parse_filter_columns(af: &XmlElement) -> Vec<FilterColumn> {
    children_local(af, "filterColumn")
        .filter_map(parse_filter_column)
        .collect()
}

pub(crate) fn parse_auto_filter(af: &XmlElement) -> Option<AutoFilter> {
    Some(AutoFilter {
        range: parse_ref(af.attr("ref")?)?,
        columns: parse_filter_columns(af),
        sort: child_local(af, "sortState").and_then(parse_sort_state),
    })
}

/// An `autoFilter` element (sheets and tables).
pub(crate) fn auto_filter_el(
    range: &Range,
    columns: &[FilterColumn],
    sort: Option<&SortState>,
) -> XmlElement {
    let mut e = XmlElement::new("autoFilter").with_attr("ref", ref_text(range));
    for c in columns {
        e.children.push(XmlNode::Element(filter_column_el(c)));
    }
    if let Some(s) = sort {
        e.children.push(XmlNode::Element(sort_state_el(s)));
    }
    e
}

// ── conditional formatting ────────────────────────────────────────────────

const OPERATORS: &[(Operator, &str)] = &[
    (Operator::Between, "between"),
    (Operator::NotBetween, "notBetween"),
    (Operator::Equal, "equal"),
    (Operator::NotEqual, "notEqual"),
    (Operator::GreaterThan, "greaterThan"),
    (Operator::LessThan, "lessThan"),
    (Operator::GreaterThanOrEqual, "greaterThanOrEqual"),
    (Operator::LessThanOrEqual, "lessThanOrEqual"),
];

fn operator_from(v: Option<&str>) -> Operator {
    v.and_then(|v| OPERATORS.iter().find(|(_, s)| *s == v))
        .map_or(Operator::Between, |(o, _)| *o)
}

fn operator_str(o: Operator) -> &'static str {
    OPERATORS
        .iter()
        .find(|(x, _)| *x == o)
        .map_or("between", |(_, s)| s)
}

const PERIODS: &[(TimePeriod, &str)] = &[
    (TimePeriod::Today, "today"),
    (TimePeriod::Yesterday, "yesterday"),
    (TimePeriod::Tomorrow, "tomorrow"),
    (TimePeriod::Last7Days, "last7Days"),
    (TimePeriod::ThisWeek, "thisWeek"),
    (TimePeriod::LastWeek, "lastWeek"),
    (TimePeriod::NextWeek, "nextWeek"),
    (TimePeriod::ThisMonth, "thisMonth"),
    (TimePeriod::LastMonth, "lastMonth"),
    (TimePeriod::NextMonth, "nextMonth"),
];

const CFVO_TYPES: &[(CfvoType, &str)] = &[
    (CfvoType::Min, "min"),
    (CfvoType::Max, "max"),
    (CfvoType::Num, "num"),
    (CfvoType::Percent, "percent"),
    (CfvoType::Percentile, "percentile"),
    (CfvoType::Formula, "formula"),
    (CfvoType::AutoMin, "autoMin"),
    (CfvoType::AutoMax, "autoMax"),
];

fn parse_cfvo(c: &XmlElement) -> Option<Cfvo> {
    let kind = CFVO_TYPES
        .iter()
        .find(|(_, s)| Some(*s) == c.attr("type"))?
        .0;
    // x14 puts the value in an `xm:f` child.
    let value = c
        .attr("val")
        .map(str::to_owned)
        .or_else(|| child_local(c, "f").map(XmlElement::text));
    let value = value.map(|v| {
        if kind == CfvoType::Formula {
            model_formula(&v)
        } else {
            v
        }
    });
    Some(Cfvo {
        kind,
        value,
        gte: abool_or(c, "gte", true),
    })
}

fn cfvo_el(c: &Cfvo, x14: bool) -> XmlElement {
    let name = if x14 { "x14:cfvo" } else { "cfvo" };
    let kind = CFVO_TYPES
        .iter()
        .find(|(k, _)| *k == c.kind)
        .map_or("num", |(_, s)| s);
    let mut e = XmlElement::new(name).with_attr("type", kind);
    if let Some(v) = &c.value {
        let v = if c.kind == CfvoType::Formula {
            file_formula(v)
        } else {
            v.clone()
        };
        if x14 {
            e.children.push(XmlNode::Element(text_el("xm:f", &v)));
        } else {
            e.attrs.set("val", v);
        }
    }
    if !c.gte {
        e.attrs.set("gte", "0");
    }
    e
}

/// A `cfRule` of the 2007 schema. `next_id` allocates model rule ids.
pub(crate) fn parse_cf_rule(r: &XmlElement, id: u32) -> Option<(CfRule, Option<String>)> {
    let dxf = au32(r, "dxfId").unwrap_or(NO_DXF);
    let formulas: Vec<String> = children_local(r, "formula")
        .map(|f| model_formula(&f.text()))
        .collect();
    let text = || r.attr("text").unwrap_or("").to_owned();
    let kind = match r.attr("type")? {
        "cellIs" => CfKind::CellIs {
            operator: operator_from(r.attr("operator")),
            formulas,
            dxf,
        },
        "expression" => CfKind::Expression {
            formula: formulas.into_iter().next().unwrap_or_default(),
            dxf,
        },
        "containsText" => CfKind::ContainsText { text: text(), dxf },
        "notContainsText" => CfKind::NotContainsText { text: text(), dxf },
        "beginsWith" => CfKind::BeginsWith { text: text(), dxf },
        "endsWith" => CfKind::EndsWith { text: text(), dxf },
        "containsBlanks" => CfKind::ContainsBlanks { dxf },
        "notContainsBlanks" => CfKind::NotContainsBlanks { dxf },
        "containsErrors" => CfKind::ContainsErrors { dxf },
        "notContainsErrors" => CfKind::NotContainsErrors { dxf },
        "timePeriod" => CfKind::TimePeriod {
            period: PERIODS
                .iter()
                .find(|(_, s)| Some(*s) == r.attr("timePeriod"))?
                .0,
            dxf,
        },
        "duplicateValues" => CfKind::DuplicateValues { dxf },
        "uniqueValues" => CfKind::UniqueValues { dxf },
        "top10" => CfKind::Top10 {
            rank: au32(r, "rank").unwrap_or(10),
            percent: abool_or(r, "percent", false),
            bottom: abool_or(r, "bottom", false),
            dxf,
        },
        "aboveAverage" => CfKind::AboveAverage {
            above: abool_or(r, "aboveAverage", true),
            equal: abool_or(r, "equalAverage", false),
            std_dev: au32(r, "stdDev").map_or(0, |s| s.min(3) as u8),
            dxf,
        },
        "colorScale" => {
            let cs = child_local(r, "colorScale")?;
            CfKind::ColorScale {
                cfvos: children_local(cs, "cfvo").filter_map(parse_cfvo).collect(),
                colors: children_local(cs, "color")
                    .filter_map(parse_color)
                    .collect(),
            }
        }
        "dataBar" => {
            let db = child_local(r, "dataBar")?;
            let cfvos: Vec<Cfvo> = children_local(db, "cfvo").filter_map(parse_cfvo).collect();
            let mut it = cfvos.into_iter();
            CfKind::DataBar(DataBar {
                min: it.next().unwrap_or(Cfvo::of(CfvoType::Min)),
                max: it.next().unwrap_or(Cfvo::of(CfvoType::Max)),
                color: child_local(db, "color")
                    .and_then(parse_color)
                    .unwrap_or(Color::rgb(0x638EC6)),
                show_value: abool_or(db, "showValue", true),
                min_length: au32(db, "minLength").unwrap_or(10),
                max_length: au32(db, "maxLength").unwrap_or(90),
                gradient: true,
                border_color: None,
                negative_color: None,
                negative_border_color: None,
                axis_color: None,
                axis_position: AxisPosition::Automatic,
                direction: BarDirection::Context,
            })
        }
        "iconSet" => {
            let is = child_local(r, "iconSet")?;
            CfKind::IconSet(IconSet {
                icon_set: is
                    .attr("iconSet")
                    .and_then(icon_set_from)
                    .unwrap_or(IconSetName::TrafficLights3),
                cfvos: children_local(is, "cfvo").filter_map(parse_cfvo).collect(),
                reverse: abool_or(is, "reverse", false),
                show_value: abool_or(is, "showValue", true),
                custom_icons: Vec::new(),
            })
        }
        _ => return None,
    };
    let ext_id = child_local(r, "extLst")
        .into_iter()
        .flat_map(|l| children_local(l, "ext"))
        .find_map(|e| child_local(e, "id").map(XmlElement::text));
    Some((
        CfRule {
            id,
            priority: au32(r, "priority").unwrap_or(1),
            stop_if_true: abool_or(r, "stopIfTrue", false),
            kind,
        },
        ext_id,
    ))
}

/// Merges an `x14:cfRule`'s options into a rule read from the 2007 part (data bars), or reads
/// an x14-only rule.
pub(crate) fn apply_x14_rule(rule: &mut CfRule, x: &XmlElement) {
    match &mut rule.kind {
        CfKind::DataBar(b) => {
            if let Some(d) = child_local(x, "dataBar") {
                apply_x14_data_bar(b, d);
            }
        }
        CfKind::IconSet(s) => {
            if let Some(i) = child_local(x, "iconSet") {
                apply_x14_icon_set(s, i);
            }
        }
        _ => {}
    }
}

fn apply_x14_data_bar(b: &mut DataBar, d: &XmlElement) {
    b.min_length = au32(d, "minLength").unwrap_or(10);
    b.max_length = au32(d, "maxLength").unwrap_or(90);
    b.gradient = abool_or(d, "gradient", true);
    b.show_value = abool_or(d, "showValue", b.show_value);
    let cfvos: Vec<Cfvo> = children_local(d, "cfvo").filter_map(parse_cfvo).collect();
    if cfvos.len() == 2 {
        b.min = cfvos[0].clone();
        b.max = cfvos[1].clone();
    }
    if abool_or(d, "border", false) {
        b.border_color = child_local(d, "borderColor").and_then(parse_color);
    }
    let neg_same = abool_or(d, "negativeBarColorSameAsPositive", false);
    if !neg_same {
        b.negative_color = child_local(d, "negativeFillColor").and_then(parse_color);
    }
    let neg_border_same = abool_or(d, "negativeBarBorderColorSameAsPositive", true);
    if !neg_border_same {
        b.negative_border_color = child_local(d, "negativeBorderColor").and_then(parse_color);
    }
    b.axis_color = child_local(d, "axisColor").and_then(parse_color);
    b.axis_position = match d.attr("axisPosition") {
        Some("middle") => AxisPosition::Middle,
        Some("none") => AxisPosition::None,
        _ => AxisPosition::Automatic,
    };
    b.direction = match d.attr("direction") {
        Some("leftToRight") => BarDirection::LeftToRight,
        Some("rightToLeft") => BarDirection::RightToLeft,
        _ => BarDirection::Context,
    };
    if let Some(c) = child_local(d, "fillColor").and_then(parse_color) {
        b.color = c;
    }
}

fn apply_x14_icon_set(s: &mut IconSet, i: &XmlElement) {
    if let Some(n) = i.attr("iconSet").and_then(icon_set_from) {
        s.icon_set = n;
    }
    s.reverse = abool_or(i, "reverse", s.reverse);
    s.show_value = abool_or(i, "showValue", s.show_value);
    let cfvos: Vec<Cfvo> = children_local(i, "cfvo").filter_map(parse_cfvo).collect();
    if !cfvos.is_empty() {
        s.cfvos = cfvos;
    }
    if abool_or(i, "custom", false) {
        s.custom_icons = children_local(i, "cfIcon")
            .filter_map(|c| {
                Some(CustomIcon {
                    icon_set: c.attr("iconSet").and_then(icon_set_from)?,
                    icon_id: au32(c, "iconId").unwrap_or(0),
                })
            })
            .collect();
    }
}

/// An x14-only rule (`x14:cfRule` without a 2007 twin).
pub(crate) fn parse_x14_only_rule(x: &XmlElement, id: u32) -> Option<CfRule> {
    let mut base = XmlElement::new("cfRule");
    for (k, v) in x.attrs.iter() {
        if k != "id" {
            base.attrs.set(k, v);
        }
    }
    // Formulas live in `xm:f` children for x14 rules.
    for f in children_local(x, "f") {
        base.children
            .push(XmlNode::Element(text_el("formula", &f.text())));
    }
    match x.attr("type") {
        Some("dataBar") => {
            base.children.push(XmlNode::Element(
                XmlElement::new("dataBar")
                    .with_child(XmlElement::new("cfvo").with_attr("type", "min"))
                    .with_child(XmlElement::new("cfvo").with_attr("type", "max")),
            ));
        }
        Some("iconSet") => {
            base.children
                .push(XmlNode::Element(XmlElement::new("iconSet")));
        }
        _ => {}
    }
    if let Some(dxf) = child_local(x, "dxf") {
        let _ = dxf; // differential formats inline in x14 rules are not modelled
    }
    let (mut rule, _) = parse_cf_rule(&base, id)?;
    apply_x14_rule(&mut rule, x);
    Some(rule)
}

fn cf_formula_el(text: &str) -> XmlElement {
    text_el("formula", &file_formula(text))
}

fn escape_quotes(s: &str) -> String {
    s.replace('"', "\"\"")
}

/// Whether a data bar needs the Excel 2010 extension to be shown as modelled.
pub(crate) fn data_bar_needs_x14(b: &DataBar) -> bool {
    !b.gradient
        || b.border_color.is_some()
        || b.negative_color.is_some()
        || b.negative_border_color.is_some()
        || b.axis_color.is_some()
        || b.axis_position != AxisPosition::Automatic
        || b.direction != BarDirection::Context
        || matches!(b.min.kind, CfvoType::AutoMin)
        || matches!(b.max.kind, CfvoType::AutoMax)
        || b.min_length != 10
        || b.max_length != 90
}

/// Whether an icon set needs the extension (custom icons, no icons).
pub(crate) fn icon_set_needs_x14(s: &IconSet) -> bool {
    !s.custom_icons.is_empty() || s.icon_set == IconSetName::NoIcons
}

/// A 2007 `cfRule`; `top_left` is the first cell of the block (text rules refer to it);
/// `x14_id` links the rule to its extension.
pub(crate) fn cf_rule_el(rule: &CfRule, top_left: (u32, u32), x14_id: Option<&str>) -> XmlElement {
    let mut e = XmlElement::new("cfRule");
    let at = cell_ref(top_left.0, top_left.1);
    let kind_name = match &rule.kind {
        CfKind::CellIs { .. } => "cellIs",
        CfKind::Expression { .. } => "expression",
        CfKind::ContainsText { .. } => "containsText",
        CfKind::NotContainsText { .. } => "notContainsText",
        CfKind::BeginsWith { .. } => "beginsWith",
        CfKind::EndsWith { .. } => "endsWith",
        CfKind::ContainsBlanks { .. } => "containsBlanks",
        CfKind::NotContainsBlanks { .. } => "notContainsBlanks",
        CfKind::ContainsErrors { .. } => "containsErrors",
        CfKind::NotContainsErrors { .. } => "notContainsErrors",
        CfKind::TimePeriod { .. } => "timePeriod",
        CfKind::DuplicateValues { .. } => "duplicateValues",
        CfKind::UniqueValues { .. } => "uniqueValues",
        CfKind::Top10 { .. } => "top10",
        CfKind::AboveAverage { .. } => "aboveAverage",
        CfKind::ColorScale { .. } => "colorScale",
        CfKind::DataBar(_) => "dataBar",
        CfKind::IconSet(_) => "iconSet",
    };
    e.attrs.set("type", kind_name);
    if let Some(d) = rule.kind.dxf().filter(|d| *d != NO_DXF) {
        e.attrs.set("dxfId", d.to_string());
    }
    e.attrs.set("priority", rule.priority.to_string());
    set_attr(&mut e, "stopIfTrue", flag(rule.stop_if_true));
    let mut kids: Vec<XmlElement> = Vec::new();
    let mut push = |c: XmlElement| kids.push(c);
    match &rule.kind {
        CfKind::CellIs {
            operator, formulas, ..
        } => {
            e.attrs.set("operator", operator_str(*operator));
            for f in formulas {
                push(cf_formula_el(f));
            }
        }
        CfKind::Expression { formula, .. } => push(cf_formula_el(formula)),
        CfKind::ContainsText { text, .. } => {
            e.attrs.set("operator", "containsText");
            e.attrs.set("text", text.clone());
            push(text_el(
                "formula",
                &format!("NOT(ISERROR(SEARCH(\"{}\",{at})))", escape_quotes(text)),
            ));
        }
        CfKind::NotContainsText { text, .. } => {
            e.attrs.set("operator", "notContains");
            e.attrs.set("text", text.clone());
            push(text_el(
                "formula",
                &format!("ISERROR(SEARCH(\"{}\",{at}))", escape_quotes(text)),
            ));
        }
        CfKind::BeginsWith { text, .. } => {
            e.attrs.set("operator", "beginsWith");
            e.attrs.set("text", text.clone());
            let t = escape_quotes(text);
            push(text_el(
                "formula",
                &format!("LEFT({at},LEN(\"{t}\"))=\"{t}\""),
            ));
        }
        CfKind::EndsWith { text, .. } => {
            e.attrs.set("operator", "endsWith");
            e.attrs.set("text", text.clone());
            let t = escape_quotes(text);
            push(text_el(
                "formula",
                &format!("RIGHT({at},LEN(\"{t}\"))=\"{t}\""),
            ));
        }
        CfKind::ContainsBlanks { .. } => push(text_el("formula", &format!("LEN(TRIM({at}))=0"))),
        CfKind::NotContainsBlanks { .. } => {
            push(text_el("formula", &format!("LEN(TRIM({at}))>0")));
        }
        CfKind::ContainsErrors { .. } => push(text_el("formula", &format!("ISERROR({at})"))),
        CfKind::NotContainsErrors { .. } => {
            push(text_el("formula", &format!("NOT(ISERROR({at}))")));
        }
        CfKind::TimePeriod { period, .. } => {
            let p = PERIODS
                .iter()
                .find(|(x, _)| x == period)
                .map_or("today", |(_, s)| s);
            e.attrs.set("timePeriod", p);
            push(text_el("formula", &time_period_formula(*period, &at)));
        }
        CfKind::DuplicateValues { .. } | CfKind::UniqueValues { .. } => {}
        CfKind::Top10 {
            rank,
            percent,
            bottom,
            ..
        } => {
            set_attr(&mut e, "percent", flag(*percent));
            set_attr(&mut e, "bottom", flag(*bottom));
            e.attrs.set("rank", rank.to_string());
        }
        CfKind::AboveAverage {
            above,
            equal,
            std_dev,
            ..
        } => {
            set_attr(&mut e, "aboveAverage", flag_default_true(*above));
            set_attr(&mut e, "equalAverage", flag(*equal));
            if *std_dev > 0 {
                e.attrs.set("stdDev", std_dev.to_string());
            }
        }
        CfKind::ColorScale { cfvos, colors } => {
            let mut cs = XmlElement::new("colorScale");
            for c in cfvos {
                cs.children.push(XmlNode::Element(cfvo_el(c, false)));
            }
            for c in colors {
                cs.children.push(XmlNode::Element(color_el("color", c)));
            }
            push(cs);
        }
        CfKind::DataBar(b) => {
            let mut db = XmlElement::new("dataBar");
            if b.min_length != 10 {
                db.attrs.set("minLength", b.min_length.min(100).to_string());
            }
            if b.max_length != 90 {
                db.attrs.set("maxLength", b.max_length.min(100).to_string());
            }
            set_attr(&mut db, "showValue", flag_default_true(b.show_value));
            let legacy = |c: &Cfvo, fallback: CfvoType| match c.kind {
                CfvoType::AutoMin | CfvoType::AutoMax => Cfvo::of(fallback),
                _ => c.clone(),
            };
            db.children.push(XmlNode::Element(cfvo_el(
                &legacy(&b.min, CfvoType::Min),
                false,
            )));
            db.children.push(XmlNode::Element(cfvo_el(
                &legacy(&b.max, CfvoType::Max),
                false,
            )));
            db.children
                .push(XmlNode::Element(color_el("color", &b.color)));
            push(db);
        }
        CfKind::IconSet(s) => {
            let mut is = XmlElement::new("iconSet");
            let name = if s.icon_set == IconSetName::NoIcons {
                IconSetName::TrafficLights3
            } else {
                s.icon_set
            };
            if name != IconSetName::TrafficLights3 {
                is.attrs.set("iconSet", name.as_str());
            }
            set_attr(&mut is, "showValue", flag_default_true(s.show_value));
            set_attr(&mut is, "reverse", flag(s.reverse));
            for c in &s.cfvos {
                is.children.push(XmlNode::Element(cfvo_el(c, false)));
            }
            push(is);
        }
    }
    e.children.extend(kids.into_iter().map(XmlNode::Element));
    if let Some(id) = x14_id {
        e.children.push(XmlNode::Element(
            XmlElement::new("extLst").with_child(
                XmlElement::new("ext")
                    .with_attr("uri", "{B025F937-C7B1-47D3-B67F-A62EFF666E3E}")
                    .with_attr(
                        "xmlns:x14",
                        "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main",
                    )
                    .with_child(XmlElement::new("x14:id").with_text(id.to_owned())),
            ),
        ));
    }
    e
}

fn time_period_formula(p: TimePeriod, at: &str) -> String {
    match p {
        TimePeriod::Today => format!("FLOOR({at},1)=TODAY()"),
        TimePeriod::Yesterday => format!("FLOOR({at},1)=TODAY()-1"),
        TimePeriod::Tomorrow => format!("FLOOR({at},1)=TODAY()+1"),
        TimePeriod::Last7Days => format!("AND(TODAY()-FLOOR({at},1)<=6,FLOOR({at},1)<=TODAY())"),
        TimePeriod::ThisWeek => format!(
            "AND(TODAY()-ROUNDDOWN({at},0)<=WEEKDAY(TODAY())-1,ROUNDDOWN({at},0)-TODAY()<=7-WEEKDAY(TODAY()))"
        ),
        TimePeriod::LastWeek => format!(
            "AND(TODAY()-ROUNDDOWN({at},0)>=(WEEKDAY(TODAY())),TODAY()-ROUNDDOWN({at},0)<(WEEKDAY(TODAY())+7))"
        ),
        TimePeriod::NextWeek => format!(
            "AND(ROUNDDOWN({at},0)-TODAY()>(7-WEEKDAY(TODAY())),ROUNDDOWN({at},0)-TODAY()<(15-WEEKDAY(TODAY())))"
        ),
        TimePeriod::ThisMonth => {
            format!("AND(MONTH({at})=MONTH(TODAY()),YEAR({at})=YEAR(TODAY()))")
        }
        TimePeriod::LastMonth => format!(
            "AND(MONTH({at})=MONTH(EDATE(TODAY(),0-1)),YEAR({at})=YEAR(EDATE(TODAY(),0-1)))"
        ),
        TimePeriod::NextMonth => format!(
            "AND(MONTH({at})=MONTH(EDATE(TODAY(),0+1)),YEAR({at})=YEAR(EDATE(TODAY(),0+1)))"
        ),
    }
}

/// An `x14:cfRule` carrying a rule's Excel 2010 options.
pub(crate) fn x14_cf_rule_el(rule: &CfRule, id: &str) -> Option<XmlElement> {
    let mut e = XmlElement::new("x14:cfRule");
    match &rule.kind {
        CfKind::DataBar(b) => {
            e.attrs.set("type", "dataBar");
            e.attrs.set("id", id.to_owned());
            let mut d = XmlElement::new("x14:dataBar")
                .with_attr("minLength", b.min_length.min(100))
                .with_attr("maxLength", b.max_length.min(100));
            set_attr(&mut d, "showValue", flag_default_true(b.show_value));
            set_attr(&mut d, "border", flag(b.border_color.is_some()));
            set_attr(&mut d, "gradient", flag_default_true(b.gradient));
            match b.direction {
                BarDirection::Context => {}
                BarDirection::LeftToRight => d.attrs.set("direction", "leftToRight"),
                BarDirection::RightToLeft => d.attrs.set("direction", "rightToLeft"),
            }
            set_attr(
                &mut d,
                "negativeBarColorSameAsPositive",
                flag(b.negative_color.is_none()),
            );
            set_attr(
                &mut d,
                "negativeBarBorderColorSameAsPositive",
                flag_default_true(b.negative_border_color.is_none()),
            );
            match b.axis_position {
                AxisPosition::Automatic => {}
                AxisPosition::Middle => d.attrs.set("axisPosition", "middle"),
                AxisPosition::None => d.attrs.set("axisPosition", "none"),
            }
            d.children.push(XmlNode::Element(cfvo_el(&b.min, true)));
            d.children.push(XmlNode::Element(cfvo_el(&b.max, true)));
            if let Some(c) = &b.border_color {
                d.children
                    .push(XmlNode::Element(color_el("x14:borderColor", c)));
            }
            if let Some(c) = &b.negative_color {
                d.children
                    .push(XmlNode::Element(color_el("x14:negativeFillColor", c)));
            }
            if let Some(c) = &b.negative_border_color {
                d.children
                    .push(XmlNode::Element(color_el("x14:negativeBorderColor", c)));
            }
            if let Some(c) = &b.axis_color {
                d.children
                    .push(XmlNode::Element(color_el("x14:axisColor", c)));
            }
            e.children.push(XmlNode::Element(d));
        }
        CfKind::IconSet(s) => {
            e.attrs.set("type", "iconSet");
            e.attrs.set("priority", rule.priority.to_string());
            e.attrs.set("id", id.to_owned());
            let mut i = XmlElement::new("x14:iconSet").with_attr("iconSet", s.icon_set.as_str());
            set_attr(&mut i, "showValue", flag_default_true(s.show_value));
            set_attr(&mut i, "reverse", flag(s.reverse));
            set_attr(&mut i, "custom", flag(!s.custom_icons.is_empty()));
            for c in &s.cfvos {
                i.children.push(XmlNode::Element(cfvo_el(c, true)));
            }
            for c in &s.custom_icons {
                i.children.push(XmlNode::Element(
                    XmlElement::new("x14:cfIcon")
                        .with_attr("iconSet", c.icon_set.as_str())
                        .with_attr("iconId", c.icon_id),
                ));
            }
            e.children.push(XmlNode::Element(i));
        }
        _ => return None,
    }
    Some(e)
}

/// `conditionalFormatting` blocks.
pub(crate) fn conditional_formatting_el(
    cf: &ConditionalFormat,
    ids: &[Option<String>],
) -> XmlElement {
    let mut e = XmlElement::new("conditionalFormatting").with_attr("sqref", sqref_text(&cf.ranges));
    let tl = cf.ranges.first().map_or((0, 0), |r| (r.r0, r.c0));
    for (i, r) in cf.rules.iter().enumerate() {
        let id = ids.get(i).and_then(|x| x.as_deref());
        e.children.push(XmlNode::Element(cf_rule_el(r, tl, id)));
    }
    e
}

// ── data validation ───────────────────────────────────────────────────────

const DV_TYPES: &[(ValidationType, &str)] = &[
    (ValidationType::Any, "none"),
    (ValidationType::Whole, "whole"),
    (ValidationType::Decimal, "decimal"),
    (ValidationType::List, "list"),
    (ValidationType::Date, "date"),
    (ValidationType::Time, "time"),
    (ValidationType::TextLength, "textLength"),
    (ValidationType::Custom, "custom"),
];

/// A `dataValidation` (2007, or `x14:dataValidation` with `xm:f`/`xm:sqref` children).
pub(crate) fn parse_data_validation(d: &XmlElement, id: u32) -> Option<DataValidation> {
    let sqref = d
        .attr("sqref")
        .map(str::to_owned)
        .or_else(|| child_local(d, "sqref").map(XmlElement::text))?;
    let ranges = parse_ranges(&sqref);
    if ranges.is_empty() {
        return None;
    }
    let formula = |n: &str| {
        child_local(d, n).map(|f| {
            let t = child_local(f, "f").map_or_else(|| f.text(), XmlElement::text);
            model_formula(&t)
        })
    };
    let s = |n: &str| d.attr(n).map(str::to_owned);
    Some(DataValidation {
        id,
        ranges,
        kind: d
            .attr("type")
            .and_then(|t| DV_TYPES.iter().find(|(_, s)| *s == t))
            .map_or(ValidationType::Any, |(k, _)| *k),
        operator: operator_from(d.attr("operator")),
        formula1: formula("formula1"),
        formula2: formula("formula2"),
        allow_blank: abool_or(d, "allowBlank", false),
        in_cell_dropdown: !abool_or(d, "showDropDown", false),
        show_input_message: abool_or(d, "showInputMessage", false),
        prompt_title: s("promptTitle"),
        prompt: s("prompt"),
        show_error_message: abool_or(d, "showErrorMessage", false),
        error_style: match d.attr("errorStyle") {
            Some("warning") => ErrorStyle::Warning,
            Some("information") => ErrorStyle::Information,
            _ => ErrorStyle::Stop,
        },
        error_title: s("errorTitle"),
        error: s("error"),
    })
}

pub(crate) fn data_validation_el(v: &DataValidation) -> XmlElement {
    let mut e = XmlElement::new("dataValidation");
    if v.kind != ValidationType::Any {
        let t = DV_TYPES
            .iter()
            .find(|(k, _)| *k == v.kind)
            .map_or("none", |(_, s)| s);
        e.attrs.set("type", t);
    }
    match v.error_style {
        ErrorStyle::Stop => {}
        ErrorStyle::Warning => e.attrs.set("errorStyle", "warning"),
        ErrorStyle::Information => e.attrs.set("errorStyle", "information"),
    }
    if v.operator != Operator::Between
        && !matches!(
            v.kind,
            ValidationType::List | ValidationType::Custom | ValidationType::Any
        )
    {
        e.attrs.set("operator", operator_str(v.operator));
    }
    set_attr(&mut e, "allowBlank", flag(v.allow_blank));
    set_attr(&mut e, "showDropDown", flag(!v.in_cell_dropdown));
    set_attr(&mut e, "showInputMessage", flag(v.show_input_message));
    set_attr(&mut e, "showErrorMessage", flag(v.show_error_message));
    for (name, val) in [
        ("errorTitle", &v.error_title),
        ("error", &v.error),
        ("promptTitle", &v.prompt_title),
        ("prompt", &v.prompt),
    ] {
        if let Some(t) = val {
            e.attrs.set(name, t.clone());
        }
    }
    e.attrs.set("sqref", sqref_text(&v.ranges));
    if let Some(f) = &v.formula1 {
        e.children
            .push(XmlNode::Element(text_el("formula1", &file_formula(f))));
    }
    if let Some(f) = &v.formula2 {
        e.children
            .push(XmlNode::Element(text_el("formula2", &file_formula(f))));
    }
    e
}

// ── page setup ────────────────────────────────────────────────────────────

/// Reads `printOptions`, `pageMargins`, `pageSetup`, `headerFooter`, `rowBreaks`, `colBreaks` and
/// `sheetPr/pageSetUpPr@fitToPage` from a worksheet root.
pub(crate) fn parse_print(root: &XmlElement) -> PrintSettings {
    let mut p = PrintSettings::default();
    if let Some(o) = root.child("printOptions") {
        p.print_gridlines = abool_or(o, "gridLines", false);
        p.print_headings = abool_or(o, "headings", false);
        p.horizontal_centered = abool_or(o, "horizontalCentered", false);
        p.vertical_centered = abool_or(o, "verticalCentered", false);
    }
    if let Some(m) = root.child("pageMargins") {
        let d = Margins::default();
        p.margins = Margins {
            left: anum(m, "left").unwrap_or(d.left),
            right: anum(m, "right").unwrap_or(d.right),
            top: anum(m, "top").unwrap_or(d.top),
            bottom: anum(m, "bottom").unwrap_or(d.bottom),
            header: anum(m, "header").unwrap_or(d.header),
            footer: anum(m, "footer").unwrap_or(d.footer),
        };
    }
    if let Some(s) = root.child("pageSetup") {
        p.paper_size = au32(s, "paperSize").unwrap_or(1);
        p.scale = au32(s, "scale").map_or(100, |v| v.clamp(10, 400) as u16);
        p.fit_to_width = au32(s, "fitToWidth").unwrap_or(1);
        p.fit_to_height = au32(s, "fitToHeight").unwrap_or(1);
        p.orientation = if s.attr("orientation") == Some("landscape") {
            Orientation::Landscape
        } else {
            Orientation::Portrait
        };
        p.down_then_over = s.attr("pageOrder") != Some("overThenDown");
        p.black_and_white = abool_or(s, "blackAndWhite", false);
        p.draft = abool_or(s, "draft", false);
        if abool_or(s, "useFirstPageNumber", false) {
            p.first_page_number = au32(s, "firstPageNumber");
        }
        p.comments = match s.attr("cellComments") {
            Some("atEnd") => PrintComments::AtEnd,
            Some("asDisplayed") => PrintComments::AsDisplayed,
            _ => PrintComments::None,
        };
        p.errors = match s.attr("errors") {
            Some("blank") => PrintErrors::Blank,
            Some("dash") => PrintErrors::Dash,
            Some("NA") => PrintErrors::NA,
            _ => PrintErrors::Displayed,
        };
    }
    p.fit_to_page = root
        .child_path(&["sheetPr", "pageSetUpPr"])
        .is_some_and(|e| abool_or(e, "fitToPage", false));
    if let Some(h) = root.child("headerFooter") {
        p.header_footer = HeaderFooter {
            odd_header: child_text_opt(h, "oddHeader"),
            odd_footer: child_text_opt(h, "oddFooter"),
            even_header: child_text_opt(h, "evenHeader"),
            even_footer: child_text_opt(h, "evenFooter"),
            first_header: child_text_opt(h, "firstHeader"),
            first_footer: child_text_opt(h, "firstFooter"),
            different_odd_even: abool_or(h, "differentOddEven", false),
            different_first: abool_or(h, "differentFirst", false),
            scale_with_doc: abool_or(h, "scaleWithDoc", true),
            align_with_margins: abool_or(h, "alignWithMargins", true),
        };
    }
    let breaks = |name: &str| -> Vec<u32> {
        root.child(name)
            .map(|b| {
                b.children_named("brk")
                    .filter_map(|x| au32(x, "id"))
                    .collect()
            })
            .unwrap_or_default()
    };
    p.row_breaks = breaks("rowBreaks");
    p.col_breaks = breaks("colBreaks");
    p
}

fn child_text_opt(e: &XmlElement, name: &str) -> Option<String> {
    e.child(name).map(XmlElement::text)
}

pub(crate) fn print_options_el(p: &PrintSettings) -> Option<XmlElement> {
    let mut e = XmlElement::new("printOptions");
    set_attr(&mut e, "horizontalCentered", flag(p.horizontal_centered));
    set_attr(&mut e, "verticalCentered", flag(p.vertical_centered));
    set_attr(&mut e, "headings", flag(p.print_headings));
    set_attr(&mut e, "gridLines", flag(p.print_gridlines));
    (!e.attrs.is_empty()).then_some(e)
}

pub(crate) fn page_margins_el(m: &Margins) -> XmlElement {
    XmlElement::new("pageMargins")
        .with_attr("left", m.left)
        .with_attr("right", m.right)
        .with_attr("top", m.top)
        .with_attr("bottom", m.bottom)
        .with_attr("header", m.header)
        .with_attr("footer", m.footer)
}

/// `pageSetup`, keeping the attributes of `base` the model does not cover (printer settings
/// relationship, resolution, copies).
pub(crate) fn page_setup_el(p: &PrintSettings, base: Option<&XmlElement>) -> Option<XmlElement> {
    let mut e = base
        .cloned()
        .unwrap_or_else(|| XmlElement::new("pageSetup"));
    let set = |e: &mut XmlElement, k: &str, v: Option<String>| set_attr(e, k, v);
    set(
        &mut e,
        "paperSize",
        (p.paper_size != 1).then(|| p.paper_size.to_string()),
    );
    set(
        &mut e,
        "scale",
        (p.scale != 100).then(|| p.scale.to_string()),
    );
    set(
        &mut e,
        "firstPageNumber",
        p.first_page_number.map(|n| n.to_string()),
    );
    set(
        &mut e,
        "fitToWidth",
        (p.fit_to_width != 1).then(|| p.fit_to_width.to_string()),
    );
    set(
        &mut e,
        "fitToHeight",
        (p.fit_to_height != 1).then(|| p.fit_to_height.to_string()),
    );
    set(
        &mut e,
        "pageOrder",
        (!p.down_then_over).then(|| "overThenDown".to_owned()),
    );
    set(
        &mut e,
        "orientation",
        match p.orientation {
            Orientation::Portrait => base
                .and_then(|b| b.attr("orientation"))
                .filter(|o| *o == "portrait")
                .map(str::to_owned),
            Orientation::Landscape => Some("landscape".to_owned()),
        },
    );
    set(&mut e, "blackAndWhite", flag(p.black_and_white));
    set(&mut e, "draft", flag(p.draft));
    set(
        &mut e,
        "cellComments",
        match p.comments {
            PrintComments::None => None,
            PrintComments::AtEnd => Some("atEnd".to_owned()),
            PrintComments::AsDisplayed => Some("asDisplayed".to_owned()),
        },
    );
    set(
        &mut e,
        "useFirstPageNumber",
        flag(p.first_page_number.is_some()),
    );
    set(
        &mut e,
        "errors",
        match p.errors {
            PrintErrors::Displayed => None,
            PrintErrors::Blank => Some("blank".to_owned()),
            PrintErrors::Dash => Some("dash".to_owned()),
            PrintErrors::NA => Some("NA".to_owned()),
        },
    );
    (!e.attrs.is_empty() || base.is_some()).then_some(e)
}

pub(crate) fn header_footer_el(h: &HeaderFooter) -> Option<XmlElement> {
    let mut e = XmlElement::new("headerFooter");
    set_attr(&mut e, "differentOddEven", flag(h.different_odd_even));
    set_attr(&mut e, "differentFirst", flag(h.different_first));
    set_attr(&mut e, "scaleWithDoc", flag_default_true(h.scale_with_doc));
    set_attr(
        &mut e,
        "alignWithMargins",
        flag_default_true(h.align_with_margins),
    );
    for (name, v) in [
        ("oddHeader", &h.odd_header),
        ("oddFooter", &h.odd_footer),
        ("evenHeader", &h.even_header),
        ("evenFooter", &h.even_footer),
        ("firstHeader", &h.first_header),
        ("firstFooter", &h.first_footer),
    ] {
        if let Some(t) = v {
            e.children.push(XmlNode::Element(text_el(name, t)));
        }
    }
    (!e.attrs.is_empty() || !e.children.is_empty()).then_some(e)
}

pub(crate) fn breaks_el(name: &str, breaks: &[u32], rows: bool) -> Option<XmlElement> {
    if breaks.is_empty() {
        return None;
    }
    let max = if rows { "16383" } else { "1048575" };
    let mut e = XmlElement::new(name)
        .with_attr("count", breaks.len())
        .with_attr("manualBreakCount", breaks.len());
    for b in breaks {
        e.children.push(XmlNode::Element(
            XmlElement::new("brk")
                .with_attr("id", *b)
                .with_attr("max", max)
                .with_attr("man", "1"),
        ));
    }
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::xml::parse_xml_str;

    fn el(xml: &str) -> XmlElement {
        parse_xml_str(xml).expect("xml").root
    }

    #[test]
    fn views_round_trip() {
        let v = SheetView {
            freeze: Some(Freeze { rows: 2, cols: 1 }),
            zoom: 150,
            show_gridlines: false,
            selection: Selection {
                ranges: vec![Range::new(3, 2, 5, 4)],
                active: CellPos::new(3, 2),
            },
            ..SheetView::default()
        };
        assert_eq!(parse_sheet_view(&sheet_view_el(&v, true)), v);
        let split = SheetView {
            split: Some((160, 120)),
            ..SheetView::default()
        };
        assert_eq!(parse_sheet_view(&sheet_view_el(&split, false)), split);
        let fixture = el(
            r#"<sheetView workbookViewId="0"><pane xSplit="2" ySplit="2" topLeftCell="C3" activePane="bottomRight" state="frozen"/><selection pane="topRight"/><selection pane="bottomLeft"/><selection pane="bottomRight" activeCell="A1" sqref="A1"/></sheetView>"#,
        );
        let v = parse_sheet_view(&fixture);
        assert_eq!(v.freeze, Some(Freeze { rows: 2, cols: 2 }));
    }

    #[test]
    fn columns_merge_into_spans() {
        let p = ColProps {
            width: Some(F64(13.0)),
            custom_width: true,
            hidden: true,
            ..ColProps::default()
        };
        let cols = vec![(2, p), (3, p), (5, p)];
        let e = cols_el(&cols).expect("cols");
        assert_eq!(e.children_named("col").count(), 2);
        assert_eq!(parse_cols(&e), cols);
    }

    #[test]
    fn protection_inverts_the_file_flags() {
        let e = el(
            r#"<sheetProtection selectLockedCells="0" selectUnlockedCells="0" sheet="1" objects="0" insertRows="1" insertHyperlinks="1" autoFilter="1" scenarios="0" formatColumns="1" deleteColumns="1" insertColumns="1" pivotTables="1" deleteRows="1" formatCells="0" formatRows="1" sort="1" password="8A85"/>"#,
        );
        let p = parse_sheet_protection(&e).expect("protected");
        assert!(p.format_cells && !p.insert_rows && p.select_locked_cells && !p.objects);
        assert_eq!(p.password, Some(PasswordHash::Legacy(0x8A85)));
        assert_eq!(parse_sheet_protection(&sheet_protection_el(&p)), Some(p));
        assert_eq!(parse_sheet_protection(&el("<sheetProtection/>")), None);
    }

    #[test]
    fn filters_round_trip() {
        let af = AutoFilter {
            range: Range::new(0, 0, 30, 2),
            columns: vec![
                FilterColumn {
                    col_id: 1,
                    hidden_button: false,
                    show_button: true,
                    filter: Some(ColumnFilter::Values {
                        values: vec!["Red".into(), "Blue".into()],
                        blank: true,
                        date_groups: vec![DateGroupItem {
                            grouping: DateGrouping::Month,
                            year: 2026,
                            month: 3,
                            day: 0,
                            hour: 0,
                            minute: 0,
                            second: 0,
                        }],
                    }),
                },
                FilterColumn {
                    col_id: 2,
                    hidden_button: true,
                    show_button: true,
                    filter: Some(ColumnFilter::Custom {
                        and: true,
                        criteria: vec![CustomCriterion {
                            op: FilterOperator::GreaterThan,
                            value: "5".into(),
                        }],
                    }),
                },
                FilterColumn {
                    col_id: 0,
                    hidden_button: false,
                    show_button: true,
                    filter: Some(ColumnFilter::Dynamic(DynamicFilter::Quarter(2))),
                },
            ],
            sort: Some(SortState {
                range: Range::new(0, 0, 30, 2),
                case_sensitive: false,
                column_sort: false,
                conditions: vec![SortCondition {
                    range: Range::new(1, 2, 30, 2),
                    descending: true,
                    sort_by: SortBy::Value,
                    custom_list: None,
                    dxf: None,
                }],
            }),
        };
        let e = auto_filter_el(&af.range, &af.columns, af.sort.as_ref());
        assert_eq!(parse_auto_filter(&e), Some(af));
    }

    #[test]
    fn conditional_formats_round_trip_with_x14() {
        let bar = CfRule {
            id: 3,
            priority: 2,
            stop_if_true: false,
            kind: CfKind::DataBar(DataBar {
                min: Cfvo::of(CfvoType::AutoMin),
                max: Cfvo::of(CfvoType::AutoMax),
                color: Color::rgb(0x638EC6),
                show_value: true,
                min_length: 0,
                max_length: 100,
                gradient: false,
                border_color: Some(Color::rgb(0x112233)),
                negative_color: Some(Color::rgb(0xFF0000)),
                negative_border_color: None,
                axis_color: Some(Color::rgb(0)),
                axis_position: AxisPosition::Middle,
                direction: BarDirection::RightToLeft,
            }),
        };
        assert!(matches!(&bar.kind, CfKind::DataBar(b) if data_bar_needs_x14(b)));
        let base = cf_rule_el(&bar, (0, 0), Some("{X}"));
        let (mut back, ext) = parse_cf_rule(&base, 3).expect("rule");
        assert_eq!(ext.as_deref(), Some("{X}"));
        let x14 = x14_cf_rule_el(&bar, "{X}").expect("x14");
        apply_x14_rule(&mut back, &x14);
        assert_eq!(back, bar);
        for kind in [
            CfKind::CellIs {
                operator: Operator::NotBetween,
                formulas: vec!["1".into(), "XLOOKUP(1,A:A,B:B)".into()],
                dxf: 0,
            },
            CfKind::ContainsText {
                text: "a\"b".into(),
                dxf: 1,
            },
            CfKind::TimePeriod {
                period: TimePeriod::LastWeek,
                dxf: NO_DXF,
            },
            CfKind::Top10 {
                rank: 5,
                percent: true,
                bottom: true,
                dxf: 2,
            },
            CfKind::AboveAverage {
                above: false,
                equal: true,
                std_dev: 2,
                dxf: 2,
            },
            CfKind::ColorScale {
                cfvos: vec![
                    Cfvo::of(CfvoType::Min),
                    Cfvo::with(CfvoType::Formula, "$A$1*2"),
                ],
                colors: vec![Color::rgb(1), Color::theme(4, 0.2)],
            },
            CfKind::IconSet(IconSet {
                icon_set: IconSetName::Arrows5,
                cfvos: vec![Cfvo::with(CfvoType::Percent, "0")],
                reverse: true,
                show_value: false,
                custom_icons: Vec::new(),
            }),
        ] {
            let rule = CfRule {
                id: 1,
                priority: 7,
                stop_if_true: true,
                kind,
            };
            let e = cf_rule_el(&rule, (2, 3), None);
            assert_eq!(parse_cf_rule(&e, 1).map(|r| r.0), Some(rule));
        }
    }

    #[test]
    fn validations_and_print_settings_round_trip() {
        let mut v = DataValidation::new(4, vec![Range::new(0, 1, 9, 1)], ValidationType::Whole);
        v.operator = Operator::GreaterThanOrEqual;
        v.formula1 = Some("Sheet2!$A$1".into());
        v.error = Some("Too small".into());
        v.in_cell_dropdown = false;
        assert_eq!(parse_data_validation(&data_validation_el(&v), 4), Some(v));
        let p = PrintSettings {
            orientation: Orientation::Landscape,
            paper_size: 9,
            fit_to_width: 1,
            fit_to_height: 0,
            print_gridlines: true,
            first_page_number: Some(3),
            header_footer: HeaderFooter {
                odd_header: Some("&C&F".into()),
                ..PrintSettings::default().header_footer
            },
            row_breaks: vec![60],
            ..PrintSettings::default()
        };
        let mut root = XmlElement::new("worksheet");
        for e in [
            print_options_el(&p),
            Some(page_margins_el(&p.margins)),
            page_setup_el(&p, None),
            header_footer_el(&p.header_footer),
            breaks_el("rowBreaks", &p.row_breaks, true),
        ]
        .into_iter()
        .flatten()
        {
            root.children.push(XmlNode::Element(e));
        }
        assert_eq!(parse_print(&root), p);
    }
}
