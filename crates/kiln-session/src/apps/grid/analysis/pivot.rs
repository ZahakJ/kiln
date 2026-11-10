// SPDX-License-Identifier: AGPL-3.0-or-later
//! PivotTables in Kiln Grid: Insert ▸ PivotTable, the PivotTable Fields pane's moves, Value and
//! Field Settings, the PivotTable Analyze and Design tabs, Refresh, Show Details, and the state
//! the pane and tabs show. Every change writes the report again as one undo step: the old
//! report's cells cleared, the new ones written, the definition stored in the sheet's metadata.

use std::sync::Arc;

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::GridRange;
use kiln_protocol::commands::grid_analysis::{
    GridPivotArea, GridPivotEdit, GridPivotFieldInfo, GridPivotFieldSettings, GridPivotFilterOp,
    GridPivotFunction, GridPivotInfo, GridPivotLabelFilter, GridPivotLayout, GridPivotOptions,
    GridPivotSort, GridPivotSubtotals, GridPivotValue, GridPivotValueFilter, GridShowAs,
    GridSubtotalPlace,
};
use kiln_protocol::commands::grid_features::GridFilterItem;
use kiln_sheet::commands::{CellInput, ClearWhat, Content, MetaField};
use kiln_sheet::pivot::cache::{PivotCache, split_reference};
use kiln_sheet::pivot::render::{PivotLineKind, render};
use kiln_sheet::pivot::{
    AxisItem, BaseItem, PivotArea, PivotField, PivotFilterOp, PivotFunction, PivotLabelFilter,
    PivotLayout, PivotOptions, PivotPage, PivotSort, PivotSource, PivotSubtotal, PivotTable,
    PivotValue, PivotValueFilter, ShowAs, SubtotalPlace,
};
use kiln_sheet::styles::{CellStyle, HAlign};
use kiln_sheet::{CellPos, CellValue, Command, Range, SheetId};

use super::super::GridDoc;
use crate::model::Change;

fn bad(s: impl Into<String>) -> KilnError {
    KilnError::InvalidInput(s.into())
}

/// The message Excel gives for a change inside a report.
pub(crate) const PIVOT_LOCKED: &str = "Cannot change this part of a PivotTable report.";

// ── protocol ↔ model ─────────────────────────────────────────────────────────────────────────

pub(crate) fn function(f: GridPivotFunction) -> PivotFunction {
    use GridPivotFunction as G;
    match f {
        G::Sum => PivotFunction::Sum,
        G::Count => PivotFunction::Count,
        G::Average => PivotFunction::Average,
        G::Max => PivotFunction::Max,
        G::Min => PivotFunction::Min,
        G::Product => PivotFunction::Product,
        G::CountNums => PivotFunction::CountNums,
        G::StdDev => PivotFunction::StdDev,
        G::StdDevP => PivotFunction::StdDevP,
        G::Var => PivotFunction::Var,
        G::VarP => PivotFunction::VarP,
    }
}

fn function_out(f: PivotFunction) -> GridPivotFunction {
    use GridPivotFunction as G;
    match f {
        PivotFunction::Sum => G::Sum,
        PivotFunction::Count => G::Count,
        PivotFunction::Average => G::Average,
        PivotFunction::Max => G::Max,
        PivotFunction::Min => G::Min,
        PivotFunction::Product => G::Product,
        PivotFunction::CountNums => G::CountNums,
        PivotFunction::StdDev => G::StdDev,
        PivotFunction::StdDevP => G::StdDevP,
        PivotFunction::Var => G::Var,
        PivotFunction::VarP => G::VarP,
    }
}

const SHOW_AS: [(GridShowAs, ShowAs); 14] = [
    (GridShowAs::Normal, ShowAs::Normal),
    (GridShowAs::PercentOfTotal, ShowAs::PercentOfTotal),
    (GridShowAs::PercentOfColumn, ShowAs::PercentOfColumn),
    (GridShowAs::PercentOfRow, ShowAs::PercentOfRow),
    (GridShowAs::PercentOfParentRow, ShowAs::PercentOfParentRow),
    (
        GridShowAs::PercentOfParentColumn,
        ShowAs::PercentOfParentColumn,
    ),
    (GridShowAs::Percent, ShowAs::Percent),
    (GridShowAs::Difference, ShowAs::Difference),
    (GridShowAs::PercentDifference, ShowAs::PercentDifference),
    (GridShowAs::RunningTotal, ShowAs::RunningTotal),
    (GridShowAs::PercentRunningTotal, ShowAs::PercentRunningTotal),
    (GridShowAs::RankAscending, ShowAs::RankAscending),
    (GridShowAs::RankDescending, ShowAs::RankDescending),
    (GridShowAs::Index, ShowAs::Index),
];

fn show_as(s: GridShowAs) -> ShowAs {
    SHOW_AS
        .iter()
        .find(|x| x.0 == s)
        .map_or(ShowAs::Normal, |x| x.1)
}

fn show_as_out(s: ShowAs) -> GridShowAs {
    SHOW_AS
        .iter()
        .find(|x| x.1 == s)
        .map_or(GridShowAs::Normal, |x| x.0)
}

const OPS: [(GridPivotFilterOp, PivotFilterOp); 14] = [
    (GridPivotFilterOp::Equal, PivotFilterOp::Equal),
    (GridPivotFilterOp::NotEqual, PivotFilterOp::NotEqual),
    (GridPivotFilterOp::GreaterThan, PivotFilterOp::GreaterThan),
    (
        GridPivotFilterOp::GreaterThanOrEqual,
        PivotFilterOp::GreaterThanOrEqual,
    ),
    (GridPivotFilterOp::LessThan, PivotFilterOp::LessThan),
    (
        GridPivotFilterOp::LessThanOrEqual,
        PivotFilterOp::LessThanOrEqual,
    ),
    (GridPivotFilterOp::BeginsWith, PivotFilterOp::BeginsWith),
    (
        GridPivotFilterOp::NotBeginsWith,
        PivotFilterOp::NotBeginsWith,
    ),
    (GridPivotFilterOp::EndsWith, PivotFilterOp::EndsWith),
    (GridPivotFilterOp::NotEndsWith, PivotFilterOp::NotEndsWith),
    (GridPivotFilterOp::Contains, PivotFilterOp::Contains),
    (GridPivotFilterOp::NotContains, PivotFilterOp::NotContains),
    (GridPivotFilterOp::Between, PivotFilterOp::Between),
    (GridPivotFilterOp::NotBetween, PivotFilterOp::NotBetween),
];

