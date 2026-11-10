// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Grid commands and queries: navigation and selection, in-cell editing with
//! autocomplete and point mode, fill, rows and columns, sheets, cell formats, frozen panes and
//! the view, Paste Special. Bold, italic, fonts, horizontal alignment, indent, the clipboard and
//! the date and time are the shared editing commands (`suite.format.*`, `suite.edit.*`), which
//! Grid applies to the selected cells.
//!
//! Positions are zero-based rows and columns of the active sheet. Points are CSS pixels of the
//! grid surface (headers included); colours are `RRGGBB` without `#`; offsets in editor text are
//! UTF-16 code units, as the DOM counts them.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::app::AppMask;
use crate::commands::edit::ClipboardData;
use crate::geom::Rect;

pub mod analysis;
pub mod features;
pub mod home;
pub mod surface;

use features::{
    GridArrows, GridBreakAction, GridCalcMode, GridCfRule, GridColumnFilter, GridFeatureState,
    GridFindSpec, GridImport, GridLink, GridPageSetup, GridPrintAreaAction, GridSortOn,
    GridSortSpec, GridSpecial, GridTableOptions, GridTextToColumns, GridTotals, GridValidation,
};

const G: AppMask = AppMask::GRID;

/// A direction on the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridDirection {
    /// Towards row 1.
    Up,
    /// Towards the last row.
    Down,
    /// Towards column A.
    Left,
    /// Towards the last column.
    Right,
}

/// A rectangle of cells (inclusive, zero-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridRange {
    /// First row.
    pub r0: u32,
    /// First column.
    pub c0: u32,
    /// Last row.
    pub r1: u32,
    /// Last column.
    pub c1: u32,
}

/// Where Insert Cells and Delete Cells move the neighbours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum CellShift {
    /// Shift cells down (insert) or up (delete).
    Vertical,
    /// Shift cells right (insert) or left (delete).
    Horizontal,
    /// Entire rows.
    Rows,
    /// Entire columns.
    Columns,
    /// Excel's choice: whole rows or columns when they are selected, else shift vertically.
    Auto,
}

/// What Home ▸ Clear clears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridClear {
    /// Everything.
    All,
    /// Values and formulas (the Delete key).
    Contents,
    /// Formats.
    Formats,
    /// Notes and comments.
    Comments,
    /// Hyperlinks.
    Hyperlinks,
}

/// What Paste Special pastes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridPaste {
    /// Everything.
    All,
    /// Values only.
    Values,
    /// Formulas (adjusted) and values.
    Formulas,
    /// Formats only.
    Formats,
    /// Values and number formats.
    ValuesAndNumberFormats,
    /// Everything but borders.
    AllExceptBorders,
    /// Formulas and number formats.
    FormulasAndNumberFormats,
    /// Notes and threaded comments only.
    Comments,
    /// Data validation only.
    Validation,
    /// The source columns' widths only.
    ColumnWidths,
}

/// Paste Special's arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridPasteOp {
    /// Replace.
    None,
    /// Add to the destination.
    Add,
    /// Subtract from it.
    Subtract,
    /// Multiply it.
    Multiply,
    /// Divide it.
    Divide,
}

/// The fill handle's options (Auto Fill Options).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridFill {
    /// What dragging does: series for dates and numbered text, trends for two or more
    /// numbers, copies otherwise.
    Auto,
    /// Copy Cells.
    Copy,
    /// Fill Series.
    Series,
    /// Fill Formatting Only.
    FormatsOnly,
    /// Fill Without Formatting.
    WithoutFormatting,
    /// Fill Days.
    Days,
    /// Fill Weekdays.
    Weekdays,
    /// Fill Months.
    Months,
    /// Fill Years.
    Years,
    /// Linear Trend.
    LinearTrend,
    /// Growth Trend.
    Growth,
}

/// Border presets (Home ▸ Borders).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridBorders {
    /// Every edge of every cell.
    All,
    /// The outline of the selection.
    Outside,
    /// The outline in a thick line.
    ThickBox,
    /// The lines between cells.
    Inside,
    /// The top edge.
    Top,
    /// The bottom edge.
    Bottom,
    /// The left edge.
    Left,
    /// The right edge.
    Right,
    /// A double bottom edge.
    BottomDouble,
    /// A thick bottom edge.
    BottomThick,
    /// Top and bottom.
    TopBottom,
    /// No borders.
    None,
}

/// A border line style (the Format Cells line gallery).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridLineStyle {
    /// No line.
    None,
    /// Thin.
    Thin,
    /// Medium.
    Medium,
    /// Thick.
    Thick,
    /// Double.
    Double,
    /// Dotted.
    Dotted,
    /// Dashed.
    Dashed,
    /// Hair.
    Hair,
    /// Medium dashed.
    MediumDashed,
    /// Dash dot.
    DashDot,
    /// Medium dash dot.
    MediumDashDot,
    /// Dash dot dot.
    DashDotDot,
    /// Medium dash dot dot.
    MediumDashDotDot,
    /// Slanted dash dot.
    SlantDashDot,
}

/// Horizontal alignment of cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridHAlign {
    /// Text left, numbers right.
    General,
    /// Left.
    Left,
    /// Centre.
    Center,
    /// Right.
    Right,
    /// Repeat to fill the cell.
    Fill,
    /// Justify.
    Justify,
    /// Centre across selection.
    CenterAcross,
    /// Distributed.
    Distributed,
}

/// Vertical alignment of cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridVAlign {
    /// Top.
    Top,
    /// Middle.
    Center,
    /// Bottom.
    Bottom,
    /// Justify.
    Justify,
    /// Distributed.
    Distributed,
}

/// One border side of a format.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct GridBorderSide {
    /// Line style.
    pub style: GridLineStyle,
    /// `RRGGBB`, `None` for automatic.
    pub color: Option<String>,
}

/// A change of cell format (Format Cells ▸ OK): members left `None` stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct GridFormat {
    /// Number format code.
    pub number_format: Option<String>,
    /// Font family.
    pub font_family: Option<String>,
    /// Font size in points.
    pub font_size: Option<f64>,
    /// Bold.
    pub bold: Option<bool>,
    /// Italic.
    pub italic: Option<bool>,
    /// Underline: `none`, `single`, `double`, `singleAccounting`, `doubleAccounting`.
    pub underline: Option<String>,
    /// Strikethrough.
    pub strike: Option<bool>,
    /// Superscript (`true`), subscript (`false`); `None` keeps it.
    pub superscript: Option<bool>,
    /// Subscript.
    pub subscript: Option<bool>,
    /// Font colour `RRGGBB` (`""` = automatic).
    pub font_color: Option<String>,
    /// Fill colour `RRGGBB` (`""` = no fill).
    pub fill_color: Option<String>,
    /// Pattern (`none`, `solid`, `gray125`, `lightGrid`…) with `pattern_color`.
    pub pattern: Option<String>,
    /// Pattern colour `RRGGBB`.
    pub pattern_color: Option<String>,
    /// Horizontal alignment.
    pub h_align: Option<GridHAlign>,
    /// Vertical alignment.
    pub v_align: Option<GridVAlign>,
    /// Wrap text.
    pub wrap: Option<bool>,
    /// Shrink to fit.
    pub shrink: Option<bool>,
    /// Indent level 0–250.
    pub indent: Option<u8>,
    /// Orientation: 0–90 counter-clockwise, 91–180 clockwise (90 + degrees), 255 stacked.
    pub rotation: Option<u8>,
    /// Merge the selection (`false` unmerges).
    pub merge: Option<bool>,
    /// Left border.
    pub border_left: Option<GridBorderSide>,
    /// Right border.
    pub border_right: Option<GridBorderSide>,
    /// Top border.
    pub border_top: Option<GridBorderSide>,
    /// Bottom border.
    pub border_bottom: Option<GridBorderSide>,
    /// Lines between rows of the selection.
    pub border_inside_h: Option<GridBorderSide>,
    /// Lines between columns of the selection.
    pub border_inside_v: Option<GridBorderSide>,
    /// Diagonal up (bottom-left to top-right).
    pub border_diagonal_up: Option<GridBorderSide>,
    /// Diagonal down.
    pub border_diagonal_down: Option<GridBorderSide>,
    /// Locked (Protection).
    pub locked: Option<bool>,
    /// Formula hidden (Protection).
    pub hidden: Option<bool>,
}

