// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sheet and workbook metadata: defined names, tables (ListObjects), conditional formatting rules,
//! data validation, AutoFilter and sort state, hyperlinks, notes and threaded comments, sheet
//! views, print settings, drawing anchors, protection and workbook properties. All of it is plain
//! data, so it snapshots, serialises and diffs trivially.
//!
//! Formulas inside metadata (names, rules, validation criteria, table column formulas) are stored
//! without the leading `=`, as in SpreadsheetML.

use kiln_formula::SheetId;
use serde::{Deserialize, Serialize};

use crate::address::{CellPos, Range};
use crate::color::Color;

fn yes() -> bool {
    true
}

// ── names ─────────────────────────────────────────────────────────────────

/// A defined name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefinedName {
    /// The name.
    pub name: String,
    /// Formula text without `=` (`Sheet1!$A$1:$B$10`).
    pub formula: String,
    /// The sheet a local name belongs to; `None` = workbook scope.
    #[serde(default, with = "opt_sheet")]
    pub scope: Option<SheetId>,
    /// Hidden from the Name Manager.
    #[serde(default)]
    pub hidden: bool,
    /// Comment.
    #[serde(default)]
    pub comment: Option<String>,
}

/// Excel's reserved built-in names.
pub mod builtin_names {
    /// Print area.
    pub const PRINT_AREA: &str = "_xlnm.Print_Area";
    /// Rows/columns repeated on each printed page.
    pub const PRINT_TITLES: &str = "_xlnm.Print_Titles";
    /// The AutoFilter range.
    pub const FILTER_DATABASE: &str = "_xlnm._FilterDatabase";
    /// Advanced filter criteria.
    pub const CRITERIA: &str = "_xlnm.Criteria";
    /// Advanced filter output.
    pub const EXTRACT: &str = "_xlnm.Extract";
    /// All of them.
    pub const ALL: [&str; 5] = [PRINT_AREA, PRINT_TITLES, FILTER_DATABASE, CRITERIA, EXTRACT];
}

pub(crate) mod opt_sheet {
    use kiln_formula::SheetId;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(crate) fn serialize<S: Serializer>(v: &Option<SheetId>, s: S) -> Result<S::Ok, S::Error> {
        v.map(|id| id.0).serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<SheetId>, D::Error> {
        Ok(Option::<u32>::deserialize(d)?.map(SheetId))
    }
}

pub(crate) mod sheet_id {
    use kiln_formula::SheetId;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(crate) fn serialize<S: Serializer>(v: &SheetId, s: S) -> Result<S::Ok, S::Error> {
        v.0.serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SheetId, D::Error> {
        Ok(SheetId(u32::deserialize(d)?))
    }
}

// ── AutoFilter and sort state ─────────────────────────────────────────────

/// Comparison operators of custom filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum FilterOperator {
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
}

/// Dynamic (date and average) filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum DynamicFilter {
    AboveAverage,
    BelowAverage,
    Today,
    Yesterday,
    Tomorrow,
    ThisWeek,
    LastWeek,
    NextWeek,
    ThisMonth,
    LastMonth,
    NextMonth,
    ThisQuarter,
    LastQuarter,
    NextQuarter,
    ThisYear,
    LastYear,
    NextYear,
    YearToDate,
    /// A quarter of any year (1–4).
    Quarter(u8),
    /// A month of any year (1–12).
    Month(u8),
}

/// The granularity of a date group in a values filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum DateGrouping {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

/// A date group of a values filter (every date in that year, month, day…).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct DateGroupItem {
    pub grouping: DateGrouping,
    pub year: u16,
    #[serde(default)]
    pub month: u8,
    #[serde(default)]
    pub day: u8,
    #[serde(default)]
    pub hour: u8,
    #[serde(default)]
    pub minute: u8,
    #[serde(default)]
    pub second: u8,
}

/// One criterion of a custom filter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CustomCriterion {
    /// Operator.
    pub op: FilterOperator,
    /// Value as typed (`*` and `?` are wildcards for text; numbers compare numerically).
    pub value: String,
}

/// The filter of one AutoFilter column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColumnFilter {
    /// Show rows whose displayed text is one of `values` (or blank, or in a date group).
    Values {
        /// Displayed texts to show.
        values: Vec<String>,
        /// Show blank cells.
        #[serde(default)]
        blank: bool,
        /// Date groups to show.
        #[serde(default)]
        date_groups: Vec<DateGroupItem>,
    },
    /// One or two comparisons.
    Custom {
        /// Both must hold (otherwise either).
        #[serde(default)]
        and: bool,
        /// One or two criteria.
        criteria: Vec<CustomCriterion>,
    },
    /// Top or bottom N items or percent.
    Top10 {
        /// Top (otherwise bottom).
        top: bool,
        /// `value` is a percentage.
        percent: bool,
        /// N.
        value: f64,
    },
    /// A dynamic filter.
    Dynamic(DynamicFilter),
    /// By cell fill colour or font colour (a differential format holding the colour).
    Color {
        /// Fill colour (otherwise font colour).
        cell_color: bool,
        /// The colour, as the dxf index of the file.
        dxf: u32,
    },
    /// By conditional-formatting icon.
    Icon {
        /// Icon set.
        icon_set: IconSetName,
        /// Icon index in the set.
        icon_id: u32,
    },
}

/// One AutoFilter column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterColumn {
    /// Column offset within the filter range.
    pub col_id: u32,
    /// Hide the dropdown button.
    #[serde(default)]
    pub hidden_button: bool,
    /// Show the button (default true).
    #[serde(default = "yes")]
    pub show_button: bool,
    /// The filter, if any.
    #[serde(default)]
    pub filter: Option<ColumnFilter>,
}

