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
    /// A picture of the first page or slide (`data:image/png;base64,…`), when one was made.
    pub thumbnail: Option<String>,
}

/// Where the core keeps something in the host's app storage: natively a `redb`
/// database in the app's data directory, on the web the Origin Private File System.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub enum StoreArea {
    /// The autosave journals of open documents (one per document, removed on a clean close).
    Journal,
    /// Version history of files that are not native bundles.
    Versions,
    /// Recent and pinned files.
    Recents,
    /// The user dictionary.
    Dictionary,
    /// User preferences.
    Prefs,
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
    /// Open a local file in the system's default application (the PDF to print, in the
    /// system viewer and its print dialog). Reply: `Done`.
    OpenFile {
        /// The file.
        path: String,
    },
    /// Print a PDF with the platform's print dialog (the web build prints it from a
    /// hidden frame). Reply: `Done`.
    PrintPdf {
        /// A name for the job.
        name: String,
        /// The PDF.
        #[serde(with = "kiln_scene::bytes")]
        #[ts(type = "Array<number>")]
        bytes: Vec<u8>,
    },
    /// Open a second window of the app on `route` (Presenter View's audience window).
    /// Reply: `Done`. Hosts without windows (the web) open a tab themselves and never get it.
    OpenWindow {
        /// The app route (`#/present?doc=3&view=audience`).
        route: String,
        /// The window's title.
        title: String,
    },
    /// opens WebSocket `socket` (a number the core chose) to `url`. `Done` once it is
    /// open, `Failed` when it cannot be.
    SocketOpen {
        /// The socket.
        socket: u32,
        /// `ws://` or `wss://`.
        url: String,
    },
    /// Sends a binary message on a socket (`Done`).
    SocketSend {
        /// The socket.
        socket: u32,
        /// The message.
        #[serde(with = "kiln_scene::bytes")]
        #[ts(type = "Array<number>")]
        bytes: Vec<u8>,
    },
    /// The next binary message the socket receives (`Bytes`), or `Empty` once it is closed.
    /// The core asks again after each message.
    SocketReceive {
        /// The socket.
        socket: u32,
    },
    /// Closes a socket (`Done`); a waiting `SocketReceive` is answered `Empty`.
    SocketClose {
        /// The socket.
        socket: u32,
    },
    /// Answers `Done` after `ms` milliseconds (reconnecting, presence and version timers).
    Timer {
        /// The delay.
        ms: u32,
    },
    /// reads a value of the app storage. Reply: `Bytes`, or `Empty` when there is none.
    StoreGet {
        /// The area.
        area: StoreArea,
        /// The key.
        key: String,
    },
    /// Replaces a value of the app storage, durably (`Done`).
    StorePut {
        /// The area.
        area: StoreArea,
        /// The key.
        key: String,
        /// The value.
        #[serde(with = "kiln_scene::bytes")]
        #[ts(type = "Array<number>")]
        bytes: Vec<u8>,
    },
    /// Appends to a value of the app storage (`Done`); reading it gives everything appended, in
    /// order.
    StoreAppend {
        /// The area.
        area: StoreArea,
        /// The key.
        key: String,
        /// What to append.
        #[serde(with = "kiln_scene::bytes")]
        #[ts(type = "Array<number>")]
        bytes: Vec<u8>,
    },
    /// Removes a value of the app storage (`Done`, also when there was none).
    StoreDelete {
        /// The area.
        area: StoreArea,
        /// The key.
        key: String,
    },
    /// The keys of an area that start with `prefix`, sorted. Reply: `Keys`. In the journal area
    /// the host leaves out the journals of documents open in another window or process.
    StoreList {
        /// The area.
        area: StoreArea,
        /// The prefix (empty for every key).
        prefix: String,
    },
    /// answers `Done` once a file is changed by another program (natively; hosts that
    /// cannot watch files never answer). The core asks again after each change.
    WatchFile {
        /// The file.
        path: String,
    },
    /// Stops watching a file (`Done`).
    UnwatchFile {
        /// The file.
        path: String,
    },
    /// The size and modification time of a file. Reply: `Stamp`, `Empty` when there is no such
    /// file, `Failed` (`Unsupported`) for a host that cannot tell.
    StatFile {
        /// Path or host handle.
        path: String,
    },
}

/// A file's size and modification time: what tells another program's change of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct FileStamp {
    /// Bytes.
    pub size: u64,
    /// When the file was last modified, in milliseconds since the Unix epoch.
    pub modified_ms: u64,
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
    /// Keys of the app storage (`StoreList`).
    Keys(Vec<String>),
    /// A file's stamp (`StatFile`).
    Stamp(FileStamp),
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
