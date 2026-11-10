// SPDX-License-Identifier: AGPL-3.0-or-later
//! PivotTables in SpreadsheetML: `pivotTableDefinition` parts on the sheets,
//! `pivotCacheDefinition` and `pivotCacheRecords` parts behind them, and the workbook's
//! `pivotCaches` list.
//!
//! Reading builds a [`kiln_sheet::pivot::PivotTable`] (fields, axes, filters, values with their
//! Show Values As, sorting, layout, totals, style) over the cache the file saved, items reordered
//! as the file lists them. What the model does not cover — calculated fields, grouped items,
//! OLAP and external sources, consolidation ranges — leaves the PivotTable a static table: its
//! cells as saved, its parts kept as they are. Writing keeps a PivotTable's parts byte for byte
//! while its model is as read; otherwise it regenerates the cache from the model (shared items,
//! records) and writes the definition with its items, marking the cache for a refresh when
//! Excel opens it.

use kiln_formula::values::ErrorValue;
use kiln_ooxml::xml::{XmlElement, XmlNode};
use kiln_sheet::pivot::cache::{CacheField, ItemKey, PivotCache};
use kiln_sheet::pivot::render::{PivotLine, PivotLineKind};
use kiln_sheet::pivot::{
    AxisItem, BaseItem, PivotField, PivotFilterOp, PivotFunction, PivotLabelFilter, PivotLayout,
    PivotOptions, PivotPage, PivotSort, PivotSource, PivotStyleInfo, PivotSubtotal, PivotTable,
    PivotValue, PivotValueFilter, ShowAs, SubtotalPlace,
};
use kiln_sheet::{CellPos, CellValue, Range};

use crate::xmlx::{abool_or, anum, au32};

/// Sheet → PivotTable.
pub(crate) const REL_PIVOT_TABLE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable";
/// PivotTable or workbook → cache definition.
pub(crate) const REL_PIVOT_CACHE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition";
/// Cache definition → records.
pub(crate) const REL_PIVOT_RECORDS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheRecords";
pub(crate) const CT_PIVOT_TABLE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml";
pub(crate) const CT_PIVOT_CACHE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml";
pub(crate) const CT_PIVOT_RECORDS: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml";

const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
/// `baseItem` of (previous) and (next).
const BASE_PREVIOUS: u32 = 1_048_828;
const BASE_NEXT: u32 = 1_048_829;
/// `x14:dataField` (Show Values As beyond the 2007 list).
const EXT_DATA_FIELD: &str = "{E15A36E0-9728-4e99-A89B-3F7291B0FE68}";
const NS_X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";

/// A PivotTable as read, with its parts.
#[derive(Debug, Clone)]
pub(crate) struct PivotRead {
    /// `/xl/pivotTables/pivotTable1.xml`.
    pub(crate) table_part: String,
    /// Its cache definition.
    pub(crate) cache_part: String,
    /// The records part, when the file saved the data.
    pub(crate) records_part: Option<String>,
    /// The workbook's `cacheId`.
    pub(crate) cache_id: u32,
    /// The model (`None`: kept as a static table).
    pub(crate) model: Option<PivotTable>,
    /// The definition's root.
    pub(crate) root: XmlElement,
}

// ── reading ──────────────────────────────────────────────────────────────────────────────────

fn item_value(e: &XmlElement) -> Option<CellValue> {
    let v = e.attr("v");
    Some(match e.name.rsplit(':').next().unwrap_or("") {
        "s" => CellValue::text(v.unwrap_or("")),
        "n" => CellValue::number(v?.trim().parse().ok()?),
        "b" => CellValue::Bool(matches!(v, Some("1" | "true"))),
        "e" => CellValue::Error(ErrorValue::from_code(v?).unwrap_or(ErrorValue::NA)),
        "m" => CellValue::Blank,
        // Dates are kept as their text (the model has no date items).
        "d" => CellValue::text(v.unwrap_or("")),
        _ => return None,
    })
}

fn function_of(name: &str) -> Option<PivotFunction> {
    Some(match name {
        "countA" => PivotFunction::Count,
        "count" => PivotFunction::CountNums,
        "avg" => PivotFunction::Average,
        other => PivotFunction::from_file_name(other)?,
    })
}

fn item_type(f: PivotFunction) -> &'static str {
    match f {
        PivotFunction::Sum => "sum",
        PivotFunction::Count => "countA",
        PivotFunction::Average => "avg",
        PivotFunction::Max => "max",
        PivotFunction::Min => "min",
        PivotFunction::Product => "product",
        PivotFunction::CountNums => "count",
        PivotFunction::StdDev => "stdDev",
        PivotFunction::StdDevP => "stdDevP",
        PivotFunction::Var => "var",
        PivotFunction::VarP => "varP",
    }
}

const SHOW_AS: [(&str, ShowAs); 14] = [
    ("normal", ShowAs::Normal),
    ("difference", ShowAs::Difference),
    ("percent", ShowAs::Percent),
    ("percentDiff", ShowAs::PercentDifference),
    ("runTotal", ShowAs::RunningTotal),
    ("percentOfRow", ShowAs::PercentOfRow),
    ("percentOfCol", ShowAs::PercentOfColumn),
    ("percentOfTotal", ShowAs::PercentOfTotal),
    ("index", ShowAs::Index),
    ("percentOfParentRow", ShowAs::PercentOfParentRow),
    ("percentOfParentCol", ShowAs::PercentOfParentColumn),
    ("percentOfRunningTotal", ShowAs::PercentRunningTotal),
    ("rankAscending", ShowAs::RankAscending),
    ("rankDescending", ShowAs::RankDescending),
];