fn op(o: GridPivotFilterOp) -> PivotFilterOp {
    OPS.iter()
        .find(|x| x.0 == o)
        .map_or(PivotFilterOp::Equal, |x| x.1)
}

fn op_out(o: PivotFilterOp) -> GridPivotFilterOp {
    OPS.iter()
        .find(|x| x.1 == o)
        .map_or(GridPivotFilterOp::Equal, |x| x.0)
}

/// The label an item shows (and the filter lists use): `(blank)`, a number in its field's
/// format, the text.
pub(crate) fn item_label(t: &PivotTable, field: usize, item: usize) -> String {
    let Some(cf) = t.cache.fields.get(field) else {
        return String::new();
    };
    match cf.items.get(item) {
        Some(v) if v.is_empty_text_or_blank() => "(blank)".into(),
        Some(v @ CellValue::Number(_)) => match &cf.num_fmt {
            Some(code) => kiln_sheet::format::format_value(v, code, false).text,
            None => v.display_general(),
        },
        Some(v) => v.display_general(),
        None => String::new(),
    }
}

/// The item of a label (as [`item_label`] shows it).
fn item_of(t: &PivotTable, field: usize, label: &str) -> Option<CellValue> {
    let cf = t.cache.fields.get(field)?;
    (0..cf.items.len())
        .find(|&i| item_label(t, field, i).eq_ignore_ascii_case(label))
        .map(|i| cf.items[i].clone())
}

fn settings_out(t: &PivotTable, f: usize) -> GridPivotFieldSettings {
    let field = &t.fields[f];
    let cf = &t.cache.fields[f];
    GridPivotFieldSettings {
        caption: field.caption.clone(),
        subtotals: match &field.subtotal {
            PivotSubtotal::Automatic => GridPivotSubtotals::Automatic,
            PivotSubtotal::None => GridPivotSubtotals::None,
            PivotSubtotal::Custom(fs) => {
                GridPivotSubtotals::Custom(fs.iter().map(|f| function_out(*f)).collect())
            }
        },
        sort: match field.sort {
            PivotSort::Ascending => GridPivotSort::Ascending,
            PivotSort::Descending => GridPivotSort::Descending,
            PivotSort::Source => GridPivotSort::Source,
        },
        sort_by_value: field.sort_by_value.and_then(|v| u32::try_from(v).ok()),
        hidden: field
            .hidden
            .iter()
            .filter_map(|h| cf.find(h))
            .map(|i| item_label(t, f, i as usize))
            .collect(),
        label_filter: field.label_filter.as_ref().map(|l| GridPivotLabelFilter {
            op: op_out(l.op),
            value: l.value.clone(),
            value2: l.value2.clone(),
        }),
        value_filter: field.value_filter.as_ref().map(|v| match v {
            PivotValueFilter::Top {
                value,
                count,
                bottom,
            } => GridPivotValueFilter {
                value: u32::try_from(*value).unwrap_or(0),
                top: Some((*count, *bottom)),
                op: GridPivotFilterOp::GreaterThan,
                a: 0.0,
                b: 0.0,
            },
            PivotValueFilter::Compare { value, op, a, b } => GridPivotValueFilter {
                value: u32::try_from(*value).unwrap_or(0),
                top: None,
                op: op_out(*op),
                a: *a,
                b: *b,
            },
        }),
    }
}

fn apply_settings(t: &mut PivotTable, f: usize, s: &GridPivotFieldSettings) -> Result<()> {
    let hidden: Vec<CellValue> = s
        .hidden
        .iter()
        .filter_map(|label| item_of(t, f, label))
        .collect();
    let items = t.cache.fields[f].items.len();
    if items > 0 && hidden.len() >= items {
        return Err(bad("At least one item must be shown."));
    }
    let nvalues = t.values.len();
    let check_value = |v: u32| -> Result<usize> {
        let v = v as usize;
        if v < nvalues {
            Ok(v)
        } else {
            Err(bad("There is no such value field."))
        }
    };
    let sort_by_value = s.sort_by_value.map(check_value).transpose()?;
    let value_filter = match &s.value_filter {
        None => None,
        Some(v) => Some(match v.top {
            Some((count, bottom)) => PivotValueFilter::Top {
                value: check_value(v.value)?,
                count: count.clamp(1, 1_000_000),
                bottom,
            },
            None => PivotValueFilter::Compare {
                value: check_value(v.value)?,
                op: op(v.op),
                a: v.a,
                b: v.b,
            },
        }),
    };
    let caption = s
        .caption
        .as_ref()
        .map(|c| c.trim().to_owned())
        .filter(|c| !c.is_empty() && *c != t.fields[f].name);
    if let Some(c) = &caption
        && (t
            .fields
            .iter()
            .enumerate()
            .any(|(i, x)| i != f && x.shown_name().eq_ignore_ascii_case(c))
            || t.values.iter().any(|v| v.name.eq_ignore_ascii_case(c)))
    {
        return Err(bad("A PivotTable field name already exists."));
    }
    let field = &mut t.fields[f];
    field.caption = caption;
    field.subtotal = match &s.subtotals {
        GridPivotSubtotals::Automatic => PivotSubtotal::Automatic,
        GridPivotSubtotals::None => PivotSubtotal::None,
        GridPivotSubtotals::Custom(fs) if fs.is_empty() => PivotSubtotal::None,
        GridPivotSubtotals::Custom(fs) => {
            PivotSubtotal::Custom(fs.iter().map(|f| function(*f)).collect())
        }
    };
    field.sort = match s.sort {
        GridPivotSort::Ascending => PivotSort::Ascending,
        GridPivotSort::Descending => PivotSort::Descending,
        GridPivotSort::Source => PivotSort::Source,
    };
    field.sort_by_value = sort_by_value;
    field.hidden = hidden;
    field.label_filter = s.label_filter.as_ref().map(|l| PivotLabelFilter {
        op: op(l.op),
        value: l.value.clone(),
        value2: l.value2.clone(),
    });
    field.value_filter = value_filter;
    Ok(())
}