/// What a sort condition compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortBy {
    /// Cell values.
    #[default]
    Value,
    /// Fill colour (`dxf`'s colour on top).
    CellColor,
    /// Font colour.
    FontColor,
    /// Conditional-formatting icon.
    Icon,
}

/// One sort level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SortCondition {
    /// The column (or row) sorted by.
    pub range: Range,
    /// Descending.
    #[serde(default)]
    pub descending: bool,
    /// What is compared.
    #[serde(default)]
    pub sort_by: SortBy,
    /// Custom list order.
    #[serde(default)]
    pub custom_list: Option<Vec<String>>,
    /// The colour sorted on top (as a dxf index).
    #[serde(default)]
    pub dxf: Option<u32>,
}

/// A remembered sort.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SortState {
    /// Sorted range.
    pub range: Range,
    /// Case sensitive.
    #[serde(default)]
    pub case_sensitive: bool,
    /// Left to right.
    #[serde(default)]
    pub column_sort: bool,
    /// Levels.
    pub conditions: Vec<SortCondition>,
}

/// A sheet's AutoFilter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoFilter {
    /// The range including the header row.
    pub range: Range,
    /// Filtered columns.
    #[serde(default)]
    pub columns: Vec<FilterColumn>,
    /// Sort state.
    #[serde(default)]
    pub sort: Option<SortState>,
}

// ── tables ────────────────────────────────────────────────────────────────

/// Totals-row functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum TotalsFunction {
    #[default]
    None,
    Sum,
    Min,
    Max,
    Average,
    Count,
    CountNums,
    StdDev,
    Var,
    Custom,
}

impl TotalsFunction {
    /// The SUBTOTAL function number Excel writes for it (`SUBTOTAL(109,[Col])`).
    pub fn subtotal_number(self) -> Option<u32> {
        Some(match self {
            TotalsFunction::Average => 101,
            TotalsFunction::CountNums => 102,
            TotalsFunction::Count => 103,
            TotalsFunction::Max => 104,
            TotalsFunction::Min => 105,
            TotalsFunction::StdDev => 107,
            TotalsFunction::Sum => 109,
            TotalsFunction::Var => 110,
            TotalsFunction::None | TotalsFunction::Custom => return None,
        })
    }
}

/// A table column.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TableColumn {
    /// Header name.
    pub name: String,
    /// Totals-row function.
    #[serde(default)]
    pub totals_function: TotalsFunction,
    /// Totals-row label (text instead of a function).
    #[serde(default)]
    pub totals_label: Option<String>,
    /// Totals-row formula for [`TotalsFunction::Custom`].
    #[serde(default)]
    pub totals_formula: Option<String>,
    /// Calculated-column formula.
    #[serde(default)]
    pub calculated_formula: Option<String>,
    /// Data-area format.
    #[serde(default)]
    pub data_dxf: Option<u32>,
    /// Header format.
    #[serde(default)]
    pub header_dxf: Option<u32>,
    /// Totals format.
    #[serde(default)]
    pub totals_dxf: Option<u32>,
}

impl TableColumn {
    /// A column with a name and nothing else.
    pub fn named(name: impl Into<String>) -> TableColumn {
        TableColumn {
            name: name.into(),
            ..TableColumn::default()
        }
    }
}

/// Table style options (Table Design › Table Style Options).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableStyleInfo {
    /// Style name (`TableStyleMedium2`).
    pub name: String,
    /// Emphasise the first column.
    #[serde(default)]
    pub show_first_column: bool,
    /// Emphasise the last column.
    #[serde(default)]
    pub show_last_column: bool,
    /// Banded rows.
    #[serde(default = "yes")]
    pub show_row_stripes: bool,
    /// Banded columns.
    #[serde(default)]
    pub show_column_stripes: bool,
}

impl Default for TableStyleInfo {
    fn default() -> TableStyleInfo {
        TableStyleInfo {
            name: "TableStyleMedium2".into(),
            show_first_column: false,
            show_last_column: false,
            show_row_stripes: true,
            show_column_stripes: false,
        }
    }
}

/// A table's own filter (columns relative to the table).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TableFilter {
    /// Filtered columns.
    #[serde(default)]
    pub columns: Vec<FilterColumn>,
    /// Sort state.
    #[serde(default)]
    pub sort: Option<SortState>,
}

/// An Excel table (ListObject).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    /// Workbook-unique numeric id.
    pub id: u32,
    /// Name used by structured references (unique in the workbook, ignoring case).
    pub name: String,
    /// Display name (usually the name).
    #[serde(default)]
    pub display_name: Option<String>,
    /// The full range including the header and totals rows.
    pub range: Range,
    /// Has a header row.
    #[serde(default = "yes")]
    pub header_row: bool,
    /// Has a totals row.
    #[serde(default)]
    pub totals_row: bool,
    /// Columns, left to right.
    pub columns: Vec<TableColumn>,
    /// Style.
    #[serde(default)]
    pub style: Option<TableStyleInfo>,
    /// The table's filter (`None` = no filter buttons).
    #[serde(default)]
    pub auto_filter: Option<TableFilter>,
    /// Comment.
    #[serde(default)]
    pub comment: Option<String>,
}

impl Table {
    /// The data rows (between the header and totals rows), or `None` when there are none.
    pub fn data_range(&self) -> Option<Range> {
        let r0 = self.range.r0 + u32::from(self.header_row);
        let r1 = self.range.r1.checked_sub(u32::from(self.totals_row))?;
        (r0 <= r1).then_some(Range {
            r0,
            c0: self.range.c0,
            r1,
            c1: self.range.c1,
        })
    }

