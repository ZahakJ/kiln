// SPDX-License-Identifier: AGPL-3.0-or-later
//! Arguments and answers of Kiln Grid's analysis features: PivotTables (Insert ▸
//! PivotTable, the PivotTable Fields pane, PivotTable Analyze and Design), sparklines (Insert ▸
//! Sparklines and the Sparkline tab), Goal Seek, Data ▸ Subtotal, Flash Fill, sheet and
//! workbook protection, and Page Layout ▸ Themes. The commands are variants of
//! [`super::GridCommand`], the questions of [`super::GridQuery`].
//!
//! References in text (`source`, `location`) are A1 text as Excel shows it, with an optional
//! sheet (`Sales!$A$1:$F$200`) or a table name (`Table1`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::GridRange;

// ── PivotTables ──────────────────────────────────────────────────────────────────────────────

/// An area of the PivotTable Fields pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridPivotArea {
    /// Filters.
    Filters,
    /// Columns.
    Columns,
    /// Rows.
    Rows,
    /// Values.
    Values,
}

/// Summarize Values By.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[allow(missing_docs)]
pub enum GridPivotFunction {
    #[default]
    Sum,
    Count,
    Average,
    Max,
    Min,
    Product,
    CountNums,
    StdDev,
    StdDevP,
    Var,
    VarP,
}

/// Show Values As.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[allow(missing_docs)]
pub enum GridShowAs {
    #[default]
    Normal,
    PercentOfTotal,
    PercentOfColumn,
    PercentOfRow,
    PercentOfParentRow,
    PercentOfParentColumn,
    Percent,
    Difference,
    PercentDifference,
    RunningTotal,
    PercentRunningTotal,
    RankAscending,
    RankDescending,
    Index,
}

/// A value field (Value Field Settings).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct GridPivotValue {
    /// The source field.
    pub field: u32,
    /// Custom Name.
    pub name: String,
    /// Summarize Values By.
    pub function: GridPivotFunction,
    /// Show Values As.
    pub show_as: GridShowAs,
    /// Base field of a calculation that needs one.
    pub base_field: Option<u32>,
    /// Base item: an item's label, `(previous)` or `(next)`.
    pub base_item: Option<String>,
    /// Number Format code (`None`: General).
    pub num_fmt: Option<String>,
}

/// Comparisons of PivotTable label and value filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[allow(missing_docs)]
pub enum GridPivotFilterOp {
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

/// A field's Label Filter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridPivotLabelFilter {
    /// Comparison.
    pub op: GridPivotFilterOp,
    /// Value (`*` and `?` are wildcards).
    pub value: String,
    /// The second value of Between.
    pub value2: String,
}

/// A field's Value Filter: Top 10 (`top`), or a comparison of the items' totals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GridPivotValueFilter {
    /// The value field (index into the values).
    pub value: u32,
    /// Top 10: how many items, and from the bottom.
    pub top: Option<(u32, bool)>,
    /// A comparison (when `top` is `None`).
    pub op: GridPivotFilterOp,
    /// Number.
    pub a: f64,
    /// Second number of Between.
    pub b: f64,
}

/// A field's subtotals.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridPivotSubtotals {
    /// Automatic.
    #[default]
    Automatic,
    /// None.
    None,
    /// Custom.
    Custom(Vec<GridPivotFunction>),
}

/// How a field's items are ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridPivotSort {
    /// A to Z.
    #[default]
    Ascending,
    /// Z to A.
    Descending,
    /// Data source order.
    Source,
}

/// Field Settings, the field's sort and its filters.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct GridPivotFieldSettings {
    /// Custom Name (`None`: the source name).
    pub caption: Option<String>,
    /// Subtotals.
    pub subtotals: GridPivotSubtotals,
    /// Sort order.
    pub sort: GridPivotSort,
    /// Sort by a value field's totals (index into the values).
    pub sort_by_value: Option<u32>,
    /// Items left out, by label.
    pub hidden: Vec<String>,
    /// Label Filter.
    pub label_filter: Option<GridPivotLabelFilter>,
    /// Value Filter.
    pub value_filter: Option<GridPivotValueFilter>,
}