fn value_out(t: &PivotTable, v: &PivotValue) -> GridPivotValue {
    GridPivotValue {
        field: u32::try_from(v.field).unwrap_or(0),
        name: v.name.clone(),
        function: function_out(v.function),
        show_as: show_as_out(v.show_as),
        base_field: v.base_field.and_then(|f| u32::try_from(f).ok()),
        base_item: v.base_field.map(|f| match &v.base_item {
            BaseItem::Previous => "(previous)".to_owned(),
            BaseItem::Next => "(next)".to_owned(),
            BaseItem::Item(item) => t.cache.fields[f]
                .find(item)
                .map(|i| item_label(t, f, i as usize))
                .unwrap_or_default(),
        }),
        num_fmt: v.num_fmt.clone(),
    }
}

fn options_out(o: &PivotOptions) -> GridPivotOptions {
    GridPivotOptions {
        layout: match o.layout {
            PivotLayout::Compact => GridPivotLayout::Compact,
            PivotLayout::Outline => GridPivotLayout::Outline,
            PivotLayout::Tabular => GridPivotLayout::Tabular,
        },
        subtotals: match o.subtotals {
            SubtotalPlace::Top => GridSubtotalPlace::Top,
            SubtotalPlace::Bottom => GridSubtotalPlace::Bottom,
            SubtotalPlace::Off => GridSubtotalPlace::Off,
        },
        row_totals: o.row_totals,
        col_totals: o.col_totals,
        repeat_labels: o.repeat_labels,
        blank_rows: o.blank_rows,
        empty_text: o.empty_text.clone(),
        style: o.style.name.clone(),
        row_headers: o.style.row_headers,
        col_headers: o.style.col_headers,
        row_stripes: o.style.row_stripes,
        col_stripes: o.style.col_stripes,
    }
}

fn axis_out(a: &[AxisItem]) -> Vec<i32> {
    a.iter()
        .map(|x| match x {
            AxisItem::Field(f) => i32::try_from(*f).unwrap_or(0),
            AxisItem::Values => -1,
        })
        .collect()
}

fn area(a: GridPivotArea) -> PivotArea {
    match a {
        GridPivotArea::Filters => PivotArea::Filters,
        GridPivotArea::Columns => PivotArea::Columns,
        GridPivotArea::Rows => PivotArea::Rows,
        GridPivotArea::Values => PivotArea::Values,
    }
}

fn source_text(s: &PivotSource) -> String {
    match s {
        PivotSource::Table { name } | PivotSource::Name { name } => name.clone(),
        PivotSource::Range { sheet, range } => {
            format!("{}!{}", super::quote_sheet(sheet), range.to_a1(true))
        }
    }
}

/// The PivotTable state of the Fields pane and the contextual tabs.
pub(crate) fn info(t: &PivotTable, active: CellPos) -> GridPivotInfo {
    let range = t.whole_range().unwrap_or(Range::cell(t.at.row, t.at.col));
    let fields = (0..t.fields.len())
        .map(|f| {
            let field = &t.fields[f];
            let page = t.pages.iter().find(|p| p.field == f);
            GridPivotFieldInfo {
                name: field.name.clone(),
                numeric: t.cache.fields[f].numeric_only(),
                area: t.area_of(f).map(|a| match a {
                    PivotArea::Filters => GridPivotArea::Filters,
                    PivotArea::Columns => GridPivotArea::Columns,
                    PivotArea::Rows => GridPivotArea::Rows,
                    PivotArea::Values => GridPivotArea::Values,
                }),
                values: u32::try_from(t.values.iter().filter(|v| v.field == f).count())
                    .unwrap_or(0),
                filtered: !field.hidden.is_empty()
                    || field.label_filter.is_some()
                    || field.value_filter.is_some()
                    || page.is_some_and(|p| p.item.is_some()),
                settings: settings_out(t, f),
            }
        })
        .collect();
    let (mut active_field, mut active_value, mut on_value) = (None, None, false);
    let out = &t.output;
    if let Some(r) = out.range
        && r.contains(active.row, active.col)
        && !out.empty
    {
        let body = r.r0 + out.header_rows;
        let label_end = r.c0 + out.label_cols;
        if active.col < label_end {
            // A row label (or its caption): the field of its level.
            let level = if t.options.layout == PivotLayout::Compact {
                (active.row >= body)
                    .then(|| out.rows.get((active.row - body) as usize))
                    .flatten()
                    .map_or(0, |l| usize::from(l.level))
            } else {
                (active.col - r.c0) as usize
            };
            if let Some(AxisItem::Field(f)) = t.rows.get(level) {
                active_field = u32::try_from(*f).ok();
            }
        } else if active.row < body {
            let level = active.row.saturating_sub(r.r0 + 1) as usize;
            if let Some(AxisItem::Field(f)) = t.cols.get(level) {
                active_field = u32::try_from(*f).ok();
            }
        }
        if active.row >= body
            && active.col >= label_end
            && let Some((Some(line), col)) = out.lines_at(active.row, active.col)
        {
            on_value = line.values && line.kind != PivotLineKind::Blank && !t.values.is_empty();
            let v = line
                .value
                .or_else(|| col.and_then(|c| c.value))
                .map_or(0, usize::from);
            if on_value {
                active_value = u32::try_from(v).ok();
                active_field = t.values.get(v).and_then(|v| u32::try_from(v.field).ok());
            }
        }
    }
    if let Some(p) = out.pages
        && p.contains(active.row, active.col)
    {
        let i = (active.row - p.r0) as usize;
        active_field = t.pages.get(i).and_then(|p| u32::try_from(p.field).ok());
    }
    GridPivotInfo {
        name: t.name.clone(),
        source: source_text(&t.source),
        range: GridRange {
            r0: range.r0,
            c0: range.c0,
            r1: range.r1,
            c1: range.c1,
        },
        fields,
        rows: axis_out(&t.rows),
        cols: axis_out(&t.cols),
        pages: t
            .pages
            .iter()
            .map(|p| {
                (
                    u32::try_from(p.field).unwrap_or(0),
                    p.item.as_ref().and_then(|v| {
                        t.cache.fields[p.field]
                            .find(v)
                            .map(|i| item_label(t, p.field, i as usize))
                    }),
                )
            })
            .collect(),
        values: t.values.iter().map(|v| value_out(t, v)).collect(),
        options: options_out(&t.options),
        active_field,
        active_value,
        on_value,
    }
}

