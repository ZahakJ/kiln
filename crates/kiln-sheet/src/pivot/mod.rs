// SPDX-License-Identifier: AGPL-3.0-or-later
//! PivotTables (Insert ▸ PivotTable): the definition Excel keeps in `pivotTableDefinition`, the
//! cache of the source it summarises (`pivotCacheDefinition` and `pivotCacheRecords`), and the
//! report it writes into the sheet's cells.
//!
//! - [`PivotTable`]: source (a range, a table or a defined name), row, column, filter and value
//!   fields, the aggregation and "Show Values As" of each value, sorting, item, label and value
//!   filters, subtotals and grand totals, the compact, outline and tabular layouts and the
//!   PivotTable style.
//! - [`cache`]: the source read into one column of shared items per field; adding or moving
//!   fields works on the cache, as in Excel, and Refresh reads the source again.
//! - [`render`]: the report as cells (values, number formats, indents) and the [`PivotOutput`]
//!   that the PivotTable style, hit tests and drill-through read; [`render::drill`] lists the
//!   source rows behind a value.
//! - [`style`]: the built-in PivotStyleLight, Medium and Dark families.
//!
//! A sheet's PivotTables are part of its metadata (`SheetMeta::pivots`), so every change is one
//! undoable step with the cells the report wrote.

pub mod cache;
mod engine;
pub mod render;
pub mod style;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::address::{CellPos, Range};
use crate::value::CellValue;

pub use cache::PivotCache;
pub use engine::compare_items;
pub use render::{PivotLine, PivotLineKind, PivotOutput, PivotReport};

/// Where a PivotTable's data comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PivotSource {
    /// A range of a sheet (by name, as the file stores it); its first row holds the field names.
    Range {
        /// The sheet.
        sheet: String,
        /// The range, headers included.
        range: Range,
    },
    /// A table (Format as Table): the report follows its rows as they grow.
    Table {
        /// The table's name.
        name: String,
    },
    /// A defined name that refers to a range.
    Name {
        /// The name.
        name: String,
    },
}

/// How a value field summarises (Value Field Settings ▸ Summarize Values By).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PivotFunction {
    /// Sum.
    #[default]
    Sum,
    /// Count of non-empty values.
    Count,
    /// Average of the numbers.
    Average,
    /// Largest number.
    Max,
    /// Smallest number.
    Min,
    /// Product of the numbers.
    Product,
    /// Count of the numbers.
    CountNums,
    /// Sample standard deviation.
    StdDev,
    /// Population standard deviation.
    StdDevP,
    /// Sample variance.
    Var,
    /// Population variance.
    VarP,
}

impl PivotFunction {
    /// Every function, in the dialog's order.
    pub const ALL: [PivotFunction; 11] = [
        PivotFunction::Sum,
        PivotFunction::Count,
        PivotFunction::Average,
        PivotFunction::Max,
        PivotFunction::Min,
        PivotFunction::Product,
        PivotFunction::CountNums,
        PivotFunction::StdDev,
        PivotFunction::StdDevP,
        PivotFunction::Var,
        PivotFunction::VarP,
    ];

    /// The name Excel puts before the field (`Sum` of Amount, `Count` of …).
    pub fn caption(self) -> &'static str {
        match self {
            PivotFunction::Sum => "Sum",
            PivotFunction::Count => "Count",
            PivotFunction::Average => "Average",
            PivotFunction::Max => "Max",
            PivotFunction::Min => "Min",
            PivotFunction::Product => "Product",
            PivotFunction::CountNums => "Count",
            PivotFunction::StdDev => "StdDev",
            PivotFunction::StdDevP => "StdDevp",
            PivotFunction::Var => "Var",
            PivotFunction::VarP => "Varp",
        }
    }

    /// The `subtotal` attribute value of SpreadsheetML (`sum`, `countNums`, `stdDevp`, …).
    pub fn file_name(self) -> &'static str {
        match self {
            PivotFunction::Sum => "sum",
            PivotFunction::Count => "count",
            PivotFunction::Average => "average",
            PivotFunction::Max => "max",
            PivotFunction::Min => "min",
            PivotFunction::Product => "product",
            PivotFunction::CountNums => "countNums",
            PivotFunction::StdDev => "stdDev",
            PivotFunction::StdDevP => "stdDevp",
            PivotFunction::Var => "var",
            PivotFunction::VarP => "varp",
        }
    }

    /// The function of a SpreadsheetML name (case-insensitive).
    pub fn from_file_name(s: &str) -> Option<PivotFunction> {
        PivotFunction::ALL
            .into_iter()
            .find(|f| f.file_name().eq_ignore_ascii_case(s))
    }
}

