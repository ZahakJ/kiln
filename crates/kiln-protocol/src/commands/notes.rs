// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Notes commands. This file belongs to WP-86 (Kiln Notes), which adds the variants and their
//! table rows; until then the enum has no variants, so no Notes command can be constructed or sent.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Kiln Notes commands (the OneNote ribbon, PLAN.md §1.4). No variants yet: WP-86 (Kiln Notes) adds them
/// together with their rows in [`NOTES_COMMANDS`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum NotesCommand {}

crate::command_table! {
    NotesCommand => NOTES_COMMANDS;
}
