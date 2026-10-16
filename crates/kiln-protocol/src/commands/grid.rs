// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Grid commands. This file belongs to WP-71 (Grid editing), which adds the variants and their
//! table rows; until then the enum has no variants, so no Grid command can be constructed or sent.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Kiln Grid commands (the Excel ribbon, PLAN.md §1.4). No variants yet: WP-71 (Grid editing) adds them
/// together with their rows in [`GRID_COMMANDS`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum GridCommand {}

crate::command_table! {
    GridCommand => GRID_COMMANDS;
}