/// Value Field Settings ▸ Show Values As.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShowAs {
    /// No Calculation.
    #[default]
    Normal,
    /// % of Grand Total.
    PercentOfTotal,
    /// % of Column Total.
    PercentOfColumn,
    /// % of Row Total.
    PercentOfRow,
    /// % of Parent Row Total.
    PercentOfParentRow,
    /// % of Parent Column Total.
    PercentOfParentColumn,
    /// % Of (the base item).
    Percent,
    /// Difference From (the base item).
    Difference,
    /// % Difference From (the base item).
    PercentDifference,
    /// Running Total In (the base field).
    RunningTotal,
    /// % Running Total In (the base field).
    PercentRunningTotal,
    /// Rank Smallest to Largest (within the base field).
    RankAscending,
    /// Rank Largest to Smallest (within the base field).
    RankDescending,
    /// Index: (value × grand total) / (row total × column total).
    Index,
}

impl ShowAs {
    /// Whether the calculation needs a base field.
    pub fn needs_base_field(self) -> bool {
        matches!(
            self,
            ShowAs::Percent
                | ShowAs::Difference
                | ShowAs::PercentDifference
                | ShowAs::RunningTotal
                | ShowAs::PercentRunningTotal
                | ShowAs::RankAscending
                | ShowAs::RankDescending
        )
    }

    /// Whether the calculation needs a base item.
    pub fn needs_base_item(self) -> bool {
        matches!(
            self,
            ShowAs::Percent | ShowAs::Difference | ShowAs::PercentDifference
        )
    }

    /// Whether the result is a fraction shown as a percentage.
    pub fn is_percent(self) -> bool {
        matches!(
            self,
            ShowAs::PercentOfTotal
                | ShowAs::PercentOfColumn
                | ShowAs::PercentOfRow
                | ShowAs::PercentOfParentRow
                | ShowAs::PercentOfParentColumn
                | ShowAs::Percent
                | ShowAs::PercentDifference
                | ShowAs::PercentRunningTotal
        )
    }
}

/// The item a "Show Values As" calculation compares with.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BaseItem {
    /// (previous).
    #[default]
    Previous,
    /// (next).
    Next,
    /// A named item.
    Item(CellValue),
}

/// One field of the Values area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PivotValue {
    /// The source field (index into [`PivotTable::fields`]).
    pub field: usize,
    /// The name shown (`Sum of Amount`).
    pub name: String,
    /// How it summarises.
    #[serde(default)]
    pub function: PivotFunction,
    /// Show Values As.
    #[serde(default)]
    pub show_as: ShowAs,
    /// The base field of a calculation that needs one.
    #[serde(default)]
    pub base_field: Option<usize>,
    /// The base item of a calculation that needs one.
    #[serde(default)]
    pub base_item: BaseItem,
    /// Number format code (`None`: General, or `0.00%` for percentages).
    #[serde(default)]
    pub num_fmt: Option<String>,
}

/// A place on the Rows or Columns axis: a field, or the Values pseudo-field (Σ Values) that
/// lays out several value fields side by side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AxisItem {
    /// A source field.
    Field(usize),
    /// Σ Values.
    Values,
}

