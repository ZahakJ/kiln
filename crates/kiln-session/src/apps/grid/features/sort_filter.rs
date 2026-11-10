// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sort and AutoFilter (Data ▸ Sort & Filter, Home ▸ Editing ▸ Sort & Filter).
//!
//! Sort A→Z and Z→A sort the table, filter or current region at the active cell by its column,
//! detecting a header row as Excel does; the Sort dialog sorts by several levels, on values,
//! cell or font colour or icon, with custom lists, case sensitivity and left to right. A sort
//! inside an AutoFilter or table is remembered (the dropdown shows it, Reapply repeats it).
//!
//! AutoFilter dropdowns list a column's distinct displayed values with their checked state; the
//! filter is one of the sheet model's column filters (values, custom criteria with wildcards,
//! top 10, dynamic, colour) and applying it hides the failing rows as `filtered`, so `SUBTOTAL`
//! and `AGGREGATE` skip them.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::GridCommand;
use kiln_protocol::commands::grid_features::{
    GridColumnFilter, GridDynamicFilter, GridFilterItem, GridFilterMenu, GridFilterOp, GridSortOn,
    GridSortSpec, GridValueKind,
};
use kiln_protocol::commands::grid_surface::GridSelectionKey;
use kiln_scene::{Bounds, DrawOp, FillRule, Paint, PathData, Stroke};
use kiln_sheet::commands::{MetaField, SortKey, TablePatch};
use kiln_sheet::meta::{
    AutoFilter, ColumnFilter, CustomCriterion, DynamicFilter, FilterColumn, FilterOperator, SortBy,
    SortCondition, SortState, TableFilter,
};
use kiln_sheet::styles::{Dxf, Fill, FontPatch, Patch};
use kiln_sheet::{CellPos, CellValue, Command, Range, SheetId};
use kiln_sheet_layout::SheetGeometry;
use rustc_hash::FxHashSet;

use super::super::GridDoc;
use super::super::format::{hex, parse_color};
use crate::model::Change;

/// A filter header row: row, first and last column, filtered column offsets, the sorted
/// column and its direction.
type FilterHeader = (u32, u32, u32, Vec<u32>, Option<(u32, bool)>);

/// The filter a header button belongs to.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FilterOwner {
    /// The sheet's AutoFilter.
    Sheet(AutoFilter),
    /// A table's filter: its name, range (without the total row) and filter.
    Table(String, Range, TableFilter),
}

impl FilterOwner {
    /// The range with the header row.
    fn range(&self) -> Range {
        match self {
            Self::Sheet(f) => f.range,
            Self::Table(_, r, _) => *r,
        }
    }

    fn columns(&self) -> &[FilterColumn] {
        match self {
            Self::Sheet(f) => &f.columns,
            Self::Table(_, _, f) => &f.columns,
        }
    }

    fn sort(&self) -> Option<&SortState> {
        match self {
            Self::Sheet(f) => f.sort.as_ref(),
            Self::Table(_, _, f) => f.sort.as_ref(),
        }
    }

    /// The command that stores a changed filter.
    fn store(
        &self,
        sheet: SheetId,
        columns: Vec<FilterColumn>,
        sort: Option<SortState>,
    ) -> Command {
        match self {
            Self::Sheet(f) => Command::SetSheetMeta {
                sheet,
                field: Box::new(MetaField::AutoFilter(Some(AutoFilter {
                    range: f.range,
                    columns,
                    sort,
                }))),
            },
            Self::Table(name, _, _) => Command::UpdateTable {
                name: name.clone(),
                patch: Box::new(TablePatch {
                    auto_filter: Patch::Set(TableFilter { columns, sort }),
                    ..TablePatch::default()
                }),
            },
        }
    }
}

fn data_rows(r: Range) -> Option<Range> {
    (r.r1 > r.r0).then(|| Range::new(r.r0 + 1, r.c0, r.r1, r.c1))
}

fn op_of(op: GridFilterOp, value: &str) -> CustomCriterion {
    use GridFilterOp as O;
    let (op, value) = match op {
        O::Equal => (FilterOperator::Equal, value.to_owned()),
        O::NotEqual => (FilterOperator::NotEqual, value.to_owned()),
        O::GreaterThan => (FilterOperator::GreaterThan, value.to_owned()),
        O::GreaterThanOrEqual => (FilterOperator::GreaterThanOrEqual, value.to_owned()),
        O::LessThan => (FilterOperator::LessThan, value.to_owned()),
        O::LessThanOrEqual => (FilterOperator::LessThanOrEqual, value.to_owned()),
        O::BeginsWith => (FilterOperator::Equal, format!("{value}*")),
        O::EndsWith => (FilterOperator::Equal, format!("*{value}")),
        O::Contains => (FilterOperator::Equal, format!("*{value}*")),
        O::NotContains => (FilterOperator::NotEqual, format!("*{value}*")),
    };
    CustomCriterion { op, value }
}

