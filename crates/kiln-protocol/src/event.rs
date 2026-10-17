// SPDX-License-Identifier: AGPL-3.0-or-later
//! Events from the core to the UI.

use kiln_common::DocId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::ErrorInfo;
use crate::host::HostRequest;
use crate::scene::SceneDelta;
use crate::selection::Selection;
use crate::status::StatusBar;
use crate::summary::DocSummary;

/// Long-running work (open, save, recalc) for the status bar's progress indicator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Progress {
    /// The document, if the work belongs to one.
    pub doc: Option<DocId>,
    /// i18n key of the task, e.g. `progress.open`.
    pub task: String,
    /// Units done.
    pub done: u32,
    /// Units in total (0 = unknown).
    pub total: u32,
}

/// A document that autosave can restore after a crash (filled in by WP-83).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RecoveryEntry {
    /// Title of the document.
    pub title: String,
    /// The file it belonged to, if saved before.
    pub path: Option<String>,
    /// When it was last autosaved, milliseconds since the Unix epoch.
    #[ts(type = "number")]
    pub saved_at_ms: u64,
}

/// Something the UI should know.
// `Changed` is the common case and is serialised right away; boxing it would only add an allocation.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum Event {
    /// A document was opened or created.
    Opened {
        /// Its summary.
        summary: DocSummary,
    },
    /// A document changed (content, selection, undo state or view).
    Changed {
        /// The summary (revision, dirty, undo/redo state).
        summary: DocSummary,
        /// The selection.
        selection: Selection,
        /// The status bar.
        status: StatusBar,
    },
    /// A document was saved.
    Saved {
        /// The summary after saving (path, format, `dirty: false`).
        summary: DocSummary,
    },
    /// A PDF export finished (WP-67): written to `path`, or `None` when the save dialog was
    /// cancelled.
    Exported {
        /// The document.
        doc: DocId,
        /// Where the PDF was written.
        path: Option<String>,
    },
    /// A document was closed.
    Closed {
        /// The document.
        doc: DocId,
    },
    /// A new frame is ready (pushed by hosts that stream frames; others call `frame`).
    Frame {
        /// The document.
        doc: DocId,
        /// The changes.
        delta: SceneDelta,
    },
    /// Progress of long-running work.
    Progress(Progress),
    /// Documents that can be recovered (WP-83).
    Recovery {
        /// Recoverable documents.
        entries: Vec<RecoveryEntry>,
    },
    /// Collaboration state of a document (WP-82).
    Collab {
        /// The document.
        doc: DocId,
        /// Other people connected.
        peers: u32,
        /// Whether the sync connection is up.
        connected: bool,
    },
    /// An asynchronous operation failed.
    Error {
        /// The document, if the error belongs to one.
        doc: Option<DocId>,
        /// What failed.
        error: ErrorInfo,
    },
    /// The core needs the host (see [`crate::host`]).
    HostRequest(HostRequest),
}

impl Event {
    /// The document the event is about, if any.
    #[must_use]
    pub fn doc(&self) -> Option<DocId> {
        match self {
            Event::Opened { summary }
            | Event::Changed { summary, .. }
            | Event::Saved { summary } => Some(summary.doc),
            Event::Closed { doc }
            | Event::Exported { doc, .. }
            | Event::Frame { doc, .. }
            | Event::Collab { doc, .. } => Some(*doc),
            Event::Progress(p) => p.doc,
            Event::Error { doc, .. } => *doc,
            Event::Recovery { .. } | Event::HostRequest(_) => None,
        }
    }
}
