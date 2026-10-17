// SPDX-License-Identifier: AGPL-3.0-or-later
//! Requests from the core to its host (files, dialogs, clipboard, preferences) and their replies.
//!
//! The core never touches the outside world directly. A request travels as
//! `Event::HostRequest`; the shell answers with `host_reply(id, reply)`. Hosts that can answer
//! synchronously (the desktop's file system) do so inside the session without an event.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::app::DocKind;
use crate::error::ErrorInfo;

/// Identifies a host request within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(transparent)]
pub struct RequestId(pub u32);

/// A file-type filter for Open/Save dialogs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FileFilter {
    /// Label, e.g. "Word Document".
    pub name: String,
    /// Extensions without dots.
    pub extensions: Vec<String>,
}

/// A recently used file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RecentFile {
    /// Path or host handle.
    pub path: String,
    /// Display name.
    pub name: String,
    /// Kind of document.
    pub kind: DocKind,
    /// Last opened, milliseconds since the Unix epoch.
    #[ts(type = "number")]
    pub opened_at_ms: u64,
    /// Pinned to the top of the list.
    pub pinned: bool,
}

/// What the core asks the host to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum HostOp {
    /// Read a whole file. Reply: `Bytes`.
    ReadFile {
        /// Path or host handle.
        path: String,
    },
    /// Write a whole file atomically. Reply: `Done`.
    WriteFile {
        /// Path or host handle.
        path: String,
        /// Contents.
        bytes: Vec<u8>,
    },
    /// Show the Open dialog. Reply: `Path` or `Cancelled`.
    PickOpen {
        /// Filters, first is the default.
        filters: Vec<FileFilter>,
    },
    /// Show the Save As dialog. Reply: `Path` or `Cancelled`.
    PickSave {
        /// Suggested file name.
        suggested_name: String,
        /// Filters, first is the default.
        filters: Vec<FileFilter>,
    },
    /// List recent files. Reply: `Recents`.
    Recents,
    /// Store a UI preference. Reply: `Done`.
    StorePref {
        /// Key.
        key: String,
        /// JSON value.
        value: String,
    },
    /// Load a UI preference. Reply: `Text` or `Empty`.
    LoadPref {
        /// Key.
        key: String,
    },
    /// Read the clipboard. Reply: `Clipboard`.
    ClipboardRead,
    /// Write the clipboard. Reply: `Done`.
    ClipboardWrite {
        /// Plain text.
        text: String,
        /// HTML flavour.
        html: Option<String>,
    },
    /// Open a URL in the system browser (http, https and mailto only). Reply: `Done`.
    OpenExternal {
        /// The URL.
        url: String,
    },
    /// Open a local file in the system's default application (WP-67: the PDF to print, in the
    /// system viewer and its print dialog). Reply: `Done`.
    OpenFile {
        /// The file.
        path: String,
    },
    /// Print a PDF with the platform's print dialog (WP-67: the web build prints it from a
    /// hidden frame). Reply: `Done`.
    PrintPdf {
        /// A name for the job.
        name: String,
        /// The PDF.
        #[serde(with = "kiln_scene::bytes")]
        #[ts(type = "Array<number>")]
        bytes: Vec<u8>,
    },
}

/// A request to the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HostRequest {
    /// Correlates the reply.
    pub id: RequestId,
    /// What to do.
    pub op: HostOp,
}

/// The host's answer to a [`HostRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum HostReply {
    /// Done, nothing to return.
    Done,
    /// Nothing found (an unknown preference).
    Empty,
    /// File contents.
    Bytes(Vec<u8>),
    /// A chosen path or handle.
    Path(String),
    /// A text value.
    Text(String),
    /// Recent files.
    Recents(Vec<RecentFile>),
    /// Clipboard contents.
    Clipboard {
        /// Plain text.
        text: String,
        /// HTML flavour.
        html: Option<String>,
    },
    /// The user cancelled the dialog.
    Cancelled,
    /// The host failed.
    Failed(ErrorInfo),
}