fn grid_op(c: &CustomCriterion) -> (GridFilterOp, String) {
    use GridFilterOp as O;
    let v = &c.value;
    let starred = v.len() > 1 && v.starts_with('*') && v.ends_with('*');
    match c.op {
        FilterOperator::Equal if starred => (O::Contains, v[1..v.len() - 1].to_owned()),
        FilterOperator::NotEqual if starred => (O::NotContains, v[1..v.len() - 1].to_owned()),
        FilterOperator::Equal if v.len() > 1 && v.ends_with('*') && !v.starts_with('*') => {
            (O::BeginsWith, v[..v.len() - 1].to_owned())
        }
        FilterOperator::Equal if v.len() > 1 && v.starts_with('*') && !v.ends_with('*') => {
            (O::EndsWith, v[1..].to_owned())
        }
        FilterOperator::Equal => (O::Equal, v.clone()),
        FilterOperator::NotEqual => (O::NotEqual, v.clone()),
        FilterOperator::GreaterThan => (O::GreaterThan, v.clone()),
        FilterOperator::GreaterThanOrEqual => (O::GreaterThanOrEqual, v.clone()),
        FilterOperator::LessThan => (O::LessThan, v.clone()),
        FilterOperator::LessThanOrEqual => (O::LessThanOrEqual, v.clone()),
    }
}

fn dynamic_of(d: GridDynamicFilter) -> DynamicFilter {
    use GridDynamicFilter as G;
    match d {
        G::AboveAverage => DynamicFilter::AboveAverage,
        G::BelowAverage => DynamicFilter::BelowAverage,
        G::Today => DynamicFilter::Today,
        G::Yesterday => DynamicFilter::Yesterday,
        G::Tomorrow => DynamicFilter::Tomorrow,
        G::ThisWeek => DynamicFilter::ThisWeek,
        G::LastWeek => DynamicFilter::LastWeek,
        G::NextWeek => DynamicFilter::NextWeek,
        G::ThisMonth => DynamicFilter::ThisMonth,
        G::LastMonth => DynamicFilter::LastMonth,
        G::NextMonth => DynamicFilter::NextMonth,
        G::ThisQuarter => DynamicFilter::ThisQuarter,
        G::LastQuarter => DynamicFilter::LastQuarter,
        G::NextQuarter => DynamicFilter::NextQuarter,
        G::ThisYear => DynamicFilter::ThisYear,
        G::LastYear => DynamicFilter::LastYear,
        G::NextYear => DynamicFilter::NextYear,
        G::YearToDate => DynamicFilter::YearToDate,
    }
}

fn grid_dynamic(d: DynamicFilter) -> Option<GridDynamicFilter> {
    use GridDynamicFilter as G;
    Some(match d {
        DynamicFilter::AboveAverage => G::AboveAverage,
        DynamicFilter::BelowAverage => G::BelowAverage,
        DynamicFilter::Today => G::Today,
        DynamicFilter::Yesterday => G::Yesterday,
        DynamicFilter::Tomorrow => G::Tomorrow,
        DynamicFilter::ThisWeek => G::ThisWeek,
        DynamicFilter::LastWeek => G::LastWeek,
        DynamicFilter::NextWeek => G::NextWeek,
        DynamicFilter::ThisMonth => G::ThisMonth,
        DynamicFilter::LastMonth => G::LastMonth,
        DynamicFilter::NextMonth => G::NextMonth,
        DynamicFilter::ThisQuarter => G::ThisQuarter,
        DynamicFilter::LastQuarter => G::LastQuarter,
        DynamicFilter::NextQuarter => G::NextQuarter,
        DynamicFilter::ThisYear => G::ThisYear,
        DynamicFilter::LastYear => G::LastYear,
        DynamicFilter::NextYear => G::NextYear,
        DynamicFilter::YearToDate => G::YearToDate,
        DynamicFilter::Quarter(_) | DynamicFilter::Month(_) => return None,
    })
}

fn sort_by(on: GridSortOn) -> SortBy {
    match on {
        GridSortOn::Values => SortBy::Value,
        GridSortOn::CellColor => SortBy::CellColor,
        GridSortOn::FontColor => SortBy::FontColor,
        GridSortOn::Icon => SortBy::Icon,
    }
}

impl GridDoc {
    /// The AutoFilter or table filter whose header row is `row` and covers `col`.
    pub(crate) fn filter_owner(&self, sid: SheetId, row: u32, col: u32) -> Option<FilterOwner> {
        let sheet = self.workbook.sheet(sid)?;
        if let Some(f) = &sheet.meta().auto_filter
            && f.range.r0 == row
            && f.range.c0 <= col
            && col <= f.range.c1
        {
            return Some(FilterOwner::Sheet(f.clone()));
        }
        sheet.tables().iter().find_map(|t| {
            let f = t.auto_filter.as_ref()?;
            let r = t.range;
            (t.header_row && r.r0 == row && r.c0 <= col && col <= r.c1).then(|| {
                let r1 = if t.totals_row {
                    r.r1.saturating_sub(1)
                } else {
                    r.r1
                };
                FilterOwner::Table(
                    t.name.clone(),
                    Range::new(r.r0, r.c0, r1.max(r.r0), r.c1),
                    f.clone(),
                )
            })
        })
    }

    /// The filter containing a cell (its header or data rows).
    fn filter_at(&self, sid: SheetId, at: CellPos) -> Option<FilterOwner> {
        let sheet = self.workbook.sheet(sid)?;
        if let Some(t) = sheet.table_at(at.row, at.col)
            && t.header_row
        {
            return self.filter_owner(sid, t.range.r0, at.col);
        }
        let f = sheet.meta().auto_filter.as_ref()?;
        f.range
            .contains(at.row, at.col)
            .then(|| FilterOwner::Sheet(f.clone()))
    }

