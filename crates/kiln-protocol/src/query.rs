// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only questions to the core (pane data, document text for the scratch view).

use kiln_common::NodeId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::a11y::A11yTree;
use crate::app::FileFormat;
use crate::collab::{CollabState, VersionEntry, VersionParagraph, VersionSlides};
use crate::commands::{
    DeckQuery, DeckQueryResult, DocPropertiesInfo, GridQuery, GridQueryResult, WriteQuery,
    WriteQueryResult,
};
use crate::host::RecentFile;
use crate::print::{PdfFile, PrintPreview, PrintSettings};
use crate::selection::Selection;
use crate::status::StatusBar;
use crate::summary::DocSummary;

/// A question about the session (`doc: None`) or one document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum Query {
    /// Session: every open document.
    Documents,
    /// Document: its summary.
    Summary,
    /// Document: its paragraphs as plain text.
    Paragraphs,
    /// Document: the selection.
    Selection,
    /// Document: the status bar fields.
    Status,
    /// Document: the accessibility tree.
    A11y,
    /// Document: the accessibility tree of the whole document, without bounds, for the
    /// Accessible text view: every paragraph, table and picture (Write), the used cells
    /// of the active sheet (Grid), every slide (Deck).
    A11yDocument,
    /// Document: the selected text as plain text (paragraphs joined by `\n`), for Copy and Cut.
    SelectedText,
    /// Document: the Compatibility Checker (Backstage ▸ Info ▸ Check for Issues): what saving in
    /// `format` (the document's own format when absent) will lose or change, then what was
    /// already lost or kept read-only when the file was opened.
    CompatReport {
        /// The format to save in.
        format: Option<FileFormat>,
    },
    /// Document: one sheet of the print preview (Backstage ▸ Print).
    PrintPreview {
        /// The settings.
        settings: PrintSettings,
        /// The sheet (0-based; clamped).
        sheet: u32,
        /// Device pixels per preview pixel (zoom × device pixel ratio), 0.05–8.
        scale: f32,
    },
    /// Document: the PDF printed with `settings` (Export ▸ PDF on the web).
    Pdf {
        /// The settings.
        settings: PrintSettings,
    },
    /// Document: its collaboration state.
    Collab,
    /// Document: the shared document's versions, oldest first.
    Versions,
    /// Document: the word-level comparison of a Write version with the document.
    VersionCompare {
        /// The version.
        id: String,
    },
    /// Document: the slides of a Deck version with their thumbnails, the changed ones
    /// marked.
    VersionSlides {
        /// The version.
        id: String,
        /// Thumbnail width in CSS px.
        width: f32,
        /// Device pixel ratio.
        dpr: f32,
    },
    /// Document: a question about a Write document (formatting, rulers, panes).
    Write(WriteQuery),
    /// Document: a question about a Grid workbook (state, hit testing, dialogs).
    Grid(GridQuery),
    /// Kiln Deck's chrome state, thumbnails and hit tests.
    Deck(DeckQuery),
    /// Session: the recent and pinned files, pinned first, then the most recently
    /// opened.
    Recents,
    /// Document: its properties (File ▸ Info ▸ Properties) and counts.
    Properties,
    /// Document: Inspect Document: the hidden data it carries.
    Inspect,
}

/// A kind of hidden data Inspect Document looks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum InspectKind {
    /// Comments (and Grid's notes).
    Comments,
    /// Tracked changes (Write).
    Revisions,
    /// Document properties and personal information (author, last modified by, company,
    /// manager).
    Properties,
    /// Hidden worksheets (Grid).
    HiddenSheets,
    /// Hidden slides (Deck).
    HiddenSlides,
    /// Speaker notes (Deck).
    SpeakerNotes,
}

/// One finding of Inspect Document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InspectItem {
    /// What was found.
    pub kind: InspectKind,
    /// How many (comments, changes, sheets, slides; properties set).
    pub count: u32,
}

/// How serious a compatibility issue is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CompatSeverity {
    /// Content or formatting will be lost.
    Loss,
    /// Kept, but saved in a different form.
    Change,
    /// Kept as it was read; nothing new is lost.
    Info,
}

/// Whether an issue comes from saving or was already there when the file was opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CompatOrigin {
    /// Caused by saving.
    Save,
    /// Lost or kept read-only when the file was opened.
    Open,
}

/// One line of the Compatibility Checker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CompatIssue {
    /// A stable code (`image.missing`, `macros.removed`, `drawing.smartArt`, …).
    pub code: String,
    /// Loss, change or information.
    pub severity: CompatSeverity,
    /// One-line summary.
    pub message: String,
    /// Occurrences in the document.
    pub count: u32,
    /// Save or open.
    pub origin: CompatOrigin,
}

/// One paragraph as plain text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ParagraphText {
    /// The paragraph's node id (for `TextPos`).
    pub id: NodeId,
    /// Its text.
    pub text: String,
}

/// The answer to a [`Query`], in the same variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum QueryResult {
    /// Open documents.
    Documents(Vec<DocSummary>),
    /// A summary.
    Summary(DocSummary),
    /// Paragraphs.
    Paragraphs(Vec<ParagraphText>),
    /// The selection.
    Selection(Selection),
    /// The status bar.
    Status(StatusBar),
    /// The accessibility tree.
    A11y(A11yTree),
    /// Plain text.
    Text(String),
    /// The Compatibility Checker's list, most serious first (empty: nothing to report).
    CompatReport(Vec<CompatIssue>),
    /// A print preview sheet.
    PrintPreview(PrintPreview),
    /// A PDF.
    Pdf(PdfFile),
    /// The answer to [`Query::Collab`].
    Collab(CollabState),
    /// The answer to [`Query::Versions`].
    Versions(Vec<VersionEntry>),
    /// The answer to [`Query::VersionCompare`].
    VersionCompare(Vec<VersionParagraph>),
    /// The answer to [`Query::VersionSlides`].
    VersionSlides(Box<VersionSlides>),
    /// The answer to a [`Query::Write`].
    Write(WriteQueryResult),
    /// The answer to a [`Query::Grid`].
    Grid(GridQueryResult),
    /// Kiln Deck's answers.
    Deck(DeckQueryResult),
    /// The answer to [`Query::Recents`].
    Recents(Vec<RecentFile>),
    /// The answer to [`Query::Properties`].
    Properties(Box<DocPropertiesInfo>),
    /// The answer to [`Query::Inspect`]: every kind the app looks for, found or not.
    Inspect(Vec<InspectItem>),
}

impl Query {
    /// Whether the query is about one document.
    #[must_use]
    pub const fn needs_document(&self) -> bool {
        !matches!(self, Query::Documents | Query::Recents)
    }
}