/// Report Layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridPivotLayout {
    /// Compact Form.
    #[default]
    Compact,
    /// Outline Form.
    Outline,
    /// Tabular Form.
    Tabular,
}

/// Design ▸ Subtotals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridSubtotalPlace {
    /// At the top of the group.
    #[default]
    Top,
    /// At the bottom of the group.
    Bottom,
    /// Do not show subtotals.
    Off,
}

/// PivotTable Options and the Design tab.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub struct GridPivotOptions {
    /// Report Layout.
    pub layout: GridPivotLayout,
    /// Subtotals.
    pub subtotals: GridSubtotalPlace,
    /// Grand totals for rows (the total column).
    pub row_totals: bool,
    /// Grand totals for columns (the total row).
    pub col_totals: bool,
    /// Repeat All Item Labels.
    pub repeat_labels: bool,
    /// Insert Blank Line after Each Item.
    pub blank_rows: bool,
    /// For empty cells show.
    pub empty_text: Option<String>,
    /// Style name (`PivotStyleLight16`; empty for none).
    pub style: String,
    /// Row Headers.
    pub row_headers: bool,
    /// Column Headers.
    pub col_headers: bool,
    /// Banded Rows.
    pub row_stripes: bool,
    /// Banded Columns.
    pub col_stripes: bool,
}

/// A change to the PivotTable at the active cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum GridPivotEdit {
    /// Puts a field in an area at a position (`field` `None` moves Σ Values); a field placed in
    /// Values adds a value field.
    Place {
        /// The field.
        field: Option<u32>,
        /// The area.
        area: GridPivotArea,
        /// The position in the area (`None`: last).
        position: Option<u32>,
    },
    /// Unticks a field: out of every area.
    Remove {
        /// The field.
        field: u32,
    },
    /// Removes a value field.
    RemoveValue {
        /// Its index.
        index: u32,
    },
    /// Moves a value field among the values.
    MoveValue {
        /// Its index.
        index: u32,
        /// The new index.
        position: u32,
    },
    /// Value Field Settings.
    Value {
        /// Index of the value field.
        index: u32,
        /// The settings.
        value: GridPivotValue,
    },
    /// Field Settings, sort and filters of a field.
    Field {
        /// The field.
        field: u32,
        /// The settings.
        settings: GridPivotFieldSettings,
    },
    /// A filter field's item (`None`: All).
    Page {
        /// The field.
        field: u32,
        /// The item's label.
        item: Option<String>,
    },
    /// Options and the Design tab.
    Options {
        /// The options.
        options: GridPivotOptions,
    },
    /// PivotTable Name.
    Rename {
        /// The name.
        name: String,
    },
    /// Change Data Source.
    Source {
        /// The new source.
        source: String,
    },
    /// Move PivotTable.
    Move {
        /// The new top-left cell (`Sheet2!A3`).
        to: String,
    },
    /// Clear ▸ Clear Filters.
    ClearFilters,
    /// Clear ▸ Clear All.
    ClearAll,
    /// Deletes the PivotTable and its report.
    Delete,
}

/// One field of the PivotTable Fields pane.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct GridPivotFieldInfo {
    /// Source name.
    pub name: String,
    /// Numbers only (a new value field sums it).
    pub numeric: bool,
    /// Where it is placed (rows, columns or filters).
    pub area: Option<GridPivotArea>,
    /// How many value fields it has.
    pub values: u32,
    /// Items are filtered out.
    pub filtered: bool,
    /// Its settings.
    pub settings: GridPivotFieldSettings,
}