    /// Whether the first row of `r` looks like headers (Excel's guess): it is all text while
    /// the key column's next value is not, or its font is bolder than the next row's.
    fn guess_header(&self, sid: SheetId, r: Range, key: u32) -> bool {
        if r.r1 <= r.r0 {
            return false;
        }
        let mut any = false;
        for c in r.c0..=r.c1.min(r.c0 + 256) {
            match self.workbook.value(sid, r.r0, c) {
                CellValue::Text(_) | CellValue::RichText(_) => any = true,
                CellValue::Blank => {}
                _ => return false,
            }
        }
        if !any {
            return false;
        }
        // A column whose next row holds a number, date or logical under a text header.
        let typed_below = (r.c0..=r.c1.min(r.c0 + 256)).any(|c| {
            !matches!(
                self.workbook.value(sid, r.r0 + 1, c),
                CellValue::Text(_) | CellValue::RichText(_) | CellValue::Blank
            ) && !self.workbook.value(sid, r.r0, c).is_blank()
        });
        if typed_below {
            return true;
        }
        let Some(sheet) = self.workbook.sheet(sid) else {
            return false;
        };
        let bold = |row: u32| {
            self.workbook
                .styles()
                .get(sheet.style_at(row, key))
                .font
                .bold
        };
        bold(r.r0) && !bold(r.r0 + 1)
    }

    #[allow(clippy::too_many_arguments)]
    /// Sorts `range` (header row excluded when `header`) and remembers the sort in the filter it
    /// belongs to.
    fn sort_range(
        &mut self,
        label: &'static str,
        sid: SheetId,
        range: Range,
        keys: Vec<SortKey>,
        header: bool,
        case_sensitive: bool,
        by_columns: bool,
    ) -> Result<()> {
        let body = if header {
            if by_columns {
                (range.c1 > range.c0)
                    .then(|| Range::new(range.r0, range.c0 + 1, range.r1, range.c1))
            } else {
                data_rows(range)
            }
        } else {
            Some(range)
        };
        let Some(body) = body else {
            return Ok(());
        };
        let mut cmds = vec![Command::Sort {
            sheet: sid,
            range: body,
            keys: keys.clone(),
            header: false,
            case_sensitive,
            by_columns,
            locale: None,
        }];
        if !by_columns
            && let Some(owner) = self.filter_at(sid, CellPos::new(range.r0, range.c0))
            && owner.range().r0 + 1 == body.r0
        {
            let state = SortState {
                range: body,
                case_sensitive,
                column_sort: false,
                conditions: keys
                    .iter()
                    .map(|k| SortCondition {
                        range: Range::new(body.r0, k.index, body.r1, k.index),
                        descending: k.descending,
                        sort_by: k.sort_by,
                        custom_list: k.custom_list.clone(),
                        dxf: None,
                    })
                    .collect(),
            };
            cmds.push(owner.store(sid, owner.columns().to_vec(), Some(state)));
        }
        self.exec_all(label, cmds)?;
        Ok(())
    }

    /// The range Sort and Remove Duplicates work on, with its first row's texts.
    pub(crate) fn data_range_info(
        &self,
    ) -> Result<kiln_protocol::commands::grid_features::GridDataRange> {
        let sid = self.sid()?;
        let r = self.data_range()?;
        let a = self.view().sel.active;
        let header = match self.filter_at(sid, a) {
            Some(o) => o.range().r0 == r.r0,
            None => match self
                .workbook
                .sheet(sid)
                .and_then(|s| s.table_at(a.row, a.col))
            {
                Some(t) if t.range.r0 == r.r0 => t.header_row,
                _ => self.guess_header(sid, r, a.col.clamp(r.c0, r.c1)),
            },
        };
        Ok(kiln_protocol::commands::grid_features::GridDataRange {
            range: kiln_protocol::commands::GridRange {
                r0: r.r0,
                c0: r.c0,
                r1: r.r1,
                c1: r.c1,
            },
            headers: (r.c0..=r.c1.min(r.c0 + 1_000))
                .map(|c| self.workbook.display_text(sid, r.r0, c))
                .collect(),
            header,
        })
    }

