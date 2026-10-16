// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-session
//!
//! The facade over the whole core and the only API the shells use (PLAN.md §3.3): a [`Session`]
//! keeps the open documents, validates every [`Command`](kiln_protocol::Command) against its
//! `CommandMeta` (app and availability) before dispatching it to the document's model, keeps an
//! undo history per document, talks to the outside world through a [`Host`], and queues
//! [`Event`](kiln_protocol::Event)s for the UI to [`poll`](Session::poll).
//!
//! Natively the session runs on its own thread behind a [`SessionHandle`] (flume channel,
//! one-shot replies); the web build drives a `Session` directly inside its Web Worker.
//!
//! Document apps register in [`apps`]; the `scratch` app (plain paragraphs, no layout) proves the
//! loop until the Write engine lands.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod apps;
mod host;
mod model;
mod session;
pub mod undo;

#[cfg(not(target_arch = "wasm32"))]
mod handle;

#[cfg(not(target_arch = "wasm32"))]
pub use handle::{SessionHandle, SessionThread};
#[cfg(not(target_arch = "wasm32"))]
pub use host::NativeHost;
pub use host::{DeferredHost, Host};
pub use model::{AppFactory, Change, DocModel};
pub use session::Session;

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
