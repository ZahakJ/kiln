// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-ooxml
//!
//! The Office Open XML foundation shared by the `.docx`, `.xlsx` and `.pptx` readers and writers:
//!
//! - [`opc`]: the zip container ([`opc::OpcPackage`]) with zip-bomb limits, part names,
//!   `[Content_Types].xml`, relationships and a writer that keeps untouched parts byte-identical.
//! - [`xml`]: an order-preserving element tree over `quick-xml` (no DTDs, no entity expansion),
//!   Office's namespace prefixes, query helpers and a deterministic writer.
//! - [`st`] and [`units`]: simple types and measurement units.
//! - [`drawingml`]: colours, fills, lines, effects, text properties, themes and geometry with the
//!   187 preset shapes evaluated to `kurbo` paths.
//! - [`passthrough`]: "never destroy what you don't understand" — parts a reader does not model
//!   are carried to the written package with their relationships fixed up.
//! - [`props`]: document properties (core, extended, custom).
//!
//! The module layout is documented in the crate README so the format crates can follow it.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod drawingml;
pub mod opc;
pub mod passthrough;
pub mod props;
pub mod st;
pub mod units;
pub mod xml;

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