    pub(crate) fn sort_filter(&mut self, cmd: &GridCommand) -> Result<Change> {
        use GridCommand as G;
        let sid = self.sid()?;
        match cmd {
            G::SortAscending | G::SortDescending => {
                let label = if matches!(cmd, G::SortAscending) {
                    "grid.data.sort_ascending"
                } else {
                    "grid.data.sort_descending"
                };
                let range = self.data_range()?;
                let a = self.view().sel.active;
                let key = a.col.clamp(range.c0, range.c1);
                let header = match self.filter_at(sid, a) {
                    Some(o) if o.range() == range || o.range().r0 == range.r0 => true,
                    _ => match self
                        .workbook
                        .sheet(sid)
                        .and_then(|s| s.table_at(a.row, a.col))
                    {
                        Some(t) if t.range == range => t.header_row,
                        _ => self.guess_header(sid, range, key),
                    },
                };
                // A table's total row stays at the bottom.
                let range = match self
                    .workbook
                    .sheet(sid)
                    .and_then(|s| s.table_at(a.row, a.col))
                {
                    Some(t) if t.range == range && t.totals_row && range.r1 > range.r0 => {
                        Range::new(range.r0, range.c0, range.r1 - 1, range.c1)
                    }
                    _ => range,
                };
                let k = if matches!(cmd, G::SortAscending) {
                    SortKey::asc(key)
                } else {
                    SortKey::desc(key)
                };
                self.sort_range(label, sid, range, vec![k], header, false, false)?;
            }
            G::ApplySort { spec } => self.apply_sort(sid, spec)?,
            G::FilterBySelection { by } => self.filter_by_selection(sid, *by)?,
            G::SortBySelectionColor { font } => {
                let range = self.data_range()?;
                let a = self.view().sel.active;
                let style = self.workbook.styles().get(
                    self.workbook
                        .sheet(sid)
                        .map_or(0, |s| s.style_at(a.row, a.col)),
                );
                let theme = self.workbook.theme();
                let color = if *font {
                    hex(style.font.color, theme, 0)
                } else {
                    hex(style.fill.main_color(), theme, 0xFF_FFFF)
                };
                let key = SortKey {
                    index: a.col.clamp(range.c0, range.c1),
                    descending: false,
                    custom_list: None,
                    sort_by: if *font {
                        SortBy::FontColor
                    } else {
                        SortBy::CellColor
                    },
                    color: color.as_deref().and_then(parse_color),
                };
                let header = match self.filter_at(sid, a) {
                    Some(o) => o.range().r0 == range.r0,
                    None => self.guess_header(sid, range, key.index),
                };
                self.sort_range(
                    "grid.data.sort_by_selection_color",
                    sid,
                    range,
                    vec![key],
                    header,
                    false,
                    false,
                )?;
            }
            G::ToggleFilter => self.toggle_filter(sid)?,
            G::FilterColumn { row, col, filter } => {
                let owner = self.filter_owner(sid, *row, *col).ok_or_else(|| {
                    KilnError::InvalidInput("there is no filter button at that cell".into())
                })?;
                let filter = filter.as_ref().map(|f| self.column_filter(f)).transpose()?;
                let col_id = col - owner.range().c0;
                let mut columns: Vec<FilterColumn> = owner
                    .columns()
                    .iter()
                    .filter(|c| c.col_id != col_id)
                    .cloned()
                    .collect();
                if let Some(f) = filter {
                    columns.push(FilterColumn {
                        col_id,
                        hidden_button: false,
                        show_button: true,
                        filter: Some(f),
                    });
                    columns.sort_by_key(|c| c.col_id);
                }
                let store = owner.store(sid, columns, owner.sort().cloned());
                self.exec_all(
                    "grid.data.filter_column",
                    vec![store, Command::ApplyFilter { sheet: sid }],
                )?;
            }
            G::SortFilterColumn {
                row,
                col,
                descending,
                on,
                color,
            } => {
                let owner = self.filter_owner(sid, *row, *col).ok_or_else(|| {
                    KilnError::InvalidInput("there is no filter button at that cell".into())
                })?;
                let key = SortKey {
                    index: *col,
                    descending: *descending,
                    custom_list: None,
                    sort_by: sort_by(*on),
                    color: color.as_deref().and_then(parse_color),
                };
                self.sort_range(
                    "grid.data.sort_filter_column",
                    sid,
                    owner.range(),
                    vec![key],
                    true,
                    false,
                    false,
                )?;
            }
            G::ClearFilter => {
                let a = self.view().sel.active;
                let owner = self
                    .filter_at(sid, a)
                    .or_else(|| {
                        let f = self.workbook.sheet(sid)?.meta().auto_filter.clone()?;
                        Some(FilterOwner::Sheet(f))
                    })
                    .ok_or_else(|| KilnError::InvalidInput("the sheet has no filter".into()))?;
                let cleared: Vec<FilterColumn> = owner
                    .columns()
                    .iter()
                    .map(|c| FilterColumn {
                        filter: None,
                        ..c.clone()
                    })
                    .filter(|c| c.hidden_button || !c.show_button)
                    .collect();
                let store = owner.store(sid, cleared, owner.sort().cloned());
                self.exec_all(
                    "grid.data.clear_filter",
                    vec![store, Command::ApplyFilter { sheet: sid }],
                )?;
            }
            G::ReapplyFilter => {
                let mut cmds = vec![Command::ApplyFilter { sheet: sid }];
                // A remembered sort is repeated too.
                let owners: Vec<FilterOwner> = self
                    .workbook
                    .sheet(sid)
                    .map(|s| {
                        let mut v: Vec<FilterOwner> = s
                            .meta()
                            .auto_filter
                            .clone()
                            .map(FilterOwner::Sheet)
                            .into_iter()
                            .collect();
                        for t in s.tables() {
                            if let Some(f) = &t.auto_filter {
                                v.push(FilterOwner::Table(t.name.clone(), t.range, f.clone()));
                            }
                        }
                        v
                    })
                    .unwrap_or_default();
                for o in owners {
                    if let Some(s) = o.sort() {
                        cmds.insert(
                            0,
                            Command::Sort {
                                sheet: sid,
                                range: s.range,
                                keys: s
                                    .conditions
                                    .iter()
                                    .map(|c| SortKey {
                                        index: c.range.c0,
                                        descending: c.descending,
                                        custom_list: c.custom_list.clone(),
                                        sort_by: c.sort_by,
                                        color: None,
                                    })
                                    .collect(),
                                header: false,
                                case_sensitive: s.case_sensitive,
                                by_columns: s.column_sort,
                                locale: None,
                            },
                        );
                    }
                }
                self.exec_all("grid.data.reapply_filter", cmds)?;
            }
            _ => {}
        }
        Ok(Change::Content)
    }