/// How a field's items are ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PivotSort {
    /// A to Z (numbers first, then text, logicals, errors; blanks last).
    #[default]
    Ascending,
    /// Z to A.
    Descending,
    /// The order the items come in the source (Excel's data source order).
    Source,
}

/// A field's subtotals (Field Settings ▸ Subtotals).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PivotSubtotal {
    /// Automatic: each value's own function.
    #[default]
    Automatic,
    /// None.
    None,
    /// Custom functions, one subtotal line each.
    Custom(Vec<PivotFunction>),
}

/// Comparisons of label and value filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum PivotFilterOp {
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    BeginsWith,
    NotBeginsWith,
    EndsWith,
    NotEndsWith,
    Contains,
    NotContains,
    Between,
    NotBetween,
}

/// Label Filters: items whose label passes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PivotLabelFilter {
    /// The comparison.
    pub op: PivotFilterOp,
    /// The value (text, or a number as typed; `*` and `?` are wildcards in text comparisons).
    pub value: String,
    /// The second value of Between.
    #[serde(default)]
    pub value2: String,
}

/// Value Filters: items whose total of a value field passes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PivotValueFilter {
    /// Top 10…: the `count` largest (or smallest) items by the value, among their siblings.
    Top {
        /// The value field (index into [`PivotTable::values`]).
        value: usize,
        /// How many items.
        count: u32,
        /// Bottom instead of top.
        bottom: bool,
    },
    /// Items whose total compares with numbers.
    Compare {
        /// The value field.
        value: usize,
        /// The comparison (Equal … NotBetween).
        op: PivotFilterOp,
        /// The number.
        a: f64,
        /// The second number of Between.
        #[serde(default)]
        b: f64,
    },
}

/// One source field as the PivotTable uses it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PivotField {
    /// The field's name (the source column's header).
    pub name: String,
    /// A custom name shown in the report instead of `name`.
    #[serde(default)]
    pub caption: Option<String>,
    /// Items left out (the field's filter list, Excel's hidden items).
    #[serde(default)]
    pub hidden: Vec<CellValue>,
    /// Item order.
    #[serde(default)]
    pub sort: PivotSort,
    /// Sort by the totals of this value field (index into [`PivotTable::values`]) instead of
    /// by label.
    #[serde(default)]
    pub sort_by_value: Option<usize>,
    /// Subtotals of the field's groups.
    #[serde(default)]
    pub subtotal: PivotSubtotal,
    /// A label filter.
    #[serde(default)]
    pub label_filter: Option<PivotLabelFilter>,
    /// A value filter.
    #[serde(default)]
    pub value_filter: Option<PivotValueFilter>,
}

impl PivotField {
    /// A field with Excel's defaults.
    pub fn named(name: impl Into<String>) -> PivotField {
        PivotField {
            name: name.into(),
            ..PivotField::default()
        }
    }

    /// The name shown.
    pub fn shown_name(&self) -> &str {
        self.caption.as_deref().unwrap_or(&self.name)
    }
}

/// A field of the Filters area and what it shows (`(All)`, one item, or several through the
/// field's hidden items).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PivotPage {
    /// The field.
    pub field: usize,
    /// The one item selected (`None`: all the items not hidden).
    #[serde(default)]
    pub item: Option<CellValue>,
}

/// Design ▸ Report Layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PivotLayout {
    /// Compact Form: the row fields share one column, indented.
    #[default]
    Compact,
    /// Outline Form: one column per row field, groups above their items.
    Outline,
    /// Tabular Form: one column per row field, items beside their group.
    Tabular,
}

/// Design ▸ Subtotals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubtotalPlace {
    /// Show all subtotals at the top of the group (the group's own line).
    #[default]
    Top,
    /// Show all subtotals at the bottom of the group.
    Bottom,
    /// Do not show subtotals.
    Off,
}

