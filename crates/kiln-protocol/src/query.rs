// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only questions to the core (pane data, document text for the scratch view).

use kiln_common::NodeId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::a11y::A11yTree;
use crate::app::FileFormat;
use crate::commands::{GridQuery, GridQueryResult, WriteQuery, WriteQueryResult};
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
    /// Document: the selected text as plain text (paragraphs joined by `\n`), for Copy and Cut
    /// (WP-54).
    SelectedText,
    /// Document: the Compatibility Checker (Backstage ▸ Info ▸ Check for Issues): what saving in
    /// `format` (the document's own format when absent) will lose or change, then what was
    /// already lost or kept read-only when the file was opened (WP-65).
    CompatReport {
        /// The format to save in.
        format: Option<FileFormat>,
    },
    /// Document: one sheet of the print preview (Backstage ▸ Print, WP-67).
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
    /// Document: a question about a Write document (formatting, rulers, panes; WP-64).
    Write(WriteQuery),
    /// Document: a question about a Grid workbook (state, hit testing, dialogs; WP-71).
    Grid(GridQuery),
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
    /// The answer to a [`Query::Write`].
    Write(WriteQueryResult),
    /// The answer to a [`Query::Grid`].
    Grid(GridQueryResult),
}

impl Query {
    /// Whether the query is about one document.
    #[must_use]
    pub const fn needs_document(&self) -> bool {
        !matches!(self, Query::Documents)
    }
}