    fn apply_sort(&mut self, sid: SheetId, spec: &GridSortSpec) -> Result<()> {
        if spec.keys.is_empty() {
            return Err(KilnError::InvalidInput("add a sort level first".into()));
        }
        let range = match spec.range {
            Some(r) => Range::new(r.r0, r.c0, r.r1, r.c1),
            None => self.data_range()?,
        };
        let header = spec.header.unwrap_or_else(|| {
            let a = self.view().sel.active;
            match self.filter_at(sid, a) {
                Some(o) => o.range().r0 == range.r0,
                None => self.guess_header(sid, range, spec.keys[0].index),
            }
        });
        let keys: Vec<SortKey> = spec
            .keys
            .iter()
            .map(|k| {
                let inside = if spec.by_columns {
                    range.r0 <= k.index && k.index <= range.r1
                } else {
                    range.c0 <= k.index && k.index <= range.c1
                };
                if inside {
                    Ok(SortKey {
                        index: k.index,
                        descending: k.descending,
                        custom_list: k.custom_list.clone().filter(|l| !l.is_empty()),
                        sort_by: sort_by(k.on),
                        color: k.color.as_deref().and_then(parse_color),
                    })
                } else {
                    Err(KilnError::InvalidInput(
                        "a sort level is outside the range sorted".into(),
                    ))
                }
            })
            .collect::<Result<_>>()?;
        self.sort_range(
            "grid.data.apply_sort",
            sid,
            range,
            keys,
            header,
            spec.case_sensitive,
            spec.by_columns,
        )
    }

    /// Filter by Selected Cell's Value, Color or Font Color (the context menu): the active
    /// cell's column of its filter, or of a new AutoFilter over the current region.
    fn filter_by_selection(&mut self, sid: SheetId, by: GridSelectionKey) -> Result<()> {
        let a = self.view().sel.active;
        let text = self.workbook.display_text(sid, a.row, a.col);
        let style = self
            .workbook
            .styles()
            .get(
                self.workbook
                    .sheet(sid)
                    .map_or(0, |s| s.style_at(a.row, a.col)),
            )
            .clone();
        let theme = self.workbook.theme();
        let filter = match by {
            GridSelectionKey::Value => GridColumnFilter::Values {
                blanks: text.is_empty(),
                values: if text.is_empty() {
                    Vec::new()
                } else {
                    vec![text]
                },
            },
            GridSelectionKey::FillColor => GridColumnFilter::Color {
                color: hex(style.fill.main_color(), theme, 0xFF_FFFF)
                    .unwrap_or_else(|| "FFFFFF".into()),
                font: false,
            },
            GridSelectionKey::FontColor => GridColumnFilter::Color {
                color: hex(style.font.color, theme, 0).unwrap_or_else(|| "000000".into()),
                font: true,
            },
        };
        if self.filter_at(sid, a).is_none() {
            self.toggle_filter(sid)?;
        }
        let owner = self
            .filter_at(sid, a)
            .ok_or_else(|| KilnError::InvalidInput("the cell is not in a list".into()))?;
        let header = owner.range().r0;
        if a.row == header {
            return Err(KilnError::InvalidInput(
                "Select a cell below the headings to filter by its value.".into(),
            ));
        }
        self.sort_filter(&GridCommand::FilterColumn {
            row: header,
            col: a.col,
            filter: Some(filter),
        })?;
        Ok(())
    }

    fn toggle_filter(&mut self, sid: SheetId) -> Result<()> {
        let a = self.view().sel.active;
        let sheet = self
            .workbook
            .sheet(sid)
            .ok_or_else(|| KilnError::InvalidInput("no sheet".into()))?;
        // Inside a table: its filter buttons.
        if let Some(t) = sheet.table_at(a.row, a.col).cloned() {
            let mut cmds = Vec::new();
            let patch = if t.auto_filter.is_some() {
                // Show every row first.
                cmds.push(Command::UpdateTable {
                    name: t.name.clone(),
                    patch: Box::new(TablePatch {
                        auto_filter: Patch::Set(TableFilter::default()),
                        ..TablePatch::default()
                    }),
                });
                cmds.push(Command::ApplyFilter { sheet: sid });
                Patch::Clear
            } else {
                Patch::Set(TableFilter::default())
            };
            cmds.push(Command::UpdateTable {
                name: t.name,
                patch: Box::new(TablePatch {
                    auto_filter: patch,
                    ..TablePatch::default()
                }),
            });
            self.exec_all("grid.data.toggle_filter", cmds)?;
            return Ok(());
        }
        if let Some(f) = sheet.meta().auto_filter.clone() {
            let cmds = vec![
                Command::SetSheetMeta {
                    sheet: sid,
                    field: Box::new(MetaField::AutoFilter(Some(AutoFilter {
                        columns: Vec::new(),
                        ..f
                    }))),
                },
                Command::ApplyFilter { sheet: sid },
                Command::SetSheetMeta {
                    sheet: sid,
                    field: Box::new(MetaField::AutoFilter(None)),
                },
            ];
            self.exec_all("grid.data.toggle_filter", cmds)?;
            return Ok(());
        }
        let range = self.data_range()?;
        // A single row of data still gets a header and an empty first data row.
        let range = if range.r1 == range.r0 {
            Range::new(range.r0, range.c0, range.r0 + 1, range.c1)
        } else {
            range
        };
        self.exec(
            "grid.data.toggle_filter",
            Command::SetSheetMeta {
                sheet: sid,
                field: Box::new(MetaField::AutoFilter(Some(AutoFilter {
                    range,
                    columns: Vec::new(),
                    sort: None,
                }))),
            },
        )?;
        Ok(())
    }

