// SPDX-License-Identifier: AGPL-3.0-or-later
//! Arguments and answers of Kiln Grid's data, formatting, review, page layout and formula
//! features: sort and AutoFilter, conditional formatting, data validation, Format as
//! Table, find and replace with Go To Special, notes, threaded comments and links, page setup
//! with print titles and breaks, defined names, formula auditing (arrows, error checking,
//! Evaluate Formula, the Watch Window), Formula Lab, imports, Text to Columns, Remove
//! Duplicates, outlines and spelling. The commands themselves are variants of
//! [`super::GridCommand`]; the questions are variants of [`super::GridQuery`].
//!
//! Colours are `RRGGBB` without `#`; rows and columns are zero-based and absolute; references
//! in text (`applies_to`, `refers_to`, print areas) are A1 text as Excel shows it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::GridRange;

// ── sort and filter ──────────────────────────────────────────────────────────────────────────

/// What a sort level compares (Data ▸ Sort ▸ Sort On).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridSortOn {
    /// Cell values.
    #[default]
    Values,
    /// Cell colour (the given colour on top).
    CellColor,
    /// Font colour (the given colour on top).
    FontColor,
    /// Conditional-formatting icon (the given icon on top).
    Icon,
}

/// One level of the Sort dialog.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridSortKey {
    /// The column sorted by (the row, sorting left to right), absolute.
    pub index: u32,
    /// Z to A, largest to smallest, or the colour at the bottom.
    pub descending: bool,
    /// What is compared.
    pub on: GridSortOn,
    /// The colour of a colour sort.
    pub color: Option<String>,
    /// The icon index of an icon sort (0 = the set's lowest).
    pub icon: Option<u32>,
    /// A custom list order (`Low, Medium, High`).
    pub custom_list: Option<Vec<String>>,
}

/// The Sort dialog.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridSortSpec {
    /// The range (`None`: the table or filter at the active cell, else its current region).
    pub range: Option<GridRange>,
    /// Levels, first first.
    pub keys: Vec<GridSortKey>,
    /// The first row (column) holds headers; `None` detects it as Excel does.
    pub header: Option<bool>,
    /// Case sensitive.
    pub case_sensitive: bool,
    /// Sort left to right.
    pub by_columns: bool,
}

/// The range a data command works on (the Sort and Remove Duplicates dialogs).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridDataRange {
    /// The range: the selection, or the table, filter or current region at the active cell.
    pub range: GridRange,
    /// The first row's texts (column headers when `header`).
    pub headers: Vec<String>,
    /// The first row looks like headers.
    pub header: bool,
}

/// Comparisons of custom AutoFilters (Text Filters, Number Filters, Date Filters).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridFilterOp {
    /// Equals.
    Equal,
    /// Does not equal.
    NotEqual,
    /// Is greater than (dates: is after).
    GreaterThan,
    /// Is greater than or equal to.
    GreaterThanOrEqual,
    /// Is less than (dates: is before).
    LessThan,
    /// Is less than or equal to.
    LessThanOrEqual,
    /// Begins with.
    BeginsWith,
    /// Ends with.
    EndsWith,
    /// Contains.
    Contains,
    /// Does not contain.
    NotContains,
}

/// One criterion of a custom AutoFilter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridCriterion {
    /// Comparison.
    pub op: GridFilterOp,
    /// Value as typed (`*` and `?` are wildcards; dates as the cells show them or ISO).
    pub value: String,
}

/// Dynamic AutoFilters: averages and dates relative to today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[allow(missing_docs)]
pub enum GridDynamicFilter {
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
}

/// The filter of one AutoFilter column.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridColumnFilter {
    /// The checked values of the dropdown list (displayed text).
    Values {
        /// The values shown.
        values: Vec<String>,
        /// Show blank cells.
        blanks: bool,
    },
    /// One or two comparisons.
    Custom {
        /// Both must hold (otherwise either).
        and: bool,
        /// One or two criteria.
        criteria: Vec<GridCriterion>,
    },
    /// Top 10: the top or bottom N items or percent.
    Top10 {
        /// Top (otherwise bottom).
        top: bool,
        /// N is a percentage.
        percent: bool,
        /// N.
        value: u32,
    },
    /// A dynamic filter.
    Dynamic {
        /// Which.
        kind: GridDynamicFilter,
    },
    /// By fill or font colour.
    Color {
        /// The colour.
        color: String,
        /// Font colour (otherwise fill colour).
        font: bool,
    },
}

/// The kind of values in a filtered column (which criteria menu the dropdown shows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridValueKind {
    /// Text Filters.
    Text,
    /// Number Filters.
    Number,
    /// Date Filters.
    Date,
}

/// One value of an AutoFilter dropdown list.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridFilterItem {
    /// Displayed text.
    pub text: String,
    /// Checked (shown by the current filter).
    pub checked: bool,
    /// The day of a date value (`2026-01-05`): the menu groups dates by year and month.
    #[serde(default)]
    pub date: Option<String>,
}

/// What an AutoFilter dropdown shows ([`super::GridQuery::FilterMenu`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridFilterMenu {
    /// The header row of the filter.
    pub row: u32,
    /// The column.
    pub col: u32,
    /// The column's header text.
    pub header: String,
    /// The kind of values (most common in the column).
    pub kind: GridValueKind,
    /// Distinct displayed values, sorted.
    pub items: Vec<GridFilterItem>,
    /// The column has blank cells: whether they are shown (`None`: none).
    pub blanks: Option<bool>,
    /// The column's current filter.
    pub filter: Option<GridColumnFilter>,
    /// Fill colours used in the column (Filter by Colour).
    pub fill_colors: Vec<String>,
    /// Font colours used in the column other than automatic.
    pub font_colors: Vec<String>,
    /// The column is the remembered sort's first level: `Some(true)` descending.
    pub sorted: Option<bool>,
    /// More distinct values than the list holds (10,000).
    pub truncated: bool,
}

// ── conditional formatting ───────────────────────────────────────────────────────────────────

/// Comparison operators of cell-value rules and validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[allow(missing_docs)]
pub enum GridCompare {
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

/// Text rules (Highlight Cells Rules ▸ Text that Contains…).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[allow(missing_docs)]
pub enum GridTextRule {
    Contains,
    NotContains,
    BeginsWith,
    EndsWith,
}

/// "A Date Occurring" periods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[allow(missing_docs)]
pub enum GridDatePeriod {
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

/// The kind of a colour scale, data bar or icon set threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridCfValueKind {
    /// Lowest value.
    Min,
    /// Highest value.
    Max,
    /// A number.
    Number,
    /// A percentage of the range.
    Percent,
    /// A percentile.
    Percentile,
    /// A formula.
    Formula,
    /// Automatic (data bars: zero or the lowest/highest value).
    Automatic,
}

/// A threshold of a colour scale, data bar or icon set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridCfValue {
    /// Kind.
    pub kind: GridCfValueKind,
    /// The number, percentage or formula (empty for Min, Max and Automatic).
    pub value: String,
    /// Icon sets: `>=` (otherwise `>`).
    pub gte: bool,
}

/// The format a highlight rule applies.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub struct GridCfStyle {
    /// Fill colour.
    pub fill: Option<String>,
    /// Font colour.
    pub font_color: Option<String>,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underline.
    pub underline: bool,
    /// Strikethrough.
    pub strike: bool,
    /// Outline border colour.
    pub border: Option<String>,
    /// Number format code.
    pub number_format: Option<String>,
}