    /// The index of a column by name (ignoring case).
    pub fn column_index(&self, name: &str) -> Option<usize> {
        let u = name.to_uppercase();
        self.columns.iter().position(|c| c.name.to_uppercase() == u)
    }

    /// The engine's view of the table.
    pub fn to_def(&self, sheet: SheetId) -> kiln_formula::engine::TableDef {
        kiln_formula::engine::TableDef {
            name: self.name.clone(),
            sheet,
            bounds: self.range.to_bounds(),
            header_row: self.header_row,
            totals_row: self.totals_row,
            columns: self.columns.iter().map(|c| c.name.clone()).collect(),
        }
    }
}

// ── conditional formatting ────────────────────────────────────────────────

/// Comparison operators of cell-value rules and validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum Operator {
    #[default]
    Between,
    NotBetween,
    Equal,
    NotEqual,
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
}

impl Operator {
    /// True for the two-operand operators.
    pub fn is_range(self) -> bool {
        matches!(self, Operator::Between | Operator::NotBetween)
    }
}

/// Time periods of "A Date Occurring" rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum TimePeriod {
    Today,
    Yesterday,
    Tomorrow,
    Last7Days,
    ThisWeek,
    LastWeek,
    NextWeek,
    ThisMonth,
    LastMonth,
    NextMonth,
}

/// Value object types of colour scales, data bars and icon sets (with the x14 automatic ones).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum CfvoType {
    Min,
    Max,
    Num,
    Percent,
    Percentile,
    Formula,
    /// x14: the minimum, or 0 when every value is positive.
    AutoMin,
    /// x14: the maximum, or 0 when every value is negative.
    AutoMax,
}

/// A threshold of a colour scale, data bar or icon set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cfvo {
    /// Kind.
    pub kind: CfvoType,
    /// Number or formula text, depending on the kind.
    #[serde(default)]
    pub value: Option<String>,
    /// Icon sets: the threshold is `>=` (otherwise `>`).
    #[serde(default = "yes")]
    pub gte: bool,
}

impl Cfvo {
    /// A value object of a kind without a value.
    pub fn of(kind: CfvoType) -> Cfvo {
        Cfvo {
            kind,
            value: None,
            gte: true,
        }
    }

    /// A value object with a value.
    pub fn with(kind: CfvoType, value: impl Into<String>) -> Cfvo {
        Cfvo {
            kind,
            value: Some(value.into()),
            gte: true,
        }
    }
}

/// Icon sets (with the x14 additions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(missing_docs)]
pub enum IconSetName {
    #[serde(rename = "3Arrows")]
    Arrows3,
    #[serde(rename = "3ArrowsGray")]
    ArrowsGray3,
    #[serde(rename = "3Flags")]
    Flags3,
    #[serde(rename = "3TrafficLights1")]
    TrafficLights3,
    #[serde(rename = "3TrafficLights2")]
    TrafficLightsRimmed3,
    #[serde(rename = "3Signs")]
    Signs3,
    #[serde(rename = "3Symbols")]
    Symbols3,
    #[serde(rename = "3Symbols2")]
    SymbolsUncircled3,
    #[serde(rename = "3Stars")]
    Stars3,
    #[serde(rename = "3Triangles")]
    Triangles3,
    #[serde(rename = "4Arrows")]
    Arrows4,
    #[serde(rename = "4ArrowsGray")]
    ArrowsGray4,
    #[serde(rename = "4RedToBlack")]
    RedToBlack4,
    #[serde(rename = "4Rating")]
    Rating4,
    #[serde(rename = "4TrafficLights")]
    TrafficLights4,
    #[serde(rename = "5Arrows")]
    Arrows5,
    #[serde(rename = "5ArrowsGray")]
    ArrowsGray5,
    #[serde(rename = "5Rating")]
    Rating5,
    #[serde(rename = "5Quarters")]
    Quarters5,
    #[serde(rename = "5Boxes")]
    Boxes5,
    /// x14: no icon (used in custom icon lists).
    NoIcons,
}

impl IconSetName {
    /// Number of icons in the set.
    pub fn len(self) -> usize {
        use IconSetName::*;
        match self {
            Arrows3 | ArrowsGray3 | Flags3 | TrafficLights3 | TrafficLightsRimmed3 | Signs3
            | Symbols3 | SymbolsUncircled3 | Stars3 | Triangles3 => 3,
            Arrows4 | ArrowsGray4 | RedToBlack4 | Rating4 | TrafficLights4 => 4,
            Arrows5 | ArrowsGray5 | Rating5 | Quarters5 | Boxes5 => 5,
            NoIcons => 0,
        }
    }

    /// True for [`IconSetName::NoIcons`].
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// The OOXML name (`3TrafficLights1`).
    pub fn as_str(self) -> &'static str {
        use IconSetName::*;
        match self {
            Arrows3 => "3Arrows",
            ArrowsGray3 => "3ArrowsGray",
            Flags3 => "3Flags",
            TrafficLights3 => "3TrafficLights1",
            TrafficLightsRimmed3 => "3TrafficLights2",
            Signs3 => "3Signs",
            Symbols3 => "3Symbols",
            SymbolsUncircled3 => "3Symbols2",
            Stars3 => "3Stars",
            Triangles3 => "3Triangles",
            Arrows4 => "4Arrows",
            ArrowsGray4 => "4ArrowsGray",
            RedToBlack4 => "4RedToBlack",
            Rating4 => "4Rating",
            TrafficLights4 => "4TrafficLights",
            Arrows5 => "5Arrows",
            ArrowsGray5 => "5ArrowsGray",
            Rating5 => "5Rating",
            Quarters5 => "5Quarters",
            Boxes5 => "5Boxes",
            NoIcons => "NoIcons",
        }
    }
}

