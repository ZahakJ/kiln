// SPDX-License-Identifier: AGPL-3.0-or-later
//! Commands: every user-visible mutation of a workbook as plain, serialisable data. Applying a
//! command ([`Workbook::apply`]) validates it, mutates the model (keeping the formula engine in
//! step), recalculates in automatic mode and returns the *inverse* command that undoes it exactly
//! — the basis of [`CommandLog`](crate::CommandLog)'s undo/redo, of journals and of
//! collaboration.
//!
//! Inverses of operations with workbook-wide side effects (structural edits, moves, sheet
//! removal…) are a batch of the opposite operation plus a [`Command::Restore`] of everything the
//! forward operation rewrote (formula texts, metadata, defined names), so undo is exact even where
//! the opposite operation alone would not be (references turned into `#REF!` by a deletion).

use kiln_formula::SheetId;
use serde::{Deserialize, Serialize};

use crate::address::{CellPos, Range};
use crate::axis::{ColProps, RowProps};
use crate::color::Color;
use crate::error::{ModelError, Result};
use crate::fill::FillMode;
use crate::meta::{
    AutoFilter, ConditionalFormat, DataValidation, DefinedName, DrawingAnchor, Hyperlink, Note,
    PrintSettings, SheetProtection, SheetView, SheetVisibility, SortBy, Table, TableColumn,
    TableFilter, TableStyleInfo, ThreadedComment, WorkbookProps, WorkbookProtection, opt_sheet,
    sheet_id,
};
use crate::num::F64;
use crate::serial::SheetSnapshot;
use crate::sheet::{SheetLayout, SheetMeta};
use crate::styles::{Patch, StylePatch};
use crate::value::CellValue;
use crate::workbook::CellRecord;

// ── command types ─────────────────────────────────────────────────────────

/// What a cell edit does to the content.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Content {
    /// Leave value and formula as they are (only the style changes).
    #[default]
    Keep,
    /// A literal value (removes any formula).
    Value(CellValue),
    /// A formula (with or without `=`).
    Formula(String),
    /// Text as typed into the cell: `=…` is a formula; numbers, percentages, currency, dates,
    /// times, logicals and error codes are recognised (and a matching number format applied
    /// to a cell formatted General), as Excel's cell entry does; `'` keeps the rest as text.
    Typed(String),
    /// Remove value and formula.
    Clear,
}

/// One cell edit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellInput {
    /// Row.
    pub row: u32,
    /// Column.
    pub col: u32,
    /// Content change.
    #[serde(default)]
    pub content: Content,
    /// New style index (`None` keeps the style; a new cell takes its row or column format).
    #[serde(default)]
    pub style: Option<u32>,
}

impl CellInput {
    /// A literal value.
    pub fn value(row: u32, col: u32, v: impl Into<CellValue>) -> CellInput {
        CellInput {
            row,
            col,
            content: Content::Value(v.into()),
            style: None,
        }
    }

    /// A formula.
    pub fn formula(row: u32, col: u32, f: impl Into<String>) -> CellInput {
        CellInput {
            row,
            col,
            content: Content::Formula(f.into()),
            style: None,
        }
    }
}

/// What Clear clears (Home › Clear).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClearWhat {
    /// Everything.
    #[default]
    All,
    /// Values and formulas.
    Contents,
    /// Formats (and merges).
    Formats,
    /// Notes and threaded comments.
    Notes,
    /// Hyperlinks.
    Hyperlinks,
}

/// What Paste Special pastes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PasteWhat {
    /// Everything (contents, formats, merges, notes).
    #[default]
    All,
    /// Values only.
    Values,
    /// Formulas (adjusted) and values, no formats.
    Formulas,
    /// Formats only.
    Formats,
    /// Values and number formats.
    ValuesAndNumberFormats,
    /// Formulas and number formats.
    FormulasAndNumberFormats,
    /// Everything except borders.
    AllExceptBorders,
    /// Notes and comments only.
    Comments,
}

/// Paste Special › Operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum PasteOp {
    #[default]
    None,
    Add,
    Subtract,
    Multiply,
    Divide,
}

