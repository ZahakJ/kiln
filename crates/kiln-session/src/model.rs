// SPDX-License-Identifier: AGPL-3.0-or-later
//! The interface between the session and an app's document model.

use std::fmt;

use kiln_common::KilnError;
use kiln_common::Result;
use kiln_protocol::commands::ClipboardData;
use kiln_protocol::{
    A11yTree, Command, CompatIssue, DocKind, FileFormat, OpenHint, ParagraphText, Query,
    QueryResult, Selection, StatusBar,
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

/// Something a model tells every window showing its document, which the session turns into an
/// event with the document's id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelNotice {
    /// A slide show moved on.
    Show(kiln_protocol::commands::deck_show::DeckShowStatus),
}

/// A document model behind the session. Implemented once per app (scratch now; Write, Grid, Deck
/// and Notes) and registered in `apps/mod.rs`.
/// Not `Debug`: a supertrait would keep every model's `Debug` (and its commands') in each vtable,
/// which the web core cannot afford (B12).
pub trait DocModel: Send {
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

    /// The accessibility tree of the whole document, for the Accessible text view;
    /// models without more than what [`DocModel::a11y`] shows answer with that.
    fn a11y_document(&self, title: &str) -> A11yTree {
        self.a11y(title)
    }

    /// The selected text as plain text (Copy, Cut), for models with text selections.
    fn selected_text(&self) -> Option<String> {
        None
    }

    /// Answers an app-specific query (`Query::Write`).
    ///
    /// # Errors
    ///
    /// `Unsupported` for queries of another app; `InvalidInput` for a malformed query (a bad
    /// Find pattern is an answer, not an error).
    fn app_query(&self, _query: &Query) -> Result<QueryResult> {
        Err(KilnError::Unsupported("the query for this document".into()))
    }

    /// Gives the model the host's clock (Unix milliseconds) before a command or frame, for
    /// dates, times and fields (`wasm32` has no clock of its own).
    fn set_clock(&mut self, now_ms: u64) {
        let _ = now_ms;
    }

    /// What the last command copied (Copy, Cut), taken once: the session writes it to the system
    /// clipboard and remembers its rich format.
    fn take_clipboard(&mut self) -> Option<ClipboardData> {
        None
    }

    /// The model's painter-facing side, for models that paint through the Scene.
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
    /// formats with a modification date can stamp it. Defaults to [`DocModel::save`].
    ///
    /// # Errors
    ///
    /// As [`DocModel::save`].
    fn save_at(&self, format: FileFormat, now_ms: u64) -> Result<Vec<u8>> {
        let _ = now_ms;
        self.save(format)
    }

    /// Prints: a preview sheet or the PDF. Models that print implement it with
    /// [`crate::print::run`] over their `PrintablePages`.
    ///
    /// # Errors
    ///
    /// `Unsupported` for models that do not print yet; `InvalidInput` for bad settings.
    fn print(&self, job: &crate::print::PrintJob<'_>) -> Result<crate::print::PrintOutput> {
        let _ = job;
        Err(kiln_common::KilnError::Unsupported(
            "printing this kind of document".into(),
        ))
    }

    /// Work printing needs the model mutably for, done before a preview or PDF (Kiln Grid
    /// evaluates the conditional formats of the printed cells). Nothing by default.
    fn prepare_print(&mut self) {}

    /// Before a query that needs the model mutably (evaluating a formula): computes what the
    /// query then reads.
    fn prepare_query(&mut self, query: &Query) {
        let _ = query;
    }

    /// The Compatibility Checker: what saving in `format` loses or changes, then what was lost
    /// or kept read-only on open. Empty for models that save everything.
    fn compat_report(&self, format: FileFormat) -> Vec<CompatIssue> {
        let _ = format;
        Vec::new()
    }

    /// What the model has to tell every window since the last call (a slide show moved
    /// on), taken once. Nothing by default.
    fn take_notices(&mut self) -> Vec<ModelNotice> {
        Vec::new()
    }

    /// what the model does for collaboration and Version History (Write, Notes, Deck).
    fn collab(&mut self) -> Option<&mut dyn crate::collab::Collaborative> {
        None
    }

    /// the document as a native bundle (`.kwrite`, `.kgrid`, `.kdeck`): the model in
    /// `model.bin` and what goes beside it (the session adds the title, dates and versions).
    ///
    /// # Errors
    ///
    /// `Unsupported` for models without a bundle (the default).
    fn to_bundle(&self) -> Result<kiln_storage::Bundle> {
        Err(KilnError::Unsupported(
            "a native bundle of this kind of document".into(),
        ))
    }

