// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-io-pptx
//!
//! Reads `.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx` and `.ppsm` files into the Kiln Deck model
//! ([`kiln_deck::Presentation`]), keeping every part it does not understand (PLAN.md §3.10),
//! and writes the model back ([`write_pptx`]): model parts regenerated, kept XML merged back in
//! place, everything else byte for byte.
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
//! Saving: [`write_pptx`] with the [`WriteSource`] of the read (and the original package, so
//! relationship ids and unchanged parts are kept), or [`rewrite_pptx`] for a read → write round
//! trip. [`compat_report`] is the Compatibility Checker's list for a save.
//!
//! What is read, what is approximated and what is kept verbatim is listed in `docs/FORMATS.md`
//! and the crate README. The reader never panics on input: malformed parts become diagnostics,
//! and only a package that is not a presentation at all is an error.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod common;
pub mod read;
pub mod write;

pub use common::extras::{OpaqueShape, PptxExtras, ShapeSource};
pub use common::kind::PptxKind;
pub use common::warnings::{PptxWarning, Severity, WarningCode};
pub use read::{
    PptxError, PptxRead, ReadOptions, SlideSummary, Summary, body_text, plain_text, read_package,
    read_pptx, read_pptx_with, shape_kind, snapshot_facts,
};
pub use write::{
    CompatIssue, CompatOptions, CompatOrigin, CompatSeverity, PptxWriteError, PptxWritten,
    RewriteError, TouchedParts, WriteIssue, WriteOptions, WriteSource, compat_report, rewrite_pptx,
    write_pptx,
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
