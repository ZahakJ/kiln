// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Grid commands and queries (WP-71): navigation and selection, in-cell editing with
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
    /// Arrow keys: one cell; with `extend` the selection grows (Shift).
    Move {
        /// Direction.
        dir: GridDirection,
        /// Extend the selection.
        extend: bool,
    },
    /// Ctrl+Arrows: to the edge of the data region (Ctrl+Shift extends).
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
    /// Point mode: arrow keys move the inserted reference.
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
    /// The app's theme, for what the core colours by it (charts on screen). Not an edit.
    Theme {
        /// The dark theme.
        dark: bool,
    },
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
    Self::SelectDrawing { .. } => { id: "grid.drawing.select", keytip: None, group: "drawing", tier: P0, apps: G, availability: Document };
    Self::PlaceDrawing { .. } => { id: "grid.drawing.place", keytip: None, group: "drawing", tier: P0, apps: G, availability: Editable };
    Self::DeleteDrawing => { id: "grid.drawing.delete", keytip: None, group: "drawing", tier: P0, apps: G, availability: Editable };
    Self::Theme { .. } => { id: "grid.view.theme", keytip: None, group: "view", tier: P0, apps: G, availability: Document };
}

// ── queries ──────────────────────────────────────────────────────────────────────────────────

/// A question about a Grid workbook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum GridQuery {
    /// Everything the chrome shows: sheets, name box, formula bar, mode, editor, the active
    /// cell's format, the pane layout.
    State,
    /// What is under a point of the surface.
    HitTest {
        /// x in surface CSS pixels.
        x: f32,
        /// y in surface CSS pixels.
        y: f32,
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
    /// The chart's state.
    pub chart: crate::commands::ChartState,
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
                },
                GridSheetTab {
                    name: "Firings «Ü»".into(),
                    color: None,
                    hidden: true,
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
            chart: <crate::commands::ChartState as Samples>::samples()
                .pop()
                .unwrap_or_else(|| unreachable!("ChartState has samples")),
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
        }
    }

    samples_of! {
        GridDirection => [GridDirection::Up, GridDirection::Right];
        GridRange => [GridRange { r0: 0, c0: 0, r1: 1_048_575, c1: 16_383 }];
        CellShift => [CellShift::Vertical, CellShift::Columns];
        GridClear => [GridClear::All, GridClear::Hyperlinks];
        GridPaste => [GridPaste::Values, GridPaste::AllExceptBorders];
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
            GridQuery::HitTest { x: 120.5, y: -3.0 },
            GridQuery::FormatCells,
            GridQuery::FormatPreview { code: "[Red]0.00".into() },
            GridQuery::FormulaHint { text: "=IF(A1>2,".into(), caret: 9 },
            GridQuery::ZoomToSelection { width: 1200.0, height: 700.0 },
            GridQuery::Functions { prefix: "SU".into(), category: Some("Math & Trig".into()) },
            GridQuery::RecommendedCharts,
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
        GridHit => [GridHit::None, GridHit::Cell { row: 3, col: 9 }, GridHit::FillHandle, GridHit::ColumnBorder { col: 2, start: 172.5, size: 64.0 }, GridHit::Corner, GridHit::Drawing { index: 1 }, GridHit::DrawingHandle { index: 0, handle: 4 }];
        GridDrawing => [drawing()];
        GridFormatCells => [format_cells()];
        GridFunctionInfo => [GridFunctionInfo { name: "SUM".into(), category: "Math & Trig".into(), syntax: "SUM(number1, …)".into(), summary: "Adds.".into() }];
        GridQueryResult => [
            GridQueryResult::State(Box::new(state())),
            GridQueryResult::Hit(GridHit::RowHeader { row: 9 }),
            GridQueryResult::FormatCells(Box::new(format_cells())),
            GridQueryResult::Preview("1,234.50".into()),
            GridQueryResult::Hint(Some(hint())),
            GridQueryResult::Hint(None),
            GridQueryResult::Zoom(1.5),
            GridQueryResult::Functions(Vec::new()),
            GridQueryResult::Charts(vec![crate::commands::ChartKind::ColumnClustered, crate::commands::ChartKind::Line]),
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
    }
}