/// The PivotTable style and its options (Design ▸ PivotTable Style Options).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PivotStyleInfo {
    /// Style name (`PivotStyleLight16`); empty for none.
    pub name: String,
    /// Row Headers.
    #[serde(default = "yes")]
    pub row_headers: bool,
    /// Column Headers.
    #[serde(default = "yes")]
    pub col_headers: bool,
    /// Banded Rows.
    #[serde(default)]
    pub row_stripes: bool,
    /// Banded Columns.
    #[serde(default)]
    pub col_stripes: bool,
}

impl Default for PivotStyleInfo {
    fn default() -> PivotStyleInfo {
        PivotStyleInfo {
            name: "PivotStyleLight16".into(),
            row_headers: true,
            col_headers: true,
            row_stripes: false,
            col_stripes: false,
        }
    }
}

fn yes() -> bool {
    true
}

/// Report options (PivotTable Options and the Design tab).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PivotOptions {
    /// Report Layout.
    #[serde(default)]
    pub layout: PivotLayout,
    /// Subtotals.
    #[serde(default)]
    pub subtotals: SubtotalPlace,
    /// Grand Totals for rows: the total column at the right (SpreadsheetML `rowGrandTotals`).
    #[serde(default = "yes")]
    pub row_totals: bool,
    /// Grand Totals for columns: the total row at the bottom (`colGrandTotals`).
    #[serde(default = "yes")]
    pub col_totals: bool,
    /// Repeat All Item Labels.
    #[serde(default)]
    pub repeat_labels: bool,
    /// Insert Blank Line after Each Item.
    #[serde(default)]
    pub blank_rows: bool,
    /// The text of empty value cells (`None`: they stay empty).
    #[serde(default)]
    pub empty_text: Option<String>,
    /// The caption of the row labels header (`Row Labels`).
    #[serde(default)]
    pub row_caption: Option<String>,
    /// The caption of the column labels header (`Column Labels`).
    #[serde(default)]
    pub col_caption: Option<String>,
    /// Characters each compact level indents by.
    #[serde(default = "one")]
    pub indent: u8,
    /// Style.
    #[serde(default)]
    pub style: PivotStyleInfo,
}

fn one() -> u8 {
    1
}

impl Default for PivotOptions {
    fn default() -> PivotOptions {
        PivotOptions {
            layout: PivotLayout::Compact,
            subtotals: SubtotalPlace::Top,
            row_totals: true,
            col_totals: true,
            repeat_labels: false,
            blank_rows: false,
            empty_text: None,
            row_caption: None,
            col_caption: None,
            indent: 1,
            style: PivotStyleInfo::default(),
        }
    }
}

/// A PivotTable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PivotTable {
    /// Name (`PivotTable1`), unique in the workbook.
    pub name: String,
    /// The data it summarises.
    pub source: PivotSource,
    /// The top-left cell of the report (below its Filters area).
    pub at: CellPos,
    /// One entry per source column, in source order.
    pub fields: Vec<PivotField>,
    /// The Rows area, outermost first.
    #[serde(default)]
    pub rows: Vec<AxisItem>,
    /// The Columns area, outermost first.
    #[serde(default)]
    pub cols: Vec<AxisItem>,
    /// The Filters area.
    #[serde(default)]
    pub pages: Vec<PivotPage>,
    /// The Values area.
    #[serde(default)]
    pub values: Vec<PivotValue>,
    /// Options.
    #[serde(default)]
    pub options: PivotOptions,
    /// The source as of the last refresh.
    pub cache: Arc<PivotCache>,
    /// The report as last written into the cells.
    #[serde(default)]
    pub output: PivotOutput,
}

