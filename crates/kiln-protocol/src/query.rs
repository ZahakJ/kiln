// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only questions to the core (pane data, document text for the scratch view).

use kiln_common::NodeId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::a11y::A11yTree;
use crate::selection::Selection;
use crate::status::StatusBar;
use crate::summary::DocSummary;

/// A question about the session (`doc: None`) or one document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
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
}

impl Query {
    /// Whether the query is about one document.
    #[must_use]
    pub const fn needs_document(&self) -> bool {
        !matches!(self, Query::Documents)
    }
}
