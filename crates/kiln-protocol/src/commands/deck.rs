// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Deck commands. This file belongs to WP-78 (Deck editing), which adds the variants and their
//! table rows; until then the enum has no variants, so no Deck command can be constructed or sent.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Kiln Deck commands (the PowerPoint ribbon, PLAN.md §1.4). No variants yet: WP-78 (Deck editing) adds them
/// together with their rows in [`DECK_COMMANDS`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum DeckCommand {}

crate::command_table! {
    DeckCommand => DECK_COMMANDS;
}
