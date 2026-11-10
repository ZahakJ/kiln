// SPDX-License-Identifier: AGPL-3.0-or-later
//! Events from the core to the UI.

use kiln_common::DocId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::app::{DocKind, FileFormat};
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

/// A document that autosave can restore after a crash (the Document Recovery pane).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RecoveryEntry {
    /// The journal (what `suite.file.recovery_open` and `suite.file.recovery_delete` take).
    pub key: String,
    /// The kind of document.
    pub kind: DocKind,
    /// Title of the document.
    pub title: String,
    /// The file it belonged to, if saved before.
    pub path: Option<String>,
    /// The file's format, if saved before.
    pub format: Option<FileFormat>,
    /// When it was last autosaved, milliseconds since the Unix epoch.
    #[ts(type = "number")]
    pub saved_at_ms: u64,
    /// Why the journal cannot be opened (written by a newer version of Kiln, damaged); it can
    /// still be deleted.
    pub problem: Option<String>,
}

/// What a recovered document did not get back, for the message that opens with it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct RecoveryNote {
    /// The replay stopped before a change it could not repeat: the document is as it was
    /// before that change.
    pub stopped: Option<RecoveryStop>,
    /// Text that was being typed in a cell and not entered. It is not put back on its own.
    pub draft: Option<RecoveredDraft>,
}

/// Where a recovered document stops short.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RecoveryStop {
    /// The command id of the change not kept (`collab.remote`: edits by others); `None` when a
    /// change never reached the disk.
    pub command: Option<String>,
    /// The changes after it, which were not kept either.
    pub after: u32,
    /// When it was made, milliseconds since the Unix epoch (0: not known).
    #[ts(type = "number")]
    pub at_ms: u64,
}

/// Text being typed in a cell when the document closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RecoveredDraft {
    /// The cell, as Go To takes it (`Sheet1!F8`, `'A1'!F8`), to name it.
    pub cell: String,
    /// The text.
    pub text: String,
    /// The sheet's id, which a rename leaves alone (`GridCommand::PutBackDraft` takes it).
    #[serde(default)]
    pub sheet: u32,
    /// The cell's row (0-based).
    #[serde(default)]
    pub row: u32,
    /// The cell's column (0-based).
    #[serde(default)]
    pub col: u32,
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
    /// A PDF export finished: written to `path`, or `None` when the save dialog was
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
    /// Documents that can be recovered, newest first: the answer to
    /// `suite.file.recovery_scan`, and the list again after one was opened or deleted.
    Recovery {
        /// Recoverable documents.
        entries: Vec<RecoveryEntry>,
    },
    /// the recent files changed (also what `Query::Recents` answers now).
    Recents {
        /// The recent and pinned files, pinned first.
        files: Vec<crate::host::RecentFile>,
    },
    /// another program changed the file a document was opened from or saved to.
    FileChanged {
        /// The document.
        doc: DocId,
    },
    /// Collaboration state of a document.
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
    /// A document came back from Document Recovery without all it had (`suite.file.recovery_open`).
    Recovered {
        /// The recovered document.
        doc: DocId,
        /// What it did not get back.
        note: RecoveryNote,
    },
    /// A slide show moved on: every window showing the document follows it (Presenter
    /// View and the audience window on the desktop).
    Show {
        /// The document.
        doc: DocId,
        /// The show's state.
        status: crate::commands::deck_show::DeckShowStatus,
    },
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
            | Event::Collab { doc, .. }
            | Event::FileChanged { doc }
            | Event::Show { doc, .. }
            | Event::Recovered { doc, .. } => Some(*doc),
            Event::Progress(p) => p.doc,
            Event::Error { doc, .. } => *doc,
            Event::Recovery { .. } | Event::Recents { .. } | Event::HostRequest(_) => None,
        }
    }
}