/// The items of a field for its filter list.
pub(crate) fn items(t: &PivotTable, field: usize) -> Vec<GridFilterItem> {
    let Some(cf) = t.cache.fields.get(field) else {
        return Vec::new();
    };
    let hidden: Vec<_> = t.fields[field]
        .hidden
        .iter()
        .map(kiln_sheet::pivot::cache::ItemKey::of)
        .collect();
    let page = t
        .pages
        .iter()
        .find(|p| p.field == field)
        .and_then(|p| p.item.as_ref())
        .map(kiln_sheet::pivot::cache::ItemKey::of);
    let mut order: Vec<usize> = (0..cf.items.len()).collect();
    order.sort_by(|&a, &b| kiln_sheet::pivot::compare_items(&cf.items[a], &cf.items[b]));
    order
        .into_iter()
        .map(|i| {
            let key = kiln_sheet::pivot::cache::ItemKey::of(&cf.items[i]);
            GridFilterItem {
                text: item_label(t, field, i),
                checked: !hidden.contains(&key) && page.as_ref().is_none_or(|p| *p == key),
                date: None,
            }
        })
        .collect()
}

impl GridDoc {
    /// The PivotTable at a cell of the active sheet: (sheet, index in the sheet's list).
    pub(crate) fn pivot_index_at(&self, at: CellPos) -> Option<(SheetId, usize)> {
        let sid = self.sid().ok()?;
        let sheet = self.workbook.sheet(sid)?;
        let i = sheet
            .pivots()
            .iter()
            .position(|p| p.whole_range().is_some_and(|r| r.contains(at.row, at.col)))?;
        Some((sid, i))
    }

    /// The PivotTable at the active cell.
    fn active_pivot(&self) -> Result<(SheetId, usize, PivotTable)> {
        let a = self.view().sel.active;
        let (sid, i) = self
            .pivot_index_at(a)
            .ok_or_else(|| bad("Select a cell in the PivotTable report first."))?;
        let p = self
            .workbook
            .sheet(sid)
            .and_then(|s| s.pivots().get(i))
            .cloned()
            .ok_or_else(|| bad("the PivotTable no longer exists"))?;
        Ok((sid, i, p))
    }

    /// A PivotTable name not used in the workbook (`PivotTable3`).
    fn next_pivot_name(&self) -> String {
        let used: Vec<String> = self
            .workbook
            .sheets()
            .iter()
            .flat_map(|s| s.pivots().iter().map(|p| p.name.to_lowercase()))
            .collect();
        (1..)
            .map(|n| format!("PivotTable{n}"))
            .find(|n| !used.contains(&n.to_lowercase()))
            .unwrap_or_else(|| "PivotTable".into())
    }

    /// The source of a reference typed in the Create PivotTable dialog.
    fn parse_source(&self, text: &str) -> Result<PivotSource> {
        let t = text.trim().trim_start_matches('=');
        if t.is_empty() {
            return Err(bad("Reference isn't valid."));
        }
        if self.workbook.find_table(t).is_some() {
            return Ok(PivotSource::Table { name: t.to_owned() });
        }
        if let Some((sheet, range)) = split_reference(t) {
            let sheet = match sheet {
                Some(s) => self
                    .workbook
                    .sheet_by_name(&s)
                    .map(|x| x.name().to_owned())
                    .ok_or_else(|| bad("Reference isn't valid."))?,
                None => self
                    .active_sheet()
                    .map(|(_, s)| s.name().to_owned())
                    .ok_or_else(|| bad("Reference isn't valid."))?,
            };
            // One cell: its current region (Excel's dialog does the same).
            let range = if range.is_single_cell() {
                let sid = self
                    .workbook
                    .sheet_by_name(&sheet)
                    .map(kiln_sheet::Sheet::id);
                if sid == self.sid().ok() {
                    self.current_region(CellPos::new(range.r0, range.c0))
                        .unwrap_or(range)
                } else {
                    range
                }
            } else {
                range
            };
            if range.height() < 1 {
                return Err(bad("Reference isn't valid."));
            }
            return Ok(PivotSource::Range { sheet, range });
        }
        if self
            .workbook
            .names()
            .iter()
            .any(|n| n.name.eq_ignore_ascii_case(t))
        {
            return Ok(PivotSource::Name { name: t.to_owned() });
        }
        Err(bad("Reference isn't valid."))
    }

