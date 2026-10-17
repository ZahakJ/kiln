// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-io-pptx
//!
//! Reads `.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx` and `.ppsm` files into the Kiln Deck model
//! ([`kiln_deck::Presentation`]), keeping every part it does not understand (PLAN.md §3.10).
//! The writer (WP-79) saves the model back.
//!
//! ```no_run
//! # fn main() -> Result<(), kiln_io_pptx::PptxError> {
//! let bytes = std::fs::read("talk.pptx").unwrap_or_default();
//! let read = kiln_io_pptx::read_pptx(&bytes)?;
//! let deck = &read.presentation; // masters, layouts, slides, notes, comments, sections
//! println!("{}", kiln_io_pptx::Summary::of(&read).to_text());
//! for w in &read.warnings {
//!     eprintln!("{w}"); // what the Compatibility Checker reports
//! }
//! # let _ = deck;
//! # Ok(())
//! # }
//! ```
//!
//! What is read, what is approximated and what is kept verbatim is listed in `docs/FORMATS.md`
//! and the crate README. The reader never panics on input: malformed parts become diagnostics,
//! and only a package that is not a presentation at all is an error.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod common;
pub mod read;

pub use common::extras::{OpaqueShape, PptxExtras, ShapeSource};
pub use common::kind::PptxKind;
pub use common::warnings::{PptxWarning, Severity, WarningCode};
pub use read::{
    PptxError, PptxRead, ReadOptions, SlideSummary, Summary, body_text, plain_text, read_package,
    read_pptx, read_pptx_with, shape_kind, snapshot_facts,
};

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
