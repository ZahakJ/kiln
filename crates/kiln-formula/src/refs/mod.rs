// SPDX-License-Identifier: AGPL-3.0-or-later
//! References: A1 addresses and sheet qualifiers, and the rewriting of formulas when rows and
//! columns are inserted or deleted, cells are copied or moved, sheets are renamed or removed,
//! between A1 and R1C1, and the F4 toggle.

mod address;
mod rewrite;

pub use address::{
    Bounds, MAX_COLS, MAX_ROWS, address_bounds, column_to_letters, format_address, format_cell,
    format_prefix, letters_to_column, parse_cell_address, quote_sheet, sheet_needs_quotes,
};
pub use rewrite::{
    MoveSpec, StructuralEdit, StructuralKind, ToggleResult, address_to_r1c1, copy_formula,
    cycle_absolute, from_r1c1, move_formula, offset_relative, remove_sheet_in_formula,
    rename_sheet_in_formula, rewrite_formula, shift_address, shift_formula, to_r1c1,
    toggle_absolute,
};