/// What a conditional-formatting rule tests and shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridCfRuleKind {
    /// Format only cells with a value compared with one or two values or formulas.
    CellValue {
        /// Operator.
        op: GridCompare,
        /// One or two values (`5`, `=$B$1`, `"text"`).
        values: Vec<String>,
    },
    /// Specific text.
    Text {
        /// The test.
        rule: GridTextRule,
        /// The text.
        text: String,
    },
    /// A date occurring.
    DateOccurring {
        /// The period.
        period: GridDatePeriod,
    },
    /// Duplicate or unique values.
    Duplicates {
        /// Unique instead of duplicate.
        unique: bool,
    },
    /// Top or bottom N items or percent.
    TopBottom {
        /// Bottom.
        bottom: bool,
        /// N is a percentage.
        percent: bool,
        /// N.
        rank: u32,
    },
    /// Above or below the average.
    Average {
        /// Below.
        below: bool,
        /// Equal values count.
        equal: bool,
        /// Standard deviations (0 = the plain average, 1–3).
        std_dev: u8,
    },
    /// Blank cells (or not).
    Blanks {
        /// No blanks instead.
        not: bool,
    },
    /// Errors (or not).
    Errors {
        /// No errors instead.
        not: bool,
    },
    /// A formula true for the cells to format (relative to the top-left cell).
    Formula {
        /// The formula, with or without `=`.
        formula: String,
    },
    /// A two- or three-colour scale.
    ColorScale {
        /// Thresholds, lowest first.
        points: Vec<GridCfValue>,
        /// One colour per threshold.
        colors: Vec<String>,
    },
    /// A data bar.
    DataBar {
        /// Shortest bar.
        min: GridCfValue,
        /// Longest bar.
        max: GridCfValue,
        /// Bar colour.
        color: String,
        /// Gradient fill (otherwise solid).
        gradient: bool,
        /// Show the cell value too.
        show_value: bool,
        /// Colour of negative bars.
        negative_color: Option<String>,
        /// Bar border colour.
        border_color: Option<String>,
    },
    /// An icon set.
    IconSet {
        /// Excel's set name (`3TrafficLights1`, `5Arrows`…).
        set: String,
        /// Thresholds of the icons after the first (one fewer than the icons), lowest first.
        thresholds: Vec<GridCfValue>,
        /// Reverse icon order.
        reverse: bool,
        /// Show the cell value too.
        show_value: bool,
    },
}

/// A conditional-formatting rule (New Rule, Edit Rule, a row of Manage Rules).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridCfRule {
    /// The rule's id (0 for a new rule).
    pub id: u32,
    /// The ranges it applies to (`$A$1:$A$20 $C$1:$C$20`; empty: the selection).
    pub applies_to: String,
    /// What it tests and shows.
    pub kind: GridCfRuleKind,
    /// The format of highlight rules (`None` for scales, bars and icons).
    pub style: Option<GridCfStyle>,
    /// Stop If True.
    pub stop_if_true: bool,
}

// ── data validation ──────────────────────────────────────────────────────────────────────────

/// What a validation rule allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[allow(missing_docs)]
pub enum GridValidationKind {
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

/// The alert of invalid entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[allow(missing_docs)]
pub enum GridErrorStyle {
    #[default]
    Stop,
    Warning,
    Information,
}

/// The Data Validation dialog.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub struct GridValidation {
    /// Allow.
    pub kind: GridValidationKind,
    /// Data.
    pub op: GridCompare,
    /// Source (lists: `Yes,No` or `=$A$1:$A$5`), minimum, value or formula.
    pub formula1: String,
    /// Maximum (between, not between).
    pub formula2: String,
    /// Ignore blank.
    pub allow_blank: bool,
    /// In-cell dropdown.
    pub dropdown: bool,
    /// Show the input message when the cell is selected.
    pub show_input: bool,
    /// Input message title.
    pub input_title: String,
    /// Input message.
    pub input: String,
    /// Show the error alert after invalid data is entered.
    pub show_error: bool,
    /// Alert style.
    pub style: GridErrorStyle,
    /// Alert title.
    pub error_title: String,
    /// Alert message.
    pub error: String,
}

// ── tables ───────────────────────────────────────────────────────────────────────────────────

/// Table Style Options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub struct GridTableOptions {
    /// Header row.
    pub header_row: bool,
    /// Total row.
    pub total_row: bool,
    /// Banded rows.
    pub banded_rows: bool,
    /// Banded columns.
    pub banded_columns: bool,
    /// First column.
    pub first_column: bool,
    /// Last column.
    pub last_column: bool,
    /// Filter button.
    pub filter_button: bool,
}

/// Total row functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[allow(missing_docs)]
pub enum GridTotals {
    #[default]
    None,
    Sum,
    Average,
    Count,
    CountNumbers,
    Max,
    Min,
    StdDev,
    Var,
}

/// A table column.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridTableColumn {
    /// Header.
    pub name: String,
    /// Total row function.
    pub totals: GridTotals,
    /// The calculated column formula.
    pub formula: Option<String>,
}

/// The table at the active cell (the Table Design tab).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridTableInfo {
    /// Table name.
    pub name: String,
    /// Its range, header and total rows included.
    pub range: GridRange,
    /// Its A1 text (`$A$1:$D$20`).
    pub reference: String,
    /// Style name (`TableStyleMedium2`).
    pub style: String,
    /// Options.
    pub options: GridTableOptions,
    /// Columns.
    pub columns: Vec<GridTableColumn>,
    /// The active cell's column in the table.
    pub column: u32,
}

/// One table style of the gallery: name and the colours of its swatch.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridTableStyle {
    /// Style name.
    pub name: String,
    /// Light, Medium or Dark.
    pub family: String,
    /// Header row fill (or `None`).
    pub header: Option<String>,
    /// Header text colour.
    pub header_text: Option<String>,
    /// Fill of the odd bands.
    pub band: Option<String>,
    /// Line colour.
    pub line: Option<String>,
}

/// One cell style of the gallery.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridCellStyleInfo {
    /// Name (`Good`, `Heading 1`, `Currency`).
    pub name: String,
    /// The gallery group (`Good, Bad and Neutral`, `Titles and Headings`, `Number Format`…).
    pub group: String,
    /// Fill colour of the swatch.
    pub fill: Option<String>,
    /// Text colour of the swatch.
    pub color: Option<String>,
    /// Bold.
    pub bold: bool,
    /// Bottom border colour of the swatch.
    pub border: Option<String>,
}

// ── find and replace ─────────────────────────────────────────────────────────────────────────

/// Look in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridLookIn {
    /// Formulas (and constants as typed).
    #[default]
    Formulas,
    /// Values as displayed.
    Values,
    /// Notes and comments.
    Notes,
}

/// Find and Replace options.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub struct GridFindSpec {
    /// Find what (`*` and `?` are wildcards, `~` escapes them).
    pub text: String,
    /// Match case.
    pub match_case: bool,
    /// Match entire cell contents.
    pub whole_cell: bool,
    /// Look in.
    pub look_in: GridLookIn,
    /// Within the workbook (otherwise the sheet).
    pub workbook: bool,
    /// Search by columns (otherwise by rows).
    pub by_columns: bool,
}

