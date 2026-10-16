// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-io-xlsx
//!
//! Reads and writes `.xlsx`, `.xlsm`, `.xltx` and `.xltm` workbooks to and from the Kiln Grid
//! model ([`kiln_sheet::Workbook`]), keeping every part it does not understand (PLAN.md §3.10).
//!
//! - [`read_xlsx`] builds the workbook — cells (shared and inline strings, rich text, every
//!   value type, shared formulas expanded, legacy array formulas and dynamic arrays), styles,
//!   defined names, views, column and row properties, merged cells, conditional formatting
//!   (with the Excel 2010 options), data validation, hyperlinks, page setup, AutoFilter, tables,
//!   notes, threaded comments and drawing anchors — and an [`XlsxSource`] holding the original
//!   package and the XML of what it read. The worksheet's `sheetData` is streamed, never built
//!   into a tree, so a million cells open in well under a second.
//! - [`write_xlsx`] writes a workbook. Given the source, it starts from the original package:
//!   every part the model covers is regenerated, reusing the XML read wherever the model still
//!   holds the same value (attributes and extensions the model does not cover survive), and
//!   every other part — pivot tables and caches, the VBA project, external links, custom XML,
//!   charts, pictures, printer settings — is copied byte for byte. Without a source it writes a
//!   new package. Output is deterministic.
//! - [`rewrite_xlsx`] is read → write without edits, the fidelity round trip.
//! - [`snapshot_facts`] and [`snapshot_cells`] describe a workbook for the fidelity scorer.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod facts;
pub mod formula_text;
mod parts;
mod read;
mod sheet_xml;
mod source;
mod strings;
mod styles;
pub mod text;
mod write;
mod xmlx;

pub use facts::{snapshot_cells, snapshot_facts};
pub use read::{ReadOptions, XlsxRead, read_xlsx, read_xlsx_with};
pub use sheet_xml::NO_DXF;
pub use source::{XlsxKind, XlsxSource};
pub use write::{WriteOptions, XlsxWritten, rewrite_xlsx, write_xlsx};

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Why a workbook could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum XlsxError {
    /// The package itself (zip, content types, limits, encryption).
    #[error(transparent)]
    Package(#[from] kiln_ooxml::opc::OpcError),
    /// The package is not a spreadsheet.
    #[error("not an Excel workbook: {0}")]
    NotWorkbook(String),
    /// A part is not well-formed XML.
    #[error("{part}: {message}")]
    Xml {
        /// The part.
        part: String,
        /// What is wrong.
        message: String,
    },
    /// The model refused what was read.
    #[error("the workbook could not be built: {0}")]
    Model(#[from] kiln_sheet::ModelError),
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
