// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-session
//!
//! The facade over the whole core and the only API the shells use: a [`Session`]
//! keeps the open documents, validates every [`Command`](kiln_protocol::Command) against its
//! `CommandMeta` (app and availability) before dispatching it to the document's model, keeps an
//! undo history per document, talks to the outside world through a [`Host`], and queues
//! [`Event`](kiln_protocol::Event)s for the UI to [`poll`](Session::poll).
//!
//! Natively the session runs on its own thread behind a [`SessionHandle`] (flume channel,
//! one-shot replies); the web build drives a `Session` directly inside its Web Worker.
//!
//! Document apps register in [`apps`]; the `scratch` app (plain paragraphs laid out on pages by
//! `kiln-text`) proves the loop until the Write engine lands. [`frame`] turns a model's Scene into
//! the `SceneDelta`s the painters apply.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod a11y;
pub mod apps;
/// Base64 for the clipboard flavours and exported pictures.
pub(crate) mod b64;
pub mod collab;
pub mod frame;
mod host;
mod model;
pub mod print;
/// document properties of workbooks and presentations, untitled names by language.
mod props;
mod session;
/// journals, recovery, AutoSave, bundles, recents and local versions.
mod storage;
pub mod undo;

#[cfg(not(target_arch = "wasm32"))]
mod handle;

#[cfg(all(feature = "collab", not(target_arch = "wasm32")))]
pub use collab::NetHost;
pub use collab::{CollabBackend, Collaborative, Whereabouts};
#[cfg(not(target_arch = "wasm32"))]
pub use handle::{SessionHandle, SessionThread};
#[cfg(not(target_arch = "wasm32"))]
pub use host::NativeHost;
pub use host::{DeferredHost, Deliver, Host};
pub use model::{AppFactory, Change, DocModel, ModelNotice};
pub use session::Session;
#[doc(hidden)]
pub use storage::JournalReplay;
#[cfg(not(target_arch = "wasm32"))]
pub use storage::StoredHost;

/// The kinds of characters laid out without a font since the last call (see
/// `kiln_text::take_missing`).
#[must_use]
pub fn take_missing_fonts() -> u32 {
    kiln_text::take_missing()
}

/// The Chinese, Japanese and Korean characters laid out without a font since the last call: the
/// web core fetches the slices of its CJK font that have them.
#[must_use]
pub fn take_missing_chars() -> Vec<u32> {
    kiln_text::take_missing_chars()
}

/// Gives the layout a document font at run time (TrueType or OpenType bytes): the web core
/// embeds none and the page registers Carlito before the first document opens (budget B12);
/// a font registered later reaches the open Write documents at their next frame.
/// Returns `false` when the font was already registered.
pub fn register_font(bytes: Vec<u8>) -> bool {
    apps::scratch::fonts::register(bytes)
}

/// Gives Kiln Grid its function documentation (the web core does not compile it in, budget B12):
/// one JSON object mapping each file stem of `crates/kiln-formula/docs` (`MATH`) to the file's
/// content. Returns whether it was taken (native builds have it already).
pub fn load_function_docs(json: &str) -> bool {
    kiln_formula::functions::load_docs(json)
}

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}

/// Tests only (`ownership-escape`): review 5's bypasses of Kiln Grid's ledger, planted at run
/// time on the Sheet Tab Color command of this thread (3 to 6, or none).
#[cfg(feature = "ownership-escape")]
#[doc(hidden)]
pub mod grid_plants {
    pub use crate::apps::grid::ledger::escape::set;
}

/// The workbook type of Kiln Grid's documents, for the tests of its ownership (`ledger.rs`).
#[doc(hidden)]
pub mod grid_ledger {
    pub use crate::apps::grid::ledger::Book;
    pub use crate::apps::grid::storage::Replaying;
}
