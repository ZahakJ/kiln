// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-protocol
//!
//! The single contract between the Rust core and the TypeScript presentation layer (PLAN.md §3.5).
//! Every type here derives `serde` and `ts_rs::TS`; `cargo xtask protocol` exports them to
//! `packages/protocol/src/generated/`, together with `catalog.json` (every [`CommandMeta`]),
//! `schema.json` (the serde data model, which drives the TypeScript postcard decoder) and the
//! postcard test vectors in `tests/vectors/`.
//!
//! Encodings: small messages (commands, results, events, queries) are JSON; frames and other bulk
//! payloads are `postcard`.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod a11y;
pub mod app;
pub mod commands;
pub mod error;
pub mod event;
pub mod exec;
pub mod geom;
pub mod host;
pub mod print;
pub mod query;
pub mod scene;
pub mod selection;
pub mod status;
pub mod summary;
pub mod viewport;

#[cfg(any(test, feature = "schema"))]
pub mod samples;
#[cfg(any(test, feature = "schema"))]
pub mod schema;

pub use a11y::{A11yNode, A11yRole, A11yTree};
pub use app::{App, AppMask, DocKind, FileFormat};
pub use commands::{
    Availability, CaretMotion, Command, CommandInfo, CommandMeta, DeckCommand, GridCommand,
    NotesCommand, SelectUnit, SuiteCommand, Tier, WriteCommand, catalog, meta_by_id,
};
pub use error::ErrorInfo;
pub use event::{Event, Progress, RecoveryEntry};
pub use exec::{CoreInfo, ExecResult, ExecValue, Outcome};
pub use host::{FileFilter, HostOp, HostReply, HostRequest, RecentFile, RequestId};
pub use kiln_common::{DocId, ErrorKind, NodeId};
pub use print::{
    PdfFile, PrintMargins, PrintOrientation, PrintPaper, PrintParity, PrintPreview, PrintSettings,
};
pub use query::{CompatIssue, CompatOrigin, CompatSeverity, ParagraphText, Query, QueryResult};
pub use scene::SceneDelta;
pub use selection::{
    Caret, CellAddr, CellRange, Compass, Handle, HandleKind, SelKind, Selection, TextPos,
};
pub use status::{CellStats, Position, StatusBar};
pub use summary::DocSummary;
pub use viewport::{ViewMode, Viewport};

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The wire protocol version. Bump it on any change that an older UI could misread; the UI checks
/// it against the value compiled into `@kiln/protocol` at startup (`Ping`).
pub const PROTOCOL_VERSION: u32 = 1;

/// Hints for opening bytes whose origin the core cannot inspect.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct OpenHint {
    /// File name (used for the title and, without `format`, to detect the format).
    pub name: Option<String>,
    /// Where the bytes came from, so Save can write back.
    pub path: Option<String>,
    /// The format, when known.
    pub format: Option<FileFormat>,
    /// Open read-only.
    pub read_only: bool,
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