    /// A protocol column filter as the model's.
    fn column_filter(&mut self, f: &GridColumnFilter) -> Result<ColumnFilter> {
        Ok(match f {
            GridColumnFilter::Values { values, blanks } => ColumnFilter::Values {
                values: values.clone(),
                blank: *blanks,
                date_groups: Vec::new(),
            },
            GridColumnFilter::Custom { and, criteria } => {
                if criteria.is_empty() || criteria.len() > 2 {
                    return Err(KilnError::InvalidInput(
                        "a custom filter has one or two criteria".into(),
                    ));
                }
                ColumnFilter::Custom {
                    and: *and,
                    criteria: criteria.iter().map(|c| op_of(c.op, &c.value)).collect(),
                }
            }
            GridColumnFilter::Top10 {
                top,
                percent,
                value,
            } => {
                let max = if *percent { 100 } else { 500 };
                if *value == 0 || *value > max {
                    return Err(KilnError::InvalidInput(format!(
                        "Top 10 shows 1 to {max} {}",
                        if *percent { "percent" } else { "items" }
                    )));
                }
                ColumnFilter::Top10 {
                    top: *top,
                    percent: *percent,
                    value: f64::from(*value),
                }
            }
            GridColumnFilter::Dynamic { kind } => ColumnFilter::Dynamic(dynamic_of(*kind)),
            GridColumnFilter::Color { color, font } => {
                let c = parse_color(color)
                    .ok_or_else(|| KilnError::InvalidInput(format!("not a colour: {color}")))?;
                let dxf = if *font {
                    Dxf {
                        font: Some(FontPatch {
                            color: Some(c),
                            ..FontPatch::default()
                        }),
                        ..Dxf::default()
                    }
                } else {
                    Dxf {
                        fill: Some(Fill::solid(c)),
                        ..Dxf::default()
                    }
                };
                ColumnFilter::Color {
                    cell_color: !*font,
                    dxf: self.log.intern_dxf(&mut self.workbook, dxf),
                }
            }
        })
    }

    /// A model column filter as the protocol's.
    fn grid_filter(&self, f: &ColumnFilter) -> Option<GridColumnFilter> {
        let theme = self.workbook.theme();
        Some(match f {
            ColumnFilter::Values { values, blank, .. } => GridColumnFilter::Values {
                values: values.clone(),
                blanks: *blank,
            },
            ColumnFilter::Custom { and, criteria } => GridColumnFilter::Custom {
                and: *and,
                criteria: criteria
                    .iter()
                    .map(|c| {
                        let (op, value) = grid_op(c);
                        kiln_protocol::commands::grid_features::GridCriterion { op, value }
                    })
                    .collect(),
            },
            ColumnFilter::Top10 {
                top,
                percent,
                value,
            } => GridColumnFilter::Top10 {
                top: *top,
                percent: *percent,
                value: value.max(0.0).min(f64::from(u32::MAX)) as u32,
            },
            ColumnFilter::Dynamic(d) => GridColumnFilter::Dynamic {
                kind: grid_dynamic(*d)?,
            },
            ColumnFilter::Color { cell_color, dxf } => {
                let d = self.workbook.styles().dxf(*dxf)?;
                let color = if *cell_color {
                    d.fill
                        .as_ref()
                        .and_then(|f| hex(f.main_color(), theme, 0xFF_FFFF))
                } else {
                    d.font.as_ref().and_then(|f| hex(f.color, theme, 0))
                }?;
                GridColumnFilter::Color {
                    color,
                    font: !*cell_color,
                }
            }
            ColumnFilter::Icon { .. } => return None,
        })
    }

    /// Before [`Self::filter_menu`]: the rows the other columns' filters let through.
    pub(crate) fn prepare_filter_menu(&mut self, row: u32, col: u32) {
        self.features.filter_others = None;
        let Ok(sid) = self.sid() else {
            return;
        };
        let Some(owner) = self.filter_owner(sid, row, col) else {
            return;
        };
        let range = owner.range();
        let col_id = col - range.c0;
        let others: Vec<FilterColumn> = owner
            .columns()
            .iter()
            .filter(|c| c.col_id != col_id && c.filter.is_some())
            .cloned()
            .collect();
        if let (Some(data), false) = (data_rows(range), others.is_empty()) {
            let pass = self.workbook.filter_pass(sid, data, &others);
            self.features.filter_others = Some((row, col, pass));
        }
    }