/// One sort level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SortKey {
    /// Absolute column (or row, with `by_columns`) to sort by.
    pub index: u32,
    /// Descending.
    #[serde(default)]
    pub descending: bool,
    /// Custom order (months, a user list); values not in the list sort after it.
    #[serde(default)]
    pub custom_list: Option<Vec<String>>,
    /// Sort on values, fill colour or font colour.
    #[serde(default)]
    pub sort_by: SortBy,
    /// For colour sorts: the colour that goes on top (bottom when descending).
    #[serde(default)]
    pub color: Option<Color>,
}

impl SortKey {
    /// An ascending value key.
    pub fn asc(index: u32) -> SortKey {
        SortKey {
            index,
            descending: false,
            custom_list: None,
            sort_by: SortBy::Value,
            color: None,
        }
    }

    /// A descending value key.
    pub fn desc(index: u32) -> SortKey {
        SortKey {
            descending: true,
            ..SortKey::asc(index)
        }
    }
}

/// Where inserted rows or columns take their format from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FormatFrom {
    /// The row above / column left (Excel's default).
    #[default]
    Before,
    /// The row below / column right.
    After,
    /// No format.
    None,
}

/// Merge modes (Home › Merge & Center).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeMode {
    /// Merge Cells.
    #[default]
    Merge,
    /// Merge & Center.
    Center,
    /// Merge Across (each row separately).
    Across,
}

/// Changes to row properties (`Some` sets; flags `false` and `None` values clear).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct RowPatch {
    #[serde(default)]
    pub height: Patch<F64>,
    #[serde(default)]
    pub custom_height: Option<bool>,
    #[serde(default)]
    pub hidden: Option<bool>,
    #[serde(default)]
    pub filtered: Option<bool>,
    #[serde(default)]
    pub outline_level: Option<u8>,
    #[serde(default)]
    pub collapsed: Option<bool>,
    #[serde(default)]
    pub style: Patch<u32>,
}

/// Changes to column properties.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct ColPatch {
    #[serde(default)]
    pub width: Patch<F64>,
    #[serde(default)]
    pub custom_width: Option<bool>,
    #[serde(default)]
    pub best_fit: Option<bool>,
    #[serde(default)]
    pub hidden: Option<bool>,
    #[serde(default)]
    pub outline_level: Option<u8>,
    #[serde(default)]
    pub collapsed: Option<bool>,
    #[serde(default)]
    pub style: Patch<u32>,
}

/// Changes to a table (Table Design).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct TablePatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub display_name: Patch<String>,
    /// Resize (from the bottom-right corner only).
    #[serde(default)]
    pub range: Option<Range>,
    #[serde(default)]
    pub header_row: Option<bool>,
    #[serde(default)]
    pub totals_row: Option<bool>,
    #[serde(default)]
    pub style: Patch<TableStyleInfo>,
    #[serde(default)]
    pub auto_filter: Patch<TableFilter>,
    #[serde(default)]
    pub comment: Patch<String>,
    /// Replace the columns (names, totals functions, calculated formulas).
    #[serde(default)]
    pub columns: Option<Vec<TableColumn>>,
}

/// A metadata field set through [`Command::SetSheetMeta`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(missing_docs)]
pub enum MetaField {
    Cf(Vec<ConditionalFormat>),
    Validations(Vec<DataValidation>),
    Hyperlinks(Vec<Hyperlink>),
    Notes(Vec<Note>),
    Threads(Vec<ThreadedComment>),
    View(SheetView),
    Print(PrintSettings),
    AutoFilter(Option<AutoFilter>),
    Drawings(Vec<DrawingAnchor>),
    Visibility(SheetVisibility),
    TabColor(Option<Color>),
    Outline {
        summary_below: bool,
        summary_right: bool,
    },
    Protection(Option<SheetProtection>),
    DefaultRowHeight(f64),
    DefaultColWidth(Option<f64>),
}

/// Cells of one sheet to restore.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetCells {
    /// Sheet.
    #[serde(with = "sheet_id")]
    pub sheet: SheetId,
    /// Records (an empty record removes the cell).
    pub records: Vec<CellRecord>,
}

/// Metadata of one sheet to restore.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetMetaEntry {
    /// Sheet.
    #[serde(with = "sheet_id")]
    pub sheet: SheetId,
    /// Metadata.
    pub meta: Box<SheetMeta>,
}