impl PivotTable {
    /// A new PivotTable over a cache, with no fields placed yet.
    pub fn new(
        name: impl Into<String>,
        source: PivotSource,
        at: CellPos,
        cache: PivotCache,
    ) -> PivotTable {
        let fields = cache
            .fields
            .iter()
            .map(|f| PivotField::named(&f.name))
            .collect();
        PivotTable {
            name: name.into(),
            source,
            at,
            fields,
            rows: Vec::new(),
            cols: Vec::new(),
            pages: Vec::new(),
            values: Vec::new(),
            options: PivotOptions::default(),
            cache: Arc::new(cache),
            output: PivotOutput::default(),
        }
    }

    /// The index of a field by name (case-insensitive).
    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.fields
            .iter()
            .position(|f| f.name.eq_ignore_ascii_case(name))
    }

    /// Where a field is placed: rows, columns, filters (values are listed apart).
    pub fn area_of(&self, field: usize) -> Option<PivotArea> {
        if self.rows.contains(&AxisItem::Field(field)) {
            Some(PivotArea::Rows)
        } else if self.cols.contains(&AxisItem::Field(field)) {
            Some(PivotArea::Columns)
        } else if self.pages.iter().any(|p| p.field == field) {
            Some(PivotArea::Filters)
        } else {
            None
        }
    }

    /// The default name of a new value field (`Sum of Amount`), unique among the values.
    pub fn value_name(&self, field: usize, function: PivotFunction) -> String {
        let base = format!(
            "{} of {}",
            function.caption(),
            self.fields.get(field).map_or("", |f| f.name.as_str())
        );
        let mut name = base.clone();
        let mut n = 2;
        while self
            .values
            .iter()
            .any(|v| v.name.eq_ignore_ascii_case(&name))
            || self
                .fields
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case(&name))
        {
            name = format!("{base}{n}");
            n += 1;
        }
        name
    }

    /// The default name of a value field without a number to make it unique.
    pub fn value_name_for(&self, field: usize, function: PivotFunction) -> String {
        format!(
            "{} of {}",
            function.caption(),
            self.fields.get(field).map_or("", |f| f.name.as_str())
        )
    }

    /// Whether the Values pseudo-field belongs on an axis (two or more values), and keeps it
    /// there or takes it away accordingly (Excel adds Σ Values to Columns with the second
    /// value field and removes it with the second last).
    pub fn settle_values_axis(&mut self) {
        let has = self.rows.contains(&AxisItem::Values) || self.cols.contains(&AxisItem::Values);
        if self.values.len() >= 2 && !has {
            self.cols.push(AxisItem::Values);
        } else if self.values.len() < 2 && has {
            self.rows.retain(|a| *a != AxisItem::Values);
            self.cols.retain(|a| *a != AxisItem::Values);
        }
    }

    /// Places a field in an area at a position (moving it from where it was); for
    /// [`PivotArea::Values`] it adds a value field with the default function (Sum for numbers,
    /// Count otherwise) unless `function` says otherwise. `field` `None` moves Σ Values.
    pub fn place(
        &mut self,
        field: Option<usize>,
        area: PivotArea,
        position: Option<usize>,
        function: Option<PivotFunction>,
    ) {
        let Some(f) = field else {
            // Σ Values moves between Rows and Columns only.
            if self.values.len() < 2 || !matches!(area, PivotArea::Rows | PivotArea::Columns) {
                return;
            }
            self.rows.retain(|a| *a != AxisItem::Values);
            self.cols.retain(|a| *a != AxisItem::Values);
            let axis = if area == PivotArea::Rows {
                &mut self.rows
            } else {
                &mut self.cols
            };
            let at = position.unwrap_or(axis.len()).min(axis.len());
            axis.insert(at, AxisItem::Values);
            return;
        };
        if f >= self.fields.len() {
            return;
        }
        if area == PivotArea::Values {
            let function = function.unwrap_or_else(|| {
                if self.cache.fields[f].numeric_only() {
                    PivotFunction::Sum
                } else {
                    PivotFunction::Count
                }
            });
            let name = self.value_name(f, function);
            let v = PivotValue {
                field: f,
                name,
                function,
                show_as: ShowAs::Normal,
                base_field: None,
                base_item: BaseItem::Previous,
                num_fmt: None,
            };
            let at = position.unwrap_or(self.values.len()).min(self.values.len());
            self.values.insert(at, v);
            self.settle_values_axis();
            return;
        }
        self.remove_field_from_axes(f);
        match area {
            PivotArea::Rows | PivotArea::Columns => {
                let axis = if area == PivotArea::Rows {
                    &mut self.rows
                } else {
                    &mut self.cols
                };
                let at = position.unwrap_or(axis.len()).min(axis.len());
                axis.insert(at, AxisItem::Field(f));
            }
            PivotArea::Filters => {
                let at = position.unwrap_or(self.pages.len()).min(self.pages.len());
                self.pages.insert(
                    at,
                    PivotPage {
                        field: f,
                        item: None,
                    },
                );
            }
            PivotArea::Values => {}
        }
    }

    /// Takes a field out of Rows, Columns and Filters.
    pub fn remove_field_from_axes(&mut self, f: usize) {
        self.rows.retain(|a| *a != AxisItem::Field(f));
        self.cols.retain(|a| *a != AxisItem::Field(f));
        self.pages.retain(|p| p.field != f);
    }

    /// Takes a field out of every area (unticking it in the field list).
    pub fn remove_field(&mut self, f: usize) {
        self.remove_field_from_axes(f);
        self.remove_values_of(f);
    }

    /// Removes the value fields of a source field.
    pub fn remove_values_of(&mut self, f: usize) {
        let before = self.values.len();
        let kept: Vec<bool> = self.values.iter().map(|v| v.field != f).collect();
        self.values.retain(|v| v.field != f);
        if self.values.len() != before {
            self.reindex_values(&kept);
        }
        self.settle_values_axis();
    }

    /// Removes one value field.
    pub fn remove_value(&mut self, index: usize) {
        if index >= self.values.len() {
            return;
        }
        let kept: Vec<bool> = (0..self.values.len()).map(|i| i != index).collect();
        self.values.remove(index);
        self.reindex_values(&kept);
        self.settle_values_axis();
    }

    /// After values were removed (`kept[i]` for each old index), fixes the value indices that
    /// sorting and value filters refer to.
    fn reindex_values(&mut self, kept: &[bool]) {
        let map = |old: usize| -> Option<usize> {
            if !kept.get(old).copied().unwrap_or(false) {
                return None;
            }
            Some(kept[..old].iter().filter(|k| **k).count())
        };
        for f in &mut self.fields {
            f.sort_by_value = f.sort_by_value.and_then(map);
            f.value_filter = match f.value_filter.take() {
                Some(PivotValueFilter::Top {
                    value,
                    count,
                    bottom,
                }) => map(value).map(|value| PivotValueFilter::Top {
                    value,
                    count,
                    bottom,
                }),
                Some(PivotValueFilter::Compare { value, op, a, b }) => {
                    map(value).map(|value| PivotValueFilter::Compare { value, op, a, b })
                }
                None => None,
            };
        }
    }

    /// The cells the report wrote (an empty report's placeholder holds none).
    pub fn cells_range(&self) -> Option<Range> {
        if self.output.empty {
            self.output.pages
        } else {
            self.whole_range()
        }
    }

    /// The report cell `GETPIVOTDATA` reads: the value field named `data_field` (its name, the
    /// default `Sum of …` name or the source field's name, without case) at the items `pairs`
    /// give for row, column and filter fields. Row and column fields left out select their
    /// total, so they must be the inner levels; a filter field must name its selected item.
    /// `None` when the report does not show such a value.
    pub fn data_cell(&self, data_field: &str, pairs: &[(String, CellValue)]) -> Option<(u32, u32)> {
        if self.output.empty {
            return None;
        }
        let v = self.values.iter().position(|v| {
            v.name.eq_ignore_ascii_case(data_field)
                || self
                    .value_name_for(v.field, v.function)
                    .eq_ignore_ascii_case(data_field)
                || self
                    .fields
                    .get(v.field)
                    .is_some_and(|f| f.name.eq_ignore_ascii_case(data_field))
        })?;
        let named = |f: usize, name: &str| {
            self.fields.get(f).is_some_and(|x| {
                x.name.eq_ignore_ascii_case(name) || x.shown_name().eq_ignore_ascii_case(name)
            })
        };
        let mut used = vec![false; pairs.len()];
        let item_of = |f: usize, used: &mut Vec<bool>| -> Option<Option<u32>> {
            match pairs.iter().position(|(n, _)| named(f, n)) {
                None => Some(None),
                Some(i) => {
                    used[i] = true;
                    let idx = self.cache.fields.get(f)?.find(&pairs[i].1)?;
                    Some(Some(idx))
                }
            }
        };
        let path_of = |axis: &[AxisItem], used: &mut Vec<bool>| -> Option<(Vec<u32>, usize)> {
            let mut path = Vec::new();
            let mut gap = false;
            let mut fields = 0;
            for a in axis {
                let AxisItem::Field(f) = *a else { continue };
                fields += 1;
                match item_of(f, used)? {
                    Some(i) if !gap => path.push(i),
                    Some(_) => return None,
                    None => gap = true,
                }
            }
            Some((path, fields))
        };
        let (row_path, row_fields) = path_of(&self.rows, &mut used)?;
        let (col_path, col_fields) = path_of(&self.cols, &mut used)?;
        for p in &self.pages {
            if let Some(i) = pairs.iter().position(|(n, _)| named(p.field, n)) {
                used[i] = true;
                let want = cache::ItemKey::of(&pairs[i].1);
                if p.item.as_ref().map(cache::ItemKey::of) != Some(want) {
                    return None;
                }
            }
        }
        if used.iter().any(|u| !u) {
            return None;
        }
        let pick = |lines: &[PivotLine], path: &[u32], fields: usize| -> Option<usize> {
            let value_ok = |l: &PivotLine| l.value.is_none_or(|x| usize::from(x) == v);
            let shows = |l: &PivotLine| l.values && l.function.is_none() && value_ok(l);
            if path.is_empty() {
                return lines
                    .iter()
                    .position(|l| l.kind == PivotLineKind::Grand && shows(l))
                    .or_else(|| {
                        (fields == 0)
                            .then(|| lines.iter().position(|l| l.path.is_empty() && shows(l)))?
                    });
            }
            let want = if path.len() == fields {
                [PivotLineKind::Item, PivotLineKind::Item]
            } else {
                [PivotLineKind::Group, PivotLineKind::Subtotal]
            };
            lines
                .iter()
                .position(|l| want.contains(&l.kind) && l.path == path && shows(l))
        };
        let ri = pick(&self.output.rows, &row_path, row_fields)?;
        let ci = pick(&self.output.cols, &col_path, col_fields)?;
        Some((
            self.output.first_data_row()? + u32::try_from(ri).ok()?,
            self.output.first_data_col()? + u32::try_from(ci).ok()?,
        ))
    }

    /// The report's whole area: the Filters area and the table.
    pub fn whole_range(&self) -> Option<Range> {
        let t = self.output.range?;
        Some(match self.output.pages {
            Some(p) => t.union(&p),
            None => t,
        })
    }
}

/// The areas of the PivotTable Fields pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PivotArea {
    /// Filters.
    Filters,
    /// Columns.
    Columns,
    /// Rows.
    Rows,
    /// Values.
    Values,
}

/// Errors of PivotTable operations, in Excel's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PivotError {
    /// The source is not a usable range.
    BadSource(String),
    /// The report would cover cells it does not own.
    Overlap(String),
}

impl std::fmt::Display for PivotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PivotError::BadSource(s) | PivotError::Overlap(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for PivotError {}