    /// An AutoFilter dropdown's list and state.
    pub(crate) fn filter_menu(&self, row: u32, col: u32) -> Result<GridFilterMenu> {
        let sid = self.sid()?;
        let owner = self.filter_owner(sid, row, col).ok_or_else(|| {
            KilnError::InvalidInput("there is no filter button at that cell".into())
        })?;
        let sheet = self
            .workbook
            .sheet(sid)
            .ok_or_else(|| KilnError::InvalidInput("no sheet".into()))?;
        let range = owner.range();
        let col_id = col - range.c0;
        let filter = owner
            .columns()
            .iter()
            .find(|c| c.col_id == col_id)
            .and_then(|c| c.filter.clone());
        let theme = self.workbook.theme();
        let mut seen: FxHashSet<String> = FxHashSet::default();
        let mut items: Vec<GridFilterItem> = Vec::new();
        let mut visible_values: FxHashSet<String> = FxHashSet::default();
        let mut blanks: Option<bool> = None;
        let (mut numbers, mut dates, mut texts) = (0u32, 0u32, 0u32);
        let mut fills: Vec<String> = Vec::new();
        let mut fonts: Vec<String> = Vec::new();
        let mut truncated = false;
        // Rows the other columns' filters hide are not listed (Excel's lists cascade).
        let others = self
            .features
            .filter_others
            .as_ref()
            .filter(|(r, c, _)| *r == row && *c == col)
            .map(|(_, _, pass)| pass.as_slice());
        let date1904 = self.workbook.props().date1904;
        if let Some(data) = data_rows(range) {
            for r in data.r0..=data.r1 {
                if others.is_some_and(|p| !p.get((r - data.r0) as usize).copied().unwrap_or(true)) {
                    continue;
                }
                let value = self.workbook.value(sid, r, col);
                let shown = !sheet.is_row_hidden(r) || !sheet.is_row_filtered(r);
                let style = self.workbook.styles().get(sheet.style_at(r, col));
                if let Some(h) = hex(style.fill.main_color(), theme, 0xFF_FFFF).filter(|_| {
                    !matches!(
                        style.fill,
                        Fill::Pattern {
                            pattern: kiln_sheet::styles::PatternType::None,
                            ..
                        }
                    )
                }) && !fills.contains(&h)
                    && fills.len() < 24
                {
                    fills.push(h);
                }
                if let Some(h) = hex(style.font.color, theme, 0).filter(|h| h != "000000")
                    && !fonts.contains(&h)
                    && fonts.len() < 24
                {
                    fonts.push(h);
                }
                if value.is_blank() {
                    blanks = Some(blanks.unwrap_or(false) || shown);
                    continue;
                }
                match value {
                    CellValue::Number(_) => {
                        if kiln_formula::numfmt::NumberFormat::parse(&style.num_fmt).is_date() {
                            dates += 1;
                        } else {
                            numbers += 1;
                        }
                    }
                    _ => texts += 1,
                }
                let text = self.workbook.display_text(sid, r, col);
                if shown {
                    visible_values.insert(text.clone());
                }
                if seen.len() >= 10_000 {
                    truncated = true;
                    continue;
                }
                if seen.insert(text.clone()) {
                    // Dates carry their day, so the menu can group them by year and month.
                    let date = match value {
                        CellValue::Number(n)
                            if kiln_formula::numfmt::NumberFormat::parse(&style.num_fmt)
                                .is_date()
                                && n >= 1.0 =>
                        {
                            let d = kiln_formula::values::serial_to_date(n.floor(), date1904);
                            Some(format!("{:04}-{:02}-{:02}", d.year, d.month, d.day))
                        }
                        _ => None,
                    };
                    items.push(GridFilterItem {
                        text,
                        checked: false,
                        date,
                    });
                }
            }
        }
        let kind = if dates > numbers && dates > texts {
            GridValueKind::Date
        } else if numbers > texts {
            GridValueKind::Number
        } else {
            GridValueKind::Text
        };
        // Numbers and dates in numeric order, text alphabetically.
        if kind == GridValueKind::Text {
            items.sort_by_cached_key(|i| i.text.to_lowercase());
        } else if let Some(data) = data_rows(range) {
            let mut order: Vec<(f64, String)> = Vec::new();
            let mut done: FxHashSet<String> = FxHashSet::default();
            for r in data.r0..=data.r1 {
                if let CellValue::Number(n) = self.workbook.value(sid, r, col) {
                    let t = self.workbook.display_text(sid, r, col);
                    if done.insert(t.clone()) {
                        order.push((n, t));
                    }
                }
            }
            order.sort_by(|a, b| a.0.total_cmp(&b.0));
            let rank: rustc_hash::FxHashMap<String, usize> = order
                .into_iter()
                .enumerate()
                .map(|(i, (_, t))| (t, i))
                .collect();
            items.sort_by_key(|i| rank.get(&i.text).copied().unwrap_or(usize::MAX));
        }
        match &filter {
            Some(ColumnFilter::Values { values, blank, .. }) => {
                let set: FxHashSet<&str> = values.iter().map(String::as_str).collect();
                for i in &mut items {
                    i.checked = set.contains(i.text.as_str());
                }
                blanks = blanks.map(|_| *blank);
            }
            None => {
                for i in &mut items {
                    i.checked = true;
                }
                blanks = blanks.map(|_| true);
            }
            Some(_) => {
                for i in &mut items {
                    i.checked = visible_values.contains(&i.text);
                }
            }
        }
        let sorted = owner.sort().and_then(|s| {
            s.conditions
                .first()
                .filter(|c| c.range.c0 == col)
                .map(|c| c.descending)
        });
        Ok(GridFilterMenu {
            row,
            col,
            header: self.workbook.display_text(sid, range.r0, col),
            kind,
            items,
            blanks,
            filter: filter.as_ref().and_then(|f| self.grid_filter(f)),
            fill_colors: fills,
            font_colors: fonts,
            sorted,
            truncated,
        })
    }