/// One cell of Find All.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridFindHit {
    /// Sheet index.
    pub sheet: u32,
    /// Sheet name.
    pub sheet_name: String,
    /// Row.
    pub row: u32,
    /// Column.
    pub col: u32,
    /// `$B$7`.
    pub cell: String,
    /// The displayed value.
    pub value: String,
    /// The formula, for formula cells.
    pub formula: Option<String>,
}

/// Go To Special.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[allow(missing_docs)]
pub enum GridSpecial {
    Notes,
    Constants,
    Formulas,
    Blanks,
    CurrentRegion,
    CurrentArray,
    LastCell,
    VisibleCells,
    Errors,
    ConditionalFormats,
    DataValidation,
    /// The cells the selection's formulas refer to directly (Ctrl+[).
    Precedents,
    /// The formulas that refer directly to the selection (Ctrl+]).
    Dependents,
}

// ── notes, comments, links ───────────────────────────────────────────────────────────────────

/// A note or a threaded comment (the Comments pane, the card at a cell).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridCommentInfo {
    /// Comment id (empty for a note).
    pub id: String,
    /// Sheet index.
    pub sheet: u32,
    /// Row.
    pub row: u32,
    /// Column.
    pub col: u32,
    /// `B7`.
    pub cell: String,
    /// Author.
    pub author: String,
    /// ISO date (comments).
    pub date: String,
    /// Text.
    pub text: String,
    /// The comment replied to.
    pub parent: Option<String>,
    /// Resolved.
    pub done: bool,
    /// A legacy note (otherwise a threaded comment).
    pub note: bool,
    /// A note kept visible (Show/Hide Note).
    pub visible: bool,
}

/// A hyperlink.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub struct GridLink {
    /// Web address, file or `mailto:` (empty for a place in the workbook).
    pub target: String,
    /// A place in this workbook (`'Sheet 2'!A1`, a defined name).
    pub location: String,
    /// Text to display.
    pub display: String,
    /// ScreenTip.
    pub tooltip: String,
}

/// A note or comment card shown over the sheet (Show All Notes, hovering the active cell).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GridNoteCard {
    /// Row.
    pub row: u32,
    /// Column.
    pub col: u32,
    /// Author.
    pub author: String,
    /// Text (a comment thread's first text).
    pub text: String,
    /// The cell's top-right corner, surface CSS pixels.
    pub x: f32,
    /// Its top.
    pub y: f32,
    /// A threaded comment (otherwise a note).
    pub thread: bool,
    /// Replies of a thread.
    pub replies: u32,
}

// ── page layout ──────────────────────────────────────────────────────────────────────────────

/// Page margins in inches, as Excel's Page Setup shows them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct GridMargins {
    /// Left.
    pub left: f64,
    /// Right.
    pub right: f64,
    /// Top.
    pub top: f64,
    /// Bottom.
    pub bottom: f64,
    /// Header.
    pub header: f64,
    /// Footer.
    pub footer: f64,
}

/// Page Setup: every field optional, so the ribbon's menus change one thing.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct GridPageSetup {
    /// Landscape.
    pub landscape: Option<bool>,
    /// Excel's paper code (1 Letter, 5 Legal, 8 A3, 9 A4, 11 A5).
    pub paper: Option<u32>,
    /// Margins.
    pub margins: Option<GridMargins>,
    /// Adjust to N % (10–400).
    pub scale: Option<u16>,
    /// Fit to N pages wide (0 = automatic); setting it or `fit_height` turns on fit to page.
    pub fit_width: Option<u32>,
    /// Fit to N pages tall (0 = automatic).
    pub fit_height: Option<u32>,
    /// Fit to page (otherwise the scale applies).
    pub fit: Option<bool>,
    /// Print gridlines.
    pub gridlines: Option<bool>,
    /// Print row and column headings.
    pub headings: Option<bool>,
    /// Centre on page horizontally.
    pub center_h: Option<bool>,
    /// Centre on page vertically.
    pub center_v: Option<bool>,
    /// Header (Excel's codes: `&L&D&C&A&R&P of &N`).
    pub header: Option<String>,
    /// Footer.
    pub footer: Option<String>,
    /// First page number (0: automatic).
    pub first_page: Option<u32>,
    /// Page order over then down (otherwise down then over).
    pub over_then_down: Option<bool>,
    /// Black and white.
    pub black_and_white: Option<bool>,
    /// Print area (`$A$1:$F$40`; empty clears it).
    pub print_area: Option<String>,
    /// Rows to repeat at top (`$1:$2`; empty clears).
    pub title_rows: Option<String>,
    /// Columns to repeat at left (`$A:$A`; empty clears).
    pub title_cols: Option<String>,
}

/// The sheet's whole page setup (the Page Setup dialog and the ribbon's state).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GridPageInfo {
    /// Landscape.
    pub landscape: bool,
    /// Paper code.
    pub paper: u32,
    /// Margins.
    pub margins: GridMargins,
    /// Scale (%).
    pub scale: u16,
    /// Fit to page.
    pub fit: bool,
    /// Pages wide (0 automatic).
    pub fit_width: u32,
    /// Pages tall (0 automatic).
    pub fit_height: u32,
    /// Print gridlines.
    pub gridlines: bool,
    /// Print headings.
    pub headings: bool,
    /// Centre horizontally.
    pub center_h: bool,
    /// Centre vertically.
    pub center_v: bool,
    /// Header.
    pub header: String,
    /// Footer.
    pub footer: String,
    /// First page number.
    pub first_page: Option<u32>,
    /// Over then down.
    pub over_then_down: bool,
    /// Black and white.
    pub black_and_white: bool,
    /// Print area.
    pub print_area: String,
    /// Rows to repeat.
    pub title_rows: String,
    /// Columns to repeat.
    pub title_cols: String,
    /// Pages the sheet prints on with these settings.
    pub pages: u32,
    /// Manual row breaks (a break before each row).
    pub row_breaks: Vec<u32>,
    /// Manual column breaks.
    pub col_breaks: Vec<u32>,
    /// Header and footer, as each page shows them (page 1).
    pub header_preview: [String; 3],
    /// Footer of page 1 (left, centre, right).
    pub footer_preview: [String; 3],
}

/// Page Layout ▸ Breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridBreakAction {
    /// Insert a break above and left of the active cell.
    Insert,
    /// Remove the breaks at the active cell.
    Remove,
    /// Reset all page breaks.
    Reset,
}

/// Page Layout ▸ Print Area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridPrintAreaAction {
    /// Set to the selection.
    Set,
    /// Add the selection.
    Add,
    /// Clear.
    Clear,
}

// ── names and formulas ───────────────────────────────────────────────────────────────────────

/// A defined name (Name Manager).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridNameInfo {
    /// Name.
    pub name: String,
    /// Refers to (`=Sheet1!$A$1:$A$10`).
    pub refers_to: String,
    /// Sheet index of a sheet-scoped name (`None`: workbook).
    pub scope: Option<u32>,
    /// Comment.
    pub comment: String,
    /// Its value as the Name Manager shows it (`{"1";"2"}`, `12`, `#REF!`).
    pub value: String,
    /// A table name (read-only in the Name Manager).
    pub table: bool,
}

/// Calculation Options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridCalcMode {
    /// Automatic.
    Automatic,
    /// Automatic except for data tables.
    AutomaticExceptTables,
    /// Manual (F9 calculates).
    Manual,
}

