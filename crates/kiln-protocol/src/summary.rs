// SPDX-License-Identifier: AGPL-3.0-or-later
//! Document summaries: what the title bar, Backstage and the command state need.

use kiln_common::DocId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::app::{App, DocKind, FileFormat};

/// The state of an open document as the chrome shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DocSummary {
    /// The document.
    pub doc: DocId,
    /// Its kind.
    pub kind: DocKind,
    /// The app that edits it.
    pub app: App,
    /// Title for the title bar ("Document1", or the file name without extension).
    pub title: String,
    /// The file it was opened from or saved to.
    pub path: Option<String>,
    /// The format it saves to.
    pub format: Option<FileFormat>,
    /// Incremented by every change, undo and redo.
    #[ts(type = "number")]
    pub revision: u64,
    /// Changed since it was opened or last saved.
    pub dirty: bool,
    /// Opened read-only.
    pub read_only: bool,
    /// Whether Undo is available.
    pub can_undo: bool,
    /// Whether Redo is available.
    pub can_redo: bool,
    /// The command id Undo would revert (for "Undo Typing"), if any.
    pub undo_label: Option<String>,
    /// The command id Redo would repeat, if any.
    pub redo_label: Option<String>,
}
