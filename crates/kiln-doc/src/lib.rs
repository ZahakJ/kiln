// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-doc
//!
//! The Kiln Write document model, shaped like WordprocessingML: nodes, marks, the style sheet,
//! numbering, sections, fields, footnotes, comments and revisions, with a transaction log and undo.
//!
//! **Status:** scaffold created by WP-50. The crate builds and is linted and tested in CI, but
//! it holds no engine logic yet; WP-61 (PLAN.md §11) implements it.
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