/// Where a data bar's axis sits (x14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum AxisPosition {
    #[default]
    Automatic,
    Middle,
    None,
}

/// Bar direction (x14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum BarDirection {
    #[default]
    Context,
    LeftToRight,
    RightToLeft,
}

/// A data bar rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataBar {
    /// Lower threshold.
    pub min: Cfvo,
    /// Upper threshold.
    pub max: Cfvo,
    /// Bar colour.
    pub color: Color,
    /// Show the cell value next to the bar.
    #[serde(default = "yes")]
    pub show_value: bool,
    /// Shortest bar, percent of the cell width (Excel 2007: 10; x14: 0).
    #[serde(default)]
    pub min_length: u32,
    /// Longest bar, percent (default 90; x14: 100).
    #[serde(default = "ninety")]
    pub max_length: u32,
    /// Gradient (otherwise solid) fill (x14; default true).
    #[serde(default = "yes")]
    pub gradient: bool,
    /// Bar border colour (x14).
    #[serde(default)]
    pub border_color: Option<Color>,
    /// Fill colour of negative bars (x14).
    #[serde(default)]
    pub negative_color: Option<Color>,
    /// Border colour of negative bars (x14).
    #[serde(default)]
    pub negative_border_color: Option<Color>,
    /// Axis colour (x14).
    #[serde(default)]
    pub axis_color: Option<Color>,
    /// Axis position (x14).
    #[serde(default)]
    pub axis_position: AxisPosition,
    /// Direction (x14).
    #[serde(default)]
    pub direction: BarDirection,
}

fn ninety() -> u32 {
    90
}

/// A custom icon of an icon-set rule (x14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CustomIcon {
    /// Set the icon comes from.
    pub icon_set: IconSetName,
    /// Index in that set.
    pub icon_id: u32,
}

/// An icon-set rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IconSet {
    /// The set.
    pub icon_set: IconSetName,
    /// One threshold per icon (the first is the minimum and ignored).
    pub cfvos: Vec<Cfvo>,
    /// Reverse the icon order.
    #[serde(default)]
    pub reverse: bool,
    /// Show the cell value.
    #[serde(default = "yes")]
    pub show_value: bool,
    /// Per-threshold icons replacing the set's (x14).
    #[serde(default)]
    pub custom_icons: Vec<CustomIcon>,
}

/// The kinds of conditional-format rules (every rule type of Excel).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CfKind {
    /// Cell value compared with one or two formulas.
    CellIs {
        /// Operator.
        operator: Operator,
        /// One or two operand formulas (relative to the top-left cell of the first range).
        formulas: Vec<String>,
        /// Format applied.
        dxf: u32,
    },
    /// A formula that is TRUE for the cells to format.
    Expression {
        /// The formula (relative to the top-left cell of the first range).
        formula: String,
        /// Format applied.
        dxf: u32,
    },
    /// Text contains.
    ContainsText {
        /// Text.
        text: String,
        /// Format.
        dxf: u32,
    },
    /// Text does not contain.
    NotContainsText {
        /// Text.
        text: String,
        /// Format.
        dxf: u32,
    },
    /// Text begins with.
    BeginsWith {
        /// Text.
        text: String,
        /// Format.
        dxf: u32,
    },
    /// Text ends with.
    EndsWith {
        /// Text.
        text: String,
        /// Format.
        dxf: u32,
    },
    /// Blank cells.
    ContainsBlanks {
        /// Format.
        dxf: u32,
    },
    /// Non-blank cells.
    NotContainsBlanks {
        /// Format.
        dxf: u32,
    },
    /// Errors.
    ContainsErrors {
        /// Format.
        dxf: u32,
    },
    /// Non-errors.
    NotContainsErrors {
        /// Format.
        dxf: u32,
    },
    /// A date occurring.
    TimePeriod {
        /// Period.
        period: TimePeriod,
        /// Format.
        dxf: u32,
    },
    /// Values that occur more than once in the ranges.
    DuplicateValues {
        /// Format.
        dxf: u32,
    },
    /// Values that occur once.
    UniqueValues {
        /// Format.
        dxf: u32,
    },
    /// Top/bottom N items or percent.
    Top10 {
        /// N.
        rank: u32,
        /// N is a percentage.
        #[serde(default)]
        percent: bool,
        /// Bottom instead of top.
        #[serde(default)]
        bottom: bool,
        /// Format.
        dxf: u32,
    },
    /// Above/below the average (optionally by N standard deviations).
    AboveAverage {
        /// Above (otherwise below).
        above: bool,
        /// Include equal values.
        #[serde(default)]
        equal: bool,
        /// Standard deviations (1–3), 0 = plain average.
        #[serde(default)]
        std_dev: u8,
        /// Format.
        dxf: u32,
    },
    /// A two- or three-colour scale.
    ColorScale {
        /// Thresholds.
        cfvos: Vec<Cfvo>,
        /// One colour per threshold.
        colors: Vec<Color>,
    },
    /// A data bar.
    DataBar(DataBar),
    /// An icon set.
    IconSet(IconSet),
}

impl CfKind {
    /// The differential format the rule applies, if it applies one.
    pub fn dxf(&self) -> Option<u32> {
        use CfKind::*;
        match self {
            CellIs { dxf, .. }
            | Expression { dxf, .. }
            | ContainsText { dxf, .. }
            | NotContainsText { dxf, .. }
            | BeginsWith { dxf, .. }
            | EndsWith { dxf, .. }
            | ContainsBlanks { dxf }
            | NotContainsBlanks { dxf }
            | ContainsErrors { dxf }
            | NotContainsErrors { dxf }
            | TimePeriod { dxf, .. }
            | DuplicateValues { dxf }
            | UniqueValues { dxf }
            | Top10 { dxf, .. }
            | AboveAverage { dxf, .. } => Some(*dxf),
            ColorScale { .. } | DataBar(_) | IconSet(_) => None,
        }
    }