/// The 2007 `showDataAs` values (the others are an `x14:dataField` extension).
fn core_show_as(s: ShowAs) -> bool {
    !matches!(
        s,
        ShowAs::PercentOfParentRow
            | ShowAs::PercentOfParentColumn
            | ShowAs::PercentRunningTotal
            | ShowAs::RankAscending
            | ShowAs::RankDescending
    )
}

const FILTER_OPS: [(&str, PivotFilterOp); 14] = [
    ("Equal", PivotFilterOp::Equal),
    ("NotEqual", PivotFilterOp::NotEqual),
    ("GreaterThan", PivotFilterOp::GreaterThan),
    ("GreaterThanOrEqual", PivotFilterOp::GreaterThanOrEqual),
    ("LessThan", PivotFilterOp::LessThan),
    ("LessThanOrEqual", PivotFilterOp::LessThanOrEqual),
    ("BeginsWith", PivotFilterOp::BeginsWith),
    ("NotBeginsWith", PivotFilterOp::NotBeginsWith),
    ("EndsWith", PivotFilterOp::EndsWith),
    ("NotEndsWith", PivotFilterOp::NotEndsWith),
    ("Contains", PivotFilterOp::Contains),
    ("NotContains", PivotFilterOp::NotContains),
    ("Between", PivotFilterOp::Between),
    ("NotBetween", PivotFilterOp::NotBetween),
];

fn el_named<'a>(e: &'a XmlElement, local: &'a str) -> impl Iterator<Item = &'a XmlElement> {
    e.elements()
        .filter(move |c| c.name.rsplit(':').next() == Some(local))
}

fn child<'a>(e: &'a XmlElement, local: &str) -> Option<&'a XmlElement> {
    e.elements()
        .find(|c| c.name.rsplit(':').next() == Some(local))
}

/// Reads a cache: fields with their shared items and the records.
pub(crate) fn read_cache(
    def: &XmlElement,
    records: Option<&XmlElement>,
) -> Result<(PivotSource, PivotCache), String> {
    let src = child(def, "cacheSource").ok_or("no cache source")?;
    if src.attr("type").unwrap_or("worksheet") != "worksheet" {
        return Err(format!(
            "the source is a {} (only worksheet sources are modelled)",
            src.attr("type").unwrap_or("?")
        ));
    }
    let ws = child(src, "worksheetSource").ok_or("no worksheet source")?;
    if ws.attr("r:id").is_some() {
        return Err("the source is another workbook".into());
    }
    let source = match (ws.attr("name"), ws.attr("ref"), ws.attr("sheet")) {
        (Some(name), _, _) => PivotSource::Table {
            name: name.to_owned(),
        },
        (None, Some(r), Some(sheet)) => PivotSource::Range {
            sheet: sheet.to_owned(),
            range: Range::parse_a1(r).ok_or_else(|| format!("bad source range {r}"))?,
        },
        _ => return Err("the source has no range".into()),
    };
    if child(def, "cacheHierarchies").is_some() {
        return Err("an OLAP cache".into());
    }
    let fields_el = child(def, "cacheFields").ok_or("no cache fields")?;
    let mut fields: Vec<CacheField> = Vec::new();
    for f in el_named(fields_el, "cacheField") {
        if f.attr("formula").is_some() {
            return Err("a calculated field".into());
        }
        if child(f, "fieldGroup").is_some() {
            return Err("grouped items".into());
        }
        let items: Vec<CellValue> = child(f, "sharedItems")
            .map(|s| s.elements().filter_map(item_value).collect())
            .unwrap_or_default();
        fields.push(CacheField {
            name: f.attr("name").unwrap_or("").to_owned(),
            items,
            index: Vec::new(),
            num_fmt: f
                .attr("numFmtId")
                .and_then(|n| n.parse().ok())
                .filter(|n| *n != 0)
                .and_then(kiln_sheet::styles::builtin_num_fmt)
                .map(str::to_owned),
        });
    }
    if fields.is_empty() {
        return Err("no cache fields".into());
    }
    let mut count = 0u32;
    if let Some(rec) = records {
        let mut keys: Vec<rustc_hash::FxHashMap<ItemKey, u32>> = fields
            .iter()
            .map(|f| {
                f.items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (ItemKey::of(v), u32::try_from(i).unwrap_or(0)))
                    .collect()
            })
            .collect();
        for r in el_named(rec, "r") {
            let mut k = 0;
            for v in r.elements() {
                let Some(field) = fields.get_mut(k) else {
                    break;
                };
                let index = if v.name.rsplit(':').next() == Some("x") {
                    v.attr("v").and_then(|x| x.parse::<u32>().ok()).unwrap_or(0)
                } else {
                    let val = item_value(v).unwrap_or_default();
                    let key = ItemKey::of(&val);
                    let next = u32::try_from(field.items.len()).unwrap_or(u32::MAX);
                    *keys[k].entry(key).or_insert_with(|| {
                        field.items.push(val);
                        next
                    })
                };
                if index as usize >= field.items.len() {
                    return Err("a record refers to an item that does not exist".into());
                }
                field.index.push(index);
                k += 1;
            }
            while k < fields.len() {
                // A short record: blanks.
                let f = &mut fields[k];
                let blank = f.find(&CellValue::Blank).unwrap_or_else(|| {
                    f.items.push(CellValue::Blank);
                    u32::try_from(f.items.len() - 1).unwrap_or(0)
                });
                f.index.push(blank);
                k += 1;
            }
            count += 1;
        }
    }
    Ok((
        source,
        PivotCache {
            fields,
            records: count,
        },
    ))
}

