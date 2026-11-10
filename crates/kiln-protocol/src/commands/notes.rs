// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Notes commands. The enum has no variants yet, so no Notes command can be constructed or
//! sent.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Kiln Notes commands (the OneNote ribbon). No variants yet: they arrive together
/// with their rows in [`NOTES_COMMANDS`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum NotesCommand {}

crate::command_table! {
    NotesCommand => NOTES_COMMANDS;
}