    /// Writes a PivotTable's report and stores its definition (`index` `None` adds it), as
    /// the commands of one step.
    fn report_commands(
        &mut self,
        sid: SheetId,
        index: Option<usize>,
        mut table: PivotTable,
    ) -> Result<(Vec<Command>, PivotTable)> {
        // A sheet the same step adds does not exist yet: nothing on it to overlap.
        let sheet = self.workbook.sheet(sid);
        let old_table = index.and_then(|i| sheet.and_then(|s| s.pivots().get(i)));
        // An empty report's placeholder owns no cells.
        let owned = old_table.and_then(PivotTable::cells_range);
        let report = render(&table)
            .ok_or_else(|| bad("The PivotTable report will not fit on the sheet."))?;
        table.output = report.output.clone();
        let new = table.whole_range();
        // The new report must not cover another report, a table, or cells it does not own.
        if let (Some(n), Some(sheet)) = (new, sheet) {
            for (j, p) in sheet.pivots().iter().enumerate() {
                if Some(j) != index && p.whole_range().is_some_and(|r| r.intersects(&n)) {
                    return Err(bad(
                        "A PivotTable report cannot overlap another PivotTable report.",
                    ));
                }
            }
            if sheet.tables().iter().any(|t| t.range.intersects(&n)) {
                return Err(bad("A PivotTable report cannot overlap a table."));
            }
            if !report.output.empty {
                for (row, col, d) in sheet.cells().iter_range(n) {
                    if d.has_content() && owned.is_none_or(|o| !o.contains(row, col)) {
                        return Err(bad(format!(
                            "There's already data in {}. Move or clear it, then try again.",
                            n.to_a1(true)
                        )));
                    }
                }
            }
            if let Some(p) = &sheet.meta().protection
                && !p.pivot_tables
            {
                return Err(KilnError::Unavailable {
                    command: super::PROTECTED.into(),
                    reason: super::PROTECTED_MESSAGE.into(),
                });
            }
        }
        let mut cmds = Vec::new();
        if let Some(o) = owned {
            cmds.push(Command::ClearRange {
                sheet: sid,
                ranges: vec![o],
                what: ClearWhat::All,
            });
        }
        // The cells, with the styles of their number formats and indents.
        let mut styles: rustc_hash::FxHashMap<(Option<Arc<str>>, u8, bool), u32> =
            rustc_hash::FxHashMap::default();
        let mut inputs = Vec::with_capacity(report.cells.len());
        for c in &report.cells {
            let key = (c.num_fmt.clone(), c.indent, c.label);
            let style = match styles.get(&key) {
                Some(s) => *s,
                None => {
                    let mut st: CellStyle = self.workbook.styles().get(0);
                    if let Some(f) = &c.num_fmt {
                        st.num_fmt = f.clone();
                    }
                    if c.indent > 0 {
                        st.alignment.indent = c.indent;
                        st.alignment.horizontal = HAlign::Left;
                    } else if c.label && matches!(c.value, CellValue::Number(_)) {
                        st.alignment.horizontal = HAlign::Left;
                    }
                    let i = self.log.intern_style(&mut self.workbook, &st);
                    styles.insert(key, i);
                    i
                }
            };
            inputs.push(CellInput {
                row: c.row,
                col: c.col,
                content: Content::Value(c.value.clone()),
                style: Some(style),
            });
        }
        if !inputs.is_empty() {
            cmds.push(Command::SetCells {
                sheet: sid,
                cells: inputs,
            });
        }
        let mut list = self
            .workbook
            .sheet(sid)
            .map(|s| s.pivots().to_vec())
            .unwrap_or_default();
        match index {
            Some(i) if i < list.len() => list[i] = table.clone(),
            _ => list.push(table.clone()),
        }
        cmds.push(Command::SetSheetMeta {
            sheet: sid,
            field: Box::new(MetaField::Pivots(list)),
        });
        Ok((cmds, table))
    }

    /// Writes a changed PivotTable as one step; keeps the active cell in the report.
    fn rewrite_pivot(
        &mut self,
        label: &'static str,
        sid: SheetId,
        index: usize,
        table: PivotTable,
    ) -> Result<Change> {
        let (cmds, table) = self.report_commands(sid, Some(index), table)?;
        let range = table.cells_range();
        self.exec_group(label, |doc| {
            doc.exec_all(label, cmds)?;
            if let Some(r) = range {
                doc.autofit_report(label, sid, r)?;
            }
            Ok(())
        })?;
        let a = self.view().sel.active;
        if !table
            .whole_range()
            .is_some_and(|r| r.contains(a.row, a.col))
        {
            self.select_cell(table.at.row, table.at.col, false, false);
        }
        Ok(Change::Content)
    }

    /// Excel's "Autofit column widths on update": the report's columns fit its cells (bold
    /// headers and the field buttons included).
    fn autofit_report(&mut self, label: &'static str, sid: SheetId, r: Range) -> Result<()> {
        let mdw = self.mdw();
        let mut cmds = Vec::new();
        for col in r.c0..=r.c1.min(r.c0 + 200) {
            let Some(need) = self.content_width_in(sid, Range::new(r.r0, col, r.r1, col)) else {
                continue;
            };
            let px = (need * 1.08 + 18.0).ceil().clamp(32.0, 600.0);
            cmds.push(Command::SetColProps {
                sheet: sid,
                at: col,
                count: 1,
                patch: kiln_sheet::commands::ColPatch {
                    width: kiln_sheet::styles::Patch::Set(kiln_sheet::num::F64(
                        kiln_sheet::axis::px_to_col_width(px, mdw),
                    )),
                    custom_width: Some(true),
                    ..kiln_sheet::commands::ColPatch::default()
                },
            });
        }
        if !cmds.is_empty() {
            self.exec_all(label, cmds)?;
        }
        Ok(())
    }

    /// Insert ▸ PivotTable.
    pub(crate) fn create_pivot(&mut self, source: &str, to: Option<&str>) -> Result<Change> {
        let src = self.parse_source(source)?;
        let cache = PivotCache::read(&self.workbook, &src).map_err(|e| bad(e.to_string()))?;
        let name = self.next_pivot_name();
        match to {
            None => {
                // A new sheet before the active one, the report at A3.
                let index = self.active;
                let id = self
                    .workbook
                    .sheets()
                    .iter()
                    .map(|s| s.id().0)
                    .max()
                    .unwrap_or(0)
                    + 1;
                let table = PivotTable::new(name, src, CellPos::new(2, 0), cache);
                let sid = SheetId(id);
                let (cmds, _) = self.report_commands(sid, None, table)?;
                let mut all = vec![Command::AddSheet {
                    name: None,
                    index: Some(index),
                    id: Some(id),
                }];
                all.extend(cmds);
                self.exec_all("grid.pivot.create", all)?;
                self.activate(index);
                self.select_cell(2, 0, false, false);
            }
            Some(text) => {
                let (sheet, range) =
                    split_reference(text).ok_or_else(|| bad("Reference isn't valid."))?;
                let sid = match sheet {
                    Some(n) => self
                        .workbook
                        .sheet_by_name(&n)
                        .map(kiln_sheet::Sheet::id)
                        .ok_or_else(|| bad("Reference isn't valid."))?,
                    None => self.sid()?,
                };
                let at = CellPos::new(range.r0, range.c0);
                let table = PivotTable::new(name, src, at, cache);
                let (cmds, _) = self.report_commands(sid, None, table)?;
                self.exec_all("grid.pivot.create", cmds)?;
                if let Some(i) = self.workbook.sheet_index(sid) {
                    self.activate(i);
                }
                self.select_cell(at.row, at.col, false, false);
            }
        }
        Ok(Change::Content)
    }