/// Row and column properties of one sheet to restore.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetLayoutEntry {
    /// Sheet.
    #[serde(with = "sheet_id")]
    pub sheet: SheetId,
    /// Layout.
    pub layout: SheetLayout,
}

/// Options of Data › Text to Columns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextToColumns {
    /// Split on delimiters (otherwise at fixed widths).
    pub delimited: bool,
    /// Delimiter characters (tab, `;`, `,`, space or any other).
    #[serde(default)]
    pub delimiters: Vec<char>,
    /// Treat consecutive delimiters as one.
    #[serde(default)]
    pub merge_delimiters: bool,
    /// Text qualifier (`"` by default; `None` for none).
    #[serde(default)]
    pub qualifier: Option<char>,
    /// Fixed width: break positions in characters.
    #[serde(default)]
    pub breaks: Vec<usize>,
    /// Per output column: how to convert (missing entries are General).
    #[serde(default)]
    pub formats: Vec<crate::data::ColumnFormat>,
    /// Destination top-left (`None` = the source cell).
    #[serde(default)]
    pub destination: Option<CellPos>,
}

/// Every mutation of a workbook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    /// Edit cells (values, formulas, styles).
    SetCells {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        /// Edits.
        cells: Vec<CellInput>,
    },
    /// Write a 2-D block of values with its top left at (row, col); `None` entries are skipped.
    SetRange {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        row: u32,
        #[allow(missing_docs)]
        col: u32,
        /// Rows of values.
        values: Vec<Vec<Option<CellValue>>>,
    },
    /// Home › Clear.
    ClearRange {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        ranges: Vec<Range>,
        #[allow(missing_docs)]
        what: ClearWhat,
    },
    /// Apply a format change; whole rows/columns set the row/column format.
    SetStyle {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        ranges: Vec<Range>,
        #[allow(missing_docs)]
        patch: Box<StylePatch>,
    },
    /// Apply a format index (Format Painter, cell styles).
    SetStyleIndex {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        ranges: Vec<Range>,
        #[allow(missing_docs)]
        style: u32,
    },
    /// Insert rows before `at`.
    InsertRows {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        at: u32,
        #[allow(missing_docs)]
        count: u32,
        #[allow(missing_docs)]
        #[serde(default)]
        format_from: FormatFrom,
    },
    /// Insert columns before `at`.
    InsertCols {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        at: u32,
        #[allow(missing_docs)]
        count: u32,
        #[allow(missing_docs)]
        #[serde(default)]
        format_from: FormatFrom,
    },
    /// Delete rows.
    DeleteRows {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        at: u32,
        #[allow(missing_docs)]
        count: u32,
    },
    /// Delete columns.
    DeleteCols {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        at: u32,
        #[allow(missing_docs)]
        count: u32,
    },
    /// Cut and paste: references follow the cells.
    MoveRange {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        range: Range,
        /// Destination sheet (`None` = same sheet).
        #[serde(default, with = "opt_sheet")]
        to_sheet: Option<SheetId>,
        /// Destination top left.
        to: CellPos,
    },
    /// Copy and paste (Paste Special options).
    CopyRange {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        range: Range,
        /// Destination sheet (`None` = same sheet).
        #[serde(default, with = "opt_sheet")]
        to_sheet: Option<SheetId>,
        /// Destination (tiled when a whole multiple of the source).
        to: Range,
        #[allow(missing_docs)]
        #[serde(default)]
        what: PasteWhat,
        #[allow(missing_docs)]
        #[serde(default)]
        op: PasteOp,
        #[allow(missing_docs)]
        #[serde(default)]
        transpose: bool,
        #[allow(missing_docs)]
        #[serde(default)]
        skip_blanks: bool,
    },
    /// Data › Sort.
    Sort {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        range: Range,
        #[allow(missing_docs)]
        keys: Vec<SortKey>,
        /// The first row (column) is a header.
        #[serde(default)]
        header: bool,
        #[allow(missing_docs)]
        #[serde(default)]
        case_sensitive: bool,
        /// Sort left to right.
        #[serde(default)]
        by_columns: bool,
        /// BCP 47 locale for collation (`None` = Excel's order).
        #[serde(default)]
        locale: Option<String>,
    },
    /// Fill handle / Home › Fill › Series: fill `target` from `source`.
    AutoFill {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        source: Range,
        /// The whole filled area (containing the source).
        target: Range,
        #[allow(missing_docs)]
        #[serde(default)]
        mode: FillMode,
    },
    /// Data › Reapply: evaluate the AutoFilter and hide the rows it filters out.
    ApplyFilter {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
    },
    /// Data › Remove Duplicates.
    RemoveDuplicates {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        range: Range,
        /// Absolute columns compared (empty = all).
        #[serde(default)]
        columns: Vec<u32>,
        /// The first row is a header.
        #[serde(default)]
        header: bool,
    },
    /// Data › Text to Columns (the source is one column).
    TextToColumns {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        range: Range,
        #[allow(missing_docs)]
        options: TextToColumns,
    },
    /// Add a sheet.
    AddSheet {
        #[allow(missing_docs)]
        #[serde(default)]
        name: Option<String>,
        #[allow(missing_docs)]
        #[serde(default)]
        index: Option<usize>,
        #[allow(missing_docs)]
        #[serde(default)]
        id: Option<u32>,
    },
    /// Remove a sheet.
    RemoveSheet {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
    },
    /// Put back a removed sheet (the inverse of [`Command::RemoveSheet`]).
    RestoreSheet {
        #[allow(missing_docs)]
        snapshot: Box<SheetSnapshot>,
        #[allow(missing_docs)]
        index: usize,
    },
    /// Rename a sheet.
    RenameSheet {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        name: String,
    },
    /// Move a sheet in the tab order.
    MoveSheet {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        index: usize,
    },
    /// Change properties of rows `at..at+count`.
    SetRowProps {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        at: u32,
        #[allow(missing_docs)]
        count: u32,
        #[allow(missing_docs)]
        patch: RowPatch,
    },
    /// Change properties of columns.
    SetColProps {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        at: u32,
        #[allow(missing_docs)]
        count: u32,
        #[allow(missing_docs)]
        patch: ColPatch,
    },
    /// Replace row properties (undo of row changes).
    SetRowEntries {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        entries: Vec<(u32, Option<RowProps>)>,
    },
    /// Replace column properties.
    SetColEntries {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        entries: Vec<(u32, Option<ColProps>)>,
    },
    /// Merge cells.
    Merge {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        range: Range,
        #[allow(missing_docs)]
        #[serde(default)]
        mode: MergeMode,
    },
    /// Unmerge every merge intersecting the range.
    Unmerge {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        range: Range,
    },
    /// Define or redefine a name.
    DefineName {
        #[allow(missing_docs)]
        name: String,
        #[allow(missing_docs)]
        formula: String,
        #[allow(missing_docs)]
        #[serde(default, with = "opt_sheet")]
        scope: Option<SheetId>,
        #[allow(missing_docs)]
        #[serde(default)]
        hidden: bool,
        #[allow(missing_docs)]
        #[serde(default)]
        comment: Option<String>,
    },
    /// Delete a name.
    DeleteName {
        #[allow(missing_docs)]
        name: String,
        #[allow(missing_docs)]
        #[serde(default, with = "opt_sheet")]
        scope: Option<SheetId>,
    },
    /// Insert › Table (an id of 0 allocates one).
    AddTable {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        table: Box<Table>,
    },
    /// Table Design › Convert to Range: structured references become A1 references.
    RemoveTable {
        #[allow(missing_docs)]
        name: String,
    },
    /// Change a table.
    UpdateTable {
        #[allow(missing_docs)]
        name: String,
        #[allow(missing_docs)]
        patch: Box<TablePatch>,
    },
    /// View › Freeze Panes (0, 0 unfreezes).
    SetFreeze {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        rows: u32,
        #[allow(missing_docs)]
        cols: u32,
    },
    /// Replace one metadata field.
    SetSheetMeta {
        #[allow(missing_docs)]
        #[serde(with = "sheet_id")]
        sheet: SheetId,
        #[allow(missing_docs)]
        field: Box<MetaField>,
    },
    /// Replace the workbook properties.
    SetProps {
        #[allow(missing_docs)]
        props: Box<WorkbookProps>,
    },
    /// Protect or unprotect the workbook structure.
    SetWorkbookProtection {
        #[allow(missing_docs)]
        protection: Option<WorkbookProtection>,
    },
    /// Several commands as one step; a failing member rolls the batch back.
    Batch {
        #[allow(missing_docs)]
        commands: Vec<Command>,
        #[allow(missing_docs)]
        #[serde(default)]
        label: Option<String>,
    },
    /// Put back cells, metadata, layout and names exactly (inverse of workbook-wide edits).
    Restore {
        #[allow(missing_docs)]
        #[serde(default)]
        cells: Vec<SheetCells>,
        #[allow(missing_docs)]
        #[serde(default)]
        meta: Vec<SheetMetaEntry>,
        #[allow(missing_docs)]
        #[serde(default)]
        layout: Vec<SheetLayoutEntry>,
        #[allow(missing_docs)]
        #[serde(default)]
        names: Option<Vec<DefinedName>>,
    },
}

