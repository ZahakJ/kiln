// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Write commands. WP-52 declares the editing commands of the scratch document; the Write work
//! packages (WP-63 onwards) own this file and append the rest of the ribbon.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::app::AppMask;
use crate::selection::TextPos;

/// How [`WriteCommand::MoveCaret`] moves the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum CaretMotion {
    /// One grapheme cluster back (Left in left-to-right text).
    Backward,
    /// One grapheme cluster forward.
    Forward,
    /// To the start of the previous word (Ctrl+Left).
    WordBackward,
    /// To the start of the next word (Ctrl+Right).
    WordForward,
    /// To the start of the line (Home). Without layout, the start of the paragraph.
    LineStart,
    /// To the end of the line (End). Without layout, the end of the paragraph.
    LineEnd,
    /// One line up. Without layout, the same offset in the previous paragraph.
    LineUp,
    /// One line down. Without layout, the same offset in the next paragraph.
    LineDown,
    /// To the start of the document (Ctrl+Home).
    DocumentStart,
    /// To the end of the document (Ctrl+End).
    DocumentEnd,
}

/// Kiln Write commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum WriteCommand {
    /// Type text at the caret, replacing the selection. A line feed starts a new paragraph.
    InsertText {
        /// The text (any Unicode; `\r\n` and `\r` count as one paragraph break).
        text: String,
    },
    /// Backspace: delete the selection, or the grapheme cluster before the caret, or join the
    /// paragraph with the previous one at its start.
    DeleteBackward,
    /// Move the caret; with `extend` the selection grows from its anchor (Shift).
    MoveCaret {
        /// The movement.
        motion: CaretMotion,
        /// Extend the selection instead of collapsing it.
        extend: bool,
    },
    /// Put the caret at a position (a click), or extend the selection to it (Shift+click, drag).
    SelectTo {
        /// The position.
        pos: TextPos,
        /// Extend the selection instead of collapsing it.
        extend: bool,
    },
}

crate::command_table! {
    WriteCommand => WRITE_COMMANDS;
    Self::InsertText { .. } => {
        id: "write.edit.insert_text", keytip: None, group: "edit",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::DeleteBackward => {
        id: "write.edit.delete_backward", keytip: None, group: "edit",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::MoveCaret { .. } => {
        id: "write.caret.move", keytip: None, group: "caret",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Document,
    };
    Self::SelectTo { .. } => {
        id: "write.caret.select_to", keytip: None, group: "caret",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Document,
    };
}