    /// A change of the PivotTable at the active cell.
    pub(crate) fn pivot_edit(&mut self, edit: &GridPivotEdit) -> Result<Change> {
        let (sid, index, mut t) = self.active_pivot()?;
        let field = |f: u32, t: &PivotTable| -> Result<usize> {
            let f = f as usize;
            if f < t.fields.len() {
                Ok(f)
            } else {
                Err(bad("There is no such field."))
            }
        };
        match edit {
            GridPivotEdit::Place {
                field: f,
                area: a,
                position,
            } => {
                let f = f.map(|f| field(f, &t)).transpose()?;
                t.place(f, area(*a), position.map(|p| p as usize), None);
                // A page field placed above the table: the table moves down to make room.
                let need = if t.pages.is_empty() {
                    0
                } else {
                    u32::try_from(t.pages.len()).unwrap_or(0) + 1
                };
                if t.at.row < need {
                    t.at.row = need;
                }
            }
            GridPivotEdit::Remove { field: f } => {
                let f = field(*f, &t)?;
                t.remove_field(f);
                // Removing a field forgets its filter, as in Excel.
                t.fields[f].hidden.clear();
                t.fields[f].label_filter = None;
                t.fields[f].value_filter = None;
            }
            GridPivotEdit::RemoveValue { index: i } => t.remove_value(*i as usize),
            GridPivotEdit::MoveValue { index: i, position } => {
                let i = *i as usize;
                if i < t.values.len() {
                    let v = t.values.remove(i);
                    let at = (*position as usize).min(t.values.len());
                    t.values.insert(at, v);
                }
            }
            GridPivotEdit::Value { index: i, value } => {
                let i = *i as usize;
                if i >= t.values.len() {
                    return Err(bad("There is no such value field."));
                }
                let name = value.name.trim().to_owned();
                if name.is_empty() {
                    return Err(bad("The PivotTable field name is not valid."));
                }
                if t.values
                    .iter()
                    .enumerate()
                    .any(|(j, v)| j != i && v.name.eq_ignore_ascii_case(&name))
                    || t.fields.iter().any(|f| f.name.eq_ignore_ascii_case(&name))
                {
                    return Err(bad("PivotTable field name already exists."));
                }
                let show = show_as(value.show_as);
                let base_field = match value.base_field {
                    Some(b) => Some(field(b, &t)?),
                    None if show.needs_base_field() => {
                        return Err(bad("Choose a base field for this calculation."));
                    }
                    None => None,
                };
                let base_item = match value.base_item.as_deref() {
                    None | Some("(previous)") => BaseItem::Previous,
                    Some("(next)") => BaseItem::Next,
                    Some(label) => base_field
                        .and_then(|b| item_of(&t, b, label))
                        .map_or(BaseItem::Previous, BaseItem::Item),
                };
                let old = &t.values[i];
                let f = old.field;
                // A function change renames a default name (`Sum of X` → `Average of X`).
                let new_fn = function(value.function);
                let name = if name.eq_ignore_ascii_case(&old.name)
                    && old.name == t.value_name_for(f, old.function)
                    && new_fn != old.function
                {
                    t.value_name_for(f, new_fn)
                } else {
                    name
                };
                t.values[i] = PivotValue {
                    field: f,
                    name,
                    function: new_fn,
                    show_as: show,
                    base_field,
                    base_item,
                    num_fmt: value
                        .num_fmt
                        .clone()
                        .filter(|c| !c.trim().is_empty() && !c.eq_ignore_ascii_case("General")),
                };
            }
            GridPivotEdit::Field { field: f, settings } => {
                let f = field(*f, &t)?;
                apply_settings(&mut t, f, settings)?;
            }
            GridPivotEdit::Page { field: f, item } => {
                let f = field(*f, &t)?;
                let value = match item.as_deref() {
                    None | Some("(All)") => None,
                    Some(label) => {
                        Some(item_of(&t, f, label).ok_or_else(|| bad("There is no such item."))?)
                    }
                };
                match t.pages.iter_mut().find(|p| p.field == f) {
                    Some(p) => p.item = value,
                    None => return Err(bad("The field is not in the Filters area.")),
                }
            }
            GridPivotEdit::Options { options: o } => apply_options(&mut t, o),
            GridPivotEdit::Rename { name } => {
                let name = name.trim();
                if name.is_empty() || name.chars().count() > 255 {
                    return Err(bad("The PivotTable name is not valid."));
                }
                let taken = self.workbook.sheets().iter().any(|s| {
                    s.pivots().iter().enumerate().any(|(j, p)| {
                        !(s.id() == sid && j == index) && p.name.eq_ignore_ascii_case(name)
                    })
                });
                if taken {
                    return Err(bad("A PivotTable report with that name already exists."));
                }
                t.name = name.to_owned();
            }
            GridPivotEdit::Source { source } => {
                let src = self.parse_source(source)?;
                let cache =
                    PivotCache::read(&self.workbook, &src).map_err(|e| bad(e.to_string()))?;
                t = remap(t, cache);
                t.source = src;
            }
            GridPivotEdit::Move { to } => return self.move_pivot(sid, index, t, to),
            GridPivotEdit::ClearFilters => {
                for f in &mut t.fields {
                    f.hidden.clear();
                    f.label_filter = None;
                    f.value_filter = None;
                }
                for p in &mut t.pages {
                    p.item = None;
                }
            }
            GridPivotEdit::ClearAll => {
                t.rows.clear();
                t.cols.clear();
                t.pages.clear();
                t.values.clear();
                for f in &mut t.fields {
                    *f = PivotField::named(&f.name);
                }
            }
            GridPivotEdit::Delete => {
                let mut cmds = Vec::new();
                if let Some(r) = t.cells_range() {
                    cmds.push(Command::ClearRange {
                        sheet: sid,
                        ranges: vec![r],
                        what: ClearWhat::All,
                    });
                }
                let mut list = self
                    .workbook
                    .sheet(sid)
                    .map(|s| s.pivots().to_vec())
                    .unwrap_or_default();
                list.remove(index);
                cmds.push(Command::SetSheetMeta {
                    sheet: sid,
                    field: Box::new(MetaField::Pivots(list)),
                });
                self.exec_all("grid.pivot.edit", cmds)?;
                return Ok(Change::Content);
            }
        }
        self.rewrite_pivot("grid.pivot.edit", sid, index, t)
    }