/// One node of a formula's evaluation (Formula Lab, Evaluate Formula).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridTraceNode {
    /// `call`, `operator`, `reference`, `name`, `literal`, `array`, `group`…
    pub kind: String,
    /// The function, operator, reference or literal.
    pub label: String,
    /// The sub-expression's text.
    pub text: String,
    /// Its start in the formula (UTF-16 units, the `=` is at 0).
    pub start: u32,
    /// Its end.
    pub end: u32,
    /// The value it produced (`#N/A`, `{1;2;3}`, `"x"`).
    pub value: String,
    /// It was evaluated (the untaken branch of `IF` is not).
    pub evaluated: bool,
    /// Address text when it evaluated to a reference.
    pub reference: Option<String>,
    /// The function's one-line summary.
    pub summary: Option<String>,
    /// Sub-expressions in source order.
    pub children: Vec<GridTraceNode>,
}

/// One step of Evaluate Formula: the formula with the evaluated parts replaced, and the part
/// evaluated next underlined.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridEvalStep {
    /// The formula text at this step.
    pub text: String,
    /// Start of the part evaluated next (UTF-16 units of `text`).
    pub start: u32,
    /// Its end.
    pub end: u32,
    /// What it evaluates to.
    pub value: String,
}

/// The evaluation of the active cell's formula ([`super::GridQuery::FormulaTrace`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridFormulaTrace {
    /// `Sheet1!$B$4`.
    pub cell: String,
    /// The formula (`=IF(SUM(A1:A3)>5,"x","y")`), empty for a constant.
    pub formula: String,
    /// The cell's value.
    pub value: String,
    /// The evaluation tree.
    pub root: Option<GridTraceNode>,
    /// Evaluate Formula's steps, first first; the last shows the result.
    pub steps: Vec<GridEvalStep>,
    /// Why there is no tree (a constant, a formula the engine cannot read).
    pub message: Option<String>,
}

/// A cell in the Watch Window.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridWatch {
    /// Sheet name.
    pub sheet: String,
    /// `B7`.
    pub cell: String,
    /// Sheet index.
    pub sheet_index: u32,
    /// Row.
    pub row: u32,
    /// Column.
    pub col: u32,
    /// Defined name that is exactly this cell.
    pub name: String,
    /// Displayed value.
    pub value: String,
    /// Formula.
    pub formula: String,
}

/// An error cell for Error Checking.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridErrorInfo {
    /// Row.
    pub row: u32,
    /// Column.
    pub col: u32,
    /// `B7`.
    pub cell: String,
    /// The formula.
    pub formula: String,
    /// The error (`#DIV/0!`).
    pub error: String,
    /// What the error means (`Divide by Zero Error`).
    pub title: String,
    /// Explanation.
    pub help: String,
}

/// Remove Arrows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridArrows {
    /// Every arrow.
    All,
    /// Precedent arrows.
    Precedents,
    /// Dependent arrows.
    Dependents,
}

// ── data tools ───────────────────────────────────────────────────────────────────────────────

/// The file an import reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridImportFormat {
    /// Delimited text (CSV, TSV, any delimiter).
    #[default]
    Text,
    /// JSON (an array of objects or arrays).
    Json,
}

/// From Text/CSV and From JSON.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub struct GridImport {
    /// The file's bytes.
    pub bytes: Vec<u8>,
    /// File name (the new sheet's name).
    pub name: String,
    /// Format.
    pub format: GridImportFormat,
    /// Delimiter (`None`: detected).
    pub delimiter: Option<String>,
    /// Encoding name (`None`: detected; `utf-8`, `windows-1252`, `utf-16le`…).
    pub encoding: Option<String>,
    /// Treat everything as text (no type detection).
    pub as_text: bool,
    /// Put the data at the active cell (otherwise on a new sheet).
    pub at_cell: bool,
    /// Numbers use a decimal comma (`1.234,56`): the file's locale.
    pub decimal_comma: bool,
}

/// The formats a range or sheet exports to (Data ▸ Export).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridExportFormat {
    /// Comma-separated values (UTF-8 with a byte order mark, as Excel's CSV UTF-8).
    Csv,
    /// Tab-separated values.
    Tsv,
    /// A JSON array of records (the first row names the keys).
    Json,
}

/// An exported file ([`super::GridQuery::Export`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridExportFile {
    /// A file name (`Budget.csv`).
    pub name: String,
    /// MIME type.
    pub mime: String,
    /// The bytes.
    pub bytes: Vec<u8>,
}

/// What an import will produce (the preview of the import dialog).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridImportPreview {
    /// The first rows as displayed (at most 50 × 50).
    pub rows: Vec<Vec<String>>,
    /// Rows in the file.
    pub total_rows: u32,
    /// Columns.
    pub total_cols: u32,
    /// The encoding used.
    pub encoding: String,
    /// The delimiter used (empty for JSON).
    pub delimiter: String,
    /// What the reader reported (cut cells, too many columns).
    pub warnings: Vec<String>,
}

/// Text to Columns.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub struct GridTextToColumns {
    /// Delimited (otherwise fixed width).
    pub delimited: bool,
    /// Delimiter characters (`\t,;` and space).
    pub delimiters: String,
    /// Treat consecutive delimiters as one.
    pub consecutive: bool,
    /// Text qualifier (`"`, `'` or empty).
    pub qualifier: String,
    /// Fixed width: break positions in characters.
    pub widths: Vec<u32>,
    /// Columns imported as text (0-based in the result).
    pub text_columns: Vec<u32>,
    /// Columns skipped.
    pub skip_columns: Vec<u32>,
}

/// One misspelled word in the cells (Review ▸ Spelling).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridSpellingHit {
    /// Sheet index.
    pub sheet: u32,
    /// Row.
    pub row: u32,
    /// Column.
    pub col: u32,
    /// The word.
    pub word: String,
    /// Its offset in the cell text, in characters.
    pub offset: u32,
    /// The cell text.
    pub text: String,
    /// Suggestions, best first.
    pub suggestions: Vec<String>,
}

// ── Function Arguments ──────────────────────────────────────────────────────────────────────

/// An argument of a function.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct GridFunctionArg {
    /// Its name (`rate`).
    pub name: String,
    /// What it is.
    pub description: String,
    /// It may be left out.
    pub optional: bool,
    /// It repeats (`number1, number2, …`).
    pub repeat: bool,
}

/// A function for the Function Arguments dialog.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct GridFunctionDoc {
    /// Name.
    pub name: String,
    /// Category (`Financial`).
    pub category: String,
    /// The signature.
    pub syntax: String,
    /// One sentence.
    pub summary: String,
    /// Arguments in order.
    pub args: Vec<GridFunctionArg>,
}

// ── pictures and shapes ──────────────────────────────────────────────────────────────────────

/// What a drawing object on a sheet is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridDrawingKind {
    /// A chart.
    #[default]
    Chart,
    /// A picture.
    Picture,
    /// A shape (it can hold text).
    Shape,
    /// A line or connector.
    Line,
}

/// How an object follows the cells under it (Format ▸ Properties).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridPlacement {
    /// Move and size with cells.
    #[default]
    MoveAndSize,
    /// Move but don't size with cells.
    Move,
    /// Don't move or size with cells.
    Fixed,
}

/// Bring Forward, Send Backward and the like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridArrange {
    /// Bring Forward.
    Forward,
    /// Bring to Front.
    Front,
    /// Send Backward.
    Backward,
    /// Send to Back.
    Back,
}

