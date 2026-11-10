// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-search
//!
//! An in-memory inverted index and fuzzy matcher for command-palette synonyms, Kiln Notes search
//! and Find.
//!
//! **Status:** a scaffold. The crate builds and is linted and tested in CI, but it holds no
//! engine logic yet.
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
