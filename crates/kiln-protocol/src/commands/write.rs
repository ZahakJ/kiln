// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Write commands. WP-52 declares the editing commands of the scratch document; the Write work
//! packages (WP-63 onwards) own this file and append the rest of the ribbon.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::app::AppMask;
use crate::geom::Point;
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
    /// To the start of the paragraph, or of the previous one when already there (Ctrl+Up).
    ParagraphBackward,
    /// To the start of the next paragraph (Ctrl+Down).
    ParagraphForward,
    /// One screen up (Page Up). Without layout, ten paragraphs.
    PageUp,
    /// One screen down (Page Down). Without layout, ten paragraphs.
    PageDown,
}

/// What a click selects: Word's single, double and triple click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum SelectUnit {
    /// A caret position (single click); dragging extends by characters.
    Char,
    /// The word under the point (double click); dragging extends by words.
    Word,
    /// The paragraph under the point (triple click); dragging extends by paragraphs.
    Paragraph,
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
    /// Delete: the selection, or the grapheme cluster after the caret, or join the next paragraph
    /// at the end of one.
    DeleteForward,
    /// Ctrl+Backspace: delete the selection, or back to the start of the word.
    DeleteWordBackward,
    /// Ctrl+Delete: delete the selection, or up to the start of the next word.
    DeleteWordForward,
    /// Select everything (Ctrl+A).
    SelectAll,
    /// A click or drag at a point of the surface (document CSS pixels at zoom 1): the core hit
    /// tests its layout and moves the caret or extends the selection by `unit`.
    SelectAt {
        /// The point.
        point: Point,
        /// Extend the selection (Shift+click, drag) instead of starting a new one.
        extend: bool,
        /// Characters, words or paragraphs.
        unit: SelectUnit,
    },
    /// The IME's current composition (`compositionstart`/`compositionupdate`): replaces the
    /// selection (first time) or the previous composition with `text`, shown underlined. Not an
    /// undo step until committed.
    SetComposition {
        /// The composition string.
        text: String,
        /// Caret offset inside it, in `char`s.
        caret: u32,
    },
    /// Ends the composition (`compositionend`) by inserting `text` in its place as one typing
    /// step. Without a composition it inserts `text` at the caret.
    CommitComposition {
        /// The committed text (may differ from the last composition, or be empty).
        text: String,
    },
    /// Removes the composition string and restores the text it replaced (Escape in the IME).
    CancelComposition,
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
    // WP-54: editing surface (layout-aware selection, deletion, IME composition).
    Self::DeleteForward => {
        id: "write.edit.delete_forward", keytip: None, group: "edit",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::DeleteWordBackward => {
        id: "write.edit.delete_word_backward", keytip: None, group: "edit",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::DeleteWordForward => {
        id: "write.edit.delete_word_forward", keytip: None, group: "edit",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::SelectAll => {
        id: "write.caret.select_all", keytip: None, group: "caret",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Document,
    };
    Self::SelectAt { .. } => {
        id: "write.caret.select_at", keytip: None, group: "caret",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Document,
    };
    Self::SetComposition { .. } => {
        id: "write.ime.set_composition", keytip: None, group: "ime",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::CommitComposition { .. } => {
        id: "write.ime.commit_composition", keytip: None, group: "ime",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
    Self::CancelComposition => {
        id: "write.ime.cancel_composition", keytip: None, group: "ime",
        tier: P0, apps: AppMask::WRITE.union(AppMask::NOTES), availability: Editable,
    };
}