/// Kiln Grid commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum GridCommand {
    // ── navigation and selection ───────────────────────────────────────────────────────────
    /// Arrow keys: one cell; with `extend` the selection grows (Shift). `Left` and `Right` are
    /// the keys: on a right-to-left sheet Right goes towards column A, as in Excel.
    Move {
        /// Direction.
        dir: GridDirection,
        /// Extend the selection.
        extend: bool,
    },
    /// Ctrl+Arrows: to the edge of the data region (Ctrl+Shift extends). The keys, as in
    /// [`GridCommand::Move`].
    Jump {
        /// Direction.
        dir: GridDirection,
        /// Extend the selection.
        extend: bool,
    },
    /// Page Up/Down (Alt+Page Up/Down: left/right) by one screen.
    Page {
        /// Direction.
        dir: GridDirection,
        /// Extend the selection.
        extend: bool,
    },
    /// Home: the first column of the row (after frozen columns, as Excel).
    RowStart {
        /// Extend the selection.
        extend: bool,
    },
    /// Ctrl+Home: the first cell (after frozen panes).
    FirstCell {
        /// Extend the selection.
        extend: bool,
    },
    /// Ctrl+End: the last used cell.
    LastCell {
        /// Extend the selection.
        extend: bool,
    },
    /// Enter, Shift+Enter, Tab and Shift+Tab without editing: the next cell, inside the
    /// selection when it has several cells.
    Advance {
        /// Against the direction (Shift).
        backward: bool,
        /// Along the row (Tab) instead of the Enter direction.
        across: bool,
    },
    /// Selects a cell (click), extends to it (Shift+click) or adds it (Ctrl+click).
    SelectCell {
        /// Row.
        row: u32,
        /// Column.
        col: u32,
        /// Extend from the active cell.
        extend: bool,
        /// Add a new range (Ctrl).
        add: bool,
    },
    /// Selects a range (the name box, a drag).
    SelectRange {
        /// The range.
        range: GridRange,
        /// Add it to the selection.
        add: bool,
    },
    /// Selects whole rows (row header click and drag).
    SelectRows {
        /// First row.
        from: u32,
        /// Last row.
        to: u32,
        /// Extend from the active row.
        extend: bool,
        /// Add to the selection.
        add: bool,
    },
    /// Selects whole columns.
    SelectColumns {
        /// First column.
        from: u32,
        /// Last column.
        to: u32,
        /// Extend from the active column.
        extend: bool,
        /// Add to the selection.
        add: bool,
    },
    /// Ctrl+A: the current region, then the whole sheet.
    SelectAll,
    /// Shift+Space: the rows of the selection.
    SelectRow,
    /// Ctrl+Space: the columns of the selection.
    SelectColumn,
    /// Go To (Ctrl+G, F5, the name box): a reference or a defined name.
    GoTo {
        /// `B12`, `Sheet2!A1:C3`, a name.
        reference: String,
        /// The name box: a valid name that is not defined yet is defined for the selection
        /// (Excel's way of naming a range); Go To refuses it.
        #[serde(default)]
        define: bool,
    },
    // ── sheets ─────────────────────────────────────────────────────────────────────────────
    /// Shows a sheet (tab click).
    ActivateSheet {
        /// Sheet index.
        index: u32,
    },
    /// Ctrl+Page Up.
    PreviousSheet,
    /// Ctrl+Page Down.
    NextSheet,
    /// Shift+F11, the + button: a new sheet after the active one.
    InsertSheet,
    /// Renames a sheet.
    RenameSheet {
        /// Sheet index.
        index: u32,
        /// New name.
        name: String,
    },
    /// Deletes a sheet.
    DeleteSheet {
        /// Sheet index.
        index: u32,
    },
    /// Moves a sheet in the tab order (drag).
    MoveSheet {
        /// From index.
        from: u32,
        /// To index.
        to: u32,
    },
    /// Duplicates a sheet (Move or Copy ▸ Create a copy).
    DuplicateSheet {
        /// Sheet index.
        index: u32,
    },
    /// Tab colour.
    SheetTabColor {
        /// Sheet index.
        index: u32,
        /// `RRGGBB`, `None` for no colour.
        color: Option<String>,
    },
    /// Hides a sheet.
    HideSheet {
        /// Sheet index.
        index: u32,
    },
    /// Shows a hidden sheet.
    UnhideSheet {
        /// Sheet index.
        index: u32,
    },
    // ── editing ────────────────────────────────────────────────────────────────────────────
    /// F2: edit the active cell in place (Edit mode, caret at the end).
    EditCell,
    /// Typing starts an entry replacing the cell (Enter mode).
    BeginEdit {
        /// What was typed.
        text: String,
    },
    /// Document Recovery's Put it back: the text that was being typed when Kiln closed goes
    /// back into its cell, found by its sheet's id (a rename since does not lose it), as an
    /// entry not yet entered.
    PutBackDraft {
        /// The sheet's id (`RecoveredDraft::sheet`).
        sheet: u32,
        /// Row.
        row: u32,
        /// Column.
        col: u32,
        /// The text.
        text: String,
    },
    /// The editor's text and selection changed (the input proxy or the formula bar).
    SetEditText {
        /// The whole text.
        text: String,
        /// Selection start, UTF-16 units.
        start: u32,
        /// Selection end (the caret), UTF-16 units.
        end: u32,
        /// The formula bar is editing (Edit mode) rather than the cell.
        formula_bar: bool,
    },
    /// Enter (down), Shift+Enter (up), Tab (right), Shift+Tab (left), or the ✓ button (stay).
    CommitEdit {
        /// Where the active cell goes; `None` stays.
        advance: Option<GridDirection>,
    },
    /// Ctrl+Enter: the entry into every selected cell (or, without an entry, the active cell's
    /// content into the selection).
    FillSelection,
    /// Ctrl+Shift+Enter: commit as an array formula over the selection.
    ArrayFormula,
    /// Esc, the ✗ button: discard the entry.
    CancelEdit,
    /// Alt+Enter: a line break in the cell.
    NewLineInCell,
    /// F4 while editing: cycle `$` on the references at the caret.
    ToggleAbsolute,
    /// Accepts an autocomplete suggestion (Tab, a click).
    AcceptCompletion {
        /// Index in the editor's completions.
        index: u32,
    },
    /// Picks the highlighted suggestion with the arrow keys.
    MoveCompletion {
        /// Down (`true`) or up.
        down: bool,
    },
    /// Point mode: a click on a cell while a formula expects a reference inserts it.
    PointRef {
        /// Row.
        row: u32,
        /// Column.
        col: u32,
        /// Extend the reference to a range.
        extend: bool,
    },
    /// Point mode: arrow keys move the inserted reference (the keys, as in
    /// [`GridCommand::Move`]).
    PointMove {
        /// Direction.
        dir: GridDirection,
        /// Extend.
        extend: bool,
    },
    /// Alt+Down: the pick list (the values of the column, or a validation list).
    OpenDropdown,
    /// Delete: clear the contents of the selection.
    ClearContents,
    /// Home ▸ Clear.
    Clear {
        /// What to clear.
        what: GridClear,
    },
    /// Ctrl+D.
    FillDown,
    /// Ctrl+R.
    FillRight,
    /// Home ▸ Fill ▸ Up.
    FillUp,
    /// Home ▸ Fill ▸ Left.
    FillLeft,
    /// The fill handle is being dragged to a cell (the preview outline).
    DragFill {
        /// Row under the pointer.
        row: u32,
        /// Column under the pointer.
        col: u32,
    },
    /// The fill handle was released (or Home ▸ Fill ▸ Series over a selection).
    AutoFill {
        /// Row of the target corner.
        row: u32,
        /// Column of the target corner.
        col: u32,
        /// Fill options.
        mode: GridFill,
    },
    /// Alt+= / Home ▸ AutoSum: `SUM` (or another function) of the data above or left.
    AutoSum {
        /// `SUM`, `AVERAGE`, `COUNT`, `MAX`, `MIN`.
        function: Option<String>,
    },
    /// Ctrl+; and Ctrl+Shift+; while not editing: today's date or the time as a value.
    InsertDateTime {
        /// The time instead of the date.
        time: bool,
        /// Minutes to add to UTC for local time (the host's time zone; JavaScript's
        /// `-getTimezoneOffset()`).
        utc_offset_minutes: i32,
    },
    /// Find Next (basic find): selects the next cell whose displayed text (or formula)
    /// contains the text.
    FindNext {
        /// What to find.
        text: String,
        /// Match case.
        match_case: bool,
        /// The whole cell must match.
        whole_cell: bool,
        /// Search backwards.
        backward: bool,
        /// Look in formulas instead of values.
        in_formulas: bool,
    },
    /// F9: recalculate every formula.
    CalculateNow,
    /// Shift+F9: recalculate the active sheet.
    CalculateSheet,
    /// Ctrl+`: formulas instead of results.
    ShowFormulas,
    // ── rows and columns ───────────────────────────────────────────────────────────────────
    /// Ctrl+Shift+= / Insert Cells.
    InsertCells {
        /// Where the neighbours go.
        shift: CellShift,
    },
    /// Ctrl+- / Delete Cells.
    DeleteCells {
        /// Where the neighbours come from.
        shift: CellShift,
    },
    /// Insert Sheet Rows above the selection.
    InsertRows,
    /// Insert Sheet Columns left of the selection.
    InsertColumns,
    /// Delete the rows of the selection.
    DeleteRows,
    /// Delete the columns of the selection.
    DeleteColumns,
    /// Format ▸ Row Height for the selected rows.
    RowHeight {
        /// Points (0–409).
        points: f64,
    },
    /// Format ▸ Column Width for the selected columns.
    ColumnWidth {
        /// Characters (0–255).
        chars: f64,
    },
    /// A row header border was dragged: rows `from..=to` (the selection's when it holds the
    /// dragged row) take the height.
    ResizeRows {
        /// First row.
        from: u32,
        /// Last row.
        to: u32,
        /// Height in CSS pixels at 100 %.
        px: f64,
    },
    /// A column header border was dragged.
    ResizeColumns {
        /// First column.
        from: u32,
        /// Last column.
        to: u32,
        /// Width in CSS pixels at 100 %.
        px: f64,
    },
    /// Double-click on a row border, Format ▸ AutoFit Row Height.
    AutoFitRows {
        /// First row (`None`: the selection's rows).
        from: Option<u32>,
        /// Last row.
        to: Option<u32>,
    },
    /// Double-click on a column border, Format ▸ AutoFit Column Width.
    AutoFitColumns {
        /// First column (`None`: the selection's columns).
        from: Option<u32>,
        /// Last column.
        to: Option<u32>,
    },
    /// Ctrl+9.
    HideRows,
    /// Ctrl+0.
    HideColumns,
    /// Ctrl+Shift+9.
    UnhideRows,
    /// Ctrl+Shift+0.
    UnhideColumns,
    // ── formats ────────────────────────────────────────────────────────────────────────────
    /// Home ▸ Borders.
    Borders {
        /// Which edges.
        preset: GridBorders,
        /// Line style (thin by default).
        style: Option<GridLineStyle>,
        /// `RRGGBB` (automatic by default).
        color: Option<String>,
    },
    /// Ctrl+Shift+&: outline border.
    BorderOutline,
    /// Ctrl+Shift+_: no borders.
    BorderNone,
    /// Home ▸ Fill Color (`None`: No Fill).
    FillColor {
        /// `RRGGBB`.
        color: Option<String>,
    },
    /// Top Align.
    AlignTop,
    /// Middle Align.
    AlignMiddle,
    /// Bottom Align.
    AlignBottom,
    /// Home ▸ Alignment for the selection (horizontal), when given explicitly.
    AlignHorizontal {
        /// The alignment.
        align: GridHAlign,
    },
    /// Orientation (degrees, 0–90 counter-clockwise, -90–-1 clockwise; 255 vertical text).
    Orientation {
        /// Angle.
        angle: i16,
    },
    /// Wrap Text (toggle).
    WrapText,
    /// Merge & Center (toggle).
    MergeCenter,
    /// Merge Across.
    MergeAcross,
    /// Merge Cells.
    MergeCells,
    /// Unmerge Cells.
    UnmergeCells,
    /// The number format combo: a preset name (`General`, `Number`, `Currency`, `Accounting`,
    /// `Short Date`, `Long Date`, `Time`, `Percentage`, `Fraction`, `Scientific`, `Text`) or a
    /// format code.
    NumberFormat {
        /// Name or code.
        format: String,
    },
    /// Ctrl+Shift+1: `#,##0.00`.
    NumberStyle,
    /// Ctrl+Shift+4: currency.
    CurrencyStyle,
    /// Ctrl+Shift+5: `0%`.
    PercentStyle,
    /// Accounting Number Format.
    AccountingStyle {
        /// `GBP`, `USD`, `EUR` (the locale's by default).
        currency: Option<String>,
    },
    /// Comma Style.
    CommaStyle,
    /// Increase Decimal.
    IncreaseDecimal,
    /// Decrease Decimal.
    DecreaseDecimal,
    /// Format Cells ▸ OK.
    ApplyFormat {
        /// What changes.
        format: Box<GridFormat>,
    },
    // ── view ───────────────────────────────────────────────────────────────────────────────
    /// Freeze Panes at the active cell (Unfreeze when frozen).
    FreezePanes,
    /// Freeze Top Row.
    FreezeTopRow,
    /// Freeze First Column.
    FreezeFirstColumn,
    /// View ▸ Split at the active cell (toggle): the panes scroll on their own.
    Split,
    /// View ▸ Gridlines (toggle).
    Gridlines,
    /// View ▸ Headings (toggle).
    Headings,
    // ── clipboard ──────────────────────────────────────────────────────────────────────────
    /// Paste Special.
    PasteSpecial {
        /// What to paste.
        what: GridPaste,
        /// Arithmetic.
        op: GridPasteOp,
        /// Skip blanks.
        skip_blanks: bool,
        /// Transpose.
        transpose: bool,
        /// What the clipboard holds (`None`: Kiln's last copy).
        data: Option<ClipboardData>,
    },
    /// Paste ▸ Values (Ctrl+Shift+V).
    PasteValues {
        /// The clipboard (`None`: Kiln's last copy).
        data: Option<ClipboardData>,
    },
    /// Paste ▸ Formulas.
    PasteFormulas {
        /// The clipboard.
        data: Option<ClipboardData>,
    },
    /// Paste ▸ Formatting.
    PasteFormatting {
        /// The clipboard.
        data: Option<ClipboardData>,
    },
    /// Paste ▸ Transpose.
    PasteTranspose {
        /// The clipboard.
        data: Option<ClipboardData>,
    },
    /// Paste ▸ Paste Link (and Paste Special ▸ Paste Link): formulas that refer to the copied
    /// cells (`=$A$1` for one cell, `=A1` for a range, with the sheet when it is another one).
    PasteLink {
        /// The clipboard (`None`: Kiln's last copy).
        data: Option<ClipboardData>,
    },
    /// Home ▸ Fill ▸ Series over the selection.
    FillSeries {
        /// The dialog's choices.
        series: home::GridSeries,
    },
    /// A choice of the Auto Fill Options button: the last fill-handle fill again with it.
    AutoFillOption {
        /// The choice.
        choice: home::GridAutoFillChoice,
    },
    /// The user's custom fill lists (File ▸ Options ▸ Advanced ▸ Edit Custom Lists), which the
    /// fill handle and Fill Series continue and the Sort dialog offers.
    SetCustomLists {
        /// Lists, each of at least two entries.
        lists: Vec<Vec<String>>,
    },
    /// Custom Lists ▸ Import: the selection's values become a new list.
    ImportCustomList,
    /// Home ▸ Borders ▸ Draw Border, Draw Border Grid, Erase Border, Line Color and Line Style:
    /// takes up the pen (Line Color and Line Style take up Draw Border unless a pen is active,
    /// as in Excel); the pen keeps its style and colour between uses.
    BorderPen {
        /// The mode (`None`: the active one, or Draw Border).
        mode: Option<home::GridPenMode>,
        /// Line style (`None`: keep it).
        style: Option<GridLineStyle>,
        /// Line colour `RRGGBB`, `""` for automatic (`None`: keep it).
        color: Option<String>,
    },
    /// Puts the border pen away (Esc, or the pen's menu item again).
    BorderPenOff,
    /// A stroke of the border pen from one point to another (surface CSS pixels); `done` on
    /// release applies it, otherwise it previews the cells.
    DrawBorder {
        /// Start x.
        x0: f32,
        /// Start y.
        y0: f32,
        /// End x.
        x1: f32,
        /// End y.
        y1: f32,
        /// The pointer was released.
        done: bool,
    },
    /// Format Cells ▸ Number ▸ Custom ▸ Delete: removes a code the workbook added; cells that
    /// use it return to General.
    DeleteNumberFormat {
        /// The code.
        code: String,
    },
    // ── drawings (charts on the sheet) ────────────────────────────────────────────────────
    /// Selects a chart on the active sheet by its drawing number (a click on it), or deselects
    /// (`None`: a click on a cell, Esc). While a chart is selected the `Chart*` commands apply
    /// to it and Delete removes it.
    SelectDrawing {
        /// The drawing number on the sheet.
        index: Option<u32>,
    },
    /// Moves or resizes a chart (a drag of it or of a sizing handle): its new rectangle in
    /// sheet pixels at 100 %.
    PlaceDrawing {
        /// The drawing number on the sheet.
        index: u32,
        /// Left.
        x: f32,
        /// Top.
        y: f32,
        /// Width.
        width: f32,
        /// Height.
        height: f32,
    },
    /// Deletes the selected chart (Delete with a chart selected).
    DeleteDrawing,
    /// Insert ▸ Pictures: a picture file over the cells at the active cell, at its own size
    /// (at most the window's), selected.
    InsertPicture {
        /// The file name (its extension decides the media type).
        name: String,
        /// The image file.
        bytes: Vec<u8>,
    },
    /// Insert ▸ Shapes: a preset shape at the active cell, selected.
    InsertShape {
        /// The preset (`rect`, `roundRect`, `ellipse`, `rightArrow`, `line`).
        preset: String,
        /// Text in it.
        text: Option<String>,
    },
    /// Picture Format and Shape Format: size, crop, alt text, fill, outline, text of the
    /// selected picture or shape.
    FormatObject {
        /// What changes.
        format: Box<features::GridObjectFormat>,
    },
    /// Reset Picture (and Size): no crop, and with `size` its own size.
    ResetPicture {
        /// Also the size.
        size: bool,
    },
    /// Bring Forward / Send Backward of the selected object.
    ArrangeDrawing {
        /// Where it goes.
        arrange: features::GridArrange,
    },
    /// The app's theme, for what the core colours by it (charts on screen). Not an edit.
    Theme {
        /// The dark theme.
        dark: bool,
    },
    // ── sort and filter ─────────────────────────────────────────────────────────────
    /// Sort A to Z: the table, filter or current region at the active cell by its column.
    SortAscending,
    /// Sort Z to A.
    SortDescending,
    /// Data ▸ Sort (the Sort dialog).
    ApplySort {
        /// What and how.
        spec: GridSortSpec,
    },
    /// Data ▸ Filter (Ctrl+Shift+L): adds or removes the AutoFilter of the current region (the
    /// table's filter buttons inside a table).
    ToggleFilter,
    /// The filter of one column of the AutoFilter (or table) whose header row is `row`.
    FilterColumn {
        /// Header row.
        row: u32,
        /// Column.
        col: u32,
        /// The filter (`None` clears it).
        filter: Option<GridColumnFilter>,
    },
    /// Sort from an AutoFilter dropdown.
    SortFilterColumn {
        /// Header row.
        row: u32,
        /// Column.
        col: u32,
        /// Z to A.
        descending: bool,
        /// Values or a colour.
        on: GridSortOn,
        /// The colour on top.
        color: Option<String>,
    },
    /// Data ▸ Clear: shows every row of the filter.
    ClearFilter,
    /// Data ▸ Reapply.
    ReapplyFilter,
    // ── conditional formatting, validation, tables, styles ──────────────────────────
    /// A new conditional-formatting rule (Highlight Cells Rules, Top/Bottom, Data Bars, Colour
    /// Scales, Icon Sets, New Rule).
    AddConditionalFormat {
        /// The rule (`applies_to` empty: the selection).
        rule: GridCfRule,
    },
    /// Manage Rules ▸ OK: the rules for the selection (or the sheet) in priority order; rules
    /// of that scope left out are deleted.
    SetConditionalFormats {
        /// The rules, highest priority first.
        rules: Vec<GridCfRule>,
        /// The scope is the sheet (otherwise the rules touching the selection).
        sheet: bool,
    },
    /// Clear Rules from Selected Cells (or the Entire Sheet).
    ClearConditionalFormats {
        /// The entire sheet.
        sheet: bool,
    },
    /// Data ▸ Data Validation for the selection (`None`: Clear All).
    SetValidation {
        /// The rule.
        rule: Option<GridValidation>,
    },
    /// Circle Invalid Data (or Clear Validation Circles).
    CircleInvalid {
        /// Show circles.
        on: bool,
    },
    /// Format as Table, Insert ▸ Table (Ctrl+T).
    CreateTable {
        /// The range (`None`: the current region).
        range: Option<GridRange>,
        /// My table has headers.
        header: bool,
        /// Style name.
        style: String,
    },
    /// Table Design ▸ Table Styles.
    TableStyle {
        /// Style name (`TableStyleMedium2`, empty for none).
        style: String,
    },
    /// Table Design ▸ Table Style Options.
    TableOptions {
        /// The options.
        options: GridTableOptions,
    },
    /// The total row function of a table column.
    TableTotals {
        /// Absolute column.
        col: u32,
        /// Function.
        totals: GridTotals,
    },
    /// Table Design ▸ Table Name.
    RenameTable {
        /// New name.
        name: String,
    },
    /// Table Design ▸ Resize Table.
    ResizeTable {
        /// The new range, headers included.
        range: GridRange,
    },
    /// Table Design ▸ Convert to Range.
    ConvertToRange,
    /// Home ▸ Cell Styles.
    ApplyCellStyle {
        /// Style name (`Good`, `Heading 1`, `Normal`).
        name: String,
    },
    /// Format Painter: the formats of `from` onto the selection (tiled as Paste Formatting).
    PaintFormat {
        /// The source.
        from: GridRange,
    },
    // ── find, notes, comments, links ────────────────────────────────────────────────
    /// Find and Replace ▸ Replace (the active cell when it matches, then the next match) or
    /// Replace All.
    Replace {
        /// What to find.
        spec: GridFindSpec,
        /// Replace with.
        with: String,
        /// Replace All.
        all: bool,
    },
    /// Go To Special.
    SelectSpecial {
        /// What to select.
        special: GridSpecial,
    },
    /// New Note, Edit Note (`None`: Delete Note) at the active cell, or at `cell`.
    SetNote {
        /// Text.
        text: Option<String>,
        /// Author of a new note.
        author: String,
        /// The cell (`D4`, on the active sheet) the note editor was opened on; `None`: the
        /// active cell (Delete Note: the selection).
        #[serde(default)]
        cell: Option<String>,
    },
    /// Show/Hide Note of the active cell (`None` toggles).
    ShowNote {
        /// Keep it shown.
        show: Option<bool>,
    },
    /// Show All Notes.
    ShowAllNotes {
        /// Show.
        show: bool,
    },
    /// New Comment or a reply at the active cell.
    AddComment {
        /// Text.
        text: String,
        /// Author.
        author: String,
        /// The thread's first comment, for a reply.
        reply_to: Option<String>,
        /// The host's clock (the comment's date).
        now_ms: u64,
    },
    /// Edit a comment.
    EditComment {
        /// Comment id.
        id: String,
        /// New text.
        text: String,
    },
    /// Delete a comment (`None`: the active cell's thread and note).
    DeleteComment {
        /// Comment id.
        id: Option<String>,
    },
    /// Resolve or reopen a thread.
    ResolveComment {
        /// The thread's first comment.
        id: String,
        /// Resolved.
        done: bool,
    },
    /// Review ▸ Next (Previous) comment or note: selects its cell.
    NextComment {
        /// Previous.
        backward: bool,
    },
    /// Notes ▸ Convert to Comments.
    ConvertNotes {
        /// Author of the converted comments when the note has none.
        author: String,
        /// The host's clock.
        now_ms: u64,
    },
    /// Insert ▸ Link (Ctrl+K) for the selection (`None`: Remove Link).
    SetLink {
        /// The link.
        link: Option<GridLink>,
    },
    // ── page layout ─────────────────────────────────────────────────────────────────
    /// Page Setup (and the ribbon's Margins, Orientation, Size, Scale to Fit): the fields given.
    PageSetup {
        /// What changes.
        setup: Box<GridPageSetup>,
    },
    /// Page Layout ▸ Print Area.
    PrintArea {
        /// Set, add or clear.
        action: GridPrintAreaAction,
    },
    /// Page Layout ▸ Breaks.
    PageBreak {
        /// Insert, remove or reset.
        action: GridBreakAction,
    },
    /// Page Break Preview: a break dragged (an automatic break dragged becomes manual).
    MovePageBreak {
        /// A column break (otherwise a row break).
        columns: bool,
        /// The break before this row (column).
        from: u32,
        /// Moved before this row (column); outside the print range removes it.
        to: u32,
    },
    /// Sheet Options ▸ Gridlines ▸ Print.
    PrintGridlines,
    /// Sheet Options ▸ Headings ▸ Print.
    PrintHeadings,
    /// Sheet Options ▸ Sheet Right-to-Left (toggle): the sheet runs from right to left, column A
    /// at the right (`sheetView rightToLeft`).
    SheetRightToLeft,
    // ── names and auditing ──────────────────────────────────────────────────────────
    /// Define Name, New or Edit in the Name Manager.
    DefineName {
        /// Name.
        name: String,
        /// Refers to.
        refers_to: String,
        /// Sheet index of a sheet-level name.
        scope: Option<u32>,
        /// Comment.
        comment: String,
        /// The name being edited (name and scope), replaced by this one.
        previous: Option<(String, Option<u32>)>,
    },
    /// Name Manager ▸ Delete.
    DeleteName {
        /// Name.
        name: String,
        /// Scope.
        scope: Option<u32>,
    },
    /// Create Names from Selection.
    CreateNames {
        /// Top row.
        top: bool,
        /// Left column.
        left: bool,
        /// Bottom row.
        bottom: bool,
        /// Right column.
        right: bool,
    },
    /// Trace Precedents of the active cell (again: one level further).
    TracePrecedents,
    /// Trace Dependents.
    TraceDependents,
    /// Remove Arrows.
    RemoveArrows {
        /// Which.
        which: GridArrows,
    },
    /// Calculation Options.
    CalculationMode {
        /// Mode.
        mode: GridCalcMode,
    },
    /// Yes on a Warning or OK on an Information validation alert: the next commit keeps the
    /// entry the cell's rule refused.
    AcceptInvalid,
    /// Options ▸ Formulas ▸ Enable iterative calculation, with its limits.
    IterativeCalculation {
        /// Iterate circular references.
        enabled: bool,
        /// Maximum Iterations (1–32,767).
        max_iterations: u32,
        /// Maximum Change.
        max_change: f64,
    },
    /// Options ▸ Advanced ▸ When calculating this workbook ▸ Use 1904 date system. The cells keep
    /// their serial numbers, so dates show four years and a day later (or earlier).
    DateSystem {
        /// The 1904 date system (otherwise 1900).
        date1904: bool,
    },
    /// Formula Lab (and Evaluate Formula) follow the active cell: the trace is kept up to date
    /// for [`GridQuery::FormulaTrace`].
    FormulaLab {
        /// Open.
        open: bool,
    },
    /// Watch Window ▸ Add Watch or Delete Watch.
    Watch {
        /// Add (otherwise delete).
        add: bool,
        /// Sheet index.
        sheet: u32,
        /// Row.
        row: u32,
        /// Column.
        col: u32,
    },
    // ── data tools, outline, spelling ───────────────────────────────────────────────
    /// From Text/CSV, From JSON.
    Import {
        /// The file and options.
        import: Box<GridImport>,
    },
    /// Refresh All: recalculates and reapplies every filter and sort.
    RefreshAll,
    /// Text to Columns on the selected column.
    TextToColumns {
        /// Options.
        options: GridTextToColumns,
    },
    /// Remove Duplicates in the selection's region.
    RemoveDuplicates {
        /// Absolute columns compared (empty: all).
        columns: Vec<u32>,
        /// My data has headers (`None`: detect).
        header: Option<bool>,
    },
    /// Data ▸ Group.
    Group {
        /// Columns (otherwise rows).
        columns: bool,
    },
    /// Data ▸ Ungroup.
    Ungroup {
        /// Columns.
        columns: bool,
    },
    /// Show Detail or Hide Detail of the group at the selection.
    OutlineDetail {
        /// Show.
        show: bool,
    },
    /// Show or hide the outline symbols.
    OutlineSymbols,
    /// an outline level button (1, 2, 3…): shows the rows (or columns) up to that level
    /// and hides the deeper ones.
    OutlineLevel {
        /// The columns' outline (otherwise the rows').
        columns: bool,
        /// The level (1 shows only the outermost summaries).
        level: u8,
    },
    /// a group's − or + button at its summary row or column: hides or shows its detail.
    OutlineToggle {
        /// Columns.
        columns: bool,
        /// The summary row or column.
        index: u32,
    },
    /// Spelling ▸ Change (Change All).
    SpellChange {
        /// Sheet index.
        sheet: u32,
        /// Row.
        row: u32,
        /// Column.
        col: u32,
        /// Offset of the word in the cell text, in characters.
        offset: u32,
        /// The word.
        word: String,
        /// The replacement.
        with: String,
        /// Change All.
        all: bool,
    },
    /// Spelling ▸ Ignore All (or Add to Dictionary).
    SpellIgnore {
        /// The word.
        word: String,
        /// Add to the dictionary.
        add: bool,
    },
    /// The en-US dictionary's files for the web core.
    LoadDictionary {
        /// `.aff` text.
        aff: String,
        /// `.dic` text.
        dic: String,
    },
    // ── analysis ──────────────────────────────────────────────────────────────────
    /// Insert ▸ PivotTable: a report of a range or table, on a new sheet (`to` `None`) or at
    /// a cell; it starts empty, with the field list open.
    CreatePivot {
        /// The source: a range (`Sales!$A$1:$G$401`) or a table name.
        source: String,
        /// The top-left of the report (`Sheet2!$A$3`).
        to: Option<String>,
    },
    /// A change to the PivotTable at the active cell; the report is written again.
    Pivot {
        /// What changes.
        edit: analysis::GridPivotEdit,
    },
    /// PivotTable Analyze ▸ Refresh (`all`: Refresh All, every PivotTable of the workbook).
    RefreshPivots {
        /// Every PivotTable.
        all: bool,
    },
    /// Show Details (double-click a value): its source rows on a new sheet.
    PivotDetails {
        /// Row of the value.
        row: u32,
        /// Column of the value.
        col: u32,
    },
    /// Insert ▸ Sparklines.
    InsertSparklines {
        /// Type.
        kind: analysis::GridSparkKind,
        /// Data range.
        data: String,
        /// Location range (one cell per row or column of the data).
        location: String,
    },
    /// The Sparkline tab.
    Sparklines {
        /// What changes.
        edit: analysis::GridSparkEdit,
    },
    /// Data ▸ What-If Analysis ▸ Goal Seek.
    GoalSeek {
        /// Set cell (a formula).
        set_cell: String,
        /// To value.
        to_value: String,
        /// By changing cell (a constant).
        by_changing: String,
    },
    /// Data ▸ Subtotal (`None`: Remove All).
    Subtotal {
        /// The dialog's settings.
        spec: Option<analysis::GridSubtotalSpec>,
    },
    /// Flash Fill (Ctrl+E): fills the active cell's column from the examples typed above.
    FlashFill,
    /// Review ▸ Protect Sheet / Unprotect Sheet.
    ProtectSheet {
        /// Protect (otherwise unprotect).
        protect: bool,
        /// Password (empty for none).
        password: String,
        /// What users may still do (protecting).
        allow: Option<analysis::GridSheetPermissions>,
    },
    /// Review ▸ Protect Workbook (structure) and its removal.
    ProtectWorkbook {
        /// Protect (otherwise unprotect).
        protect: bool,
        /// Password (empty for none).
        password: String,
    },
    /// Home ▸ Format ▸ Lock Cell: the selection's Locked setting.
    LockCells {
        /// Locked.
        locked: bool,
    },
    /// Page Layout ▸ Themes, Colors and Fonts.
    SetTheme {
        /// Theme id or name.
        theme: String,
        /// What it takes.
        part: analysis::GridThemePart,
    },
    // ── surface gestures ──────────────────────────────────────────────────────────
    /// F8 (Extend Selection) and Shift+F8 (Add or Remove Selection): turns the mode on, or off
    /// when it is already on; `Normal` turns either off.
    SelectionMode {
        /// The mode.
        mode: surface::GridSelectMode,
    },
    /// A drag of the selection's border: moves the cells (references follow, as Cut and
    /// Paste), copies them with Ctrl, and inserts them between the cells at the destination
    /// with Shift. `row`/`col` is the cell under the pointer, `grab_row`/`grab_col` the cell
    /// of the selection the drag started on.
    DragCells {
        /// Preview, drop or cancel.
        phase: surface::GridDragPhase,
        /// Row under the pointer.
        row: u32,
        /// Column under the pointer.
        col: u32,
        /// Row the drag started on.
        grab_row: u32,
        /// Column the drag started on.
        grab_col: u32,
        /// Copy (Ctrl).
        copy: bool,
        /// Insert, shifting the cells at the destination (Shift).
        insert: bool,
        /// Overwrite cells that hold data (the answer to Excel's question).
        replace: bool,
    },
    /// The context menu's Filter ▸ Filter by Selected Cell's Value (or its fill or font
    /// colour): filters the active cell's column by it, adding an AutoFilter when there is none.
    FilterBySelection {
        /// What the filter keeps.
        by: surface::GridSelectionKey,
    },
    /// The context menu's Sort ▸ Put Selected Cell Color (Font Color) On Top.
    SortBySelectionColor {
        /// The font colour (otherwise the fill colour).
        font: bool,
    },
    /// The error indicator's Ignore Error: the selection's cells stop being flagged.
    IgnoreError,
    /// The error indicator's Convert to Number (a number stored as text).
    ConvertToNumber,
}

