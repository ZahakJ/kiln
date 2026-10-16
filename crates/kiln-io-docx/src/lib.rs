// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-io-docx
//!
//! Reads `.docx`, `.dotx`, `.docm` and `.dotm` files into the Kiln Write model
//! ([`kiln_doc::Document`]), keeping every part it does not understand (the writer, WP-65, puts
//! them back).
//!
//! ```no_run
//! # fn main() -> Result<(), kiln_io_docx::DocxError> {
//! let bytes = std::fs::read("report.docx").unwrap_or_default();
//! let read = kiln_io_docx::read_docx(&bytes)?;
//! let doc = &read.document; // sections, headers/footers, notes, comments, styles, lists
//! for w in &read.warnings {
//!     eprintln!("{w}"); // what the Compatibility Checker reports
//! }
//! # let _ = doc;
//! # Ok(())
//! # }
//! ```
//!
//! What is read and what is kept verbatim is listed in `docs/FORMATS.md` and the crate README.
//! The reader never panics on input: malformed parts become diagnostics, and only a package that
//! is not a Word document at all is an error.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod common;
pub mod read;

pub use common::extras::DocxExtras;
pub use common::symbols::{symbol_code_to_unicode, symbol_text, symbol_to_unicode};
pub use common::warnings::{DocxWarning, Severity, WarningCode};
pub use read::{
    ChartRef, DocxError, DocxKind, DocxRead, ReadOptions, RelTarget, Summary, read_docx,
    read_docx_with, read_package, snapshot_facts,
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
