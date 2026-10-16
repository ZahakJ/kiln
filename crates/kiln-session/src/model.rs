// SPDX-License-Identifier: AGPL-3.0-or-later
//! The interface between the session and an app's document model.

use std::fmt;

use kiln_common::Result;
use kiln_protocol::{
    A11yTree, Command, CompatIssue, DocKind, FileFormat, OpenHint, ParagraphText, Selection,
    StatusBar,
};

use crate::frame::FrameSource;
use crate::undo::StateId;

/// What an app command changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Nothing.
    None,
    /// Only the selection or caret.
    Selection,
    /// The content (an undo step was recorded).
    Content,
}

/// A document model behind the session. Implemented once per app (scratch now; Write, Grid, Deck
/// and Notes by their work packages) and registered in `apps/mod.rs`.
pub trait DocModel: Send + fmt::Debug {
    /// The document kind.
    fn kind(&self) -> DocKind;

    /// Runs an app command. The session has already checked the command's app and availability.
    ///
    /// # Errors
    ///
    /// `InvalidInput` for arguments the model rejects (a position outside the document),
    /// `Unsupported` for commands it does not implement yet.
    fn execute(&mut self, command: &Command) -> Result<Change>;

    /// Reverts the last change. Returns `false` when there is nothing to undo.
    fn undo(&mut self) -> bool;
    /// Re-applies the last undone change. Returns `false` when there is nothing to redo.
    fn redo(&mut self) -> bool;
    /// Whether Undo is possible.
    fn can_undo(&self) -> bool;
    /// Whether Redo is possible.
    fn can_redo(&self) -> bool;
    /// The command id Undo would revert.
    fn undo_label(&self) -> Option<&'static str>;
    /// The command id Redo would re-apply.
    fn redo_label(&self) -> Option<&'static str>;
    /// The id of the current history state (see [`crate::undo::StateId`]).
    fn state(&self) -> StateId;
    /// Ends the current typing group (called when the document is saved).
    fn seal_history(&mut self);

    /// The selection.
    fn selection(&self) -> Selection;
    /// App-specific status bar fields (the session fills in zoom and view).
    fn status(&self) -> StatusBar;
    /// The document's paragraphs as plain text, for models that have paragraphs.
    fn paragraphs(&self) -> Option<Vec<ParagraphText>>;
    /// The accessibility tree, with `title` as the document's accessible name.
    fn a11y(&self, title: &str) -> A11yTree;

    /// The selected text as plain text (Copy, Cut), for models with text selections.
    fn selected_text(&self) -> Option<String> {
        None
    }

    /// The model's painter-facing side, for models that paint through the Scene (WP-54).
    fn frame_source(&mut self) -> Option<&mut dyn FrameSource> {
        None
    }

    /// The formats the model saves to; the first is the default.
    fn save_formats(&self) -> &'static [FileFormat];
    /// Serialises the document.
    ///
    /// # Errors
    ///
    /// `Unsupported` for a format not in [`DocModel::save_formats`].
    fn save(&self, format: FileFormat) -> Result<Vec<u8>>;

    /// Serialises the document for a save at `now_ms` (Unix milliseconds, the host's clock), so
    /// formats with a modification date can stamp it (WP-65). Defaults to [`DocModel::save`].
    ///
    /// # Errors
    ///
    /// As [`DocModel::save`].
    fn save_at(&self, format: FileFormat, now_ms: u64) -> Result<Vec<u8>> {
        let _ = now_ms;
        self.save(format)
    }

    /// The Compatibility Checker: what saving in `format` loses or changes, then what was lost
    /// or kept read-only on open (WP-65). Empty for models that save everything.
    fn compat_report(&self, format: FileFormat) -> Vec<CompatIssue> {
        let _ = format;
        Vec::new()
    }
}

/// Creates and opens documents of one kind.
pub trait AppFactory: Send + fmt::Debug {
    /// The kind of document this app creates.
    fn kind(&self) -> DocKind;
    /// Whether this app opens `format`.
    fn opens(&self, format: FileFormat) -> bool;
    /// A new, empty document.
    fn create(&self) -> Box<dyn DocModel>;
    /// Parses a file.
    ///
    /// # Errors
    ///
    /// `Format` when the bytes are not a valid file of the format.
    fn open(&self, bytes: &[u8], format: FileFormat, hint: &OpenHint) -> Result<Box<dyn DocModel>>;
}