/// A change to the selected picture or shape (each `None` field is kept). Sizes are sheet pixels
/// at 100 %, weights and text sizes points, crops percentages of the picture's sides.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct GridObjectFormat {
    /// Name (the name box shows it).
    pub name: Option<String>,
    /// Alternative text (empty for none).
    pub alt_text: Option<String>,
    /// Width.
    pub width: Option<f32>,
    /// Height.
    pub height: Option<f32>,
    /// Keep the proportions when one side changes.
    pub lock_aspect: Option<bool>,
    /// Crop: left, top, right and bottom, percentages.
    pub crop: Option<[f32; 4]>,
    /// How it follows the cells.
    pub placement: Option<GridPlacement>,
    /// A shape's geometry (`rect`, `ellipse`).
    pub preset: Option<String>,
    /// Fill (`RRGGBB`, `none`, or a theme slot `scheme:accent1`).
    pub fill: Option<String>,
    /// Outline colour, same forms.
    pub line: Option<String>,
    /// Outline weight.
    pub line_width: Option<f32>,
    /// A shape's text.
    pub text: Option<String>,
    /// Its colour.
    pub text_color: Option<String>,
    /// Its size.
    pub text_size: Option<f32>,
}

/// The selected picture or shape for the Picture Format and Shape Format tabs.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct GridObjectInfo {
    /// A shape's geometry.
    pub preset: String,
    /// Fill (`RRGGBB`, `none`, `scheme:accent1`; empty when not set).
    pub fill: String,
    /// Outline colour.
    pub line: String,
    /// Outline weight in points.
    pub line_width: f32,
    /// A shape's text.
    pub text: String,
    /// Its colour.
    pub text_color: String,
    /// Its size in points.
    pub text_size: f32,
    /// A picture's crop, percentages (left, top, right, bottom).
    pub crop: [f32; 4],
    /// Proportions kept.
    pub lock_aspect: bool,
    /// Alternative text.
    pub alt_text: String,
    /// How it follows the cells.
    pub placement: GridPlacement,
    /// A picture's own size in pixels (0 when not known).
    pub natural_width: f32,
    /// Height.
    pub natural_height: f32,
}

// ── state ────────────────────────────────────────────────────────────────────────────────────

/// What the Grid chrome shows of the features (part of [`super::GridState`]).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct GridFeatureState {
    /// The sheet has an AutoFilter.
    pub filter: bool,
    /// Some column of it filters rows.
    pub filtered: bool,
    /// The table at the active cell.
    pub table: Option<GridTableInfo>,
    /// The active cell's hyperlink.
    pub link: Option<GridLink>,
    /// The active cell has a note.
    pub note: bool,
    /// The active cell has a threaded comment.
    pub comment: bool,
    /// The workbook has notes or comments.
    pub any_comments: bool,
    /// Cards to show (notes kept visible, the active cell's note or comment).
    pub cards: Vec<GridNoteCard>,
    /// The active cell's validation input message (title, text).
    pub prompt: Option<[String; 2]>,
    /// An entry the active cell's validation refused: the alert to show (style, title, message).
    /// Stop offers Retry and Cancel; Warning and Information ask whether to keep the entry
    /// ([`super::GridCommand::AcceptInvalid`]).
    #[serde(default)]
    pub alert: Option<(GridErrorStyle, String, String)>,
    /// The active cell has an in-cell dropdown list.
    pub dropdown: bool,
    /// Calculation mode.
    pub calc: Option<GridCalcMode>,
    /// Formulas wait for F9 (manual calculation after an edit).
    pub calc_pending: bool,
    /// Iterative calculation: on, maximum iterations, maximum change.
    #[serde(default)]
    pub iterative: (bool, u32, f64),
    /// The workbook uses the 1904 date system (Options ▸ Advanced).
    #[serde(default)]
    pub date1904: bool,
    /// The cells in circular references (`Sheet1!A1`), first on the active sheet.
    #[serde(default)]
    pub circular: Vec<String>,
    /// Trace arrows are shown.
    pub arrows: bool,
    /// Invalid data is circled.
    pub circles: bool,
    /// Pages the active sheet prints on.
    pub pages: u32,
    /// Print gridlines.
    pub print_gridlines: bool,
    /// Print headings.
    pub print_headings: bool,
    /// Landscape.
    pub landscape: bool,
    /// The sheet has a print area.
    pub print_area: bool,
    /// Outline symbols shown.
    pub outline_symbols: bool,
    /// Formula Lab follows the active cell.
    pub lab: bool,
    /// The cells of the Watch Window.
    pub watches: Vec<GridWatch>,
    /// The result of the last data command (`3 duplicate values found and removed`).
    pub message: Option<String>,
    /// The active cell on the surface: x, y, width, height in CSS pixels (cards and the input
    /// message sit beside it).
    pub cell_rect: [f32; 4],
}

#[cfg(any(test, feature = "schema"))]
pub(crate) mod samples {
    //! Representative values (see `crate::samples`).

    use super::*;
    use crate::commands::Command;
    use crate::commands::grid::GridCommand as C;
    use crate::samples::Samples;

    fn range() -> GridRange {
        GridRange {
            r0: 0,
            c0: 0,
            r1: 20,
            c1: 3,
        }
    }

    pub(crate) fn function_doc() -> GridFunctionDoc {
        GridFunctionDoc {
            name: "PMT".into(),
            category: "Financial".into(),
            syntax: "PMT(rate, nper, pv, [fv], [type])".into(),
            summary: "Returns the periodic payment for a loan.".into(),
            args: vec![GridFunctionArg {
                name: "rate".into(),
                description: "The interest rate per period.".into(),
                optional: false,
                repeat: false,
            }],
        }
    }

    pub(crate) fn object_info() -> GridObjectInfo {
        GridObjectInfo {
            preset: "roundRect".into(),
            fill: "C2481C".into(),
            line: "none".into(),
            line_width: 1.0,
            text: "Firing".into(),
            text_color: "FFFFFF".into(),
            text_size: 11.0,
            crop: [0.0; 4],
            lock_aspect: false,
            alt_text: "A kiln".into(),
            placement: GridPlacement::Move,
            natural_width: 0.0,
            natural_height: 0.0,
        }
    }

    pub(crate) fn cf_rule() -> GridCfRule {
        GridCfRule {
            id: 0,
            applies_to: "$C$2:$C$20".into(),
            kind: GridCfRuleKind::DataBar {
                min: GridCfValue {
                    kind: GridCfValueKind::Automatic,
                    value: String::new(),
                    gte: true,
                },
                max: GridCfValue {
                    kind: GridCfValueKind::Percent,
                    value: "90".into(),
                    gte: true,
                },
                color: "638EC6".into(),
                gradient: true,
                show_value: true,
                negative_color: Some("FF0000".into()),
                border_color: None,
            },
            style: None,
            stop_if_true: false,
        }
    }

    pub(crate) fn validation() -> GridValidation {
        GridValidation {
            kind: GridValidationKind::List,
            formula1: "Low,Medium,High".into(),
            allow_blank: true,
            dropdown: true,
            show_input: true,
            input_title: "Priority".into(),
            input: "Pick one".into(),
            show_error: true,
            style: GridErrorStyle::Warning,
            ..GridValidation::default()
        }
    }