/// The PivotTable at the active cell (the Fields pane and the contextual tabs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GridPivotInfo {
    /// Name.
    pub name: String,
    /// Source as text.
    pub source: String,
    /// The report (Filters area included).
    pub range: GridRange,
    /// Fields in source order.
    pub fields: Vec<GridPivotFieldInfo>,
    /// Rows, outermost first (field index; -1 is Σ Values).
    pub rows: Vec<i32>,
    /// Columns.
    pub cols: Vec<i32>,
    /// Filters: field and the item shown (`None`: All).
    pub pages: Vec<(u32, Option<String>)>,
    /// Values.
    pub values: Vec<GridPivotValue>,
    /// Options.
    pub options: GridPivotOptions,
    /// The field of the active cell (Active Field).
    pub active_field: Option<u32>,
    /// The value field of the active cell.
    pub active_value: Option<u32>,
    /// The active cell is a value (Show Details works).
    pub on_value: bool,
}

// ── sparklines ───────────────────────────────────────────────────────────────────────────────

/// Sparkline type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridSparkKind {
    /// Line.
    #[default]
    Line,
    /// Column.
    Column,
    /// Win/Loss.
    WinLoss,
}

/// Show empty cells as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridSparkEmpty {
    /// Gaps.
    #[default]
    Gap,
    /// Zero.
    Zero,
    /// Connect data points with line.
    Span,
}

/// The vertical axis minimum or maximum.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, TS)]
pub enum GridSparkAxis {
    /// Automatic for Each Sparkline.
    #[default]
    Individual,
    /// Same for All Sparklines.
    Group,
    /// Custom Value.
    Custom(f64),
}

/// A sparkline group's settings (the Sparkline tab).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct GridSparkOptions {
    /// Type.
    pub kind: GridSparkKind,
    /// High Point.
    pub high: bool,
    /// Low Point.
    pub low: bool,
    /// First Point.
    pub first: bool,
    /// Last Point.
    pub last: bool,
    /// Negative Points.
    pub negative: bool,
    /// Markers.
    pub markers: bool,
    /// Show Axis.
    pub show_axis: bool,
    /// Line weight in points.
    pub line_weight: f32,
    /// Empty cells.
    pub empty: GridSparkEmpty,
    /// Show data in hidden rows and columns.
    pub show_hidden: bool,
    /// Vertical axis minimum.
    pub min: GridSparkAxis,
    /// Vertical axis maximum.
    pub max: GridSparkAxis,
    /// Plot Data Right-to-Left.
    pub right_to_left: bool,
    /// Colours `RRGGBB`: series, negative, markers, high, low, first, last, axis.
    pub colors: Vec<String>,
}

/// A change to the sparklines at the selection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum GridSparkEdit {
    /// The group's settings.
    Options {
        /// Settings.
        options: GridSparkOptions,
    },
    /// Edit Data: the group's data and location.
    Data {
        /// Data range.
        data: String,
        /// Location range.
        location: String,
    },
    /// Clear the selected sparklines (or their whole groups).
    Clear {
        /// The whole groups.
        groups: bool,
    },
    /// A Sparkline Style (the gallery's 24 presets: four rows of the six accents, darker to
    /// lighter), in the workbook's theme colours.
    Style {
        /// Preset 0–23.
        index: u32,
    },
    /// Group the selected sparklines.
    Group,
    /// Ungroup them.
    Ungroup,
}

/// The sparkline group at the active cell.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct GridSparkInfo {
    /// Settings.
    pub options: GridSparkOptions,
    /// The group's data, as text.
    pub data: String,
    /// The group's location, as text.
    pub location: String,
    /// How many sparklines the group has.
    pub count: u32,
}

// ── Goal Seek, Subtotal, Flash Fill ──────────────────────────────────────────────────────────

/// Goal Seek Status.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub struct GridGoalSeekStatus {
    /// The set cell (`B5`).
    pub cell: String,
    /// A solution was found.
    pub found: bool,
    /// Target value as typed.
    pub target: String,
    /// The set cell's value now, as shown.
    pub current: String,
}

