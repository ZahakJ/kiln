// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-common
//!
//! Shared foundations for every Kiln crate: stable identifiers (`DocId`, `NodeId`), units (EMU,
//! twips, points, pixels), colours, geometry on top of `kurbo`, error types and feature tiers.
//!
//! **Status:** scaffold created by WP-50. The crate builds and is linted and tested in CI, but
//! it holds no engine logic yet; WP-52 (PLAN.md §11) implements it.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// The suite version, shared by every crate in the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The product name shown in window titles, installers and the About dialog.
pub const PRODUCT_NAME: &str = "Kiln";

/// The application identifier used by Tauri and as the Android `applicationId`.
pub const APP_IDENTIFIER: &str = "io.github.zahakj.kiln";

/// The suite name and version as one line, for example `kiln 0.1.0`.
#[must_use]
pub fn version_line() -> String {
    format!("kiln {VERSION}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_line_names_the_suite_and_version() {
        assert_eq!(
            version_line(),
            format!("kiln {}", env!("CARGO_PKG_VERSION"))
        );
        assert!(!VERSION.is_empty());
        assert_eq!(PRODUCT_NAME, "Kiln");
        assert_eq!(APP_IDENTIFIER, "io.github.zahakj.kiln");
    }
}