    /// The formulas inside the rule, mutably (for reference rewriting).
    pub fn formulas_mut(&mut self) -> Vec<&mut String> {
        match self {
            CfKind::CellIs { formulas, .. } => formulas.iter_mut().collect(),
            CfKind::Expression { formula, .. } => vec![formula],
            CfKind::ColorScale { cfvos, .. } => formula_cfvos(cfvos.iter_mut()),
            CfKind::DataBar(b) => formula_cfvos([&mut b.min, &mut b.max].into_iter()),
            CfKind::IconSet(s) => formula_cfvos(s.cfvos.iter_mut()),
            _ => Vec::new(),
        }
    }
}

fn formula_cfvos<'a>(it: impl Iterator<Item = &'a mut Cfvo>) -> Vec<&'a mut String> {
    it.filter(|c| c.kind == CfvoType::Formula)
        .filter_map(|c| c.value.as_mut())
        .collect()
}

/// A conditional-format rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CfRule {
    /// Workbook-unique id.
    pub id: u32,
    /// Lower numbers are evaluated first.
    pub priority: u32,
    /// Stop evaluating lower-priority rules for a cell where this one applies.
    #[serde(default)]
    pub stop_if_true: bool,
    /// The rule.
    pub kind: CfKind,
}

/// One `conditionalFormatting` block: rules applied to a list of ranges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionalFormat {
    /// Ranges.
    pub ranges: Vec<Range>,
    /// Rules.
    pub rules: Vec<CfRule>,
}

// ── data validation ───────────────────────────────────────────────────────

/// Validation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum ValidationType {
    #[default]
    Any,
    Whole,
    Decimal,
    List,
    Date,
    Time,
    TextLength,
    Custom,
}

/// Error alert styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorStyle {
    /// Refuse the value.
    #[default]
    Stop,
    /// Ask (Yes keeps the value).
    Warning,
    /// Inform (OK keeps the value).
    Information,
}

/// A data-validation rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataValidation {
    /// Workbook-unique id.
    pub id: u32,
    /// Ranges.
    pub ranges: Vec<Range>,
    /// Type.
    pub kind: ValidationType,
    /// Operator (whole, decimal, date, time, text length).
    #[serde(default)]
    pub operator: Operator,
    /// First criterion without `=`: a value, a formula, or a list source (`"Yes,No"` or
    /// `$A$1:$A$5`).
    #[serde(default)]
    pub formula1: Option<String>,
    /// Second criterion (between/not between).
    #[serde(default)]
    pub formula2: Option<String>,
    /// Accept blanks.
    #[serde(default = "yes")]
    pub allow_blank: bool,
    /// Show the in-cell dropdown of a list.
    #[serde(default = "yes")]
    pub in_cell_dropdown: bool,
    /// Show the input message when the cell is selected.
    #[serde(default = "yes")]
    pub show_input_message: bool,
    /// Input message title.
    #[serde(default)]
    pub prompt_title: Option<String>,
    /// Input message.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Show the error alert for invalid data.
    #[serde(default = "yes")]
    pub show_error_message: bool,
    /// Alert style.
    #[serde(default)]
    pub error_style: ErrorStyle,
    /// Alert title.
    #[serde(default)]
    pub error_title: Option<String>,
    /// Alert message.
    #[serde(default)]
    pub error: Option<String>,
}

impl DataValidation {
    /// A rule of a type over ranges with defaults for everything else.
    pub fn new(id: u32, ranges: Vec<Range>, kind: ValidationType) -> DataValidation {
        DataValidation {
            id,
            ranges,
            kind,
            operator: Operator::Between,
            formula1: None,
            formula2: None,
            allow_blank: true,
            in_cell_dropdown: true,
            show_input_message: true,
            prompt_title: None,
            prompt: None,
            show_error_message: true,
            error_style: ErrorStyle::Stop,
            error_title: None,
            error: None,
        }
    }
}

// ── hyperlinks and comments ───────────────────────────────────────────────

/// A hyperlink.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hyperlink {
    /// The cells it covers.
    pub range: Range,
    /// External target (URL, `mailto:`, file path).
    #[serde(default)]
    pub target: Option<String>,
    /// Location inside the workbook (`Sheet2!A1` or a defined name).
    #[serde(default)]
    pub location: Option<String>,
    /// Display text.
    #[serde(default)]
    pub display: Option<String>,
    /// Screen tip.
    #[serde(default)]
    pub tooltip: Option<String>,
}

/// A note (legacy comment, the yellow sticky note).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Note {
    /// Cell.
    pub at: CellPos,
    /// Author.
    pub author: String,
    /// Text.
    pub text: String,
    /// Always shown.
    #[serde(default)]
    pub visible: bool,
    /// Box size in px when shown.
    #[serde(default)]
    pub size: Option<(u32, u32)>,
}

/// An @mention inside a threaded comment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Mention {
    /// The person mentioned.
    pub person_id: String,
    /// Start of the mention in the text (UTF-16 units, as in the file).
    pub start: u32,
    /// Length.
    pub length: u32,
}

