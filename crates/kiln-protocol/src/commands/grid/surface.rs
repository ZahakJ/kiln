// SPDX-License-Identifier: AGPL-3.0-or-later
//! Arguments and answers of Kiln Grid's surface gestures: Extend Selection (F8) and
//! Add or Remove Selection (Shift+F8), drag and drop of cells by the selection's border, the
//! error indicator's button and menu, and the cell map of a page in Page Layout view. The
//! commands are variants of [`super::GridCommand`], the questions of [`super::GridQuery`].

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// How a click or an arrow key changes the selection (the status bar shows the mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, TS)]
pub enum GridSelectMode {
    /// Clicks and arrows select a new cell.
    #[default]
    Normal,
    /// F8, Extend Selection: clicks and arrows extend the selection as with Shift.
    Extend,
    /// Shift+F8, Add or Remove Selection: clicks add ranges as with Ctrl, and the arrows move
    /// a new cell while the other ranges stay selected.
    Add,
}

/// The step of a drag of the selection's border.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridDragPhase {
    /// The pointer moved: the outline previews the destination.
    Preview,
    /// The button was released: the cells move, or are copied.
    Drop,
    /// Esc, or the pointer left without a drop: nothing changes.
    Cancel,
}

/// Which neighbours an insert-drag (Shift) shifts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridDragInsert {
    /// The cells are inserted between rows; the cells below shift down.
    Rows,
    /// The cells are inserted between columns; the cells to the right shift right.
    Columns,
}

/// A drag of cells in progress (the tooltip and the replace question).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GridDragInfo {
    /// The destination as Excel's tooltip shows it (`D4:E9`).
    pub target: String,
    /// Cells outside the source hold data a drop would overwrite (Excel asks first).
    pub occupied: bool,
    /// A copy (Ctrl).
    pub copy: bool,
    /// An insert-drag (Shift).
    pub insert: Option<GridDragInsert>,
}

/// An error-checking rule (File ▸ Options ▸ Formulas ▸ Error checking rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridErrorRule {
    /// Cells containing formulas that result in an error.
    ErrorValue,
    /// Numbers formatted as text or preceded by an apostrophe.
    NumberAsText,
}

/// The error indicator of the active cell (its button and menu).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GridErrorCheck {
    /// The rule that flags it.
    pub rule: GridErrorRule,
    /// The error value (`#DIV/0!`), or the text for a number stored as text.
    pub error: String,
    /// The menu's heading (`Divide by Zero Error`, `Number Stored as Text`).
    pub title: String,
    /// What it means (Help on this Error).
    pub help: String,
}

/// View state of the surface gestures, part of [`super::GridState`].
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
pub struct GridSurfaceState {
    /// Extend or Add mode.
    #[serde(default)]
    pub select_mode: GridSelectMode,
    /// A drag of cells in progress.
    #[serde(default)]
    pub drag: Option<GridDragInfo>,
    /// The active cell's error indicator.
    #[serde(default)]
    pub error: Option<GridErrorCheck>,
}

/// What the context menu's Filter and Sort take from the active cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum GridSelectionKey {
    /// Its value (Filter by Selected Cell's Value).
    Value,
    /// Its fill colour.
    FillColor,
    /// Its font colour.
    FontColor,
}

/// One row or column printed on a page.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct GridPageLine {
    /// Row or column index.
    pub index: u32,
    /// Offset from the band's origin, page px.
    pub at: f32,
    /// Height or width, page px.
    pub size: f32,
}

/// A block of cells printed on a page (the page's own cells, or its print titles).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GridPageBand {
    /// Left of the band, page px.
    pub x: f32,
    /// Top of the band, page px.
    pub y: f32,
    /// Its rows, top to bottom.
    pub rows: Vec<GridPageLine>,
    /// Its columns, left to right.
    pub cols: Vec<GridPageLine>,
}

/// Where the cells of a printed page are (Page Layout view edits on the page).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GridPageMap {
    /// Paper width, page px.
    pub width: f32,
    /// Paper height, page px.
    pub height: f32,
    /// The bands; the last one is the page's own cells.
    pub bands: Vec<GridPageBand>,
}

#[cfg(any(test, feature = "schema"))]
pub(crate) mod samples {
    //! Representative values (see `crate::samples`).

    use super::*;
    use crate::commands::Command;
    use crate::commands::grid::GridCommand as C;
    use crate::samples::Samples;

    pub(crate) fn state() -> GridSurfaceState {
        GridSurfaceState {
            select_mode: GridSelectMode::Add,
            drag: Some(GridDragInfo {
                target: "D4:E9".into(),
                occupied: true,
                copy: false,
                insert: Some(GridDragInsert::Rows),
            }),
            error: Some(error()),
        }
    }

    fn error() -> GridErrorCheck {
        GridErrorCheck {
            rule: GridErrorRule::ErrorValue,
            error: "#DIV/0!".into(),
            title: "Divide by Zero Error".into(),
            help: "The formula or function divides by zero or by an empty cell.".into(),
        }
    }

    pub(crate) fn page_map() -> GridPageMap {
        GridPageMap {
            width: 816.0,
            height: 1056.0,
            bands: vec![GridPageBand {
                x: 67.2,
                y: 72.0,
                rows: vec![GridPageLine {
                    index: 0,
                    at: 0.0,
                    size: 20.0,
                }],
                cols: vec![GridPageLine {
                    index: 2,
                    at: 0.0,
                    size: 64.0,
                }],
            }],
        }
    }

    /// One sample of every surface command.
    pub(crate) fn commands() -> Vec<Command> {
        [
            C::SelectionMode {
                mode: GridSelectMode::Extend,
            },
            C::DragCells {
                phase: GridDragPhase::Preview,
                row: 9,
                col: 4,
                grab_row: 2,
                grab_col: 1,
                copy: true,
                insert: false,
                replace: false,
            },
            C::DragCells {
                phase: GridDragPhase::Drop,
                row: 0,
                col: 0,
                grab_row: 0,
                grab_col: 0,
                copy: false,
                insert: true,
                replace: true,
            },
            C::FilterBySelection {
                by: GridSelectionKey::FillColor,
            },
            C::SortBySelectionColor { font: true },
            C::IgnoreError,
            C::ConvertToNumber,
        ]
        .into_iter()
        .map(Command::Grid)
        .collect()
    }

    crate::samples::samples_for! {
        GridSelectMode => vec![GridSelectMode::Normal, GridSelectMode::Add];
        GridDragPhase => vec![GridDragPhase::Preview, GridDragPhase::Cancel];
        GridDragInsert => vec![GridDragInsert::Rows, GridDragInsert::Columns];
        GridSelectionKey => vec![GridSelectionKey::Value, GridSelectionKey::FontColor];
        GridDragInfo => vec![state().drag.unwrap_or_else(|| unreachable!())];
        GridErrorRule => vec![GridErrorRule::ErrorValue, GridErrorRule::NumberAsText];
        GridErrorCheck => vec![error()];
        GridSurfaceState => vec![state(), GridSurfaceState::default()];
        GridPageLine => vec![page_map().bands[0].rows[0]];
        GridPageBand => vec![page_map().bands[0].clone()];
        GridPageMap => vec![page_map()];
    }

    /// Registers the types with the schema and test-vector visitor.
    pub fn visit(v: &mut impl crate::schema::Visitor) {
        crate::schema::visit_types!(v;
            GridSelectMode, GridDragPhase, GridDragInsert, GridSelectionKey, GridDragInfo, GridErrorRule,
            GridErrorCheck, GridSurfaceState, GridPageLine, GridPageBand, GridPageMap,
        );
    }
}