/// Data ▸ Subtotal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub struct GridSubtotalSpec {
    /// At each change in (the column, absolute).
    pub at_change_in: u32,
    /// Use function.
    pub function: GridPivotFunction,
    /// Add subtotal to (columns, absolute).
    pub columns: Vec<u32>,
    /// Replace current subtotals.
    pub replace: bool,
    /// Page break between groups.
    pub page_breaks: bool,
    /// Summary below data.
    pub below: bool,
}

/// A cell Flash Fill would fill, shown greyed before it is accepted.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct GridFlashCell {
    /// Row.
    pub row: u32,
    /// Column.
    pub col: u32,
    /// The text it would get.
    pub text: String,
    /// x, y, width, height in surface CSS pixels.
    pub rect: [f32; 4],
}

// ── protection and themes ────────────────────────────────────────────────────────────────────

/// What users of a protected sheet may still do (Protect Sheet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
#[allow(missing_docs)]
pub struct GridSheetPermissions {
    pub select_locked: bool,
    pub select_unlocked: bool,
    pub format_cells: bool,
    pub format_columns: bool,
    pub format_rows: bool,
    pub insert_columns: bool,
    pub insert_rows: bool,
    pub insert_hyperlinks: bool,
    pub delete_columns: bool,
    pub delete_rows: bool,
    pub sort: bool,
    pub auto_filter: bool,
    pub pivot_tables: bool,
    pub edit_objects: bool,
    pub edit_scenarios: bool,
}

/// A theme of the Themes gallery.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub struct GridThemeInfo {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// The colours `RRGGBB` in scheme order (dark 1, light 1, dark 2, light 2, accents 1–6,
    /// hyperlink, followed hyperlink).
    pub colors: Vec<String>,
    /// Headings font.
    pub major_font: String,
    /// Body font.
    pub minor_font: String,
}

/// What a theme change takes from a theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridThemePart {
    /// Colours and fonts.
    #[default]
    All,
    /// Colours.
    Colors,
    /// Fonts.
    Fonts,
}

/// The analysis part of the chrome's state.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct GridAnalysisState {
    /// The PivotTable at the active cell.
    pub pivot: Option<GridPivotInfo>,
    /// The sparkline group at the active cell.
    pub sparkline: Option<GridSparkInfo>,
    /// The last Goal Seek.
    pub goal_seek: Option<GridGoalSeekStatus>,
    /// Flash Fill's suggestions for the column of the last entry (Enter accepts them).
    pub flash: Vec<GridFlashCell>,
    /// How many cells the suggestions fill.
    pub flash_count: u32,
    /// The active sheet is protected.
    pub sheet_protected: bool,
    /// Its protection has a password.
    pub sheet_password: bool,
    /// Its permissions.
    pub permissions: GridSheetPermissions,
    /// The workbook structure is protected.
    pub workbook_protected: bool,
    /// Its protection has a password.
    pub workbook_password: bool,
    /// The active cell is locked (Format ▸ Lock Cell).
    pub locked: bool,
    /// A message box to show (a refused edit of a protected sheet or PivotTable).
    pub alert: Option<String>,
    /// Counts the message boxes raised, so a repeated refusal shows again.
    pub alert_seq: u32,
    /// The workbook theme's name.
    pub theme: String,
    /// An empty PivotTable on the active sheet: its name and the placeholder's rectangle (x,
    /// y, width, height in surface CSS pixels), where the chrome writes Excel's prompt.
    pub placeholder: Option<(String, [f32; 4])>,
}

#[cfg(any(test, feature = "schema"))]
pub(crate) mod samples {
    //! Representative values (see `crate::samples`).

    use super::*;
    use crate::commands::Command;
    use crate::commands::grid::GridCommand as C;
    use crate::samples::Samples;

    pub(crate) fn value() -> GridPivotValue {
        GridPivotValue {
            field: 4,
            name: "Sum of Units".into(),
            function: GridPivotFunction::Sum,
            show_as: GridShowAs::PercentOfParentRow,
            base_field: Some(0),
            base_item: Some("(previous)".into()),
            num_fmt: Some("0.0%".into()),
        }
    }