crate::command_table! {
    GridCommand => GRID_COMMANDS;
    Self::Move { .. } => { id: "grid.nav.move", keytip: None, group: "nav", tier: P0, apps: G, availability: Document };
    Self::Jump { .. } => { id: "grid.nav.jump", keytip: None, group: "nav", tier: P0, apps: G, availability: Document };
    Self::Page { .. } => { id: "grid.nav.page", keytip: None, group: "nav", tier: P0, apps: G, availability: Document };
    Self::RowStart { .. } => { id: "grid.nav.row_start", keytip: None, group: "nav", tier: P0, apps: G, availability: Document };
    Self::FirstCell { .. } => { id: "grid.nav.first_cell", keytip: None, group: "nav", tier: P0, apps: G, availability: Document };
    Self::LastCell { .. } => { id: "grid.nav.last_cell", keytip: None, group: "nav", tier: P0, apps: G, availability: Document };
    Self::Advance { .. } => { id: "grid.nav.advance", keytip: None, group: "nav", tier: P0, apps: G, availability: Document };
    Self::SelectCell { .. } => { id: "grid.nav.select_cell", keytip: None, group: "nav", tier: P0, apps: G, availability: Document };
    Self::SelectRange { .. } => { id: "grid.nav.select_range", keytip: None, group: "nav", tier: P0, apps: G, availability: Document };
    Self::SelectRows { .. } => { id: "grid.nav.select_rows", keytip: None, group: "nav", tier: P0, apps: G, availability: Document };
    Self::SelectColumns { .. } => { id: "grid.nav.select_columns", keytip: None, group: "nav", tier: P0, apps: G, availability: Document };
    Self::SelectAll => { id: "grid.nav.select_all", keytip: Some("SA"), group: "nav", tier: P0, apps: G, availability: Document };
    Self::SelectRow => { id: "grid.editing.select_row", keytip: None, group: "editing", tier: P0, apps: G, availability: Document };
    Self::SelectColumn => { id: "grid.editing.select_column", keytip: None, group: "editing", tier: P0, apps: G, availability: Document };
    Self::GoTo { .. } => { id: "grid.nav.go_to", keytip: Some("G"), group: "nav", tier: P0, apps: G, availability: Document };
    Self::ActivateSheet { .. } => { id: "grid.sheet.activate", keytip: None, group: "sheet", tier: P0, apps: G, availability: Document };
    Self::PreviousSheet => { id: "grid.cells.previous_sheet", keytip: None, group: "cells", tier: P0, apps: G, availability: Document };
    Self::NextSheet => { id: "grid.cells.next_sheet", keytip: None, group: "cells", tier: P0, apps: G, availability: Document };
    Self::InsertSheet => { id: "grid.cells.new_sheet", keytip: Some("S"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::RenameSheet { .. } => { id: "grid.cells.rename_sheet", keytip: Some("R"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::DeleteSheet { .. } => { id: "grid.sheet.delete", keytip: None, group: "sheet", tier: P0, apps: G, availability: Editable };
    Self::MoveSheet { .. } => { id: "grid.sheet.move", keytip: None, group: "sheet", tier: P0, apps: G, availability: Editable };
    Self::DuplicateSheet { .. } => { id: "grid.sheet.duplicate", keytip: None, group: "sheet", tier: P0, apps: G, availability: Editable };
    Self::SheetTabColor { .. } => { id: "grid.sheet.tab_color", keytip: Some("T"), group: "sheet", tier: P0, apps: G, availability: Editable };
    Self::HideSheet { .. } => { id: "grid.sheet.hide", keytip: None, group: "sheet", tier: P0, apps: G, availability: Editable };
    Self::UnhideSheet { .. } => { id: "grid.sheet.unhide", keytip: None, group: "sheet", tier: P0, apps: G, availability: Editable };
    Self::EditCell => { id: "grid.editing.edit_cell", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::BeginEdit { .. } => { id: "grid.editing.begin", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::PutBackDraft { .. } => { id: "grid.editing.put_back_draft", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::SetEditText { .. } => { id: "grid.editing.set_text", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::CommitEdit { .. } => { id: "grid.editing.commit", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::FillSelection => { id: "grid.editing.fill_selection", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::ArrayFormula => { id: "grid.editing.array_formula", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::CancelEdit => { id: "grid.editing.cancel_edit", keytip: None, group: "editing", tier: P0, apps: G, availability: Document };
    Self::NewLineInCell => { id: "grid.editing.new_line_in_cell", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::ToggleAbsolute => { id: "grid.editing.toggle_absolute", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::AcceptCompletion { .. } => { id: "grid.editing.accept_completion", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::MoveCompletion { .. } => { id: "grid.editing.move_completion", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::PointRef { .. } => { id: "grid.editing.point_ref", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::PointMove { .. } => { id: "grid.editing.point_move", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::OpenDropdown => { id: "grid.editing.open_dropdown", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::ClearContents => { id: "grid.editing.clear_contents", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::Clear { .. } => { id: "grid.editing.clear", keytip: Some("E"), group: "editing", tier: P0, apps: G, availability: Editable };
    Self::FillDown => { id: "grid.editing.fill_down", keytip: Some("D"), group: "editing", tier: P0, apps: G, availability: Editable };
    Self::FillRight => { id: "grid.editing.fill_right", keytip: Some("R"), group: "editing", tier: P0, apps: G, availability: Editable };
    Self::FillUp => { id: "grid.editing.fill_up", keytip: Some("U"), group: "editing", tier: P0, apps: G, availability: Editable };
    Self::FillLeft => { id: "grid.editing.fill_left", keytip: Some("L"), group: "editing", tier: P0, apps: G, availability: Editable };
    Self::DragFill { .. } => { id: "grid.editing.drag_fill", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::AutoFill { .. } => { id: "grid.editing.auto_fill", keytip: Some("S"), group: "editing", tier: P0, apps: G, availability: Editable };
    Self::AutoSum { .. } => { id: "grid.editing.auto_sum", keytip: Some("U"), group: "editing", tier: P0, apps: G, availability: Editable };
    Self::InsertDateTime { .. } => { id: "grid.editing.insert_date_time", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::FindNext { .. } => { id: "grid.editing.find_next", keytip: Some("FD"), group: "editing", tier: P0, apps: G, availability: Document };
    Self::CalculateNow => { id: "grid.formulas.calculate_now", keytip: Some("B"), group: "formulas", tier: P0, apps: G, availability: Document };
    Self::CalculateSheet => { id: "grid.formulas.calculate_sheet", keytip: Some("J"), group: "formulas", tier: P0, apps: G, availability: Document };
    Self::ShowFormulas => { id: "grid.formulas.show_formulas", keytip: Some("H"), group: "formulas", tier: P0, apps: G, availability: Document };
    Self::InsertCells { .. } => { id: "grid.cells.insert_cells", keytip: Some("I"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::DeleteCells { .. } => { id: "grid.cells.delete_cells", keytip: Some("D"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::InsertRows => { id: "grid.cells.insert_rows", keytip: Some("R"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::InsertColumns => { id: "grid.cells.insert_columns", keytip: Some("C"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::DeleteRows => { id: "grid.cells.delete_rows", keytip: Some("R"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::DeleteColumns => { id: "grid.cells.delete_columns", keytip: Some("C"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::RowHeight { .. } => { id: "grid.cells.row_height", keytip: Some("H"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::ColumnWidth { .. } => { id: "grid.cells.column_width", keytip: Some("W"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::ResizeRows { .. } => { id: "grid.cells.resize_rows", keytip: None, group: "cells", tier: P0, apps: G, availability: Editable };
    Self::ResizeColumns { .. } => { id: "grid.cells.resize_columns", keytip: None, group: "cells", tier: P0, apps: G, availability: Editable };
    Self::AutoFitRows { .. } => { id: "grid.cells.autofit_rows", keytip: Some("A"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::AutoFitColumns { .. } => { id: "grid.cells.autofit_columns", keytip: Some("I"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::HideRows => { id: "grid.cells.hide_rows", keytip: Some("R"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::HideColumns => { id: "grid.cells.hide_columns", keytip: Some("C"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::UnhideRows => { id: "grid.cells.unhide_rows", keytip: Some("O"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::UnhideColumns => { id: "grid.cells.unhide_columns", keytip: Some("L"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::Borders { .. } => { id: "grid.font.borders", keytip: Some("B"), group: "font", tier: P0, apps: G, availability: Editable };
    Self::BorderOutline => { id: "grid.font.border_outline", keytip: Some("S"), group: "font", tier: P0, apps: G, availability: Editable };
    Self::BorderNone => { id: "grid.font.border_none", keytip: Some("N"), group: "font", tier: P0, apps: G, availability: Editable };
    Self::FillColor { .. } => { id: "grid.font.fill_color", keytip: Some("H"), group: "font", tier: P0, apps: G, availability: Editable };
    Self::AlignTop => { id: "grid.alignment.align_top", keytip: Some("AT"), group: "alignment", tier: P0, apps: G, availability: Editable };
    Self::AlignMiddle => { id: "grid.alignment.align_middle", keytip: Some("AM"), group: "alignment", tier: P0, apps: G, availability: Editable };
    Self::AlignBottom => { id: "grid.alignment.align_bottom", keytip: Some("AB"), group: "alignment", tier: P0, apps: G, availability: Editable };
    Self::AlignHorizontal { .. } => { id: "grid.alignment.horizontal", keytip: None, group: "alignment", tier: P0, apps: G, availability: Editable };
    Self::Orientation { .. } => { id: "grid.alignment.orientation", keytip: Some("FQ"), group: "alignment", tier: P0, apps: G, availability: Editable };
    Self::WrapText => { id: "grid.alignment.wrap_text", keytip: Some("W"), group: "alignment", tier: P0, apps: G, availability: Editable };
    Self::MergeCenter => { id: "grid.alignment.merge_center", keytip: Some("M"), group: "alignment", tier: P0, apps: G, availability: Editable };
    Self::MergeAcross => { id: "grid.alignment.merge_across", keytip: Some("A"), group: "alignment", tier: P0, apps: G, availability: Editable };
    Self::MergeCells => { id: "grid.alignment.merge_cells", keytip: Some("M"), group: "alignment", tier: P0, apps: G, availability: Editable };
    Self::UnmergeCells => { id: "grid.alignment.unmerge_cells", keytip: Some("U"), group: "alignment", tier: P0, apps: G, availability: Editable };
    Self::NumberFormat { .. } => { id: "grid.number.number_format", keytip: Some("N"), group: "number", tier: P0, apps: G, availability: Editable };
    Self::NumberStyle => { id: "grid.number.number", keytip: None, group: "number", tier: P0, apps: G, availability: Editable };
    Self::CurrencyStyle => { id: "grid.number.currency", keytip: None, group: "number", tier: P0, apps: G, availability: Editable };
    Self::PercentStyle => { id: "grid.number.percent", keytip: Some("P"), group: "number", tier: P0, apps: G, availability: Editable };
    Self::AccountingStyle { .. } => { id: "grid.number.accounting", keytip: Some("AN"), group: "number", tier: P0, apps: G, availability: Editable };
    Self::CommaStyle => { id: "grid.number.comma", keytip: Some("K"), group: "number", tier: P0, apps: G, availability: Editable };
    Self::IncreaseDecimal => { id: "grid.number.increase_decimal", keytip: Some("0"), group: "number", tier: P0, apps: G, availability: Editable };
    Self::DecreaseDecimal => { id: "grid.number.decrease_decimal", keytip: Some("9"), group: "number", tier: P0, apps: G, availability: Editable };
    Self::ApplyFormat { .. } => { id: "grid.format.apply", keytip: None, group: "format", tier: P0, apps: G, availability: Editable };
    Self::FreezePanes => { id: "grid.view.freeze_panes", keytip: Some("F"), group: "view", tier: P0, apps: G, availability: Document };
    Self::FreezeTopRow => { id: "grid.view.freeze_top_row", keytip: Some("R"), group: "view", tier: P0, apps: G, availability: Document };
    Self::FreezeFirstColumn => { id: "grid.view.freeze_first_column", keytip: Some("C"), group: "view", tier: P0, apps: G, availability: Document };
    Self::Split => { id: "grid.view.split", keytip: Some("S"), group: "view", tier: P0, apps: G, availability: Document };
    Self::Gridlines => { id: "grid.view.gridlines", keytip: Some("VG"), group: "view", tier: P0, apps: G, availability: Document };
    Self::Headings => { id: "grid.page_layout.headings", keytip: Some("VH"), group: "page_layout", tier: P0, apps: G, availability: Document };
    Self::PasteSpecial { .. } => { id: "grid.clipboard.paste_special", keytip: Some("S"), group: "clipboard", tier: P0, apps: G, availability: Editable };
    Self::PasteValues { .. } => { id: "grid.clipboard.paste_values", keytip: Some("V"), group: "clipboard", tier: P0, apps: G, availability: Editable };
    Self::PasteFormulas { .. } => { id: "grid.clipboard.paste_formulas", keytip: Some("F"), group: "clipboard", tier: P0, apps: G, availability: Editable };
    Self::PasteFormatting { .. } => { id: "grid.clipboard.paste_formatting", keytip: Some("R"), group: "clipboard", tier: P0, apps: G, availability: Editable };
    Self::PasteTranspose { .. } => { id: "grid.clipboard.paste_transpose", keytip: Some("T"), group: "clipboard", tier: P0, apps: G, availability: Editable };
    Self::PasteLink { .. } => { id: "grid.clipboard.paste_link", keytip: Some("N"), group: "clipboard", tier: P0, apps: G, availability: Editable };
    Self::FillSeries { .. } => { id: "grid.editing.fill_series", keytip: Some("S"), group: "editing", tier: P0, apps: G, availability: Editable };
    Self::AutoFillOption { .. } => { id: "grid.editing.auto_fill_option", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::SetCustomLists { .. } => { id: "grid.editing.custom_lists", keytip: None, group: "editing", tier: P0, apps: G, availability: Document };
    Self::ImportCustomList => { id: "grid.editing.import_custom_list", keytip: None, group: "editing", tier: P0, apps: G, availability: Document };
    Self::BorderPen { .. } => { id: "grid.font.border_pen", keytip: None, group: "font", tier: P0, apps: G, availability: Editable };
    Self::BorderPenOff => { id: "grid.font.border_pen_off", keytip: None, group: "font", tier: P0, apps: G, availability: Document };
    Self::DrawBorder { .. } => { id: "grid.font.draw_border", keytip: None, group: "font", tier: P0, apps: G, availability: Editable };
    Self::DeleteNumberFormat { .. } => { id: "grid.number.delete_format", keytip: None, group: "number", tier: P0, apps: G, availability: Editable };
    Self::SelectDrawing { .. } => { id: "grid.drawing.select", keytip: None, group: "drawing", tier: P0, apps: G, availability: Document };
    Self::PlaceDrawing { .. } => { id: "grid.drawing.place", keytip: None, group: "drawing", tier: P0, apps: G, availability: Editable };
    Self::DeleteDrawing => { id: "grid.drawing.delete", keytip: None, group: "drawing", tier: P0, apps: G, availability: Editable };
    Self::InsertPicture { .. } => { id: "grid.insert.picture", keytip: None, group: "insert", tier: P0, apps: G, availability: Editable };
    Self::InsertShape { .. } => { id: "grid.insert.shape", keytip: None, group: "insert", tier: P0, apps: G, availability: Editable };
    Self::FormatObject { .. } => { id: "grid.drawing.format", keytip: None, group: "drawing", tier: P0, apps: G, availability: Editable };
    Self::ResetPicture { .. } => { id: "grid.picture.reset", keytip: None, group: "picture", tier: P0, apps: G, availability: Editable };
    Self::ArrangeDrawing { .. } => { id: "grid.drawing.arrange", keytip: None, group: "drawing", tier: P0, apps: G, availability: Editable };
    Self::Theme { .. } => { id: "grid.view.theme", keytip: None, group: "view", tier: P0, apps: G, availability: Document };
    Self::SortAscending => { id: "grid.data.sort_ascending", keytip: Some("SA"), group: "data", tier: P0, apps: G, availability: Editable };
    Self::SortDescending => { id: "grid.data.sort_descending", keytip: Some("SD"), group: "data", tier: P0, apps: G, availability: Editable };
    Self::ApplySort { .. } => { id: "grid.data.apply_sort", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::ToggleFilter => { id: "grid.data.toggle_filter", keytip: Some("T"), group: "data", tier: P0, apps: G, availability: Editable };
    Self::FilterColumn { .. } => { id: "grid.data.filter_column", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::SortFilterColumn { .. } => { id: "grid.data.sort_filter_column", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::ClearFilter => { id: "grid.data.clear_filter", keytip: Some("C"), group: "data", tier: P0, apps: G, availability: Editable };
    Self::ReapplyFilter => { id: "grid.data.reapply_filter", keytip: Some("Y"), group: "data", tier: P0, apps: G, availability: Editable };
    Self::AddConditionalFormat { .. } => { id: "grid.styles.add_rule", keytip: None, group: "styles", tier: P0, apps: G, availability: Editable };
    Self::SetConditionalFormats { .. } => { id: "grid.styles.set_rules", keytip: None, group: "styles", tier: P0, apps: G, availability: Editable };
    Self::ClearConditionalFormats { .. } => { id: "grid.styles.clear_rules", keytip: Some("C"), group: "styles", tier: P0, apps: G, availability: Editable };
    Self::SetValidation { .. } => { id: "grid.data.set_validation", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::CircleInvalid { .. } => { id: "grid.data.circle_invalid", keytip: Some("I"), group: "data", tier: P0, apps: G, availability: Document };
    Self::CreateTable { .. } => { id: "grid.table.create", keytip: None, group: "table", tier: P0, apps: G, availability: Editable };
    Self::TableStyle { .. } => { id: "grid.table.style", keytip: None, group: "table", tier: P0, apps: G, availability: Editable };
    Self::TableOptions { .. } => { id: "grid.table.options", keytip: None, group: "table", tier: P0, apps: G, availability: Editable };
    Self::TableTotals { .. } => { id: "grid.table.totals", keytip: None, group: "table", tier: P0, apps: G, availability: Editable };
    Self::RenameTable { .. } => { id: "grid.table.rename", keytip: None, group: "table", tier: P0, apps: G, availability: Editable };
    Self::ResizeTable { .. } => { id: "grid.table.resize", keytip: None, group: "table", tier: P0, apps: G, availability: Editable };
    Self::ConvertToRange => { id: "grid.table.convert_to_range", keytip: Some("V"), group: "table", tier: P0, apps: G, availability: Editable };
    Self::ApplyCellStyle { .. } => { id: "grid.styles.apply_cell_style", keytip: None, group: "styles", tier: P0, apps: G, availability: Editable };
    Self::PaintFormat { .. } => { id: "grid.clipboard.paint_format", keytip: None, group: "clipboard", tier: P0, apps: G, availability: Editable };
    Self::Replace { .. } => { id: "grid.editing.replace", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::SelectSpecial { .. } => { id: "grid.editing.select_special", keytip: None, group: "editing", tier: P0, apps: G, availability: Document };
    Self::SetNote { .. } => { id: "grid.review.set_note", keytip: None, group: "review", tier: P0, apps: G, availability: Editable };
    Self::ShowNote { .. } => { id: "grid.review.show_note", keytip: None, group: "review", tier: P0, apps: G, availability: Document };
    Self::ShowAllNotes { .. } => { id: "grid.review.show_all_notes", keytip: None, group: "review", tier: P0, apps: G, availability: Document };
    Self::AddComment { .. } => { id: "grid.review.add_comment", keytip: None, group: "review", tier: P0, apps: G, availability: Editable };
    Self::EditComment { .. } => { id: "grid.review.edit_comment", keytip: None, group: "review", tier: P0, apps: G, availability: Editable };
    Self::DeleteComment { .. } => { id: "grid.review.delete", keytip: Some("D"), group: "review", tier: P0, apps: G, availability: Editable };
    Self::ResolveComment { .. } => { id: "grid.review.resolve_comment", keytip: None, group: "review", tier: P0, apps: G, availability: Editable };
    Self::NextComment { .. } => { id: "grid.review.next", keytip: None, group: "review", tier: P0, apps: G, availability: Document };
    Self::ConvertNotes { .. } => { id: "grid.review.convert_notes", keytip: None, group: "review", tier: P0, apps: G, availability: Editable };
    Self::SetLink { .. } => { id: "grid.insert.set_link", keytip: None, group: "insert", tier: P0, apps: G, availability: Editable };
    Self::PageSetup { .. } => { id: "grid.page_layout.page_setup", keytip: None, group: "page_layout", tier: P0, apps: G, availability: Editable };
    Self::PrintArea { .. } => { id: "grid.page_layout.set_print_area", keytip: None, group: "page_layout", tier: P0, apps: G, availability: Editable };
    Self::PageBreak { .. } => { id: "grid.page_layout.page_break", keytip: None, group: "page_layout", tier: P0, apps: G, availability: Editable };
    Self::MovePageBreak { .. } => { id: "grid.page_layout.move_page_break", keytip: None, group: "page_layout", tier: P0, apps: G, availability: Editable };
    Self::PrintGridlines => { id: "grid.page_layout.print_gridlines", keytip: Some("PG"), group: "page_layout", tier: P0, apps: G, availability: Editable };
    Self::PrintHeadings => { id: "grid.page_layout.print_headings", keytip: Some("PH"), group: "page_layout", tier: P0, apps: G, availability: Editable };
    Self::SheetRightToLeft => { id: "grid.page_layout.sheet_right_to_left", keytip: None, group: "page_layout", tier: P0, apps: G, availability: Editable };
    Self::DefineName { .. } => { id: "grid.names.define", keytip: None, group: "names", tier: P0, apps: G, availability: Editable };
    Self::DeleteName { .. } => { id: "grid.names.delete", keytip: None, group: "names", tier: P0, apps: G, availability: Editable };
    Self::CreateNames { .. } => { id: "grid.names.create", keytip: None, group: "names", tier: P0, apps: G, availability: Editable };
    Self::TracePrecedents => { id: "grid.formulas.trace_precedents", keytip: Some("P"), group: "formulas", tier: P0, apps: G, availability: Document };
    Self::TraceDependents => { id: "grid.formulas.trace_dependents", keytip: Some("D"), group: "formulas", tier: P0, apps: G, availability: Document };
    Self::RemoveArrows { .. } => { id: "grid.formulas.remove_arrows", keytip: Some("A"), group: "formulas", tier: P0, apps: G, availability: Document };
    Self::CalculationMode { .. } => { id: "grid.formulas.calculation_mode", keytip: None, group: "formulas", tier: P0, apps: G, availability: Editable };
    Self::AcceptInvalid => { id: "grid.data.accept_invalid", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::IterativeCalculation { .. } => { id: "grid.formulas.iterative", keytip: None, group: "formulas", tier: P0, apps: G, availability: Editable };
    Self::DateSystem { .. } => { id: "grid.formulas.date_1904", keytip: None, group: "formulas", tier: P0, apps: G, availability: Editable };
    Self::FormulaLab { .. } => { id: "grid.formulas.trace_formula", keytip: None, group: "formulas", tier: P0, apps: G, availability: Document };
    Self::Watch { .. } => { id: "grid.formulas.watch", keytip: None, group: "formulas", tier: P0, apps: G, availability: Document };
    Self::Import { .. } => { id: "grid.data.import", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::RefreshAll => { id: "grid.data.refresh_all", keytip: Some("RA"), group: "data", tier: P0, apps: G, availability: Editable };
    Self::TextToColumns { .. } => { id: "grid.data.split_column", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::RemoveDuplicates { .. } => { id: "grid.data.dedupe", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::Group { .. } => { id: "grid.data.group", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::Ungroup { .. } => { id: "grid.data.ungroup", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::OutlineDetail { .. } => { id: "grid.data.outline_detail", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::OutlineSymbols => { id: "grid.data.show_outline_symbols", keytip: Some("J"), group: "data", tier: P0, apps: G, availability: Document };
    Self::OutlineLevel { .. } => { id: "grid.data.show_outline_level", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::OutlineToggle { .. } => { id: "grid.data.toggle_outline_group", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::SpellChange { .. } => { id: "grid.review.spell_change", keytip: None, group: "review", tier: P0, apps: G, availability: Editable };
    Self::SpellIgnore { .. } => { id: "grid.review.spell_ignore", keytip: None, group: "review", tier: P0, apps: G, availability: Document };
    Self::LoadDictionary { .. } => { id: "grid.review.load_dictionary", keytip: None, group: "review", tier: P0, apps: G, availability: Document };
    Self::CreatePivot { .. } => { id: "grid.pivot.create", keytip: None, group: "pivot", tier: P0, apps: G, availability: Editable };
    Self::Pivot { .. } => { id: "grid.pivot.edit", keytip: None, group: "pivot", tier: P0, apps: G, availability: Editable };
    Self::RefreshPivots { .. } => { id: "grid.pivot.refresh", keytip: Some("R"), group: "pivot", tier: P0, apps: G, availability: Editable };
    Self::PivotDetails { .. } => { id: "grid.pivot.show_details", keytip: None, group: "pivot", tier: P0, apps: G, availability: Editable };
    Self::InsertSparklines { .. } => { id: "grid.sparkline.insert", keytip: None, group: "sparkline", tier: P0, apps: G, availability: Editable };
    Self::Sparklines { .. } => { id: "grid.sparkline.edit", keytip: None, group: "sparkline", tier: P0, apps: G, availability: Editable };
    Self::GoalSeek { .. } => { id: "grid.data.run_goal_seek", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::Subtotal { .. } => { id: "grid.data.apply_subtotal", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::FlashFill => { id: "grid.editing.flash_fill", keytip: Some("FF"), group: "editing", tier: P0, apps: G, availability: Editable };
    Self::ProtectSheet { .. } => { id: "grid.review.set_sheet_protection", keytip: None, group: "review", tier: P0, apps: G, availability: Editable };
    Self::ProtectWorkbook { .. } => { id: "grid.review.set_workbook_protection", keytip: None, group: "review", tier: P0, apps: G, availability: Editable };
    Self::LockCells { .. } => { id: "grid.cells.lock_cell", keytip: Some("L"), group: "cells", tier: P0, apps: G, availability: Editable };
    Self::SetTheme { .. } => { id: "grid.page_layout.apply_theme", keytip: None, group: "page_layout", tier: P0, apps: G, availability: Editable };
    Self::SelectionMode { .. } => { id: "grid.nav.selection_mode", keytip: None, group: "nav", tier: P0, apps: G, availability: Document };
    Self::DragCells { .. } => { id: "grid.editing.drag_cells", keytip: None, group: "editing", tier: P0, apps: G, availability: Editable };
    Self::FilterBySelection { .. } => { id: "grid.data.filter_by_selection", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::SortBySelectionColor { .. } => { id: "grid.data.sort_by_selection_color", keytip: None, group: "data", tier: P0, apps: G, availability: Editable };
    Self::IgnoreError => { id: "grid.formulas.ignore_error", keytip: None, group: "formulas", tier: P0, apps: G, availability: Editable };
    Self::ConvertToNumber => { id: "grid.formulas.convert_to_number", keytip: None, group: "formulas", tier: P0, apps: G, availability: Editable };
}

// ── queries ──────────────────────────────────────────────────────────────────────────────────

/// A question about a Grid workbook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum GridQuery {
    /// Everything the chrome shows: sheets, name box, formula bar, mode, editor, the active
    /// cell's format, the pane layout.
    State,
    /// The custom fill lists (the user's, without Excel's built-in ones).
    CustomLists,
    /// What is under a point of the surface.
    HitTest {
        /// x in surface CSS pixels.
        x: f32,
        /// y in surface CSS pixels.
        y: f32,
        /// A finger (a coarse pointer): the small outline buttons take a larger target.
        #[serde(default)]
        coarse: bool,
    },
    /// The Format Cells dialog's values for the selection.
    FormatCells,
    /// A number format applied to the active cell's value (the dialog's Sample).
    FormatPreview {
        /// The code.
        code: String,
    },
    /// The argument tooltip for formula text with the caret at `caret` (UTF-16 units).
    FormulaHint {
        /// Formula text.
        text: String,
        /// Caret.
        caret: u32,
    },
    /// The zoom that fits the selection into a `width × height` pane (View ▸ Zoom to
    /// Selection).
    ZoomToSelection {
        /// Pane width, CSS pixels.
        width: f32,
        /// Pane height, CSS pixels.
        height: f32,
    },
    /// The functions whose names start with a prefix, with their summaries (Insert Function).
    Functions {
        /// Prefix (empty: every function).
        prefix: String,
        /// Category name (`Math & Trig`…), `None` for all.
        category: Option<String>,
    },
    /// Recommended Charts for the selection (its shape: text headers, numbers, dates), best
    /// first.
    RecommendedCharts,
    /// an AutoFilter dropdown (header row and column of its button).
    FilterMenu {
        /// Header row.
        row: u32,
        /// Column.
        col: u32,
    },
    /// the conditional-formatting rules (Manage Rules) of the selection or the sheet.
    ConditionalFormats {
        /// The whole sheet.
        sheet: bool,
    },
    /// the active cell's validation (the Data Validation dialog).
    Validation,
    /// the table styles of the gallery.
    TableStyles,
    /// the cell styles of the gallery.
    CellStyles,
    /// Find All.
    FindAll {
        /// What to find.
        spec: features::GridFindSpec,
    },
    /// every note and comment of the workbook (the Comments pane).
    Comments,
    /// the active sheet's page setup.
    PageSetup,
    /// the defined names.
    Names,
    /// the active cell's evaluation (Formula Lab, Evaluate Formula), kept up to date
    /// while [`GridCommand::FormulaLab`] is open, computed afresh otherwise.
    FormulaTrace,
    /// the error cells of the sheet (Error Checking), from the active cell on.
    Errors,
    /// where the cells of a printed page of the active sheet are (Page Layout view).
    PageMap {
        /// The page (0-based).
        page: u32,
    },
    /// the preview of an import.
    ImportPreview {
        /// The file and options.
        import: Box<features::GridImport>,
    },
    /// the next misspelled word from the active cell (Review ▸ Spelling).
    Spelling,
    /// the range Sort and Remove Duplicates work on, with its first row.
    DataRange,
    /// the selection (or the active sheet) as a CSV, TSV or JSON file.
    Export {
        /// Format.
        format: features::GridExportFormat,
        /// The selection (otherwise the sheet).
        selection: bool,
    },
    /// A function's documentation with its arguments (Function Arguments).
    FunctionDoc {
        /// The function's name.
        name: String,
    },
    /// The value a formula would have in the active cell, as displayed (Function Arguments'
    /// argument values and result). Nothing changes.
    Evaluate {
        /// The formula, with or without `=`.
        formula: String,
    },
    /// a PivotTable field's items and whether each is shown (its filter list).
    PivotItems {
        /// The field.
        field: u32,
    },
    /// the PivotTable Styles gallery.
    PivotStyles,
    /// the Themes gallery.
    Themes,
}

// `Query` is `Eq`; the points and sizes of a query are finite numbers the UI measured (a NaN
// compares unequal to itself, which only makes two such queries differ).
impl Eq for GridQuery {}

/// What the status bar mode cell shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridMode {
    /// Not editing.
    Ready,
    /// Typing an entry (arrows commit and move).
    Enter,
    /// Editing in place (arrows move the caret).
    Edit,
    /// Pointing at cells for a formula.
    Point,
}

/// A sheet tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GridSheetTab {
    /// Name.
    pub name: String,
    /// Tab colour `RRGGBB`.
    pub color: Option<String>,
    /// Hidden.
    pub hidden: bool,
    /// A chart sheet (it shows one chart and has no cells).
    #[serde(default)]
    pub chart: bool,
}

/// A coloured reference in editor text (UTF-16 offsets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GridRefSpan {
    /// Start.
    pub start: u32,
    /// End.
    pub end: u32,
    /// `RRGGBB`.
    pub color: String,
}

/// What an autocomplete suggestion is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum CompletionKind {
    /// A worksheet function.
    Function,
    /// A defined name or table.
    Name,
    /// A value from the column (AutoComplete).
    Value,
    /// A table column, `#All`, `#Data`, `#Headers`, `#Totals`, `#This Row` or `@` inside a
    /// structured reference (`Table1[`).
    Structured,
}

/// One autocomplete suggestion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GridCompletion {
    /// What the list shows.
    pub label: String,
    /// Kind.
    pub kind: CompletionKind,
    /// One-line documentation (functions: the summary from the function docs).
    pub detail: Option<String>,
    /// The signature (functions).
    pub syntax: Option<String>,
}

/// One argument in a formula hint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GridHintArg {
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Optional.
    pub optional: bool,
}

/// The argument tooltip: the function around the caret and the argument being typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GridFormulaHint {
    /// Function name.
    pub function: String,
    /// Signature.
    pub syntax: String,
    /// Summary.
    pub summary: String,
    /// Arguments.
    pub args: Vec<GridHintArg>,
    /// Index of the argument at the caret (repeating arguments map to their last entry).
    pub current: u32,
    /// Excel version that introduced it.
    pub since: String,
}

/// The in-cell editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GridEditor {
    /// The entry.
    pub text: String,
    /// Selection start, UTF-16.
    pub start: u32,
    /// Selection end (caret), UTF-16.
    pub end: u32,
    /// Coloured references.
    pub refs: Vec<GridRefSpan>,
    /// Autocomplete suggestions (functions, names, column values).
    pub completions: Vec<GridCompletion>,
    /// The highlighted suggestion.
    pub completion_index: u32,
    /// Inline AutoComplete text after the caret (shown selected), for text entries.
    pub inline_completion: Option<String>,
    /// The argument tooltip.
    pub hint: Option<GridFormulaHint>,
    /// The editor box on the surface (CSS pixels), for the input proxy and popovers.
    pub rect: Rect,
    /// The caret on the surface (CSS pixels).
    pub caret: Rect,
    /// The pick list (Alt+Down), when open.
    pub pick_list: Option<Vec<String>>,
    /// The formula bar is the one editing.
    pub formula_bar: bool,
}

/// The format of the active cell, as the ribbon shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GridCellFormat {
    /// Font family.
    pub font_family: String,
    /// Size in points.
    pub font_size: f64,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underlined.
    pub underline: bool,
    /// Struck through.
    pub strike: bool,
    /// Font colour `RRGGBB` (`None`: automatic).
    pub font_color: Option<String>,
    /// Fill `RRGGBB`.
    pub fill_color: Option<String>,
    /// Horizontal alignment.
    pub h_align: GridHAlign,
    /// Vertical alignment.
    pub v_align: GridVAlign,
    /// Wrapped.
    pub wrap: bool,
    /// In a merge.
    pub merged: bool,
    /// Indent.
    pub indent: u8,
    /// Orientation.
    pub rotation: u8,
    /// Number format code.
    pub number_format: String,
    /// The number format combo's entry (`General`, `Currency`, … or `Custom`).
    pub number_format_name: String,
}

/// Where the panes are (sheet CSS pixels at 100 %).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct GridLayout {
    /// Row header width.
    pub header_w: f32,
    /// Column header height.
    pub header_h: f32,
    /// Frozen rows.
    pub freeze_rows: u32,
    /// Frozen columns.
    pub freeze_cols: u32,
    /// Height of the frozen rows.
    pub frozen_h: f32,
    /// Width of the frozen columns.
    pub frozen_w: f32,
    /// Scrollable width of the cells.
    pub content_w: f32,
    /// Scrollable height of the cells.
    pub content_h: f32,
    /// The sheet's zoom (`0` when the file says nothing).
    pub sheet_zoom: f32,
    /// A split (not frozen) view.
    pub split: bool,
    /// A right-to-left sheet (Page Layout ▸ Sheet Right-to-Left): the surface shows the panes,
    /// headers and tiles mirrored, column A at the right (`kiln_sheet_layout::mirror`).
    #[serde(default)]
    pub rtl: bool,
}

/// A scroll the view should make (keyboard navigation reaching the edge, Go To).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct GridReveal {
    /// Increases with every request.
    pub seq: u32,
    /// Scroll x (sheet pixels).
    pub x: f32,
    /// Scroll y (sheet pixels).
    pub y: f32,
}

/// The selection's statistics as the status bar shows them (formatted like the active cell).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GridStats {
    /// Average of the numbers.
    pub average: Option<String>,
    /// Non-empty cells.
    pub count: u32,
    /// Cells with numbers.
    pub numerical_count: u32,
    /// Smallest number.
    pub min: Option<String>,
    /// Largest number.
    pub max: Option<String>,
    /// Sum of the numbers.
    pub sum: Option<String>,
}

/// Everything the Grid chrome shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GridState {
    /// Sheet tabs.
    pub sheets: Vec<GridSheetTab>,
    /// The active sheet.
    pub active_sheet: u32,
    /// The name box (`G10`, `A1:C3`, `3R x 2C` while selecting, a defined name).
    pub name_box: String,
    /// The formula bar's content for the active cell (its formula, or its value as typed).
    pub formula: String,
    /// The mode.
    pub mode: GridMode,
    /// The editor, while editing.
    pub editor: Option<GridEditor>,
    /// The active cell's format.
    pub format: GridCellFormat,
    /// Pane layout.
    pub layout: GridLayout,
    /// Gridlines shown.
    pub gridlines: bool,
    /// Headings shown.
    pub headings: bool,
    /// Formulas shown instead of results.
    pub show_formulas: bool,
    /// The last scroll request.
    pub reveal: Option<GridReveal>,
    /// Copy or Cut marked a range (Esc clears it).
    pub copy_marked: bool,
    /// The selection is several cells.
    pub multi: bool,
    /// The selection is one or more whole rows.
    pub whole_rows: bool,
    /// The selection is one or more whole columns.
    pub whole_columns: bool,
    /// Statistics of a multi-cell selection.
    pub stats: Option<GridStats>,
    /// Defined names the name box lists.
    pub names: Vec<String>,
    /// The selected chart: its drawing number, rectangle (sheet pixels) and state for the Chart
    /// Design and Format tabs and the Format pane.
    pub drawing: Option<GridDrawing>,
    /// filters, the table at the active cell, notes, links, page setup, auditing.
    #[serde(default)]
    pub features: GridFeatureState,
    /// the PivotTable and sparklines at the active cell, Goal Seek, Flash Fill,
    /// protection, the theme.
    #[serde(default)]
    pub analysis: analysis::GridAnalysisState,
    /// the border pen and the Auto Fill Options button.
    #[serde(default)]
    pub home: home::GridHomeState,
    /// the selection mode, a drag of cells, the active cell's error indicator.
    #[serde(default)]
    pub surface: surface::GridSurfaceState,
}

/// The selected chart of [`GridState`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GridDrawing {
    /// The drawing number on the sheet.
    pub index: u32,
    /// Its name (`Chart 1`).
    pub name: String,
    /// Left, in sheet pixels at 100 %.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
    /// What it is.
    #[serde(default)]
    pub kind: features::GridDrawingKind,
    /// A chart's state.
    pub chart: Option<crate::commands::ChartState>,
    /// A picture's or shape's look.
    #[serde(default)]
    pub object: Option<features::GridObjectInfo>,
}

/// What [`GridQuery::HitTest`] found.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub enum GridHit {
    /// Nothing (outside the grid).
    None,
    /// A cell.
    Cell {
        /// Row.
        row: u32,
        /// Column.
        col: u32,
    },
    /// The fill handle.
    FillHandle,
    /// A column header.
    ColumnHeader {
        /// Column.
        col: u32,
    },
    /// A row header.
    RowHeader {
        /// Row.
        row: u32,
    },
    /// A column border (resize).
    ColumnBorder {
        /// The column left of it.
        col: u32,
        /// Surface x of the column's left edge (CSS pixels).
        start: f32,
        /// The column's width at 100 % (CSS pixels).
        size: f32,
    },
    /// A row border (resize).
    RowBorder {
        /// The row above it.
        row: u32,
        /// Surface y of the row's top edge (CSS pixels).
        start: f32,
        /// The row's height at 100 % (CSS pixels).
        size: f32,
    },
    /// The corner (Select All).
    Corner,
    /// A chart on the sheet.
    Drawing {
        /// The drawing number on the sheet.
        index: u32,
    },
    /// A sizing handle of the selected chart: 0–7 clockwise from the top-left corner (top-left,
    /// top, top-right, right, bottom-right, bottom, bottom-left, left).
    DrawingHandle {
        /// The drawing number on the sheet.
        index: u32,
        /// The handle.
        handle: u8,
    },
    /// the dropdown button of an AutoFilter or table header.
    FilterButton {
        /// Header row.
        row: u32,
        /// Column.
        col: u32,
    },
    /// the in-cell dropdown button of a validation list beside the active cell.
    ListButton,
    /// a PivotTable's field button (Row Labels, Column Labels, a filter's value).
    PivotButton {
        /// Row.
        row: u32,
        /// Column.
        col: u32,
    },
    /// an outline level button in the corner.
    OutlineLevel {
        /// The columns' buttons.
        columns: bool,
        /// The level.
        level: u8,
    },
    /// a group's − or + button in an outline gutter.
    OutlineToggle {
        /// Columns.
        columns: bool,
        /// The summary row or column.
        index: u32,
    },
    /// a page break of Page Break Preview.
    PageBreak {
        /// A column break.
        columns: bool,
        /// The break before this row (column).
        index: u32,
    },
    /// the selection's border (a drag moves the cells); the cell of the selection
    /// nearest the point.
    SelectionBorder {
        /// Row.
        row: u32,
        /// Column.
        col: u32,
    },
    /// the error indicator's button beside the active cell.
    ErrorButton,
}

/// The Format Cells dialog's values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GridFormatCells {
    /// Number format code.
    pub number_format: String,
    /// Number format category (`General`, `Number`, `Currency`, `Accounting`, `Date`, `Time`,
    /// `Percentage`, `Fraction`, `Scientific`, `Text`, `Special`, `Custom`).
    pub category: String,
    /// The active cell's value in that format.
    pub sample: String,
    /// Font, fill and alignment.
    pub format: GridCellFormat,
    /// Underline style name.
    pub underline: String,
    /// Superscript.
    pub superscript: bool,
    /// Subscript.
    pub subscript: bool,
    /// Pattern name.
    pub pattern: String,
    /// Pattern colour.
    pub pattern_color: Option<String>,
    /// Shrink to fit.
    pub shrink: bool,
    /// Left border.
    pub border_left: Option<GridBorderSide>,
    /// Right border.
    pub border_right: Option<GridBorderSide>,
    /// Top border.
    pub border_top: Option<GridBorderSide>,
    /// Bottom border.
    pub border_bottom: Option<GridBorderSide>,
    /// Diagonal up.
    pub border_diagonal_up: Option<GridBorderSide>,
    /// Diagonal down.
    pub border_diagonal_down: Option<GridBorderSide>,
    /// Locked.
    pub locked: bool,
    /// Formula hidden.
    pub hidden: bool,
    /// The fonts the core can draw (family names).
    pub fonts: Vec<String>,
    /// Codes the workbook uses beyond Excel's built-in ones (Custom lists them after the
    /// built-in codes; Delete removes them).
    #[serde(default)]
    pub custom_codes: Vec<String>,
}

/// One function for Insert Function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GridFunctionInfo {
    /// Name.
    pub name: String,
    /// Category.
    pub category: String,
    /// Signature.
    pub syntax: String,
    /// Summary.
    pub summary: String,
}

/// The answer to a [`GridQuery`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum GridQueryResult {
    /// [`GridQuery::State`].
    State(Box<GridState>),
    /// [`GridQuery::CustomLists`].
    Lists(Vec<Vec<String>>),
    /// [`GridQuery::HitTest`].
    Hit(GridHit),
    /// [`GridQuery::FormatCells`].
    FormatCells(Box<GridFormatCells>),
    /// [`GridQuery::FormatPreview`].
    Preview(String),
    /// [`GridQuery::FormulaHint`].
    Hint(Option<GridFormulaHint>),
    /// [`GridQuery::ZoomToSelection`].
    Zoom(f32),
    /// [`GridQuery::Functions`].
    Functions(Vec<GridFunctionInfo>),
    /// [`GridQuery::RecommendedCharts`].
    Charts(Vec<crate::commands::ChartKind>),
    /// [`GridQuery::FilterMenu`].
    FilterMenu(Box<features::GridFilterMenu>),
    /// [`GridQuery::ConditionalFormats`].
    Rules(Vec<features::GridCfRule>),
    /// [`GridQuery::Validation`].
    Validation(Option<features::GridValidation>),
    /// [`GridQuery::TableStyles`].
    TableStyles(Vec<features::GridTableStyle>),
    /// [`GridQuery::CellStyles`].
    CellStyles(Vec<features::GridCellStyleInfo>),
    /// [`GridQuery::FindAll`].
    Found(Vec<features::GridFindHit>),
    /// [`GridQuery::Comments`].
    Comments(Vec<features::GridCommentInfo>),
    /// [`GridQuery::PageSetup`].
    PageSetup(Box<features::GridPageInfo>),
    /// [`GridQuery::Names`].
    Names(Vec<features::GridNameInfo>),
    /// [`GridQuery::FormulaTrace`].
    Trace(Box<features::GridFormulaTrace>),
    /// [`GridQuery::Errors`].
    Errors(Vec<features::GridErrorInfo>),
    /// [`GridQuery::ImportPreview`].
    ImportPreview(Box<features::GridImportPreview>),
    /// [`GridQuery::Spelling`].
    Spelling(Option<features::GridSpellingHit>),
    /// [`GridQuery::DataRange`].
    DataRange(Box<features::GridDataRange>),
    /// [`GridQuery::Export`].
    Export(Box<features::GridExportFile>),
    /// [`GridQuery::FunctionDoc`].
    FunctionDoc(Box<features::GridFunctionDoc>),
    /// [`GridQuery::Evaluate`]: the value as displayed, or the reason it cannot be computed.
    Evaluated(String),
    /// [`GridQuery::PivotItems`].
    Items(Vec<features::GridFilterItem>),
    /// [`GridQuery::Themes`].
    Themes(Vec<analysis::GridThemeInfo>),
    /// [`GridQuery::PageMap`] (`None`: the sheet prints no such page).
    PageMap(Option<Box<surface::GridPageMap>>),
}

#[cfg(any(test, feature = "schema"))]
pub(crate) mod samples {
    //! Representative values of the Grid protocol types (see `crate::samples`).

    use super::*;
    use crate::commands::Command;
    use crate::samples::Samples;

    fn clip() -> ClipboardData {
        ClipboardData {
            text: "1\t2\n3\t4".into(),
            html: Some("<table><tr><td>1</td></tr></table>".into()),
            kiln: None,
        }
    }

    fn side() -> GridBorderSide {
        GridBorderSide {
            style: GridLineStyle::MediumDashDot,
            color: Some("1D8A60".into()),
        }
    }

    fn format() -> GridCellFormat {
        GridCellFormat {
            font_family: "Calibri".into(),
            font_size: 11.0,
            bold: true,
            italic: false,
            underline: false,
            strike: false,
            font_color: Some("C00000".into()),
            fill_color: None,
            h_align: GridHAlign::Right,
            v_align: GridVAlign::Bottom,
            wrap: false,
            merged: false,
            indent: 1,
            rotation: 45,
            number_format: "#,##0.00 \"£\"".into(),
            number_format_name: "Custom".into(),
        }
    }

    /// One sample of every Grid command.
    #[must_use]
    pub fn commands() -> Vec<Command> {
        use GridCommand as C;
        use GridDirection::*;
        let d = || Some(clip());
        [
            C::Move {
                dir: Down,
                extend: true,
            },
            C::Jump {
                dir: Right,
                extend: false,
            },
            C::Page {
                dir: Up,
                extend: false,
            },
            C::RowStart { extend: true },
            C::FirstCell { extend: false },
            C::LastCell { extend: true },
            C::Advance {
                backward: true,
                across: true,
            },
            C::SelectCell {
                row: 1_048_575,
                col: 16_383,
                extend: false,
                add: true,
            },
            C::SelectRange {
                range: GridRange {
                    r0: 2,
                    c0: 1,
                    r1: 9,
                    c1: 4,
                },
                add: false,
            },
            C::SelectRows {
                from: 3,
                to: 7,
                extend: false,
                add: false,
            },
            C::SelectColumns {
                from: 0,
                to: 2,
                extend: true,
                add: false,
            },
            C::SelectAll,
            C::SelectRow,
            C::SelectColumn,
            C::GoTo {
                reference: "'Q1 «Ü»'!B2:C9".into(),
                define: false,
            },
            C::ActivateSheet { index: 2 },
            C::PreviousSheet,
            C::NextSheet,
            C::InsertSheet,
            C::RenameSheet {
                index: 0,
                name: "Budget 2026".into(),
            },
            C::DeleteSheet { index: 3 },
            C::MoveSheet { from: 0, to: 2 },
            C::DuplicateSheet { index: 1 },
            C::SheetTabColor {
                index: 1,
                color: Some("E05A27".into()),
            },
            C::HideSheet { index: 1 },
            C::UnhideSheet { index: 1 },
            C::EditCell,
            C::BeginEdit { text: "=".into() },
            C::PutBackDraft {
                sheet: 1,
                row: 7,
                col: 5,
                text: "=SUM(".into(),
            },
            C::SetEditText {
                text: "=SUM(A1:A9)+😀".into(),
                start: 5,
                end: 9,
                formula_bar: true,
            },
            C::CommitEdit {
                advance: Some(Down),
            },
            C::FillSelection,
            C::ArrayFormula,
            C::CancelEdit,
            C::NewLineInCell,
            C::ToggleAbsolute,
            C::AcceptCompletion { index: 1 },
            C::MoveCompletion { down: true },
            C::PointRef {
                row: 4,
                col: 6,
                extend: true,
            },
            C::PointMove {
                dir: Left,
                extend: false,
            },
            C::OpenDropdown,
            C::ClearContents,
            C::Clear {
                what: GridClear::Formats,
            },
            C::FillDown,
            C::FillRight,
            C::FillUp,
            C::FillLeft,
            C::DragFill { row: 30, col: 2 },
            C::AutoFill {
                row: 30,
                col: 2,
                mode: GridFill::Months,
            },
            C::AutoSum {
                function: Some("AVERAGE".into()),
            },
            C::InsertDateTime {
                time: true,
                utc_offset_minutes: -240,
            },
            C::FindNext {
                text: "kiln".into(),
                match_case: false,
                whole_cell: true,
                backward: false,
                in_formulas: true,
            },
            C::CalculateNow,
            C::CalculateSheet,
            C::ShowFormulas,
            C::InsertCells {
                shift: CellShift::Auto,
            },
            C::DeleteCells {
                shift: CellShift::Horizontal,
            },
            C::InsertRows,
            C::InsertColumns,
            C::DeleteRows,
            C::DeleteColumns,
            C::RowHeight { points: 22.5 },
            C::ColumnWidth { chars: 12.71 },
            C::ResizeRows {
                from: 0,
                to: 4,
                px: 31.5,
            },
            C::ResizeColumns {
                from: 2,
                to: 2,
                px: 140.0,
            },
            C::AutoFitRows {
                from: None,
                to: None,
            },
            C::AutoFitColumns {
                from: Some(1),
                to: Some(3),
            },
            C::HideRows,
            C::HideColumns,
            C::UnhideRows,
            C::UnhideColumns,
            C::Borders {
                preset: GridBorders::ThickBox,
                style: Some(GridLineStyle::Double),
                color: Some("000000".into()),
            },
            C::BorderOutline,
            C::BorderNone,
            C::FillColor {
                color: Some("FFE066".into()),
            },
            C::AlignTop,
            C::AlignMiddle,
            C::AlignBottom,
            C::AlignHorizontal {
                align: GridHAlign::CenterAcross,
            },
            C::Orientation { angle: -45 },
            C::WrapText,
            C::MergeCenter,
            C::MergeAcross,
            C::MergeCells,
            C::UnmergeCells,
            C::NumberFormat {
                format: "Short Date".into(),
            },
            C::NumberStyle,
            C::CurrencyStyle,
            C::PercentStyle,
            C::AccountingStyle {
                currency: Some("GBP".into()),
            },
            C::CommaStyle,
            C::IncreaseDecimal,
            C::DecreaseDecimal,
            C::ApplyFormat {
                format: Box::new(GridFormat {
                    number_format: Some("0.0%".into()),
                    bold: Some(true),
                    font_size: Some(14.5),
                    border_left: Some(side()),
                    h_align: Some(GridHAlign::Distributed),
                    v_align: Some(GridVAlign::Center),
                    rotation: Some(255),
                    ..GridFormat::default()
                }),
            },
            C::FreezePanes,
            C::FreezeTopRow,
            C::FreezeFirstColumn,
            C::Split,
            C::Gridlines,
            C::Headings,
            C::SheetRightToLeft,
            C::PasteSpecial {
                what: GridPaste::ValuesAndNumberFormats,
                op: GridPasteOp::Multiply,
                skip_blanks: true,
                transpose: true,
                data: d(),
            },
            C::PasteValues { data: d() },
            C::PasteFormulas { data: None },
            C::PasteFormatting { data: None },
            C::PasteTranspose { data: d() },
            C::SelectDrawing { index: Some(0) },
            C::PlaceDrawing {
                index: 0,
                x: 200.0,
                y: 40.0,
                width: 480.0,
                height: 288.0,
            },
            C::DeleteDrawing,
            C::Theme { dark: true },
        ]
        .into_iter()
        .map(Command::Grid)
        .chain(super::features::samples::commands())
        .chain(super::analysis::samples::commands())
        .chain(super::home::samples::commands())
        .chain(super::surface::samples::commands())
        .collect()
    }

    impl Samples for GridCommand {
        fn samples() -> Vec<Self> {
            commands()
                .into_iter()
                .filter_map(|c| match c {
                    Command::Grid(g) => Some(g),
                    _ => None,
                })
                .collect()
        }
    }

    macro_rules! samples_of {
        ($($ty:ty => [$($v:expr),* $(,)?];)*) => {
            $(impl Samples for $ty {
                fn samples() -> Vec<Self> {
                    vec![$($v),*]
                }
            })*
        };
    }

    fn hint() -> GridFormulaHint {
        GridFormulaHint {
            function: "XLOOKUP".into(),
            syntax: "XLOOKUP(lookup_value, lookup_array, return_array, …)".into(),
            summary: "Looks up a value.".into(),
            args: vec![GridHintArg {
                name: "lookup_value".into(),
                description: "What to find.".into(),
                optional: false,
            }],
            current: 0,
            since: "365".into(),
        }
    }

    fn state() -> GridState {
        GridState {
            sheets: vec![
                GridSheetTab {
                    name: "Budget".into(),
                    color: Some("1D8A60".into()),
                    hidden: false,
                    chart: false,
                },
                GridSheetTab {
                    name: "Firings «Ü»".into(),
                    color: None,
                    hidden: true,
                    chart: true,
                },
            ],
            active_sheet: 0,
            name_box: "G10".into(),
            formula: "=SUM(G5:G9)".into(),
            mode: GridMode::Edit,
            editor: Some(GridEditor {
                text: "=SUM(G5:G9)".into(),
                start: 11,
                end: 11,
                refs: vec![GridRefSpan {
                    start: 5,
                    end: 10,
                    color: "1D8A60".into(),
                }],
                completions: vec![GridCompletion {
                    label: "SUMIFS".into(),
                    kind: CompletionKind::Function,
                    detail: Some("Adds the cells that meet several criteria.".into()),
                    syntax: Some("SUMIFS(sum_range, criteria_range1, criteria1, …)".into()),
                }],
                completion_index: 0,
                inline_completion: Some("na".into()),
                hint: Some(hint()),
                rect: Rect::new(320.5, 250.0, 92.0, 28.0),
                caret: Rect::new(400.0, 254.0, 1.5, 18.0),
                pick_list: Some(vec!["apple".into(), "cherry".into()]),
                formula_bar: false,
            }),
            format: format(),
            layout: GridLayout {
                header_w: 44.0,
                header_h: 24.0,
                freeze_rows: 1,
                freeze_cols: 0,
                frozen_h: 20.0,
                frozen_w: 0.0,
                content_w: 1_048_576.0,
                content_h: 20_971_520.0,
                sheet_zoom: 1.25,
                split: false,
                rtl: false,
            },
            gridlines: true,
            headings: true,
            show_formulas: false,
            reveal: Some(GridReveal {
                seq: 7,
                x: 0.0,
                y: 1200.5,
            }),
            copy_marked: true,
            multi: false,
            whole_rows: false,
            whole_columns: true,
            stats: Some(stats()),
            names: vec!["SalesTax".into(), "Q1_Total".into()],
            drawing: Some(drawing()),
            features: features::GridFeatureState::default(),
            analysis: analysis::samples::state(),
            home: home::samples::state(),
            surface: surface::samples::state(),
        }
    }

    fn drawing() -> GridDrawing {
        GridDrawing {
            index: 0,
            name: "Chart 1".into(),
            x: 1008.5,
            y: 550.0,
            width: 400.0,
            height: 268.0,
            kind: features::GridDrawingKind::Chart,
            chart: <crate::commands::ChartState as Samples>::samples().pop(),
            object: None,
        }
    }

    fn stats() -> GridStats {
        GridStats {
            average: Some("£1,590.00".into()),
            count: 5,
            numerical_count: 5,
            min: Some("£380.00".into()),
            max: None,
            sum: Some("£7,950.00".into()),
        }
    }

    fn format_cells() -> GridFormatCells {
        GridFormatCells {
            number_format: "0.00%".into(),
            category: "Percentage".into(),
            sample: "12.50%".into(),
            format: format(),
            underline: "doubleAccounting".into(),
            superscript: false,
            subscript: true,
            pattern: "lightGrid".into(),
            pattern_color: Some("A5A5A5".into()),
            shrink: true,
            border_left: Some(side()),
            border_right: None,
            border_top: None,
            border_bottom: Some(side()),
            border_diagonal_up: None,
            border_diagonal_down: Some(side()),
            locked: true,
            hidden: false,
            fonts: vec!["Calibri".into(), "Figtree".into()],
            custom_codes: vec!["0.0\" kg\"".into()],
        }
    }

    samples_of! {
        GridDirection => [GridDirection::Up, GridDirection::Right];
        GridRange => [GridRange { r0: 0, c0: 0, r1: 1_048_575, c1: 16_383 }];
        CellShift => [CellShift::Vertical, CellShift::Columns];
        GridClear => [GridClear::All, GridClear::Hyperlinks];
        GridPaste => [GridPaste::Values, GridPaste::AllExceptBorders, GridPaste::ColumnWidths];
        GridPasteOp => [GridPasteOp::None, GridPasteOp::Divide];
        GridFill => [GridFill::Auto, GridFill::Growth];
        GridBorders => [GridBorders::All, GridBorders::BottomDouble];
        GridLineStyle => [GridLineStyle::Thin, GridLineStyle::SlantDashDot];
        GridHAlign => [GridHAlign::General, GridHAlign::Fill];
        GridVAlign => [GridVAlign::Top, GridVAlign::Distributed];
        GridBorderSide => [side(), GridBorderSide { style: GridLineStyle::None, color: None }];
        GridFormat => [GridFormat::default(), GridFormat { font_color: Some(String::new()), merge: Some(false), indent: Some(250), border_inside_v: Some(side()), ..GridFormat::default() }];
        GridQuery => [
            GridQuery::State,
            GridQuery::CustomLists,
            GridQuery::HitTest {
                x: 120.5,
                y: -3.0,
                coarse: true,
            },
            GridQuery::FormatCells,
            GridQuery::FormatPreview { code: "[Red]0.00".into() },
            GridQuery::FormulaHint { text: "=IF(A1>2,".into(), caret: 9 },
            GridQuery::ZoomToSelection { width: 1200.0, height: 700.0 },
            GridQuery::Functions { prefix: "SU".into(), category: Some("Math & Trig".into()) },
            GridQuery::RecommendedCharts,
            GridQuery::FilterMenu { row: 0, col: 2 },
            GridQuery::ConditionalFormats { sheet: true },
            GridQuery::Validation,
            GridQuery::TableStyles,
            GridQuery::CellStyles,
            GridQuery::FindAll { spec: features::GridFindSpec { text: "glaze".into(), ..features::GridFindSpec::default() } },
            GridQuery::Comments,
            GridQuery::PageSetup,
            GridQuery::Names,
            GridQuery::FormulaTrace,
            GridQuery::Errors,
            GridQuery::ImportPreview { import: Box::new(features::GridImport { bytes: b"a;b".to_vec(), name: "x.csv".into(), ..features::GridImport::default() }) },
            GridQuery::Spelling,
            GridQuery::DataRange,
            GridQuery::Export { format: features::GridExportFormat::Tsv, selection: true },
            GridQuery::FunctionDoc { name: "PMT".into() },
            GridQuery::Evaluate { formula: "=SUM(A1:A3)".into() },
            GridQuery::PivotItems { field: 2 },
            GridQuery::PivotStyles,
            GridQuery::Themes,
            GridQuery::PageMap { page: 0 },
        ];
        GridMode => [GridMode::Ready, GridMode::Point];
        GridSheetTab => [state().sheets[1].clone()];
        GridRefSpan => [GridRefSpan { start: 1, end: 3, color: "3159C6".into() }];
        CompletionKind => [CompletionKind::Function, CompletionKind::Value];
        GridCompletion => [GridCompletion { label: "apple pie".into(), kind: CompletionKind::Value, detail: None, syntax: None }];
        GridHintArg => [hint().args[0].clone()];
        GridFormulaHint => [hint()];
        GridEditor => [state().editor.unwrap_or_else(|| unreachable!())];
        GridCellFormat => [format()];
        GridLayout => [state().layout];
        GridReveal => [GridReveal { seq: 0, x: 64.0, y: 0.0 }];
        GridStats => [stats()];
        GridState => [state()];
        GridHit => [GridHit::None, GridHit::Cell { row: 3, col: 9 }, GridHit::FillHandle, GridHit::ColumnBorder { col: 2, start: 172.5, size: 64.0 }, GridHit::Corner, GridHit::Drawing { index: 1 }, GridHit::DrawingHandle { index: 0, handle: 4 }, GridHit::FilterButton { row: 0, col: 3 }, GridHit::ListButton, GridHit::PivotButton { row: 2, col: 0 }, GridHit::OutlineLevel { columns: false, level: 2 }, GridHit::OutlineToggle { columns: true, index: 5 }, GridHit::PageBreak { columns: true, index: 7 }, GridHit::SelectionBorder { row: 4, col: 2 }, GridHit::ErrorButton];
        GridDrawing => [drawing(), GridDrawing { kind: features::GridDrawingKind::Shape, chart: None, object: Some(features::samples::object_info()), ..drawing() }];
        GridFormatCells => [format_cells()];
        GridFunctionInfo => [GridFunctionInfo { name: "SUM".into(), category: "Math & Trig".into(), syntax: "SUM(number1, …)".into(), summary: "Adds.".into() }];
        GridQueryResult => [
            GridQueryResult::State(Box::new(state())),
            GridQueryResult::Lists(vec![vec!["Low".into(), "High".into()]]),
            GridQueryResult::Hit(GridHit::RowHeader { row: 9 }),
            GridQueryResult::FormatCells(Box::new(format_cells())),
            GridQueryResult::Preview("1,234.50".into()),
            GridQueryResult::Hint(Some(hint())),
            GridQueryResult::Hint(None),
            GridQueryResult::Zoom(1.5),
            GridQueryResult::Functions(Vec::new()),
            GridQueryResult::Charts(vec![crate::commands::ChartKind::ColumnClustered, crate::commands::ChartKind::Line]),
            GridQueryResult::Rules(vec![super::features::samples::cf_rule()]),
            GridQueryResult::Validation(Some(super::features::samples::validation())),
            GridQueryResult::Validation(None),
            GridQueryResult::TableStyles(Vec::new()),
            GridQueryResult::CellStyles(Vec::new()),
            GridQueryResult::Found(Vec::new()),
            GridQueryResult::Comments(Vec::new()),
            GridQueryResult::PageSetup(Box::new(super::features::samples::page_info())),
            GridQueryResult::Names(Vec::new()),
            GridQueryResult::Trace(Box::new(super::features::samples::trace())),
            GridQueryResult::Errors(Vec::new()),
            GridQueryResult::Spelling(None),
            GridQueryResult::Export(Box::new(features::GridExportFile { name: "Budget.json".into(), mime: "application/json".into(), bytes: b"[]".to_vec() })),
            GridQueryResult::FunctionDoc(Box::new(features::samples::function_doc())),
            GridQueryResult::Evaluated("£1,590.00".into()),
            GridQueryResult::Items(vec![features::GridFilterItem { text: "East".into(), checked: true, date: None }]),
            GridQueryResult::Themes(Vec::new()),
            GridQueryResult::PageMap(Some(Box::new(surface::samples::page_map()))),
            GridQueryResult::PageMap(None),
            GridQueryResult::DataRange(Box::new(features::GridDataRange { range: GridRange { r0: 3, c0: 0, r1: 9, c1: 7 }, headers: vec!["Item".into()], header: true })),
            GridQueryResult::FilterMenu(Box::new(features::GridFilterMenu { row: 0, col: 0, header: "Item".into(), kind: features::GridValueKind::Number, items: Vec::new(), blanks: None, filter: Some(features::GridColumnFilter::Values { values: vec!["1".into()], blanks: true }), fill_colors: Vec::new(), font_colors: Vec::new(), sorted: None, truncated: true })),
            GridQueryResult::ImportPreview(Box::new(features::GridImportPreview { rows: Vec::new(), total_rows: 0, total_cols: 0, encoding: "windows-1252".into(), delimiter: "\t".into(), warnings: vec!["Row 3 has 20,000 columns".into()] })),
        ];
    }

    /// Registers the Grid types with the schema visitor.
    pub fn visit(v: &mut impl crate::schema::Visitor) {
        crate::schema::visit_types!(v;
            GridCommand, GridDirection, GridRange, CellShift, GridClear, GridPaste, GridPasteOp,
            GridFill, GridBorders, GridLineStyle, GridHAlign, GridVAlign, GridBorderSide,
            GridFormat, GridQuery, GridMode, GridSheetTab, GridRefSpan, CompletionKind,
            GridCompletion, GridHintArg, GridFormulaHint, GridEditor, GridCellFormat, GridLayout,
            GridReveal, GridState, GridHit, GridFormatCells, GridFunctionInfo, GridQueryResult,
            GridStats, GridDrawing,
        );
        super::features::samples::visit(v);
        super::analysis::samples::visit(v);
        super::home::samples::visit(v);
        super::surface::samples::visit(v);
    }
}