    fn move_pivot(
        &mut self,
        sid: SheetId,
        index: usize,
        t: PivotTable,
        to: &str,
    ) -> Result<Change> {
        let (sheet, range) = split_reference(to).ok_or_else(|| bad("Reference isn't valid."))?;
        let dest = match sheet {
            Some(n) => self
                .workbook
                .sheet_by_name(&n)
                .map(kiln_sheet::Sheet::id)
                .ok_or_else(|| bad("Reference isn't valid."))?,
            None => sid,
        };
        let mut moved = t.clone();
        let pages = u32::try_from(t.pages.len()).unwrap_or(0);
        let need = if pages > 0 { pages + 1 } else { 0 };
        moved.at = CellPos::new(range.r0.max(need), range.c0);
        if dest == sid {
            return self.rewrite_pivot("grid.pivot.edit", sid, index, moved);
        }
        // Another sheet: the report leaves this one and is written there.
        let mut cmds = Vec::new();
        if let Some(r) = t.cells_range() {
            cmds.push(Command::ClearRange {
                sheet: sid,
                ranges: vec![r],
                what: ClearWhat::All,
            });
        }
        let mut list = self
            .workbook
            .sheet(sid)
            .map(|s| s.pivots().to_vec())
            .unwrap_or_default();
        list.remove(index);
        cmds.push(Command::SetSheetMeta {
            sheet: sid,
            field: Box::new(MetaField::Pivots(list)),
        });
        let (more, moved) = self.report_commands(dest, None, moved)?;
        cmds.extend(more);
        self.exec_all("grid.pivot.edit", cmds)?;
        if let Some(i) = self.workbook.sheet_index(dest) {
            self.activate(i);
        }
        self.select_cell(moved.at.row, moved.at.col, false, false);
        Ok(Change::Content)
    }

    /// Refresh (the PivotTable at the active cell) or Refresh All (every PivotTable).
    pub(crate) fn refresh_pivots(&mut self, all: bool) -> Result<Change> {
        let targets: Vec<(SheetId, usize)> = if all {
            self.workbook
                .sheets()
                .iter()
                .flat_map(|s| (0..s.pivots().len()).map(move |i| (s.id(), i)))
                .collect()
        } else {
            let (sid, i, _) = self.active_pivot()?;
            vec![(sid, i)]
        };
        if targets.is_empty() {
            return Ok(Change::None);
        }
        let mut cmds = Vec::new();
        let mut ranges = Vec::new();
        for (sid, i) in targets {
            let Some(t) = self
                .workbook
                .sheet(sid)
                .and_then(|s| s.pivots().get(i))
                .cloned()
            else {
                continue;
            };
            let cache =
                PivotCache::read(&self.workbook, &t.source).map_err(|e| bad(e.to_string()))?;
            let t = remap(t, cache);
            // Each report is written over the state the previous commands leave, so the
            // commands of one sheet are applied in order within the step.
            let (more, t) = self.report_commands_after(sid, i, t, &cmds)?;
            cmds.extend(more);
            if let Some(r) = t.cells_range() {
                ranges.push((sid, r));
            }
        }
        let label = "grid.pivot.refresh";
        self.exec_group(label, |doc| {
            doc.exec_all(label, cmds)?;
            for (sid, r) in ranges {
                doc.autofit_report(label, sid, r)?;
            }
            Ok(())
        })?;
        Ok(Change::Content)
    }

    /// [`Self::report_commands`] for a refresh among others: earlier refreshes of the same
    /// sheet have changed its PivotTable list, so the list is rebuilt from theirs.
    fn report_commands_after(
        &mut self,
        sid: SheetId,
        index: usize,
        table: PivotTable,
        earlier: &[Command],
    ) -> Result<(Vec<Command>, PivotTable)> {
        let (mut cmds, table) = self.report_commands(sid, Some(index), table)?;
        let before = earlier.iter().rev().find_map(|c| match c {
            Command::SetSheetMeta { sheet, field } if *sheet == sid => match field.as_ref() {
                MetaField::Pivots(list) => Some(list.clone()),
                _ => None,
            },
            _ => None,
        });
        if let Some(mut list) = before {
            if index < list.len() {
                list[index] = table.clone();
            }
            if let Some(Command::SetSheetMeta { field, .. }) = cmds.last_mut() {
                **field = MetaField::Pivots(list);
            }
        }
        Ok((cmds, table))
    }