/// Builds the model of a definition over its cache.
#[allow(clippy::too_many_lines)]
pub(crate) fn read_table(
    root: &XmlElement,
    source: PivotSource,
    mut cache: PivotCache,
) -> Result<PivotTable, String> {
    let name = root.attr("name").unwrap_or("PivotTable1").to_owned();
    let loc = child(root, "location").ok_or("no location")?;
    let r = loc
        .attr("ref")
        .and_then(Range::parse_a1)
        .ok_or("bad location")?;
    let nf = cache.fields.len();
    let pf: Vec<&XmlElement> = child(root, "pivotFields")
        .map(|p| el_named(p, "pivotField").collect())
        .unwrap_or_default();
    // Item lists: the order the file shows them in; the cache follows it for manual sorts.
    let mut item_lists: Vec<Vec<u32>> = vec![Vec::new(); nf];
    let mut fields: Vec<PivotField> = cache
        .fields
        .iter()
        .map(|f| PivotField::named(&f.name))
        .collect();
    let mut compact = true;
    let mut outline = abool_or(root, "outline", false);
    let mut subtotal_top = true;
    let mut any_row_subtotals = false;
    let row_axis: Vec<i64> = child(root, "rowFields")
        .map(|f| {
            el_named(f, "field")
                .filter_map(|x| x.attr_int("x"))
                .collect()
        })
        .unwrap_or_default();
    for (i, p) in pf.iter().enumerate().take(nf) {
        let list: Vec<(u32, bool)> = child(p, "items")
            .map(|items| {
                el_named(items, "item")
                    .filter(|it| it.attr("t").is_none_or(|t| t == "data"))
                    .filter_map(|it| Some((au32(it, "x")?, abool_or(it, "h", false))))
                    .collect()
            })
            .unwrap_or_default();
        item_lists[i] = list.iter().map(|x| x.0).collect();
        let field = &mut fields[i];
        field.caption = p.attr("name").map(str::to_owned);
        field.sort = match p.attr("sortType") {
            Some("ascending") => PivotSort::Ascending,
            Some("descending") => PivotSort::Descending,
            _ => PivotSort::Source,
        };
        field.hidden = list
            .iter()
            .filter(|x| x.1)
            .filter_map(|x| cache.fields[i].items.get(x.0 as usize).cloned())
            .collect();
        // Subtotals.
        let custom: Vec<PivotFunction> = [
            ("sumSubtotal", PivotFunction::Sum),
            ("countASubtotal", PivotFunction::Count),
            ("avgSubtotal", PivotFunction::Average),
            ("maxSubtotal", PivotFunction::Max),
            ("minSubtotal", PivotFunction::Min),
            ("productSubtotal", PivotFunction::Product),
            ("countSubtotal", PivotFunction::CountNums),
            ("stdDevSubtotal", PivotFunction::StdDev),
            ("stdDevPSubtotal", PivotFunction::StdDevP),
            ("varSubtotal", PivotFunction::Var),
            ("varPSubtotal", PivotFunction::VarP),
        ]
        .into_iter()
        .filter(|(a, _)| abool_or(p, a, false))
        .map(|(_, f)| f)
        .collect();
        field.subtotal = if !custom.is_empty() {
            PivotSubtotal::Custom(custom)
        } else if abool_or(p, "defaultSubtotal", true) {
            PivotSubtotal::Automatic
        } else {
            PivotSubtotal::None
        };
        // Sort by a value.
        if let Some(scope) = child(p, "autoSortScope") {
            let v = scope
                .descendants("reference")
                .into_iter()
                .find(|r| r.attr("field") == Some("4294967294"))
                .and_then(|r| r.elements().next().and_then(|x| au32(x, "v")));
            field.sort_by_value = v.map(|v| v as usize);
            if field.sort == PivotSort::Source {
                field.sort = PivotSort::Ascending;
            }
        }
        if p.attr("axis") == Some("axisRow") {
            let pos = row_axis.iter().position(|&x| x == i as i64);
            let last = pos.is_some_and(|k| k + 1 == row_axis.len());
            compact &= abool_or(p, "compact", true);
            if !abool_or(p, "outline", true) {
                outline = false;
            } else if pos.is_some() {
                outline = true;
            }
            if !last {
                subtotal_top &= abool_or(p, "subtotalTop", true);
                any_row_subtotals |= field.subtotal != PivotSubtotal::None;
            }
        }
    }
    // Manual order: the cache takes the file's item order.
    for (i, f) in fields.iter().enumerate() {
        if f.sort == PivotSort::Source && !item_lists[i].is_empty() {
            let order = item_lists[i].clone();
            cache.fields[i].reorder(&order);
            item_lists[i] = (0..u32::try_from(order.len()).unwrap_or(0)).collect();
        }
    }
    let axis = |name: &str| -> Result<Vec<AxisItem>, String> {
        let Some(list) = child(root, name) else {
            return Ok(Vec::new());
        };
        el_named(list, "field")
            .map(|f| match f.attr_int("x") {
                Some(-2) => Ok(AxisItem::Values),
                Some(x) if x >= 0 && (x as usize) < nf => Ok(AxisItem::Field(x as usize)),
                other => Err(format!("a field {other:?} that does not exist")),
            })
            .collect()
    };
    let rows = axis("rowFields")?;
    let cols = axis("colFields")?;
    let pages: Vec<PivotPage> = child(root, "pageFields")
        .map(|p| {
            el_named(p, "pageField")
                .filter_map(|pg| {
                    let field = au32(pg, "fld")? as usize;
                    if field >= nf {
                        return None;
                    }
                    let item = au32(pg, "item")
                        .and_then(|i| item_lists[field].get(i as usize).copied())
                        .and_then(|x| cache.fields[field].items.get(x as usize).cloned());
                    Some(PivotPage { field, item })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut values = Vec::new();
    for d in child(root, "dataFields")
        .map(|d| el_named(d, "dataField").collect::<Vec<_>>())
        .unwrap_or_default()
    {
        let field = au32(d, "fld").ok_or("a value without a field")? as usize;
        if field >= nf {
            return Err("a value of a field that does not exist".into());
        }
        let function = d
            .attr("subtotal")
            .and_then(function_of)
            .unwrap_or(PivotFunction::Sum);
        let ext = d
            .descendants("x14:dataField")
            .first()
            .and_then(|x| x.attr("pivotShowAs"))
            .map(str::to_owned);
        let show_as = ext
            .as_deref()
            .or(d.attr("showDataAs"))
            .and_then(|s| SHOW_AS.iter().find(|x| x.0 == s).map(|x| x.1))
            .unwrap_or(ShowAs::Normal);
        let base_field = show_as
            .needs_base_field()
            .then(|| au32(d, "baseField").map(|b| b as usize))
            .flatten()
            .filter(|b| *b < nf);
        let base_item = match au32(d, "baseItem") {
            Some(BASE_PREVIOUS) | None => BaseItem::Previous,
            Some(BASE_NEXT) => BaseItem::Next,
            Some(i) => base_field
                .and_then(|b| {
                    let x = item_lists[b].get(i as usize).copied().unwrap_or(i);
                    cache.fields[b].items.get(x as usize).cloned()
                })
                .map_or(BaseItem::Previous, BaseItem::Item),
        };
        values.push(PivotValue {
            field,
            name: d.attr("name").map_or_else(
                || format!("{} of {}", function.caption(), fields[field].name),
                str::to_owned,
            ),
            function,
            show_as,
            base_field,
            base_item,
            num_fmt: d
                .attr("numFmtId")
                .and_then(|n| n.parse().ok())
                .filter(|n| *n != 0)
                .and_then(kiln_sheet::styles::builtin_num_fmt)
                .map(str::to_owned),
        });
    }
    // Label and value filters.
    if let Some(filters) = child(root, "filters") {
        for f in el_named(filters, "filter") {
            let Some(field) = au32(f, "fld").map(|x| x as usize).filter(|x| *x < nf) else {
                continue;
            };
            let ty = f.attr("type").unwrap_or("");
            let s1 = f.attr("stringValue1").unwrap_or("").to_owned();
            let s2 = f.attr("stringValue2").unwrap_or("").to_owned();
            if let Some(rest) = ty.strip_prefix("caption") {
                if let Some(op) = FILTER_OPS.iter().find(|x| x.0 == rest).map(|x| x.1) {
                    fields[field].label_filter = Some(PivotLabelFilter {
                        op,
                        value: s1,
                        value2: s2,
                    });
                }
            } else if let Some(rest) = ty.strip_prefix("value") {
                let value = au32(f, "iMeasureFld").unwrap_or(0) as usize;
                if let Some(op) = FILTER_OPS.iter().find(|x| x.0 == rest).map(|x| x.1) {
                    fields[field].value_filter = Some(PivotValueFilter::Compare {
                        value,
                        op,
                        a: s1.parse().unwrap_or(0.0),
                        b: s2.parse().unwrap_or(0.0),
                    });
                }
            } else if ty == "count" {
                let top = f.descendants("top10").first().copied();
                let value = au32(f, "iMeasureFld").unwrap_or(0) as usize;
                let count = top.and_then(|t| anum(t, "val")).unwrap_or(10.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let count = count.clamp(1.0, 1e6) as u32;
                fields[field].value_filter = Some(PivotValueFilter::Top {
                    value,
                    count,
                    bottom: top.is_some_and(|t| !abool_or(t, "top", true)),
                });
            }
        }
    }
    let style = child(root, "pivotTableStyleInfo");
    let options = PivotOptions {
        layout: if compact {
            PivotLayout::Compact
        } else if outline {
            PivotLayout::Outline
        } else {
            PivotLayout::Tabular
        },
        subtotals: if !any_row_subtotals && rows.len() > 1 {
            SubtotalPlace::Off
        } else if subtotal_top && compact_or_outline(compact, outline) {
            SubtotalPlace::Top
        } else {
            SubtotalPlace::Bottom
        },
        row_totals: abool_or(root, "rowGrandTotals", true),
        col_totals: abool_or(root, "colGrandTotals", true),
        repeat_labels: false,
        blank_rows: false,
        empty_text: abool_or(root, "showMissing", true)
            .then(|| root.attr("missingCaption").map(str::to_owned))
            .flatten(),
        row_caption: root.attr("rowHeaderCaption").map(str::to_owned),
        col_caption: root.attr("colHeaderCaption").map(str::to_owned),
        indent: 1,
        style: PivotStyleInfo {
            name: style.and_then(|s| s.attr("name")).unwrap_or("").to_owned(),
            row_headers: style.is_some_and(|s| abool_or(s, "showRowHeaders", false)),
            col_headers: style.is_some_and(|s| abool_or(s, "showColHeaders", false)),
            row_stripes: style.is_some_and(|s| abool_or(s, "showRowStripes", false)),
            col_stripes: style.is_some_and(|s| abool_or(s, "showColStripes", false)),
        },
    };
    let mut t = PivotTable::new(name, source, CellPos::new(r.r0, r.c0), cache);
    t.fields = fields
        .into_iter()
        .zip(t.cache.fields.iter())
        .map(|(f, c)| PivotField {
            name: c.name.clone(),
            ..f
        })
        .collect();
    t.rows = rows;
    t.cols = cols;
    t.pages = pages;
    t.values = values;
    t.options = options;
    // A table whose Σ Values sits on no axis with several values (LibreOffice's files leave it
    // out) gets it in Columns, as Excel shows it.
    t.settle_values_axis();
    Ok(t)
}

fn compact_or_outline(compact: bool, outline: bool) -> bool {
    compact || outline
}

// ── writing ──────────────────────────────────────────────────────────────────────────────────

fn item_el(v: &CellValue) -> XmlElement {
    match v {
        CellValue::Blank => XmlElement::new("m"),
        CellValue::Number(n) => {
            XmlElement::new("n").with_attr("v", kiln_ooxml::xml::format_number(*n))
        }
        CellValue::Bool(b) => XmlElement::new("b").with_attr("v", *b),
        CellValue::Error(e) => XmlElement::new("e").with_attr("v", e.code()),
        other => XmlElement::new("s").with_attr("v", other.as_text().unwrap_or_default()),
    }
}

/// The fields whose items the cache lists (the others' values are written in the records).
fn shared(t: &PivotTable, f: usize) -> bool {
    let c = &t.cache.fields[f];
    t.area_of(f).is_some() || !c.numeric_only() || c.items.len() <= 64
}

/// `pivotCacheDefinition` and `pivotCacheRecords` for a model.
pub(crate) fn cache_parts(t: &PivotTable, records_rel: &str) -> (XmlElement, XmlElement) {
    let mut fields =
        XmlElement::new("cacheFields").with_attr("count", t.cache.fields.len().to_string());
    for (i, c) in t.cache.fields.iter().enumerate() {
        let mut si = XmlElement::new("sharedItems");
        let (mut s, mut n, mut b, mut e, mut blank, mut ints) =
            (false, false, false, false, false, true);
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for v in &c.items {
            match v {
                CellValue::Blank => blank = true,
                CellValue::Number(x) => {
                    n = true;
                    ints &= x.fract() == 0.0;
                    lo = lo.min(*x);
                    hi = hi.max(*x);
                }
                CellValue::Bool(_) => b = true,
                CellValue::Error(_) => e = true,
                _ => s = true,
            }
        }
        if !s && !b && !e && !blank && n {
            si.attrs.set("containsSemiMixedTypes", "0");
        }
        if !s {
            si.attrs.set("containsString", "0");
        }
        if blank {
            si.attrs.set("containsBlank", "1");
        }
        if usize::from(s) + usize::from(n) + usize::from(b) + usize::from(e) > 1 {
            si.attrs.set("containsMixedTypes", "1");
        }
        if n {
            si.attrs.set("containsNumber", "1");
            if ints {
                si.attrs.set("containsInteger", "1");
            }
            si.attrs.set("minValue", kiln_ooxml::xml::format_number(lo));
            si.attrs.set("maxValue", kiln_ooxml::xml::format_number(hi));
        }
        if shared(t, i) {
            si.attrs.set("count", c.items.len().to_string());
            si.children = c
                .items
                .iter()
                .map(|v| XmlNode::Element(item_el(v)))
                .collect();
        }
        let num = c
            .num_fmt
            .as_deref()
            .and_then(kiln_sheet::styles::builtin_num_fmt_id)
            .unwrap_or(0);
        fields.children.push(XmlNode::Element(
            XmlElement::new("cacheField")
                .with_attr("name", c.name.as_str())
                .with_attr("numFmtId", num.to_string())
                .with_child(si),
        ));
    }
    let ws = match &t.source {
        PivotSource::Range { sheet, range } => XmlElement::new("worksheetSource")
            .with_attr("ref", range.to_a1(false))
            .with_attr("sheet", sheet.as_str()),
        PivotSource::Table { name } | PivotSource::Name { name } => {
            XmlElement::new("worksheetSource").with_attr("name", name.as_str())
        }
    };
    let def = XmlElement::new("pivotCacheDefinition")
        .with_attr("xmlns", NS)
        .with_attr("xmlns:r", NS_R)
        .with_attr("r:id", records_rel)
        .with_attr("refreshOnLoad", "1")
        .with_attr("refreshedBy", "Kiln")
        .with_attr("createdVersion", "6")
        .with_attr("refreshedVersion", "6")
        .with_attr("minRefreshableVersion", "3")
        .with_attr("recordCount", t.cache.records.to_string())
        .with_child(
            XmlElement::new("cacheSource")
                .with_attr("type", "worksheet")
                .with_child(ws),
        )
        .with_child(fields);
    let mut rec = XmlElement::new("pivotCacheRecords")
        .with_attr("xmlns", NS)
        .with_attr("xmlns:r", NS_R)
        .with_attr("count", t.cache.records.to_string());
    let share: Vec<bool> = (0..t.cache.fields.len()).map(|f| shared(t, f)).collect();
    for r in 0..t.cache.len() {
        let mut row = XmlElement::new("r");
        for (f, c) in t.cache.fields.iter().enumerate() {
            let x = c.index.get(r).copied().unwrap_or(0);
            row.children.push(XmlNode::Element(if share[f] {
                XmlElement::new("x").with_attr("v", x.to_string())
            } else {
                item_el(c.items.get(x as usize).unwrap_or(&CellValue::Blank))
            }));
        }
        rec.children.push(XmlNode::Element(row));
    }
    (def, rec)
}

/// A field's items in display order (indices into the cache's items).
fn display_order(t: &PivotTable, f: usize) -> Vec<u32> {
    let items = &t.cache.fields[f].items;
    let mut v: Vec<u32> = (0..u32::try_from(items.len()).unwrap_or(0)).collect();
    match t.fields[f].sort {
        PivotSort::Source => {}
        s => {
            v.sort_by(|&a, &b| {
                let o = kiln_sheet::pivot::compare_items(&items[a as usize], &items[b as usize]);
                if s == PivotSort::Descending {
                    o.reverse()
                } else {
                    o
                }
            });
        }
    }
    v
}

/// The `<i>` elements of rowItems or colItems.
fn line_items(axis: &[AxisItem], lines: &[PivotLine], positions: &[Vec<u32>]) -> XmlElement {
    let fields: Vec<usize> = axis
        .iter()
        .filter_map(|a| match a {
            AxisItem::Field(f) => Some(*f),
            AxisItem::Values => None,
        })
        .collect();
    let index_of = |f: usize, cache_item: u32| -> u32 {
        positions[f]
            .iter()
            .position(|&x| x == cache_item)
            .and_then(|p| u32::try_from(p).ok())
            .unwrap_or(0)
    };
    let mut out = XmlElement::new("rowItems").with_attr("count", lines.len().max(1).to_string());
    let mut prev: Vec<u32> = Vec::new();
    if lines.is_empty() {
        out.children.push(XmlNode::Element(XmlElement::new("i")));
        return out;
    }
    for line in lines {
        // The line's x values: one per axis position down to its level.
        let mut xs: Vec<u32> = Vec::new();
        let mut fi = 0;
        let depth = match line.kind {
            PivotLineKind::Grand => 0,
            _ => usize::from(line.level) + 1,
        };
        for a in axis.iter().take(depth.max(1)) {
            match a {
                AxisItem::Values => xs.push(u32::from(line.value.unwrap_or(0))),
                AxisItem::Field(_) => {
                    if let Some(&item) = line.path.get(fi) {
                        xs.push(index_of(fields[fi], item));
                    } else {
                        xs.push(0);
                    }
                    fi += 1;
                }
            }
        }
        if line.kind == PivotLineKind::Grand {
            xs = vec![u32::from(line.value.unwrap_or(0))];
        }
        let mut i = XmlElement::new("i");
        match line.kind {
            PivotLineKind::Grand => i.attrs.set("t", "grand"),
            PivotLineKind::Subtotal => {
                i.attrs.set("t", line.function.map_or("default", item_type));
            }
            PivotLineKind::Blank => i.attrs.set("t", "blank"),
            _ => {}
        }
        if let Some(v) = line.value
            && v > 0
        {
            i.attrs.set("i", v.to_string());
        }
        let repeat = if matches!(line.kind, PivotLineKind::Item | PivotLineKind::Group) {
            prev.iter()
                .zip(&xs)
                .take_while(|(a, b)| a == b)
                .count()
                .min(xs.len().saturating_sub(1))
        } else {
            0
        };
        if repeat > 0 {
            i.attrs.set("r", repeat.to_string());
        }
        for &x in &xs[repeat..] {
            let mut e = XmlElement::new("x");
            if x > 0 {
                e.attrs.set("v", x.to_string());
            }
            i.children.push(XmlNode::Element(e));
        }
        prev = xs;
        out.children.push(XmlNode::Element(i));
    }
    out
}

/// `pivotTableDefinition` for a model; `read` is the definition read, whose extension list is
/// kept.
#[allow(clippy::too_many_lines)]
pub(crate) fn table_part(t: &PivotTable, cache_id: u32, read: Option<&XmlElement>) -> XmlElement {
    let out = &t.output;
    let o = &t.options;
    let compact = o.layout == PivotLayout::Compact;
    let outline = o.layout != PivotLayout::Tabular;
    let range = out.range.unwrap_or(Range::cell(t.at.row, t.at.col));
    let mut root = XmlElement::new("pivotTableDefinition")
        .with_attr("xmlns", NS)
        .with_attr("name", t.name.as_str())
        .with_attr("cacheId", cache_id.to_string())
        .with_attr("applyNumberFormats", "0")
        .with_attr("applyBorderFormats", "0")
        .with_attr("applyFontFormats", "0")
        .with_attr("applyPatternFormats", "0")
        .with_attr("applyAlignmentFormats", "0")
        .with_attr("applyWidthHeightFormats", "1")
        .with_attr("dataCaption", "Values")
        .with_attr("updatedVersion", "6")
        .with_attr("minRefreshableVersion", "3")
        .with_attr("useAutoFormatting", "1")
        .with_attr("itemPrintTitles", "1")
        .with_attr("createdVersion", "6")
        .with_attr("indent", "0")
        .with_attr("outline", "1")
        .with_attr("outlineData", "1")
        .with_attr("multipleFieldFilters", "0");
    if !o.row_totals {
        root.attrs.set("rowGrandTotals", "0");
    }
    if !o.col_totals {
        root.attrs.set("colGrandTotals", "0");
    }
    if !compact {
        root.attrs.set("compact", "0");
        root.attrs.set("compactData", "0");
    }
    if let Some(c) = &o.row_caption {
        root.attrs.set("rowHeaderCaption", c.clone());
    }
    if let Some(c) = &o.col_caption {
        root.attrs.set("colHeaderCaption", c.clone());
    }
    if let Some(m) = &o.empty_text {
        root.attrs.set("missingCaption", m.clone());
    }
    if t.rows.contains(&AxisItem::Values) {
        root.attrs.set("dataOnRows", "1");
    }
    let mut loc = XmlElement::new("location")
        .with_attr("ref", range.to_a1(false))
        .with_attr("firstHeaderRow", "1")
        .with_attr("firstDataRow", out.header_rows.to_string())
        .with_attr("firstDataCol", out.label_cols.to_string());
    if !t.pages.is_empty() {
        loc.attrs.set("rowPageCount", t.pages.len().to_string());
        loc.attrs.set("colPageCount", "1");
    }
    root.children.push(XmlNode::Element(loc));
    // Fields with their items in display order.
    let positions: Vec<Vec<u32>> = (0..t.fields.len()).map(|f| display_order(t, f)).collect();
    let mut pfs = XmlElement::new("pivotFields").with_attr("count", t.fields.len().to_string());
    for (f, field) in t.fields.iter().enumerate() {
        let mut p = XmlElement::new("pivotField");
        if let Some(c) = &field.caption {
            p.attrs.set("name", c.clone());
        }
        let area = t.area_of(f);
        match area {
            Some(kiln_sheet::pivot::PivotArea::Rows) => p.attrs.set("axis", "axisRow"),
            Some(kiln_sheet::pivot::PivotArea::Columns) => p.attrs.set("axis", "axisCol"),
            Some(kiln_sheet::pivot::PivotArea::Filters) => p.attrs.set("axis", "axisPage"),
            _ => {}
        }
        if t.values.iter().any(|v| v.field == f) {
            p.attrs.set("dataField", "1");
        }
        if !compact {
            p.attrs.set("compact", "0");
        }
        if !outline {
            p.attrs.set("outline", "0");
        }
        p.attrs.set("showAll", "0");
        if area.is_none() && field.subtotal == PivotSubtotal::None {
            p.attrs.set("defaultSubtotal", "0");
        }
        if area.is_some() {
            match field.sort {
                PivotSort::Ascending => p.attrs.set("sortType", "ascending"),
                PivotSort::Descending => p.attrs.set("sortType", "descending"),
                PivotSort::Source => {}
            }
            if o.subtotals == SubtotalPlace::Bottom {
                p.attrs.set("subtotalTop", "0");
            }
            let mut items = XmlElement::new("items");
            let hidden: Vec<ItemKey> = field.hidden.iter().map(ItemKey::of).collect();
            for &x in &positions[f] {
                let mut it = XmlElement::new("item").with_attr("x", x.to_string());
                if hidden.contains(&ItemKey::of(&t.cache.fields[f].items[x as usize])) {
                    it.attrs.set("h", "1");
                }
                items.children.push(XmlNode::Element(it));
            }
            match &field.subtotal {
                PivotSubtotal::Automatic if o.subtotals != SubtotalPlace::Off => {
                    items.children.push(XmlNode::Element(
                        XmlElement::new("item").with_attr("t", "default"),
                    ));
                }
                PivotSubtotal::Custom(fs) if o.subtotals != SubtotalPlace::Off => {
                    p.attrs.set("defaultSubtotal", "0");
                    for func in fs {
                        let attr = match item_type(*func) {
                            "countA" => "countASubtotal".to_owned(),
                            "avg" => "avgSubtotal".to_owned(),
                            other => format!("{other}Subtotal"),
                        };
                        p.attrs.set(&attr, "1");
                        items.children.push(XmlNode::Element(
                            XmlElement::new("item").with_attr("t", item_type(*func)),
                        ));
                    }
                }
                _ => p.attrs.set("defaultSubtotal", "0"),
            }
            items.attrs.set("count", items.children.len().to_string());
            p.children.push(XmlNode::Element(items));
            if let Some(v) = field.sort_by_value {
                p.children.push(XmlNode::Element(
                    XmlElement::new("autoSortScope").with_child(
                        XmlElement::new("pivotArea")
                            .with_attr("dataOnly", "0")
                            .with_attr("outline", "0")
                            .with_attr("fieldPosition", "0")
                            .with_child(
                                XmlElement::new("references")
                                    .with_attr("count", "1")
                                    .with_child(
                                        XmlElement::new("reference")
                                            .with_attr("field", "4294967294")
                                            .with_attr("count", "1")
                                            .with_attr("selected", "0")
                                            .with_child(
                                                XmlElement::new("x").with_attr("v", v.to_string()),
                                            ),
                                    ),
                            ),
                    ),
                ));
            }
        }
        pfs.children.push(XmlNode::Element(p));
    }
    root.children.push(XmlNode::Element(pfs));
    let field_list = |name: &str, axis: &[AxisItem]| -> XmlElement {
        let mut e = XmlElement::new(name).with_attr("count", axis.len().to_string());
        for a in axis {
            let x = match a {
                AxisItem::Field(f) => f.to_string(),
                AxisItem::Values => "-2".to_owned(),
            };
            e.children
                .push(XmlNode::Element(XmlElement::new("field").with_attr("x", x)));
        }
        e
    };
    if !t.rows.is_empty() {
        root.children
            .push(XmlNode::Element(field_list("rowFields", &t.rows)));
    }
    let mut rows = line_items(&t.rows, &out.rows, &positions);
    rows.name = "rowItems".into();
    root.children.push(XmlNode::Element(rows));
    if !t.cols.is_empty() {
        root.children
            .push(XmlNode::Element(field_list("colFields", &t.cols)));
    }
    let mut cols = line_items(&t.cols, &out.cols, &positions);
    cols.name = "colItems".into();
    root.children.push(XmlNode::Element(cols));
    if !t.pages.is_empty() {
        let mut pages = XmlElement::new("pageFields").with_attr("count", t.pages.len().to_string());
        for p in &t.pages {
            let mut e = XmlElement::new("pageField")
                .with_attr("fld", p.field.to_string())
                .with_attr("hier", "-1");
            if let Some(item) = &p.item
                && let Some(x) = t.cache.fields[p.field].find(item)
                && let Some(pos) = positions[p.field].iter().position(|&y| y == x)
            {
                e.attrs.set("item", pos.to_string());
            }
            pages.children.push(XmlNode::Element(e));
        }
        root.children.push(XmlNode::Element(pages));
    }
    if !t.values.is_empty() {
        let mut data = XmlElement::new("dataFields").with_attr("count", t.values.len().to_string());
        for v in &t.values {
            let mut e = XmlElement::new("dataField")
                .with_attr("name", v.name.as_str())
                .with_attr("fld", v.field.to_string());
            if v.function != PivotFunction::Sum {
                e.attrs.set("subtotal", v.function.file_name());
            }
            if v.show_as != ShowAs::Normal {
                let name = SHOW_AS
                    .iter()
                    .find(|x| x.1 == v.show_as)
                    .map_or("normal", |x| x.0);
                if core_show_as(v.show_as) {
                    e.attrs.set("showDataAs", name);
                } else {
                    e.children.push(XmlNode::Element(
                        XmlElement::new("extLst").with_child(
                            XmlElement::new("ext")
                                .with_attr("uri", EXT_DATA_FIELD)
                                .with_attr("xmlns:x14", NS_X14)
                                .with_child(
                                    XmlElement::new("x14:dataField").with_attr("pivotShowAs", name),
                                ),
                        ),
                    ));
                }
            }
            let base = v.base_field.unwrap_or(0);
            e.attrs.set("baseField", base.to_string());
            let base_item = match &v.base_item {
                BaseItem::Previous => BASE_PREVIOUS,
                BaseItem::Next => BASE_NEXT,
                BaseItem::Item(item) => t.cache.fields[base]
                    .find(item)
                    .and_then(|x| positions[base].iter().position(|&y| y == x))
                    .and_then(|p| u32::try_from(p).ok())
                    .unwrap_or(0),
            };
            e.attrs.set(
                "baseItem",
                if v.base_field.is_some() { base_item } else { 0 }.to_string(),
            );
            if let Some(id) = v
                .num_fmt
                .as_deref()
                .and_then(kiln_sheet::styles::builtin_num_fmt_id)
            {
                e.attrs.set("numFmtId", id.to_string());
            }
            // Children (the extension list) after the attributes.
            data.children.push(XmlNode::Element(e));
        }
        root.children.push(XmlNode::Element(data));
    }
    let s = &o.style;
    root.children.push(XmlNode::Element(
        XmlElement::new("pivotTableStyleInfo")
            .with_attr("name", s.name.as_str())
            .with_attr("showRowHeaders", s.row_headers)
            .with_attr("showColHeaders", s.col_headers)
            .with_attr("showRowStripes", s.row_stripes)
            .with_attr("showColStripes", s.col_stripes)
            .with_attr("showLastColumn", true),
    ));
    // Label and value filters.
    let mut filters = XmlElement::new("filters");
    let mut id = 1;
    for (f, field) in t.fields.iter().enumerate() {
        if let Some(l) = &field.label_filter {
            let op = FILTER_OPS
                .iter()
                .find(|x| x.1 == l.op)
                .map_or("Equal", |x| x.0);
            let mut e = XmlElement::new("filter")
                .with_attr("fld", f.to_string())
                .with_attr("type", format!("caption{op}"))
                .with_attr("evalOrder", "-1")
                .with_attr("id", id.to_string())
                .with_attr("stringValue1", l.value.as_str());
            if matches!(l.op, PivotFilterOp::Between | PivotFilterOp::NotBetween) {
                e.attrs.set("stringValue2", l.value2.clone());
            }
            e.children.push(XmlNode::Element(auto_filter_el(None)));
            filters.children.push(XmlNode::Element(e));
            id += 1;
        }
        match &field.value_filter {
            Some(PivotValueFilter::Top {
                value,
                count,
                bottom,
            }) => {
                let mut top = XmlElement::new("top10")
                    .with_attr("val", count.to_string())
                    .with_attr("filterVal", count.to_string());
                if *bottom {
                    top.attrs.set("top", "0");
                }
                filters.children.push(XmlNode::Element(
                    XmlElement::new("filter")
                        .with_attr("fld", f.to_string())
                        .with_attr("type", "count")
                        .with_attr("evalOrder", "-1")
                        .with_attr("id", id.to_string())
                        .with_attr("iMeasureFld", value.to_string())
                        .with_child(auto_filter_el(Some(top))),
                ));
                id += 1;
            }
            Some(PivotValueFilter::Compare { value, op, a, b }) => {
                let name = FILTER_OPS
                    .iter()
                    .find(|x| x.1 == *op)
                    .map_or("Equal", |x| x.0);
                let mut e = XmlElement::new("filter")
                    .with_attr("fld", f.to_string())
                    .with_attr("type", format!("value{name}"))
                    .with_attr("evalOrder", "-1")
                    .with_attr("id", id.to_string())
                    .with_attr("iMeasureFld", value.to_string())
                    .with_attr("stringValue1", kiln_ooxml::xml::format_number(*a));
                if matches!(op, PivotFilterOp::Between | PivotFilterOp::NotBetween) {
                    e.attrs
                        .set("stringValue2", kiln_ooxml::xml::format_number(*b));
                }
                e.children.push(XmlNode::Element(auto_filter_el(None)));
                filters.children.push(XmlNode::Element(e));
                id += 1;
            }
            None => {}
        }
    }
    if !filters.children.is_empty() {
        filters
            .attrs
            .set("count", filters.children.len().to_string());
        root.children.push(XmlNode::Element(filters));
    }
    if let Some(ext) = read.and_then(|r| child(r, "extLst")) {
        root.children.push(XmlNode::Element(ext.clone()));
    }
    root
}

fn auto_filter_el(top: Option<XmlElement>) -> XmlElement {
    let col = XmlElement::new("filterColumn").with_attr("colId", "0");
    let col = match top {
        Some(t) => col.with_child(t),
        None => col,
    };
    XmlElement::new("autoFilter")
        .with_attr("ref", "A1")
        .with_child(col)
}
