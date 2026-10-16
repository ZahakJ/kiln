// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-ooxml
//!
//! The Office Open XML foundation: OPC packages (zip), an order-preserving XML layer on `quick-
//! xml`, namespaces, simple types, units, DrawingML (colours, fills, lines, effects, text, themes,
//! 187 preset geometries), opaque-part passthrough, document properties and a deterministic writer.
//!
//! **Status:** scaffold created by WP-50. The crate builds and is linted and tested in CI, but
//! it holds no engine logic yet; WP-55 (PLAN.md §11) implements it.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