/// A threaded (modern) comment; replies point at the first comment with `parent_id`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadedComment {
    /// Id (a GUID in files).
    pub id: String,
    /// Cell.
    pub at: CellPos,
    /// Author display name.
    pub author: String,
    /// Person id.
    #[serde(default)]
    pub author_id: Option<String>,
    /// ISO 8601 timestamp.
    pub date: String,
    /// Text.
    pub text: String,
    /// The comment this replies to.
    #[serde(default)]
    pub parent_id: Option<String>,
    /// The thread is resolved.
    #[serde(default)]
    pub done: bool,
    /// @mentions.
    #[serde(default)]
    pub mentions: Vec<Mention>,
}

/// A person who wrote threaded comments.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Person {
    /// Id.
    pub id: String,
    /// Display name.
    pub display_name: String,
    /// Account id.
    #[serde(default)]
    pub user_id: Option<String>,
    /// Identity provider.
    #[serde(default)]
    pub provider_id: Option<String>,
}

// ── views ─────────────────────────────────────────────────────────────────

/// The selection of a sheet view.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Selection {
    /// Selected ranges.
    pub ranges: Vec<Range>,
    /// The active cell.
    pub active: CellPos,
}

impl Default for Selection {
    fn default() -> Selection {
        Selection {
            ranges: vec![Range::cell(0, 0)],
            active: CellPos::new(0, 0),
        }
    }
}

/// View modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum ViewMode {
    #[default]
    Normal,
    PageBreakPreview,
    PageLayout,
}

/// Frozen rows and columns (Freeze Panes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Freeze {
    /// Rows above the split.
    pub rows: u32,
    /// Columns left of the split.
    pub cols: u32,
}

/// A sheet window's view.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SheetView {
    /// Frozen panes.
    #[serde(default)]
    pub freeze: Option<Freeze>,
    /// Unfrozen split position in px at 100 % zoom (View › Split).
    #[serde(default)]
    pub split: Option<(u32, u32)>,
    /// The top-left visible cell.
    #[serde(default)]
    pub top_left: CellPos,
    /// Zoom percent, 10–400.
    pub zoom: u16,
    /// View mode.
    #[serde(default)]
    pub mode: ViewMode,
    /// Selection.
    #[serde(default)]
    pub selection: Selection,
    /// Gridlines.
    #[serde(default = "yes")]
    pub show_gridlines: bool,
    /// Row and column headings.
    #[serde(default = "yes")]
    pub show_headings: bool,
    /// Zero values (otherwise blank).
    #[serde(default = "yes")]
    pub show_zeros: bool,
    /// Formulas instead of results.
    #[serde(default)]
    pub show_formulas: bool,
    /// Ruler in Page Layout view.
    #[serde(default = "yes")]
    pub show_ruler: bool,
    /// Right-to-left sheet.
    #[serde(default)]
    pub right_to_left: bool,
    /// Gridline colour.
    #[serde(default)]
    pub gridline_color: Option<Color>,
}

impl Default for SheetView {
    fn default() -> SheetView {
        SheetView {
            freeze: None,
            split: None,
            top_left: CellPos::new(0, 0),
            zoom: 100,
            mode: ViewMode::Normal,
            selection: Selection::default(),
            show_gridlines: true,
            show_headings: true,
            show_zeros: true,
            show_formulas: false,
            show_ruler: true,
            right_to_left: false,
            gridline_color: None,
        }
    }
}

// ── print settings ────────────────────────────────────────────────────────

/// Headers and footers (`&L&C&R` codes as stored).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct HeaderFooter {
    #[serde(default)]
    pub odd_header: Option<String>,
    #[serde(default)]
    pub odd_footer: Option<String>,
    #[serde(default)]
    pub even_header: Option<String>,
    #[serde(default)]
    pub even_footer: Option<String>,
    #[serde(default)]
    pub first_header: Option<String>,
    #[serde(default)]
    pub first_footer: Option<String>,
    #[serde(default)]
    pub different_odd_even: bool,
    #[serde(default)]
    pub different_first: bool,
    #[serde(default = "yes")]
    pub scale_with_doc: bool,
    #[serde(default = "yes")]
    pub align_with_margins: bool,
}

/// Page margins in inches.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct Margins {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
    pub header: f64,
    pub footer: f64,
}

impl Default for Margins {
    /// Excel's "Normal" margins.
    fn default() -> Margins {
        Margins {
            left: 0.7,
            right: 0.7,
            top: 0.75,
            bottom: 0.75,
            header: 0.3,
            footer: 0.3,
        }
    }
}

/// Page orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

/// How comments print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum PrintComments {
    #[default]
    None,
    AtEnd,
    AsDisplayed,
}

/// How errors print.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum PrintErrors {
    #[default]
    Displayed,
    Blank,
    Dash,
    NA,
}

/// Page setup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrintSettings {
    /// Orientation.
    #[serde(default)]
    pub orientation: Orientation,
    /// SpreadsheetML paper size code (1 Letter, 9 A4, 5 Legal…).
    pub paper_size: u32,
    /// Percent, 10–400 (when not fitting to pages).
    pub scale: u16,
    /// Fit to pages.
    #[serde(default)]
    pub fit_to_page: bool,
    /// Pages wide when fitting (0 = automatic).
    pub fit_to_width: u32,
    /// Pages tall when fitting (0 = automatic).
    pub fit_to_height: u32,
    /// Margins.
    #[serde(default)]
    pub margins: Margins,
    /// Centre horizontally.
    #[serde(default)]
    pub horizontal_centered: bool,
    /// Centre vertically.
    #[serde(default)]
    pub vertical_centered: bool,
    /// Print gridlines.
    #[serde(default)]
    pub print_gridlines: bool,
    /// Print headings.
    #[serde(default)]
    pub print_headings: bool,
    /// Down, then over (otherwise over, then down).
    #[serde(default = "yes")]
    pub down_then_over: bool,
    /// Black and white.
    #[serde(default)]
    pub black_and_white: bool,
    /// Draft quality.
    #[serde(default)]
    pub draft: bool,
    /// First page number.
    #[serde(default)]
    pub first_page_number: Option<u32>,
    /// Comments.
    #[serde(default)]
    pub comments: PrintComments,
    /// Errors.
    #[serde(default)]
    pub errors: PrintErrors,
    /// Headers and footers.
    #[serde(default)]
    pub header_footer: HeaderFooter,
    /// Manual page breaks before these rows.
    #[serde(default)]
    pub row_breaks: Vec<u32>,
    /// Manual page breaks before these columns.
    #[serde(default)]
    pub col_breaks: Vec<u32>,
}