    pub(crate) fn table() -> GridTableInfo {
        GridTableInfo {
            name: "Budget".into(),
            range: range(),
            reference: "$A$1:$D$21".into(),
            style: "TableStyleMedium2".into(),
            options: GridTableOptions {
                header_row: true,
                total_row: true,
                banded_rows: true,
                filter_button: true,
                ..GridTableOptions::default()
            },
            columns: vec![GridTableColumn {
                name: "Q1".into(),
                totals: GridTotals::Sum,
                formula: Some("=[@Q1]*2".into()),
            }],
            column: 0,
        }
    }

    pub(crate) fn margins() -> GridMargins {
        GridMargins {
            left: 0.7,
            right: 0.7,
            top: 0.75,
            bottom: 0.75,
            header: 0.3,
            footer: 0.3,
        }
    }

    pub(crate) fn page_info() -> GridPageInfo {
        GridPageInfo {
            landscape: true,
            paper: 9,
            margins: margins(),
            scale: 100,
            fit: true,
            fit_width: 1,
            fit_height: 0,
            gridlines: false,
            headings: false,
            center_h: true,
            center_v: false,
            header: "&L&D&C&A&RPage &P of &N".into(),
            footer: String::new(),
            first_page: None,
            over_then_down: false,
            black_and_white: false,
            print_area: "$A$1:$F$40".into(),
            title_rows: "$1:$1".into(),
            title_cols: String::new(),
            pages: 3,
            row_breaks: vec![20],
            col_breaks: Vec::new(),
            header_preview: ["18/10/2026".into(), "Budget".into(), "Page 1 of 3".into()],
            footer_preview: [String::new(), String::new(), String::new()],
        }
    }

    pub(crate) fn trace() -> GridFormulaTrace {
        let leaf = GridTraceNode {
            kind: "reference".into(),
            label: "A1:A3".into(),
            text: "A1:A3".into(),
            start: 8,
            end: 13,
            value: "{1;2;3}".into(),
            evaluated: true,
            reference: Some("A1:A3".into()),
            summary: None,
            children: Vec::new(),
        };
        GridFormulaTrace {
            cell: "Sheet1!$B$1".into(),
            formula: "=IF(SUM(A1:A3)>5,\"x\",\"y\")".into(),
            value: "\"x\"".into(),
            root: Some(GridTraceNode {
                kind: "call".into(),
                label: "SUM".into(),
                text: "SUM(A1:A3)".into(),
                start: 4,
                end: 14,
                value: "6".into(),
                evaluated: true,
                reference: None,
                summary: Some("Adds its arguments.".into()),
                children: vec![leaf],
            }),
            steps: vec![GridEvalStep {
                text: "=IF(SUM(A1:A3)>5,\"x\",\"y\")".into(),
                start: 8,
                end: 13,
                value: "{1;2;3}".into(),
            }],
            message: None,
        }
    }

    /// Every command.
    pub fn commands() -> Vec<Command> {
        vec![
            C::SortAscending,
            C::SortDescending,
            C::ApplySort {
                spec: GridSortSpec {
                    range: Some(range()),
                    keys: vec![
                        GridSortKey {
                            index: 1,
                            descending: true,
                            on: GridSortOn::Values,
                            color: None,
                            icon: None,
                            custom_list: Some(vec!["Low".into(), "High".into()]),
                        },
                        GridSortKey {
                            index: 2,
                            descending: false,
                            on: GridSortOn::CellColor,
                            color: Some("FFC7CE".into()),
                            icon: None,
                            custom_list: None,
                        },
                    ],
                    header: None,
                    case_sensitive: true,
                    by_columns: false,
                },
            },
            C::ToggleFilter,
            C::FilterColumn {
                row: 0,
                col: 2,
                filter: Some(GridColumnFilter::Values {
                    values: vec!["Firing".into(), "Glaze".into()],
                    blanks: false,
                }),
            },
            C::FilterColumn {
                row: 0,
                col: 3,
                filter: Some(GridColumnFilter::Custom {
                    and: true,
                    criteria: vec![GridCriterion {
                        op: GridFilterOp::GreaterThan,
                        value: "100".into(),
                    }],
                }),
            },
            C::FilterColumn {
                row: 0,
                col: 1,
                filter: None,
            },
            C::SortFilterColumn {
                row: 0,
                col: 1,
                descending: true,
                on: GridSortOn::FontColor,
                color: Some("C00000".into()),
            },
            C::ClearFilter,
            C::ReapplyFilter,
            C::AddConditionalFormat { rule: cf_rule() },
            C::SetConditionalFormats {
                rules: vec![GridCfRule {
                    id: 7,
                    applies_to: "$A$1:$A$9".into(),
                    kind: GridCfRuleKind::CellValue {
                        op: GridCompare::GreaterThan,
                        values: vec!["5".into()],
                    },
                    style: Some(GridCfStyle {
                        fill: Some("FFC7CE".into()),
                        font_color: Some("9C0006".into()),
                        ..GridCfStyle::default()
                    }),
                    stop_if_true: true,
                }],
                sheet: true,
            },
            C::ClearConditionalFormats { sheet: false },
            C::SetValidation {
                rule: Some(validation()),
            },
            C::SetValidation { rule: None },
            C::CircleInvalid { on: true },
            C::CreateTable {
                range: Some(range()),
                header: true,
                style: "TableStyleMedium9".into(),
            },
            C::TableStyle {
                style: "TableStyleLight1".into(),
            },
            C::TableOptions {
                options: table().options,
            },
            C::TableTotals {
                col: 2,
                totals: GridTotals::Average,
            },
            C::RenameTable {
                name: "Sales".into(),
            },
            C::ResizeTable { range: range() },
            C::ConvertToRange,
            C::ApplyCellStyle {
                name: "Good".into(),
            },
            C::PaintFormat { from: range() },
            C::Replace {
                spec: GridFindSpec {
                    text: "kiln".into(),
                    match_case: true,
                    look_in: GridLookIn::Formulas,
                    ..GridFindSpec::default()
                },
                with: "Kiln".into(),
                all: true,
            },
            C::SelectSpecial {
                special: GridSpecial::Blanks,
            },
            C::SetNote {
                text: Some("Check «glaze» cost".into()),
                author: "Ana".into(),
                cell: Some("D4".into()),
            },
            C::SetNote {
                text: None,
                author: String::new(),
                cell: None,
            },
            C::ShowNote { show: None },
            C::ShowAllNotes { show: true },
            C::AddComment {
                text: "Is this right?".into(),
                author: "Ana".into(),
                reply_to: None,
                now_ms: 1_760_000_000_000,
            },
            C::EditComment {
                id: "{A1B2}".into(),
                text: "Fixed".into(),
            },
            C::DeleteComment { id: None },
            C::ResolveComment {
                id: "{A1B2}".into(),
                done: true,
            },
            C::NextComment { backward: true },
            C::ConvertNotes {
                author: "Ana".into(),
                now_ms: 0,
            },
            C::SetLink {
                link: Some(GridLink {
                    target: "https://kiln.example/".into(),
                    display: "Kiln".into(),
                    ..GridLink::default()
                }),
            },
            C::SetLink { link: None },
            C::PageSetup {
                setup: Box::new(GridPageSetup {
                    landscape: Some(true),
                    fit_width: Some(1),
                    fit_height: Some(0),
                    title_rows: Some("$1:$1".into()),
                    first_page: Some(0),
                    ..GridPageSetup::default()
                }),
            },
            C::PrintArea {
                action: GridPrintAreaAction::Set,
            },
            C::PageBreak {
                action: GridBreakAction::Insert,
            },
            C::MovePageBreak {
                columns: false,
                from: 20,
                to: 25,
            },
            C::DefineName {
                name: "Rates".into(),
                refers_to: "=Sheet1!$A$1:$A$10".into(),
                scope: None,
                comment: String::new(),
                previous: Some(("Rate".into(), None)),
            },
            C::DeleteName {
                name: "Rates".into(),
                scope: Some(0),
            },
            C::CreateNames {
                top: true,
                left: false,
                bottom: false,
                right: false,
            },
            C::TracePrecedents,
            C::TraceDependents,
            C::RemoveArrows {
                which: GridArrows::All,
            },
            C::CalculationMode {
                mode: GridCalcMode::Manual,
            },
            C::AcceptInvalid,
            C::DateSystem { date1904: true },
            C::IterativeCalculation {
                enabled: true,
                max_iterations: 100,
                max_change: 0.001,
            },
            C::FormulaLab { open: true },
            C::Watch {
                add: true,
                sheet: 0,
                row: 3,
                col: 1,
            },
            C::Import {
                import: Box::new(GridImport {
                    bytes: b"a,b\n1,2\n".to_vec(),
                    name: "data.csv".into(),
                    ..GridImport::default()
                }),
            },
            C::RefreshAll,
            C::TextToColumns {
                options: GridTextToColumns {
                    delimited: true,
                    delimiters: ",".into(),
                    qualifier: "\"".into(),
                    ..GridTextToColumns::default()
                },
            },
            C::RemoveDuplicates {
                columns: vec![0, 2],
                header: Some(true),
            },
            C::Group { columns: false },
            C::Ungroup { columns: true },
            C::OutlineDetail { show: false },
            C::OutlineSymbols,
            C::OutlineLevel {
                columns: false,
                level: 2,
            },
            C::OutlineToggle {
                columns: true,
                index: 4,
            },
            C::SpellChange {
                sheet: 0,
                row: 2,
                col: 1,
                offset: 0,
                word: "teh".into(),
                with: "the".into(),
                all: false,
            },
            C::SpellIgnore {
                word: "Kiln".into(),
                add: true,
            },
            C::LoadDictionary {
                aff: "SET UTF-8\n".into(),
                dic: "1\nkiln\n".into(),
            },
            C::PrintGridlines,
            C::PrintHeadings,
            C::InsertPicture {
                name: "kiln.png".into(),
                bytes: vec![0x89, 0x50, 0x4e, 0x47],
            },
            C::InsertShape {
                preset: "roundRect".into(),
                text: Some("Firing".into()),
            },
            C::FormatObject {
                format: Box::new(GridObjectFormat {
                    height: Some(120.0),
                    alt_text: Some("Kiln".into()),
                    ..GridObjectFormat::default()
                }),
            },
            C::ResetPicture { size: true },
            C::ArrangeDrawing {
                arrange: GridArrange::Front,
            },
        ]
        .into_iter()
        .map(Command::Grid)
        .collect()
    }