    /// Show Details: the source rows of a value on a new sheet, as a table.
    pub(crate) fn pivot_details(&mut self, row: u32, col: u32) -> Result<Change> {
        let at = CellPos::new(row, col);
        let (sid, i) = self
            .pivot_index_at(at)
            .ok_or_else(|| bad("Select a value of the PivotTable report first."))?;
        let t = self
            .workbook
            .sheet(sid)
            .and_then(|s| s.pivots().get(i))
            .cloned()
            .ok_or_else(|| bad("the PivotTable no longer exists"))?;
        let records = kiln_sheet::pivot::render::drill(&t, row, col)
            .ok_or_else(|| bad("Cannot change this part of a PivotTable report."))?;
        if records.len() + 1 >= kiln_sheet::MAX_ROWS as usize {
            return Err(bad("There are too many rows to show."));
        }
        let index = self.active;
        let id = self
            .workbook
            .sheets()
            .iter()
            .map(|s| s.id().0)
            .max()
            .unwrap_or(0)
            + 1;
        let new = SheetId(id);
        let cache = &t.cache;
        let width = u32::try_from(cache.fields.len()).unwrap_or(0);
        let mut values: Vec<Vec<Option<CellValue>>> = Vec::with_capacity(records.len() + 1);
        values.push(
            t.fields
                .iter()
                .map(|f| Some(CellValue::text(f.name.clone())))
                .collect(),
        );
        for &r in &records {
            values.push(
                cache
                    .fields
                    .iter()
                    .map(|f| Some(f.value(r).clone()))
                    .collect(),
            );
        }
        let height = u32::try_from(values.len()).unwrap_or(1);
        let mut cmds = vec![
            Command::AddSheet {
                name: None,
                index: Some(index),
                id: Some(id),
            },
            Command::SetRange {
                sheet: new,
                row: 0,
                col: 0,
                values,
            },
        ];
        // Dates and other formats of the source columns.
        for (c, f) in cache.fields.iter().enumerate() {
            if let Some(code) = &f.num_fmt
                && height > 1
            {
                cmds.push(Command::SetStyle {
                    sheet: new,
                    ranges: vec![Range::new(1, c as u32, height - 1, c as u32)],
                    patch: Box::new(kiln_sheet::styles::StylePatch {
                        num_fmt: Some(code.clone()),
                        ..kiln_sheet::styles::StylePatch::default()
                    }),
                });
            }
        }
        let name = self.workbook.next_table_name();
        cmds.push(Command::AddTable {
            sheet: new,
            table: Box::new(kiln_sheet::meta::Table {
                id: 0,
                name,
                display_name: None,
                range: Range::new(0, 0, height.max(2) - 1, width.max(1) - 1),
                header_row: true,
                totals_row: false,
                columns: t
                    .fields
                    .iter()
                    .map(|f| kiln_sheet::meta::TableColumn::named(&f.name))
                    .collect(),
                style: Some(kiln_sheet::meta::TableStyleInfo {
                    name: self.workbook.styles().default_table_style().to_owned(),
                    ..kiln_sheet::meta::TableStyleInfo::default()
                }),
                auto_filter: Some(kiln_sheet::meta::TableFilter {
                    columns: Vec::new(),
                    sort: None,
                }),
                comment: None,
            }),
        });
        self.exec_all("grid.pivot.show_details", cmds)?;
        self.activate(index);
        self.select_cell(0, 0, false, false);
        Ok(Change::Content)
    }
}

/// Applies the Design tab and PivotTable Options.
fn apply_options(t: &mut PivotTable, o: &GridPivotOptions) {
    t.options.layout = match o.layout {
        GridPivotLayout::Compact => PivotLayout::Compact,
        GridPivotLayout::Outline => PivotLayout::Outline,
        GridPivotLayout::Tabular => PivotLayout::Tabular,
    };
    t.options.subtotals = match o.subtotals {
        GridSubtotalPlace::Top => SubtotalPlace::Top,
        GridSubtotalPlace::Bottom => SubtotalPlace::Bottom,
        GridSubtotalPlace::Off => SubtotalPlace::Off,
    };
    t.options.row_totals = o.row_totals;
    t.options.col_totals = o.col_totals;
    t.options.repeat_labels = o.repeat_labels;
    t.options.blank_rows = o.blank_rows;
    t.options.empty_text = o.empty_text.clone().filter(|s| !s.is_empty());
    let style = o.style.trim();
    t.options.style.name =
        if style.is_empty() || kiln_sheet::pivot::style::builtin_family(style).is_some() {
            style.to_owned()
        } else {
            t.options.style.name.clone()
        };
    t.options.style.row_headers = o.row_headers;
    t.options.style.col_headers = o.col_headers;
    t.options.style.row_stripes = o.row_stripes;
    t.options.style.col_stripes = o.col_stripes;
}

/// A PivotTable over a new cache (Refresh, Change Data Source): fields keep their settings by
/// name; fields that are gone leave the report. A field in manual (data source) order keeps the
/// order of its items, and new items go at the end, as in Excel.
pub(crate) fn remap(mut t: PivotTable, mut cache: PivotCache) -> PivotTable {
    let old_fields = std::mem::take(&mut t.fields);
    let map: Vec<Option<usize>> = old_fields
        .iter()
        .map(|f| {
            cache
                .fields
                .iter()
                .position(|c| c.name.eq_ignore_ascii_case(&f.name))
        })
        .collect();
    for (i, (f, new)) in old_fields.iter().zip(&map).enumerate() {
        if let (PivotSort::Source, Some(n), Some(prior)) = (f.sort, new, t.cache.fields.get(i)) {
            cache.fields[*n].keep_order(&prior.items);
        }
    }
    let mut fields: Vec<PivotField> = cache
        .fields
        .iter()
        .map(|c| PivotField::named(&c.name))
        .collect();
    for (old, new) in old_fields.into_iter().zip(&map) {
        if let Some(n) = new {
            let name = fields[*n].name.clone();
            fields[*n] = PivotField { name, ..old };
        }
    }
    let m = |f: usize| map.get(f).copied().flatten();
    t.rows = t
        .rows
        .iter()
        .filter_map(|a| match a {
            AxisItem::Field(f) => m(*f).map(AxisItem::Field),
            AxisItem::Values => Some(AxisItem::Values),
        })
        .collect();
    t.cols = t
        .cols
        .iter()
        .filter_map(|a| match a {
            AxisItem::Field(f) => m(*f).map(AxisItem::Field),
            AxisItem::Values => Some(AxisItem::Values),
        })
        .collect();
    t.pages = t
        .pages
        .iter()
        .filter_map(|p| {
            Some(PivotPage {
                field: m(p.field)?,
                item: p.item.clone(),
            })
        })
        .collect();
    t.values = t
        .values
        .iter()
        .filter_map(|v| {
            Some(PivotValue {
                field: m(v.field)?,
                base_field: v.base_field.and_then(m),
                ..v.clone()
            })
        })
        .collect();
    t.fields = fields;
    t.cache = Arc::new(cache);
    t.settle_values_axis();
    t
}