impl Default for PrintSettings {
    fn default() -> PrintSettings {
        PrintSettings {
            orientation: Orientation::Portrait,
            paper_size: 1,
            scale: 100,
            fit_to_page: false,
            fit_to_width: 1,
            fit_to_height: 1,
            margins: Margins::default(),
            horizontal_centered: false,
            vertical_centered: false,
            print_gridlines: false,
            print_headings: false,
            down_then_over: true,
            black_and_white: false,
            draft: false,
            first_page_number: None,
            comments: PrintComments::None,
            errors: PrintErrors::Displayed,
            header_footer: HeaderFooter {
                scale_with_doc: true,
                align_with_margins: true,
                ..HeaderFooter::default()
            },
            row_breaks: Vec::new(),
            col_breaks: Vec::new(),
        }
    }
}

// ── drawings ──────────────────────────────────────────────────────────────

/// A cell-relative anchor point; offsets in EMU (914,400 per inch, 9,525 per px).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct AnchorPoint {
    pub row: u32,
    pub col: u32,
    pub row_off: i64,
    pub col_off: i64,
}

/// How a two-cell anchor behaves when cells move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditAs {
    /// Move and size with cells.
    #[default]
    TwoCell,
    /// Move but don't size.
    OneCell,
    /// Don't move or size.
    Absolute,
}

/// Where a drawing sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Anchor {
    /// Between two cells.
    TwoCell {
        /// Top-left.
        from: AnchorPoint,
        /// Bottom-right.
        to: AnchorPoint,
        /// Behaviour.
        #[serde(default)]
        edit_as: EditAs,
    },
    /// At a cell, with a size in EMU.
    OneCell {
        /// Top-left.
        from: AnchorPoint,
        /// Width and height in EMU.
        ext: (i64, i64),
    },
    /// At a position in EMU.
    Absolute {
        /// Position.
        pos: (i64, i64),
        /// Size.
        ext: (i64, i64),
    },
}

/// Kinds of drawing objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum DrawingKind {
    Chart,
    Picture,
    Shape,
    Group,
    Connector,
    Other,
}

/// A chart, picture or shape placed on a sheet; the object itself lives elsewhere (the chart
/// model, an image part) and is referenced by `reference`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DrawingAnchor {
    /// Id.
    pub id: String,
    /// Kind.
    pub kind: DrawingKind,
    /// The object.
    pub reference: String,
    /// Placement.
    pub anchor: Anchor,
    /// Name.
    #[serde(default)]
    pub name: Option<String>,
    /// Alternative text.
    #[serde(default)]
    pub alt_text: Option<String>,
    /// Hidden.
    #[serde(default)]
    pub hidden: bool,
    /// Locked.
    #[serde(default = "yes")]
    pub locked: bool,
}

// ── protection ────────────────────────────────────────────────────────────

/// A stored password: the legacy 16-bit hash, or a salted hash (kept for round trips).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PasswordHash {
    /// `password="CBEB"`.
    Legacy(u16),
    /// `algorithmName`, `hashValue`, `saltValue`, `spinCount` (base64 as in the file).
    Modern {
        /// Algorithm (`SHA-512`).
        algorithm: String,
        /// Hash, base64.
        hash: String,
        /// Salt, base64.
        salt: String,
        /// Iterations.
        spin_count: u32,
    },
}

/// Excel's legacy 16-bit password hash (ECMA-376 Part 4 §14.7.1).
pub fn legacy_password_hash(password: &str) -> u16 {
    let bytes: Vec<u8> = password
        .chars()
        .map(|c| u8::try_from(u32::from(c) & 0xFF).unwrap_or(b'?'))
        .take(15)
        .collect();
    let mut h: u16 = 0;
    for &b in bytes.iter().rev() {
        h = ((h >> 14) & 1) | ((h << 1) & 0x7FFF);
        h ^= u16::from(b);
    }
    h = ((h >> 14) & 1) | ((h << 1) & 0x7FFF);
    h ^= bytes.len() as u16;
    h ^ 0xCE4B
}

impl PasswordHash {
    /// True when `password` matches (salted hashes can only be checked by a hashing layer, so
    /// they report `None`).
    pub fn verify(&self, password: &str) -> Option<bool> {
        match self {
            PasswordHash::Legacy(h) => Some(*h == legacy_password_hash(password)),
            PasswordHash::Modern { .. } => None,
        }
    }
}

/// Sheet protection (Review › Protect Sheet): what users may still do.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct SheetProtection {
    #[serde(default)]
    pub password: Option<PasswordHash>,
    #[serde(default = "yes")]
    pub select_locked_cells: bool,
    #[serde(default = "yes")]
    pub select_unlocked_cells: bool,
    #[serde(default)]
    pub format_cells: bool,
    #[serde(default)]
    pub format_columns: bool,
    #[serde(default)]
    pub format_rows: bool,
    #[serde(default)]
    pub insert_columns: bool,
    #[serde(default)]
    pub insert_rows: bool,
    #[serde(default)]
    pub insert_hyperlinks: bool,
    #[serde(default)]
    pub delete_columns: bool,
    #[serde(default)]
    pub delete_rows: bool,
    #[serde(default)]
    pub sort: bool,
    #[serde(default)]
    pub auto_filter: bool,
    #[serde(default)]
    pub pivot_tables: bool,
    /// Objects are protected.
    #[serde(default = "yes")]
    pub objects: bool,
    /// Scenarios are protected.
    #[serde(default = "yes")]
    pub scenarios: bool,
}