    pub(crate) fn settings() -> GridPivotFieldSettings {
        GridPivotFieldSettings {
            caption: Some("Area".into()),
            subtotals: GridPivotSubtotals::Custom(vec![GridPivotFunction::Max]),
            sort: GridPivotSort::Descending,
            sort_by_value: Some(0),
            hidden: vec!["(blank)".into()],
            label_filter: Some(GridPivotLabelFilter {
                op: GridPivotFilterOp::BeginsWith,
                value: "E".into(),
                value2: String::new(),
            }),
            value_filter: Some(GridPivotValueFilter {
                value: 0,
                top: Some((10, false)),
                op: GridPivotFilterOp::GreaterThan,
                a: 0.0,
                b: 0.0,
            }),
        }
    }

    pub(crate) fn options() -> GridPivotOptions {
        GridPivotOptions {
            layout: GridPivotLayout::Tabular,
            subtotals: GridSubtotalPlace::Bottom,
            row_totals: true,
            col_totals: false,
            repeat_labels: true,
            blank_rows: false,
            empty_text: Some("0".into()),
            style: "PivotStyleMedium9".into(),
            row_headers: true,
            col_headers: true,
            row_stripes: true,
            col_stripes: false,
        }
    }

    pub(crate) fn pivot() -> GridPivotInfo {
        GridPivotInfo {
            name: "PivotTable1".into(),
            source: "Sales!$A$1:$G$401".into(),
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 12,
                c1: 3,
            },
            fields: vec![GridPivotFieldInfo {
                name: "Region".into(),
                numeric: false,
                area: Some(GridPivotArea::Rows),
                values: 0,
                filtered: true,
                settings: settings(),
            }],
            rows: vec![0, -1],
            cols: vec![1],
            pages: vec![(2, Some("Q1".into()))],
            values: vec![value()],
            options: options(),
            active_field: Some(0),
            active_value: None,
            on_value: true,
        }
    }

    pub(crate) fn spark() -> GridSparkOptions {
        GridSparkOptions {
            kind: GridSparkKind::Column,
            high: true,
            low: false,
            first: false,
            last: true,
            negative: true,
            markers: false,
            show_axis: true,
            line_weight: 0.75,
            empty: GridSparkEmpty::Span,
            show_hidden: false,
            min: GridSparkAxis::Group,
            max: GridSparkAxis::Custom(100.0),
            right_to_left: false,
            colors: vec!["376092".into(), "D00000".into()],
        }
    }

    pub(crate) fn state() -> GridAnalysisState {
        GridAnalysisState {
            pivot: Some(pivot()),
            sparkline: Some(GridSparkInfo {
                options: spark(),
                data: "A2:E9".into(),
                location: "F2:F9".into(),
                count: 8,
            }),
            goal_seek: Some(GridGoalSeekStatus {
                cell: "B5".into(),
                found: true,
                target: "100".into(),
                current: "100".into(),
            }),
            flash: vec![GridFlashCell {
                row: 2,
                col: 3,
                text: "Ana".into(),
                rect: [120.0, 40.0, 64.0, 20.0],
            }],
            flash_count: 9,
            sheet_protected: true,
            sheet_password: true,
            permissions: GridSheetPermissions {
                select_locked: true,
                select_unlocked: true,
                sort: true,
                ..GridSheetPermissions::default()
            },
            workbook_protected: false,
            workbook_password: false,
            locked: true,
            alert: Some(
                "The cell or chart you're trying to change is on a protected sheet.".into(),
            ),
            alert_seq: 3,
            theme: "Kiln".into(),
            placeholder: Some(("PivotTable1".into(), [44.0, 64.0, 192.0, 360.0])),
        }
    }

    pub(crate) fn commands() -> Vec<Command> {
        [
            C::CreatePivot {
                source: "Table1".into(),
                to: None,
            },
            C::CreatePivot {
                source: "Sales!$A$1:$G$401".into(),
                to: Some("Sheet2!$A$3".into()),
            },
            C::Pivot {
                edit: GridPivotEdit::Place {
                    field: Some(0),
                    area: GridPivotArea::Rows,
                    position: None,
                },
            },
            C::Pivot {
                edit: GridPivotEdit::Place {
                    field: None,
                    area: GridPivotArea::Rows,
                    position: Some(1),
                },
            },
            C::Pivot {
                edit: GridPivotEdit::Remove { field: 2 },
            },
            C::Pivot {
                edit: GridPivotEdit::RemoveValue { index: 0 },
            },
            C::Pivot {
                edit: GridPivotEdit::MoveValue {
                    index: 1,
                    position: 0,
                },
            },
            C::Pivot {
                edit: GridPivotEdit::Value {
                    index: 0,
                    value: value(),
                },
            },
            C::Pivot {
                edit: GridPivotEdit::Field {
                    field: 0,
                    settings: settings(),
                },
            },
            C::Pivot {
                edit: GridPivotEdit::Page {
                    field: 2,
                    item: Some("Q1".into()),
                },
            },
            C::Pivot {
                edit: GridPivotEdit::Options { options: options() },
            },
            C::Pivot {
                edit: GridPivotEdit::Rename {
                    name: "Sales by region".into(),
                },
            },
            C::Pivot {
                edit: GridPivotEdit::Source {
                    source: "Sales!$A:$G".into(),
                },
            },
            C::Pivot {
                edit: GridPivotEdit::Move {
                    to: "Sheet3!B2".into(),
                },
            },
            C::Pivot {
                edit: GridPivotEdit::ClearFilters,
            },
            C::Pivot {
                edit: GridPivotEdit::ClearAll,
            },
            C::Pivot {
                edit: GridPivotEdit::Delete,
            },
            C::RefreshPivots { all: true },
            C::PivotDetails { row: 4, col: 1 },
            C::InsertSparklines {
                kind: GridSparkKind::WinLoss,
                data: "B2:E9".into(),
                location: "$F$2:$F$9".into(),
            },
            C::Sparklines {
                edit: GridSparkEdit::Options { options: spark() },
            },
            C::Sparklines {
                edit: GridSparkEdit::Data {
                    data: "B2:E9".into(),
                    location: "F2:F9".into(),
                },
            },
            C::Sparklines {
                edit: GridSparkEdit::Clear { groups: true },
            },
            C::Sparklines {
                edit: GridSparkEdit::Group,
            },
            C::Sparklines {
                edit: GridSparkEdit::Style { index: 7 },
            },
            C::Sparklines {
                edit: GridSparkEdit::Ungroup,
            },
            C::GoalSeek {
                set_cell: "B5".into(),
                to_value: "100".into(),
                by_changing: "$B$2".into(),
            },
            C::Subtotal {
                spec: Some(GridSubtotalSpec {
                    at_change_in: 0,
                    function: GridPivotFunction::Sum,
                    columns: vec![2, 3],
                    replace: true,
                    page_breaks: false,
                    below: true,
                }),
            },
            C::Subtotal { spec: None },
            C::FlashFill,
            C::ProtectSheet {
                protect: true,
                password: "kiln".into(),
                allow: Some(GridSheetPermissions {
                    select_locked: true,
                    select_unlocked: true,
                    ..GridSheetPermissions::default()
                }),
            },
            C::ProtectSheet {
                protect: false,
                password: String::new(),
                allow: None,
            },
            C::ProtectWorkbook {
                protect: true,
                password: "kiln".into(),
            },
            C::LockCells { locked: false },
            C::SetTheme {
                theme: "tenmoku".into(),
                part: GridThemePart::Colors,
            },
        ]
        .into_iter()
        .map(Command::Grid)
        .collect()
    }

    crate::samples::samples_for! {
        GridPivotArea => vec![GridPivotArea::Filters, GridPivotArea::Values];
        GridPivotFunction => vec![GridPivotFunction::Sum, GridPivotFunction::VarP];
        GridShowAs => vec![GridShowAs::Normal, GridShowAs::RankDescending];
        GridPivotValue => vec![value(), GridPivotValue::default()];
        GridPivotFilterOp => vec![GridPivotFilterOp::Between, GridPivotFilterOp::NotContains];
        GridPivotLabelFilter => vec![settings().label_filter.unwrap_or_else(|| unreachable!())];
        GridPivotValueFilter => vec![GridPivotValueFilter { value: 1, top: None, op: GridPivotFilterOp::Between, a: 10.0, b: 20.5 }];
        GridPivotSubtotals => vec![GridPivotSubtotals::Automatic, GridPivotSubtotals::None, GridPivotSubtotals::Custom(vec![GridPivotFunction::Count])];
        GridPivotSort => vec![GridPivotSort::Ascending, GridPivotSort::Source];
        GridPivotFieldSettings => vec![settings(), GridPivotFieldSettings::default()];
        GridPivotLayout => vec![GridPivotLayout::Compact, GridPivotLayout::Outline];
        GridSubtotalPlace => vec![GridSubtotalPlace::Top, GridSubtotalPlace::Off];
        GridPivotOptions => vec![options(), GridPivotOptions::default()];
        GridPivotEdit => vec![GridPivotEdit::ClearAll, GridPivotEdit::Page { field: 1, item: None }];
        GridPivotFieldInfo => vec![pivot().fields[0].clone()];
        GridPivotInfo => vec![pivot()];
        GridSparkKind => vec![GridSparkKind::Line, GridSparkKind::WinLoss];
        GridSparkEmpty => vec![GridSparkEmpty::Gap, GridSparkEmpty::Zero];
        GridSparkAxis => vec![GridSparkAxis::Individual, GridSparkAxis::Custom(-2.5)];
        GridSparkOptions => vec![spark(), GridSparkOptions::default()];
        GridSparkEdit => vec![GridSparkEdit::Group, GridSparkEdit::Clear { groups: false }];
        GridSparkInfo => vec![state().sparkline.unwrap_or_else(|| unreachable!())];
        GridGoalSeekStatus => vec![GridGoalSeekStatus { cell: "C9".into(), found: false, target: "1e6".into(), current: "999,812".into() }];
        GridSubtotalSpec => vec![GridSubtotalSpec::default()];
        GridFlashCell => vec![state().flash[0].clone()];
        GridSheetPermissions => vec![GridSheetPermissions::default(), state().permissions];
        GridThemeInfo => vec![GridThemeInfo { id: "kiln".into(), name: "Kiln".into(), colors: vec!["1C1917".into(), "FFFFFF".into()], major_font: "Fraunces".into(), minor_font: "Figtree".into() }];
        GridThemePart => vec![GridThemePart::All, GridThemePart::Fonts];
        GridAnalysisState => vec![state(), GridAnalysisState::default()];
    }

    /// Registers the types with the schema and test-vector visitor.
    pub fn visit(v: &mut impl crate::schema::Visitor) {
        crate::schema::visit_types!(v;
            GridPivotArea, GridPivotFunction, GridShowAs, GridPivotValue, GridPivotFilterOp,
            GridPivotLabelFilter, GridPivotValueFilter, GridPivotSubtotals, GridPivotSort,
            GridPivotFieldSettings, GridPivotLayout, GridSubtotalPlace, GridPivotOptions,
            GridPivotEdit, GridPivotFieldInfo, GridPivotInfo, GridSparkKind, GridSparkEmpty,
            GridSparkAxis, GridSparkOptions, GridSparkEdit, GridSparkInfo, GridGoalSeekStatus,
            GridSubtotalSpec, GridFlashCell, GridSheetPermissions, GridThemeInfo, GridThemePart,
            GridAnalysisState,
        );
    }
}