    /// whether [`DocModel::to_bundle`] works (the session then offers the bundle as a
    /// save format and keeps an autosave journal and versions).
    fn bundles(&self) -> bool {
        false
    }

    /// starts or stops recording the operations the model applies, for the autosave
    /// journal.
    fn set_journaling(&mut self, on: bool) {
        let _ = on;
    }

    /// the document properties (File ▸ Info ▸ Properties) with the document's counts;
    /// `None` for documents without properties.
    fn properties(&self) -> Option<kiln_protocol::commands::DocPropertiesInfo> {
        None
    }

    /// sets the document properties; `Ok(true)` when one changed.
    ///
    /// # Errors
    ///
    /// `Unsupported` for documents without properties, `InvalidInput` for a value over 1,024
    /// characters.
    fn set_properties(
        &mut self,
        edit: &kiln_protocol::commands::write_features::PropertiesEdit,
    ) -> Result<bool> {
        let _ = edit;
        Err(KilnError::Unsupported("document properties here".into()))
    }

    /// Inspect Document: every kind of hidden data the app looks for, with its count.
    fn inspect(&self) -> Vec<kiln_protocol::InspectItem> {
        Vec::new()
    }

    /// Inspect Document ▸ Remove All for one kind; `Ok(true)` when something went.
    ///
    /// # Errors
    ///
    /// `Unsupported` for a kind the app does not have.
    fn inspect_remove(&mut self, item: kiln_protocol::InspectKind) -> Result<bool> {
        Err(KilnError::Unsupported(format!("removing {item:?} here")))
    }

    /// the speller's options changed (`kiln_spell::set_skip_rules`): spelling results
    /// kept from before are stale.
    fn spelling_changed(&mut self) {}

    /// the interface language (a tag such as `de` or `ar`), for what the core draws in
    /// it: Deck's placeholder prompts. `true` when the view changes.
    fn interface_language(&mut self, _tag: &str) -> bool {
        false
    }

    /// the operations applied since the last call, each in the model's own encoding
    /// (empty when none); `None` for models that record none. When the document changed but
    /// no operation was recorded, the journal takes a checkpoint instead.
    fn take_journal(&mut self) -> Option<Vec<Vec<u8>>> {
        None
    }

    /// What the user is typing that is not part of the document yet, in the model's own encoding,
    /// when it changed since the last call: `Some(Some(text))` for new text, `Some(None)` when
    /// the entry ended, `None` when nothing changed. Only a model with an open entry has any.
    fn take_draft(&mut self) -> Option<Option<Vec<u8>>> {
        None
    }

    /// The cell and text of an entry a crash interrupted, for the recovery message: it is not
    /// put back on its own.
    fn describe_draft(&self, draft: &[u8]) -> Option<kiln_protocol::RecoveredDraft> {
        let _ = draft;
        None
    }

    /// The journal format of the operations [`DocModel::apply_journal`] gets next.
    fn set_journal_format(&mut self, format: u32) {
        let _ = format;
    }

    /// applies one recorded operation (replaying a journal after a crash).
    ///
    /// # Errors
    ///
    /// Bytes that are not an operation of this model, or one it refuses.
    fn apply_journal(&mut self, op: &[u8], replay: &crate::JournalReplay) -> Result<()> {
        let _ = (op, replay);
        Err(KilnError::Unsupported(
            "replaying operations of this kind of document".into(),
        ))
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
    /// A new, empty document whose text is in `language` (BCP 47), the default editing language.
    /// Apps whose documents carry no default language create the usual document.
    fn create_in(&self, language: &str) -> Box<dyn DocModel> {
        let _ = language;
        self.create()
    }
    /// Parses a file.
    ///
    /// # Errors
    ///
    /// `Format` when the bytes are not a valid file of the format.
    fn open(&self, bytes: &[u8], format: FileFormat, hint: &OpenHint) -> Result<Box<dyn DocModel>>;
    /// A developer document (the scratch surface): opens a file only when the hint asks for its
    /// kind or no other app reads the format.
    fn internal(&self) -> bool {
        false
    }

    /// the document of a native bundle (`.kwrite`, `.kgrid`, `.kdeck`, an autosave
    /// checkpoint, a version).
    ///
    /// # Errors
    ///
    /// `Format` for a model this app cannot read, `Unsupported` for apps without bundles.
    fn open_bundle(
        &self,
        bundle: &kiln_storage::Bundle,
        hint: &OpenHint,
    ) -> Result<Box<dyn DocModel>> {
        let _ = (bundle, hint);
        Err(KilnError::Unsupported(format!(
            "native bundles of {:?} documents",
            self.kind()
        )))
    }
}