impl Default for SheetProtection {
    /// Excel's defaults: select locked and unlocked cells, nothing else.
    fn default() -> SheetProtection {
        SheetProtection {
            password: None,
            select_locked_cells: true,
            select_unlocked_cells: true,
            format_cells: false,
            format_columns: false,
            format_rows: false,
            insert_columns: false,
            insert_rows: false,
            insert_hyperlinks: false,
            delete_columns: false,
            delete_rows: false,
            sort: false,
            auto_filter: false,
            pivot_tables: false,
            objects: true,
            scenarios: true,
        }
    }
}

/// Workbook protection (Review › Protect Workbook).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct WorkbookProtection {
    /// Sheets cannot be added, removed, renamed, moved or hidden.
    #[serde(default)]
    pub lock_structure: bool,
    /// Windows cannot be moved or resized.
    #[serde(default)]
    pub lock_windows: bool,
    /// Password.
    #[serde(default)]
    pub password: Option<PasswordHash>,
}

// ── workbook properties ───────────────────────────────────────────────────

/// Calculation modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CalcMode {
    /// Recalculate after every change.
    #[default]
    Auto,
    /// Automatic except data tables.
    AutoNoTable,
    /// Only on F9.
    Manual,
}

/// Calculation settings (File › Options › Formulas).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CalcProps {
    /// Mode.
    pub mode: CalcMode,
    /// Recalculate before saving (manual mode).
    pub calc_on_save: bool,
    /// Recalculate everything when the file opens.
    pub full_calc_on_load: bool,
    /// Iterative calculation.
    pub iterate: bool,
    /// Maximum iterations.
    pub iterate_count: u32,
    /// Maximum change.
    pub iterate_delta: f64,
    /// Full precision (otherwise "precision as displayed").
    pub full_precision: bool,
}

impl Default for CalcProps {
    fn default() -> CalcProps {
        CalcProps {
            mode: CalcMode::Auto,
            calc_on_save: true,
            full_calc_on_load: false,
            iterate: false,
            iterate_count: 100,
            iterate_delta: 0.001,
            full_precision: true,
        }
    }
}

/// Workbook properties.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WorkbookProps {
    /// The 1904 date system (Mac Excel legacy).
    #[serde(default)]
    pub date1904: bool,
    /// Calculation settings.
    #[serde(default)]
    pub calc: CalcProps,
    /// Active sheet index (workbook order).
    #[serde(default)]
    pub active_sheet: usize,
    /// First visible sheet tab.
    #[serde(default)]
    pub first_sheet: usize,
    /// VBA code name.
    #[serde(default)]
    pub code_name: Option<String>,
    /// Remove personal information on save.
    #[serde(default)]
    pub filter_privacy: bool,
    /// Width of the sheet tab strip, ‰ of the window.
    #[serde(default)]
    pub tab_ratio: Option<u32>,
}

/// Sheet visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SheetVisibility {
    /// Shown.
    #[default]
    Visible,
    /// Hidden (Unhide lists it).
    Hidden,
    /// Hidden from Unhide.
    VeryHidden,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_password_hash_matches_excel() {
        // Values Excel writes for these passwords.
        assert_eq!(legacy_password_hash("password"), 0x83AF);
        assert_eq!(legacy_password_hash(""), 0xCE4B);
        assert_eq!(PasswordHash::Legacy(0x83AF).verify("password"), Some(true));
        assert_eq!(PasswordHash::Legacy(0x83AF).verify("Password"), Some(false));
    }

    #[test]
    fn table_geometry() {
        let t = Table {
            id: 1,
            name: "T".into(),
            display_name: None,
            range: Range::new(0, 0, 3, 1),
            header_row: true,
            totals_row: true,
            columns: vec![TableColumn::named("A"), TableColumn::named("b")],
            style: None,
            auto_filter: None,
            comment: None,
        };
        assert_eq!(t.data_range(), Some(Range::new(1, 0, 2, 1)));
        assert_eq!(t.column_index("B"), Some(1));
        assert_eq!(TotalsFunction::Sum.subtotal_number(), Some(109));
        let def = t.to_def(SheetId(3));
        assert_eq!(def.columns, vec!["A".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn icon_sets_know_their_sizes_and_names() {
        assert_eq!(IconSetName::TrafficLights3.len(), 3);
        assert_eq!(IconSetName::Boxes5.len(), 5);
        assert_eq!(
            IconSetName::TrafficLightsRimmed3.as_str(),
            "3TrafficLights2"
        );
        let json = serde_json::to_string(&IconSetName::Quarters5).unwrap_or_default();
        assert_eq!(json, "\"5Quarters\"");
    }

    #[test]
    fn rule_formulas_are_reachable_for_rewriting() {
        let mut k = CfKind::IconSet(IconSet {
            icon_set: IconSetName::Arrows3,
            cfvos: vec![
                Cfvo::of(CfvoType::Percent),
                Cfvo::with(CfvoType::Formula, "$A$1"),
                Cfvo::with(CfvoType::Num, "5"),
            ],
            reverse: false,
            show_value: true,
            custom_icons: Vec::new(),
        });
        assert_eq!(k.formulas_mut().len(), 1);
        assert_eq!(k.dxf(), None);
    }
}