    crate::samples::samples_for! {
        GridSortOn => vec![GridSortOn::Values, GridSortOn::Icon];
        GridSortKey => vec![GridSortKey { index: 0, descending: false, on: GridSortOn::Icon, color: None, icon: Some(2), custom_list: None }];
        GridDataRange => vec![GridDataRange { range: range(), headers: vec!["Item".into(), "Qty".into()], header: true }];
        GridSortSpec => vec![GridSortSpec { range: None, keys: Vec::new(), header: Some(true), case_sensitive: false, by_columns: true }];
        GridFilterOp => vec![GridFilterOp::Contains, GridFilterOp::LessThanOrEqual];
        GridCriterion => vec![GridCriterion { op: GridFilterOp::BeginsWith, value: "Kil*".into() }];
        GridDynamicFilter => vec![GridDynamicFilter::AboveAverage, GridDynamicFilter::YearToDate];
        GridColumnFilter => vec![
            GridColumnFilter::Top10 { top: false, percent: true, value: 25 },
            GridColumnFilter::Dynamic { kind: GridDynamicFilter::ThisMonth },
            GridColumnFilter::Color { color: "FFEB9C".into(), font: false },
        ];
        GridValueKind => vec![GridValueKind::Text, GridValueKind::Date];
        GridFilterItem => vec![GridFilterItem { text: "Clay".into(), checked: true, date: None }];
        GridFilterMenu => vec![GridFilterMenu {
            row: 3, col: 1, header: "Category".into(), kind: GridValueKind::Text,
            items: vec![GridFilterItem { text: "Firing".into(), checked: false, date: Some("2026-01-05".into()) }],
            blanks: Some(true), filter: None, fill_colors: vec!["C6EFCE".into()],
            font_colors: Vec::new(), sorted: Some(false), truncated: false,
        }];
        GridCompare => vec![GridCompare::Between, GridCompare::LessThanOrEqual];
        GridTextRule => vec![GridTextRule::Contains, GridTextRule::EndsWith];
        GridDatePeriod => vec![GridDatePeriod::Last7Days, GridDatePeriod::NextMonth];
        GridCfValueKind => vec![GridCfValueKind::Min, GridCfValueKind::Formula];
        GridCfValue => vec![GridCfValue { kind: GridCfValueKind::Percentile, value: "50".into(), gte: false }];
        GridCfStyle => vec![GridCfStyle { bold: true, border: Some("000000".into()), number_format: Some("0.0%".into()), ..GridCfStyle::default() }];
        GridCfRuleKind => vec![
            GridCfRuleKind::Text { rule: GridTextRule::BeginsWith, text: "Kiln".into() },
            GridCfRuleKind::DateOccurring { period: GridDatePeriod::Today },
            GridCfRuleKind::Duplicates { unique: true },
            GridCfRuleKind::TopBottom { bottom: false, percent: false, rank: 10 },
            GridCfRuleKind::Average { below: true, equal: false, std_dev: 1 },
            GridCfRuleKind::Blanks { not: true },
            GridCfRuleKind::Errors { not: false },
            GridCfRuleKind::Formula { formula: "=MOD(ROW(),2)=0".into() },
            GridCfRuleKind::ColorScale {
                points: vec![
                    GridCfValue { kind: GridCfValueKind::Min, value: String::new(), gte: true },
                    GridCfValue { kind: GridCfValueKind::Max, value: String::new(), gte: true },
                ],
                colors: vec!["F8696B".into(), "63BE7B".into()],
            },
            GridCfRuleKind::IconSet {
                set: "3TrafficLights1".into(),
                thresholds: vec![
                    GridCfValue { kind: GridCfValueKind::Percent, value: "33".into(), gte: true },
                    GridCfValue { kind: GridCfValueKind::Percent, value: "67".into(), gte: true },
                ],
                reverse: false,
                show_value: true,
            },
        ];
        GridCfRule => vec![cf_rule()];
        GridValidationKind => vec![GridValidationKind::Whole, GridValidationKind::Custom];
        GridErrorStyle => vec![GridErrorStyle::Stop, GridErrorStyle::Information];
        GridValidation => vec![validation(), GridValidation { kind: GridValidationKind::Whole, op: GridCompare::Between, formula1: "1".into(), formula2: "10".into(), ..GridValidation::default() }];
        GridTableOptions => vec![GridTableOptions::default()];
        GridTotals => vec![GridTotals::None, GridTotals::StdDev];
        GridTableColumn => vec![table().columns[0].clone()];
        GridTableInfo => vec![table()];
        GridTableStyle => vec![GridTableStyle { name: "TableStyleMedium2".into(), family: "Medium".into(), header: Some("4472C4".into()), header_text: Some("FFFFFF".into()), band: Some("D9E1F2".into()), line: None }];
        GridCellStyleInfo => vec![GridCellStyleInfo { name: "Good".into(), group: "Good, Bad and Neutral".into(), fill: Some("C6EFCE".into()), color: Some("006100".into()), bold: false, border: None }];
        GridLookIn => vec![GridLookIn::Values, GridLookIn::Notes];
        GridFindSpec => vec![GridFindSpec { text: "?ln".into(), whole_cell: true, workbook: true, by_columns: true, ..GridFindSpec::default() }];
        GridFindHit => vec![GridFindHit { sheet: 0, sheet_name: "Budget".into(), row: 6, col: 1, cell: "$B$7".into(), value: "Kiln".into(), formula: None }];
        GridSpecial => vec![GridSpecial::CurrentRegion, GridSpecial::DataValidation];
        GridCommentInfo => vec![GridCommentInfo { id: "{A1B2}".into(), sheet: 0, row: 4, col: 2, cell: "C5".into(), author: "Ana".into(), date: "2026-10-18T09:00:00Z".into(), text: "Why?".into(), parent: None, done: false, note: false, visible: false }];
        GridLink => vec![GridLink { location: "'Sheet 2'!A1".into(), tooltip: "Totals".into(), ..GridLink::default() }];
        GridNoteCard => vec![GridNoteCard { row: 2, col: 3, author: "Ana".into(), text: "Kiln hire".into(), x: 300.0, y: 64.0, thread: true, replies: 2 }];
        GridMargins => vec![margins()];
        GridPageSetup => vec![GridPageSetup { margins: Some(margins()), header: Some("&C&A".into()), print_area: Some(String::new()), ..GridPageSetup::default() }];
        GridPageInfo => vec![page_info()];
        GridBreakAction => vec![GridBreakAction::Remove, GridBreakAction::Reset];
        GridPrintAreaAction => vec![GridPrintAreaAction::Add, GridPrintAreaAction::Clear];
        GridNameInfo => vec![GridNameInfo { name: "Rates".into(), refers_to: "=Sheet1!$A$1:$A$3".into(), scope: Some(1), comment: "VAT".into(), value: "{0.2;0.05;0}".into(), table: false }];
        GridCalcMode => vec![GridCalcMode::Automatic, GridCalcMode::AutomaticExceptTables];
        GridTraceNode => vec![trace().root.unwrap_or_else(|| unreachable!())];
        GridEvalStep => vec![trace().steps[0].clone()];
        GridFormulaTrace => vec![trace(), GridFormulaTrace { cell: "Sheet1!$A$1".into(), formula: String::new(), value: "5".into(), root: None, steps: Vec::new(), message: Some("The cell holds a constant.".into()) }];
        GridWatch => vec![GridWatch { sheet: "Budget".into(), cell: "G10".into(), sheet_index: 0, row: 9, col: 6, name: "Total".into(), value: "7,950".into(), formula: "=SUM(G5:G9)".into() }];
        GridErrorInfo => vec![GridErrorInfo { row: 1, col: 1, cell: "B2".into(), formula: "=1/0".into(), error: "#DIV/0!".into(), title: "Divide by Zero Error".into(), help: "The formula divides by zero or an empty cell.".into() }];
        GridArrows => vec![GridArrows::Precedents, GridArrows::Dependents];
        GridImportFormat => vec![GridImportFormat::Text, GridImportFormat::Json];
        GridImport => vec![GridImport { bytes: b"[{\"a\":1}]".to_vec(), name: "data.json".into(), format: GridImportFormat::Json, delimiter: Some(";".into()), encoding: Some("utf-8".into()), as_text: true, at_cell: true, decimal_comma: true }];
        GridExportFormat => vec![GridExportFormat::Csv, GridExportFormat::Json];
        GridExportFile => vec![GridExportFile { name: "Budget.csv".into(), mime: "text/csv".into(), bytes: b"a,b\r\n".to_vec() }];
        GridImportPreview => vec![GridImportPreview { rows: vec![vec!["a".into(), "b".into()]], total_rows: 2, total_cols: 2, encoding: "utf-8".into(), delimiter: ",".into(), warnings: Vec::new() }];
        GridTextToColumns => vec![GridTextToColumns { delimited: false, widths: vec![4, 10], text_columns: vec![1], skip_columns: vec![2], ..GridTextToColumns::default() }];
        GridSpellingHit => vec![GridSpellingHit { sheet: 0, row: 3, col: 0, word: "glazze".into(), offset: 5, text: "Blue glazze".into(), suggestions: vec!["glaze".into()] }];
        GridFunctionArg => function_doc().args;
        GridFunctionDoc => vec![function_doc()];
        GridDrawingKind => vec![GridDrawingKind::Picture, GridDrawingKind::Shape];
        GridPlacement => vec![GridPlacement::Move, GridPlacement::Fixed];
        GridArrange => vec![GridArrange::Front, GridArrange::Backward];
        GridObjectFormat => vec![GridObjectFormat { width: Some(240.0), lock_aspect: Some(true), crop: Some([10.0, 0.0, 5.0, 0.0]), fill: Some("scheme:accent1".into()), text: Some("Kiln".into()), ..GridObjectFormat::default() }];
        GridObjectInfo => vec![object_info()];
        GridFeatureState => vec![GridFeatureState::default(), GridFeatureState {
            filter: true, filtered: true, table: Some(table()), note: true, any_comments: true,
            prompt: Some(["Priority".into(), "Pick one".into()]), dropdown: true,
            calc: Some(GridCalcMode::Automatic), pages: 2, lab: true, message: Some("Done".into()),
            iterative: (true, 100, 0.001), circular: vec!["Sheet1!A1".into()], date1904: true,
            alert: Some((GridErrorStyle::Warning, "Shelves".into(), "Between 1 and 10".into())),
            ..GridFeatureState::default()
        }];
    }

