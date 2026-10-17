// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-io-odf
//!
//! Imports and exports OpenDocument (ODF 1.3) text documents, spreadsheets and presentations
//! (`.odt`, `.ods`, `.odp`, and their templates on input) through the same models as the Office
//! formats: [`kiln_doc::Document`], [`kiln_sheet::Workbook`] and [`kiln_deck::Presentation`].
//!
//! Kiln's models are shaped like OOXML, so every ODF construct goes through a mapping table
//! (documented on the modules: [`props`] for character and paragraph formatting, [`text`] for
//! documents, [`sheet`] for spreadsheets with [`formula`] and [`numfmt`], [`deck`] for
//! presentations, [`meta`] for properties). What has no counterpart is reported in the read's
//! `warnings`, which the Compatibility Checker lists.
//!
//! ```no_run
//! # fn main() -> Result<(), kiln_io_odf::OdfError> {
//! let bytes = std::fs::read("report.odt").unwrap_or_default();
//! let read = kiln_io_odf::read_odt(&bytes)?;
//! let saved = kiln_io_odf::write_odt(&read.document, &kiln_io_odf::WriteOptions::default())?;
//! # let _ = saved;
//! # Ok(())
//! # }
//! ```
//!
//! Readers never panic on input: malformed parts and values become warnings or defaults, and
//! only a package that is not an ODF document of the expected kind is an error. Packages are
//! opened with zip-bomb limits; XML is parsed without DTDs or external entities.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod deck;
pub mod el;
pub mod formula;
pub mod meta;
pub mod numfmt;
pub mod package;
pub mod props;
pub mod sheet;
pub mod text;
pub mod units;
pub mod xml;

pub use deck::{OdpRead, deck_facts, deck_text, read_odp, write_odp};
pub use package::{OdfKind, Package};
pub use sheet::{OdsRead, read_ods, write_ods};
pub use text::{OdtRead, read_odt, write_odt};

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The `meta:generator` Kiln writes.
pub const GENERATOR: &str = concat!("Kiln/", env!("CARGO_PKG_VERSION"), " (kiln-io-odf)");

/// Why a package could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OdfError {
    /// Not a zip archive.
    #[error("not an OpenDocument file (not a zip archive)")]
    NotZip,
    /// A zip, but not an ODF package (or not of the kind asked for).
    #[error("not an OpenDocument file: {0}")]
    NotOdf(String),
    /// Password-protected (ODF encryption, or an OLE container).
    #[error("the file is password-protected or encrypted")]
    Encrypted,
    /// A broken archive or part.
    #[error("damaged file: {0}")]
    Corrupt(String),
    /// A limit was exceeded before anything was inflated.
    #[error("the file is too large or suspicious: {0}")]
    ZipBomb(String),
    /// A part that is not well-formed XML.
    #[error("{part} is not well-formed XML: {message}")]
    Xml {
        /// The part.
        part: String,
        /// What is wrong.
        message: String,
    },
    /// The model refused what was read.
    #[error("the document could not be built: {0}")]
    Model(String),
    /// Writing failed.
    #[error("could not write the file: {0}")]
    Write(String),
}

/// How to write.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteOptions {
    /// Write a template (`.ott`, `.ots`, `.otp` media types).
    pub template: bool,
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