impl Command {
    /// A command that does nothing.
    pub fn noop() -> Command {
        Command::Batch {
            commands: Vec::new(),
            label: None,
        }
    }

    /// True for a batch with no commands.
    pub fn is_noop(&self) -> bool {
        matches!(self, Command::Batch { commands, .. } if commands.iter().all(Command::is_noop))
    }

    /// The command's name (`setCells`).
    pub fn kind(&self) -> &'static str {
        match self {
            Command::SetCells { .. } => "setCells",
            Command::SetRange { .. } => "setRange",
            Command::ClearRange { .. } => "clearRange",
            Command::SetStyle { .. } => "setStyle",
            Command::SetStyleIndex { .. } => "setStyleIndex",
            Command::InsertRows { .. } => "insertRows",
            Command::InsertCols { .. } => "insertCols",
            Command::DeleteRows { .. } => "deleteRows",
            Command::DeleteCols { .. } => "deleteCols",
            Command::MoveRange { .. } => "moveRange",
            Command::CopyRange { .. } => "copyRange",
            Command::Sort { .. } => "sort",
            Command::AutoFill { .. } => "autoFill",
            Command::ApplyFilter { .. } => "applyFilter",
            Command::RemoveDuplicates { .. } => "removeDuplicates",
            Command::TextToColumns { .. } => "textToColumns",
            Command::AddSheet { .. } => "addSheet",
            Command::RemoveSheet { .. } => "removeSheet",
            Command::RestoreSheet { .. } => "restoreSheet",
            Command::RenameSheet { .. } => "renameSheet",
            Command::MoveSheet { .. } => "moveSheet",
            Command::SetRowProps { .. } => "setRowProps",
            Command::SetColProps { .. } => "setColProps",
            Command::SetRowEntries { .. } => "setRowEntries",
            Command::SetColEntries { .. } => "setColEntries",
            Command::Merge { .. } => "merge",
            Command::Unmerge { .. } => "unmerge",
            Command::DefineName { .. } => "defineName",
            Command::DeleteName { .. } => "deleteName",
            Command::AddTable { .. } => "addTable",
            Command::RemoveTable { .. } => "removeTable",
            Command::UpdateTable { .. } => "updateTable",
            Command::SetFreeze { .. } => "setFreeze",
            Command::SetSheetMeta { .. } => "setSheetMeta",
            Command::SetProps { .. } => "setProps",
            Command::SetWorkbookProtection { .. } => "setWorkbookProtection",
            Command::Batch { .. } => "batch",
            Command::Restore { .. } => "restore",
        }
    }

    /// The command as JSON (journals, the control channel).
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| ModelError::invalid(e.to_string()))
    }

    /// A command from JSON.
    pub fn from_json(text: &str) -> Result<Command> {
        serde_json::from_str(text)
            .map_err(|e| ModelError::invalid(format!("not a sheet command: {e}")))
    }

    /// The command as postcard bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        postcard::to_allocvec(self).map_err(|e| ModelError::invalid(e.to_string()))
    }

    /// A command from postcard bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Command> {
        postcard::from_bytes(bytes)
            .map_err(|e| ModelError::invalid(format!("not a sheet command: {e}")))
    }
}