    /// Registers the types with the schema and test-vector visitor.
    pub fn visit(v: &mut impl crate::schema::Visitor) {
        crate::schema::visit_types!(v;
            GridSortOn, GridSortKey, GridSortSpec, GridDataRange, GridFilterOp, GridCriterion, GridDynamicFilter,
            GridColumnFilter, GridValueKind, GridFilterItem, GridFilterMenu, GridCompare,
            GridTextRule, GridDatePeriod, GridCfValueKind, GridCfValue, GridCfStyle,
            GridCfRuleKind, GridCfRule, GridValidationKind, GridErrorStyle, GridValidation,
            GridTableOptions, GridTotals, GridTableColumn, GridTableInfo, GridTableStyle,
            GridCellStyleInfo, GridLookIn, GridFindSpec, GridFindHit, GridSpecial,
            GridCommentInfo, GridLink, GridNoteCard, GridMargins, GridPageSetup, GridPageInfo,
            GridBreakAction, GridPrintAreaAction, GridNameInfo, GridCalcMode, GridTraceNode,
            GridEvalStep, GridFormulaTrace, GridWatch, GridErrorInfo, GridArrows,
            GridImportFormat, GridImport, GridImportPreview, GridExportFormat, GridExportFile, GridTextToColumns, GridSpellingHit,
            GridFunctionArg, GridFunctionDoc, GridDrawingKind, GridPlacement, GridArrange, GridObjectFormat, GridObjectInfo,
            GridFeatureState,
        );
    }
}