    /// Every filter header of the active sheet: (row, first col, last col, filtered col ids,
    /// sorted col and direction).
    fn filter_headers(&self) -> Vec<FilterHeader> {
        let Some((_, sheet)) = self.active_sheet() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let sorted = |s: Option<&SortState>| {
            s.and_then(|s| s.conditions.first().map(|c| (c.range.c0, c.descending)))
        };
        if let Some(f) = &sheet.meta().auto_filter {
            out.push((
                f.range.r0,
                f.range.c0,
                f.range.c1,
                f.columns
                    .iter()
                    .filter(|c| c.filter.is_some())
                    .map(|c| c.col_id)
                    .collect(),
                sorted(f.sort.as_ref()),
            ));
        }
        for t in sheet.tables() {
            if let (Some(f), true) = (&t.auto_filter, t.header_row) {
                out.push((
                    t.range.r0,
                    t.range.c0,
                    t.range.c1,
                    f.columns
                        .iter()
                        .filter(|c| c.filter.is_some())
                        .map(|c| c.col_id)
                        .collect(),
                    sorted(f.sort.as_ref()),
                ));
            }
        }
        out
    }

    /// The rectangle of a header's button (sheet pixels).
    fn button_rect(geo: &SheetGeometry, row: u32, col: u32) -> Bounds {
        let cell = geo.display_rect(row, col);
        let s = (cell.h - 4.0).clamp(10.0, 17.0);
        Bounds::new(cell.right() - s - 2.0, cell.bottom() - s - 2.0, s, s)
    }

    pub(crate) fn filter_buttons_ops(&self, geo: &SheetGeometry, out: &mut Vec<DrawOp>) {
        for (row, c0, c1, filtered, sorted) in self.filter_headers() {
            if geo.rows.is_hidden(row) {
                continue;
            }
            for col in c0..=c1.min(c0 + 2_000) {
                if geo.cols.is_hidden(col) {
                    continue;
                }
                let b = Self::button_rect(geo, row, col);
                let active = filtered.contains(&(col - c0));
                out.push(DrawOp::FillRect {
                    rect: b,
                    paint: if active {
                        Paint::Role(kiln_scene::ColorRole::AppTint)
                    } else {
                        Paint::Role(kiln_scene::ColorRole::Page)
                    },
                    radius: 3.0,
                });
                out.push(DrawOp::StrokeRect {
                    rect: b.inflate(-0.5, -0.5),
                    paint: Paint::Role(if active {
                        kiln_scene::ColorRole::AppFill
                    } else {
                        kiln_scene::ColorRole::Line
                    }),
                    width: 1.0,
                    radius: 3.0,
                });
                let ink = Paint::Role(if active {
                    kiln_scene::ColorRole::AppText
                } else {
                    kiln_scene::ColorRole::Muted
                });
                let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
                let mut p = PathData::new();
                let sort = sorted.filter(|(c, _)| *c == col);
                if active {
                    // A funnel.
                    p.move_to(cx - 4.0, cy - 3.5)
                        .line_to(cx + 4.0, cy - 3.5)
                        .line_to(cx + 1.0, cy)
                        .line_to(cx + 1.0, cy + 3.5)
                        .line_to(cx - 1.0, cy + 2.5)
                        .line_to(cx - 1.0, cy)
                        .close();
                    out.push(DrawOp::Path {
                        path: p,
                        fill: Some(ink),
                        stroke: None,
                        rule: FillRule::NonZero,
                    });
                } else {
                    // A chevron, with an arrow beside it once sorted.
                    let dx = if sort.is_some() { -2.0 } else { 0.0 };
                    p.move_to(cx - 3.0 + dx, cy - 1.5)
                        .line_to(cx + dx, cy + 1.5)
                        .line_to(cx + 3.0 + dx, cy - 1.5);
                    out.push(DrawOp::Path {
                        path: p,
                        fill: None,
                        stroke: Some(Stroke::solid(ink, 1.4)),
                        rule: FillRule::NonZero,
                    });
                }
                if let Some((_, desc)) = sort {
                    let mut a = PathData::new();
                    let x = cx + 4.0;
                    let (y0, y1) = if desc {
                        (cy - 4.0, cy + 4.0)
                    } else {
                        (cy + 4.0, cy - 4.0)
                    };
                    a.move_to(x, y0).line_to(x, y1);
                    a.move_to(x - 2.0, y1 + if desc { -2.0 } else { 2.0 })
                        .line_to(x, y1)
                        .line_to(x + 2.0, y1 + if desc { -2.0 } else { 2.0 });
                    out.push(DrawOp::Path {
                        path: a,
                        fill: None,
                        stroke: Some(Stroke::solid(ink, 1.1)),
                        rule: FillRule::NonZero,
                    });
                }
            }
        }
    }

    pub(crate) fn filter_button_hit(
        &self,
        x: f32,
        y: f32,
    ) -> Option<kiln_protocol::commands::GridHit> {
        let headers = self.filter_headers();
        if headers.is_empty() {
            return None;
        }
        self.with_geometry(|geo| {
            let hw = self.header_w(geo);
            let (_, sx, sy) = geo.to_sheet(&self.last_view, x, y, hw)?;
            let row = geo.rows.index_at(f64::from(sy));
            let col = geo.cols.index_at(f64::from(sx));
            for (r, c0, c1, _, _) in headers {
                if r == row && (c0..=c1).contains(&col) {
                    let b = Self::button_rect(geo, row, col).inflate(1.0, 1.0);
                    if b.contains(sx, sy) {
                        return Some(kiln_protocol::commands::GridHit::FilterButton { row, col });
                    }
                }
            }
            None
        })
    }
}
